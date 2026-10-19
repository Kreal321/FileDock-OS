# Phase 11 PVE retest — 2026-10-18

## Result

The five previously reported defects pass the focused installed-VM retest. **Phase 11 is
not fully accepted:** a reproducible WebDAV cross-role file visibility defect remains, and
the hardware/provider acceptance cases listed below are still pending. No application source
or installed binaries were changed during this retest.

## Environment

- VM `192.168.68.26`, Debian 13 amd64, kernel `6.12.111+deb13-amd64`.
- Management HTTP port `8021`; administrator sign-in succeeded.
- Deployed Core SHA-256:
  `51043707b1cd84efad813c61faaaca4704ae1e29e9cd3c357ce77f75c0eeeccd`.
- Only disk: 200 GiB partitioned OS disk `/dev/sda`, serial `drive-scsi0`.
- System Pool baseline generation 3, limits 50% / 100 GB reserve, available and writable.
- Baseline folder, share-user, connection and app lists were empty.
- Disposable folder `/storage/fd-p11-3ef0d5`, two dedicated share users, one locally imported
  tiny Docker image, one Compose app and one credential-free Azure configuration were used.
- Commands ran during October 18 local time. Journal timestamps include October 19 UTC.

## Focused fix verification

| Prior finding | Installed result |
| --- | --- |
| Root disk misclassified | `/disks` now returns `isSystemDisk: true` for `/dev/sda`. No disk erase was attempted. |
| WebDAV mount dependency escaping | Both role units start for the generated hyphenated folder; ports 8443/8444 serve HTTP. Writer PUT returns 201; unauthenticated requests return 401; reader PUT is denied without changing the file. Existing-file reader GET succeeds. See the separate new-file visibility defect below. |
| WebDAV failure removes NFS | A WebDAV rule referencing a nonexistent test group produces `Failed`, detail `getent failed`. SMB and NFS remain Ready; `exportfs -v` retains the folder, a real NFSv4 mount reads its canary, and smbclient reads its canary. Restoring valid rules recovers WebDAV to Ready. |
| Resize with active NFS fails | With SMB/NFS/WebDAV and a Docker binding active, reserve 100 → 101 GB shrinks successfully, then 101 → 100 GB restores the original limits. Both operations have completed steps. Canary bytes survive; Docker reads the mounted data afterward; NFS remains active. |
| Azure chooses schema-less owner | Browser renders Storage account and Storage endpoint. A browser-created connection is persisted as `type: bound`, `connectorId: rclone`. Editing its name in the browser preserves that owner and configuration. Google Drive also renders its schema fields. No actual provider authorization was attempted. |

Resize operations:

- Shrink: `01a15180-d4e6-733a-b6c3-80aa001baef2`, Succeeded, resize/commit both done.
- Growth: `01a15181-0456-7164-8538-35ce654596ff`, Succeeded, resize/commit both done.

## Additional live checks

- Real bind target and pool directory have matching device/inode; a written canary appears
  through both paths.
- Share users reconcile to Ready.
- SMB writer reads/uploads; reader upload is denied.
- NFSv4 reads, a matching share UID writes, root-squash denies root writes, and a `ro` export
  denies the share UID's write with `Read-only file system`.
- A real Docker container built from installed binaries reads the System Pool canary before
  and after resize; no image registry download was required.
- Percentage 9 and reserve 19 GB are rejected without changing the generation. Shrink without
  interruption acknowledgement is rejected.
- A deliberately malformed **test app's** generated Compose file makes an update fail locally.
  Operation `01a15181-9879-70af-b75d-d1c2072af293` is Failed and its `apply` step is `failed`,
  attempts 1, detail `app.image_pull_failed`. The original Compose file was restored immediately.
- Removing the last test export leaves NFS active automatically, with an empty export table;
  no manual NFS start was required.

Automatic approval review rejected the live System Pool DELETE refusal check because an
unexpected validation failure could delete real storage. It was omitted from this run.

## Remaining defect: WebDAV reader misses newly uploaded files

Reproduction on two healthy role listeners for the same local folder:

1. Start both roles and read through the reader endpoint, populating its directory cache.
2. PUT a unique new file through the writer endpoint: HTTP 201.
3. GET that file through the writer endpoint: HTTP 200, correct bytes.
4. GET the same file through the reader endpoint: HTTP 404, `Not Found`.
5. Restart only the disposable reader unit and repeat GET: HTTP 200, correct bytes.

This was reproduced independently after restoring valid rules and after the resize tests.
It is not the initial harness attempt that read a file before uploading it.

Likely cause: each role is a separate rclone VFS process with its own directory cache.
The installed `rclone serve webdav --help` documents a five-minute default directory cache
and delayed detection of changes from another process. The current adapter supplies no cache
override. Cache expiry itself was not timed, so five minutes is the documented default rather
than a measured recovery time in this test.

Follow-up: configure prompt visibility of local changes across role processes and other share
protocols, then verify new files, renames and deletions without restarting services. This is
a functional consistency gap even while both exports report Ready.

## Cleanup and final health

- Original empty folder/user/connection/app lists restored.
- Test Compose project, imported image, share accounts, source files, bind mount and WebDAV
  units removed. No test NFS client mounts remain; export table is empty.
- System Pool limits restored to 50% / 100 GB reserve.
- Temporary API session signed out; plaintext share authentication files and saved session
  JSON removed. Redacted results and helper scripts remain under the root-only directory
  `/var/tmp/filedock-phase11-20261018-retest`.
- Core and Management `NRestarts=0` at the end.
- All eight dashboard components Healthy: Core, Management, Docker, Samba, NFS, Kopia,
  native S3 and rclone.
- Existing recovery-protection and missing-notification-channel notices remain; these were
  not introduced by this retest.

## Acceptance cases still pending

- Additional data disks: pool create/adopt/grow, member failure and recovery/recreation.
- Disposable-machine full-disk and reserve/hard-cap fault injection.
- Full VM reboot recovery.
- Real cloud credentials, provider connectivity and OAuth completion.
- Remaining full Phase 11 acceptance cases in `phase11-testing.md`; this focused repair
  retest does not replace the complete acceptance matrix.
