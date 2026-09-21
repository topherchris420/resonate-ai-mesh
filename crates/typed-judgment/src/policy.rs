use crate::envelope::{
    Disposition, JudgmentAnswer, JudgmentEnvelope, PrimitiveKind, ProviderStatus,
};
use crate::questions::{
    Q_CONTRADICTION, Q_EVIDENCE_QUALITY, Q_HUMAN_REVIEW, Q_PROPOSAL_SUPPORT, Q_SCOPE,
    SUPPORT_INSUFFICIENT, SUPPORT_MIXED, SUPPORT_SUPPORTED, SUPPORT_UNSUPPORTED,
};

pub const POLICY_VERSION: &str = "pordenone.judgment.policy.v1";

#[derive(Debug, Clone, PartialEq)]
pub struct JudgmentPolicy {
    pub version: String,
    pub minimum_confidence: f64,
    pub minimum_evidence_score: f64,
    /// Noul values strictly above this threshold revise the proposal.
    pub scope_violation_threshold: f64,
    /// Contradiction probability strictly above this threshold revises.
    pub contradiction_revise_threshold: f64,
    /// Contradiction probability strictly above this threshold requests human review.
    pub contradiction_review_threshold: f64,
    /// Human-review probability strictly above this threshold requests human review.
    pub human_review_threshold: f64,
}

impl Default for JudgmentPolicy {
    fn default() -> Self {
        Self {
            version: POLICY_VERSION.to_string(),
            minimum_confidence: 0.70,
            minimum_evidence_score: 2.0,
            scope_violation_threshold: 0.50,
            contradiction_revise_threshold: 0.50,
            contradiction_review_threshold: 0.75,
            human_review_threshold: 0.50,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Severity {
    Pass,
    Revise,
    HumanReview,
}

impl JudgmentPolicy {
    /// Deterministic interpretation of a typed judgment. The model does not select the disposition.
    pub fn apply(&self, envelope: &mut JudgmentEnvelope) {
        let (disposition, reasons) = self.evaluate(envelope);
        envelope.disposition = disposition;
        envelope.reason_codes = reasons;
        envelope.policy_version = self.version.clone();
    }

    pub fn evaluate(&self, envelope: &JudgmentEnvelope) -> (Disposition, Vec<String>) {
        if envelope.provider_status == ProviderStatus::Disabled {
            return (Disposition::Skipped, vec!["JUDGMENT_DISABLED".to_string()]);
        }
        if envelope.provider_status != ProviderStatus::Ok {
            return (
                Disposition::Unavailable,
                vec![envelope.provider_status.reason_code().to_string()],
            );
        }

        let mut severity = Severity::Pass;
        let mut reasons = Vec::new();

        let support = match require_choice(
            &envelope.answers,
            Q_PROPOSAL_SUPPORT,
            &[
                SUPPORT_SUPPORTED,
                SUPPORT_MIXED,
                SUPPORT_UNSUPPORTED,
                SUPPORT_INSUFFICIENT,
            ],
        ) {
            Ok(answer) => answer,
            Err(code) => return unavailable(code),
        };
        let evidence = match require_score(&envelope.answers, Q_EVIDENCE_QUALITY) {
            Ok(answer) => answer,
            Err(code) => return unavailable(code),
        };
        let contradiction = match require_noul(&envelope.answers, Q_CONTRADICTION) {
            Ok(value) => value,
            Err(code) => return unavailable(code),
        };
        let scope = match require_noul(&envelope.answers, Q_SCOPE) {
            Ok(value) => value,
            Err(code) => return unavailable(code),
        };
        let human = match require_noul(&envelope.answers, Q_HUMAN_REVIEW) {
            Ok(value) => value,
            Err(code) => return unavailable(code),
        };

        match support.choice.as_deref() {
            Some(SUPPORT_UNSUPPORTED) => {
                raise(&mut severity, Severity::Revise);
                reasons.push("SUPPORT_UNSUPPORTED".to_string());
            }
            Some(SUPPORT_INSUFFICIENT) => {
                raise(&mut severity, Severity::Revise);
                reasons.push("SUPPORT_INSUFFICIENT_EVIDENCE".to_string());
            }
            Some(SUPPORT_MIXED) => {
                raise(&mut severity, Severity::Revise);
                reasons.push("SUPPORT_MIXED".to_string());
            }
            Some(SUPPORT_SUPPORTED) => {}
            _ => return unavailable("MALFORMED_RESPONSE"),
        }

        if evidence.score.unwrap_or(0.0) < self.minimum_evidence_score {
            raise(&mut severity, Severity::Revise);
            reasons.push("EVIDENCE_BELOW_MINIMUM".to_string());
        }

        gate_confidence(support, self, &mut severity, &mut reasons);
        gate_confidence(evidence, self, &mut severity, &mut reasons);

        if scope > self.scope_violation_threshold {
            raise(&mut severity, Severity::Revise);
            reasons.push("SCOPE_VIOLATION".to_string());
        }
        if contradiction > self.contradiction_review_threshold {
            raise(&mut severity, Severity::HumanReview);
            reasons.push("CONTRADICTION_PRESENT".to_string());
        } else if contradiction > self.contradiction_revise_threshold {
            raise(&mut severity, Severity::Revise);
            reasons.push("CONTRADICTION_PRESENT".to_string());
        }
        if human > self.human_review_threshold {
            raise(&mut severity, Severity::HumanReview);
            reasons.push("HUMAN_REVIEW_INDICATED".to_string());
        }

        let disposition = match severity {
            Severity::Pass => {
                reasons.push("ELIGIBLE_PASS".to_string());
                Disposition::Pass
            }
            Severity::Revise => Disposition::Revise,
            Severity::HumanReview => Disposition::HumanReview,
        };
        (disposition, reasons)
    }
}

fn unavailable(code: &str) -> (Disposition, Vec<String>) {
    (Disposition::Unavailable, vec![code.to_string()])
}

fn raise(current: &mut Severity, next: Severity) {
    if next > *current {
        *current = next;
    }
}

fn gate_confidence(
    answer: &JudgmentAnswer,
    policy: &JudgmentPolicy,
    severity: &mut Severity,
    reasons: &mut Vec<String>,
) {
    match answer.confidence {
        None => {
            raise(severity, Severity::HumanReview);
            push_unique(reasons, "MISSING_CONFIDENCE");
        }
        Some(confidence) if confidence < policy.minimum_confidence => {
            raise(severity, Severity::HumanReview);
            push_unique(reasons, "LOW_CONFIDENCE");
        }
        Some(_) => {}
    }
}

fn push_unique(reasons: &mut Vec<String>, code: &str) {
    if !reasons.iter().any(|reason| reason == code) {
        reasons.push(code.to_string());
    }
}

fn find_answer<'a>(
    answers: &'a [JudgmentAnswer],
    id: &str,
) -> Result<&'a JudgmentAnswer, &'static str> {
    answers
        .iter()
        .find(|answer| answer.question_id == id)
        .ok_or("MISSING_FIELDS")
}

fn require_choice<'a>(
    answers: &'a [JudgmentAnswer],
    id: &str,
    options: &[&str],
) -> Result<&'a JudgmentAnswer, &'static str> {
    let answer = find_answer(answers, id)?;
    if answer.primitive != PrimitiveKind::Choice {
        return Err("UNSUPPORTED_PRIMITIVE");
    }
    let choice = answer.choice.as_deref().ok_or("MALFORMED_RESPONSE")?;
    if !options.contains(&choice) {
        return Err("MALFORMED_RESPONSE");
    }
    Ok(answer)
}

fn require_score<'a>(
    answers: &'a [JudgmentAnswer],
    id: &str,
) -> Result<&'a JudgmentAnswer, &'static str> {
    let answer = find_answer(answers, id)?;
    if answer.primitive != PrimitiveKind::Score {
        return Err("UNSUPPORTED_PRIMITIVE");
    }
    match answer.score {
        Some(score) if score.is_finite() => Ok(answer),
        _ => Err("MALFORMED_RESPONSE"),
    }
}

