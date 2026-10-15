# Phase 0 Research: Core Appliance Foundation and OS Setup

**Feature**: `001-core-os-foundation` | **Date**: 2026-10-11 | **Plan**: [plan.md](./plan.md)

This document records the decisions that resolve every open item in the plan's Technical Context.
The constraints were the spec, constitution v1.1.0, and the product guide (`FileDock-OS.md`). The
user gave these inputs directly:

- There are no scale limits in this release.
- There is no local disk encryption at rest for now, though it may be needed later.
- The system disk provides a System Pool, capped by default at 50% of the disk and keeping at least
  100 GB free.
- Appliance state has a 5-minute recovery point, and a reinstall needs only the recovery key.
- Appliance state is a configuration backup, closer to a configuration file than to a database
  copy. It covers pools (name and identity, not disks), folders, core configuration, admins,
  tokens, and secrets, and no history. Extension data lives in managed folders, protected the
  Hybrid Backup way. Extension packages are kept with the backups. The backup can be downloaded
  as a file (spec Clarifications).
- Ship an official native S3 connector as a built-in subsystem, using the same manifest and
  contract as rclone. It is the default for S3-compatible storage, and rclone stays for broad
  provider coverage and cloud drives. Several connectors can be enabled together, with routes
  deciding what goes where. Reconsider the earlier rejection of `object_store` (spec Revised note,
  FR-112 to FR-123).

Each entry gives the decision, the rationale, and the alternatives considered.

---

## R1. Base OS and platform

- **Decision**: Debian 13 "trixie" (the current Debian Stable), amd64 only, systemd as the init
  system and supervisor.
- **Rationale**: The product guide names Debian Stable and systemd. Debian 13 is the stable
  release on the plan date. One distribution and one architecture keep the pinned bundle
  testable, as guide section 64 requires.
- **Alternatives considered**:
  - Ubuntu LTS: it has a similar package base, but a less conservative default stack.
  - Supporting several distributions: this multiplies the validation matrix.
  - arm64: deferred, because the spec excludes it.

## R2. Installer and pinned bundle

- **Decision**:
  - A single `install.sh` bootstrap installs signed FileDock `.deb` packages:
    - `filedock-core`;
    - `filedock-management`;
    - `filedock-builtins`, a metapackage that depends on exact versions. It also contains the
      native S3 connector `fd-s3`, its binary and manifest (R23).
  - Third-party versions are pinned in `packaging/versions.lock`. The installer applies them with
    apt preferences (`Pin-Priority: 1001`) for Debian and Docker packages. rclone and Kopia use
    their upstream release packages, verified by SHA-256.
  - The installer runs every prerequisite check before changing anything, using a read-only
    preflight phase. Re-running it acts as repair: it reinstalls missing package files and units,
    and never touches `/etc/filedock` state or pools.
  - The installer generates the one-time setup code and prints it to the console.
  - Before printing a setup code on a fresh install, the installer looks for appliance-state copies
    on the machine's disks. If it finds any, it offers a recovery-key-only restore at the console
    (R22).
- **Rationale**:
  - Debian packaging gives clean repair and reinstall, file ownership, and a path to future
    upgrades.
  - Pinning implements FR-003 and guide section 64.
  - Debian's packaged rclone is too old for the features later specs need, which is why upstream
    packages are used.
- **Alternatives considered**:
  - A shell-only installer copying binaries: repair and uninstall are fragile.
  - Ansible: a heavy dependency for one host.
  - A bootable image: out of scope (Stage 3 in the guide).

## R3. Core Service technology

- **Decision**:
  - Rust, stable toolchain, edition 2024.
  - `tokio` async runtime, `tonic`/`prost` for gRPC, `rusqlite` (bundled SQLite, WAL mode) with
    `rusqlite_migration`, `serde` with `serde_yaml`/`serde_json`, and `tracing` logging to
    journald.
  - The Core runs as root under a hardened systemd unit with `ProtectHome`, `PrivateTmp`, and
    `NoNewPrivileges` on helpers. It keeps the capabilities it needs for mounts, LVM, exports,
    and Docker.
- **Rationale**: The guide requires a small, fast, low-memory Rust core. `rusqlite` is synchronous
  and predictable, and blocking work is wrapped in `spawn_blocking`. The idle target (SC-009) is
  easy to meet without an ORM.
- **Alternatives considered**:
  - `sqlx`: it is async and checks queries at compile time, but needs more machinery for an
    embedded SQLite file.
  - Go: it doesn't match the guide.

## R4. Management Service and web UI technology

- **Decision**:
  - Management Service: ASP.NET Core on .NET 10 LTS (minimal APIs), with EF Core and SQLite for
    `app.db`.
  - Browser push: SignalR.
  - Authentication: cookie auth for the UI and bearer API tokens for scripts. There is no
    ASP.NET Identity, just a small custom user store.
  - Passwords are hashed with Argon2id (`Konscious.Security.Cryptography`).
  - The service runs as the system user `filedock-mgmt` with no extra privileges.
  - Web UI: a React + TypeScript single-page app built with Vite. Its static files are served by
    the Management Service. It calls the same `/api/v1` REST API that API tokens use.
- **Rationale**:
  - .NET and SQLite match the guide.
  - Because the SPA uses the public REST API, scripts with tokens (FR-021) get exactly what the UI
    gets, and the API is exercised by every UI test.
  - SignalR is the guide's suggestion for browser updates.
- **Alternatives considered**:
  - Blazor Server: one language, but UI state lives in server circuits, and the UI would not
    exercise the REST API. Memory grows with each connection.
  - Blazor WebAssembly: large initial download.
  - Server-rendered Razor pages: poor fit for live dashboards.

## R5. Service-to-service channel

- **Decision**:
  - gRPC over the Unix domain socket `/run/filedock/core.sock`, owned by `root:filedock-mgmt` with
    mode `0660` and created by a systemd socket unit.
  - The Core also verifies the peer's uid with `SO_PEERCRED`. Only the `filedock-mgmt` uid and
    root (used by `filedockctl`) are accepted.
  - Both sides send `interface_version` as metadata, using a `major.minor` scheme. A different
    major version fails closed: read-only status calls are allowed and mutations are refused
    (FR-011).
  - Events use a server-streaming RPC with a monotonic sequence number, so the Management Service
    can resume after a reconnect without losing events.
- **Rationale**: Implements guide section 7 and FR-009 to FR-011. Peer-credential checks add
  defense in depth beyond file permissions.
- **Alternatives considered**:
  - HTTP/JSON over the socket: weaker typing and no native streaming.
  - D-Bus: awkward from .NET, and policy is configured separately.
  - TCP on localhost: reachable by any local user.

## R6. Desired state, reconciliation, and the operation journal

- **Decision**:
  - The Core stores desired state as typed resources: `Pool`, `ManagedFolder`, `Export`,
    `ShareUser`, `CloudConnection`, `ContainerApp`, `Extension`, `ApplianceStateDestination`, and
    `Settings`. Each resource has a `generation`.
  - Changes arrive as one `ApplyChange` transaction, a batch of upserts and deletes with expected
    generations. It is validated, then committed in one SQLite transaction (FR-014). The
    reconciler queue is notified only after the commit.
  - One controller per resource kind compares desired and observed state and acts through
    subsystem adapters. Reconciliation runs at startup, after each commit, on subsystem events,
    and as a periodic resync every 10 minutes.
  - Critical operations are durable state machines in `operation` and `operation_step` tables.
    Critical operations include pool create, extend, and recreate; System Pool resize; restore;
    takeover; and extension install and upgrade. Each step is idempotent and records its intent before acting.
    On startup the Core resumes or rolls back each unfinished operation (FR-015, FR-016).
- **Rationale**: This is the guide's controller model (section 9) and its journal (section 47),
  kept small and deterministic.
