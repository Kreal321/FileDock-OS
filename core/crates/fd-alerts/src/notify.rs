//! Durable alert transitions and per-channel delivery attempts; queue history is disposable.
use crate::Alert;
use fd_state::db::{Db, DbError};
use rusqlite::{Connection, params};
pub fn ensure_schema(c: &Connection) -> Result<(), DbError> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS alert_transition(seq INTEGER PRIMARY KEY AUTOINCREMENT, payload TEXT NOT NULL, transition TEXT NOT NULL, created INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS notification_delivery(channel TEXT NOT NULL, seq INTEGER NOT NULL, attempt INTEGER NOT NULL DEFAULT 0, next_at INTEGER NOT NULL DEFAULT 0, done INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(channel,seq));")?;
    Ok(())
}
pub fn record(c: &Connection, a: &Alert, transition: &str) -> Result<(), DbError> {
    ensure_schema(c)?;
    c.execute(
        "INSERT INTO alert_transition(payload,transition,created) VALUES(?1,?2,unixepoch())",
        params![serde_json::to_string(a)?, transition],
    )?;
    Ok(())
}
pub struct Pending {
    pub seq: i64,
    pub alert: Alert,
    pub transition: String,
    pub attempt: u32,
    pub key: String,
}
pub fn pending(db: &Db, channel: &str, min: crate::Severity) -> Result<Vec<Pending>, DbError> {
    db.with(|c|{
 ensure_schema(c)?;
 c.execute("INSERT OR IGNORE INTO notification_delivery(channel,seq) SELECT ?1,seq FROM alert_transition WHERE created>=unixepoch()-86400",[channel])?;
 let mut q=c.prepare("SELECT t.seq,t.payload,t.transition,d.attempt FROM notification_delivery d JOIN alert_transition t ON t.seq=d.seq WHERE d.channel=?1 AND d.done=0 AND d.next_at<=unixepoch() AND t.created>=unixepoch()-86400 ORDER BY t.seq LIMIT 100")?;
 let rows=q.query_map([channel],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,u32>(3)?)))?.collect::<Result<Vec<_>,_>>()?;
 let mut out=vec![];for (seq,payload,transition,attempt) in rows{let alert:Alert=serde_json::from_str(&payload)?;if alert.severity<min{c.execute("UPDATE notification_delivery SET done=1 WHERE channel=?1 AND seq=?2",params![channel,seq])?;continue}out.push(Pending{seq,alert,transition,attempt,key:format!("{channel}:{seq}")});}Ok(out)
})
}
pub fn finish(
    db: &Db,
    channel: &str,
    item: &Pending,
    delivered: bool,
    retryable: bool,
) -> Result<(), DbError> {
    db.with(|c|{let delay=(1_i64<<item.attempt.min(11)).min(3600);c.execute("UPDATE notification_delivery SET attempt=attempt+1,next_at=unixepoch()+?1,done=?2 WHERE channel=?3 AND seq=?4",params![delay,delivered||!retryable,channel,item.seq])?;Ok(())})
}
pub fn prune(db: &Db) -> Result<(), DbError> {
    db.with(|c|{ensure_schema(c)?;c.execute("DELETE FROM notification_delivery WHERE seq IN(SELECT seq FROM alert_transition WHERE created<unixepoch()-86400)",[])?;c.execute("DELETE FROM alert_transition WHERE created<unixepoch()-86400",[])?;Ok(())})
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlertStore, MIGRATION_0004_ALERTS, Severity};
    use fd_state::db::MIGRATION_0001_INIT;
    #[test]
    fn failed_delivery_is_deferred_and_retry_keeps_the_idempotency_key() {
        let db = Db::open_in_memory(&[MIGRATION_0001_INIT, MIGRATION_0004_ALERTS]).unwrap();
        let store = AlertStore::new(db.clone());
        store
            .raise("test", None, "critical", Severity::Critical, "test")
            .unwrap();
        let pending = pending(&db, "retry", Severity::Info).unwrap();
        let key = pending[0].key.clone();
        finish(&db, "retry", &pending[0], false, true).unwrap();
        assert!(
            super::pending(&db, "retry", Severity::Info)
                .unwrap()
                .is_empty()
        );
        db.with(|c| c.execute("UPDATE notification_delivery SET next_at=0", []))
            .unwrap();
        let retried = super::pending(&db, "retry", Severity::Info).unwrap();
        assert_eq!(retried[0].key, key);
        assert_eq!(retried[0].attempt, 1);
        assert_eq!(store.list(&[], None, 100).unwrap().len(), 1);
    }
    #[test]
    fn transitions_retry_idempotently_and_acknowledgements_are_distinct() {
        let db = Db::open_in_memory(&[MIGRATION_0001_INIT, MIGRATION_0004_ALERTS]).unwrap();
        let s = AlertStore::new(db.clone());
        let a = s
            .raise("test", None, "test", Severity::Critical, "condition")
            .unwrap();
        s.raise("test", None, "test", Severity::Critical, "condition")
            .unwrap();
        let p = pending(&db, "channel", Severity::Warning).unwrap();
        assert_eq!(p.len(), 1);
        finish(&db, "channel", &p[0], true, false).unwrap();
        assert!(pending(&db, "channel", Severity::Info).unwrap().is_empty());
        s.acknowledge(&a.id, "admin").unwrap();
        let p = pending(&db, "channel", Severity::Info).unwrap();
        assert_eq!(p[0].transition, "acknowledged");
        assert_ne!(p[0].key, "channel:1");
    }
}
