# Provenance

Every number the system reports traces back to an event, and every event traces back to the configuration that produced it.

## Run bundle (`resonate-ai-mesh.run-bundle.v1`)

| File | Content | Deterministic | Hashed in provenance |
| --- | --- | --- | --- |
| `manifest.json` | The fully resolved `RunConfig`: scenario, kernel configuration, overrides, seed, run id. Its canonical hash is the genesis of the event chain. | yes | yes |
| `events.jsonl` | Every event, each with `prev_hash` and `hash` | yes | yes |
| `decisions.jsonl` | One line per decision, flattened for analysis, with the independent oracle's verdict. Derived from events. | yes | yes |
| `intents.jsonl` | Recorded turns of non-deterministic external agents (empty otherwise) | yes | yes |
| `metrics.json` | Metric values, the resonance vector, rejection and withhold reasons, invariant results | yes | yes |
| `environment.json` | The world as configured (arena, hazards, agents, faults) and the final state | yes | yes |
| `topology.json` | The typed component graph for this run | yes | yes |
| `replay.json` | Seed, genesis, head hash, final state hash, counts, substitutions, policy versions | yes | yes |
| `provenance.json` | Software version, git commit and dirty flag, rustc, OS and architecture, command line, wall-clock start and finish, every policy and schema version, judge and agent descriptors, parent run for branches, and the digests of the files above | no (wall times) | — |
| `timing.json` | Wall-clock throughput and pipeline latency (p50, p95, max) | no | no; never compared by replay |
| `report.md` | Human-readable summary: what ran, how to reproduce it, and what it found | — | no |

Run ids are `<experiment>.<condition>.r<rep>.s<seed>`. Counterfactual branches append `~<label>`, and live sessions are `live.<scenario>.<UTC time>`.

## Event provenance (envelope v1.2.0)

Each event records `event_id`, `event_type`, `timestamp` (logical milliseconds), `source` (the component), `subject_id`, `correlation_id` (one decision), `causation_id` (the direct cause), `provenance` (component version and the evidence used, e.g. `pordenone.validator.v2:obs-000015`), `run_id`, `experiment_id`, `seq`, `tick`, `mode` (`SIMULATED`, `LIVE`, `REPLAY`), `policy_version`, `software_version`, `ai_involved`, and a typed `payload`. `schemas/json/canonical-event.json` is the schema. `tests/contract/` validates every recorded golden event against it and parses them with the generated protobuf types.

`ai_involved` is true only when a probabilistic or remote model produced or influenced the event. With the deterministic mock judges it is false. The cockpit and `mesh explain` say so explicitly ("A probabilistic model was involved: no").

## Hashing

- **Canonical JSON**: object keys sorted by code point (UTF-8 byte order), no whitespace, strings escaped as `serde_json` does, and numbers in `serde_json`'s shortest round-trip form. Integers stay integers and floats keep a decimal point (`100.0`). `serde_json` 1.0.151 writes `1e+16` and `1e-7`.
- **Link hash**: `hash = "sha256:" + hex(SHA-256(prev_hash + "\n" + canonical(event without prev_hash and hash)))`.
- **Genesis**: `sha256` of the canonical JSON of `manifest.json`, so changing any configuration value changes every hash in the run.
- **State hash**: the canonical hash of the authoritative state after each transition. Each `state_transition` event records `before_hash` and `after_hash`, and consecutive transitions must chain.

Recorded floats are quantized (positions to 4 decimals, rates to 6) so the record is readable, and `float_roundtrip` is enabled in `serde_json`, so parsing a recorded number returns the identical double. A property test found that the default parser does not guarantee this, and a regression test keeps it fixed.

## Three independent verifiers

| Verifier | Language | Shares code with the recorder? |
| --- | --- | --- |
| `event_bus::verify_chain` / `mesh verify` | Rust | yes |
| `tools/verify_bundle.py` | Python (standard library only) | no |
| `apps/c2-dashboard/src/lib/verify.ts` | TypeScript + WebCrypto | no |

`fixtures/canonical/floats.json` pins the rendering of more than 500 doubles: boundary cases and pseudo-random values across the exponent range. The Rust test fails if a dependency upgrade changes the rendering, and the Python and TypeScript tests fail if either verifier disagrees with it. A float-formatting change in a dependency is therefore a visible, deliberate event rather than a silent change to every hash.

The Python verifier also re-derives the execution law from the events alone: commits name a passing validation of the same proposal, judgments follow passing validations, state revisions and hashes form an unbroken sequence, and data-mode labels are consistent. A forger who recomputes the whole chain after deleting a commit still fails those cross-checks and the file digests (see `tests/verifier/`).

## What provenance does not prove

A valid chain proves the record is internally consistent and unaltered since it was hashed. It does not prove that the simulator models anything real, and it does not prove the recording machine was honest about its wall times or git state. `mesh replay` proves the current code reproduces the record. Signing bundles is not implemented.
