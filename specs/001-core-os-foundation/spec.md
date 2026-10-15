# Feature Specification: Core Appliance Foundation and OS Setup

**Feature Branch**: `001-core-os-foundation`

**Created**: 2026-10-11

**Status**: Draft

**Input**: User description: "Based on the FileDock-OS.md, create the 01 requirement for the core and
basic OS Setup, including the installation of the OS, docker, two services, .net UI management
service and rust service basic, so these two can communicate. admin auth, secret storage, recovery,
logs, extensions for subsystems, generic interfaces, manifest, etc for all supporting"

**Revised**: 2026-10-11. Managed folder creation uses the six-section flow Identity, Storage Mode,
Source/Backend, Data Importance, Access, Advanced. Data Importance replaces the earlier durability
classes and maps to internal data classes. In the words of the request, storage mode answers
"where/how the data lives" and data importance answers "what FileDock is allowed to lose or
rebuild". Backup and Data Importance exist only for Hybrid Backup. The other modes have neither.

**Revised**: 2026-10-11. FileDock is a single-node, single-writer hybrid storage appliance. It is
optimized for low cost and recoverability, not high availability. Hybrid Backup and Tiered Storage
write one way to cloud object storage that only FileDock writes to. A second FileDock installation
may access that storage only for disaster recovery. FileDock promises neither zero downtime nor zero
data loss. It promises recoverability to a recovery point as recent as 5 minutes.

**Revised**: 2026-10-11. The system disk provides a built-in **System Pool** with stricter rules
than other pools. By default it may use at most 50% of the system disk, and at least 100 GB of the
system disk always stays free for the system. Appliance state, a configuration backup plus the
secret store, has a 5-minute recovery point. Encrypted copies are kept
automatically on the appliance's own disks. Reinstalling FileDock OS on the same machine therefore
needs nothing from the administrator except the recovery key.

**Revised**: 2026-10-11. Cloud access goes through pluggable **cloud connectors**. FileDock ships two
built-in connectors that use the same manifest and generic contract as any extension:

- an official **native S3 connector**, the default for S3-compatible storage where correctness and
  control matter;
- the **general cloud connector** (rclone), for broad provider coverage and cloud drives.

Several connectors can be enabled at once. **Connector routes** decide which connector serves each
provider family and use, for example S3 to the native connector and cloud drives to rclone. The
administrator can change the routes in extension management. Hybrid Backup repositories on
S3-compatible storage use the backup engine's own S3 access by default.

**Revised**: 2026-10-13. The Management UI follows the user-supplied light-theme,
storage-focused visual reference and [UI design guide](./ui-design-guide.md). This adds US11,
FR-124–FR-135, and SC-027–SC-031 without changing existing storage, security, recovery, or
capability requirements. The image's example dashboard data and functions are illustrative;
only actual supported functionality and observed data may be presented as available.

## Overview

This feature delivers the base FileDock OS appliance that every storage mode depends on. It covers:

- Turning a fresh supported machine or VM into a FileDock appliance.
- Two first-party services: an unprivileged **Management Service**, which runs the web UI and API,
  and a privileged **Core Service**, which runs orchestration.
- A local, permission-restricted channel between the two services.
- Administrator authentication, encrypted secret storage, appliance-state protection, and
  disaster recovery.
- Logs, audit history, health, and live events.
- A generic subsystem and extension framework built on declarative manifests and capabilities.
- **Cloud connectors** with routing. Several connectors can be enabled together, and each provider
  family and use is routed to exactly one of them:

  | Use | Default route |
  |---|---|
  | Appliance-state remote copy on S3-compatible storage | Native S3 connector |
  | Ownership claims on S3-compatible storage | Native S3 connector |
  | Tiered Storage objects on S3-compatible storage | Native S3 connector |
  | FileDock metadata and manifests on S3-compatible storage | Native S3 connector |
  | Hybrid Backup repository on S3-compatible storage | The backup engine's own S3 access |
  | Google Drive, OneDrive, and Dropbox (Cloud Mount) | General cloud connector |
  | Any other provider | General cloud connector |
- The managed-folder model, which every storage feature shares. Each folder has a **Storage Mode**
  (where and how its data lives), chosen together with its FileDock path and **Reference Path**
  (where the data actually lives). The mode covers every subfolder. Hybrid Backup folders also have
  a **Data Importance** (what FileDock may lose or rebuild), which can be overridden per subfolder.
  Subfolders without an override use the parent's importance.
- The share, cloud-connection, and container foundations that features 002 (Hybrid Backup), 003
  (Hybrid Tiered Storage), and 004 (Cloud Mount) build on.
- The **single-writer cloud storage** rule shared by Hybrid Backup and Tiered Storage.
- **Pools** as the only local storage that managed folders use. Data disks form pools. The system
  disk provides exactly one **System Pool**, which is capped so the operating system always keeps
  room:

  ```text
  Physical Disks (data disks)            System Disk
     ↓                                      ↓
  Pools  (RAID-0 style)                  System Pool  (built in; max 50% of the disk,
     ↓                                      ↓           at least 100 GB kept free)
  Managed Folders (Storage Modes) ◀─────────┘
  ```

- **Appliance-state protection** with a 5-minute recovery point. Encrypted snapshots of the
  appliance's configuration and its secret store are kept on the appliance's own disks
  automatically, and optionally at a remote destination. Reinstalling on the same machine
  needs only the recovery key. Recovering after the loss of the whole machine also needs the remote
  destination.

**Single node, single writer.** FileDock is a single-node, single-writer hybrid storage appliance,
optimized for low cost and recoverability rather than high availability.

```text
FileDock OS
   │
   ├── local authoritative data
   │
   └── one-way writes
        ↓
   cloud object storage (S3-compatible, Wasabi, Backblaze B2, Azure Blob)
```

FileDock is the only writer to each configured object storage location (a bucket or container,
plus a prefix). Other systems must not modify that location directly. The one exception is
disaster recovery: a second FileDock installation may read the location in order to restore from
it, and may take over as its writer once the administrator confirms the takeover.

FileDock does not promise zero downtime or zero data loss. If the appliance fails, its data is
unavailable until the appliance is repaired or restored on new hardware. Changes made after the
latest durable recovery point may be lost. What FileDock does promise is low-cost recoverability,
with a target recovery point as recent as 5 minutes.

## Clarifications

### Session 2026-10-11

- Q: Who should be allowed to complete first-run setup and create the first administrator account
  after installation? → A: Whoever has the one-time setup code. The installer prints the code on the
  console, setup in the web UI requires it, and the code stops working once setup is complete.
- Q: How should the appliance notify the administrator about critical problems when nobody is
  looking at the web UI? → A: The web UI only, out of the box. Delivery channels such as email or
  webhooks are left to extensions through a notification capability, and none is built in.
- Q: When the appliance has more than one local data disk, how should FileDock use them? → A: It
  pools them. A pool combines one or more disks RAID-0 style: their capacities add up, and there is
  no redundancy. The pool is the only local storage a managed folder can reference, and it is the
  durable abstraction for local storage. The layers are Physical Disks → Pools → Managed Folders
  (Storage Modes).
- Q: From which networks should the management web UI be reachable? → A: The local network only.
  The UI is served on the appliance's network interfaces, and the administrator may restrict it to
  chosen interfaces. Internet exposure and built-in remote access are out of scope; administrators
  use a VPN or an extension instead.
- Q: Should scripts and other tools be able to call the management API without an interactive
  sign-in, and with what access? → A: Yes, through named API tokens. Each token is Read-only (status,
  alerts, and lists) or Full (everything an administrator can do). Each is shown once, can expire,
  can be revoked, and its use is audited.
- Q: What should the appliance-state backup contain? → A: A versioned configuration document plus
  the encrypted secrets, and no history. It is the configuration needed to rebuild the same
  FileDock on another machine, much like a configuration file:
  - how pools and folders are configured, and everything else about the core features;
  - administrators and tokens;
  - the secrets.

  Audit entries, alerts, and snapshot history start fresh after a restore. Subsystem and extension
  data is not part of it, and is protected the Hybrid Backup way.
- Q: When an extension needs to keep its own data, where should that data live so that Hybrid
  Backup protects it? → A: In managed folders, the same way container apps keep theirs. The
  extension manifest declares its data paths. At install, the administrator binds each path to a
  managed folder, with Hybrid Backup suggested. The appliance warns when a bound folder is not
  protected.
- Q: After a restore on another machine, where should FileDock get the extension package files
  needed to reinstall each recorded extension? → A: From the appliance-state copies. Each installed
  package version is stored once next to the snapshots, outside the configuration document, and
  restore reinstalls the extensions automatically.
- Q: When restoring, should the administrator always choose the disks for each pool again, even
  when the pool's original disks are still attached? → A: No. Recognized original disks are
  reattached automatically with their data. The administrator chooses disks only for pools whose
  disks are missing, for example on a different machine. A pool's name, identity, folders, and
  settings always come from the backup. Only the disk assignment is asked for.
- Q: Should administrators be able to download the configuration backup as a file and use that
  file to restore or build an appliance? → A: Yes, as an encrypted file in the same format as a
  snapshot, including the stored extension packages. Restore on a fresh install accepts an uploaded
  file and needs the setup code plus the recovery key.
- Q: Should connector routes apply only per provider family, or may a single connection override
  the route for its family? → A: Both. Each family and use has a default route, and a connection
  may override it for itself only. The UI shows every connection that overrides its family
  default.
- Q: May a third-party connector extension serve FileDock's own data at owned locations, that is,
  appliance-state copies, ownership claims, tier objects, and metadata? → A: Yes, but only after a
  separate acknowledgement that names the risk. The owned locations then show that a third-party
  connector serves them.
- Q: If the service, or the connector routed for it, can't do conditional writes, should FileDock
  still accept the bucket as an owned location? → A: Yes, after an acknowledgement. The location is
  labelled "basic claim safety", and a dashboard warning stays until it is served with conditional
  writes.
- Q: Which kinds of credentials should an S3-compatible connection accept? → A: A static access key
  and secret key only.
- Q: How should Azure Blob be handled for owned locations, given that the general connector can't
  do conditional writes there? → A: Keep it on the general connector. Azure Blob owned locations
  have basic claim safety, with the acknowledgement and the dashboard warning.

### Session 2026-10-13 — Management UI design

- The administrator supplied a FileDock Management UI Design Guide and requested that its visual
  design apply throughout the system. Its sample dashboard content does not establish correct
  product functionality. Adopt its light theme, navigation shell, typography, colors, rounded
  cards, spacing, and component style; retain the functional requirements and actual data as the
  authority for what the system can do. The reference is retained in
  [assets/management-ui-design-guide.png](./assets/management-ui-design-guide.png).

### Session 2026-10-15 — Approved layout refinement

- One sidebar with nested Settings subpages; exactly one active leaf, expansion distinct from selection.
- Lists and saved summaries lead; Add administrator and Change password open dialogs; shared table
  alignment and compact action treatment apply across the system.
- Create/Edit Folder is a full page with all six numbered sections visible in a single scroll, no
  tabs or wizard gating. Name generates the read-only FileDock path.
- Retain prototypes with written corrections in [ui-layout-revision.md](./ui-layout-revision.md).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Install the appliance and reach the management UI (Priority: P1)

An administrator has a blank bare-metal machine or VM running a fresh install of the supported base
OS. They run the FileDock installer. It checks prerequisites, then installs the Core Service, the
Management Service, the container runtime, and the built-in subsystems at validated, pinned versions.
The administrator opens the management UI in a browser, completes first-run setup by creating the
first administrator account and naming the appliance, and lands on a dashboard showing every service
as healthy.

**Why this priority**: Nothing else in the product can be used or tested until the appliance is
installed and its two services are running and talking to each other.

**Independent Test**: Run the installer on a fresh supported VM, complete first-run setup, and confirm
the dashboard shows the Core Service, Management Service, container runtime, and built-in subsystems
as healthy.

**Acceptance Scenarios**:

1. **Given** a fresh supported machine that meets minimum requirements, **When** the administrator
   runs the installer, **Then** installation completes without further command-line steps and prints
   the address of the management UI.
2. **Given** a machine that fails a prerequisite (unsupported OS version or architecture, too little
   memory or disk, or conflicting services already installed), **When** the installer runs,
   **Then** it stops before changing the system and lists each failed check and how to fix it.
3. **Given** a completed installation, **When** the administrator opens the management UI for the
   first time, **Then** they are guided through first-run setup and no other page is reachable until
   setup is complete. Setup starts only after the one-time setup code printed by the installer is
   entered. A wrong code is refused, and repeated wrong codes are throttled.
4. **Given** first-run setup is complete, **When** the dashboard loads, **Then** it shows the health
   of the Core Service, Management Service, container runtime, and each built-in subsystem.
5. **Given** an installed appliance, **When** the installer runs again, **Then** it repairs missing
   or damaged appliance components without changing configuration or user data.

---

### User Story 2 - Secure administrator access (Priority: P1)

Only authenticated administrators can view or change the appliance. Administrators sign in to the
management UI, sessions expire when idle, and repeated failed sign-ins are throttled. Every
administrative change is recorded in an audit history. The browser-facing service runs without
elevated privileges. It can only ask the Core Service to perform privileged work.

**Why this priority**: A storage appliance holds cloud credentials and user data. An unauthenticated
or over-privileged management surface is unacceptable from the first release.

**Independent Test**: Attempt UI and API access without a session, with a wrong password repeatedly,
and with a valid session. Confirm denial, throttling, success, and audit entries.

**Acceptance Scenarios**:

1. **Given** no valid session, **When** anyone requests any management page or API other than sign-in,
   **Then** access is denied.
2. **Given** an administrator account, **When** the wrong password is entered 5 times in a row,
   **Then** further attempts for that account are blocked for 15 minutes and the event is audited.
3. **Given** a signed-in administrator, **When** they are idle for 30 minutes, **Then** the session
   expires and they must sign in again.
4. **Given** a signed-in administrator, **When** they create another administrator, change a password,
   or remove an administrator, **Then** the change takes effect immediately and an audit entry records
   who did what and when.
5. **Given** the only remaining administrator account, **When** someone attempts to delete or disable
   it, **Then** the action is refused.
6. **Given** an administrator who has forgotten their password, **When** they use the local console
   reset procedure on the appliance, **Then** they can set a new password. Remote password reset
   without local access is not possible.
7. **Given** a Read-only API token, **When** a script reads status and alerts, **Then** the calls
   succeed. **When** the same token attempts any change, **Then** it is refused and the attempt is
   audited.
8. **Given** a Full API token, **When** it is revoked or expires, **Then** its next request is
   refused immediately.

---

### User Story 3 - Core keeps the appliance running independently of the UI (Priority: P1)

The administrator describes what they want, such as folders, shares, mounts, and containers. The
Core Service stores this as desired state and continually reconciles the machine to match it. After a
reboot, crash, or subsystem failure, the Core Service restores the appliance to its desired state
without help. If the Management Service stops or restarts, existing shares, mounts, and containers
keep working. Status changes reach the browser live instead of through constant polling.

