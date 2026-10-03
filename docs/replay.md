# Replay, verification, and counterfactuals

A recorded run can be checked in three ways, from cheapest to strongest:

| Command | What it establishes | Re-executes? |
| --- | --- | --- |
| `mesh verify <bundle>` or `python3 tools/verify_bundle.py <bundle>` | The record was not altered: every hash-chain link, the genesis (hash of `manifest.json`), file digests, counts, and the execution-law invariants as written in the events | no |
| The cockpit's **Verify hash chain in this browser** | The chain and genesis, recomputed with WebCrypto | no |
| `mesh replay <run>` | The code still produces this record: the run is executed again from its configuration and compared event by event | yes |

## What replay does

1. Checks integrity first. A tampered bundle is not re-executed.
2. Builds **substitutions**: inputs that came from outside the deterministic core and so cannot be regenerated.
   - judgments from a networked or recorded judge (the recorded envelopes are returned; no socket is opened)
   - turns of non-deterministic external agents (intent, explanation, protocol faults)
   - operator commands from a live session (fault injections, operator proposals, review decisions)
   - an operator stopping a live session early
   - human-state data that arrived on `/ingest`

   Everything else is re-executed: the world, built-in agents, the validator, deterministic judges, policy, state transitions, metrics, and invariants.
3. Runs with `allow_network: false`. A networked judge cannot be constructed, so replay cannot make an external model call. The report includes `network_calls`, and it is 0.
4. Compares every event after normalizing only `software_version` (a release bump alone is not a behavior change) and, when judgments were substituted, the judgment's `evaluation_mode`. Then it compares counts by event type, every metric, the final state hash, and the head hash.

```
$ mesh replay fixtures/golden/perturbed-mesh
REPLAY VERIFIED  perturbed-mesh.default.r000.s42

  Integrity:            OK (chain intact, 8/8 file hashes match)
  Events:                  1424 / 1424
  Proposals:                172 / 172
  Validator outputs:        176 / 176
  Judgments:                164 / 164
  Policy decisions:         176 / 176
  State transitions:        157 / 157
  Human-state samples:       80 / 80
  Observations:             390 / 390
  Faults injected:            2 / 2
  Human resolutions:          4 / 4
  Identical events:        1424 / 1424
  Metrics:              35 / 35 match
  Final state hash:     MATCH sha256:fc0c9f1499b8eefc74340edcd04e2598871449d570d19999961f193e355ea28e
  Event log head:       MATCH sha256:77230d4bc4811d6ee223528c5c8307dfd7d64b40244beeeb4e2e082f70300655
  Network calls:        0
```

When anything differs, the report names the **first divergent event**, its index, both versions, and the differing fields. It also lists every metric that changed:

```
REPLAY DIVERGED
  First divergence at event #670
    recorded: evt-000939 run_completed (tick 38)
    replayed: evt-000939 human_state (tick 39)
```

That example is real: before operator stops were substituted, a live session stopped at tick 38 replayed to the scenario's end. Every event up to the stop matched, and the report pointed at the exact place.

## Golden recordings

`fixtures/golden/` holds three committed bundles: the canonical Perturbed Mesh run, a corrupted-event run, and an external-agent run. Each has a `source.yaml` that says how to regenerate it. CI runs `mesh golden verify`, which replays all three. A change to the simulator, validator, judges, or policy that alters any recorded event fails CI with the first divergent event. After an intentional behavior change, `mesh golden update` regenerates the bundles and the diff shows what changed.

The external-agent fixture replays without Python: the agent's recorded turns are substituted.

## Counterfactuals

```bash
mesh counterfactual <run> --set kernel.judgment.provider=disabled --label no-judgment
mesh counterfactual <run> --set 'kernel.validator.disabled_checks=["hazard_clearance"]'
mesh diff <run-a> <run-b>
```

A counterfactual re-runs a recording with the same seed, the same substituted inputs, and one or more configuration changes. The original bundle is never modified. The branch is saved as `<run>~<label>` with `divergence.json` and a provenance link to its parent (run id, head hash, overrides). The comparison reports:

- the first divergent event, where the record parts
- the first tick at which a decision's outcome differs, where behavior parts
- the first tick at which the authoritative state hash differs, plus the state hash at every tick on both sides
- every decision whose final outcome changed, with the tick each side decided it
- every metric that changed

For the canonical run with judgment disabled, the record parts at event #19: the original has a judgment event where the branch goes straight to commitment. Behavior parts at tick 12, and 9 decisions end differently. One run is an observation. Use an experiment to estimate an effect.

## Replaying in the cockpit

The Replay view scrubs any recorded run tick by tick and inspects each decision. The Compare view shows branches. With `mesh serve`, Replay also offers **Re-execute on server** (the same as `mesh replay`), and Compare can create new branches.
