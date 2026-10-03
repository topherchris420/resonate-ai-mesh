# Bounded judgment and TypeSafe Jev

Judgment in the Pordenone kernel is **optional, bounded, and advisory**. It runs only on proposals that already passed every deterministic check. It answers a fixed set of typed questions about the evidence. Kernel policy, not the judge, decides what the answers allow. A judge can make the kernel *more* cautious (withhold, or route to a person). It can never make it less cautious: it cannot turn a failed check into a commit, and it cannot write state.

Jev, TypeSafe's model, is one possible judge. Nothing in the system requires it.

## Where judgment sits

```
proposal → validation ──fail──→ rejected_deterministic          (judge never called)
               │pass
               ▼
         routing: always | only for unstable agents
               │
               ▼
         judge(evidence package) → envelope (advisory)
               │
               ▼
         judgment policy → disposition: PASS | REVISE | HUMAN_REVIEW | UNAVAILABLE | SKIPPED
               │
               ▼
         kernel policy → committed | withheld | awaiting_human_review
```

## Judges

| Provider | Kind | Networked | Use |
| --- | --- | --- | --- |
| `disabled` | — | no | No judgment. Validation and policy only. |
| `mock`, profile `evidence_heuristic` | deterministic | no | Rule-based stand-in that reacts to evidence quality (`evidence-heuristic-v1`, below). **Not a model.** |
| `mock`, profile `contrarian` | deterministic | no | Disagrees with a fixed third of valid proposals, chosen by a stable hash, with high confidence. Measures how the kernel handles adversarial judgment. |
| `recorded` | recorded | no | Returns envelopes recorded in another run. Checks that the evidence package's hash matches before answering. |
| `typesafe` | remote model | **yes** | Jev via `POST {TYPESAFE_BASE_URL}/v1/systemone`. Needs `TYPESAFE_API_KEY` and `--allow-network`. |

`evidence-heuristic-v1`: staleness `s = clamp((1 − deterministic_confidence) / 0.5, 0, 1)`; evidence score `= clamp(3 − 2s − (1 − signal_quality), 0, 3)`; support `supported` if score ≥ 2, `mixed` if ≥ 1, otherwise `insufficient_evidence`; confidence `0.95 − 0.5s`; contradiction 0.9 if validation recorded contradictions, else 0.05; scope `0.1 + 0.6s`; human review `0.1 + 0.5·[priority ≥ 5] + 0.3s`.

## Questions (`pordenone.judgment.questions.v1`)

All five are asked together against the same immutable evidence package. None refers to another, and none asks the model to act.

| Id | Primitive | Question |
| --- | --- | --- |
| `proposal_support` | Choice | Given only the supplied evidence, how well is this proposal supported? `supported`, `mixed`, `unsupported`, `insufficient_evidence` |
| `evidence_quality` | Score 0–3 | How strong is the supplied evidence? (insufficient, weak or indirect, adequate but limited, strong and directly relevant) |
| `contradiction_present` | Noul | The supplied evidence meaningfully contradicts the proposal. |
| `scope_violation` | Noul | The proposal goes materially beyond what the observations establish. |
| `human_review` | Noul | The ambiguity or consequence warrants human review before commitment. |

Choice and Score answers carry a probability distribution and a confidence. A Noul is a single probability with **no** confidence; the schema forbids one.

## Judgment policy (`pordenone.judgment.policy.v1`)

Thresholds come from the scenario (`kernel.judgment_policy`). Noul comparisons are strict.

| Condition | Disposition |
| --- | --- |
| provider timeout, auth failure, rate limit, malformed response, missing credentials, state error, network loss | `UNAVAILABLE` → withheld |
| support is `unsupported`, `insufficient_evidence`, or `mixed`; evidence score < 2; scope > 0.50; contradiction in (0.50, 0.75] | `REVISE` → withheld |
| contradiction > 0.75; human review > 0.50; Choice or Score confidence missing or < 0.70 | `HUMAN_REVIEW` → awaiting a person |
| supported, adequate evidence, nouls at or below thresholds, confidence ≥ minimum | `PASS` → committed (subject to operator-load gating) |

When several rules match, the strictest wins and every reason code is kept. Failures never become `PASS`. A human approval of a `HUMAN_REVIEW` proposal is re-validated against current state, and if the recheck fails the proposal is rejected (`HUMAN_APPROVAL_BLOCKED`).

## The evidence package (`pordenone.judgment.state.v2`)

The minimum the judge needs: the proposal (id, agent, action, target, priority), the source observation id and whether it is simulated, the deterministic validation result (feasibility, checks, contradictions, deterministic confidence), the operator level, derived operator indices from an allow-list (`operator_load_index`, `signal_quality`, `state_confidence`, and a few others), spatial bounds, explicit limitations (missing inputs, excluded field *names*, truncation), and provenance ids. The canonical JSON is hashed (`state_hash`) and recorded on the envelope. Raw physiological signals, identities, credentials, transcripts, and parameter blobs are never included. A test seeds those fields with sentinel values and checks that none appear in the serialized package.

TypeSafe states that Jev is not trained on customer requests, and that zero data retention is an enterprise option rather than the default. The package is minimized regardless.

## Running Jev

```bash
export TYPESAFE_API_KEY=...                      # never logged; errors omit it
mesh run scenarios/perturbed-mesh.yaml --set kernel.judgment.provider=typesafe --set ticks=10 --allow-network
mesh experiment run experiments/judgment-ablation/manifest.yaml --allow-network   # adds the `jev` condition
mesh replay <that run>                           # substitutes the recorded envelopes; 0 network calls
```

Optional settings: `JUDGMENT_MODEL` (default `jev-latest`), `TYPESAFE_BASE_URL`, and the judge timeout in the scenario (`kernel.judgment.timeout_ms`). The response's `model` field (for example `jev-1.13.0`) is recorded as `provider_model_version`. Every judgment event from a remote model is marked `ai_involved: true`, and the run's metrics count `network_calls`.

Without `--allow-network`, a run that asks for Jev fails with an explanation. In an experiment, the condition is skipped and reported as skipped. Replay never constructs a networked judge.

## What the ablation measures

`experiments/judgment-ablation` compares, at the same seeds, no judge, the evidence-heuristic judge, its recorded judgments replayed, the contrarian judge, and Jev when available. Results so far, with mock judges only: the evidence judge removes 2.79 degraded-evidence commits per run [−2.94, −2.64], at a cost of 1.5 commits. Replay reproduces it exactly. Task success does not change. Unsafe commits are 0 in every condition, because validation runs first. Jev has not yet been run through this experiment at scale, and nothing here claims how a language model would judge.
