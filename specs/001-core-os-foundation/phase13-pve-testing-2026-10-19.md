# Phase 13 implementation and PVE validation — 2026-10-19

## Scope and acceptance

Phase 13 adds background scheduling, measured idle-footprint checks, truthful recovery-point
copy, generated OpenAPI contracts, the revised product guide and failure-injection suites.
The existing Debian 13 amd64 development VM has two vCPUs and about 3,921 MiB usable RAM.
It is not a fresh installation or the 2 GiB minimum-hardware acceptance environment.
**T184 and full feature 001 acceptance remain open.** See the separate
[missing features and testing register](missing-features-and-testing.md).

## Implementation

- Core, native S3 and rclone workers use packaged Nice=10 / idle I/O drop-ins. Upgrades
  restart already-running connector workers so the installed settings take effect.
  Samba, NFS, WebDAV and container units retain their existing scheduling.
- Stable subsystem health checks run every five seconds; startup/backoff retain 500 ms
  checks and Kopia retains its existing 30-second interval. SMART inventory supplies cached
  system-disk identities, refreshed on device events and periodic discovery; the two-second
  mount/reserve checks continue. Exact mount/read-only observations now read the kernel table
  rather than forking recurring findmnt commands. Regression tests cover the timing and health matching.
- Shared recovery-point captions distinguish actual reported timestamps, cached observations
  and unreported/unverified state. Configuration recovery is labelled separately from file
  data. Folder captions consume the provider observation, not an invented top-level field.
- Authenticated `/api/v1/openapi.json` is generated from endpoints. Tests compare all 107
  operations across 87 paths and their documented error-response metadata with the contract.
- Section 50 of the product guide uses Storage Mode, actual Data Importance values,
  connectors and routes.
- VM tests measure aggregate cgroup memory/CPU, restart seven runtime units, verify both
  SQLite databases and committed canaries, and exercise three power cuts on an owned
  disposable libvirt VM. Power-cut cases explicitly skip on an existing PVE guest.

## Local validation

| Check | Result |
| --- | --- |
| `cargo test --workspace --offline` | 127 passed, 3 ignored, 0 failures |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | Passed |
| Management solution build | Passed, no warnings/errors |
| Actual xUnit executables (API / unit) | 27 / 14 passed; zero-discovery `dotnet test` is not used |
| Frontend unit tests / lint / production build | 61 tests across 19 files passed; lint and build passed |
| Contract checks | 22 passed, 2 skipped because buf is unavailable |
| E2E collection | 35 collected; collection is not a VM execution result |
| Packaging regression / installer syntax | Passed |
| Isolated native Core/API regression | Passed |

## Idle CPU regression and repair

The original installed release 37 failed the CPU budget. Measuring the actual service
cgroups for 120.53 seconds gave **4.7122% of one core** and **490,594,304 bytes** peak combined
memory. A separate sample attributed most CPU to Core, including frequent child commands.
Release 41 reduced the stable supervisor probing rate: **2.8826%** CPU over 120.51 seconds,
**133,599,232 bytes** peak memory. That still failed the 2% limit, prompting removal of
redundant disk-discovery commands from frequent pool monitoring. No CPU quota or artificial
memory limit is used to force a passing result.

## Installed VM results

Installed through `filedock-bootstrap --update --yes`: **test-0.1.0-43-1**, BUILD.txt commit
`f4fd676fcd233437555828e40993093d01b4ce50`. GitHub release run 43 succeeded; the updater exited 0.

- Generated OpenAPI returned 87 paths with the expected route/error metadata.
- Actual Core, native S3 and rclone MainPIDs showed Nice 10 and idle ionice; systemd
  configuration also matched. This checks running processes after upgrade, not just files.
- Management, Core, Docker, Samba, NFS, native S3 and rclone restarts all passed.
  After every restart: all eight components Healthy, Core writable, both SQLite integrity
  and foreign-key checks clean, every resource spec/status valid JSON, committed synthetic
  connection spec/generation unchanged, and a successful encrypted snapshot.
- Kopia was Healthy after Core adoption. There is no independent persistent Kopia daemon.

