use crate::{Error, Result};
use std::path::Path;
#[derive(Clone, Copy, Debug)]
pub struct Space {
    pub total: u64,
    pub free: u64,
    pub used: u64,
}
#[allow(clippy::unnecessary_cast)] // statvfs counters have different widths on macOS and Linux.
pub fn space(path: &Path) -> Result<Space> {
    let p = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| Error("path.invalid".into()))?;
    let mut s = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(p.as_ptr(), s.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let s = unsafe { s.assume_init() };
    let total = (s.f_blocks as u64).saturating_mul(s.f_frsize);
    let free = (s.f_bavail as u64).saturating_mul(s.f_frsize);
    Ok(Space {
        total,
        free,
        used: total.saturating_sub((s.f_bfree as u64).saturating_mul(s.f_frsize)),
    })
}
/// Only named disposable roots; never traverse directories or follow symlinks.
pub fn trim_disposable(root: &Path, age: std::time::Duration) -> Result<u64> {
    let mut count = 0;
    if !root.exists() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let m = entry.path().symlink_metadata()?;
        if m.is_file() && m.modified()?.elapsed().unwrap_or_default() > age {
            std::fs::remove_file(entry.path())?;
            count += 1
        }
    }
    Ok(count)
}
