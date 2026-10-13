//! Durable replay is the source of truth; broadcast only wakes subscribers.
use crate::proto_conv;
use fd_api::core::v1 as pb;
use fd_observe::EventBus;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

pub fn stream(
    bus: Arc<EventBus>,
    after_seq: u64,
) -> Result<ReceiverStream<Result<pb::Event, Status>>, Status> {
    // Subscribe before reading the cursor so a concurrent commit cannot fall in a gap.
    let mut live = bus.subscribe();
    let mut cursor = if after_seq == 0 {
        bus.latest_seq().map_err(internal)?
    } else {
        after_seq
    };
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            loop {
                let page = match bus.after(cursor, 256) {
                    Ok(page) => page,
                    Err(error) => {
                        let _ = tx.send(Err(internal(error))).await;
                        return;
                    }
                };
                if page.is_empty() {
                    break;
                }
                for event in page {
                    cursor = event.seq;
                    if tx.send(Ok(to_proto(&event))).await.is_err() {
                        return;
                    }
                }
            }
            tokio::select! {
                _ = tx.closed() => return,
                _ = tick.tick() => {},
                result = live.recv() => {
                    if matches!(result, Err(tokio::sync::broadcast::error::RecvError::Closed)) { return; }
                    // Lag is repaired from SQLite, never silently skipped.
                }
            }
        }
    });
    Ok(ReceiverStream::new(rx))
}

fn internal(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}
pub fn timestamp(text: &str) -> Option<prost_types::Timestamp> {
    fd_state::db::parse_timestamp(text).map(|at| prost_types::Timestamp {
        seconds: at.timestamp(),
        nanos: at.timestamp_subsec_nanos() as i32,
    })
}
fn to_proto(event: &fd_observe::Event) -> pb::Event {
    pb::Event {
        seq: event.seq,
        at: timestamp(&event.at),
        r#type: event.r#type.clone(),
        subject: event.subject.as_ref().map(proto_conv::ref_to_proto),
        payload: Some(proto_conv::value_to_struct(&event.payload)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fd_state::db::{Db, MIGRATION_0001_INIT};
    use tokio_stream::StreamExt;
    fn bus() -> Arc<EventBus> {
        Arc::new(EventBus::new(
            Db::open_in_memory(&[MIGRATION_0001_INIT, fd_observe::MIGRATION_0003_EVENTS]).unwrap(),
        ))
    }
    #[tokio::test]
    async fn replay_pages_and_lag_are_lossless_and_ordered() {
        let bus = bus();
        let first = bus.emit("initial", None, serde_json::json!({})).unwrap();
        for _ in 0..1100 {
            bus.emit("change", None, serde_json::json!({})).unwrap();
        }
        let mut stream = stream(bus.clone(), first.seq).unwrap();
        for _ in 0..600 {
            bus.emit("live", None, serde_json::json!({})).unwrap();
        }
        for expected in first.seq + 1..=first.seq + 1700 {
            let event = tokio::time::timeout(std::time::Duration::from_secs(3), stream.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(event.seq, expected);
            assert!(event.at.is_some());
        }
    }
    #[tokio::test]
    async fn zero_starts_now_and_reconnect_resumes() {
        let bus = bus();
        bus.emit("old", None, serde_json::json!({})).unwrap();
        let mut live = stream(bus.clone(), 0).unwrap();
        let fresh = bus.emit("new", None, serde_json::json!({})).unwrap();
        assert_eq!(live.next().await.unwrap().unwrap().seq, fresh.seq);
        drop(live);
        let next = bus.emit("offline", None, serde_json::json!({})).unwrap();
        assert_eq!(
            stream(bus, fresh.seq)
                .unwrap()
                .next()
                .await
                .unwrap()
                .unwrap()
                .seq,
            next.seq
        );
    }
}
