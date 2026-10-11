# Feature Specification: Hybrid Backup Storage Mode

**Feature Branch**: `002-hybrid-backup`

**Created**: 2026-10-11

**Status**: Draft

**Input**: User description: "Based on the FileDock-OS.md, create the 02 requirement for the Hybrid
Backup"

## Overview

In Hybrid Backup, local storage holds the authoritative copy of a managed folder. A remote
destination keeps frequent, incremental, encrypted recovery points of it. It is intended for
application data, documents, configuration, and local working files. The target is a recovery point
objective (RPO) of about 5 minutes after the most recent backup completes. This feature builds on
the managed folders, cloud connections, secrets, operation journal, events, and capability registry
from feature 001.

Hybrid Backup is the **only** storage mode that offers backup. Cloud Mount needs none, because the
cloud copy is authoritative. Tiered Storage only balances cost and has no backup.

The backup destination is **single-writer cloud object storage**: an S3-compatible bucket and prefix
(such as AWS S3, Wasabi, or Backblaze B2) or an Azure Blob container and prefix. FileDock is the
only writer, and backups flow one way from the appliance to the destination. Other systems must not
change the destination directly. During disaster recovery, a second FileDock installation may read
the destination to restore from it, and it becomes the writer only after a confirmed takeover
(feature 001). Hybrid Backup does not promise zero data loss. It promises recoverability to the
latest durable recovery point, as recent as 5 minutes for Critical data.

Every subfolder of a Hybrid Backup folder is in Hybrid Backup mode, because a mode covers the whole
managed folder (feature 001). Within the folder, the administrator can override the data importance
of any subfolder. A subfolder without an override uses its parent's importance. Importance is the
only setting a subfolder can override, and Hybrid Backup is the only mode that allows any override.
Importance decides what is backed up and how often:

- **Critical**: backed up every 5 minutes, and never less often than every 15 minutes.
- **Important**: backed up every 15 minutes.
- **Rebuildable**: not backed up by default, which is the suggested choice. The administrator can
  add backup.
- **Temporary**: never backed up.

For example, `/storage/appdata` can back up `homeassistant` as Critical. In the same folder,
`emby/metadata` can be Rebuildable with no backup, and `emby/transcode` can be Temporary.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Protect a local folder on a frequent schedule (Priority: P1)

An administrator creates or edits a managed folder, for example `/storage/appdata/emby`, and selects
the Hybrid Backup mode. They choose a backup destination and a schedule: every 5 minutes, every 15
minutes, hourly, daily, or custom. They also set a retention policy. From then on the appliance
creates incremental, deduplicated, compressed, encrypted recovery points on that schedule. Clients
and containers keep using the folder at local speed.

**Why this priority**: Frequent protection of local data is the reason this storage mode exists.

**Independent Test**: Put a local managed folder on a 5-minute schedule. Change files, wait for two
cycles, and confirm two new recovery points exist at the destination. Confirm the second one
transferred only changed data.

**Acceptance Scenarios**:

1. **Given** a registered destination, **When** the administrator creates a managed folder with
   storage mode Hybrid Backup and data importance Critical, **Then** the backup interval defaults to
   5 minutes and the first recovery point is created immediately.
2. **Given** a Critical Hybrid Backup folder, **When** the administrator sets an interval longer than
   15 minutes, **Then** the setting is refused with the reason.
3. **Given** a protected folder with a few changed files, **When** the next cycle runs, **Then** only
   new or changed data is transferred and stored.
4. **Given** a protected folder, **When** a cycle runs, **Then** clients and containers keep reading and
   writing the folder without being blocked.
5. **Given** a cycle is still running when the next one is due, **When** the schedule fires, **Then** the
   due cycle is merged into a single follow-up cycle rather than running in parallel or piling up.
6. **Given** a new destination, **When** it is first used, **Then** the appliance creates an encrypted
   repository whose key is stored as a secret and included in appliance-state snapshots.
7. **Given** a new Hybrid Backup folder with importance Rebuildable, **When** Advanced is shown,
   **Then** backup is off and labeled "No backup (suggested)". The administrator can turn it on with
   an hourly default.

