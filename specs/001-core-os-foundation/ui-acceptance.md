# Phase 5A / US11 — implementation and acceptance record

**Date:** 2026-10-13  
**Status:** Implementation and local automated checks complete; combined P1 manual acceptance pending.  
**Scope:** US1 + US2 + US3 + US11, HTTP Management UI/API on port 8021. No backend endpoints or storage schema changed.

> This record describes the 2026-10-13 implementation. The approved
> [2026-10-15 layout revision](./ui-layout-revision.md) is planned and requires its own implementation
> and acceptance evidence; the results below do not certify it.

## Implemented

- Light FileDock theme, accessible semantic variants, local Inter v4.1 font/license and original SVGs.
- Shared semantic actions, fields, selects, switches, true tabs, modal confirmations, tables, panels and state notices.
- AuthLayout and persistent AppShell, real account identity/sign-out, desktop sidebar, mobile disclosure and hash navigation.
- Setup, recovery download, sign-in, administrators, tokens and network settings migrated without changing required security steps.
- Settings overview, paged Operations/detail and Audit views over current APIs; only actor/action audit filters.
- Truthful health-first dashboard with all eight expected services, extra reported subsystems, all notices and five-record operation/audit previews.
- Missing/unknown/empty/stale/unreachable values remain distinct. Protection coverage stays Unknown until the original US4 work provides actual detail.
- One authenticated live connection across routes; Core/browser guards disable mutations while inspection, filtering, retry and sign-out remain usable.
- Startup 423/401 gates and network/502/503 retry states; protected 401 clears the session shell/private queries/secrets. Disposed live callbacks cannot restore state.

## Local evidence

Environment: macOS arm64, Node 22.18.0, Playwright 1.64.0 / Chromium 156.0.8078.4 (revision 1248).
Fixtures use fixed UTC 2026-10-13 12:00, en-US, reduced motion and local font readiness. Captures mask
recovery/token contents; no real credentials or appliance data were used. This is local frontend
coverage, not a report that the PVE appliance passed or that a release was published.

| Check | Actual outcome |
| --- | --- |
| Test-first run | Failed before implementation: five missing-module suites and two form/security regressions |
| `pnpm -C web lint` | PASS |
| `pnpm -C web test` | PASS: 9 files, 29 tests |
| `pnpm -C web build` | PASS: TypeScript and Vite; local WOFF2 and public OFL license included in Management wwwroot |
| `FD_BASE_URL=http://127.0.0.1:5173 pnpm -C web exec playwright test ui-design.spec.ts --workers=1` | PASS: 23 scenarios, without updating snapshots |
| Production asset inspection | Local hashed Inter asset and license present; remote font imports removed; test fixtures absent from the production entry/module graph |
| Visual candidates | 44 macOS captures; all reviewed in contact sheets against the guide, with desktop/dashboard and phone/network inspected at full size |
| Linux canonical baseline | PENDING: Docker client exists but its daemon is stopped; macOS captures are supplemental candidates, not the planned Linux baseline approval |
| Real VM / release archive | Installed PVE VM smoke PASS: 17 API/supervision and five real-browser checks; fresh install, release provenance and full P1 acceptance remain pending |

Visual adaptations: immediate navigation is Dashboard and Settings, with implemented Settings child
views; unsupported reference sections/search/bell/charts/capacity/container counts are absent.
Service health and recovery exposure replace the sample's storage/backup widgets. No sample folders,
John Doe, cloud counts, invented history or green global protection claims ship. The guide's pale
background, white rounded panels, blue brand/actions, semantic tints, spacing/type scale and soft
shadows are shared across screens. Forms remain native and warnings visible.

The browser suite covers desktop 1440 px and phone 360 px for all implemented routes and public gates,
setup/mismatch/recovery reauthentication, all service state variants, empty/error lists, shared busy/
error/disabled/dialog states, local assets with external requests blocked, history/unknown routes,
keyboard skip link and dialog focus, protected 401, dirty forms under Core events and transport loss.
Unit tests cover adapter truthfulness, native semantics, hash/disclosure, password matching, recovery
TXT-before-confirmation, last enabled administrator, current-password requirement, token secret-once/
expiry, network acknowledgement, event dedup/retries and disposed reconnect callbacks.

## Requirement coverage

