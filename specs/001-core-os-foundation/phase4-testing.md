# Phase 4: administrator access

Phase 3 manual acceptance was confirmed by the user on 2026-10-12. Phase 4 implements T070–T077. This phase affects the Management Service, CLI, UI and Management systemd unit; it adds no Rust resource controllers.

## Build and install on the PVE VM

Build the offline bundle using the existing GitHub **Offline bundle release** workflow from the commit containing Phase 4. Download and extract the new release separately from your previous bundle. Alternatively, on a Debian 13 amd64 build machine with the project toolchains:

```bash
make bundle RID=linux-x64 CONFIG=Release
```

In the new bundle directory, run these commands as root on the PVE VM:

```bash
sha256sum -c SHA256SUMS
bash install.sh --offline-bundle "$PWD" --check-only
bash install.sh --offline-bundle "$PWD"
systemctl status filedock-management --no-pager
```

The installer preserves existing administrator accounts and setup state. Hard-refresh `http://192.168.68.26:8021/` after updating. The dashboard now has Administrators, API tokens and Network settings navigation.

## Independent manual check

1. Sign out. From a separate terminal, `curl -i http://192.168.68.26:8021/api/v1/system/status` must return `401`. Sign in again and confirm the dashboard works.
2. In Administrators, create a temporary second account. In a private browser window, fail its sign-in five times: the fifth attempt and further attempts must return `429`, with a 15-minute lock. Keep your original administrator session for the remaining checks.
3. On the PVE console as root, run `filedockctl reset-password <temporary-username>`. Enter and confirm the new password privately. The account must unlock, and old sessions must be refused. Sign in with its new password.
4. Create a Read-only token in API tokens. Save the token before dismissing its one-time display. Refreshing the list must never reveal its secret.
5. In a terminal on your client machine:

```bash
read -rsp 'Read-only token: ' FD_PHASE4_TOKEN
printf '\n'
curl -i -H "Authorization: Bearer $FD_PHASE4_TOKEN" http://192.168.68.26:8021/api/v1/system/status
curl -i -X POST -H "Authorization: Bearer $FD_PHASE4_TOKEN" -H 'Content-Type: application/json' -d '{}' http://192.168.68.26:8021/api/v1/folders
curl -i -H "Authorization: Bearer $FD_PHASE4_TOKEN" http://192.168.68.26:8021/api/v1/tokens
```

Expect `200`, `403`, and `403`. Resource mutations such as folders arrive in later phases; the scope guard already rejects a Read-only token on these paths.

6. Create a Full token. It can read system status and audit. It must still get `403` for administrator/token administration, password changes, first-run completion and changing listener settings. Normal resource mutations use Full scope; the API tests verify that policy independently of later-phase resource controllers.
7. Create a token with a near-term expiry and verify it is refused with `401` after expiry. Revoke a token in the UI and immediately repeat its status request: expect `401`. `filedockctl tokens revoke-all` must invalidate all existing API tokens immediately.
8. View `GET /api/v1/audit` using a valid token or session. Check named `token.use` entries, `auth.denied` refusals, administrator/token changes, and `Console` entries for recovery commands. Read-only GET polling is sampled once per minute; denied requests and mutations are recorded separately.
9. In Administrators, disable and re-enable the temporary account; disabled accounts cannot sign in. Remove it. Attempt to disable or remove the last enabled administrator: expect refusal (`409`). Changing your own password requires your current password and matching re-entry, and signs out all your sessions. Another account's password can only be reset from the console.
10. Check idle expiry by leaving a signed-in browser inactive for more than 30 minutes. Its next protected request must be refused and lead back to sign-in. “Sign out all sessions” must invalidate other signed-in windows immediately.
11. In Network settings, select only the VM's LAN IP and save. A change that removes the current connection's address must be refused unless the disconnect checkbox is checked. Listener changes apply shortly after the response; reconnect at the chosen address. Restart the service and confirm the restriction persists.
12. From the root console, run `filedockctl network reset`. HTTP access on port 8021 must return on all interfaces. Console endpoints must remain inaccessible via TCP, even with a valid administrator session.
13. On the VM, check the packaged service restrictions:

```bash
systemctl show filedock-management -p User -p NoNewPrivileges -p ProtectSystem -p CapabilityBoundingSet -p AmbientCapabilities
stat -c '%a %U' /run/filedock/management-admin.sock
```

Expect `filedock-mgmt`, `NoNewPrivileges=yes`, `ProtectSystem=strict`, only `CAP_NET_BIND_SERVICE`, and socket mode `600`. Root can use the console socket independently of the public listener.

## Automated local validation

```bash
dotnet test --solution management/FileDock.Management.sln
pnpm -C web test
pnpm -C web lint
pnpm -C web build
dotnet build management/src/FileDock.Management.Api
python3 tests/integration/phase4_smoke.py
```

The native smoke check launches a separate Management Service with temporary state and a temporary port. It exercises real Kestrel listener reloads, persistence over restart, missing-interface console recovery, TCP console denial, token revocation, password reset/session invalidation and console audit entries. It uses the test Core gateway, so it does not modify your running PVE installation or prove Linux systemd hardening.

On Debian, the same smoke script can use a published self-contained executable without installing the SDK:

```bash
FD_PHASE4_SERVER=/usr/lib/filedock/management/FileDock.Management.Api python3 tests/integration/phase4_smoke.py
```

Phase 4 local validation passed: 19 .NET tests, 2 React tests, React build and ESLint, the Linux x64 self-contained Management Service and CLI publish, and the native smoke check. The packaged Linux service restrictions and the complete PVE manual check remain to be confirmed on the target VM. This is not a claim that Phase 4 VM acceptance has passed.
