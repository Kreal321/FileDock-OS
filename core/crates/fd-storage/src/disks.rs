use crate::{Error, Result, run};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub serial: String,
    pub wwn: String,
    pub model: String,
    pub size_bytes: u64,
    pub device_path: String,
    pub is_system_disk: bool,
    pub is_blank: bool,
    pub existing_filesystem: String,
    pub pool_id: String,
    pub health: String,
}
fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().trim().into()
}
fn system(v: &Value) -> bool {
    v["mountpoints"].as_array().is_some_and(|m| {
        m.iter().any(|p| {
            matches!(
                p.as_str(),
                Some(
                    "/" | "/boot"
                        | "/boot/efi"
                        | "/usr"
                        | "/var"
                        | "/var/lib"
                        | "/var/lib/filedock"
                        | "[SWAP]"
                )
            )
        })
    }) || v["children"]
        .as_array()
        .is_some_and(|c| c.iter().any(system))
}
fn occupied(v: &Value) -> bool {
    !text(v, "fstype").is_empty()
        || !text(v, "pttype").is_empty()
        || v["mountpoints"]
            .as_array()
            .is_some_and(|m| m.iter().any(|p| p.is_string()))
        || v["children"].as_array().is_some_and(|c| !c.is_empty())
}
fn filesystem_node(v: &Value) -> Option<&Value> {
    if !text(v, "fstype").is_empty() {
        return Some(v);
    }
    let children = v["children"].as_array()?;
    if children.len() == 1 {
        filesystem_node(&children[0])
    } else {
        None
    }
}
pub fn parse(v: &Value) -> Result<Vec<Disk>> {
    Ok(v["blockdevices"]
        .as_array()
        .ok_or_else(|| Error("disk.inventory_invalid".into()))?
        .iter()
        .filter(|d| text(d, "type") == "disk")
        .map(|d| Disk {
            serial: text(d, "serial"),
            wwn: text(d, "wwn"),
            model: text(d, "model"),
            size_bytes: d["size"]
                .as_u64()
                .or_else(|| text(d, "size").parse().ok())
                .unwrap_or(0),
            device_path: text(filesystem_node(d).unwrap_or(d), "path"),
            is_system_disk: system(d),
            is_blank: !occupied(d),
            existing_filesystem: filesystem_node(d)
                .map(|f| text(f, "fstype"))
                .unwrap_or_default(),
            pool_id: String::new(),
            health: "Unknown".into(),
        })
        .collect())
}
pub fn discover() -> Result<Vec<Disk>> {
    parse(&serde_json::from_str(&run(
        "lsblk",
        &[
            "--json",
            "--bytes",
            "--paths",
            "--output",
            "PATH,TYPE,SERIAL,WWN,MODEL,SIZE,FSTYPE,PTTYPE,MOUNTPOINTS",
        ],
    )?)?)
}
pub fn select(disks: &[Disk], serials: &[String], confirmation: Option<&str>) -> Result<Vec<Disk>> {
    if serials.is_empty() {
        return Err(Error("pool.disks_required".into()));
    }
    if let Some(c) = confirmation
        && c != format!("ERASE {}", serials.join(" "))
    {
        return Err(Error("pool.confirmation_mismatch".into()));
    }
    let mut selected = Vec::new();
    for serial in serials {
        let matches = disks
            .iter()
            .filter(|d| !serial.is_empty() && d.serial == *serial)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(Error("disk.identity_missing_or_ambiguous".into()));
        }
        let d = matches[0];
        if d.is_system_disk || !d.pool_id.is_empty() {
            return Err(Error("disk.in_use".into()));
        }
        if selected.iter().any(|d: &Disk| d.serial == *serial) {
            return Err(Error("disk.duplicate".into()));
        }
        if confirmation.is_some() && !d.is_blank {
            return Err(Error("disk.not_blank".into()));
        }
        selected.push(d.clone());
    }
    Ok(selected)
}
pub fn smart(path: &str) -> Result<String> {
    // smartctl uses nonzero status bits for device warnings; parse its JSON even then.
    let o = std::process::Command::new("timeout")
        .args([
            "--kill-after=5",
            "30",
            "ionice",
            "-c",
            "3",
            "nice",
            "-n",
            "10",
            "smartctl",
            "-j",
            "-a",
            path,
        ])
        .output()?;
    let v: Value = serde_json::from_slice(&o.stdout)?;
    if v["smart_status"]["passed"] == false {
        return Ok("Failing".into());
    }
    if v["ata_smart_attributes"]["table"]
        .as_array()
        .is_some_and(|a| {
            a.iter().any(|x| {
                [5, 187, 188, 197, 198].contains(&x["id"].as_u64().unwrap_or(0))
                    && x["raw"]["value"].as_u64().unwrap_or(0) > 0
            })
        })
    {
        return Ok("Warning".into());
    }
    Ok(if v["smart_status"]["passed"] == true {
        "OK"
    } else {
        "Unknown"
    }
    .into())
}
