# FileDock OS

## Product Requirements and Implementation Guide

**Project name:** FileDock OS  
**Product type:** Lightweight software-defined storage appliance OS  
**Primary goal:** Reduce dependence on large traditional NAS systems by combining a small local storage footprint with cloud-backed capacity, intelligent caching, tiered storage, frequent incremental protection, and built-in Docker application hosting.

---

# 1. Vision

FileDock OS is a lightweight software-defined storage appliance for local, cloud, and hybrid storage.

The product is designed around a simple idea:

> **Less NAS. More cloud. Local-speed access.**

Instead of requiring users to purchase a large NAS chassis populated with many high-capacity drives, FileDock OS should allow users to combine:

- Small amounts of fast local storage
- Cloud object or cloud-drive capacity
- Intelligent local caching
- Hybrid local/cloud storage
- Frequent incremental recovery points
- Standard file sharing protocols
- Docker-hosted applications

A typical deployment may use:

- 1–2 TB local SSD or HDD storage
- 100–500 GB local cache
- Wasabi, Backblaze B2, Azure Blob, Google Drive, OneDrive, S3-compatible storage, or similar cloud services
- Recovery points as frequent as every 5 minutes

FileDock OS should make the physical appliance replaceable.

The architectural principle is:

> **Everything is either durable or rebuildable.**

The local operating system, application binaries, containers, Docker images, caches, and temporary data should be considered disposable and rebuildable.

Configuration, credentials, tier metadata, protected local-primary data, and application state must be durable.

A complete appliance failure should require only:

1. A fresh FileDock OS installation
2. The user's recovery credentials/key
3. Reconnection to the configured cloud repositories
4. Restoration of protected configuration and application data
5. Rebuilding disposable caches

The acceptable recovery point objective for protected local data is approximately **5 minutes**, assuming the most recent scheduled backup completed successfully.

---

# 2. Product Positioning

FileDock OS is **not** intended to compete directly with traditional NAS systems by reproducing every feature of Synology, TrueNAS, Unraid, or enterprise SAN systems.

It should instead focus on:

- Cloud-first and hybrid storage
- Small local storage footprints
- Intelligent caching
- Easy recovery
- Open standard protocols
- Extensibility
- Docker workloads consuming FileDock-managed storage

Suggested positioning:

> **A lightweight software-defined storage appliance that combines local cache, cloud storage, tiering, and frequent incremental backup to reduce the need for large traditional NAS systems.**

Short positioning options:

- **Less NAS. More cloud. Local-speed access.**
- **Small local storage. Cloud-scale capacity. Five-minute recovery.**
- **Your local storage gateway for cloud, cache, backup, and containers.**

---

# 3. Core Product Principles

FileDock OS should follow these principles.

## 3.1 Storage owns the folders

Docker, Samba, NFS, WebDAV, and other applications consume storage managed by FileDock OS.

Applications should not independently create opaque storage that FileDock OS later tries to discover.

Preferred model:

```text
FileDock Managed Folder
        ↓
/storage/appdata/emby
        ↓
Docker bind mount
        ↓
Emby /config
```

Avoid using opaque Docker-managed volumes for important state when a managed bind mount can be used instead.

---

## 3.2 Control plane and data plane must be separated

The management UI must not be required for existing shares or mounts to continue operating.

If the management service fails:

- Existing SMB shares should remain available.
- Existing NFS exports should remain available.
- Existing cloud mounts should continue when possible.
- Existing Docker containers should continue.
- Backup jobs may be delayed but should not corrupt storage.
- The core orchestration service must continue independently.

---

## 3.3 The core should orchestrate, not reimplement proven storage software

FileDock OS should use proven underlying tools where practical.

Examples:

- Samba for SMB
- Linux kernel NFS for NFS
- rclone for cloud-provider integration
- Kopia for backup
- Docker Engine for application containers
- systemd for process supervision

FileDock OS provides:

- Unified policy
- Lifecycle management
- Desired-state reconciliation
- Health monitoring
- Recovery
- Storage abstraction
- Caching
- Tiering
- Unified UI

It should not unnecessarily reimplement protocol stacks or backup formats.

---

## 3.4 Cache must be disposable

Read cache should never be authoritative.

Deleting the read cache must not result in data loss.

The appliance must always be able to reconstruct cache contents from the authoritative source.

---

## 3.5 No destructive action before durability is verified

For tier migration and write-back behavior, the sequence must be:

```text
Create/upload remote copy
        ↓
Verify remote durability
        ↓
Commit metadata/state
        ↓
Only then evict/delete local data
```

A crash during any intermediate step must be recoverable.

---

# 4. Supported Deployment Models

FileDock OS should support the following deployment models.

## 4.1 Bare metal

```text
Physical machine
        ↓
FileDock OS
        ↓
Storage + Docker
```

This is a primary deployment target.

---

## 4.2 Virtual machine

Example:

```text
Proxmox
   ↓
Debian/Ubuntu VM
   ↓
FileDock OS
   ↓
Docker + managed storage
```

This should also be a primary supported model.

The VM may receive:

- Passed-through disks
- Virtual disks
- Network-attached block storage
- Local SSD-backed virtual disks

---

## 4.3 LXC/container installation

This may be possible but should not be an initial primary support target.

Nested Docker, FUSE, mount propagation, device access, SMB/NFS, and privileged operations create additional complexity.

The preferred MVP support targets are:

1. Bare metal
2. VM

---

# 5. OS Strategy

FileDock OS should initially be an appliance layer on top of a stable Linux distribution rather than a custom Linux distribution.

Recommended base:

- Debian Stable

Alternative:

- Ubuntu Server

