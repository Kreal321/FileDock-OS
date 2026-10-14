//! Alert store (FR-096).

pub mod store;

pub use store::{Alert, AlertState, AlertStore, Severity};

pub const MIGRATION_0004_ALERTS: &str = "
CREATE TABLE alert (
    id              TEXT PRIMARY KEY,
    dedup_key       TEXT NOT NULL UNIQUE,
    severity        TEXT NOT NULL,
    kind            TEXT NOT NULL,
    source          TEXT NOT NULL,
    object_kind     TEXT,
    object_id       TEXT,
    message         TEXT NOT NULL,
    first_seen_at   TEXT NOT NULL,
    last_seen_at    TEXT NOT NULL,
    state           TEXT NOT NULL,
    acknowledged_by TEXT
) STRICT;

CREATE INDEX alert_by_state ON alert (state, last_seen_at DESC);
";

pub mod notify;
