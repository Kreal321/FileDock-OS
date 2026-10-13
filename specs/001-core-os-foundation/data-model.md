# Data Model: Core Appliance Foundation and OS Setup

**Feature**: `001-core-os-foundation` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md) |
**Research**: [research.md](./research.md)

## Where the data lives

State is split between the two services. Neither service writes the other's store (FR-012).

| Store | Owner | Path | Contents |
|---|---|---|---|
| `core.db` | Core Service | `/var/lib/filedock-core/core.db` | Desired state, observed status, operation journal, subsystem and extension registry, alerts, events, ownership records, snapshot index |
| `secrets.enc` | Core Service | `/var/lib/filedock/secrets.enc` | Encrypted secret records (research R10) |
| `app.db` | Management Service | `/var/lib/filedock-management/app.db` | Administrators, sessions, API tokens, audit history, setup state, UI settings |
| Appliance-state snapshot | Core Service, written to every copy | `<pool root>/.filedock/appliance-state/`, `/var/backups/filedock/appliance-state/`, and `<remote destination>/`, each followed by `appliance/<appliance-id>/` | Encrypted archive of the configuration document `config.json` (no history), `secrets.enc`, and the wrapped DEK. Each copy also holds one encrypted `.fdpkg` per installed extension version (research R11, R14, R22; contracts/appliance-state-layout.md) |
| Configuration backup file | Administrator download | `*.fdbackup` | One-snapshot copy, including its packages (FR-111) |
| System Pool image | Core Service | `/var/lib/filedock/system-pool/pool.img` | Sparse ext4 image backing the System Pool (research R21). Holds user data, not appliance state |

These conventions apply to every entity below:

- Every entity has `id`, a UUIDv7 that is time-ordered and stable across restore. It also has
  `created_at` and `updated_at`.
- Desired-state resources also carry a `generation`, used for optimistic concurrency in
  `ApplyChange`.
- No numeric scale caps are set in this release (research R20).

---

## Management Service entities (`app.db`)

### Administrator

| Field | Type | Rules |
|---|---|---|
| username | string | 1–64 characters, unique, case-insensitive |
| password_hash | string | Argon2id. The password must be non-empty; setup recommends more than 8 characters without a length minimum (FR-022) |
| enabled | bool | The last enabled administrator can't be disabled or removed (FR-025) |
| failed_attempts | int | Reset on successful sign-in |
| locked_until | timestamp? | Set for 15 minutes after 5 consecutive failures (FR-024) |
| last_sign_in_at | timestamp? | |

### Session

| Field | Type | Rules |
|---|---|---|
| administrator_id | FK → Administrator | |
| token_hash | bytes | The cookie value is random, and only its hash is stored |
| created_at, last_seen_at | timestamp | Expires after 30 minutes idle (FR-023) |
| client_ip, user_agent | string | Recorded for audit |

"Sign out all sessions" deletes every session of the administrator.

### ApiToken

| Field | Type | Rules |
|---|---|---|
| name | string | Unique per appliance |
| scope | enum `ReadOnly` \| `Full` | ReadOnly allows `GET` on status, alerts, and lists only (FR-021) |
| secret_hash | bytes | Argon2id. The plaintext is shown once |
| created_by | FK → Administrator | |
| expires_at | timestamp? | Optional |
| revoked_at | timestamp? | Revocation takes effect immediately |
| last_used_at | timestamp? | |

Tokens can never complete setup, reset passwords, or manage tokens.

### AuditEntry (append-only)

| Field | Type | Rules |
|---|---|---|
| at | timestamp | |
| actor_kind | enum `Administrator` \| `ApiToken` \| `Console` \| `System` | |
| actor_id / actor_name | string | |
| action | string | Dotted, such as `folder.create`, `pool.extend`, or `token.revoke` |
| target | string | Resource kind and id |
| result | enum `Succeeded` \| `Failed` \| `Refused` | |
| detail | json | Redacted, with no secret values |

Entries are kept on the appliance for at least 90 days (FR-059). They are not in appliance-state
snapshots. A restored appliance starts a new history with an `appliance.restore` entry.

### SetupState (singleton)

| Field | Type | Rules |
|---|---|---|
| status | enum `AwaitingSetup` \| `Complete` | All UI and API access except setup is blocked while `AwaitingSetup` (FR-020) |
| setup_code_hash | bytes? | Written by the installer and cleared on completion |
| setup_code_issued_at | timestamp | A new code invalidates the old one |
| attempt_counters | json | Throttled per client IP and in total |

