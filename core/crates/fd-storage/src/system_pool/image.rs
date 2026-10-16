use crate::{Error, Result, run};
use std::path::Path;
pub fn create(image: &Path, target: &Path, bytes: u64) -> Result<()> {
    if bytes < 1_000_000_000 {
        return Err(Error("system_pool.disk_too_small".into()));
    }
    if !image.exists() {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::create_dir_all(image.parent().unwrap())?;
        crate::atomic(
            &image.with_extension("initializing"),
            b"FileDock sparse image creation",
            0o600,
        )?;
        let f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(image)?;
        f.set_len(bytes)?;
        f.sync_all()?;
    }
    let path = image.to_str().unwrap();
    let kind = run("blkid", &["-s", "TYPE", "-o", "value", path]).unwrap_or_default();
    if kind.is_empty() {
        if !image.with_extension("initializing").exists() {
            return Err(Error("system_pool.image_unowned".into()));
        }
        let file = std::fs::OpenOptions::new().write(true).open(image)?;
        file.set_len(bytes)?;
        file.sync_all()?;
        run("mkfs.ext4", &["-q", "-F", path])?;
    } else if kind != "ext4" {
        return Err(Error("system_pool.image_conflict".into()));
    }
    std::fs::create_dir_all(target)?;
    let name = run(
        "systemd-escape",
        &["--path", "--suffix=mount", target.to_str().unwrap()],
    )?;
    let loop_service = "filedock-system-pool-loop.service";
    let body = format!(
        "# FileDock system pool loop device\n[Unit]\nBefore={name}\n[Service]\nType=oneshot\nRemainAfterExit=yes\nExecStart=/usr/lib/filedock/filedock-core system-pool-loop\n[Install]\nWantedBy=multi-user.target\n"
    );
    crate::atomic(
        Path::new("/etc/systemd/system/filedock-system-pool-loop.service"),
        body.as_bytes(),
        0o644,
    )?;
    let mount = format!(
        "# FileDock storage\n[Unit]\nRequires={loop_service}\nAfter={loop_service}\n[Mount]\nWhat=/run/filedock/system-pool-device\nWhere={}\nType=ext4\nOptions=discard,nosuid,nodev\n[Install]\nWantedBy=multi-user.target\n",
        crate::mount_path(target)?
    );
    crate::atomic(
        &Path::new("/etc/systemd/system").join(&name),
        mount.as_bytes(),
        0o644,
    )?;
    run("systemctl", &["daemon-reload"])?;
    run("systemctl", &["enable", "--now", loop_service, &name])?;
    let marker = image.with_extension("initializing");
    if marker.exists() {
        std::fs::remove_file(marker)?;
    }
    Ok(())
}
pub fn resize(image: &Path, target: &Path, bytes: u64) -> Result<()> {
    let path = image.to_str().unwrap();
    let old = std::fs::metadata(image)?.len();
    if bytes >= old {
        std::fs::OpenOptions::new()
            .write(true)
            .open(image)?
            .set_len(bytes)?;
        let loopdev = std::fs::canonicalize("/run/filedock/system-pool-device")?;
        run("losetup", &["--set-capacity", loopdev.to_str().unwrap()])?;
        run("resize2fs", &[loopdev.to_str().unwrap()])?;
        let unit = run(
            "systemd-escape",
            &["--path", "--suffix=mount", target.to_str().unwrap()],
        )?;
        run("systemctl", &["start", &unit])?;
        return Ok(());
    }
    let unit = run(
        "systemd-escape",
        &["--path", "--suffix=mount", target.to_str().unwrap()],
    )?;
    run("systemctl", &["stop", &unit])?;
    let loopdev = std::fs::canonicalize("/run/filedock/system-pool-device")?;
    if run(
        "findmnt",
        &["--source", loopdev.to_str().unwrap(), "--noheadings"],
    )
    .is_ok()
    {
        return Err(Error("system_pool.still_in_use".into()));
    }
    let check = std::process::Command::new("e2fsck")
        .args(["-p", "-f", path])
        .status()?;
    if !matches!(check.code(), Some(0 | 1)) {
        return Err(Error("system_pool.fsck_failed".into()));
    }

    run("resize2fs", &[path, &format!("{}K", bytes / 1024)])?;
    if bytes < old {
        std::fs::OpenOptions::new()
            .write(true)
            .open(image)?
            .set_len(bytes)?;
    }
    let loopdev = std::fs::canonicalize("/run/filedock/system-pool-device")?;
    run("losetup", &["--set-capacity", loopdev.to_str().unwrap()])?;
    run("systemctl", &["start", &unit])?;
    Ok(())
}

/// Invoked by its independent systemd service at every boot.
pub fn attach_loop(image: &Path) -> Result<()> {
    let rows = run(
        "losetup",
        &[
            "--associated",
            image.to_str().unwrap(),
            "--noheadings",
            "--output",
            "NAME",
        ],
    )?;
    let device = if let Some(line) = rows.lines().next() {
        run("losetup", &["--direct-io=on", line])?;
        line.to_string()
    } else {
        run(
            "losetup",
            &[
                "--find",
                "--show",
                "--direct-io=on",
                image.to_str().unwrap(),
            ],
        )?
    };
    let link = Path::new("/run/filedock/system-pool-device");
    std::fs::create_dir_all(link.parent().unwrap())?;
    if link.symlink_metadata().is_ok() {
        std::fs::remove_file(link)?;
    }
    std::os::unix::fs::symlink(device, link)?;
    Ok(())
}
