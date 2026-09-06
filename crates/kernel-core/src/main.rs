use event_bus::EventBus;
use kernel_core::KernelEngine;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    info!("Starting Pordenone Kernel Core...");

    let event_bus = EventBus::default();
    let _kernel = KernelEngine::new(event_bus);

    info!("Pordenone Kernel Core active. Listening for agent proposals & events.");

    tokio::signal::ctrl_c().await?;
    info!("Pordenone Kernel Core shutting down.");

    Ok(())
}
