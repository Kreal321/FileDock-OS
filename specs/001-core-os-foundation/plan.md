# Implementation Plan: Core Appliance Foundation and OS Setup

**Branch**: `001-core-os-foundation` | **Date**: 2026-10-11 | **Spec**: [spec.md](./spec.md)

**UI revision**: 2026-10-13 — US11, FR-124–FR-135, SC-027–SC-031.
**Current Git checkout**: `main`; `001-core-os-foundation` is the selected feature identifier.

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

The UI revision is planned in the existing `web/` application, using the user-approved
[design guide](./ui-design-guide.md) and [UI contract](./contracts/management-ui.md). It replaces the
current presentation with a light shared shell and components while retaining the service,
authentication, and live-event foundations. Existing completed tasks retain their history;
US11 requires new work and separate visual acceptance before the updated P1 delivery is accepted.

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
- Web: Vite, React, TanStack Query, `@microsoft/signalr`; shared native HTML controls and CSS
  custom properties. A small hash-route registry provides navigation without a router dependency.
  Inter v4.1 is a pinned local font asset with its license; icons are original inline SVGs.
  No UI framework or charting dependency is needed for the current screens.
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

All Technical Context items are resolved. See [research.md](./research.md), R1 to R27.
UI view models are transient and introduce no database migrations or new REST/gRPC endpoints.
The UI continues to ship inside Management on HTTP port 8021. Visual fixtures are test-only.
Browser coverage is current stable Chromium, Firefox, and WebKit engines; screenshot baselines
are generated and compared in one pinned Linux/Chromium environment.

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

## UI revision: implementation approach and delivery

### Existing implementation and scope

The present screens are setup, sign-in, recovery-key download, Dashboard, Administrators,
API tokens, and Network settings. Current REST routes also provide identity, operation history,
and audit history. The redesign includes read-only Operations/detail and Audit views over those
existing routes so these tools are discoverable under Settings. It adds no new privileged work.

The immediate sidebar has **Dashboard** and **Settings**. Settings links to Administrators,
API tokens, Network, Operations, and Audit when those screens ship. Other reference destinations
are added in its prescribed order with their original stories. Search, an alert bell/count,
folder/pool/cloud statistics, resource/cache graphs, backup successes, and container lists stay
absent until real screens and sources are available. A false `stateProtectionWarning` flag is
not evidence that remote recovery coverage exists; the current API lacks the detailed US4 data.

### Shared foundations

- Centralize guide colors, accessible foreground variants, 32/20/16/14 px type scale, 8 px spacing,
  12 px cards, borders, shadows, and focus states in `web/src/styles/tokens.css`; keep application
  layout/utility rules in `styles.css`. Brand blue remains `#3B82F6`; white-text primary buttons
  use a contrast-compliant darker variant, documented in the UI contract.
- Build thin semantic Button, Field, Select, Switch, Tabs, StatusBadge, Notice, Panel, PageHeader,
  StateView, Dialog, and responsive table components. Standardize states before migrating forms.
  Use native elements and disclosure navigation rather than custom application-menu semantics.
- Ship Inter v4.1 normal variable WOFF2 through Vite's asset processing, using `font-display: swap`
  and a sans-serif fallback. Record source, release, file checksum, and original OFL/copyright
  notice. Serve the license under `/assets/licenses/` so it is accessible during setup. Original
  folder/storage/cloud/service/settings SVGs share sizing and stroke conventions.
- Build an authenticated AppShell with desktop Sidebar and TopBar and a narrow-screen disclosure
  menu; keep a separate branded AuthLayout for setup/sign-in. Identity comes from `/auth/me`.
  Omit the reference's outer guide title, design-legend panels, example user, and sample dates.
- Use a single hash-route table with `#/dashboard` and Settings child routes. Browser back/forward
  and refresh preserve valid views without Management deep-link fallback changes. Unknown routes
  lead to Dashboard with an unavailable-page notice after authentication. Public setup/sign-in
  gates take precedence over every protected hash; no compatibility aliases are required.

### Data, state, and security integration

Typed adapters compose the existing responses into the view models in [data-model.md](./data-model.md).
Retain raw state text, distinguish unavailable data from a confirmed zero, and show every applicable
notice. Derive service counts only from received component records. Current mode, protection, and
history fields are not interchangeable; there is no generic "everything protected" badge.

One authenticated event subscription stays mounted in AppShell across navigation. Reuse query
keys, event invalidation, reconnect resync, and the durable backend cursor. Mutations require both
browser live connectivity and Core `canMutate`; navigation, inspection, retry, and sign-out remain
usable during an outage. Cached subsystem values are visibly stale even when a fresh HTTP response
contains them. Local fetch times are labeled as receipt times, not Core observation times.

Preserve the quiet startup auth probe: 423 means setup, 401 means sign-in. Network/502/503 failures
show appliance-unavailable/retry instead of pretending the session is invalid. Unexpected protected
401 clears private queries and one-time secret state and ends the live connection. Use session
cookies and CSRF headers as today; never put credentials or secret state in hashes/storage/screenshots.

