# FileDock interface system

**Approved**: 2026-10-15. Approved reference; first increment T236–T251 implemented, later migrations pending.
**Authority**: `specs/001-core-os-foundation/ui-layout-revision.md` and UI contract.

## Direction and intent

A calm single-appliance storage console. Let administrators scan real resources and exposure,
then perform one deliberate change. Identity pairs with path/identifier; protection is always
separate from mode. Preserve existing locally bundled Inter and original FileDock icons.

Domain: pools, mounts, paths, recovery points, capabilities, health, ownership.
Colors come from equipment white, cool gray structure, navy lettering, blue folder markers,
green health lights, amber cautions and red failures. Reject giant form cards, duplicate rails,
and repeated oversized row-button bars. Use summaries, one sidebar and focused actions.

## Foundations and hierarchy

- Keep existing semantic tokens, extending them instead of creating parallel page-local palettes.
  Brand `--blue` #3B82F6; primary #2563EB, ink #101A42, muted #536384, background #F4F8FD,
  surface white. Keep accessible semantic foreground/background pairs and measure actual contrast.
- Add semantic secondary/tertiary/disabled text, inset-control, quiet-border, menu/dialog surface,
  compact-control, action-target and row-density tokens during implementation. No hardcoded cell CSS.
- Depth strategy: subtle shadows for elevated surfaces, quiet boundaries for tables/fields;
  no decorative gradients/heavy shadow. Sidebar shares canvas tone; inputs slightly inset;
  menus/dialogs sit above their parent. Page → panel → overlay remain legible.
- Inter: heading 28/600, section 20/600, form-section 16/600, body 14/400, caption 12/400.
  Use weight and color, not many nearly identical font sizes; line-height body ~1.5;
  headings slightly tightened; values/row names600, labels500, metadata400. Tabular dynamic numbers.
- Base grid4px. Standard gaps8/16/24/32px; micro gaps4px. Dense panel inset16px;
  main inset24px desktop/16px phone; major sections24–32px. Radius6px controls,8px panels,
  12px dialogs. Later explicit direction refines the original32/20/16/14 and12px-all-card baseline.

## Shared patterns

| Pattern | Measurements / behavior |
| --- | --- |
| Shell | One 224px desktop sidebar, 64px utility bar; flexible remaining content; no second rail or settings tab strip |
| Navigation | Exact one current link, blue selected fill; Settings button has expanded state only; nested leaves indented16px; Operations detail selects Operations |
| Button | Standard40px visible, compact36px visible, text14/500, horizontal12–16px, radius6px; effective targets >=44x44px with no overlap; touch layouts may use44px visible |
| Row actions | Single quiet ellipsis with resource-specific accessible name,44x44px effective area; optional one primary row action; danger revealed deliberately with confirmation |
| DataTable | One semantic table; columns declare start/end/actions alignment; header/cell padding12px16px consistent; typical minimum row56px with growth for detail; action track96px (56px proved too narrow for its label); no universal100px td min-width or blanket nowrap |
| Table identity | Name14/600, path/identifier12/400 muted; paths can wrap safely; numeric columns end-aligned and tabular; headers follow corresponding cells |
| Settings row | Label and supporting summary left; compact relevant action right; wraps vertically on phone; no permanent creation form |
| Dialog | Reuse native Dialog: header/title/close, field body, one footer Cancel + submit; standard width440–480px, max viewport minus32px, max height viewport minus32px with body scroll; trap/restore focus, Escape, field errors; preserve drafts on failed submit |
| Long editor | Routed full page; one form with six expanded numbered folder sections; explanation column200–240px, fields max~640px; stack on phone; no tabs, step wizard or modal |
| Form footer | Cancel and submit aligned end; flow layout, never covers errors/warnings; dirty leave confirmation; busy/errors scoped to actual mutation |
| Status | Mode != protection; preserve exact raw state; Unprotected and At risk use different words/glyphs with explicit exposure; no false health on unknown/stale |

Action menus use native buttons/disclosure/list semantics; use a proven primitive only when true
menu keyboard semantics are required. Do not hand-roll an incomplete ARIA menu. Compactness must
not reduce targets, contrast, keyboard access or readable text. Preserve loading/empty/error/stale,
hover/focus/active/disabled/busy states; motion restrained150–200ms transform/opacity only with
reduced-motion support. Existing auth, CSRF, one live subscription, mutation guards and secret
lifecycle are unaffected. Refetch never resets dirty fields or moves focus.

## Visual references

Project-relative files under `specs/001-core-os-foundation/assets/ui-layout-2026-10-15/`:

- `01-folder-page-nested-navigation.png`: approved single sidebar and full-page folder sections.
- `02-administrators-dialogs.png`: dialog/list reference; its second rail is superseded.
- `03-extension-folder-tables.png`: density/identity/action reference; its inset wizard is superseded.

Follow written requirements where images deviate: only the current leaf selected, all implemented
routes preserved, six sections on one page, Hybrid importance in folder lists, no fictional data.
Review at 1440px/360px, real1280px at 200% zoom, keyboard-only, offline assets and live updates.


## Implementation evidence — first increment

T236–T251 implemented2026-10-15. Native details/summary for nested Settings, one current link;
RowActions uses named native disclosure/group actions portaled above tables. Dialog has a single
footer and form-bound submit. Button boxes are44px, faces36/40px. Action column96px includes
readable heading and44px control; this is a reviewed refinement of the initial56px draft.
Actual macOS desktop/mobile captures and scope are in
`specs/001-core-os-foundation/ui-layout-acceptance.md`.46 unit and5 browser scenarios passed,
with lint/type/build. Full Linux/VM/zoom/novice acceptance and later pages remain pending.

## Logs and activity refinement — 2026-10-15

Intent: administrator scans timestamps/source/severity for an incident, then opens one technical
entry. Hierarchy: concise message preview leads, metadata has stable columns, full trace is secondary.
Palette/depth/surfaces: existing navy/slate/blue, quiet panel, inset code field, raised dialog; no
new accent. Typography:14px preview,12px metadata/monospace details; spacing4px base,16px panel.
Logs: fixed-layout table with minimum readable width; Time200px, Source240px, Severity128px,
Message flexible; one-line ellipsis preview is a native named button with44px target, opens a
bounded scrollable details dialog with complete returned message and metadata. Table alone scrolls
on phone. Never truncate the backing entry or squeeze metadata into vertical letter columns.
Activity: meaningful appliance lifecycle/configuration/health/protection events, not file access or
an audit replacement. Coalesce adjacent identical legacy events into a count/time range without
removing data or combining distinct changes. Emit protection events only when protection changes.

## Durable recovery observation — Phase 13

Reuse `RecoveryPoint` for dashboard configuration coverage, each recovery copy and
folder-data protection. 12 px / 400 / muted caption with a semantic `<time dateTime>`; flows
within its parent so timestamps wrap at phone widths. Label configuration versus
folder-data recovery explicitly. A missing/invalid date says “Not reported — recovery
coverage is unverified”; stale/offline observations are marked Cached. It never infers
Health from an absent point, or uses a configuration timestamp as a folder-data backup.
Keep the existing 4 px rhythm and quiet surfaces; show the timestamp once per copy.
Desktop 1440 px and phone 360 px evidence is in `assets/phase13/` under feature 001. The larger
recovery/editor layout migration remains T252–T255, not part of this copy refinement.
