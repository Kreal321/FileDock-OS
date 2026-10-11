# Feature Specification: Hybrid Tiered Storage Mode

**Feature Branch**: `003-hybrid-tiered-storage`

**Created**: 2026-10-11

**Status**: Draft

**Input**: User description: "Based on the FileDock-OS.md, create the 03 requirement for the Hybrid
Tiered Storage"

## Overview

In Tiered Storage, a managed folder uses local storage as the hot tier and cloud storage as the cold,
capacity tier. Every file stays visible in the folder no matter where its data lives. Cold files move
to the cloud automatically when local capacity runs short. They are brought back automatically when
accessed, which is called hydration. Local data is never removed until a verified durable cloud copy
exists. Tier metadata is authoritative and protected frequently, so a replacement appliance can
rebuild the full folder. This feature builds on feature 001.

The only purpose of this mode is to balance cost between a fast local hot drive and cheaper cloud
storage. Tiered Storage has **no backup**. Data is uploaded only when it migrates to the cloud tier,
and files still on the hot tier have no off-appliance copy. Folders that need backup use Hybrid
Backup (feature 002).

The cold tier is **single-writer cloud object storage**: an S3-compatible bucket and prefix (such as
AWS S3, Wasabi, or Backblaze B2) or an Azure Blob container and prefix. FileDock is the only writer,
and data flows one way from the appliance to the cloud tier. Other systems must not change the
bucket or prefix directly. During disaster recovery, a second FileDock installation may read the
cloud tier to rebuild the folder, and it becomes the writer only after a confirmed takeover
(feature 001).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Create a tiered folder with capacity policies (Priority: P1)

An administrator creates a managed folder, for example `Archive`, in Tiered Storage mode. They set:

- a local capacity, such as 500 GB;
- a cloud target from registered connections, such as Backblaze B2;
- a high watermark (default 85%) and a low watermark (default 70%);
- a cold threshold (default 30 days without access);
- hydration on access.

They export the folder over SMB. Clients see one ordinary folder whose capacity extends beyond the
local disk.

**Why this priority**: This is the entry point of the storage mode, and every later story needs it.

**Independent Test**: Create a tiered folder with defaults, export it, copy files in from a client, and
confirm the folder works like a normal local folder with each file's tier state visible.

**Acceptance Scenarios**:

1. **Given** local storage and a registered cloud connection, **When** the administrator creates a
   tiered folder with capacity, target, watermarks, and cold threshold, **Then** the folder is created
   in Tiered Storage mode with the policy shown. No data importance or backup settings are asked for.
2. **Given** the Tiered Storage mode, **When** the administrator reviews the folder before saving,
   **Then** a notice states that files still on the hot tier have no off-appliance copy, and suggests
   Hybrid Backup for data that needs backup.
3. **Given** watermarks where the low value is not below the high value, or a capacity larger than
   free local space, **When** the administrator saves, **Then** the save is refused with the reason.
4. **Given** a tiered folder, **When** a client writes new files, **Then** they are written to the hot
   tier at local speed with tier state Local.

---

### User Story 2 - Safe automatic migration to the cloud tier (Priority: P1)

When the folder's local usage passes the high watermark, the appliance migrates the least recently
accessed files past the cold threshold to the cloud tier until usage drops to the low watermark.
Each migration uploads the file, verifies the cloud copy, records the new state in tier metadata, and
only then frees local space. Every step is journaled, so a crash at any point resumes safely and never
loses data.

**Why this priority**: The constitution requires durability to be verified before anything is
destroyed. Migration is where data loss would happen if this were wrong.

**Independent Test**: Fill a tiered folder above its high watermark with old files. Kill the appliance
at random points during migration, restart, and confirm every file is intact and readable and usage
ends at or below the low watermark.

**Acceptance Scenarios**:

1. **Given** local usage above the high watermark and eligible cold files, **When** the tier engine
   runs, **Then** files are migrated oldest-access first until usage is at or below the low watermark.
2. **Given** a file being migrated, **When** each step completes, **Then** it passes through these
   states in order: uploading, verifying, remote durable, evicting, Cloud Only. Local data is freed
   only after remote durability is recorded.
3. **Given** a crash during any migration step, **When** the appliance restarts, **Then** the migration
   resumes from its last recorded step and the file stays readable throughout.
4. **Given** a cloud copy that fails verification, **When** migration checks it, **Then** the local
   data is kept, the file returns to Local, the faulty remote copy is cleaned up, and an error is
   reported.
