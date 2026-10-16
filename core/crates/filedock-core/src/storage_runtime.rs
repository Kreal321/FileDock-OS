use crate::api::App;
use fd_api::core::v1 as pb;
use fd_state::{Change, ChangeOrigin, ResourceKind, ResourceRef};
use fd_storage::{Error, Result};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub const SYSTEM_ID: &str = "00000000-0000-7000-8000-000000000001";
pub const KINDS: &[&str] = &[
    "MapPools",
    "CreatePool",
    "AdoptFilesystem",
    "ExtendPool",
    "RecreatePool",
    "SystemPoolResize",
];
pub fn enabled() -> std::result::Result<(), tonic::Status> {
    if !cfg!(target_os = "linux") || std::env::var("FD_RUNTIME_ENABLED").as_deref() == Ok("0") {
        Err(tonic::Status::failed_precondition(
            "storage.runtime_unavailable",
        ))
    } else {
        Ok(())
    }
}
fn err(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}
pub fn image() -> PathBuf {
    PathBuf::from("/var/lib/filedock/system-pool/pool.img")
}
pub fn rows(app: &App, kind: ResourceKind) -> Result<Vec<fd_state::StoredResource>> {
    let mut rows = Vec::new();
    let mut cursor = None;
    loop {
        let page = app.store.list(kind, cursor.as_deref(), 500).map_err(err)?;
        if page.is_empty() {
            break;
        }
        cursor = page.last().map(|r| r.resource.id.clone());
        rows.extend(page);
    }
    Ok(rows)
}
pub fn inventory(app: &App) -> Result<Vec<fd_storage::disks::Disk>> {
    let mut disks = fd_storage::disks::discover()?;
    for pool in rows(app, ResourceKind::Pool)? {
        for member in pool.spec["memberDisks"].as_array().into_iter().flatten() {
            for disk in &mut disks {
                if member["serial"] == disk.serial {
                    disk.pool_id = pool.resource.id.clone();
                    disk.is_blank = false;
                }
            }
        }
    }
    for disk in &mut disks {
        disk.health = app
            .store
            .db()
            .with(|c| fd_state::db::get_setting(c, &format!("disk.health.{}", disk.serial)))
            .map_err(err)?
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or("Unknown".into());
    }
    Ok(disks)
}
pub fn ensure_no_operation(
    app: &App,
    id: &str,
    serials: &[String],
) -> std::result::Result<(), tonic::Status> {
    for op in app
        .journal
        .unfinished()
        .map_err(|e| tonic::Status::internal(e.to_string()))?
    {
        if KINDS.contains(&op.kind.as_str())
            && (op.params["id"] == id
                || op.params["serials"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|s| serials.iter().any(|v| s == v))))
        {
            return Err(tonic::Status::aborted("storage.operation_in_progress"));
        }
    }
    Ok(())
}
pub fn spawn(app: Arc<App>, id: String) {
    tokio::spawn(async move {
        let _gate = app.storage_lock.lock().await;
        let copy = app.clone();
        let _ = tokio::task::spawn_blocking(move || resume(&copy, &id)).await;
    });
}
pub fn resume(app: &App, id: &str) -> Result<()> {
    if app.journal.get(id).map_err(err)?.kind == "MapPools" {
        return crate::pool_recovery::resume(app, id);
    }
    let op = app.journal.get(id).map_err(err)?;
    let p = &op.params;
    let pool = p["id"].as_str().ok_or_else(|| err("pool.id_missing"))?;
    tracing::info!(operation=id,kind=%op.kind,"storage operation started");
    for step in &op.steps {
        if step.state == "done" {
            continue;
        }
        app.journal.begin_step(id, &step.name).map_err(err)?;
        let outcome = (|| -> Result<Value> {
            let disks = if p["serials"].is_array() {
                let inventory = fd_storage::disks::discover()?;
                let mut out = Vec::new();
                for s in p["serials"].as_array().unwrap() {
                    let serial = s.as_str().unwrap_or("");
                    let d = inventory
                        .iter()
                        .filter(|d| d.serial == serial && !d.is_system_disk)
                        .collect::<Vec<_>>();
                    if d.len() != 1 {
                        return Err(err("disk.identity_missing_or_ambiguous"));
                    }
                    out.push(d[0].clone());
                }
                out
            } else {
                vec![]
            };
            match step.name.as_str() {
                "retire" => {
                    let target = app.config.pools_root.join(pool);
                    let unit = fd_storage::run(
                        "systemd-escape",
                        &["--path", "--suffix=mount", target.to_str().unwrap()],
                    )?;
                    let _ = fd_storage::run("systemctl", &["stop", &unit]);
                    fd_storage::pool::lvm::retire(pool)?;
                }
                "pv" => fd_storage::pool::lvm::pv(&disks, pool)?,
                "volume" => fd_storage::pool::lvm::volume(&disks, pool)?,
                "format" => fd_storage::pool::lvm::format(pool)?,
                "mount" => fd_storage::pool::finish(
                    &app.config.pools_root,
                    pool,
                    p["spec"]["name"].as_str().unwrap(),
                    &fd_storage::pool::lvm::device(pool),
                )?,
                "adopt" => fd_storage::pool::adopt::adopt(
                    &app.config.pools_root,
                    pool,
                    p["spec"]["name"].as_str().unwrap(),
                    &disks[0],
                )?,
                "extend" => fd_storage::pool::extend::extend(pool, &disks[0])?,
                "resize" => {
                    let bytes = p["capacity"].as_u64().unwrap();
                    let target = app.config.pools_root.join(pool);
                    if image().exists() {
                        if bytes < std::fs::metadata(image())?.len() {
                            quiesce(app, pool)?;
                        }
                        fd_storage::system_pool::image::resize(&image(), &target, bytes)?;
                    } else if bytes >= 1_000_000_000 {
                        fd_storage::system_pool::image::create(&image(), &target, bytes)?;
                    }
                }
                "commit" => {
                    let (generation, spec) = if op.kind == "SystemPoolResize" {
                        let row = app
                            .store
                            .get(&ResourceRef::new(ResourceKind::Pool, pool))
                            .map_err(err)?
                            .ok_or_else(|| err("pool.missing"))?;
                        let mut spec = row.spec;
                        spec["systemLimits"] = json!({"maxUsagePercent":p["percent"],"reservedFreeBytes":p["reserve"]});
                        (p["generation"].as_u64().unwrap(), spec)
                    } else {
                        (p["generation"].as_u64().unwrap_or(0), p["spec"].clone())
                    };
                    let reference = ResourceRef::new(ResourceKind::Pool, pool);
                    let old = app.store.get(&reference).map_err(err)?;
                    if old.as_ref().is_none_or(|r| r.spec != spec) {
                        app.store
                            .apply(
                                ChangeOrigin::System {
                                    reason: format!("storage operation {id}"),
                                },
                                &[Change::Upsert {
                                    resource: reference,
                                    expected_generation: generation,
                                    spec,
                                }],
                            )
                            .map_err(err)?;
                    }
                }
                _ => return Err(err("storage.step_unknown")),
            }
            Ok(json!({"completed":true}))
        })();
        let failed = outcome.as_ref().err().map(ToString::to_string);
        app.journal
            .finish_step(id, &step.name, outcome.map_err(|e| e.to_string()))
            .map_err(err)?;
        if let Some(error) = failed {
            tracing::warn!(operation=id,%error,"storage operation failed; side effects retained for review");
            let _ = app
                .events
                .emit("operation.failed", None, json!({"id":id,"detail":error}));
            return Err(err(error));
        }
    }
    let _ = app
        .events
        .emit("operation.completed", None, json!({"id":id}));
    tracing::info!(operation = id, "storage operation completed");
    Ok(())
}
pub fn system_view(app: &App) -> Result<pb::SystemPool> {
    let row = app
        .store
        .get(&ResourceRef::new(ResourceKind::Pool, SYSTEM_ID))
        .map_err(err)?;
    let Some(row) = row else {
        return Ok(pb::SystemPool {
            pool_id: SYSTEM_ID.into(),
            unavailable_reason: "System Pool initialization pending".into(),
            ..Default::default()
        });
    };
    let limits = &row.spec["systemLimits"];
    let percent = limits["maxUsagePercent"].as_u64().unwrap_or(50) as u32;
    let reserve = limits["reservedFreeBytes"]
        .as_u64()
        .unwrap_or(100_000_000_000);
    let sys = fd_storage::pressure::space(Path::new("/var/lib/filedock"))?;
    let target = app.config.pools_root.join(SYSTEM_ID);
    let pool = fd_storage::pool::health::mounted(&app.config.pools_root, SYSTEM_ID).ok();
    let cap = fd_storage::system_pool::limits::capacity(
        sys.total,
        sys.total
            .saturating_sub(sys.free)
            .saturating_sub(pool.map(|p| p.used).unwrap_or(0)),
        0,
        percent,
        reserve,
    )?;
    let available = pool.is_some();
    let usage = pool.map(|s| s.used).unwrap_or(0);
    let ro = available
        && fd_storage::run(
            "findmnt",
            &[
                "--mountpoint",
                target.to_str().unwrap(),
                "-n",
                "-o",
                "OPTIONS",
            ],
        )
        .is_ok_and(|s| s.split(',').any(|o| o == "ro"));
    Ok(pb::SystemPool {
        pool_id: SYSTEM_ID.into(),
        max_usage_percent: percent,
        reserved_free_bytes: reserve,
        system_disk_size_bytes: sys.total,
        system_disk_free_bytes: sys.free,
        used_bytes: usage,
        effective_capacity_bytes: cap.bytes,
        limiting_factor: if cap.reserve_limited { 2 } else { 1 },
        writable: if ro {
            3
        } else if pool.is_some_and(|s| s.free < 4096) {
            2
        } else {
            1
        },
        available,
        unavailable_reason: if available {
            String::new()
        } else if cap.bytes < 1_000_000_000 {
            format!(
                "system disk too small for the {} GB reserve",
                reserve / 1_000_000_000
            )
        } else {
            "System Pool is not mounted".into()
        },
        generation: row.generation,
    })
}
pub fn initialize(app: &App) -> Result<()> {
    let reference = ResourceRef::new(ResourceKind::Pool, SYSTEM_ID);
    if app.store.get(&reference).map_err(err)?.is_none() {
        app.store.apply(ChangeOrigin::System{reason:"initialize System Pool".into()},&[Change::Upsert{resource:reference,expected_generation:0,spec:json!({"name":"system","kind":"System","layers":["image"],"memberDisks":[],"systemLimits":{"maxUsagePercent":50,"reservedFreeBytes":100_000_000_000u64}})}]).map_err(err)?;
    }
    let view = system_view(app)?;
    let configured = app.recovery.try_lock().is_ok_and(|r| r.current().is_some());
    if configured && !view.available && view.effective_capacity_bytes >= 1_000_000_000 {
        fd_storage::system_pool::image::create(
            &image(),
            &app.config.pools_root.join(SYSTEM_ID),
            view.effective_capacity_bytes,
        )?;
        fd_storage::atomic(
            &app.config
                .pools_root
                .join(SYSTEM_ID)
                .join(".filedock-pool.json"),
            &serde_json::to_vec(&json!({"poolId":SYSTEM_ID,"name":"system"}))?,
            0o644,
        )?;
        std::fs::create_dir_all(app.config.pools_root.join(SYSTEM_ID).join(".filedock"))?;
    }
    Ok(())
}
pub async fn run(app: Arc<App>) {
    if enabled().is_err() {
        return;
    }
    // Reserve enforcement is independent of slow SMART/LVM/pressure commands.
    let watchdog = app.clone();
    tokio::spawn(async move {
        loop {
            let copy = watchdog.clone();
            let _=tokio::task::spawn_blocking(move||{
                let reference=ResourceRef::new(ResourceKind::Pool,SYSTEM_ID);
                if let Ok(Some(row))=copy.store.get(&reference){
                    let target=copy.config.pools_root.join(SYSTEM_ID);
                    if fd_storage::pool::health::mounted(&copy.config.pools_root,SYSTEM_ID).is_ok(){
                        let reserve=row.spec["systemLimits"]["reservedFreeBytes"].as_u64().unwrap_or(100_000_000_000);
                        match fd_storage::system_pool::watchdog::enforce(Path::new("/var/lib/filedock"),&target,reserve){
                            Ok(true)=>{let _=copy.alerts.raise("storage",Some(reference),"system_pool.reserve",fd_alerts::Severity::Critical,"System Pool is read-only to preserve the configured reserve; no data deleted");},
                            Ok(false)=>{let _=copy.alerts.resolve("storage",Some(&reference),"system_pool.reserve");},
                            Err(error)=>tracing::warn!(%error,"System Pool reserve enforcement failed"),
                        }
                    }
                }
            }).await;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
    {
        let _guard = app.storage_lock.lock().await;
        let copy = app.clone();
        let _ = tokio::task::spawn_blocking(move || {
            for op in copy.journal.unfinished().unwrap_or_default() {
                if KINDS.contains(&op.kind.as_str()) {
                    let _ = resume(&copy, &op.id);
                }
            }
            if let Err(error) = initialize(&copy) {
                tracing::warn!(%error,"System Pool initialization unavailable");
            }
        })
        .await;
    }
    let changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let udev = changed.clone();
    let udev_events = app.events.clone();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut command = tokio::process::Command::new("udevadm");
        command
            .args(["monitor", "--kernel", "--subsystem-match=block"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(target_os = "linux")]
        unsafe {
            command.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        if let Ok(mut child) = command.spawn()
            && let Some(stdout) = child.stdout.take()
        {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if line.starts_with("KERNEL") {
                    udev.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = udev_events.emit("disks.changed", None, json!({}));
                }
            }
        }
    });
    let polling = app.clone();
    tokio::spawn(async move {
        let mut last = None;
        loop {
            if changed.swap(false, std::sync::atomic::Ordering::Relaxed)
                || last.is_none_or(|at: std::time::Instant| {
                    at.elapsed() >= std::time::Duration::from_secs(21600)
                })
            {
                let copy = polling.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    for disk in fd_storage::disks::discover().unwrap_or_default() {
                        let health =
                            fd_storage::disks::smart(&disk.device_path).unwrap_or("Unknown".into());
                        let _ = copy.store.db().with(|c| {
                            fd_state::db::put_setting(
                                c,
                                &format!("disk.health.{}", disk.serial),
                                &json!(health),
                            )
                        });
                    }
                })
                .await;
                last = Some(std::time::Instant::now());
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
    let mut tick = 0u64;
    loop {
        let copy = app.clone();
        let _ = tokio::task::spawn_blocking(move || monitor(&copy, tick)).await;
        tick += 1;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
fn monitor(app: &App, tick: u64) -> Result<()> {
    // SMART probes can take minutes on failing disks; never hold up failure/reserve detection.
    let smart: std::collections::BTreeMap<String, String> = app
        .store
        .db()
        .with(|c| -> std::result::Result<_, fd_state::DbError> {
            let mut q =
                c.prepare("SELECT key,value FROM setting WHERE key LIKE 'disk.health.%'")?;
            let values = q
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(values
                .into_iter()
                .filter_map(|(key, value)| {
                    serde_json::from_str::<String>(&value)
                        .ok()
                        .map(|health| (key.trim_start_matches("disk.health.").to_string(), health))
                })
                .collect())
        })
        .map_err(err)?;
    if tick.is_multiple_of(15)
        && let Ok(_guard) = app.storage_lock.try_lock()
    {
        let _ = initialize(app);
    }
    let pools = rows(app, ResourceKind::Pool)?;
    for row in &pools {
        let id = &row.resource.id;
        if app
            .journal
            .unfinished()
            .map_err(err)?
            .iter()
            .any(|op| KINDS.contains(&op.kind.as_str()) && op.params["id"] == *id)
        {
            continue;
        }
        let target = app.config.pools_root.join(id);
        let observation = fd_storage::pool::health::mounted(&app.config.pools_root, id);
        let mut status = match &observation {
            Ok(s) => {
                json!({"state":"Ready","capacityBytes":s.total,"usedBytes":s.used,"freeBytes":s.free})
            }
            Err(_) => {
                json!({"state":if row.spec["kind"]=="System"{"Unavailable"}else{"Failed"},"detail":"Pool device or mount unavailable"})
            }
        };
        if row.spec["kind"] == "Lvm" && fd_storage::pool::health::lvm_missing(id) {
            status["state"] = json!("Failed");
        }
        if row.spec["kind"] == "System"
            && observation.is_ok()
            && fd_storage::disks::discover()?
                .iter()
                .filter(|d| d.is_system_disk)
                .any(|d| {
                    matches!(
                        smart.get(&d.serial).map(String::as_str),
                        Some("Warning" | "Failing")
                    )
                })
        {
            status["state"] = json!("Degraded");
        }
        if status["state"] == "Ready"
            && row.spec["memberDisks"].as_array().is_some_and(|a| {
                a.iter().any(|d| {
                    matches!(
                        smart
                            .get(d["serial"].as_str().unwrap_or(""))
                            .map(String::as_str),
                        Some("Warning" | "Failing")
                    )
                })
            })
        {
            status["state"] = json!("Degraded");
        }
        if row.spec["kind"] == "System"
            && observation.is_ok()
            && fd_storage::run(
                "findmnt",
                &[
                    "--mountpoint",
                    target.to_str().unwrap(),
                    "-n",
                    "-o",
                    "OPTIONS",
                ],
            )
            .is_ok_and(|s| s.split(',').any(|o| o == "ro"))
        {
            status["state"] = json!("FullReserve");
        }
        if row.spec["kind"] != "System"
            && observation.is_ok()
            && fd_storage::run(
                "findmnt",
                &[
                    "--mountpoint",
                    target.to_str().unwrap(),
                    "-n",
                    "-o",
                    "OPTIONS",
                ],
            )
            .is_ok_and(|s| s.split(',').any(|o| o == "ro"))
        {
            status["state"] = json!("Failed");
            status["detail"] = json!("Data filesystem is read-only, possibly after an I/O error");
        }
        if status["state"] == "Failed" {
            let folders = rows(app, ResourceKind::ManagedFolder)?
                .iter()
                .filter(|f| f.spec["reference"]["poolId"] == *id)
                .map(|f| {
                    format!(
                        "{}: {}",
                        f.spec["name"].as_str().unwrap_or("folder"),
                        if f.spec["storageMode"] == "LocalOnly" {
                            "no backup; data lost if disks cannot be recovered"
                        } else {
                            "restore through its storage-mode provider"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let _ = app.alerts.raise(
                "storage",
                Some(row.resource.clone()),
                "pool.failed",
                fd_alerts::Severity::Critical,
                &format!("Pool unavailable. {folders}"),
            );
        } else {
            let _ = app
                .alerts
                .resolve("storage", Some(&row.resource), "pool.failed");
        }
        if row.status.as_ref() != Some(&status) {
            app.store
                .write_status(&row.resource, &status)
                .map_err(err)?;
            if row.status.as_ref().map(|s| &s["state"]) != Some(&status["state"]) {
                let _ = app
                    .events
                    .emit("pool.health.changed", Some(row.resource.clone()), status);
            }
        }
    }
    if tick.is_multiple_of(15) {
        let settings = app
            .store
            .get(&ResourceRef::new(ResourceKind::Settings, "default"))
            .map_err(err)?
            .map(|r| r.spec)
            .unwrap_or_default();
        let warning = settings["diskPressureWarningPercent"]
            .as_u64()
            .unwrap_or(90);
        let critical = settings["diskPressureCriticalPercent"]
            .as_u64()
            .unwrap_or(95);
        let mut paths = vec![(PathBuf::from("/var/lib/filedock"), None)];
        paths.extend(
            pools
                .iter()
                .filter(|r| {
                    fd_storage::pool::health::mounted(&app.config.pools_root, &r.resource.id)
                        .is_ok()
                })
                .map(|r| {
                    (
                        app.config.pools_root.join(&r.resource.id),
                        Some(r.resource.clone()),
                    )
                }),
        );
        for (path, object) in paths {
            let s = fd_storage::pressure::space(&path)?;
            let used = s.used.saturating_mul(100) / s.total.max(1);
            if used >= warning {
                if object.is_none() && used >= critical {
                    let _ = fd_storage::pressure::trim_disposable(
                        &app.config.extension_inbox,
                        std::time::Duration::from_secs(86400),
                    );
                    let _ = fd_storage::pressure::trim_disposable(
                        Path::new("/var/cache/filedock/diagnostics"),
                        std::time::Duration::ZERO,
                    );
                }
                let _ = app.alerts.raise(
                    "storage",
                    object,
                    "storage.disk_pressure",
                    if used >= critical {
                        fd_alerts::Severity::Critical
                    } else {
                        fd_alerts::Severity::Warning
                    },
                    &format!(
                        "{} is {used}% used. User data is never deleted automatically.",
                        path.display()
                    ),
                );
            } else {
                let _ = app
                    .alerts
                    .resolve("storage", object.as_ref(), "storage.disk_pressure");
            }
        }
    }
    Ok(())
}

/// Close clients and stop dependent containers before an offline ext4 shrink. Never fsck a mounted image.
fn quiesce(app: &App, pool: &str) -> Result<()> {
    let reference = ResourceRef::new(ResourceKind::Pool, pool);
    app.store
        .write_status(
            &reference,
            &json!({"state":"Unavailable","detail":"Offline resize in progress"}),
        )
        .map_err(err)?;
    let folders = rows(app, ResourceKind::ManagedFolder)?
        .into_iter()
        .filter(|f| f.spec["reference"]["poolId"] == pool)
        .collect::<Vec<_>>();
    for folder in &folders {
        app.store.write_status(&folder.resource,&json!({"state":"Unavailable","observedGeneration":folder.generation,"detail":"Offline resize in progress"})).map_err(err)?;
    }
    for application in rows(app, ResourceKind::ContainerApp)? {
        let sources = fd_state::validate::folder::compose_sources(
            application.spec["compose"].as_str().unwrap_or(""),
            &application.spec["environment"],
        );
        if folders
            .iter()
            .any(|f| sources.iter().any(|s| f.spec["filedockPath"] == *s))
        {
            fd_storage::run(
                "docker",
                &[
                    "compose",
                    "--project-directory",
                    app.config
                        .apps_root
                        .join(&application.resource.id)
                        .to_str()
                        .unwrap(),
                    "-p",
                    &format!("fd-{}", application.spec["name"].as_str().unwrap_or("")),
                    "stop",
                ],
            )?;
        }
    }
    let mut smb = Vec::new();
    let mut nfs = Vec::new();
    let all_folders = rows(app, ResourceKind::ManagedFolder)?;
    for export in rows(app, ResourceKind::Export)? {
        if folders
            .iter()
            .any(|f| export.spec["folderId"] == f.resource.id)
        {
            continue;
        }
        if let Some(folder) = all_folders.iter().find(|f| {
            export.spec["folderId"] == f.resource.id
                && f.status.as_ref().is_some_and(|s| s["state"] == "Ready")
        }) {
            let path = folder.spec["filedockPath"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let rules = serde_json::from_value(export.spec["rules"].clone()).map_err(err)?;
            match export.spec["protocol"].as_str() {
                Some("Smb") => smb.push((export.resource.id, path, rules)),
                Some("Nfs") => nfs.push((path, rules)),
                _ => {}
            }
        }
    }
    fd_storage::atomic(
        Path::new("/etc/filedock/samba/shares.conf"),
        fd_subsystems::adapters::samba::render(&smb)
            .map_err(err)?
            .as_bytes(),
        0o644,
    )?;
    fd_storage::run("smbcontrol", &["all", "reload-config"])?;
    fd_storage::atomic(
        Path::new("/etc/exports.d/filedock.exports"),
        fd_subsystems::adapters::nfs::render(&nfs)
            .map_err(err)?
            .as_bytes(),
        0o644,
    )?;
    fd_storage::run("exportfs", &["-ra"])?;
    for export in rows(app, ResourceKind::Export)? {
        if let Some(folder) = folders
            .iter()
            .find(|f| export.spec["folderId"] == f.resource.id)
        {
            match export.spec["protocol"].as_str().unwrap_or("") {
                "Smb" => {
                    fd_storage::run("smbcontrol", &["all", "close-share", &export.resource.id])?;
                }
                "Nfs" => {
                    for rule in export.spec["rules"].as_array().into_iter().flatten() {
                        if let Some(client) = rule["client"].as_str() {
                            fd_storage::run(
                                "exportfs",
                                &[
                                    "-u",
                                    &format!(
                                        "{client}:{}",
                                        folder.spec["filedockPath"].as_str().unwrap_or("")
                                    ),
                                ],
                            )?;
                        }
                    }
                }
                "WebDav" => {
                    for role in ["read", "write"] {
                        let _ = fd_storage::run(
                            "systemctl",
                            &[
                                "stop",
                                &format!("filedock-webdav-{}-{role}.service", export.resource.id),
                            ],
                        );
                    }
                }
                _ => {}
            }
        }
    }
    for folder in folders {
        let unit = fd_storage::run(
            "systemd-escape",
            &[
                "--path",
                "--suffix=mount",
                folder.spec["filedockPath"].as_str().unwrap(),
            ],
        )?;
        fd_storage::run("systemctl", &["stop", &unit])?;
    }
    Ok(())
}
