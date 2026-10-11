# Implementation Plan: Core Appliance Foundation and OS Setup

**Branch**: `001-core-os-foundation` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-core-os-foundation/spec.md`

## Summary

This feature turns a fresh Debian 13 machine into a FileDock appliance. A pinned installer sets up
two first-party services:

- An **unprivileged ASP.NET Core Management Service**. It serves a React web UI and a REST API
  usable with API tokens.
- A **privileged Rust Core Service**. It owns desired-state reconciliation, the operation journal,
  storage, secrets, subsystems, alerts, and appliance-state protection.

The two services talk over gRPC on a Unix socket. The Core orchestrates proven tools rather than
reimplementing them:

- LVM and ext4 for RAID-0-style **pools** on data disks;
- a capped, loop-mounted ext4 image for the built-in **System Pool** on the system disk, limited
  by default to 50% of the disk with at least 100 GB always kept free;
- Samba, kernel NFS, and rclone for exports and WebDAV;
- pluggable **cloud connectors** for cloud I/O, behind one contract:
  - the built-in native S3 connector `fd-s3`, built on `object_store`, for S3-compatible
    storage, with conditional ownership claims and verified integrity;
  - rclone, the general connector, for every other provider and for mounts.

  Connector routes, with a family default and optional per-connection overrides, decide which
  connector serves each provider family and use;
- Docker Compose for applications.

Data-plane processes run as their own systemd units, so shares and containers survive restarts of
either service.

Recovery rests on these pieces:

- encrypted appliance-state snapshots with a 5-minute recovery point. Each one is a declarative
  configuration document plus the secret store, closer to a configuration file than to a
  database copy. It holds folders, core configuration, admins, tokens, and pool names and
  identities. It holds no disk assignments and no history;
- extension packages stored once per version beside the snapshots, with extension data kept in
  managed folders protected by their mode, typically Hybrid Backup;
- local copies of those snapshots, kept automatically on every data pool and on the system disk,
  plus an optional single-writer remote destination;
- a portable recovery key, which is the only input needed when reinstalling on the same machine;
- a downloadable `.fdbackup` file, which can rebuild the same appliance anywhere with the setup
  code and the recovery key;
- pool identities, which are reattached in place or remapped to new disks on restore;
- an ownership claim that makes takeover explicit. Takeover is automatic only after a reinstall.

There is no high availability, no scale caps, and no local disk encryption in this release.

## Technical Context

**Language/Version**:

- Rust stable, edition 2024 (Core Service)
- C# on .NET 10 LTS (Management Service and `filedockctl`)
- TypeScript 5 with React 19 (web UI)
- Bash (installer)

**Primary Dependencies**:

- Core: `tokio`, `tonic`/`prost`, `rusqlite` (bundled), `rusqlite_migration`, `serde`,
  `tracing`, `chacha20poly1305`, `argon2`, `zstd`, `tar`
- Native S3 connector (`fd-s3`): `object_store` (feature `aws`), `aws-sigv4` for the few
  requests the crate lacks, `sha2`, `tonic` (research R23)
- Management: ASP.NET Core minimal APIs, SignalR, EF Core with SQLite, `Grpc.Net.Client`,
  Konscious Argon2
- Web: Vite, React, TanStack Query, `@microsoft/signalr`
- System, pinned in `versions.lock`: systemd, LVM2, e2fsprogs, smartmontools, Samba,
  `nfs-kernel-server`, rclone, Kopia, and Docker Engine with the Compose plugin

**Storage**:

- SQLite `core.db` (Core) and `app.db` (Management). The two databases are separate and never
  shared.
- Encrypted `secrets.enc`.
- Appliance-state snapshots at every copy: `<pool root>/.filedock/appliance-state/` on each data
  pool, `/var/backups/filedock/appliance-state/` on the system disk, and the optional remote
  destination, which is a single-writer object storage prefix or a removable disk
  ([contracts/appliance-state-layout.md](./contracts/appliance-state-layout.md)).
- User data lives in pools, mounted at `/mnt/filedock/pools/<id>` and bind-mounted to
  `/storage/<folder>`. Data pools use LVM plus ext4. The System Pool is a sparse ext4 image at
  `/var/lib/filedock/system-pool/pool.img` (research R21).

**Testing**:

- `cargo nextest` (Core)
- xUnit with `WebApplicationFactory` (Management)
- Vitest and Playwright (web)
- `buf lint` and breaking-change checks, plus JSON Schema validation (contracts)
- `pytest` driving QEMU/libvirt Debian 13 VMs (acceptance, DR, and failure injection)

**Target Platform**: Debian 13 "trixie" amd64, on bare metal or a VM. The minimum is 2 GB RAM
(4 GB recommended) and a 16 GB system disk. The System Pool needs a system disk larger than the
reserve plus the OS, so more than about 120 GB at the default 100 GB reserve. Data pools need data
disks other than the system disk.

**Project Type**: An appliance OS layer: a privileged system daemon, an unprivileged web service
with a single-page app, Debian packaging, and an installer.

**Performance Goals**:

- Idle usage of both services together under 512 MB RAM and under 2% of one core (SC-009).
- An appliance-state snapshot at every available copy within 1 minute of a change for 95% of
  changes, and always within 5 minutes (SC-005).
- A reinstall restore with only the recovery key completes in under 30 minutes (SC-021).
- The System Pool's free-space watchdog checks every 2 seconds and stops writes before the reserve
  is reached (SC-022, research R21).
- Alerts in the UI within 1 minute of the condition (SC-020).
- Pool failure detected within 1 minute (SC-018).
- Restore of configuration in under 60 minutes, excluding user data (SC-006).
- Folder creation in under 2 minutes with default settings (SC-011).

**Constraints**:

- The Management Service never runs privileged work.
- The data plane is independent of both services.
- No plaintext secrets at rest, in logs, or in snapshots.
- Recovery never depends on hardware-bound keys.
- Each cloud location has one writer.
- Cloud I/O goes only through the connector routed for it, with no automatic fallback. A
  connector holds only the credentials of the connections routed to it.
- The UI is for the local network only.
- No HA machinery (Principle V).
- The System Pool never takes system disk free space below the reserve, and never deletes data to
  stay within it.

**Scale/Scope**:

- **No scale limits** in this release, as directed by the user. APIs paginate, events resume by
  cursor, and reconcilers are incremental (research R20).
- One appliance, a few administrators, any number of pools and folders.
- Local disk encryption is out of scope, with a `layers` hook for later (research R9).

All Technical Context items are resolved. See [research.md](./research.md), R1 to R23.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Checked against constitution **v1.1.0**.

| Principle or rule | How this plan complies | Gate |
|---|---|---|
| **I. Fast**: shares, mounts, and containers keep serving when the Management Service or UI is down | Samba, NFS, Docker, WebDAV (as transient units), pool mounts, and bind mounts are independent systemd units. The Core is never in the I/O path, and the Management Service never is either (R7). Quickstart checks it | PASS |
| **I. Fast**: no synchronous remote round trip on hot paths | This feature adds no cloud reads to the I/O path. Cloud I/O covers only snapshots, claims, and tests | PASS |
| **II. Efficient**: orchestrate proven tools and don't reimplement them | Pools use LVM and ext4. The System Pool uses a loop device and ext4. Exports use Samba, NFS, and rclone WebDAV, cloud I/O uses rclone rcd plus a thin native S3 connector over the established `object_store` crate (no provider code written from scratch), apps use Docker Compose, and logs use journald. Snapshots serialize existing desired state, with no second database engine or replicator (R8, R11, R13, R15–R17, R21, R23) | PASS |
| **II. Efficient**: background work yields to foreground I/O | Snapshot uploads and SMART polling run at low CPU and I/O priority (`Nice`, `IOSchedulingClass=idle`) | PASS |
| **III. Recoverable**: total loss needs only a fresh install, the recovery key, and the destination | The restore flow (R11), pool remapping (R8), and a portable recovery key under Argon2id (R10, R11). Reinstalling on the same machine needs even less: the recovery key alone, from local copies (R22) | PASS |
| **III. Recoverable**: a recovery point of about 5 minutes; database copies must be consistent | Appliance state has a 5-minute target at every copy (R11). It is a configuration document serialized inside one read transaction, never a raw copy of a live database. Configuration, credentials, and tier metadata are durable. Audit and alert history are local history that the constitution doesn't require to be durable | PASS |
| **III. Recoverable**: application state is durable | Container and extension data must live in managed folders. Extensions are bound at install and sandboxed so they can't write anywhere else. Hybrid Backup is suggested (FR-087, FR-109, R14, R16) | PASS |
| **III. Recoverable**: configuration is durable, and the OS, binaries, and caches are disposable | The data classification table in data-model.md. The snapshot holds only authoritative critical state | PASS |
| **III. Recoverable**: a crash at any step leaves a recoverable state | Every pool, restore, takeover, and extension operation is a journaled, idempotent state machine (R6) | PASS |
| **IV. Extensible**: capabilities, out-of-process extensions, and a manifest | Capability registry, `subsystem-manifest.schema.json`, the extension gRPC protocol, three runtime types, and permission sandboxing (R14) | PASS |
| **IV. Extensible**: providers are swappable | Cloud connectors, built-in or third-party, share the `CloudRemote` contract and the manifest's `cloud` section. Routes select a connector per provider family and use, and can be overridden per connection (R13, R23) | PASS |
| **IV. Extensible**: applications use managed folders | Compose bind validation, and a warning for data outside managed folders (R16) | PASS |
| **V. Single-node**: one writer per cloud location, and no HA machinery | `owner.json` claim and generation, a check before every write session, Recovery-reader restore, and audited takeover (R12). Claims are conditional where the service and connector support it. Elsewhere they are basic, with an acknowledgement and a standing warning (FR-123). No clustering, consensus, or locking | PASS |
| **V. Single-node**: loss is bounded and visible | Pool failure reports each folder's recovery path. Local Only folders show Unprotected. The restore report lists lost data. System Pool folders warn that they share the OS disk. The dashboard shows which losses are covered (FR-105) | PASS |
| **V. Single-node**: a takeover can't silently create two writers | An automatic takeover after a reinstall requires the generation recorded in the snapshot and no write in the last 15 minutes. Otherwise it needs confirmation, or the appliance stays a Recovery reader. Fencing at the next check before writing is unchanged (R12) | PASS |
| **V. Single-node**: no promise of zero downtime or zero loss | The UI copy says "recoverable to the latest recovery point". Reviewed against SC-017 | PASS |
| **Operating constraints**: separate control and data planes, and create → verify → commit → evict | This feature doesn't evict anything. Snapshot pruning happens only after the next snapshot is durable | PASS |
| **Compliance**: no third-party code in the Core process | Extensions run only as native-process, systemd-service, or docker-service units. Both cloud connectors run outside the Core too: `fd-s3` as its own unit, and rclone as `rclone rcd` behind a Core adapter | PASS |

**Result**: PASS. No violations, so Complexity Tracking is not needed.

### Post-design re-check (after Phase 1)

The data model, contracts, and quickstart introduce nothing that conflicts with the table above.
Specific points:

- The gRPC contract has no high-availability or multi-writer constructs. A manual takeover needs
  an explicit `observed_generation` and confirmation. The automatic takeover after a reinstall is
  limited to the generation recorded in the snapshot.
- Local copies need no ownership claim, because each pool and the system disk belong to one
  appliance. Only the remote object storage copy carries `owner.json`.
- The `fdconfig` document is versioned separately from the database schemas. It records pools
  without disks, so it rebuilds the same appliance on any hardware. A contract test ensures every
  desired-state resource kind is mapped.
- Extension data bindings use only existing sandbox mechanisms (systemd `BindPaths` and
  `ReadWritePaths`, Docker bind mounts), so no new privileged path is added.
- The `.fdbackup` restore path needs the setup code, like any source that isn't on this machine's
  disks.
- Restoring with only the recovery key is accepted without the setup code only for copies found
  on the machine's own disks. A remote source typed in by hand still needs the setup code, so
  nobody on the network can plant their own appliance state on a fresh install.
- The snapshot format is versioned and stays readable by later releases.
- Pools carry a `layers` field for future encryption, without changing the folder model.
- The REST API exposes the same capabilities to tokens as to the UI. Read-only tokens can't
  mutate anything.
- Connector routes are desired state: validated, audited, and stored in `config.json`. A route
  change is previewed, then applied with every acknowledgement the preview reported. Running
  write sessions stay pinned to the connector they started with.
- No automatic failover between connectors. A failed connector fails its uses visibly, so claim
  safety can never change without the administrator knowing.
- A contract test reads through each connector what the other wrote, so switching routes can't
  strand data at a location.
- Third-party connectors are fenced by the permission `cloud.credentials`, by receiving only the
  credentials of connections routed to them, and by a separate acknowledgement before they can
  serve owned locations.

**Result**: PASS.

### Design notes the reviewer should see

- **Adopted pools can't grow in this release.** An existing filesystem adopted as a pool is not
  under LVM, so the spec's optional "add a disk to a pool" (FR-062, a MAY) applies only to pools
  created from blank disks. The UI explains this when an administrator tries.
- **The System Pool makes single-disk appliances usable, within limits.** A machine with only a
  system disk can hold managed folders in the System Pool, but those folders are lost with that
  disk. Its reinstall coverage is also weaker: the only local copy is on the system disk, so a
  reinstall that erases that disk needs the remote destination.
- **"Only the recovery key" depends on a surviving local copy.** A copy on a data pool survives
  any reinstall of the system disk. After the loss of the whole machine, the remote destination is
  still needed, and the dashboard warns until one is registered.
- **Azure Blob owned locations are basic claim safety.** They are served by rclone, which has no
  conditional writes, so they carry an acknowledgement and a standing warning until a native Azure
  connector exists. `object_store` already supports Azure, which keeps that connector cheap.
- **`object_store` crate gaps.** A spike task confirms per-request storage class and the
  incomplete-upload listing for the pinned crate version. Either is covered by hand-signed SigV4
  requests if needed (research R23).
- **Test VMs**: a 256 GB sparse system disk gives a usable System Pool under the defaults. One or
  two extra blank disks are needed for the data-pool, pool-failure, and reinstall tests.

## Project Structure

### Documentation (this feature)

```text
specs/001-core-os-foundation/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1–R23
├── data-model.md        # Phase 1: entities, classification, state machines
├── quickstart.md        # Phase 1: build/install/validation guide
├── contracts/
│   ├── core-api.proto                 # Management ⇄ Core gRPC
│   ├── extension-protocol.proto       # Core ⇄ subsystems/extensions (incl. notify.channel)
│   ├── subsystem-manifest.schema.json # manifest schema (FR-047)
│   ├── management-api.md              # REST /api/v1 contract (UI + API tokens)
│   ├── appliance-state-layout.md      # snapshot format + owner.json claim
│   └── cli.md                         # install.sh and filedockctl
├── checklists/requirements.md
└── tasks.md             # Phase 2 (/speckit-tasks), not created here
```

### Source Code (repository root)

```text
proto/                               # source of truth for gRPC (copied from contracts/ at start)
├── filedock/core/v1/core.proto
└── filedock/extension/v1/extension.proto