**Why this priority**: The constitution requires the data plane to keep working without the control
plane. Reconciliation is also the main recovery model used by every later feature.

**Independent Test**: Configure a share and a container. Stop the Management Service and confirm both
keep serving. Kill a subsystem and reboot the machine, then confirm the Core Service restores the
desired state and reports it.

**Acceptance Scenarios**:

1. **Given** a configured share and running container, **When** the Management Service is stopped,
   **Then** clients keep reading and writing through the share and the container keeps running.
2. **Given** an appliance with configured desired state, **When** the machine reboots, **Then** the
   Core Service loads the desired state, inspects the real machine state, fixes differences, and
   reports healthy without administrator action.
3. **Given** a supervised subsystem, **When** it crashes, **Then** the Core Service restarts it with
   increasing delays, emits failure and recovery events, and marks it Failed with diagnostics if it
   keeps crashing.
4. **Given** the management UI is open, **When** a subsystem changes state, **Then** the change appears
   in the UI within 2 seconds without a page refresh.
5. **Given** the Core Service is unavailable, **When** an administrator uses the UI, **Then** the UI
   shows that the core is unreachable, blocks changes, and recovers by itself when the core returns.

---

### User Story 11 - Navigate a consistent, clear management interface (Priority: P1)

The administrator uses a consistent FileDock interface to understand appliance health, find the
available management areas, and make changes without confusing unavailable features or stale
information with a healthy, configured system. The user-approved visual design applies to setup,
sign-in, the dashboard, settings, and every later management screen.

**Why this priority**: The existing P1 appliance needs a clear and consistent interface for its
combined manual acceptance. The new design must preserve visible failures and security guardrails.

**Independent Test**: Review setup, sign-in, dashboard, and administrator/network/token screens
against the [approved design guide](./ui-design-guide.md), using healthy, failed, empty, and
unreachable appliance states. Repeat the review at desktop and narrow widths and using only a
keyboard. Later feature screens undergo the same review when their functionality is implemented.

**Acceptance Scenarios**:

1. **Given** an implemented management screen, **When** the administrator opens it, **Then** its
   colors, type hierarchy, spacing, cards, controls, and status labels follow the approved design
   guide. Authenticated screens share the sidebar/top-bar layout on desktop.
2. **Given** an implemented destination with no configured resources, **When** it is opened,
   **Then** it shows a useful empty state and a supported next action. A feature not yet available
   is omitted or clearly unavailable, with no nonfunctional navigation or decorative controls.
3. **Given** actual appliance status and configured resources, **When** the dashboard is shown,
   **Then** its cards, lists, capacity bars, and activity reflect those values. No sample user,
   folder, date, count, history, or backup success from the reference is presented as actual data.
4. **Given** a failed subsystem, a stale recovery point, missing protection, or a lost Core
   connection, **When** the administrator views the redesigned UI, **Then** the condition remains
   visible with an appropriate label and existing recovery actions. Unsafe changes remain blocked.
5. **Given** a user on a 360 px-wide viewport or at 200% zoom on a 1280 px-wide viewport,
   **When** they use an implemented screen, **Then** navigation and actions remain reachable,
   panels retain their reading order, and horizontal scrolling is limited to tables or charts.
6. **Given** keyboard-only use or reduced-motion preferences, **When** the administrator completes
   setup, signs in, or uses management controls, **Then** focus, labels, status meaning, and all
   essential actions remain usable without relying on color, hover, animation, or a mouse.
7. **Given** the appliance has no internet access, **When** the administrator opens the UI,
   **Then** its branding, icons, typography or readable fallback, and controls remain usable.
   Required password confirmation and recovery-key download behavior remain unchanged.

8. **Given** any authenticated route, **When** the administrator navigates among main destinations,
   Settings children and resource editors/details, **Then** exactly one destination is current,
   Settings children nest in the sole sidebar, and expansion does not select the Settings parent.
9. **Given** Administrators or a short settings action, **When** the page opens, **Then** its list
   or saved summary leads, with compact actions opening focused dialogs; fields are not permanently
   shown on the overview. Security checks, validation and 44px targets remain intact.
10. **Given** Create/Edit Folder, **When** the page opens, **Then** all six numbered sections appear
    on one scrollable page without tabs or wizard steps, with generated path and applicable controls.

---

### User Story 4 - Encrypted secrets and appliance-state protection (Priority: P2)

All credentials the appliance holds are stored encrypted and never shown in full after entry. These
include cloud tokens, storage keys, share passwords, backup repository keys, API tokens, and
application secrets. During first-run setup the administrator receives a portable recovery key and
confirms they have saved it.

From then on, the appliance protects its own state with a 5-minute recovery point. Within 5 minutes
of any committed configuration change, an encrypted snapshot of the appliance state is stored.
Appliance state is a configuration backup plus the secret store. It is what is needed to rebuild
the same appliance elsewhere, not its history. The snapshot goes to copies the
appliance keeps by itself on its own disks, with no setup. It also goes to an optional remote
destination that the administrator registers, such as a cloud connection or an external disk.

**Why this priority**: Recovery after a reinstall or on new hardware depends on this. It is P2 only
because an appliance that has not yet stored anything has nothing to lose.

**Independent Test**: Save several secrets and configuration changes, then inspect local storage,
logs, the diagnostic bundle, the local copies, and the remote snapshot. Confirm no secret appears in
plaintext and every copy updates within 5 minutes of each change.

**Acceptance Scenarios**:

1. **Given** first-run setup, **When** the recovery key step is reached, **Then** the appliance shows a
   recovery key once and provides a button to download it as a text file and finish setup.
2. **Given** a secret was saved, **When** any administrator views it, **Then** only a masked form is
   shown. Replacing it is allowed but revealing it is not.
3. **Given** a registered appliance-state destination, **When** a configuration change is committed,
   **Then** an encrypted appliance-state snapshot is stored at the destination within 1 minute while
   the destination is reachable.
4. **Given** the destination is unreachable, **When** changes are committed, **Then** the dashboard
   shows appliance state as unprotected since the last successful snapshot, and protection resumes
   automatically when the destination returns.
5. **Given** no remote appliance-state destination is registered, **When** any page loads,
   **Then** a persistent warning says the appliance can be reinstalled on this machine with the
   recovery key, but cannot be recovered after the loss of the machine or its disks.
6. **Given** the administrator regenerates the recovery key, **When** regeneration completes,
   **Then** new snapshots use the new key, the old key no longer opens new snapshots, and the event is
   audited.
7. **Given** an appliance with at least one data pool, **When** a configuration change is
   committed, **Then** within 5 minutes an encrypted snapshot containing the change is stored on
   every Healthy data pool and on the system disk, with no destination configured. Such changes
   include a new share user, a token revocation, and a folder edit. New audit entries and alerts
   alone do not cause a snapshot.
8. **Given** an appliance whose only storage is the System Pool and that has no remote
   destination, **When** any page loads, **Then** a persistent warning says that a reinstall that
   erases the system disk, or a system disk failure, loses the appliance state.
9. **Given** a local copy that cannot be written, for example because its pool is Failed,
   **When** the 5-minute target is missed for that copy, **Then** the copy is shown as stale and a
   warning alert is raised. The other copies continue.

---

### User Story 5 - Restore the appliance after a reinstall or on new hardware (Priority: P2)

**Reinstall on the same machine.** The administrator reinstalls FileDock OS, for example after a
broken update or a replaced system disk. The installer finds the appliance-state copies on the
machine's disks and asks for one thing only: the recovery key. Everything else comes from the
snapshot:

- administrators and their passwords;
- cloud connections and their credentials;
- the remote destination;
- pools and folders.

Pools whose disks are still attached come back with their data. Folders on them serve again
unchanged.

**New hardware.** The original appliance has been destroyed. The administrator installs FileDock on
a blank machine or VM and chooses **Restore Appliance** instead of first-run setup. They choose the
remote appliance-state destination, provide access to it, and enter the recovery key. They pick an
appliance snapshot and restore it, mapping each recorded pool to disks on the new machine.

In both cases, the appliance rebuilds its administrators, connections, managed folders, shares,
schedules, policies, container definitions, extension state, and permissions, then reconciles to
that state. Caches start empty and container images are downloaded again.

**Why this priority**: This is the acceptance test behind the "everything is durable or rebuildable"
principle. It depends on User Story 4.

**Independent Test**:

- Configure an appliance with a data pool, then reinstall the OS on its system disk. Restore by
  entering only the recovery key, and compare the restored configuration and data with the original.
- Separately, destroy the VM, install on a new VM, restore from the remote destination with the
  recovery key, and compare again.

**Acceptance Scenarios**:

1. **Given** a fresh install, **When** the administrator chooses Restore Appliance and provides
   destination access and a valid recovery key, **Then** the available appliance snapshots are listed
   with appliance name and snapshot time.
2. **Given** a selected snapshot, **When** restore completes, **Then** administrators, cloud
   connections, managed folders, shares, share users, schedules, policies, container definitions,
   application bindings, and extension registry match the snapshot.
3. **Given** a restored appliance, **When** reconciliation runs, **Then** connections reconnect,
   built-in subsystems and recorded extensions are reinstalled, container images are downloaded
   again, caches start empty, and the dashboard reports each item's restore status.
4. **Given** a wrong recovery key, **When** restore is attempted, **Then** the restore is refused
   with a clear message, nothing is changed, and repeated attempts are throttled.
5. **Given** a snapshot that is damaged or incomplete, **When** restore is attempted, **Then** the
   appliance refuses that snapshot and offers the next most recent valid one.
6. **Given** a restoring installation, **When** it reads the original appliance's object storage
   locations, **Then** it acts as a Recovery reader. It lists and restores data and writes nothing.
7. **Given** a restored appliance, **When** the administrator confirms the takeover after seeing the
   current owner and its last write time, **Then** the restored appliance becomes the only writer.
   Backup, tiering, and appliance-state snapshots resume, and the takeover is audited.
8. **Given** FileDock OS is reinstalled on a machine whose data disks hold appliance-state copies,
   **When** the installer finishes, **Then** it names the appliance and its newest snapshot time and
   asks only for the recovery key. It asks for no setup code, destination, credentials, or disk
   mapping for disks that are present.
9. **Given** that restore completes, **Then**:
   - pools whose disks are present are reattached with their data, and their folders serve again
     unchanged;
   - the System Pool is recreated on the system disk;
   - cloud connections and the remote destination reconnect with the restored credentials;
   - administrators sign in with their existing passwords.
10. **Given** a reinstall restore, **When** the remote destination holds a newer valid snapshot than
    the local copies, **Then** the newer snapshot is restored.
11. **Given** a reinstall restore, **When** the appliance resumes writing to the locations recorded
    in the snapshot, **Then** it takes them over automatically and audits the takeover. If a
    location was written within the last 15 minutes, the normal takeover confirmation is required
    first.
12. **Given** an administrator downloaded the configuration backup file, **When** they install
    FileDock on a different machine and choose Restore Appliance with that file, the setup code, and
    the recovery key, **Then** the appliance is rebuilt with the same pool names, folders, shares,
    connections, apps, and extensions. The administrator only assigns disks to each pool, and no
    history is brought back.

---

### User Story 6 - Plug in subsystems and extensions through a generic contract (Priority: P2)

Storage, sharing, cloud, backup, cache, and container behavior is provided by subsystems. Each
subsystem has a declarative manifest that names its identity, version, class, capabilities, runtime
type, health check, configuration schema, required permissions, and any persistent data paths. Those
paths are bound to managed folders, so extension data is protected by the folder's mode, not by
appliance state. The administrator can install an
optional extension package. The appliance validates the manifest, grants only the declared
permissions, and starts the extension outside the Core Service process. The extension's capabilities
are then registered. Managed folders request capabilities rather than specific products, and the Core
Service resolves each one to a registered provider.

**Why this priority**: Every later feature registers its providers through this contract. It is P2
because the built-in subsystems can be hand-registered first while the contract is being completed.

**Independent Test**: Install a sample extension of each runtime type, confirm its capabilities appear,
use one of them from a managed folder, kill the extension, and confirm the Core Service and other
subsystems are unaffected.

**Acceptance Scenarios**:

1. **Given** a valid extension package, **When** the administrator installs it, **Then** its manifest is
   validated, its permissions are shown for approval, and on approval it is started and its
   capabilities are listed.
2. **Given** a package whose manifest is invalid, has an unsupported runtime type, needs an
   incompatible appliance version, or fails integrity verification, **When** installation is
   attempted, **Then** it is refused with the specific reason and nothing is installed.
3. **Given** a running extension, **When** its process crashes, **Then** only that extension is
   affected. The Core Service keeps running, other subsystems are undisturbed, and the extension is
   restarted under the normal restart policy.
4. **Given** a managed folder that requires a capability, **When** exactly one provider is registered,
   **Then** it is used. **When** several are registered, **Then** the administrator-chosen default is
   used. **When** none is registered, **Then** the folder shows the missing capability as the reason
   it is not ready.
5. **Given** an extension upgrade, **When** the new version fails its health check, **Then** the
   previous version is restored and the failure is reported.
6. **Given** a subsystem whose capability is in use by a managed folder, **When** removal is attempted,
   **Then** removal is blocked and the dependent folders are listed. Core subsystems cannot be removed.
7. **Given** an extension whose manifest declares a data path, **When** the administrator installs
   it, **Then** they must bind the path to a managed folder, with a Hybrid Backup folder suggested.
   Choosing an unprotected folder shows a warning. The extension can write persistent data only to
   its bound folders.

---

### User Story 7 - Logs, activity, and diagnostics (Priority: P2)

The administrator can view logs from the appliance services and every subsystem in one place,
filtered by source, severity, and time. They can review an activity timeline of operations and an
audit history of administrative actions. They can also download a diagnostic bundle for support with
all secrets redacted.

**Why this priority**: Operators need this to understand failures in every later feature, but basic
operation does not depend on it.

**Independent Test**: Trigger a subsystem failure and an administrative change. Find both in the log
and audit views using filters, download a diagnostic bundle, and confirm it contains no secrets.

**Acceptance Scenarios**:

1. **Given** logs from several sources, **When** the administrator filters by source, severity, and
   time range, **Then** only matching entries are shown, newest first.
2. **Given** any administrative change, **When** the audit history is viewed, **Then** the entry shows
   the administrator, action, target, time, and result.
3. **Given** a diagnostic bundle request, **When** the bundle is produced, **Then** it includes service
   status, recent logs, subsystem health, and configuration with every secret redacted.
4. **Given** operational logs reach their storage cap, **When** new entries arrive, **Then** the oldest
   operational entries are removed first and audit history is not removed before its retention period.

---

### User Story 8 - Create a managed folder by storage mode (Priority: P2)

The administrator creates a managed folder through one guided flow with six sections:

1. **Identity**: a name, such as `Movies`, and a FileDock path, such as `/storage/movies`.
2. **Storage Mode**: Local Only, Cloud Mount, Hybrid Backup, or Tiered Storage.
3. **Source / Backend (Reference Path)**: where the data actually lives, plus any backend the mode
   needs.
4. **Data Importance** (Hybrid Backup only): Critical, Important, Rebuildable, or Temporary.
5. **Access**: any of SMB, NFS, WebDAV, and Docker.
6. **Advanced**: cache, backup interval, retention, tiering, and consistency settings, shown only
   when relevant to the mode.

The storage mode is chosen while the FileDock path and the reference path are being defined.
Together they form the folder's **root definition**:

- **FileDock path**: where clients and containers see the data.
- **Reference path**: where the data actually lives.

The mode applies to the whole folder. Every subfolder and sub-path under the FileDock path uses the
same mode, and each one maps to the matching sub-path under the reference path. A subfolder can never
have a different mode. Content that needs another mode must be a separate managed folder at a path
that does not overlap.

Storage mode says where and how the data lives. Data importance says what FileDock is allowed to
lose or rebuild, and it exists only to decide what Hybrid Backup backs up and how often. Data
Importance is therefore shown and stored only for Hybrid Backup folders and their subfolders. The
other modes need no importance:

- **Cloud Mount**: the cloud copy is authoritative.
- **Tiered Storage**: placement is only about cost.
- **Local Only**: the data is simply kept on the appliance.

In Hybrid Backup, each importance maps to an internal data class. That class drives backup,
alerting, restore order, and what FileDock may delete on its own.

| Data Importance | Internal class | What FileDock may do |
|---|---|---|
| Critical | Authoritative critical state | Must be recoverable to a recent point. Back it up most often (target recovery point 5 minutes), alert at the highest severity, and restore it first. |
| Important | Authoritative user data | Must be recoverable. Back it up on the normal schedule (target recovery point 15 minutes). |
| Rebuildable | Rebuildable | It may be lost and rebuilt, re-downloaded, or regenerated. It is not backed up by default, but the administrator can add backup. |
| Temporary (subfolders only) | Disposable | It may be deleted at any time, including automatically under disk pressure. |

**Backup and Data Importance belong to Hybrid Backup only.** In the other modes:

- **Cloud Mount** needs no backup, because the cloud copy is authoritative.
- **Tiered Storage** only balances cost between the local hot drive and cheaper cloud storage, so
  files still on the hot tier have no off-appliance copy.
- **Local Only** keeps no off-appliance copy.

Inside a Hybrid Backup folder, the administrator can override the importance of a subfolder. That
decides whether and how often the subfolder is backed up (feature 002). A subfolder without an
override uses its parent's importance. In every other mode, and for every other setting, subfolders
always use the parent folder's settings.

**Why this priority**: Every storage feature (002–004) creates folders through this flow. Keeping
importance to the one mode where it changes behavior keeps the flow simple everywhere else.

**Independent Test**: Create one folder for each storage mode the appliance currently supports.
Confirm the source fields change with the mode, and that Data Importance is asked for only in Hybrid
Backup. Confirm each saved folder shows its mode and protection state, plus its importance for Hybrid
Backup folders.

**Acceptance Scenarios**:

1. **Given** the Create Managed Folder flow, **When** the administrator enters the name `Movies`,
   **Then** the FileDock path is generated as `/storage/movies` and shown read-only as the name is entered.
2. **Given** a name or path already used by another managed folder, or a path inside or containing
   another managed folder, **When** the administrator continues, **Then** the flow stops at Identity
   and gives the reason.
3. **Given** a selected storage mode, **When** the Source / Backend section opens, **Then** it asks for
   exactly the reference path and backend that mode needs:
   - Local Only: a local reference path, which is a pool and a path inside it, for example pool
     `media`, path `/movies`.
   - Cloud Mount: a cloud connection and remote reference path, for example `gdrive:/Movies`.
   - Hybrid Backup: a local reference path and a backup destination.
   - Tiered Storage: a local reference path (hot tier) and a cloud reference path (cold tier).
4. **Given** a storage mode whose capability no registered subsystem provides, **When** the mode list
   is shown, **Then** that mode is visible but disabled, with the missing capability named.
5. **Given** an existing managed folder `/storage/appdata`, **When** the administrator tries to create
   a managed folder at `/storage/appdata/emby` with any mode, **Then** creation is refused. The flow
   explains that subfolders always use the parent's mode, and that in Hybrid Backup the subfolder's
   importance can be overridden instead.
6. **Given** a local reference path that is the same as, inside, or contains another managed
   folder's local reference path, **When** the administrator continues, **Then** the flow stops with
   the conflicting folder named.
7. **Given** a saved folder, **When** a client creates a subfolder under its FileDock path, **Then**
   the subfolder uses the folder's mode and maps to the same sub-path under the reference path.
8. **Given** Local Only, Cloud Mount, or Tiered Storage, **When** the flow is shown, **Then** the Data
   Importance section is not shown and no importance is stored.
9. **Given** Local Only or Tiered Storage, **When** the administrator reviews the folder before
   saving, **Then** a notice states that data kept only on the appliance has no off-appliance copy.
   It suggests Hybrid Backup for data that needs backup.
10. **Given** Hybrid Backup, **When** the Data Importance section is shown, **Then** Critical,
    Important, and Rebuildable are offered with none preselected. Each explains what FileDock may lose,
    and the folder cannot be saved without a choice.
11. **Given** a mode, **When** Advanced is opened, **Then** only settings relevant to that mode are
    shown. Backup interval, Retention, and per-subfolder importance appear only for Hybrid Backup,
    pre-filled with defaults chosen by importance.
12. **Given** a Hybrid Backup folder with importance Rebuildable, **When** Advanced is opened,
    **Then** backup is off and marked as the suggested choice, and the administrator can turn it on.
13. **Given** a Hybrid Backup folder, **When** the administrator sets a different importance on one
    of its subfolders, including Temporary, **Then** that subfolder's backup follows its own
    importance. Subfolders without an override use their parent's importance.
14. **Given** a subfolder of a Local Only, Cloud Mount, or Tiered Storage folder, **When** the
    administrator opens it, **Then** no per-subfolder settings are offered and all settings shown
    are the parent folder's.
15. **Given** Access, **When** the administrator selects any combination of SMB, NFS, WebDAV, and
    Docker, **Then** the folder is exported only over the selected protocols and can be bound into
    containers only if Docker is selected.
16. **Given** a Hybrid Backup folder or subfolder, **When** the administrator lowers its importance,
    for example from Important to Rebuildable, **Then** they must confirm what FileDock will now be
    allowed to lose. The change is audited.
17. **Given** a folder changed from Hybrid Backup to another mode, **When** the change is confirmed,
    **Then** its importance settings are removed. A folder changed into Hybrid Backup must have an
    importance chosen.
18. **Given** saved folders, **When** the folder list is shown, **Then** each folder shows its name,
    path, storage mode, access, and protection state. Hybrid Backup folders also show their
    importance.

---

### User Story 9 - Local storage, shares, cloud connections, and containers (Priority: P3)

The administrator builds local storage as pools. A pool is made from one or more blank disks after
an explicit confirmation, or by adopting an existing filesystem as a single-disk pool. Every
appliance also has a built-in System Pool on the system disk. It is capped by two limits, so
folders there can never crowd out the operating system:

```text
Maximum disk usage:                       [ 50 ] %
Minimum free space reserved for system:   [ 100 ] GB
```

Every managed folder lives in a pool. They share a managed folder with clients over SMB, NFS, or
WebDAV and bind it into a container. They register a cloud connection, test it, and keep its
credentials in the secret store. Backup, tiering, and cloud mounts use it later.

**Why this priority**: These shared building blocks are completed by features 002–004. A basic version
here proves the end-to-end path from the UI to the Core Service to the subsystem to the client.

**Independent Test**: Create a Local Only managed folder with SMB, NFS, and Docker access. Bind it
into a sample container, write a file from a client, and read the file from inside the container.

**Acceptance Scenarios**:

1. **Given** discovered local disks and filesystems, **When** the administrator creates a pool from
   blank disks, **Then** the pool is created only after they type an explicit confirmation naming
   every disk. The confirmation warns that losing any one disk loses the whole pool.
2. **Given** an existing filesystem that already holds data, **When** the administrator adopts it,
   **Then** it becomes a single-disk pool and none of its data is erased.
3. **Given** a pool, **When** the administrator adds a blank disk to it, **Then** the pool's
   capacity grows, and every folder in the pool keeps working without being redefined.
4. **Given** a multi-disk pool, **When** one member disk fails or is removed, **Then** the pool is
   marked Failed and its folders unavailable. A critical alert lists each affected folder and how it
   can be recovered.
5. **Given** a managed folder, **When** SMB, NFS, or WebDAV export is enabled with access rules,
   **Then** permitted clients can access it and other clients cannot.
6. **Given** a container definition that binds a managed folder with Docker access, **When** the
   container is deployed, **Then** the folder is available inside the container at the chosen path.
7. **Given** a container definition that keeps persistent data in storage FileDock does not manage,
   **When** it is deployed, **Then** the administrator is warned that this data will not be protected
   or recoverable.
8. **Given** a new cloud connection, **When** the administrator saves it, **Then** the connection is
   tested, its credentials are stored as secrets, and expiring credentials are refreshed
   automatically or flagged before they stop working.
9. **Given** a fresh install, **When** the administrator opens local storage, **Then** the System
   Pool is listed on the system disk. It shows its two limits (50% and 100 GB by default), its
   effective capacity, and its usage, and it needed no disk selection or confirmation.
10. **Given** a folder in the System Pool, **When** its writes reach the System Pool's effective
    capacity, **Then** further writes are refused with a disk-full error, existing data stays
    readable, and a critical alert is raised. The system disk's free space never drops below the
    reserve because of System Pool writes.
11. **Given** the System Pool, **When** the administrator tries to delete it, rename it, add a disk
    to it, or erase it, **Then** the action is refused with an explanation.
12. **Given** the System Pool, **When** the administrator lowers a limit below what its folders
    already use, **Then** the change is refused and the current usage is shown.
13. **Given** the administrator creates a folder in the System Pool, **When** they review it,
    **Then** a warning says that the folder shares the disk with the operating system. Its data is
    lost if the system disk fails or a reinstall erases it, unless its storage mode protects it.

---

### User Story 10 - Choose which cloud connector serves each provider (Priority: P3)

Cloud access goes through cloud connectors, which are subsystems with the same manifest and
contract as any extension. Out of the box, the appliance has two enabled connectors. The native S3
connector serves S3-compatible storage, where FileDock needs correctness and control: conditional
writes for ownership claims, verified integrity on every object, and explicit storage settings.
The general cloud connector serves every other provider, including cloud drives such as Google
Drive, OneDrive, and Dropbox, and provides mounts. In extension management, the administrator sees
which connector serves each provider family and use, and can change any route. Third-party
connectors can be installed later and routed the same way.

**Why this priority**: FileDock's own data at owned locations (appliance state, ownership claims,
tier objects, and metadata) depends on correct cloud writes. Broad provider coverage is still
needed for Cloud Mount. The defaults work with no configuration, so this is P3. The routing contract
is still needed before features 002–004 are planned.

**Independent Test**: On a fresh install, register an S3-compatible connection as the remote
appliance-state destination and a Google Drive connection. Confirm that the snapshots and the
ownership claim go through the native S3 connector, and that the Drive connection uses the general
connector. Switch the S3 route to the general connector, take a snapshot, switch back, and confirm
that every snapshot stays readable and nothing at the location was rewritten.

**Acceptance Scenarios**:

1. **Given** a fresh install, **When** the administrator opens the cloud connectors in extension
   management, **Then** both built-in connectors are listed as enabled, with the default routes in
   the Overview table, and no configuration was needed.
2. **Given** a new S3-compatible connection, **When** it is tested, **Then** the result names the
   connector that served it. It reports whether the service supports conditional writes for
   ownership claims, and the checks performed: list, write, read, verify, and delete inside the
   prefix, and clock skew.
3. **Given** the default routes, **When** the administrator routes S3-compatible owned object
   storage to the general connector, **Then** a confirmation lists the affected connections,
   folders, and owned locations, and states any protection this connector lacks, such as
   conditional writes. After confirmation, the next write session uses the new connector, and the
   data already at each location stays unchanged and readable.
4. **Given** a third-party connector extension that declares support for a provider family, **When**
   it is installed, **Then** no route changes. **When** the administrator routes a use to it,
   **Then** the confirmation names every connection whose credentials it will receive. **When**
   that use is FileDock-owned object storage, **Then** a separate, audited acknowledgement of the
   recovery risk is also required. Afterwards each affected owned location shows that a third-party
   connector serves it.
5. **Given** a route whose connector is Failed or disabled, **When** a use on that route runs,
   **Then** the use fails visibly with an alert naming the connector and the route. The appliance
   does not switch to another connector by itself.
6. **Given** a connector that some route points to, **When** the administrator tries to disable or
   remove it, **Then** the action is refused, and the routes that use it are listed.
7. **Given** an S3-compatible connection used both for Tiered Storage and for a Cloud Mount on a
   different bucket, **When** both run, **Then** the tier objects go through the native S3 connector
   and the mount through the general connector, each as its route says.
8. **Given** a restore, **When** it completes, **Then** the enabled connectors and routes are as
   recorded in the snapshot, with no administrator input.
9. **Given** two S3-compatible connections, **When** the administrator overrides one of them to use
   the general connector, **Then** only that connection moves. The other stays on the native S3
   connector. The route table and the connection list show the override, and a later change to the
   family default does not affect the overridden connection.

---

### Edge Cases

- A redesigned card has no measurements or history. It shows Unknown/Unavailable or an explicit
  empty state; it never shows zero usage or a healthy trend as a substitute for missing data.
- A feature shown in the reference has not been implemented or has no available provider. Its
  navigation/action is omitted or marked unavailable, while implemented areas with zero resources
  remain accessible.
- A long appliance/folder name, translated-size text, narrow viewport, or enlarged text would
  overflow a card. Content wraps or truncates with a way to reveal the complete value; essential
  actions and protection warnings remain visible.
- The live connection fails while a healthy summary is visible. The view marks stale information
  and keeps the Core warning/change restrictions; the redesign cannot imply continued freshness.
- A backup succeeded for only one folder while appliance-state protection is missing. Success is
  scoped to that folder, and the broader protection warning remains visible.
- A display color is difficult to distinguish or lacks text contrast. A label/icon preserves the
  meaning, and readable text/focus colors take precedence over copying the screenshot literally.

- Power is lost during installation. Re-running the installer finishes or repairs the installation.
- The administrator restricts the UI to one network interface while signed in through another. The
  change is refused unless they confirm that their current session will be cut off.
- Someone on the network opens the UI of a freshly installed appliance before its owner does.
  Without the setup code printed on the console, they cannot complete setup or reach any other page.
