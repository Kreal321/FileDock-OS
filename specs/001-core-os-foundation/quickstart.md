# Quickstart and Validation Guide: Core Appliance Foundation

**Feature**: `001-core-os-foundation` | **Plan**: [plan.md](./plan.md)

This guide proves the feature end to end on disposable VMs. Implementation details belong in
`tasks.md`. This document only describes how to build, install, and validate.

## Prerequisites

- A Linux build host with Rust stable (edition 2024) and `cargo-nextest`, `protoc`
  (`protobuf-compiler` plus `libprotobuf-dev` on Debian, used by tonic-prost-build), the .NET 10 SDK, Node.js 22 LTS with `pnpm`,
  `buf`, `dpkg-dev`, and Python 3.12 with `uv`.
- For the VM suite: libvirt with `virsh`, `virt-install`, and `qemu-img`, a Debian 13 amd64
  genericcloud qcow2 image (`FD_E2E_BASE_IMAGE`), and Docker for MinIO. See
  `tests/e2e/conftest.py` for the settings.
- Test VMs: Debian 13 amd64 with 4 GB RAM, a **256 GB sparse system disk** (so the System Pool is
  usable under the default 100 GB reserve), and **two extra blank virtual disks** of 20 GB each
  for data pools. The system disk never joins a data pool (FR-062).
- For the small-disk check: a VM with a 64 GB system disk.
- For disaster-recovery tests: an S3-compatible bucket (a local MinIO container is fine) and a
  second identical VM.
- For connector tests:
  - the same MinIO, which enforces `If-None-Match` and `If-Match`;
  - `tests/fixtures/s3-no-conditional`, a small S3 proxy in front of MinIO that ignores the
    conditional headers and can drop or flip bytes on request;
  - an Azurite container (Azure Blob emulator);
  - a Google Drive test account, or the rclone `local` stub family from `tests/fixtures`;
  - the sample third-party connector `tests/fixtures/sample-connector.fdext`, which declares
    `s3-compatible` with `cloud.remote` and no conditional writes.

## Build and package

```bash
make proto        # buf lint + generate Rust/C# stubs from proto/
make core         # cargo build --release (Core Service and fd-s3)
make management   # web UI build (web/) + dotnet publish (management/)
make bundle       # complete offline bundle: FileDock packages + pinned dependencies
make test         # cargo nextest, dotnet test, vitest, contract checks (manifest schema, buf)
```

For the Phase 3 stop point and PVE commands, see [phase3-testing.md](./phase3-testing.md).

## Install and first-run setup (User Stories 1 and 2)

For online installation and future updates, use the small `filedock-bootstrap.sh` release asset. See the [bootstrap guide](../../packaging/installer/README.md). The manual offline installation remains available below.

1. Copy all `dist/*.deb`, `dist/versions.lock`, `dist/SHA256SUMS`, and `dist/install.sh` to a fresh VM, then run
   `sudo bash install.sh --offline-bundle ./dist`.
   - **Expect**: preflight passes, installation finishes, and the console prints the UI address
     and a one-time setup code ([cli.md](./contracts/cli.md)).
2. Run `sudo bash install.sh --check-only` on a 1 GB-RAM VM.
   - **Expect**: exit code `2`, a failed memory check, and no changes on disk.
3. Open the UI from another machine. Enter a wrong code 5 times.
   - **Expect**: throttling. No page other than setup is reachable (`423` from the API).
4. Enter the correct code, then create the administrator and appliance name.
   - **Expect**: the recovery key is shown once; clicking Download recovery key and continue downloads a TXT file and finishes setup.
5. **Expect**: the dashboard shows the Core, the Management Service, the container runtime, and
   the Samba, NFS, rclone, `fd-s3`, and Kopia subsystems as healthy (SC-001).
6. Re-run the installer.
   - **Expect**: a repair with no configuration change, and no new setup code, because setup is
     complete.

## Authentication and tokens (User Story 2)

See [phase4-testing.md](./phase4-testing.md) for the Phase 4 build, PVE and console recovery checks.

