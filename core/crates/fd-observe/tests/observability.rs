use fd_observe::{bundle, logs};
use serde_json::json;
#[test]
fn journal_entries_are_filtered_redacted_and_bounded() {
    let filter = logs::Filter {
        sources: vec!["filedock-core.service".into()],
        min_severity: "Warning".into(),
        text: "disk".into(),
        limit: 1,
        ..Default::default()
    };
    let raw = "{\"__CURSOR\":\"c1\",\"__REALTIME_TIMESTAMP\":\"1000000\",\"_SYSTEMD_UNIT\":\"filedock-core.service\",\"PRIORITY\":\"3\",\"MESSAGE\":\"disk password=canary\"}\n{\"__CURSOR\":\"c2\",\"PRIORITY\":\"6\",\"MESSAGE\":\"other\"}";
    let page = logs::parse(raw, &filter).unwrap();
    assert_eq!(page.entries.len(), 1);
    assert!(!page.entries[0].message.contains("canary"));
    assert_eq!(page.next_page_token, "c1");
}
#[test]
fn bundle_redacts_before_archiving() {
    let bytes = bundle::create(vec![(
        "configuration.json".into(),
        json!({"password":"canary"}),
    )])
    .unwrap();
    let decoded = zstd::stream::decode_all(&bytes[..]).unwrap();
    assert!(!String::from_utf8_lossy(&decoded).contains("canary"));
}
