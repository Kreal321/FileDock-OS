# Sample connector (test only)

Build `fd-s3` for the target Debian architecture, then run `bash build.sh OUTPUT`.
The resulting unsigned `sample-connector.fdext` runs the native S3 implementation
as a separate extension with the generic cloud configuration envelope. It declares
basic safety conservatively and does not advertise conditional writes or checksums.
Its actual conditional-write implementation must not be used to infer advertised
claim safety. Use the proxy fixture when testing providers that ignore conditions.

Approval must show `network.outbound` and `cloud.credentials`. Installing the package
must leave the route table unchanged. Test routing only disposable connections.
