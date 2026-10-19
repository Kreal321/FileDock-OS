use object_store::aws::{AmazonS3Builder, Checksum, S3ConditionalPut};
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload, UpdateVersion};
use tonic::Status;

use crate::config::{RoutedConnection, read_secret};
use crate::integrity::{Hasher, READ_BACK_LIMIT, sha256_hex};
use crate::multipart::{PART_SIZE, should_multipart};

pub async fn store_for(conn: &RoutedConnection) -> Result<impl ObjectStore, Status> {
    if !conn.endpoint.starts_with("https://") {
        return Err(Status::failed_precondition("connection.https_required"));
    }
    let access = read_secret(&conn.access_key_file).map_err(|e| Status::internal(e.to_string()))?;
    let secret = read_secret(&conn.secret_key_file).map_err(|e| Status::internal(e.to_string()))?;
    let builder = AmazonS3Builder::new()
        .with_access_key_id(access)
        .with_secret_access_key(secret)
        .with_endpoint(&conn.endpoint)
        .with_region(&conn.region)
        .with_bucket_name(&conn.bucket)
        .with_virtual_hosted_style_request(conn.virtual_hosted)
        .with_allow_http(false)
        .with_conditional_put(S3ConditionalPut::ETagMatch)
        .with_retry(object_store::RetryConfig {
            max_retries: 4,
            ..Default::default()
        });
    let builder = if let Some(key) = &conn.kms_key_id {
        builder.with_sse_kms_encryption(key)
    } else {
        builder
    };
    let builder = if conn.checksums {
        builder.with_checksum_algorithm(Checksum::SHA256)
    } else {
        builder
    };
    builder.build().map_err(|e| Status::internal(e.to_string()))
}

pub async fn stat(store: &dyn ObjectStore, key: &str) -> Result<(u64, String), Status> {
    let meta = store.head(&Path::from(key)).await.map_err(map_obj)?;
    Ok((meta.size, meta.e_tag.unwrap_or_default()))
}

pub async fn get(
    store: &dyn ObjectStore,
    key: &str,
    expected_sha256: Option<&str>,
) -> Result<(Vec<u8>, String), Status> {
    let (bytes, digest, _) = get_with_metadata(store, key, expected_sha256).await?;
    Ok((bytes, digest))
}

pub async fn get_with_metadata(
    store: &dyn ObjectStore,
    key: &str,
    expected_sha256: Option<&str>,
) -> Result<(Vec<u8>, String, String), Status> {
    let result = store.get(&Path::from(key)).await.map_err(map_obj)?;
    let etag = result.meta.e_tag.clone().unwrap_or_default();
    let bytes = result
        .bytes()
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
    let digest = sha256_hex(&bytes);
    if let Some(expected) = expected_sha256.filter(|s| !s.is_empty())
        && expected != digest
    {
        return Err(Status::data_loss("checksum mismatch"));
    }
    Ok((bytes.to_vec(), digest, etag))
}

pub async fn put(
    store: &dyn ObjectStore,
    key: &str,
    body: Vec<u8>,
    mode: PutMode,
    checksums: bool,
) -> Result<String, Status> {
    put_with_attributes(store, key, body, mode, checksums, Default::default()).await
}

pub async fn put_with_attributes(
    store: &dyn ObjectStore,
    key: &str,
    body: Vec<u8>,
    mode: PutMode,
    checksums: bool,
    attributes: object_store::Attributes,
) -> Result<String, Status> {
    if body.len() > READ_BACK_LIMIT && !checksums {
        return Err(Status::failed_precondition(
            "provider checksums are required for writes above the read-back limit",
        ));
    }
    if should_multipart(body.len() as u64) {
        if !matches!(mode, PutMode::Overwrite) {
            return Err(Status::failed_precondition(
                "conditional multipart writes are unsupported",
            ));
        }
        return put_multipart(store, key, body, mode, attributes).await;
    }
    let payload = PutPayload::from(body.clone());
    let result = store
        .put_opts(
            &Path::from(key),
            payload,
            PutOptions {
                mode,
                attributes,
                ..Default::default()
            },
        )
        .await
        .map_err(map_put)?;
    let etag = result.e_tag.unwrap_or_default();
    if !checksums && body.len() <= READ_BACK_LIMIT {
        let (read_back, _) = get(store, key, None).await?;
        if read_back != body {
            return Err(Status::data_loss("read-back mismatch"));
        }
    }
    let _ = Hasher::new();
    Ok(etag)
}

