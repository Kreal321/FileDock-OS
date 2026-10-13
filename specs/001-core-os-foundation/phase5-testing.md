# Incremental Delivery 1: combined Phases 3–5 acceptance

Phase 3 manual acceptance was confirmed on 2026-10-12. Phase 4 implements administrator access (US2); Phase 5 implements independent supervision and live status (US3). This guide is the combined manual gate for US1, US2, and US3. **Manual VM acceptance remains pending.**

The storage/share/application provisioning controllers belong to US9, a later phase. For this increment, use the supplied disposable HTTP read/write subsystem to exercise the independent data plane. This does not certify SMB/NFS shares or Compose application provisioning; repeat those checks with real shares and apps when US9 is implemented. No remote appliance-state backup or disaster recovery is promised by this increment; those arrive in US4/US5.

## 1. Build and install the release

Use a **Debian 13 amd64 VM** in PVE for reboot and power-cut checks. A container cannot demonstrate the VM boot/power-cut behavior. Keep the PVE console available for the deliberate control-service outages and listener restrictions.

From a commit containing Phase 5, run GitHub Actions → **Offline bundle release** → **Run workflow**. Download that run's release archive and its SHA-256 file. Check `BUILD.txt` in the extracted bundle to confirm the source commit. The currently published older release will not include local, uncommitted Phase 5 changes.

Alternatively, build on Debian 13 amd64 with the project toolchains:

```bash
make bundle RID=linux-x64 CONFIG=Release
```

On the target VM as root, from the extracted bundle directory:

```bash
sha256sum -c SHA256SUMS
cat BUILD.txt
bash install.sh --offline-bundle "$PWD" --check-only
bash install.sh --offline-bundle "$PWD"
systemctl status filedock-core filedock-management --no-pager
curl -i http://127.0.0.1:8021/health
curl -i http://127.0.0.1:8021/api/v1/health
```

Both public health URLs must return `200` without credentials. `/api/v1/system/status` contains appliance details and must return `401` without credentials after setup (`423` before setup). The UI must show sign-in when it receives the expected startup `401`.

An existing installation retains its accounts and desired state. On a fresh VM, complete first-run setup: use the one-time code, default administrator name `root` or your chosen name, password plus matching confirmation, and download the recovery-key text file. Save it privately. Sign out, sign in, and confirm the eight built-in components become Healthy. Hard-refresh `http://192.168.68.26:8021/` after an update.

## 2. Administrator security (Phase 4)

