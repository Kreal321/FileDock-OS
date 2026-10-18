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
        return Ok(());
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
