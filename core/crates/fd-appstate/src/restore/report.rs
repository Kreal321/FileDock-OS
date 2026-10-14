use serde_json::{Value, json};
pub fn build_report(config: &Value, present: &[String], operation: &str, taken_at: &str) -> Value {
    let mut folders = Vec::new();
    for f in config["core"]["folders"].as_array().into_iter().flatten() {
        let pool = f["reference"]["poolId"].as_str().unwrap_or("");
        let exists = present.iter().any(|p| p == pool);
        let mode = f["storageMode"].as_str().unwrap_or("Unknown");
        let importance = f["dataImportance"].as_str().unwrap_or("");
        let order = if exists {
            0
        } else {
            match (mode, importance) {
                ("HybridBackup", "Critical") => 1,
                ("HybridBackup", "Important") => 2,
                ("CloudMount" | "TieredStorage", _) => 3,
                ("HybridBackup", "Rebuildable")
                    if f["advanced"]["backupEnabled"].as_bool() == Some(true) =>
                {
                    4
                }
                ("HybridBackup", _) => 5,
                _ => 6,
            }
        };
        let (outcome, detail) = if exists {
            ("DataPresent", "Pool survived; data remains in place.")
        } else if !pool.is_empty() {
            (
                "PoolUnmapped",
                "Pool needs a disk assignment before data recovery can proceed.",
            )
        } else {
            match order {
                1 | 2 | 4 => (
                    "AwaitingDataRestore",
                    "Hybrid Backup must restore data to the latest durable recovery point.",
                ),
                3 => (
                    "Reconnected",
                    "Cloud storage reconnects when its capability provider is available.",
                ),
                5 => (
                    "ToRebuild",
                    "Recreated empty; rebuildable/temporary data has no required backup.",
                ),
                _ => (
                    "Lost",
                    "Local-only data had no off-appliance copy and was lost.",
                ),
            }
        };
        folders.push(json!({"folderId":f["id"],"name":f["name"],"storageMode":mode,"dataImportance":importance,"recoveryOrder":order,"outcome":outcome,"detail":detail}));
    }
    folders.sort_by(|a, b| {
        a["recoveryOrder"]
            .as_u64()
            .cmp(&b["recoveryOrder"].as_u64())
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    let mut items = Vec::new();
    for (field, kind) in [
        ("pools", "Pool"),
        ("subsystems", "Extension"),
        ("apps", "ContainerApp"),
        ("connections", "Connection"),
        ("ownedLocations", "OwnedLocation"),
    ] {
        for item in config["core"][field].as_array().into_iter().flatten() {
            let status = match kind {
                "Pool" if !present.iter().any(|id| item["id"].as_str() == Some(id)) => {
                    "PoolUnmapped"
                }
                "Extension" => "PackageNeeded",
                "ContainerApp" => "AwaitingProvisioning",
                "OwnedLocation" => "RecoveryReader",
                _ => "Restored",
            };
            items.push(json!({"kind":kind,"id":item["id"],"name":item["name"],"status":status,"detail":if kind=="Extension"{"Package must be installed by its capability provider before it can run."}else{""}}));
        }
    }
    json!({"restoreOperationId":operation,"folders":folders,"items":items,"snapshotTakenAt":taken_at})
}
