use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Logs to journald when available, otherwise stderr. The env filter defaults to `info`.
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt = tracing_subscriber::fmt::layer().with_target(true);
    match tracing_journald::layer() {
        Ok(journal) => {
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(fmt)
                .with(journal)
                .try_init();
        }
        Err(_) => {
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(fmt)
                .try_init();
        }
    }
}
