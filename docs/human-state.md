# Human-state data

The kernel can adapt to the state of the person supervising it. When operator load is high, it defers background work. That is a reasonable thing to study and an easy thing to misrepresent, so the rules here are strict.

## What exists

| Source | Mode | Status |
| --- | --- | --- |
| `sim.operator_load.v1`, inside every run | SIMULATED | implemented |
| `services/biometric-pipeline`, a Python process streaming to `/ingest` | SIMULATED | implemented |
| Recorded traces (`human_state.trace`, JSONL of datums) | REPLAY, keeping `original_mode` | implemented |
| A hardware sensor adapter | LIVE | **not implemented** |

No real person's data is used anywhere in this repository. Asking the Python pipeline for `MODE=LIVE` exits with an error instead of producing values that would look like measurements.

## The datum

`schemas/json/human-state-datum.json` and `event_bus::HumanStateDatum`:

```json
{"metric": "operator_load_index", "value": 0.2779, "unit": "index[0,1]", "timestamp": 1700000000000,
 "source": "sim.operator_load.v1", "mode": "SIMULATED", "original_mode": null,
 "confidence": 0.9, "quality": "GOOD"}
```

- `mode` is `SIMULATED`, `LIVE`, or `REPLAY`. Unlabeled data is `SIMULATED`, so nothing is presented as live by default.
- `original_mode` is required for `REPLAY` (what the data was when recorded) and forbidden otherwise.
- `confidence` is the source's own reliability estimate in [0, 1]. It is not a probability that the value is true.
- `quality` is `GOOD`, `DEGRADED`, `INVALID`, or `MISSING`. Only `GOOD` and `DEGRADED` values are used.
- Values must be finite, and text fields must be 1–128 characters. Validation happens at construction and again at ingest.

## The simulated operator-load model

`operator_load_index` is a workload index driven by what happens in the run. It is not a model of cognition. Each tick:

```
target = baseline + w_rejections · (recent rejections / (window · agents))
                  + w_reviews · (pending reviews / agents)
                  + w_complexity · (active hazards / declared hazards)
                  + scripted perturbations + noise
load   = clamp(load + alpha · (target − load), 0, 1)
```

The defaults are baseline 0.25, alpha 0.3, weights 0.6, 0.3, and 0.15, a window of 6 ticks, and noise ±0.01 from the seeded `human_state.noise` stream. The canonical scenario adds a load spike. The model always advances, even when a fault hides the signal, so the random streams stay aligned across experimental conditions.

## How the kernel uses it

| Load | Level | Effect |
| --- | --- | --- |
| < 0.40 | NORMAL | none |
| ≥ 0.40 | ELEVATED | none |
| ≥ 0.65 | HIGH | proposals at or below `background_priority_max` are withheld (`OPERATOR_LOAD_DEFERRAL`) |
| ≥ 0.85 | CRITICAL | same |

When the signal is lost (a `human_state_dropout` fault, `MISSING` or `INVALID` data, or no ingest data within 2 s in a live session), the default `on_signal_loss: assume_high` raises the level to at least HIGH, which fails closed. `hold_last` keeps the previous level. The fault-injection experiment measured the difference: under a dropout just before the load spike, `assume_high` withholds 3.9 more commits than with no fault, while `hold_last` *commits* 12.1 more, because gating silently switched off.

Each change of level is an `adaptive_level` event, so the cockpit's timeline shades the HIGH and CRITICAL ticks and `mesh explain` shows the level each decision was made under. The metric `human_state_coupling` reports the correlation between load and the fraction of proposals not committed. It describes this simulator's design and is not a finding about people.

## Ingest

```bash
mesh serve
MESH_INGEST_URL=ws://127.0.0.1:7878/ingest python services/biometric-pipeline/main.py
```

`/ingest` admits `human_state` and `observation` envelopes only (`schemas/json/ingest-event.json`). The datum's mode must equal the envelope's. `LIVE` data is refused unless its `source` is listed in `MESH_LIVE_SOURCES`. Every reply says `accepted` or gives the reason, and `/metrics` counts rejections by reason. A live session started with `"human_state": "ingest"` uses the latest admitted datum, and data older than 2 s is treated as lost. Ingested data is recorded and substituted on replay.

## Privacy

When a remote judge is enabled, the evidence package may carry only these operator fields: `operator_load_index`, `cognitive_load`, `signal_quality`, `baseline_delta`, `cross_signal_coherence`, `state_confidence`, and `is_simulated`. Any other field is dropped and only its *name* is listed under `excluded_fields`. Raw waveforms (EEG, PPG, EDA), identities, names, and free text are never sent. A test checks that sentinel values placed in those fields do not appear in the serialized state.

If a LIVE adapter is ever added, it should: register its source explicitly, send derived indices rather than raw signals, record its own confidence honestly, and be covered by the same contract tests (`tests/contract/test_contract.py`) that the simulated pipeline passes today.
