# Feature Specification: Cloud Mount with Two-Level Cache

**Feature Branch**: `004-cloud-mount-cache`

**Created**: 2026-10-11

**Status**: Draft

**Input**: User description: "Based on the FileDock-OS.md, create the 04 requirement for the Cloud
Mount with two level caches"

## Overview

In Cloud Mount, a cloud location is authoritative and appears as a local managed folder that can be
shared over SMB, NFS, and WebDAV or bound into containers. Local storage is used only as cache,
metadata index, and temporary write staging.

Two cache levels make access fast:

- **Level 1, the Media Optimistic Cache**: A persistent cache for write-once, read-many media. It
  stores directory metadata, media metadata, and selected byte ranges. It trusts its local index and
  reconciles with the cloud in the background, so library scans and repeated opens rarely reach the
  cloud.
- **Level 2, the Generic Cache**: A general-purpose fallback for other file types and for Level 1
  misses.

The read cache is always disposable. Write staging is durable until the upload is verified. This
feature builds on feature 001.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Mount a cloud folder and use it like local storage (Priority: P1)

An administrator creates a managed folder, for example `Movies`, in Cloud Mount mode. They choose a
cloud connection, such as Google Drive, a remote path, a cache profile, and a cache capacity, such as
500 GB. They enable SMB and allow container access. Clients and containers then browse and read the
cloud content through an ordinary local path.

**Why this priority**: This is the basic value of the mode. Both cache levels serve this path.

**Independent Test**: Mount a cloud folder, export it over SMB, bind it into a container, and confirm
both can list and read files that exist in the cloud.

**Acceptance Scenarios**:

1. **Given** a registered cloud connection, **When** the administrator creates a Cloud Mount folder
   with a remote path, cache profile, and capacity, **Then** the folder is created in Cloud Mount mode
   and becomes Ready. No data importance or backup settings are asked for.
2. **Given** a Ready Cloud Mount folder, **When** a client lists and reads files over SMB or a
   container reads them through a bind, **Then** contents match the cloud.
3. **Given** a reboot or Core Service restart, **When** the appliance comes back, **Then** the mount is
   restored by reconciliation and keeps its cache.
4. **Given** the cloud connection fails at mount time, **When** the folder starts, **Then** it shows
   Degraded with the reason and retries automatically.

---

### User Story 2 - Fast media library scans from the Level 1 cache (Priority: P1)

A media server inside a container, or a client media app such as Emby or Infuse, scans a large cloud
media library. The first scan fills Level 1 with directory metadata, media metadata such as
container, duration, resolution, codecs, and tracks, and fixed head and tail byte ranges. Later scans
and repeated opens are served from Level 1 without contacting the cloud. Remote changes are picked up
by background reconciliation at the chosen consistency level.

**Why this priority**: The product guide names this as FileDock's largest practical performance
advantage. It is also how the constitution's "Fast" principle is met for cloud media.

**Independent Test**: Scan a library of several thousand media files twice. Count cloud requests and
measure time for each scan, and confirm the second scan is served almost entirely locally.

**Acceptance Scenarios**:

1. **Given** a folder using the Media cache profile, **When** a media file is first opened, **Then** its
   head and tail byte ranges and media metadata are kept in Level 1.
2. **Given** a media file already in the Level 1 index whose cloud identity is unchanged, **When** it
   is opened or probed again, **Then** metadata and cached ranges are served without a cloud request.
3. **Given** the Balanced consistency level, **When** a file is added, deleted, or replaced in the
   cloud outside FileDock, **Then** the change becomes visible within about 1 hour.
4. **Given** an open or listing request, **When** background reconciliation is due, **Then** the
   request is not delayed by reconciliation unless the requested content is not available locally.
5. **Given** a file renamed or moved in the cloud, **When** reconciliation runs, **Then** cached data
   follows the file and is not discarded.
6. **Given** a file replaced in the cloud with different content or size, **When** reconciliation
   detects it, **Then** stale cached ranges for the old version are discarded.

---

### User Story 3 - Smooth playback and seeking (Priority: P1)

