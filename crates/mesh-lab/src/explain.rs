//! "Why did this happen?" Walk the causal chain of a decision.

use event_bus::ChainedEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainLink {
    pub seq: u64,
    pub event_id: String,
    pub event_type: String,
    pub tick: Option<u64>,
    pub source: String,
    pub causation_id: String,
    pub summary: String,
    pub ai_involved: bool,
    pub mode: String,
    pub policy_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Explanation {
    pub target: String,
    pub correlation_id: String,
    pub chain: Vec<ChainLink>,
    pub state_changed: bool,
    pub ai_involved: bool,
}

fn summarize(event: &event_bus::EventEnvelope) -> String {
    let p = &event.payload;
    let list = |v: &Value| {
        v.as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    };
    match event.event_type.as_str() {
        "observation" => format!(
            "sensor reading for {} observed_at={} quality={}{}",
            event.subject_id,
            p["observation"]["observed_at"],
            p["observation"]["quality"].as_str().unwrap_or("?"),
            p["fault_effect"]
                .as_str()
                .map(|f| format!(" (fault: {f})"))
                .unwrap_or_default()
        ),
        "proposal" => format!(
            "{} proposes {} to {} priority {}: \"{}\"",
            event.subject_id,
            p["proposal"]["action_type"].as_str().unwrap_or("?"),
            p["proposal"]["target"],
            p["proposal"]["priority"],
            p["rationale"].as_str().unwrap_or("")
        ),
        "validation" => {
            if p["accepted"] == true {
                format!(
                    "PASS: {} checks; observation age {} ms; deterministic confidence {}",
                    p["checks"].as_array().map(|c| c.len()).unwrap_or(0),
                    p["observation_age_ms"],
                    p["confidence"]
                )
            } else {
                format!(
                    "FAIL: {} ({})",
                    list(&p["reasons"]),
                    p["contradictions"]
                        .as_array()
                        .map(|c| c
                            .iter()
                            .filter_map(|i| i.as_str())
                            .collect::<Vec<_>>()
                            .join("; "))
                        .unwrap_or_default()
                )
            }
        }
        "judgment" => format!(
            "{} / {} -> {} [{}] status {}",
            p["provider"].as_str().unwrap_or("?"),
            p["provider_model_version"].as_str().unwrap_or("?"),
            p["disposition"].as_str().unwrap_or("?"),
            list(&p["reason_codes"]),
            p["provider_status"].as_str().unwrap_or("?")
        ),
        "commitment" => format!(
            "policy {} -> {} [{}] at operator level {}",
            p["policy_version"].as_str().unwrap_or("?"),
            p["outcome"].as_str().unwrap_or("?"),
            list(&p["reason_codes"]),
            p["adaptive_level"].as_str().unwrap_or("?")
        ),
        "state_transition" => format!(
            "{} {} revision {}: {} -> {}",
            p["mutation"].as_str().unwrap_or("?"),
            p["subject_id"].as_str().unwrap_or("?"),
            p["revision"],
            short(p["before_hash"].as_str().unwrap_or("")),
            short(p["after_hash"].as_str().unwrap_or(""))
        ),
        "human_resolution" => format!(
            "{} decided {} ({})",
            p["operator_ref"].as_str().unwrap_or("?"),
            p["decision"].as_str().unwrap_or("?"),
            p["note"].as_str().unwrap_or("")
        ),
        "human_state" => format!(
            "{} = {} {} ({}, quality {})",
            p["datum"]["metric"].as_str().unwrap_or("?"),
            p["datum"]["value"],
            p["datum"]["unit"].as_str().unwrap_or(""),
            p["datum"]["mode"].as_str().unwrap_or("?"),
            p["datum"]["quality"].as_str().unwrap_or("?")
        ),
        other => format!("{other} event"),
    }
}

fn short(hash: &str) -> String {
    hash.trim_start_matches("sha256:")
        .chars()
        .take(10)
        .collect()
}

/// Explain a proposal id, correlation id, or event id.
pub fn explain(events: &[ChainedEvent], target: &str) -> Option<Explanation> {
    let by_id: BTreeMap<&str, &ChainedEvent> = events
        .iter()
        .map(|c| (c.event.event_id.as_str(), c))
        .collect();
    let correlation = if let Some(event) = by_id.get(target) {
        event.event.correlation_id.clone()
    } else if let Some(event) = events.iter().find(|c| {
        c.event.event_type == "proposal" && c.event.payload["proposal"]["proposal_id"] == target
    }) {
        event.event.correlation_id.clone()
    } else if events.iter().any(|c| c.event.correlation_id == target) {
        target.to_string()
    } else {
        return None;
    };
    let mut selected: BTreeSet<u64> = events
        .iter()
        .filter(|c| c.event.correlation_id == correlation)
        .map(|c| c.event.seq)
        .collect();
    // Causal ancestors outside the correlation (the observation a proposal used).
    let mut frontier: Vec<String> = events
        .iter()
        .filter(|c| selected.contains(&c.event.seq))
        .map(|c| c.event.causation_id.clone())
        .collect();
    while let Some(cause) = frontier.pop() {
        if let Some(event) = by_id.get(cause.as_str()) {
            if matches!(event.event.event_type.as_str(), "run_started") {
                continue;
            }
            if selected.insert(event.event.seq) {
                frontier.push(event.event.causation_id.clone());
            }
        }
    }
    let chain: Vec<ChainLink> = events
        .iter()
        .filter(|c| selected.contains(&c.event.seq))
        .map(|c| ChainLink {
            seq: c.event.seq,
            event_id: c.event.event_id.clone(),
            event_type: c.event.event_type.clone(),
            tick: c.event.tick,
            source: c.event.source.clone(),
            causation_id: c.event.causation_id.clone(),
            summary: summarize(&c.event),
            ai_involved: c.event.ai_involved,
            mode: c.event.mode.as_str().to_string(),
            policy_version: c.event.policy_version.clone(),
        })
        .collect();
    Some(Explanation {
        target: target.to_string(),
        correlation_id: correlation,
        state_changed: chain.iter().any(|l| l.event_type == "state_transition"),
        ai_involved: chain.iter().any(|l| l.ai_involved),
        chain,
    })
}

pub fn render(explanation: &Explanation) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "WHY: {}  (correlation {})\n",
        explanation.target, explanation.correlation_id
    );
    for link in &explanation.chain {
        let _ = writeln!(
            out,
            "  t{:<4} #{:<5} {:<17} {}",
            link.tick
                .map(|t| t.to_string())
                .unwrap_or_else(|| "-".into()),
            link.seq,
            link.event_type,
            link.summary
        );
        let _ = writeln!(
            out,
            "         {:<6} caused by {:<12} source {} mode {}{}",
            "",
            link.causation_id.chars().take(12).collect::<String>(),
            link.source,
            link.mode,
            if link.ai_involved { " AI-INVOLVED" } else { "" }
        );
    }
    let _ = writeln!(
        out,
        "\n  Authoritative state changed: {}",
        if explanation.state_changed {
            "yes"
        } else {
            "no"
        }
    );
    let _ = writeln!(
        out,
        "  A probabilistic model was involved: {}",
        if explanation.ai_involved { "yes" } else { "no" }
    );
    out
}
