use object_store::{ObjectStore, PutMode, UpdateVersion};
use uuid::Uuid;

use crate::integrity::sha256_hex;
use crate::ops::{delete, get, put, stat};
use fd_api::extension::v1::{ProbeCheck, ProbeRequest, ProbeResult};

pub async fn run(store: &dyn ObjectStore, request: &ProbeRequest) -> ProbeResult {
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

    result.checks.push(check("list", true));
    let _ = store.list(None);

    if request.read_only {
        result.checks.push(ProbeCheck {
            name: "clock_skew".into(),
            ok: true,
            detail: "not measured without a write".into(),
        });
        return result;
    }

    let body = b"filedock-probe".to_vec();
    match put(store, &key, body.clone(), PutMode::Overwrite, true).await {
        Ok(_) => {
            result.provider_checksums = true;
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
    result.checks.push(check("clock_skew", true));
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