- Power is lost during a configuration change. The change is either fully applied or not applied at
  all after restart.
- The Core Service and Management Service are different versions after a partial update. The
  services detect the mismatch, refuse unsafe operations, and report it.
- The control channel's permissions are changed or someone else tries to use it. The Core Service
  rejects callers that are not authorized.
- The appliance clock jumps forward or back. Sessions, schedules, and snapshot ordering stay correct.
- The system disk fills up. Logs and disposable data are trimmed first, and the appliance reports
  disk pressure instead of corrupting state.
- The recovery key is lost while the appliance is still healthy. The administrator can regenerate it
  from a signed-in session.
- The recovery key is lost after the appliance is destroyed. The appliance state cannot be recovered,
  and the UI states this plainly at key-creation time.
- Two extensions claim the same capability. Both are registered and the administrator picks the
  default.
- An extension asks for permissions beyond its manifest at runtime. The request is denied and logged.
- An extension declares a data path, and the administrator binds it to a Local Only folder. The
  install continues after a warning, and the folder and extension show the data as Unprotected.
  After a restore on new hardware, the extension is rebound to the recreated folder, which is
  empty and listed as lost.
- An extension tries to write persistent data outside its bound folders. The write is denied by
  its sandbox and logged.
- During restore, an extension's stored package is missing or damaged at the chosen copy. The
  appliance uses the same package version from another copy. If no copy has a valid one, the
  extension is shown as "package needed", with its configuration and bindings kept. It installs as
  soon as the administrator uploads that version. The rest of the restore is not blocked.
- An Azure Blob container is configured as a Tiered Storage reference path. It is accepted after
  the basic claim safety acknowledgement, and the dashboard warning stays while the general
  connector serves it.
- An S3-compatible service does not support conditional writes. The location can be owned only
  after an acknowledgement. Claims fall back to write-then-verify, the location is labelled "basic
  claim safety", and the dashboard warns until it is served with conditional writes.
- The administrator changes a route while a snapshot upload or tier migration is running. The
  running write session finishes on the previous connector, and the next one uses the new connector.
- A route is changed for a family used by a Cloud Mount. The mount restarts on the new connector,
  which briefly interrupts access, and the confirmation says so first.
- A restored route names an extension connector that is not installed yet. The appliance reads the
  remote destination through the built-in connector for that family, reinstalls the extension from
  its stored package (FR-110), and then applies the recorded route.
- A third-party connector crashes. Only the uses routed to it fail, with an alert. They are not
  moved to another connector automatically.
- A restore runs while the original appliance is still online with the same destination. The
  restored appliance reads from the destination but writes nothing until the administrator confirms
  a takeover. If the original wrote recently, it is flagged as possibly still running.
- The original appliance comes back after a takeover. At its next ownership check it finds that it
  is no longer the writer. It stops all writes, deletions, and evictions that depend on that
  location, raises a critical alert, and marks the affected folders "Ownership lost".
- Another system writes into, or deletes from, a FileDock-owned bucket or prefix. FileDock reports an
  external modification when verification or reconciliation finds it. It never imports or merges
  those changes.
- Two appliances are configured with the same bucket and prefix. The second one is refused, and the
  current owner is named.
- The appliance hardware fails. Clients lose access until the appliance is repaired or restored on
  new hardware. This is expected, because there is no failover node.
- A container image download fails after restore. The container shows a retrying state and does not
  block other restored items.
- The administrator removes Docker access from a folder that running containers still bind. The
  change is refused, and the bound containers are listed.
- The administrator changes a folder out of Hybrid Backup. After confirmation, importance settings
  and schedules are removed, and existing recovery points are kept until retention expires.
- Local disk pressure occurs. Temporary subfolders of Hybrid Backup folders are emptied first,
  oldest content first, before any other action is taken. Each emptying is reported as an event, and
  no other data is ever deleted automatically.
- The administrator raises a Hybrid Backup folder's or subfolder's importance, for example from
  Rebuildable to Critical. The stricter defaults take effect immediately, and backup catches up at
  once.
- A client moves or copies data from one managed folder into another, for example from a Local Only
  folder into a Hybrid Backup folder. The data takes the destination folder's mode and settings. In
  Hybrid Backup it takes the importance of its new location.
- A local reference path already contains data when the folder is created. The data is adopted as
  the folder's content under the chosen mode and nothing is deleted. In Hybrid Backup, a first full
  backup starts.
- A disk is mounted by hand somewhere inside a pool, outside FileDock. FileDock does not treat it as
  pool storage, and it reports the mount as unmanaged. Disks become usable only by joining a pool.
- One disk of a three-disk pool fails. The whole pool is lost, not just a third of it:
  - Hybrid Backup folders in the pool are restored from backup into the repaired pool.
  - Tiered Storage folders get back every file that had a cloud copy.
  - Local Only data and data that was only on the hot tier are reported as lost.
- The appliance is restored on new hardware with different disks. The administrator maps each
  recorded pool to new disks. The folders in each pool then come back unchanged, with their data
  restored by mode.
- The administrator tries to give one subfolder of a Tiered Storage folder a different mode, for
  example Hybrid Backup for `/storage/media/photos`. This is refused, and the flow suggests creating
  a separate managed folder for that content.
- The system disk is too small to give the System Pool any capacity after the reserve, for example a
  64 GB disk with the default 100 GB reserve. The System Pool is shown as Unavailable with the
  reason, and folders can't be created in it. The appliance still works with data pools.
- The operating system's own usage grows, for example through container images, until system disk
  free space falls below the reserve while the System Pool is not full. The System Pool stops
  accepting writes until free space is back above the reserve. Nothing is deleted to make room,
  and a critical alert names the cause.
- FileDock OS is reinstalled on a single-disk appliance, and the reinstall erases the system disk.
  No local copy survives, so the administrator restores from the remote destination, as on new
  hardware. Without a remote destination the appliance state is lost. The dashboard warned about
  this beforehand.
- The reinstall finds copies from two different appliances, for example because disks were moved
  from another machine. Both are listed by appliance name and snapshot time, and the administrator
  chooses one.
- At reinstall, some recorded pools have no disks present. The disks that are present are
  reattached. Each missing pool is shown, and the administrator may map it to new disks, adopt a
  filesystem for it, or leave it Failed.
- The local copies differ in age, for example because one pool was full for an hour. Restore uses
  the newest valid snapshot found on any copy.
- A reinstall restore uses a snapshot taken before another appliance took over a recorded
  location. The ownership generation at the location is then higher than the one in the snapshot.
  The restored appliance stays a Recovery reader for that location and raises an alert. It does not
  take over automatically.

## Requirements *(mandatory)*

### Functional Requirements

**Installation and platform**

- **FR-001**: The installer MUST convert a fresh install of the supported base OS on bare metal or a
  VM into a FileDock appliance without further manual configuration steps.
- **FR-002**: The installer MUST check prerequisites before changing anything, including OS version,
  architecture, memory, disk space, network, and conflicting services. It MUST stop with an
  actionable report if any check fails.
- **FR-003**: The installer MUST install the Core Service, Management Service, container runtime, and
  built-in subsystems (SMB, NFS, WebDAV, the native S3 connector, the general cloud connector, and
  the backup engine) at the versions validated for that FileDock release.
- **FR-004**: Re-running the installer MUST be safe. It MUST repair missing or damaged appliance
  components and MUST NOT change configuration or user data.
- **FR-005**: The appliance MUST place its configuration, state, cache, storage, and mount locations
  under predictable, documented paths owned by FileDock.
- **FR-006**: All long-running appliance services and subsystems MUST be supervised so they start at
  boot and restart after failure.

**Two-service architecture and communication**

- **FR-007**: The Management Service MUST run without elevated privileges. It MUST NOT perform
  privileged system operations or manage subsystems directly.
- **FR-008**: The Core Service MUST perform all privileged operations and subsystem management on
  request from the Management Service.
- **FR-009**: The two services MUST communicate over a local channel that is not reachable from the
  network and is restricted to the Management Service identity by operating-system permissions.
- **FR-010**: The control channel MUST support typed request/response calls for desired-state
  changes, status queries, extension management, operation start, and restore. It MUST also support
  a continuous stream of events from the Core Service.
- **FR-011**: Each service MUST declare its interface version. Each MUST detect an incompatible peer
  and refuse unsafe operations while reporting the mismatch.
- **FR-012**: The two services MUST keep separate state stores and MUST NOT write to each other's
  store.

**Desired state, reconciliation, and operations**

- **FR-013**: The Core Service MUST store desired state durably and reconcile the actual machine state
  to it at startup, after each committed change, and when a subsystem changes state.
- **FR-014**: Configuration changes MUST be validated before commit and applied as transactions.
  After any interruption, a change is either fully applied or not applied at all.
- **FR-015**: The Core Service MUST record critical operations in a durable operation journal as state
  machines. After a restart it MUST resume or safely roll back every unfinished operation.
- **FR-016**: Every critical operation step MUST be idempotent, so repeating a step after a crash
  produces the same result.
- **FR-017**: Existing shares, mounts, and containers MUST keep working while the Management Service
  is stopped, restarting, or failed.
- **FR-018**: The Core Service MUST publish events at least for: subsystem started, stopped, and
  failed; mount started, ready, and failed; share reloaded; disk pressure; remote unavailable;
  operation started, progressed, completed, and failed; and restore started and completed.
- **FR-019**: The management UI MUST receive status changes from the event stream without
  periodic full polling. It MUST show the Core Service as unreachable when the stream is lost.

**Administrator authentication**

- **FR-020**: First-run setup MUST create the first administrator account. All other UI and API
  access MUST be blocked until setup is complete.
  - The installer MUST generate a one-time setup code and print it on the machine's console.
  - Setup MUST NOT proceed without the code.
  - Wrong codes MUST be throttled.
  - The code MUST stop working once setup completes.
  - Re-running the installer before setup is complete MUST issue a new code and invalidate the old
    one.
  - Restoring from an appliance-state copy found on the machine's disks is the one path that needs
    the recovery key instead of the setup code (FR-106).
- **FR-021**: Every management UI and API request except sign-in MUST require either an
  authenticated administrator session or a valid API token.
  - Administrators MUST be able to create named API tokens with a scope of Read-only or Full.
    Read-only allows status, alerts, and lists, and never changes, secrets, or restores. Full allows
    everything an administrator can do.
  - A token MUST be shown once, when it is created, and stored as a secret.
  - A token MUST support an optional expiry and MUST be revocable at once.
  - Each use of a token MUST be audited, recording the token's name.
  - Tokens MUST NOT complete first-run setup, reset passwords, or create other tokens.
- **FR-022**: Administrator passwords MUST be non-empty and MUST be stored only as
  salted, slow one-way hashes. Setup MUST suggest more than 8 characters without enforcing a
  minimum length, and MUST require matching password re-entry. The default setup username MUST
  be `root` and MUST be editable.
- **FR-023**: Sessions MUST expire after 30 minutes idle by default. An administrator MUST be able to
  sign out all of their sessions.
- **FR-024**: After 5 consecutive failed sign-ins, the appliance MUST block further attempts for that
  account for 15 minutes.
- **FR-025**: Administrators MUST be able to create, disable, and remove other administrator
  accounts. The last enabled administrator MUST NOT be removable.
- **FR-026**: A password reset MUST be possible only through a local console procedure on the
  appliance.
- **FR-027**: The management UI and API MUST use HTTP on TCP port 8021 by default.
  Development builds MUST NOT offer HTTPS listeners or certificate configuration.
  - The UI and API are intended for the local network. They are served on the appliance's network
    interfaces by default.
  - The administrator MUST be able to restrict them to chosen interfaces.
  - The appliance MUST NOT open router ports, set up tunnels, or otherwise expose the UI to the
    internet on its own.
  - Built-in remote access is out of scope. Remote administration goes through the administrator's
    VPN or an extension.

**Secret storage**

- **FR-028**: All secrets MUST be encrypted at rest. This includes cloud tokens, storage keys, share
  passwords, backup repository keys, API tokens, and application secrets.
- **FR-029**: Secrets MUST NOT appear in plaintext in logs, events, audit entries, diagnostic bundles,
  UI responses, or remote appliance-state snapshots.
- **FR-030**: After a secret is saved, the UI MUST show only a masked form. Administrators MAY replace
  or delete it.
- **FR-031**: Each subsystem and extension MUST receive only the secrets its configuration
  references.
- **FR-032**: Recovery from hardware loss MUST NOT depend on keys bound to the original machine's
  hardware.

**Recovery key and appliance state**

- **FR-033**: First-run setup MUST generate a portable recovery key, show it once, and require the
  administrator to click a button that downloads it as a text file and finishes setup. Manual
  recovery-key group re-entry MUST NOT be required.
- **FR-034**: Administrators MUST be able to regenerate the recovery key. Regeneration MUST be
  audited, and the old key MUST stop opening newly created snapshots.
- **FR-035**: The appliance MUST classify every piece of managed state into one of four internal
  classes:
  - authoritative critical state;
  - authoritative user data;
  - rebuildable;
  - disposable.

  Appliance configuration and critical metadata, such as tier metadata, are always authoritative
  critical state. Only that state goes in the appliance-state snapshot. Hybrid Backup data is
  classified by its Data Importance (FR-074). Data in Local Only, Cloud Mount, and Tiered Storage
  folders is authoritative user data. Local cache copies are disposable.
- **FR-036**: The appliance-state snapshot MUST be a configuration backup: the configuration needed
  to rebuild the same appliance on a different machine, not a copy of everything. It MUST contain:
  - a versioned configuration document;
  - the encrypted secret store;
  - the data needed to open them with the recovery key.

  The configuration document covers the core features:
  - the appliance name and settings;
  - administrators and their password hashes, and API tokens;
  - pools by identity and name, without their disks (the System Pool also keeps its limits);
  - managed folders;
  - storage, cache, tier, and backup policies, and schedules;
  - share definitions, share users, and access rules;
  - cloud connections, the remote destination, and owned locations;
  - enabled cloud connectors and connector routes (FR-115);
  - container definitions and environment, and application bindings;
  - notification channels;
  - the extension registry and each extension's configuration;
  - critical tier metadata.

  The snapshot MUST NOT contain history or volatile data: audit entries, alerts, snapshot
  history, operation history, sessions, the setup code, observed machine state, live events, and
  logs. After a restore, these start fresh. Subsystem and extension data, and application data,
  are not part of the snapshot. They are protected through managed folders, for example in Hybrid
  Backup mode.
- **FR-037**: Administrators MUST be able to register one remote appliance-state destination, either
  a cloud connection or a removable disk. It is in addition to the local copies (FR-104).
- **FR-038**: Appliance state MUST have a target recovery point of 5 minutes. Every committed change
  to the configuration covered by FR-036 MUST be in an encrypted snapshot at every available copy
  within 5 minutes, and normally within 1 minute. Copies include the local copies and the remote
  destination. History and volatile fields, such as audit entries, alerts, and last-used times,
  MUST NOT trigger a snapshot.