Debian is preferred because it provides:

- High stability
- Low overhead
- Mature Samba/NFS/FUSE support
- systemd
- Strong package availability
- Long lifecycle
- Good bare-metal and VM compatibility

The product should evolve in stages.

## Stage 1: Installable appliance package

Install FileDock OS onto a supported Debian installation.

## Stage 2: Automated appliance installer

An installer configures a blank Debian-compatible machine as a FileDock appliance.

## Stage 3: Bootable appliance image/ISO

A dedicated installation image may be introduced after the storage stack and upgrade process are mature.

The appliance should feel like an operating system to the user even if it is technically built on Debian.

---

# 6. Major Runtime Services

FileDock OS should contain two first-party long-running services.

---

# 6.1 Management Service

Recommended implementation:

- ASP.NET Core
- .NET
- SQLite
- Web UI
- REST/internal API
- SignalR for browser updates if desired

The Management Service owns:

- Web UI
- Authentication
- User management
- Managed-folder configuration
- Storage policy configuration
- Cloud-provider setup workflows
- Backup policy configuration
- Restore workflows
- Docker application management
- Extension/subsystem marketplace
- Dashboard
- Logs and activity history
- Audit history
- Update UX
- Recovery UX

The Management Service should run as an **unprivileged process**.

It must not directly execute privileged shell commands against subsystems.

---

# 6.2 Core Orchestration Service

Recommended implementation:

- Rust

The core should be:

- Very small
- Fast
- Low-memory
- Long-running
- Conservative
- Privileged only where necessary
- Independent from the UI
- Designed to run continuously for months

The Core Service owns:

- Desired-state reconciliation
- Process supervision
- Mount lifecycle
- Share lifecycle
- Subsystem lifecycle
- Extension lifecycle
- Health checks
- Operation journaling
- Crash recovery
- Disk and mount state
- Backup execution coordination
- Cache coordination
- Tiering coordination
- Event production
- Privileged system interactions

The core should prioritize **stability and deterministic behavior over feature richness**.

---

# 7. Communication Between Services

The Management Service and Core Service should communicate locally using:

- gRPC
- Unix Domain Socket

Example:

```text
/run/filedock/core.sock
```

Benefits:

- No public TCP port required
- Low overhead
- Filesystem-based permissions
- Strongly typed APIs
- Bidirectional streaming
- Efficient event streaming

Conceptual calls:

```text
SetDesiredState()
GetSystemStatus()
GetFolderStatus()
InstallExtension()
ConfigureExtension()
StartOperation()
RestoreSnapshot()
StreamEvents()
```

The Management Service should not directly manage Samba, rclone, Kopia, Docker, NFS, or other privileged subsystems.

Correct path:

```text
Browser
   ↓
Management Service
   ↓
Core API
   ↓
Core Orchestrator
   ↓
Subsystem Adapter
   ↓
Subsystem
```

---

# 8. Event-Driven Status Model

Avoid constant UI polling.

The Core Service should publish events such as:

- MountStarted
- MountReady
- MountFailed
- ShareReloaded
- BackupStarted
- BackupProgress
- BackupCompleted
- BackupFailed
- CachePressure
- DiskPressure
- RemoteUnavailable
- SubsystemStarted
- SubsystemStopped
- SubsystemFailed
- TierMigrationStarted
- TierMigrationCompleted
- RestoreStarted
- RestoreCompleted

Recommended path:

```text
Rust Core
   ↓
gRPC event stream
   ↓
.NET Management Service
   ↓
SignalR/WebSocket
   ↓
Browser
```

---

# 9. Desired-State Architecture

The core should use a reconciliation model similar in concept to Kubernetes controllers, but without requiring Kubernetes.

Example desired state:

```text
Folder: Media

Source:
Google Drive

Mode:
Cloud Mount

Cache:
Media Optimistic

SMB:
Enabled
```

Actual state may be:

```text
Google remote: configured
Cloud mount: missing
Media cache: running
SMB share: missing
```

The core should reconcile by:

1. Starting the cloud mount
2. Creating or updating the SMB share
3. Reloading Samba if required
4. Reporting the final state

After a reboot:

```text
Core starts
    ↓
Loads desired state
    ↓
Examines actual machine state
    ↓
Reconciles differences
    ↓
Returns to healthy state
```

This should be the primary restart/recovery model.

---

# 10. Subsystem Architecture

FileDock OS must be generic enough to support new storage subsystems over time.

The Core Service should understand **capabilities**, not hardcoded product-specific behavior wherever possible.

Example capabilities:

```text
protocol.smb
protocol.nfs
protocol.webdav

cloud.remote
cloud.mount

backup.snapshot
backup.restore
backup.retention

cache.media
cache.generic

tier.storage

container.runtime
```

A managed folder asks for capabilities.

Example:

```text
Folder: Media

Required capabilities:
- cloud.mount
- cache.media
- protocol.smb
```

The orchestrator resolves:

```text
cloud.mount → rclone
cache.media → FileDock Media Cache
protocol.smb → Samba
```

Another example:

```text
Folder: AppData

Required capabilities:
- storage.local
- backup.snapshot
- protocol.nfs
```

Resolution:

```text
storage.local → native filesystem
backup.snapshot → Kopia
protocol.nfs → Linux NFS
```

---

# 11. Extension Model

Do not load third-party extensions directly into the Core Service process.

Avoid:

```text
filedock-core
  ├── plugin1.so
  ├── plugin2.so
  └── plugin3.so
```

Instead, extensions should be out-of-process or externally supervised.

This provides:

- Fault isolation
- Independent upgrades
- Language independence
- Better crash containment
- Better security boundaries
- Easier compatibility management

