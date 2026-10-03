# Research methodology

The mesh is built so that a result can be reproduced, checked, and doubted. This page explains how a question becomes evidence and what that evidence is worth.

## From question to claim

```
question → manifest (hypothesis + one pre-registered prediction)
         → experiment (conditions × paired repetitions, every run recorded)
         → summary (statistics, verdict, flagged surprises, limitations)
         → claim (a person's statement, status, evidence checks, limitations)
         → mesh claims check (does the recorded evidence still agree?)
```

1. **Pre-registration.** The manifest states the prediction (metric, direction, baseline, treatment) before the run. The verdict is computed mechanically from the paired 95% interval (`docs/experiments.md`). Other comparisons are reported, but they are exploratory.
2. **Determinism.** A run is a pure function of its resolved configuration and seed. Logical time, sequential ids, named random streams, and IEEE-exact arithmetic make it reproducible to the byte on the same code. Replay checks this event by event, and the golden fixtures check it in CI.
3. **Common random numbers.** Repetition *r* uses the same seed in every condition, so the conditions see the same sensor noise and the same load curve. Comparisons are paired, which removes between-seed variance from the difference.
4. **Ground truth separate from the system under test.** Safety outcomes come from an independent geometric oracle, not from the validator's own verdict.
5. **Reporting what was not expected.** Every summary lists violated invariants, skipped conditions and why, replay-fidelity failures, and contradicted predictions. Nothing is filtered by significance.
6. **Claims are written by people.** The checker never changes a claim's status.

## Statistics, and their limits

- Per condition: n, mean, median, sd, variance, min, max, and a t 95% interval.
- Per comparison: the paired mean and median difference, a t interval and a seeded percentile bootstrap interval (4000 resamples) on the differences, Cohen's d_z, Hedges' g, and counts of pairs that increased, decreased, or stayed equal. Raw per-run values are in `runs.csv`.
- **No multiple-comparison correction is applied.** An experiment reports every dependent variable for every pair of conditions. Only the pre-registered prediction is confirmatory. Read the rest as exploratory, and expect some intervals to exclude 0 by chance.
- **Huge effect sizes are a property of the simulator.** With common random numbers and a deterministic environment, paired differences often have very small variance, so d_z can be 10 or 40. That says the manipulation is reliable *in this simulator*. It says nothing about effect size in the world.
- **Zero-variance pairs.** When every pair differs by the same amount, no interval is computed and the verdict follows the sign of that difference. The report says so explicitly.
- **"No change" is not equivalence.** An interval that includes 0 under a `no_change` prediction is reported as inconclusive. No equivalence test is implemented.
- **Repetitions vary noise, not design.** Seeds change sensor noise and the operator-load noise. They do not change the scenario's geometry or the agents' behaviors. Generalizing beyond the scenario needs more scenarios, not more seeds.

## Claims

`claims/*.yaml`:

```yaml
id: judgment-withholds-degraded-evidence
claim: >
  On the Perturbed Mesh, adding the evidence-heuristic judgment stage ...
status: provisional        # hypothesis | provisional | supported | contradicted | retracted
evidence:
  - experiment: judgment-ablation
    role: supports         # or contradicts
    check: { type: comparison, metric: degraded_evidence_commits, baseline: deterministic_only,
             treatment: mock_judgment, direction: decrease }
  - experiment: judgment-ablation
    role: supports
    check: { type: replay_fidelity, condition: replayed_judgment }
limitations:
  - The evidence-heuristic judge is a deterministic rule ...
```

Check types: `comparison` (the paired interval lies on one side of 0), `condition_max` and `condition_min` (bounds on a metric in every run of a condition), `invariant` (held in every run), `prediction` (the pre-registered prediction was supported), and `replay_fidelity` (replayed judgments reproduced their source in every repetition). Each check is evaluated against the recorded `summary.json`, and the result cites that summary's hash.

| Declared | Evidence | Level |
| --- | --- | --- |
| supported | consistent | ok |
| supported | incomplete or inconsistent | **error**: the claim overstates its evidence |
| hypothesis or provisional | consistent | note: the status stays until a person changes it |
| hypothesis or provisional | inconsistent | warning |
| contradicted | consistent | warning |

The current registry holds ten claims: three declared supported and five provisional, all with consistent evidence. One hypothesis, that judgment improves task success, is recorded with evidence against it: task success did not change in any of 100 pairs. One claim, that instability routing preserves protection, is recorded as contradicted. These are kept so the record shows what was tested and failed, not only what worked.

## Threats to validity

- **Construct validity.** "Unsafe" means crossing a declared circle, a non-finite target, or out of bounds. "Degraded evidence" means older than half the staleness limit or not GOOD. "Operator load" is a scripted workload index. Each is an operational definition in this simulator.
- **Internal validity.** Agents are deterministic programs with known behaviors, including deliberately bad ones (greedy, oscillating, malformed). Effects can come from how a behavior interacts with a rule. The corrupted-observation fault, for example, increased commits through the greedy agent's step-halving. Use `mesh explain` and the cockpit to look at the mechanism before interpreting an effect.
- **External validity.** One 2D arena family, five agents, and simulated reviewers in batch runs. Mock judges are not language models. Nothing here measures a person.
- **Software.** A change to the code changes recordings by design. Golden fixtures make such changes visible, and `software_version` and the git commit are in every bundle's provenance.

## Reproducing a result

```bash
git checkout <commit from provenance.json>
cargo build --release -p mesh-lab
mesh experiment run experiments/<id>/manifest.yaml        # identical runs.csv rows and head hashes
python3 tools/verify_bundle.py artifacts/experiments/<id>/runs/*/
```

`runs.csv` lists each run's head hash, so two people can compare entire experiments by comparing a column.
