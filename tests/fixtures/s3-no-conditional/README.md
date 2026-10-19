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