- **Alternatives considered**:
  - An event-sourced store: more complex, with no requirement that needs it.
  - Embedding etcd: a cluster-oriented component that conflicts with Principle V.

## R7. Data-plane independence

- **Decision**: The Core never owns data-plane processes as its own child processes. Instead:
  - Samba (`smbd`), NFS (`nfs-server`), and Docker run as their own systemd units.
  - Pools and folder bind mounts are persistent systemd `.mount` units that the Core writes.
  - WebDAV servers and native-process extensions run as transient systemd units created with
    `systemd-run`, so they survive a Core restart.
  - The Management Service is never in the I/O path.
- **Rationale**: FR-017, Principle I, and the operating constraints. Restarting either service
  must not interrupt shares or containers.
- **Alternatives considered**: Spawning children directly from the Core. A Core crash would then
  kill shares.

## R8. Local storage: Physical Disks → Pools → Managed Folders

- **Decision**:
  - A pool is an LVM2 volume group with one linear (concatenated) logical volume, formatted ext4.
    The volume group is tagged `filedock_pool=<pool-uuid>`.
  - A marker file `/.filedock-pool.json` at the filesystem root records the pool's identity and
    name. The pool root also holds the reserved directory `/.filedock/`, which contains the local
    appliance-state copy (R22). Folder reference paths are always directories below the root, so
    neither the marker nor the reserved directory is ever exported.
  - The system disk is never an LVM pool member. It backs only the System Pool (R21).
  - Pools mount at `/mnt/filedock/pools/<pool-uuid>`. A local reference path is
    `<pool-uuid>:<relative path>`.
  - A managed folder's FileDock path (`/storage/<name>`) is a bind mount of
    `/mnt/filedock/pools/<pool-uuid>/<relative path>`.
  - Disk discovery uses `lsblk --json --output-all` plus udev events.
  - Disk health uses `smartctl --json`, polled every 6 hours and on udev change events.
    Early-failure attributes (reallocated, pending, or uncorrectable sectors; NVMe critical
    warnings; overall SMART failure) mark the pool Degraded with a warning alert.
  - Growing a pool is `pvcreate`, then `vgextend`, `lvextend`, and an online `resize2fs`, run as a
    journaled operation.
  - Adopting an existing filesystem creates a single-disk, non-LVM pool. The marker file is written
    and nothing is erased. In this release an adopted pool cannot grow. To grow, the administrator
    creates an LVM pool from blank disks.
  - Removing a disk from a pool is not offered.
  - Pool loss is detected in three ways: LVM reports missing physical volumes, the mount fails, or
    I/O errors are seen. The pool is then Failed, and every folder in it is Unavailable.
  - The pool is recreated on replacement disks with the same pool UUID, which is written into the
    volume group tag and the marker file.
