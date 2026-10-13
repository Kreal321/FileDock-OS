# Management REST API v1 (contract)

**Served by**: the Management Service, over HTTP on port 8021 for both UI and API (FR-027).
**Base path**: `/api/v1`.
**Clients**: the web UI, and scripts that use API tokens (research R4).

The machine-readable OpenAPI 3.1 document is generated from the implementation at
`/api/v1/openapi.json` and must stay consistent with this contract.

## Conventions

- **Authentication**: either a session cookie `fd_http_session` with the header `X-CSRF-Token` on
  mutations, or `Authorization: Bearer fdt_<id>_<secret>` (FR-021). Requests without valid
  credentials get `401`. A Read-only token on any mutating route gets `403`. Both are audited.
- **Before setup**: until setup is complete, every route except `/setup/*` and `/health` returns
  `423 Locked` (FR-020). Every `/setup/*` route requires the setup code, except the
  `/setup/restore/local` routes, which require the recovery key instead (FR-106).
- **Errors**: `application/problem+json`. Validation errors include
  `errors: [{code, field, message, conflicting}]`, mirroring the Core `ValidationIssue`.
- **Concurrency**: resource responses carry `generation`. Updates send `If-Match: <generation>`,
  and a mismatch returns `409`.
- **Disk full**: a change that can't be committed because the system disk is out of space
  returns `507` with code `storage.disk_full`, and nothing is committed (SC-013).
- **Long-running work**: returns `202 Accepted` with `{operationId}`. Progress is available from
  `/operations/{id}` and SignalR.
- **Pagination**: `?pageSize=&pageToken=`. Responses include `nextPageToken`. There are no hard
  caps (research R20).
- **Secrets**: request fields named `*Secret`, `password`, or `credentials` are write-only.
  Responses contain only `maskedHint`.

## Scope legend

| Scope | Meaning |
|---|---|
| R | Read-only token, Full token, or session |
| F | Full token or session |
| S | Session only. Tokens are refused (FR-021) |
| P | Public before setup only |

## Endpoints

### Setup, sessions, administrators, tokens

| Method and path | Scope | Purpose | FR |
|---|---|---|---|
| `GET /health` | public | Liveness, with no details | — |
| `POST /setup/verify-code` `{code}` | P | Validates the one-time setup code. Throttled | FR-020 |
| `POST /setup/complete` `{code, applianceName, adminUsername, adminPassword, adminPasswordConfirmation}` | P | Creates the first administrator, signs them in (sets `fd_http_session`), and generates the recovery key | FR-020, FR-033 |
| `POST /setup/recovery-key/confirm` `{keyId}` | S | Acknowledges the recovery-key download and finishes setup. The request identifies the current recovery key | FR-033 |
| `POST /setup/restore/...` | P | Restore Appliance entry, the alternative to setup (see Recovery) | FR-041 |
| `POST /auth/sign-in` `{username, password}` | public | Creates a session. Locks the account for 15 minutes after 5 failures | FR-024 |
| `POST /auth/sign-out` / `POST /auth/sign-out-all` | S | | FR-023 |
| `GET/POST /admins`, `PATCH/DELETE /admins/{id}` | S | Last enabled administrator protected | FR-025 |
| `POST /admins/{id}/password` | S | Change own password | FR-022 |
| `GET /tokens` | S | List tokens with name, scope, expiry, last use, and revoked state | FR-021 |
| `POST /tokens` `{name, scope: ReadOnly\|Full, expiresAt?}` | S | Returns `{token}` once | FR-021 |
| `DELETE /tokens/{id}` | S | Revokes immediately | FR-021 |

### System, events, alerts, logs, audit

