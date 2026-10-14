//! OS-owned extension runtimes. The Core never hosts third-party code.
use crate::{
    LoadedManifest, SubsystemAdapter,
    package::{Result, invalid, relative},
    remote::Remote,
    sandbox::Sandbox,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
#[derive(Clone)]
pub struct Runtime {
    pub manifest: LoadedManifest,
    pub payload: PathBuf,
    pub sandbox: Sandbox,
    pub runtime: PathBuf,
    pub env_file: PathBuf,
    pub bridge: PathBuf,
    pub token: String,
    pub secrets: BTreeMap<String, PathBuf>,
}
impl Runtime {
    pub fn unit(&self) -> String {
        format!("filedock-extension-{}.service", self.manifest.id)
    }
    pub fn container(&self) -> String {
        format!("fd-extension-{}", self.manifest.id)
    }
    pub fn user(&self) -> String {
        use sha2::{Digest, Sha256};
        format!(
            "fdx-{}",
            &hex::encode(Sha256::digest(self.manifest.id.as_bytes()))[..16]
        )
    }
    pub fn remote(&self) -> Remote {
        Remote {
            id: self.manifest.id.clone(),
            socket: self.runtime.join("subsystem.sock"),
        }
    }
    pub fn command(&self) -> Result<Vec<String>> {
        let raw = &self.manifest.raw["runtime"];
        let mut command = match raw["type"].as_str() {
            Some("native-process") => {
                let executable = raw["executable"]
                    .as_str()
                    .ok_or_else(|| invalid("extension.executable_missing"))?;
                let path = executable
                    .strip_prefix("./")
                    .ok_or_else(|| invalid("extension.executable_must_be_packaged"))?;
                relative(path)?;
                let mut command = vec![self.payload.join(path).to_string_lossy().into_owned()];
                command.extend(
                    raw["args"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|s| s.as_str().map(str::to_owned)),
                );
                command
            }
            Some("systemd-service") => {
                let unit = raw["unit"]
                    .as_str()
                    .ok_or_else(|| invalid("extension.unit_missing"))?;
                relative(unit)?;
                let content = std::fs::read_to_string(self.payload.join(unit))
                    .map_err(|_| invalid("extension.unit_missing"))?;
                let words = service_command(&content)?;
                let first = words[0].strip_prefix("./").unwrap();
                let mut command = vec![self.payload.join(first).to_string_lossy().into_owned()];
                command.extend(words.into_iter().skip(1));
                command
            }
            _ => vec![],
        };
        if command.iter().any(|a| a.contains(['\n', '\r', '\0'])) {
            return Err(invalid("extension.command_invalid"));
        }
        if !command.is_empty() && !std::path::Path::new(&command[0]).is_file() {
            return Err(invalid("extension.executable_missing"));
        }
        command.shrink_to_fit();
        Ok(command)
    }
    pub async fn start(&self) -> Result<()> {
        let user = self.user();
        if !run_ok("id", &["-u".into(), user.clone()]).await {
            run(
                "useradd",
                &[
                    "--system".into(),
                    "--user-group".into(),
                    "--no-create-home".into(),
                    "--shell".into(),
                    "/usr/sbin/nologin".into(),
                    user.clone(),
                ],
            )
            .await?;
        }
        for binding in &self.sandbox.bindings {
            let actual = std::fs::canonicalize(&binding.source)
                .map_err(|_| invalid("extension.bound_folder_unavailable"))?;
            if !actual.is_dir() || !actual.starts_with("/storage") {
                return Err(invalid("extension.bound_folder_unavailable"));
            }
            run(
                "setfacl",
                &[
                    "-R".into(),
                    "-m".into(),
                    format!("u:{user}:rwX"),
                    binding.source.clone(),
                ],
            )
            .await?;
            run(
                "setfacl",
                &[
                    "-m".into(),
                    format!("u:{user}:rwx,d:u:{user}:rwx"),
                    binding.source.clone(),
                ],
            )
            .await?;
        }
        let uid = run_output("id", &["-u".into(), user.clone()]).await?;
        if uid.trim().parse::<u32>().ok().is_none_or(|uid| uid == 0) {
            return Err(invalid("extension.runtime_user_invalid"));
        }
        for device in &self.sandbox.devices {
            let actual = std::fs::canonicalize(device)
                .map_err(|_| invalid("extension.device_unavailable"))?;
            if !actual.starts_with("/dev/") {
                return Err(invalid("extension.device_invalid"));
            }
            run(
                "setfacl",
                &[
                    "-m".into(),
                    format!("u:{user}:rw"),
                    actual.display().to_string(),
                ],
            )
            .await?;
        }
        let kind = self.manifest.raw["runtime"]["type"].as_str().unwrap_or("");
        if kind == "docker-service" {
            self.start_docker(uid.trim()).await?;
        } else {
            let command = self.command()?;
            let mut props = self.sandbox.systemd(&user, &self.runtime.to_string_lossy());
            props.insert(
                "WorkingDirectory".into(),
                self.payload.display().to_string(),
            );
            let mut readonly = vec![format!(
                "{}:{}/bridge.sock",
                self.bridge.display(),
                self.runtime.display()
            )];
            if !self.secrets.is_empty()
                || self.manifest.raw["permissions"]
                    .as_array()
                    .is_some_and(|p| p.iter().any(|p| p == "cloud.credentials"))
            {
                let source = self
                    .secrets
                    .values()
                    .next()
                    .and_then(|p| p.parent())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        self.bridge
                            .parent()
                            .unwrap()
                            .join("secrets")
                            .join(&self.manifest.id)
                    });
                std::fs::create_dir_all(&source).map_err(|_| invalid("extension.runtime_io"))?;
                run(
                    "setfacl",
                    &[
                        "-m".into(),
                        format!("u:{}:rx", self.user()),
                        source.display().to_string(),
                    ],
                )
                .await?;
                for file in self.secrets.values() {
                    run(
                        "setfacl",
                        &[
                            "-m".into(),
                            format!("u:{user}:r"),
                            file.display().to_string(),
                        ],
                    )
                    .await?;
                }
                readonly.push(format!(
                    "{}:{}/secrets",
                    source.display(),
                    self.runtime.display()
                ));
            }
            props.insert("BindReadOnlyPaths".into(), readonly.join(" "));
            props.insert(
                "EnvironmentFile".into(),
                self.env_file.display().to_string(),
            );
            if kind == "native-process" {
                let mut args = vec![
                    "--quiet".into(),
                    "--collect".into(),
                    format!("--unit={}", self.unit()),
                ];
                for (k, v) in &props {
                    for value in if matches!(k.as_str(), "DeviceAllow" | "SocketBindAllow") {
                        v.split('|').collect::<Vec<_>>()
                    } else {
                        vec![v.as_str()]
                    } {
                        args.push(format!("--property={k}={value}"));
                    }
                }
                args.push("--".into());
                args.extend(command);
                run("systemctl", &["reset-failed".into(), self.unit()])
                    .await
                    .ok();
                run("systemd-run", &args).await?;
            } else {
                let exec = command
                    .iter()
                    .map(|a| {
                        format!(
                            "\"{}\"",
                            a.replace('\\', "\\\\")
                                .replace('"', "\\\"")
                                .replace('%', "%%")
                                .replace('$', "$$")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                let body = format!(
                    "[Unit]\nDescription=FileDock extension {}\n\n[Service]\nType=simple\nExecStart={}\n\n[Install]\nWantedBy=multi-user.target\n",
                    self.manifest.id, exec
                );
                let systemd = crate::Systemd::new("/etc/systemd/system");
                std::fs::write(format!("/etc/systemd/system/{}", self.unit()), body)
                    .map_err(|_| invalid("extension.unit_write_failed"))?;
                let dropin = props
                    .iter()
                    .map(|(k, v)| {
                        if matches!(k.as_str(), "DeviceAllow" | "SocketBindAllow") {
                            v.split('|')
                                .map(|value| format!("{k}={value}\n"))
                                .collect::<String>()
                        } else {
                            format!("{k}={v}\n")
                        }
                    })
                    .collect::<String>();
                systemd
                    .write_dropin(
                        &self.unit(),
                        "50-filedock-sandbox.conf",
                        &format!("[Service]\n{dropin}"),
                    )
                    .map_err(|_| invalid("extension.unit_write_failed"))?;
                run("systemctl", &["daemon-reload".into()]).await?;
                run("systemctl", &["reset-failed".into(), self.unit()])
                    .await
                    .ok();
                run("systemctl", &["restart".into(), self.unit()]).await?;
            }
        }
        Ok(())
    }
    async fn start_docker(&self, uid: &str) -> Result<()> {
        let image = self.manifest.raw["runtime"]["image"]
            .as_str()
            .ok_or_else(|| invalid("extension.image_invalid"))?;
        let digest = image
            .rsplit_once("@sha256:")
            .map(|(_, d)| d)
            .or_else(|| image.strip_prefix("sha256:"))
            .ok_or_else(|| invalid("extension.image_must_be_pinned"))?;
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("extension.image_must_be_pinned"));
        }
        // Refuse image-declared volumes so Docker cannot silently create anonymous persistence.
        let inspected = docker("GET", &format!("/images/{image}/json"), None).await?;
        if inspected["Config"]["Volumes"]
            .as_object()
            .is_some_and(|m| !m.is_empty())
        {
            return Err(invalid("extension.image_volumes_forbidden"));
        }
        std::fs::create_dir_all(&self.runtime).map_err(|_| invalid("extension.runtime_io"))?;
        run("chown", &[uid.into(), self.runtime.display().to_string()]).await?;
        use std::os::unix::fs::PermissionsExt;
        let token_file = self.runtime.join("token");
        std::fs::write(&token_file, self.token.as_bytes())
            .map_err(|_| invalid("extension.runtime_io"))?;
        std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o400))
            .map_err(|_| invalid("extension.runtime_io"))?;
        run("chown", &[uid.into(), token_file.display().to_string()]).await?;
        let mut config = self.sandbox.docker(
            uid,
            &self.runtime.to_string_lossy(),
            &self.bridge.to_string_lossy(),
            &self.token,
        )?;
        config["Image"] = json!(image);
        let mut command = self.manifest.raw["runtime"]["command"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if command.is_empty() {
            command.extend(
                inspected["Config"]["Entrypoint"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            command.extend(
                inspected["Config"]["Cmd"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        if command.is_empty() {
            return Err(invalid("extension.image_command_missing"));
        }
        config["Cmd"] = json!(command);
        config["Entrypoint"] = json!(["/run/filedock/extension-launcher"]);
        config["HostConfig"]["Mounts"].as_array_mut().unwrap().push(json!({"Type":"bind","Source":"/usr/lib/filedock/fd-extension-launcher","Target":"/run/filedock/extension-launcher","ReadOnly":true}));
        let env = config["Env"].as_array_mut().unwrap();
        env.push(json!(format!("FD_EXTENSION_ID={}", self.manifest.id)));
        env.push(json!(format!(
            "FD_EXTENSION_VERSION={}",
            self.manifest.version
        )));
        for (k, v) in runtime_env(&self.manifest)? {
            env.push(json!(format!("{k}={v}")));
        }
        if !self.secrets.is_empty()
            || self.manifest.raw["permissions"]
                .as_array()
                .is_some_and(|p| p.iter().any(|p| p == "cloud.credentials"))
        {
            let source = self
                .secrets
                .values()
                .next()
                .and_then(|p| p.parent())
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    self.bridge
                        .parent()
                        .unwrap()
                        .join("secrets")
                        .join(&self.manifest.id)
                });
            std::fs::create_dir_all(&source).map_err(|_| invalid("extension.runtime_io"))?;
            run(
                "setfacl",
                &[
                    "-m".into(),
                    format!("u:{}:rx", self.user()),
                    source.display().to_string(),
                ],
            )
            .await?;
            for file in self.secrets.values() {
                run(
                    "setfacl",
                    &[
                        "-m".into(),
                        format!("u:{}:r", self.user()),
                        file.display().to_string(),
                    ],
                )
                .await?;
            }
            config["HostConfig"]["Mounts"].as_array_mut().unwrap().push(json!({"Type":"bind","Source":source,"Target":format!("{}/secrets",self.runtime.display()),"ReadOnly":true}));
        }
        if !self.sandbox.listen.is_empty() {
            config["ExposedPorts"] = json!(
                self.sandbox
                    .listen
                    .iter()
                    .map(|p| (format!("{p}/tcp"), json!({})))
                    .collect::<serde_json::Map<_, _>>()
            );
        }
        docker(
            "DELETE",
            &format!("/containers/{}?force=true", self.container()),
            None,
        )
        .await
        .ok();
        docker(
            "POST",
            &format!("/containers/create?name={}", self.container()),
            Some(config),
        )
        .await?;
        docker(
            "POST",
            &format!("/containers/{}/start", self.container()),
            None,
        )
        .await?;
        Ok(())
    }
    pub async fn stop(&self) -> Result<()> {
        if self.manifest.raw["runtime"]["type"] == "docker-service" {
            docker(
                "POST",
                &format!("/containers/{}/stop?t=5", self.container()),
                None,
            )
            .await?;
        } else {
            run("systemctl", &["stop".into(), self.unit()]).await?;
        }
        Ok(())
    }
    pub async fn active(&self) -> bool {
        if self.manifest.raw["runtime"]["type"] == "docker-service" {
            docker(
                "GET",
                &format!("/containers/{}/json", self.container()),
                None,
            )
            .await
            .is_ok_and(|v| v["State"]["Running"] == true)
        } else {
            run_ok(
                "systemctl",
                &["is-active".into(), "--quiet".into(), self.unit()],
            )
            .await
        }
    }
    pub async fn health(&self) -> bool {
        self.active().await && self.remote().health().await.is_ok_and(|h| h.healthy)
    }
    pub async fn remove(&self) -> Result<()> {
        if self.manifest.raw["runtime"]["type"] == "docker-service" {
            docker(
                "DELETE",
                &format!("/containers/{}?force=true", self.container()),
                None,
            )
            .await?;
        } else {
            if self.active().await {
                self.stop().await?;
            }
            let file = PathBuf::from(format!("/etc/systemd/system/{}", self.unit()));
            if file.exists() {
                std::fs::remove_file(file).map_err(|_| invalid("extension.unit_remove_failed"))?;
                let _ = std::fs::remove_dir_all(format!("/etc/systemd/system/{}.d", self.unit()));
                run("systemctl", &["daemon-reload".into()]).await?;
            }
        }
        for binding in &self.sandbox.bindings {
            run(
                "setfacl",
                &[
                    "-R".into(),
                    "-x".into(),
                    format!("u:{}", self.user()),
                    binding.source.clone(),
                ],
            )
            .await?;
            run(
                "setfacl",
                &[
                    "-x".into(),
                    format!("d:u:{}", self.user()),
                    binding.source.clone(),
                ],
            )
            .await?;
        }
        Ok(())
    }
}
pub fn runtime_env(manifest: &LoadedManifest) -> Result<BTreeMap<String, String>> {
    let mut env = BTreeMap::new();
    for (k, v) in manifest.raw["runtime"]["env"]
        .as_object()
        .into_iter()
        .flatten()
    {
        if k.starts_with("FD_")
            || k.is_empty()
            || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || !v.is_string()
            || v.as_str().unwrap().contains(['\n', '\r', '\0'])
        {
            return Err(invalid("extension.environment_invalid"));
        }
        env.insert(k.clone(), v.as_str().unwrap().into());
    }
    Ok(env)
}
async fn run_output(program: &str, args: &[String]) -> Result<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(program)
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| invalid("extension.runtime_timeout"))?
    .map_err(|_| invalid("extension.runtime_unavailable"))?;
    if !output.status.success() {
        tracing::warn!(program, "extension runtime command failed");
        return Err(invalid("extension.runtime_failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
async fn run(program: &str, args: &[String]) -> Result<()> {
    run_output(program, args).await.map(|_| ())
}
async fn run_ok(program: &str, args: &[String]) -> bool {
    run(program, args).await.is_ok()
}
async fn docker(method: &str, path: &str, body: Option<Value>) -> Result<Value> {
    let client = reqwest::Client::builder()
        .unix_socket("/run/docker.sock")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| invalid("extension.docker_unavailable"))?;
    let mut request = client.request(
        method
            .parse()
            .map_err(|_| invalid("extension.docker_invalid"))?,
        format!("http://localhost{path}"),
    );
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request
        .send()
        .await
        .map_err(|_| invalid("extension.docker_unavailable"))?;
    if method == "DELETE" && response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Value::Null);
    }
    if !response.status().is_success() {
        return Err(invalid("extension.docker_failed"));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| invalid("extension.docker_failed"))?;
    if bytes.is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_slice(&bytes).map_err(|_| invalid("extension.docker_failed"))
    }
}

pub fn service_command(content: &str) -> Result<Vec<String>> {
    let mut exec = None;
    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| invalid("extension.unit_invalid"))?;
        match key {
            "Description" => {}
            "Type" if value == "simple" => {}
            "ExecStart" if exec.is_none() => exec = Some(value),
            _ => return Err(invalid("extension.unit_directive_not_allowed")),
        }
    }
    let words = exec
        .ok_or_else(|| invalid("extension.executable_missing"))?
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if words
        .iter()
        .any(|w| w.contains(['\n', '\r', '\0', '"', '\'', '%', '$']))
    {
        return Err(invalid("extension.unit_command_invalid"));
    }
    let first = words
        .first()
        .and_then(|s| s.strip_prefix("./"))
        .ok_or_else(|| invalid("extension.executable_must_be_packaged"))?;
    relative(first)?;
    Ok(words)
}
