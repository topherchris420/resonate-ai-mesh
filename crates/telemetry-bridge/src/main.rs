use event_bus::EventBus;
use telemetry_bridge::{BridgeConfig, TelemetryBridge};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let event_bus = EventBus::default();
    let config = BridgeConfig {
        ws_bind_addr: std::env::var("WS_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
    };

    let bridge = TelemetryBridge::new(event_bus, config);
    bridge.run().await?;

    Ok(())
}