- **Rationale**:
  - LVM gives a RAID-0-style pool, where capacity adds up and there is no redundancy. Online
    growth is simple, and the tooling is proven (Principle II: orchestrate, don't reimplement).
  - Linear concatenation, rather than striping, allows disks of different sizes and one-disk
    growth. Either way the spec treats the whole pool as lost when any disk fails.
  - Folders refer to the pool UUID, never to devices, so restore onto new disks only remaps pools
    (FR-063).
- **Alternatives considered**:
  - mdadm RAID 0: growth is limited, and member sizes must be similar.
  - Btrfs multi-device: not the guide's default filesystem, and it has its own failure semantics.
  - mergerfs: placement is per file, so it doesn't match "RAID-0 style" or whole-pool semantics.
  - LVM striped volumes: growth needs a multiple of the stripe count.
- **Future encryption hook**: The pool record has an ordered `layers` list, such as `["lvm"]`. A
  future `"luks"` layer can sit between a physical volume and the volume group without changing the
  folder model. Key custody for that layer would live in the secret store, under the recovery key.

## R9. Local disk encryption at rest

- **Decision**: Out of scope for this release, as directed by the user. Encryption still applies
  to:
  - the secret store (R10);
  - appliance-state snapshots (R11);
  - all future cloud-bound data (features 002 and 003).
- **Rationale**: It avoids the unattended-boot unlock problem and recovery-key coupling for local
  disks in the MVP. The pool `layers` field (R8) keeps the door open.
- **Alternatives considered**:
  - LUKS with a key file on the system disk: little protection.
  - TPM-sealed keys: these conflict with FR-032, because recovery must not depend on the original
    hardware.

## R10. Secret store

- **Decision**:
  - The Core owns all secrets, in `/var/lib/filedock/secrets.enc`.
  - Each secret record is encrypted with XChaCha20-Poly1305 under a local data-encryption key (the
    DEK). The DEK is stored in `/var/lib/filedock/keys/local.key` (root, `0600`).
  - A copy of the DEK, wrapped by the recovery key, goes into every appliance-state snapshot.
  - The Management Service sends secrets write-only through `PutSecret` and receives only the
    masked form and a reference.
  - Subsystems receive secrets materialized into per-subsystem files on tmpfs under
    `/run/filedock/secrets/<subsystem>/`, mode `0400`, and only the secrets their configuration
    references (FR-031). A cloud connector receives only the credentials of the connections
    routed to it (FR-119, R13). One example is a generated `rclone.conf` that lists only the
    connections routed to rclone.
  - Logs, events, and audit entries pass through a redaction layer that drops values of known
    secret fields. The canary secrets in SC-007 test this.
- **Rationale**: Privileged consumers are all Core-managed, so the Core is the natural custodian.
  Tmpfs materialization avoids plaintext secrets on disk. The recovery-key wrap meets FR-032.
- **Alternatives considered**:
  - Management-owned secrets: privileged subsystems would then depend on the UI service.
  - systemd-creds: tied to the host or TPM by default.
  - HashiCorp Vault: far too heavy.

## R11. Recovery key and appliance-state snapshots

- **Decision**:
  - **Recovery key**:
    - 25 random characters from a 32-symbol, unambiguous alphabet, grouped `XXXXX-XXXXX-…`,
      giving about 125 bits.
    - It is stretched with Argon2id (m=256 MiB, t=3, p=1) and a per-key salt into a key that
      wraps the snapshot key.
    - Regeneration creates a new salt and wrap. New snapshots use it, and the old key keeps
      opening only older snapshots (FR-034).
  - **Snapshot content** (FR-036): a declarative **configuration document** (`config.json`,
    schema `fdconfig/1`), plus the secret store. It is not a database copy
    (contracts/appliance-state-layout.md).
    - The Core serializes its desired-state resources into the `core` section, inside one read
      transaction on `core.db`, so the section is consistent:
      - pools (identity, name, kind, layers, System Pool limits, and never disks);
      - folders, exports, share users, connections, the remote destination, and owned locations;
      - apps, notification channels, and capability defaults;
      - enabled cloud connectors, connector routes (family defaults and per-connection
        overrides), and the recorded acknowledgements for basic claim safety and third-party
        owned storage (R13);
      - the subsystem registry with versions, package hashes, approved permissions,
        configuration, and data bindings;
      - tier metadata (feature 003).
    - The Management Service serializes the `management` section (administrators with password
      hashes, API token metadata and hashes, settings) and pushes it with `PutManagementConfig`.
      Neither service writes the other's database (FR-012).
    - `secrets.enc` as stored (still under the DEK), the DEK wrapped by the recovery key, and the
      appliance TLS certificate. The certificate's private key is a secret, so it lives in
      `secrets.enc`.
    - Excluded by construction, because they are not desired-state resources: audit entries,
      alerts, snapshot records, the operation journal, sessions, setup state, observed status,
      events, and logs.
    - The schema is versioned independently of the database migrations. Restore upgrades older
      `fdconfig` versions in memory, then imports, so older snapshots stay restorable
      (Principle III).
    - Restore imports the document into a fresh `core.db` and a fresh `app.db`, as if an
      administrator had entered it, then reconciles. The new audit history starts with
      `appliance.restore`.
  - **Snapshot format**: a `tar` of these parts, compressed with zstd, then encrypted with
    XChaCha20-Poly1305 in a streaming construction. A SHA-256 checksum is stored in a signed index
    file per snapshot.
  - **Change detection**: each service keeps a configuration-change counter.
    - The Core counts committed `ApplyChange` transactions and subsystem registry changes.
    - The Management Service counts changes to administrators, tokens, and settings, through an
      EF Core `SaveChanges` interceptor. It ignores audit, session, and `last_used_at`/
      `last_seen_at` writes.
    - Audit entries and alerts never trigger a snapshot (FR-038).
  - **Snapshot timing** (FR-038): the first pending change starts a 30-second quiet-period
    debounce. Each new change restarts it, but the snapshot is always cut at most 60 seconds after
    the first pending change. Each copy then uploads independently with retries. A copy that has
    not stored a snapshot covering a change within 5 minutes is marked stale, and a warning alert
    is raised.
  - **Copies**:
    - the local copies on every Healthy data pool and on the system disk (R22);
    - the optional remote destination, either a cloud object storage location accessed through
      rclone (R13), or a removable disk.

    All copies use the same layout and format, and one failing copy never blocks the others.
  - **Retention** (FR-039): per copy, the 30 most recent snapshots, the newest of each hour for 24
    hours, and the newest of each day for 30 days. Without history, a snapshot is typically a few
    to a few hundred kilobytes, so retention costs little. Extension packages are stored once per
    version per copy, not per snapshot (R14, R22).
- **Rationale**:
  - Argon2id resists offline guessing of the recovery key.
  - One encrypted archive keeps restore atomic, and the index allows integrity checks before
    decrypting (FR-043).
  - A configuration document matches the clarified scope: rebuild the same FileDock elsewhere,
    and leave history behind. It is small, diffable after decryption, and independent of internal
    table layouts, so schema refactors never break old backups.
  - Serializing desired state is complete by construction, because everything an administrator
    can configure is a desired-state resource (R6) or a Management configuration entity. A
    contract test fails when a new resource kind has no `fdconfig` mapping.
  - The cap of 60 seconds on the debounce keeps 95% of changes within 1 minute.
- **Alternatives considered**:
  - Using Kopia for appliance state: that couples critical state to the user-data backup engine.
  - `age` files: viable. A single crate-level construction avoids an external binary.
  - Whole-database copies (`VACUUM INTO`): complete, but they carry history and volatile tables
    the clarified scope excludes, and they tie every backup to internal schemas and migrations.
  - Streaming WAL replication (Litestream style): it gives a finer recovery point than 5 minutes,
    but replicates history, and adds a continuously running replicator per database and per copy.

## R12. Single-writer ownership claims

- **Decision**:
  - Every owned cloud location has the layout `<prefix>/_filedock/owner.json`. The file holds
    `appliance_id`, `appliance_name`, `generation`, `claimed_at`, and `last_write_at`.
  - **Claim**: allowed only if the prefix is empty, or `owner.json` names this appliance. The
    claim writes `owner.json` with `generation = 1`.
  - **Check before each write session**: re-read `owner.json`. If `appliance_id` or `generation`
    differs from the local record, the appliance stops, marks the location Ownership lost, and
    raises a critical alert. After a successful session it updates `last_write_at`.
  - **Takeover**: the administrator confirms. The new owner writes `owner.json` with
    `generation + 1`. If the old `last_write_at` is less than 15 minutes ago, the UI warns that the
    original may still be running (FR-092).
  - **Automatic takeover after a reinstall restore** (FR-108): once a restore from a local copy
    completes, the Core runs one `Takeover` operation per owned location recorded in the snapshot,
    without asking. It still re-reads `owner.json` first:
    - If the claim's `generation` equals the generation recorded in the snapshot and
      `lastWriteAt` is more than 15 minutes old, it takes over (`generation + 1`, audited as
      `System`, with the reason `reinstall-restore`).
    - If `lastWriteAt` is within 15 minutes, it waits for the normal confirmation.
    - If `generation` is higher than the recorded one, it stays a Recovery reader and raises an
      alert.

    The fencing is unchanged: if the old installation is somehow alive, its next check before
    writing sees the higher generation and stops.
  - Recovery readers never write `_filedock/`.
  - **Transport**: every claim, check, `lastWriteAt` update, and takeover goes through the
    connector routed for the location's connection and the owned object storage use (R13).
  - **Claim safety** (FR-123) is computed per location:
    - **Conditional**: the routed connector declares `conditionalWrites` for the family in its
      manifest (R14), and the connection's latest probe proved that the service enforces both
      conditions (R23).
      - A new claim is `Put(CREATE_ONLY)`, which is `If-None-Match: *`.
      - A takeover and each `lastWriteAt` update are `Put(REPLACE_IF_MATCH, etag)`, which is
        `If-Match`.
      - A `FAILED_PRECONDITION` result means someone else wrote first. The appliance re-reads,
        then refuses the claim, re-evaluates the takeover, or, for a `lastWriteAt` update, enters
        Ownership lost.
    - **Basic**: anything else, including every Azure Blob location served by the general
      connector. The appliance writes, reads back, and compares `applianceId` and `generation`.
    - Creating a basic location, or a route change that makes one basic, needs the audited
      acknowledgement `basic_claim_safety`. The warning `location.basic_claim_safety` stays on
      the dashboard until the location is conditional.
    - Claim safety is recomputed after each connection test, each daily probe, and each route
      change. If a probe finds that a service stopped enforcing conditions, the location becomes
      basic and a warning alert is raised. Writes continue, because the check before each write
      session still fences a former owner, and stopping would break the 5-minute recovery point.
- **Rationale**:
  - Implements FR-088 to FR-094 and FR-123 without a lock service (Principle V).
  - Conditional writes make simultaneous claims of an empty location safe (SC-025). They don't
    turn a claim into a lock (spec Assumptions).
- **Alternatives considered**:
  - DynamoDB-style lock tables: provider-specific and multi-writer oriented.
  - S3 Object Lock: about retention, not ownership.
  - Leases with heartbeats: these imply HA semantics the spec rejects.

## R13. Cloud connectors, routing, and cloud I/O

- **Decision**:
  - **One connector contract**: a cloud connector is a subsystem that serves `CloudRemote` from
    `contracts/extension-protocol.proto`. Connectors that provide `cloud.mount` also serve
    `CloudMount`, which feature 004 defines. The Core talks to every connector the same way, so
    routing is a table lookup and not a separate code path. Outside a connector, the Core never
    calls a provider SDK or rclone.
  - **Built-in connectors** (FR-113):
    - `fd-s3`, the native S3 connector: a separate Rust binary, `native-process`, class
      `built-in`. It serves `CloudRemote` itself (R23).
    - `rclone`, the general cloud connector. rclone doesn't speak the protocol, so a Core-side
      adapter implements `CloudRemote` over `rclone rcd`, the same way Samba, NFS, and Kopia have
      adapters. `rclone rcd` runs as a transient unit on `/run/filedock/rclone.sock`. The adapter
      uses `operations/list`, `operations/stat`, `operations/copyfile`, `operations/deletefile`,
      and `config/create` and `config/update` with non-interactive state. It also provides
      `cloud.mount` for feature 004, and declares `conditionalWrites: false` for every family.
  - **Connections** (FR-114): each has a `providerFamily`. The definition is one of two kinds:
    - **Connector-neutral S3**: `endpoint`, `region`, `addressingStyle` (`path` or `virtual`),
      `bucket`, `accessKeyRef`, `secretKeyRef`, `storageClass`, `serverSideEncryption`
      (`provider-default` or `kms:<key-id>`), and `bandwidthLimit`. Only static keys are
      accepted. The Core translates the definition for each connector:
      - for `fd-s3`, into a `Configure` entry;
      - for rclone, into an `s3` remote with `provider=Other`, `env_auth=false`, `endpoint`,
        `region`, `force_path_style`, `storage_class`, and `server_side_encryption`.
    - **Connector-bound**: validated against the owning connector's `connectionSchema` (R14).
      Google Drive, OneDrive, Dropbox, and every other rclone family are defined this way.
  - **Routes** (FR-115):
    - The singleton desired-state resource `ConnectorRoutes` holds the family defaults. Each
      `CloudConnection` holds its own `routeOverrides`.
    - The uses are `OwnedObjectStorage`, `BackupRepository`, and `CloudMount`. A target is
      `connector:<subsystem-id>`, or `backup-engine` for `BackupRepository` only.
    - **Resolution**: the override for (connection, use), then the family default for (family,
      use). If neither exists, the result is `route.missing`.
    - **Validation**, on every `ApplyChange`:
      - the target connector is enabled;
      - it declares the family and the capability the use needs (`cloud.remote` or
        `cloud.mount`);
      - the use is allowed for the family. `OwnedObjectStorage` and `BackupRepository` are
        allowed only for S3-compatible and Azure Blob (FR-088).
    - **Default routes**, seeded on first boot:

      | Use | `s3-compatible` | `azure-blob` | Other families |
      |---|---|---|---|
      | `OwnedObjectStorage` | `connector:fd-s3` | `connector:rclone` | Not allowed |
      | `BackupRepository` | `backup-engine` | `backup-engine` (Kopia has native Azure) | Not allowed |
      | `CloudMount` | `connector:rclone` | `connector:rclone` | `connector:rclone` |

  - **Session pinning** (FR-118):
    - A write session resolves its route once, when it starts, and keeps that connector until it
      ends. Write sessions include snapshot uploads, claim checks, tier migrations, and backup
      cycles.
    - A mount resolves its route when it starts.
    - Each route change advances `ConnectorRoutes.generation`. Running sessions finish on the
      old connector, and new sessions use the new one. The reconciler restarts mounts whose
      route changed.
  - **Same object layout** (FR-118):
    - Every connector stores a FileDock object at exactly `<prefix>/<key>`, without transforming
      it.
    - FileDock never depends on connector-specific object metadata, directory markers, or
      listings of "directories". Generated rclone remotes never use `crypt` or `chunker`, and
      leave directory markers off.
    - A contract test writes through one connector and reads through the other, in both
      directions, for every object kind in this feature: `owner.json`, `index.json`, keys,
      `.fdstate`, and `.fdpkg`.
  - **Credentials** (FR-119):
    - A connector is configured with exactly the connections routed to it, for any use. Their
      secrets are materialized only into that connector's tmpfs directory (R10).
    - After a route change, reconfiguring removes the files of connections no longer routed to
      it.
    - Third-party connectors must declare the permission `cloud.credentials`, and the approval
      screen shows it.
  - **Route change flow**:
    - `PreviewRouteChange` computes the impact:
      - the connections, folders, and owned locations affected;
      - the features lost, such as conditional writes;
      - which connector receives which credentials;
      - the mounts that will restart.
    - `ApplyRouteChange` requires every acknowledgement the preview reported:
      - `basic_claim_safety`, when an owned location would become basic (FR-123);
      - `third_party_owned_storage`, when owned object storage moves to a connector that isn't
        built-in (FR-119).

      Both are audited. The change commits as one `ApplyChange`, so it lands in `config.json`.
  - **Failure** (FR-120, FR-121): when the routed connector isn't Running, the session fails
    with `connector.unavailable`, and the alert `connector.route_unavailable` names the connector
    and the route. There is no fallback. Disabling or removing a connector is refused while any
    route points to it.
  - **`backup-engine` target** (FR-116):
    - The Kopia adapter receives the connection's keys as declared secrets, and uses Kopia's own
      `s3` (or `azure`) storage.
    - When a `BackupRepository` route names a connector instead, Kopia uses its `rclone`
      storage through that connector.
    - Feature 002's plan owns the repository details.
  - **Browser-based OAuth**, for rclone families:
    - The UI opens the provider's consent page. The provider redirects to rclone's registered
      loopback redirect, which fails harmlessly on the administrator's machine.
    - The UI then asks the administrator to paste that redirect URL, and the Management Service
      passes the code to the Core to finish the configuration.
    - Administrators may instead supply their own OAuth client ID with a redirect URI on the
      appliance's address.
  - **Credential refresh**: rclone refreshes OAuth tokens automatically. The Core reads token
    expiry from the connection state and raises a warning alert 7 days before a credential that
    cannot be refreshed expires (FR-084). Static S3 keys don't expire.
  - **Restore** (FR-122):
    - Routes come back in `config.json`.
    - A remote source entered by hand is opened through the built-in connector for its family:
      `fd-s3` for S3-compatible, rclone otherwise.
    - A restored route that names an extension connector uses the built-in connector read-only
      until that extension is reinstalled from its stored package (R14). Then the route applies.
- **Rationale**:
  - Correctness where FileDock owns the data, and coverage where users need it, behind one
    contract (Principles II and IV).
  - Routes are configuration, so they are audited, validated, and restored like everything else.
  - Session pinning and an identical object layout make switching connectors safe (SC-024).
- **Alternatives considered**:
  - rclone only: its remote-control API has no conditional writes, and gives little control
    over checksums and storage settings. This was the gap in the earlier design of R12.
  - Native S3 inside the Core: one hop less, but it breaks the uniform contract. The connector
    could no longer be disabled, switched, or crash-isolated, and the user asked for it as an
    extension with a manifest.
  - Per-folder routing: finer than needed. Per-connection overrides cover a quirky service
    (spec Clarifications).
  - Automatic failover between connectors: it hides failures and can change claim safety
    silently (FR-120).

## R14. Subsystems, the manifest, and extensions

- **Decision**:
  - **Manifest**: every subsystem, built-in or extension, has a YAML manifest validated against
    `contracts/subsystem-manifest.schema.json`. Built-in manifests ship in
    `/usr/share/filedock/subsystems/`.
  - **Runtime types**:
    - `native-process`: run as a transient systemd unit with sandboxing derived from the
      manifest's permissions.
    - `systemd-service`: an existing or packaged unit, plus a drop-in with the derived sandboxing.
    - `docker-service`: a container created through the Docker Engine API, with capabilities,
      mounts, and network derived from the permissions.
  - **Permissions** are a closed vocabulary that maps to sandbox settings. Examples are
    `storage.read:<folder>`, `storage.write:<folder>`, `network.outbound`, `network.listen:<port>`,
    `core.events.read`, `core.alerts.write`, and `secrets:<name>`. Anything undeclared is denied by
    the sandbox. Calls back to the Core are checked per extension identity (FR-053).
  - **Extension protocol**: an extension registers over the extension socket
    `/run/filedock/extensions.sock` using the gRPC service in
    `contracts/extension-protocol.proto`. That service is the generic subsystem contract plus
    capability-specific services such as `NotifyChannel`. Built-ins that can't speak gRPC (Samba,
    NFS, Docker, Kopia) get Core-side adapters that implement the same contract.
  - **Cloud connectors** (FR-112): a manifest that lists `cloud.remote` or `cloud.mount` must
    also have a `cloud` section. For each provider family it serves, the section lists:
    - the capabilities it provides for that family;
    - whether it supports `conditionalWrites` and `providerChecksums`;
    - for connector-bound families, a `connectionSchema`.

    Families are `s3-compatible`, `azure-blob`, `google-drive`, `onedrive`, `dropbox`, or
    `x-<connector-id>-<name>` for families only that connector serves. A connector needs the
    permission `cloud.credentials` to receive connection credentials (R13). Built-in connectors
    can be disabled when no route uses them, and can't be removed.
  - **Package format**: `.fdext`, a `tar.zst` archive containing `manifest.yaml`, the payload, and
    `SHA256SUMS`. It may also carry a minisign-format Ed25519 signature. Unsigned packages are
    allowed, but the administrator must acknowledge that they are unsigned.
  - **Install and upgrade**: these are journaled operations. Upgrades keep the previous version
    and roll back automatically if the health check fails (FR-055).
  - **Extension data** (FR-109): the manifest's `data` list declares persistent paths. At install
    the administrator binds each one to a managed folder. The install flow proposes creating a
    Hybrid Backup folder at the manifest's `suggestedImportance`, and requires an acknowledgement
    when an unprotected folder is chosen. Binding becomes `storage.read` and `storage.write`
    permissions on that folder, and the sandbox enforces it:
    - `native-process` and `systemd-service`: `BindPaths=<folder>:<mountPath>`, together with
      `ProtectSystem=strict`, `ReadWritePaths=` limited to the bound paths, and `PrivateTmp`. Any
      other persistent write fails, and the failure is logged.
    - `docker-service`: a bind mount for each bound folder, a read-only root filesystem, and
      `tmpfs` for `/tmp`. Named volumes are refused.

    Bindings are stored in the subsystem's desired-state record, so they travel in `config.json`.
    A bound folder can't be deleted while the binding exists.
  - **Package retention** (FR-110): after a successful install or upgrade, the Core encrypts the
    `.fdext` as a `.fdpkg` and writes it to every appliance-state copy before the next snapshot
    refers to it (R22). On restore, packages are verified against the hash recorded in
    `config.json`. A missing or bad package is fetched from another reachable copy. Otherwise the
    extension is `PackageNeeded`, and only that extension waits for an upload of the same version.
  - **Restart policy**: exponential backoff from 1 s up to 5 min. The subsystem is marked Failed
    after 5 consecutive failures within 15 minutes (FR-054).
