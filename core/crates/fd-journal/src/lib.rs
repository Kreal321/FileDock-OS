//! Operation journal: durable, idempotent state machines (FR-015, FR-016, research R6).

pub mod runner;

use rusqlite::Connection;

pub use runner::{
    Journal, JournalError, Operation, OperationState, OperationStep, StepOutcome, StepRunner,
};

/// Schema applied after `fd-state`'s initial migration.
pub const MIGRATION_0002_JOURNAL: &str = include_str!("schema.sql");

pub fn ensure_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(MIGRATION_0002_JOURNAL)
}
