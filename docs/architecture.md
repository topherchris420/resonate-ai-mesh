# Architecture

Resonate AI Mesh is one Rust workspace (the kernel and the experiment engine), one Next.js app (the research cockpit), and a few small Python tools. A run is a pure function of its resolved configuration and seed. The only exceptions are inputs from outside the process, and those are recorded.

## Names

| Name | What it is | Where |
| --- | --- | --- |
| Resonate AI Mesh | The whole system: simulated world, agents, kernel, recorder, experiments, cockpit | this repository |
| Pordenone kernel | The deterministic decision core: validation → optional judgment → policy → commit | `crates/kernel-core`, `crates/epistemic-validator`, `crates/typed-judgment` |
| Mesh Lab | Simulator, recorder, replay, counterfactuals, experiments, statistics, claims, and the `mesh` CLI and server | `crates/mesh-lab` |
| Research cockpit | Browser UI over recorded and live runs | `apps/c2-dashboard` (directory name kept for deployment) |
| Jev | TypeSafe's model, usable as an optional bounded judge | `crates/typed-judgment/src/typesafe.rs` |
| R.A.I.N. | An external deliberative agent, attached through `mesh-agent/1` | `docs/rain-adapter.md` |

Retired labels: CIRCLE (now the simulated human-state source), NEXUS (the event bus), and DRR (now the operator-load gating policy, which has an operational definition instead of a "resonance" score).

## Crates

| Crate | Responsibility |
| --- | --- |
| `event-bus` | Event envelope v1.2.0, canonical JSON and SHA-256, the hash chain (`EventChain`, `verify_chain`), `HumanStateDatum`, an in-process broadcast bus with metrics |
| `epistemic-validator` | Eleven deterministic checks returning a `Verdict`. Only a pass produces a sealed `ValidatedProposal`. |
| `spatial-state` | `Vector3` and a `BTreeMap` spatial index (ordered iteration keeps hashes stable) |
| `typed-judgment` | Judgment providers (disabled, deterministic mocks, recorded, TypeSafe), the evidence package, the judgment policy |
| `kernel-core` | `KernelEngine`: runs a proposal through the pipeline and owns `AuthoritativeState`, the only mutable state |
| `telemetry-bridge` | The ingest gate: what an outside process may submit (`admit`), per-connection rate limiting |
| `mesh-lab` | Simulator, agents, recorder, metrics, invariants, replay, counterfactuals, experiments, statistics, claims, topology, capabilities, the server, and the `mesh` binary |

## One tick

```
for each tick:
  apply operator commands (live sessions; recorded for replay)
  update hazards, faults, and the human-state signal → events
  for each agent (sorted by id):
    deliver its observation (subject to faults) → observation event
    adapter.observe(); intent = adapter.propose()          ← no authority
    kernel.submit(proposal):
      validate (11 checks) → validation event
      if failed: policy rejects → commitment event; done
      route judgment (always, or only for unstable agents)
      judge → judgment event (advisory)
      policy decides (judgment disposition, operator-load gating) → commitment event
      if committed: AuthoritativeState::apply(authorization) → state_transition event
    adapter.receive_outcome()
  resolve due human reviews (re-validate, then decide)
  record a resonance sample
```

Every event is appended to the recorder's `EventChain`, which links it to the previous event's hash. Time is logical (`start_time_ms + tick × dt_ms`), ids are sequential, and randomness comes from SplitMix64 streams named by purpose and agent, so adding an agent does not shift another agent's random numbers. The simulator uses only IEEE-exact arithmetic and `sqrt`, with no transcendental functions, so results do not depend on the platform's math library.

## The single mutation path

```
EpistemicValidator::validate(proposal, context) → Verdict
    └─ pass → ValidatedProposal          (sealed: only the validator constructs it)
policy::decide(validated, judgment, level, config) → PolicyDecision
policy::authorize(validated, decision, authorized_by) → Option<CommitAuthorization>
    └─ the only constructor; None unless the decision permits a commit
AuthoritativeState::apply(authorization) → StateTransition (revision, before/after hash)
```

`AuthoritativeState` has no other mutation method for agent positions. A judge receives an owned evidence package and returns an envelope, and it holds no reference to the kernel. The compiler enforces these guarantees. The recorder then checks them again on every run:

