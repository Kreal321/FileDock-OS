//! Native S3 connector (research R23).

mod config;
mod integrity;
mod multipart;
mod ops;
mod probe;
#[cfg(test)]
mod retry;
mod server;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let config = config::Config::from_env();
    server::serve(config).await
}