| Method and path | Scope | Purpose | FR |
|---|---|---|---|
| `GET /system/status` | R | Health of services and subsystems, snapshot age, notices | FR-019, FR-040, FR-097 |
| `GET /system/network` / `PUT /system/network` `{listenAddresses[], confirmDisconnect?}` | R / S | Restrict which interfaces serve the UI | FR-027 |
| SignalR hub `/hubs/events` | R | Live events, relayed from the Core stream with resume by `seq` | FR-019 |
| `GET /alerts?state=&minSeverity=` | R | Alert list | FR-096 |
| `POST /alerts/{id}/ack` | F | Acknowledge an alert | FR-096 |
| `GET /logs?source=&minSeverity=&since=&until=&q=` | R | Unified logs, newest first | FR-057 |
| `GET /audit?actor=&action=&since=` | R | Audit history | FR-059 |
| `POST /diagnostics/bundle` | F | Streams a redacted `tar.zst` | FR-060 |
| `GET /operations`, `GET /operations/{id}` | R | Journaled operations | FR-015 |

### Local storage

| Method and path | Scope | Purpose | FR |
|---|---|---|---|
| `GET /disks` | R | Physical disks with health and pool membership | FR-061 |
| `GET /pools`, `GET /pools/{id}` | R | Pools with health, capacity, and folders | FR-061 |
| `POST /pools` `{name, diskSerials[], typedConfirmation}` | F | Create from blank disks. Returns `202` | FR-062 |
| `POST /pools/adopt` `{name, diskSerial}` | F | Adopt an existing filesystem without erasing it. `202` | FR-062 |
| `POST /pools/{id}/disks` `{diskSerial, typedConfirmation}` | F | Grow a pool. `202` | FR-062 |
| `POST /pools/{id}/recreate` `{diskSerials[], typedConfirmation}` | F | Recreate a failed pool with the same identity. `202` | FR-064 |
| `GET /pools/system` | R | System Pool: limits, effective capacity, limiting factor, usage, writable state, availability | FR-098, FR-099 |
| `PUT /pools/system/limits` `{maxUsagePercent, reservedFreeGb, acknowledgedInterruption?}` + `If-Match` | S | Change limits. `202`. `422` with `system_pool.below_usage` when below current usage, or `system_pool.interruption_unacknowledged` when shrinking without acknowledgement | FR-099, FR-101 |

`typedConfirmation` must equal `ERASE <serial> <serial> ...`, listing the target disks in order.

For the System Pool, every other pool mutation (`DELETE`, rename, `/disks`, `/recreate`, `/adopt`)
returns `422` with code `system_pool.immutable` (FR-101).

### Managed folders and shares

| Method and path | Scope | Purpose | FR |
|---|---|---|---|
| `POST /folders/validate` | F | Dry-run of the six-section flow. Returns issues and warnings | FR-065..FR-072 |
| `GET /folders`, `GET /folders/{id}` | R | Folder list: name, path, mode, access, protection state, and importance for Hybrid Backup | FR-079 |
| `POST /folders` | F | Create (body below) | FR-065..FR-077 |
| `PATCH /folders/{id}` | F | Edit through the same sections and validation. Changes to the local reference path are refused | FR-065, FR-072 |
| `DELETE /folders/{id}` | F | Refused while exports, containers, or extension data bindings depend on it | FR-109 |
| `GET/POST /share-users`, `PATCH/DELETE /share-users/{id}` | R / F | Share users. They can't use the UI | — |
| `GET/PUT /folders/{id}/exports/{smb\|nfs\|webdav}` | R / F | Export rules | FR-082 |

Create-folder request body:

```json
{
  "identity": { "name": "Movies", "fileDockPath": "/storage/movies" },
  "storageMode": "LocalOnly",
  "reference": { "poolId": "0192…", "relativePath": "/movies" },
  "dataImportance": null,
  "access": ["Smb", "Nfs", "Docker"],
  "advanced": {}
}
```

`dataImportance` is required when `storageMode` is `HybridBackup`, and must be `null` otherwise
(FR-075). Modes whose capability has no provider return `422`, with code `mode.capability_missing`
naming the capability (FR-067).

More reference-path rules:

- `relativePath` must be a directory below the pool root. `/` and anything under `/.filedock`
  return `422` with `reference.pool_root_reserved` (FR-061).
- A folder in the System Pool returns the warning `system_pool.shared_with_os` from `validate`
  (FR-102).
- While the System Pool is Unavailable, a folder there returns `422` with
  `system_pool.unavailable` (FR-101).

