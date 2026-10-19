//! Bounded CloudRemote and rclone adapters, shared by writers and read-only recovery sources.
use crate::ownership::{ObjectData, ObjectIo, OwnershipError, Result, WriteCondition};
use async_trait::async_trait;
use fd_api::extension::v1 as ext;
use sha2::{Digest, Sha256};
use tonic::transport::Channel;
const MAX: usize = 128 * 1024 * 1024;
fn unavailable(_: impl std::fmt::Display) -> OwnershipError {
    OwnershipError::Unavailable
}
pub struct S3Objects {
    pub channel: Channel,
    pub connection_id: String,
    pub prefix: String,
    pub cleanup_directory: Option<std::path::PathBuf>,
}
impl S3Objects {
    fn key(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }
}
#[async_trait]
impl ObjectIo for S3Objects {
    async fn abort_incomplete(
        &self,
        prefix: &str,
        hours: u32,
        root_owner_checked: bool,
    ) -> Result<u32> {
        let response = ext::cloud_remote_client::CloudRemoteClient::new(self.channel.clone())
            .abort_incomplete_uploads(ext::AbortIncompleteUploadsRequest {
                connection_id: self.connection_id.clone(),
                prefix: self.key(prefix),
                older_than_hours: hours,
                root_owner_checked,
            })
            .await
            .map_err(unavailable)?
            .into_inner();
        Ok(response.aborted)
    }
    async fn read(&self, key: &str) -> Result<Option<ObjectData>> {
        let mut client = ext::cloud_remote_client::CloudRemoteClient::new(self.channel.clone());
        let response = client
            .get(ext::GetObjectRequest {
                r#ref: Some(ext::ObjectRef {
                    connection_id: self.connection_id.clone(),
                    key: self.key(key),
                }),
                expected_sha256: String::new(),
                offset: 0,
                length: 0,
            })
            .await;
        let mut stream = match response {
            Ok(r) => r.into_inner(),
            Err(e) if e.code() == tonic::Code::NotFound => return Ok(None),
            Err(e) => return Err(unavailable(e)),
        };
        let mut bytes = Vec::new();
        let mut trailer = None;
        while let Some(chunk) = stream.message().await.map_err(unavailable)? {
            match chunk.part {
                Some(ext::get_object_chunk::Part::Data(data)) => {
                    if trailer.is_some() || bytes.len() + data.len() > MAX {
                        return Err(unavailable("limit"));
                    }
                    bytes.extend(data);
                }
                Some(ext::get_object_chunk::Part::Trailer(t)) => {
                    if trailer.is_some() {
                        return Err(unavailable("trailer"));
                    }
                    trailer = Some(t);
                }
                None => return Err(unavailable("chunk")),
            }
        }
        let trailer = trailer.ok_or_else(|| unavailable("trailer"))?;
        if hex::encode(Sha256::digest(&bytes)) != trailer.sha256 {
            return Err(unavailable("checksum"));
        }
        Ok(Some(ObjectData {
            bytes,
            etag: trailer.etag,
        }))
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut client = ext::cloud_remote_client::CloudRemoteClient::new(self.channel.clone());
        let mut stream = client
            .list(ext::ListObjectsRequest {
                connection_id: self.connection_id.clone(),
                prefix: self.key(prefix),
            })
            .await
            .map_err(unavailable)?
            .into_inner();
        let mut keys = Vec::new();
        while let Some(object) = stream.message().await.map_err(unavailable)? {
            if keys.len() >= 2048 {
                return Err(unavailable("limit"));
            }
            if let Some(key) = object.key.strip_prefix(&self.prefix) {
                keys.push(key.into());
            }
        }
        Ok(keys)
    }
    async fn put(&self, key: &str, bytes: &[u8], condition: WriteCondition) -> Result<String> {
        let (mode, etag) = match condition {
            WriteCondition::Overwrite => (1, String::new()),
            WriteCondition::CreateOnly => (2, String::new()),
            WriteCondition::IfMatch(e) => (3, e),
        };
        let mut chunks = vec![ext::PutObjectChunk {
            part: Some(ext::put_object_chunk::Part::Header(ext::PutObjectHeader {
                r#ref: Some(ext::ObjectRef {
                    connection_id: self.connection_id.clone(),
                    key: self.key(key),
                }),
                mode,
                if_match_etag: etag,
                sha256: hex::encode(Sha256::digest(bytes)),
                size: bytes.len() as u64,
            })),
        }];
        for chunk in bytes.chunks(65536) {
            chunks.push(ext::PutObjectChunk {
                part: Some(ext::put_object_chunk::Part::Data(chunk.to_vec())),
            });
        }
        let result = ext::cloud_remote_client::CloudRemoteClient::new(self.channel.clone())
            .put(tokio_stream::iter(chunks))
            .await
            .map_err(unavailable)?
            .into_inner();
        if !matches!(result.verification, 1 | 2) {
            return Err(unavailable("write verification missing"));
        }
        Ok(result.etag)
    }
    async fn delete(&self, key: &str) -> Result<()> {
        ext::cloud_remote_client::CloudRemoteClient::new(self.channel.clone())
            .delete(ext::DeleteObjectRequest {
                r#ref: Some(ext::ObjectRef {
                    connection_id: self.connection_id.clone(),
                    key: self.key(key),
                }),
                if_match_etag: String::new(),
            })
            .await
            .map_err(unavailable)?;
        Ok(())
    }
}
impl Drop for S3Objects {
    fn drop(&mut self) {
        if let Some(root) = &self.cleanup_directory {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
pub struct RcloneObjects {
    pub adapter: crate::rclone::adapter::RcloneAdapter,
    pub remote: String,
    pub prefix: String,
    pub cleanup_name: Option<String>,
}
#[async_trait]
impl ObjectIo for RcloneObjects {
    async fn read(&self, key: &str) -> Result<Option<ObjectData>> {
        let key = format!("{}{}", self.prefix, key);
        match self.adapter.stat(&self.remote, &key).await {
            Ok(stat) => {
                if stat["item"]["Size"].as_u64().unwrap_or(0) > MAX as u64 {
                    return Err(unavailable("limit"));
                }
            }
            Err(e) if e.contains("not found") || e.contains("404") => return Ok(None),
            Err(e) => return Err(unavailable(e)),
        };
        let bytes = self
            .adapter
            .read(&self.remote, &key)
            .await
            .map_err(unavailable)?;
        if bytes.len() > MAX {
            return Err(unavailable("limit"));
        }
        Ok(Some(ObjectData {
            etag: hex::encode(Sha256::digest(&bytes)),
            bytes,
        }))
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let keys = self
            .adapter
            .list(&self.remote, &format!("{}{}", self.prefix, prefix))
            .await
            .map_err(unavailable)?;
        Ok(keys
            .into_iter()
            .filter_map(|k| k.strip_prefix(&self.prefix).map(str::to_owned))
            .collect())
    }
    async fn put(&self, key: &str, bytes: &[u8], condition: WriteCondition) -> Result<String> {
        if !matches!(condition, WriteCondition::Overwrite) {
            return Err(OwnershipError::Refused(
                "location.conditional_unsupported".into(),
            ));
        }
        self.adapter
            .write_verified(&self.remote, &format!("{}{}", self.prefix, key), bytes)
            .await
            .map_err(unavailable)?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
    async fn delete(&self, key: &str) -> Result<()> {
        self.adapter
            .deletefile(&self.remote, &format!("{}{}", self.prefix, key))
            .await
            .map_err(unavailable)
    }
}

impl Drop for RcloneObjects {
    fn drop(&mut self) {
        if let (Some(name), Ok(handle)) = (
            self.cleanup_name.clone(),
            tokio::runtime::Handle::try_current(),
        ) {
            let adapter =
                crate::rclone::adapter::RcloneAdapter::with_socket(self.adapter.socket.clone());
            handle.spawn(async move {
                let _ = adapter.delete_recovery_remote(&name).await;
            });
        }
    }
}
