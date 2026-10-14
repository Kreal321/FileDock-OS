# Incremental delivery 2: Phase 6 and Phase 7 testing

## Current scope

Configuration protection and recovery are implemented: automatic encrypted snapshots, per-copy
coverage, recovery-key rotation, a downloadable `.fdbackup`, recovery-key-only local recovery,
remote/file recovery sessions, administrator/token/settings import, ownership fencing, and a
retained restore report. The UI and API remain on **HTTP port 8021**.

**Full data-plane restore is not certified yet.** Phase 7 explicitly depends on **US6 / Phase 8**
for installing extension packages and **US9 / Phase 11** for pool provisioning (see tasks.md).
Missing pools remain `PoolUnmapped`; extensions whose verified package is available wait for their
installer, while missing packages are `PackageNeeded`. Pool mappings currently report
`restore.storage_provider_unavailable` and never format a disk. Diagnostic-bundle canary checks
also require US7 / Phase 9. These dependencies must be completed before claiming the full Phase 7
independent test passes.

Use a **Debian 13 amd64 PVE VM**, rather than an LXC container, for the disk-discovery and restore
acceptance checks. Keep the existing manual-test VM and create a second fresh VM for recovery.
A fresh database import is required; updating an already configured appliance is not a restore.

## Build and install

On the Debian 13 build host, with Rust, .NET 10, pnpm, buf, and the normal package-build dependencies:

```sh
make all
make bundle
```

`make management` includes the React build. `make bundle` also packages `filedockctl`, the
installer, dependencies, and the native S3 connector. Alternatively, run the **Offline release**
GitHub Actions workflow on this revision and download its release bundle from
<https://github.com/Kreal321/FileDock-OS/releases>. A previously published bundle does not contain
uncommitted changes from this implementation.

On the target VM, extract that bundle and run:

```sh
sudo bash install.sh --offline-bundle . --yes
sudo systemctl status filedock-core filedock-management --no-pager
sudo filedockctl state status
```

Open `http://<vm-ip>:8021/`, complete first-run setup, and save the recovery-key TXT **outside the
VM**. Do not remove the current installation until you have downloaded both its recovery key and
a verified configuration backup.

## Phase 6: protection and key rotation

1. Open **Settings → State protection**. The system-disk copy is registered automatically at
   `/var/backups/filedock/appliance-state/`. After setup, allow up to 60 seconds for the first
   snapshot. Without a data-pool or remote copy, the coverage warnings are expected.
2. Create or change an administrator or API token. Check the copy's last snapshot time updates
   within 60 seconds. Signing in/out, token last-use updates, audit entries, and alerts must not
   create a new snapshot on their own.
3. Click **Snapshot now** and open the operation link. It reports success only after every
   registered, available copy is durable; failed copies remain visible and retry independently.
4. Click **Download configuration backup**, or use:

   ```sh
   sudo filedockctl state export /root/filedock-test.fdbackup
   sudo find /var/backups/filedock/appliance-state -type f
   ```

   Copy the `.fdbackup` off the VM. Snapshot/package bytes must not contain plaintext secrets.
5. Register a remote destination:
   - For S3-compatible storage, use **Create a backup connection** with a trusted HTTPS endpoint,
     then its connection ID, the configured bucket, and a dedicated empty prefix. Cloud-storage
     HTTPS is separate from the HTTP management listener. A successful conditional-write probe
     permits the initial claim without the basic-safety acknowledgement.
   - For a removable disk, mount a separate filesystem at a stable path such as `/mnt/recovery`
     and enter that mount path. An ordinary directory on the system filesystem is rejected.
     Keep the removable copy physically separate from the appliance to protect against its loss.
6. Confirm the remote copy has a durable snapshot and machine-loss coverage becomes covered.
   For object storage, inspect `<prefix>/_filedock/owner.json`: the first generation is `1`.
7. Temporarily disconnect the remote target, make another configuration change, and wait five
   minutes. The system copy must continue updating. The failed copy must show its error and stale
   warning; reconnecting must let it catch up.
8. Regenerate the key, download the new TXT, and confirm it. Later snapshots require the new key.
   The new key can open retained older epochs. Keep the old key for copies that have not caught up.

   Console equivalent, which saves a private TXT and acknowledges the saved key:

   ```sh
   sudo filedockctl recovery-key regenerate /root/filedock-new-recovery-key.txt
   ```

   Choose a new file path. Store this file off the VM too.

