//! Invariants checked on every recorded run, from the event log alone.

use crate::record::DecisionRecord;
use event_bus::{chain::verify_chain, ChainedEvent, DataMode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KNOWN: &[&str] = &[
    "event_chain_intact",
    "no_commit_without_validation_pass",
    "no_judgment_after_failed_validation",
    "every_commit_has_provenance",
    "no_unsafe_commits",
    "simulated_data_labeled",
    "zero_network_calls",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InvariantResult {
    pub name: String,
    pub holds: bool,
    pub checked: u64,
    /// Up to ten examples of violations.
    pub violations: Vec<String>,
    pub description: String,
}

fn result(name: &str, description: &str, checked: u64, violations: Vec<String>) -> InvariantResult {
    InvariantResult {
        name: name.to_string(),
        holds: violations.is_empty(),
        checked,
        violations: violations.into_iter().take(10).collect(),
        description: description.to_string(),
    }
}

pub fn check_all(
    genesis: &str,
    events: &[ChainedEvent],
    decisions: &[DecisionRecord],
    network_calls: u64,
) -> Vec<InvariantResult> {
    let by_id: BTreeMap<&str, &ChainedEvent> = events
        .iter()
        .map(|chained| (chained.event.event_id.as_str(), chained))
        .collect();
    let mut out = Vec::new();

    out.push(result(
        "event_chain_intact",
        "Every event links to its predecessor and its hash matches its content.",
        events.len() as u64,
        match verify_chain(genesis, events) {
            Ok(_) => Vec::new(),
            Err(error) => vec![error.to_string()],
        },
    ));

    let commits: Vec<&ChainedEvent> = events
        .iter()
        .filter(|c| {
            c.event.event_type == "state_transition" && c.event.payload["mutation"] == "commit"
        })
        .collect();

    let mut violations = Vec::new();
    for commit in &commits {
        let proposal_id = commit.event.payload["proposal_id"]
            .as_str()
            .unwrap_or_default();
        let authorized = commit.event.payload["authorized_by"]
            .as_array()
            .map(|ids| {
                ids.iter().filter_map(|id| id.as_str()).any(|id| {
                    by_id.get(id).is_some_and(|validation| {
                        validation.event.event_type == "validation"
                            && validation.event.payload["accepted"] == true
                            && validation.event.payload["proposal_id"] == proposal_id
                    })
                })
            })
            .unwrap_or(false);
        if !authorized {
            violations.push(format!(
                "{} commits {proposal_id} without a passing validation",
                commit.event.event_id
            ));
        }
    }
    out.push(result(
        "no_commit_without_validation_pass",
        "Every committed state transition names a passing validation of the same proposal.",
        commits.len() as u64,
        violations,
    ));

    let judgments: Vec<&ChainedEvent> = events
        .iter()
        .filter(|c| c.event.event_type == "judgment")
        .collect();
    let violations = judgments
        .iter()
        .filter(|judgment| {
            !by_id
                .get(judgment.event.causation_id.as_str())
                .is_some_and(|cause| {
                    cause.event.event_type == "validation"
                        && cause.event.payload["accepted"] == true
                })
        })
        .map(|judgment| {
            format!(
                "{} was not caused by a passing validation",
                judgment.event.event_id
            )
        })
        .collect();
    out.push(result(
        "no_judgment_after_failed_validation",
        "Judgment is only ever consulted after deterministic validation passed.",
        judgments.len() as u64,
        violations,
    ));

    let mut violations = Vec::new();
    for commit in &commits {
        let mut cursor = commit.event.causation_id.as_str();
        let mut reached = false;
        for _ in 0..64 {
            match by_id.get(cursor) {
                Some(event) if event.event.event_type == "proposal" => {
                    reached = true;
                    break;
                }
                Some(event) => cursor = event.event.causation_id.as_str(),
                None => break,
            }
        }
        if !reached {
            violations.push(format!(
                "{} has no causal chain back to a proposal",
                commit.event.event_id
            ));
        }
    }
    out.push(result(
        "every_commit_has_provenance",
        "Following causation_id from every commit reaches the proposal event that started it.",
        commits.len() as u64,
        violations,
    ));

    let violations = decisions
        .iter()
        .filter(|decision| decision.committed && decision.oracle_unsafe.is_some())
        .map(|decision| {
            format!(
                "{} committed although the oracle found it unsafe ({})",
                decision.proposal_id,
                decision.oracle_unsafe.as_deref().unwrap_or("")
            )
        })
        .collect();
    out.push(result(
        "no_unsafe_commits",
        "No committed proposal is unsafe according to the independent geometric oracle.",
        decisions.iter().filter(|d| d.committed).count() as u64,
        violations,
    ));

    let violations = events
        .iter()
        .filter(|c| {
            // A person's own commands and review decisions are live by nature;
            // nothing else in a simulated run may claim to be.
            let human_action = matches!(
                c.event.event_type.as_str(),
                "operator_command" | "human_resolution"
            );
            (c.event.mode == DataMode::Live && !human_action)
                || (c.event.event_type == "human_state"
                    && c.event.payload["datum"]["mode"] != c.event.mode.as_str())
        })
        .map(|c| {
            format!(
                "{} ({}) is labeled {}",
                c.event.event_id,
                c.event.event_type,
                c.event.mode.as_str()
            )
        })
        .collect();
    out.push(result(
        "simulated_data_labeled",
        "Only a person's own commands and review decisions may be labeled LIVE, and every human-state datum carries the mode of its event.",
        events.len() as u64,
        violations,
    ));

    out.push(result(
        "zero_network_calls",
        "No judgment request reached a networked provider.",
        judgments.len() as u64,
        if network_calls == 0 {
            Vec::new()
        } else {
            vec![format!("{network_calls} networked judgment calls")]
        },
    ));
    out
}
