<!--
Sync Impact Report
- Version change: 1.0.0 → 1.1.0 (MINOR: one principle added, one rule added, nothing removed)
- Modified principles:
  - III. Recoverable: added a rule that recoverable means restorable to the latest durable recovery
    point, not lossless. The 5-minute recovery point and all existing obligations are unchanged.
- Added sections:
  - Core Principles: V. Single-Node, Not Highly Available
  - Compliance: rejection rule for high-availability machinery and zero-loss or zero-downtime
    promises, and a review check for ownership checks on cloud write paths
- Removed sections: none
- Templates: plan, spec, and tasks templates read the constitution at runtime, so none were
  edited. The Constitution Check in plans now covers Principle V.
- Specs: specs 001–004 already match (001 FR-085 to FR-092, 002 FR-046 to FR-049, 003 FR-038 to
  FR-040, and 004 Assumptions).
- Deferred TODOs: none
-->

# FileDock OS Constitution

## Core Principles

### I. Fast

User-visible storage access MUST feel local even when capacity lives in the cloud.

- Hot reads, directory listings, and repeated opens of unchanged cached objects
  MUST be served from local metadata and cached byte ranges.
- A routine media library scan MUST be satisfiable from local metadata and
  cached ranges when the remote identity is unchanged.
- Existing SMB shares, NFS exports, cloud mounts, and running containers MUST
  keep serving I/O when the management service or UI is down.
- A change that adds a synchronous remote round trip to a hot read or listing
  path MUST document why a local path cannot meet the request.

Rationale: the product promise is local-speed access on a small appliance,
not cloud latency on every open.

### II. Efficient

Work and local capacity MUST stay proportional to what changed and what is in use.

- Local disk is cache and staging. Designs MUST remain viable on about 1–2 TB
  of local storage and a 100–500 GB cache against cloud-scale capacity.
- Protection cycles MUST advance from a change journal or equivalent incremental
  state. A full rescan on the default interval is allowed only when that state
  is missing or inconsistent.
- Read cache MUST be disposable and reconstructable from the authoritative
  source. Deleting it MUST NOT lose data or force a permanent second copy.
- FileDock MUST orchestrate proven tools for SMB, NFS, cloud access, backup,
  containers, and supervision. Reimplementing a protocol stack or backup format
  requires a written exception.
- Background reconciliation, eviction, and backup MUST yield to foreground I/O.

Rationale: a small appliance stays fast only if it avoids repeated full scans,
duplicate authoritative copies, and unnecessary protocol reimplementation.

### III. Recoverable

Everything on the appliance MUST be either durable or rebuildable. The physical
machine MUST be replaceable.

- Recovery from total appliance loss MUST require only a fresh install, the
  user's recovery credentials, reconnection to configured cloud repositories,
  restoration of protected configuration and application data, and a rebuild of
  disposable caches.
- Configuration, credentials, tier metadata, protected local-primary data, and
  application state MUST be durable. The OS image, binaries, container images,
  caches, and temporary data MUST be treated as disposable.
- Protected local data MUST support a recovery point objective of about 5
  minutes when the latest scheduled backup completed. Status MUST show scheduled
  frequency, the last successful durable recovery point, pending local changes,
  and backup failure separately.
- Local data MUST NOT be evicted or deleted until the remote copy exists,
  durability is verified, and metadata is committed. A crash at any earlier
  step MUST leave a recoverable state.
- A raw copy of a running database MUST be labeled crash-consistent. Logical
  consistency requires an application-aware backup path.
- Recoverable means restorable to the latest durable recovery point. It does
  not mean lossless (see Principle V).

Rationale: cloud capacity is useful only if a failed box can be replaced
without silent data loss.

### IV. Extensible

New behavior MUST plug in through declared capabilities. The core MUST NOT grow
a hardcoded branch for each backend.

- Storage, protocol, backup, cache, and container behavior MUST be delivered by
  subsystems that declare capabilities. Managed folders request capabilities;
  the orchestrator resolves them.
- Third-party code MUST NOT load into the core process. Extensions MUST run
  out of process as a native process, a systemd service, or a Docker service.
- Every subsystem MUST expose identity, version, capabilities, desired state,
  current state, configure/start/stop/restart/reload, health, events, and
  diagnostics, plus a declarative manifest.