- Sign in with a wrong password 5 times.
  - **Expect**: a 15-minute lock, recorded in the audit history.
- Create a Read-only token, then call `GET /api/v1/system/status` and `POST /api/v1/folders`
  with it.
  - **Expect**: `200`, then `403`, with both audited.
- Revoke the token, then repeat the call.
  - **Expect**: `401`.
- Run `sudo filedockctl reset-password admin`.
  - **Expect**: the new password works, and the audit actor is `Console`.

## Pools and managed folders (User Stories 8 and 9)

1. `GET /api/v1/disks` lists both blank disks, and the system disk is flagged.
2. Create pool `media` from both disks, confirming with `ERASE <serial1> <serial2>`.
   - **Expect**: the pool is Healthy, its capacity is the sum of the two disks, and the
     no-redundancy warning was shown.
3. Create the folder `Movies`: path `/storage/movies`, Local Only, pool `media`, path `/movies`,
   with SMB, NFS, and Docker access.
   - **Expect**: the Unprotected notice, no importance question, and the folder Ready.
4. Try creating `/storage/movies/extra`.
   - **Expect**: refused, with code `folder.path_overlap` and the subfolder explanation.
5. Write a file over SMB, read it over NFS, and read it from a Compose app that binds
   `/storage/movies`.
   - **Expect**: the same file content everywhere.
6. Deploy an app with a named volume.
   - **Expect**: the FR-087 warning.

## System Pool (User Story 9 scenarios 9–13, SC-022)

1. Run `GET /api/v1/pools/system` on a fresh install.
   - **Expect**: limits of 50% and 100 GB. The effective capacity is about 128 GB, limited by
     maximum usage. It is writable, and no disk selection was needed.
2. Create the folder `Notes`: path `/storage/notes`, Local Only, System Pool, path `/notes`, with
   SMB access.
   - **Expect**: the `system_pool.shared_with_os` warning, then the folder Ready.
3. Try `relativePath: "/"` for another folder.
   - **Expect**: `422 reference.pool_root_reserved`.
4. Set the limits to 10% and 230 GB, then fill `Notes` over SMB until writes fail.
   - **Expect**: a disk-full error on the client, and the files already written stay readable.
     `df /` still shows at least 230 GB free, because writes stop at the guard band above the
     reserve. There is a warning alert at 90% and a critical alert when full, and 0 files are
     deleted.
5. Lower the maximum usage below current usage.
   - **Expect**: `422 system_pool.below_usage`.
6. With the System Pool partly used, fill the root filesystem outside FileDock
   (`fallocate -l <size> /var/tmp/big`) until it is just above the reserve.
   - **Expect**: the System Pool becomes `Full(reserve)` within seconds, with a critical alert
     naming the reserve. After the file is deleted, it becomes writable again.
7. Try `DELETE /pools/<system-id>` and `POST /pools/<system-id>/disks`.
   - **Expect**: `422 system_pool.immutable`.
8. On the 64 GB VM, install.
   - **Expect**: the System Pool is Unavailable, with the reason "system disk too small for the
     100 GB reserve". Folder creation there is refused, and data pools still work.

## Data-plane independence (User Story 3, SC-010)

- Run `systemctl stop filedock-management`.
  - **Expect**: SMB, NFS, and app I/O continue.
- Run `systemctl kill -s KILL filedock-core`.
  - **Expect**: shares keep serving. The Core restarts, reconciles, and reports healthy.
- Reboot.
  - **Expect**: pools, bind mounts, exports, and apps return without intervention.
- Run the failure-injection suite: `uv run pytest tests/e2e -m failure_injection`.
  - **Expect**: 0 corrupted state records.

## Secrets and appliance state (User Story 4)

1. Before registering any remote destination, run `GET /api/v1/state-protection`.
   - **Expect**: local copies exist on pool `media` and on the system disk, with no setup needed.
     The coverage is reinstall: yes, machine loss: no, and the dashboard warns about the missing
     remote destination.
