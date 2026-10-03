//! Canonical event model for Resonate AI Mesh.
//!
//! Every important event carries enough provenance to answer: who created it,
//! when (logical and wall time are kept apart), from what cause, under which
//! policy and software version, in which run and experiment, whether a model
//! was involved, and whether the data was simulated, live, or replayed.

pub mod canonical;
pub mod chain;
pub mod human_state;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;
use uuid::Uuid;

pub use canonical::{canonical_json, canonical_json_of, hash_canonical, quantize, sha256_hex};
pub use chain::{ChainError, ChainedEvent, EventChain};
pub use human_state::{HumanStateDatum, HumanStateError, SignalQuality};

/// Envelope schema for events emitted by this version.
///
/// 1.0.0 – original envelope. 1.1.0 – judgment, commitment, human_resolution.
/// 1.2.0 – run/experiment provenance, data mode, policy/software versions,
/// structured payload. 1.2.0 readers accept 1.0/1.1 envelopes (missing fields
/// default to an unlabeled SIMULATED event and `payload_json` is parsed).
pub const EVENT_SCHEMA_VERSION: &str = "1.2.0";
pub const EVENT_SCHEMA_V1: &str = "1.0.0";
pub const EVENT_SCHEMA_V1_1: &str = "1.1.0";

/// Semantic software version stamped into events. It changes only with a
/// release, so golden recordings stay valid across ordinary commits. The git
/// commit is recorded separately in run provenance.
pub const SOFTWARE_VERSION: &str = concat!("resonate-ai-mesh/", env!("CARGO_PKG_VERSION"));

/// Origin of the data an event describes.
/// Unlabeled data defaults to SIMULATED: nothing is presented as live by default.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataMode {
    /// Produced by a simulator in this repository.
    #[default]
    Simulated,
    /// Produced by a registered live source at the time of the event.
    Live,
    /// Re-fed from an earlier recording into a new run.
    Replay,
}

impl DataMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simulated => "SIMULATED",
            Self::Live => "LIVE",
            Self::Replay => "REPLAY",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventEnvelope {
    pub event_id: String,
    pub event_type: String,
    pub schema_version: String,
    /// Event time in milliseconds. In experiments this is logical simulation
    /// time, so it is reproducible. Wall time is kept in run provenance.
    pub timestamp: i64,
    pub source: String,
    pub subject_id: String,
    pub correlation_id: String,
    /// Event that directly caused this one ("caused_by").
    pub causation_id: String,
    pub provenance: String,
    pub run_id: String,
    pub experiment_id: String,
    /// Position in the run's event log. Assigned by the recorder.
    pub seq: u64,
    /// Simulation tick, when the event belongs to a stepped run.
    pub tick: Option<u64>,
    pub mode: DataMode,
    pub policy_version: String,
    pub software_version: String,
    /// True when a probabilistic or remote model produced or influenced this event.
    pub ai_involved: bool,
    pub payload: Value,
}

/// Accepts current envelopes and the legacy `payload_json` string form.
#[derive(Deserialize)]
struct WireEnvelope {
    event_id: String,
    event_type: String,
    schema_version: String,
    timestamp: i64,
    source: String,
    subject_id: String,
    correlation_id: String,
    causation_id: String,
    #[serde(default)]
    provenance: String,
    #[serde(default)]
    run_id: String,
    #[serde(default)]
    experiment_id: String,
    #[serde(default)]
    seq: u64,
    #[serde(default)]
    tick: Option<u64>,
    #[serde(default)]
    mode: DataMode,
    #[serde(default)]
    policy_version: String,
    #[serde(default)]
    software_version: String,
    #[serde(default)]
    ai_involved: bool,
    #[serde(default)]
    payload: Option<Value>,
    #[serde(default)]
    payload_json: Option<String>,
}

impl<'de> Deserialize<'de> for EventEnvelope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireEnvelope::deserialize(deserializer)?;
        let payload = match (wire.payload, wire.payload_json) {
            (Some(payload), _) => payload,
            (None, Some(text)) => serde_json::from_str(&text).map_err(serde::de::Error::custom)?,
            (None, None) => Value::Null,
        };
        Ok(Self {
            event_id: wire.event_id,
            event_type: wire.event_type,
            schema_version: wire.schema_version,
            timestamp: wire.timestamp,
            source: wire.source,
            subject_id: wire.subject_id,
            correlation_id: wire.correlation_id,
            causation_id: wire.causation_id,
            provenance: wire.provenance,
            run_id: wire.run_id,
            experiment_id: wire.experiment_id,
            seq: wire.seq,
            tick: wire.tick,
            mode: wire.mode,
            policy_version: wire.policy_version,
            software_version: wire.software_version,
            ai_involved: wire.ai_involved,
            payload,
        })
    }
}

impl EventEnvelope {
    /// Build an event with a random id and the current wall-clock time.
    /// Deterministic producers use [`EventStamp::envelope`] instead.
    pub fn new(
        event_type: impl Into<String>,
        source: impl Into<String>,
        subject_id: impl Into<String>,
        correlation_id: impl Into<String>,
        causation_id: impl Into<String>,
        provenance: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            schema_version: EVENT_SCHEMA_VERSION.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: source.into(),
            subject_id: subject_id.into(),
            correlation_id: correlation_id.into(),
            causation_id: causation_id.into(),
            provenance: provenance.into(),
            run_id: String::new(),
            experiment_id: String::new(),
            seq: 0,
            tick: None,
            mode: DataMode::Simulated,
            policy_version: String::new(),
            software_version: SOFTWARE_VERSION.to_string(),
            ai_involved: false,
            payload,
        }
    }

    pub fn with_schema_version(mut self, version: impl Into<String>) -> Self {
        self.schema_version = version.into();
        self
    }

    /// Typed view of the payload.
    pub fn payload_as<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        T::deserialize(&self.payload)
    }
}