---

### User Story 2 - Choose importance per subfolder (Priority: P1)

The administrator opens a Hybrid Backup folder and overrides the data importance of individual
subfolders. Subfolders without an override use the parent setting: the nearest parent subfolder
with an override, or else the folder. Subfolders cannot change the mode, the reference path, the
backup destination, Access, or any other folder setting. Each importance in use forms a protection group with its own
schedule and retention, and the defaults come from the importance. Rebuildable subfolders are not
backed up unless the administrator chooses to add backup. Temporary subfolders are never backed up.

**Why this priority**: Application data mixes critical state with caches and regenerable files. Per
subfolder importance backs up what matters often and skips what can be rebuilt. This keeps backup
fast and destination costs low.

**Independent Test**: In `/storage/appdata` (Important), set `homeassistant` to Critical,
`emby/metadata` to Rebuildable, and `emby/transcode` to Temporary. Run cycles and confirm:

- `homeassistant` is backed up every 5 minutes;
- the rest of the folder is backed up every 15 minutes;
- `emby/metadata` and `emby/transcode` add nothing to the destination.

Then turn on backup for `emby/metadata` and confirm it is backed up hourly.

**Acceptance Scenarios**:

1. **Given** a Hybrid Backup folder, **When** the administrator sets a subfolder's importance,
   **Then** that subfolder and everything below it uses the new importance unless a deeper subfolder
   has its own setting.
2. **Given** a Critical subfolder inside an Important folder, **When** cycles run, **Then** the Critical
   subfolder is backed up on its 5-minute schedule and the rest on the 15-minute schedule.
3. **Given** a Rebuildable subfolder, **When** cycles run, **Then** it is not backed up and is shown as
   "No backup (suggested)".
4. **Given** a Rebuildable subfolder, **When** the administrator turns on backup for it, **Then** it is
   backed up on an hourly default schedule they can change.
5. **Given** a Temporary subfolder, **When** cycles run, **Then** it is never backed up, and no option
   to back it up is offered.
6. **Given** a subfolder whose importance is lowered or whose backup is turned off, **When** the
   administrator confirms, **Then** the change is audited, new cycles skip that subfolder, and its
   existing recovery points are kept until retention expires.
7. **Given** a folder view, **When** the administrator opens it, **Then** each subfolder shows its
   effective importance, whether it is inherited or set directly, and its backup status.
8. **Given** a subfolder with an override, **When** the administrator removes the override, **Then**
   the subfolder returns to its parent's importance, and the change is confirmed if that lowers it.
9. **Given** any subfolder, **When** the administrator opens its settings, **Then** only Data
   Importance (and backup on/off for Rebuildable) can be changed. Mode, reference path, destination,
   and Access are shown as inherited from the folder.

---

### User Story 3 - See protection status at a glance (Priority: P1)

For every protected folder the administrator sees:

- the scheduled frequency;
- the latest durable recovery point;
- the amount of pending local changes not yet protected;
- the target RPO;
- whether the last attempt failed and why.

The dashboard summarizes protection across all folders and raises an alert when a folder falls
behind its target.

**Why this priority**: A schedule alone does not prove protection. Administrators need to know the
actual recovery point.

**Independent Test**: Run a protected folder normally, then disconnect the destination. Confirm the
status shows the last durable point, growing pending changes, and an alert once the target RPO is
exceeded.

**Acceptance Scenarios**:

1. **Given** a protected folder, **When** the administrator opens it, **Then** the scheduled
   frequency, latest durable recovery point time, pending change size, target RPO, and last result are
   shown as separate values.
2. **Given** the latest durable recovery point is older than twice the scheduled interval, **When**
   status updates, **Then** the folder is flagged At Risk and an alert event is raised.
3. **Given** a backup attempt fails, **When** status updates, **Then** the failure reason is shown, and
   the latest durable recovery point still shows the last successful one.
4. **Given** an upload was interrupted partway through, **When** status updates, **Then** that partial
   upload is not counted as a recovery point.

---

### User Story 4 - Restore files, folders, or a whole managed folder (Priority: P1)