Live refetches must not replace a dirty form or move focus. Server checks remain authoritative.
Disable individual mutation regions/actions instead of enclosing all read-only settings in a
disabled fieldset. Pending switches/buttons describe saving; accepted long-running work links to
an operation instead of showing immediate success. Preserve default editable `root`, matching
password confirmation, advisory length guidance, recovery download-to-continue, last-admin guards,
current-password checks, token secret-once behavior, and network disconnect acknowledgement.

### Ordered implementation slices for task generation

1. Define typed UI models/adapters and design tokens; add test-only state fixtures and local font,
   icon, and license assets. Establish shared components and their accessibility/state contracts.
2. Introduce hash navigation, AppShell/AuthLayout, top-bar identity, responsive disclosure and
   Settings subnavigation; keep auth and one live subscription intact.
3. Migrate setup, sign-in, recovery download, administrator, token and network forms without
   changing their behavior. Verify focus, busy/error states, one-time secrets, and outage guards.
4. Compose the truthful health-first dashboard, complete notices, recent operations, and clearly
   labeled administrative activity. Add read-only Operations/detail and Audit screens with paging
   and only supported filters. Manual reconcile remains optional and is not required for US11.
5. Establish reviewed visual baselines for all implemented pages and component states; run offline,
   keyboard, contrast, responsive, dirty-form/live-event and security regression checks. Follow the
   [UI validation guide](./ui-validation.md) and combined [Phase 4–5 VM guide](./phase5-testing.md).
6. Apply shared primitives to each later story when it ships. Charts are introduced only alongside
   real current/history data and textual equivalents; resource/resource-history backends remain
   in their existing scope, not in this UI increment.

These are design slices, not completion marks. `/speckit-tasks 001` must append US11 work with new
IDs and dependencies, preserving all existing IDs/completion marks, including completed T001–T087
and repair/usability tasks T185–T201. The highest current ID is T201; new UI tasks start after it.
Place the US11 delivery checkpoint after Phase 5 and before later story implementation. It must connect
later UI tasks to the shared foundations and add SC-027–SC-031 acceptance coverage. No production
code or automated tests are created/executed during this planning command.

### UI constitution gate: before research and after design

| Principle | UI design check | Result |
| --- | --- | --- |
| I — Fast | Layout and assets stay local; one live subscription reuses the control plane; no UI operation enters storage I/O paths | PASS |
| II — Efficient | Existing stack, small native components, paged lists, bounded previews; no new framework, metric collector, routine full polling, or remote font runtime | PASS |
| III — Recoverable | Protection/exposure notices stay visible; UI view state is rebuildable; secrets and existing recovery flow retain their safeguards | PASS |
| IV — Extensible | Available destinations follow implemented features/providers; future screens reuse shared components; no backend-specific protection promises | PASS |
| V — Single-node | No HA, failover, cloud write/ownership, or zero-loss behavior is introduced; a successful operation is scoped to its real target | PASS |
| Operating constraints | UI outages/reconnects do not restart data-plane units; authentication, root console, CSRF, and Core mutation guards are preserved | PASS |

Post-design review of the presentation models, UI contract, and validation guide passes the same
gates. No unresolved technical clarifications or constitution exceptions remain.

## Project Structure

### Documentation (this feature)

```text
specs/001-core-os-foundation/
├── plan.md              # This file
├── research.md          # Phase 0: decisions R1–R23
├── data-model.md        # Phase 1: entities, classification, state machines
├── quickstart.md        # Phase 1: build/install/validation guide
├── ui-design-guide.md   # approved visual requirements
├── ui-validation.md     # redesign validation and visual/VM acceptance
├── assets/management-ui-design-guide.png
├── contracts/
│   ├── core-api.proto                 # Management ⇄ Core gRPC
│   ├── extension-protocol.proto       # Core ⇄ subsystems/extensions (incl. notify.channel)
│   ├── subsystem-manifest.schema.json # manifest schema (FR-047)
│   ├── management-api.md              # REST /api/v1 contract (UI + API tokens)
│   ├── management-ui.md               # UI routes/components/state/data contract
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

Planned UI additions use these existing roots:

```text
web/src/styles/tokens.css
web/src/assets/fonts/InterVariable.woff2
web/src/assets/fonts/SOURCE.md
web/public/assets/licenses/inter-OFL.txt
web/src/components/{Button,Field,Select,Switch,Tabs,Dialog,Panel,PageHeader,StatusBadge,Notice,StateView,DataTable,Icon}.tsx
web/src/layout/{AppShell,AuthLayout,Sidebar,TopBar}.tsx
web/src/navigation/{destinations,useHashRoute}.ts
web/src/api/view-models.ts
web/src/features/dashboard/dashboard-model.ts
web/src/features/settings/SettingsPage.tsx
web/src/features/operations/{OperationsPage,OperationDetailPage}.tsx
web/src/features/audit/AuditPage.tsx
web/tests/fixtures/ui-states.ts
web/tests/e2e/ui-design.spec.ts
```

Paths describe planned responsibilities; they are not newly generated source files. Fonts are
acquired and checksummed during implementation, not downloaded on appliance startup. All UI output
continues to go to Management's `wwwroot` and into the offline release archive.

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