| Invariant | Checked from the event log |
| --- | --- |
| `event_chain_intact` | every link recomputes |
| `no_commit_without_validation_pass` | every commit names a passing validation of the same proposal in `authorized_by` |
| `no_judgment_after_failed_validation` | every judgment is caused by a passing validation |
| `every_commit_has_provenance` | following `causation_id` from a commit reaches its proposal |
| `no_unsafe_commits` | the independent geometric oracle (65 samples along each committed path) finds no hazard crossing, non-finite target, or out-of-bounds target |
| `simulated_data_labeled` | only a person's commands and review decisions may be `LIVE`, and each human-state datum's mode matches its event |
| `zero_network_calls` | no networked judge was reached |

## Validation

| Check | Kind | Reason code on failure |
| --- | --- | --- |
| `unique_proposal` | core | `DUPLICATE_PROPOSAL` |
| `finite_values` | core | `NON_FINITE_VALUE` |
| `action_allowlist` | core | `ACTION_NOT_ALLOWED` |
| `agent_registered` | core | `AGENT_NOT_REGISTERED` |
| `priority_range` | core | `PRIORITY_OUT_OF_RANGE` |
| `observation_freshness` | core | `OBSERVATION_STALE` |
| `clock_skew` | core | `TIMESTAMP_IN_FUTURE` |
| `coordinate_bounds` | core | `OUT_OF_BOUNDS` |
| `max_step` | domain | `STEP_TOO_LARGE` |
| `hazard_clearance` | domain | `HAZARD_INTERSECTION` (segment–circle distance along the path) |
| `separation` | domain | `SEPARATION_VIOLATION` |

Core checks protect the kernel's integrity and cannot be disabled. A configuration that tries is rejected. Domain checks encode environment rules and may be disabled for ablation experiments (`kernel.validator.disabled_checks`). The validator fails closed: a missing observation, an unknown agent, or a non-finite number is a failure, never a skip.

## Policy

| Input | Outcome |
| --- | --- |
| validation failed | `rejected_deterministic` (judgment never consulted) |
| judgment not consulted (disabled, or routed away from a stable agent) | `committed`, basis `deterministic_only` |
| judgment `PASS` | `committed`, basis `judgment_pass` |
| judgment `HUMAN_REVIEW` | `awaiting_human_review`, then the person decides; approval is re-validated |
| judgment `REVISE`, `UNAVAILABLE`, `PENDING` | `withheld` |
| would commit, operator level HIGH or CRITICAL, priority ≤ `background_priority_max` | `withheld` (`OPERATOR_LOAD_DEFERRAL`) |

Operator-load levels come from the human-state signal with thresholds 0.4, 0.65, and 0.85, which give ELEVATED, HIGH, and CRITICAL. When the signal is lost, the default `on_signal_loss: assume_high` raises the level to at least HIGH. The alternative, `hold_last`, keeps the last level. The fault-injection experiment shows that `hold_last` silently disables gating during a load spike, which is why it is not the default. Policy version: `pordenone.kernel.policy.v2`.

## Mesh Lab

- `sim.rs`: the world (hazards that appear and retire, points of interest, sensors, faults) and the simulated operator-load curve.
- `agents.rs`: `MeshAgentAdapter` (observe, propose, explain, receive_outcome). Built-in behaviors: cautious, greedy, oscillating, patrol, malformed (deliberately invalid every n-th proposal), and scripted. `ExternalProcessAgent` runs any `mesh-agent/1` process.
- `runner.rs`: the tick loop above. `RunOptions` carries live control and replay substitutions.
- `record.rs` and `bundle.rs`: run bundles, reports, and provenance.
- `replay.rs`: re-execution and comparison (`docs/replay.md`). `counterfactual.rs`: branches and divergence.
- `experiment.rs` and `stats.rs`: manifests, paired designs, intervals, effect sizes, and verdicts (`docs/experiments.md`).
- `claims.rs`: claims checked against experiment summaries. `explain.rs`: causal chains.
- `topology.rs`: a typed graph of components and trust boundaries. `capabilities.rs`: what this installation can do, probed.
- `server.rs`: `mesh serve` (`docs/mesh-protocol.md`, `docs/security.md`). `web.rs`: the static cockpit export.

## Cockpit

`apps/c2-dashboard` reads either a running `mesh serve` or the static export in `public/demo` (`mesh export-web`). It computes nothing it then presents as data. Decisions are rebuilt from events, and a test requires them to equal `decisions.jsonl`. `explain` is a port that must match `mesh explain`. Hash chains are re-verified in the browser with WebCrypto. Views: Live, Experiment, Evidence, Replay, Compare, Topology, Capabilities.