The administrator picks a protected folder and a recovery point. They browse its contents and
restore a single file, a subfolder, or the entire managed folder. The restore can go back to the
original location or to another location. When restoring a whole folder that a container uses, the
appliance can stop the bound containers first and start them again afterwards.

**Why this priority**: Backups are only valuable if restore works.

**Independent Test**: Delete and modify files in a protected folder. Restore one file to an alternate
location and the whole folder to its original location, then compare the results with the
recovery-point contents.

**Acceptance Scenarios**:

1. **Given** a protected folder, **When** the administrator selects a recovery point, **Then** they can
   browse its directory tree and see file sizes and modification times.
2. **Given** a selected file or subfolder, **When** it is restored to an alternate location, **Then**
   the restored content exactly matches the recovery point and the original location is unchanged.
3. **Given** a restore to the original location where files already exist, **When** the administrator
   chooses a conflict policy (keep both, overwrite, or skip), **Then** that policy is applied. The
   default is keep both.
4. **Given** a whole-folder restore for a folder bound to running containers, **When** the
   administrator confirms, **Then** the bound containers are stopped, the folder is restored, and the
   containers are started again.
5. **Given** a restore is interrupted by a crash or reboot, **When** the appliance restarts, **Then** the
   restore resumes or reports exactly which items were and were not restored.
6. **Given** a freshly restored appliance (feature 001), **When** it reconnects to the backup
   destination, **Then** all protected folders' recovery points are listed and can be restored.

---

### User Story 5 - Incremental protection without full rescans (Priority: P2)

The appliance keeps a journal of changes in each protected folder. Each backup cycle processes only
what changed since the last durable checkpoint, without rescanning the whole folder. If the journal
is lost or found inconsistent, the appliance rescans once, re-establishes a checkpoint, and reports
that it did so.

**Why this priority**: Without incremental change tracking, 5-minute schedules do not scale to large
folders. This is required by the constitution's efficiency principle.

**Independent Test**: Protect a folder with a very large number of unchanged files. Change a handful
of them and confirm the cycle finishes in time proportional to the changes. Then delete the journal
and confirm a single, reported rescan follows.

**Acceptance Scenarios**:

1. **Given** a protected folder with change tracking active, **When** a cycle runs, **Then** it reads
   only the changes recorded since the last durable checkpoint.
2. **Given** a cycle completes and the recovery point is verified at the destination, **When** the
   cycle finishes, **Then** the durable checkpoint advances, and not before.
3. **Given** change tracking is lost after a crash or reboot, or is inconsistent, **When** the next
   cycle runs, **Then** a full rescan is done once, it is reported, and incremental tracking resumes.

---

### User Story 6 - Retention, repository health, and verification (Priority: P2)

Old recovery points are removed according to the folder's retention policy. The administrator can see
how much space each destination uses. The appliance periodically verifies that stored recovery points
are intact and reports any that are damaged.

**Why this priority**: This keeps destination costs predictable and makes sure restore points are
usable, but it is not required for the first backup to work.

**Independent Test**: Set a short retention period, run cycles past it, and confirm expired points are
removed and kept points are intact. Damage a stored object and confirm verification reports it.

**Acceptance Scenarios**:

1. **Given** a retention policy, **When** recovery points expire, **Then** they are removed and storage
   used only by them is reclaimed. The most recent recovery point is never removed by retention.
2. **Given** a restore is in progress from a recovery point, **When** retention runs, **Then** that
   recovery point is kept until the restore ends.
3. **Given** scheduled verification, **When** damaged data is found, **Then** the affected recovery
   points are marked damaged, an alert is raised, and the next cycle stores fresh copies of the
   affected data.

---

### User Story 7 - Honest consistency for databases and application data (Priority: P3)

An administrator can mark a protected folder as containing a database or application with live state.
The appliance labels its recovery points crash-consistent and warns that such points may need the
application's own recovery on restore. Application-aware backup providers registered through the
extension framework can be selected per folder for logically consistent recovery points.

**Why this priority**: Being honest about consistency matters, but application-aware providers are
future extensions. The MVP only needs clear labeling and an extension point.