2. Register the bucket as the remote appliance-state destination.
   - **Expect**: no acknowledgement is asked, because MinIO through `fd-s3` is conditional claim
     safety. `_filedock/owner.json` is created with generation 1
     ([appliance-state-layout.md](./contracts/appliance-state-layout.md)). Machine-loss coverage
     becomes yes.
3. Change a setting, create a share user, and revoke a token.
   - **Expect**: within 1 minute, every copy (`media`, the system disk, and the bucket) has a new
     snapshot, and the dashboard shows each copy's time.
   - Then sign in again and acknowledge an alert. **Expect**: no new snapshot, because audit and
     alerts aren't configuration (FR-038).
4. Decrypt the newest snapshot with the test helper, `uv run tools/fdstate-inspect --key …`.
   - **Expect**: the snapshot contains `manifest.json`, `config.json`, `secrets/secrets.enc`, and
     `secrets/dek.wrapped`, and no database files.
   - **Expect**: `config.json` validates as `fdconfig/1`
     ([appliance-state-layout.md](./contracts/appliance-state-layout.md)). It contains the new
     share user and no longer contains the revoked token. Pool `media` appears with its id, name,
     and layers, but no disk serials or device paths.
   - **Expect**: there are no audit entries, alerts, operations, snapshot records, sessions, or
     setup code hash anywhere in the archive.
5. Make the `media` copy unwritable, for example by filling the pool, then commit a change.
   - **Expect**: after 5 minutes that copy shows stale with a warning alert, and the other copies
     are current.
6. Grep the logs, events, the diagnostic bundle, and the snapshot bytes at every copy for the
   planted canary secrets.
   - **Expect**: 0 hits (SC-007).

## Reinstall with only the recovery key (User Story 5 scenarios 8–11, SC-021)

1. On VM A, with pool `media`, folder `Movies`, the bucket destination, and some files: note the
   time, wait more than 15 minutes, then reinstall Debian 13 on the system disk, erasing it. Keep
   both data disks attached. Run `install.sh`.
   - **Expect**: the installer prints `Found appliance state for "filedock-home"`, with the newest
     snapshot time and "on pool media". It asks only for the recovery key.
2. Enter a wrong key, then the right one.
   - **Expect**: the wrong key is refused and nothing changes. With the right key, restore runs
     without any setup code, credentials, or disk mapping.
3. Open the UI.
   - **Expect**:
     - sign-in works with the old administrator password;
     - `media` is reattached and `Movies` serves the same files over SMB;
     - the System Pool is recreated with its old limits, and `Notes` is listed as lost (Local Only
       on the erased disk);
     - the bucket destination is Writer with generation 2;
     - the audit log starts fresh with `appliance.restore`, followed by the automatic takeover
       with the reason `reinstall-restore`;
     - the extensions installed on VM A are Running again at the same versions, reinstalled from
       the stored packages with no upload, and their bound folders still hold their data;
     - the whole restore took under 30 minutes from starting the installer.
4. Repeat, but make a change on VM A less than 15 minutes before reinstalling.
   - **Expect**: the restore succeeds, but the bucket location waits in Recovery reader for the
     takeover confirmation.
5. Repeat, with a newer snapshot only in the bucket (block the `media` copy before the last
   change).
   - **Expect**: the bucket's newer snapshot is preselected and restored.
6. Repeat, but run `install.sh --yes` so the console prompt is skipped.
   - **Expect**: the first-run page offers "Restore filedock-home found on this machine" and
     accepts only the recovery key.

## Disaster recovery on new hardware and takeover (User Story 5, SC-006, SC-015, SC-016, SC-019)

1. Destroy VM A, or keep it running for the takeover test. On VM B, run the installer, then choose
   **Restore Appliance** with the bucket and the recovery key.
   - **Expect**: VM B acts as a Recovery reader. Snapshots are listed with integrity flags, and
     nothing is written to the bucket. MinIO's request log shows no PUT, DELETE, or multipart
     abort under the location's prefix, including from VM B's connection re-probe.
