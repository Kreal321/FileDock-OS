use fd_appstate::restore::report::build_report;
use serde_json::json;
#[test]
fn every_folder_loss_is_visible_and_report_has_recovery_order() {
    let cfg = json!({"core":{"folders":[
 {"id":"local","name":"Local","storageMode":"LocalOnly","reference":{"poolId":"missing"}},
 {"id":"present","name":"Present","storageMode":"LocalOnly","reference":{"poolId":"found"}},
 {"id":"critical","name":"Critical","storageMode":"HybridBackup","dataImportance":"Critical","reference":{"poolId":"missing"}},
 {"id":"temporary","name":"Temp","storageMode":"HybridBackup","dataImportance":"Temporary","reference":{"poolId":"missing"}}
 ],"pools":[],"apps":[],"connections":[],"subsystems":[]}});
    let r = build_report(&cfg, &["found".into()], "op", "2026-10-13T00:00:00Z");
    assert_eq!(r["folders"].as_array().unwrap().len(), 4);
    assert_eq!(r["folders"][0]["outcome"], "DataPresent");
    assert!(r.to_string().contains("PoolUnmapped"));
    assert!(r.to_string().contains("snapshotTakenAt"));
}