A user streams a large movie, tens of gigabytes, from the cloud mount. Sequential reads trigger
read-ahead, so playback stays ahead of the player. When the user seeks, outdated prefetching stops
and new prefetching starts near the new position. Played ranges are kept in Level 1, so replaying
or seeking back is local.

**Why this priority**: Playback is the main reason to read media from the mount, and it must work
from the first release.

**Independent Test**: Play a large remote video, seek several times, then replay. Confirm continuous
playback, quick resumption after seeks, and local service on replay.

**Acceptance Scenarios**:

1. **Given** sequential reading of a file, **When** reads continue, **Then** the appliance prefetches
   ahead of the read position, up to the read-ahead window.
2. **Given** a seek to a new position, **When** reads start there, **Then** prefetching for the old
   position is cancelled where possible and starts near the new one.
3. **Given** a range already read, **When** it is read again, **Then** it is served locally.

---

### User Story 4 - Generic files through the Level 2 cache (Priority: P2)

Documents, archives, installers, and unknown file types, along with Level 1 misses, are handled by
the Level 2 generic cache. Reads check Level 1 first, then Level 2, then the cloud. Blocks fetched for
media files can be promoted into Level 1.

**Why this priority**: Cloud Mount folders hold more than media, but media is the differentiating
case.

**Independent Test**: Read documents and archives from a cloud mount, then read them again. Confirm
correct content and that the repeated reads are served locally from Level 2.

**Acceptance Scenarios**:

1. **Given** a non-media file, **When** it is read, **Then** it is served through Level 2 and repeated
   reads within cache capacity are local.
2. **Given** a media file range not present in Level 1, **When** it is read, **Then** the appliance
   asks Level 2, then the cloud, returns the data, and keeps the fetched blocks in Level 1 when the
   profile allows.
3. **Given** the Generic cache profile, **When** a folder is used, **Then** no head or tail prefetching
   is done and only Level 2 behavior applies.

---

### User Story 5 - Safe writes to the cloud mount (Priority: P2)

Clients and containers can create, replace, rename, and delete files in a Cloud Mount folder. New and
replaced content is written to local write staging, which survives restarts. After the file is closed
it is uploaded and verified. Only then is it reported as durable in the cloud. The administrator can
see pending uploads.

**Why this priority**: Applications such as media managers need to write into the mount. Writes must
never be reported as safe before they are.

**Independent Test**: Copy files into a cloud mount, cut power before uploads finish, and restart.
Confirm every closed file uploads and is verified, and that pending status was shown.

**Acceptance Scenarios**:

1. **Given** a client creates a file and closes it, **When** staging completes, **Then** the file is
   immediately readable at the mount and is shown as pending upload.
2. **Given** a pending upload, **When** upload and verification succeed, **Then** the file is marked
   durable in the cloud and its staged copy becomes ordinary disposable cache.
3. **Given** a power loss or crash with pending uploads, **When** the appliance restarts, **Then** every
   closed staged file is uploaded and verified, and none are lost.
4. **Given** an application tries to change bytes in the middle of an existing large cloud file,
   **When** the write is attempted, **Then** it is either staged as a complete-file replacement or
   rejected with a clear error. It never partially corrupts the remote file.
5. **Given** renames and deletes, **When** they are issued, **Then** they are applied to the cloud and
   to the local index.

---

### User Story 6 - Cache capacity, eviction, pinning, and status (Priority: P2)

The administrator sets each folder's cache capacity and watermarks, which default to 90% and 80%.
Least recently used data is evicted in a defined priority order. The administrator can pin content as
Keep Locally so it is fully cached and never evicted. A cache view shows usage by level and category,
hit rates, pending uploads, and pinned size. The administrator can clear the read cache at any time
without losing data.

**Why this priority**: Predictable use of a small local disk is a constitution requirement, but
default policies are enough for the first scans to work.

**Independent Test**: Fill a small cache past its high watermark and confirm eviction order and that
pinned data remains. Clear the cache and confirm all files are still readable.

**Acceptance Scenarios**:

1. **Given** cache usage passes the high watermark, **When** eviction runs, **Then** data is removed in
   this order until usage reaches the low watermark:
   1. old generic blocks;
   2. old playback ranges;
   3. media prefetch ranges;
   4. media metadata and head/tail ranges.
