# Deterministic Session Recording and Replay

Pordenone includes session recording and replay tooling for experimental verification and auditability.

## Workflow

1. **Record Session**
   ```bash
   python3 scripts/record-session output_session.json
   ```
   Records telemetry events, simulation state transitions, and random seeds into `output_session.json`.

2. **Replay Session**
   ```bash
   python3 scripts/replay-session output_session.json
   ```
   Re-instantiates the pipelines with the session seed, steps through time points, and compares computed outputs against recorded values.

## Determinism Rules
- Random number generators are seeded per instance using `seed`.
- Floating-point calculations use fixed precision rounding (e.g., 4 decimal places for metrics).
- Equal input events + identical seed -> 100% match.

## Judgment replay

Recorded sessions may include `judgment` events. Replay uses the stored envelope and labels the replay copy `RECORDED_JUDGMENT`.

Replay does not call TypeSafe, even if `TYPESAFE_API_KEY` is present. The stored answers, disposition, and `provider_model_version` stay on the original event.

A later live reevaluation is a separate comparison. Example:

```text
recorded:     jev-1.x  proposal_support=supported  confidence=0.88
reevaluation: jev-1.y  proposal_support=mixed      confidence=0.67
drift_detected=true
```

Disagreement is retained. The original envelope is not overwritten. The kernel `RecordedJudgmentProvider` follows the same rule for in-process replay.
