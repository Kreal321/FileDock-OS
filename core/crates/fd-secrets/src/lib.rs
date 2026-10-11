//! Secret store, redaction, and tmpfs materialization (FR-028 to FR-031).

pub mod materialize;
pub mod recovery_key;
pub mod redact;
pub mod store;

pub use materialize::{Materialized, materialize};
pub use recovery_key::{GeneratedKey, RecoveryError, RecoveryKeyStore};
pub use redact::{redact_text, redact_value};
pub use store::{SecretInfo, SecretStore, StoreError};