5. **Given** a file that is open or modified during migration, **When** eviction would occur,
   **Then** eviction is skipped for that file and it stays Local.

---

### User Story 3 - Transparent access to cloud-tier files (Priority: P1)

A client or container opens a file whose data is only in the cloud. The appliance hydrates it
automatically and serves reads as data arrives, so the client sees an ordinary file. A hydrated file
becomes Local + Cloud, so it can be freed again instantly later without uploading. If the cloud is
unreachable, the open fails with a clear error instead of hanging.

**Why this priority**: A file the client cannot open when needed breaks the experience of a single
folder.

**Independent Test**: Migrate a file to Cloud Only, open it from an SMB client and from a container,
and confirm the content is correct and the tier state becomes Local + Cloud. Repeat with the network
disconnected and confirm a timely error.

**Acceptance Scenarios**:

1. **Given** a Cloud Only file, **When** a client reads it, **Then** it is hydrated, the requested data
   is returned as it arrives, and the state moves Hydrating to Local + Cloud.
2. **Given** a Cloud Only file, **When** the cloud is unreachable, **Then** the read fails within 30
   seconds with an I/O error. The file stays Cloud Only and the event is reported.
3. **Given** a Local + Cloud file that is not modified, **When** space is needed, **Then** its local
   data is freed without uploading again.
4. **Given** a Local + Cloud file, **When** a client modifies it, **Then** the cloud copy is treated as
   stale and the file returns to Local until it migrates again.
5. **Given** a directory listing or file-information request on Cloud Only files, **When** a client
   browses the folder, **Then** names, sizes, and timestamps are served from tier metadata without
   hydrating.

---

### User Story 4 - Cost-first placement with no backup (Priority: P2)

Tiered Storage uploads data only when a file migrates, either automatically or through Move to Cloud
Now. There is no background copying, versioning, or backup. Cloud storage and upload bandwidth are
spent only on data that has actually moved to the cold tier. Every tiered folder clearly shows how
much data is local-only and therefore has no off-appliance copy.

**Why this priority**: Cost balancing is the reason this mode exists. Administrators must understand
that hot-tier data is unprotected, so they can choose Hybrid Backup where they need it.

**Independent Test**: Write files to a tiered folder and leave them below the cold threshold, then
confirm nothing is uploaded. Force migration of some files and confirm uploads equal the migrated
data. Destroy the appliance, restore it with feature 001, and confirm migrated files are readable and
local-only files are listed in the restore report.

**Acceptance Scenarios**:

1. **Given** new files that are not eligible for migration, **When** time passes, **Then** nothing is
   uploaded and the files stay Local.
2. **Given** a tiered folder, **When** the administrator views it, **Then** the size of local-only
   data is shown, with a note that it has no off-appliance copy.
3. **Given** a tiered folder, **When** the administrator opens Advanced, **Then** only tiering
   settings are shown and no backup interval or retention is offered.

---

### User Story 5 - Tier visibility and manual control (Priority: P2)

The administrator can see each file's tier state: Local, Local + Cloud, Cloud Only, Hydrating,
Migrating, or Error. They can also see folder totals by state. They can pin files or subfolders to
always stay local, which is called Keep Locally. They can push selected items to the cloud tier now
or hydrate a subfolder ahead of use.

**Why this priority**: Administrators need control for predictable access, such as keeping a project
local during active work, but automation already works without it.

**Independent Test**: Pin a subfolder, push another to the cloud, and hydrate a third. Confirm states
change accordingly and pinned files are never evicted under pressure.

**Acceptance Scenarios**:

1. **Given** a tiered folder, **When** the administrator views it, **Then** per-file tier state and
   folder totals for each state are shown.
2. **Given** a pinned subfolder, **When** local usage passes the high watermark, **Then** none of its
   files are evicted, and Cloud Only files in it are hydrated.
3. **Given** selected files, **When** the administrator chooses Move to Cloud Now, **Then** they are
   migrated through the safe migration sequence regardless of the cold threshold.
4. **Given** a subfolder, **When** the administrator chooses Hydrate, **Then** all of its files are
   brought local and progress is shown.

---

### User Story 6 - Recover a tiered folder on new hardware (Priority: P2)

Tier metadata records each file's identity, path, size, checksum, tier state, cloud object, and
migration and hydration state. It is authoritative and protected at least every 5 minutes with the
appliance state. After an appliance is restored on new hardware, the tiered folder reappears with its
full namespace. Cloud-tier files are available immediately. Files that were Local + Cloud become Cloud
Only until accessed.

