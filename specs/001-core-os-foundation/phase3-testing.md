# Phase 3 independent test

Use a **fresh Debian 13 amd64 PVE VM** with systemd, 2 vCPUs, 4 GB RAM and a 256 GB sparse system disk. Phase 3 does not require extra disks. An LXC container is not the supported acceptance target: NFS, Docker and systemd need the normal VM environment. Build on a separate Debian 13 amd64 machine so the target starts without FileDock or third-party Samba/NFS/Docker configuration.

For a GitHub-built downloadable bundle, see [packaging/RELEASES.md](../../packaging/RELEASES.md).

## Build a complete offline bundle

Install the prerequisites listed in quickstart.md on the build machine (Rust, .NET 10 SDK, Node/pnpm, protoc, buf, cargo-nextest, Python/uv), plus `make`, `file`, `dpkg-dev` and `sudo`.

From the repository root:

```bash
make proto
make test
make bundle
```

`make bundle` builds Rust, builds React into the .NET API's wwwroot, publishes self-contained Linux API and CLI applications, builds three FileDock Debian packages, and downloads the pinned runtime packages plus their dependency closure. It needs network access and uses sudo to configure the pinned Docker repository on the build host. A missing package version or failed checksum stops the build. Native macOS/ARM binaries cannot be packaged for this target.

`dist/install.sh`, `dist/versions.lock`, `dist/RELEASE_VERSION`, `dist/SHA256SUMS` and **all** `dist/*.deb` are required. `make packages` alone is not a complete offline bundle. The online installer also needs published FileDock release artifacts; use the local bundle for this test.

## Manual test on the fresh PVE VM

On the build machine, replace `USER` and `VM_IP`:

```bash
ssh USER@VM_IP 'mkdir -p ~/filedock-bundle'
scp dist/install.sh dist/versions.lock dist/RELEASE_VERSION dist/SHA256SUMS dist/*.deb USER@VM_IP:~/filedock-bundle/
ssh USER@VM_IP
```

On the VM:

```bash
cd ~/filedock-bundle
sudo bash install.sh --offline-bundle "$PWD" --check-only
sudo bash install.sh --offline-bundle "$PWD"
```

The installer prints `http://VM_IP:8021/` and a one-time setup code. Open that address from your computer, enter the setup code, name the appliance and create an administrator (`root` is the editable default); enter the password twice. More than 8 characters is suggested, with no minimum-length enforcement. Click Download recovery key and continue, then store the downloaded TXT file somewhere safe. Sign out and sign back in.

All eight cards must show Healthy: FileDock Core, Management Service, Docker Engine, Samba, NFS, Kopia, Native S3 connector and rclone. Kopia is an on-demand engine at this stage; its health means its binary can execute, not that a backup repository is configured. Cloud connection tests and later user stories are separate.

Reboot the VM, sign in again, and confirm the same eight health cards. Then repeat the installer command: it must repair the installation and preserve your administrator, name and recovery key.

For troubleshooting:

```bash
sudo systemctl status filedock-core filedock-management filedock-s3 filedock-rclone docker smbd nfs-server --no-pager
sudo journalctl -u filedock-core -u filedock-management -u filedock-s3 -u filedock-rclone -b --no-pager -n 150
curl -fsS http://127.0.0.1:8021/health
```

## Automated browser and repair acceptance

Run this from the repository on your computer against a **fresh**, disposable VM. The suite installs the bundle, drives the shipped UI through recovery confirmation and logout/login, checks all eight health states, then tests repair. Use an SSH account with passwordless sudo (root also works).

```bash
pnpm -C web install --frozen-lockfile
pnpm -C web exec playwright install chromium
export FD_E2E_EXISTING_HOST=VM_IP
export FD_E2E_SSH_USER=USER
export FD_E2E_SSH_KEY="$HOME/.ssh/id_ed25519"
export FD_DIST="$PWD/dist"
uv run --project tests/e2e pytest tests/e2e/test_install.py -v
```

