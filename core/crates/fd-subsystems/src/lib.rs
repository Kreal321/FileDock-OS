//! Subsystem manifests, registry, adapters, and systemd helpers.

pub mod adapter;
pub mod adapters;
pub mod manifest;
pub mod registry;
pub mod supervisor;
pub mod systemd;

pub use adapter::{AdapterError, SubsystemAdapter};
pub use adapters::{DockerAdapter, KopiaAdapter, NfsAdapter, SambaAdapter};
pub use manifest::{LoadedManifest, ManifestError, load_dir, validate_manifest};
pub use registry::{SubsystemRecord, SubsystemRegistry};
pub use systemd::Systemd;

pub const MIGRATION_0005_SUBSYSTEMS: &str = "
CREATE TABLE subsystem (
    id                   TEXT PRIMARY KEY,
    class                TEXT NOT NULL,
    manifest             TEXT NOT NULL CHECK (json_valid(manifest)),
    version              TEXT NOT NULL,
    previous_version     TEXT,
    approved_permissions TEXT NOT NULL,
    signature            TEXT NOT NULL,
    state                TEXT NOT NULL,
    enabled              INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    restart_count        INTEGER NOT NULL DEFAULT 0,
    last_failure_at      TEXT,
    package_sha256       TEXT,
    data_bindings        TEXT NOT NULL DEFAULT '{}',
    updated_at           TEXT NOT NULL
) STRICT;
";
