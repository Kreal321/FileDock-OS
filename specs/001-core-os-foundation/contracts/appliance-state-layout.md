# Appliance-State Snapshot and Ownership Layout (contract)

This is the on-destination format that any FileDock release must read in order to restore. It is
versioned, and changes must stay readable by later releases (Principle III).

## Copy locations

Every snapshot is written to every copy, and each copy uses the same layout below its root
(FR-104, research R22):

| Copy | Root | `_filedock/owner.json` |
|---|---|---|
| Each Healthy data pool | `<pool root>/.filedock/appliance-state/` | No. The pool belongs to this appliance |
| System disk | `/var/backups/filedock/appliance-state/` | No |
| Remote: object storage | `<bucket>/<prefix>/`, configured by the administrator | Yes |
| Remote: removable disk | `<chosen directory>/` | No |

## Layout under each copy root

```text
<copy root>/
├── _filedock/
│   └── owner.json                         # single-writer claim (remote object storage only)
└── appliance/
    └── <appliance-id>/
        ├── index.json                     # signed list of snapshots
        ├── keys/
        │   └── <key-id>.json              # recovery-key wrap metadata (salt, KDF params)
        ├── packages/
        │   └── <package-sha256>.fdpkg     # one per installed extension version (FR-110)
        └── snapshots/
            └── <taken-at-utc>_<snapshot-id>.fdstate
```

## `owner.json`

This file implements FR-090 to FR-092. It is written only by the owning appliance, or by a
confirmed takeover.

```json
{
  "schemaVersion": 1,
  "applianceId": "0192d3c4-…",
  "applianceName": "filedock-home",
  "generation": 3,
  "claimedAt": "2026-10-11T16:02:11Z",
  "lastWriteAt": "2026-10-11T16:40:00Z",
  "purpose": "ApplianceState"
}
```

Rules:

- **Claim**: allowed only when the prefix is empty, or this file names this appliance. All access
  goes through the connector routed for the location (research R13).
  - With **conditional** claim safety, the writes are conditional: `CREATE_ONLY`
    (`If-None-Match: *`) for a new claim, and `REPLACE_IF_MATCH` (`If-Match: <etag>`) for a
    takeover and each `lastWriteAt` update.
  - With **basic** claim safety, the appliance writes, then reads back and compares. This needs
    the recorded acknowledgement (FR-123).
- **Same layout through every connector**: each object is stored at exactly
  `<prefix>/<path shown here>`, with no transformation and no connector-specific metadata
  (FR-118).
- **Check before writing**: before each write session, the appliance re-reads the file and
  compares `applianceId` and `generation` with its local record. If they differ, it stops and
  enters the Ownership lost state.
- **After a write**: after a successful session, the appliance updates `lastWriteAt` with a
  conditional write.
- **Takeover**: sets `generation` to the old value plus 1, `applianceId` to the taking-over
  appliance, and both `claimedAt` and `lastWriteAt` to the current time. A restored appliance keeps
  its recorded `applianceId`, so after a restore only `generation` changes.
- **Automatic takeover after a reinstall restore** (FR-108): allowed only when `generation`
  equals the generation recorded in the restored snapshot, and `lastWriteAt` is more than 15
  minutes old. Otherwise the confirmed takeover above applies, or the appliance stays a Recovery
  reader if `generation` is higher.
- **Recovery readers**: never write anything under `_filedock/`.

## `index.json`

```json
{
  "schemaVersion": 1,
  "applianceId": "0192d3c4-…",
  "applianceName": "filedock-home",
  "snapshots": [
    {
      "snapshotId": "0192d3d0-…",
      "takenAt": "2026-10-11T16:40:00Z",
      "file": "snapshots/20261011T164000Z_0192d3d0.fdstate",
      "sizeBytes": 183422,
      "sha256": "…",
      "keyId": "rk-2026-10-11-01",
      "retentionClass": "Recent",
      "packages": ["<package-sha256>"],
      "releaseVersion": "0.1.0"
    }
  ],
  "mac": "…"
}
```

- `mac` is an HMAC-SHA256 over the canonical JSON of `applianceId`, `applianceName`, and
  `snapshots`. Its key is derived from the recovery key. `applianceName` and the snapshot times can
  be read before the key is entered, so reinstall detection can list them (FR-106). Nothing is
  treated as valid until the MAC is verified (FR-043).
