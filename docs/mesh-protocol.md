# Mesh protocol

How components and clients exchange data: the event envelope, the event types, the server's HTTP and WebSocket API, and the ingest gate. The agent protocol (`mesh-agent/1`) is in `docs/rain-adapter.md`.

The wire format is JSON. `schemas/json/` holds the schemas: `canonical-event`, `human-state-datum`, `judgment-envelope`, `decision-record`, and `ingest-event`. `proto/` describes the same messages for generated clients. `tests/contract/` checks both against real recordings.

## Envelope v1.2.0

```json
{"event_id": "evt-000024", "event_type": "validation", "schema_version": "1.2.0",
 "timestamp": 1700000000000, "source": "pordenone.kernel", "subject_id": "patrol_04",
 "correlation_id": "corr-00000-patrol_04", "causation_id": "evt-000023",
 "provenance": "pordenone.validator.v2:obs-000015",
 "run_id": "perturbed-mesh.default.r000.s42", "experiment_id": "perturbed-mesh",
 "seq": 17, "tick": 0, "mode": "SIMULATED", "policy_version": "pordenone.validator.v2",
 "software_version": "resonate-ai-mesh/0.2.0", "ai_involved": false,
 "payload": {"accepted": true, "checks": [...], "reasons": ["ALL_CHECKS_PASSED"], ...},
 "prev_hash": "sha256:...", "hash": "sha256:..."}
```

`prev_hash` and `hash` are present on recorded events (see `docs/provenance.md`). Live WebSocket events are delivered before hashing and do not carry them. Version 1.0 envelopes, with the payload as a JSON string in `payload_json`, are still accepted on ingest and converted.

## Event types

| Type | Source | Meaning |
| --- | --- | --- |
| `run_started` | `mesh.lab` | Configuration hash, seed, agents, judge descriptor, substitutions |
| `state_transition` | `pordenone.kernel` | The only record of a state change: `register_agent` and `declare_hazard` at setup, then `commit`. Carries `revision`, `before_hash`, `after_hash`, and for commits `authorized_by` (the validation, commitment, and any human-resolution event ids). |
| `environment_change` | `environment.*` | A hazard appeared or retired |
| `human_state` | `sim.*` or an ingest source | One `HumanStateDatum` |
| `adaptive_level` | `pordenone.kernel` | Operator level changed (NORMAL, ELEVATED, HIGH, CRITICAL), with the signal that caused it |
| `observation` | `sensor.<agent>` | What an agent's sensor delivered, including any fault effect |
| `proposal` | the agent | Action, target, priority, source observation, rationale (recorded, never used for authority) |
| `validation` | `pordenone.kernel` | Every check with status, measured value, and limit, plus reasons and the deterministic confidence |
| `judgment` | `pordenone.kernel` | A judgment envelope (`docs/jev.md`) |
| `commitment` | `pordenone.kernel` | Policy outcome (`committed`, `withheld`, `awaiting_human_review`, `rejected_deterministic`), basis, reason codes, operator level |
| `human_resolution` | `pordenone.kernel` | A person's (or the simulated reviewer's) decision on a withheld proposal |
| `operator_command` | `operator_console` | A command from a live console: fault injection, operator proposal, or review decision |
| `fault_injected`, `fault_effect`, `fault_cleared` | `mesh.lab` | Fault lifecycle and each concrete effect |
| `resonance_sample` | `mesh.lab` | Rolling commit and rejection rates and the operator level (timeline only) |
| `run_completed` | `mesh.lab` | Termination reason (`max_ticks`, `all_goals_reached`, `stopped_by_operator`), ticks run, final state hash, pending reviews |

`correlation_id` groups the events of one decision, from proposal to commit, including a later human resolution. `causation_id` names the direct cause. `mesh explain` and the cockpit follow both.

## HTTP API (`mesh serve`)

All `/api` routes require the token when `MESH_API_TOKEN` is set (`docs/security.md`).

| Method and path | Purpose |
| --- | --- |
| `GET /health` | Liveness, version, uptime, live-session status, event-bus queue depth (unauthenticated) |
| `GET /metrics` | Prometheus text: events published and streamed, dropped WebSocket events, queue depth, ingest accepted and rejected by reason, runs, replays, replay divergences, live ticks, last-run throughput and p50/p95 pipeline latency (unauthenticated) |
| `GET /api/capabilities` | What this installation can do now, probed |
| `GET /api/scenarios`, `GET /api/experiments` | Scenario and manifest summaries |
| `GET /api/experiments/:id/summary`, `GET /api/experiments/:id/files/:file` | Results, `report.md`, `runs.csv` |
| `POST /api/experiments/:id/run` `{"repetitions": n}` | Run an experiment (1–1000 repetitions) |
| `GET /api/topology?scenario=id` | The typed component graph |
| `GET /api/claims` | Claims checked against current evidence |
| `GET /api/metrics/definitions` | Every metric's unit and definition |
| `GET /api/runs`; `POST /api/runs` `{"scenario", "seed", "set"}` | List or record runs |
| `GET /api/runs/:id/files/:file` | Allow-listed bundle files |
| `POST /api/runs/:id/replay` | Re-execute and compare; returns the replay report |
| `POST /api/runs/:id/counterfactual` `{"set", "label"}` | Branch a run; returns the comparison |
| `GET /api/runs/:id/explain/:target` | Causal chain for a proposal, correlation, or event |
| `GET /api/live`; `POST /api/live/start` `{"scenario", "seed", "tick_ms", "human_state": "simulated"\|"ingest"}`; `POST /api/live/stop` | Live sessions (one at a time). The start response includes the resolved scenario for drawing. |
| `POST /api/live/command` | `{"command": "inject_fault", "fault": {...}}`, `{"command": "operator_proposal", "agent_id", "intent"}`, or `{"command": "human_decision", "proposal_id", "approve", "note"}`; applied at the next tick and recorded |

Errors are `{"error": "..."}` with a meaningful status (400 malformed, 401 token, 403 forbidden override or origin, 404 unknown, 409 a live session is already running, 415 not JSON).

## WebSockets

- `GET /ws` streams `{"type": "event", "event": <envelope>}` for every event published during live sessions, and `{"type": "lagged", "skipped": n}` when a slow client falls behind. The recording is unaffected. The stream is read-only.
- `GET /ingest` takes one envelope per message and replies `{"accepted": true, "event_id"}` or `{"accepted": false, "code", "error"}`. Limits: 64 KiB per message, 50 messages per second per connection, `human_state` and `observation` only, and `LIVE` only from registered sources.

Both refuse browser origins outside `--allow-origin`.

## Logging

`mesh` writes structured logs to stderr through `tracing`. The filter comes from `MESH_LOG` (default `error`; for example `MESH_LOG=info`), and `MESH_LOG_FORMAT=json` switches to JSON lines. Operational metrics are on `/metrics`.
