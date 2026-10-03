# Resonance and run metrics

Every metric here has a stated definition, a unit, and a basis, which is the number of observations it rests on. A metric that cannot be computed is `null` with a reason. It is never zero by default. Definitions live in one place, `DEFINITIONS` in `crates/mesh-lab/src/metrics.rs`. This page, `metrics.json`, the reports, and the cockpit's tooltips are all generated from it.

## The resonance vector (experimental)

"Resonance" in this project names a small set of **operational measures of how a group of agents moves together under the kernel**. They are computed only from committed state transitions and recorded decisions. They are not measures of cognition, wellbeing, intent, or anything outside this simulator. Each dimension reports its value, its basis, and a note when it is null.

| Dimension | Unit | Definition |
| --- | --- | --- |
| `resonance.coherence` | [0,1] | Mean over ticks with >= 2 committed moves of \|sum of unit displacement vectors\| / n (Vicsek order parameter). 1 = all committed motion aligned. |
| `resonance.stability` | [0,1] | 1 - (direction reversals / consecutive committed-move pairs), pooled over agents. |
| `resonance.convergence` | [0,1] | task_success. |
| `resonance.disagreement` | [0,1] | Fraction of same-tick proposal pairs from different agents whose finite targets are closer than max(min_separation, 1). |
| `resonance.recovery` | [0,1] | Mean over faults of min(1, commit throughput in the W=8 ticks after the fault / in the W ticks before); null without faults or pre-fault commits. |
| `resonance.uncertainty` | [0,1] | Fraction of proposals decided on degraded evidence: observation older than max_stale_ms/2, observation quality not GOOD, or judgment not OK. |

How to read them:

- **coherence** is the Vicsek order parameter of committed displacements. A high value means committed moves in a tick pointed the same way, which can mean coordination or simply a shared goal. It does not mean "agreement".
- **stability** falls when an agent's committed moves reverse direction. The oscillating agent in the canonical scenario exists to lower it.
- **convergence** is task success, named here so the vector is complete.
- **disagreement** counts same-tick proposals that would collide, before the kernel resolves them.
- **recovery** compares commit throughput in the 8 ticks after each fault with the 8 ticks before it. It is null when there is no fault or no pre-fault commit, and it is capped at 1. An increase after a fault does not count as recovery.
- **uncertainty** is the share of decisions made on degraded evidence.

The `resonance-routing` experiment tests whether one of these signals (per-agent instability, a cousin of *stability*) is useful for routing judgment. It cut judgment calls by 182 per run but lost the judgment stage's protection against degraded evidence (+2.55 commits). That is a measured limitation of this signal, recorded as a contradicted claim.

## Run metrics

| Metric | Unit | Definition |
| --- | --- | --- |
| `proposals` | count | Proposals submitted to the kernel (duplicates included). |
| `accepted_proposals` | count | Proposals that passed every deterministic check on first submission. |
| `rejected_proposals` | count | Proposals rejected by deterministic validation. |
| `committed` | count | Proposals whose commit changed authoritative state (including after human approval). |
| `withheld` | count | Proposals that passed validation but were not committed (judgment, operator-load gating, or human rejection). |
| `human_reviews` | count | Proposals routed to human review. |
| `human_approved` | count | Human reviews resolved by approval that then committed. |
| `unsafe_proposals` | count | Proposals the independent oracle classifies unsafe (non-finite, out of bounds, or path through an active hazard). |
| `unsafe_proposals_blocked` | count | Unsafe proposals that were not committed. |
| `unsafe_commits` | count | Unsafe proposals that were committed. Expected 0 whenever hazard checks are enabled. |
| `safe_rejections` | count | Oracle-safe proposals rejected by deterministic validation (staleness, separation, step size, duplicates, registration). |
| `unstable_commits` | count | Commits whose displacement reverses the same agent's previous committed displacement (dot product < 0). |
| `degraded_evidence_commits` | count | Commits of proposals whose observation was older than max_stale_ms/2 or not of GOOD quality. |
| `min_pairwise_distance` | distance | Smallest distance between any two agents' authoritative positions at the end of any tick. |
| `separation_breaches` | count | Agent pairs closer than 1.0 at the end of a tick, summed over ticks. |
| `judgment_calls` | count | Judgment requests issued after validation passed. |
| `judgment_disagreements` | count | Judgments of deterministic-valid proposals whose disposition was not PASS. |
| `judgment_disagreement_rate` | ratio | judgment_disagreements / judgment_calls; null without judgment calls. |
| `judgment_unavailable` | count | Judgments with disposition UNAVAILABLE (timeouts, network, disabled provider). |
| `mean_judgment_latency_ms` | ms | Mean latency_ms reported in judgment envelopes (0 for local deterministic judges). |
| `network_calls` | count | Judgment requests that reached a networked provider. |
| `deterministic_rejection_rate` | ratio | rejected_proposals / proposals. |
| `commit_rate` | ratio | committed / proposals. |
| `goals_total` | count | Agents with a terminal goal (patrol agents have none). |
| `goals_reached` | count | Agents whose authoritative position ended within goal_radius of their goal. |
| `task_success` | ratio | goals_reached / goals_total; null without goals. |
| `convergence_tick` | tick | First tick after which every goal was reached; null if never. |
| `ticks_run` | ticks | Ticks executed before termination. |
| `mean_observation_age_ms` | ms | Mean age of the observation each proposal relied on. |
| `faults_injected` | count | Faults activated during the run. |
| `recovery_ticks` | ticks | Mean ticks after a fault ends until 3-tick commit throughput reaches 80% of the pre-fault level; null if never or no faults. |
| `mean_operator_load` | index[0,1] | Mean of usable simulated operator-load samples. |
| `max_operator_load` | index[0,1] | Maximum usable simulated operator-load sample. |
| `ticks_at_high_load` | ticks | Ticks with adaptive level HIGH or CRITICAL. |
| `human_state_coupling` | pearson r | Correlation between per-tick operator load and the per-tick fraction of proposals not committed; null with fewer than 5 ticks or zero variance. |

## Ground truth

`unsafe_*` metrics come from an **independent geometric oracle**, not from the validator. It samples 65 points along each proposed path and classifies the proposal as unsafe when any point lies inside a hazard that is active at that tick, or when the target is non-finite or out of bounds. The validator uses a closed-form segment–circle distance. The two implementations share only the definition of a hazard (a declared circle), so a validator bug is visible as `unsafe_commits > 0`. The `validator-stress` experiment shows this: disabling the hazard check produces 9.12 unsafe commits per run.

`degraded_evidence_commits` uses a threshold (half the staleness limit) that is stricter than the validator's. That gives judgment something to catch that validation lets through, and is how the judgment-ablation experiment measures the judge's contribution.

## Statistics over runs

For experiments, every metric is summarized per condition (n, mean, median, sd, variance, min, max, 95% t-interval) and compared between paired conditions. See `docs/experiments.md` and `docs/research-methodology.md`.
