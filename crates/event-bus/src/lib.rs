use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: String,
    pub event_type: String,
    pub schema_version: String,
    pub timestamp: i64,
    pub source: String,
    pub subject_id: String,
    pub correlation_id: String,
    pub causation_id: String,
    pub provenance: String,
    pub payload_json: String,
}

impl EventEnvelope {
    pub fn new(
        event_type: impl Into<String>,
        source: impl Into<String>,
        subject_id: impl Into<String>,
        correlation_id: impl Into<String>,
        causation_id: impl Into<String>,
        provenance: impl Into<String>,
        payload_json: impl Into<String>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            schema_version: "1.0.0".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: source.into(),
            subject_id: subject_id.into(),
            correlation_id: correlation_id.into(),
            causation_id: causation_id.into(),
            provenance: provenance.into(),
            payload_json: payload_json.into(),
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<EventEnvelope>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    #[allow(clippy::result_large_err)]
    pub fn publish(
        &self,
        event: EventEnvelope,
    ) -> Result<usize, broadcast::error::SendError<EventEnvelope>> {
        info!(
            event_id = %event.event_id,
            event_type = %event.event_type,
            correlation_id = %event.correlation_id,
            source = %event.source,
            "Event Published"
        );
        self.sender.send(event)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(100);
        let mut rx = bus.subscribe();

        let event = EventEnvelope::new(
            "telemetry",
            "circle_pipeline",
            "operator_01",
            "corr_123",
            "caus_123",
            "SIMULATED",
            r#"{"cognitive_load": 0.45}"#,
        );

        bus.publish(event.clone()).unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event_id, event.event_id);
        assert_eq!(received.correlation_id, "corr_123");
        assert_eq!(received.event_type, "telemetry");
    }
}
