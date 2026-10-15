use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Logs once to journald when available, otherwise stderr. Keep dependency noise
/// at warning while retaining FileDock lifecycle information; RUST_LOG overrides this.
pub fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,filedock=info,fd_=info"));
    let fmt = tracing_subscriber::fmt::layer().with_target(true);
    match tracing_journald::layer() {
        Ok(journal) => {
            let _ = tracing_subscriber::registry()
                .with(filter)
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
