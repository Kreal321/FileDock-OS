# Phase 12 PVE validation — 2026-10-19

## Deployment and conclusion

Runtime under final test: **test-0.1.0-37-1**, BUILD.txt commit
`d5ff96ff373b89e75d800794a643cb263474bd0a`, Debian 13 amd64, installed through the
bootstrap updater from the completed GitHub offline release. Several earlier release
iterations were used to find and repair integration defects. T164–T176 are implemented;
**T177 remains open** because the complete independent acceptance test is not yet established.
No credentials, fixture secrets, recovery keys or snapshot plaintext are recorded here.

## Fixes found by live testing

- Native Get now preserves ETags, allowing production ownership and index conditional updates.
- Protobuf numeric bandwidth values parse correctly. MinIO's checksum-specific error is
  recognized by the negative checksum challenge.
- Empty extension bindings no longer produce an invalid systemd BindPaths assignment.
  Connector extensions accept the empty initial Configure envelope before routing exists.
- A socket epoch detects native restarts that begin and end between supervisor polls, so
  startup multipart cleanup still runs.
- MinIO's pinned server returns an empty nonempty-prefix multipart listing. The native
  sweep retries a read-only bucket listing and retains the original prefix/age filtering
  for every abort. If that listing is denied, cleanup fails closed.
- An rclone HTTP 200 stat response containing `item:null` is treated as an absent object;
  this enables a fresh Azure ownership claim.
- Snapshot session construction now waits for the atomic route lock/configuration instead
  of timing out after one second and falsely marking the copy unavailable. Regression
  tests hold the lock beyond that old timeout and verify both eventual success and
  real unavailable/non-Writer refusal. Provider RPC/delivery deadlines remain.
- Native Get streams in 64 KiB messages instead of exceeding gRPC's default 4 MiB message
  limit when reading an encrypted extension package. A large-read regression is included.
- A concurrent Management session deletion no longer raises a database concurrency exception.
- The Makefile runs the actual xUnit executables: the previous MTP solution command found
  zero tests and therefore was not usable as evidence.

## Local checks

| Check | Result |
| --- | --- |
| `cargo test --workspace --offline` after the final runtime change | 124 passed, 3 ignored; no failures |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | Passed |
| Real isolated Core/Management `tests/integration/phase12_smoke.py` | Passed |
| Management API xUnit executable | 26 passed |
| Management unit xUnit executable | 14 passed |
| Management API/CLI build | Passed, zero warnings |
| Web tests | 56 passed across 18 files |
| Web production build and lint | Passed |
| Python compilation, fixture shell syntax, `git diff --check` | Passed |
| `python3 tests/integration/phase12_fixture_checks.py` | Passed; default condition removal and explicit preservation |

The three ignored Rust cases are the externally provisioned cross-connector test and
child-process helper entry points. They are not counted as passing provider acceptance.
The live production ObjectIo checks below were run separately on Linux.

UI review used the production frontend with an isolated API at desktop 1440 and phone 360.
Preview acknowledgement gating, compact row actions, one sidebar, mount interruption copy,
Escape/discard and focus return were checked. Captures: [desktop preview](assets/phase12/route-preview-desktop.png),
[phone preview](assets/phase12/route-preview-phone.png), [desktop routes](assets/phase12/routes-desktop.png),
[phone routes](assets/phase12/routes-phone.png).

## Live acceptance matrix

Fixtures used trusted TLS and random disposable accounts: MinIO built from official tag
`RELEASE.2025-10-15T17-29-55Z`, the bundled fault proxy and the sample connector rebuilt
from the installed release's native binary. Azurite image digest:
`sha256:830430c1da1a2d537e08f3e6764dd1f5ae00cf0346bcaf625b968ec3f0971fd5`.
Azurite used `--skipApiVersionCheck` because the current SDK/rclone API versions exceed
its supported version; this is a fixture compatibility flag, not an appliance setting.