**Independent Test**: Mark a folder as a database folder, take a recovery point, and confirm the
crash-consistent label and warning appear. Register a sample application-aware provider and confirm
it can be selected.

**Acceptance Scenarios**:

1. **Given** a folder marked as a database folder with no application-aware provider, **When**
   recovery points are listed, **Then** each is labeled crash-consistent with a warning.
2. **Given** a registered application-aware backup provider, **When** the administrator selects it for
   a folder, **Then** recovery points produced through it are labeled application-consistent.

---

### Edge Cases

- The destination is unreachable or rate-limited. Cycles retry with increasing delays, pending
  changes accumulate, and local data stays readable and writable.
- The destination runs out of quota. Cycles fail with a clear quota error, the last durable point is
  unchanged, and an alert is raised.
- Destination credentials expire. The appliance raises a credential warning before expiry when
  possible, and an error when they are rejected.
- A file changes while it is being read for a recovery point. The point contains either the earlier
  or the later version of that file, never a mixture.
- A crash happens mid-upload. The partial upload is never a recovery point, and the next cycle
  continues or repeats the work safely.
- The local disk fills up. Protection keeps working on existing data and reports local disk
  pressure. The appliance deletes nothing except the contents of Temporary subfolders.
- A protected folder is deleted. The administrator chooses whether to keep or delete its recovery
  points. The default is keep.
- Hybrid Backup is disabled on a folder. Existing recovery points are kept until retention or the
  administrator removes them.
- The appliance clock jumps. Recovery-point ordering and retention use a monotonic sequence and are
  not confused.
- A restore target lacks free space. Restore checks space first and refuses, with the required
  amount shown, rather than partially filling the disk.
- Very large single files are protected. Only changed portions are uploaded on later cycles.
- The same destination is used by several folders or by the appliance-state snapshots. Their data is
  kept separate and removing one does not affect the others.
- Nested subfolder settings conflict, for example a Temporary subfolder inside a Critical one. The
  deepest setting wins for its own subtree.
- A file moves between subfolders of different importance. From the next cycle it is protected
  according to its new location.
- Data is moved into a Hybrid Backup folder from a folder in another mode. It becomes Hybrid Backup
  data and takes the importance of its new location from the next cycle.
- Another system deletes or rewrites objects at the destination. Verification marks the affected
  recovery points damaged and reports an external modification. The next cycle writes a new
  complete recovery point.
- A replacement appliance restores from the destination while the original is still running. The
  replacement restores as a Recovery reader. After the takeover, the original finds at its next
  cycle that it has lost ownership, stops backing up, and alerts.
- A subfolder with its own importance is renamed or moved within the folder. Its setting follows
  it. If the subfolder is deleted, its setting is removed and the removal is audited.
- A Temporary subfolder is emptied under local disk pressure while an application is using it. This
  is permitted by its importance, and an event names the subfolder and the amount freed.
- A whole-folder restore includes Rebuildable subfolders without backup and Temporary subfolders.
  Those subfolders are recreated empty and listed as "to rebuild".

## Requirements *(mandatory)*

### Functional Requirements

**Configuration**

- **FR-001**: Hybrid Backup MUST be available as a storage mode in the Create Managed Folder flow
  (feature 001). It MUST also be available when editing a Local Only folder. In this mode, local
  storage is authoritative and Source / Backend asks for a local reference path and a backup
  destination. All subfolders are in Hybrid Backup mode. The folder-level importance MUST be
  Critical, Important, or Rebuildable. Temporary is allowed only on subfolders. Hybrid Backup MUST
  be the only storage mode that offers backup settings and the only one that has a Data Importance.
- **FR-002**: Administrators MUST be able to choose a backup destination from registered cloud
  object storage connections (S3-compatible or Azure Blob) by giving a bucket or container and a
  prefix. The destination MUST meet the single-writer rule (FR-046).
- **FR-003**: Schedules MUST include every 5 minutes, every 15 minutes, hourly, daily, and a custom
  schedule. The default comes from data importance:
  - Critical: every 5 minutes. A Critical folder or subfolder MUST NOT be given an interval longer
    than 15 minutes.
  - Important: every 15 minutes.
  - Rebuildable: no backup by default, shown as the suggested choice. When the administrator turns
    backup on, the default is hourly.

  Backup interval and Retention appear in the flow's Advanced section.
