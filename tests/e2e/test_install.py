"""US1 install and first-run acceptance (quickstart: Install and first-run setup)."""

from __future__ import annotations

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
    if os.environ.get("FD_E2E_BASE_IMAGE", "") == "":
        pytest.skip("VM acceptance prerequisites missing")
    vm = vm_factory()
    install = vm.install()
    assert install.returncode == 0
    assert "setup code" in install.stdout.lower() or "Open https://" in install.stdout

    small = vm_factory(LOW_MEMORY_VM)
    check = small.install("--check-only", check=False)
    assert check.returncode == 2
    assert "2 GB RAM" in check.stdout or "RAM" in check.stdout