---

## Core Service entities: desired state (`core.db`)

Every resource row has these columns:

| Column | Purpose |
|---|---|
| `kind` | Resource kind |
| `id` | Resource identity |
| `generation` | Optimistic-concurrency counter |
| `spec` | The desired configuration, as typed JSON |
| `status` | Observed state, written only by reconcilers |
| `deleted` | Tombstone flag |

### PhysicalDisk (observed, not desired)

| Field | Type | Rules |
|---|---|---|
| serial, wwn | string | The stable identity used to match disks across reboots |
| model, size_bytes, transport | | |
| device_path | string | Informational only, and never referenced by folders |
| is_system_disk | bool | A system disk backs only the System Pool, and can never join a data pool |
| pool_id | FK → Pool? | A disk belongs to at most one pool |
| health | enum `Ok` \| `Warning` \| `Failing` \| `Missing` | From SMART (research R8) |

### Pool

| Field | Type | Rules |
|---|---|---|
| name | string | Unique, lowercase `[a-z0-9-]{1,32}`. The System Pool is always `system`, and the name is reserved |
| kind | enum `Lvm` \| `Adopted` \| `System` | An `Adopted` pool is a single disk and cannot grow in this release. Exactly one `System` pool exists (FR-098) |
| layers | list | Such as `["lvm"]` or `["image"]` for the System Pool. Reserved for a future `"luks"` layer (research R8, R9, R21) |
| member_disks | list of disk serial/wwn | At least one. Growth adds members, and removal is not supported. For `System`, always the system disk only, and never editable |
| capacity_bytes, used_bytes | int | Observed. For `System`, `capacity_bytes` is the effective capacity |
| mount_point | string | `/mnt/filedock/pools/<pool-id>` |
| health | enum | See the state machine below |
| system_limits | object? | `System` only: `max_usage_percent` (int 10–90, default 50) and `reserved_free_bytes` (≥ 20 GB, default 100 GB). Audited. Lowering a limit below current usage is refused (FR-099, FR-101) |
| limiting_factor | enum? `MaxUsage` \| `Reserve` | `System` only, observed. Which limit sets the effective capacity |
| writable | enum `Writable` \| `Full(capacity)` \| `Full(reserve)` | Observed. `System` only, under FR-100 |
| availability | enum `Available` \| `Unavailable(reason)` | `System` only. `Unavailable` when the limits give no capacity. Folders can't be created in it then |

The pool root holds the marker `/.filedock-pool.json` and the reserved directory `/.filedock/`.
Neither is ever part of a folder (FR-061).

**Pool state transitions**:

```text
Creating ──ok──▶ Healthy ◀──────────────┐
   │                │  SMART warning     │ warning cleared
   │fail            ▼                    │
   ▼             Degraded ───────────────┘
Failed ◀──── any member missing / mount fail / I/O errors (from Healthy or Degraded)
   │
   └─ admin maps replacement disks ──▶ Recreating ──ok──▶ Healthy (same pool id)
Healthy ──add disk──▶ Extending ──ok──▶ Healthy
```

When a pool enters `Failed`, all of its folders become `Unavailable`, and a critical alert lists
each folder and its recovery path (FR-064).

**System Pool**: it is created as `Healthy` by the installer, or as `Unavailable` if the disk is
too small. The administrator can't extend or recreate it. It has its own transitions:

```text
Healthy ──change limits──▶ Resizing (journaled; shrink briefly stops its exports) ──ok──▶ Healthy
Writable ──usage = max size──▶ Full(capacity) ──space freed──▶ Writable
Writable ──free < reserve + guard──▶ Full(reserve) ──free > reserve + 2×guard──▶ Writable
(restore) ──▶ recreated on the system disk with the same pool id and limits (FR-103)
```

### ManagedFolder

| Field | Type | Rules |
|---|---|---|
| name | string | 1–64 characters, unique (FR-066) |
| filedock_path | string | Under `/storage/`, unique, and never inside or containing another folder's path |
| storage_mode | enum `LocalOnly` \| `CloudMount` \| `HybridBackup` \| `TieredStorage` | Exactly one, covering the whole subtree (FR-069). This feature implements `LocalOnly`, and the other values are accepted only once their capability providers exist |
| reference | ReferencePath | See below |
| data_importance | enum? `Critical` \| `Important` \| `Rebuildable` | Present only for `HybridBackup` (FR-075). Feature 002 owns subfolder overrides |
| access | set of `Smb`, `Nfs`, `WebDav`, `Docker` | Any combination, including none |
| advanced | json | Per mode. Empty for `LocalOnly` |
| required_capabilities | list | Derived from mode and access, such as `storage.local` and `protocol.smb` |
| status | enum `Pending` \| `Ready` \| `NotReady(missing capability)` \| `Unavailable(pool failed)` | |
| protection_state | enum `Protected` \| `AtRisk` \| `Unprotected` \| `NotRequired` | `Unprotected` for `LocalOnly` |

