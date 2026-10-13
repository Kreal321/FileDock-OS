# Specification Quality Checklist: Core Appliance Foundation and OS Setup

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-10-11
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

- The user asked for a .NET Management Service, a Rust Core Service, and Docker. These, along with
  the product guide's technology choices, appear only in Assumptions under "Planning constraints".
  Requirements describe the two services by role (unprivileged Management Service and privileged
  Core Service).
- Protocol names (SMB, NFS, WebDAV) and capability identifiers (such as `protocol.smb`) are
  user-visible product concepts, not implementation choices.
- Defaults with no clarification needed are recorded in the spec. They include the password policy
  (non-empty password, advisory length over 8 characters, lockout after 5 failures for 15 minutes), a 30-minute idle session, 90-day audit
  retention, a 1 GB log cap, and snapshot retention (30 most recent plus 30 daily).
- Revision 2026-10-11 (re-validated, all items still pass):
  - The Create Managed Folder flow (User Story 8, FR-062 to FR-074) replaces the four durability
    classes with two independent settings: Storage Mode and Data Importance.
  - The four importances map to four internal classes (FR-067).
  - The compatibility rules (FR-068) and importance-driven defaults (FR-069) were chosen without
    asking. Temporary is Local Only. Critical is never Local Only. Important with Local Only needs
    acknowledgement. Only Temporary is ever deleted automatically.
  - Old FR-064 to FR-069 are renumbered FR-075 to FR-080.
- Revision 2026-10-11 (backup scope, re-validated):
  - Backup is offered only in Hybrid Backup (FR-071).
  - FR-068 is now a mode-by-importance matrix. Critical is refused for Local Only and Tiered
    Storage. Important needs acknowledgement in both. Temporary is Local Only, or a Hybrid Backup
    subfolder.
  - FR-069: Rebuildable defaults to no backup (suggested), with opt-in backup.
  - Per-subfolder importance in Hybrid Backup is defined in feature 002.
- Revision 2026-10-11 (importance scope, re-validated, all items pass):
  - Data Importance is shown and stored only for Hybrid Backup (FR-062, FR-066, FR-068). The
    mode-by-importance matrix and the acknowledgement step were removed.
  - Local Only and Tiered Storage show a non-blocking notice that local-only data has no
    off-appliance copy.
  - Only Temporary subfolder content can be deleted automatically.
  - FR-074 now orders disaster recovery across all modes.
  - SC-011 to SC-013 were updated to match.
- Revision 2026-10-11 (mode per folder and reference path, re-validated, all items pass):
  - The storage mode is chosen together with the FileDock path and the new **Reference Path**, which
    is where the data actually lives. The three form the folder's root definition (FR-065).
  - A mode covers the whole subtree, and subfolders can never have their own mode (FR-066).
  - Reference paths cannot overlap across folders or modes. Cloud Mount remote paths may overlap
    only with each other (FR-067).
  - Subfolders use the parent's settings. The only override is Data Importance, and only in Hybrid
    Backup (FR-068).
  - Mode changes apply to the whole folder. Moving the local reference path, and converting into or
    out of Cloud Mount, are out of scope (FR-069 and Assumptions).
  - Four FRs were inserted, so old FR-066 to FR-080 became FR-070 to FR-084. FR numbers in the
    earlier revision notes above use the old numbering.
  - US8 gained scenarios 5–7 and 14, and the list was renumbered to 18 scenarios. Four edge cases,
    a Reference Path entity, and SC-014 were added.
- Revision 2026-10-11 (single-node, single-writer scope, re-validated, all items pass):
  - The Overview and Assumptions now state that FileDock is single-node, single-writer hybrid
    storage, optimized for low cost and recoverability, not high availability.
  - Explicit non-goals: clustered controller, active-active FileDock, distributed lock manager,
    shared writable filesystem, load-balanced storage nodes, consensus protocol, and multi-writer
    cloud tier.
  - New FR-085 to FR-092:
    - Hybrid Backup and Tiered Storage use single-writer cloud object storage (S3-compatible or
      Azure Blob).
    - Writes flow one way, and each location holds one ownership claim with a generation.
    - The appliance checks the claim before each write session and stops writing if it is no
      longer the owner.
    - The disaster-recovery exception: read-only access as a Recovery reader, then a confirmed and
      audited takeover.
    - External modifications are reported, and each location's role is shown.
    - The UI and documentation make no promise of zero downtime or zero data loss.
  - FR-045 now refers to the takeover rule. The importance table no longer says "Never lose it" and
    states target recovery points instead.
  - US5 gained scenarios 6–7. Five edge cases, an Owned Location entity, and SC-015 to SC-017 were
    added.
  - Judgement call: the single-writer rule also covers an appliance-state destination stored in
    cloud object storage. A local or external disk remains allowed for appliance state.