- **Write order**: write and fsync the `.fdstate` file, then atomically replace `index.json`
  (write to a temporary file, then rename; on object storage, a single PUT). A crash between the
  two leaves an unlisted file, which the next write cleans up.
- **Retention**: per copy, keep the 30 newest snapshots, the newest of each hour for 24 hours, and
  the newest of each day for 30 days (FR-039). Pruning happens only after the next snapshot is
  durable in that copy. On object storage it also requires that this appliance holds the claim.

## `.fdstate` file

The file is a 16-byte header, followed by an encrypted stream.

| Offset | Content |
|---|---|
| 0–7 | Magic `FDSTATE\0` |
| 8–9 | Format version (u16, currently 1) |
| 10–15 | Reserved |
| 16– | The archive, encrypted with XChaCha20-Poly1305 in 64 KiB chunks (STREAM construction) under a random per-snapshot key |

The per-snapshot key is wrapped by the snapshot key, which the recovery key protects. The wrapped
per-snapshot key is stored in the first archive frame header.

Decrypted, the archive is a `tar` stream compressed with zstd:

```text
manifest.json             # schemaVersion, applianceId, applianceName, takenAt, releaseVersion,
                          #   configSchemaVersion, coreConfigSeq, managementConfigSeq,
                          #   recordedGenerations{location → generation},
                          #   packages[{subsystemId, version, sha256}]
config.json               # the configuration document (FR-036), canonical JSON, see below
secrets/secrets.enc       # secret store, still encrypted under the local DEK (incl. TLS key)
secrets/dek.wrapped       # local DEK wrapped by the recovery-key-derived key
```

`recordedGenerations` are the ownership generations that were current when the snapshot was taken.
The automatic takeover after a reinstall uses them (FR-108). `packages` lists the stored packages
this snapshot needs. Each must already exist under `packages/` in the same copy (FR-110).

### `config.json` (configuration document, schema `fdconfig/1`)

The document is declarative: it describes what to rebuild, not how the old machine stored it.
Secrets appear only as references (`secretRef`) into `secrets.enc`.

```json
{
  "schema": "fdconfig/1",
  "appliance": { "id": "0192d3c4-…", "name": "filedock-home", "settings": { "…": "…" },
                 "certificate": { "certPem": "…", "keySecretRef": "appliance/tls-key" } },
  "core": {
    "pools": [
      { "id": "0192…", "name": "media", "kind": "Lvm", "layers": ["lvm"] },
      { "id": "0193…", "name": "system", "kind": "System",
        "systemLimits": { "maxUsagePercent": 50, "reservedFreeBytes": 107374182400 } }
    ],
    "folders": [ { "id": "…", "name": "Movies", "fileDockPath": "/storage/movies",
                   "storageMode": "LocalOnly",
                   "reference": { "poolId": "0192…", "relativePath": "/movies" },
                   "access": ["Smb", "Nfs", "Docker"], "advanced": {} } ],
    "exports": [], "shareUsers": [],
    "connections": [
      { "id": "c1", "name": "wasabi", "providerFamily": "s3-compatible",
        "s3": { "endpoint": "https://s3.us-east-1.wasabisys.com", "region": "us-east-1",
                "addressingStyle": "Virtual", "bucket": "fd-home",
                "accessKeyRef": "connections/c1/access-key",
                "secretKeyRef": "connections/c1/secret-key",
                "storageClass": null, "serverSideEncryption": "ProviderDefault" },
        "routeOverrides": {} },
      { "id": "c2", "name": "gdrive", "providerFamily": "google-drive",
        "bound": { "connectorId": "rclone", "config": { "…": "…" },
                   "credentials": ["connections/c2/token"] } } ],
    "connectors": { "enabled": ["fd-s3", "rclone"] },
    "connectorRoutes": {
      "generation": 4,
      "defaults": [
        { "family": "s3-compatible", "use": "OwnedObjectStorage", "target": "connector:fd-s3" },
        { "family": "s3-compatible", "use": "BackupRepository", "target": "backup-engine" },
        { "family": "s3-compatible", "use": "CloudMount", "target": "connector:rclone" },
        { "family": "azure-blob", "use": "OwnedObjectStorage", "target": "connector:rclone" },
        { "family": "google-drive", "use": "CloudMount", "target": "connector:rclone" } ],
      "acknowledgements": [] },
    "stateDestination": {},
    "ownedLocations": [], "apps": [], "notificationChannels": [],
    "subsystems": [ { "id": "sample-native", "version": "1.2.0", "packageSha256": "…",
                      "approvedPermissions": ["…"], "config": {},
                      "dataBindings": { "db": "<folder-id>" } } ],
    "capabilityDefaults": {}, "tierMetadata": null
  },
  "management": { "administrators": [], "apiTokens": [], "settings": {} }
}
```