| Requirement | Implemented / automated evidence | Remaining acceptance |
| --- | --- | --- |
| FR-124 | tokens.css, fonts.css, SOURCE.md/OFL, Icon; asset/build/browser checks | Linux visual review; real bundle offline check |
| FR-125 | AppShell/AuthLayout/Sidebar/TopBar; protected/public routes and disclosure tests | Real keyboard/zoom review |
| FR-126 | Implemented destinations only; Settings links, Operations/Audit | Verify production resources and empty-state clarity on VM |
| FR-127 | dashboard-model and current API panels; eight service cards | Compare actual healthy/failed appliance output |
| FR-128 | DataView/status adapters; missing/zero/unknown/stale fixtures | Verify actual Core/browser outage source behavior |
| FR-129 | All notices and persistent Core banner; scoped protection Unknown | Check recovery/exposure notices in combined P1 run |
| FR-130 | Shared components plus busy/error/dialog/native behavior tests | Full manual keyboard/control-state review |
| FR-131 | 360 px captures and no main-page overflow assertions | Actual 1280 px window at 200% browser zoom; tables may scroll |
| FR-132 | Component focus/keyboard/status tests; accessible variants/44 px targets/reduced motion | Measure rendered contrast pairs and full manual keyboard paths |
| FR-133 | `/auth/me` identity, sign-out, no unsupported search/bell | Real sign-out/account check |
| FR-134 | Form/security/live regression tests and updated first-run/Phase5 browser tests | Real sessions, listener guards, token revoke and setup rerun |
| FR-135 | 44 state/page captures, 23 browser scenarios | Linux baseline plus human/manual acceptance |
| SC-027 | Local candidates reviewed against guide with documented functional adaptations | Canonical Linux review and final human approval |
| SC-028 | Actual endpoint fields and record counts, no production fixtures | Actual VM data/action availability review |
| SC-029 | Phone reflow and targeted keyboard automation | Actual zoom, full keyboard, contrast/touch review |
| SC-030 | Local assets/build and external-network-blocked browser scenario | Packaged fresh-cache offline and P1 VM regression |
| SC-031 | Novice procedure ready | Five participants; at least four complete all three tasks in ≤2 minutes without assistance |

## Combined P1 testing handoff

1. Build the commit containing these changes with **Offline bundle release** on GitHub, or on a
   Debian 13 amd64 build host run `make bundle RID=linux-x64 CONFIG=Release`. Local frontend build
   success is not proof that the complete Linux bundle has been built.
2. Verify the downloaded archive SHA-256/BUILD.txt and install on a fresh disposable supported PVE VM.
   Follow [phase5-testing.md](./phase5-testing.md) for setup, administrator/token/network security,
   live/outage recovery, data-plane independence and reboot checks. Open `http://<VM-IP>:8021`.
3. Follow [ui-validation.md](./ui-validation.md) for all current screens, 360 px, **actual** 1280 px/
   200% browser zoom, keyboard, contrast/44 px targets, reduced motion and dirty-form continuity.
4. Block external browser requests while allowing the appliance origin and event transport, then
   load every screen in a fresh browser context/cache. A fully lost LAN connection instead shows
   the unreachable/cached/retry state.
5. Record novice outcomes and the release/browser/VM evidence below. Leave unfinished checks pending.

| Manual evidence | Result to fill in |
| --- | --- |
| Release/tag/commit, archive SHA-256 and BUILD.txt | Pending |
| PVE VM OS/resources and fresh setup/recovery/sign-in | Pending |
| US2 account/password/token/listener guards | Pending |
| US3 data-plane independence/live updates/outage/reboot | Pending |
| Offline packaged assets in fresh browser context | Pending |
| 360 px and actual 1280 px/200% zoom | Pending |
| Full keyboard/focus/modal/disclosure review | Pending |
| Contrast/touch targets/reduced motion | Pending |
| Novice participants 1–5: elapsed time and assistance | Pending |
| Linux baseline environment and reviewed images | Pending |
| Overall P1 delivery acceptance | Pending |

T202–T229 and local automated validation T231 are complete. T230 remains pending canonical baseline
approval; T232–T235 remain pending manual/VM/novice evidence and final acceptance. The user's request
to test P1 together authorizes this handoff; it does not turn these pending checks into passes.

## Installed PVE smoke update

On 2026-10-13, [the installed VM smoke](./phase5-pve-smoke-2026-10-13.md) passed live/outage,
dirty-form/focus, automatic recovery, account sign-in/out and Operations/Audit navigation checks
against the real deployed UI, plus supervision/API checks. The temporary fixture was removed and
the final dashboard showed eight healthy built-ins. This adds real-VM evidence to FR-127–FR-129,
FR-133–FR-134 and SC-030; full T233, fresh setup/offline packaging, zoom/accessibility, novice and
Linux visual approval remain pending. No pending acceptance task is marked complete by this smoke.

## Phase 11 UI extension (2026-10-15)

Pools & disks, Cloud, Shares and Docker now use the same implemented sidebar destinations,
compact tables, row menus and action dialogs. The System Pool is shown separately from data
pools; no chart/usage samples were introduced. Creation, limits, share-user, export-rule,
connection/OAuth and Compose dialogs preserve explicit error/busy/read-only behavior.

`web/tests/e2e/phase11.spec.ts` checks 1440 px and 360 px layouts, disk serial confirmation,
retained inputs and disabled writes during a Core outage. Synthetic screenshots were reviewed
locally; these do not replace canonical Linux baseline approval. See `phase11-testing.md` for
actual data-plane and provider acceptance still pending. Existing golden screenshots were not
silently replaced to accommodate the newly enabled navigation.