- **FR-004**: Each protected folder MUST have a retention policy. The default keeps:
  - all recovery points for 24 hours;
  - hourly points for 7 days;
  - daily points for 30 days.
- **FR-005**: Administrators MUST be able to exclude file patterns from protection within a folder,
  for example `*.tmp`, in addition to per-subfolder importance.
- **FR-006**: Administrators MUST be able to start a backup cycle on demand for a whole folder or for
  one protection group.

**Per-subfolder importance**

- **FR-007**: Administrators MUST be able to override the data importance of any subfolder of a
  Hybrid Backup folder with Critical, Important, Rebuildable, or Temporary.
  - A subfolder without an override MUST use the parent setting: the nearest parent subfolder that
    has an override, or else the folder. The deepest override wins.
  - Removing an override MUST return the subfolder to the parent setting.
  - Importance, together with turning on backup for a Rebuildable subfolder (FR-009), MUST be the
    only per-subfolder setting. Mode, reference path, backup destination, Access, and other folder
    settings always come from the folder (feature 001).
- **FR-008**: Content with the same effective importance in a folder MUST form one protection group.
  Each group has its own schedule, retention, recovery points, and status. Defaults come from
  importance (FR-003).
- **FR-009**: Rebuildable groups MUST NOT be backed up by default, and the appliance MUST show this as
  the suggested choice. Administrators MUST be able to turn backup on per Rebuildable folder or
  subfolder.
- **FR-010**: Temporary subfolders MUST never be backed up, and backup MUST NOT be offered for them.
  They MAY be emptied under local disk pressure, as defined in feature 001.
- **FR-011**: The folder view MUST show, for each subfolder:
  - its effective importance;
  - whether that importance is inherited or set directly;
  - its backup status.

  The folder's protection state MUST be the worst state among its backed-up groups.
- **FR-012**: Lowering a subfolder's importance, or turning off its backup, MUST require confirmation
  and MUST be audited. Existing recovery points for that content MUST be kept until retention expires.
- **FR-013**: A subfolder's importance setting MUST follow it when it is renamed or moved within the
  folder. The setting MUST be removed, and the removal audited, when the subfolder is deleted.
- **FR-014**: A file moved into a subfolder of a different importance MUST be protected according to
  its new location from the next cycle of that group.

**Backup execution**

- **FR-015**: Each cycle MUST create an incremental recovery point. It stores only data not already
  present at the destination, using deduplication and compression.
- **FR-016**: All data and metadata stored at the destination MUST be encrypted before leaving the
  appliance. Repository keys MUST be stored as secrets and included in appliance-state snapshots.
- **FR-017**: A recovery point MUST be considered durable only after all of its data and metadata are
  confirmed stored at the destination. Partial uploads MUST NOT count as recovery points.
- **FR-018**: Each cycle MUST run as a journaled operation that can resume or safely repeat after a
  crash, reboot, or network loss.
- **FR-019**: Overlapping schedule triggers MUST be merged so that at most one cycle per protection
  group runs at a time.
- **FR-020**: Backup cycles MUST NOT block client or container access to the folder.
- **FR-021**: Backup work MUST yield to foreground I/O. Administrators MUST be able to set a bandwidth
  limit per destination.
- **FR-022**: Each file in a recovery point MUST be a single consistent version, either before or
  after any concurrent change.

**Change tracking**

- **FR-023**: The appliance MUST track changes in each protected folder continuously. Each cycle MUST
  process only changes since the last durable checkpoint.
- **FR-024**: The durable checkpoint MUST advance only after the recovery point is confirmed durable.
- **FR-025**: If change tracking is missing or inconsistent, the appliance MUST do one full rescan,
  report it, and then return to incremental tracking.

**Protection status and alerts**

- **FR-026**: For each protected folder and each of its backed-up protection groups, the appliance
  MUST show the following as separate values:
  - scheduled frequency;
  - latest durable recovery point time;
  - pending local change size;
  - target RPO;
  - last cycle result and reason.