### Cloud connections, containers, extensions, notifications

| Method and path | Scope | Purpose | FR |
|---|---|---|---|
| `GET/POST /connections`, `PATCH/DELETE /connections/{id}` | R / F | Each connection has a `providerFamily`. S3-compatible connections take the connector-neutral definition (`endpoint`, `region`, `addressingStyle`, `bucket`, `accessKey`, `secretKey`, `storageClass`, `serverSideEncryption`, `bandwidthLimit`). Only static keys are accepted, so `sessionToken` or `roleArn` get `422 connection.credential_type_unsupported`. Other families take the owning connector's schema. A family no enabled connector serves gets `422 connection.family_unserved`. Listings show `routeOverrides` and the effective connector per use. Removal is refused while the connection is in use | FR-083, FR-114, FR-115 |
| `POST /connections/{id}/test` | F | Runs the routed connector's probe. Returns `connectorId`, the checks (list, write, read, verify, conditional create and replace, delete, clock skew), `providerChecksums`, and `clockSkewMs` | FR-083, FR-117 |
| `GET /connectors` | R | Cloud connectors (built-in and third-party), with enabled flag, state, families and capabilities, conditional-write support, and the routes that target each one | FR-112, FR-113 |
| `PATCH /connectors/{id}` `{enabled}` | F | Enable or disable. `409 connector.in_use` lists the routes while any route targets it. Built-ins can't be removed | FR-121 |
| `GET /connector-routes` | R | Family defaults and per-connection overrides, plus `generation` | FR-115 |
| `POST /connector-routes/preview` `{changes[], expectedGeneration}` | F | Impact: affected connections, folders, and owned locations (with resulting claim safety), lost features, credential recipients, mounts that restart, and the acknowledgements required. Nothing changes | FR-118, FR-119, FR-123 |
| `PUT /connector-routes` `{changes[], expectedGeneration, acknowledgedBasicClaimSafety?, acknowledgedThirdPartyOwnedStorage?}` | F | Apply. Each change sets a family default (`{family, use}`) or a connection override (`{family, use, connectionId}`). A null target removes an override. Errors: `409 route.generation_mismatch`, `422 route.ack_required` (names the missing acknowledgement), `422 route.connector_incapable`, `422 route.connector_disabled`, `422 route.use_not_allowed` (FR-088). Audited, including each acknowledgement | FR-115, FR-118, FR-119, FR-123 |
| `POST /connections/oauth/start` → `{authUrl, flowId}`; `POST /connections/oauth/finish` `{flowId, redirectUrl}` | F | Browser authorization with the paste-back flow (research R13) | FR-083 |
| `GET/POST /apps`, `PATCH/DELETE /apps/{id}`, `POST /apps/{id}/{start\|stop\|update}` | R / F | Compose applications. The response includes binding warnings | FR-085..FR-087 |
| `GET /subsystems` | R | Subsystems, capabilities, state, and default providers | FR-050 |
| `PUT /capabilities/{capability}/default` `{subsystemId}` | F | Choose the default provider | FR-051 |
| `POST /extensions/inspect` (multipart `.fdext`) | F | Validate the package. Returns requested permissions, signature state, and declared data paths, each with a suggested Hybrid Backup folder | FR-052, FR-109 |
| `POST /extensions` `{stagedPackageId, approvedPermissions[], acknowledgedUnsigned, dataBindings: {pathName: folderId}, acknowledgedUnprotectedData?}` | F | Install. `202`. Every declared data path must be bound. A path bound to an unprotected folder needs `acknowledgedUnprotectedData`, otherwise `422 extension.data_unprotected`. The package is then stored with every appliance-state copy | FR-052, FR-109, FR-110 |
| `POST /extensions/{id}/upgrade`, `DELETE /extensions/{id}` | F | Upgrade with automatic rollback. Removal guarded | FR-055, FR-056 |
| `POST /extensions/{id}/package` (multipart `.fdext`) | F | For an extension in `PackageNeeded` after a restore: supply the recorded version. It installs with the recorded configuration, permissions, and bindings. A different version gets `422 extension.version_mismatch` | FR-110 |
| `GET/POST /notification-channels`, `PATCH/DELETE /notification-channels/{id}`, `POST /notification-channels/{id}/test` | R / F | `notify.channel` configuration | FR-097 |

