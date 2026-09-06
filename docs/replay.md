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
