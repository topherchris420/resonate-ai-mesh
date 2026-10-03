//! Ingest gate for data arriving from outside the kernel process.
//!
//! External sources (a sensor process, the simulated biometric pipeline, a
//! notebook) may contribute *observations* and *human-state data*. They may
//! not inject kernel events: `validation`, `judgment`, `commitment`,
//! `state_transition`, and `human_resolution` are produced only inside the
//! kernel, so an ingested message of those types is refused rather than
//! re-broadcast.
//!
//! Every admitted human-state datum is schema-checked. A datum may claim LIVE
//! only when its source is on the configured live-source list; otherwise it is
//! refused. Nothing is ever relabeled from SIMULATED to LIVE.

use event_bus::{DataMode, EventEnvelope, HumanStateDatum};
use serde::Deserialize;
use std::collections::BTreeSet;
use thiserror::Error;

/// Event types an external source may submit.
pub const INGESTIBLE_EVENT_TYPES: &[&str] = &["human_state", "observation"];

#[derive(Debug, Clone)]
pub struct IngestPolicy {
    pub max_message_bytes: usize,
    pub max_messages_per_second: u32,
    /// Sources allowed to submit LIVE data.
    pub live_sources: BTreeSet<String>,
}

impl Default for IngestPolicy {
    fn default() -> Self {
        Self {
            max_message_bytes: 64 * 1024,
            max_messages_per_second: 50,
            live_sources: BTreeSet::new(),
        }
    }
}

impl IngestPolicy {
    /// `MESH_LIVE_SOURCES` is a comma-separated list of source names.
    pub fn from_env() -> Self {
        let mut policy = Self::default();
        if let Ok(list) = std::env::var("MESH_LIVE_SOURCES") {
            policy.live_sources = list
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect();
        }
        policy
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IngestError {
    #[error("message is {0} bytes; the limit is {1}")]
    TooLarge(usize, usize),
    #[error("message is not a valid event envelope: {0}")]
    Malformed(String),
    #[error("event type `{0}` cannot be submitted from outside the kernel")]
    ForbiddenEventType(String),
    #[error("human-state datum rejected: {0}")]
    InvalidDatum(String),
    #[error("source `{0}` is not registered as a LIVE source")]
    UnregisteredLiveSource(String),
    #[error("envelope mode {0} does not match datum mode {1}")]
    ModeMismatch(&'static str, &'static str),
    #[error("rate limit exceeded")]
    RateLimited,
}

impl IngestError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLarge(..) => "TOO_LARGE",
            Self::Malformed(_) => "MALFORMED",
            Self::ForbiddenEventType(_) => "FORBIDDEN_EVENT_TYPE",
            Self::InvalidDatum(_) => "INVALID_DATUM",
            Self::UnregisteredLiveSource(_) => "UNREGISTERED_LIVE_SOURCE",
            Self::ModeMismatch(..) => "MODE_MISMATCH",
            Self::RateLimited => "RATE_LIMITED",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanStatePayload {
    datum: HumanStateDatum,
}

/// An event admitted from an external source, with its parsed datum if any.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmittedEvent {
    pub envelope: EventEnvelope,
    pub datum: Option<HumanStateDatum>,
}

/// Check one inbound text message against the policy.
pub fn admit(text: &str, policy: &IngestPolicy) -> Result<AdmittedEvent, IngestError> {
    if text.len() > policy.max_message_bytes {
        return Err(IngestError::TooLarge(text.len(), policy.max_message_bytes));
    }
    let envelope: EventEnvelope = serde_json::from_str(text)
        .map_err(|error| IngestError::Malformed(truncate(&error.to_string(), 160)))?;
    if !INGESTIBLE_EVENT_TYPES.contains(&envelope.event_type.as_str()) {
        return Err(IngestError::ForbiddenEventType(truncate(
            &envelope.event_type,
            64,
        )));
    }
    for (field, value) in [
        ("event_id", &envelope.event_id),
        ("source", &envelope.source),
        ("subject_id", &envelope.subject_id),
    ] {
        if value.trim().is_empty() || value.chars().count() > 128 {
            return Err(IngestError::Malformed(format!(
                "`{field}` is empty or longer than 128 characters"
            )));
        }
    }
    let datum = if envelope.event_type == "human_state" {
        let payload = HumanStatePayload::deserialize(&envelope.payload)
            .map_err(|error| IngestError::InvalidDatum(truncate(&error.to_string(), 160)))?;
        let datum = payload.datum;
        datum
            .validate()
            .map_err(|error| IngestError::InvalidDatum(error.to_string()))?;
        if datum.mode != envelope.mode {
            return Err(IngestError::ModeMismatch(
                envelope.mode.as_str(),
                datum.mode.as_str(),
            ));
        }
        if datum.mode == DataMode::Live && !policy.live_sources.contains(&datum.source) {
            return Err(IngestError::UnregisteredLiveSource(truncate(
                &datum.source,
                64,
            )));
        }
        Some(datum)
    } else {
        if envelope.mode == DataMode::Live && !policy.live_sources.contains(&envelope.source) {
            return Err(IngestError::UnregisteredLiveSource(truncate(
                &envelope.source,
                64,
            )));
        }
        None
    };
    Ok(AdmittedEvent { envelope, datum })
}

/// Token bucket per connection. Time is passed in so tests stay deterministic.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    capacity: f64,
    tokens: f64,
    refill_per_ms: f64,
    last_ms: i64,
}