core/                                # Rust workspace: Core Service
├── Cargo.toml
├── crates/
│   ├── filedock-core/               # daemon binary: startup, socket servers, wiring
│   ├── fd-api/                      # generated tonic/prost types
│   ├── fd-state/                    # core.db schema, migrations, desired-state store, ApplyChange
│   ├── fd-journal/                  # operation journal and state-machine runner
│   ├── fd-reconcile/                # controller framework + per-kind controllers
│   ├── fd-storage/                  # disk discovery, SMART, pools (LVM/ext4/adopt), System Pool
│   │                                #   (loop image, limits, reserve watchdog), mount units
│   ├── fd-subsystems/               # manifest, registry, capability resolution, sandbox mapping,
│   │                                #   adapters: samba, nfs, rclone, docker, kopia
│   ├── fd-connectors/               # connections, route table + resolution, route-change preview,
│   │                                #   CloudRemote client, rclone CloudRemote adapter, probes,
│   │                                #   claim safety
│   ├── fd-secrets/                  # secret store, DEK, recovery key, redaction
│   ├── fd-appstate/                 # fdconfig document, snapshots, package store, .fdbackup,
│   │                                #   local copies + remote destination,
│   │                                #   detect-state, ownership claims, restore, auto-takeover
│   ├── fd-alerts/                   # alert store, dedup, notify.channel fan-out
│   └── fd-observe/                  # journald queries, diagnostic bundles, event ring
└── tests/                           # component tests (temp dirs, loop devices, fake adapters)

