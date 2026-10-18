//! Independent Samba/NFS/WebDAV configuration, reconciled from desired state.
use crate::{api::App, storage_runtime::rows};
use fd_state::ResourceKind;
use fd_subsystems::adapters::{nfs, samba, share_users, webdav};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};
pub async fn run(app: Arc<App>) {
    if crate::storage_runtime::enabled().is_err() {
        return;
    }
    loop {
        {
            // Resizes must not race a share pass which could re-export a quiesced folder.
            let _storage = app.storage_lock.lock().await;
            if let Err(error) = reconcile(&app).await {
                tracing::warn!(%error,"share reconciliation failed");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn update(path: &Path, data: &[u8], mode: u32) -> Result<bool, String> {
    if std::fs::read(path).ok().as_deref() == Some(data) {
        return Ok(false);
    }
    fd_storage::atomic(path, data, mode).map_err(error)?;
    Ok(true)
}
async fn reconcile(app: &App) -> Result<(), String> {
    let users = rows(app, ResourceKind::ShareUser).map_err(error)?;
    let mut hashes = std::collections::BTreeMap::new();
    for row in &users {
        let result: Result<(), String> = async {
            let username = row.spec["username"]
                .as_str()
                .ok_or("share_user.name_missing")?;
            let hashpath = Path::new("/etc/filedock/webdav/users").join(&row.resource.id);
            if row
                .status
                .as_ref()
                .is_none_or(|s| s["observedGeneration"] != row.generation)
                || !hashpath.exists()
            {
                let reference = row.spec["passwordSecret"]
                    .as_str()
                    .ok_or("share_user.password_missing")?;
                let pass = app.secrets.lock().await.get(reference).map_err(error)?;
                let hash = share_users::ensure(
                    username,
                    std::str::from_utf8(&pass).map_err(error)?,
                    row.spec["enabled"] != false,
                )?;
                fd_storage::atomic(&hashpath, hash.as_bytes(), 0o600).map_err(error)?;
                app.store
                    .write_status(
                        &row.resource,
                        &json!({"state":"Ready","observedGeneration":row.generation}),
                    )
                    .map_err(error)?;
            }
            if row.spec["enabled"] != false {
                hashes.insert(
                    username.to_string(),
                    std::fs::read_to_string(hashpath).map_err(error)?,
                );
            }
            Ok(())
        }
        .await;
        if let Err(detail) = result {
            let status =
                json!({"state":"Failed","detail":detail,"observedGeneration":row.generation});
            if row.status.as_ref() != Some(&status) {
                app.store
                    .write_status(&row.resource, &status)
                    .map_err(error)?;
                tracing::warn!(resource=%row.resource.id, %detail,"share user reconciliation failed");
            }
        }
    }
    for row in app.store.deleted(ResourceKind::ShareUser).map_err(error)? {
        let marker = Path::new("/etc/filedock/webdav/users").join(&row.resource.id);
        if marker.exists() {
            share_users::remove(row.spec["username"].as_str().unwrap_or(""))?;
            std::fs::remove_file(marker).map_err(error)?;
        }
    }
    let exports = rows(app, ResourceKind::Export).map_err(error)?;
    let folders = rows(app, ResourceKind::ManagedFolder).map_err(error)?;
    let mut smb = Vec::new();
    let mut nfsexports = Vec::new();
    let mut davport = 8440u16;
    let mut statuses = Vec::new();
    for row in &exports {
        let result: Result<serde_json::Value, String> = async {
        let folder = folders
            .iter()
            .find(|f| row.spec["folderId"] == f.resource.id)
            .ok_or("export.folder_missing")?;
        let ready = folder
            .status
            .as_ref()
            .is_some_and(|s| s["state"] == "Ready" && s["observedGeneration"] == folder.generation);
        let path = folder.spec["filedockPath"]
            .as_str()
            .ok_or("export.path_missing")?;
        let protocol = row.spec["protocol"].as_str().unwrap_or("");
        let rules: Vec<fd_state::resources::ExportRule> =
            serde_json::from_value(row.spec["rules"].clone()).map_err(error)?;
        let mut status = json!({"state":if ready{"Ready"}else{"Unavailable"},"observedGeneration":row.generation});
        // No exports over unmounted fallback directories.
        if ready {
            match protocol {
                "Smb" => smb.push((row.resource.id.clone(), path.to_string(), rules.clone())),
                "Nfs" => nfsexports.push((path.to_string(), rules.clone())),
                "WebDav" => {
                    let mut endpoints = Vec::new();
                    for (write, role) in [(false, "read"), (true, "write")] {
                        let selected = rules
                            .iter()
                            .filter(|r| (r.access == "read-write") == write)
                            .collect::<Vec<_>>();
                        if selected.is_empty() {
                            let unit =
                                format!("filedock-webdav-{}-{role}.service", row.resource.id);
                            let _ = share_users::command("systemctl", &["stop", &unit]);
                            continue;
                        }
                        let mut passwd = String::new();
                        for r in selected {
                            let p = r.principal.as_deref().ok_or("export.principal_missing")?;
                            if let Some(user) = p.strip_prefix("user:") {
                                if let Some(hash) = hashes.get(user) {
                                    passwd.push_str(&format!("{user}:{hash}\n"));
                                }
                            } else if let Some(group) = p.strip_prefix("group:") {
                                let entry = share_users::command("getent", &["group", group])?;
                                for user in entry.split(':').nth(3).unwrap_or("").split(',') {
                                    if let Some(hash) = hashes.get(user) {
                                        passwd.push_str(&format!("{user}:{hash}\n"));
                                    }
                                }
                            }
                        }
                        if passwd.is_empty() {
                            let unit =
                                format!("filedock-webdav-{}-{role}.service", row.resource.id);
                            let _ = share_users::command("systemctl", &["stop", &unit]);
                            continue;
                        }
                        // Stable, durable port allocation avoids endpoint changes when another export is removed.
                        let key = format!("webdav.{}.{}.port", row.resource.id, role);
                        let port = app
                            .store
                            .db()
                            .with(|c| fd_state::db::get_setting(c, &key))
                            .map_err(error)?
                            .and_then(|p| p.as_u64())
                            .map(|p| p as u16)
                            .unwrap_or_else(|| {
                                let p = davport;
                                davport += 1;
                                p
                            });
                        if port > 8499 {
                            return Err("webdav.ports_exhausted".into());
                        }
                        let other = app
                            .store
                            .db()
                            .with(|c| {
                                let mut q = c.prepare(
                                    "SELECT value FROM setting WHERE key LIKE 'webdav.%.port'",
                                )?;
                                q.query_map([], |r| r.get::<_, String>(0))?
                                    .collect::<Result<Vec<_>, _>>()
                            })
                            .unwrap_or_default();
                        let mut port = port;
                        while other.iter().any(|v| v.parse::<u16>().ok() == Some(port))
                            && app
                                .store
                                .db()
                                .with(|c| fd_state::db::get_setting(c, &key))
                                .map_err(error)?
                                .is_none()
                        {
                            port += 1;
                            if port > 8499 {
                                return Err("webdav.ports_exhausted".into());
                            }
                        }
                        app.store
                            .db()
                            .with(|c| fd_state::db::put_setting(c, &key, &json!(port)))
                            .map_err(error)?;
                        let unitid = format!("{}-{role}", row.resource.id);
                        let passwdpath =
                            Path::new("/etc/filedock/webdav").join(format!("{unitid}.htpasswd"));
                        if update(&passwdpath, passwd.as_bytes(), 0o600)? {
                            let _ = share_users::command(
                                "systemctl",
                                &["stop", &format!("filedock-webdav-{unitid}.service")],
                            );
                        }
                        webdav::apply(&unitid, path, port, &passwdpath, !write)?;
                        endpoints.push(json!({"port":port,"access":if write{"read-write"}else{"read"},"scheme":"http"}));
                    }
                    status["endpoints"] = json!(endpoints);
                }
                _ => return Err("export.protocol_invalid".into()),
            }
            // Filesystem access is explicit. Samba/WebDAV authentication still limits each export.
            for r in &rules {
                if let Some(principal) = r.principal.as_deref() {
                    let (kind, name) = principal
                        .split_once(':')
                        .ok_or("export.principal_invalid")?;
                    // Filesystem ACLs permit the union; each protocol still enforces its own rules.
                    let any_write = exports.iter().any(|other| other.spec["folderId"] == row.spec["folderId"] && other.spec["rules"].as_array().is_some_and(|rules| rules.iter().any(|rule| rule["principal"].as_str()==Some(principal) && rule["access"]=="read-write")));
                    let permission = if any_write {
                        "rwx"
                    } else {
                        "r-x"
                    };
                    share_users::command(
                        "setfacl",
                        &[
                            "-m",
                            &format!(
                                "{}:{name}:{permission}",
                                if kind == "group" { "g" } else { "u" }
                            ),
                            path,
                        ],
                    )?;
                    share_users::command(
                        "setfacl",
                        &[
                            "-m",
                            &format!(
                                "d:{}:{name}:{permission}",
                                if kind == "group" { "g" } else { "u" }
                            ),
                            path,
                        ],
                    )?;
                }
            }
        } else if protocol == "WebDav" {
            for role in ["read", "write"] {
                let _ = share_users::command(
                    "systemctl",
                    &[
                        "stop",
                        &format!("filedock-webdav-{}-{role}.service", row.resource.id),
                    ],
                );
            }
        }
        Ok(status)
        }.await;
        match result {
            Ok(status) => statuses.push((row, status)),
            Err(detail) => {
                match row.spec["protocol"].as_str() {
                    Some("Smb") => smb.retain(|(id, _, _)| id != &row.resource.id),
                    Some("Nfs") => {
                        if let Some(folder) = folders
                            .iter()
                            .find(|f| row.spec["folderId"] == f.resource.id)
                        {
                            nfsexports.retain(|(path, _)| folder.spec["filedockPath"] != *path);
                        }
                    }
                    Some("WebDav") => {
                        for role in ["read", "write"] {
                            let _ = share_users::command(
                                "systemctl",
                                &[
                                    "stop",
                                    &format!("filedock-webdav-{}-{role}.service", row.resource.id),
                                ],
                            );
                        }
                    }
                    _ => {}
                }
                statuses.push((
                    row,
                    json!({"state":"Failed","detail":detail,"observedGeneration":row.generation}),
                ));
            }
        }
    }
    for row in app.store.deleted(ResourceKind::Export).map_err(error)? {
        if row.spec["protocol"] == "WebDav" {
            for role in ["read", "write"] {
                let _ = share_users::command(
                    "systemctl",
                    &[
                        "stop",
                        &format!("filedock-webdav-{}-{role}.service", row.resource.id),
                    ],
                );
                let _ = std::fs::remove_file(
                    Path::new("/etc/filedock/webdav")
                        .join(format!("{}-{role}.htpasswd", row.resource.id)),
                );
            }
        }
    }
    // Each backend applies independently; readiness belongs only to that backend's result.
    let smb_applied = apply_smb(app, &smb);
    let nfs_applied = apply_nfs(app, &nfsexports);
    for (row, mut status) in statuses {
        let failure = match row.spec["protocol"].as_str() {
            Some("Smb") => smb_applied.as_ref().err(),
            Some("Nfs") => nfs_applied.as_ref().err(),
            _ => None,
        };
        if let Some(detail) = failure {
            status = json!({"state":"Failed","detail":detail,"observedGeneration":row.generation});
        }
        if row.status.as_ref() != Some(&status) {
            app.store
                .write_status(&row.resource, &status)
                .map_err(error)?;
            let _ = app
                .events
                .emit("export.state.changed", Some(row.resource.clone()), status);
        }
    }
    smb_applied.and(nfs_applied)
}

// Record only successfully applied content. A failed reload must be retried even if
// the desired file is already on disk, including when the last export was deleted.
fn pending_config(
    app: &App,
    protocol: &str,
    path: &Path,
    text: &str,
) -> Result<(bool, String), String> {
    let changed = update(path, text.as_bytes(), 0o644)?;
    let hash = hex::encode(Sha256::digest(text.as_bytes()));
    let applied = app
        .store
        .db()
        .with(|c| fd_state::db::get_setting(c, &format!("shares.applied.{protocol}")))
        .map_err(error)?;
    Ok((
        changed || applied.as_ref().and_then(serde_json::Value::as_str) != Some(hash.as_str()),
        hash,
    ))
}
fn applied_config(app: &App, protocol: &str, hash: &str) -> Result<(), String> {
    app.store
        .db()
        .with(|c| fd_state::db::put_setting(c, &format!("shares.applied.{protocol}"), &json!(hash)))
        .map_err(error)
}
fn subsystem_enabled(app: &App, id: &str) -> Result<bool, String> {
    Ok(app
        .subsystems
        .get(id)
        .map_err(error)?
        .is_some_and(|s| s.enabled))
}
fn apply_smb(
    app: &App,
    exports: &[(String, String, Vec<fd_state::resources::ExportRule>)],
) -> Result<(), String> {
    let text = samba::render(exports)?;
    let (pending, hash) = pending_config(
        app,
        "smb",
        Path::new("/etc/filedock/samba/shares.conf"),
        &text,
    )?;
    if pending {
        share_users::command("testparm", &["--suppress-prompt"])?;
        if subsystem_enabled(app, "samba")? {
            share_users::command("smbcontrol", &["all", "reload-config"])?;
        }
        applied_config(app, "smb", &hash)?;
    }
    Ok(())
}
pub(crate) fn apply_nfs(
    app: &App,
    exports: &[(String, Vec<fd_state::resources::ExportRule>)],
) -> Result<(), String> {
    let text = nfs::render(exports)?;
    let (pending, hash) = pending_config(
        app,
        "nfs",
        Path::new("/etc/exports.d/filedock.exports"),
        &text,
    )?;
    if pending {
        // Debian's NFS generator derives mount dependencies from the exports file.
        // Refresh them before stopping a removed/shrinking mount so NFS itself is not stopped.
        share_users::command("systemctl", &["daemon-reload"])?;
        if subsystem_enabled(app, "nfs")? {
            share_users::command("systemctl", &["start", "nfs-server.service"])?;
        }
        share_users::command("exportfs", &["-ra"])?;
        applied_config(app, "nfs", &hash)?;
    }
    Ok(())
}