- **Rationale**: Implements guide sections 10 to 14 and Principle IV. Third-party code never runs
  inside the Core.
- **Alternatives considered**:
  - WASM plugins in-process: violates the out-of-process rule.
  - Free-form permissions: impossible to enforce consistently.

## R15. Shares and share users

- **Decision**:
  - **SMB**: the Core renders `/etc/filedock/samba/shares.conf`, included from `smb.conf`, and
    runs `smbcontrol all reload-config`. Share users are Linux system users with no login shell,
    in group `filedock-share`, with Samba passwords in `tdbsam`.
  - **NFS**: the Core renders `/etc/exports.d/filedock.exports`, then runs `exportfs -ra`. Access
    rules are client networks or hosts with read-only or read-write access, NFSv4 with `sec=sys`.
  - **WebDAV**:
    - one `rclone serve webdav` transient unit per WebDAV-enabled folder;
    - TLS uses the appliance certificate, and a per-export htpasswd file is generated from share
      users;
    - ports are allocated from 8440–8499 and recorded in the export's status.
- **Rationale**: These are the proven built-ins the guide names. Configuration files survive Core
  restarts, which preserves data-plane independence (R7). One WebDAV process per folder is the
  only way to give each export its own access rules with rclone.
- **Alternatives considered**:
  - A single WebDAV server combining all folders: it can't have per-folder users.
  - Proxying WebDAV through the Management Service: puts the control plane in the data path.