2. **Given** pinned content, **When** eviction runs, **Then** pinned content is never removed.
3. **Given** the administrator clears the read cache, **When** clearing finishes, **Then** all files are
   still listed and readable from the cloud and pending uploads are untouched.
4. **Given** a folder, **When** the administrator opens its cache view, **Then** usage by level and
   category, hit rate, pinned size, and pending uploads are shown.

---

### User Story 7 - Choose the consistency level (Priority: P3)

The administrator chooses how strongly Level 1 trusts its index:

- **Aggressive**: reconcile about every 24 hours.
- **Balanced**: reconcile about every hour. This is the default.
- **Fresh**: reconcile every 5–15 minutes.

The administrator can also request an immediate refresh of a folder or subfolder.

**Why this priority**: The default suits most libraries. Tuning is a refinement.

**Independent Test**: Set each level, make an outside change in the cloud, and confirm the change
appears within each level's interval and immediately after a manual refresh.

**Acceptance Scenarios**:

1. **Given** a consistency level, **When** a remote change occurs, **Then** it appears within that
   level's reconciliation interval.
2. **Given** any level, **When** the administrator requests a refresh, **Then** the selected path is
   reconciled immediately.

---

### Edge Cases

- The cloud is unreachable. Cached metadata and ranges are still served. Reads of uncached data fail
  within 30 seconds with an I/O error. Writes keep staging until staging is full.
- Write staging fills up. New writes fail with an out-of-space error. Staged data is never evicted
  before it is uploaded and verified.
- The cache disk fills up because of other data. Read cache shrinks first, and staging is protected.
- Cache metadata is corrupt or deleted. The affected cache is discarded and rebuilt from the cloud
  with no data loss.
- The provider limits request rates. Requests back off and retry, foreground reads take priority
  over prefetch and reconciliation, and the folder reports throttling.
- The cloud connection's credentials expire. The folder becomes Degraded with a credential alert, and
  cached reads continue.
- A file is replaced in the cloud during playback. The current session finishes on the version it
  started with where possible. New opens see the new version after reconciliation.
- The same or an overlapping remote path is mounted by two Cloud Mount folders. Each folder works
  correctly, and cached data for the same remote file is not stored twice. A remote path used as
  a Tiered Storage cloud reference path cannot also be mounted (feature 001).
- A file is renamed while its upload is pending. The upload completes under the final name.
- A media scanner opens thousands of files rapidly. Requests are served from Level 1 and cloud
  requests stay bounded by the backoff and concurrency limits.
- A file is larger than the whole cache. Playback still works by streaming, and eviction keeps
  working.

## Requirements *(mandatory)*

### Functional Requirements

**Mount configuration and lifecycle**

- **FR-001**: Cloud Mount MUST be available as a storage mode in the Create Managed Folder flow
  (feature 001). The cloud is authoritative.
  - Source / Backend asks for a cloud connection and remote reference path. Every subfolder maps to
    the same relative remote path and uses the folder's Cloud Mount settings. Subfolders have no
    settings of their own.
  - Advanced shows Cache (profile, capacity, watermarks) and Consistency.
  - Backup settings MUST NOT be offered, because the cloud copy is already authoritative.
  - Data Importance MUST NOT be asked for or stored for this mode.
- **FR-002**: The cache profiles MUST be:
  - **Generic**: no prefetch.
  - **Media**: 32 MiB head and 16 MiB tail. This is the default for media folders.
  - **Aggressive Media**: 64 MiB head and 32 MiB tail.
- **FR-003**: Cloud Mount folders MUST support SMB, NFS, and WebDAV exports and container bindings.
- **FR-004**: The Core Service MUST start, supervise, and restore mounts through reconciliation. A
  mount that cannot start MUST show Degraded with the reason and retry automatically.
- **FR-005**: Mount events MUST include mount started, ready, and failed; remote unavailable; cache
  pressure; and throttled.

**Level 1: Media Optimistic Cache**

- **FR-006**: Level 1 MUST persist across restarts. It keeps directory metadata, media metadata, and
  selected byte ranges:
  - Directory metadata: name, path, type, remote identity, size, last observed time, and parent.
  - Media metadata: container, duration, resolution, video codec, audio and subtitle tracks, size,
    and remote identity.
