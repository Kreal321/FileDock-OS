pub mod adopt;
pub mod extend;
pub mod health;
pub mod lvm;
use crate::{Result, atomic, mount_unit};
use std::path::Path;
pub fn finish(root: &Path, id: &str, name: &str, device: &Path) -> Result<()> {
    let mount = root.join(id);
    std::fs::create_dir_all(&mount)?;
    mount_unit(device, &mount, "defaults,nosuid,nodev")?;
    let marker = mount.join(".filedock-pool.json");
    if marker.exists() {
        let previous: serde_json::Value = serde_json::from_slice(&std::fs::read(&marker)?)?;
        if previous["poolId"] != id {
            if let Ok(unit) = crate::run(
                "systemd-escape",
                &["--path", "--suffix=mount", mount.to_str().unwrap()],
            ) {
                let _ = crate::run("systemctl", &["disable", "--now", &unit]);
                let _ = std::fs::remove_file(Path::new("/etc/systemd/system").join(unit));
                let _ = crate::run("systemctl", &["daemon-reload"]);
            }
            return Err(crate::Error("pool.identity_mismatch".into()));
        }
    }
    atomic(
        &mount.join(".filedock-pool.json"),
        &serde_json::to_vec(&serde_json::json!({"poolId":id,"name":name}))?,
        0o644,
    )?;
    std::fs::create_dir_all(mount.join(".filedock"))?;
    Ok(())
}
