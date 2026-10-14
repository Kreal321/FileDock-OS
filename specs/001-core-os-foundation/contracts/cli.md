# Command-Line Contracts

## `install.sh` (the appliance installer)

```text
curl -fsSLO https://get.filedock.dev/<release>/install.sh
sudo bash install.sh [--release <version>] [--offline-bundle <path>] [--yes] [--check-only]
```

| Phase | Behavior | FR |
|---|---|---|
| Preflight (read-only) | Checks for Debian 13 amd64, at least 2 GB RAM, at least 16 GB system disk, network or an offline bundle, and no conflicting services (existing Samba or NFS configuration not owned by FileDock, Docker from other sources, port 8021 in use). On any failure it prints one line per check with a fix hint, and exits `2` without changing anything | FR-002 |
| Install | Adds the pinned FileDock and Docker repositories and apt pins from `versions.lock`. Installs `filedock-core`, `filedock-management`, and `filedock-builtins` at exact versions, verifying upstream rclone and Kopia packages by SHA-256. Creates users and groups (`filedock-mgmt`, `filedock-share`), paths (FR-005), units, and the journald cap | FR-001, FR-003, FR-005, FR-006, FR-058 |
| System Pool | On first install, creates the System Pool image on the root filesystem with the default limits (50%, 100 GB). If the disk is too small, it records the System Pool as Unavailable and prints the reason | FR-098, FR-101 |
| Detect state | On a fresh install, runs `filedock-core detect-state` read-only (research R22). If appliance-state copies are found, it prints each appliance's name, newest snapshot time, and location. On an interactive console it asks whether to restore, then asks for the recovery key with hidden input, and nothing else. With `--yes`, or without a TTY, it skips the prompt and the first-run page offers the restore instead | FR-106, FR-107 |
| Setup code | If setup is not complete and no local restore was started, generates a new one-time code, invalidates any previous one, and prints it with the management UI address | FR-020 |
| Repair (re-run) | Reinstalls missing or damaged package files and units, and restores default unit drop-ins. Never modifies `/var/lib/filedock*`, `/var/backups/filedock`, `/etc/filedock` desired state, pools (including the System Pool image), or user data | FR-004 |
| `--check-only` | Preflight only | FR-002 |

Exit codes:

| Code | Meaning |
|---|---|
| `0` | Success |
| `2` | Preflight failed |
| `3` | Install or repair failed. The system is left in its previous or repaired state |
| `4` | Unsupported invocation |
| `5` | Restore from a local copy failed. Nothing was changed, and the first-run page still offers restore and setup |

Example output after a successful install:

```text
FileDock 0.1.0 installed.
Open http://192.168.1.40:8021/ to finish setup.
One-time setup code: 7KQ4-M2XD
```

Example output when reinstalling on a machine with surviving copies:

```text
FileDock 0.1.0 installed.
Found appliance state for "filedock-home":
  newest snapshot 2026-10-11 16:40 UTC, on pool "media" and on the system disk
Restore it now? [Y/n] y
Recovery key: *****-*****-*****-*****-*****
Restoring... pools reattached: media. System Pool recreated. Connections: 2 reconnected.
Restore complete. Open http://192.168.1.40:8021/ and sign in with your existing account.
```

## `filedockctl` (local administration, root only)

The tool talks to the Management Service over `/run/filedock/management-admin.sock` (root,
`0600`), or to the Core over `core.sock`, as noted.

| Command | Purpose | FR |
|---|---|---|
| `filedockctl status` | Service, subsystem, and pool health summary (Core) | FR-019 |
| `filedockctl setup-code` | Prints a fresh setup code if setup is not complete | FR-020 |
| `filedockctl reset-password <username>` | Prompts for a new password, unlocks the account, and is audited as `Console` | FR-026 |
| `filedockctl tokens revoke-all` | Emergency revocation of every API token | FR-021 |
| `filedockctl network reset` | Restores the UI listener to all interfaces, the lockout recovery path | FR-027 |
| `filedockctl diagnostics <out.tar.zst>` | Writes a redacted bundle (Core) | FR-060 |
| `filedockctl recovery-key regenerate` | Same as the UI. Prints the key once and asks for confirmation | FR-034 |
| `filedockctl state export <out.fdbackup>` | Writes the newest snapshot and its extension packages as a configuration backup file. Audited as `Console` | FR-111 |
| `filedockctl state status` | Each appliance-state copy with its last snapshot time and stale flag, plus reinstall and machine-loss coverage (Core) | FR-040, FR-105 |
| `filedockctl restore-local` | Re-runs detection and the recovery-key-only restore from the console, before setup only | FR-106 |
| `filedockctl system-pool limits [--max-usage <pct>] [--reserve <GB>]` | Shows or changes System Pool limits, with the same rules as the UI (Core) | FR-099, FR-101 |
| `filedockctl connectors list` | Cloud connectors with enabled flag, state, families, and the routes that target each (Core) | FR-112, FR-113 |
| `filedockctl routes show` | Family defaults and per-connection overrides, and the claim safety of each owned location (Core) | FR-115, FR-123 |
| `filedockctl connections test <name>` | Runs the routed connector's probe and prints each check (Core) | FR-083, FR-117 |

Exit code `0` means success, and any non-zero value means the error was printed to stderr. Output
can be human-readable or `--json`.

## Diagnostic export

`filedockctl diagnostics OUTPUT` writes a redacted `.tar.zst` to a new private file through the
root-only Management admin socket. Existing files are refused. The export is audited.
