use super::{HealthOnly, systemd_health};
use crate::adapter::Health;

#[derive(Default)]
pub struct NfsAdapter;

impl HealthOnly for NfsAdapter {
    const ID: &'static str = "nfs";
    const NAME: &'static str = "NFS";
    const VERSION: &'static str = "2.8.3";
    const CAPABILITIES: &'static [&'static str] = &["protocol.nfs"];

    fn probe(&self) -> Health {
        systemd_health("nfs-server.service")
    }
}
