//! Human-state data with explicit provenance.
//!
//! A datum always states where it came from and whether it is SIMULATED, LIVE,
//! or REPLAY. Values are indices or measurements as declared by `unit`; they
//! are not diagnoses and make no claim about cognition, emotion, or wellness.

use crate::DataMode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SignalQuality {
    Good,
    Degraded,
    Invalid,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HumanStateDatum {
    /// What was measured or simulated, e.g. `operator_load_index`.
    pub metric: String,
    pub value: f64,
    /// Unit or scale, e.g. `index[0,1]` or `bpm`.
    pub unit: String,
    pub timestamp: i64,
    /// Producing component, e.g. `sim.operator_load.v1`.
    pub source: String,
    pub mode: DataMode,
    /// For REPLAY data, the mode in which it was originally recorded.
    #[serde(default)]
    pub original_mode: Option<DataMode>,
    /// The source's own reliability estimate in [0, 1]. Not a probability that
    /// the value is "true".
    pub confidence: f64,
    pub quality: SignalQuality,
}

#[derive(Debug, Error, PartialEq)]
pub enum HumanStateError {
    #[error("value is not finite")]
    NonFinite,
    #[error("confidence {0} is outside [0, 1]")]
    Confidence(f64),
    #[error("field `{0}` is empty or longer than 128 characters")]
    Field(&'static str),
    #[error("REPLAY data must state its original mode")]
    MissingOriginalMode,
    #[error("original_mode is only valid for REPLAY data")]
    UnexpectedOriginalMode,
}

impl HumanStateDatum {
    /// The only constructor simulators use. The mode is fixed to SIMULATED.
    pub fn simulated(
        metric: &str,
        value: f64,
        unit: &str,
        timestamp: i64,
        source: &str,
        confidence: f64,
        quality: SignalQuality,
    ) -> Self {
        Self {
            metric: metric.to_string(),
            value,
            unit: unit.to_string(),
            timestamp,
            source: source.to_string(),
            mode: DataMode::Simulated,
            original_mode: None,
            confidence,
            quality,
        }
    }

    /// Re-label a recorded datum for use as input to a new run.
    pub fn as_replay(&self) -> Self {
        let original = if self.mode == DataMode::Replay {
            self.original_mode
        } else {
            Some(self.mode)
        };
        Self {
            mode: DataMode::Replay,
            original_mode: original,
            ..self.clone()
        }
    }

    pub fn validate(&self) -> Result<(), HumanStateError> {
        if !self.value.is_finite() {
            return Err(HumanStateError::NonFinite);
        }
        if !(0.0..=1.0).contains(&self.confidence) || !self.confidence.is_finite() {
            return Err(HumanStateError::Confidence(self.confidence));
        }
        for (name, text) in [
            ("metric", &self.metric),
            ("unit", &self.unit),
            ("source", &self.source),
        ] {
            if text.trim().is_empty() || text.chars().count() > 128 {
                return Err(HumanStateError::Field(name));
            }
        }
        match (self.mode, self.original_mode) {
            (DataMode::Replay, None) => Err(HumanStateError::MissingOriginalMode),
            (DataMode::Simulated | DataMode::Live, Some(_)) => {
                Err(HumanStateError::UnexpectedOriginalMode)
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> HumanStateDatum {
        HumanStateDatum::simulated(
            "operator_load_index",
            0.4,
            "index[0,1]",
            1000,
            "sim.operator_load.v1",
            0.9,
            SignalQuality::Good,
        )
    }

    #[test]
    fn simulated_constructor_labels_simulated() {
        let datum = sample();
        assert_eq!(datum.mode, DataMode::Simulated);
        assert!(datum.validate().is_ok());
    }

    #[test]
    fn replay_keeps_the_original_mode() {
        let replay = sample().as_replay();
        assert_eq!(replay.mode, DataMode::Replay);
        assert_eq!(replay.original_mode, Some(DataMode::Simulated));
        assert_eq!(replay.as_replay().original_mode, Some(DataMode::Simulated));
        assert!(replay.validate().is_ok());
    }

    #[test]
    fn invalid_data_is_rejected() {
        let mut datum = sample();
        datum.value = f64::NAN;
        assert_eq!(datum.validate(), Err(HumanStateError::NonFinite));
        let mut datum = sample();
        datum.confidence = 1.5;
        assert!(matches!(
            datum.validate(),
            Err(HumanStateError::Confidence(_))
        ));
        let mut datum = sample();
        datum.mode = DataMode::Replay;
        assert_eq!(datum.validate(), Err(HumanStateError::MissingOriginalMode));
        let mut datum = sample();
        datum.original_mode = Some(DataMode::Live);
        assert_eq!(
            datum.validate(),
            Err(HumanStateError::UnexpectedOriginalMode)
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let json = serde_json::json!({
            "metric": "m", "value": 1.0, "unit": "u", "timestamp": 1, "source": "s",
            "mode": "LIVE", "confidence": 1.0, "quality": "GOOD", "raw_eeg": [1, 2]
        });
        assert!(serde_json::from_value::<HumanStateDatum>(json).is_err());
    }
}
