# Delivery validation status

**2026-10-13:** Phase 5A implementation and local frontend checks passed (29 unit tests and 23
Chromium browser scenarios). See [ui-acceptance.md](../ui-acceptance.md) for actual evidence and
pending work. Historical Phase 3 manual acceptance remains recorded in the feature documents.

The user will run combined P1 delivery acceptance for US1, US2, US3 and US11. Real VM, actual zoom,
full accessibility review, novice outcomes and canonical Linux visual approval remain pending.
This record does not claim completion of later stories or the full quickstart validation T184.

Installed PVE smoke on 2026-10-13: PASS (17 API/supervision and five real-browser checks, final
eight built-ins healthy). See [the record](../phase5-pve-smoke-2026-10-13.md). This is partial real-VM
evidence; the remaining full P1/manual requirements above remain pending.

Phase 6/7 configuration-protection and recovery implementation on 2026-10-13:
52 focused Rust tests passed (three environment/helper tests ignored); Clippy passed with warnings
as errors. Management build passed with no warnings, and all 30 .NET tests passed. All 31 frontend
tests, lint, and production build passed. The contract suite passed 22 checks with two buf checks
skipped because buf is unavailable; 14 VM tests collected. The isolated native smoke passed durable
snapshot/export, five secret canary types, authentication exclusion, wrong key/setup code rejection,
recovery-key-only local restore, fresh multipart backup-file restore, restored sign-in, and retained
report/dismissal. No PVE installation was changed. See [combined guide](../phase6-7-testing.md).
Full Phase 7 pool mapping/System Pool recreation/extension installation/data reconciliation remains
pending US9/US6, and full VM/copy-fault/diagnostic-bundle acceptance remains pending as recorded in
tasks.md. This entry does not claim those acceptance gates have passed.

Installed PVE Phase 6/7 smoke on 2026-10-13: PASS for configuration protection and isolated
configuration recovery, including key rotation and retained epoch verification using the installed
Linux binaries with production KDF settings. See [PVE smoke record](../phase6-7-pve-smoke-2026-10-13.md).
Automatic snapshot latency was 33.88 seconds. All eight installed services remained healthy.
Remote/data-pool/disaster-recovery hardware paths and later provider dependencies remain pending.

Phase 8 implementation on 2026-10-14: package/permission/cache/capability/binding tests and
native extension API guards passed; Phase 6/7 native recovery smoke still passes. Management
build and 31 .NET tests passed; frontend 32 tests, lint and build passed. Contract checks passed
22 with two buf skips; 16 VM tests collected. See [Phase 8 guide](../phase8-testing.md) for exact
local evidence and the pending Linux runtime/isolation/20-kill/rollback acceptance gate. The PVE
installation was not updated or changed during Phase 8 implementation.

### Phases 9–10 local implementation validation (2026-10-14)

Operations, notification channels and managed-folder flow are implemented. Local results are
recorded in [phase9-10-testing.md](../phase9-10-testing.md): 66 focused Rust tests, Clippy,
32 .NET tests, 35 frontend tests, 22 contracts (two buf checks skipped), production frontend build,
and native Phases 9–10, 8 and 6–7 smokes passed. Eighteen VM tests collect. Actual Linux mount,
journal/notification acceptance and browser visual checks remain pending. Pool provisioning remains
Phase 11, and other storage-mode data planes remain features 002–004. Existing Phase 8 PVE evidence
and its pending runtime acceptance are unchanged.

## Phase 13 checkpoint — 2026-10-19

Implementation/local checks do not establish full feature acceptance. The [Phase 13 PVE
report](../phase13-pve-testing-2026-10-19.md) records the exact installed release and measured results. Consolidated remaining
work is in [missing features and testing](../missing-features-and-testing.md).

| Quickstart area | Current evidence / remaining scope |
| --- | --- |
| Build/install/first run | Published offline upgrades and historical manual first run; current fresh install pending |
| Authentication/tokens | Automated API coverage and historical installed checks; fresh full matrix pending |
| Pools/folders/shares/apps | System Pool/local protocol checks recorded historically; additional-disk provisioning/adoption/grow/failure pending |
| System Pool | Limits/restart/reserve configuration verified historically; full pressure/hard-cap/destructive fault matrix pending disposable hardware |
| Data-plane independence | Historical share/app continuity and service supervision; Phase 13 restarts/integrity recorded separately; power cuts pending host control |
| Secrets/appliance state | Focused encrypted snapshots, canaries, copy checks passed; full T102/T103 matrix incomplete |
| Reinstall/recovery/backup file | Isolated configuration/key/file restore passed; full hardware/new-disk/runtime/remote recovery pending |
| Cloud connectors/routes | Native/rclone/stub/Azurite/fault/third-party focused live checks passed; strict two-VM claims and backup-only provider path pending |
| Pool failure | Not run without expendable data disks |
| Alerts/notifications/extensions | Focused lifecycle and notification fixtures have evidence; complete fresh quickstart matrix not accepted |
| UI | Shared foundations/copy and focused desktop/phone reviews; remaining page migrations, canonical Linux baselines, full accessibility and novice studies pending |
| Idle footprint | New minimum-VM acceptance fixture; larger installed-VM measurement recorded in Phase 13 evidence, not a substitute for minimum hardware |

T184 remains unchecked. No blanket “all quickstart checks passed” claim is made.

Phase 13 local checks: 127 Rust tests passed (3 ignored), Clippy passed, 27 API and 14 unit
xUnit tests passed, 61 frontend tests passed, frontend lint/build passed, 22 contracts passed
(two buf skips), and 35 VM cases collected. Installed release 44 (`c178bfe`) passed generated
OpenAPI/process-priority checks and all seven daemon restart/integrity/canary/snapshot checks.
The 120.51-second idle sample passed on this larger VM at 1.975% of one core and 132.8 MiB
combined peak cgroup memory. CPU margin is narrow; minimum-hardware and sustained-idle
acceptance remain pending. Earlier failed samples and the work-reduction fixes are retained
in the dated report. No power-cut result is claimed.

Release-44 reboot passed without manual starts: all eight components Healthy, static address/
gateway/DNS retained, runtime directory 0770 and Management admin socket 0600, admin ping OK,
Management NRestarts=0, UI HTTP 200 and no current-boot Core/Management journal errors.
Disposable resources/vault references removed; active-resource baseline restored and private
diagnostic/session trees removed. See the dated report for exact runtime identity and scope.
