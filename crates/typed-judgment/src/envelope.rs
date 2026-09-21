use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ENVELOPE_SCHEMA_VERSION: &str = "pordenone.judgment.envelope.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveKind {
    Choice,
    Score,
    Noul,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Disposition {
    Pending,
    Pass,
    Revise,
    HumanReview,
    Unavailable,
    Skipped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Ok,
    Disabled,
    Timeout,
    Unauthorized,
    Forbidden,
    RateLimited,
    Overloaded,
    ServerError,
    MalformedResponse,
    NetworkError,
    UnknownModel,
    InvalidRequest,
    MissingCredentials,
    StateTooLarge,
    SerializationFailure,
    UnsupportedPrimitive,
    MissingFields,
}

impl ProviderStatus {
    pub fn reason_code(self) -> &'static str {
        match self {
            Self::Ok => "PROVIDER_OK",
            Self::Disabled => "JUDGMENT_DISABLED",
            Self::Timeout => "PROVIDER_TIMEOUT",
            Self::Unauthorized => "PROVIDER_UNAUTHORIZED",
            Self::Forbidden => "PROVIDER_FORBIDDEN",
            Self::RateLimited => "PROVIDER_RATE_LIMITED",
            Self::Overloaded => "PROVIDER_OVERLOADED",
            Self::ServerError => "PROVIDER_SERVER_ERROR",
            Self::MalformedResponse => "MALFORMED_RESPONSE",
            Self::NetworkError => "PROVIDER_NETWORK",
            Self::UnknownModel => "UNKNOWN_MODEL",
            Self::InvalidRequest => "PROVIDER_INVALID_REQUEST",
            Self::MissingCredentials => "MISSING_CREDENTIALS",
            Self::StateTooLarge => "STATE_TOO_LARGE",
            Self::SerializationFailure => "SERIALIZATION_FAILURE",
            Self::UnsupportedPrimitive => "UNSUPPORTED_PRIMITIVE",
            Self::MissingFields => "MISSING_FIELDS",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::Ok)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvaluationMode {
    Live,
    RecordedJudgment,
    LiveReevaluation,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgmentAnswer {
    pub question_id: String,
    pub primitive: PrimitiveKind,
    pub choice: Option<String>,
    pub score: Option<f64>,
    pub noul: Option<f64>,
    pub probabilities: BTreeMap<String, f64>,
    pub confidence: Option<f64>,
    pub legend: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JudgmentEnvelope {
    pub schema_version: String,
    pub judgment_id: String,
    pub provider: String,
    pub model: String,
    pub provider_model_version: String,
    pub question_set_version: String,
    pub state_hash: String,
    pub state_schema_version: String,
    pub truncated: bool,
    pub proposal_id: String,
    pub correlation_id: String,
    pub causation_id: String,
    pub answers: Vec<JudgmentAnswer>,
    pub disposition: Disposition,
    pub reason_codes: Vec<String>,
    pub policy_version: String,
    pub requested_at: i64,
    pub completed_at: i64,
    pub latency_ms: u64,
    pub provider_status: ProviderStatus,
    pub evaluation_mode: EvaluationMode,
    pub simulation_label: String,
    pub provider_request_id: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl JudgmentAnswer {
    pub fn choice(
        question_id: &str,
        selected: &str,
        probabilities: BTreeMap<String, f64>,
        confidence: Option<f64>,
    ) -> Self {
        Self {
            question_id: question_id.to_string(),
            primitive: PrimitiveKind::Choice,
            choice: Some(selected.to_string()),
            score: None,
            noul: None,
            probabilities,
            confidence,
            legend: BTreeMap::new(),
        }
    }

    pub fn score(
        question_id: &str,
        score: f64,
        probabilities: BTreeMap<String, f64>,
        legend: BTreeMap<String, String>,
        confidence: Option<f64>,
    ) -> Self {
        Self {
            question_id: question_id.to_string(),
            primitive: PrimitiveKind::Score,
            choice: None,
            score: Some(score),
            noul: None,
            probabilities,
            confidence,
            legend,
        }
    }

    pub fn noul(question_id: &str, noul: f64) -> Self {
        Self {
            question_id: question_id.to_string(),
            primitive: PrimitiveKind::Noul,
            choice: None,
            score: None,
            noul: Some(noul),
            probabilities: BTreeMap::new(),
            confidence: None,
            legend: BTreeMap::new(),
        }
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn new_judgment_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
