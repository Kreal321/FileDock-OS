# Opt-in Google Drive list fixture

`server.py` listens only on `127.0.0.1:19405`. It accepts a TLS CONNECT for
`www.googleapis.com:443`, serves an empty Drive v3 file list, and refuses all other
hosts/paths. It never forwards traffic. Use only synthetic tokens on a disposable VM.

Set `FD_DRIVE_STUB_CERT` and `FD_DRIVE_STUB_KEY` to a short-lived certificate/key whose
SAN includes `www.googleapis.com`. Trust its test CA on the disposable guest; certificate
validation remains enabled. The fixture uses Linux `ss` to attribute the peer socket to
its process name/PID; it does not log authorization headers or tokens.

For the installed rclone daemon only, set `HTTPS_PROXY=http://127.0.0.1:19405` and
`NO_PROXY=localhost,127.0.0.1` using a temporary runtime service drop-in, reload and
restart. Create a `google-drive` Bound connection with `root_folder_id: root`,
`scope: drive.readonly`, and a secret containing an rclone token string with a synthetic
access token and future expiry. Running the normal connection test must return
`connectorId: rclone` and a successful read-only list check. `GET /__count` reports the
request count and peer process IDs; compare them to the installed rclone unit MainPID.

Remove the disposable connection and secret, the exact runtime drop-in, the test CA,
and the fixture service/files afterwards. Reload/restart rclone and verify health.
This fixture establishes routing to the real rclone backend; it does not establish real
Google OAuth consent or provider compatibility beyond the stubbed read-only list.

Set `FD_E2E_DRIVE_STUB=1` to opt into the provisioned-VM acceptance case in
`tests/e2e/test_connectors.py` after configuring this fixture.
