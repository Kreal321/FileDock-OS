//! Bounded journald queries. Only appliance services are admitted; arguments never invoke a shell.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Default, Clone)]
pub struct Filter {
    pub sources: Vec<String>,
    pub min_severity: String,
    pub since: Option<String>,
    pub until: Option<String>,
    pub text: String,
    pub cursor: String,
    pub limit: usize,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub at: String,
    pub source: String,
    pub severity: String,
    pub message: String,
}
pub struct Page {
    pub entries: Vec<Entry>,
    pub next_page_token: String,
}
fn rank(s: &str) -> Option<u8> {
    match s {
        "" | "Debug" => Some(7),
        "Info" => Some(6),
        "Warning" => Some(4),
        "Error" => Some(3),
        "Critical" => Some(2),
        _ => None,
    }
}
pub fn source_allowed(s: &str) -> bool {
    s.len() <= 160
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.@".contains(&b))
        && ((s.starts_with("filedock-") || s.starts_with("fd-extension-"))
            || ["smbd.service", "nfs-server.service", "docker.service"].contains(&s))
}
pub fn parse(raw: &str, f: &Filter) -> Result<Page, String> {
    let minimum = rank(&f.min_severity).ok_or("logs.severity_invalid")?;
    let mut entries = vec![];
    let mut cursor = String::new();
    for line in raw.lines() {
        let v: Value = serde_json::from_str(line).map_err(|_| "logs.journal_invalid")?;
        cursor = v["__CURSOR"].as_str().unwrap_or("").into();
        let source = v["CONTAINER_NAME"]
            .as_str()
            .map(|n| format!("{n}.service"))
            .or_else(|| v["_SYSTEMD_UNIT"].as_str().map(str::to_owned))
            .unwrap_or_default();
        if !source_allowed(&source) || (!f.sources.is_empty() && !f.sources.contains(&source)) {
            continue;
        }
        let priority = v["PRIORITY"]
            .as_str()
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(6);
        let message = v["MESSAGE"].as_str().unwrap_or("");
        if priority > minimum || !message.to_lowercase().contains(&f.text.to_lowercase()) {
            continue;
        }
        let micros = v["__REALTIME_TIMESTAMP"]
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let at = DateTime::<Utc>::from_timestamp_micros(micros)
            .ok_or("logs.timestamp_invalid")?
            .to_rfc3339();
        if f.since.as_ref().is_some_and(|s| {
            DateTime::parse_from_rfc3339(s).is_ok_and(|t| t.timestamp_micros() > micros)
        }) || f.until.as_ref().is_some_and(|s| {
            DateTime::parse_from_rfc3339(s).is_ok_and(|t| t.timestamp_micros() < micros)
        }) {
            continue;
        }
        entries.push(Entry {
            at,
            source,
            severity: match priority {
                0..=2 => "Critical",
                3 => "Error",
                4 => "Warning",
                5 | 6 => "Info",
                _ => "Debug",
            }
            .into(),
            message: fd_secrets::redact::redact_text(
                &message.chars().take(16384).collect::<String>(),
            ),
        });
        if entries.len() >= f.limit.clamp(1, 500) {
            break;
        }
    }
    Ok(Page {
        entries,
        next_page_token: cursor,
    })
}
pub async fn query(f: &Filter) -> Result<Page, String> {
    if f.sources.iter().any(|s| !source_allowed(s))
        || f.text.len() > 1024
        || f.cursor.len() > 4096
        || f.cursor.contains(['\n', '\0'])
    {
        return Err("logs.filter_invalid".into());
    }
    for time in [&f.since, &f.until].into_iter().flatten() {
        DateTime::parse_from_rfc3339(time).map_err(|_| "logs.time_invalid")?;
    }
    rank(&f.min_severity).ok_or("logs.severity_invalid")?;
    let mut cmd = tokio::process::Command::new("journalctl");
    cmd.args([
        "--no-pager",
        "--quiet",
        "--output=json",
        "--reverse",
        "--lines=5000",
    ]);
    if !f.cursor.is_empty() {
        cmd.arg(format!("--cursor={}", f.cursor));
    }
    if let Some(time) = &f.since {
        cmd.arg(format!("--since={time}"));
    }
    if let Some(time) = &f.until {
        cmd.arg(format!("--until={time}"));
    }
    // Restrict the journal itself before parsing, including Docker's journald container records.
    cmd.args([
        "--unit=filedock-*.service",
        "--unit=smbd.service",
        "--unit=nfs-server.service",
        "--unit=docker.service",
    ]);
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        cmd.kill_on_drop(true).output(),
    )
    .await
    .map_err(|_| "logs.timeout")?
    .map_err(|_| "logs.unavailable")?;
    if !output.status.success() {
        return Err("logs.unavailable".into());
    }
    let mut raw = String::from_utf8(output.stdout).map_err(|_| "logs.encoding_invalid")?;
    // --cursor includes the cursor entry: omit it for exclusive reverse paging.
    if !f.cursor.is_empty() {
        raw = raw
            .lines()
            .filter(|l| {
                serde_json::from_str::<Value>(l)
                    .ok()
                    .and_then(|v| v["__CURSOR"].as_str().map(str::to_owned))
                    .as_deref()
                    != Some(&f.cursor)
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    parse(&raw, f)
}
