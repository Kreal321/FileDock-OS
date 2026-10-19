# WebDAV visibility repair — 2026-10-18

## Implementation and release

- Runtime fix committed and pushed to `main`: `fe125bd8b2961214f7b488ce2689a70eb849f0d6`.
- GitHub Actions run `39180454339` built and published `test-0.1.0-21-1`.
- VM `192.168.68.26` upgraded successfully through `filedock-bootstrap --update --yes`.
  Archive checksum passed; installed `BUILD.txt` identifies the expected commit.
- Installed Core SHA-256:
  `e98d4d6cd64f93b2c3ec64a7e704bb1eebdc6689bee78dca945b9666eb9b571c`.

Each local WebDAV role now uses `--dir-cache-time=0s`, so changes through other role processes,
SMB, NFS and local applications are visible without waiting for the directory cache to expire.
Reconciliation also replaces an active transient unit if its running command lacks this policy;
existing exports receive the correction after upgrade.

## Validation passed

- `cargo test -p fd-subsystems`: 9 unit tests and 6 integration tests passed.
- `cargo clippy -p fd-subsystems --all-targets -- -D warnings`: passed.
- `cargo check -p filedock-core`, formatting and diff whitespace checks: passed.
- Extended disposable-VM acceptance coverage warms the reader cache before another role writes,
  then checks SMB-created files, external rename and deletion. Python syntax compilation passed;
  the entire destructive acceptance suite was not run on this existing VM.

## Live installed-release regression

The disposable folder `/storage/fd-p11-561a48` and both WebDAV role units were created **before**
the upgrade. After upgrade, both running commands contained the new flag without manual unit
restarts. Reader/writer PIDs changed from `1539201`/`1539220` to `1542154`/`1542175`.

All checks below passed against the installed release, with no cache-expiry waits:

- Warm reader directory using PROPFIND, then GET a missing unique file.
- Writer PUT creates the file; reader immediately reads the expected bytes.
- Writer overwrites with a different-length body; reader immediately reads the new bytes.
- External rename: old name returns 404, new name returns 200 with correct bytes.
- External deletion: reader returns 404.
- Reader PUT remains denied and creates no file.
- Real smbclient upload: reader immediately sees the file.
- Real SMB rename: reader sees the new name and no longer sees the old name.
- Writer DELETE: reader immediately returns 404.
- Real kernel NFSv4 mount and share UID create/delete: reader sees both changes immediately.
- Offline Docker fixture starts and reads the System Pool canary.
- All eight dashboard components Healthy before reboot.

## Reboot verification in progress

A full VM reboot was initiated after these checks. SSH and Management HTTP have not yet
returned at the original address. PVE console output/current guest IP was requested to diagnose
this separately; reboot recovery is **not passed**. No speculative boot repair has been applied.

The disposable folder, share accounts, Compose fixture and root-only test state are retained
pending reconnection and cleanup. Scripts/state are under
`/var/tmp/filedock-phase11-20261018-retest`. Original resource lists were empty; original
System Pool limits were 50% / 100 GB reserve and were not changed in this run.

Additional data-disk fault/fill tests and real-provider OAuth still require the environments
and credentials described in `phase11-testing.md`.
