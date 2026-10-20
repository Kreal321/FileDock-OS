# Remaining features and testing — feature 001

Updated 2026-10-19 during Phase 13. This is the consolidated gap register requested by the
user. A written test, collected case, simulated result, or larger-VM measurement is not a
passed fresh-machine acceptance test. Credentials and current VM access details stay in
ignored `AGENTS.local.md`.

## Release acceptance status

Feature 001 is **not fully accepted**. Phase 13 implementation is published; focused installed-VM validation is recorded in
[the Phase 13 report](phase13-pve-testing-2026-10-19.md); T184 (the complete quickstart on fresh VMs) remains open. Earlier unchecked tasks
also remain open. Detailed evidence belongs in the dated Phase 13 report and
`checklists/validation.md`; this file identifies what still needs doing.

## 1. Features or UI behavior still incomplete

| Area | Missing behavior / limit | Follow-up |
| --- | --- | --- |
| Summary-first recovery UI | State protection still contains permanent connection/destination forms. Dedicated editors, dirty navigation and deliberate dialogs remain planned. Phase 13 adds truthful recovery timestamps, not that larger redesign. | T252–T255 |
| Extension inventory/package pages | Complete inventory-first layout, routed package review, compact actions and draft preservation. | T256–T257 |
| Operations/notifications/observability layouts | Finish shared table, toolbar, dialog and action migrations across channels, activity, audit and operations. Existing log message truncation/details already works. | T258–T260 |
| Folder editor routing | Complete dedicated create/edit pages, six expanded sections, generated paths, draft/refetch/outage behavior and compact list actions. Current form exists; the planned page migration is unfinished. | T261–T265 |
| Hybrid Backup data plane | Foundation advertises only available capabilities. `backup.restore` and `backup.retention` are absent; HybridBackup creation is refused. Backup-only live secret-materialization checks cannot pass until the provider exists. | Backup feature/provider, plus T177 |
| Cloud Mount / tiered data planes | CloudMount/cache and tier providers remain separate feature work (002–004 and associated providers). Preview/reconciliation requests are implemented; a real mount restart cannot be claimed without a mount provider. | Separate feature acceptance |
| Restore runtime completion | Configuration/file-key restore works in isolated tests, but full pool mapping, new-disk provisioning, extension package reinstallation, data rehydration and report outcomes still lack complete acceptance. Some restore tasks remain unchecked. | T106, T108, T114, T116–T117 |

## 2. Tests needing a disposable VM, disks or host control

| Test | Why not established on the current development VM | Required environment |
| --- | --- | --- |
| Complete quickstart / fresh offline install | Current guest already contains an installation. Upgrade/repair is not a fresh-install result. | Fresh Debian 13 amd64 VM and complete bundle; T184 |
| Minimum-hardware idle footprint / sustained idle (SC-009) | Release 44 passed a 120.5-second sample at 1.975% CPU / 132.8 MiB on the current roughly 4 GiB, two-CPU guest. CPU headroom is narrow; repeat/long-soak and actual 2 GiB minimum behavior remain unproven. | New `minimum_installed_vm` fixture: 2 GiB, two CPUs; T179 acceptance |
| Power loss during configuration changes (SC-010) | Guest root cannot cut hypervisor power. The acceptance suite deliberately refuses power-cut tests on an existing PVE guest. A clean reboot is not a power cut. | Disposable libvirt VM or explicit PVE host control; `test_failure_injection.py` |
| Pool create/adopt/grow/recreate and member loss | No disposable additional data disks are attached. Never target the OS disk. | Blank expendable disks plus an independently seeded filesystem for adoption; SC-018/019 |
| Reserve/hard-cap/full-disk fault injection | Filling or detaching the only development system disk would jeopardize the installation. | Separate disposable machine/disks; SC-013/022 |
| Strict two-installation conditional claims | 100 claims through two native processes against real MinIO passed; the specified two-VM topology is still untested. | Two independent installations, common disposable MinIO prefix; SC-025 / T177 |
| Writer fencing / live takeover / read-only recovery | Full two-appliance loss, generation, fencing and absence-of-writes matrix remains incomplete. | Two appliances, provider fixture and trace; SC-015/016 / T117 |
| Remote bootstrap and third-party restore | File-key configuration restore and stored package preservation passed with runtime disabled. Built-in S3 bootstrap followed by automatic third-party reinstall is not proven by that test. Two full runtimes on this guest share hardcoded pool-image/unit names. | Fresh disposable target VM or a properly isolated full-runtime harness; T177 |
| Sustained snapshot/copy failures and broad canary scan | Focused snapshots/canaries passed historically; every secret type and all logs/events/audit/diagnostics/API/remote bytes still need the full matrix. | Complete T102–T103 fixture suite with remote and failing copies |