2. Restore the latest snapshot, then map pool `media` to VM B's two blank disks.
   - **Expect**: disk assignment is the only pool input; the pool name, id, and layers come from
     the backup. Folders, shares, users, apps, connections, and extensions match VM A.
     Extensions are reinstalled from the bucket's stored packages. `Movies` is recreated empty
     and listed as lost (Local Only). Hybrid Backup folders, including extension data folders,
     rehydrate from the cloud. The System Pool is recreated without mapping. The restore
     finishes in under 60 minutes.
   - **Expect**: the restore report lists every folder in recovery order with its outcome.
     `Movies` is `Lost`, and the snapshot time after which changes are lost is shown. The report
     stays on the dashboard until dismissed.
3. Confirm the takeover. If VM A wrote less than 15 minutes ago, a warning appears first.
   - **Expect**: the generation advances by 1, and the takeover is audited. The restore came from
     a remote source, so there is no automatic takeover.
4. With VM A still running, trigger a configuration change on it.
   - **Expect**: VM A sees Ownership lost at its next check, writes nothing, and raises a critical
     alert. Its daily probe and upload sweep also make no writes at the location.

## Configuration backup file (FR-111)

1. On VM A, run `filedockctl state export /tmp/a.fdbackup`, or use **Download backup file** in the
   UI.
   - **Expect**: a `.fdbackup` that `tar -tf` lists as one copy root
     ([appliance-state-layout.md](./contracts/appliance-state-layout.md)), with one `.fdstate`
     and the needed `.fdpkg` files, no plaintext configuration, and an `appliance.backup_exported`
     audit entry.
2. On a fresh VM C with no remote destination configured, run the installer, choose **Restore
   Appliance → Backup file**, and upload the file with the setup code and the recovery key.
   - **Expect**: a wrong recovery key is refused. With the right one, the same restore flow runs
     as from the bucket: assign disks to missing pools, extensions come back from the file's
     packages, and nothing is taken over automatically.
3. Delete one `.fdpkg` from a copy of the file, then restore again.
   - **Expect**: only that extension is `PackageNeeded`. Uploading the same version through
     `POST /extensions/{id}/package` brings it back with its recorded configuration and bindings.

## Cloud connectors and routing (User Story 10, SC-023 to SC-026)

Contracts: [core-api.proto](./contracts/core-api.proto) `Connectors`,
[extension-protocol.proto](./contracts/extension-protocol.proto) `CloudRemote`, and
[management-api.md](./contracts/management-api.md) `/connectors` and `/connector-routes`.

1. On a fresh install, run `GET /api/v1/connectors` and `GET /api/v1/connector-routes`.
   - **Expect**: `fd-s3` and `rclone` are enabled, and the defaults match research R13. No
     configuration was needed (US10 scenario 1).
2. Create an S3-compatible connection to MinIO, then run `POST /connections/{id}/test`.
   - **Expect**: `connectorId` is `fd-s3`. Every check passes, including conditional create and
     replace, and the clock skew is reported (scenario 2).
3. With the MinIO destination from "Secrets and appliance state", trace 20 snapshots and the claim
   checks. Also create a Google Drive (or stub) connection and list it.
   - **Expect**: 100% of bucket requests come from the `fd-s3` unit, and 100% of Drive requests
     from `rclone rcd` (SC-023).
4. Preview routing `s3-compatible` `OwnedObjectStorage` to `rclone`.
   - **Expect**: the impact lists the MinIO connection and the appliance-state location, shows
     `conditional_writes` as lost and claim safety becoming `Basic`, and requires
     `acknowledgedBasicClaimSafety`.
   - Apply it without the acknowledgement. **Expect**: `422 route.ack_required`.
   - Apply it with the acknowledgement. **Expect**: the change is audited, the location shows
     `Basic`, and the dashboard shows the warning (scenario 3, FR-123).
5. Switch the route back and forth 10 times while snapshots are being taken every few seconds.
   - **Expect**: every running upload finishes on the connector it started with. Comparing
     object listings and ETags before and after shows 0 objects rewritten or lost. Every snapshot
     decrypts, and the cross-connector read test passes (SC-024).