- **FR-027**: The target RPO MUST equal the scheduled interval unless the administrator sets a
  different value.
- **FR-028**: A protection group MUST be flagged At Risk, and an alert event raised, when its latest
  durable recovery point is older than twice its target RPO. Alert severity follows the group's
  importance: critical for Critical, warning for Important, and informational for Rebuildable groups
  with backup turned on.
- **FR-029**: Backup events MUST include started, progress, completed, failed, and at-risk.
- **FR-030**: The dashboard MUST summarize protection across all folders as healthy, at risk, or
  failing.

**Restore**

- **FR-031**: Administrators MUST be able to list recovery points per folder and browse their
  contents.
- **FR-032**: Administrators MUST be able to restore a file, a subfolder, or a whole folder, to the
  original location or to another location.
- **FR-033**: Restores to an existing location MUST apply the chosen conflict policy: keep both,
  overwrite, or skip. The default is keep both.
- **FR-034**: Whole-folder restores of folders bound to containers MUST offer to stop the bound
  containers first and restart them afterwards.
- **FR-035**: Restore MUST check free space before starting and refuse if there is not enough.
- **FR-036**: Restores MUST verify restored content against the recovery point.
- **FR-037**: Restores MUST run as journaled operations that resume or report precise partial results
  after interruption.
- **FR-038**: On an appliance restored through feature 001, the appliance MUST reconnect to backup
  destinations and make all existing recovery points restorable. It MUST offer to restore every Hybrid
  Backup folder's latest points in one action, ordered by protection group importance across all
  folders: Critical, then Important, then Rebuildable groups with backup. Rebuildable subfolders
  without backup and Temporary subfolders MUST be recreated empty and listed as "to rebuild".

**Retention and verification**

- **FR-039**: The appliance MUST remove expired recovery points according to retention and reclaim
  storage used only by them. It MUST NOT remove a folder's most recent recovery point.
- **FR-040**: Retention MUST NOT remove a recovery point that a restore in progress is using.
- **FR-041**: The appliance MUST verify the integrity of stored recovery points on a schedule, weekly
  by default. It MUST mark damaged points and alert.
- **FR-042**: The appliance MUST show the storage used per destination and per protected folder.
- **FR-043**: When a protected folder is deleted or its protection disabled, existing recovery points
  MUST be kept unless the administrator explicitly deletes them.

**Consistency and extensibility**

- **FR-044**: Administrators MUST be able to mark a folder as containing live database or application
  state. Recovery points for such folders MUST be labeled crash-consistent and shown with a warning.
- **FR-045**: Backup MUST be provided through the `backup.snapshot`, `backup.restore`, and
  `backup.retention` capabilities, so that alternative or application-aware providers can be
  registered as extensions and selected per folder.

**Single-writer destination**

- **FR-046**: A backup destination MUST be a single-writer cloud object storage location, as
  defined in feature 001 (FR-088 to FR-094). Only the owning appliance may write recovery points,
  apply retention, or remove data at the destination.
- **FR-047**: A recovery installation that has not taken over MUST be able to list, verify, and
  restore recovery points from the destination as a Recovery reader. Until a takeover is confirmed,
  backup cycles, retention, and pruning MUST NOT run against the destination.
- **FR-048**: If verification finds recovery-point data that this appliance did not write, or data
  that has changed or gone missing, the affected recovery points MUST be marked damaged. The
  appliance MUST report an external modification. The next cycle MUST write a new, complete recovery
  point instead of reusing the damaged data.
- **FR-049**: An appliance that loses ownership of a destination MUST stop backup cycles and
  retention for it. The affected folders MUST show At Risk, with the reason "Ownership lost". Local
  data MUST stay untouched.

### Key Entities

- **Protected Folder**: A managed folder in Hybrid Backup mode. It has folder-level importance,
  destination, exclusions, consistency label, protection groups, and an overall protection status.
- **Subfolder Importance Rule**: A data importance override set directly on a subfolder. It applies
  to that subfolder's subtree, and deeper rules override it. Without a rule, a subfolder uses its
  parent setting. It is the only per-subfolder setting in any mode.
