# Logs, activity and expected restore refusals — 2026-10-15

## Changes

- The local admin endpoint maps expected RPC failures to Problem Details; `restore.target_not_fresh`
  is409 with a clear title rather than an unhandled500 stack trace. Unexpected Internal/Unknown/
  DataLoss RPC errors still reach the exception handler. Fresh-target checks remain authoritative.
- The snapshot tick no longer emits protection activity unconditionally. The existing run-loop
  comparison remains the only publisher, so unchanged two-second polls add no such events.
- Activity describes meaningful appliance jobs/configuration/health/protection events, distinct
  from technical Logs and administrator Audit. Consecutive identical legacy entries are grouped
  on the current page with counts/ranges; canonical payload comparison preserves different changes.
  History is not deleted, pagination is retained, and distinct transitions are not merged.
- Log rows use stable Time/Source/Severity widths and a one-line message preview. Native named
  buttons open the complete API-returned text and metadata in a bounded scrollable dialog. Only
  the table scrolls horizontally on phones. Source remains inspectable in details. The existing
  API message cap/redaction is unchanged; UI truncation does not modify the returned message.
- Management console prefixes fail/warn/crit/debug are recognized when journald stdout priority
  would otherwise call them Info. Error/severity filters now use that corrected level. Genuine
  warnings/errors are retained; no exception or security warning is hidden just to reduce noise.

## Validation

- Rust `cargo check -p filedock-core --offline`: passed.
- Initial .NET solution build passed; the final API build with `--no-restore --disable-build-servers
  -m:1` passed with zero warnings/errors. One redundant stalled build was terminated afterward.
- Frontend:48 unit tests, lint, TypeScript/Vite production build and formatting passed.
- Chromium:2 focused browser scenarios passed at1440px/360px. Verified actual overflow ellipsis,
  complete returned stack trace in dialog, stable header height,44px message target, no document
  horizontal overflow, dialog bounded to viewport, Escape/focus return and legacy activity grouping.
- macOS screenshots below were inspected. They are synthetic fixtures, not PVE/backend runtime or
  canonical Linux acceptance. Existing unrelated UI tasks remain at their prior completion states.

[Desktop logs](./assets/log-details-2026-10-15/logs-1440.png) ·
[Desktop details](./assets/log-details-2026-10-15/log-details-1440.png) ·
[Phone details](./assets/log-details-2026-10-15/log-details-360.png)

No VM deployment, release, data deletion or restore was performed. Install the updated build and
restart services to apply backend fixes. Previously stored log/activity rows remain subject to
existing retention; UI grouping makes repeated activity readable in the meantime.