**ReferencePath** (a value object):

| Mode | Fields |
|---|---|
| LocalOnly | `pool_id`, `relative_path` |
| CloudMount | `connection_id`, `remote_path` (feature 004) |
| HybridBackup | `pool_id`, `relative_path`, plus `backup_destination` (feature 002) |
| TieredStorage | `pool_id`, `relative_path`, `connection_id`, `remote_path` (feature 003) |

Validation (FR-070):

- Local reference paths can't be the same as, inside, or containing another folder's local
  reference path.
- A local reference path can't be inside a FileDock-internal location. It must be a directory
  below the pool root, never the root itself or anything under `/.filedock/`.
- A folder can be created in the System Pool only while it is `Available`. Creation returns the
  FR-102 warning.
- Tiered cloud paths can't overlap any other folder's cloud path.
- A backup destination can't be inside any folder's reference path.
- The local reference path can't change after creation (FR-072).

### Export

| Field | Type | Rules |
|---|---|---|
| folder_id | FK → ManagedFolder | Requires the matching protocol in the folder's `access` |
| protocol | enum `Smb` \| `Nfs` \| `WebDav` | |
| rules | list | SMB and WebDAV: share users or groups with `read` or `read-write`. NFS: client host or CIDR with `ro` or `rw` |
| status | observed | WebDAV includes the allocated port |

### ShareUser

| Field | Type | Rules |
|---|---|---|
| username | string | Unique. Becomes a Linux system user with no shell. Can't sign in to the management UI |
| password_secret | SecretRef | Stored in the secret store, and also as the Samba password and WebDAV htpasswd entry |
| enabled | bool | |

### CloudConnection

| Field | Type | Rules |
|---|---|---|
| name | string | Unique |
| provider_family | enum `s3-compatible` \| `azure-blob` \| `google-drive` \| `onedrive` \| `dropbox` \| `x-<connector>-<name>` | Must be served by at least one enabled connector (FR-083, FR-114) |
| definition | one of S3Definition, BoundDefinition | See below |
| route_overrides | map use → RouteTarget | Optional, one per use. Overrides the family default for this connection only (FR-115) |
| object_storage | derived bool | True for `s3-compatible` and `azure-blob`. Only these can be single-writer locations (FR-088) |
| last_test | ConnectionTestResult? | From the latest connection test or daily probe |
| health | enum `Ok` \| `Expiring` \| `Failed` | `Expiring` 7 days before a credential that can't be refreshed expires (FR-084). Never `Expiring` for static S3 keys |
| in_use_by | derived | A connection in use can't be removed (FR-083) |

**S3Definition** (connector-neutral, for `s3-compatible` only):

| Field | Type | Rules |
|---|---|---|
| endpoint | URL | `https` only |
| region | string | |
| addressing_style | enum `Path` \| `Virtual` | |
| bucket | string | |
| access_key_ref, secret_key_ref | SecretRef | Static keys only. No session token, role, or environment credentials (FR-114) |
| storage_class | string? | Passed through to the service |
| server_side_encryption | `ProviderDefault` \| `Kms(key id)` | |
| bandwidth_limit | bytes/s? | Enforced by the connector (FR-117) |

**BoundDefinition** (every other family): `connector_id` → Subsystem, plus `config`, which is json
validated against that connector's `connectionSchema`, and `credentials` (SecretRef[]). The
connection is tied to that connector, so its routes can only point there.

**ConnectionTestResult** (value object):

| Field | Type | Rules |
|---|---|---|
| tested_at | timestamp | |
| connector_id | → Subsystem | The connector that served the test (US10 scenario 2) |
| checks | list of (name, ok, detail) | list, write, read, verify, delete, clock skew |
| conditional_create, conditional_replace | bool? | Proven by the probe (research R23). Both are needed for conditional claim safety. `null` (not tested) after a read-only probe, which never counts as proven |
| provider_checksums | bool | |
| clock_skew_ms | int | A warning above 5 minutes |