- **FR-039**: Each copy MUST keep at least:
  - the 30 most recent appliance-state snapshots;
  - one per hour for the last 24 hours;
  - one per day for the last 30 days.
- **FR-040**: The dashboard MUST show, for each copy, the time of its last successful snapshot. It
  MUST warn when no remote destination is registered, and when any copy misses the 5-minute target.

**Disaster recovery**

- **FR-041**: A fresh install MUST offer Restore Appliance as an alternative to first-run setup.
  When copies are found on the machine's disks, restoring from them is offered first (FR-106).
- **FR-042**: Restore from a remote destination MUST require access to it and the recovery key.
  Restore from an uploaded configuration backup file MUST require the setup code and the recovery
  key (FR-111).
  Restore from a copy found on the machine needs only the recovery key (FR-106). Either way, restore
  MUST list available snapshots by appliance name and time.
- **FR-043**: Restore MUST verify a snapshot's integrity before applying it. It MUST refuse damaged
  snapshots and offer the most recent valid one.
- **FR-044**: After restore, the Core Service MUST reconcile to the restored desired state. This
  includes reconnecting connections, reinstalling recorded subsystems and extensions, downloading
  container images again, and starting caches empty. Extensions MUST be reinstalled from the
  packages kept with the appliance-state copies (FR-110), with their recorded configuration,
  approved permissions, and folder bindings, and without asking the administrator again. The
  appliance MUST report restore status per item.
- **FR-045**: A restored appliance MUST NOT write to any location owned by another appliance,
  including the appliance-state destination, until the administrator confirms a takeover (FR-092).
  The one exception is the automatic takeover after a reinstall restore (FR-108).
- **FR-046**: User data restore MUST be delegated to the owning storage feature. For example, Hybrid
  Backup restores protected local data. The appliance restore MUST show which folders still need
  data restoration.

**Subsystems, extensions, generic interface, and manifest**

- **FR-047**: Every subsystem MUST have a declarative manifest. The manifest declares:
  - identity, name, and version;
  - class (core, built-in, or extension);
  - capabilities;
  - runtime type;
  - health-check method;
  - configuration schema;
  - required permissions;
  - required secrets;
  - persistent data paths, if any (FR-109);
  - compatible appliance versions.
- **FR-048**: The supported runtime types MUST be native process, supervised system service, and
  container service. Third-party code MUST NOT run inside the Core Service process.
- **FR-049**: Every subsystem MUST be operable through one generic contract:
  - identity, version, and capabilities;
  - desired state and current state;
  - configure, start, stop, restart, and reload;
  - health;
  - events;
  - diagnostics.
- **FR-050**: The appliance MUST register capabilities in a capability registry. The product guide's
  examples include `protocol.smb`, `protocol.nfs`, `protocol.webdav`, `cloud.remote`, `cloud.mount`,
  `backup.snapshot`, `backup.restore`, `backup.retention`, `cache.media`, `cache.generic`,
  `tier.storage`, `storage.local`, and `container.runtime`. This feature adds `notify.channel` for
  alert delivery (FR-097).
- **FR-051**: Managed folders MUST request capabilities, and the Core Service MUST resolve each one
  to a provider. With several providers, the administrator-chosen default is used. With none, the
  folder MUST report the missing capability. Cloud capabilities are resolved per provider family
  and use, through connector routes (FR-115).
- **FR-052**: Extension installation MUST validate the manifest, verify package integrity, show the
  requested permissions for administrator approval, and refuse incompatible or invalid packages.
- **FR-053**: Each extension MUST be limited at runtime to the permissions declared in its manifest.
  Requests outside those permissions MUST be denied and logged.
- **FR-054**: A crashing or misbehaving extension MUST NOT stop the Core Service or other subsystems.
  It MUST be restarted with increasing delays and marked Failed after repeated failures.
- **FR-055**: An extension upgrade MUST be verified with a health check. If the check fails, the
  previous version MUST be restored automatically.
- **FR-056**: Core subsystems MUST NOT be removable. Built-in subsystems and extensions MUST NOT be
  removable while a managed folder depends on their capabilities.

**Logs, audit, and diagnostics**

- **FR-057**: The appliance MUST collect logs from both services and all subsystems into one view.
  The view MUST be filterable by source, severity, and time.
- **FR-058**: Operational logs MUST be capped in local disk use, by default 1 GB. The oldest entries
  are removed first. Operational logs are classified as disposable.
- **FR-059**: Audit history MUST record every administrative action and sign-in attempt. Each entry
  records actor, action, target, time, and result. Audit history MUST be kept on the appliance for
  at least 90 days. It is not part of appliance-state snapshots (FR-036), so a restored appliance
  starts a new audit history and records the restore as its first entry.
- **FR-060**: Administrators MUST be able to download a diagnostic bundle with service status,
  subsystem health, recent logs, and redacted configuration.

**Local storage**

- **FR-061**: The appliance MUST discover physical disks and existing filesystems, and MUST
  organize local storage as Physical Disks → Pools → Managed Folders.
  - A pool combines one or more physical disks RAID-0 style: capacity is the sum of its disks, and
    there is no redundancy.
  - A pool MUST be the only local storage a managed folder can reference. A local reference path is
    a pool plus a directory inside it. It MUST NOT be a disk, a device, a mount point, or the pool
    root. The pool root is reserved for FileDock, which keeps the pool's identity and the local
    appliance-state copies there.
  - The system disk backs exactly one pool, the System Pool, under stricter rules (FR-098 to
    FR-103).
- **FR-062**: Pool creation and growth MUST follow these rules:
  - Creating a pool from blank disks MUST require an explicit typed confirmation naming every disk to
    be erased. It MUST warn that losing any one disk loses the whole pool, and that each extra disk
    raises that risk.
  - An existing filesystem MAY be adopted as a single-disk pool without erasing its data.
  - A blank disk MAY be added to an existing pool to grow it. Removing a disk from a pool is not
    supported in this release.
  - A disk MUST belong to at most one pool. The system disk MUST NOT be a member of any pool other
    than the System Pool.
  - Pool names MUST be unique.
- **FR-063**: The pool MUST be the durable abstraction for local storage:
  - A pool's identity, name, member disks, and folders MUST be stored in appliance state.
  - Managed folders MUST refer to a pool by its identity, so disks can be replaced without
    redefining folders.
  - On appliance restore, a pool's name, identity, folders, and settings MUST come from appliance
    state. The backup does not record which physical disks a pool used, so disk assignment is the
    only pool input ever asked for.
  - Pools whose member disks are present MUST be reattached automatically, recognized by their
    pool identity on the disk, with their data. The administrator assigns disks only to pools whose
    disks are missing, for example on a different machine, either by choosing new disks or by
    adopting an existing filesystem. The System Pool is recreated on the
    system disk without mapping (FR-103).
  - A remapped or recreated pool keeps its identity, its folders reconcile, and each folder's data
    is then restored according to its mode.
- **FR-064**: Because a pool has no redundancy, the failure or removal of any member disk MUST
  be treated as loss of the whole pool. The appliance MUST then:
  - mark the pool Failed and every folder in it unavailable;
  - raise a critical alert;
  - report, for each folder, how it recovers. Hybrid Backup folders are restorable from backup.
    Tiered Storage folders get back their cloud-tier files through hydration. Local Only data and
    data that was only on the hot tier are lost.

  Recovery MUST recreate the pool on replacement disks with the same identity. The appliance MUST
  also raise a warning alert, and mark the pool Degraded, when a member disk reports early-failure
  health indicators.

**Managed folders: creation, storage mode, and data importance**

- **FR-065**: Administrators MUST create managed folders through one flow with six sections, in this
  order:
  1. Identity
  2. Storage Mode
  3. Source / Backend (Reference Path)
  4. Data Importance, shown only when Storage Mode is Hybrid Backup
  5. Access
  6. Advanced

  Editing an existing folder MUST use the same sections and validation. Create and edit MUST use
  a full page with all six numbered sections in one scroll, without tabs or step gating. Required
  fields MUST remain visible in their applicable sections; Advanced settings MUST be in-place,
  not a separate tab or modal. Inapplicable sections show a short explanation.
- **FR-066**: Identity MUST include:
  - a **Name**, 1–64 characters and unique among managed folders;
  - a **FileDock Path** under the storage root.

  The path MUST be generated from the name, for example `Movies` becomes `/storage/movies`, and
  displayed read-only as the name is entered. Changing the name updates the generated path. It MUST be unique and MUST NOT be inside or contain another managed folder's path.
- **FR-067**: The storage modes MUST be Local Only, Cloud Mount, Hybrid Backup, and Tiered Storage.
  Each folder has exactly one. This feature delivers Local Only, and features 004, 002, and 003
  deliver Cloud Mount, Hybrid Backup, and Tiered Storage. A mode whose required capability has no
  registered provider MUST be shown disabled, with the missing capability named.
- **FR-068**: Source / Backend MUST ask only for the reference path and backend the chosen mode
  needs. The reference path is where the folder's data actually lives:
  - **Local Only**: a local reference path, which is a pool and a path inside it.
  - **Cloud Mount**: a cloud connection and a remote reference path.
  - **Hybrid Backup**: a local reference path and a backup destination.
  - **Tiered Storage**: a local reference path (hot tier), and a cloud connection with a cloud
    reference path (cold tier).

  The FileDock path, storage mode, and reference path together form the folder's root definition.
  The mode is chosen as part of defining the two paths. A folder MUST NOT be saved until all three
  are set and valid.
- **FR-069**: A folder's storage mode MUST apply to its whole subtree:
  - Every subfolder and sub-path under the FileDock path MUST use the folder's mode.
  - Each one MUST map to the same relative sub-path under the reference path.
  - Storage mode MUST NOT be settable on a subfolder.
  - Creating a managed folder inside, or around, another managed folder MUST be refused (FR-066).
    The refusal MUST explain that subfolders always use the parent's mode, and that in Hybrid Backup
    the subfolder's importance can be overridden instead.
- **FR-070**: Reference paths MUST NOT overlap in a way that would place the same data under two
  folders or two modes:
  - A local reference path MUST NOT be the same as, inside, or contain another managed folder's local
    reference path or FileDock path.
  - A local reference path MUST NOT overlap an appliance-internal location, such as caches, write
    staging, or appliance state.
  - A Tiered Storage cloud reference path MUST NOT be the same as, inside, or contain any other managed
    folder's cloud reference path.
  - A Cloud Mount remote reference path MAY overlap only with other Cloud Mount folders' remote paths
    (feature 004).
  - A Hybrid Backup destination MUST NOT be inside any managed folder's reference path.

  A refused path MUST name the conflicting folder or location.
- **FR-071**: Subfolders MUST use their parent folder's settings, with one exception: in Hybrid Backup,
  a subfolder's Data Importance can be overridden (feature 002). This includes turning on backup for a
  Rebuildable subfolder.
  - A Hybrid Backup subfolder without an override MUST use the importance of its nearest parent
    subfolder that has one, or of the folder.
  - Subfolders of Local Only, Cloud Mount, and Tiered Storage folders MUST NOT have settings of
    their own.
  - Actions on content defined by features 003 and 004, such as Keep Locally pins and manual refresh,
    are not subfolder settings and remain available.
- **FR-072**: Changing a folder's storage mode MUST apply to the folder and every subfolder at once:
  - Changes between Local Only, Hybrid Backup, and Tiered Storage keep the local reference path.
  - Leaving Tiered Storage first brings all data back to local storage (feature 003).
  - Changing a folder into or out of Cloud Mount is not supported in this release. The
    administrator creates a new folder instead.
  - A folder's local reference path MUST NOT be changed after creation in this release.
- **FR-073**: In Hybrid Backup, Data Importance MUST be chosen explicitly with none preselected:
  - The folder level offers Critical, Important, or Rebuildable.
  - Subfolders also offer Temporary (feature 002).

  Each option MUST state in plain words what FileDock may lose or rebuild. The appliance MAY suggest
  an option, for example Critical for paths under `/storage/appdata`. The administrator MUST still
  confirm it.
- **FR-074**: Each Data Importance MUST map to one internal data class:
  - **Critical** maps to authoritative critical state.
  - **Important** maps to authoritative user data.
  - **Rebuildable** maps to rebuildable.
  - **Temporary** maps to disposable.

  The internal class MUST NOT be a separate user setting.
- **FR-075**: Data Importance MUST be offered and stored only for Hybrid Backup folders and their
  subfolders. Local Only, Cloud Mount, and Tiered Storage MUST NOT ask for or store an importance.
  - Before saving a Local Only or Tiered Storage folder, the flow MUST show a notice that data kept
    only on the appliance has no off-appliance copy, and suggest Hybrid Backup for data that needs
    backup.
  - Changing a folder out of Hybrid Backup MUST remove its importance settings after confirmation.
  - Changing a folder into Hybrid Backup MUST require an importance.
- **FR-076**: In Hybrid Backup, data importance MUST set behavior and defaults for the folder and for
  each subfolder.
  - **Critical**:
    - Backed up every 5 minutes by default. The interval MUST NOT be set longer than 15 minutes.
    - Protection alerts are raised at critical severity.
    - Restored first in disaster recovery.
    - Deleting the folder requires typing its name.
  - **Important**:
    - Backed up every 15 minutes by default.
    - Protection alerts are raised at warning severity.
    - Restored after Critical data.
  - **Rebuildable**:
    - Not backed up by default, and the appliance marks this as the suggested choice. The
      administrator MAY turn backup on, with an hourly default interval.
    - Loss after appliance failure is reported as informational.
    - In disaster recovery, Rebuildable data is reconnected or rebuilt.
  - **Temporary** (subfolders only):
    - Never backed up.
    - MAY be emptied by the appliance under local disk pressure before any other action is taken.
      Each emptying is reported as an event.
    - In disaster recovery, Temporary data is recreated empty.

  Only Temporary data MAY be deleted automatically by the appliance to relieve pressure. Data in any
  other importance or storage mode MUST NOT be.
- **FR-077**: Access MUST offer SMB, NFS, WebDAV, and Docker in any combination, including none.
  Selecting a protocol creates its export. Docker permits binding the folder into containers.
  Removing Docker access MUST be refused while containers still bind the folder.
- **FR-078**: Advanced MUST show only the settings relevant to the chosen mode:
  - **Cloud Mount**: Cache and Consistency.
  - **Hybrid Backup**: Backup interval, Retention, and per-subfolder importance.
  - **Tiered Storage**: Tiering.
  - **Local Only**: no Advanced settings.

  Backup settings MUST be offered only in Hybrid Backup mode.

  Advanced MUST remain expanded within the single-page folder form and be pre-filled with
  defaults chosen from the mode. For
  Hybrid Backup, the defaults also depend on data importance.
