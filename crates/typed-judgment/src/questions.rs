use serde::Serialize;
use serde_json::{Map, Value};

pub const QUESTION_SET_VERSION: &str = "pordenone.judgment.questions.v1";

pub const Q_PROPOSAL_SUPPORT: &str = "proposal_support";
pub const Q_EVIDENCE_QUALITY: &str = "evidence_quality";
pub const Q_CONTRADICTION: &str = "contradiction_present";
pub const Q_SCOPE: &str = "scope_violation";
pub const Q_HUMAN_REVIEW: &str = "human_review";

pub const SUPPORT_SUPPORTED: &str = "supported";
pub const SUPPORT_MIXED: &str = "mixed";
pub const SUPPORT_UNSUPPORTED: &str = "unsupported";
pub const SUPPORT_INSUFFICIENT: &str = "insufficient_evidence";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionSet {
    pub version: String,
    pub questions: Vec<AtomicQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomicQuestion {
    Choice {
        id: String,
        instructions: String,
        options: Vec<(String, String)>,
    },
    Score {
        id: String,
        instructions: String,
        levels: Vec<String>,
    },
    Noul {
        id: String,
        instructions: String,
        yes: String,
        no: String,
    },
}

impl AtomicQuestion {
    pub fn id(&self) -> &str {
        match self {
            Self::Choice { id, .. } | Self::Score { id, .. } | Self::Noul { id, .. } => id,
        }
    }

    pub fn instructions(&self) -> &str {
        match self {
            Self::Choice { instructions, .. }
            | Self::Score { instructions, .. }
            | Self::Noul { instructions, .. } => instructions,
        }
    }

    pub fn primitive_name(&self) -> &'static str {
        match self {
            Self::Choice { .. } => "choice",
            Self::Score { .. } => "score",
            Self::Noul { .. } => "noul",
        }
    }
}

/// Versioned atomic questions for one proposal-evidence package.
///
/// Questions are independent, evaluate the same state, and do not instruct
/// the model to choose a system disposition or mutate state.
pub fn proposal_question_set() -> QuestionSet {
    QuestionSet {
        version: QUESTION_SET_VERSION.to_string(),
        questions: vec![
            AtomicQuestion::Choice {
                id: Q_PROPOSAL_SUPPORT.to_string(),
                instructions: "Given only the supplied evidence, how well is this proposal supported?".to_string(),
                options: vec![
                    (
                        SUPPORT_SUPPORTED.to_string(),
                        "The supplied evidence directly supports the proposal.".to_string(),
                    ),
                    (
                        SUPPORT_MIXED.to_string(),
                        "The supplied evidence partly supports and partly does not support the proposal.".to_string(),
                    ),
                    (
                        SUPPORT_UNSUPPORTED.to_string(),
                        "The supplied evidence does not support the proposal.".to_string(),
                    ),
                    (
                        SUPPORT_INSUFFICIENT.to_string(),
                        "The supplied evidence is not sufficient to judge support.".to_string(),
                    ),
                ],
            },
            AtomicQuestion::Score {
                id: Q_EVIDENCE_QUALITY.to_string(),
                instructions: "How strong is the supplied evidence for evaluating this proposal?".to_string(),
                levels: vec![
                    "insufficient evidence".to_string(),
                    "weak or indirect".to_string(),
                    "adequate but limited".to_string(),
                    "strong and directly relevant".to_string(),
                ],
            },
            AtomicQuestion::Noul {
                id: Q_CONTRADICTION.to_string(),
                instructions: "The supplied evidence meaningfully contradicts the proposal.".to_string(),
                yes: "The evidence meaningfully contradicts the proposal.".to_string(),
                no: "The evidence does not meaningfully contradict the proposal.".to_string(),
            },
            AtomicQuestion::Noul {
                id: Q_SCOPE.to_string(),
                instructions: "The proposal goes materially beyond what the supplied observations establish.".to_string(),
                yes: "The proposal goes materially beyond the supplied observations.".to_string(),
                no: "The proposal stays within what the supplied observations establish.".to_string(),
            },
            AtomicQuestion::Noul {
                id: Q_HUMAN_REVIEW.to_string(),
                instructions: "The ambiguity, uncertainty, or potential consequence warrants explicit human review before state commitment.".to_string(),
                yes: "The ambiguity, uncertainty, or potential consequence warrants explicit human review.".to_string(),
                no: "The case does not warrant explicit human review on those grounds.".to_string(),
            },
        ],
    }
}

