# Phase 11 implementation and testing

## Delivery status

T143–T163 implement disks/pools, the System Pool and reserve enforcement, pressure alerts,
SMB/NFS/WebDAV users and exports, cloud connections/OAuth, and Compose apps. T273 fixes the
previously observed quoted systemd bind-mount source and verifies the resulting mount identity.
The PVE installation has **not** been upgraded or modified during this implementation.

Local validation covers Rust policies/rendering/reconciliation, Management, React, temporary
real Core/API instances, and browser interactions. Real Linux block-device/systemd/protocol I/O,
reserve/fill/failure injection, reboot, and live-provider OAuth still need acceptance below.

## Build and install

Use **Offline bundle release** on GitHub after these changes are pushed. Main-branch pushes
already trigger that workflow. Download the release archive and checksum, verify it, extract it,
and run its bundled installer as root on the test VM. The archive includes runtime dependencies;
cloud authorization and pulling new container images still require their own network access.

On a configured Debian 13 amd64 build host, the equivalent complete build is:

```sh
make proto
make bundle RID=linux-x64 CONFIG=Release
```

Use a disposable **Debian 13 VM** in PVE with 2 vCPUs, 4 GB RAM, a 256 GB system disk, and
at least two additional blank data disks (five for the full create/grow/adopt/recreate sequence).
Assign unique disk serials in PVE; a disk without a stable serial cannot be selected for erasure.
A container does not provide the same block-device, mount, systemd and NFS environment.

After installation and first-run confirmation, open `http://<VM-IP>:8021`.
The System Pool image is created after setup/recovery confirmation; allow up to 30 seconds.
A 64 GB system disk cannot meet the default 100 GB reserve and correctly shows Unavailable.
The recovery target remains fresh despite its automatic System Pool metadata.

```sh
sudo filedockctl system-pool show
sudo systemctl status filedock-core filedock-management --no-pager
sudo journalctl -u filedock-core -u filedock-management -p warning -n 100 --no-pager
```

## Independent test: folders, shares and containers

1. In **Pools & disks**, create `media` using a blank data disk. Type exactly the displayed
   `ERASE <serial>` confirmation. Wait for the operation to succeed and the pool to become Ready.
2. Create a Local Only managed folder named `Media` on that pool. Its generated path is
   `/storage/media`. Enable SMB, NFS, WebDAV and Docker access. Wait for Ready.
3. In **Shares**, add a dedicated share user, then configure that folder's rules:
   SMB/WebDAV use `user:<share-username>` with `read` or `read-write`; NFS uses a client address
   or CIDR with `ro` or `rw`. The management administrator is separate from share users.
4. Write a canary using SMB; read it through NFS and the WebDAV endpoint listed in export status.
   Confirm unauthenticated access is denied. Verify read-only users/clients can read but cannot
   write. Mixed WebDAV read/write rules use separate ports, both within 8440–8499.
5. In **Docker**, add a Compose application with a **preloaded** test image and an explicit
   `/storage/media:/data` bind. Read the same canary inside the container. Named volumes and
   external binds must display an outside-protection warning. Removing Docker access or deleting
   the folder while the app binds it must be refused. Project names cannot be changed in place.
6. Verify this is the real pool, including the repair for the earlier mount defect:

   ```sh
   findmnt --mountpoint /storage/media
   stat -c '%d:%i' /storage/media /mnt/filedock/pools/<pool-uuid>/media
   ```

   Both device/inode pairs must match. The source must not contain literal quote characters.
7. Stop Management, then restart Core separately. Existing SMB/NFS/WebDAV/container reads must
   continue. Reboot and verify pool mounts, folder mounts, exports and apps return correctly.
8. Grow the pool with another blank disk and verify capacity increased without changing the canary.
   Adopt a separate ext4 filesystem containing a canary; its existing data must remain untouched.

## System Pool, failure and pressure acceptance

Use a separate disposable VM for fill or disk-detach tests. These tests consume disk capacity
or explicitly recreate a failed pool; do not run them against an installation containing needed data.

