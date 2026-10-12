"""Regression checks for shipping a usable appliance bundle."""
from pathlib import Path
import hashlib
import os
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def test_packager_rejects_unsupported_architecture(tmp_path):
    result = subprocess.run(
        ['bash', str(ROOT / 'packaging/debian/build-packages.sh'), str(tmp_path)],
        capture_output=True, text=True, env={**os.environ, 'FD_ARCH': 'invalid'},
    )
    assert result.returncode != 0
    assert not list(tmp_path.glob('*.deb'))


def installer_functions():
    # Load function definitions without invoking the privileged installation entry point.
    source = (ROOT / 'packaging/installer/install.sh').read_text()
    return source[source.index('verify_deb() {'):source.index('configure_services() {')]


def test_checksum_rejects_corruption(tmp_path):
    package = tmp_path / 'rclone.deb'
    package.write_bytes(b'original package')
    lock = tmp_path / 'versions.lock'
    lock.write_text(f'deb rclone 1.0 https://example.invalid/rclone.deb {hashlib.sha256(package.read_bytes()).hexdigest()}\n')
    script = installer_functions() + '\nverify_deb rclone "$PACKAGE"\n'
    env = {**os.environ, 'LOCK': str(lock), 'PACKAGE': str(package)}
    assert subprocess.run(['bash', '-c', script], env=env, capture_output=True).returncode == 0
    package.write_bytes(b'corrupted package')
    assert subprocess.run(['bash', '-c', script], env=env, capture_output=True).returncode != 0


def test_offline_install_propagates_apt_failure(tmp_path):
    for name in ('filedock-core_0.1.0_amd64', 'filedock-management_0.1.0_amd64', 'filedock-builtins_0.1.0_amd64', 'rclone', 'kopia'):
        (tmp_path / f'{name}.deb').write_bytes(b'fixture')
    lock = tmp_path / 'versions.lock'
    lock.write_text('apt systemd 1.0\n')
    script = installer_functions() + '\napt-get() { return 42; }\ninstall_packages\n'
    env = {**os.environ, 'LOCK': str(lock), 'OFFLINE_BUNDLE': str(tmp_path), 'RELEASE': '0.1.0'}
    result = subprocess.run(['bash', '-c', script], env=env, capture_output=True)
    assert result.returncode != 0
