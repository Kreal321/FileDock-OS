use crate::{Result, pressure::space, run};
use std::path::Path;
pub fn enforce(system: &Path, pool: &Path, reserve: u64) -> Result<bool> {
    let s = space(system)?;
    let guarded = reserve.saturating_add(super::limits::guard(s.total));
    let output = run(
        "findmnt",
        &[
            "--mountpoint",
            pool.to_str().unwrap(),
            "--noheadings",
            "-o",
            "OPTIONS",
        ],
    )?;
    let readonly = output.split(',').any(|s| s == "ro");
    let full = if readonly {
        s.free < reserve.saturating_add(super::limits::guard(s.total).saturating_mul(2))
    } else {
        s.free <= guarded
    };
    if readonly != full {
        run(
            "mount",
            &[
                "-o",
                if full { "remount,ro" } else { "remount,rw" },
                pool.to_str().unwrap(),
            ],
        )?;
    }
    Ok(full)
}