## 3. UI and human acceptance still missing

- Approved, pinned Linux/Chromium visual baselines for all implemented screens and states
  (T230, T267). Current macOS captures are evidence, not portable golden baselines.
- Full real 1280 px / 200% browser zoom, 360 px reflow, keyboard/screen-reader names,
  contrast, nonoverlapping 44 px targets, reduced motion, dirty-form and live-refetch
  behavior across every page (T232, T266, T268–T270).
- Fresh offline release / empty browser cache verification, blocked external assets and
  combined P1 setup, sign-in, admin/token/network, recovery and outage flows (T233).
- Five first-time administrators and individual timing/assistance results. An automated
  browser or developer review cannot substitute for participants (T234, T271 / SC-031).
- Final UI coverage matrices and design-system reconciliation once those checks pass
  (T235, T272). Phase 13 only reviews the recovery-point additions at desktop/phone widths.

## 4. Provider and tool limits

- Real provider OAuth/consent and cloud interoperability still require disposable provider
  accounts/credentials. The Google Drive stub proves rclone routing, not real OAuth.
- Azurite, conditional-write MinIO, fault proxy, checksums, multipart cleanup, no fallback,
  native↔rclone reads and third-party acknowledgement/secret isolation have focused live
  evidence in `phase12-pve-testing-2026-10-19.md`; do not label these entirely untested.
- Kopia is currently an on-demand tool adapter, not a persistent daemon. Core restart and
  health re-adoption can be checked; killing a real backup/restore job needs its provider.
- The Linux-only `live_kernel_root_mount_matches_findmnt` storage unit test needs
  `cargo test -p fd-storage --lib` on Linux. The local macOS count excludes it; a Linux
  release build and installed monitor checks do not execute that unit test.
- `buf` is unavailable locally. JSON/schema contracts pass, but two protobuf lint/breaking
  checks are skipped. Install/provide buf and rerun the contract suite for that gate.
- Nice/idle scheduling is applied to Core and connector RC/native workers. Data-plane
  Samba/NFS/WebDAV/container units retain normal priority. Future providers that execute
  foreground hot reads inside those connector workers must separate scheduling before
  claiming foreground latency guarantees.

## 5. Historical defects already repaired

Do not reopen these merely because older reports still describe their reproduction:

- `/run/filedock` ownership/admin socket across installation, updates and reboot.
- Native connector cold-start ResetFailedUnit handling.
- Local-folder mount-unit path quoting/source validation and WebDAV cross-role directory
  freshness, with later repairs recorded in the project history/local access document.
- Native ETags, >4 MiB streamed reads, MinIO multipart listing/startup sweep, Azure null stat,
  empty extension configuration, and snapshot route-lock timeout (Phase 12 live fixes).

New failures or regressions must be recorded separately with the tested BUILD.txt commit.

## 6. Open task inventory

The following entries reproduce every task still unchecked after the Phase 13 implementation
checkpoint. They include unfinished acceptance as well as implementation work; an unchecked
box alone does not mean the entire described feature is absent. `tasks.md` remains authoritative.

