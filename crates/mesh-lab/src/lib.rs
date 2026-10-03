//! Mesh Lab: the experiment engine of Resonate AI Mesh.
//!
//! It drives the Pordenone kernel with simulated environments, agents, human
//! state, and faults; records every event in a hash-chained log; computes
//! metrics; and verifies, replays, and branches recorded runs.

pub mod agents;
pub mod bundle;
pub mod cli;
pub mod config;
pub mod counterfactual;
pub mod experiment;
pub mod invariants;
pub mod metrics;
pub mod record;
pub mod replay;
pub mod rng;
pub mod runner;
pub mod sim;
pub mod stats;
pub mod topology;