## R16. Containers

- **Decision**:
  - A `ContainerApp` resource holds a Docker Compose definition, its environment, and references to
    application secrets.
  - The Core runs `docker compose -p fd-<app> up -d` with the definition materialized under
    `/var/lib/filedock/apps/<app>/`.
  - **Validation**:
    - bind sources must be FileDock paths of managed folders that have Docker access (FR-085);
    - named volumes and binds outside managed folders produce the FR-087 warning;
    - removing Docker access from a folder that a running app uses is refused.
  - After restore, the apps are recreated and their images pulled again.
- **Rationale**: The guide names Compose (section 56). Compose definitions are small authoritative
  state.
- **Alternatives considered**:
  - Kubernetes or Portainer: out of character for a single node.
  - Single containers only: the spec requires multi-container definitions.

## R17. Logs, audit, diagnostics, alerts, and notifications

- **Decision**:
  - **Logs**: all services and subsystems log to journald, and Docker uses the journald log
    driver. A drop-in sets `SystemMaxUse=1G` (FR-058). The Core serves filtered queries
    (source, severity, time) by reading the journal with `journalctl --output=json` and cursor
    paging.
  - **Audit**: the audit history lives in the Management Service's `app.db`. Entries are
    append-only and kept on the appliance for 90 days. They are not in snapshots (FR-059), and
    after a restore the history starts with an `appliance.restore` entry.
  - **Diagnostic bundle**: a `tar.zst` built by the Core with service status, subsystem health,
    recent journal entries, and redacted configuration. Redaction uses the same layer as R10.
  - **Alerts**: the Core owns the alert store in `core.db`, because alert conditions originate
    there. Alerts deduplicate by `(source, object, kind)`, resolve automatically when the
    condition clears, and can be acknowledged through the Core API.
  - **Notifications**: delivery uses only `notify.channel` extensions over the extension protocol.
    Each channel has a minimum severity. Failed deliveries retry with backoff for up to 24 hours,
    are logged, and never affect the alert itself (FR-097).
- **Rationale**: journald is already the system log and has a native size cap. Alerts derive from
  Core state, so keeping them in the Core avoids split-brain between the two services.
- **Alternatives considered**:
  - Loki or Elasticsearch: heavy.
  - Alerts in the Management Service: it would have to infer Core conditions.

## R18. Authentication details

- **Decision**:
  - **Sessions**: server-side sessions in `app.db` with a 30-minute idle expiry and a cookie with
    `HttpOnly` and `SameSite=Strict` on the required HTTP listener, plus CSRF tokens for mutations.
  - **Lockout**: tracked per account, 15 minutes after 5 consecutive failures.
  - **First-run setup code**:
    - 8 base32 characters, stored hashed;
    - attempts throttled to 5 per 15 minutes per client IP and in total;
    - the code is invalidated when setup completes or the installer runs again.
  - **API tokens**: `fdt_<id>_<secret>`, where only an Argon2id hash of the secret is stored.
    Scope is Read-only or Full. Tokens have an optional expiry and are revoked at once. Every use
    is audited, with Read-only GET polling sampled once per minute. Full-token and mutating
    requests are audited individually. Tokens are
    refused for setup, password reset, and token management.
  - **Console password reset**: the `filedockctl reset-password <user>` command runs as root. It
    calls the Management Service over the root-only socket `/run/filedock/management-admin.sock`.
  - **HTTP listener**: the UI and API share port 8021, with no TLS or certificate configuration.
    Listener addresses can be restricted to local interfaces (FR-027); the default is all
    addresses. The appliance never configures UPnP or tunnels.
- **Rationale**: These are standard, auditable mechanisms that need no external identity provider,
  matching the spec's out-of-scope list.
- **Alternatives considered**:
  - JWT sessions: revocation is harder.
  - OAuth device flow for scripts: unnecessary on a local network.

## R19. Testing strategy

- **Decision**:
  - **Rust**: `cargo nextest` for unit tests and component tests. Component tests use temporary
    directories, a fake subsystem adapter, and loop devices for LVM in privileged CI.
  - **.NET**: xUnit and `WebApplicationFactory` for API, auth, and token tests, with a fake gRPC
    Core.
  - **Web**: Vitest for components, and Playwright for UI flows against a test appliance.
  - **Contracts**: `buf lint`, plus breaking-change checks for the `.proto` files. JSON Schema
    validation for the manifests.
  - **Appliance acceptance**: `pytest` drives disposable Debian 13 VMs (QEMU/libvirt) over SSH and
    the REST API. It covers install, setup, pools, folders, shares, containers, snapshots, DR
    restore, takeover, and the failure-injection set (SC-010, SC-015, SC-018 to SC-020, guide
    section 63).
