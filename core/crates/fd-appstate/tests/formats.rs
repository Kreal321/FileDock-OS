use fd_appstate::format::fdstate::{SnapshotPayload, decode, encode};
use serde_json::json;
#[test]
fn encrypted_stream_roundtrip_rejects_wrong_key_tamper_and_truncation() {
    let payload = SnapshotPayload {
        manifest: json!({"schemaVersion":1,"applianceId":"a"}),
        config: json!({"schema":"fdconfig/1","core":{},"management":{}}),
        secrets: b"encrypted-secret-store".to_vec(),
        dek: [3; 32],
    };
    let bytes = encode(&payload, &[7; 32]).unwrap();
    assert_eq!(&bytes[..8], b"FDSTATE\0");
    assert!(!String::from_utf8_lossy(&bytes).contains("encrypted-secret-store"));
    assert_eq!(decode(&bytes, &[7; 32]).unwrap().config, payload.config);
    assert!(decode(&bytes, &[8; 32]).is_err());
    let mut tampered = bytes.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(decode(&tampered, &[7; 32]).is_err());
    assert!(decode(&bytes[..bytes.len() - 1], &[7; 32]).is_err());
}
#[test]
fn multiframe_stream_cannot_be_reordered_or_appended() {
    let mut raw = vec![0u8; 200_000];
    getrandom::fill(&mut raw).unwrap();
    let p = SnapshotPayload {
        manifest: json!({}),
        config: json!({"schema":"fdconfig/1"}),
        secrets: raw,
        dek: [1; 32],
    };
    let bytes = encode(&p, &[2; 32]).unwrap();
    assert_eq!(decode(&bytes, &[2; 32]).unwrap().secrets, p.secrets);
    // Swap the first two complete frames while preserving their lengths and ciphertext.
    let first = 107;
    let first_end =
        first + 5 + u32::from_le_bytes(bytes[first + 1..first + 5].try_into().unwrap()) as usize;
    let second_end = first_end
        + 5
        + u32::from_le_bytes(bytes[first_end + 1..first_end + 5].try_into().unwrap()) as usize;
    let mut reordered = bytes[..first].to_vec();
    reordered.extend_from_slice(&bytes[first_end..second_end]);
    reordered.extend_from_slice(&bytes[first..first_end]);
    reordered.extend_from_slice(&bytes[second_end..]);
    assert!(decode(&reordered, &[2; 32]).is_err());
    let mut append = bytes;
    append.push(0);
    assert!(decode(&append, &[2; 32]).is_err());
}
