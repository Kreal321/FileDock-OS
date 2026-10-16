//! Dedicated no-login share accounts. Never modify an account not in FileDock's share group.
use std::io::Write;
use std::process::{Command, Stdio};
pub fn command(program: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("{program} failed"));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().into())
}
fn input(program: &str, args: &[&str], data: &[u8]) -> Result<String, String> {
    let mut c = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    c.stdin
        .take()
        .unwrap()
        .write_all(data)
        .map_err(|e| e.to_string())?;
    let o = c.wait_with_output().map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("{program} failed"));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().into())
}
pub fn ensure(username: &str, password: &str, enabled: bool) -> Result<String, String> {
    if password.contains(['\n', '\r', '\0']) {
        return Err("share_user.password_invalid".into());
    }
    if command("getent", &["group", "filedock-share"]).is_err() {
        command("groupadd", &["--system", "filedock-share"])?;
    }
    if command("getent", &["passwd", username]).is_ok() {
        if !command("id", &["-nG", username])?
            .split_whitespace()
            .any(|g| g == "filedock-share")
        {
            return Err("share_user.system_account_conflict".into());
        }
    } else {
        command(
            "useradd",
            &[
                "--system",
                "--no-create-home",
                "--shell",
                "/usr/sbin/nologin",
                "--gid",
                "filedock-share",
                username,
            ],
        )?;
    }
    input(
        "smbpasswd",
        &["-s", "-a", username],
        format!("{password}\n{password}\n").as_bytes(),
    )?;
    command("smbpasswd", &[if enabled { "-e" } else { "-d" }, username])?;
    input(
        "openssl",
        &["passwd", "-apr1", "-stdin"],
        password.as_bytes(),
    )
}
pub fn remove(username: &str) -> Result<(), String> {
    if command("id", &["-nG", username])
        .is_ok_and(|g| g.split_whitespace().any(|g| g == "filedock-share"))
    {
        let _ = command("smbpasswd", &["-x", username]);
        command("userdel", &[username])?;
    }
    Ok(())
}
