use crate::envelope::{now_ms, JudgmentAnswer, JudgmentEnvelope, PrimitiveKind, ProviderStatus};
use crate::provider::{draft_envelope, JudgmentRequest};
use crate::questions::{typesafe_questions_wire, AtomicQuestion, QuestionSet};
use async_trait::async_trait;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(test)]
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SYSTEM_ONE_PATH: &str = "/v1/systemone";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawHttpResponse {
    pub status: u16,
    pub body: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    Timeout,
    Network,
}

#[async_trait]
pub trait TypeSafeTransport: Send + Sync {
    async fn post_system_one(&self, body: Value) -> Result<RawHttpResponse, TransportError>;
    fn redact_secret(&self) -> String;
}

pub struct TypeSafeJudgmentProvider<T: TypeSafeTransport> {
    model: String,
    transport: T,
}

impl<T: TypeSafeTransport> TypeSafeJudgmentProvider<T> {
    pub fn new(model: impl Into<String>, transport: T) -> Self {
        Self {
            model: model.into(),
            transport,
        }
    }
}

#[async_trait]
impl<T: TypeSafeTransport + Send + Sync> crate::provider::JudgmentProvider
    for TypeSafeJudgmentProvider<T>
{
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        let started = Instant::now();
        let requested_at = now_ms();
        let question_set = crate::questions::proposal_question_set();
        let body =
            build_system_one_request(&request.prepared.state_json, &self.model, &question_set);
        let raw = match self.transport.post_system_one(body).await {
            Ok(raw) => raw,
            Err(TransportError::Timeout) => {
                return finish(
                    request,
                    &self.model,
                    ProviderStatus::Timeout,
                    Vec::new(),
                    None,
                    None,
                    None,
                    None,
                    requested_at,
                    started,
                );
            }
            Err(TransportError::Network) => {
                return finish(
                    request,
                    &self.model,
                    ProviderStatus::NetworkError,
                    Vec::new(),
                    None,
                    None,
                    None,
                    None,
                    requested_at,
                    started,
                );
            }
        };
        let secret = self.transport.redact_secret();
        let interpreted = interpret_http(&raw, &question_set, &secret);
        finish(
            request,
            &self.model,
            interpreted.status,
            interpreted.answers,
            interpreted.provider_model_version,
            interpreted.request_id,
            interpreted.input_tokens,
            interpreted.output_tokens,
            requested_at,
            started,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn finish(
    request: &JudgmentRequest,
    model: &str,
    status: ProviderStatus,
    answers: Vec<JudgmentAnswer>,
    provider_model_version: Option<String>,
    request_id: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    requested_at: i64,
    started: Instant,
) -> JudgmentEnvelope {
    let mut envelope = draft_envelope(request, "typesafe", model, status, answers);
    envelope.provider_model_version = provider_model_version.unwrap_or_else(|| model.to_string());
    envelope.provider_request_id = request_id;
    envelope.input_tokens = input_tokens;
    envelope.output_tokens = output_tokens;
    envelope.requested_at = requested_at;
    envelope.completed_at = now_ms();
    envelope.latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    envelope
}

pub(crate) fn build_system_one_request(
    state: &Value,
    model: &str,
    questions: &QuestionSet,
) -> Value {
    let mut body = Map::new();
    body.insert("state".to_string(), state.clone());
    body.insert("model".to_string(), Value::String(model.to_string()));
    body.insert(
        "questions".to_string(),
        Value::Object(typesafe_questions_wire(questions)),
    );
    Value::Object(body)
}

struct InterpretedResponse {
    status: ProviderStatus,
    answers: Vec<JudgmentAnswer>,
    provider_model_version: Option<String>,
    request_id: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

pub fn redact_secret(text: &str, secret: &str) -> String {
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, "[REDACTED]")
}

pub fn failure_log_line(status: ProviderStatus, http_status: u16, request_id: &str) -> String {
    format!(
        "typed judgment provider request failed provider_status={} http_status={http_status} request_id={request_id}",
        status.reason_code()
    )
}

fn interpret_http(
    raw: &RawHttpResponse,
    questions: &QuestionSet,
    secret: &str,
) -> InterpretedResponse {
    let request_id = raw
        .request_id
        .as_deref()
        .map(|value| redact_secret(value, secret))
        .filter(|value| !value.is_empty());
    if raw.status != 200 {
        let status = map_http_status(raw.status, &raw.body);
        let line = failure_log_line(status, raw.status, request_id.as_deref().unwrap_or(""));
        tracing::warn!("{line}");
        return InterpretedResponse {
            status,
            answers: Vec::new(),
            provider_model_version: None,
            request_id,
            input_tokens: None,
            output_tokens: None,
        };
    }
    match parse_system_one_response(&raw.body, questions) {
        Ok(parsed) => InterpretedResponse {
            status: ProviderStatus::Ok,
            answers: parsed.answers,
            provider_model_version: Some(parsed.provider_model_version),
            request_id,
            input_tokens: parsed.input_tokens,
            output_tokens: parsed.output_tokens,
        },
        Err(fault) => {
            let status = fault.status();
            let line = failure_log_line(status, raw.status, request_id.as_deref().unwrap_or(""));
            tracing::warn!("{line}");
            InterpretedResponse {
                status,
                answers: Vec::new(),
                provider_model_version: None,
                request_id,
                input_tokens: None,
                output_tokens: None,
            }
        }
    }
}

fn map_http_status(status: u16, body: &str) -> ProviderStatus {
    match status {
        401 => ProviderStatus::Unauthorized,
        403 => ProviderStatus::Forbidden,
        404 => ProviderStatus::UnknownModel,
        422 => {
            if body.to_ascii_lowercase().contains("model") {
                ProviderStatus::UnknownModel
            } else {
                ProviderStatus::InvalidRequest
            }
        }
        429 => ProviderStatus::RateLimited,
        529 => ProviderStatus::Overloaded,
        500..=599 => ProviderStatus::ServerError,
        _ => ProviderStatus::ServerError,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ResponseFault {
    Malformed,
    MissingFields,
    UnsupportedPrimitive,
}

impl ResponseFault {
    fn status(self) -> ProviderStatus {
        match self {
            Self::Malformed => ProviderStatus::MalformedResponse,
            Self::MissingFields => ProviderStatus::MissingFields,
            Self::UnsupportedPrimitive => ProviderStatus::UnsupportedPrimitive,
        }
    }
}

pub(crate) struct ParsedResponse {
    provider_model_version: String,
    answers: Vec<JudgmentAnswer>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

pub(crate) fn parse_system_one_response(
    body: &str,
    questions: &QuestionSet,
) -> Result<ParsedResponse, ResponseFault> {
    let value: Value = serde_json::from_str(body).map_err(|_| ResponseFault::Malformed)?;
    let root = value.as_object().ok_or(ResponseFault::Malformed)?;
    let provider_model_version = root
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .ok_or(ResponseFault::MissingFields)?
        .to_string();
    let answers_object = root
        .get("answers")
        .and_then(Value::as_object)
        .ok_or(ResponseFault::MissingFields)?;
    let mut answers = Vec::with_capacity(questions.questions.len());
    for question in &questions.questions {
        let raw = answers_object
            .get(question.id())
            .ok_or(ResponseFault::MissingFields)?;
        answers.push(parse_answer(question, raw)?);
    }
    let (input_tokens, output_tokens) = parse_usage(root.get("usage"))?;
    Ok(ParsedResponse {
        provider_model_version,
        answers,
        input_tokens,
        output_tokens,
    })
}

fn parse_usage(usage: Option<&Value>) -> Result<(Option<u64>, Option<u64>), ResponseFault> {
    let Some(usage) = usage else {
        return Ok((None, None));
    };
    if usage.is_null() {
        return Ok((None, None));
    }
    let object = usage.as_object().ok_or(ResponseFault::Malformed)?;
    Ok((
        optional_u64(object, "input_tokens")?,
        optional_u64(object, "output_tokens")?,
    ))
}

fn optional_u64(object: &Map<String, Value>, key: &str) -> Result<Option<u64>, ResponseFault> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let number = value.as_u64().ok_or(ResponseFault::Malformed)?;
            Ok(Some(number))
        }
    }
}

fn parse_answer(question: &AtomicQuestion, raw: &Value) -> Result<JudgmentAnswer, ResponseFault> {
    let object = raw.as_object().ok_or(ResponseFault::Malformed)?;
    let primitive = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or(ResponseFault::Malformed)?;
    match question {
        AtomicQuestion::Choice { id, options, .. } => {
            if primitive != "choice" {
                return Err(ResponseFault::UnsupportedPrimitive);
            }
            let choice = object
                .get("choice")
                .and_then(Value::as_str)
                .ok_or(ResponseFault::Malformed)?;
            if !options.iter().any(|(option, _)| option == choice) {
                return Err(ResponseFault::Malformed);
            }
            let probabilities = read_probabilities(object.get("probabilities"))?;
            for (option, _) in options {
                if !probabilities.contains_key(option) {
                    return Err(ResponseFault::Malformed);
                }
            }
            Ok(JudgmentAnswer {
                question_id: id.clone(),
                primitive: PrimitiveKind::Choice,
                choice: Some(choice.to_string()),
                score: None,
                noul: None,
                probabilities,
                confidence: read_confidence(object)?,
                legend: BTreeMap::new(),
            })
        }
        AtomicQuestion::Score { id, .. } => {
            if primitive != "score" {
                return Err(ResponseFault::UnsupportedPrimitive);
            }
            let score = object
                .get("score")
                .and_then(Value::as_f64)
                .filter(|score| score.is_finite())
                .ok_or(ResponseFault::Malformed)?;
            Ok(JudgmentAnswer {
                question_id: id.clone(),
                primitive: PrimitiveKind::Score,
                choice: None,
                score: Some(score),
                noul: None,
                probabilities: read_probabilities(object.get("probabilities"))?,
                confidence: read_confidence(object)?,
                legend: read_legend(object)?,
            })
        }
        AtomicQuestion::Noul { id, .. } => {
            if primitive != "noul" {
                return Err(ResponseFault::UnsupportedPrimitive);
            }
            let noul = object
                .get("noul")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                .ok_or(ResponseFault::Malformed)?;
            Ok(JudgmentAnswer {
                question_id: id.clone(),
                primitive: PrimitiveKind::Noul,
                choice: None,
                score: None,
                noul: Some(noul),
                probabilities: BTreeMap::new(),
                confidence: None,
                legend: BTreeMap::new(),
            })
        }
    }
}

fn read_probabilities(value: Option<&Value>) -> Result<BTreeMap<String, f64>, ResponseFault> {
    let object = value
        .and_then(Value::as_object)
        .ok_or(ResponseFault::Malformed)?;
    let mut probabilities = BTreeMap::new();
    let mut sum = 0.0;
    for (key, raw) in object {
        let number = raw
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or(ResponseFault::Malformed)?;
        if !(0.0..=1.0).contains(&number) {
            return Err(ResponseFault::Malformed);
        }
        sum += number;
        probabilities.insert(key.clone(), number);
    }
    if probabilities.is_empty() || !(0.95..=1.05).contains(&sum) {
        return Err(ResponseFault::Malformed);
    }
    Ok(probabilities)
}

fn read_confidence(object: &Map<String, Value>) -> Result<Option<f64>, ResponseFault> {
    match object.get("confidence") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let number = value
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or(ResponseFault::Malformed)?;
            if !(0.0..=1.0).contains(&number) {
                return Err(ResponseFault::Malformed);
            }
            Ok(Some(number))
        }
    }
}