### Secrets, recovery, appliance state, restore, ownership

| Method and path | Scope | Purpose | FR |
|---|---|---|---|
| `GET /secrets` | R | Masked list | FR-030 |
| `POST /recovery-key/regenerate` | S | Returns the new key once, then requires confirmation | FR-034 |
| `GET /state-protection` | R | Every copy (local pools, system disk, remote) with last snapshot time and stale flag, plus the reinstall and machine-loss coverage summary | FR-038..FR-040, FR-104, FR-105 |
| `PUT /state-protection/destination` `{objectStorage\|removableDiskPath, acknowledgedBasicClaimSafety?, acknowledgedThirdPartyConnector?}` | S | Register or replace the one remote destination. When the resolved claim safety is basic, `422 location.basic_claim_ack_required` is returned until acknowledged. A third-party connector needs its own acknowledgement | FR-037, FR-119, FR-123 |
| `POST /state-protection/snapshot` | F | Snapshot now to every copy. `202` | FR-038 |
| `GET /state-protection/backup-file` | S | Downloads the newest snapshot as an encrypted `.fdbackup` file, including its extension packages. Tokens are refused, and the download is audited as `appliance.backup_exported` | FR-111 |
| `GET /setup/restore/local` | public before setup | Appliances found on this machine's disks: name, newest snapshot time, where found. Empty when none | FR-106 |
| `POST /setup/restore/local` `{localSourceToken, recoveryKey}` | public before setup | Opens a local copy. The recovery key is the only input, with no setup code. Throttled like the setup code. Starts a restore session | FR-106, FR-107 |
| `POST /setup/restore/source` `{objectStorage\|removableDiskPath, providerFamily, connectionDefinition, credentials, recoveryKey}` | P | Opens a remote source as a Recovery reader, through the built-in connector for its family. Requires the setup code | FR-042, FR-092, FR-122 |
| `POST /setup/restore/file` (multipart `.fdbackup` + `recoveryKey`) | P | Opens an uploaded configuration backup file. Requires the setup code. `422 backup_file.invalid` or `backup_file.too_new` with nothing changed | FR-042, FR-111 |
| `GET /setup/restore/snapshots` | P (restore session) | Snapshots with integrity flag | FR-042, FR-043 |
| `POST /setup/restore` `{snapshotId}` → `202` | P (restore session) | Apply the snapshot | FR-044 |
| `POST /setup/restore/pool-mappings` `{mappings[]}` | P (restore session) | Assign disks to recorded pools whose disks are missing. Each pool keeps its recorded name and identity. Present pools are reattached automatically, and the System Pool is recreated without mapping | FR-063, FR-103 |
| `GET /restore/report` | R | The latest restore's report: each folder in FR-081 order with its outcome (`DataPresent`, `AwaitingDataRestore`, `Reconnected`, `ToRebuild`, `Lost`, `PoolUnmapped`), every other restored item with its status, and the snapshot time after which changes are lost. Shown until dismissed | FR-046, FR-081 |
| `POST /restore/report/dismiss` | F | Dismisses the report. Audited | FR-046 |
| `GET /locations` | R | Owned locations with owner, generation, last write, role, `servedByConnector`, `servedByThirdParty`, and `claimSafety` (`Conditional` or `Basic`) | FR-094, FR-119, FR-123 |
| `POST /locations/takeover` `{connectionId, bucket, prefix, observedGeneration, acknowledgedRecentWriter}` | S | Confirmed takeover. Audited | FR-092 |

A "restore session" is a short-lived session that exists only during restore and before any
administrator exists. It is bound either to the setup code, for a remote source or an uploaded
file, or to a successfully verified recovery key, for a local copy (FR-106, FR-111). When a restore from a local copy
completes, the takeovers recorded in the snapshot run automatically, with the exceptions in
FR-108.
