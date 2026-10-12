"""VM acceptance harness (quickstart.md).

Each test VM is a libvirt domain booted from a Debian 13 cloud image overlay and provisioned with
a cloud-init NoCloud seed. Tests skip, rather than fail, when the host lacks the prerequisites.

Environment:
  FD_E2E_EXISTING_HOST Existing disposable PVE VM IP (alternative to libvirt)
  FD_E2E_SSH_USER     SSH account with passwordless sudo on the existing VM
  FD_E2E_SSH_KEY      SSH private key for the existing VM
  FD_E2E_BASE_IMAGE   Debian 13 amd64 genericcloud qcow2 (libvirt only)
  FD_E2E_LIBVIRT_URI  libvirt connection URI (default qemu:///system)
  FD_E2E_NETWORK      libvirt network (default "default")
  FD_E2E_WORKDIR      disk images, seeds, and SSH keys (default tests/e2e/.work)
  FD_E2E_KEEP_VMS     set to 1 to keep VMs after the session for debugging
  FD_E2E_MINIO_IMAGE  MinIO image (default quay.io/minio/minio:latest)
  FD_DIST             directory with the built .debs and install.sh (default dist/)
"""

from __future__ import annotations

import io
import os
import re
import shutil
import socket
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from pathlib import Path

import boto3
import httpx
import pycdlib
import pytest
import yaml

REPO = Path(__file__).resolve().parents[2]
SSH_USER = "fd-e2e"
REQUIRED_TOOLS = ("virsh", "virt-install", "qemu-img", "ssh", "scp", "ssh-keygen")


@dataclass(frozen=True)
class Env:
    base_image: Path
    libvirt_uri: str
    network: str
    workdir: Path
    dist: Path
    keep_vms: bool
    existing_host: str
    ssh_user: str
    existing_key: Path

    @staticmethod
    def from_environment() -> Env:
        return Env(
            base_image=Path(os.environ.get("FD_E2E_BASE_IMAGE", "")),
            libvirt_uri=os.environ.get("FD_E2E_LIBVIRT_URI", "qemu:///system"),
            network=os.environ.get("FD_E2E_NETWORK", "default"),
            workdir=Path(os.environ.get("FD_E2E_WORKDIR", REPO / "tests/e2e/.work")),
            dist=Path(os.environ.get("FD_DIST", REPO / "dist")),
            keep_vms=os.environ.get("FD_E2E_KEEP_VMS") == "1",
            existing_host=os.environ.get("FD_E2E_EXISTING_HOST", ""),
            ssh_user=os.environ.get("FD_E2E_SSH_USER", SSH_USER),
            existing_key=Path(os.environ.get("FD_E2E_SSH_KEY", "")),
        )

    def missing_prerequisites(self) -> list[str]:
        if self.existing_host:
            missing = [f"{tool} not on PATH" for tool in ("ssh", "scp") if shutil.which(tool) is None]
            if not self.existing_key.is_file(): missing.append("FD_E2E_SSH_KEY must point to the VM's SSH private key")
            return missing
        missing = [f"{tool} not on PATH" for tool in REQUIRED_TOOLS if shutil.which(tool) is None]
        if not self.base_image.is_file():
            missing.append("FD_E2E_BASE_IMAGE does not point to a Debian 13 qcow2 image")
        return missing


@dataclass(frozen=True)
class VmSpec:
    """Defaults match the quickstart's standard test VM."""

    memory_mib: int = 4096
    vcpus: int = 2
    system_disk_gb: int = 256
    data_disks_gb: tuple[int, ...] = (20, 20)


STANDARD_VM = VmSpec()
SMALL_DISK_VM = VmSpec(system_disk_gb=64)
LOW_MEMORY_VM = VmSpec(memory_mib=1024, data_disks_gb=())


def run(*args: str | Path, check: bool = True, timeout: float = 600) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(a) for a in args], capture_output=True, text=True, check=check, timeout=timeout
    )