### CloudConnector (view over Subsystem)

Any Subsystem whose manifest has a `cloud` section (FR-112).

| Field | Type | Rules |
|---|---|---|
| subsystem_id | → Subsystem | `fd-s3` and `rclone` are `BuiltIn` (FR-113) |
| families | list of (family, capabilities, conditional_writes, provider_checksums, connection_schema?) | From the manifest |
| enabled | bool | Can't be false while any route targets it (FR-121). Built-in connectors can be disabled, never removed |
| third_party | derived bool | True when class is `Extension` |

### ConnectorRoutes (singleton desired-state resource)

| Field | Type | Rules |
|---|---|---|
| defaults | map (family, use) → RouteTarget | Seeded with the defaults in research R13. Uses: `OwnedObjectStorage`, `BackupRepository`, `CloudMount` |
| generation | int | Advanced by every route change. Write sessions pin the route they started with (FR-118) |
| acknowledgements | list of (kind, scope, audit_entry_id) | `BasicClaimSafety` and `ThirdPartyOwnedStorage` (FR-119, FR-123), kept so restore re-applies acknowledged routes |

**RouteTarget**: `Connector(subsystem_id)`, or `BackupEngine` (allowed only for
`BackupRepository`, FR-116).

Validation on every change:

- the target is enabled;
- the target declares the family and the capability the use needs;
- the use is allowed for the family (FR-088);
- a BoundDefinition connection may only target its own connector;
- every acknowledgement that `PreviewRouteChange` reported is present.

**Resolution**: the connection's override for the use, then the family default. Only `fd-s3` and
`rclone` can be targets while restoring, before extensions are reinstalled (FR-122).

### ContainerApp

| Field | Type | Rules |
|---|---|---|
| name | string | Unique. The Compose project is `fd-<name>` |
| compose | text | Multi-container definition (FR-085) |
| environment | map | Non-secret values |
| secrets | SecretRef[] | Application secrets |
| bindings | derived list of (folder_id, container path) | Each folder must have `Docker` access |
| warnings | derived | Persistent data outside managed folders (FR-087) |
| status | observed | Per-service container state |

Classification: the definition, environment, and secrets are authoritative critical state, and the
images and runtime data are rebuildable (FR-086).

### Subsystem / Extension

| Field | Type | Rules |
|---|---|---|
| subsystem_id | string | Taken from the manifest `id`, and unique |
| class | enum `Core` \| `BuiltIn` \| `Extension` | Core subsystems can't be removed |
| manifest | json | Validated against `contracts/subsystem-manifest.schema.json` |
| version, previous_version | string | `previous_version` is kept for rollback (FR-055) |
| approved_permissions | list | The permissions the administrator approved at install |
| signature | enum `Signed(key id)` \| `Unsigned(acknowledged)` | |
| state | enum | See the state machine below |
| restart_count, last_failure_at | | Backoff and the Failed threshold (research R14) |
| package_sha256 | string | `Extension` only. Hash of the installed `.fdext`, which is stored as a `.fdpkg` at every copy (FR-110) |
| data_bindings | list of DataBinding | One per manifest `data` entry. All are required before install (FR-109) |

**Extension state transitions**:

```text
Validating ─invalid─▶ Rejected
    │ok
AwaitingApproval ─declined─▶ Rejected
    │approved
Installing ─fail─▶ Rejected (nothing left installed)
    │ok
Running ⇄ Restarting (backoff) ── 5 failures / 15 min ──▶ Failed
    │upgrade
Upgrading ─health ok─▶ Running
    └─health fail─▶ RollingBack ─▶ Running (previous version)
Running ─remove (no dependent folders)─▶ Removed
(restore) ─package valid at some copy─▶ Installing ─▶ Running (recorded config, permissions, bindings)
(restore) ─no valid package─▶ PackageNeeded ─admin uploads same version─▶ Installing
```

### DataBinding (value object on Subsystem)

| Field | Type | Rules |
|---|---|---|
| name | string | Matches a manifest `data[].name` |
| folder_id | FK → ManagedFolder | The folder can't be deleted while bound. The extension may write persistent data only here (FR-109) |
| mount_path | string | From the manifest. Where the folder appears inside the runtime |
| protected | derived bool | False when the folder's protection state is `Unprotected`. Install then needs an acknowledgement, and the extension is shown as having unprotected data |

### StoredPackage (per appliance-state copy)