**Why this priority**: This depends on stories 2 and 4 and on feature 001's restore. Without it,
tiering would break the promise that the appliance is replaceable.

**Independent Test**: Run the disaster-recovery acceptance test on an appliance with a tiered folder
and confirm every file that had a durable cloud copy is listed and readable on the new appliance.

**Acceptance Scenarios**:

1. **Given** tier metadata changes, **When** 5 minutes pass, **Then** the changes are included in
   protected appliance state.
2. **Given** a restored appliance, **When** the tiered folder reconciles, **Then** every file with a
   durable cloud copy is listed and readable through hydration.
3. **Given** files that were local-only at the moment of loss, **When** restore completes, **Then**
   they are listed in a restore report as unrecoverable, with their paths.

---

### User Story 7 - Behave predictably when the hot tier is full (Priority: P3)

If the hot tier stays above its high watermark because no files are eligible to move, the appliance
raises a capacity-pressure alert that explains why. Eligible files may be blocked because they are
pinned, recent, or local-only. If the folder reaches its local capacity, new writes fail with a clear
out-of-space error instead of damaging data.

**Why this priority**: This is an edge-of-capacity behavior, but it must be predictable before
release.

**Independent Test**: Fill a tiered folder with pinned and recent files past its capacity and confirm
the alert, the reason, and a clean out-of-space error on further writes.

**Acceptance Scenarios**:

1. **Given** usage above the high watermark with no eligible files, **When** the tier engine runs,
   **Then** a capacity-pressure alert is raised with the reason and suggested actions.
2. **Given** usage at local capacity, **When** a client writes, **Then** the write fails with an
   out-of-space error and existing data is unaffected.

---

### Edge Cases

- A Cloud Only file is renamed or moved within the folder. Only tier metadata changes and no data
  is transferred.
- A file is deleted. The cloud copy is kept for a recovery grace period, 7 days by default, then
  removed. The administrator can restore it within that period.
- The cloud object is deleted or changed outside FileDock, which breaks the single-writer rule.
  Reconciliation finds the mismatch, marks the file Error, and reports an external modification.
  Local data, if any, is kept, and the changed object is never served as valid.
- The original appliance comes back after a replacement has taken over the cloud tier. At its next
  ownership check it stops migrating, evicting, and deleting cloud objects, and it alerts. Its local
  data is untouched.
- The checksum of hydrated data does not match tier metadata. The data is discarded, the file is not
  served as valid, and an error is reported.
- Hydration and eviction of the same file happen at once. Hydration wins and eviction is cancelled.
- Many clients open the same Cloud Only file. One hydration serves all of them.
- Cloud quota is exceeded or credentials expire. Migration pauses with an alert, and local data is
  never evicted.
- A very large file of tens of gigabytes is migrated. The upload can resume after interruption
  without starting over.
- The administrator changes a folder from Tiered to another mode. The appliance hydrates all files
  first, after confirming that enough local space exists, then changes the mode.
- The local disk is replaced or lost while the appliance survives. Files with durable cloud copies
  stay readable through hydration.

## Requirements *(mandatory)*

### Functional Requirements

**Configuration**

- **FR-001**: Tiered Storage MUST be available as a storage mode in the Create Managed Folder flow
  (feature 001). Source / Backend asks for a local reference path (hot tier) and a cloud reference
  path (cold tier). Every subfolder maps to the same relative path in both tiers and uses the
  folder's tiering settings. Subfolders have no settings of their own. Keep Locally pins are actions
  on content, not subfolder settings. Only tiering settings appear in Advanced. Backup settings MUST
  NOT be offered for this mode.
  - Data Importance MUST NOT be asked for or stored for this mode.
  - Before saving, the flow MUST show a notice that hot-tier data has no off-appliance copy.
- **FR-002**: Each tiered folder MUST have:
  - a local capacity;
  - a cloud target chosen from registered cloud connections;
  - a high watermark (default 85%) and a low watermark (default 70%);
  - a cold threshold (default 30 days without access);
  - a deleted-file grace period (default 7 days);
  - hydration on access (always on in this release).
- **FR-003**: Policies MUST be validated before saving. The low watermark must be below the high
  watermark, and the capacity must not exceed available local space.
- **FR-004**: Tiered folders MUST support all exports and container bindings available to other
  managed folders.

**Namespace and tier state**

- **FR-005**: Every file MUST stay visible in the folder namespace with its true name, size, and
  timestamps regardless of where its data is.
- **FR-006**: Each file MUST have one tier state: Local, Local + Cloud, Cloud Only, Hydrating,
  Migrating, or Error.
