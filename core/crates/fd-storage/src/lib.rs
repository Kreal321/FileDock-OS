//! Linux storage orchestration. Commands are argument arrays; destructive work is journaled by Core.
pub mod disks;
pub mod pool;
pub mod pressure;
pub mod system_pool;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self(e.to_string())
    }
}
pub fn run(program: &str, args: &[&str]) -> Result<String> {
    let out = if cfg!(target_os = "linux") {
        Command::new("timeout")
            .args(["--kill-after=5", "120", program])
            .args(args)
            .output()?
    } else {
        Command::new(program).args(args).output()?
    };
    if !out.status.success() {
        return Err(Error(format!(
            "{program}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().into())
}
pub fn input(program: &str, args: &[&str], secret: &[u8]) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child.stdin.take().unwrap().write_all(secret)?;
    if !child.wait()?.success() {
        return Err(Error(format!("{program} failed")));
    }
    Ok(())
}
pub fn atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| Error("path.parent_missing".into()))?,
    )?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(mode)
        .open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)?;
    std::fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
/// Scalar mount settings do not use shell/list quoting. Reject controls and systemd specifiers.
pub fn mount_path(path: &Path) -> Result<String> {
    let s = path
        .to_str()
        .ok_or_else(|| Error("mount.path_invalid".into()))?;
    if !s.starts_with('/') || s.contains(['\n', '\r', '\0', '%', '\\', '"']) {
        return Err(Error("mount.path_invalid".into()));
    }
    Ok(s.into())
}
pub fn mount_unit(source: &Path, target: &Path, options: &str) -> Result<String> {
    let name = run(
        "systemd-escape",
        &["--path", "--suffix=mount", &mount_path(target)?],
    )?;
    let body = format!(
        "# FileDock storage\n[Unit]\nDescription=FileDock storage mount\n[Mount]\nWhat={}\nWhere={}\nType=ext4\nOptions={}\n[Install]\nWantedBy=multi-user.target\n",
        mount_path(source)?,
        mount_path(target)?,
        options
    );
    let unitpath = Path::new("/etc/systemd/system").join(&name);
    if unitpath.exists() && !std::fs::read_to_string(&unitpath)?.starts_with("# FileDock storage") {
        return Err(Error("pool.mount_unit_not_owned".into()));
    }
    atomic(&unitpath, body.as_bytes(), 0o644)?;
    run("systemctl", &["daemon-reload"])?;
    run("systemctl", &["enable", "--now", &name])?;
    Ok(name)
}
