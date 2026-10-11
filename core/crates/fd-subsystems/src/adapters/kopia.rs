use super::{HealthOnly, process_health};
use crate::adapter::Health;

#[derive(Default)]
pub struct KopiaAdapter;

impl HealthOnly for KopiaAdapter {
    const ID: &'static str = "kopia";
    const NAME: &'static str = "Kopia";
    const VERSION: &'static str = "0.23.1";
    const CAPABILITIES: &'static [&'static str] = &["backup.snapshot"];

    fn probe(&self) -> Health {
        process_health("kopia")
    }
}