class ApiClient:
    """Thin `/api/v1` client. The appliance uses a self-signed certificate until one is set."""

    def __init__(self, host: str, token: str | None = None) -> None:
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        self.http = httpx.Client(
            base_url=f"https://{host}/api/v1", headers=headers, verify=False, timeout=30
        )

    def get(self, path: str, **kwargs: object) -> httpx.Response:
        return self.http.get(path, **kwargs)  # type: ignore[arg-type]

    def post(self, path: str, json: object = None, **kwargs: object) -> httpx.Response:
        kwargs.setdefault("headers", self._csrf())
        return self.http.post(path, json=json, **kwargs)  # type: ignore[arg-type]

    def _csrf(self) -> dict[str, str]:
        token = self.http.cookies.get("fd_csrf")
        return {"X-CSRF-Token": token} if token else {}

    def put(self, path: str, json: object = None, *, etag: str | None = None) -> httpx.Response:
        return self.http.put(path, json=json, headers={"If-Match": etag} if etag else None)

    def delete(self, path: str, *, etag: str | None = None) -> httpx.Response:
        return self.http.delete(path, headers={"If-Match": etag} if etag else None)

    def wait_operation(self, operation_id: str, timeout: float = 600) -> dict[str, object]:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            body = self.get(f"/operations/{operation_id}").raise_for_status().json()
            if body.get("state") in ("Succeeded", "Failed", "Cancelled"):
                return body
            time.sleep(2)
        raise TimeoutError(f"operation {operation_id} did not finish in {timeout}s")

    def close(self) -> None:
        self.http.close()


