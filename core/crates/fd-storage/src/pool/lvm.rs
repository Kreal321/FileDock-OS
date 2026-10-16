use crate::{Error, Result, disks::Disk, run};
use std::path::{Path, PathBuf};
pub fn vg(id: &str) -> String {
    format!("fd_{}", id.replace('-', ""))
}
pub fn device(id: &str) -> PathBuf {
    Path::new("/dev").join(vg(id)).join("data")
}
pub fn pv(disks: &[Disk], id: &str) -> Result<()> {
    for d in disks {
        if run("pvs", &["--noheadings", "-o", "pv_tags", &d.device_path]).is_ok_and(|s| {
            s.split(',')
                .any(|t| t.trim() == format!("filedock_pool={id}"))
        }) {
            continue;
        }
        // Re-read signatures immediately before pvcreate; a nonblank device is never overwritten.
        if !run(
            "wipefs",
            &["--noheadings", "--output", "TYPE", &d.device_path],
        )?
        .is_empty()
        {
            return Err(Error("disk.not_blank".into()));
        }
        run(
            "pvcreate",
            &[
                "--yes",
                "--addtag",
                &format!("filedock_pool={id}"),
                &d.device_path,
            ],
        )?;
    }
    Ok(())
}
pub fn volume(disks: &[Disk], id: &str) -> Result<()> {
    let name = vg(id);
    let tag = format!("filedock_pool={id}");
    if let Ok(tags) = run("vgs", &["--noheadings", "-o", "vg_tags", &name]) {
        if !tags.split(',').any(|t| t.trim() == tag) {
            return Err(Error("pool.vg_not_owned".into()));
        }
    } else {
        let mut args = vec!["--addtag", &tag, &name];
        args.extend(disks.iter().map(|d| d.device_path.as_str()));
        run("vgcreate", &args)?;
    }
    if !device(id).exists() {
        run(
            "lvcreate",
            &[
                "--yes", "--type", "linear", "-l", "100%FREE", "-n", "data", &name,
            ],
        )?;
    }
    Ok(())
}
pub fn format(id: &str) -> Result<()> {
    let path = device(id);
    let p = path.to_str().unwrap();
    let fs = run("blkid", &["-s", "TYPE", "-o", "value", p]).unwrap_or_default();
    if fs == "ext4" {
        return Ok(());
    }
    if !fs.is_empty() {
        return Err(Error("pool.filesystem_conflict".into()));
    }
    run("mkfs.ext4", &["-q", p])?;
    Ok(())
}

/// Explicit failed-pool recreation retires only the tagged old VG. New disks are separately confirmed.
pub fn retire(id: &str) -> Result<()> {
    let name = vg(id);
    let tags = match run("vgs", &["--noheadings", "-o", "vg_tags", &name]) {
        Ok(tags) => tags,
        Err(_) => return Ok(()),
    };
    if !tags
        .split(',')
        .any(|t| t.trim() == format!("filedock_pool={id}"))
    {
        return Err(Error("pool.vg_not_owned".into()));
    }
    let logical = format!("{name}/data");
    // A missing PV can remove the /dev symlink while LVM still holds the LV metadata.
    if run("lvs", &["--noheadings", &logical]).is_ok() {
        run("lvremove", &["--force", "--yes", &logical])?;
    }
    run("vgremove", &["--force", "--yes", &name])?;
    Ok(())
}