- **FR-079**: Wherever folders are listed, the appliance MUST show name, path, storage mode, access,
  and protection state. For Hybrid Backup folders it MUST also show data importance. Protection
  state is one of:
  - **Protected**: backups are within target (Hybrid Backup), or the cloud copy is authoritative with
    no overdue uploads (Cloud Mount).
  - **At Risk**: backups or uploads are behind their target.
  - **Unprotected**: data is kept only on the appliance. This covers Local Only folders and the
    local-only portion of Tiered Storage folders.
  - **Not Required**: Hybrid Backup data that is Rebuildable without backup, or Temporary.
- **FR-080**: Lowering a Hybrid Backup folder's or subfolder's data importance MUST require explicit
  confirmation stating what FileDock will then be allowed to lose. Raising it MUST apply the
  stricter defaults and start backup immediately. Both changes MUST be audited.
- **FR-081**: Folders in pools that were reattached with their disks keep their data and need no
  data restore. Appliance restore MUST recover every other folder in this order:
  1. Critical Hybrid Backup data.
  2. Important Hybrid Backup data.
  3. Cloud Mount and Tiered Storage folders, which reconnect to the cloud.
  4. Rebuildable Hybrid Backup data with backup.
  5. Rebuildable data without backup and Temporary data, recreated empty and listed as "to
     rebuild".
  6. Local Only folders, recreated empty and listed as lost.

**Sharing, cloud connections, and containers**

- **FR-082**: The appliance MUST create SMB, NFS, and WebDAV exports for the protocols selected in a
  folder's Access section. Per-export access rules control which clients and share users have
  access.
- **FR-083**: Administrators MUST be able to register, test, update, and remove cloud connections for
  any provider family that an enabled cloud connector serves (FR-112, FR-114). This includes
  browser-based authorization flows. A connection in use by a folder MUST NOT be removable.
- **FR-084**: The appliance MUST refresh expiring connection credentials automatically. It MUST raise
  a health warning before credentials that cannot be refreshed expire.
- **FR-085**: Administrators MUST be able to list containers and to deploy, start, stop, update, and
  remove container applications from multi-container definitions. Only managed folders with Docker
  access can be bound into containers.
- **FR-086**: Container definitions, environment configuration, and application secrets MUST be
  stored as authoritative critical state. Container images and runtime data MUST be treated as
  rebuildable.
- **FR-087**: The appliance MUST warn when a container keeps persistent data outside managed folders.

**Single-writer cloud storage and recovery scope**

- **FR-088**: Hybrid Backup destinations and Tiered Storage cloud reference paths MUST be cloud
  object storage locations, each a bucket or container plus a prefix. Supported services are
  S3-compatible storage (such as AWS S3, Wasabi, and Backblaze B2) and Azure Blob. Cloud Mount is not
  covered by this rule, because its cloud copy is authoritative and other systems may change it
  (feature 004).
- **FR-089**: Each such location MUST have exactly one writer: the FileDock appliance that owns it.
  The same applies to an appliance-state destination that is a cloud object storage location.
  - Data MUST flow one way, from the appliance to the location.
  - The appliance MUST NOT import, merge, or sync changes made at the location by anything else.
- **FR-090**: Configuring a location MUST record an ownership claim in it, naming the owning
  appliance and an ownership generation. The appliance MUST accept the location only if it is empty
  or already holds this appliance's claim. A location claimed by another appliance MAY be taken over
  only through recovery (FR-092). The appliance MUST refuse a location that holds data FileDock did
  not write.
- **FR-091**: Before each write session at a location, the appliance MUST confirm that it still
  holds the current claim. Write sessions include a backup cycle, a tier migration, cleanup of
  deleted files, retention pruning, and an appliance-state snapshot. If the claim is no longer its
  own, the appliance MUST:
  - stop all writes and deletions at that location;
  - stop evicting local data that depends on it;
  - leave local data unchanged;
  - raise a critical alert.

  This check MUST NOT depend on a lock service, a cluster, or any other running appliance.
- **FR-092**: Disaster recovery is the one exception to the single-writer rule:
  - A second FileDock installation MUST be able to read a location owned by another appliance
    without claiming it. It can list and verify appliance snapshots, recovery points, and tier
    objects, and it can restore data.
  - Writing MUST require an explicit takeover confirmed by the administrator. The confirmation MUST
    show the current owner and its last write time. If the owner wrote within the last 15 minutes,
    the confirmation MUST warn that the original appliance may still be running. After a reinstall
    restore, the takeover is automatic under the conditions in FR-108.
  - Takeover MUST advance the ownership generation and MUST be audited. It is the only way ownership
    moves between appliances.
- **FR-093**: The appliance MUST report as an external modification any data at an owned location
  that it did not write, or that has changed or gone missing. Verification and reconciliation
  (features 002 and 003) find these changes. Such data MUST NOT be used as valid.
- **FR-094**: For each owned location, the appliance MUST show:
  - the owning appliance and ownership generation;
  - the connector that serves it, and whether that connector is built-in or third-party;
  - its claim safety, conditional or basic (FR-123);
  - the last successful write;
  - this appliance's role: Writer, Recovery reader, or Ownership lost.
- **FR-095**: The appliance is a single node. The UI and product documentation MUST NOT promise zero
  downtime or zero data loss. They MUST describe protection as recoverability to a recovery point,
  with a target as recent as 5 minutes for Critical Hybrid Backup data. They MUST state that changes
  made after the latest durable recovery point may be lost if the appliance is lost.

**Alerts and notifications**

- **FR-096**: The appliance MUST keep an alert list in the web UI. Each alert MUST record:
  - severity: info, warning, or critical;
  - source and affected object;
  - first-seen and last-seen times;
  - state: Active, Acknowledged, or Resolved.

  Alerts MUST resolve automatically when their condition clears. An active critical alert MUST stay
  visible on every page until it is acknowledged or resolved.
- **FR-097**: Delivering alerts outside the web UI MUST be done only by extensions that provide the
  `notify.channel` capability. No delivery channel, such as email or webhooks, is built in.
  - Every alert MUST be offered to each registered channel, filtered by the minimum severity
    configured for that channel.
  - A failed delivery MUST be retried and logged. It MUST NOT block or drop the alert in the UI.
  - When no channel is registered, the dashboard MUST show a persistent notice that alerts are
    visible only in the web UI.

**System Pool**

- **FR-098**: Every appliance MUST have exactly one System Pool on the system disk. It MUST be
  created at installation without administrator action. It is the only way managed folders can use
  the system disk. Folders reference it the same way as any other pool, and it is listed with the
  other pools.
- **FR-099**: The System Pool MUST have two administrator-set limits:

  ```text
  Maximum disk usage:                       [ 50 ] %
  Minimum free space reserved for system:   [ 100 ] GB
  ```

  - **Maximum disk usage** is the share of the system disk's capacity that the System Pool may
    use. The default is 50%, and the allowed range is 10% to 90%.
  - **Minimum free space reserved for system** is free space on the system disk that the System
    Pool MUST NOT use. The default is 100 GB, and the minimum is 20 GB. The operating system,
    logs, container images, and FileDock's own state use this space.
  - The System Pool's effective capacity is whichever limit is reached first. The UI MUST show both
    limits, the effective capacity, and which limit currently applies.
- **FR-100**: The limits MUST be enforced, not only reported:
  - Writes that would exceed the effective capacity MUST be refused with a disk-full error. Reads
    MUST continue.
  - If system disk free space falls below the reserve for any reason, the System Pool MUST stop
    accepting writes until free space is back above it.
  - The appliance MUST NOT delete System Pool data to restore the reserve. The only exception is
    Temporary content under the local disk pressure rule.
  - The appliance MUST raise a warning alert at 90% of effective capacity. It MUST raise a critical
    alert when the System Pool is full or the reserve is reached.
- **FR-101**: The System Pool MUST have stricter rules than other pools:
  - It MUST NOT be deleted, renamed, erased, adopted, extended with disks, or recreated by the
    administrator.
  - Changing a limit MUST be audited. Lowering a limit below what System Pool folders already use
    MUST be refused. If applying a lower limit briefly interrupts access to System Pool folders,
    the administrator MUST be told before confirming.
  - If the system disk can't give the System Pool any capacity under its limits, the System Pool
    MUST be shown as Unavailable with the reason. Folders MUST NOT be created in it.
  - Early-failure health indicators on the system disk MUST mark the System Pool Degraded and
    raise a warning alert, as for other pools.
- **FR-102**: Creating a folder in the System Pool MUST show a warning. The folder shares the disk
  with the operating system, and its data is lost if the system disk fails or a reinstall erases
  it. Only its storage mode can bring it back: Hybrid Backup from backup, and Tiered Storage from
  its cloud copies. The flow MUST recommend a data pool for large or unprotected data.
- **FR-103**: On restore, the System Pool MUST be recreated on the system disk with its recorded
  identity and limits, without mapping. Its folders reconcile and their data is restored according
  to their mode. If the new system disk can't hold what those folders need under the limits, the
  appliance MUST report which folders don't fit and leave them pending.

**Local appliance-state copies and reinstall**

- **FR-104**: The appliance MUST keep local appliance-state copies automatically, without any
  configuration:
  - on every Healthy data pool, in the pool's reserved area;
  - on the system disk, outside FileDock's working state.

  Local copies MUST use the same encrypted format, integrity checks, and retention as the remote
  destination. They need no ownership claim, because each pool belongs to one appliance. A copy that
  can't be written MUST be shown as stale, and the other copies MUST continue.
- **FR-105**: The dashboard MUST show what each kind of loss would need:
  - **Reinstall on this machine**: the recovery key only, when a current copy exists on a disk that
    a reinstall does not erase.
  - **Loss of the machine or all its disks**: the recovery key and the remote destination.

  The dashboard MUST warn when either case is not covered.
- **FR-106**: When FileDock OS is installed on a machine whose disks hold appliance-state copies, the
  installer and the first-run screen MUST detect them. They MUST offer to restore, showing the
  appliance name and the newest snapshot time.
  - That restore MUST ask only for the recovery key. The setup code, destination details,
    credentials, and mapping of disks that are present MUST NOT be required.
  - Everything else, including administrators and their passwords, cloud connections and their
    credentials, and the remote destination, MUST come from the snapshot.
  - A wrong recovery key MUST be refused with nothing changed, and repeated attempts MUST be
    throttled.
  - Restoring from a remote destination entered by hand still requires the setup code. This keeps
    someone on the network from restoring their own appliance state onto a fresh install.
- **FR-107**: Restore MUST use the newest valid snapshot among all copies it can reach. It reads the
  local copies first. After decrypting one, it reads the remote destination recorded in it, as a
  Recovery reader. The administrator MAY choose an older snapshot instead.
- **FR-108**: After a restore from a copy found on the machine, the appliance MUST take over the
  owned locations recorded in the snapshot automatically. Each takeover advances the ownership
  generation and is audited. There are two exceptions:
  - If a location was written within the last 15 minutes, the takeover confirmation of FR-092 MUST
    be required, because the original installation may still be running.
  - If a location's ownership generation is higher than the one recorded in the snapshot, another
    appliance took it over after the snapshot. The appliance MUST stay a Recovery reader for that
    location and raise an alert.

**Subsystem and extension data**

- **FR-109**: Subsystem and extension data MUST live in managed folders, never in appliance state:
  - A subsystem or extension that keeps persistent data MUST declare each data path in its
    manifest.
  - At install, the administrator MUST bind each declared path to a managed folder. The flow MUST
    suggest a Hybrid Backup folder. It MUST warn, before installing, when the chosen folder is not
    protected, for example a Local Only folder.
  - The extension MUST be given access only to its bound folders, as `storage.read` and
    `storage.write` permissions, and MUST NOT write persistent data anywhere else.
  - The bindings are configuration and are part of appliance state. After a restore, the extension
    is rebound to the same folders, and its data comes back the way the folder's mode restores it.
  - A folder bound to an extension MUST NOT be removed while the binding exists.
  - Built-in subsystems keep only configuration and secrets, which appliance state covers. Their
    user data is the content of managed folders.
- **FR-110**: The appliance MUST keep the package of every installed extension version with each
  appliance-state copy, local and remote:
  - Each package version MUST be stored once per copy, outside the configuration document, and
    checked for integrity. Snapshots refer to it, so it is not repeated every 5 minutes.
  - A package MUST be kept while any retained snapshot at that copy refers to it.
  - A package MUST be stored at a copy before any snapshot there refers to it.
  - Built-in subsystems are not stored, because the installer provides them.
  - Packages MUST be encrypted like snapshots, because they may contain configuration defaults.
    The recovery key is the only key needed to read them.

**Configuration backup file**

- **FR-111**: Administrators MUST be able to download the newest appliance-state snapshot as one
  file:
  - The file MUST use the snapshot's encrypted format and integrity checks, and MUST include the
    stored packages of installed extensions (FR-110). No readable or secret-free export is offered.
  - Downloading MUST require an interactive session, not an API token, and MUST be audited.
  - On a fresh install, Restore Appliance MUST accept an uploaded file as a source. That restore
    MUST require the setup code and the recovery key, and otherwise behaves like a restore from a
    remote destination. Disks are assigned only to pools whose disks are missing, and no takeover
    is automatic.
  - A file whose integrity check fails, or whose format version is newer than the installed
    release can read, MUST be refused with nothing changed.

**Cloud connectors and routing**

- **FR-112**: All cloud access MUST go through a cloud connector, or through the backup engine's own
  access where a route names it (FR-116):
  - A cloud connector is a subsystem. Its manifest MUST declare each cloud capability it provides,
    `cloud.remote` (object and file operations) or `cloud.mount`, and the provider families it
    serves for each one.
  - Connectors MUST be operated through the generic contract (FR-049), run outside the Core Service
    process (FR-048), and follow the extension rules for permissions, crashes, and upgrades
    (FR-052 to FR-055).
  - Several connectors MAY be enabled at the same time.
- **FR-113**: The installer MUST provide two built-in connectors, both enabled:
  - the **native S3 connector**, which provides `cloud.remote` for the S3-compatible family. It does
    not provide `cloud.mount`.
  - the **general cloud connector**, which provides `cloud.remote` and `cloud.mount` for every
    provider family it supports, including S3-compatible, Azure Blob, Google Drive, OneDrive, and
    Dropbox.

  Built-in connectors can be disabled when no route uses them (FR-121), but can't be removed.
- **FR-114**: Every cloud connection MUST have one provider family: S3-compatible, Azure Blob,
  Google Drive, OneDrive, Dropbox, or a family that only specific connectors serve.
  - S3-compatible connections MUST be defined independently of any connector. The definition
    covers the endpoint, region, addressing style, bucket, and credentials. The same connection
    then works with any connector routed for that family.
  - S3-compatible credentials MUST be a static access key and secret key, stored as secrets.
    Session tokens, assumed roles, and credentials provided by the host environment are not
    accepted. Because static keys don't expire, the automatic refresh of FR-084 doesn't apply to
    them. A key the service rejects MUST fail the connection's uses visibly, with an alert, like
    any connection failure.
  - A connection of a family that only one connector serves is defined by that connector's
    configuration schema and is tied to it.
