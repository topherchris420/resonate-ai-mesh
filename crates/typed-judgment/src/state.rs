use crate::questions::QUESTION_SET_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const STATE_SCHEMA_VERSION: &str = "pordenone.judgment.state.v1";

const OPERATOR_ALLOWLIST: &[&str] = &[
    "cognitive_load",
    "signal_quality",
    "baseline_delta",
    "cross_signal_coherence",
    "state_confidence",
    "confidence",
    "is_simulated",
];

const DENY_SUBSTRINGS: &[&str] = &[
    "eeg",
    "ppg",
    "eda",
    "waveform",
    "biosignal",
    "raw",
    "participant",
    "name",
    "credential",
    "password",
    "secret",
    "api_key",
    "token",
    "transcript",
    "memory",
];

#[derive(Debug, Clone, PartialEq)]
pub struct StateLimits {
    pub max_bytes: usize,
    pub max_text_chars: usize,
}

impl Default for StateLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16_384,
            max_text_chars: 512,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdaptiveSnapshot {
    pub state: String,
    pub stability: f64,
    pub resonance: f64,
    pub confidence: f64,
}

/// Owned inputs for one judgment. The provider never receives a write handle.
#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentCase {
    pub proposal_id: String,
    pub agent_id: String,
    pub action_type: String,
    pub target: [f64; 3],
    pub priority: i32,
    pub source_observation: String,
    pub observation_simulated: bool,
    pub validation_feasibility: f64,
    pub validation_accepted: bool,
    pub constraints_checked: Vec<String>,
    pub contradictions: Vec<String>,
    pub deterministic_confidence: f64,
    pub validation_provenance: String,
    pub adaptive: Option<AdaptiveSnapshot>,
    pub operator_raw: Value,
    pub spatial_coordinate_system: String,
    pub within_declared_bounds: bool,
    pub correlation_id: String,
    pub causation_id: String,
    pub simulation_status: String,
    pub source_event_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalJudgmentState {
    pub schema_version: String,
    pub simulation_label: String,
    pub proposal: ProposalEvidence,
    pub observation: ObservationEvidence,
    pub validation: ValidationEvidence,
    pub adaptive: AdaptiveEvidence,
    pub operator: OperatorEvidence,
    pub spatial: SpatialEvidence,
    pub limitations: Limitations,
    pub provenance: ProvenanceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProposalEvidence {
    pub proposal_id: String,
    pub agent_id: String,
    pub action_type: String,
    pub target: Vec3Evidence,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Vec3Evidence {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservationEvidence {
    pub source_observation_id: String,
    pub is_simulated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationEvidence {
    pub feasibility: f64,
    pub accepted: bool,
    pub constraints_checked: Vec<String>,
    pub contradictions: Vec<String>,
    pub deterministic_confidence: f64,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdaptiveEvidence {
    pub present: bool,
    pub state: Option<String>,
    pub stability: Option<f64>,
    pub resonance: Option<f64>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperatorEvidence {
    pub present: bool,
    pub cognitive_load: Option<f64>,
    pub signal_quality: Option<f64>,
    pub baseline_delta: Option<f64>,
    pub cross_signal_coherence: Option<f64>,
    pub state_confidence: Option<f64>,
    pub is_simulated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpatialEvidence {
    pub coordinate_system: String,
    pub target: Vec3Evidence,
    pub within_declared_bounds: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Limitations {
    pub missing_inputs: Vec<String>,
    pub excluded_fields: Vec<String>,
    pub truncated: bool,
    pub truncation_notes: Vec<String>,
    pub simulation_status: String,
    pub uncertainty: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvenanceEvidence {
    pub correlation_id: String,
    pub causation_id: String,
    pub validation_provenance: String,
    pub source_ids: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub state_schema_version: String,
    pub question_set_version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedJudgment {
    pub state: CanonicalJudgmentState,
    pub state_json: Value,
    pub canonical_json: String,
    pub state_hash: String,
    pub truncated: bool,
    pub simulation_label: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PrepareError {
    #[error("judgment state contains a non-finite number")]
    NonFinite,
    #[error("judgment state is {bytes} bytes and exceeds the {limit} byte bound")]
    StateTooLarge { bytes: usize, limit: usize },
    #[error("judgment state could not be serialized")]
    Serialization,
}

pub fn prepare_case(
    case: &JudgmentCase,
    limits: &StateLimits,
) -> Result<PreparedJudgment, PrepareError> {
    let mut state = assemble_state(case, limits)?;
    let mut canonical = canonical_json(&state)?;
    if canonical.len() > limits.max_bytes {
        shrink_state(&mut state, 64);
        state
            .limitations
            .truncation_notes
            .push("evidence text shortened to satisfy the remote size bound".to_string());
        state.limitations.truncated = true;
        canonical = canonical_json(&state)?;
    }
    if canonical.len() > limits.max_bytes {
        state.adaptive = AdaptiveEvidence {
            present: false,
            state: None,
            stability: None,
            resonance: None,
            confidence: None,
        };
        state
            .limitations
            .missing_inputs
            .push("adaptive_state".to_string());
        state.limitations.missing_inputs.sort();
        state.limitations.missing_inputs.dedup();
        state
            .limitations
            .truncation_notes
            .push("adaptive state omitted to satisfy the remote size bound".to_string());
        state.limitations.truncated = true;
        canonical = canonical_json(&state)?;
    }
    if canonical.len() > limits.max_bytes {
        return Err(PrepareError::StateTooLarge {
            bytes: canonical.len(),
            limit: limits.max_bytes,
        });
    }
    let state_json: Value =
        serde_json::from_str(&canonical).map_err(|_| PrepareError::Serialization)?;
    let truncated = state.limitations.truncated;
    let simulation_label = state.simulation_label.clone();
    Ok(PreparedJudgment {
        state_hash: hash_canonical(&canonical),
        state,
        state_json,
        canonical_json: canonical,
        truncated,
        simulation_label,
    })
}

fn assemble_state(
    case: &JudgmentCase,
    limits: &StateLimits,
) -> Result<CanonicalJudgmentState, PrepareError> {
    let (operator, mut excluded, mut operator_notes) = project_operator(&case.operator_raw)?;
    excluded.sort();
    excluded.dedup();

    let mut missing = Vec::new();
    if !operator.present {
        missing.push("operator_derived_state".to_string());
    }
    let adaptive = match &case.adaptive {
        Some(snapshot) => AdaptiveEvidence {
            present: true,
            state: Some(clip_token(&snapshot.state, 32)),
            stability: Some(round6(snapshot.stability)?),
            resonance: Some(round6(snapshot.resonance)?),
            confidence: Some(round6(snapshot.confidence)?),
        },
        None => {
            missing.push("adaptive_state".to_string());
            AdaptiveEvidence {
                present: false,
                state: None,
                stability: None,
                resonance: None,
                confidence: None,
            }
        }
    };

    let mut truncation_notes = Vec::new();
    let mut truncated = false;
    let (observation_id, observation_note) =
        bound_observation_id(&case.source_observation, limits.max_text_chars);
    if let Some(note) = observation_note {
        truncation_notes.push(note);
        truncated = true;
    }
    let (provenance, provenance_note) =
        clip_text(&case.validation_provenance, limits.max_text_chars);
    if let Some(note) = provenance_note {
        truncation_notes.push(note);
        truncated = true;
    }
    let mut contradictions = Vec::new();
    for (index, contradiction) in case.contradictions.iter().enumerate() {
        let (text, note) = clip_text(contradiction, limits.max_text_chars);
        if note.is_some() {
            truncated = true;
            truncation_notes.push(format!("validation.contradictions[{index}] truncated"));
        }
        contradictions.push(text);
    }

    let simulation_label =
        resolve_simulation_label(case, operator.is_simulated, &mut operator_notes);
    let mut uncertainty = vec![
        "Remote judgment cannot establish deterministic truth.".to_string(),
        "Proposal parameters are omitted from the remote evidence package.".to_string(),
        "Deterministic validation confidence is separate from typed-judgment confidence."
            .to_string(),
    ];
    uncertainty.extend(operator_notes);

    missing.sort();
    missing.dedup();

    Ok(CanonicalJudgmentState {
        schema_version: STATE_SCHEMA_VERSION.to_string(),
        simulation_label: simulation_label.clone(),
        proposal: ProposalEvidence {
            proposal_id: clip_token(&case.proposal_id, 128),
            agent_id: clip_token(&case.agent_id, 128),
            action_type: clip_token(&case.action_type, 64),
            target: vec3(case.target)?,
            priority: case.priority,
        },
        observation: ObservationEvidence {
            source_observation_id: observation_id,
            is_simulated: case.observation_simulated || simulation_label == "SIMULATED",
        },
        validation: ValidationEvidence {
            feasibility: round6(case.validation_feasibility)?,
            accepted: case.validation_accepted,
            constraints_checked: case
                .constraints_checked
                .iter()
                .map(|item| clip_token(item, 64))
                .collect(),
            contradictions,
            deterministic_confidence: round6(case.deterministic_confidence)?,
            provenance,
        },
        adaptive,
        operator,
        spatial: SpatialEvidence {
            coordinate_system: clip_token(&case.spatial_coordinate_system, 32),
            target: vec3(case.target)?,
            within_declared_bounds: case.within_declared_bounds,
        },
        limitations: Limitations {
            missing_inputs: missing,
            excluded_fields: excluded,
            truncated,
            truncation_notes,
            simulation_status: simulation_label,
            uncertainty,
        },
        provenance: ProvenanceEvidence {
            correlation_id: clip_token(&case.correlation_id, 128),
            causation_id: clip_token(&case.causation_id, 128),
            validation_provenance: clip_token(&case.validation_provenance, 160),
            source_ids: vec![clip_token(&case.proposal_id, 128)],
            source_event_ids: case
                .source_event_ids
                .iter()
                .map(|item| clip_token(item, 128))
                .collect(),
            state_schema_version: STATE_SCHEMA_VERSION.to_string(),
            question_set_version: QUESTION_SET_VERSION.to_string(),
        },
    })
}

fn shrink_state(state: &mut CanonicalJudgmentState, max_chars: usize) {
    let (provenance, _) = clip_text(&state.validation.provenance, max_chars);
    state.validation.provenance = provenance;
    for contradiction in &mut state.validation.contradictions {
        let (text, _) = clip_text(contradiction, max_chars);
        *contradiction = text;
    }
    state.limitations.truncated = true;
}

fn project_operator(
    raw: &Value,
) -> Result<(OperatorEvidence, Vec<String>, Vec<String>), PrepareError> {
    let mut excluded = Vec::new();
    let mut notes = Vec::new();
    let Some(object) = raw.as_object() else {
        if raw.is_null() {
            return Ok((empty_operator(), excluded, notes));
        }
        notes.push("operator payload was not an object and was dropped".to_string());
        return Ok((empty_operator(), excluded, notes));
    };
    if object.is_empty() {
        return Ok((empty_operator(), excluded, notes));
    }

    let mut evidence = empty_operator();
    evidence.present = true;
    for (key, value) in object {
        if denied_key(key) || !OPERATOR_ALLOWLIST.contains(&key.as_str()) {
            excluded.push(key.clone());
            continue;
        }
        match key.as_str() {
            "cognitive_load" => {
                assign_number(&mut evidence.cognitive_load, key, value, &mut excluded)?
            }
            "signal_quality" => {
                assign_number(&mut evidence.signal_quality, key, value, &mut excluded)?
            }
            "baseline_delta" => {
                assign_number(&mut evidence.baseline_delta, key, value, &mut excluded)?
            }
            "cross_signal_coherence" => assign_number(
                &mut evidence.cross_signal_coherence,
                key,
                value,
                &mut excluded,
            )?,
            "state_confidence" => {
                assign_number(&mut evidence.state_confidence, key, value, &mut excluded)?
            }
            "confidence" => {
                if evidence.state_confidence.is_none() {
                    assign_number(&mut evidence.state_confidence, key, value, &mut excluded)?;
                    if evidence.state_confidence.is_some() {
                        notes.push(
                            "operator.confidence was mapped to operator.state_confidence"
                                .to_string(),
                        );
                    }
                }
            }
            "is_simulated" => match value.as_bool() {
                Some(flag) => evidence.is_simulated = Some(flag),
                None => excluded.push(key.clone()),
            },
            _ => excluded.push(key.clone()),
        }
    }
    let any_value = evidence.cognitive_load.is_some()
        || evidence.signal_quality.is_some()
        || evidence.baseline_delta.is_some()
        || evidence.cross_signal_coherence.is_some()
        || evidence.state_confidence.is_some()
        || evidence.is_simulated.is_some();
    evidence.present = any_value;
    Ok((evidence, excluded, notes))
}

fn empty_operator() -> OperatorEvidence {
    OperatorEvidence {
        present: false,
        cognitive_load: None,
        signal_quality: None,
        baseline_delta: None,
        cross_signal_coherence: None,
        state_confidence: None,
        is_simulated: None,
    }
}

fn denied_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    DENY_SUBSTRINGS.iter().any(|needle| lower.contains(needle))
}

fn assign_number(
    slot: &mut Option<f64>,
    key: &str,
    value: &Value,
    excluded: &mut Vec<String>,
) -> Result<(), PrepareError> {
    match value.as_f64() {
        Some(number) => *slot = Some(round6(number)?),
        None => excluded.push(key.to_string()),
    }
    Ok(())
}

fn resolve_simulation_label(
    case: &JudgmentCase,
    operator_simulated: Option<bool>,
    notes: &mut Vec<String>,
) -> String {
    let flags_simulated = case.observation_simulated || operator_simulated == Some(true);
    let status = case.simulation_status.trim();
    if flags_simulated {
        if status.eq_ignore_ascii_case("LIVE") {
            notes
                .push("simulation flags conflicted; remote state is labeled SIMULATED".to_string());
        }
        return "SIMULATED".to_string();
    }
    if status.eq_ignore_ascii_case("LIVE") {
        return "LIVE".to_string();
    }
    if status.eq_ignore_ascii_case("SIMULATED") || status.is_empty() {
        return "SIMULATED".to_string();
    }
    clip_token(status, 32)
}

fn bound_observation_id(value: &str, max_chars: usize) -> (String, Option<String>) {
    if is_bounded_token(value) {
        return (value.to_string(), None);
    }
    let _ = max_chars;
    (
        "omitted_unstructured".to_string(),
        Some("unstructured observation text was omitted from the remote state".to_string()),
    )
}

fn is_bounded_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
}

fn clip_token(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in value.chars().take(max_chars) {
        if ch.is_control() {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}

fn clip_text(value: &str, max_chars: usize) -> (String, Option<String>) {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return (value.to_string(), None);
    }
    let clipped: String = value.chars().take(max_chars).collect();
    (
        clipped,
        Some(format!(
            "text truncated from {char_count} to {max_chars} characters"
        )),
    )
}

fn vec3(target: [f64; 3]) -> Result<Vec3Evidence, PrepareError> {
    Ok(Vec3Evidence {
        x: round6(target[0])?,
        y: round6(target[1])?,
        z: round6(target[2])?,
    })
}

fn round6(value: f64) -> Result<f64, PrepareError> {
    if !value.is_finite() {
        return Err(PrepareError::NonFinite);
    }
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded.abs() < f64::EPSILON {
        Ok(0.0)
    } else {
        Ok(rounded)
    }
}

fn canonical_json(state: &CanonicalJudgmentState) -> Result<String, PrepareError> {
    serde_json::to_string(state).map_err(|_| PrepareError::Serialization)
}

pub fn hash_canonical(canonical_json: &str) -> String {
    let digest = Sha256::digest(canonical_json.as_bytes());
    let mut hex = String::with_capacity(7 + digest.len() * 2);
    hex.push_str("sha256:");
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Used by tests that need an object with deliberately ordered keys.
#[cfg(test)]
pub fn operator_object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = serde_json::Map::new();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_case() -> JudgmentCase {
        JudgmentCase {
            proposal_id: "prop_1".into(),
            agent_id: "agent_alpha".into(),
            action_type: "PATROL".into(),
            target: [10.0, 20.5, 0.0],
            priority: 1,
            source_observation: "obs_001".into(),
            observation_simulated: true,
            validation_feasibility: 1.0,
            validation_accepted: true,
            constraints_checked: vec!["coordinate_bounds".into(), "action_allowlist".into()],
            contradictions: vec![],
            deterministic_confidence: 0.95,
            validation_provenance: "Validator:epistemic-v1:obs_001".into(),
            adaptive: Some(AdaptiveSnapshot {
                state: "ELEVATED".into(),
                stability: 0.8,
                resonance: 0.4,
                confidence: 0.7,
            }),
            operator_raw: serde_json::json!({
                "cognitive_load": 0.42,
                "confidence": 0.9,
                "is_simulated": true
            }),
            spatial_coordinate_system: "local_sim".into(),
            within_declared_bounds: true,
            correlation_id: "corr_1".into(),
            causation_id: "prop_1".into(),
            simulation_status: "SIMULATED".into(),
            source_event_ids: vec!["evt_obs_1".into()],
        }
    }

    #[test]
    fn canonical_state_hash_is_deterministic() {
        let case = sample_case();
        let first = prepare_case(&case, &StateLimits::default()).unwrap();
        let second = prepare_case(&case, &StateLimits::default()).unwrap();
        assert_eq!(first.state_hash, second.state_hash);
        assert!(first.state_hash.starts_with("sha256:"));
        assert_eq!(first.canonical_json, second.canonical_json);
        assert_eq!(first.state.schema_version, STATE_SCHEMA_VERSION);
        assert!(first
            .state
            .provenance
            .question_set_version
            .starts_with("pordenone.judgment.questions"));
    }

    #[test]
    fn hash_changes_when_proposal_changes() {
        let case = sample_case();
        let original = prepare_case(&case, &StateLimits::default()).unwrap();
        let mut changed = case;
        changed.priority = 4;
        let next = prepare_case(&changed, &StateLimits::default()).unwrap();
        assert_ne!(original.state_hash, next.state_hash);
    }

    #[test]
    fn operator_key_order_does_not_change_hash() {
        let mut left = sample_case();
        left.operator_raw = operator_object(vec![
            ("is_simulated", Value::Bool(true)),
            ("cognitive_load", serde_json::json!(0.42)),
            ("confidence", serde_json::json!(0.9)),
        ]);
        let mut right = sample_case();
        right.operator_raw = operator_object(vec![
            ("confidence", serde_json::json!(0.9)),
            ("cognitive_load", serde_json::json!(0.42)),
            ("is_simulated", Value::Bool(true)),
        ]);
        let left_prepared = prepare_case(&left, &StateLimits::default()).unwrap();
        let right_prepared = prepare_case(&right, &StateLimits::default()).unwrap();
        assert_eq!(left_prepared.state_hash, right_prepared.state_hash);
    }

    #[test]
    fn truncation_is_explicit() {
        let mut case = sample_case();
        case.validation_provenance = format!("prov_{}", "Z".repeat(800));
        let prepared = prepare_case(
            &case,
            &StateLimits {
                max_bytes: 16_384,
                max_text_chars: 32,
            },
        )
        .unwrap();
        assert!(prepared.truncated);
        assert!(prepared.state.limitations.truncated);
        assert!(!prepared.state.limitations.truncation_notes.is_empty());
        assert!(!prepared.canonical_json.contains(&"Z".repeat(800)));
        assert!(
            prepared.canonical_json.contains("incomplete")
                || prepared.canonical_json.contains("truncated")
        );
    }

    #[test]
    fn simulated_telemetry_is_labeled() {
        let prepared = prepare_case(&sample_case(), &StateLimits::default()).unwrap();
        assert_eq!(prepared.simulation_label, "SIMULATED");
        assert_eq!(prepared.state.simulation_label, "SIMULATED");
        assert!(prepared.state.observation.is_simulated);
        assert_eq!(prepared.state.operator.is_simulated, Some(true));
    }

    #[test]
    fn raw_biosignals_and_identity_are_excluded() {
        let mut case = sample_case();
        case.operator_raw = serde_json::json!({
            "cognitive_load": 0.33,
            "signal_quality": 0.8,
            "is_simulated": true,
            "heart_rate": 188,
            "hrv": 12,
            "eeg": "RAW_EEG_BUFFER_9f3a",
            "ppg": "RAW_PPG_BUFFER_zz",
            "eda": "RAW_EDA_BUFFER_qq",
            "waveform": [0.1, 0.2, 0.3],
            "operator_name": "Ada Lovelace",
            "participant_id": "person-77",
            "api_key": "super-secret-key",
            "transcript": "full private transcript should never leave"
        });
        let prepared = prepare_case(&case, &StateLimits::default()).unwrap();
        let json = &prepared.canonical_json;
        for marker in [
            "RAW_EEG_BUFFER_9f3a",
            "RAW_PPG_BUFFER_zz",
            "RAW_EDA_BUFFER_qq",
            "Ada Lovelace",
            "person-77",
            "super-secret-key",
            "full private transcript should never leave",
            "188",
        ] {
            assert!(!json.contains(marker), "{marker} leaked into remote state");
        }
        assert!(prepared
            .state
            .limitations
            .excluded_fields
            .iter()
            .any(|field| field == "heart_rate"));
        assert_eq!(prepared.state.operator.cognitive_load, Some(0.33));
        assert_eq!(prepared.state.operator.signal_quality, Some(0.8));
        assert!(prepared
            .state
            .limitations
            .excluded_fields
            .iter()
            .any(|field| field == "eeg"));
        assert!(prepared
            .state
            .limitations
            .excluded_fields
            .iter()
            .any(|field| field == "heart_rate"));
    }

    #[test]
    fn unstructured_transcript_observation_is_omitted() {
        let mut case = sample_case();
        case.source_observation =
            "This is an entire private transcript that must not be sent.".into();
        let prepared = prepare_case(&case, &StateLimits::default()).unwrap();
        assert_eq!(
            prepared.state.observation.source_observation_id,
            "omitted_unstructured"
        );
        assert!(!prepared
            .canonical_json
            .contains("entire private transcript"));
        assert!(prepared.truncated);
    }

    #[test]
    fn state_too_large_fails_closed() {
        let error = prepare_case(
            &sample_case(),
            &StateLimits {
                max_bytes: 8,
                max_text_chars: 8,
            },
        )
        .unwrap_err();
        assert!(matches!(error, PrepareError::StateTooLarge { .. }));
    }

    #[test]
    fn non_finite_numbers_fail_closed() {
        let mut case = sample_case();
        case.deterministic_confidence = f64::NAN;
        let error = prepare_case(&case, &StateLimits::default()).unwrap_err();
        assert_eq!(error, PrepareError::NonFinite);
    }
}