- **FR-007**: Level 1 MUST identify cached content by provider, account, remote object identity, and
  size. It MUST NOT identify content by path alone, so renames and moves keep useful cached data.
- **FR-008**: When a file's remote identity or size changes, Level 1 MUST treat it as replaced and
  discard its stale ranges.
- **FR-009**: Opens, probes, and listings of content in the Level 1 index MUST be served from local
  data without a cloud request when the index has no change recorded for that file.
- **FR-010**: Level 1 MUST store data in fixed-size blocks and track which blocks are present.
- **FR-011**: On first open of a file in a media profile, Level 1 MUST fetch and keep the profile's
  head and tail ranges.
- **FR-012**: Sequential reads MUST trigger read-ahead up to a 128 MiB window by default. On a seek,
  outdated prefetching MUST be cancelled where possible and prefetching MUST start near the new
  position.
- **FR-013**: Ranges served to clients MUST be kept in Level 1 for media profiles, subject to
  eviction.

**Level 2: Generic Cache**

- **FR-014**: Non-media files and Level 1 misses MUST be served through Level 2. It provides general
  file behavior and caches recently read data.
- **FR-015**: The read path MUST be: Level 1, then Level 2 on a miss, then the cloud on a miss. Blocks
  fetched for media profiles MAY be promoted into Level 1.

**Consistency and reconciliation**

- **FR-016**: The consistency levels MUST be:
  - **Aggressive**: about every 24 hours.
  - **Balanced**: about every 1 hour. This is the default.
  - **Fresh**: every 5–15 minutes.
- **FR-017**: Background reconciliation MUST:
  - list remote changes;
  - compare remote identities;
  - detect additions, deletions, renames, and replacements;
  - update the local index;
  - discard stale cached ranges.
- **FR-018**: Opens and listings MUST NOT wait for reconciliation unless the requested content is
  unavailable locally.
- **FR-019**: Administrators MUST be able to request an immediate refresh of a folder or subfolder.

**Writes**

- **FR-020**: Clients MUST be able to create, replace whole files, rename, and delete in Cloud Mount
  folders.
- **FR-021**: New and replaced content MUST be written to local write staging. Staging is kept
  separate from the read cache, is journaled, and survives restarts and power loss once the file is
  closed.
- **FR-022**: Staged files MUST be uploaded after close and verified in the cloud. Only then are they
  reported as durable. Staged data MUST NOT be evicted or deleted before verification.
- **FR-023**: After verification, the staged copy MUST become ordinary disposable cache.
- **FR-024**: Changing bytes in the middle of an existing remote file MUST either be staged as a
  complete-file replacement or rejected with a clear error. It MUST NOT be applied as a partial remote
  change.
- **FR-025**: The appliance MUST show pending uploads per folder, with size and age. An alert MUST be
  raised at warning severity when uploads fail repeatedly. The folder MUST be flagged At Risk when a
  closed file has been pending for more than 15 minutes.

**Capacity, eviction, and pinning**

- **FR-026**: Each folder's read cache MUST have a capacity and high and low watermarks, defaulting to
  90% and 80%. When usage passes the high watermark, least recently used data MUST be evicted until
  usage reaches the low watermark.
- **FR-027**: Eviction MUST remove data in this order:
  1. old Level 2 generic blocks;
  2. old playback ranges;
  3. media prefetch ranges;
  4. media metadata and head/tail ranges.
- **FR-028**: Administrators MUST be able to pin files or subfolders as Keep Locally. Pinned content
  MUST be fully cached and MUST NOT be evicted. It stays cloud-backed.
- **FR-029**: Administrators MUST be able to clear a folder's read cache at any time without losing
  data and without affecting write staging.
- **FR-030**: Deleting or corrupting read-cache data MUST NOT lose any data. Corrupt cache metadata
  MUST be detected, discarded, and rebuilt from the cloud.
- **FR-031**: For each folder, the appliance MUST show cache usage by level and category, hit rate,
  pinned size, and pending upload size.

**Resilience and efficiency**

- **FR-032**: When the cloud is unreachable, cached content MUST still be served. Uncached reads MUST
  fail with an I/O error within 30 seconds.
