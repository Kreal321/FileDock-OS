use crate::{
    Error, Result,
    pressure::{Space, space},
    run,
};
use std::path::Path;
pub fn mounted(root: &Path, id: &str) -> Result<Space> {
    let p = root.join(id);
    let m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(p.join(".filedock-pool.json"))?)?;
    if m["poolId"] != id {
        return Err(Error("pool.identity_mismatch".into()));
    }
    mount_readonly(&p)?;
    space(&p)
}
pub fn lvm_missing(id: &str) -> bool {
    run(
        "lvs",
        &[
            "--noheadings",
            "-o",
            "lv_attr",
            super::lvm::device(id).to_str().unwrap(),
        ],
    )
    .map(|s| s.chars().nth(8) == Some('p'))
    .unwrap_or(true)
}

/// Read the kernel mount table directly: monitoring must not fork timeout/findmnt
/// several times every two seconds. Require an exact mountpoint, not its parent.
pub fn mount_readonly(path: &Path) -> Result<bool> {
    #[cfg(target_os = "linux")]
    {
        let path = std::fs::canonicalize(path)?;
        mount_readonly_from(&std::fs::read_to_string("/proc/self/mountinfo")?, &path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        run(
            "findmnt",
            &[
                "--mountpoint",
                path.to_str().unwrap(),
                "-n",
                "-o",
                "OPTIONS",
            ],
        )
        .map(|s| s.split(',').any(|o| o == "ro"))
    }
}

#[cfg(any(target_os = "linux", test))]
fn mount_readonly_from(table: &str, path: &Path) -> Result<bool> {
    // mountinfo encodes whitespace and backslashes as octal escapes.
    let escaped = path
        .to_str()
        .ok_or_else(|| Error("path.invalid".into()))?
        .chars()
        .fold(String::new(), |mut s, b| {
            match b {
                ' ' => s.push_str("\\040"),
                '\t' => s.push_str("\\011"),
                '\n' => s.push_str("\\012"),
                '\\' => s.push_str("\\134"),
                _ => s.push(b),
            }
            s
        });
    table
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.get(4).copied() != Some(escaped.as_str()) {
                return None;
            }
            let separator = fields.iter().position(|f| *f == "-")?;
            let readonly = [*fields.get(5)?, *fields.get(separator + 3)?]
                .iter()
                .any(|options| options.split(',').any(|o| o == "ro"));
            Some(readonly)
        })
        .next_back()
        .ok_or_else(|| Error("pool.mount_unavailable".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn live_kernel_root_mount_matches_findmnt() {
        let expected = run("findmnt", &["--mountpoint", "/", "-n", "-o", "OPTIONS"])
            .unwrap()
            .split(',')
            .any(|o| o == "ro");
        assert_eq!(mount_readonly(Path::new("/")).unwrap(), expected);
        let child = std::env::temp_dir().join(format!("fd-not-mounted-{}", std::process::id()));
        std::fs::create_dir_all(&child).unwrap();
        assert!(mount_readonly(&child).is_err());
        std::fs::remove_dir(child).unwrap();
    }
    #[test]
    fn kernel_mount_observations_are_exact_and_handle_escaped_paths() {
        let table = "1 0 8:1 / / rw - ext4 /dev/root rw\n2 1 7:0 / /pool rw,noatime - ext4 /dev/loop0 rw\n3 1 7:1 / /space\\040pool ro - ext4 /dev/loop1 rw\n4 1 7:2 / /super rw - ext4 /dev/loop2 ro\n";
        assert!(!mount_readonly_from(table, Path::new("/pool")).unwrap());
        assert!(mount_readonly_from(table, Path::new("/space pool")).unwrap());
        assert!(mount_readonly_from(table, Path::new("/super")).unwrap());
        assert!(mount_readonly_from(table, Path::new("/pool/child")).is_err());
        assert!(mount_readonly_from("malformed", Path::new("/pool")).is_err());
    }
}