async fn put_multipart(
    store: &dyn ObjectStore,
    key: &str,
    body: Vec<u8>,
    _mode: PutMode,
    attributes: object_store::Attributes,
) -> Result<String, Status> {
    let mut upload = store
        .put_multipart_opts(
            &Path::from(key),
            object_store::PutMultipartOptions {
                attributes,
                ..Default::default()
            },
        )
        .await
        .map_err(map_obj)?;
    for chunk in body.chunks(PART_SIZE) {
        if let Err(error) = upload.put_part(PutPayload::from(chunk.to_vec())).await {
            let _ = upload.abort().await;
            return Err(Status::internal(error.to_string()));
        }
    }
    match upload.complete().await {
        Ok(r) => Ok(r.e_tag.unwrap_or_default()),
        Err(error) => {
            let _ = upload.abort().await;
            Err(Status::internal(error.to_string()))
        }
    }
}

pub async fn delete(store: &dyn ObjectStore, key: &str) -> Result<(), Status> {
    store.delete(&Path::from(key)).await.map_err(map_obj)
}

fn map_obj(error: object_store::Error) -> Status {
    match error {
        object_store::Error::NotFound { .. } => Status::not_found(error.to_string()),
        other => Status::internal(other.to_string()),
    }
}

fn map_put(error: object_store::Error) -> Status {
    match error {
        object_store::Error::AlreadyExists { .. } | object_store::Error::Precondition { .. } => {
            Status::failed_precondition(error.to_string())
        }
        other => map_obj(other),
    }
}

pub fn put_mode(mode: i32, etag: &str) -> PutMode {
    use fd_api::extension::v1::put_object_header::Mode;
    match Mode::try_from(mode).unwrap_or(Mode::Overwrite) {
        Mode::CreateOnly => PutMode::Create,
        Mode::ReplaceIfMatch => PutMode::Update(UpdateVersion {
            e_tag: Some(etag.into()),
            version: None,
        }),
        _ => PutMode::Overwrite,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn storage_class_is_forwarded_for_single_and_multipart_uploads() {
        let store = object_store::memory::InMemory::new();
        let mut attributes = object_store::Attributes::new();
        attributes.insert(object_store::Attribute::StorageClass, "STANDARD_IA".into());
        put_with_attributes(
            &store,
            "single",
            b"single".to_vec(),
            PutMode::Overwrite,
            false,
            attributes.clone(),
        )
        .await
        .unwrap();
        put_multipart(
            &store,
            "multi",
            b"multipart".to_vec(),
            PutMode::Overwrite,
            attributes,
        )
        .await
        .unwrap();
        for key in ["single", "multi"] {
            let result = store.get(&Path::from(key)).await.unwrap();
            assert_eq!(
                result
                    .attributes
                    .get(&object_store::Attribute::StorageClass)
                    .map(|v| v.as_ref()),
                Some("STANDARD_IA")
            );
        }
    }
    #[tokio::test]
    async fn create_only_preserves_existing_bytes_and_get_checks_digest() {
        let store = object_store::memory::InMemory::new();
        put(&store, "key", b"original".to_vec(), PutMode::Create, false)
            .await
            .unwrap();
        let error = put(
            &store,
            "key",
            b"replacement".to_vec(),
            PutMode::Create,
            false,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            get(&store, "key", Some(&sha256_hex(b"original")))
                .await
                .unwrap()
                .0,
            b"original"
        );
        let (_, _, etag) = get_with_metadata(&store, "key", None).await.unwrap();
        assert!(
            !etag.is_empty(),
            "Get must retain the ETag used by ownership If-Match"
        );
        put(
            &store,
            "key",
            b"updated".to_vec(),
            put_mode(3, &etag),
            false,
        )
        .await
        .unwrap();
        assert!(
            put(&store, "key", b"stale".to_vec(), put_mode(3, &etag), false)
                .await
                .is_err()
        );
        assert_eq!(
            get(&store, "key", Some(&sha256_hex(b"wrong")))
                .await
                .unwrap_err()
                .code(),
            tonic::Code::DataLoss
        );
    }
}