The harness does not destroy an existing PVE VM. Revert it to its clean snapshot before another fresh-install run. Missing VM prerequisites are reported as skipped and do **not** prove acceptance. Browser traces contain the test recovery key; keep those artifacts private. The default test account is `phase3-admin` with password `Phase3-Acceptance-Password!`; use only on a disposable VM, or override `FD_TEST_ADMIN` and `FD_TEST_PASSWORD`.

For an already installed but still unconfigured VM, obtain a fresh code using `sudo filedockctl setup-code --json`, then run only the browser test:

```bash
FD_BASE_URL=http://VM_IP:8021 FD_SETUP_CODE=CODE pnpm -C web test:e2e
```

## Validation status

Local macOS checks exercise the real Rust Core and .NET API over a Unix socket, including setup, recovery confirmation, logout/login and dashboard response. Run that check after `cargo build -p filedock-core` and `dotnet build management/FileDock.Management.sln`:

```bash
uv run --project tests/e2e python tests/integration/phase3_smoke.py
```

This local check deliberately uses a small recovery KDF cost and cannot validate Linux services. Production uses the normal KDF settings. The fresh Debian/PVE install, Linux systemd D-Bus calls, offline dependency closure, reboot and all-healthy browser test remain to be validated on the target VM. **Do not mark the Phase 3 checkpoint passed or proceed to Phase 4 until that test passes.**


Local validation on 2026-10-11:

- Rust nextest: 40 passed; the live shared-bucket cross-connector test is explicitly ignored until configured.
- .NET: 9 passed; formatting verification passed.
- React production build, ESLint, Prettier and Vitest: passed (1 UI unit test). Playwright discovers the independent acceptance test; it has not run against a VM.
- Contract/packaging tests: 22 passed, 2 buf checks skipped because buf is unavailable on this host.
- Installer preflight: 2 passed; 2 VM cases skipped because no VM target or libvirt image is available.
- Real Core/API integration check: passed; local status returned eight component entries. This does not assert Linux components are healthy on macOS.
- Rust Clippy with warnings treated as errors, shell syntax and Git whitespace checks: passed.


PVE VM installation validation on 2026-10-12:

- Target: Debian 13 amd64 VM at 192.168.68.26; bundle `test-0.1.0-3-1`, commit `7fe47a2ca7436888b7c3c5824efdf298688f5da5`.
- Fixed a missing semicolon in the temporary APT configuration and transferred the corrected repository installer as `/filedock-bundle/install-fixed.sh`.
- Offline installation using only local package sources succeeded. The released CLI rejected `setup-code --json`; the working invocation is `--json setup-code`. Repository CLI options now inherit to subcommands and the installer uses the compatible invocation.
- Completed the install marker after verifying setup-code issuance, then reran the corrected installer; the rerun exited 0. Logs are `/filedock-bundle/install-fixed.log` and `/filedock-bundle/install-repair.log` on the VM.
- Core, Management, fd-s3, rclone, Docker and Samba are active/running; NFS is active/exited as expected for its service. Kopia executes `--version`, rclone responds to `core/stats`, and HTTPS `/health` returns `{"status":"ok"}`.
- First-run administrator creation, recovery-key confirmation, sign-out/sign-in, the authenticated eight-card dashboard, reboot, and repair after completed setup are still pending. This is not a full Phase 3 checkpoint pass.


Development reset on 2026-10-12:

- Management now serves HTTP on port 8844; HTTPS and recovery-group compatibility have been removed from source.
- Uninstalled the three FileDock packages plus Docker/containerd, Samba, NFS, rclone and Kopia from the PVE VM at 192.168.68.26. APT purge exited 0.
- Cleared FileDock configuration, database, recovery keys, runtime files, backups/cache, empty storage directories, service account/groups and journald override. No containers, volumes or mounted FileDock pools were present.
- Ports 443 and 8844 are clear. The old `/filedock-bundle` is preserved; manual testing of these edits requires a newly built release extracted separately.
- The updated HTTP build and first-run flow have not been run on the VM. After reinstall, use `http://192.168.68.26:8021/`.


Port revision: Management UI and API both use HTTP port 8021. Browser requests use the same origin. The Rust Core continues to use its Unix socket. Installer preflight checks TCP port 8021. This revised listener has not been run on the VM.
