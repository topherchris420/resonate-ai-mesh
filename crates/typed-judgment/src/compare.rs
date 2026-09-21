use crate::envelope::{EvaluationMode, JudgmentEnvelope};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionObservation {
    pub question_id: String,
    pub agreement: bool,
    pub left: String,
    pub right: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderComparison {
    pub left_provider: String,
    pub right_provider: String,
    pub agreement: bool,
    pub questions: Vec<QuestionObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReevaluationComparison {
    pub recorded_judgment_id: String,
    pub reevaluation_judgment_id: String,
    pub drift_detected: bool,
    pub comparison: ProviderComparison,
}

/// Compare two providers on the same questions. Disagreement is preserved, never averaged.
pub fn compare_envelopes(left: &JudgmentEnvelope, right: &JudgmentEnvelope) -> ProviderComparison {
    let mut questions = Vec::new();
    let ids = question_ids(left, right);
    for id in ids {
        let left_summary = summarize(left, &id);
        let right_summary = summarize(right, &id);
        questions.push(QuestionObservation {
            question_id: id,
            agreement: left_summary == right_summary,
            left: left_summary,
            right: right_summary,
        });
    }
    let agreement = questions.iter().all(|question| question.agreement);
    ProviderComparison {
        left_provider: left.provider.clone(),
        right_provider: right.provider.clone(),
        agreement,
        questions,
    }
}

/// Compare a stored judgment with a later live reevaluation without mutating either envelope.
pub fn compare_reevaluation(
    recorded: &JudgmentEnvelope,
    live: &JudgmentEnvelope,
) -> ReevaluationComparison {
    let comparison = compare_envelopes(recorded, live);
    ReevaluationComparison {
        recorded_judgment_id: recorded.judgment_id.clone(),
        reevaluation_judgment_id: live.judgment_id.clone(),
        drift_detected: !comparison.agreement,
        comparison,
    }
}

pub fn mark_live_reevaluation(envelope: &mut JudgmentEnvelope) {
    envelope.evaluation_mode = EvaluationMode::LiveReevaluation;
}

fn question_ids(left: &JudgmentEnvelope, right: &JudgmentEnvelope) -> Vec<String> {
    let mut ids = Vec::new();
    for answer in left.answers.iter().chain(right.answers.iter()) {
        if !ids.iter().any(|id: &String| id == &answer.question_id) {
            ids.push(answer.question_id.clone());
        }
    }
    ids
}

fn summarize(envelope: &JudgmentEnvelope, question_id: &str) -> String {
    let Some(answer) = envelope
        .answers
        .iter()
        .find(|answer| answer.question_id == question_id)
    else {
        return "missing".to_string();
    };
    if let Some(choice) = &answer.choice {
        return format!("choice:{choice}");
    }
    if let Some(score) = answer.score {
        return format!("score:{score:.2}");
    }
    if let Some(noul) = answer.noul {
        return format!("noul:{noul:.2}");
    }
    "empty".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{Disposition, EvaluationMode, ProviderStatus};
    use crate::provider::{draft_envelope, scripted_answers, JudgmentRequest, MockScenario};
    use crate::state::{prepare_case, JudgmentCase, StateLimits};

    fn request() -> JudgmentRequest {
        let case = JudgmentCase {
            proposal_id: "prop_cmp".into(),
            agent_id: "agent_alpha".into(),
            action_type: "INSPECT".into(),
            target: [3.0, 4.0, 0.0],
            priority: 1,
            source_observation: "obs_cmp".into(),
            observation_simulated: true,
            validation_feasibility: 1.0,
            validation_accepted: true,
            constraints_checked: vec!["coordinate_bounds".into()],
            contradictions: vec![],
            deterministic_confidence: 0.95,
            validation_provenance: "Validator:epistemic-v1:obs_cmp".into(),
            adaptive: None,
            operator_raw: serde_json::Value::Null,
            spatial_coordinate_system: "local_sim".into(),
            within_declared_bounds: true,
            correlation_id: "corr_cmp".into(),
            causation_id: "prop_cmp".into(),
            simulation_status: "SIMULATED".into(),
            source_event_ids: vec![],
        };
        JudgmentRequest {
            prepared: prepare_case(&case, &StateLimits::default()).unwrap(),
            proposal_id: "prop_cmp".into(),
            correlation_id: "corr_cmp".into(),
            causation_id: "prop_cmp".into(),
        }
    }

    #[test]
    fn disagreement_is_recorded_and_original_is_unchanged() {
        let req = request();
        let mut recorded = draft_envelope(
            &req,
            "typesafe",
            "jev-1.x",
            ProviderStatus::Ok,
            scripted_answers(MockScenario::Supported).1,
        );
        recorded.judgment_id = "recorded-1".into();
        recorded.provider_model_version = "jev-1.x".into();
        recorded.disposition = Disposition::Pass;
        recorded.evaluation_mode = EvaluationMode::Live;
        let mut live = draft_envelope(
            &req,
            "typesafe",
            "jev-1.y",
            ProviderStatus::Ok,
            scripted_answers(MockScenario::Mixed).1,
        );
        live.judgment_id = "live-2".into();
        live.provider_model_version = "jev-1.y".into();
        mark_live_reevaluation(&mut live);

        let before = recorded.clone();
        let comparison = compare_reevaluation(&recorded, &live);
        assert!(comparison.drift_detected);
        assert!(!comparison.comparison.agreement);
        let support = comparison
            .comparison
            .questions
            .iter()
            .find(|question| question.question_id == "proposal_support")
            .unwrap();
        assert_eq!(support.left, "choice:supported");
        assert_eq!(support.right, "choice:mixed");
        assert!(!support.agreement);
        assert_eq!(recorded, before);
        assert_eq!(live.evaluation_mode, EvaluationMode::LiveReevaluation);
        assert_eq!(recorded.evaluation_mode, EvaluationMode::Live);
    }
}
