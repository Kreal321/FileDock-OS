# Runtime-directory lifecycle repair

## Scope

Prevent Management admin-socket startup failures caused by `/run/filedock` being recreated
as root:root 0755, and prevent a stale admin socket from blocking the repaired process.
Required ownership is root:filedock-mgmt, mode 0770. Secret directories stay root-only.

## Persistent implementation

- `filedock-core.deb` ships a sysusers definition, the complete vendor tmpfiles rule,
  `filedock-runtime.service`, and a `postinst configure` hook.
- Package configuration creates the service account and applies the vendor directory rules
  on install, upgrade and reconfigure.
- The exact one-line `/etc/tmpfiles.d/filedock.conf` introduced by the preceding installer
  fix is removed. It masked the rest of the vendor rule; other local overrides are retained.
- Core/extension sockets and Core/Management services require the runtime preparation unit,
  ordered after sysusers/tmpfiles initialization and before those FileDock consumers.
- Core and Management reapply root:filedock-mgmt 0770 before starting. Management's fixed
  privileged preparation command runs before its unprivileged application process.
- Management removes only its own admin socket before starting, after systemd has stopped
  the preceding process. Core/connector sockets and the shared directory are not removed.
- The bundled installer uses the same vendor rule on install/update/repair.

Commits: `15538c8` (package/startup lifecycle), `fdfaa9c` (stale admin socket).

## Validation

| Check | Evidence |
| --- | --- |
| Shell syntax and whitespace | Passed |
| Fresh directory/account creation | Rooted Debian sysusers/tmpfiles test passed without changing host accounts |
| Wrong-mode repair | Rooted test changed root directory to root:root 0755; vendor rule restored 0770 and service group |
| Empty runtime tree | Rooted test deleted its temporary `/run`; vendor rule recreated the correct hierarchy |
| Core/Management unit ordering | `systemd-analyze verify` passed for changed runtime/Core/Management units |
| Actual package update | Bootstrap installed `test-0.1.0-25-1`, commit `15538c8`; installed postinst present and obsolete local override removed |
| Installed configure hook | Deliberately wrong live ownership/mode repaired by installed package `postinst configure` |
| Management restart | With stale-socket cleanup applied, deliberately wrong live permissions repaired and UI HTTP 200 returned |
| Core restart | Deliberately wrong live permissions repaired; UI stayed available |
| Admin socket and CLI | Admin `/health` returned `{"status":"ok"}`; `filedockctl --json system-pool show` succeeded |
| Private credentials | `/run/filedock/secrets` remained root:root 0700 |

The first installed restart fault test found a second failure: the old process could not
unlink its socket under bad permissions, leaving `Address already in use`. The ownership
guard itself passed; startup still failed until stale-socket cleanup was added. The combined
fault test then passed. This result is preserved rather than reporting the first attempt as
a successful lifecycle test.

## Final published-release verification

`filedock-bootstrap --repair --yes` successfully installed `test-0.1.0-26-1`, source commit
`fdfaa9c9e338af7a6968a8a72361dfe89eeb963c`, on `192.168.68.104`. This tested the actual
bundled installer and Debian packages, replacing the earlier locally staged socket change.
Repeating the live fault matrix passed package configure, Management restart, HTTP recovery,
Core restart, secret permissions and installed unit ordering checks.

A subsequent full reboot passed **before any manual service recovery**:

- `/run/filedock`: root:filedock-mgmt 0770.
- `/run/filedock/secrets`: root:root 0700.
- `filedock-runtime.service`: Result=success, active.
- Management active, `NRestarts=0`, UI HTTP 200.
- Admin socket `/health`: `{"status":"ok"}`.
- `filedockctl --json system-pool show`: successful, original 50% / 100 GB reserve retained.

Thus actual package update, repair, permission-drift recovery, service restarts and cold boot
passed for this issue. The isolated fresh-root test validates account/directory initialization;
a complete first-run installation on a second fresh VM was not repeated. Native S3/rclone
automatic startup remains a separate previously recorded finding; those services still needed
manual starts after reboot and are not counted as part of this directory-repair pass.