connectors/                          # built-in cloud connectors (same Cargo workspace)
└── fd-s3/                           # native S3 connector: object_store, Subsystem + CloudRemote,
                                     #   probe, integrity, multipart sweep

management/                          # .NET solution: Management Service + CLI
├── FileDock.Management.sln
├── src/
│   ├── FileDock.Management.Api/     # host: REST /api/v1, SignalR hub, auth, static web UI
│   ├── FileDock.Management.Domain/  # admins, sessions, tokens, audit, setup state
│   ├── FileDock.Management.Data/    # EF Core SQLite (app.db) + migrations
│   ├── FileDock.Management.Core/    # gRPC client for core.sock, event relay, bundle export
│   └── FileDock.Management.Cli/     # filedockctl
└── tests/
    ├── FileDock.Management.UnitTests/
    └── FileDock.Management.ApiTests/   # WebApplicationFactory + fake Core

web/                                 # React + TypeScript SPA
├── src/
│   ├── api/                         # typed REST client + SignalR
│   ├── features/                    # setup, dashboard, pools, folders, shares, connections,
│   │                                #   connectors and routes,
│   │                                #   apps, extensions, alerts, logs, audit, recovery, settings
│   └── components/
└── tests/                           # vitest (unit), playwright (e2e)

packaging/
├── installer/install.sh
├── debian/                          # filedock-core, filedock-management, filedock-builtins
├── systemd/                         # units, socket units, drop-ins (journald cap, sandboxing)
├── subsystems/                      # built-in manifests: samba, nfs, rclone, fd-s3, kopia, docker
└── versions.lock                    # pinned third-party versions and checksums

tests/
├── e2e/                             # pytest + libvirt: install, setup, pools, folders, shares,
│                                    #   apps, snapshots, DR restore, takeover, failure injection,
│                                    #   connector routing (MinIO + a no-conditional-write stub)
└── fixtures/                        # sample .fdext packages (native/systemd/docker/notify-test)

Makefile                             # proto, core, management, packages, test
```

**Structure Decision**: The repository has three implementation roots, one per runtime the guide
names: `core/` (Rust), `management/` (.NET), and `web/` (SPA, built into the Management Service's
static files). Around them:

- `connectors/` holds built-in connectors that run as their own subsystems. They are in the Core's
  Cargo workspace but never linked into the Core binary.
- `proto/` holds the shared gRPC contracts.
- `packaging/` holds the installer and Debian packaging.
- `tests/e2e/` holds the VM acceptance suite.

This is the minimum the constitution's two-service split requires. Nothing more is added.

## Complexity Tracking

None. The Constitution Check has no violations.
