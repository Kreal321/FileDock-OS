//! Disposable live acceptance driver; never launched by the appliance.
use fd_connectors::{
    object_io::{RcloneObjects, S3Objects},
    ownership::{self, ClaimSafety, ObjectIo, WriteCondition},
    rclone::adapter::RcloneAdapter,
};
use tonic::transport::Channel;
async fn channel(socket: String) -> Channel {
    tonic::transport::Endpoint::from_static("http://localhost")
        .connect_with_connector(tower::service_fn(move |_| {
            let socket = socket.clone();
            async move {
                tokio::net::UnixStream::connect(socket)
                    .await
                    .map(hyper_util::rt::TokioIo::new)
            }
        }))
        .await
        .expect("fixture connector socket")
}
fn object(channel: Channel, prefix: String) -> S3Objects {
    S3Objects {
        channel,
        prefix,
        connection_id: std::env::var("FD_LIVE_CONNECTION").unwrap(),
        cleanup_directory: None,
    }
}
#[tokio::main]
async fn main() {
    let first = channel(std::env::var("FD_LIVE_SOCKET").unwrap()).await;
    let prefix = format!("acceptance/{}/", uuid::Uuid::now_v7());
    match std::env::var("FD_LIVE_KIND").as_deref() {
        Ok("claims") => {
            let second = channel(std::env::var("FD_LIVE_SOCKET_2").unwrap()).await;
            for n in 0..100 {
                let a = object(first.clone(), format!("{prefix}{n}/"));
                let b = object(second.clone(), format!("{prefix}{n}/"));
                let x = uuid::Uuid::now_v7().to_string();
                let y = uuid::Uuid::now_v7().to_string();
                let (left, right) = tokio::join!(
                    ownership::claim(&a, &x, "fixture-a", ClaimSafety::Conditional, false),
                    ownership::claim(&b, &y, "fixture-b", ClaimSafety::Conditional, false)
                );
                assert_eq!(
                    usize::from(left.is_ok()) + usize::from(right.is_ok()),
                    1,
                    "round {n}: {left:?} {right:?}"
                );
                let owner: ownership::Owner = serde_json::from_slice(
                    &a.read(ownership::OWNER_KEY).await.unwrap().unwrap().bytes,
                )
                .unwrap();
                assert_eq!(owner.appliance_id, if left.is_ok() { x } else { y });
                a.delete(ownership::OWNER_KEY).await.unwrap();
            }
            println!(
                "PASS: 100 competing production claims through two native connector processes; exactly one recorded owner each round"
            );
        }
        Ok("cross") => {
            let a = object(first, prefix.clone());
            let b = RcloneObjects {
                adapter: RcloneAdapter::with_socket(
                    std::env::var("FD_LIVE_RCLONE_SOCKET").unwrap().into(),
                ),
                remote: std::env::var("FD_LIVE_RCLONE_REMOTE").unwrap(),
                prefix,
                cleanup_name: None,
            };
            for name in [
                "owner.json",
                "index.json",
                "keys/local.key",
                "snapshot.fdstate",
                "extension.fdpkg",
            ] {
                let bytes = format!("native fixture {name}").into_bytes();
                a.put(name, &bytes, WriteCondition::Overwrite)
                    .await
                    .unwrap();
                assert_eq!(b.read(name).await.unwrap().unwrap().bytes, bytes);
                let bytes = format!("rclone fixture {name}").into_bytes();
                b.put(name, &bytes, WriteCondition::Overwrite)
                    .await
                    .unwrap();
                assert_eq!(a.read(name).await.unwrap().unwrap().bytes, bytes);
                a.delete(name).await.unwrap();
            }
            println!(
                "PASS: native/rclone bidirectional production ObjectIo reads and verified writes for all five object types"
            );
        }
        _ => panic!("FD_LIVE_KIND must be claims or cross"),
    }
}