- Limits: stricter of percentage and retained free reserve; percentage 10–90, reserve at least
  20 GB. Shrinking requires acknowledgement, interrupts dependent shares/apps, and is rejected
  below usage plus a safety margin. Limits are session-only management actions.
- Fill: filesystem capacity is enforced; user files remain. Consuming OS space to the reserve
  guard makes the System Pool read-only and raises a critical alert; freeing space restores writes.
- Pool failure: remove a member disk; within one minute the pool is Failed, folders are Unavailable,
  and the critical alert names affected folders/recovery paths. Recreate on replacement blank disks
  using exact confirmation; pool UUID stays the same and old Local Only data is not claimed recovered.
- Pressure: defaults warn at 90% and become critical at 95%, adjustable in Alert thresholds.
  Full OS-disk changes return `storage.disk_full`/HTTP 507 with no desired-state commit.
  Only expired extension staging and owned generated diagnostics may be trimmed; canaries remain.
- Cloud: add a static-key S3 connection and a schema-based provider, run connection tests, complete
  OAuth paste-back with real provider credentials, restart and retest refreshed authorization.
  Unserved families and unsupported session/role credentials must be rejected without exposing secrets.

## Automated validation commands

Run on the development host after building Core and Management:

```sh
cargo test -p fd-storage -p fd-state -p fd-reconcile -p fd-subsystems -p fd-appstate -p filedock-core --lib --bins --tests
cargo clippy -p fd-storage -p fd-state -p fd-reconcile -p fd-subsystems -p fd-connectors -p fd-appstate -p filedock-core --all-targets -- -D warnings
dotnet test --solution management/FileDock.Management.sln
pnpm -C web build
pnpm -C web test
pnpm -C web lint
python3 tests/integration/phase11_smoke.py
```

The API smoke uses temporary state with runtime disabled; it cannot provision real disks or prove
Linux data-plane behavior. Browser checks use synthetic responses:

```sh
pnpm -C web dev --host 127.0.0.1 --port 5173
# In another terminal:
FD_BASE_URL=http://127.0.0.1:5173 pnpm -C web exec playwright test phase11.spec.ts
```

The Linux acceptance suite provisions **fresh disposable libvirt VMs**, using an existing Debian
cloud image and the complete offline distribution. See `tests/e2e/conftest.py` for prerequisites:

```sh
export FD_E2E_BASE_IMAGE=/absolute/path/to/debian-13-genericcloud-amd64.qcow2
export FD_DIST=/absolute/path/to/extracted/filedock-bundle
uv run --project tests/e2e pytest tests/e2e/test_storage_shares.py -v
```

All five storage/fill tests deliberately skip when `FD_E2E_EXISTING_HOST` is set, before installing
or changing that host. This suite is not a non-destructive smoke for the current PVE installation.

## Evidence to record after deployment

| Check | Current result |
| --- | --- |
| Local Phase 11 CRUD, stale edits, credential redaction, schema guards, pressure settings | Passed |
| Local Rust focused suites, strict Clippy, Management and React regression suites | Passed (78 Rust, 32 Management, 53 React) |
| Desktop 1440 px / phone 360 px browser mutation guards and dialogs | Passed; synthetic API |
| Fresh restore with automatic System Pool metadata | Passed locally |
| Earlier Phase 6–7, Phase 8 and Phase 9–10 isolated API regressions | Passed |
| Contract tests | 22 passed; 2 Buf checks skipped because Buf is unavailable |
| New Linux acceptance definitions | Five tests collect; not executed on Linux |
| Full Debian offline bundle / Linux-only code paths | Pending release build |
| PVE pool mounts, shares, container I/O and reboot | Pending |
| Reserve, hard cap, pool loss/recreation and disk-pressure injection | Pending disposable VM run |
| Real cloud-provider OAuth/test/refresh and expiry warnings | Pending provider credentials |

Reviewed browser captures are under ignored `web/test-results/phase11-*.png`; they are local
review artifacts, not replacement canonical Linux screenshot baselines. Full UI human/zoom/keyboard
acceptance remains subject to the existing UI acceptance plan.
