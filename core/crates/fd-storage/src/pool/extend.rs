use crate::{Result, disks::Disk, run};
pub fn extend(id: &str, disk: &Disk) -> Result<()> {
    super::lvm::pv(std::slice::from_ref(disk), id)?;
    let vg = super::lvm::vg(id);
    if run("pvs", &["--noheadings", "-o", "vg_name", &disk.device_path])?.trim() != vg {
        run("vgextend", &[&vg, &disk.device_path])?;
    }
    let device = super::lvm::device(id);
    let device = device.to_str().unwrap();
    let free = run("vgs", &["--noheadings", "-o", "vg_free_count", &vg])?
        .trim()
        .parse::<u64>()
        .unwrap_or(0);
    if free > 0 {
        run("lvextend", &["--yes", "-l", "+100%FREE", device])?;
    }
    run("resize2fs", &[device])?;
    Ok(())
}