Supported extension runtime types for MVP:

```text
native-process
systemd-service
docker-service
```

Possible future extension transport:

- Unix socket + gRPC
- JSON-RPC over stdin/stdout for simple integrations

The core should define a protocol rather than requiring all extensions to use Rust or .NET.

---

# 12. Generic Subsystem Contract

Each subsystem should expose or be adapted to a conceptual contract:

```text
Subsystem
├── Identity
├── Version
├── Capabilities
├── DesiredState
├── CurrentState
├── Configure()
├── Start()
├── Stop()
├── Restart()
├── Reload()
├── Health()
├── Events()
└── Diagnostics()
```

Example subsystem capability declaration:

```text
Samba
- protocol.smb

rclone
- cloud.remote
- cloud.mount
- protocol.webdav

Kopia
- backup.snapshot
- backup.restore
- backup.retention

Docker
- container.runtime
```

---

# 13. Subsystem Manifest

Each subsystem should have a declarative manifest.

Example:

```yaml
id: rclone
name: Rclone

capabilities:
  - cloud.remote
  - cloud.mount
  - protocol.webdav

runtime:
  type: native-process
  executable: /usr/bin/rclone

health:
  type: process

configSchema:
  path: /usr/share/filedock/extensions/rclone/schema.json
```

Samba example:

```yaml
id: samba
name: Samba

capabilities:
  - protocol.smb

runtime:
  type: systemd-service
  unit: smbd.service
```

The exact manifest schema can evolve later.

---

# 14. Subsystem Classes

Subsystems should be divided into three classes.

## Core

Cannot be removed.

Examples:

- Debian base
- systemd
- FileDock Management Service
- FileDock Core Service
- SQLite
- Base storage metadata

## Built-in

Installed by default and managed by FileDock.

Recommended built-ins:

- Docker Engine
- Samba
- Linux NFS
- rclone
- Kopia

## Extensions

Optional and user-installable.

Examples:

- ZFS
- Btrfs tools
- Restic
- Borg
- MinIO
- Syncthing
- cd2
- iSCSI
- Tailscale
- WireGuard
- Cloudflare Tunnel
- Prometheus
- Grafana
- File Browser
- Nextcloud
- Other apps

---

# 15. Minimal Built-In Subsystems

The MVP should include the following.

| Area | Default |
|---|---|
| Base OS | Debian |
| Supervisor | systemd |
| Management Service | ASP.NET Core/.NET |
| Core Service | Rust |
| Metadata DB | SQLite |
| Local filesystem | ext4 support |
| SMB | Samba |
| NFS | Linux kernel NFS |
| WebDAV | rclone serve WebDAV |
| Cloud connectors | rclone |
| Cloud mount | rclone |
| Backup | Kopia |
| Container runtime | Docker Engine |
| Cache | FileDock Media Optimistic Cache + rclone VFS fallback |
| Monitoring | FileDock built-in health |
| Updates | FileDock appliance updater |

---

# 16. Managed Folder Model

The core user-facing abstraction should be the **Managed Folder**.

A Managed Folder is a logical storage object composed from several policies.

```text
ManagedFolder
│
├── Source
├── StorageMode
├── CachePolicy
├── TierPolicy
├── BackupPolicy
├── DurabilityClass
├── Exports
├── AccessControl
└── ApplicationBindings
```

Example:

```text
Managed Folder: Movies

Source:
Google Drive

Storage Mode:
Cloud Mount

Cache:
Media Optimistic

SMB:
Enabled

NFS:
Disabled

Docker access:
Allowed

Backup:
None
```

---

# 17. Storage Modes

The product should initially expose three primary storage modes.

---

# 17.1 Cloud Mount

Cloud is authoritative.

Local storage is used only as:

- Cache
- Metadata index
- Temporary write staging

Example:

```text
Google Drive
     ↓
Cloud connector
     ↓
FileDock cache
     ↓
/storage/media
     ↓
SMB / NFS / Docker
```

If local cache disappears, the cloud copy remains authoritative.

Typical use:

- Movies
- TV shows
- Large archives
- Cloud-backed shared data

---

# 17.2 Hybrid Backup

Local storage is authoritative.

Cloud is used as incremental protection.

Example:

```text
Local SSD
   ↓
/storage/appdata
   ↓
Kopia snapshots
   ↓
Wasabi / B2 / Azure
```

Supported intervals:

- 5 minutes
- 15 minutes
- 1 hour
- Additional configurable schedules

Typical use:

- Container application state
- Documents
- Configuration
- Local working files
- Databases with appropriate application-aware backup support

---

# 17.3 Tiered Storage

Local storage is the hot tier.

Cloud storage is the cold/capacity tier.

Example:

```text
Hot local SSD
      ↕
FileDock tier engine
      ↕
Cloud object storage
```

Policies may include:

```text
High watermark: 85%
Low watermark: 70%

Move cold files when:
- unused for 30 days
- local capacity is above high watermark

Hydrate:
- on access
```

A file may remain visible in the namespace even when most or all of its data resides in cloud storage.

Tier state may be:

- Local
- Local + Cloud
- Cloud Only
- Hydrating
- Migrating
- Error

---

# 18. Cache Architecture

The MVP should use two cache layers.

```text
Layer 1: FileDock Media Optimistic Cache
Layer 2: Generic rclone VFS Cache
```

The Media Optimistic Cache is a differentiating FileDock feature.

The VFS cache provides generic compatibility and fallback behavior.

---

# 19. Layer 1: Media Optimistic Cache

The Media Optimistic Cache is designed for write-once/read-many media workloads.

Assumption:

