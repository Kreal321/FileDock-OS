use super::{HealthOnly, systemd_health};
use crate::adapter::Health;

#[derive(Default)]
pub struct SambaAdapter;

impl HealthOnly for SambaAdapter {
    const ID: &'static str = "samba";
    const NAME: &'static str = "Samba";
    const VERSION: &'static str = "4.22.11";
    const CAPABILITIES: &'static [&'static str] = &["protocol.smb"];

    fn probe(&self) -> Health {
        systemd_health("smbd.service")
    }
}