- Revision 2026-10-11 (System Pool and recovery-key-only reinstall, re-validated, all items pass):
  - New FR-098 to FR-103 define the System Pool:
    - It is built in on the system disk.
    - Maximum disk usage defaults to 50% (range 10–90%). The minimum free space reserved for the
      system defaults to 100 GB (at least 20 GB). The stricter limit wins.
    - Both limits are enforced: writes are refused, nothing is deleted, and alerts are raised.
    - It can't be deleted, renamed, erased, or extended.
    - Folders in it get a warning when created.
    - It is recreated on restore without mapping.
  - New FR-104 to FR-108 define the local copies and the reinstall:
    - Local appliance-state copies are kept automatically on every data pool and on the system
      disk.
    - The dashboard shows what each kind of loss would need.
    - A reinstall restore asks only for the recovery key: no setup code, credentials, or mapping of
      present disks.
    - Restore uses the newest valid snapshot across all copies.
    - After a reinstall restore, takeover is automatic, unless the location was written within the
      last 15 minutes or was taken over after the snapshot.
  - Changed in place:
    - FR-020: the recovery-key path is an exception to the setup code.
    - FR-036: the snapshot holds both service databases and the secret store, without volatile
      data.
    - FR-037 to FR-042, FR-045, and FR-092: references to the new FRs.
    - FR-038: a 5-minute recovery point at every copy.
    - FR-039: hourly retention added.
    - FR-061: the pool root is reserved.
    - FR-062: the system disk is used only by the System Pool.
    - FR-063: present pools are reattached automatically.
    - FR-081: reattached pools need no data restore.
  - The new FRs were appended at the end, so existing numbers, and the references from 002 and 003,
    are unchanged.
  - US4 gained scenarios 7–9. US5 now covers reinstall, with scenarios 8–11. US9 gained scenarios
    9–13. Eight edge cases, the System Pool and Appliance-State Copy entities, SC-021, SC-022, and
    three assumptions were added. SC-005 now covers every copy.
  - Judgement calls:
    - The two limits are read as "the System Pool may use at most this share of the disk" and "the
      System Pool never uses the last N GB".
    - "Only the recovery key" holds while a local copy survives. Loss of the whole machine still
      needs the remote destination.
- Revision 2026-10-11 (cloud connectors and routing, re-validated, all items pass):
  - New FR-112 to FR-122:
    - Cloud access goes through pluggable connectors, which are subsystems declaring capabilities
      and provider families.
    - Two built-ins ship: the native S3 connector and the general cloud connector.
    - Connections carry a provider family. S3-compatible connections are connector-neutral.
    - A route table maps each family and use (FileDock-owned object storage, Hybrid Backup
      repository, Cloud Mount) to one connector, or to the backup engine's own access for
      repositories.
    - The native connector's correctness rules: conditional claims, verified integrity, no partial
      objects, a predictable layout, and explicit storage settings.
    - Route changes need confirmation, are audited, and leave data unchanged.
    - Credentials go only to the routed connector, with no automatic fallback.
    - Routes are part of appliance state and are restored.
  - Changed in place:
    - FR-003: both connectors are built-ins.
    - FR-036: connectors and routes are in the configuration document.
    - FR-051: cloud capabilities resolve through routes.
    - FR-083: connections cover any family an enabled connector serves.
  - Added:
    - a header revision note and an Overview routing table;
    - US10 with 8 scenarios;
    - 5 edge cases;
    - the Cloud Connector, Provider Family, and Connector Route entities, and an updated Cloud
      Connection;
    - SC-023 to SC-026;
    - two assumptions, and planning constraints naming rclone, Kopia's S3 backend, and the
      `object_store` reconsideration.
  - Requirements name connectors by role: "native S3 connector" and "general cloud connector".
    Product names appear only in the header note and the planning constraints. S3 and the
    provider names are user-visible provider families, not implementation choices.
  - Judgement calls, all without clarification markers:
    - Azure Blob stays on the general connector, because the native connector is S3-only.
    - Routes never fail over to another connector automatically.
    - Installing a connector changes no routes.
    - Cloud Mount on S3 stays on the general connector, because the native connector provides no
      mounts.
  - Plan artifacts (research R12 and R13, data model, contracts) predate this revision. Re-run
    `/speckit-plan`.
- Revision after `/speckit-analyze`: FR-117 now forbids connection tests, probes, and upload
  cleanup from writing to or deleting from an owned location unless this appliance is its Writer
  (Principle V). Research R23 and the extension protocol were updated to match.
- Out of scope: role-based access, single sign-on, multi-factor authentication, ISO images, appliance
  updates, and an extension marketplace. These are candidates for `/speckit-clarify` if wanted.
