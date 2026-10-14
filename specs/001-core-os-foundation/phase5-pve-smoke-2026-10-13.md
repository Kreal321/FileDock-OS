# Phase 5 PVE smoke-test record — 2026-10-13

**Result: PASS for the smoke scope below. Full combined P1 acceptance remains pending.**

Target: `192.168.68.26` (`filedock-dev`), Debian GNU/Linux 13.7 amd64, kernel
`6.12.111+deb13-amd64`, KVM VM. Installed `filedock-core`, `filedock-management` and
`filedock-builtins` report version `0.1.0`. The deployed UI index SHA-256 matches the local built
index: `5d2fe7c8df2123f1fb4448d066f36c7b36501a3c62de6a3dfee5d867d2b99902`.
Local checkout HEAD at test time: `8e4470078a9f0679fa370a48cfc6a8890ce6da75`.
This UI match does not establish the backend's exact build commit; no retained release BUILD.txt
was found in the inspected bundle locations.

## Method

Connected through the existing `kyson` SSH account and `su -` to root. Direct root SSH login was
rejected, and `sudo` is not installed. Neither SSH settings nor OS packages were changed.
Authenticated using the existing FileDock administrator, without resetting its password/accounts.
Credentials, session cookies, token secrets and recovery keys are excluded from this record.

Used the guide's disposable `phase5-sample` unit/manifest/read-write HTTP fixture on port 18025,
which was absent before the test. API checks ran on the VM using Python's standard library.
Authenticated SignalR was checked through both SSE and the project's actual Node SignalR client.
Real browser checks used local macOS Chromium against the VM's deployed UI/API, with no request
fixtures or API interception. The browser's network form was edited but **never submitted**.

Initial attempts using a custom probe missed messages because of probe transport/parsing behavior;
the SSE probe was corrected to handle an empty `sync` argument list. The project's SignalR client
received health/sync correctly. These preliminary probe failures are not product failures. The final
API/supervision run completed **17 checks with zero failures**.

## API and supervision results

| Check | Outcome / evidence |
| --- | --- |
| Administrator sign-in | PASS |
| All installed components Healthy | PASS: eight built-ins plus the temporary fixture |
| Authenticated SignalR connection and initial sync | PASS |
| ReadOnly token scope | PASS: operations GET 200, reconcile POST 403 |
| Full token reconcile and persisted operation | PASS: POST 202, detail Succeeded with steps/attempts; completion event **0.615 s** |
| Data plane during Management outage | PASS: fixture PUT/GET retained exact bytes |
| Core crash and re-adoption | PASS: fixture PID **33137** unchanged; PUT/GET still worked |
| Management restart | PASS: authenticated project client automatically reconnected and received health/sync |
| Single fixture failure/recovery | PASS: live failure event **0.283 s**, followed by automatic Healthy |
| Crash-loop backoff and terminal failure | PASS: persisted policy contained five failures over **32.5 s** and diagnostics (window includes the preceding single-kill test) |
| Failed state after Core restart | PASS: terminal Failed persisted rather than restarting indefinitely |
| Explicit repaired-unit recovery | PASS: removing the injected fault and starting the unit restored Healthy; other components remained Healthy |
| Core unavailable | PASS: mutation returned 503 with `core.unreachable`; public health 200 and fixture PUT/GET continued |
| Core automatic recovery | PASS: existing live connection received healthy mutation permission without reconnecting manually |
| Database integrity after failure injection | PASS: both integrity checks `ok`, no foreign-key violations |
| Temporary token revocation | PASS: both tokens revoked; revoked ReadOnly token refused with 401 |
| Sign-out and live-session revocation | PASS: session-expired event and subsequent protected status 401 |

Operation used for the reconciliation check: `01a13798-d445-72e3-9101-157605db4b3d`.
Built-in S3/rclone units report `Restart=no`, so Core owns their supervision policy.

## Real browser results

Five checks passed against the deployed appliance:

1. Sign-in and all nine service cards Healthy with the fixture installed.
2. Actual Core outage disabled Save; dirty listener text and focus remained intact, the field
   remained readable/editable, and sign-out remained available.
3. Core restart re-enabled Save automatically **without a page refresh**.
4. Settings, Network, Operations and Audit navigation worked; healthy network Save was enabled.
5. Browser sign-out returned to Sign in.

After fixture cleanup, a fresh browser session additionally confirmed **eight built-in cards
Healthy**, Settings/Network/Operations/Audit navigation and sign-out.

## Final state and cleanup

- Core socket, Core service and Management service active; `/health` returned 200.
- `/api/v1/health` initially returned 200; anonymous `/api/v1/system/status` returned 401.
- HTTP `/admin/ping` returned 403; Unix admin ping succeeded; admin socket permissions are 600.
- `filedockctl --json` works in the root login shell at `/usr/bin/filedockctl`.
- Removed only the created sample unit, manifest, data directory and its named subsystem/policy rows.
  Port 18025 no longer has a listener. Final registry contains docker, fd-s3, kopia, nfs, rclone, samba.
- Both temporary tokens remain revoked with their audit history. Existing accounts/listener settings
  were retained. Service-failure and reconcile history remains as legitimate test evidence.
- Final database integrity/FK checks passed, and every resource JSON document parsed successfully.
- Temporary helpers removed from the VM. Non-secret API results retained root-owned, mode 600, at
  `/root/filedock-phase5-smoke-results-20261013.json`.

## Checks not performed / acceptance limits

No VM reboot, PVE hard power cut, fresh install/setup/recovery-key flow, interface-version fault,
full Phase 4 lockout/password/session-expiry/listener-restriction suite, actual browser zoom/full
accessibility audit, novice review or canonical Linux visual-baseline approval was performed here.
SMB/NFS and Compose provisioning remain US9 work; this fixture proves only the current independent
supervision/data-plane smoke scope. T233 and the combined P1 acceptance gate remain pending these
broader checks. No production code or package change was needed for this smoke test.