- **FR-007**: Directory listings and file-information requests MUST be served from tier metadata
  without hydrating data.
- **FR-008**: Renames and moves within the folder MUST update tier metadata only and MUST NOT
  transfer file data.

**Migration and eviction**

- **FR-009**: When local usage exceeds the high watermark, the appliance MUST migrate eligible files,
  least recently accessed first, until usage is at or below the low watermark. Eligible files are
  those past the cold threshold that are neither pinned nor open.
- **FR-010**: Migration MUST follow this order: create the cloud copy, verify it, commit tier
  metadata, then free local data. No step may be skipped or reordered.
- **FR-011**: Each migration MUST be a journaled operation that resumes from its last recorded step
  after a crash or reboot. Every step MUST be idempotent.
- **FR-012**: Local data MUST NOT be freed for a file that is open, modified since its cloud copy, or
  pinned.
- **FR-013**: A failed verification MUST keep local data, return the file to Local, clean up the
  invalid remote copy, and report an error.
- **FR-014**: Freeing local data for a Local + Cloud file that has not changed MUST NOT require
  another upload.
- **FR-015**: Tier work MUST yield to foreground I/O. Administrators MUST be able to set a bandwidth
  limit per cloud target.

**Cost-first placement (no backup)**

- **FR-016**: Data MUST be uploaded to the cloud tier only when a file migrates, either automatically
  (FR-009) or by Move to Cloud Now (FR-025). The appliance MUST NOT copy hot-tier data in the
  background, keep versions, or take backups in this mode.
- **FR-017**: Modifying a Local + Cloud file MUST mark its cloud copy stale and return the file to
  Local until it migrates again. The stale cloud copy MUST be removed after the new copy is verified,
  or when the deleted-file grace period ends.
- **FR-018**: Each tiered folder MUST show the size of local-only data and state that it has no
  off-appliance copy. While any local-only data exists, the folder's protection state MUST be
  Unprotected.

**Hydration**

- **FR-019**: Reading a Cloud Only file MUST hydrate it automatically and return requested data as it
  arrives.
- **FR-020**: Concurrent reads of the same file MUST share one hydration.
- **FR-021**: Hydrated data MUST be checked against tier metadata, using a checksum where one is
  available, before it is served as valid.
- **FR-022**: If the cloud is unreachable, a read of Cloud Only data MUST fail with an I/O error
  within 30 seconds and MUST NOT leave the file in a damaged state.
- **FR-023**: After hydration completes, the file MUST be Local + Cloud.

**Manual control**

- **FR-024**: Administrators MUST be able to pin and unpin files and subfolders as Keep Locally.
  Pinned Cloud Only content MUST be hydrated, and pinned content MUST NOT be evicted.
- **FR-025**: Administrators MUST be able to move selected items to the cloud tier immediately,
  through the same safe migration sequence.
- **FR-026**: Administrators MUST be able to hydrate a subfolder on demand and see its progress.
- **FR-027**: Administrators MUST be able to view per-file tier state and folder totals for each
  state.

**Deletion and consistency**

- **FR-028**: Deleting a file MUST remove it from the namespace immediately. Its cloud copy MUST be
  kept for the grace period, during which the administrator can restore it.
- **FR-029**: The appliance MUST periodically reconcile tier metadata against the cloud target, daily
  by default. A missing or changed cloud object MUST mark the file Error and raise an alert.

**Tier metadata and recovery**

- **FR-030**: Tier metadata MUST record, per file:
  - identity and logical path;
  - size and checksum when available;
  - tier state and cloud object identity;
  - cloud target;
  - local data ranges;
  - migration and hydration state.
- **FR-031**: Tier metadata MUST be authoritative. Changes to it MUST be included in protected
  appliance state at least every 5 minutes.
- **FR-032**: After an appliance restore, a tiered folder MUST rebuild its full namespace from tier
  metadata. Every file with a durable cloud copy MUST be readable through hydration.
- **FR-033**: Files that had no durable cloud copy when the appliance was lost MUST be listed by path
  in a restore report as lost, with their total size.

**Capacity pressure and mode changes**

- **FR-034**: If usage stays above the high watermark with no eligible files, the appliance MUST
  raise a capacity-pressure alert. The alert gives the reason (pinned, too recent, or local-only) and
  suggested actions.
- **FR-035**: At local capacity, new writes MUST fail with an out-of-space error without affecting
  existing data.
- **FR-036**: Changing a folder out of Tiered mode MUST first check that local space is enough. It
  MUST then hydrate all files before applying the new mode.