/// Shared provenance applied to every event a component emits within one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventStamp {
    pub run_id: String,
    pub experiment_id: String,
    pub mode: DataMode,
    pub software_version: String,
}

impl Default for EventStamp {
    fn default() -> Self {
        Self {
            run_id: "interactive".to_string(),
            experiment_id: "none".to_string(),
            mode: DataMode::Simulated,
            software_version: SOFTWARE_VERSION.to_string(),
        }
    }
}

/// Fields a producer supplies for one event.
#[derive(Debug, Clone)]
pub struct EventDraft<'a> {
    pub event_type: &'a str,
    pub source: &'a str,
    pub subject_id: &'a str,
    pub correlation_id: &'a str,
    pub causation_id: &'a str,
    pub provenance: &'a str,
    pub policy_version: &'a str,
    pub ai_involved: bool,
    pub payload: Value,
}

impl EventStamp {
    pub fn envelope(
        &self,
        event_id: String,
        timestamp: i64,
        tick: Option<u64>,
        draft: EventDraft<'_>,
    ) -> EventEnvelope {
        EventEnvelope {
            event_id,
            event_type: draft.event_type.to_string(),
            schema_version: EVENT_SCHEMA_VERSION.to_string(),
            timestamp,
            source: draft.source.to_string(),
            subject_id: draft.subject_id.to_string(),
            correlation_id: draft.correlation_id.to_string(),
            causation_id: draft.causation_id.to_string(),
            provenance: draft.provenance.to_string(),
            run_id: self.run_id.clone(),
            experiment_id: self.experiment_id.clone(),
            seq: 0,
            tick,
            mode: self.mode,
            policy_version: draft.policy_version.to_string(),
            software_version: self.software_version.clone(),
            ai_involved: draft.ai_involved,
            payload: draft.payload,
        }
    }
}

/// In-process fan-out of events to live observers (dashboard, logs).
///
/// The bus is for observation only. It is lossy for slow subscribers by
/// design; the authoritative, lossless record of a run is the event log that
/// the kernel returns to its caller and the recorder hash-chains.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<EventEnvelope>,
    published: Arc<AtomicU64>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            sender,
            published: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Publish to current subscribers. Returns the number of receivers; zero
    /// receivers is not an error for an observation bus.
    pub fn publish(&self, event: EventEnvelope) -> usize {
        debug!(
            event_id = %event.event_id,
            event_type = %event.event_type,
            correlation_id = %event.correlation_id,
            source = %event.source,
            "event published"
        );
        self.published.fetch_add(1, Ordering::Relaxed);
        self.sender.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.sender.subscribe()
    }

    pub fn published_count(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// Events queued but not yet read by the slowest subscriber.
    pub fn queue_depth(&self) -> usize {
        self.sender.len()
    }

    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
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
    use serde_json::json;

    #[tokio::test]
    async fn publish_and_subscribe_preserve_identity() {
        let bus = EventBus::new(100);
        let mut rx = bus.subscribe();
        let event = EventEnvelope::new(
            "human_state",
            "biometric_pipeline",
            "operator_01",
            "corr_123",
            "caus_123",
            "sim.operator_load.v1",
            json!({"value": 0.45}),
        );
        assert_eq!(bus.publish(event.clone()), 1);
        let received = rx.recv().await.unwrap();
        assert_eq!(received, event);
        assert_eq!(bus.published_count(), 1);
    }

    #[test]
    fn publishing_without_subscribers_is_not_an_error() {
        let bus = EventBus::new(4);
        let event = EventEnvelope::new("x", "s", "sub", "c", "c", "p", Value::Null);
        assert_eq!(bus.publish(event), 0);
    }

    #[test]
    fn legacy_payload_json_envelopes_are_accepted() {
        let legacy = json!({
            "event_id": "bio_1",
            "event_type": "telemetry",
            "schema_version": "1.0.0",
            "timestamp": 1000,
            "source": "biometric_pipeline",
            "subject_id": "operator_alpha",
            "correlation_id": "c",
            "causation_id": "c",
            "provenance": "CIRCLE:SIMULATION",
            "payload_json": "{\"cognitive_load\":0.4}"
        });
        let event: EventEnvelope = serde_json::from_value(legacy).unwrap();
        assert_eq!(event.payload["cognitive_load"], json!(0.4));
        assert_eq!(event.mode, DataMode::Simulated);
        assert_eq!(event.run_id, "");
    }

    #[test]
    fn unlabeled_mode_defaults_to_simulated_never_live() {
        assert_eq!(DataMode::default(), DataMode::Simulated);
    }

    #[test]
    fn stamp_applies_run_provenance() {
        let stamp = EventStamp {
            run_id: "run-1".into(),
            experiment_id: "exp-1".into(),
            mode: DataMode::Simulated,
            software_version: SOFTWARE_VERSION.into(),
        };
        let event = stamp.envelope(
            "evt-1".into(),
            42,
            Some(3),
            EventDraft {
                event_type: "validation",
                source: "kernel",
                subject_id: "agent",
                correlation_id: "corr",
                causation_id: "prop",
                provenance: "validator",
                policy_version: "policy.v2",
                ai_involved: false,
                payload: json!({"accepted": true}),
            },
        );
        assert_eq!(event.run_id, "run-1");
        assert_eq!(event.tick, Some(3));
        assert_eq!(event.schema_version, EVENT_SCHEMA_VERSION);
        let round: EventEnvelope =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(round, event);
    }
}