fn require_noul(answers: &[JudgmentAnswer], id: &str) -> Result<f64, &'static str> {
    let answer = find_answer(answers, id)?;
    if answer.primitive != PrimitiveKind::Noul {
        return Err("UNSUPPORTED_PRIMITIVE");
    }
    match answer.noul {
        Some(value) if (0.0..=1.0).contains(&value) => Ok(value),
        _ => Err("MALFORMED_RESPONSE"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::scripted_answers;
    use crate::provider::MockScenario;
    use crate::questions::QUESTION_SET_VERSION;
    use crate::state::STATE_SCHEMA_VERSION;

    fn envelope(scenario: MockScenario) -> JudgmentEnvelope {
        let (status, answers) = scripted_answers(scenario);
        JudgmentEnvelope {
            schema_version: crate::envelope::ENVELOPE_SCHEMA_VERSION.into(),
            judgment_id: "jdg_test".into(),
            provider: "deterministic_mock".into(),
            model: "deterministic-mock".into(),
            provider_model_version: "deterministic-mock".into(),
            question_set_version: QUESTION_SET_VERSION.into(),
            state_hash: "sha256:test".into(),
            state_schema_version: STATE_SCHEMA_VERSION.into(),
            truncated: false,
            proposal_id: "prop_1".into(),
            correlation_id: "corr_1".into(),
            causation_id: "val_1".into(),
            answers,
            disposition: Disposition::Pending,
            reason_codes: vec!["SHOULD_BE_REPLACED".into()],
            policy_version: "unset".into(),
            requested_at: 1,
            completed_at: 2,
            latency_ms: 3,
            provider_status: status,
            evaluation_mode: crate::envelope::EvaluationMode::Live,
            simulation_label: "SIMULATED".into(),
            provider_request_id: None,
            input_tokens: None,
            output_tokens: None,
        }
    }

    #[test]
    fn pass_only_when_support_and_confidence_clear_the_gates() {
        let policy = JudgmentPolicy::default();
        let mut env = envelope(MockScenario::Supported);
        policy.apply(&mut env);
        assert_eq!(env.disposition, Disposition::Pass);
        assert_eq!(env.reason_codes, vec!["ELIGIBLE_PASS"]);
        assert_eq!(env.policy_version, POLICY_VERSION);
    }

    #[test]
    fn unsupported_revises() {
        let policy = JudgmentPolicy::default();
        let (disposition, reasons) = policy.evaluate(&envelope(MockScenario::Unsupported));
        assert_eq!(disposition, Disposition::Revise);
        assert_eq!(reasons, vec!["SUPPORT_UNSUPPORTED"]);
    }

    #[test]
    fn insufficient_evidence_and_mixed_revise() {
        let policy = JudgmentPolicy::default();
        let (insufficient, _) = policy.evaluate(&envelope(MockScenario::InsufficientEvidence));
        let (mixed, mixed_reasons) = policy.evaluate(&envelope(MockScenario::Mixed));
        assert_eq!(insufficient, Disposition::Revise);
        assert_eq!(mixed, Disposition::Revise);
        assert_eq!(mixed_reasons, vec!["SUPPORT_MIXED"]);
    }

    #[test]
    fn scope_violation_revises() {
        let policy = JudgmentPolicy::default();
        let (disposition, reasons) = policy.evaluate(&envelope(MockScenario::ScopeViolation));
        assert_eq!(disposition, Disposition::Revise);
        assert_eq!(reasons, vec!["SCOPE_VIOLATION"]);
    }

    #[test]
    fn contradiction_and_human_review_do_not_look_like_errors() {
        let policy = JudgmentPolicy::default();
        let (contradiction, contradiction_reasons) =
            policy.evaluate(&envelope(MockScenario::Contradiction));
        let (human, human_reasons) = policy.evaluate(&envelope(MockScenario::HumanReview));
        assert_eq!(contradiction, Disposition::HumanReview);
        assert_eq!(contradiction_reasons, vec!["CONTRADICTION_PRESENT"]);
        assert_eq!(human, Disposition::HumanReview);
        assert_eq!(human_reasons, vec!["HUMAN_REVIEW_INDICATED"]);
    }

    #[test]
    fn low_and_missing_confidence_route_to_human_review() {
        let policy = JudgmentPolicy::default();
        let (low, low_reasons) = policy.evaluate(&envelope(MockScenario::LowConfidence));
        let (missing, missing_reasons) =
            policy.evaluate(&envelope(MockScenario::MissingConfidence));
        assert_eq!(low, Disposition::HumanReview);
        assert_eq!(low_reasons, vec!["LOW_CONFIDENCE"]);
        assert_eq!(missing, Disposition::HumanReview);
        assert_eq!(missing_reasons, vec!["MISSING_CONFIDENCE"]);
    }

    #[test]
    fn provider_failures_are_unavailable_and_never_pass() {
        let policy = JudgmentPolicy::default();
        for scenario in [
            MockScenario::Timeout,
            MockScenario::Malformed,
            MockScenario::Unauthorized,
        ] {
            let (disposition, reasons) = policy.evaluate(&envelope(scenario));
            assert_eq!(disposition, Disposition::Unavailable);
            assert!(!reasons.iter().any(|reason| reason == "ELIGIBLE_PASS"));
        }
        let (timeout, reasons) = policy.evaluate(&envelope(MockScenario::Timeout));
        assert_eq!(timeout, Disposition::Unavailable);
        assert_eq!(reasons, vec!["PROVIDER_TIMEOUT"]);
    }

    #[test]
    fn disabled_skips_without_pretending_the_model_passed() {
        let policy = JudgmentPolicy::default();
        let (disposition, reasons) = policy.evaluate(&envelope(MockScenario::Disabled));
        assert_eq!(disposition, Disposition::Skipped);
        assert_eq!(reasons, vec!["JUDGMENT_DISABLED"]);
    }

    #[test]
    fn stricter_disposition_wins_when_rules_overlap() {
        let policy = JudgmentPolicy::default();
        let (disposition, reasons) =
            policy.evaluate(&envelope(MockScenario::UnsupportedLowConfidence));
        assert_eq!(disposition, Disposition::HumanReview);
        assert!(reasons.contains(&"SUPPORT_UNSUPPORTED".to_string()));
        assert!(reasons.contains(&"LOW_CONFIDENCE".to_string()));
    }

    #[test]
    fn thresholds_are_strictly_above() {
        let policy = JudgmentPolicy::default();
        let mut at = envelope(MockScenario::Supported);
        set_noul(&mut at, Q_HUMAN_REVIEW, 0.50);
        set_noul(&mut at, Q_SCOPE, 0.50);
        set_noul(&mut at, Q_CONTRADICTION, 0.50);
        assert_eq!(policy.evaluate(&at).0, Disposition::Pass);

        set_noul(&mut at, Q_SCOPE, 0.51);
        assert_eq!(policy.evaluate(&at).0, Disposition::Revise);
    }

    fn set_noul(envelope: &mut JudgmentEnvelope, id: &str, value: f64) {
        let answer = envelope
            .answers
            .iter_mut()
            .find(|answer| answer.question_id == id)
            .unwrap();
        answer.noul = Some(value);
        answer.confidence = None;
    }
}
