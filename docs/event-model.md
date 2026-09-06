# Event Model & Canonical Schemas

All cross-service communications in Pordenone utilize the `CanonicalEventEnvelope` schema.

## Canonical Event Envelope Schema

```json
{
  "event_id": "UUID string",
  "event_type": "telemetry | proposal | validation | spatial | adaptive_state | custom",
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
- `confidence` (0.0 - 1.0)
- `reasons` (list of strings)
- `provenance`
