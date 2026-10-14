# Phase 8 PVE smoke — 2026-10-14

## Scope and result

Tested the existing Debian 13 amd64 appliance at `192.168.68.26`, running FileDock
packages `0.1.0` and `acl 2.3.2-2+b1`, through its real HTTP Management API and Core.
This is a **partial smoke check**, not a completed Phase 8 acceptance test.

## Defect found and corrected

Uploading an extension to `POST /api/v1/extensions/inspect` initially returned HTTP 500:

```text
Read-only file system: /var/lib/filedock/extensions/inbox/<upload>.tmp
```

The Management service had `ProtectSystem=strict` but only `/run/filedock` in
`ReadWritePaths`. The package staging inbox was therefore read-only inside the service.

- Repository fix: add `/var/lib/filedock/extensions/inbox` to `ReadWritePaths` in
  `packaging/systemd/filedock-management.service`.
- VM repair: installed `/etc/systemd/system/filedock-management.service.d/50-extensions-inbox.conf`:

  ```ini
  [Service]
  ReadWritePaths=/var/lib/filedock/extensions/inbox
  ```

- Reloaded systemd and restarted Management. Inspection and its validation checks then passed.
  The override remains in place so this installed release can accept uploads. It grants only the
  intended inbox; it does not disable the filesystem sandbox.

## Passed checks

- Administrator sign-in: HTTP 200.
- System status, Core health, subsystem list, folder candidates and state-protection APIs: HTTP 200.
- All eight system health components Healthy; Core reports `canMutate: true`.
- Built-in Docker subsystem removal refused with `extension.not_removable`.
- Generic default selection for `cloud.remote` refused with `capability.requires_connector_route`.
- Staged-package path traversal refused with HTTP 400.
- Tampered archive, traversal entry and malformed signature each produce `valid: false`.
- A valid unsigned inspection fixture is accepted for inspection.
- Missing exact permission approval refused with `extension.permission_approval_required`.
- Missing unsigned acknowledgement refused with `extension.unsigned_ack_required`.
- Missing required data binding refused with `extension.binding_required`.
- Binding to an unknown folder refused with `extension.folder_missing`.
- No test extension was installed. Uploaded valid inspection fixtures were removed from the inbox,
  and the temporary test sign-in cookie file was deleted after checking final health.

## Pending acceptance

The installed release contains Phase 5 fixtures, but no compiled Phase 8 samples. The VM has no
Rust/C/protobuf build toolchain. Automatic approval review rejected uploading repository source
because that includes private Core/connector code; explicit user approval was requested before
proceeding. No repository source was transferred and no build container was started.

Consequently, these checks remain unrun:

- Actual native-process, systemd-service and Docker extension installation and sandbox execution.
- Unprotected-folder acknowledgement, bound writes and deletion protection in a running runtime.
- Twenty process kills, restart/backoff and zero Core restarts.
- Broken-upgrade rollback and rollback journal recovery.
- Installed-package copies, deduplication/pruning and PackageNeeded recovery uploads.
- Browser interaction and visual UI checks.

The current appliance has no managed folders. Production folder/pool integration depends on later
phases; synthetic folder records must be confined to an isolated temporary test instance or a
fresh disposable VM, not inserted into this installation.
