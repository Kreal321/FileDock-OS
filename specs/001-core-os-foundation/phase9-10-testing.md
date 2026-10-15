# Phases 9–10: operations and managed folders

## Implementation and scope

Phase 9 adds filtered journal queries, activity history, audit filters, diagnostic tar.zst downloads,
UI alerts and acknowledgements, a persistent critical banner, and extension-provided notification
channels. Notification transitions and delivery attempts are durable; failed channels retry with
backoff for up to 24 hours without blocking another channel or removing the UI alert. The
notification fixture records attempt keys so retries can be checked for duplicate delivery.

Phase 10 adds transactional folder validation, mode/importance defaults, guarded edits/deletion,
HTTP CRUD and validation, and the shared six-section editor. Local Only uses a systemd bind-mount
unit. A failed or absent pool produces Unavailable and never falls back to writing into an
unmounted pool directory. Removing a folder stops its owned mount; underlying pool data is kept.

**Dependencies:** pool creation and the System Pool are Phase 11. Local Only end-to-end therefore
needs an already provisioned, mounted pool. Features 002–004 supply the other storage modes. Their
missing capabilities are named and those modes are disabled. Valid restored configuration may be
retained while a provider is missing; it does not become Ready or Protected merely because its
configuration was accepted. Protection observations must come from the owning mode provider.

This prepares delivery #3; it does not certify the pending Phase 8 Linux runtime acceptance, nor
pool/share/container provisioning from Phase 11. HTTP remains port **8021**.

## Build

On the supported Debian 13 amd64 build host:

```sh
make all
make bundle
```

The bundle now contains `test-fixtures/phase8/` with native, systemd, broken-upgrade and
`notify-test.fdext` packages. They are test fixtures, not automatically installed extensions.
The normal release build needs no Docker daemon to generate those packages. Build all fixtures,
including the local immutable Docker image, separately with:

```sh
bash tests/fixtures/build.sh "$PWD/dist/fixtures"
```

Do not run fixture packages on an appliance holding important data. Use the existing disposable
PVE test VM or the disposable libvirt harness. Synthetic pool/folder records in acceptance tests
are restricted to fresh libvirt VMs; do not insert them into an existing installation.

## Phase 9 manual checks

1. Sign in, open Settings → Logs. Filter by `filedock-core.service`, severity, ISO timestamps and
   text. Entries must be newest first. Page forward/back without repeating the cursor entry.
   A missing/unavailable journal must show an error, not a fabricated empty success.
2. Open Activity and Audit. Make an administrator change and find it by actor/action/target/result
   and time. Operational log retention does not remove `app.db` audit history (90 days minimum).
3. Download diagnostics from Logs, or run:

   ```sh
   sudo filedockctl diagnostics /var/tmp/filedock-diagnostics.tar.zst
   tar --zstd -tf /var/tmp/filedock-diagnostics.tar.zst
   ```

   The output file must be new. It contains `configuration.json`, `subsystem-health.json`,
   `services.json` and `logs.json`. It contains neither database files nor secret-store/key files.
   Known vault plaintext and secret-looking fields are masked before archiving. Export is audited.
4. With no configured channel, confirm the dashboard notice says alerts are visible only in the UI.
5. Install `notify-test.fdext`, bind `data` to a disposable managed folder, and acknowledge its
   unprotected policy if Local Only is used. Add a notification channel for that provider with
   minimum severity Warning and empty configuration. Its Send test action must succeed, and its
   bound `deliveries.jsonl` must record the test.
6. Raise a test critical condition using the fresh-VM acceptance test, or a real subsystem failure.
   Within one minute, the notification recorder must receive it. The critical banner must remain
   visible across navigation until the alert is acknowledged or resolved. Acknowledgement must
   also be delivered. Changing minimum severity filters subsequent transitions.
7. Stop the recording extension, raise another condition, and verify the UI alert remains while
   failures are logged. Restart the extension; a retry must reuse the same attempt key. Each
   channel works independently. Removing the channel leaves existing UI alerts intact.
8. Repeat API mutations with a ReadOnly token: expect 403. A Full token may query logs/audit and
   use operations but cannot manage administrator/session-only endpoints.

## Phase 10 manual checks

1. Open Managed Folders → Create Managed Folder. Confirm Identity, Storage Mode, Source/Backend,
   Data Importance (Hybrid Backup only), Access, and collapsed Advanced appear in that order.
   Entering the name displays a generated, read-only `/storage/<name>` path; changing the name updates it.
2. Confirm unavailable modes are disabled with their missing capabilities. With no pools, the form
   explains the pool dependency. It must not silently use an ordinary directory on the OS disk.
3. With a real available pool, create Local Only with no access selected. Confirm the reference is
   a path below that pool; `/`, `/.filedock/`, traversal, nested folders and overlapping references
   are rejected. Refusals identify the conflicting folder/location.
4. Check the resulting bind mount and independent ownership:

   ```sh
   findmnt --mountpoint /storage/YOUR_FOLDER
   systemctl status 'storage-YOUR_FOLDER.mount'
   ```

   Use `systemd-escape --path --suffix=mount /storage/YOUR_FOLDER` for names containing hyphens or
   other escaped characters. Stopping Management must leave the mount and existing I/O intact.
