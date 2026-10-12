"""US1 install and first-run acceptance (quickstart: Install and first-run setup)."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

import pytest

from conftest import LOW_MEMORY_VM, REPO

INSTALLER = REPO / "packaging/installer/install.sh"


def _preflight(env: dict[str, str], extra: list[str] | None = None) -> subprocess.CompletedProcess[str]:
    merged = os.environ.copy()
    merged.update(env)
    return subprocess.run(
        ["bash", str(INSTALLER), *(extra or [])],
        capture_output=True,
        text=True,
        check=False,
        env=merged,
    )


PASSING_HOST = {
    "FD_TEST_OS_ID": "debian",
    "FD_TEST_OS_VERSION": "13",
    "FD_TEST_ARCH": "x86_64",
    "FD_TEST_MEM_KB": "2097152",
    "FD_TEST_DISK_GB": "16",
    "FD_TEST_NETWORK": "1",
    "FD_TEST_PORT_443": "0",
    "FD_TEST_CONFLICT_SAMBA": "0",
    "FD_TEST_CONFLICT_NFS": "0",
    "FD_TEST_CONFLICT_DOCKER": "0",
}


def test_check_only_low_memory_exits_2_and_changes_nothing(tmp_path: Path) -> None:
    env = {
        **PASSING_HOST,
        "FD_TEST_MEM_KB": "1048576",
        "FD_TEST_DISK_GB": "16",
    }
    before = list(tmp_path.iterdir())
    result = _preflight(env, ["--check-only"])
    assert result.returncode == 2
    assert "2 GB RAM" in result.stdout
    assert list(tmp_path.iterdir()) == before


def test_check_only_passes_on_supported_host() -> None:
    result = _preflight(PASSING_HOST, ["--check-only"])
    assert result.returncode == 0
    assert "Preflight passed" in result.stdout


@pytest.mark.usefixtures("vm_factory")
def test_fresh_install_setup_and_sign_in(vm_factory) -> None:
    vm = vm_factory()
    install = vm.install()
    assert install.returncode == 0
    assert "setup code" in install.stdout.lower() or "Open https://" in install.stdout

    api = vm.api()
    try:
        assert api.get("/system/status").status_code == 423
        for _ in range(5):
            assert api.post("/setup/verify-code", json={"code": "wrong"}).status_code == 400
        assert api.post("/setup/verify-code", json={"code": "wrong"}).status_code == 429
    finally:
        api.close()
    # A console-issued replacement resets the throttle without waiting 15 minutes.
    code = vm.sudo("filedockctl setup-code --json").stdout
    browser_env = os.environ.copy()
    browser_env.update(FD_BASE_URL=f"https://{vm.ip}", FD_SETUP_CODE=json.loads(code)["code"])
    # Exercise the shipped SPA against the real Core and services on the VM.
    browser = subprocess.run(
        ["pnpm", "-C", str(REPO / "web"), "exec", "playwright", "test", "first-run.spec.ts"],
        env=browser_env, capture_output=True, text=True, timeout=240,
    )
    assert browser.returncode == 0, browser.stdout + browser.stderr
    repair = vm.install("--yes")
    assert repair.returncode == 0
    api = vm.api()
    try:
        signed_in = api.post("/auth/sign-in", json={
            "username": browser_env.get("FD_TEST_ADMIN", "phase3-admin"),
            "password": browser_env.get("FD_TEST_PASSWORD", "Phase3-Acceptance-Password!"),
        })
        signed_in.raise_for_status()
        status = api.get("/system/status").raise_for_status().json()
        assert status["applianceName"] == "phase3-test", "Repair must preserve desired state"
    finally:
        api.close()


def test_low_memory_vm_refuses_install(vm_factory) -> None:
    vm = vm_factory(LOW_MEMORY_VM)
    check = vm.install("--check-only", check=False)
    assert check.returncode == 2
    assert "2 GB RAM" in check.stdout
    assert vm.sudo("test ! -f /etc/filedock/.installed", check=False).returncode == 0
