use futures::TryStreamExt;
use object_store::{ObjectStore, PutMode, UpdateVersion};
use uuid::Uuid;

use crate::integrity::sha256_hex;
use crate::ops::{delete, get, put, stat};
use fd_api::extension::v1::{ProbeCheck, ProbeRequest, ProbeResult};

pub async fn run(store: &dyn ObjectStore, request: &ProbeRequest, checksums: bool) -> ProbeResult {
    let prefix = request.prefix.trim_end_matches('/');
    let key = format!("{prefix}/{}", Uuid::now_v7());
    let mut result = ProbeResult {
        checks: Vec::new(),
        conditional_create: false,
        conditional_replace: false,
        provider_checksums: false,
        clock_skew_ms: 0,
        write_steps_run: !request.read_only,
    };

    if let Err(error) = store.list(None).try_collect::<Vec<_>>().await {
        result.checks.push(fail("list", &error.to_string()));
        return result;
    }
    result.checks.push(check("list", true));

    if request.read_only {
        result.checks.push(ProbeCheck {
            name: "clock_skew".into(),
            ok: true,
            detail: "not measured without a write".into(),
        });
        return result;
    }

    let body = b"filedock-probe".to_vec();
    match put(store, &key, body.clone(), PutMode::Overwrite, checksums).await {
        Ok(_) => {
            result.provider_checksums = checksums;
            result.checks.push(check("write", true));
        }
        Err(_) => match put(store, &key, body.clone(), PutMode::Overwrite, false).await {
            Ok(_) => result.checks.push(check("write", true)),
            Err(error) => {
                result.checks.push(fail("write", &error.to_string()));
                return result;
            }
        },
    }

    match get(store, &key, Some(&sha256_hex(&body))).await {
        Ok(_) => {
            result.checks.push(check("read", true));
            result.checks.push(check("verify", true));
        }
        Err(error) => result.checks.push(fail("read", &error.to_string())),
    }

    match put(store, &key, body.clone(), PutMode::Create, false).await {
        Err(error) if error.code() == tonic::Code::FailedPrecondition => {
            result.conditional_create = true;
            result.checks.push(check("conditional_create", true));
        }
        Ok(_) => result
            .checks
            .push(fail("conditional_create", "service ignored If-None-Match")),
        Err(error) => result
            .checks
            .push(fail("conditional_create", &error.to_string())),
    }

    match put(
        store,
        &key,
        body.clone(),
        PutMode::Update(UpdateVersion {
            e_tag: Some("\"wrong\"".into()),
            version: None,
        }),
        false,
    )
    .await
    {
        Err(error) if error.code() == tonic::Code::FailedPrecondition => {
            if let Ok((_, etag)) = stat(store, &key).await
                && put(
                    store,
                    &key,
                    body.clone(),
                    PutMode::Update(UpdateVersion {
                        e_tag: Some(etag),
                        version: None,
                    }),
                    false,
                )
                .await
                .is_ok()
            {
                result.conditional_replace = true;
                result.checks.push(check("conditional_replace", true));
            } else {
                result
                    .checks
                    .push(fail("conditional_replace", "right etag failed"));
            }
        }
        _ => result
            .checks
            .push(fail("conditional_replace", "wrong etag was accepted")),
    }

    match delete(store, &key).await {
        Ok(()) => result.checks.push(check("delete", true)),
        Err(error) => result.checks.push(fail("delete", &error.to_string())),
    }
    result.checks.push(ProbeCheck {
        name: "clock_skew".into(),
        ok: true,
        detail: "not measured; no provider server time is exposed".into(),
    });
    result
}

fn check(name: &str, ok: bool) -> ProbeCheck {
    ProbeCheck {
        name: name.into(),
        ok,
        detail: String::new(),
    }
}

fn fail(name: &str, detail: &str) -> ProbeCheck {
    ProbeCheck {
        name: name.into(),
        ok: false,
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_only_probe_does_not_write_or_claim_checksum_support() {
        let store = object_store::memory::InMemory::new();
        let probe = run(
            &store,
            &ProbeRequest {
                prefix: "_filedock/probe/test".into(),
                read_only: true,
                ..Default::default()
            },
            false,
        )
        .await;
        assert!(!probe.write_steps_run);
        assert!(!probe.provider_checksums);
        assert!(!probe.conditional_create);
        assert!(probe.checks.iter().all(|c| c.ok));
        assert!(
            store
                .list(None)
                .try_collect::<Vec<_>>()
                .await
                .unwrap()
                .is_empty()
        );
    }
}
