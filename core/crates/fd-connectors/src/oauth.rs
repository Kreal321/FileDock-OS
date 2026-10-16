//! rclone owns provider authorization. Only its fixed configuration endpoints are exposed.
use crate::rclone::RcloneAdapter;
use serde_json::{Value, json};
pub fn backend(family: &str) -> Result<&'static str, String> {
    match family {
        "google-drive" => Ok("drive"),
        "onedrive" => Ok("onedrive"),
        "dropbox" => Ok("dropbox"),
        "azure-blob" => Ok("azureblob"),
        _ => Err("connection.oauth_unsupported".into()),
    }
}
pub async fn start(
    adapter: &RcloneAdapter,
    name: &str,
    family: &str,
    mut parameters: Value,
) -> Result<Value, String> {
    parameters["config_is_local"] = json!(false);
    adapter.call("config/create",json!({"name":name,"type":backend(family)?,"parameters":parameters,"opt":{"nonInteractive":true,"obscure":true}})).await
}
pub async fn finish(
    adapter: &RcloneAdapter,
    name: &str,
    state: &str,
    result: &str,
) -> Result<Value, String> {
    adapter.call("config/update",json!({"name":name,"parameters":{},"opt":{"nonInteractive":true,"continue":true,"state":state,"result":result}})).await
}
pub async fn credentials(adapter: &RcloneAdapter, name: &str) -> Result<Value, String> {
    adapter.call("config/get", json!({"name":name})).await
}
pub async fn discard(adapter: &RcloneAdapter, name: &str) -> Result<(), String> {
    adapter.delete_recovery_remote(name).await
}
pub fn state(value: &Value) -> &str {
    value["State"]
        .as_str()
        .or_else(|| value["state"].as_str())
        .unwrap_or("")
}
pub fn question(value: &Value) -> &str {
    value["Option"]["Help"]
        .as_str()
        .or_else(|| value["option"]["Help"].as_str())
        .unwrap_or("")
}
pub async fn remotes(adapter: &RcloneAdapter) -> Result<Vec<String>, String> {
    let value = adapter.call("config/listremotes", json!({})).await?;
    Ok(value["remotes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect())
}