- **Protection Group**: All content in a folder with the same effective importance. It has
  schedule (or no backup), retention, target RPO, recovery points, and status.
- **Backup Destination**: A single-writer cloud object storage location (a bucket or container plus
  a prefix) that holds an encrypted backup repository. It has an owning appliance, usage, and
  health.
- **Recovery Point**: An immutable, encrypted, point-in-time version of a protection group. It has
  time, sequence, size, consistency label, and integrity status.
- **Change Journal**: A per-folder record of changes since the last durable checkpoint.
- **Durable Checkpoint**: The marker for the last change included in a confirmed-durable recovery
  point.
- **Backup Cycle**: A journaled operation that produces one recovery point. It has state, progress,
  and result.
- **Restore Job**: A journaled operation that restores selected content from a recovery point. It has
  target, conflict policy, progress, and result.
- **Retention Policy**: Rules deciding which recovery points are kept.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a folder on a 5-minute schedule with a reachable destination and normal change
  rates, the latest durable recovery point is no older than 10 minutes during 95% of any 24-hour
  period.
- **SC-002**: A cycle on a folder with 1,000,000 unchanged files and 10 changed files completes in
  under 60 seconds, apart from the time to upload the changed data.
- **SC-003**: Killing the appliance at random points during 50 backup cycles results in 0 corrupted
  or unrestorable recovery points. Every previously durable recovery point remains restorable.
- **SC-004**: An administrator can find and restore a single deleted file in under 3 minutes from
  opening the folder.
- **SC-005**: 100% of restored files match their recovery point exactly in automated comparison.
- **SC-006**: When protection falls behind, the At Risk alert appears within 1 minute of the
  threshold being crossed.
- **SC-007**: In the disaster-recovery acceptance test, every Hybrid Backup folder can be restored to
  its latest recovery point on new hardware using only the recovery key and destination access.
  Critical folders are restored before any Important folder.
- **SC-008**: For typical application data, a second recovery point of unchanged data adds less than
  1% of the folder's size to destination storage.
- **SC-009**: Client file operations on a protected folder slow down by no more than 10% while a
  backup cycle runs, compared with when none is running.
- **SC-010**: Temporary subfolders, and Rebuildable subfolders without backup turned on, add 0 bytes
  to destination storage.
- **SC-011**: An administrator can set or change a subfolder's importance in under 30 seconds. The
  new schedule takes effect from that subfolder's next cycle.
- **SC-012**: In the disaster-recovery test, a replacement appliance restores Critical data to a
  recovery point no older than 5 minutes before the original was lost, as long as the original's
  last scheduled cycle completed. Before takeover it writes 0 objects to the destination.

## Assumptions

- **Dependencies**: This feature depends on feature 001 for managed folders, cloud connections,
  secrets, the recovery key, the operation journal, events, alerts, and the capability registry.
- **Destinations**: Destinations are single-writer cloud object storage only: S3-compatible
  services (AWS S3, Wasabi, Backblaze B2, and others) and Azure Blob. WebDAV, SFTP, consumer cloud
  drives, and local or removable disks are not backup destinations in this release.
- **Recoverability, not availability**: Hybrid Backup protects against losing the appliance by
  restoring to a recovery point. It does not keep data available while the appliance is down, and
  changes after the latest durable recovery point may be lost. Active-active appliances and
  repositories with several writers are non-goals (feature 001).
- **Consistency**: MVP provides crash-consistent protection with clear labeling. Application-aware
  database providers (PostgreSQL, MySQL, MongoDB) are future extensions.
- **Separate from appliance-state protection**: Hybrid Backup protects user and application data.
  Appliance configuration is protected separately by feature 001, even when both use the same
  destination.
- **Out of scope**: Replication between destinations, multiple destinations per folder, immutable or
  object-lock retention, and point-in-time database recovery.
- **Planning constraint from the product guide**: Kopia is the recommended built-in backup engine,
  with FileDock owning the schedule and user experience. This is carried into planning, not required
  by this spec.
