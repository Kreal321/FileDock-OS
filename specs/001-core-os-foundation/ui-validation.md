# UI redesign validation — 001 / US11

**Design**: [approved guide](./ui-design-guide.md)
**Plan**: [plan.md](./plan.md#ui-revision-implementation-approach-and-delivery)
**Contract**: [management-ui.md](./contracts/management-ui.md)

The redesigned screens and local assets are implemented. Local lint/build, unit tests and macOS
Chromium scenarios pass; reviewed macOS screenshot candidates exist. See [ui-acceptance.md](./ui-acceptance.md)
for actual evidence. Canonical Linux baselines and combined P1 manual acceptance remain pending.
Existing Phase 1–5 completion is historical service evidence, not acceptance of every UI requirement.

## Prerequisites and builds

Use the existing project toolchains. Visual comparisons use one pinned Linux/Chromium environment
with installed browser dependencies; other browser engines receive behavioral/keyboard smoke
coverage. Use disposable synthetic accounts/keys for captures, and keep real secrets out of traces.

From the repository root after implementation:

```bash
pnpm -C web install --frozen-lockfile
pnpm -C web lint
pnpm -C web test
pnpm -C web build
```

Expected: type/build/lint checks and behavior tests pass; local font/icon/license files are included
in Management's `wwwroot`. Production HTML/CSS must not reference external font/icon/CDN URLs.
A font failure still leaves readable fallback text and usable controls.

For complete packaging on a Debian 13 amd64 build host:

```bash
make bundle RID=linux-x64 CONFIG=Release
```

Alternatively run **Offline bundle release** on the commit containing the UI work. Verify the
archive checksum and `BUILD.txt`, then install/repair that extracted bundle as root using the
[combined PVE guide](./phase5-testing.md#1-build-and-install-the-release). Open HTTP port 8021.
Do not use an old published release as evidence for local UI changes.

## Deterministic browser/visual review

`web/tests/e2e/ui-design.spec.ts` intercepts API responses and event delivery using
test-only fixtures. Run this suite against Vite: its component gallery is a test-only module,
excluded from the production build. Real appliance checks use the separately packaged UI. Use the same fixed browser/OS/version, locale,
viewport, clock, reduced motion, and data for baseline generation and comparison. Wait for fonts
before captures. Font/OS changes require a reviewed baseline update, not blindly accepted images.

Run the existing scenario with the locally installed matching Playwright browser:

```bash
pnpm -C web exec playwright install chromium
# Serve the generated UI from Management, or serve web source with the existing proxy:
pnpm -C web dev --host 127.0.0.1
# In a separate terminal; fixture interception supplies test-only responses/events:
FD_BASE_URL=http://127.0.0.1:5173 pnpm -C web exec playwright test ui-design.spec.ts
```

Install browsers/dependencies on the test host before an offline appliance check. The first
baseline is produced with `--update-snapshots` only after reviewing resulting images against the
approved guide. Keep credentials and one-time secret contents masked/excluded in captured regions;
separate behavior checks verify the actual secret display/download flow.

| Coverage | Required states / expected result |
| --- | --- |
| Public screens | Checking, setup-code error/throttle, administrator setup, password mismatch, recovery download/reauthentication, sign-in error/lock; branded AuthLayout and no private navigation |
| Dashboard | Complete/partial component status; failed/degraded/stopped/unknown; no data; Core/browser disconnect; all notices; zero operations/activity; genuine operation/audit records |
| Settings | Overview, Administrators, Tokens, Network, Operations/detail, Audit; busy/error/saved and paging/filter states; real account identity |
| Shared components | Primary/secondary/danger, focus/busy/disabled, switch pending/failure, tabs/dialogs, empty/error/stale notices, long table names and errors |
| Routing | Direct hashes, refresh, back/forward, unknown/unavailable destination, protected hash before setup/sign-in; only implemented sidebar destinations |
| Reflow | 1440 px desktop and 360 px phone; no main-page horizontal overflow; long text does not hide warnings/actions |

Capture each implemented page and representative shared states. The health-first dashboard is an
intentional adaptation: storage summaries/charts from the image remain absent until supported.
Confirm no example folders, pool sizes, app names, John Doe identity, invented history, search box,
or bell count appear in production merely to resemble the image.

## Manual accessibility and continuity checks

1. At an actual browser window width of 1280 px, set browser zoom to 200%. Navigate every screen,
   read warnings, operate forms, and check that only designated tables/charts scroll horizontally.
   A simulated smaller viewport is supplemental and does not certify actual zoom.
2. Use only keyboard: skip link, sidebar/mobile disclosure, settings navigation, account actions,
   every form, row action, dialog and retry. Check focus visibility, Escape, and focus restoration.
   Native page navigation and true in-page tabs must use their respective keyboard semantics.
3. Measure foreground/background pairs for text, selected/hover/disabled/error states, and control
   and focus indicators. Verify 4.5:1 ordinary text, 3:1 large text/control indicators, and 44 px
   targets. Confirm every status has words and an icon/shape, not color alone.
4. Enable reduced motion. The UI remains readable/usable without transitions or animation.
5. Begin editing a form, then deliver a live status event/reconnect. Entered values, focus, and
   dialogs remain stable. On outage, only mutations are disabled; inspection/paging/sign-out work.
6. Check unknown and stale data: missing is not zero or Healthy. An HTTP response containing cached
   subsystem state is not presented as fresh. Protection success is scoped and no warning is hidden.

## Real appliance regression and offline acceptance

Follow [phase5-testing.md](./phase5-testing.md) for combined US1/US2/US3 acceptance against the
packaged PVE VM. Re-run real setup (on a fresh disposable VM), recovery-key download, sign-out/back
in, password confirmation/lockout, token secret-once/revoke, last-admin protection, listener guards,
Core/browser outage and recovery, data-plane independence, and live changes under two seconds.
Check that the new shell keeps readonly access and console recovery guidance usable during outages.

Block the test browser's external network requests while allowing its appliance HTTP origin and
same-origin event transport. Load setup/sign-in and each authenticated screen in a fresh browser
context/cache. All font/icon/control assets must remain local and readable. This is different from
switching the browser entirely offline, which would also block the appliance itself. A fully lost
LAN connection should instead show the unreachable/retry/stale presentation.

## Novice review and acceptance record

With representative non-secret data, ask five first-time administrators to identify a failed
service, locate administrator management, and find sign-out. Record individual elapsed times and
whether assistance was needed. At least four must finish all three within two minutes (SC-031).

Record release/commit, browser/OS/baseline version, images reviewed, every implemented screen/state,
contrast/keyboard/zoom/offline results, existing behavior regressions, real-VM results, and novice
outcomes. The design is accepted only after FR-124–FR-135 and SC-027–SC-031 evidence passes; planned
checks, baseline updates, or mock-only success are not a manual acceptance pass.

## Screenshot platform status

Current `*-darwin.png` captures are supplemental reviewed macOS candidates. The pinned Linux/Chromium
baseline still requires generation, visual review and approval on a Linux test host. Never rename
macOS images into Linux baselines. Baseline generation uses the fixture suite against Vite and
`--update-snapshots`; inspect every result against the guide, then rerun without that option.
The default browser command deliberately fails when its platform baseline is missing.
