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
    run(
        "findmnt",
        &["--mountpoint", p.to_str().unwrap(), "--noheadings"],
    )?;
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