| Quickstart step | Observed result / limit |
| --- | --- |
| 1: default connectors/routes | Built-ins Running; S3 owned storage → fd-s3, mount → rclone, backup → backup engine. REST and root CLI list/routes checks passed. |
| 2: native MinIO probe | Conditional create/replace and checksum challenge passed. `filedockctl --json connections test fd-phase12-primary` exited 0 on release 36. |
| 3: request routing | Native/rclone production adapters exercised against MinIO; Azure requests through rclone verified. Twenty Google Drive stub list probes passed; all 20 outgoing connections were attributed via Linux ss to the installed filedock-rclone MainPID. Synthetic token only; TLS validation enabled. On release 37, all 311 S3 requests during 20 snapshots were attributed to the installed filedock-s3 MainPID. Thus both requested backend paths have live process attribution; Google used the allowed stub, not real OAuth. |
| 4: preview/acknowledgement | Impact lists connections/locations and lost conditional writes; missing acknowledgement rejected. Audited apply and Basic standing warning passed. |
| 5: switch/snapshot stability | Final fresh-bucket run: 20 snapshots with two-second pauses during 10 switches with one-second pauses, sample package >4 MiB installed. Every operation succeeded; pre-existing immutable ETags unchanged. All 21 resulting snapshots authenticated, decompressed and parsed. Production native↔rclone reads/writes passed for owner, index, key, snapshot and package object types. |
| 6: override | Connection override retained precedence across family-default changes. |
| 7: Basic provider | Proxy ignored conditional headers; Basic acknowledgement required; standing warning shown. Foreign root probe remained read-only with unchanged object listing/ETags. |
| 8: claims | 100 paired claims through **two isolated native processes on one VM**, using production ownership/ObjectIo against real MinIO: exactly one recorded owner each round. **The specified separate two-VM topology is pending.** |
| 9: faults/multipart | Upload/download flip and truncation never reported success; corrupt Get returned DATA_LOSS. Installed fd-s3 killed during a 65 MiB multipart upload; supervisor restarted it automatically. Fresh upload preserved; after fixture ageing to 25 hours and another restart, owned aged upload swept; out-of-prefix upload preserved. No 24-hour wait is claimed. |
| 10: third party | Inspection requested cloud.credentials; install changed no routes; neither single acknowledgement sufficed. Both acknowledgements accepted, ThirdParty location shown, repeated snapshots passed, only routed connection secrets visible. Routed disable refused; unrouted disable/re-enable returned to Running. Native then read the >4 MiB existing package successfully. |
| 11: no fallback | Mask/stop of routed native unit made probe return 503 and remote snapshot fail with state.copy_pending; route-unavailable alert named connector/route and remote listing stayed unchanged. Routed disable returned 409. Unit unmasked/restarted afterwards. |
| 12: Azure | Actual Azurite destination through rclone accepted only with Basic acknowledgement; snapshot and standing warning passed. |
| 13: recovery | Separate disposable source/target instances on this VM restored using only their own test recovery key and exported backup. Routes/defaults/overrides/acknowledgements matched; encrypted stored package survived. Runtime disabled by design, so target reported AwaitingRuntime. **Remote-S3 bootstrap read followed by automatic extension reinstall is not established by this file-backup test.** |

### Observations not hidden by the final pass

An earlier large-package concurrent run returned one `state.copy_pending`. A subsequent
20-snapshot run passed all operations, but its final listing comparison encountered normal
retention pruning in the already-used bucket (32 retained copies). The final fresh-bucket
run avoids that retention confound and passed all operations and immutable-object checks.
Source inspection identified a one-second timeout that included waiting for the atomic
route lock/configuration. Commit d5ff96f removes that false-unavailability path; the
regression holds the route lock for 1.1 seconds and succeeds after release. A final
release-37 stress retest is recorded below; this does not claim an unlimited soak test.

A HybridBackup folder was correctly refused because `backup.restore` and `backup.retention`
are missing. Therefore live **backup-only** credential materialization has not been verified;
the acceptance case explicitly skips when this provider is unavailable. Source routing
limits ordinary credentials to actual referenced uses, but source inspection is not a
substitute for that live check. Do not advertise feature005 capabilities merely to pass it.

CloudMount reconciliation is queued and previews warn of interruption. Research R13 places
the actual non-local mount provider contract in feature004; feature001 truthfully reports
those providers NotReady. A real cloud mount restart has not been tested here.