- Core subsystems MUST NOT be removable. Built-ins ship and update with the
  appliance. Optional extensions MUST be installable without forking the core.
- Applications MUST consume FileDock-managed folders. Important state MUST NOT
  live only in an opaque container volume when a managed bind mount can hold it.

Rationale: fault isolation and a stable capability contract let the appliance
gain backends and apps without rewriting the orchestrator.

### V. Single-Node, Not Highly Available

FileDock MUST be a single-node, single-writer appliance, optimized for low cost
and recoverability. It accepts downtime and bounded data loss in exchange for
that.

- Each cloud object storage location FileDock writes to MUST have exactly one
  writer, the owning appliance, and writes MUST flow one way to it. This covers
  Hybrid Backup destinations, Tiered Storage cloud tiers, and appliance state.
  FileDock MUST NOT import or merge changes that other systems make there.
- These are non-goals: a clustered controller, active-active FileDock, a
  distributed lock manager, a shared writable filesystem, load-balanced storage
  nodes, a consensus protocol, and a multi-writer cloud tier. Adding any of
  them requires a MAJOR amendment.
- Downtime is accepted. When the appliance fails, its data MAY be unavailable
  until the appliance is repaired or restored on new hardware. No failover
  node is provided.
- Data loss is accepted, but it MUST be bounded and visible. The following MAY
  be lost with the appliance:
  - changes made after the latest durable recovery point;
  - data with no off-appliance copy, such as Local Only data, hot-tier data
    not yet migrated, Rebuildable data without backup, and Temporary data.

  The appliance MUST show this exposure before a loss and MUST report what was
  lost after a restore. Loss MUST never be silent.
- The product MUST NOT promise zero downtime or zero data loss. It promises
  low-cost recoverability, with a target recovery point as recent as 5 minutes.
- Disaster recovery is the only exception to the single-writer rule. A second
  installation MAY read an owned location in order to restore from it, and MAY
  write only after an explicit, audited takeover. The appliance MUST check
  ownership before each write session and MUST stop writing once it no longer
  owns the location.

Rationale: high availability would multiply cost and complexity on a small
appliance. A single writer with frequent, verifiable recovery points gives
most of the protection for a fraction of the price, and is far easier to
reason about.

## Operating Constraints

- The control plane and the data plane MUST be separate. Management failure
  MUST NOT be required to stop existing shares, exports, mounts, or containers.
- Backup jobs MAY delay on control-plane failure. They MUST NOT corrupt storage.
- Destructive tiering and write-back MUST follow create, verify, commit, then
  evict. No other order is acceptable.
- Bare metal is the primary deployment target. The same principles apply to
  virtual-machine and container installs.
- Product requirements in `FileDock-OS.md` guide implementation. Where they
  conflict with this constitution, this constitution wins.

## Compliance

- Specs, plans, and pull requests MUST name the principles they affect.
- A change that slows a hot path, replaces incremental protection with routine
  full scans, weakens the 5-minute recovery point, deletes local data before
  remote durability is verified, or loads third-party code into the core
  process MUST be rejected or carry an explicit exception.
- An exception MUST name the principle, the reason, and the condition that
  removes it.
- Reviewers MUST check data-plane independence, cache disposability, and the
  durability-before-delete order on any storage or recovery change.
- A change MUST be rejected if it introduces high-availability machinery
  (clustering, consensus, distributed locking, or multiple writers to an owned
  location) or promises zero downtime or zero data loss.
- Reviewers MUST check that every cloud write path verifies ownership first,
  and that every unprotected data class is shown to the administrator.

## Governance

This constitution supersedes conflicting practice. Amendments MUST be written
into this file, MUST bump the version below, and MUST record impact in the
sync comment at the top.

Version policy:

- MAJOR: remove or redefine a principle in a backward-incompatible way.
- MINOR: add a principle or materially expand a rule.
- PATCH: clarify wording without changing obligations.

Compliance is reviewed when a spec, plan, or pull request is written.
`FileDock-OS.md` is the runtime product guide and MUST NOT weaken these rules.
Complexity beyond orchestration of proven tools MUST be justified against
Efficient and Extensible.

**Version**: 1.1.0 | **Ratified**: 2026-10-11 | **Last Amended**: 2026-10-11