Release 43 idle remained above budget: **2.5800% CPU**, **138,543,104 bytes** peak combined
memory over 120.52 seconds. Profiling attributed about 0.9% of one core to Core's child
commands. The next runtime change reads exact mountpoints/read-only options from
`/proc/self/mountinfo`, replacing repeated timeout/findmnt subprocesses while retaining the
two-second reserve/mount checks. Parser tests cover mount options, filesystem options,
escaped paths, missing mounts and malformed input; a Linux-only regression also compares the root observation
with findmnt. That Linux-only test is not part of the macOS test count; installed Linux
reserve/mount behavior is checked separately below.

### Final runtime release 44

GitHub run 44 succeeded. The bootstrap upgrade exited 0; BUILD.txt is
**test-0.1.0-44-1**, commit `c178bfe900176f0cf0e2737ce441019e53f40b4b`.
The OpenAPI/process-priority checks and all seven restart/integrity/canary/encrypted-snapshot
checks passed again on this exact runtime. Core's pool monitor reported the existing
System Pool available/writable after the kernel mount-observation change, with unchanged
generation 5, 50% maximum usage and 100,000,000,000-byte reserve.

Idle sample after a successful snapshot and 35-second settle, with browser signed out:
**120.5105 seconds**, **139,264,000 bytes (132.8 MiB)** peak combined service-cgroup memory,
**1.974996% of one core** aggregate CPU. Both service MainPIDs stayed stable. This passes
the configured limits on this VM, with narrow CPU headroom; it is not minimum-hardware
acceptance or a long-duration idle soak. No limits/quotas were imposed to obtain the result.

Cleanup removed the recorded synthetic connection and its two exact vault references,
created a final snapshot and signed out. Active resources returned to exactly one
ConnectorRoutes, one Pool and one Settings; historical tombstones/audits were preserved.

A full release-44 reboot passed without manual service starts:

- Boot ID changed to `8424dc6d-06f6-4ad6-a2c5-d9a4d2c3992f`.
- All eight authenticated components Healthy; Core writable; both DB integrity checks,
  OpenAPI and actual priority checks passed again.
- Static `192.168.68.104/24`, gateway/DNS `192.168.68.1` retained.
- `/run/filedock`: root:filedock-mgmt 0770. `/run/filedock/management-admin.sock`:
  filedock-mgmt:filedock-mgmt 0600. `/admin/ping` returned `ok:true` over that socket.
- Management NRestarts=0, UI HTTP 200, CLI System Pool available/writable with original
  generation/limits, and zero current-boot Core/Management journal error entries.

The private diagnostic directory (cookies/protobuf helpers/venv/test state), transferred
helpers and isolated local UI preview state were removed. Sessions were signed out.

## UI evidence

The existing light FileDock design system is retained. Recovery metadata uses the shared
caption scale, semantic time elements and explicit unverified text. Desktop (1440 px) and
phone (360 px) reviews checked readable wrapping and the hierarchy between protection state,
recovery point and actions. These macOS captures are evidence, not approved Linux visual baselines.

- [Recovery desktop](assets/phase13/recovery-desktop.png)
- [Recovery phone](assets/phase13/recovery-phone.png)
- [Installed dashboard](assets/phase13/pve-dashboard.png)
- [Installed recovery desktop](assets/phase13/pve-recovery-desktop.png)
- [Installed recovery phone](assets/phase13/pve-recovery-phone.png)

The installed release-44 post-reboot dashboard showed all eight components Healthy and an actual durable
configuration point. Missing off-machine recovery/notification configuration remained visible;
healthy services do not establish configured data protection.

## Limitations

Seven daemon restarts verify configuration integrity and recovery to healthy state. They do
not substitute for power cuts, loaded share/application continuity, a live backup-job kill,
a minimum-hardware run or full fresh quickstart acceptance. No additional data disks or
PVE host-control credentials are available. No destructive tests target the installed OS disk.

## Reproduction entry points

On a disposable libvirt host, configure the image/bundle prerequisites described in
`tests/e2e/conftest.py`, then run:

```sh
uv run --project tests/e2e pytest -v -s tests/e2e/test_idle_footprint.py tests/e2e/test_failure_injection.py
```

Use the provisioned minimum-hardware fixture for the idle gate. Existing-host mode must
not be used as evidence of a fresh install or power cut. The installed-VM run used a
private temporary helper to execute the same measurement/integrity checks and authenticated
snapshot operations, preserving baseline resources and keeping session cookies off the report.