@dataclass
class Vm:
    name: str
    ip: str
    spec: VmSpec
    env: Env
    ssh_key: Path
    disks: list[Path] = field(default_factory=list)

    def _ssh_opts(self) -> list[str]:
        return [
            "-i", str(self.ssh_key),
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "LogLevel=ERROR",
            "-o", "ConnectTimeout=10",
        ]  # fmt: skip

    def ssh(self, command: str, *, check: bool = True, timeout: float = 600) -> subprocess.CompletedProcess[str]:
        return run("ssh", *self._ssh_opts(), f"{self.env.ssh_user}@{self.ip}", command, check=check, timeout=timeout)

    def sudo(self, command: str, *, check: bool = True, timeout: float = 1800) -> subprocess.CompletedProcess[str]:
        return self.ssh(f"sudo bash -c {_quote(command)}", check=check, timeout=timeout)

    def copy_to(self, *local: Path, remote: str) -> None:
        run("scp", *self._ssh_opts(), "-r", *local, f"{self.env.ssh_user}@{self.ip}:{remote}")

    def install(self, *extra_args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        """Copy dist/ to the VM and run the installer as the quickstart does."""
        dist = self.env.dist
        if not (dist / "install.sh").is_file():
            pytest.fail(f"{dist}/install.sh is missing; run `make bundle` first")
        self.ssh("rm -rf ~/dist && mkdir -p ~/dist")
        files = sorted(dist.glob("*.deb")) + [dist / name for name in ("install.sh", "versions.lock", "SHA256SUMS")]
        assert all(path.is_file() for path in files), "Build a complete bundle with make bundle"
        self.copy_to(*files, remote="dist/")
        self.ssh("cp ~/dist/install.sh ~/install.sh")
        args = " ".join(_quote(a) for a in ("--offline-bundle", "./dist", *extra_args))
        return self.ssh(f"cd ~ && sudo bash install.sh {args}", check=check, timeout=3600)

    def api(self, token: str | None = None) -> ApiClient:
        return ApiClient(self.ip, token)

    def virsh(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        return run("virsh", "--connect", self.env.libvirt_uri, *args, self.name, check=check)

    def wait_for_ssh(self, timeout: float = 600) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.ssh("cloud-init status --wait >/dev/null; true", check=False, timeout=900).returncode == 0:
                return
            time.sleep(5)
        raise TimeoutError(f"{self.name}: SSH not reachable at {self.ip} after {timeout}s")

    def destroy(self) -> None:
        self.virsh("destroy", check=False)
        run("virsh", "--connect", self.env.libvirt_uri, "undefine", self.name, "--nvram", check=False)
        for disk in self.disks:
            disk.unlink(missing_ok=True)


def _quote(value: str) -> str:
    return "'" + value.replace("'", "'\"'\"'") + "'"


def _seed_iso(path: Path, hostname: str, public_key: str) -> None:
    """Build a cloud-init NoCloud seed (volume label `cidata`)."""
    user_data = "#cloud-config\n" + yaml.safe_dump(
        {
            "hostname": hostname,
            "users": [
                {
                    "name": SSH_USER,
                    "sudo": "ALL=(ALL) NOPASSWD:ALL",
                    "shell": "/bin/bash",
                    "ssh_authorized_keys": [public_key],
                }
            ],
            "ssh_pwauth": False,
            "package_update": False,
        },
        sort_keys=False,
    )
    meta_data = yaml.safe_dump({"instance-id": hostname, "local-hostname": hostname})

    iso = pycdlib.PyCdlib()
    iso.new(interchange_level=3, joliet=3, rock_ridge="1.09", vol_ident="cidata")
    for iso_name, name, content in (
        ("/USERDATA.;1", "user-data", user_data),
        ("/METADATA.;1", "meta-data", meta_data),
    ):
        data = content.encode()
        iso.add_fp(io.BytesIO(data), len(data), iso_name, rr_name=name, joliet_path=f"/{name}")
    iso.write(str(path))
    iso.close()


def _ssh_keypair(workdir: Path) -> Path:
    key = workdir / "id_ed25519"
    if not key.exists():
        run("ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", "filedock-e2e", "-f", key)
    return key


def _domain_ip(env: Env, name: str, timeout: float = 300) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        out = run("virsh", "--connect", env.libvirt_uri, "domifaddr", name, "--source", "lease", check=False)
        match = re.search(r"ipv4\s+(\d+\.\d+\.\d+\.\d+)/", out.stdout)
        if match:
            return match.group(1)
        time.sleep(3)
    raise TimeoutError(f"{name}: no DHCP lease on network {env.network} after {timeout}s")


def _network_gateway(env: Env) -> str:
    xml = run("virsh", "--connect", env.libvirt_uri, "net-dumpxml", env.network).stdout
    ip = ET.fromstring(xml).find("ip")
    if ip is None or "address" not in ip.attrib:
        raise RuntimeError(f"libvirt network {env.network} has no IPv4 address")
    return ip.attrib["address"]


def create_vm(env: Env, spec: VmSpec, ssh_key: Path) -> Vm:
    name = f"fd-e2e-{uuid.uuid4().hex[:8]}"
    vmdir = env.workdir / name
    vmdir.mkdir(parents=True)

    system = vmdir / "system.qcow2"
    run("qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", env.base_image.resolve(),
        system, f"{spec.system_disk_gb}G")  # fmt: skip
    data = []
    for index, size in enumerate(spec.data_disks_gb, start=1):
        disk = vmdir / f"data-{index}.qcow2"
        run("qemu-img", "create", "-q", "-f", "qcow2", disk, f"{size}G")
        data.append(disk)
    seed = vmdir / "seed.iso"
    _seed_iso(seed, name, ssh_key.with_suffix(".pub").read_text().strip())

    disk_args = [f"--disk=path={system},format=qcow2,bus=virtio"]
    disk_args += [
        f"--disk=path={disk},format=qcow2,bus=virtio,serial=fd-data-{i}"
        for i, disk in enumerate(data, start=1)
    ]
    disk_args.append(f"--disk=path={seed},device=cdrom")
    run(
        "virt-install", "--connect", env.libvirt_uri, "--name", name,
        "--memory", str(spec.memory_mib), "--vcpus", str(spec.vcpus),
        "--import", *disk_args,
        "--network", f"network={env.network},model=virtio",
        "--osinfo", "detect=on,require=off",
        "--graphics", "none", "--noautoconsole",
    )  # fmt: skip

    vm = Vm(name=name, ip="", spec=spec, env=env, ssh_key=ssh_key, disks=[system, *data, seed])
    try:
        vm.ip = _domain_ip(env, name)
        vm.wait_for_ssh()
    except Exception:
        if not env.keep_vms:
            vm.destroy()
        raise
    return vm


@pytest.fixture(scope="session")
def e2e_env() -> Env:
    env = Env.from_environment()
    missing = env.missing_prerequisites()
    if missing:
        pytest.skip("VM acceptance prerequisites missing: " + "; ".join(missing))
    env.workdir.mkdir(parents=True, exist_ok=True)
    return env


@pytest.fixture(scope="session")
def vm_factory(e2e_env: Env) -> Iterator[Callable[[VmSpec], Vm]]:
    """Creates VMs on demand and destroys every one of them at the end of the session."""
    ssh_key = e2e_env.existing_key if e2e_env.existing_host else _ssh_keypair(e2e_env.workdir)
    created: list[Vm] = []

    def make(spec: VmSpec = STANDARD_VM) -> Vm:
        if e2e_env.existing_host:
            if spec != STANDARD_VM: pytest.skip("VM size variants require libvirt provisioning")
            vm = Vm(name="pve-existing", ip=e2e_env.existing_host, spec=spec, env=e2e_env, ssh_key=ssh_key)
        else:
            vm = create_vm(e2e_env, spec, ssh_key)
        created.append(vm)
        return vm

    yield make
    if not e2e_env.keep_vms:
        for vm in created:
            if not e2e_env.existing_host: vm.destroy()


@pytest.fixture
def vm(vm_factory: Callable[[VmSpec], Vm]) -> Vm:
    """A fresh standard VM: 4 GB RAM, a 256 GB sparse system disk, and two 20 GB blank disks."""
    return vm_factory(STANDARD_VM)


@pytest.fixture
def small_disk_vm(vm_factory: Callable[[VmSpec], Vm]) -> Vm:
    """A fresh VM with a 64 GB system disk."""
    return vm_factory(SMALL_DISK_VM)


@pytest.fixture
def installed_vm(vm: Vm) -> Vm:
    vm.install()
    return vm


@dataclass(frozen=True)
class Minio:
    host_endpoint: str
    vm_endpoint: str
    access_key: str
    secret_key: str
    container: str

    def client(self):  # noqa: ANN201 - boto3 client types are generated at runtime
        return boto3.client(
            "s3",
            endpoint_url=self.host_endpoint,
            aws_access_key_id=self.access_key,
            aws_secret_access_key=self.secret_key,
            region_name="us-east-1",
        )

    def create_bucket(self, name: str | None = None) -> str:
        bucket = name or f"fd-e2e-{uuid.uuid4().hex[:8]}"
        self.client().create_bucket(Bucket=bucket)
        return bucket


def _free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("0.0.0.0", 0))
        return sock.getsockname()[1]


@pytest.fixture(scope="session")
def minio(e2e_env: Env) -> Iterator[Minio]:
    """A MinIO container reachable from the host and from VMs on the libvirt network."""
    if shutil.which("docker") is None or run("docker", "info", check=False).returncode != 0:
        pytest.skip("docker is not available for the MinIO fixture")
    image = os.environ.get("FD_E2E_MINIO_IMAGE", "quay.io/minio/minio:latest")
    port = _free_port()
    access_key, secret_key = "fde2eaccess", uuid.uuid4().hex
    container = run(
        "docker", "run", "-d", "--rm", "-p", f"0.0.0.0:{port}:9000",
        "-e", f"MINIO_ROOT_USER={access_key}", "-e", f"MINIO_ROOT_PASSWORD={secret_key}",
        image, "server", "/data",
    ).stdout.strip()  # fmt: skip
    gateway = _network_gateway(e2e_env)
    server = Minio(
        host_endpoint=f"http://127.0.0.1:{port}",
        vm_endpoint=f"http://{gateway}:{port}",
        access_key=access_key,
        secret_key=secret_key,
        container=container,
    )
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        try:
            if httpx.get(f"{server.host_endpoint}/minio/health/ready", timeout=2).status_code == 200:
                break
        except httpx.TransportError:
            pass
        time.sleep(1)
    else:
        run("docker", "rm", "-f", container, check=False)
        raise TimeoutError("MinIO did not become ready in 60s")
    yield server
    run("docker", "rm", "-f", container, check=False)
