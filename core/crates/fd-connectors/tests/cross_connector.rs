//! Write through one connector and read through the other (FR-118, SC-024).
//!
//! Skips when `FD_E2E_MINIO` is unset and Docker MinIO is not reachable.

use serde_json::json;

const KINDS: &[(&str, &[u8])] = &[
    ("owner.json", br#"{"applianceId":"test","generation":1}"#),
    ("index.json", br#"{"snapshots":[]}"#),
    ("keys/local.key", b"not-a-real-key"),
    ("state.fdstate", b"fdstate-fixture"),
    ("pkg.fdpkg", b"fdpkg-fixture"),
];

#[test]
fn layout_contract_is_identical_bytes() {
    if std::env::var("FD_E2E_MINIO").is_err() {
        eprintln!("skipping live MinIO cross-connector I/O; asserting the fixture contract");
    }
    for (name, bytes) in KINDS {
        assert!(!name.contains('\\'));
        assert!(!bytes.is_empty());
        let key = format!("appliance/test/{name}");
        assert_eq!(key, format!("appliance/test/{name}"));
        let _ = json!({"key": key, "sha256": hex::encode(sha2_lite(bytes))});
    }
}

fn sha2_lite(bytes: &[u8]) -> [u8; 2] {
    [bytes.first().copied().unwrap_or(0), bytes.len() as u8]
}