## Reproduction entry points

Build the offline bundle using the repository Makefile/release workflow. The bundle includes
`test-fixtures/phase12/sample-connector.fdext`, the fault proxy and the Linux
`phase12-live` driver. The driver uses actual production adapters; `FD_LIVE_KIND=claims`
requires two configured fixture connector sockets, while `cross` requires a configured
native socket and rclone remote. See its source in
`core/crates/fd-connectors/examples/phase12-live.rs` for the exact FD_LIVE variables.

The provisioned-VM suite is `tests/e2e/test_connectors.py`; its module documentation and
`tests/e2e/conftest.py` describe required environment variables. Missing fixtures are
explicit skips. It is not a one-command setup for a production appliance: use a disposable
VM, fixture credentials and recorded cleanup. Existing VM state must be preserved.

## Cleanup and reboot

The disposable sample extension, remote destination, connection overrides, owned locations,
connections, vault references, MinIO objects/buckets and Azurite container were removed.
Baseline family routes restored; no folders/users/apps were created. Only the recorded test
owned-location entries were removed from Core's internal setting with Core/socket stopped;
no user resources or audit/history were erased. Two obsolete test-location downgrade alerts
were resolved. The test CA was removed and the system certificate store refreshed.
System Pool remains generation 5, 50% maximum usage, 100,000,000,000-byte reserve.

Release 36 cold-start reboot passed with all eight authenticated dashboard components
Healthy, native/rclone Running and no manual starts. Boot ID changed, static address,
gateway and DNS persisted, runtime directory was root:filedock-mgmt 0770 and the
Management admin socket was filedock-mgmt:filedock-mgmt 0600. Release 37 also passed a full reboot without manual service starts: all eight
components Healthy, native/rclone active, Management NRestarts=0, UI HTTP 200,
static address/gateway/DNS retained, runtime ownership/modes correct, System Pool
limits/generation unchanged. Boot ID changed to d11c9711-859a-4fab-9a70-7c2faa37830e.
The private diagnostic tree, fixture keys/cookies/ciphertext and transferred scripts
were removed, and the final administrator session was signed out.

## Final release-37 retest

- Bootstrap upgrade exited 0 and BUILD.txt matched `d5ff96f` exactly; GitHub run 37 succeeded.
- Third-party approval, required acknowledgements, routed-secret isolation, repeated
  snapshots, native reads of the existing >4 MiB package, and disable/re-enable all passed.
- Native-only window: **20 snapshots, 311/311 S3 requests attributed to the installed
  filedock-s3 MainPID**. Drive stub: **20/20 requests attributed to filedock-rclone MainPID**
  again after upgrading. These are actual backend requests, not merely route labels.
- Fresh separate bucket: **20 successful snapshots during 10 route switches**, with
  pauses to sustain concurrent work and the large sample package installed. Existing
  immutable ETags unchanged; **all 21 resulting snapshots authenticated/decompressed/parsed**.
- An additional **26 native-window snapshots** authenticated/decompressed/parsed. Keys
  stayed in process memory; only ciphertext was saved temporarily and is removed in cleanup.
- Final cleanup again removed disposable resources/secrets/objects and signed out.
  Resource baseline is one ConnectorRoutes, one Pool, one Settings; no active alerts.
  Temporary proxy drop-in, CA, fixture units, Azurite container/image, isolated recovery
  directories and the sample extension's unused test service account were removed.

## Remaining acceptance work (T177)

1. Strict two-installation/two-VM claim test against the same MinIO prefix.
2. Live backup-only credential materialization once the HybridBackup provider is available.
3. Remote S3 recovery/bootstrap with the third-party stored package reinstalled automatically.
   The existing isolated harness disables runtime deliberately. Enabling a second full
   appliance runtime on this VM would share the hardcoded System Pool image/unit names;
   use a fresh disposable VM or a properly isolated runtime harness for this check.
4. CloudMount live restart in feature004's provider acceptance, as scoped by research R13.

No spec-kit extension hook file exists at this checkpoint; no after-implement hooks apply.
