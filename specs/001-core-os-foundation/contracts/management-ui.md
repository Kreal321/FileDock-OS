# Management UI contract — US11

**Revision**: 2026-10-13
**Authority**: [spec.md](../spec.md), FR-124–FR-135; [approved design guide](../ui-design-guide.md)
**Models**: [data-model.md](../data-model.md#ui-presentation-models-2026-10-13-revision)
**Lifecycle/API contract**: [management-api.md](./management-api.md)

This is the planned UI interface, not a claim that these redesigned views are implemented.
No new REST/gRPC endpoints, database fields, or privileged actions are required. Only the current
read APIs are used for the new Operations/Audit presentation surfaces.

## 1. Layout and assets

- Desktop: light cool background, 224 px left sidebar, 64 px top bar, main area with 24 px padding;
  these are implementation starting values within the guide's spacing rhythm, not pixel-match
  requirements for the reference sheet. At narrower widths, collapse the sidebar and stack panels;
  use 16 px main padding on phones. Layout adapts to content rather than assuming fixed screen height.
- Titles/section/body/caption: Inter 32/20/16/14 px, semibold/semibold/regular/regular. Respect the
  guide's 12 px cards and 8/16/24/32 px spacing. Keep styles centralized and use thin borders/shadows.
- Local Inter v4.1 WOFF2, readable fallback, and original SVGs are available without external requests.
  Font SOURCE metadata and the OFL/copyright notice ship with the UI. Screenshot artwork stays
  documentation-only; no embedded sample labels, metrics, avatars, or marketing/guide panels.
- Base guide swatches are preserved as brand/semantic tokens. Separate readable foreground tokens
  are used for rendered text, actions, and focus; for example white on primary-button `#2563EB`,
  selected/focus blue `#1D4ED8`, success text `#166534`, warning text `#92400E`, and error text
  `#B91C1C`. Actual foreground/background pairs, hover/busy/disabled states, and tinted backgrounds
  must be measured against FR-132. Do not reduce an entire control's opacity until text disappears.

## 2. Navigation interface

| Hash route | Destination | Access and source |
| --- | --- | --- |
| `#/dashboard` | Dashboard | Verified administrator session; system status, Core health, operations, administrative activity |
| `#/settings` | Settings overview | Implemented setting destinations with concise descriptions |
| `#/settings/administrators` | Administrators | Existing session-only account behavior |
| `#/settings/tokens` | API tokens | Existing session-only token behavior; secret shown once |
| `#/settings/network` | Network settings | Existing local listener/disconnect rules; HTTP port 8021 |
| `#/settings/operations` | Operations | Paged read-only journal list |
| `#/settings/operations/{id}` | Operation detail | Exact returned ID, encoded as a path segment; no credential or arbitrary content in route |
| `#/settings/audit` | Audit | Paged administrative history with supported actor/action filters |

The sidebar initially enables Dashboard and Settings. Settings child views retain their own page
heading and named secondary navigation. The reference's other sections are inserted in its order
when their real views ship. Omit search and the notification bell in the first redesign increment.
Do not infer availability from the existence of a future endpoint in the broader API contract.

Use native links with real hash destinations, `aria-current` for selection, a named navigation
landmark, a skip-to-main link, and one main landmark/page title. Handle hash changes, back/forward,
and refresh without full-page navigation. Unknown/unavailable routes resolve to Dashboard with a
plain unavailable-page notice after authentication. No aliases to earlier development navigation.

A mobile disclosure button has a name, `aria-expanded`, and `aria-controls`; hidden navigation
cannot receive focus. Escape/selection closes it. Restore focus when an overlay is dismissed and
move focus to the new page heading for deliberate navigation, never for background refetches.
Account actions use named buttons/disclosure semantics; do not declare an ARIA menu without its
required keyboard behavior. The header identity uses the actual `/auth/me` username/derived initials.

## 3. Gates, authentication, and mutation availability

| Condition | Visible result |
| --- | --- |
| First gate request pending | Branded checking/loading state; no private content |
| Gate 423 | Existing setup flow in AuthLayout |
| Expected startup 401 | Sign-in; no global expiry/cache-clear loop |
| Network failure / 502 / 503 at gate | Appliance unavailable with retry; do not fabricate sign-out |
| Verified session | AppShell, real identity, registered routes, one live subscription |
| Protected request 401 / session-expired event | Clear authenticated caches and secret state, stop subscription, reset navigation, show sign-in |
| Core Starting/Unreachable/Incompatible or browser event connection lost | Persistent labeled banner, stale/read-only information, disabled mutations, active retry/navigation/sign-out |
| Connectivity restored and fresh data obtained | Clear stale marking for refreshed sources and automatically re-enable permitted mutations |

Do not call public `/health` a detailed health/status source. Detailed `/system/status` stays
authenticated. Retain cookie/CSRF/session-only restrictions and the server's Core availability guard.
A client disabled button alone is never authorization. Enable a mutation only when both live browser
connectivity and Core `canMutate` are known true; absent information is false.

## 4. Actual data sources and dashboard slots

| UI projection | Implemented source | Constraints |
| --- | --- | --- |
| Appliance identity/version | `GET /system/status` | Missing name/version is unavailable, not a guessed release |
| Service cards/counts | `GET /system/status` components | Keep Core, Management, Docker, Samba, NFS, Kopia, fd-s3, rclone visible; unknown expected components get Unknown presentation, not a invented healthy observation |
| Core banner/change availability | `GET /system/core-health` + browser live connection | Preserve Starting/Healthy/Unreachable/Incompatible and detail; cached HTTP values are not proof of a live connection |
| Notices | System notices + warning flags | Render all applicable notices; false/absent protection flag is not proof of remote backup coverage |
| Recent operations / journal | `GET /operations?activeOnly=&pageSize=&pageToken=`, detail route | Dashboard preview requests five rows, offers real View all; full view pages explicitly and preserves continuation |
| Administrative activity / Audit | `GET /audit?pageSize=&pageToken=&actor=&action=` | Five-row dashboard preview; exact actor/action filters supported. No time filter, alert feed, or raw detail JSON in the first view |
| Account | `GET /auth/me` | Session identity, no fake John Doe/avatar |

Until detailed US4 coverage exists, show protection information as unavailable with a plain
notice and no green protected/backup summary. Do not use the existing false warning flag to
suppress that missing-information notice.

Use summary cards for actual Core connection, Management/container-runtime status, and a scoped
reported-service summary. Use content panels for every subsystem's health/detail, notices, recent
operations, and administrative activity. Include real navigation to Operations/Audit detail where
supported. Do not calculate "all healthy" from an empty array or count Unknown synthetic rows as
observed records. Docker service health is not the number/health of its containers.

The following slots are deferred: folder/pool/cloud counts and lists, capacity, protected-folder
counts, backup history/status, cache/CPU/RAM charts, alert counts/list, container inventory. They
require their original feature's real data. Do not request those unimplemented endpoints or fill
these positions with fixtures. Later storage panels must retain System Pool limits and protection
scope; a mode badge never establishes a recovery point.

The operation list/detail is read-only in this increment. A manual reconcile action is optional
future UI work over the existing F-scope endpoint; if added, 202 means accepted and links to
persisted progress rather than claiming instant success.

## 5. Shared component behavior

| Component | Required interface/state behavior |
| --- | --- |
| Button | primary/secondary/danger; busy/disabled; native button with explicit type and name; 44 px target; focus and failure remain visible |
| Field / Select | Label, value, help, required/error state; error associated with the field; native form semantics and password autocomplete behavior retained |
| Switch | Labeled native checkbox/switch semantics; saved value distinguished from pending/failed submission; no false saved visual state |
| Tabs | For actual in-page tabs, correct tab/panel relationships and arrow/Home/End focus; Settings page links use navigation semantics instead |
| Panel / PageHeader | Stable heading structure, guide spacing/radius, optional genuine actions; no clickable card without a named real destination |
| StatusBadge / Notice | Text plus icon/shape and semantic tone; raw domain states retained; actionable persistent warnings are not dismissed by an unrelated success |
| StateView | Separate loading, confirmed empty, error/retry, unavailable, and stale presentations; preserve data while refreshing |
| DataTable | Caption/name, headers, paging, overflow within its region, readable long values, accessible row actions; no default dump of secret/detail payloads |
| Dialog / disclosure | Explicit close, Escape, initial focus and focus restoration; modal background is noninteractive only while truly modal |
| Icon | Standardized viewBox/stroke/currentColor; decorative icons hidden; standalone actions have accessible names |

Charts are a future component with real history, textual summaries, keyboard-readable information,
and reduced-motion support. The sample's charts do not require a graph library in this increment.

## 6. Form and live-update invariants

- Preserve default editable `root`, password re-entry and advisory length, setup code, and recovery
  download/confirmation/reauthentication sequence. No required step is removed for visual fidelity.
- Preserve current-password and last-admin guards, sign-out-all, token scope/expiry/revocation and
  one-time secret display, network disconnect confirmation, and console recovery instructions.
- A resource refetch while a form is dirty never replaces entered fields or silently commits a
  switch. Preserve selected rows/tabs, focus and dialogs through background status invalidation.
- Keep read-only inspection/filter/paging separate from mutation controls. A Core outage cannot
  disable the entire Settings container or prevent sign-out.
- One event subscription lives at AppShell level. Deduplicate by sequence and invalidate relevant
  queries; reconnect/sync refreshes views. Do not add routine full polling for dashboard content.
- All retained values are visibly cached/stale when their source is unavailable. "Last fetched"
  is local receipt time; the API currently provides no component observation timestamp.
- Never store password/recovery/token secrets in hashes, browser storage, screenshots, logs, or
  production demo fixtures. Do not persist presentation state in appliance snapshots/databases.

## 7. Acceptance traceability

| Requirements / outcome | Planned evidence |
| --- | --- |
| FR-124, SC-027 | Tokens/components + offline font/icon/license assets + approved visual baselines |
| FR-125, FR-126 | Authenticated/public shells, route registry, supported/unavailable/empty navigation fixtures |
| FR-127, FR-128, SC-028 | Dashboard adapter tests and real-response/empty/unknown/failed/stale views; no fixture data in production |
| FR-129 | Every notice, protection scope, and Core/browser outage scenario retained |
| FR-130, FR-134 | Form/security regression and dirty-form/focus/secret-lifecycle checks |
| FR-131, SC-029 | 1440 px desktop and 360 px captures; actual 1280 px/200% browser zoom; scoped table overflow |
| FR-132 | Keyboard semantics, focus, contrast, target-size and reduced-motion checks; future chart text equivalence |
| FR-133 | Actual identity/sign-out and omission of unsupported search/alerts |
| FR-135, SC-030 | Reviewed screenshots + offline packaged-appliance check + prior live/VM acceptance rerun |
| SC-031 | Five first-time administrators; at least four complete the three specified tasks within two minutes |

No visual baseline is accepted merely because it matches a previous rendering. Its first approval
must compare the composition and states with the design guide and this contract. Test fixtures
live only in test paths and request interception; they never turn into production defaults.