- T230 [US11] Review every page/state capture against `specs/001-core-os-foundation/ui-design-guide.md` and establish approved baselines in `web/tests/e2e/ui-design.spec.ts-snapshots/`; document intentional health-first adaptations, font/browser versions and masked secret regions in `specs/001-core-os-foundation/ui-acceptance.md` rather than blindly approving snapshot updates (SC-027) ([task entry](tasks.md#L369))
- T232 [US11] Perform real 1280 px/200% browser zoom, 360 px reflow, keyboard/focus/dialog/disclosure, contrast/44 px target, reduced-motion and dirty-form/live-refetch reviews for every implemented screen; record measured results/evidence in `specs/001-core-os-foundation/ui-acceptance.md` and leave acceptance pending until actual manual evidence is available (FR-131–FR-135, SC-029) ([task entry](tasks.md#L371))
- T233 [US11] Build/install the redesigned offline bundle on a fresh supported PVE VM and rerun `specs/001-core-os-foundation/phase5-testing.md` plus `ui-validation.md`: setup/recovery download, sign-in, admin/token/network security, live/outage recovery, data-plane independence and fresh-cache external-network-blocked UI at HTTP 8021; record commit/archive checksum/results in `specs/001-core-os-foundation/ui-acceptance.md`, keeping manual checks pending without real VM evidence (SC-030) ([task entry](tasks.md#L372))
- T234 [US11] Conduct the five-novice-admin review from `specs/001-core-os-foundation/ui-validation.md` using non-secret representative data; record individual elapsed times and assistance for failed-service identification, administrator management and sign-out in `specs/001-core-os-foundation/ui-acceptance.md`; require at least four of five to complete all three within two minutes and leave pending until participants provide evidence (SC-031) ([task entry](tasks.md#L373))
- T235 [US11] Complete the FR-124–FR-135 / SC-027–SC-031 coverage and acceptance matrix in `specs/001-core-os-foundation/ui-acceptance.md`, linking automated/manual/VM/novice evidence and documenting any remaining failures; mark US11 accepted only when all criteria pass and update `specs/001-core-os-foundation/checklists/validation.md` without treating earlier completion as redesign acceptance ([task entry](tasks.md#L374))
- T102 [US4] Write the canary-secret leak test `tests/e2e/test_secrets_canary.py`. It plants canaries in every secret type, then searches logs, events, audit, diagnostic bundles, API responses, and snapshot bytes at every copy (SC-007) ([task entry](tasks.md#L448))
- T103 [US4] Write the acceptance test `tests/e2e/test_appliance_state.py` (SC-005): ([task entry](tasks.md#L449))
- T106 [US5] Implement the journaled restore operation in `core/crates/fd-appstate/src/restore/apply.rs` (FR-044, FR-063, FR-103, FR-110, FR-122): ([task entry](tasks.md#L486))
- T108 [US5] Implement DetectLocalState, OpenLocalRestoreSource, OpenRestoreSource, UploadRestoreFile, ListRestoreSnapshots, BeginRestore, MapPools, GetManagementConfigForRestore, and GetRestoreReport in `core/crates/filedock-core/src/api/recovery.rs` ([task entry](tasks.md#L504))
- T114 [P] [US5] Build the Restore Appliance flow in `web/src/features/restore/`: ([task entry](tasks.md#L520))
- T116 [US5] Write the acceptance test `tests/e2e/test_reinstall.py` (US5 scenarios 8–11, SC-021): ([task entry](tasks.md#L529))
- T117 [US5] Write the acceptance test `tests/e2e/test_disaster_recovery.py` (SC-006, SC-015, SC-016, SC-019, FR-111): ([task entry](tasks.md#L537))
- T252 [US4] Extend `web/src/features/recovery/recovery-views.test.tsx` for summary-first state protection, deliberate snapshot/key actions, dedicated long editors, dirty/outage continuity, basic/third-party claim acknowledgements, actual recovery-point scope and reauthentication/secret cleanup ([task entry](tasks.md#L739))
- T253 [US4] Refactor `web/src/features/recovery/StateProtectionPage.tsx` and add `web/src/features/recovery/StateProtectionEditorPage.tsx` with explicit hash/dispatch wiring in `web/src/navigation/destinations.ts` and `web/src/App.tsx`: recovery summaries lead, short key/reauth actions use dialogs, long connection/destination editors use dedicated pages; reuse current APIs and retain all copy/ownership/protection warnings and audits ([task entry](tasks.md#L740))
- T254 [US5] Add `web/src/features/recovery/locations-layout.test.tsx` for aligned ownership rows, takeover confirmations, read-only outage behavior, correct selected location under refetch and required restore report warnings; reuse synthetic recovery fixtures without weakening the original restore scenarios ([task entry](tasks.md#L748))
- T255 [US5] Migrate `web/src/features/recovery/LocationsPage.tsx`, `web/src/features/restore/RestoreWizard.tsx`, and `web/src/features/restore/RestoreReport.tsx` to shared compact table/summary/action patterns; preserve source/setup-code/recovery-key sequence, actual ownership and takeover acknowledgements, pool mappings and post-restore exposure/loss details, using existing long-flow pages instead of cramped dialogs ([task entry](tasks.md#L749))
- T256 [US6] Extend `web/src/features/extensions/extensions.test.tsx` for inventory-first layout, on-demand install/upgrade review, accessible resource-specific row actions, preservation of core/built-in removal restrictions, unsigned/permission/data-binding/default-provider consent and live/outage draft behavior ([task entry](tasks.md#L757))
- T257 [US6] Refactor `web/src/features/extensions/ExtensionsPage.tsx` and add `web/src/features/extensions/ExtensionPackagePage.tsx` with hash/dispatch wiring in `web/src/navigation/destinations.ts` and `web/src/App.tsx`: use aligned inventory rows and compact controls, move long package inspection/review to a dedicated page, retain all classes/diagnostics/configuration/health, permission and data binding review and real accepted-operation progress ([task entry](tasks.md#L758))
- T258 [US7] Extend `web/src/features/alerts/AlertsPage.test.tsx` and add `web/src/features/operations/operations-layout.test.tsx` for supported list filters/paging, current Operations detail selection, compact alert ack/diagnostics actions, missing-channel/critical visibility, channel dialog errors/test result and offline read-only behavior ([task entry](tasks.md#L766))
- T259 [P] [US7] Migrate `web/src/features/alerts/NotificationChannelsPage.tsx` to shared aligned table, toolbar/row actions and single-footer add/edit/test/remove dialogs; keep actual provider discovery, no-channel warning, configuration/secret references, validation and delivered-only test success ([task entry](tasks.md#L767))
- T260 [P] [US7] Apply shared column/toolbar/action patterns to `web/src/features/logs/LogsPage.tsx`, `web/src/features/activity/ActivityPage.tsx`, `web/src/features/audit/AuditPage.tsx`, `web/src/features/operations/OperationsPage.tsx`, `web/src/features/operations/OperationDetailPage.tsx`, and `web/src/features/alerts/AlertsPage.tsx`; retain supported filters, cursors, raw severity/state, diagnostic download/audit, persistent critical banner and read-only detail access ([task entry](tasks.md#L768))
- T261 [US8] Extend `web/src/features/folders/FolderForm.test.tsx` and add `web/src/features/folders/folder-pages.test.tsx` for new/edit hashes, six expanded sections/no tabs, generated read-only path, every mode and Advanced fields, explicit Hybrid-only importance, no-provider/empty-pool states, ETag conflict draft retention, field-linked error summary, dirty Cancel/history and destructive/dependency safeguards ([task entry](tasks.md#L777))
- T262 [US8] Introduce `web/src/features/folders/FolderEditorPage.tsx` and refactor `web/src/features/folders/FolderForm.tsx` with route/dispatch changes in `web/src/navigation/destinations.ts` and `web/src/App.tsx` for `#/folders/new` and `#/folders/{id}/edit`; preserve existing list URLs and exact IDs, select Managed Folders, use one form with six expanded numbered sections and responsive explanation/field columns, no tabs/wizard/modal or collapsed Advanced ([task entry](tasks.md#L778))
- T263 [US8] Complete editor behavior in `web/src/features/folders/FolderEditorPage.tsx` and `web/src/features/folders/FolderForm.tsx`: derive read-only path from name consistently with API normalization, preserve mode-specific sources and all advanced settings, show importance applicability, wire field IDs/error-summary focus and dirty-navigation protection, preserve drafts on ETag conflict/outage/refetch and retain System Pool, provider, loss and typed-deletion confirmations without bypassing server validation ([task entry](tasks.md#L779))
- T264 [US8] Refactor `web/src/features/folders/FoldersPage.tsx` into the shared aligned list with name/path, mode, access, Hybrid importance and separate protection/exposure; use page links for create/edit and compact named row actions for guarded deletion, keep Unprotected distinct from At risk with text/glyph/explanation and do not turn provider absence into a fake healthy state ([task entry](tasks.md#L780))
- T265 Integrate all migrated pages through `web/src/App.tsx`, `web/src/navigation/destinations.ts`, `web/src/api/events.ts`, and `web/src/api/useLiveCoreStatus.ts`; reconcile page/editor route registration and parent ownership, dirty guard lifecycle, session-expiry/private-secret cleanup, one subscription, reconnect/query invalidation and mutation-only outage blocking; resolve shared style/callsite regressions in `web/src/styles.css` without per-page alignment hacks ([task entry](tasks.md#L784))
- T266 Extend `web/tests/e2e/ui-design.spec.ts`, `web/tests/fixtures/component-gallery.tsx`, and `web/playwright.config.ts` for all implemented pages/dialogs/long editors at 1440/360 px with long text, loading/empty/error/stale, nested navigation, all folder modes, keyboard/mobile, offline assets and live dirty-draft scenarios; retain deterministic platform/clock/font readiness and secret masking ([task entry](tasks.md#L785))
- T267 Review actual screen/component captures against `specs/001-core-os-foundation/ui-layout-revision.md` and `.interface-design/system.md`, then establish reviewed pinned Linux/Chromium baselines in `web/tests/e2e/ui-design.spec.ts-snapshots/`; record image-specific raster corrections and intentional functional differences in `specs/001-core-os-foundation/ui-layout-acceptance.md`, never pass by blindly updating snapshots or relabeling macOS captures ([task entry](tasks.md#L786))
- T268 Run the implementation validation commands from `specs/001-core-os-foundation/ui-validation.md` (web lint, unit tests, type/production build, browser checks and existing applicable API/native smoke regressions); inspect packaged local font/icon/license assets, absence of production fixtures/external requests and unchanged HTTP 8021 behavior; fix failures and record actual commands/results/release in `specs/001-core-os-foundation/ui-layout-acceptance.md` ([task entry](tasks.md#L787))
- T269 Verify every implemented page/dialog/editor at 360 px, desktop and actual 1280 px browser width/200% zoom with keyboard-only, screen-reader names, focus, text/control contrast, nonoverlapping 44 px targets, reduced motion and scoped table overflow; record measurements and remaining failures in `specs/001-core-os-foundation/ui-layout-acceptance.md` rather than treating viewport emulation or screenshots as complete accessibility evidence ([task entry](tasks.md#L788))
- T270 Build the UI-containing offline release per `specs/001-core-os-foundation/quickstart.md` and run the relevant real-appliance flows on a disposable/explicitly authorized test VM using `specs/001-core-os-foundation/ui-validation.md` and `specs/001-core-os-foundation/phase9-10-testing.md`; verify account/secret/listener, extension/notification/folder guards, actual outage/reconnect and external-network-blocked assets, record outcomes in `specs/001-core-os-foundation/ui-layout-acceptance.md`, and leave unavailable-provider/Phase 8/Linux checks pending unless actually exercised ([task entry](tasks.md#L789))
- T271 Conduct the five first-time-administrator review from `specs/001-core-os-foundation/ui-validation.md` (SC-031: four of five identify a failure, locate administrator management and find sign-out within two minutes), and record individual timings/help and honest pass/pending results in `specs/001-core-os-foundation/ui-layout-acceptance.md`; do not simulate human outcomes or block unrelated implementation while participants are unavailable ([task entry](tasks.md#L790))
- T272 Reconcile `specs/001-core-os-foundation/ui-layout-acceptance.md`, `specs/001-core-os-foundation/checklists/validation.md`, `specs/001-core-os-foundation/ui-acceptance.md`, and `.interface-design/system.md` with the implemented layouts and actual evidence; preserve prior acceptance history/task IDs, report local versus Linux/VM/novice outcomes separately and declare the revision accepted only when its applicable criteria actually pass ([task entry](tasks.md#L791))
- T177 [US10] Write the acceptance test `tests/e2e/test_connectors.py`, covering quickstart "Cloud connectors and routing" steps 1–13 and US10 scenarios 1–9 (SC-023 to SC-026): ([task entry](tasks.md#L985))
- T184 Run the complete quickstart.md validation on fresh VMs, and record the results in `specs/001-core-os-foundation/checklists/validation.md` ([task entry](tasks.md#L1016))