> If the same remote object still exists, FileDock may optimistically assume media contents have not changed until background reconciliation indicates otherwise.

This is appropriate for files such as:

- MKV
- MP4
- M4V
- ISO
- TS
- Other large media files

Media libraries usually replace or delete media rather than modifying arbitrary bytes in place.

---

# 20. Media Cache Goals

The Media Optimistic Cache should reduce cloud requests generated by:

- Emby library scans
- Infuse library scans
- Media browsing
- Container probing
- Codec and stream inspection
- Repeated opening of media files
- Directory enumeration

A routine library scan should ideally be served almost entirely from local metadata and cached ranges.

---

# 21. Media Cache Components

The media cache should store three categories.

## 21.1 Directory metadata

Examples:

- Name
- Path
- Type
- Remote file ID
- Size
- Last observed timestamp
- Parent identity

## 21.2 File/media metadata

Examples:

- Container
- Duration
- Resolution
- Video codec
- Audio tracks
- Subtitle tracks
- File size
- Known remote identity

## 21.3 Selected byte ranges

Examples:

- Head blocks
- Tail blocks
- Previously requested playback ranges
- Prefetched ranges

---

# 22. Optimistic Consistency

Do not perform a cloud stat/head operation on every open.

Instead:

```text
Application opens media file
        ↓
FileDock finds it in local media index
        ↓
Serve cached metadata/ranges immediately
```

Remote reconciliation should occur asynchronously.

Suggested user modes:

## Aggressive

- Trust local media index
- Reconcile approximately every 24 hours

## Balanced

- Trust local media index
- Reconcile approximately every 1 hour
- Recommended default

## Fresh

- Reconcile every 5–15 minutes

Folder access should not block on reconciliation unless the requested content is unavailable locally.

---

# 23. Remote Identity

Do not use path alone as the cache identity.

Preferred key:

```text
provider
account
remote_object_id
size
block_number
```

Example:

```text
google:account1:1AbcXYZ:84235928129:block0
```

Benefits:

- Renames do not invalidate useful cache
- Moved files may preserve cache
- Stable remote IDs provide stronger identity than paths

If the remote object ID changes or the file size changes, the media cache should treat the object as replaced and invalidate stale ranges.

---

# 24. Block Cache Design

Use fixed-size blocks.

Suggested initial block size:

- 4 MiB

Example:

```text
80 GB movie

Block 0
Block 1
Block 2
...
```

The cache metadata tracks which blocks exist locally.

The physical implementation does not need to create one file per block if that creates filesystem overhead.

---

# 25. Metadata Prefetch

Do not use head/tail percentages.

A 5% head + 5% tail policy on an 80 GB file would consume approximately 8 GB just for metadata-related caching.

Use fixed/adaptive byte ranges instead.

Suggested initial media profile:

```text
Head: 32 MiB
Tail: 16 MiB
```

Possible user presets:

```text
Generic
- no prefetch

Media
- head 32 MiB
- tail 16 MiB

Aggressive Media
- head 64 MiB
- tail 32 MiB
```

Later, FileDock may make this media-format-aware.

---

# 26. Future Media-Aware Parsing

Future versions may understand container structures.

Examples:

## MKV

Often useful metadata can be found near the beginning.

## MP4/M4V

Important atoms such as `moov` may appear near the beginning or end.

## ISO

Requires different access patterns.

## TS

Requires another strategy.

A future cache engine can choose ranges intelligently rather than applying one static rule.

This is not required for the first MVP.

---

# 27. Read Path

Conceptual read path:

```text
Application
    ↓
FileDock StorageFS
    ↓
Media Optimistic Cache
    ↓ miss
rclone VFS Cache
    ↓ miss
Cloud
```

Example:

```text
READ movie.mkv
offset = 4 GB
length = 8 MB
```

The media cache checks the required blocks.

If present:

- Serve locally.

If missing:

1. Ask the VFS fallback.
2. If VFS misses, fetch from cloud.
3. Return data.
4. Optionally promote the resulting blocks into the persistent media cache.

---

# 28. Read-Ahead

Sequential reads should trigger read-ahead.

MVP recommendation:

- 128 MiB read-ahead window

Example:

```text
Client reads:
block 100
block 101
block 102

FileDock may begin prefetching:
103
104
105
...
```

If a media player seeks to another location:

- Cancel obsolete prefetch where practical.
- Start prefetch near the new offset.

Advanced adaptive read-ahead can be added later.

---

# 29. Cache Eviction

The cache should use configurable capacity limits.

Example:

```text
Cache capacity: 500 GB
High watermark: 90%
Low watermark: 80%
```

When the high watermark is reached:

- Evict least-recently-used disposable blocks until the low watermark is reached.

Suggested eviction priority:

1. Generic VFS old blocks
2. Old playback blocks
3. Media prefetch blocks
4. Media metadata/head-tail ranges
5. Pinned data should not be evicted

---

# 30. Pinning

Users should be able to mark content as:

```text
Keep Locally
```

Pinned content:

- Should fully cache locally
- Should not be evicted
- May still remain cloud-backed

This can later be used by tiered storage as well.

---

# 31. Layer 2: Generic VFS Cache

For unsupported file types and generic filesystem behavior, FileDock should retain rclone's VFS behavior as a fallback for MVP.

Use cases:

- Documents
- ZIP archives
- Installers
- Generic files
- Unknown file formats
- Files requiring ordinary VFS behavior
- Media cache misses

The custom FileDock Media Cache should not attempt to replace all VFS semantics in the first version.

---

# 32. Write Path

Read cache and write staging must be separate concepts.

## Read cache

- Disposable
- Rebuildable
- Safe to delete

## Write staging