- **FR-115**: The appliance MUST keep a table of connector routes. Each provider family and use
  routes to exactly one enabled connector that declares the capability the use needs for that
  family. The uses are:
  - **FileDock-owned object storage**: the appliance-state remote copy, ownership claims and
    takeovers, Tiered Storage objects, and the FileDock metadata and manifests written to owned
    locations. Connection tests and browsing follow this route.
  - **Hybrid Backup repository**: may also name the backup engine's own access (FR-116).
  - **Cloud Mount**.

  The default routes are:

  | Use | S3-compatible | Azure Blob | Google Drive, OneDrive, Dropbox, and others |
  |---|---|---|---|
  | FileDock-owned object storage | Native S3 connector | General cloud connector | Not allowed (FR-088) |
  | Hybrid Backup repository | Backup engine's own access | Backup engine's own access if it supports the family, otherwise the general cloud connector | Not allowed (FR-088) |
  | Cloud Mount | General cloud connector | General cloud connector | General cloud connector |

  Administrators MAY change any route in extension management. A route MUST NOT point to a
  connector that is disabled, or that doesn't declare the family and capability.

  With these defaults, Azure Blob owned locations have basic claim safety (FR-123), because the
  general cloud connector doesn't do conditional writes for them. They are accepted after the
  acknowledgement and stay labelled until a connector with conditional writes serves them.

  These are the family defaults. A connection MAY override the route for any of its uses, for that
  connection only. For example, a single S3-compatible service that misbehaves with the native
  connector can go to the general connector while every other S3-compatible connection stays
  native.
  - An override follows the same rules as a family route, including FR-118 to FR-121.
  - Connection lists and the route table MUST show every connection that overrides its family
    default, and which connector it uses instead.
  - A connection without an override follows later changes to its family default.
  - Removing an override returns the connection to the family default, as a route change. A new family that a
  newly installed connector serves gets routes only when the administrator assigns them.
- **FR-116**: For the Hybrid Backup repository use, a route MAY name the backup engine's own access
  to a provider family instead of a connector. The backup engine then receives that connection's
  credentials directly, as a declared secret. This is the default for S3-compatible storage. Feature
  002 defines the repository behavior.
- **FR-117**: The native S3 connector MUST provide correctness and control:
  - It MUST work with any S3-compatible service through a configurable endpoint, region, and
    addressing style. Examples include AWS S3, Wasabi, Backblaze B2, Cloudflare R2, and MinIO.
  - It MUST use conditional writes for ownership claims and takeovers when the service supports
    them: create only if absent, and replace only if unchanged since read. The connection test MUST
    detect support and show it. Without support, claims MUST fall back to write-then-verify, as
    for any connector without conditional writes (FR-123).
  - It MUST verify the integrity of every object it writes and reads. A mismatch MUST fail the
    operation, and it MUST never be reported as success.
  - It MUST NOT leave partial objects visible. Interrupted multi-part uploads MUST be aborted and
    cleaned up.
  - Connection tests, daily probes, and upload cleanup MUST NOT write to or delete from an owned
    location unless this appliance is its Writer (Principle V). Probes write only under the
    appliance's own probe key, outside every owned location. Where that isn't possible, the probe
    runs read-only and conditional-write support is recorded as not tested.
  - It MUST store each FileDock object as exactly one object under the location's prefix, at a
    predictable key, without caching or transforming object data.
  - Per connection, the administrator MUST be able to set the storage class, the server-side
    encryption setting, and a bandwidth limit.
- **FR-118**: Changing a route, whether a family default or a connection override, MUST:
  - require a confirmation that lists the affected connections, folders, and owned locations, and
    states any protection the new connector lacks, such as conditional writes. A change that would
    leave an owned location without conditional writes also needs the acknowledgement of FR-123;
  - take effect at the next write session. Running sessions finish on the previous connector.
    Cloud mounts restart on the new connector, and the confirmation MUST say that access is
    briefly interrupted;
  - leave the data at every location unchanged. Connectors serving the same family MUST use the
    same object layout, so data written through one is readable through the other;
  - be audited.
- **FR-119**: A connector MUST receive the credentials of the connections routed to it, and no
  others. Routing a use to a third-party connector MUST show, before confirmation, every connection
  whose credentials it will receive.
  - Routing FileDock-owned object storage to a third-party connector, as a family default or a
    connection override, MUST also require a separate acknowledgement. It MUST state that this
    connector will hold the credentials for, and make the ownership decisions at, locations that
    appliance recovery depends on. The acknowledgement MUST be audited.
  - Every owned location served by a third-party connector MUST show this in its ownership view
    (FR-094) and in the dashboard's protection status.
  - The appliance MUST NOT route owned object storage to a third-party connector by any other path.
    This includes restore: a restored route of this kind is applied only because it was
    acknowledged before the snapshot.
- **FR-120**: When the connector on a route is unavailable, disabled, or Failed, the uses on that
  route MUST fail visibly, with an alert naming the connector and the route. The appliance MUST NOT
  switch to another connector automatically.
- **FR-121**: A connector MUST NOT be disabled or removed while any route points to it. Installing a
  connector extension MUST NOT change any route.
- **FR-122**: Enabled connectors and routes are configuration, so they are part of appliance state
  (FR-036) and are restored without administrator input.
  - A remote destination entered by hand at restore MUST be read through the built-in connector for
    its family.
  - When a restored route names an extension connector, the connector MUST be reinstalled from its
    stored package (FR-110) before the route is applied. Until then, the built-in connector for the
    family is used only to read.
- **FR-123**: Every owned location MUST have a claim safety: **conditional** when the service and the
  connector routed for it support conditional writes, otherwise **basic** (write-then-verify).
  - Accepting a location with basic claim safety, or making an existing location basic through a
    route change, MUST require an acknowledgement. It MUST state that two installations claiming
    the location at the same moment could both act as owner until the next ownership check. The
    acknowledgement MUST be audited.
  - Basic locations MUST be labelled "basic claim safety" in the ownership view (FR-094). The
    dashboard MUST show a warning while any owned location is basic. The warning clears by itself
    once the location is served with conditional writes, for example after a route change or a
    service upgrade detected at the next connection test.
  - Basic claim safety MUST NOT relax the check before each write session (FR-091).

**Management UI visual design and presentation**

The approved [UI design guide](./ui-design-guide.md), including its retained reference image,
is normative for presentation. Functional behavior remains governed by the requirements above.

- **FR-124**: Every management screen MUST follow the guide's light theme, FileDock identity,
  type hierarchy, color roles, a 4 px spacing grid with 8/16/24/32 px major spacing, rounded white cards, and restrained borders
  and shadows. The design MUST remain usable offline, including icons and a readable typeface.
- **FR-125**: Authenticated desktop screens MUST use a shared left navigation sidebar, top bar,
  and main content area. Navigation MUST show the active destination, preserve the guide's
  icon-plus-label hierarchy, and expose implemented areas through the specified destination
  labels. Exactly one current destination MUST be selected. Settings subpages MUST nest in the
  same sidebar; expansion MUST NOT select the parent. There MUST NOT be a second sidebar or
  duplicate in-content Settings navigation. Setup and sign-in MUST use the same visual language without exposing authenticated
  information or navigation before access is granted.
- **FR-126**: Navigation, search, alert controls, links, and actions MUST lead to real supported
  behavior. An implemented area with no resources MUST remain accessible with an empty state;
  an unavailable feature MUST be omitted or explicitly unavailable without an enabled action.
  The guide MUST NOT add features from the sample image to this release's functional scope.
- **FR-127**: The dashboard MUST use compact summary cards followed by related content panels,
  reflecting the guide's hierarchy. It MUST retain Core, Management Service, container runtime,
  and built-in subsystem health. Folder/pool lists, connection counts, protection, recent activity,
  cache/resource charts, and container summaries MUST appear only when corresponding functionality
  and actual information are available; missing data MUST be explicitly unavailable or omitted.
- **FR-128**: Displayed counts, names, paths, capacity bars, timestamps, statuses, and trends MUST
  come from actual appliance information, with clear scope and units. Unknown, loading, stale,
  empty, stopped, and failed states MUST be distinguishable. Missing history MUST NOT be invented,
  and missing measurements MUST NOT be converted to zero or Healthy.
- **FR-129**: The redesigned dashboard and detail screens MUST preserve the existing exposure,
  recovery-point, claim-safety, notification, and Core connection warnings. Summary success MUST
  identify what it covers. A storage-mode badge MUST NOT imply backup or protection unsupported by
  the resource's recorded state. FR-019 change blocking and live-status behavior MUST be retained.
- **FR-130**: Buttons, fields, selects, switches, tabs, dialogs, tables, cards, and status badges
  MUST follow the shared component rules in the guide, including primary/secondary/danger roles
  and visible focus, busy, disabled, error, and saved states. Destructive confirmation, secret
  masking/one-time display, and the existing form validation rules MUST be preserved. Short
  account/settings actions MUST open focused dialogs rather than permanent forms on overview
  pages. Tables MUST align headers/cells and use compact accessible row actions. Long editors
  MUST have enough space on dedicated pages.
- **FR-131**: Layout MUST adapt at 360 px-wide viewports and at 200% zoom on a 1280 px-wide viewport.
  Navigation MUST remain accessible, cards MUST wrap and panels stack in reading order, and all
  essential controls and warnings MUST remain visible. Only table/chart regions may require
  horizontal scrolling; the main page MUST NOT.
- **FR-132**: Essential UI actions MUST be keyboard-accessible with visible focus and accessible
  names. Status meaning MUST include text and an icon or shape. Ordinary text MUST have at least
  4.5:1 contrast, large text at least 3:1, and control/focus indicators at least 3:1 against adjacent
  colors. Touch targets MUST be at least 44 by 44 px; optional motion MUST respect reduced-motion
  preferences, and charts MUST include textual value summaries.
- **FR-133**: Search, when available, MUST find supported resources and settings and navigate to
  real results, without implying search of user file contents. The account menu MUST identify the
  signed-in administrator and keep sign-out accessible. Alert controls MUST show actual alert
  information; they MUST NOT imply built-in external notifications.
- **FR-134**: Setup, sign-in, recovery, administrator/token management, network settings, and every
  later feature screen MUST receive the shared visual treatment without changing their authorized
  operations, required inputs, flow order, confirmations, or security boundaries. Information
  refreshed live MUST preserve the user's entered values, focus, and useful visible context.
- **FR-135**: Visual acceptance MUST compare every implemented screen and shared component against
  the guide at desktop and narrow widths and in healthy, loading, empty, failed, and unavailable
  states. Existing completion of service or UI tasks MUST NOT be treated as evidence that this
  new design revision has been implemented or accepted.

### Key Entities

- **Appliance**: The installed FileDock instance. It has a name, release version, health, its
  local appliance-state copies, and an optional remote appliance-state destination.
- **Administrator**: A person allowed to manage the appliance. It has credentials, enabled state,
  and sessions.
- **Session**: An authenticated administrator session with creation time and idle expiry.
- **API Token**: A named credential for non-interactive API use. It has a scope (Read-only or
  Full), its creator, creation time, optional expiry, last use, and a revoked state. It is stored as
  a secret.
- **Secret**: An encrypted credential referenced by connections, subsystems, shares, or applications.
  It is never readable in plaintext through the UI.
- **Recovery Key**: A portable key held by the administrator. It is required to decrypt any
  appliance-state snapshot. When a local copy survives, it is the only input a reinstall needs.
- **Appliance-State Snapshot**: An encrypted, versioned, integrity-checked bundle of the
  configuration needed to rebuild the appliance: a versioned configuration document and the secret
  store, with no history. It is written to every appliance-state copy.
- **Appliance-State Copy**: A place where snapshots are kept. It is either a local copy, on a data
  pool or the system disk, or the remote destination. It has a last successful snapshot time and a
  stale flag. It also holds one stored package per installed extension version.
- **Desired State**: The administrator's declared configuration. The Core Service reconciles the
  machine to it.
- **Operation**: A journaled, resumable state machine for a critical action. It has type, current
  step, progress, and result.
- **Event**: A typed status notification from the Core Service. It has source, type, time, and
  details.
- **Subsystem**: A provider of capabilities. It is described by a manifest and operated through the
  generic contract. Its class is core, built-in, or extension.
- **Manifest**: A subsystem's declaration of its identity, version, class, capabilities, runtime,
  health check, configuration schema, permissions, secrets, persistent data paths, and
  compatibility.
- **Data Binding**: The link from a subsystem's or extension's declared data path to a managed
  folder. It is configuration and is restored with appliance state.
- **Capability**: A named function, such as `protocol.smb`, that managed folders request and
  subsystems provide.
- **Managed Folder**: The main user-facing storage object. It has:
  - a name;
  - a root definition: FileDock path, storage mode, and reference path, plus any backend the mode
    needs. The mode covers the whole subtree.
  - a data importance, with optional per-subfolder overrides (Hybrid Backup only);
  - access (SMB, NFS, WebDAV, Docker) with access rules;
  - advanced policies (cache, backup, retention, tiering, consistency);
  - application bindings;
  - a protection state.
- **Storage Mode**: Where and how a folder's data lives: Local Only, Cloud Mount, Hybrid Backup, or
  Tiered Storage. It is set once per managed folder and applies to every subfolder. It determines
  the reference path and backend inputs and the relevant advanced settings.
- **Alert**: A condition that needs the administrator's attention. It has severity, source, affected
  object, first-seen and last-seen times, and state (Active, Acknowledged, or Resolved). It is shown
  in the web UI and offered to any registered notification channels.
- **Owned Location**: A cloud object storage location (a bucket or container plus a prefix) that
  exactly one appliance writes to. It holds an ownership claim with the owning appliance and a
  generation. Each installation's role at the location is Writer, Recovery reader, or Ownership lost.
  Its claim safety is conditional or basic, depending on the service and the connector routed for
  it.
- **Physical Disk**: A disk discovered on the appliance, with its model, serial, size, and health.
  It belongs to at most one pool. The system disk backs only the System Pool.
- **Pool**: The durable local-storage abstraction. A data pool combines one or more physical disks
  RAID-0 style, so capacity adds up and there is no redundancy. It has a stable identity, a name,
  member disks, capacity, usage, health (Healthy, Degraded warning, or Failed), and the folders it
  holds. It is recorded in appliance state.
- **System Pool**: The one built-in pool on the system disk. It has the same identity and folder
  model as other pools. It also has two limits (maximum disk usage and minimum free space reserved
  for the system), an effective capacity, and a writable state. It can't be deleted, extended, or
  erased.
- **Reference Path**: Where a managed folder's data actually lives. This is a local path (a pool
  plus a path inside it), a remote path on a cloud connection, or both for Tiered Storage. Each sub-path of the FileDock path maps
  to the same relative sub-path under it.