Rules:

- Pools carry identity, name, kind, layers, and System Pool limits. They never carry disk
  serials, device paths, or sizes. Disks are assigned at restore only for pools whose disks are
  missing (FR-063).
- These are never included: audit entries, alerts, snapshot records, operation history,
  sessions, setup state, observed status, events, and logs (FR-036).
- `connectors` and `connectorRoutes` restore as recorded (FR-122). The built-in connectors serve
  restore reads until a routed extension connector is reinstalled. A route to a third-party
  connector for `OwnedObjectStorage`, or a route leaving a location basic, is restored only with
  its acknowledgement recorded here.
- Connection test results are not included. They are re-probed after restore, following the
  probe rules in research R23, so a Recovery reader writes nothing inside a location it doesn't
  own.
- `management` is produced by the Management Service (`PutManagementConfig`) and is opaque to the
  Core.
- Readers must accept every earlier `fdconfig` version. Unknown fields from a newer minor version
  are ignored. A newer major version is refused (FR-111).

## `.fdpkg` file (stored extension package)

The `.fdext` package exactly as installed, encrypted in the same header and STREAM format as
`.fdstate` (magic `FDPKG\0\0\0`). The file name is the SHA-256 of the plaintext package, so each
version is stored once per copy. It is written and fsynced before any `index.json` that lists a
snapshot needing it. It is pruned when no retained snapshot in that copy refers to it.

## `.fdbackup` file (downloadable configuration backup, FR-111)

The file is a single uncompressed `tar` that is a one-snapshot copy root:

```text
appliance/<appliance-id>/index.json          # one snapshot entry, MAC as above
appliance/<appliance-id>/keys/<key-id>.json
appliance/<appliance-id>/snapshots/<…>.fdstate
appliance/<appliance-id>/packages/<sha256>.fdpkg   # every package the snapshot lists
```

Restoring from it opens the file as a removable-disk source. It needs the setup code plus the
recovery key, and no automatic takeover happens.

Restore:

1. Open the source.
   - **Local copy** (reinstall, FR-106): found by `detect-state` (research R22). The recovery key
     is the only input.
   - **Remote source**: opened as a Recovery reader, through the built-in connector for its
     family (FR-122). Requires the setup code and access details.
   - **Uploaded `.fdbackup` file** (FR-111): requires the setup code and the recovery key.
2. Verify the index MAC with the derived key.
3. For a local copy, also open the remote destination recorded in the newest valid snapshot as a
   Recovery reader. Choose the newest valid snapshot across all sources, unless the administrator
   picks another (FR-107).
4. Verify the snapshot's SHA-256 (FR-043).
5. Decrypt and unwrap the DEK.
6. Install the secret store. Import `config.json` into a fresh `core.db` as desired state, upgrading
   older `fdconfig` versions first. Hand `management` to the Management Service, which imports it
   into a fresh `app.db`. Audit history starts with an `appliance.restore` entry.
7. Reattach pools whose disks are present, recreate the System Pool, and have disks assigned to
   the missing pools (FR-063, FR-103).
8. Reinstall extensions from the `.fdpkg` files, with their recorded permissions, configuration,
   and data bindings, without asking again. Then apply the recorded connector routes. Until a
   routed extension connector is Running, its locations are read only through the built-in
   connector. A missing package falls back to another copy, then to
   `PackageNeeded` (FR-044, FR-110). Then reconcile (FR-044).
9. Before any write to an owned location, require a takeover (FR-045, FR-092). After a local
   restore, the takeover is automatic under FR-108.