| Field | Type | Rules |
|---|---|---|
| sha256 | string | Of the plaintext `.fdext`. Also the file name `packages/<sha256>.fdpkg` |
| subsystem_id, version | | |
| copies | list → ApplianceStateCopy | Written to a copy before any snapshot there refers to it. Pruned when no retained snapshot in that copy refers to it (FR-110) |

### CapabilityBinding

| Field | Type | Rules |
|---|---|---|
| capability | string | Such as `protocol.smb` or `notify.channel` |
| provider_ids | list → Subsystem | All registered providers |
| default_provider_id | → Subsystem? | Chosen by the administrator when there are several (FR-051) |

Note that `notify.channel` is used fan-out style: every registered channel receives alerts, not
only the default provider. `cloud.remote` and `cloud.mount` are not resolved through
`default_provider_id`, but per family and use through ConnectorRoutes (FR-051, FR-115).

### NotificationChannel

| Field | Type | Rules |
|---|---|---|
| subsystem_id | → Subsystem | It must provide `notify.channel` |
| name | string | |
| min_severity | enum `Info` \| `Warning` \| `Critical` | |
| config | json, plus SecretRef[] | |

### ApplianceStateCopy

Every place snapshots are written. Local copies are created and removed by the Core as pools come
and go. The remote destination is the only one the administrator configures.

| Field | Type | Rules |
|---|---|---|
| kind | enum `LocalPool` \| `LocalSystemDisk` \| `RemoteObjectStorage` \| `RemoteRemovableDisk` | One `LocalSystemDisk` copy. One `LocalPool` copy per Healthy data pool (FR-104). At most one remote copy (FR-037) |
| pool_id | FK → Pool? | `LocalPool` only. The path is `<pool root>/.filedock/appliance-state/` |
| connection_id + bucket + prefix | | `RemoteObjectStorage` only. Subject to ownership claims (FR-089) |
| disk_path | string? | `RemoteRemovableDisk` only |
| last_snapshot_at | timestamp? | Shown per copy (FR-040) |
| covered_change_seq | int | The newest authoritative-change sequence stored in this copy |
| stale | bool | True when a change older than 5 minutes isn't covered yet. Raises a warning alert (FR-038, FR-040) |
| last_error | string? | Redacted |

**Derived protection summary** (FR-105), shown on the dashboard:

| Loss | Covered when | Needs |
|---|---|---|
| Reinstall on this machine | A non-stale `LocalPool` copy exists. The `LocalSystemDisk` copy alone covers only a reinstall that keeps the OS disk | Recovery key only |
| Loss of the machine or its disks | A non-stale remote copy exists | Recovery key and remote destination |

### OwnedLocation

| Field | Type | Rules |
|---|---|---|
| connection_id, bucket, prefix | | Unique as a combination |
| purpose | enum `ApplianceState` \| `BackupDestination` \| `TierTarget` | The last two are used by features 002 and 003 |
| owner_appliance_id | uuid | |
| generation | int | Increased by each takeover |
| last_write_at | timestamp | |
| role | enum `Writer` \| `RecoveryReader` \| `OwnershipLost` | FR-094 |
| served_by | derived → Subsystem | Resolved route for (connection, `OwnedObjectStorage`). Shown with built-in or third-party (FR-094, FR-119) |
| claim_safety | derived enum `Conditional` \| `Basic` | `Conditional` only when `served_by` declares conditional writes for the family and the connection's latest test proved both conditions. Azure Blob through rclone is always `Basic` (FR-123) |
| basic_acknowledged | audit_entry_id? | Required while `claim_safety = Basic`. Its absence blocks accepting the location. The dashboard warning `location.basic_claim_safety` shows while Basic |

**OwnedLocation role transitions**:

```text
(empty prefix) ──claim──▶ Writer
Writer ──pre-write check finds other id/higher generation──▶ OwnershipLost (critical alert)
(restore) ──read another appliance's location──▶ RecoveryReader
RecoveryReader ──admin confirms takeover (generation+1)──▶ Writer
RecoveryReader ──reinstall restore, same generation as snapshot, last write > 15 min ago
               ──automatic takeover (generation+1, audited as System)──▶ Writer      (FR-108)
```

### SnapshotRecord (index of appliance-state snapshots)