- **Data Importance**: What FileDock may lose or rebuild for a Hybrid Backup folder or subfolder:
  Critical, Important, Rebuildable, or Temporary (Temporary on subfolders only). It maps one-to-one
  to an internal data class. Other storage modes have no importance.
- **Internal Data Class**: One of authoritative critical state, authoritative user data,
  rebuildable, or disposable. It drives protection defaults, alert severity, restore order, and
  whether the appliance may delete data automatically.
- **Cloud Connection**: A configured remote storage account. It has a provider family, and its
  credentials are stored as secrets. S3-compatible connections are defined independently of any
  connector. Connections of families that only one connector serves are tied to it.
- **Cloud Connector**: A subsystem that gives FileDock access to cloud storage. Its manifest
  declares the capabilities (`cloud.remote`, `cloud.mount`) and provider families it serves. The
  built-in connectors are the native S3 connector and the general cloud connector. Others can be
  installed as extensions.
- **Provider Family**: A group of cloud services with the same access model: S3-compatible, Azure
  Blob, Google Drive, OneDrive, Dropbox, or a family that only specific connectors serve.
- **Connector Route**: The assignment of one provider family and use to exactly one enabled
  connector, or, for a Hybrid Backup repository, to the backup engine's own access. It is either a
  family default, or an override for a single connection and use. It is configuration and is
  restored with appliance state.
- **Export**: A share of a managed folder over SMB, NFS, or WebDAV with access rules.
- **Container Application**: A container definition with environment, secrets, and managed-folder
  bindings.
- **Log Entry and Audit Entry**: An operational record, and a record of an administrative action.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An administrator starting from a fresh supported machine completes installation and
  first-run setup in under 30 minutes. The only command-line step is starting the installer.
- **SC-002**: After an unplanned reboot, every share, mount, and container that was healthy before
  returns to healthy within 3 minutes of the machine finishing boot, without administrator action.
- **SC-003**: While the Management Service is stopped or restarting, 100% of active share client
  sessions and running containers continue without interruption.
- **SC-004**: 95% of subsystem and operation status changes appear in an open management UI within 2
  seconds.
- **SC-005**: For each appliance-state copy that is available, 95% of committed changes to
  authoritative state are in a snapshot there within 1 minute, and 100% within 5 minutes.
- **SC-006**: In the disaster-recovery acceptance test, restoring from a destroyed appliance onto a
  blank machine rebuilds 100% of recorded administrators, connections, pools (by name and
  identity), folders, shares, schedules, policies, container definitions, and extension
  registrations and configuration. It brings back no history. It completes in under 60 minutes,
  not counting user-data transfer.
- **SC-007**: Planted canary secrets are found 0 times in plaintext across logs, events, audit
  history, diagnostic bundles, UI and API responses, and remote snapshots.
- **SC-008**: Killing a running extension 20 times in a row causes 0 Core Service restarts and 0
  interruptions to other subsystems.
- **SC-009**: At idle, the two first-party services together use under 512 MB of memory and under 2%
  of one CPU core on the minimum supported hardware.
- **SC-010**: Running the baseline failure-injection set corrupts 0 authoritative configuration or
  state records. The set restarts both services, the container runtime, and each subsystem, and cuts
  power during a configuration change.
- **SC-011**: An administrator can create a managed folder with default Advanced settings in under 2
  minutes.
- **SC-012**: 100% of Local Only, Cloud Mount, and Tiered Storage folders are created without being
  asked for a data importance. 100% of Hybrid Backup folders have one and receive its documented
  defaults.
- **SC-013**: Under sustained local disk pressure, the appliance automatically deletes 0 bytes of
  data other than Temporary subfolder content.
- **SC-014**: 100% of attempts to create a managed folder whose FileDock path or reference path
  overlaps another folder's are refused before anything is saved. 100% of subfolders report the
  same storage mode as their managed folder.
- **SC-015**: In takeover tests where the original appliance is still running, the original makes 0
  writes or deletions at the location after its next ownership check. 0 recovery points, appliance
  snapshots, or tier objects become unreadable.
- **SC-016**: A recovery installation can list and restore data from a location owned by another
  appliance while making 0 writes to it before takeover.
- **SC-017**: 0 screens or documents promise zero downtime or zero data loss. 100% of protection
  status views show the latest durable recovery point.
- **SC-018**: Removing one disk from a running multi-disk pool marks the pool Failed and raises a
  critical alert within 1 minute. The alert lists 100% of the affected folders, each with its
  recovery path.
- **SC-019**: In the disaster-recovery test on hardware with different disks, mapping each recorded
  pool to new disks brings back 100% of managed folders with their original definitions, and none
  has to be redefined.
- **SC-020**: In failure-injection tests, 100% of critical conditions appear as an active alert in
  the web UI within 1 minute. When a test notification extension is installed, 100% are offered to it
  within 1 minute.
- **SC-021**: On a machine with a data pool, reinstalling FileDock OS on an erased system disk and
  entering only the recovery key restores 100% of the items listed in SC-006. Every folder in the
  reattached pool serves its data unchanged. The administrator provides no other input, and the
  restore completes in under 30 minutes after the installer starts.
- **SC-022**: In fill tests, writes to the System Pool never take system disk free space below the
  configured reserve, and never take System Pool usage above the configured maximum. 0 bytes of
  existing data are deleted, and the operating system and both services keep running.
- **SC-023**: On a fresh install with no connector configuration, 100% of appliance-state uploads,
  ownership checks, and claim writes to an S3-compatible destination go through the native S3
  connector. 100% of operations on Google Drive, OneDrive, and Dropbox connections go through the
  general cloud connector.
- **SC-024**: Switching the S3-compatible route between the two built-in connectors and back, 10
  times while snapshots are being taken, rewrites or loses 0 objects. 100% of snapshots at the
  location stay readable.
- **SC-025**: Against a service that supports conditional writes, two installations claiming the
  same empty location at the same moment through the native S3 connector produce exactly one owner
  in 100% of 100 runs.
- **SC-026**: In fault-injection tests that truncate or corrupt objects in transit, 0 corrupted
  objects are reported as successfully written or read through the native S3 connector.

- **SC-027**: In visual acceptance, 100% of implemented management screens conform to the guide's
  shared shell where applicable, typography hierarchy, palette roles, spacing, card treatment,
  controls, and status presentation. Any intentional functional adaptation is identified in the
  review and consistent with FR-126–FR-129.
- **SC-028**: Across healthy, failed, unknown, empty, stale, and unsupported-feature scenarios,
  100% of displayed values and enabled actions correspond to actual available data and supported
  behavior, with 0 illustrative values or false protection/health claims.
- **SC-029**: At 360 px-wide viewports and at 200% zoom on a 1280 px-wide viewport, administrators
  can reach 100% of essential navigation/actions and read protection/error notices without main-page
  horizontal scrolling. Keyboard-only review completes setup, sign-in, and existing management
  flows with visible focus; required contrast and touch-target thresholds are met.
- **SC-030**: With internet access disconnected, setup, sign-in, navigation, and implemented
  management screens retain readable typography, icons, and functional controls. The existing
  live-update and Core-unavailable acceptance criteria continue to pass after the visual revision.
- **SC-031**: In a review with five first-time administrators, at least four can identify a failed
  service, locate administrator management, and find sign-out within two minutes without assistance.

## Assumptions

- **UI reference precedence**: The user-supplied image is the visual authority; this specification
  remains the functional authority. Its sample information and decorative documentation panels
  are not dashboard data or new product requirements. The light theme is the default; a separate
  dark theme is not required by this revision.
- **UI delivery during ongoing implementation**: The new design applies to existing screens and
  to later screens as they become available. Completed Phase 1–5 tasks retain their implementation
  history, but their previous acceptance does not certify FR-124–FR-135. The plan and UI task
  coverage need updating before the redesign is implemented. Backend features remain in their
  existing stories/phases; the image does not pull later-feature functionality into the P1 delivery.

- **Target users**: Home-lab users, small offices, and technical administrators who manage one
  appliance. Every administrator has full rights. Role-based access, single sign-on, directory
  integration, and multi-factor authentication are out of scope for this feature.
- **Installation scope**: The appliance installs onto a fresh install of the supported base OS. This
  is Stage 1/2 in the product guide. A bootable appliance image or ISO is out of scope.
- **Supported platform**: 64-bit x86 machines and VMs, with at least 2 GB of memory (4 GB
  recommended) and 16 GB of system disk. Container-based installs such as LXC and other CPU
  architectures are out of scope. A system disk that small installs, but its System Pool is
  Unavailable under the default 100 GB reserve. The System Pool needs a system disk larger than the
  reserve plus the operating system.
- **Appliance updates**: Whole-appliance version updates are out of scope. Pinned dependency
  versions are set at install time. Extension upgrades with automatic rollback are in scope.
- **Pools have no redundancy**: Pools are RAID-0 style by design, which fits Principle V (accept
  data loss, but keep it bounded and visible). Mirroring, parity, and RAID management are out of
  scope. Whether a pool stripes or concatenates its disks is a planning decision, as long as
  capacity adds up and the loss of any disk loses the pool.
- **No scale limits in this release**: No maximum number of pools, disks, managed folders, exports,
  containers, extensions, or files is set. The design avoids hard-coded caps, and limits may be
  added after measurement.
- **No local disk encryption at rest**: Pools and the system disk are not encrypted at rest in this
  release. Cloud-bound data, secrets, and appliance-state snapshots are still encrypted. Local disk
  encryption may be added later, so pool creation and key handling must leave room for an
  encryption layer.
- **System Pool limits**: Both limits apply at once, and the stricter one wins. For example, a
  1 TB system disk with 40 GB used by the system gives at most 500 GB under the 50% limit and at
  most 860 GB under the 100 GB reserve, so the System Pool gets 500 GB. A 160 GB disk with the same
  system usage gives at most 80 GB and 20 GB, so it gets 20 GB. The System Pool shares the system
  disk's performance with the operating system. It suits small folders and folders whose data is
  protected by their storage mode. Large data belongs in data pools.
- **Recovery key only, on the same machine**: The promise that a reinstall needs only the recovery
  key depends on a local copy that survives the reinstall. A copy on a data pool survives any
  reinstall of the system disk. The system disk copy survives a reinstall of FileDock that keeps
  the operating system's disk. After the loss of the whole machine, or of every disk holding a copy,
  restore also needs the remote destination. That is why the dashboard warns until one is
  registered.
- **Pool root is reserved**: Folders use directories inside a pool, never the pool root. Data at the
  root of an adopted filesystem stays where it is, and the administrator creates folders for its
  top-level directories.
- **Network exposure**: The management UI is for the local network. Exposing it to the internet,
  for example through port forwarding, is unsupported and not hardened for. Remote access uses the
  administrator's VPN or an extension.
- **Notifications**: Built-in email, webhook, and push delivery are out of scope. They come from
  extensions that provide `notify.channel`. Without one, an administrator who does not check the web
  UI can miss alerts. The dashboard notice states this.
- **Extension sources**: Extensions install from a local package file. A marketplace or catalog is
  out of scope.
- **Static S3 keys only**: Temporary and role-based S3 credentials (session tokens, assumed roles,
  and instance or environment credentials) are out of scope for this release. Administrators are
  advised to create a dedicated key limited to FileDock's buckets and prefixes.
- **Native connector scope**: The native S3 connector covers only the S3-compatible family. Azure
  Blob uses the general cloud connector, so its owned locations have basic claim safety and a
  standing dashboard warning. A native connector for Azure Blob or another family can be added
  later as another connector, routed the same way, and would clear that warning.
- **Claim safety still depends on the provider**: Conditional writes make simultaneous claims of an
  empty location safe. They do not make an ownership claim a lock (see "Ownership is a claim, not a
  lock").
- **Share users**: Share users who access SMB, NFS, and WebDAV exports are managed separately from
  administrators. They cannot sign in to the management UI.
- **Lost recovery key**: If the recovery key is lost after hardware loss, the appliance state cannot
  be recovered. This is stated to the administrator at key creation.
- **Dependent features**: Features 002 (Hybrid Backup), 003 (Hybrid Tiered Storage), and 004 (Cloud
  Mount) depend on this feature. They depend on managed folders, cloud connections, secrets, the
  operation journal, events, and the capability registry. Each adds its storage mode to the Create
  Managed Folder flow and its settings to Advanced.
- **Replaces the product guide's durability classes**: The guide's section 50 classes are replaced
  by Storage Mode, plus Data Importance inside Hybrid Backup:
  - Cloud Primary becomes Cloud Mount mode.
  - Backed Up becomes Hybrid Backup mode.
  - Tiered becomes Tiered Storage mode.
  - Disposable becomes Temporary importance on Hybrid Backup subfolders.

  `FileDock-OS.md` should be updated to match.
- **Single node, not high availability**: FileDock is single-node, single-writer hybrid storage,
  optimized for low cost and recoverability. These are explicit non-goals:
  - a clustered controller;
  - active-active FileDock;
  - a distributed lock manager;
  - a shared writable filesystem;
  - load-balanced storage nodes;
  - a consensus protocol;
  - a multi-writer cloud tier.
- **Ownership is a claim, not a lock**: The ownership claim and the check before each write session
  stop a former owner at its next check. They do not provide mutual exclusion from moment to moment.
  The administrator confirms that the original appliance is gone or stopped before taking over. That
  is why takeover warns when the owner wrote recently.
- **Bucket access controls**: Administrators are advised to give FileDock credentials that only
  FileDock uses, and to keep other systems from writing to FileDock's bucket or prefix. FileDock
  does not enforce provider-side policies.
- **One mode per managed folder**: Mode is decided only at the managed folder's root, and managed
  folders never nest. This keeps every file under exactly one mode, so placement, protection, and
  restore never conflict. Content that needs a different mode becomes its own managed folder at a
  separate FileDock path and reference path.
- **Reference path changes**: Moving a folder's data to a different local reference path, and
  converting a folder into or out of Cloud Mount, are out of scope for this release. Both would
  require relocating data. The administrator creates a new folder instead.
- **Planning constraints from the product guide**: These are not requirements of this spec, but they
  are carried into planning:
  - Debian Stable as the base OS;
  - systemd for supervision;
  - a .NET (ASP.NET Core) Management Service with its own SQLite database;
  - a Rust Core Service with its own SQLite database;
  - gRPC over a Unix domain socket for service-to-service communication;
  - SignalR or WebSocket for browser updates;
  - Docker Engine with Compose support;
  - Samba, Linux kernel NFS, rclone (the general cloud connector and WebDAV), and Kopia (the backup
    engine, using its own S3 backend for S3-compatible repositories) as the built-ins;
  - an official native S3 connector, built as a separate subsystem with its own manifest. The
    earlier objection to a native object storage library such as `object_store`, that it adds a
    second cloud stack beside rclone, no longer applies, because connectors are pluggable by design.
    The library is a planning decision;
  - ext4 for local storage.
