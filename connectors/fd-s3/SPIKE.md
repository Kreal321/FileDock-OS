# Native S3 storage controls and multipart cleanup

Inspected the locked `object_store` **0.14.2** source used by this repository:

- `src/attributes.rs::Attribute::StorageClass` and `src/aws/client.rs` map the
  attribute to `x-amz-storage-class`.
- Both `PutOptions.attributes` and `PutMultipartOptions.attributes` are public.
  FileDock forwards the connection's storage class for single and multipart writes.
- `AmazonS3Builder::with_sse_kms_encryption` supplies SSE-KMS headers for uploads.
  A configured connection uses that builder for all its requests.
- The public `ObjectStore` API exposes the abort method on an active multipart
  upload handle, but does not list abandoned multipart uploads or abort an upload
  discovered by ID. Listing ordinary objects does not return incomplete uploads.

The missing control operations are in `src/sigv4.rs` and `src/sweep.rs`:
AWS's `aws-sigv4` signer signs S3 ListMultipartUploads and AbortMultipartUpload
requests, with S3 path normalization disabled, single percent encoding and an
`x-amz-content-sha256` header. They support path and virtual hosted addressing,
HTTPS certificate validation, pagination, a 30-second request timeout and no
redirects. Provider errors do not log credentials or response bodies.

Core supplies the scope. Empty/root sweep prefixes require the explicit Core Writer-check flag; ages
under 24 hours are refused. A listed key is checked against the requested prefix and age before
abort. Core checks Writer ownership before sweeping an owned prefix and skips
RecoveryReader/OwnershipLost locations. An unregistered destination can only have
this appliance's UUID-scoped probe namespace swept, after the bucket owner check.

The token bucket is shared by requests for a configured connection and retained
across identical Configure calls. It reserves at most 50ms worth of bytes at a time
and asynchronously waits, allowing other tasks to run. Session copies retain their
own immutable credentials/configuration across route changes.

Validation: native unit tests cover conditional writes, read-back integrity,
shared throttling, and rejecting young/out-of-scope sweep candidates. Live provider
and VM acceptance results are recorded separately; these unit tests do not establish
that an S3 provider honors a particular storage class or KMS policy.

## Checksum capability proof

Core previously configured `checksums: false` unconditionally, which prevented
multipart writes above the 64 MiB read-back ceiling. A writable probe now checks
that the provider rejects an intentionally incorrect SHA-256 checksum with a
checksum-specific `BadDigest` response, then accepts and verifies a correct upload.
A generic permission/authentication error does not count as checksum support.
Read-only probes do not perform this challenge. Core uses the resulting
per-connector observation for subsequent immutable session configurations.

The locked object_store source sets `x-amz-checksum-algorithm` when initiating
multipart uploads and calculates per-part checksum headers. AWS documents the
checksum mismatch rejection and multipart requirements in
[Checking object integrity for data uploads](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html).
Live MinIO/proxy validation of this path is still required.

Live MinIO acceptance (official `RELEASE.2025-10-15T17-29-55Z`) returns
HTTP400 `XAmzContentChecksumMismatch` for the invalid SHA256 challenge. Treat
that checksum-specific code like `BadDigest`; generic rejection still proves nothing.

The pinned MinIO multipart implementation indexes nonempty prefixes as exact object
keys (`cmd/erasure-multipart.go:ListMultipartUploads`). A fresh live check returned
three uploads for the bucket and zero for `state/` although all keys began with it.
The sweep retries an empty first page with a read-only bucket listing, then applies
the original prefix and 24-hour filter locally before every abort. Restricted bucket
listing permissions fail closed; this does not grant a wider write/abort scope.