| Field | Type | Rules |
|---|---|---|
| snapshot_id | uuid | |
| taken_at | timestamp | |
| appliance_id, appliance_name | | Listed during restore (FR-042) |
| key_id | string | Identifies the recovery key that wraps this snapshot (FR-034) |
| size_bytes, sha256 | | Verified before restore (FR-043) |
| retention_class | enum `Recent` \| `Hourly` \| `Daily` | Per copy: 30 recent, 24 hourly, and 30 daily (FR-039) |
| copies | list → ApplianceStateCopy | The copies that hold this snapshot |
| package_refs | list of sha256 | Stored packages this snapshot needs (FR-110) |

SnapshotRecord rows are a local index of what each copy holds. They are not in the snapshot
itself: after a restore they are rebuilt from each copy's `index.json`.

### Secret

| Field | Type | Rules |
|---|---|---|
| ref | string | Such as `connection/<id>/token` |
| ciphertext, nonce | bytes | XChaCha20-Poly1305 under the local DEK |
| consumers | list → Subsystem | Only these subsystems receive it (FR-031) |
| masked_hint | string | Such as `••••a1f3`. The only form ever shown (FR-030) |

### RecoveryKeyInfo

| Field | Type | Rules |
|---|---|---|
| key_id | string | |
| salt, kdf_params | | Argon2id |
| wrapped_dek | bytes | The DEK wrapped by the derived key |
| created_at, superseded_at | | Regeneration supersedes the old key for new snapshots |

### Operation (journal)

| Field | Type | Rules |
|---|---|---|
| kind | enum | Such as `PoolCreate`, `PoolExtend`, `PoolRecreate`, `SystemPoolResize`, `ExtensionInstall`, `ExtensionUpgrade`, `ApplianceRestore`, `Takeover`, `SnapshotUpload` |
| state | enum | Kind-specific. Every kind ends in `Succeeded`, `Failed`, or `RolledBack` |
| steps | list of OperationStep | Each step has an `intent` recorded before acting, a `result`, and an attempt count |
| resource_ref | | The resource being acted on |
| started_by | | An audit actor |

**Generic operation lifecycle**:

```text
Pending ─▶ Running(step n) ─▶ Succeeded
              │crash/restart: resume step n (idempotent)
              ├─unrecoverable─▶ RollingBack ─▶ RolledBack
              └─fatal─▶ Failed
```

### Alert

| Field | Type | Rules |
|---|---|---|
| dedup_key | string | `(source, object, kind)` |
| severity | enum `Info` \| `Warning` \| `Critical` | |
| source, object_ref | | |
| message | string | Redacted |
| first_seen_at, last_seen_at | timestamp | |
| state | enum `Active` \| `Acknowledged` \| `Resolved` | Resolved automatically when the condition clears (FR-096) |
| acknowledged_by | actor? | |

**Alert state transitions**:

```text
Active ──admin ack──▶ Acknowledged
Active | Acknowledged ──condition clears──▶ Resolved
Resolved ──condition recurs──▶ Active (same dedup_key, new first_seen_at)
```

### Event (stream)

| Field | Type | Rules |
|---|---|---|
| seq | int | Monotonic, used to resume the stream |
| at | timestamp | |
| type | enum | At least the types listed in FR-018 |
| subject | resource ref | |
| payload | json | Redacted |

Events are kept in a ring of the last 7 days. They are disposable.

---

## Data classification (FR-035)

| Class | Entities |
|---|---|
| Authoritative critical state (in the snapshot, as `config.json` + secrets) | All desired-state resources: Pool (identity, name, kind, layers, System Pool limits, and never disks), ManagedFolder, Export, ShareUser, CloudConnection (with route overrides, but not `last_test`), ConnectorRoutes, connector enabled flags, ApplianceStateCopy (the remote destination only), OwnedLocation, ContainerApp, Subsystem registry with configuration and DataBindings, CapabilityBinding defaults, NotificationChannel. Plus Secret, RecoveryKeyInfo, Administrator, ApiToken metadata and hashes, and settings. Stored packages travel next to the snapshot |
| Authoritative user data | Content of Local Only folders in pools, including the System Pool, and extension and application data in their bound folders. Each is protected by its folder's mode, never by the snapshot |
| Rebuildable | Container images and runtime data, observed status, PhysicalDisk records, SnapshotRecord index, connection test results (re-probed after restore) |
| Disposable with local retention (kept on this appliance for its retention period, not in snapshots, starts fresh after a restore) | AuditEntry, Alert, Operation journal |
| Disposable | Session, setup code, Event, operational logs, caches, materialized tmpfs secrets |

Sessions are deliberately excluded from snapshots, so administrators sign in again after a
restore, with their existing passwords.
