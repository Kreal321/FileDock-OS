# Specification Quality Checklist: Hybrid Backup Storage Mode

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

- The backup engine choice (Kopia) appears only in Assumptions as a planning constraint.
- Defaults chosen without asking:
  - a 15-minute schedule;
  - retention of all points for 24 hours, hourly for 7 days, and daily for 30 days;
  - At Risk when a folder is behind by more than twice its target RPO;
  - weekly verification;
  - a "keep both" restore conflict policy.
- Revision 2026-10-11 (re-validated, all items pass):
  - Hybrid Backup is the only mode that offers backup.
  - New User Story 2 and FR-007 to FR-014 add per-subfolder importance. Subfolders inherit their
    importance, and each importance in use forms a protection group with its own schedule.
  - Defaults: Critical every 5 minutes (capped at 15), Important every 15 minutes, Rebuildable no
    backup (suggested) with hourly as an opt-in, and Temporary never backed up.
  - Temporary is allowed only on subfolders, not on the whole folder.
  - User stories 2 to 6 became 3 to 7, and the FRs were renumbered sequentially.
  - Hybrid Backup is also the only mode with a Data Importance (FR-001).
- Revision 2026-10-11 (subfolders follow the folder, re-validated, all items pass):
  - All subfolders are in Hybrid Backup mode. Subfolder importance is an optional override, and
    without one a subfolder uses the parent setting. Removing an override returns it to the parent
    setting (FR-007, US2 scenarios 8–9).
  - Importance, plus backup on/off for Rebuildable, is the only per-subfolder setting. Mode,
    reference path, destination, and Access always come from the folder.
  - An edge case was added for data moved in from a folder in another mode.
- Revision 2026-10-11 (single-writer destination, re-validated, all items pass):
  - Destinations are single-writer cloud object storage only: S3-compatible or Azure Blob, given as a
    bucket or container plus a prefix (FR-002, FR-046).
  - Judgement call: WebDAV, SFTP, consumer drives, and local or removable disks were removed as
    destinations.
  - A Recovery reader can restore without writing before takeover (FR-047).
  - External modifications mark recovery points damaged (FR-048).
  - When ownership is lost, backup stops and the folder shows At Risk (FR-049).
  - The Overview and Assumptions say that zero data loss is not promised.
  - Two edge cases and SC-012 were added.
- Depends on feature 001 (`specs/001-core-os-foundation`).
