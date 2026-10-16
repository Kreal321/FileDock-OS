use crate::{Error, Result, disks::Disk, run};
use std::path::Path;
pub fn adopt(root: &Path, id: &str, name: &str, disk: &Disk) -> Result<()> {
    if disk.is_system_disk || disk.existing_filesystem != "ext4" {
        return Err(Error("pool.adopt_requires_unmounted_ext4".into()));
    }
    if run("findmnt", &["--source", &disk.device_path, "--noheadings"]).is_ok() {
        if super::health::mounted(root, id).is_ok() {
            return Ok(());
        }
        return Err(Error("disk.in_use".into()));
    }
    let uuid = run("blkid", &["-s", "UUID", "-o", "value", &disk.device_path])?;
    if uuid.is_empty() {
        return Err(Error("disk.identity_missing".into()));
    }
    super::finish(root, id, name, &Path::new("/dev/disk/by-uuid").join(uuid))
}
