//! WebDAV authentication is per-export; read-only accounts cannot gain write rights.
use super::share_users::command;
use std::path::Path;
pub fn apply(
    id: &str,
    path: &str,
    port: u16,
    htpasswd: &Path,
    readonly: bool,
) -> Result<(), String> {
    if !(8440..=8499).contains(&port) || !path.starts_with("/storage/") {
        return Err("webdav.binding_invalid".into());
    }
    let unit = format!("filedock-webdav-{id}.service");
    if command("systemctl", &["is-active", "--quiet", &unit]).is_ok() {
        let running = command(
            "systemctl",
            &["show", "--property=ExecStart", "--value", &unit],
        )?;
        if uses_live_directory_entries(&running) {
            return Ok(());
        }
        // Transient units survive a Core upgrade. Reconcile the running command too,
        // so existing exports receive the same cache policy as newly created ones.
        command("systemctl", &["stop", &unit])?;
    }
    let mount = command("systemd-escape", &["--path", "--suffix=mount", path])?;
    // systemd-run parses property values again. Preserve the literal backslashes in
    // escaped unit names, and quote paths so spaces remain inside one path entry.
    let dependency = property_word(&mount);
    let writable_path = property_word(path);
    let mut args = vec![
        "--collect".into(),
        "--unit".to_owned(),
        unit,
        "--property=Type=exec".into(),
        "--property=Restart=on-failure".into(),
        "--property=ProtectSystem=strict".into(),
        "--property=NoNewPrivileges=yes".into(),
        format!("--property=BindsTo={dependency}"),
        format!("--property=After={dependency}"),
        format!("--property=ReadWritePaths={writable_path}"),
        "/usr/bin/rclone".into(),
        "serve".into(),
        "webdav".into(),
        path.into(),
        "--addr".into(),
        format!("0.0.0.0:{port}"),
        "--htpasswd".into(),
        htpasswd.display().to_string(),
        // Each role has its own VFS. Local files can also change through SMB, NFS
        // or Docker; a cached directory would hide those changes for five minutes.
        "--dir-cache-time=0s".into(),
    ];
    if readonly {
        args.push("--read-only".into())
    }
    command(
        "systemd-run",
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    Ok(())
}

fn property_word(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('\"', "\\\""))
}

fn uses_live_directory_entries(command: &str) -> bool {
    command
        .split_whitespace()
        .any(|word| word == "--dir-cache-time=0s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_unit_requires_the_exact_live_directory_policy() {
        assert!(uses_live_directory_entries(
            "{ path=/usr/bin/rclone ; argv[]=/usr/bin/rclone serve webdav /storage/test --dir-cache-time=0s ; }"
        ));
        for command in [
            "rclone serve webdav /storage/test",
            "rclone serve webdav /storage/test --dir-cache-time=5m",
            "rclone serve webdav /storage/test --dir-cache-time=0s-extra",
        ] {
            assert!(!uses_live_directory_entries(command));
        }
    }
}