fn read_legend(object: &Map<String, Value>) -> Result<BTreeMap<String, String>, ResponseFault> {
    match object.get("legend") {
        None | Some(Value::Null) => Ok(BTreeMap::new()),
        Some(Value::Object(entries)) => {
            let mut legend = BTreeMap::new();
            for (key, value) in entries {
                let text = value.as_str().ok_or(ResponseFault::Malformed)?;
                legend.insert(key.clone(), text.to_string());
            }
            Ok(legend)
        }
        Some(_) => Err(ResponseFault::Malformed),
    }
}

pub struct HttpTypeSafeTransport {
    endpoint: String,
    api_key: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl HttpTypeSafeTransport {
    pub fn new(
        base_url: &str,
        api_key: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, &'static str> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err("missing credentials");
        }
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|_| "http client configuration failed")?;
        let endpoint = format!("{}{SYSTEM_ONE_PATH}", base_url.trim_end_matches('/'));
        Ok(Self {
            endpoint,
            api_key,
            timeout,
            client,
        })
    }
}

impl std::fmt::Debug for HttpTypeSafeTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpTypeSafeTransport")
            .field("endpoint", &redact_secret(&self.endpoint, &self.api_key))
            .field("timeout", &self.timeout)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[async_trait]
impl TypeSafeTransport for HttpTypeSafeTransport {
    async fn post_system_one(&self, body: Value) -> Result<RawHttpResponse, TransportError> {
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .header("accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| classify_transport_error(&error, &self.api_key))?;
        let status = response.status().as_u16();
        let request_id = response
            .headers()
            .get("x-typesafe-request-id")
            .and_then(|value| value.to_str().ok())
            .map(|value| redact_secret(value, &self.api_key));
        let body = response.text().await.map_err(|_| TransportError::Network)?;
        Ok(RawHttpResponse {
            status,
            body,
            request_id,
        })
    }

    fn redact_secret(&self) -> String {
        self.api_key.clone()
    }
}

fn classify_transport_error(error: &reqwest::Error, secret: &str) -> TransportError {
    let detail = redact_secret(&error.to_string(), secret);
    if error.is_timeout() {
        tracing::warn!(detail = %detail, "typed judgment request timed out");
        TransportError::Timeout
    } else {
        tracing::warn!(detail = %detail, "typed judgment network request failed");
        TransportError::Network
    }
}

#[cfg(test)]
pub struct ScriptedTransport {
    pub response: Mutex<Result<RawHttpResponse, TransportError>>,
    pub secret: String,
    pub calls: AtomicUsize,
    pub last_body: Mutex<Option<Value>>,
}

#[cfg(test)]
impl ScriptedTransport {
    pub fn new(
        secret: impl Into<String>,
        response: Result<RawHttpResponse, TransportError>,
    ) -> Self {
        Self {
            response: Mutex::new(response),
            secret: secret.into(),
            calls: AtomicUsize::new(0),
            last_body: Mutex::new(None),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl TypeSafeTransport for ScriptedTransport {
    async fn post_system_one(&self, body: Value) -> Result<RawHttpResponse, TransportError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut slot) = self.last_body.lock() {
            *slot = Some(body);
        }
        self.response
            .lock()
            .map_err(|_| TransportError::Network)?
            .clone()
    }

    fn redact_secret(&self) -> String {
        self.secret.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::JudgmentProvider;
    use crate::questions::{
        proposal_question_set, Q_CONTRADICTION, Q_EVIDENCE_QUALITY, Q_HUMAN_REVIEW,
        Q_PROPOSAL_SUPPORT, Q_SCOPE,
    };
    use crate::state::{prepare_case, JudgmentCase, StateLimits};
    use std::sync::Arc;

    fn sample_request() -> JudgmentRequest {
        let case = JudgmentCase {
            proposal_id: "prop_ts".into(),
            agent_id: "agent_alpha".into(),
            action_type: "PATROL".into(),
            target: [8.0, 9.0, 0.0],
            priority: 1,
            source_observation: "obs_ts".into(),
            observation_simulated: true,
            validation_feasibility: 1.0,
            validation_accepted: true,
            constraints_checked: vec!["coordinate_bounds".into()],
            contradictions: vec![],
            deterministic_confidence: 0.95,
            validation_provenance: "Validator:epistemic-v1:obs_ts".into(),
            adaptive: None,
            operator_raw: serde_json::json!({"cognitive_load": 0.2, "is_simulated": true}),
            spatial_coordinate_system: "local_sim".into(),
            within_declared_bounds: true,
            correlation_id: "corr_ts".into(),
            causation_id: "val_ts".into(),
            simulation_status: "SIMULATED".into(),
            source_event_ids: vec!["evt_1".into()],
        };
        JudgmentRequest {
            prepared: prepare_case(&case, &StateLimits::default()).unwrap(),
            proposal_id: "prop_ts".into(),
            correlation_id: "corr_ts".into(),
            causation_id: "val_ts".into(),
        }
    }

    fn success_body() -> String {
        r#"{
          "model": "jev-1.13.0",
          "answers": {
            "proposal_support": {
              "type": "choice",
              "choice": "supported",
              "probabilities": {
                "supported": 0.91,
                "mixed": 0.05,
                "unsupported": 0.02,
                "insufficient_evidence": 0.02
              },
              "confidence": 0.88
            },
            "evidence_quality": {
              "type": "score",
              "score": 2.1,
              "legend": {"0": "insufficient evidence", "1": "weak or indirect", "2": "adequate but limited", "3": "strong and directly relevant"},
              "probabilities": {"0": 0.05, "1": 0.10, "2": 0.55, "3": 0.30},
              "confidence": 0.86
            },
            "contradiction_present": {"type": "noul", "noul": 0.07, "confidence": 0.99},
            "scope_violation": {"type": "noul", "noul": 0.11},
            "human_review": {"type": "noul", "noul": 0.16}
          },
          "usage": {"input_tokens": 120, "output_tokens": 18}
        }"#
        .to_string()
    }

    #[test]
    fn parses_choice_score_and_noul_without_inventing_noul_confidence() {
        let parsed = parse_system_one_response(&success_body(), &proposal_question_set()).unwrap();
        assert_eq!(parsed.provider_model_version, "jev-1.13.0");
        assert_eq!(parsed.input_tokens, Some(120));
        let support = &parsed.answers[0];
        assert_eq!(support.question_id, Q_PROPOSAL_SUPPORT);
        assert_eq!(support.choice.as_deref(), Some("supported"));
        assert_eq!(support.probabilities.get("supported"), Some(&0.91));
        assert_eq!(support.confidence, Some(0.88));
        let quality = &parsed.answers[1];
        assert_eq!(quality.question_id, Q_EVIDENCE_QUALITY);
        assert_eq!(quality.score, Some(2.1));
        assert_eq!(quality.probabilities.get("2"), Some(&0.55));
        assert_eq!(quality.confidence, Some(0.86));
        assert_eq!(
            quality.legend.get("0").map(String::as_str),
            Some("insufficient evidence")
        );
        let contradiction = parsed
            .answers
            .iter()
            .find(|answer| answer.question_id == Q_CONTRADICTION)
            .unwrap();
        assert_eq!(contradiction.noul, Some(0.07));
        assert!(contradiction.confidence.is_none());
        assert_eq!(
            parsed
                .answers
                .iter()
                .find(|answer| answer.question_id == Q_SCOPE)
                .unwrap()
                .noul,
            Some(0.11)
        );
        assert_eq!(
            parsed
                .answers
                .iter()
                .find(|answer| answer.question_id == Q_HUMAN_REVIEW)
                .unwrap()
                .noul,
            Some(0.16)
        );
    }

    #[test]
    fn missing_choice_confidence_stays_absent() {
        let mut body: Value = serde_json::from_str(&success_body()).unwrap();
        body["answers"]["proposal_support"]
            .as_object_mut()
            .unwrap()
            .remove("confidence");
        let parsed =
            parse_system_one_response(&body.to_string(), &proposal_question_set()).unwrap();
        assert_eq!(parsed.answers[0].confidence, None);
    }

    #[test]
    fn malformed_json_and_unknown_choice_are_rejected() {
        let questions = proposal_question_set();
        assert!(parse_system_one_response("not-json", &questions).is_err());
        assert!(parse_system_one_response("{\"model\":\"jev-1.13.0\"}", &questions).is_err());
        let mut body: Value = serde_json::from_str(&success_body()).unwrap();
        body["answers"]["proposal_support"]["choice"] = Value::String("approve_anyway".into());
        assert!(parse_system_one_response(&body.to_string(), &questions).is_err());
    }

    #[tokio::test]
    async fn timeout_becomes_a_typed_provider_status() {
        let secret = "tskey_6f0c9a_DO_NOT_LOG";
        let transport = ScriptedTransport::new(secret, Err(TransportError::Timeout));
        let provider = TypeSafeJudgmentProvider::new("jev-latest", transport);
        let envelope = provider.evaluate(&sample_request()).await;
        assert_eq!(envelope.provider_status, ProviderStatus::Timeout);
        let rendered = serde_json::to_string(&envelope).unwrap();
        assert!(!rendered.contains(secret));
    }

    #[test]
    fn auth_errors_are_sanitized_and_the_api_key_is_not_logged() {
        let secret = "tskey_6f0c9a_DO_NOT_LOG";
        let raw = RawHttpResponse {
            status: 401,
            body: format!("{{\"error\":\"invalid {secret}\", \"model\":\"unknown\"}}"),
            request_id: Some(format!("req-{secret}")),
        };
        let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
        let logs_for_writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_ansi(false)
            .with_writer(move || LogWriter(logs_for_writer.clone()))
            .finish();
        let interpreted = tracing::subscriber::with_default(subscriber, || {
            interpret_http(&raw, &proposal_question_set(), secret)
        });
        assert_eq!(interpreted.status, ProviderStatus::Unauthorized);
        assert!(interpreted.answers.is_empty());
        assert_eq!(interpreted.request_id.as_deref(), Some("req-[REDACTED]"));
        let logged = String::from_utf8(logs.lock().expect("log buffer").clone()).unwrap();
        assert!(
            !logged.contains(secret),
            "api key leaked into logs: {logged}"
        );
        assert!(logged.contains("PROVIDER_UNAUTHORIZED"));
        assert!(!logged.contains("invalid tskey"));
    }

    #[test]
    fn transport_debug_and_log_line_redact_the_key() {
        let secret = "tskey_6f0c9a_DO_NOT_LOG";
        let transport =
            HttpTypeSafeTransport::new("https://api.typesafe.ai", secret, Duration::from_secs(10))
                .unwrap();
        let rendered = format!("{transport:?}");
        assert!(!rendered.contains(secret));
        assert!(rendered.contains("[REDACTED]"));
        let line = failure_log_line(
            ProviderStatus::Unauthorized,
            401,
            &redact_secret(&format!("request {secret}"), secret),
        );
        assert!(!line.contains(secret));
        assert!(line.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn request_body_contains_atomic_questions_and_not_the_key() {
        let secret = "tskey_6f0c9a_DO_NOT_LOG";
        let transport = ScriptedTransport::new(secret, Err(TransportError::Network));
        let provider = TypeSafeJudgmentProvider::new("jev-latest", transport);
        let _ = provider.evaluate(&sample_request()).await;
        let body = provider
            .transport
            .last_body
            .lock()
            .unwrap()
            .clone()
            .unwrap();
        let rendered = body.to_string();
        assert!(!rendered.contains(secret));
        assert_eq!(body["model"], "jev-latest");
        assert!(body["state"]["simulation_label"] == "SIMULATED");
        assert_eq!(body["questions"]["proposal_support"]["type"], "choice");
        assert_eq!(body["questions"]["evidence_quality"]["type"], "score");
        assert_eq!(body["questions"]["contradiction_present"]["type"], "noul");
        assert_eq!(provider.transport.calls.load(Ordering::SeqCst), 1);
    }

    struct LogWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogWriter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .map_err(|error| std::io::Error::other(error.to_string()))?
                .extend_from_slice(data);
            Ok(data.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