/// Provider-neutral description of the question set. This is not a wire payload.
#[derive(Debug, Serialize, PartialEq)]
pub struct QuestionSetDocument {
    pub version: String,
    pub questions: Vec<QuestionDocument>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct QuestionDocument {
    pub id: String,
    pub primitive: String,
    pub instructions: String,
}

impl QuestionSet {
    pub fn document(&self) -> QuestionSetDocument {
        QuestionSetDocument {
            version: self.version.clone(),
            questions: self
                .questions
                .iter()
                .map(|question| QuestionDocument {
                    id: question.id().to_string(),
                    primitive: question.primitive_name().to_string(),
                    instructions: question.instructions().to_string(),
                })
                .collect(),
        }
    }
}

/// TypeSafe System One wire shape. Only the TypeSafe provider may send this.
pub fn typesafe_questions_wire(question_set: &QuestionSet) -> Map<String, Value> {
    let mut questions = Map::new();
    for question in &question_set.questions {
        let value = match question {
            AtomicQuestion::Choice {
                instructions,
                options,
                ..
            } => {
                let mut criteria = Map::new();
                for (option, description) in options {
                    criteria.insert(option.clone(), Value::String(description.clone()));
                }
                serde_json::json!({
                    "type": "choice",
                    "instructions": instructions,
                    "criteria": criteria,
                })
            }
            AtomicQuestion::Score {
                instructions,
                levels,
                ..
            } => serde_json::json!({
                "type": "score",
                "instructions": instructions,
                "criteria": levels,
            }),
            AtomicQuestion::Noul {
                instructions,
                yes,
                no,
                ..
            } => serde_json::json!({
                "type": "noul",
                "instructions": instructions,
                "criteria": { "true": yes, "false": no },
            }),
        };
        questions.insert(question.id().to_string(), value);
    }
    questions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_question_set_is_independent_and_versioned() {
        let set = proposal_question_set();
        assert_eq!(set.version, QUESTION_SET_VERSION);
        assert_eq!(set.questions.len(), 5);

        let ids: Vec<&str> = set.questions.iter().map(AtomicQuestion::id).collect();
        assert_eq!(
            ids,
            vec![
                Q_PROPOSAL_SUPPORT,
                Q_EVIDENCE_QUALITY,
                Q_CONTRADICTION,
                Q_SCOPE,
                Q_HUMAN_REVIEW
            ]
        );

        for question in &set.questions {
            let instructions = question.instructions();
            assert!(!instructions.is_empty());
            assert!(!instructions
                .to_ascii_lowercase()
                .contains("allow this action"));
            assert!(!instructions
                .to_ascii_lowercase()
                .contains("you decide the policy"));
            for other in &set.questions {
                if other.id() != question.id() {
                    assert!(
                        !instructions.contains(other.id()),
                        "{} instructions reference {}",
                        question.id(),
                        other.id()
                    );
                }
            }
        }

        let document = set.document();
        assert_eq!(document.questions[0].primitive, "choice");
        assert_eq!(document.questions[1].primitive, "score");
        assert_eq!(document.questions[2].primitive, "noul");
    }

    #[test]
    fn typesafe_wire_matches_current_system_one_shape() {
        let wire = typesafe_questions_wire(&proposal_question_set());
        assert_eq!(wire.len(), 5);

        let support = wire.get(Q_PROPOSAL_SUPPORT).unwrap();
        assert_eq!(support["type"], "choice");
        assert_eq!(
            support["instructions"],
            "Given only the supplied evidence, how well is this proposal supported?"
        );
        assert!(support["criteria"]["supported"].is_string());
        assert!(support["criteria"]["insufficient_evidence"].is_string());

        let quality = wire.get(Q_EVIDENCE_QUALITY).unwrap();
        assert_eq!(quality["type"], "score");
        let levels = quality["criteria"].as_array().unwrap();
        assert_eq!(levels.len(), 4);
        assert_eq!(levels[0], "insufficient evidence");
        assert_eq!(levels[3], "strong and directly relevant");

        let contradiction = wire.get(Q_CONTRADICTION).unwrap();
        assert_eq!(contradiction["type"], "noul");
        assert_eq!(
            contradiction["instructions"],
            "The supplied evidence meaningfully contradicts the proposal."
        );
        assert!(contradiction["criteria"]["true"].is_string());
        assert!(contradiction.get("confidence").is_none());
    }
}
