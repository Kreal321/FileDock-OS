pub mod core_section;
use crate::{Result, invalid};
pub use core_section::{section_for, serialize};
use serde_json::Value;
pub fn validate(document: &Value) -> Result<()> {
    if document["schema"] != "fdconfig/1" {
        return Err(invalid("backup_file.too_new"));
    }
    if !document["core"].is_object()
        || !document["management"].is_object()
        || !document["appliance"].is_object()
    {
        return Err(invalid("fdconfig.invalid"));
    }
    Ok(())
}