- **FR-033**: Cloud requests MUST back off when throttled. Foreground reads MUST take priority over
  prefetch and reconciliation.
- **FR-034**: Caching MUST be provided through the `cloud.mount`, `cache.media`, and `cache.generic`
  capabilities, so cache providers can be added or replaced as extensions.

### Key Entities

- **Cloud Mount Folder**: A managed folder in Cloud Mount mode, with no data importance. It has
  cloud connection, remote path,
  cache profile, capacity, watermarks, consistency level, and status.
- **Cache Profile**: A preset for prefetch behavior: Generic, Media, or Aggressive Media.
- **Media Index Entry**: Level 1 directory and media metadata for one remote object, keyed by remote
  identity.
- **Cached Block**: A fixed-size range of a remote object held locally. It has a category (metadata,
  head/tail, prefetch, playback, or generic) and a last-used time.
- **Remote Identity**: Provider, account, remote object identity, and size. Together they identify
  remote content independently of its path.
- **Write Staging Entry**: A journaled local copy of new or replaced content. It has states: pending,
  uploading, verified, and failed.
- **Pin**: A Keep Locally rule on a file or subfolder.
- **Reconciliation Run**: A background comparison of the local index with the cloud. It has scope,
  time, and changes found.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A second scan of an unchanged 5,000-item media library makes at least 95% fewer cloud
  requests than the first scan and finishes at least 5 times faster.
- **SC-002**: Opening a media file whose head and tail are cached returns its metadata to the media
  app in under 1 second, with no cloud request.
- **SC-003**: Playback of a high-bitrate movie (40 Mbps) over a 100 Mbps connection runs without
  stalls after the first 5 seconds. 95% of seeks resume within 3 seconds.
- **SC-004**: Under Balanced consistency, 95% of outside cloud changes are visible within 70 minutes.
- **SC-005**: Cutting power with 100 closed files pending upload results in 0 lost files. All are
  uploaded and verified after restart.
- **SC-006**: Clearing or deleting the entire read cache results in 0 data loss and 0 unreadable
  files.
- **SC-007**: Read-cache usage never exceeds its configured capacity by more than 5%, and pinned
  content is evicted 0 times.
- **SC-008**: With the cloud unreachable, 100% of fully cached files stay readable, and uncached
  reads fail within 30 seconds rather than hanging.
- **SC-009**: A library scan on a mount can be served by 1 TB of local cache or less for a library of
  100 TB or more. Only metadata and head/tail ranges need to be local.

## Assumptions

- **Dependencies**: This feature depends on feature 001 for managed folders, exports, container
  bindings, cloud connections, secrets, the operation journal, events, and the capability registry.
- **Not single-writer storage**: The single-writer rule for cloud object storage (feature 001)
  does not apply to Cloud Mount. The remote is authoritative, and other devices and services may
  change it. That is why consistency modes and refresh exist. As with the rest of FileDock, Cloud
  Mount runs on one node and makes no high-availability promise.
- **Target workload**: The primary workload is write-once, read-many media. Libraries usually replace
  or delete files rather than editing them in place.
- **Media detection**: Media files are recognized by file type for the Media profiles. Format-aware
  range selection, such as MKV or MP4 structure analysis, is future work. Fixed head and tail ranges
  are used in this release.
- **Write scope**: Full random in-place writes to remote files, complete custom filesystem semantics,
  and full replacement of the generic cache layer are out of scope, per the product guide's MVP
  non-goals.
- **Change detection**: Change detection uses periodic reconciliation. Provider change feeds or
  webhooks are future work.
- **Recovery behavior**: After an appliance restore, Cloud Mount folders reconnect with empty caches.
  The cloud is authoritative, so no user data restore is needed.
- **No backup in this mode**: FileDock does not back up Cloud Mount folders. Version history and
  recovery of deleted files rely on the cloud provider's own features, where it has them.
- **Default block size**: The default block size is 4 MiB, tunable during planning.
- **Planning constraint from the product guide**: Level 1 is a FileDock component coordinated by the
  Core Service, and Level 2 is rclone's VFS cache layer. This is carried into planning, not required
  by this spec.