Complete the thirteen checks in [phase4-testing.md](./phase4-testing.md#independent-manual-check), including:

- Read-only/Full token scopes, one-time token display, expiry, revocation, and named audit entries.
- Five failed sign-ins and console unlock/password reset.
- Last enabled administrator protection, password confirmation, and session invalidation.
- Idle session expiry and sign-out-all in another browser window. Open event connections are closed when their credentials expire or are revoked; background events do not renew idle sessions.
- Listener restrictions persist across restart; `filedockctl network reset` works from the root console.
- HTTP console endpoints remain forbidden; the root-only Unix socket remains mode `600`.

For a useful Phase 5 token check, `GET /api/v1/operations` must accept a Read-only token. `POST /api/v1/operations/reconcile` must reject it with `403`; a Full token is allowed while the Core connection is healthy.

## 3. Install the disposable data-plane fixture

The new offline bundle includes `test-fixtures/phase5/`. These files are never enabled automatically. Run the following as root **from the extracted bundle directory**:

```bash
mkdir -p /var/lib/filedock-phase5-test/data
install -m 0644 test-fixtures/phase5/sample_server.py /var/lib/filedock-phase5-test/sample_server.py
install -m 0644 test-fixtures/phase5/filedock-phase5-sample.service /etc/systemd/system/
install -m 0644 test-fixtures/phase5/sample.yaml /usr/share/filedock/subsystems/phase5-sample.yaml
systemctl daemon-reload
systemctl enable --now filedock-phase5-sample.service
systemctl restart filedock-core.service
```

The dashboard must show **Phase 5 test subsystem** as Healthy. Its test port is **18025**, separate from the production UI/API port **8021**. From a client machine:

```bash
curl --fail -X PUT --data 'phase5-before' http://192.168.68.26:18025/
curl --fail http://192.168.68.26:18025/
```

Expect the same bytes. The fixture has no authentication and holds only disposable test data; remove it after the check.

## 4. Data-plane independence and re-adoption

On the VM console:

```bash
systemctl show filedock-phase5-sample -p MainPID --value
systemctl stop filedock-management.service
```

Keep reading **and writing** with the client curl commands. They must continue to work while the UI is unavailable. Then:

```bash
systemctl kill -s KILL filedock-core.service
systemctl show filedock-phase5-sample -p MainPID --value
systemctl start filedock-management.service
```

The sample's PID must stay the same across the Core restart. The Core re-adopts the running unit rather than stopping/recreating it. Reopen the UI and confirm Healthy status returns automatically. Check `systemctl show filedock-s3.service filedock-rclone.service -p Restart`: expect `no`, because the Core owns their restart/backoff policy.

## 5. Live failure, backoff, Failed, and recovery

Leave the dashboard open. Kill the sample once:

```bash
systemctl kill -s KILL filedock-phase5-sample.service
```

Without refreshing, its card must change to Degraded within two seconds, then return to Healthy after the delayed restart. Now trigger a persistent crash loop:

```bash
touch /var/lib/filedock-phase5-test/data/crash
```

Expect increasing restart delays (1, 2, 4, 8 seconds for this short crash loop; the policy caps at five minutes), then Failed after five failures within fifteen minutes. Other components must stay Healthy. Inspect the persisted policy and diagnostics as root:

```bash
python3 - <<'PY'
import json, sqlite3
c = sqlite3.connect('/var/lib/filedock-core/core.db')
value = c.execute("SELECT value FROM setting WHERE key='supervision.phase5-sample'").fetchone()[0]
print(json.dumps(json.loads(value), indent=2))
PY
systemctl restart filedock-core.service
```

Failed must survive the Core restart; it must not restart forever. Remove the fault and explicitly start the repaired unit:

```bash
rm /var/lib/filedock-phase5-test/data/crash
systemctl reset-failed filedock-phase5-sample.service
systemctl start filedock-phase5-sample.service
```

Expect automatic Healthy status and a recovery event. Do not use a real data-bearing service for this crash-loop exercise.

## 6. Core unavailable and automatic UI recovery

Stop **both** the service and socket; leaving the socket active would restart the Core when Management probes it:

```bash
systemctl stop filedock-core.socket filedock-core.service
```

Expect an unreachable banner, disabled change controls, and continued sample reads/writes. Authenticated API mutations must return `503` with `core.unreachable`; public `/health` stays `200` because Management is still alive. Sign-out remains available. Then:

```bash
systemctl start filedock-core.socket filedock-core.service
```

Without refreshing, the banner must clear, controls become enabled, and status update. Restart Management with the browser open and confirm the browser reconnects and resynchronizes its views. The interface-major mismatch guard is covered by automated tests; do not alter production binaries just to simulate it.

## 7. Reboot, operation history, and power loss

Save a sample value, then reboot from the console:

```bash
reboot
```

Within three minutes of boot, the sample must serve the saved value and all enabled components must become Healthy. While signed in, inspect `/api/v1/operations` (or use a token). `POST /api/v1/operations/reconcile` returns `202` with an operation ID; its detail endpoint shows the persisted step, attempts, times, and final state. It never erases data or reprovisions a healthy runtime.

For a power-cut check, use a disposable VM with a PVE snapshot. Start a reconciliation request, then use PVE **Stop** to cut power, and Start again. Confirm sign-in, saved sample bytes, Healthy components, and valid operation history. This VM check covers startup recovery; the automated ApplyChange test also kills a process *after partial SQL writes*, and the journal test kills a process *after a side effect but before recording its result*. After the power cut, as root:

```bash
python3 - <<'PY'
import json, sqlite3
for path in ['/var/lib/filedock-core/core.db', '/var/lib/filedock-management/app.db']:
    c = sqlite3.connect(path)
    assert c.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    assert not c.execute('PRAGMA foreign_key_check').fetchall()
c = sqlite3.connect('/var/lib/filedock-core/core.db')
for (spec,) in c.execute('SELECT spec FROM resource'):
    json.loads(spec)
print('State integrity OK')
PY
```

## 8. Remove the test fixture

Stop Core before removing the fixture's registry entry so it cannot restart the test unit during cleanup. Only the explicitly named fixture state is removed:

```bash
systemctl stop filedock-core.socket filedock-core.service
systemctl disable --now filedock-phase5-sample.service
rm /etc/systemd/system/filedock-phase5-sample.service
rm /usr/share/filedock/subsystems/phase5-sample.yaml
python3 - <<'PY'
import sqlite3
with sqlite3.connect('/var/lib/filedock-core/core.db') as c:
    c.execute("DELETE FROM subsystem WHERE id='phase5-sample'")
    c.execute("DELETE FROM setting WHERE key='supervision.phase5-sample'")
PY
rm -r /var/lib/filedock-phase5-test
systemctl daemon-reload
systemctl start filedock-core.socket filedock-core.service
```

## Automated validation and acceptance record

Local validation on 2026-10-13: 35 Rust tests, 28 .NET tests, and five React tests passed;
Rust Clippy, web lint/build, and both native Phase 4/Phase 5 smoke checks passed. Two ignored Rust
helper tests are subprocess entry points invoked by the passing crash tests. Four VM acceptance
tests collect successfully; they have not been executed on a Linux VM. Linux systemd behavior,
Debian packaging execution, browser acceptance, and the combined PVE manual gate remain pending.

From the repository:

```bash
cargo test -p fd-state -p fd-journal -p fd-observe -p fd-reconcile -p fd-subsystems -p filedock-core
dotnet build management/FileDock.Management.sln
# The test projects are xUnit executable runners:
dotnet management/tests/FileDock.Management.UnitTests/bin/Debug/net10.0/FileDock.Management.UnitTests.dll
dotnet management/tests/FileDock.Management.ApiTests/bin/Debug/net10.0/FileDock.Management.ApiTests.dll
pnpm -C web test
pnpm -C web lint
pnpm -C web build
cargo build -p filedock-core
python3 tests/integration/phase5_smoke.py
```

The isolated native smoke uses temporary databases/sockets, the real Core, real gRPC, and real SignalR. It reduces the recovery-key KDF memory only in its temporary test process, and does not modify an installed appliance. It does not exercise Linux systemd. The VM suite uses the same sample fixture:

```bash
cd tests/e2e
FD_E2E_BASE_IMAGE=/absolute/path/debian-13-genericcloud-amd64.qcow2 uv run pytest test_independence.py
```

The host needs libvirt/QEMU, SSH, a complete `dist/` bundle, pnpm, and installed Playwright browsers. Missing VM prerequisites produce skips, not evidence of a pass. Hard power cuts are refused for an existing host; use disposable libvirt domains or the manual PVE procedure.

The browser client retries the initial connection as well as reconnecting an established one;
SignalR's automatic reconnect alone does not cover initial start failures ([Microsoft documentation](https://learn.microsoft.com/en-us/aspnet/core/signalr/javascript-client?view=aspnetcore-10.0)).

Record: release tag/commit, Debian version, VM ID, test date, setup/sign-in result, Phase 4 results, fixture I/O during outages, PID preservation, failure/recovery timing, boot time, operation/state integrity, and any failed check. The P1 delivery gate is accepted only after these manual checks pass.
