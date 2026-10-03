//! Hash-chained event log.
//!
//! Each recorded event stores the hash of its predecessor. The chain starts at
//! a genesis hash (for experiment runs, the hash of the resolved manifest), so
//! a log is bound to the configuration that produced it. Editing, inserting,
//! removing, or reordering any event breaks every later hash.

use crate::canonical::{canonical_json_of, sha256_hex};
use crate::EventEnvelope;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainedEvent {
    #[serde(flatten)]
    pub event: EventEnvelope,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct EventChain {
    head: String,
    next_seq: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChainError {
    #[error("event #{index} has seq {found}, expected {expected}")]
    Sequence {
        index: usize,
        expected: u64,
        found: u64,
    },
    #[error("event #{index} prev_hash does not match the previous event")]
    Link { index: usize },
    #[error("event #{index} hash does not match its content")]
    Content { index: usize },
    #[error("event #{index} could not be serialized")]
    Serialization { index: usize },
}

impl EventChain {
    pub fn new(genesis: impl Into<String>) -> Self {
        Self {
            head: genesis.into(),
            next_seq: 0,
        }
    }

    pub fn head(&self) -> &str {
        &self.head
    }

    pub fn len(&self) -> u64 {
        self.next_seq
    }

    pub fn is_empty(&self) -> bool {
        self.next_seq == 0
    }

    /// Assign the next sequence number and link the event to the chain.
    pub fn append(&mut self, mut event: EventEnvelope) -> ChainedEvent {
        event.seq = self.next_seq;
        let hash = link_hash(&self.head, &event).expect("event envelopes always serialize");
        let chained = ChainedEvent {
            event,
            prev_hash: self.head.clone(),
            hash: hash.clone(),
        };
        self.head = hash;
        self.next_seq += 1;
        chained
    }
}

pub fn link_hash(prev_hash: &str, event: &EventEnvelope) -> Result<String, serde_json::Error> {
    let body = canonical_json_of(event)?;
    let mut material = String::with_capacity(prev_hash.len() + 1 + body.len());
    material.push_str(prev_hash);
    material.push('\n');
    material.push_str(&body);
    Ok(sha256_hex(material.as_bytes()))
}

/// Verify a recorded log against its genesis. Returns the head hash.
pub fn verify_chain(genesis: &str, events: &[ChainedEvent]) -> Result<String, ChainError> {
    let mut head = genesis.to_string();
    for (index, chained) in events.iter().enumerate() {
        if chained.event.seq != index as u64 {
            return Err(ChainError::Sequence {
                index,
                expected: index as u64,
                found: chained.event.seq,
            });
        }
        if chained.prev_hash != head {
            return Err(ChainError::Link { index });
        }
        let expected =
            link_hash(&head, &chained.event).map_err(|_| ChainError::Serialization { index })?;
        if expected != chained.hash {
            return Err(ChainError::Content { index });
        }
        head = expected;
    }
    Ok(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn event(n: u64) -> EventEnvelope {
        let mut event = EventEnvelope::new(
            "observation",
            "sensor",
            "agent",
            "corr",
            "cause",
            "test",
            json!({"n": n}),
        );
        event.event_id = format!("evt-{n}");
        event.timestamp = 1000 + n as i64;
        event
    }

    fn build(count: u64) -> Vec<ChainedEvent> {
        let mut chain = EventChain::new("sha256:genesis");
        (0..count).map(|n| chain.append(event(n))).collect()
    }

    #[test]
    fn chain_verifies_and_reports_head() {
        let events = build(5);
        let head = verify_chain("sha256:genesis", &events).unwrap();
        assert_eq!(head, events.last().unwrap().hash);
    }

    #[test]
    fn wrong_genesis_is_detected() {
        let events = build(2);
        assert_eq!(
            verify_chain("sha256:other", &events),
            Err(ChainError::Link { index: 0 })
        );
    }

    #[test]
    fn chained_event_round_trips_through_jsonl() {
        let events = build(3);
        for chained in &events {
            let line = serde_json::to_string(chained).unwrap();
            let back: ChainedEvent = serde_json::from_str(&line).unwrap();
            assert_eq!(&back, chained);
        }
    }

    proptest! {
        /// Hashes are recomputed from parsed JSON, so every finite float must
        /// survive a write/read round trip bit for bit.
        #[test]
        fn chains_with_arbitrary_floats_verify_after_a_round_trip(values in proptest::collection::vec(-1.0e9f64..1.0e9, 1..20)) {
            let mut chain = EventChain::new("sha256:genesis");
            let events: Vec<ChainedEvent> = values
                .iter()
                .enumerate()
                .map(|(n, v)| {
                    let mut e = event(n as u64);
                    e.payload = json!({"value": v, "tiny": v / 1.0e12});
                    chain.append(e)
                })
                .collect();
            let reparsed: Vec<ChainedEvent> = events
                .iter()
                .map(|c| serde_json::from_str(&serde_json::to_string(c).unwrap()).unwrap())
                .collect();
            prop_assert!(verify_chain("sha256:genesis", &reparsed).is_ok());
        }

        #[test]
        fn any_payload_edit_is_detected(count in 2u64..20, victim in 0usize..20, value in any::<i64>()) {
            let mut events = build(count);
            let victim = victim % events.len();
            let original = events[victim].event.payload.clone();
            events[victim].event.payload = json!({"n": value});
            prop_assume!(events[victim].event.payload != original);
            let error = verify_chain("sha256:genesis", &events).unwrap_err();
            prop_assert_eq!(error, ChainError::Content { index: victim });
        }

        #[test]
        fn removing_an_event_is_detected(count in 2u64..20, victim in 0usize..20) {
            let mut events = build(count);
            let victim = victim % events.len();
            events.remove(victim);
            prop_assume!(victim < events.len());
            prop_assert!(verify_chain("sha256:genesis", &events).is_err());
        }
    }
}