## Phase 7: configuration recovery checks available now

### Backup-file recovery on the second VM

1. Install the same bundle on a fresh VM with `--yes`.
2. Choose **Restore an existing appliance → Configuration backup file**.
3. Supply the new VM's setup code, the saved recovery key, and the `.fdbackup`.
4. A wrong setup code/key or a damaged archive must be refused. Select the newest verified
   snapshot and start restore.
5. Sign in with the administrator and password from that snapshot. Sessions are not restored.
   Active token hashes and metadata are restored; test a saved token separately.
6. Review the dashboard's restore report. Its snapshot time is the recovery point: later changes
   were lost. The first new Management audit entry is `appliance.restore`. Dismissal is explicit
   and audited. Missing storage/package providers must remain visible as pending.

### Local-copy reinstall

This is a destructive acceptance scenario. Use a disposable test VM or a PVE snapshot/clone.
Preserve `/var/backups/filedock/appliance-state/` and any data-pool filesystems when replacing
FileDock's configuration databases and key files. **Erasing the entire system disk also erases
its system-disk backup**; that scenario needs a data-pool, removable, remote, or downloaded copy.

The installer's interactive path offers local recovery with hidden key input. With `--yes`, the
first-run page offers the detected appliance instead. Root can also use:

```sh
sudo filedockctl detect-state
sudo filedockctl restore-local
```

The key is entered interactively and is not a command-line argument. No setup code is needed.
For an object-storage location, an unchanged generation with no write for over 15 minutes permits
one automatic takeover attempt, audited as `System` with reason `reinstall-restore`. A recent or
changed owner stays a Recovery reader until explicit confirmation in **Owned locations**.

### Remote recovery

On another fresh VM choose S3-compatible or Azure Blob storage and supply the new setup code,
connection definition/temporary credentials, location, and recovery key. Opening/listing a source
must not PUT, DELETE, probe with writes, or sweep objects in that location. After restore, an
explicit takeover must use the observed generation. A generation mismatch must be refused.

**Do not mark the full reinstall/disaster-recovery scenarios passed yet:** recreating the System
Pool, assigning missing disks, reinstalling extensions, and reconciling their actual data plane
remain dependent on US9/US6.

## Automated checks

From the repository, with local dependencies installed:

```sh
cargo test -p fd-appstate -p fd-connectors -p fd-secrets -p fd-journal -p filedock-core
# Uses the project's Microsoft.Testing.Platform configuration:
dotnet test --solution management/FileDock.Management.sln --ignore-exit-code 8
pnpm -C web test
pnpm -C web build
python3 tests/integration/phase6_7_smoke.py
uv run --project tests/contracts pytest tests/contracts
```

The native smoke test uses temporary directories, real Core/gRPC/Kestrel, and test credentials.
It does not touch the installed appliance or PVE VM. It exercises durable snapshots, canary
secrets, authentication exclusion, multipart backup upload, wrong key/code rejection, local and
file recovery, restored sign-in, and report dismissal.

The VM test modules are `test_appliance_state.py`, `test_secrets_canary.py`, `test_reinstall.py`,
and `test_disaster_recovery.py`. Run them only against disposable test machines. Reinstall tests
refuse to reset an `FD_E2E_EXISTING_HOST`; remote-copy tests require `FD_E2E_STATE_ENDPOINT` (trusted
TLS MinIO/S3), `FD_E2E_STATE_ACCESS_KEY`, and `FD_E2E_STATE_SECRET_KEY`.

## Validation record

- Rust configuration/format/copy/ownership/key/journal tests: passed locally.
- .NET tests: passed locally, including configuration-only counter and fresh/idempotent import.
- Web unit tests, production build, and lint: passed locally.
- Native configuration-recovery smoke: passed locally against the final Core and Management builds.
- Contract suite: passed; the two buf checks are skipped when buf is unavailable.
- Installed PVE configuration protection and isolated recovery: **passed**; see [PVE smoke record](phase6-7-pve-smoke-2026-10-13.md).
- Unmounted Linux/LVM discovery, remote provider, real system-disk reinstall, and full data-plane acceptance: **pending manual/dependency validation**, not claimed as passed.