- **Rationale**: Each layer is tested in its native tooling. The constitution's recoverability and
  data-plane rules can only be proven on real VMs.
- **Alternatives considered**: Only containerized end-to-end tests. They can't exercise LVM,
  systemd mounts, or reboots faithfully.

## R20. Scale

- **Decision**: No scale limits in this release, as directed by the user. The design avoids
  hard-coded caps: lists in the API are paginated, the event stream resumes by cursor, and
  reconcilers work incrementally. Limits may be added after measurement.
- **Rationale**: User direction. Pagination and incremental work cost little now and prevent
  rework later.
- **Alternatives considered**: Fixed caps such as 64 folders. They are arbitrary without data.

## R21. System Pool on the system disk

- **Decision**:
  - **Backing store**: the System Pool is an ext4 filesystem inside a sparse image file on the root
    filesystem, `/var/lib/filedock/system-pool/pool.img`.
    - It is attached as a loop device with direct I/O (`losetup --direct-io=on`) and mounted with
      `discard`, so deleting files inside it returns space to the root filesystem.
    - It mounts at `/mnt/filedock/pools/<system-pool-uuid>` through a persistent systemd `.mount`
      unit, like any other pool. Folders bind-mount from it in the same way (R8).
    - Its `layers` list is `["image"]`.
  - **Maximum disk usage**: this limit is a hard size. The image's apparent size, and so the inner
    filesystem's size, is `max_usage_percent × root filesystem size`. Writes beyond it fail
    inside the filesystem with `ENOSPC`, which SMB, NFS, WebDAV, and containers all report as
    "disk full".
  - **Reserved free space**: this limit is a watchdog with a guard band.
    - The Core checks root filesystem free space with `statvfs` every 2 seconds.
    - When free space falls below `reserve + guard`, it remounts the System Pool read-only, marks
      it `Full (reserve)`, and raises a critical alert. The guard is 5 GB, or 2% of the disk if
      that is smaller.
    - When free space recovers above `reserve + 2 × guard`, it remounts read-write.
    - The guard band absorbs the writes that can land between two checks, so the reserve itself
      is not reached.
    - Nothing is deleted (FR-100). Temporary content follows the existing disk-pressure rule.
  - **Effective capacity**, reported to the UI, is
    `min(max_usage_percent × disk, used_by_system_pool + free − reserve)`, together with which
    limit applies.
  - **Changing the limits** is a journaled operation:
    - Raising a limit grows the image (`truncate`, `losetup -c`, then an online `resize2fs`) with
      no interruption.
    - Lowering a limit requires the new size to be at least current usage plus 5%. Otherwise it is
      refused. It shrinks offline: the Core stops the exports of System Pool folders, unmounts,
      shrinks with `resize2fs`, truncates, remounts, and restarts the exports. The UI states that
      interruption before the administrator confirms (FR-101).
  - **Availability**: if the effective capacity at install time is under 1 GB, the System Pool
    record exists but is `Unavailable`. The image is created later, when the limits or the disk
    allow it.
  - **Health**: SMART on the system disk drives Degraded, as for any pool (R8).
- **Rationale**:
  - It works on every Debian root layout (plain partition, LVM, or anything else) without
    repartitioning a live system disk.
  - The image gives a true hard cap for maximum usage, and the folder model, mounts, and exports
    are identical to data pools.
  - Sparse allocation with `discard` means empty System Pool space costs nothing.
  - Loop devices with direct I/O add little overhead for small and medium workloads, and large
    data belongs in data pools anyway (spec Assumptions).
- **Alternatives considered**:
  - A plain directory with ext4 project quotas: hard limits, but the root filesystem needs the
    `project` feature, which Debian doesn't enable by default and which can't be turned on while
    the filesystem is mounted.
  - A plain directory with only the watchdog: no hard cap, and measuring usage needs repeated
    scans.
  - A dedicated partition: can't be created on an installed, full-disk system without
    repartitioning.
  - An LVM logical volume in free extents of the system volume group: only possible when Debian
    was installed on LVM with free space. This could become an optional backing store later.
  - Preallocating the image: guarantees the space, but takes up to 50% of the disk on day one.

## R22. Local appliance-state copies and recovery-key-only reinstall

- **Decision**:
  - **Where the local copies live** (FR-104):
    - On each Healthy data pool, under `<pool root>/.filedock/appliance-state/`, using the same
      layout as a remote destination but without `_filedock/owner.json`
      (contracts/appliance-state-layout.md).
    - On the system disk, under `/var/backups/filedock/appliance-state/`. Debian leaves
      `/var/backups` alone on package purge, so this copy survives removing and reinstalling the
      FileDock packages.
    - The System Pool image gets no extra copy, because it is on the same disk as
      `/var/backups`.
  - **Writing the copies**: each snapshot (R11) is written to every local copy, then to the remote
    destination. For each copy, the Core:
    1. writes any `.fdpkg` the snapshot needs that the copy doesn't have yet;
    2. writes and fsyncs the `.fdstate`;
    3. atomically replaces `index.json`.

    Every copy prunes its own snapshots and unreferenced packages only after its newest snapshot is
    durable.
  - **Configuration backup file** (FR-111): `ExportBackupFile` streams a `.fdbackup`, which is a
    one-snapshot copy root as a `tar` (contracts/appliance-state-layout.md). It is assembled from
    the system-disk copy, so it never needs the remote destination. Restore treats an uploaded
    file as a removable-disk source: it needs the setup code and the recovery key, and makes no
    automatic takeover.
  - **Appliance identity**: a restored appliance keeps its `applianceId` and name. Snapshots
    therefore continue under the same `appliance/<appliance-id>/` directory everywhere. The
    ownership generation, not a new id, tells installations apart (R12).
  - **Detection at reinstall** (FR-106): before generating a setup code, the installer runs
    `filedock-core detect-state` read-only. It:
    - activates volume groups tagged `filedock_pool` and mounts their logical volumes read-only in
      a private directory;
    - mounts read-only, briefly, any filesystem whose root holds `/.filedock-pool.json` (adopted
      pools);
    - reads `/var/backups/filedock/appliance-state/`;
    - lists each appliance it finds, with the name, newest snapshot time, and where the copies are.

    `index.json` stores `applianceName` in plaintext, so the list can be shown before the key is
    entered. The MAC is verified once the key is entered.
  - **Restore without a setup code**:
    - On an interactive console, the installer prompts for the recovery key with hidden input,
      then starts restore through the Core.
    - Otherwise, the first-run page shows "Restore *name* found on this machine" and accepts only
      the recovery key. The Core verifies the key against the copy's `keys/<key-id>.json`. Wrong
      keys are throttled, as for the setup code (R18).
    - Because the key both authorizes and decrypts, no setup code is needed. Restoring from a
      remote source typed in by hand still requires the setup code.
  - **Choosing the snapshot** (FR-107):
    1. Decrypt the newest valid local snapshot.
    2. Read its recorded remote destination as a Recovery reader, through the built-in
       connector for its family until the restored routes take effect (FR-122, R13).
    3. If the destination's newest valid snapshot is newer, restore that one instead.
  - **Reattaching pools** (FR-063): pools whose volume-group tag or marker matches a recorded
    pool id are reattached in place, with no mapping and no erase. Missing pools go to the mapping
    step. The System Pool is recreated with its recorded uuid and limits (FR-103).
  - **After restore**: the automatic takeovers run (R12), then normal reconciliation (R6).
