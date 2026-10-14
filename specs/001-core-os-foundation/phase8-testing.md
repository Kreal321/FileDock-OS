# Phase 8: extension packages and runtimes

## What is implemented

- Verified `.fdext` tar.zst packages, exact SHA256SUMS coverage, manifest and release compatibility checks, optional trusted minisign signatures.
- Per-instance CoreBridge tokens, declared capability registration, permission checks, revocation, event replay and secret redaction.
- Generic Subsystem RPC adapter; independent extension supervision and startup grace, persistent failure/backoff state, explicit restart.
- Journaled install/upgrade/remove; permission, unsigned-package and data-loss acknowledgements; health verification and durable rollback resumption.
- Encrypted local package cache, package-before-snapshot publication, deduplication and per-copy retained-snapshot package pruning.
- Recorded-hash/version recovery uploads and package reinstatement, with data bindings kept and unavailable folders reported as pending.
- Default capability providers; cloud selection remains through connector routes.
- **Extensions** in the main sidebar: upload/inspect, approvals, folder selection, upgrade, restart, removal, package-needed upload and defaults.

The configuration/runtime application is in `core/crates/filedock-core/src/extension_runtime.rs`;
package validation and sandbox/runtime/contract helpers are in `fd-subsystems`. No third-party code
is loaded into the Core. The Management listener remains HTTP port **8021**.

## Build the release and fixtures

Use a **Debian 13 amd64 build host**, with the repository's normal Rust/.NET/pnpm/buf dependencies,
C compiler and static libc development libraries, plus Docker for the sample image:

```sh
make all
make bundle
bash tests/fixtures/build.sh "$PWD/dist/fixtures"
```

`make core` builds the small static first-party container entrypoint as well as Rust services.
The release installs it at `/usr/lib/filedock/fd-extension-launcher`. It reads the bridge token
from a private tmpfs file before executing the extension, keeping the token out of Docker's saved
container environment. The bundle now includes the pinned Debian `acl` dependency.

The fixture builder creates:

```text
dist/fixtures/sample-native.fdext
dist/fixtures/sample-systemd.fdext
dist/fixtures/sample-docker.fdext
dist/fixtures/sample-native-broken.fdext
dist/fixtures/sample-docker-image.tar
```

The Docker fixture is built locally from the sample binary and its library closure, with no mutable
external base. Its manifest pins the actual image SHA-256. Load that image on the target VM:

```sh
sudo docker load -i sample-docker-image.tar
```

Install the newly built release bundle using the normal offline installer. A previously published
release cannot contain these uncommitted source changes.

## Manual checks on a disposable PVE VM

1. Sign in and open **Extensions**. Existing built-ins are listed and cannot be removed.
2. Upload `sample-native.fdext`. Inspect its identity/version, unsigned state and requested
   `core.events.read` permission. Installation must be refused without the exact permission set
   or without the unsigned acknowledgement. Approve and install; follow its operation link until
   **Succeeded**. Repeat for `sample-systemd.fdext`.
3. Select a `cache.generic` default when multiple sample providers exist. Cloud capabilities must
   use their family/use routes instead of the generic default endpoint.
4. For the Docker sample, bind `data` to an existing managed folder. A Hybrid Backup folder at
   **Important** is recommended. Choosing Local Only requires explicit loss acknowledgement.
   Real folder/pool creation belongs to Phases 10/11; the automated disposable-VM test supplies an
   isolated tmpfs-backed folder fixture solely to test sandbox enforcement, not disk provisioning.
5. After install, confirm `sample.txt` appears in the bound folder. The sample also attempts a
   persistent write outside its binding and reports `SANDBOX_DENIED` when it fails. Check:

   ```sh
   sudo journalctl -u filedock-extension-sample-native --no-pager
   sudo journalctl -u filedock-extension-sample-systemd --no-pager
   sudo docker logs fd-extension-sample-docker
   sudo docker inspect fd-extension-sample-docker
   ```

   The container must have a read-only root, bind mounts rather than volumes, dropped capabilities,
   no-new-privileges, and no bridge token value in `Config.Env`. The token file exists only in its
   private runtime directory. Native/service units must use a distinct unprivileged system user,
   ProtectSystem=strict, restricted writable paths and the approved bindings.
6. Kill the sample native process 20 times, waiting for each restart before the next kill:

   ```sh
   sudo systemctl show filedock-core -p MainPID -p NRestarts
   sudo systemctl kill --signal=KILL --kill-whom=main filedock-extension-sample-native.service
   ```

   Restart backoff applies. After five failures within 15 minutes, use **Restart extension** to
   recover it before continuing. Core's PID/restart count and other running services must remain
   unchanged. The automated test performs all 20 cycles and checks Core's PID/restart counter.