- Not disposable until remote commit succeeds
- Must survive restart
- Must be journaled

Example:

```text
Application writes file
        ↓
Local write staging
        ↓
File close/commit
        ↓
Upload remote
        ↓
Verify remote
        ↓
Mark remote durable
        ↓
Convert local staging copy into cache or evict later
```

Do not claim write data is durable until remote commit is verified.

---

# 33. Random Writes

For MVP, arbitrary random modification of existing large cloud files should be limited or unsupported.

Reasons:

- Many cloud providers do not support arbitrary byte-range modification.
- Full compatibility quickly becomes equivalent to rebuilding a mature VFS engine.

MVP priorities:

- Read
- Create/upload
- Delete
- Rename
- Staged complete-file update

Full random write semantics can be considered later.

---

# 34. Background Reconciliation

FileDock should maintain a local view of remote folders.

Reconciliation should:

1. List remote directory changes
2. Compare stable remote identity where available
3. Detect additions
4. Detect deletions
5. Detect replacements
6. Update local metadata/index
7. Invalidate stale cached blocks when necessary

Out-of-band cloud changes may not appear immediately.

Visibility delay depends on the configured reconciliation interval.

Future versions may use provider-specific change feeds/webhooks.

---

# 35. Docker Integration

Docker should be included by default.

Docker is a **consumer of FileDock-managed storage**, not the storage owner.

Example:

```text
Cloud / Local / Tiered storage
            ↓
FileDock Managed Folder
            ↓
/storage/media
            ↓
Docker bind mount
            ↓
Emby
```

Recommended container mount style:

```yaml
volumes:
  - /storage/media:/media
  - /storage/appdata/emby:/config
```

Avoid opaque named Docker volumes for important data when possible.

---

# 36. Docker UI Structure

Suggested FileDock UI separation:

```text
Storage
  - Folders
  - Cloud
  - Backup
  - Tiering
  - Shares
  - Cache

Applications
  - Containers
  - Images
  - Networks
  - Compose
  - App templates/catalog
```

Application hosting should remain secondary to storage management.

FileDock should not become a general-purpose homelab OS.

---

# 37. Recommended Folder Categories

Example:

```text
/storage/media
/storage/documents
/storage/archive
/storage/appdata
```

Application state should normally live under:

```text
/storage/appdata/<application>
```

Examples:

```text
/storage/appdata/emby
/storage/appdata/homeassistant
/storage/appdata/immich
```

Each can receive its own backup policy.

---

# 38. Docker Data Durability

Do not back up:

- Docker Engine
- Docker images
- Image layers
- Build cache
- Re-creatable containers

Do back up:

- Compose definitions
- Environment configuration
- Application secrets
- Application persistent data
- App-specific configuration
- Required user-created configuration

Recovery model:

```text
Pull image
    +
Restore compose/config
    +
Restore /storage/appdata/<app>
    =
Application rebuilt
```

---

# 39. Backup Architecture

Kopia is the recommended built-in backup engine for MVP.

Desired capabilities:

- Incremental snapshots
- Deduplication
- Encryption
- Compression
- Retention
- Restore
- S3-compatible storage
- Backblaze B2
- Azure
- Google-compatible destinations where applicable
- Filesystem targets
- WebDAV/SFTP where supported

FileDock owns the schedule and UX.

Kopia remains the underlying backup engine.

---

# 40. Backup Intervals

User-facing options should include at minimum:

- Every 5 minutes
- Every 15 minutes
- Every hour
- Daily
- Custom schedule

The UI should clearly distinguish:

- Scheduled frequency
- Last successful durable recovery point
- Pending local changes
- Backup failure state

Example:

```text
Protection Status

Latest durable recovery point:
09:05:12

Pending local changes:
127 MB

Target RPO:
5 minutes
```

---

# 41. Local Change Journal

Do not rescan every protected folder every five minutes if avoidable.

Maintain a change journal or equivalent state.

Example:

```text
09:00:01 modify A
09:00:08 create B
09:00:30 modify C
09:01:14 delete D
```

At the next protection cycle:

```text
journal
   ↓
incremental backup
   ↓
cloud repository
   ↓
checkpoint
```

After successful upload, advance the durable checkpoint.

---

# 42. Database Data

Database directories require special treatment.

A raw filesystem copy of a running database may not provide a logically consistent backup.

FileDock should support two conceptual levels.

## Basic

Crash-consistent filesystem backup.

## Application-aware

Native database backup integration.

Future examples:

### PostgreSQL

- pg_dump
- pg_basebackup
- WAL archiving

### MySQL

- mysqldump
- physical backup mechanisms

### MongoDB

- native dump/backup workflow

MVP may initially support filesystem-level protection with clear warnings.

Application-aware database backup should be part of a later extension system.

---

# 43. Data Classification

Every FileDock-managed piece of data should belong to one of three categories.

## Authoritative

Must be protected.

Examples:

- Managed folder configuration
- Cloud credentials
- Share definitions
- Backup policies
- Tier metadata
- Docker Compose
- Protected local data
- Application persistent data

## Rebuildable

Does not need backup.

Examples:

- Debian packages
- FileDock binaries
- Docker Engine
- Samba executable
- rclone executable
- Kopia executable
- Docker images

## Disposable

Can be deleted at any time.

Examples:

- Cloud read cache
- Media prefetch blocks
- Thumbnails
- Temporary downloads
- Transcoding data
- Regeneratable indexes
- Nonessential logs

---

# 44. Appliance State

The appliance should maintain a small authoritative system-state bundle containing:

```text
Managed folders
Storage policies
Cloud remotes
Share definitions
Users / ACL configuration
Backup policies
Tier policies
Docker Compose definitions
Application bindings
Extension registry
Critical tier metadata
Configuration metadata
```

This state should be small compared with user data.

---

# 45. Core State Database

The Core Service should maintain a small SQLite database.

Example:

```text
/var/lib/filedock-core/core.db
```

It should contain:

- Desired state
- Actual state cache
- Operation journal
- Extension registry
- Leases
- Critical in-progress transactions
- Tier transition state
- Mount state

It should not become a general application database.

---

# 46. Management Database

The Management Service may maintain a separate SQLite database.

Example:

```text
/var/lib/filedock-management/app.db
```

The two services should **not write the same SQLite database**.

They should communicate through APIs/events.

---

# 47. Critical Operation State Machines

Storage operations must be modeled as durable state machines.

Example tier migration:

```text
LOCAL
  ↓
UPLOADING
  ↓
VERIFYING
  ↓
REMOTE_DURABLE
  ↓
EVICTING
  ↓
CLOUD_ONLY
```

If the core crashes during VERIFYING:

```text
Restart
   ↓
Load operation journal
   ↓
Resume VERIFYING
```

All critical operations should be idempotent and resumable.

---

# 48. Configuration Backup

The appliance configuration is small and should be protected more frequently than large user data.

Whenever configuration changes:

```text
User changes configuration
        ↓
Transaction committed
        ↓
Configuration snapshot
        ↓
Encrypted remote upload
```

Also run periodic checkpoints such as every 5 minutes.

Configuration RPO should therefore normally be significantly better than 5 minutes.

---

# 49. Secrets and Recovery Key

Secrets may include:

- Google OAuth tokens
- S3 credentials
- Azure credentials
- SMB passwords
- Backup repository keys
- API tokens
- Application environment secrets

These must never be uploaded in plaintext.

FileDock should maintain encrypted secrets.

Example:

```text
/var/lib/filedock/secrets.enc
```

Disaster recovery must not depend only on TPM-bound encryption because the original machine may be destroyed.

Users should receive or create a portable recovery key.

Conceptual form:

```text
XXXX-XXXX-XXXX-XXXX-XXXX
```

Recovery:

```text
New FileDock installation
        ↓
Enter recovery key
        ↓
Retrieve encrypted appliance state
        ↓
Decrypt credentials
        ↓
Reconnect cloud services
```

---

# 50. Durability Classes for Managed Folders

Every managed folder should declare its durability class.

Suggested UI:

```text
Durability

- Disposable
- Cloud Primary
- Backed Up
- Tiered
```

## Disposable

Examples:

- Cache
- Temp
- Transcode

No backup.

## Cloud Primary

Cloud is authoritative.

Local copies are cache only.

## Backed Up

Local is authoritative.

Remote incremental snapshots protect it.

## Tiered

Data placement may be local or cloud.

Tier metadata is authoritative and must be durable.

---

# 51. Tier Metadata

For tiered storage, FileDock must know:

```text
File identity
Logical path
File size
Checksum if available
Tier state
Cloud object ID
Remote repository
Local ranges
Migration state
Hydration state
```

Tier metadata is authoritative and must be protected frequently.

---

# 52. Disaster Recovery

Disaster recovery must be designed before MVP release.

Acceptance test:

```text
Original appliance destroyed
        ↓
Create blank VM or machine
        ↓
Install FileDock OS
        ↓
Select Restore Appliance
        ↓
Choose backup provider
        ↓
Enter recovery key
        ↓
Find appliance backup
        ↓
Restore
```

FileDock should reconstruct:

- Users
- Shares
- Cloud remotes
- Managed folders
- Backup policies
- Tier policies
- Docker application definitions
- Permissions
- Schedules
- Extension state

Then:

- Cloud mounts reconnect
- Caches start empty
- Docker images are re-downloaded
- Protected app data is restored
- Tier metadata is restored
- Services reconcile to desired state

---

# 53. Protocol Exports

Initial supported export protocols:

## SMB

- Samba
- Installed by default
- Primary Windows/media sharing protocol

## NFS

- Linux kernel NFS
- Installed by default
- Useful for Linux, Proxmox, containers, and infrastructure

## WebDAV

- Initially provided through rclone serve WebDAV
- Avoid adding another dedicated server for MVP

Future protocols may include:

- SFTP
- FTP/FTPS
- iSCSI
- S3 gateway

---

# 54. Cloud Providers

rclone should provide the initial cloud abstraction.

Potential providers include:

- Google Drive
- OneDrive
- Dropbox
- S3
- Wasabi
- Backblaze B2
- Azure Blob
- Google Cloud Storage
- WebDAV
- SFTP
- Box
- Other rclone-supported remotes

FileDock should avoid reimplementing OAuth and provider APIs in MVP.

---

# 55. Local Filesystems

MVP default:

- ext4

Initial goals:

- Existing local block devices
- Existing mounted filesystems
- Local SSD/HDD
- USB storage
- VM disks

Optional future extensions:

- XFS
- Btrfs
- ZFS
- mdraid
- LVM

ZFS should not be mandatory in the base product.

---

# 56. Docker Application Model

Docker is installed by default.

Users may:

1. Deploy FileDock OS in bare metal or a VM.
2. Create managed folders.
3. Install Docker applications.
4. Bind FileDock-managed folders into containers.

Example:

```text
Managed Folder: Media
/storage/media

Managed Folder: Emby AppData
/storage/appdata/emby
```

Docker:

```text
Emby container
  /media  → /storage/media
  /config → /storage/appdata/emby
```

This allows FileDock to apply different durability policies:

