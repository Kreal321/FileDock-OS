//! Live interoperability test. Both connectors must already target the same disposable bucket.
use fd_api::extension::v1 as ext;
use fd_connectors::{client::CloudRemoteClient, rclone::adapter::RcloneAdapter};
use sha2::{Digest, Sha256};

#[tokio::test]
#[ignore = "requires configured fd-s3 and rclone sockets and a disposable shared bucket"]
async fn both_connectors_read_each_others_objects() {
    let socket = std::env::var("FD_S3_SOCKET").expect("FD_S3_SOCKET");
    let connection = std::env::var("FD_CROSS_CONNECTION").expect("FD_CROSS_CONNECTION");
    let remote =
        std::env::var("FD_CROSS_RCLONE_REMOTE").expect("FD_CROSS_RCLONE_REMOTE (remote:bucket)");
    let rclone = RcloneAdapter::with_socket(
        std::env::var("FD_RCLONE_SOCKET")
            .expect("FD_RCLONE_SOCKET")
            .into(),
    );
    let channel = tonic::transport::Endpoint::from_static("http://localhost")
        .connect_with_connector(tower::service_fn(move |_| {
            let socket = socket.clone();
            async move {
                tokio::net::UnixStream::connect(socket)
                    .await
                    .map(hyper_util::rt::TokioIo::new)
            }
        }))
        .await
        .unwrap();
    let mut native = ext::cloud_remote_client::CloudRemoteClient::new(channel);
    let prefix = format!("_filedock/cross-test/{}", uuid::Uuid::now_v7());
    for name in [
        "owner.json",
        "index.json",
        "keys/local.key",
        "state.fdstate",
        "pkg.fdpkg",
    ] {
        let key = format!("{prefix}/{name}");
        let bytes = format!("interoperability fixture for {name}").into_bytes();
        let object = ext::ObjectRef {
            connection_id: connection.clone(),
            key,
        };
        let chunks = vec![
            ext::PutObjectChunk {
                part: Some(ext::put_object_chunk::Part::Header(ext::PutObjectHeader {
                    r#ref: Some(object.clone()),
                    sha256: hex::encode(Sha256::digest(&bytes)),
                    size: bytes.len() as u64,
                    ..Default::default()
                })),
            },
            ext::PutObjectChunk {
                part: Some(ext::put_object_chunk::Part::Data(bytes.clone())),
            },
        ];
        native.put(tokio_stream::iter(chunks)).await.unwrap();
        assert_eq!(rclone.read(&remote, &object.key).await.unwrap(), bytes);
        let replacement = format!("written by rclone: {name}").into_bytes();
        rclone
            .write_verified(&remote, &object.key, &replacement)
            .await
            .unwrap();
        let stream = native
            .get(ext::GetObjectRequest {
                r#ref: Some(object.clone()),
                expected_sha256: hex::encode(Sha256::digest(&replacement)),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            CloudRemoteClient::get_verified(stream.into_inner())
                .await
                .unwrap()
                .0,
            replacement
        );
        native
            .delete(ext::DeleteObjectRequest {
                r#ref: Some(object),
                ..Default::default()
            })
            .await
            .unwrap();
    }
}
