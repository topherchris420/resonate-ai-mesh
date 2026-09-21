# Event Model & Canonical Schemas

All cross-service communications in Pordenone utilize the `CanonicalEventEnvelope` schema.

## Canonical Event Envelope Schema

```json
{
  "event_id": "UUID string",
  "event_type": "telemetry | proposal | validation | judgment | commitment | human_resolution | spatial | adaptive_state | custom",
  "schema_version": "1.0.0",
  "timestamp": 1700000000000,
  "source": "component_name",
  "subject_id": "target_entity_id",
  "correlation_id": "corr_xyz",
  "causation_id": "caus_xyz",
  "provenance": "provenance_string",
  "payload": { ... }
}
```

## Payload Schemas

### Operator State Telemetry (`telemetry`)
- `cognitive_load` (0.0 - 1.0)
- `arousal` (0.0 - 1.0)
- `hrv` (ms)
- `heart_rate` (bpm)
- `attention` (0.0 - 1.0)
- `stress` (0.0 - 1.0)
- `confidence` (0.0 - 1.0)
- `is_simulated` (boolean)

### Action Proposal (`proposal`)
- `proposal_id`
- `agent_id`
- `action_type` (`MOVE`, `PATROL`, `INSPECT`, `HOLD`)
- `target_position` (`x`, `y`, `z`)
- `priority`
- `correlation_id`

### Validation Result (`validation`)
- `proposal_id`
- `agent_id`
- `accepted` (boolean)
- `feasibility` (0.0 - 1.0)
- `contradictions` (list of strings)
- `confidence` (0.0 - 1.0) — deterministic validator confidence, not typed-judgment confidence
- `reasons` (list of strings)
- `provenance`

Validation events remain schema `1.0.0`.

### Typed Judgment (`judgment`, schema `1.1.0`)
Payload is a `JudgmentEnvelope` (`pordenone.judgment.envelope.v1`):

- `judgment_id`, `provider`, `model`, `provider_model_version`
- `question_set_version`, `state_hash`, `state_schema_version`, `truncated`
- `proposal_id`, `correlation_id`, `causation_id`
- `answers[]` with primitive-specific `choice` / `score` / `noul`, `probabilities`, and `confidence` only when the provider returned it
- `disposition` (`PASS`, `REVISE`, `HUMAN_REVIEW`, `UNAVAILABLE`, `SKIPPED`)
- `reason_codes`, `policy_version`
- `provider_status`, `evaluation_mode`, `simulation_label`
- `requested_at`, `completed_at`, `latency_ms`

`causation_id` on a judgment event is the validation event id. Consumers that ignore unknown `event_type` values are unchanged.

### Commitment (`commitment`, schema `1.1.0`)
- `proposal_id`
- `outcome` (`committed`, `withheld`, `rejected_deterministic`)
- `disposition`
- `reason_codes`
- `judgment_id`

### Human Resolution (`human_resolution`, schema `1.1.0`)
- `proposal_id`
- `decision` (`approve` or `reject`)
- `operator_ref`
- `judgment_id`
- `validation_event_id`
- `committed`
- `deterministic_block`

A human decision does not rewrite the original judgment answers.