5. Every folder listing must show name, path, mode, access and protection/exposure. Local Only is
   Unprotected. At Risk uses a distinct warning presentation. Hybrid Backup additionally shows
   importance. No recent durable backup or upload observation means At Risk, not Protected.
6. Validate duplicate/nested FileDock paths and local references in both directions; pool root and
   internal paths; Cloud Mount overlap only with Cloud Mount; Tiered cloud overlap; and a backup
   destination inside any managed reference. Cloud-mode execution requires its later provider.
7. Edits require `If-Match` with the current generation. A stale edit is refused. Local reference
   changes and transitions into/out of Cloud Mount are refused; leaving Tiered requires recall.
8. With a Hybrid Backup provider, importance has no preselected value. Critical defaults to 300 s
   and cannot exceed 900 s; Important to 900 s; Rebuildable defaults to no backup (3600 s if enabled).
   Temporary appears only for subfolder overrides. Lowering importance or removing a stricter
   override requires explicit loss acknowledgement; leaving Hybrid Backup removes importance only
   after confirmation. Critical deletion requires typing the folder name. Policy changes are audited.
9. Exports, applications and extension bindings block deletion. Removing Docker access is refused
   while an application still binds the folder. Deleting an unused folder keeps its pool data.
   Export access-rule application belongs to Phase 11, even though protocol selection creates its
   declarative export in the same transaction.
10. Fail/unmount the backing test pool. Within the polling interval the folder must be Unavailable,
    with its mount stopped. Reattach the same pool identity and verify reconciliation restores it.

## Local automated validation

```sh
cargo test -p fd-state -p fd-alerts -p fd-observe -p fd-reconcile -p fd-appstate -p fd-subsystems -p filedock-core
cargo clippy --workspace --all-targets -- -D warnings
dotnet test --solution management/FileDock.Management.sln --ignore-exit-code 8
pnpm -C web test
pnpm -C web lint
pnpm -C web build
cargo build -p filedock-core -p fd-extension-sample
python3 tests/integration/phase9_10_smoke.py
python3 tests/integration/phase8_smoke.py
python3 tests/integration/phase6_7_smoke.py
```

The native Phase 9–10 smoke uses real temporary services and a separate gRPC notification recorder.
Its isolated registry/pool fixtures test API policies, not actual backup providers or mount execution.
It uses a journalctl process fixture for deterministic HTTP log-filter/cursor checks on macOS.
It checks diagnostic redaction with a vault canary, filtered activity, folder validation/create/edit,
generation conflicts, typed deletion, importance confirmations, notification test/critical delivery,
idempotency and acknowledgement delivery.

Linux acceptance after preparing all fixture packages:

```sh
FD_E2E_EXTENSION_PACKAGES="$PWD/dist/fixtures" \
  uv run --project tests/e2e pytest tests/e2e/test_alerts.py tests/e2e/test_extensions.py -v
```

The audit retention test vacuums the journal only on a fresh disposable libvirt VM. It refuses an
existing host. Full Linux mount execution, journal paging/rotation, browser visual acceptance and
provider-backed backup/upload behavior remain VM/integration checks; local compilation alone does
not mark them passed.

## Validation record — 2026-10-14

- 66 focused Rust tests passed; one subprocess helper is ignored in the ordinary test enumeration.
- Workspace Clippy passed with warnings denied; Rust formatting and diff whitespace checks passed.
- 32 .NET tests passed; the Management/CLI build has zero warnings and errors.
- 35 frontend tests passed; frontend lint and production build passed.
- 22 contract tests passed; two buf checks skipped because buf is not installed locally.
- All 18 VM tests collected successfully, including the new notification/audit acceptance tests.
- Real temporary Core + Management Phases 9–10 smoke passed: notification registration/send-test,
  critical and acknowledgement delivery, canary-redacted diagnostics, audit export, mode gating,
  folder validation/create/edit, generation conflicts, immutable references, typed deletion,
  importance acknowledgement, and atomic export dependencies.
- Phase 8 API regression smoke and Phase 6–7 configuration-recovery smoke both passed.
- Shell syntax checks passed for package/fixture builders. Full Debian package construction and
  actual systemd bind mounts/journald retention were not run on this macOS host.
- The in-app browser blocked the temporary localhost preview (`ERR_BLOCKED_BY_CLIENT`), so no
  screenshot or visual-browser acceptance is claimed. The temporary preview services were stopped.
- This implementation turn made no changes to the PVE VM.

## Default service log policy

Management suppresses framework HTTP request, routing, SQL and connection information logs;
framework warnings/errors and FileDock lifecycle information remain enabled. Console entries use
a single line. Audit records remain in the audit database without an extra information log per record.
Core emits each journal event once, retains FileDock information and dependency warnings/errors,
and logs operation start/completion, failure and rollback without progress ticks or payloads.
`RUST_LOG` and .NET `Logging__LogLevel__...` environment settings can enable targeted debugging.
This policy applies after installing the updated services and restarting them; it does not remove
existing journal records.
