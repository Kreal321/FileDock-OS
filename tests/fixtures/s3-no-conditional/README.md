# S3 provider fault proxy (test only)

Run `proxy.py` on the disposable MinIO test host. Set `FD_S3_UPSTREAM`,
`FD_S3_ACCESS_KEY`, `FD_S3_SECRET_KEY`, `FD_S3_CERT`, `FD_S3_KEY`, and optionally
`FD_S3_CA`, `FD_S3_REGION`, `FD_S3_LISTEN`, `FD_S3_PORT` (default 9443).
Trust the test CA on that host; do not disable production certificate validation.
The listening endpoint uses HTTPS and re-signs requests to MinIO after removing
conditional headers. Keys, object paths and responses are not logged.

`POST /__fault` with `{"upload":"flip"}`, `{"upload":"truncate"}`, or
`{"download":"flip"}` enables persistent fault injection. Reset both to `none`
after each check. The control endpoint has no authentication: bind to loopback or
an isolated test network. Do not use real provider credentials or real data.

The proxy supports ordinary SigV4 header-signed requests and HTTP chunk framing.
AWS signed streaming payload encoding and presigned URLs are outside this fixture.

For incomplete-upload acceptance, `holdPart: true` holds a successful part response
for at most 30 seconds; `GET /__fault` reports `partSeen`. Kill only the disposable
upload process, then clear `holdPart`. `ageMultipart: true` reports test uploads as
25 hours old in listing responses, so the production 24-hour sweep can be exercised
without changing the VM clock or weakening its age guard. Always reset these flags.

## Optional process attribution mode

`FD_S3_PRESERVE_CONDITIONS=1` forwards conditional headers when re-signing requests;
default behavior still removes them for Basic claim testing. `FD_S3_TRACE_PROCESS=1`
uses Linux `ss` to record peer process names/PIDs for up to 10,000 requests, without
logging credentials. `POST /__trace` resets the trace and `GET /__trace` returns it.
These controls are unauthenticated fixture endpoints: keep the proxy on loopback in a
disposable test environment. For a native-only snapshot window, compare every recorded
peer to the installed `filedock-s3` unit MainPID. Reset before the window and avoid direct
SDK or control calls through this proxy while measuring.
