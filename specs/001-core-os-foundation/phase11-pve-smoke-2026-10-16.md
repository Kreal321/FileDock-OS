# Phase 11 PVE smoke — 2026-10-16

## Result

**Phase 11 is not accepted yet.** Live storage, share and container tests passed in the cases
below, but five reproducible implementation gaps require fixes. No application source or
installed binaries were changed during this test.

## Environment

- VM `192.168.68.26`, Debian 13 amd64, kernel `6.12.111+deb13-amd64`.
- Installed FileDock packages: `0.1.0`; Management HTTP port `8021`.
- Core SHA-256: `b0fc90592b48a9948e2a33dc93de44ed8c6905f5b34b5d04a92b19897af7de69`.
- 200 GiB OS disk, serial `drive-scsi0`, partitioned ext4 root and swap. **No additional data disks.**
- Actual System Pool: loop-mounted ext4, direct I/O enabled (`DIO=1`), `discard,nosuid,nodev`,
  sparse image. Approximately 91.7 GB effective capacity with defaults 50% / 100 GB reserve.
- Baseline: no managed folders, share users, connections, Compose apps or container images.
- Testing used two uniquely named folders on the existing System Pool, dedicated no-login share
  users and a tiny offline Docker image assembled from installed Debian binaries. No registry
  image pull or provider credentials were needed.

## Passed live checks

| Check | Evidence |
| --- | --- |
| Administrator sign-in and Phase 11 read APIs | Core Healthy, mutations enabled; pools/disks/folders/users/connections/apps accessible |
| System Pool backing store | Real ext4 loop mount, direct I/O and discard; available and writable |
| Limits validation | Percentage 9 and reserve 19 GB refused; generation unchanged |
| System Pool immutability | Deletion refused with `system_pool.immutable` |
| Missing shrink acknowledgement | Refused with `system_pool.interruption_unacknowledged` |
| Bind-source repair from Phase 10 | `/storage/fd-p11-344fef` and its real pool directory have matching device/inode; canary appears through both paths |
| Share users | Linux no-login accounts created and observed Ready |
| SMB actual client I/O | Writer uploads and reads; reader reads; reader upload returns `NT_STATUS_ACCESS_DENIED` |
| NFS actual kernel client I/O in isolation | NFSv4 client mounts and reads; matching share UID writes; root-squash denies root writes; `ro` rule denies share UID writes |
| WebDAV without escaped mount dependency | Folder `p11dav344fef`: independent read/write listeners, ports 8441/8442; writer PUT 201; reader GET 200; reader PUT rejected; wrong-role and unauthenticated access 401 |
| Compose application | Running real container reads the SMB-written canary through `/storage/...:/data` |
| App binding guard | Removing Docker access while bound is refused with `folder.bound_to_app` / `app.docker_access_required` |
| App project-name guard | Rename refused with `app.name_immutable` |
| App start/stop | Journaled actions succeed |
| Management independence | SMB, WebDAV and container I/O continue while Management is stopped |
| Core independence/recovery | Same reads continue across Core SIGKILL; Core restarts and Management reconnects |
| Resize without NFS rules | Reserve 100 → 101 GB shrink and 101 → 100 GB growth succeed; canaries preserved; defaults restored |
| Provider guards | Unserved family and unsupported session token refused; token value not echoed |
| Browser | Real dashboard, Shares, Docker and Cloud pages inspected; eight healthy services before tests; real container shown Running |
| Cleanup | Original empty folder/user/connection/app lists restored; test mounts/accounts/project/image removed |

NFS I/O passed **after the failing WebDAV rules on that folder were cleared**. This is not a pass
for combined exports under failure. The first NFS UID-write attempt used a client mount inside a
0700 diagnostic directory; repeating on a traversable client mount passed. That initial permission
failure was in the test harness, not FileDock.

## Gaps requiring fixes

### 1. Disk discovery loses partition ancestry and misclassifies the OS disk

The production `/disks` response reports `/dev/sda`, serial `drive-scsi0`, as
`isSystemDisk: false` even though its descendants contain `/` and swap. Running the exact
production `lsblk` command shows a flat JSON array with no `children`; adding `--tree` restores
the ancestry the parser expects.

Location: `core/crates/fd-storage/src/disks.rs:88`.

Fix discovery to request/retain the device tree and regress actual partitioned-root output.
Existing nonblank-signature checks still reject this disk for blank-disk creation; no erase of
an OS disk was attempted. The incorrect classification nevertheless breaks the intended system-disk
protection/inventory and System Pool SMART association.

### 2. WebDAV cannot start for a folder whose mount unit contains escaped characters

For generated path `/storage/fd-p11-344fef`, the dependency is:

```text
storage-fd\x2dp11\x2d344fef.mount
```

Replaying the adapter's exact `systemd-run` arguments in a disposable probe returns:

```text
Failed to start transient service unit: Unit storage-fdx2dp11x2d344fef.mount not found.
```