impl RateLimiter {
    pub fn new(per_second: u32, now_ms: i64) -> Self {
        let capacity = f64::from(per_second.max(1));
        Self {
            capacity,
            tokens: capacity,
            refill_per_ms: capacity / 1000.0,
            last_ms: now_ms,
        }
    }

    pub fn try_acquire(&mut self, now_ms: i64) -> Result<(), IngestError> {
        let elapsed = (now_ms - self.last_ms).max(0) as f64;
        self.tokens = (self.tokens + elapsed * self.refill_per_ms).min(self.capacity);
        self.last_ms = now_ms;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            Err(IngestError::RateLimited)
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    text.chars()
        .take(max)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn human_state(mode: &str, source: &str) -> serde_json::Value {
        json!({
            "event_id": "bio-1",
            "event_type": "human_state",
            "schema_version": "1.2.0",
            "timestamp": 1000,
            "source": "biometric_pipeline",
            "subject_id": "operator_alpha",
            "correlation_id": "c",
            "causation_id": "c",
            "provenance": "test",
            "mode": mode,
            "payload": {"datum": {
                "metric": "operator_load_index", "value": 0.4, "unit": "index[0,1]",
                "timestamp": 1000, "source": source, "mode": mode,
                "confidence": 0.9, "quality": "GOOD"
            }}
        })
    }

    #[test]
    fn simulated_human_state_is_admitted() {
        let admitted = admit(
            &human_state("SIMULATED", "sim").to_string(),
            &IngestPolicy::default(),
        )
        .unwrap();
        assert_eq!(admitted.datum.unwrap().mode, DataMode::Simulated);
    }

    #[test]
    fn kernel_event_types_cannot_be_injected() {
        for forged in [
            "commitment",
            "validation",
            "judgment",
            "state_transition",
            "human_resolution",
        ] {
            let mut value = human_state("SIMULATED", "sim");
            value["event_type"] = json!(forged);
            assert_eq!(
                admit(&value.to_string(), &IngestPolicy::default())
                    .unwrap_err()
                    .code(),
                "FORBIDDEN_EVENT_TYPE"
            );
        }
    }

    #[test]
    fn live_data_requires_a_registered_source() {
        let message = human_state("LIVE", "wristband-7").to_string();
        assert_eq!(
            admit(&message, &IngestPolicy::default()).unwrap_err(),
            IngestError::UnregisteredLiveSource("wristband-7".into())
        );
        let mut policy = IngestPolicy::default();
        policy.live_sources.insert("wristband-7".into());
        assert_eq!(
            admit(&message, &policy).unwrap().datum.unwrap().mode,
            DataMode::Live
        );
    }

    #[test]
    fn envelope_cannot_relabel_simulated_data_as_live() {
        let mut value = human_state("SIMULATED", "sim");
        value["mode"] = json!("LIVE");
        assert_eq!(
            admit(&value.to_string(), &IngestPolicy::default())
                .unwrap_err()
                .code(),
            "MODE_MISMATCH"
        );
    }

    #[test]
    fn oversized_malformed_and_raw_payloads_are_refused() {
        let policy = IngestPolicy {
            max_message_bytes: 64,
            ..IngestPolicy::default()
        };
        assert_eq!(
            admit(&"x".repeat(65), &policy).unwrap_err().code(),
            "TOO_LARGE"
        );
        assert_eq!(
            admit("not json", &IngestPolicy::default())
                .unwrap_err()
                .code(),
            "MALFORMED"
        );
        let mut value = human_state("SIMULATED", "sim");
        value["payload"]["datum"]["raw_eeg"] = json!([1, 2, 3]);
        assert_eq!(
            admit(&value.to_string(), &IngestPolicy::default())
                .unwrap_err()
                .code(),
            "INVALID_DATUM"
        );
        let mut value = human_state("SIMULATED", "sim");
        value["payload"]["datum"]["value"] = json!("NaN");
        assert_eq!(
            admit(&value.to_string(), &IngestPolicy::default())
                .unwrap_err()
                .code(),
            "INVALID_DATUM"
        );
    }

    #[test]
    fn rate_limiter_refills_over_time() {
        let mut limiter = RateLimiter::new(2, 0);
        assert!(limiter.try_acquire(0).is_ok());
        assert!(limiter.try_acquire(0).is_ok());
        assert_eq!(limiter.try_acquire(0), Err(IngestError::RateLimited));
        assert!(limiter.try_acquire(500).is_ok());
    }
}
