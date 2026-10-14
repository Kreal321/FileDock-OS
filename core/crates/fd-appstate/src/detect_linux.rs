//! Privileged Linux discovery helper. Caller starts this in a private mount namespace.
//! It never mounts a currently mounted device or allows filesystem journal replay.
use crate::{Result, invalid};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn run(program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(invalid("restore.discovery_command_failed"));
    }
    Ok(output.stdout)
}
fn copy_tree(source: &Path, target: &Path, count: &mut usize, total: &mut u64) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            return Err(invalid("restore.discovery_symlink"));
        }
        let dest = target.join(entry.file_name());
        if ty.is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_tree(&entry.path(), &dest, count, total)?;
        } else if ty.is_file() {
            let size = entry.metadata()?.len();
            *count += 1;
            *total += size;
            if *count > 2048 || size > 128 * 1024 * 1024 || *total > 2 * 1024 * 1024 * 1024 {
                return Err(invalid("restore.discovery_limit"));
            }
            crate::atomic_write(&dest, &std::fs::read(entry.path())?)?;
        }
    }
    Ok(())
}
fn leaves(value: &Value, devices: &mut Vec<(String, String)>) {
    for d in value.as_array().into_iter().flatten() {
        let mounted = d["mountpoints"]
            .as_array()
            .is_some_and(|a| a.iter().any(|v| v.is_string()));
        if !mounted
            && let (Some(path), Some(fs)) = (d["path"].as_str(), d["fstype"].as_str())
            && path.starts_with("/dev/")
            && !path.contains("..")
            && matches!(fs, "ext4" | "xfs" | "btrfs" | "vfat" | "exfat" | "ntfs")
        {
            devices.push((path.into(), fs.into()));
        }
        leaves(&d["children"], devices);
    }
}
pub fn scan_unmounted(output: &Path) -> Result<Vec<(PathBuf, String)>> {
    if !cfg!(target_os = "linux") {
        return Ok(vec![]);
    }
    run("mount", &["--make-rprivate", "/"])?;
    // Only FileDock-tagged VGs are activated. No format, fsck, repair, or writable mount is used.
    if let Ok(raw) = run("vgs", &["--reportformat", "json", "-o", "vg_name,vg_tags"])
        && let Ok(v) = serde_json::from_slice::<Value>(&raw)
    {
        for vg in v["report"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|r| r["vg"].as_array().into_iter().flatten())
        {
            let name = vg["vg_name"].as_str().unwrap_or("");
            let tags = vg["vg_tags"].as_str().unwrap_or("");
            if tags.split(',').any(|t| t == "filedock_pool")
                && !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
            {
                let _ = run("vgchange", &["-ay", name]);
            }
        }
    }
    let raw = run(
        "lsblk",
        &["--json", "--paths", "--output", "PATH,FSTYPE,MOUNTPOINTS"],
    )?;
    let value: Value = serde_json::from_slice(&raw)?;
    let mut devices = Vec::new();
    leaves(&value["blockdevices"], &mut devices);
    devices.truncate(256);
    let mut roots = Vec::new();
    std::fs::create_dir_all(output)?;
    for (device, fs) in devices {
        let id = uuid::Uuid::new_v4().to_string();
        let mountpoint = output.join(format!("mount-{id}"));
        std::fs::create_dir(&mountpoint)?;
        let options = match fs.as_str() {
            "ext4" => "ro,noload,nodev,nosuid,noexec",
            "xfs" => "ro,norecovery,nodev,nosuid,noexec",
            "btrfs" => "ro,nologreplay,nodev,nosuid,noexec",
            _ => "ro,nodev,nosuid,noexec",
        };
        if run(
            "mount",
            &[
                "-t",
                &fs,
                "-o",
                options,
                &device,
                mountpoint
                    .to_str()
                    .ok_or_else(|| invalid("restore.discovery_path"))?,
            ],
        )
        .is_ok()
        {
            let source = mountpoint.join(".filedock/appliance-state");
            let marker = mountpoint.join(".filedock-pool.json");
            let marked = std::fs::symlink_metadata(&marker)
                .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink() && m.len() < 65536)
                && std::fs::read(&marker)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                    .is_some_and(|v| {
                        v["poolId"]
                            .as_str()
                            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                    });
            if marked && source.is_dir() {
                let target = output.join(format!("copy-{id}"));
                std::fs::create_dir(&target)?;
                if copy_tree(&source, &target, &mut 0, &mut 0).is_ok() {
                    roots.push((target, format!("surviving pool on {device}")));
                }
            }
            let _ = run("umount", &[mountpoint.to_str().unwrap()]);
        }
        let _ = std::fs::remove_dir(mountpoint);
    }
    Ok(roots)
}