- **FR-037**: Tiering MUST be provided through the `tier.storage` capability, and tier events
  (migration started and completed, hydration started and completed, pressure, error) MUST be
  published to the event stream.

**Single-writer cloud tier**

- **FR-038**: The cloud reference path MUST be a single-writer cloud object storage location, as
  defined in feature 001 (FR-088 to FR-094). Only the owning appliance may upload, overwrite, or
  delete cloud-tier objects. This includes removing objects after the deleted-file grace period.
- **FR-039**: A recovery installation that has not taken over MUST be able to rebuild the namespace
  and hydrate files as a Recovery reader. Until a takeover is confirmed, it MUST NOT migrate files,
  evict local data, or delete cloud objects.
- **FR-040**: An appliance that loses ownership of its cloud tier MUST stop migration, eviction, and
  cloud deletions for that folder, and MUST raise a critical alert. Local data MUST stay untouched.

### Key Entities

- **Tiered Folder**: A managed folder in Tiered Storage mode. It has local capacity, cloud target,
  watermarks, cold threshold, and grace period. It has no data importance and no backup settings.
- **Tier Policy**: The set of rules governing when files migrate, copy, and hydrate.
- **Tiered File Record**: A file's tier metadata entry. It has identity, path, size, checksum, tier
  state, cloud object identity, local ranges, and migration and hydration state.
- **Tier Migration**: A journaled operation moving a file through uploading, verifying, remote
  durable, evicting, and Cloud Only.
- **Hydration**: An operation that brings a file's cloud data back to the hot tier.
- **Pin**: A Keep Locally rule on a file or subfolder.
- **Cloud Target**: The cloud connection and location that holds the cold tier. The location is the
  folder's cloud reference path (feature 001), and it covers every subfolder. It is a single-writer
  object storage location with one owning appliance.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Killing the appliance at random points during 100 migrations and 100 hydrations results
  in 0 lost or corrupted files. Every file is readable after restart.
- **SC-002**: Within 1 hour of crossing the high watermark, with enough eligible files and a
  reachable cloud target, local usage of a tiered folder is at or below the low watermark. The time
  for data transfer itself is not counted.
- **SC-003**: Browsing a directory of 10,000 Cloud Only files returns in under 2 seconds and transfers
  no file data.
- **SC-004**: For a Cloud Only file, the first bytes reach the client within 5 seconds of opening it
  on a typical broadband connection.
- **SC-005**: Data uploaded to the cloud tier equals the data migrated, plus re-migration of modified
  files, within 1%. 0 bytes are uploaded for files that never become eligible for migration.
- **SC-006**: In the disaster-recovery acceptance test, 100% of files that had a durable cloud copy
  are listed and readable on the replacement appliance.
- **SC-007**: Pinned content is evicted 0 times across sustained capacity-pressure testing.
- **SC-008**: A tiered folder can hold at least 10 times its local capacity in total data while
  remaining fully browsable.

## Assumptions

- **Dependencies**: This feature depends on feature 001 for managed folders, exports, cloud
  connections, secrets, the operation journal, events, the capability registry, and
  appliance-state protection.
- **No backup in this mode**: Tiered Storage exists only to balance cost between local hot storage
  and cheaper cloud storage. Hot-tier data is not protected off the appliance, and the mode has no
  data importance. Data that needs backup belongs in a Hybrid Backup folder.
- **Hydration**: Hydration is on access only in this release. Scheduled and predictive hydration are
  future work.
- **Topology**: Two tiers only, local and cloud, in this release. Multiple local tiers (such as
  NVMe, then HDD, then cloud) and access-frequency policies are future work.
- **Cloud targets**: Cloud targets are single-writer cloud object storage only: S3-compatible
  services (AWS S3, Wasabi, Backblaze B2, and others) and Azure Blob. Consumer cloud drives are not
  cloud-tier targets. They belong to Cloud Mount (feature 004).
- **Cost and recoverability, not availability**: Tiered Storage is single-node. Hot-tier data, and
  any change not yet migrated, is unavailable while the appliance is down, and it is lost if the
  appliance is lost. A multi-writer cloud tier is a non-goal (feature 001).
- **Encryption**: Cloud-tier data is encrypted before it leaves the appliance. The keys are secrets
  included in appliance state.
- **Access tracking**: Last access is tracked by the appliance itself and does not depend on
  filesystem access-time settings.
- **Planning constraint from the product guide**: The FileDock tier engine coordinates through the
  Rust Core Service and uses rclone for cloud access. This is carried into planning, not required by
  this spec.