7. Choose **Upgrade**, upload `sample-native-broken.fdext`, and approve its permissions/signature
   acknowledgement. The operation must become **RolledBack** and native version **1.0.0** must
   be running again.
8. A bound managed folder must not be deleted while its binding exists. Removing an unused
   extension preserves the folder/data; routes and dependent folders prevent removal. The Rust
   transaction test and disposable-VM gRPC fixture exercise the deletion guard.
9. Create a state snapshot and inspect each available copy. Every referenced package must have
   `packages/<sha256>.fdpkg`; another unchanged snapshot must reuse it. Packages no retained
   snapshot references are pruned only after a durable index update.
10. During configuration recovery, verified packages are restored automatically with their saved
    permissions/configuration/bindings. Missing packages appear as **PackageNeeded** in Extensions;
    supply the exact recorded artifact. A different hash or version is refused. Missing/unmounted
    bound folders remain pending instead of claiming a healthy extension.

## Package author rules

- Package root contains `manifest.yaml`, payload files, `SHA256SUMS`, and optionally
  `SHA256SUMS.minisig`. Every payload/manifest/schema file must be included in the checksum list.
  Links, special files, path traversal, duplicates and unchecked payloads are rejected.
- Native executables must be packaged relative paths (`./sample`) with executable mode. A packaged
  systemd template declares a simple service and a relative `ExecStart`; Core generates its unit
  and enforces its drop-in, refusing free-form service directives that could bypass the sandbox.
- Docker images must be immutable digests and available on the target Engine. Image-declared
  volumes are refused, including anonymous persistence. The manifest command is the executable
  and arguments; otherwise the image's entrypoint/command are used by the trusted launcher.
- Every runtime implements/registers the Subsystem gRPC contract. Core checks identity, version,
  declared capabilities, OS runtime state and Subsystem.Health. `FD_EXTENSION_ID`,
  `FD_EXTENSION_VERSION`, `FD_EXTENSION_TOKEN`, `FD_CORE_SOCKET`, and `FD_EXTENSION_SOCKET` are
  supplied by Core; package environment variables may not override the `FD_` namespace.
- IP networking is isolated unless `network.outbound` is requested and approved. `network.listen`
  limits exposed/native bind ports when IP networking is enabled. Unix sockets remain available
  for the bridge. Persistent write access is confined to approved data bindings; temporary/runtime
  directories are separate.
- Secrets use granted consumer references and temporary secret files. Store no user credentials in
  manifests or default configuration. Callback/diagnostic observations are redacted before public
  persistence, including known vault values.
- Sign `SHA256SUMS` with minisign. An administrator places the matching trusted `.pub` under
  `/etc/filedock/extensions/trusted-keys/`. Inspection rejects a present but invalid/untrusted
  signature; it does not silently downgrade to unsigned. A verified recovery snapshot's recorded
  hash pins the originally approved package even when rebuilding onto a machine without that
  trust-directory file.

The mapping follows the [systemd execution controls](https://github.com/systemd/systemd/blob/main/man/systemd.exec.xml),
[Docker Engine API](https://docs.docker.com/reference/api/engine/version/v1.51/), and
[minisign verifier](https://docs.rs/minisign-verify/0.2.5/minisign_verify/) interfaces.

## Automated validation

```sh
cargo test -p fd-subsystems -p fd-appstate -p fd-state -p fd-secrets -p fd-journal -p filedock-core
cargo clippy --workspace --all-targets -- -D warnings
dotnet test --solution management/FileDock.Management.sln --ignore-exit-code 8
pnpm -C web test
pnpm -C web lint
pnpm -C web build
python3 tests/integration/phase8_smoke.py
python3 tests/integration/phase6_7_smoke.py
```

VM acceptance after building the fixture packages:

```sh
FD_E2E_EXTENSION_PACKAGES="$PWD/dist/fixtures" \
  uv run --project tests/e2e pytest tests/e2e/test_extensions.py -v
```

The existing VM harness is for disposable machines. Missing fixture binaries/images cause explicit
skips. The synthetic data fixture refuses an existing PVE host. It never formats a disk.

**Validation record (2026-10-14)**: 59 focused Rust tests (two Linux process tests ignored locally), Clippy, 31 .NET tests, 32 frontend tests,
frontend lint/build, 22 contract tests and both native API/recovery smokes passed locally. Two buf
checks skipped because buf is unavailable; 16 VM tests collected. C launcher and shell scripts
passed syntax checks. Full Linux systemd/Docker execution, 20 kills, rollback and real managed-folder
provider integration remain pending VM validation. No Phase 8 deployment or test changed the PVE
installation during this implementation.
