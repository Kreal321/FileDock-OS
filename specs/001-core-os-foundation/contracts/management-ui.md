# Management UI contract — US11

**Revision**: 2026-10-15 (refines 2026-10-13)
**Authority**: [spec.md](../spec.md), FR-124–FR-135; [approved design guide](../ui-design-guide.md)
**Models**: [data-model.md](../data-model.md#ui-presentation-models-2026-10-13-revision)
**Lifecycle/API contract**: [management-api.md](./management-api.md)

This is the planned UI interface, not a claim that these redesigned views are implemented.
No new REST/gRPC endpoints, database fields, or privileged actions are required. The 2026-10-15 revision reuses the implemented operations, audit, alerts, extension, folder,
notification and recovery interfaces. See the [layout revision](../ui-layout-revision.md) and
[design system](../../../.interface-design/system.md) for prototype precedence and measurements.

## 1. Layout and assets

- Desktop: light cool background, 224 px left sidebar, 64 px top bar, main area with 24 px padding;
  these are implementation starting values within the guide's spacing rhythm, not pixel-match
  requirements for the reference sheet. At narrower widths, collapse the sidebar and stack panels;
  use 16 px main padding on phones. Layout adapts to content rather than assuming fixed screen height.
- Type: Inter28px page title,20px section title,16px form-section title,14px body,12px caption;
  semibold headings/row identity, medium labels, regular metadata. Use4px base grid, primarily
  8/16/24/32px spacing;6px controls,8px panels,12px dialogs. Keep tokens/styles centralized.
- One sidebar only. Remove in-content Settings navigation and secondary rails. Panels contain
  the primary list or saved summary; short edits appear through deliberate actions, not permanent forms.
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

The sidebar currently exposes Dashboard, Managed Folders, Extensions and Settings. Settings is
one disclosure group within that sidebar, not another rail or horizontal navigation strip. Its
Overview link owns `#/settings`; child links own their existing routes. Include Administrators,
API tokens, Network, Notifications, State protection, Owned locations, Operations, Activity, Alerts,
Logs and Audit. Preserve Operations as a link, not only a group label. Only implemented views appear.

Exactly one link has `aria-current="page"` and selected fill. The Settings disclosure has only
`aria-expanded`, separate from selection. Active child routes auto-reveal the group. Operation
detail selects Operations; folder create/edit selects Managed Folders. No broad non-Dashboard test.
Parent links and expand controls must not have conflicting click behavior.

Additional page routes (planned, existing request APIs reused):

| Hash route | Page / active destination |
| --- | --- |
| `#/folders` | Folder list / Managed Folders |
| `#/folders/new` | Full-page Create folder / Managed Folders |
| `#/folders/{id}/edit` | Full-page Edit folder / Managed Folders; exact encoded ID |
| `#/extensions` | Implemented subsystem/extension inventory / Extensions |
| `#/settings/state-protection` / `#/settings/locations` | Recovery coverage / owned locations |
| `#/settings/logs` / `#/settings/activity` / `#/settings/alerts` | Actual operations data views |
| `#/settings/notification-channels` | Real channel list and actions |

Register explicit folder routes before generic resource matches. Keep public auth gates and unknown
route handling. Hashes never carry credentials, form draft data or secrets. Omit unavailable global
search/alert controls; existing persistent critical alerts remain visible.

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

| UI projection | Existing source | Constraints |
| --- | --- | --- |
| Appliance identity/services/notices | `/system/status`, `/system/core-health` and live events | Keep actual Core/Management/runtime/built-in observations, all warnings, unknown/stale distinction; no fabricated coverage |
| Account | `/auth/me`, existing administrator/token endpoints | Actual username/derived initials; no sample identity, secret defaults or changed session rules |
| Operations | `/operations` and exact-ID detail | Paged list; accepted work links to persistent progress; no success inferred from202 |
| Audit | `/audit` | Supported actor/action/target/result/time filters and opaque paging from management-api.md; never raw secrets/details by default |
| Logs / Activity / Alerts | `/logs`, `/activity`, `/alerts` and ack | Actual filtered events/records; persistent critical banner and no-channel notice; no data != empty |
| Notifications | `/notification-channels` and test/action endpoints | Actual provider capability and channel state; test success only on actual delivery result |
| Folders | `/folders`, `/folders/options`, `/folders/validate` and resource mutations | Capability-based modes/access, generation checks; separate name/path/mode/Hybrid importance/protection/exposure |
| Extensions / recovery | Existing extension/subsystem and state-protection/location APIs | Retain actual classes, permissions, health, ownership and recovery-point scope |

Reuse implemented request DTOs and existing view adapters. The UI revision adds no endpoint,
collector or invented backend field. Future capacity/cache/CPU/RAM/history/container inventory slots
appear only with real source data; a current point is not a historical trend. System Pool capacity
must retain its limits; successful folder backup never certifies appliance-wide recovery.

The dashboard retains scoped summaries/content panels, not a forced card count. Do not calculate
all-healthy from an empty array. Cached subsystem health remains stale even with a fresh HTTP
receipt. Use receipt labels rather than invented observation timestamps. Optional global controls
remain absent until they have real implemented behavior. Paged views remain available during outages.

## 5. Shared component behavior

| Component | Required interface/state behavior |
| --- | --- |
| Button | primary/secondary/danger plus shared quiet/icon action treatment and standard/compact sizing; native button with explicit type/name; effective 44px target even when 36 px visual; focus/busy/failure visible |
| Field / Select | Label, value, help, required/error state; error associated with the field; native form semantics and password autocomplete behavior retained |
| Switch | Labeled native checkbox/switch semantics; saved value distinguished from pending/failed submission; no false saved visual state |
| Tabs | For actual in-page tabs, correct tab/panel relationships and arrow/Home/End focus; Settings page links use navigation semantics instead |
| Panel / PageHeader | Stable heading structure, guide spacing/radius, optional genuine actions; no clickable card without a named real destination |
| StatusBadge / Notice | Text plus icon/shape and semantic tone; raw domain states retained; actionable persistent warnings are not dismissed by an unrelated success |
| StateView | Separate loading, confirmed empty, error/retry, unavailable, and stale presentations; preserve data while refreshing |
| DataTable | Caption/name; shared column metadata controls header/cell alignment and width; consistent padding/middle alignment; end-aligned tabular numbers; compact trailing action column; paging/scoped overflow, long values and accessible resource-named actions; no payload dump |
| Dialog / disclosure | Existing native dialog, named header/description/body and one footer; sensible initial field/error focus, trap/return, Escape/close, busy/error behavior and dirty-cancel confirmation; viewport-bounded scrollable body; background inert only while truly modal |
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


## 8. Current page/action layout contract — 2026-10-15

[Layout revision](../ui-layout-revision.md#page-cleanup-inventory) is the full-screen migration inventory.
No API/domain behavior is changed by this presentation contract. Implemented later-story APIs in management-api.md supply folders/extensions/alerts/notifications/
recovery and supported audit filters. Metrics/providers still not implemented remain omitted.

- Administrators: table leads; PageHeader Add administrator opens dialog; compact Your password row
  opens Change password dialog. No permanent account creation/password form. Preserve actor-only
  password behavior, last enabled account checks, matching re-entry and advisory length.
- Tokens: list plus Create action; dialog retains scope/expiry, then memory-only secret-once result.
  Closing/sign-out/expiry clears it; no secret screenshot or persistent draft.
- Notifications: list plus channel editor/test/remove actions, retain missing-channel warning and
  actual delivery result. Extensions: preserve all subsystem classes, health, permissions, binding,
  default-provider and package-review controls; compact list first, focused review on demand.
- Network/recovery: saved-state summaries and actual warnings lead; short actions use dialogs,
  long connection/destination/takeover editors use full pages retaining all safety acknowledgements.
- Folders: create/edit full page with one form and six sequential expanded numbered sections.
  Identity, Storage Mode, Source/Backend, Data Importance, Access, Advanced; no tabs or wizard.
  Importance input onlyHybrid, otherwise applicability text. Advanced stays on page. Name-derived
  read-only path matches API normalization. ETag conflict and validation preserve draft; no blind retry.
- A folder error summary focuses and links to corresponding fields; preserve typed-name deletion,
  importance/mode change acknowledgements and dependency errors. Cancel/back/navigation asks before
  discarding dirty values. Refresh/close uses browser-supported warning without persistent secret drafts.
- Shared cards/summary rows have one relevant action, not tall inline forms. Lists retain filtering,
  paging and read-only access on outage. Compact ellipsis actions are resource-named, keyboard-usable,
  and do not depend on hover. Use disclosure/list semantics or a proven menu primitive with complete
  keyboard behavior, never incomplete hand-rolled ARIA menus.
- Accepted visual dimensions and responsive behavior are defined in system.md. No raster reference
  reduces44px effective targets, hides Hybrid importance or required protection/exposure, or claims
  unimplemented capabilities. Only real observed state is presented as healthy/protected.

All 2026-10-15 work remains planned. Prior screenshots/tests are historical and do not certify this
revision. New behavioral/visual coverage must follow ui-validation.md before acceptance.