- **Rationale**:
  - The data pools are the disks most likely to survive an OS reinstall or a system disk
    replacement.
  - The configuration document (R11), stored extension packages (R14), and credentials restored
    from `secrets.enc` mean the recovery key is the only input needed.
  - Keeping the appliance id means a reinstall looks like the same appliance to the cloud, and
    only the generation moves.
- **Alternatives considered**:
  - Requiring a remote destination for any restore: this doesn't meet the "recovery key only"
    goal.
  - Encoding the destination and its credentials into a longer "recovery kit": credentials rotate,
    and the kit becomes a second secret to keep current.
  - Storing copies inside a managed folder: the copies would be exposed to clients and would be
    subject to the folder's mode.

## R23. Native S3 connector (`fd-s3`)

- **Decision**:
  - **Packaging**: a Rust binary in `connectors/fd-s3/`, built in the same Cargo workspace and
    shipped in `filedock-builtins`, with the manifest `packaging/subsystems/fd-s3.yaml`.
    - It runs as a `native-process` transient unit with `network.outbound` and
      `cloud.credentials`, and no storage permissions.
    - It serves `Subsystem` and `CloudRemote` on its own socket.
    - Its manifest declares `s3-compatible` with `cloud.remote`, `conditionalWrites: true`, and
      `providerChecksums: true`.
  - **Library**: `object_store` (Apache Arrow project), with the `aws` feature
    (`AmazonS3Builder`). The earlier rejection, that it adds a second cloud stack, is reversed,
    because pluggable connectors make a second stack intended (spec Assumptions).
    - **Addressing**: endpoint, region, and `with_virtual_hosted_style_request` are taken from
      the connection. Plain HTTP is refused.
    - **Conditional writes**: `PutMode::Create` maps to `If-None-Match: *`, and
      `PutMode::Update(UpdateVersion { e_tag })` maps to `If-Match`, with
      `S3ConditionalPut::ETagMatch` configured. `AlreadyExists` and `Precondition` errors become
      gRPC `FAILED_PRECONDITION`.
    - **Checksums**: when the probe found support, `with_checksum_algorithm(Checksum::SHA256)` is
      set, so the service verifies each upload.
    - **Multipart**: objects over 64 MiB use `put_multipart` with 16 MiB parts. Any error aborts
      the upload.
    - **Server-side encryption**: the provider default, or SSE-KMS through
      `with_sse_kms_encryption(<key-id>)`.
    - **Gaps in the crate** are covered by a few hand-signed SigV4 requests (`aws-sigv4`): a
      per-request storage class if the pinned crate version lacks it, and `ListMultipartUploads`
      for the incomplete-upload sweep. A spike task confirms which of these the pinned version
      needs.
  - **Integrity** (FR-117, SC-026):
    - **Writes**: the connector hashes each stream with SHA-256 while uploading. When the
      service supports checksums, it verifies the upload and rejects a mismatch. Otherwise, every
      object up to 64 MiB is read back and compared before `Put` returns success. That covers
      every object kind in this feature. Feature 003 decides how larger tier objects are
      verified without a provider checksum.
    - **Reads**: the Core passes the expected SHA-256 in `GetRequest`. It comes from
      `index.json`, a manifest, or the claim. The connector hashes the stream and ends with
      `DATA_LOSS` on a mismatch. The Core discards data from any stream that doesn't end with a
      verified trailer.
  - **No partial objects**: a single PUT is atomic, and a failed multipart upload is aborted. At
    connector start and once a day, the Core runs a sweep that aborts incomplete uploads older
    than 24 hours. The connector never sweeps on its own. The Core calls
    `AbortIncompleteUploads` only for its own probe key and for owned locations where this
    appliance is the Writer, each inside a write session that passed the ownership check (R12).
    A Recovery reader or an appliance whose ownership was lost never sweeps that location.
  - **Layout**: the key is `<prefix>/<key>`, the object is not transformed, no metadata is relied
    on, and nothing is cached (R13).
  - **Bandwidth limit**: a token bucket per connection over request bodies.
  - **Probe**: the `Probe` RPC runs at each connection test and once a day. It works under the
    appliance's own probe key, `_filedock/probe/<appliance-id>/<random>` at the bucket root,
    which is outside every owned location with a non-empty prefix. Before the probe, the Core
    reads `owner.json` at the bucket root. If a location is owned there and this appliance isn't
    its Writer, the Core asks for a read-only probe: only steps 1 and 7 run, and conditional
    create and replace are recorded as not tested, so claim safety can't be Conditional from that
    probe. This keeps a Recovery reader, including one re-probing after a restore, from writing
    inside a location it doesn't own (Principle V, SC-016). The full probe steps are:
    1. List.
    2. Put, with a SHA-256 checksum. If the service rejects the checksum header, checksums are
       recorded as unsupported and the put is retried without it.
    3. Get, and compare.
    4. Put with `CREATE_ONLY` again. It must fail with 412. If it doesn't, conditional create is
       recorded as unsupported, because the service ignored the condition.
    5. Put with `REPLACE_IF_MATCH` and a wrong ETag, which must fail with 412. Then with the
       right ETag, which must succeed.
    6. Delete.
    7. Measure clock skew from the `Date` header. More than 5 minutes is a warning, because SigV4
       fails beyond 15 minutes.

    The result is stored on the connection and drives claim safety (R12).
  - **Retries**: exponential backoff with jitter on 5xx, 429, and timeouts, up to 5 attempts.
    Only idempotent requests are retried blindly. A conditional PUT that timed out is retried
    only after a re-read, because the first attempt may have succeeded.
- **Rationale**:
  - `object_store` is mature and widely used, for example by DataFusion and delta-rs. It
    supports conditional writes and checksums, works with non-AWS S3 services, and is async on
    tokio.
  - Its dependency tree is far smaller than the AWS SDK's.
  - It also supports Azure Blob, which keeps a later native Azure connector cheap (spec
    Assumptions).
- **Alternatives considered**:
  - `aws-sdk-s3`: the full API, but AWS-first defaults. The SDK's default integrity
    protections, introduced in early 2025, sent checksums that several S3-compatible services
    rejected until clients opted out. It also has a large dependency tree and long compile
    times.
  - `rusty-s3`: a small sans-IO signer. FileDock would have to build retries, multipart, and
    streaming itself.
  - `rust-s3`: less actively maintained, with uneven handling of non-AWS services.
  - A connector in Go using the MinIO SDK: a second language for one component.

## R24. UI design tokens, semantic components, and offline visual assets

**Revision**: 2026-10-13; US11 / FR-124, FR-130, FR-132, FR-134.

- **Decision**: Reuse the existing React/Vite application with CSS custom properties and thin
  components over native controls. Separate semantic foreground/background/focus tokens from
  brand swatches so actual text/control pairs meet the approved contrast thresholds. Bundle the
  normal variable Inter WOFF2 from the official v4.1 release, preserving its OFL/copyright notice;
  record asset source and checksum at implementation. Use original inline SVGs for the small
  current icon set. No UI framework, icon font, runtime CDN, or chart package is introduced.
