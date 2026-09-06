use event_bus::{EventBus, EventEnvelope};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub ws_bind_addr: String,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            ws_bind_addr: "127.0.0.1:8080".to_string(),
        }
    }
}

pub struct TelemetryBridge {
    event_bus: EventBus,
    config: BridgeConfig,
}

impl TelemetryBridge {
    pub fn new(event_bus: EventBus, config: BridgeConfig) -> Self {
        Self { event_bus, config }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(&self.config.ws_bind_addr).await?;
        info!("Telemetry Bridge listening on WebSocket address: {}", self.config.ws_bind_addr);

        while let Ok((stream, addr)) = listener.accept().await {
            let bus = self.event_bus.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, addr, bus).await {
                    warn!("WebSocket client error for {}: {}", addr, e);
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(
        stream: TcpStream,
        addr: SocketAddr,
        event_bus: EventBus,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Accepted WebSocket connection from {}", addr);
        let ws_stream = accept_async(stream).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        let mut bus_receiver = event_bus.subscribe();

        let send_task = tokio::spawn(async move {
            while let Ok(event) = bus_receiver.recv().await {
                let json = serde_json::to_string(&event).unwrap_or_default();
                if ws_sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        });

        while let Some(msg) = ws_receiver.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                if let Ok(envelope) = serde_json::from_str::<EventEnvelope>(&text) {
                    let _ = event_bus.publish(envelope);
                } else {
                    let raw_event = EventEnvelope::new(
                        "telemetry",
                        "ws_client",
                        "client",
                        "ws_corr",
                        "ws_caus",
                        "LIVE",
                        text,
                    );
                    let _ = event_bus.publish(raw_event);
                }
            }
        }

        send_task.abort();
        info!("Closed WebSocket connection for {}", addr);
        Ok(())
    }
}
