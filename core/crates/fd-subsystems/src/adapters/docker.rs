use super::{HealthOnly, systemd_health};
use crate::adapter::Health;

#[derive(Default)]
pub struct DockerAdapter;

impl HealthOnly for DockerAdapter {
    const ID: &'static str = "docker";
    const NAME: &'static str = "Docker Engine";
    const VERSION: &'static str = "29.9.0";
    const CAPABILITIES: &'static [&'static str] = &["runtime.container"];

    fn probe(&self) -> Health {
        systemd_health("docker.service")
    }
}