```text
/storage/media
Cloud Primary
Media cache
No local backup required

/storage/appdata/emby
Local Primary
Backup every 15 minutes
```

---

# 57. Built-In UI for MVP

The first UI does not need to fully replace every subsystem UI.

MVP can provide:

```text
Dashboard
- Service health
- Managed folders
- Storage health
- Cache usage
- Backup status
- Docker status
- Extension status
```

Subsystem management may initially include links to subsystem-specific UI/configuration where necessary.

However, FileDock should progressively absorb common workflows.

Migration order:

1. Managed folders
2. SMB/NFS export
3. Cloud remote creation
4. Cloud mount
5. Cache
6. Backup schedule
7. Restore
8. Tiered storage
9. Advanced protocol settings
10. Docker applications

The end goal is that subsystem UIs become advanced/debug interfaces rather than the normal user path.

---

# 58. Folder Configuration Example

```text
Name:
Movies

Source:
Google Drive

Mode:
Cloud Mount

Cache:
Media Optimistic

Cache capacity:
500 GB

Consistency:
Balanced

Metadata prefetch:
Automatic

SMB:
Enabled

NFS:
Disabled

WebDAV:
Disabled

Docker:
Allowed

Backup:
None
```

Another example:

```text
Name:
AppData

Source:
Local SSD

Mode:
Hybrid Backup

Backup target:
Wasabi

Backup interval:
5 minutes

Retention:
30 days

SMB:
Disabled

NFS:
Optional

Docker:
Allowed
```

---

# 59. Tiered Storage Example

```text
Name:
Archive

Mode:
Tiered Storage

Local capacity:
500 GB

Cloud target:
Backblaze B2

High watermark:
85%

Low watermark:
70%

Cold threshold:
30 days without access

Hydration:
On access

SMB:
Enabled
```

---

# 60. Suggested Filesystem Layout

Example internal layout:

```text
/etc/filedock/
  core/
  management/
  extensions/
  samba/
  rclone/
  backup/

/var/lib/filedock/
  state/
  extensions/
  operation-journal/

/var/lib/filedock-core/
  core.db

/var/lib/filedock-management/
  app.db

/var/cache/filedock/
  media/
  vfs/
  temporary/

/storage/
  media/
  documents/
  archive/
  appdata/

/mnt/filedock/
  cloud/
  tier/
  internal/
```

Exact layout may change, but FileDock should own predictable paths from day one.

---

# 61. Security Boundary

The browser-facing Management Service should not run as root.

Preferred design:

```text
Browser
   ↓
Unprivileged Management Service
   ↓
Authenticated Unix socket
   ↓
Privileged Core Service
   ↓
Storage operations
```

Extension permissions should be restricted to only what each subsystem needs.

---

# 62. Reliability Requirements

The following should be considered baseline reliability requirements.

- Existing shares should remain available during Management Service restart.
- Core restart must reconcile state automatically.
- Backup operations must be resumable/idempotent.
- Tier migrations must survive interruption.
- Cache corruption must not cause authoritative data loss.
- Failed cloud requests must not corrupt local state.
- Partial uploads must not be considered durable.
- Remote durability must be verified before local eviction.
- Configuration updates must be transactional.
- Failed subsystem upgrades must support rollback or safe failure.

---

# 63. Failure Injection Testing

FileDock should deliberately test failures such as:

- Kill process during cloud upload
- Kill process during backup
- Kill process during tier migration
- Reboot during mount
- Disconnect network
- Fill cache disk
- Fill primary local disk
- Expire OAuth token
- Return remote 500 errors
- Lose DNS
- Corrupt cache metadata
- Delete cache files
- Stop Samba
- Stop NFS
- Stop rclone
- Stop Kopia
- Restart Docker
- Restart core
- Restart management service

Expected outcome:

- No authoritative data corruption
- Safe retry or degraded state
- Clear health reporting
- Automatic reconciliation where possible

---

# 64. Update Strategy

Do not automatically track latest versions of all dependencies.

FileDock releases should pin validated combinations of:

- Samba
- NFS userland components
- rclone
- Kopia
- Docker
- FileDock Core
- FileDock Management
- Other built-in dependencies

Each FileDock release should represent a known-good tested bundle.

---

# 65. MVP Scope

FileDock OS v0.1 should aim to deliver:

## Core

- Debian-based appliance
- systemd
- Rust Core Service
- .NET Management Service
- SQLite
- Desired-state reconciliation
- Operation journal
- Event stream
- Extension registry

## Local Storage

- ext4
- Local disk/mount discovery
- Managed folders

## Sharing

- SMB
- NFS
- WebDAV

## Cloud

- rclone remote configuration
- Cloud mount
- Google Drive
- OneDrive
- S3-compatible storage
- Other rclone-supported providers where feasible

## Cache

- FileDock Media Optimistic Cache
- Persistent media metadata/index
- Fixed-size block cache
- Head/tail byte-range prefetch
- Read-ahead
- LRU eviction
- Pin locally
- rclone VFS fallback
- Local write staging

## Backup

- Kopia
- 5-minute/15-minute/1-hour schedules
- Incremental snapshots
- Retention
- Restore
- Protection status

## Docker

- Docker Engine installed by default
- Container listing
- Compose support
- Managed-folder bind mounts
- Basic app lifecycle

## Recovery

- Appliance-state backup
- Encrypted secrets
- Recovery key
- Fresh-install restoration workflow

## Extensions

- Native process runtime
- systemd service runtime
- Docker service runtime
- Declarative manifests
- Capability-based registration

---

# 66. Explicit MVP Non-Goals

Do not delay MVP for:

- Full custom POSIX filesystem semantics
- Complete replacement of rclone VFS
- Random in-place modification of remote large files
- ZFS management
- RAID management
- Kubernetes
- Ceph
- Clustered storage
- Multi-node HA
- Multi-node distributed cache
- Full S3 server
- Enterprise identity integration
- Every database-aware backup type
- Sophisticated AI cache prediction
- Custom Linux kernel
- Fully custom distribution
- Full subsystem UI replacement

---

# 67. Future Roadmap

Potential future capabilities:

## Storage

- ZFS
- Btrfs
- RAID management
- iSCSI
- S3 gateway
- Object storage

## Cache

- Media-format-aware parsing
- MKV-aware range selection
- MP4 atom discovery
- Adaptive read-ahead
- Sparse local files
- Shared cache
- Cache deduplication
- Predictive prefetch

## Tiering

- Multi-tier local storage
- NVMe → HDD → Cloud
- Access-frequency policies
- User-defined tier policies
- Intelligent hydration

## Backup

- PostgreSQL-aware backup
- MySQL-aware backup
- MongoDB-aware backup
- WAL/PITR workflows
- Remote replication
- Backup verification

## Extensions

- Tailscale
- WireGuard
- Cloudflare Tunnel
- MinIO
- Syncthing
- Restic
- Borg
- Monitoring packages
- App catalog

## Management

- Unified configuration for all built-in subsystems
- Extension marketplace
- Policy templates
- Remote appliance management
- Fleet management

---

# 68. Performance Philosophy

The Core Service should be designed to be:

- Fast
- Low latency
- Low memory
- Event-driven
- Mostly asynchronous
- Stable under long runtimes
- Minimal in responsibility

Raw orchestration throughput is less important than:

- Correctness
- Predictability
- Recovery
- Low overhead
- Fast reconciliation
- No unnecessary polling
- Efficient cloud request reduction

The Media Optimistic Cache is expected to provide one of the largest practical performance benefits by avoiding repeated cloud metadata and range requests during media-library scans.

---

# 69. Recommended Technology Split

| Component | Technology |
|---|---|
| Management Web UI/API | ASP.NET Core/.NET |
| Management DB | SQLite |
| Browser updates | SignalR/WebSocket |
| Core orchestrator | Rust |
| Core state | SQLite |
| Management/Core communication | gRPC over Unix Domain Socket |
| Supervisor | systemd |
| SMB | Samba |
| NFS | Linux kernel NFS |
| Cloud providers | rclone |
| Generic VFS fallback | rclone VFS |
| Backup | Kopia |
| Containers | Docker Engine |
| Media cache | FileDock Rust component |
| Local filesystem | ext4 initially |

---

# 70. Architectural Summary

```text
┌─────────────────────────────────────────────┐
│          FileDock Management (.NET)         │
│                                             │
│ Web UI                                      │
│ Auth                                        │
│ Folder policies                             │
│ Docker management                           │
│ Backup/restore UX                           │
│ Extension marketplace                       │
└───────────────────┬─────────────────────────┘
                    │
               gRPC / UDS
                    │
┌───────────────────▼─────────────────────────┐
│             FileDock Core (Rust)            │
│                                             │
│ Desired-state engine                        │
│ Reconciler                                  │
│ Process supervision                         │
│ Operation journal                           │
│ Health                                      │
│ Events                                      │
│ Extension manager                           │
│ Cache/tier coordination                     │
└───────┬──────────┬──────────┬───────────────┘
        │          │          │
        ▼          ▼          ▼
     Samba       rclone      Kopia
        │          │          │
        │          │          └── Cloud Backup
        │          │
        │          ├── Google Drive
        │          ├── OneDrive
        │          ├── S3 / Wasabi
        │          ├── B2
        │          └── Azure
        │
        ▼
       SMB

               ┌────────────────────┐
               │ FileDock Media     │
               │ Optimistic Cache   │
               │                    │
               │ Metadata index     │
               │ Sparse blocks      │
               │ Prefetch           │
               │ Read-ahead         │
               └────────────────────┘

┌─────────────────────────────────────────────┐
│                 Docker                      │
│                                             │
│ Emby / Immich / Home Assistant / Apps       │
│              ↓                              │
│       FileDock Managed Folders              │
└─────────────────────────────────────────────┘
```

---

# 71. Product Rule of Thumb

Every feature should answer these questions:

1. **Where is the authoritative copy?**
2. **Can local state be deleted safely?**
3. **What is the recovery point objective?**
4. **How is this state rebuilt?**
5. **Which subsystem provides the capability?**
6. **What happens if that subsystem crashes?**
7. **Can the core reconcile it automatically?**
8. **Can the user restore the entire appliance on new hardware?**

If those questions have clear answers, the feature fits the FileDock architecture.

---

# 72. Final Product Definition

FileDock OS is:

> **A lightweight, extensible, software-defined storage appliance OS that combines local storage, cloud-backed storage, intelligent media caching, tiered capacity, frequent incremental protection, open file-sharing protocols, and Docker application hosting.**

Its defining design goals are:

- **Less NAS**
- **Smaller local storage requirements**
- **Cloud-scale capacity**
- **Local-speed access where it matters**
- **Media-optimized cloud caching**
- **Recovery points as frequent as five minutes**
- **Everything durable or rebuildable**
- **Docker applications consuming managed storage**
- **A small, stable orchestration core**
- **A rich but non-critical management UI**
- **Extensible subsystem support**
- **No unnecessary data lock-in**

The long-term goal is not merely to create another NAS interface.

The goal is to make the physical NAS hardware increasingly replaceable by combining a small amount of local storage with inexpensive cloud capacity, intelligent caching, durable configuration, and reliable recovery.