`systemd-run --property` consumes the backslashes in `BindsTo` / `After`. The export reports
Failed with `systemd-run failed`, and no WebDAV listener starts. A second generated path with no
hyphens starts successfully and its protocol access checks pass.

Location: `core/crates/fd-subsystems/src/adapters/webdav.rs:27`.

Correct the property escaping or use a typed systemd API; regress generated names containing
hyphens, spaces and other escaped characters.

### 3. A WebDAV failure removes another protocol's NFS export but leaves NFS Ready

With SMB, NFS and WebDAV rules on the same folder, the WebDAV failure above leaves
`/etc/exports.d/filedock.exports` containing only its comment header. `exportfs -v` is empty;
an NFSv4 mount receives `access denied by server`. The API/UI still report the NFS export Ready.
Clearing just the failing WebDAV rules restores the NFS configuration and actual NFS I/O.

Location: `core/crates/filedock-core/src/sharing_runtime.rs:264`.

The shared error handler removes NFS entries by folder path even when the failed row is WebDAV.
Limit rollback to the failed protocol/resource and report readiness from the applied result.

### 4. Acknowledged System Pool shrink fails when NFS rules are active

The 100 → 101 GB reserve change with acknowledgement fails during quiescing:

```text
exportfs: Could not find '127.0.0.1:/storage/fd-p11-344fef' to unexport.
```

Operation `01a146e5-b0ce-7273-853b-bf2f91a1abea` is Failed: `resize` remains `acting`, and
`commit` remains pending. Desired limits/generation remain unchanged. Folder mounts temporarily
stop; reconciliation later restores access and the source canary is intact.

Location: `core/crates/filedock-core/src/storage_runtime.rs:727` and the subsequent NFS unexport loop.

Quiesce first rewrites exports without the pool and runs `exportfs -ra`, then tries to unexport
those already-removed entries individually. Make quiescing idempotent and handle failure recovery
and journal step state explicitly. With the test NFS rules cleared, the same shrink succeeds
(`01a146e6-fd1d-76a5-b7ba-bdd215bb7db8`), followed by successful growth back to the original reserve
(`01a146e7-4404-729c-b2e9-8a429b0a6e50`). Canaries survive both.

During resize/cleanup the NFS service also became inactive and supervision reached Failed.
It was manually started to restore the baseline; its registry returned to Running. Planned mount
maintenance should not leave NFS stopped or consume its failure budget without recovery.

### 5. Azure's connection form selects a schema-less backup engine

The installed options list Azure providers in this order:

```text
kopia:   azure-blob, no connection schema
rclone:  azure-blob, connection schema present
```

The UI takes the first enabled connector claiming the family, so choosing Azure shows no schema
fields and saving its inferred `connectorId: kopia` is refused with `connection.schema_missing`.
The same UI shows the expected schema fields for Google Drive.

Location: `web/src/features/connections/ConnectionsPage.tsx` (schema selection and `save`).

Choose a connection-capable schema owner, preserve existing ownership when editing, and keep the
OAuth implementation's owner consistent with the saved bound connection.

## Pending acceptance

- Data-pool create, adopt, grow, missing-disk detection and same-UUID recreation: no extra disks.
  Two blank disks with unique serials cover creation/growth; more disposable disks or staged
  replacement are needed for the full sequence.
- Full System Pool hard-cap/reserve fill and full OS/data-disk pressure/no-partial-commit injection:
  not run on this installed VM. Use the disposable-VM acceptance suite.
- Full VM reboot/persistent boot order: not performed; service stop/kill recovery was tested.
- Real S3/provider success, OAuth paste-back/refresh/expiry and real credentials: not available.
- Named-volume outside-protection warning and every UI keyboard/mobile state: not repeated here.

## Final state and cleanup

- Core and Management active; all six built-in registry entries Running again. The final real
  dashboard confirms **8 reported / 8 Healthy** services.
- Core PID `504159`, `NRestarts=1` from the intentional kill. Management PID `504094`.
- System Pool available/writable, limits 50% / 100 GB, desired generation 3 after successful
  shrink/growth. Generation and operation/audit history legitimately advanced; they were not reset.
- Only ConnectorRoutes, Settings and the original System Pool remain live in desired state.
- No test containers, WebDAV units, user accounts, folder mounts or fixture image remain.
- Test canaries were verified before removing their uniquely named pool directories.
- Stock Debian `smbclient`, `libsmbclient0` and `libarchive13t64` were installed as test tools
  (about 3.4 MB); no existing packages were upgraded. They remain installed.
- API and browser test sessions signed out; temporary plaintext passwords/auth files and API cookies removed.
  Redacted diagnostics/results remain in root-only `/var/tmp/filedock-phase11-20261016`.
- Expected failed operations and audit records remain available for diagnosis. No installed
  binaries, application source, administrator credentials or original user data were replaced.