- **Rationale**: The current forms need consistent presentation and semantics, and the appliance
  must work offline. Vite processes referenced assets and ships public assets in the build
  ([Vite asset handling](https://vite.dev/guide/assets)). The font release is available from
  [Inter v4.1](https://github.com/rsms/inter/releases/tag/v4.1); redistribution must retain the
  [Inter license](https://github.com/rsms/inter/blob/v4.1/LICENSE.txt). The guide's blue/green/amber
  swatches are not automatically suitable for every text/background combination; actual pairs
  must satisfy [W3C contrast guidance](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
- **Alternatives considered**: A component/CSS framework adds dependencies and replacement work;
  page-local styling reproduces inconsistency; CDN fonts fail offline; system fonts alone miss
  the approved Inter treatment; copying screenshot logos/icons provides poor scaling and unclear
  provenance. A chart library is reconsidered only with a real measurement/history feature.

## R25. Shared shell, hash navigation, and implementation availability

**Revision**: 2026-10-13; FR-125, FR-126, FR-131, FR-133.

- **Decision**: Use an authenticated AppShell and branded public AuthLayout. A small hash-route
  registry supports Dashboard and Settings with administrator/token/network/operation/audit
  child views. Current APIs can populate operation/audit read views; all remaining reference
  destinations stay absent until their screens and required sources ship. Account identity comes
  from `/auth/me`. Omit global search and the alert bell until their actual behavior exists.
  Narrow navigation is a disclosure with native controls and explicit focus handling.
- **Rationale**: App.tsx currently uses page state, with no router dependency or server deep-link
  fallback. Hash routes add back/forward and refresh semantics while keeping the HTTP request at
  `/`, which works with the present static host. Ordinary sidebar navigation should use links
  and disclosure semantics, not an application menu ([W3C disclosure navigation](https://www.w3.org/WAI/ARIA/apg/patterns/disclosure/examples/disclosure-navigation/)).
  A route registry is a UI implementation inventory, not a replacement for server authorization
  or subsystem capability discovery.
- **Alternatives considered**: Keeping ephemeral page state loses addressable views/back/forward;
  adding a routing dependency and server fallback is unnecessary for this bounded increment;
  enabling every reference destination creates unusable pages. A universal capability endpoint
  would expand backend scope; future features integrate their actual availability checks.

## R26. Truthful dashboard models and preserved security/live lifecycle

**Revision**: 2026-10-13; FR-127–FR-129, FR-134, SC-028, SC-030.

- **Decision**: Compose transient typed view models from implemented status, Core health, identity,
  operations, and audit responses. Retain raw states; distinguish unavailable/unknown from empty
  or zero and mark cached components stale during a Core or browser-stream outage. Show all
  applicable notices. Use a bounded recent-operation preview with a paginated full list; keep
  administrative audit activity distinct from system events. Only actor/action audit filtering
  is supported today. A local timestamp is "Last fetched", never an invented observation time.
- **Rationale**: The existing implementation contains no pool/folder/cloud counts, capacity,
  backup history, cache/CPU/RAM history, alert list, or container inventory. A false protection
  warning flag alone cannot prove a durable remote recovery point. Preserve one live connection
  across shell navigation, server mutation guards, the quiet startup 401 probe, session expiry,
  CSRF, and one-time secrets. Cached read data must not make stale healthy cards appear current.
- **Alternatives considered**: Screenshot fixtures in production would invent state; merged
  anonymous "recent activity" obscures audit actors and event scope; mounting subscriptions per
  page causes unnecessary reconnects; periodic full dashboard polling weakens the existing event
  design. New collectors/summary endpoints are unnecessary for the approved first UI increment.

## R27. Visual acceptance, accessibility, and incremental delivery

**Revision**: 2026-10-13; FR-135, SC-027–SC-031.

- **Decision**: Add test-only deterministic state fixtures and visual scenarios using existing
  Playwright/Vitest. Capture all implemented pages and shared states at desktop/360 px, with fixed
  clocks/data and fonts ready. Generate/compare baselines in the same pinned Linux/Chromium
  environment, approving the initial images by reference review. Supplement with real 200% browser
  zoom at 1280 px, keyboard/focus/contrast/44 px target checks, blocked external networking, live
  dirty-form preservation, and five-person novice acceptance. Preserve prior completed tasks and
  append new US11 tasks when `/speckit-tasks` runs.
- **Rationale**: Screenshot output varies with browser/OS, so environment-specific baselines are
  required ([Playwright visual comparisons](https://playwright.dev/docs/test-snapshots)). Screenshots
  alone cannot prove keyboard operation, real browser zoom, or the novice task outcome. Reflow
  review must account for enlarged content and scoped table/chart scrolling ([W3C reflow](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html)).
- **Alternatives considered**: Pixel-matching the entire sample enforces fictional contents;
  screenshots taken against changing VM data create unstable baselines; a 640 px viewport alone
  is not proof of actual browser zoom; resetting completed backend tasks loses implementation
  history. The UI increment does not certify storage/app flows that are still in later stories.

**Phase 0 UI research result**: Decisions R24–R27 resolve the UI revision's stack, asset provenance,
route integration, real-data scope, state handling, and acceptance approach. No technical
clarification remains. Independent read-only research reviewed visual tooling and the local
API/auth/live contracts; no code changes, package installs, or tests were performed.


## R28. Single nested navigation and current route ownership

**Revision**: 2026-10-15; FR-125/126/131/134. Supersedes R25's separate Settings navigation.

- **Decision**: One sidebar, nested Settings children, one exact current leaf. The Settings
  disclosure communicates expansion, not selection. Folder create/edit belongs to Folders;
  operation detail belongs to Operations. Register explicit create/edit hashes in existing routing.
- **Rationale**: Local inspection of Sidebar.tsx found every non-Dashboard item tests
  `route.id !== 'dashboard'`; AppShell.tsx adds a settings-nav in content. Both directly explain
  the reported selection and duplicate-rail issues. Current destinations already enumerate
  implemented links, so no backend navigation service or router package is needed.
- **Alternatives considered**: Two rails/long horizontal tabs repeat navigation; broad URL-prefix
  selection recreates ambiguity; changing to a new router expands scope without addressing the cause.

## R29. Data-first lists, focused actions, shared density

**Revision**: 2026-10-15; FR-124/130/132/134/135.

- **Decision**: Compact semantic table columns and row actions; summaries replace permanently open
  creation forms. Extend existing Button/DataTable/Dialog with size/alignment/footer contracts.
  Keep 44px effective targets even for 36px visible controls. Preserve local Inter with a refined
  28/20/16/14/12 scale and4px micro grid. Short actions use dialogs; long reviews use pages.
- **Rationale**: Local DataTable has string-only headers and global minimum-width/wrapping rules;
  Button has one generous size and `.actions` wraps long controls. Admins/Tokens render permanent
  create/password forms, whereas Notifications already provides a reusable dialog pattern. Existing
  native dialog supports modal focus behavior but needs a coherent form footer/initial-focus contract.
- **Alternatives considered**: Shrinking all hit areas violates FR-132; hiding all actions on hover
  harms discoverability/accessibility; page-specific CSS creates drift; a new UI kit is unnecessary.
  Audit/security/recovery semantics are not changed by moving their controls.

## R30. Full-page folder editor and prototype authority

**Revision**: 2026-10-15; FR-065/066/129/131/134/135.

- **Decision**: Routed create/edit pages, six expanded numbered sections on one scroll, no tabs or
  wizard. Generated path is a read-only preview. Keep mode-dependent inputs and explicit inapplicable
  notes. Save three generated boards under assets; annotate earlier second-rail/wizard treatments
  as superseded. Save reusable measurements in `.interface-design/system.md`.
- **Rationale**: The user approved the visual direction and explicitly chose one-page sections
  and one nested sidebar. All required settings need space. Existing FolderForm holds six sections
  but Advanced is collapsed and FoldersPage switches view with local state. Written contracts resolve
  the raster's double Settings highlight, omitted Operations link and missing Hybrid importance.
- **Alternatives considered**: Modal wizard obscures complex validation; step gating hides settings;
  taking generated artwork literally contradicts functional requirements; prototypes are not visual
  acceptance evidence. No new image is needed because all remaining ambiguity is specified in writing.

**Phase 0 result (2026-10-15)**: R28–R30 resolve this update. Read-only delegated research inspected
actual components, styles, routes and form guards. No production code, package installs or tests
were changed/run. Constitution gates pass before research and after design.
