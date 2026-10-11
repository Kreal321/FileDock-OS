//! Events and journald logging.

pub mod events;
pub mod logging;

pub use events::{Event, EventBus};
pub use logging::init_logging;

pub const MIGRATION_0003_EVENTS: &str = "
CREATE TABLE event (
    seq         INTEGER PRIMARY KEY,
    at          TEXT    NOT NULL,
    type        TEXT    NOT NULL,
    subject_kind TEXT,
    subject_id   TEXT,
    payload     TEXT    NOT NULL CHECK (json_valid(payload))
) STRICT;

CREATE INDEX event_by_at ON event (at);
";