6. Create a second S3-compatible connection, through `s3-no-conditional`, and override only that
   connection to `rclone`.
   - **Expect**: only that connection moves. The first stays on `fd-s3`. The route table and
     connection list show the override. A later change to the family default leaves it alone
     (scenario 9).
7. Register a location through `s3-no-conditional`, routed to `fd-s3`.
   - **Expect**: the probe reports conditional create as unsupported, and registering needs the
     basic claim safety acknowledgement.
8. With two VMs, claim the same empty MinIO prefix through `fd-s3` at the same moment, 100 times.
   - **Expect**: exactly one owner in every run (SC-025).
9. Through `s3-no-conditional`, flip bytes on upload and on download, and truncate an upload.
   - **Expect**: every affected `Put` and `Get` fails (`DATA_LOSS` or read-back mismatch), and 0
     corrupted objects are reported as success (SC-026). Kill `fd-s3` during a multi-part upload.
     **Expect**: after restart, the sweep aborts the incomplete upload.
10. Install `sample-connector.fdext`.
    - **Expect**: approval shows `cloud.credentials`, and no route changes. Routing
      `OwnedObjectStorage` to it requires both acknowledgements, and the location then shows a
      third-party connector. Only the connections routed to it appear in its tmpfs secret
      directory (scenario 4, FR-119).
11. Stop the routed connector, then trigger a snapshot.
    - **Expect**: that copy fails with the alert `connector.route_unavailable`, naming the
      connector and route. Nothing is sent through another connector (scenario 5).
    - Disable a routed connector. **Expect**: `409 connector.in_use` with the routes (scenario 6).
12. Register an Azurite container as a destination.
    - **Expect**: it is served by `rclone`, and accepted only with the basic claim safety
      acknowledgement. The standing warning is shown.
13. Restore from the reinstall test above, with an override and a third-party route in place.
    - **Expect**: the connectors, routes, overrides, and acknowledgements return as recorded. The
      remote destination is read through `fd-s3` until the sample connector is reinstalled from
      its stored package (scenario 8, FR-122).

## Pool failure (SC-018)

- Detach one virtual disk from the running pool `media`.
  - **Expect**: within 1 minute, the pool is Failed, its folders are Unavailable, and a critical
    alert lists each folder with its recovery path.
- Attach two new blank disks, then run `POST /pools/{id}/recreate`.
  - **Expect**: the same pool ID, and the folders come back empty and Ready.

## Alerts and notifications (SC-020)

- With no notification extension installed, **expect** the dashboard notice that alerts are
  visible only in the UI.
- Install the test extension `tests/fixtures/notify-test.fdext`, which records deliveries, and
  configure it at minimum severity Warning. Then trigger a critical condition.
  - **Expect**: an active alert in the UI, delivered to the extension, both within 1 minute.

## Extensions (User Story 6, SC-008)

- Install `tests/fixtures/sample-native.fdext`, `sample-systemd.fdext`, and `sample-docker.fdext`.
  - **Expect**: permissions shown for approval, and capabilities listed.
- Kill the sample native extension 20 times.
  - **Expect**: 0 Core restarts. Backoff applies, and the extension is marked Failed after the
    threshold.
- Upgrade it to `sample-native-broken.fdext`.
  - **Expect**: an automatic rollback to the previous version.
- Install `sample-docker.fdext`, which declares a `data` path, without binding it.
  - **Expect**: the install is refused until the path is bound. The inspect step proposes a new
    Hybrid Backup folder at the manifest's suggested importance.
- Bind it to a Local Only folder.
  - **Expect**: `422 extension.data_unprotected` until the acknowledgement is given. After that,
    the extension is listed with a "data not protected" warning.
- Inside the running extension, write to its bound path and to `/var/data-elsewhere`.
  - **Expect**: the first write lands in the bound folder. The second fails, and the failure is
    logged (FR-109).
- Try to delete the bound folder.
  - **Expect**: refused while the binding exists.
- Check every appliance-state copy.
  - **Expect**: each holds one `packages/<sha256>.fdpkg` per installed extension version (FR-110).
