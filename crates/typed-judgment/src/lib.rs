//! Provider-neutral typed judgment for Pordenone.
//!
//! Deterministic validation stays in `epistemic-validator`. This crate evaluates
//! bounded questions about evidence that already passed those checks. A provider
//! returns typed answers. Pordenone policy decides what those answers may do.
//! Providers receive an owned evidence package and no write handle.

mod compare;
mod config;
mod envelope;
mod policy;
mod provider;
mod questions;
mod state;
mod typesafe;

pub use compare::{
    compare_envelopes, compare_reevaluation, mark_live_reevaluation, ProviderComparison,
    QuestionObservation, ReevaluationComparison,
};
pub use config::{
    build_provider, startup_log_line, JudgmentSettings, ProviderSelection, API_KEY_ENV,
    DEFAULT_BASE_URL, DEFAULT_MODEL,
};
pub use envelope::{
    now_ms, Disposition, EvaluationMode, JudgmentAnswer, JudgmentEnvelope, PrimitiveKind,
    ProviderStatus, ENVELOPE_SCHEMA_VERSION,
};
pub use policy::{JudgmentPolicy, POLICY_VERSION};
pub use provider::{
    commit_allowed, draft_envelope, local_failure_envelope, scripted_answers,
    DeterministicMockJudgmentProvider, DisabledJudgmentProvider, JudgmentProvider, JudgmentRequest,
    MockScenario, RecordedJudgmentProvider, StaticStatusProvider,
};
pub use questions::{
    proposal_question_set, typesafe_questions_wire, AtomicQuestion, QuestionSet,
    QUESTION_SET_VERSION,
};
pub use state::{
    prepare_case, AdaptiveSnapshot, JudgmentCase, PrepareError, PreparedJudgment, StateLimits,
    STATE_SCHEMA_VERSION,
};
pub use typesafe::{
    failure_log_line, redact_secret, HttpTypeSafeTransport, TypeSafeJudgmentProvider,
};

#[cfg(test)]
mod schema_fixture {
    use super::*;

    #[test]
    fn shared_fixture_round_trips() {
        let raw = include_str!("../../../schemas/fixtures/judgment-envelope.sample.json");
        let envelope: JudgmentEnvelope = serde_json::from_str(raw).expect("fixture parses");
        assert_eq!(envelope.schema_version, ENVELOPE_SCHEMA_VERSION);
        assert_eq!(envelope.disposition, Disposition::Pass);
        assert_eq!(envelope.provider, "typesafe");
        assert_eq!(envelope.provider_model_version, "jev-1.13.0");
        assert_eq!(envelope.answers.len(), 5);
        let noul = envelope
            .answers
            .iter()
            .find(|answer| answer.question_id == "contradiction_present")
            .unwrap();
        assert!(noul.confidence.is_none());
        assert_eq!(noul.noul, Some(0.07));
        let rendered = serde_json::to_string(&envelope).unwrap();
        let again: JudgmentEnvelope = serde_json::from_str(&rendered).unwrap();
        assert_eq!(again.disposition, envelope.disposition);
        assert_eq!(again.state_hash, envelope.state_hash);
    }
}
