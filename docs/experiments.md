# Experiments

An experiment is a YAML manifest: a question, a hypothesis with one pre-registered prediction, conditions that differ in named configuration fields, a repetition count, and the invariants that must hold. `mesh experiment run` executes every condition at every repetition, records the runs, and writes a summary a person can check.

```bash
mesh experiment run experiments/judgment-ablation/manifest.yaml           # 100 repetitions × 5 conditions
mesh experiment run experiments/judgment-ablation/manifest.yaml --reps 10 # quicker
mesh experiment compare judgment-ablation                                  # paired comparisons from a finished run
mesh experiment run experiments/judgment-ablation/manifest.yaml --allow-network   # also runs the Jev condition (needs TYPESAFE_API_KEY)
```

Output in `artifacts/experiments/<id>/`:

| File | Content |
| --- | --- |
| `summary.json` | Per-condition statistics, paired comparisons, prediction verdict, invariant counts, flagged surprises, limitations, manifest hash |
| `runs.csv` | One row per run: condition, repetition, seed, run id, head hash, every metric. This is the raw data. |
| `report.md` | The same in prose and tables. No language model writes any of it. |
| `runs/` | Full bundles for repetition 0 of each condition (`--keep all` keeps every run) |

## Manifest

```yaml
id: judgment-ablation
title: Judgment ablation on the perturbed mesh
question: >
  Does a bounded judgment stage after deterministic validation change which
  proposals are committed, and at what cost?
hypothesis:
  statement: >
    With the evidence-heuristic judge, proposals made on degraded evidence are
    withheld instead of committed. ...
  prediction:
    metric: degraded_evidence_commits
    direction: decrease          # decrease | increase | no_change
    baseline: deterministic_only
    treatment: mock_judgment
seed: 42
repetitions: 100
scenario: ../../scenarios/perturbed-mesh.yaml
independent_variables: [kernel.judgment.provider, kernel.judgment.profile]
dependent_variables: [committed, withheld, degraded_evidence_commits, unsafe_commits, ...]
conditions:
  - id: deterministic_only
    description: Validation and policy only; judgment is not consulted.
    set: { kernel.judgment.provider: disabled }
  - id: jev
    description: TypeSafe Jev over the network.
    set: { kernel.judgment.provider: typesafe }
    requires: [typesafe]       # skipped, and reported as skipped, when unavailable
invariants: [event_chain_intact, no_commit_without_validation_pass, no_unsafe_commits, ...]
limitations:
  - The mock judges are deterministic rules; they are not a language model.
```

`set` accepts any configuration path: validator limits, disabled domain checks, judge provider and profile, routing, gating, faults, agents, ticks, and human review mode. Unknown fields are rejected, so a typo cannot silently become a no-op.

## Design

- **Paired repetitions (common random numbers).** Repetition *r* uses seed `repetition_seed(base, r)` in every condition. The conditions differ only in the manipulated fields, so differences are measured within a pair.
- **Statistics** (`crates/mesh-lab/src/stats.rs`). Per condition: n, mean, median, sd, variance, min, max, and a t 95% interval. Per comparison, on paired differences: mean and median difference, t interval, seeded percentile bootstrap interval (4000 resamples), Cohen's d_z (mean difference / sd of differences), Hedges' g (between-condition, small-sample corrected), and counts of pairs that increased, decreased, or stayed equal.
- **Prediction verdict.**
  - `supported`: the 95% interval of the paired difference lies entirely on the predicted side of 0.
  - `contradicted`: the interval lies entirely on the other side.
  - `inconclusive`: the interval includes 0. For `no_change`, an interval that includes 0 is still only `inconclusive`, because no difference detected is not proof of equivalence.
  - Degenerate case: when every pair differs by exactly the same amount, the verdict follows the sign of that difference, and the report says no interval was needed.
  - `not_evaluable`: a named condition did not run.
- **Flagged surprises.** The summary lists, without filtering, every violated invariant (expected or not), every condition that was skipped and why, any repetition where replayed judgments failed to reproduce their source condition, and a contradicted prediction.

## Current experiments

All runs are SIMULATED. Mock judges are deterministic rules. The 95% intervals below are on the paired difference (treatment − baseline).

| Manifest | Question | Result |
| --- | --- | --- |
| `baseline` | Reference values for the canonical scenario | 30 repetitions; no invariant violated |
| `judgment-ablation` | What does a judgment stage after validation change? | Evidence judge: degraded-evidence commits −2.79 [−2.94, −2.64], withheld +2.46, committed −1.5, unsafe commits 0 in every condition. Replayed judgments reproduce the judged condition exactly. The contrarian judge withholds 97.8 more per run. Task success is unchanged. Jev is skipped without `--allow-network`. |
| `validator-stress` | Is each domain check causal for safety? | No hazard check: unsafe commits +9.12 [8.90, 9.34]. No step limit: 0 unsafe commits, task success −0.25 [−0.29, −0.21] (mechanism not yet examined). No domain checks: +5.66. |
| `fault-injection` | Do authority boundaries hold under faults? | Ten fault kinds × 40 pairs: 0 unsafe commits, every invariant holds in all 440 runs. Judge timeout and network loss withhold 30.9 per run (fail closed). Corrupted observations *raise* commits by 34.9, an emergent effect: the greedy agent's step-halving never recovers. Human-state loss fails closed (−3.9 commits); `hold_last` silently disables gating (+12.1). |
| `human-state-adaptation` | Does operator-load gating defer background work? | Committed −31.92 [−32.49, −31.35]; withheld +36. |
| `multi-agent-coordination` | What does a larger minimum separation cost? | Separation 4 vs 2: task success −0.21 [−0.28, −0.15], min pairwise distance +2.0, rejections +41.1. |
| `resonance-routing` | Can judgment be routed only to unstable agents? | Judgment calls −182.2 [−184.3, −180.1] (prediction supported), but degraded-evidence commits +2.55 [2.34, 2.76]: stale evidence came from a historically stable agent whose sensor dropped out, so instability routing missed it. |

These results are claims about this simulator. `claims/*.yaml` records which statements they support, with limitations, and `mesh claims check` re-checks them (see `docs/research-methodology.md`).

## Writing a new experiment

1. Copy a manifest, state the question, and pick one prediction before running anything.
2. Run with a few repetitions and read `report.md`, especially the flagged surprises.
3. Run at full repetitions and commit the manifest. Results are artifacts, so they are regenerated, not committed. The cockpit's export (`mesh export-web`) is the exception: it commits one copy of every summary for the public page.
4. If the result supports or contradicts a statement worth making, add a claim in `claims/` with its evidence and limitations.
