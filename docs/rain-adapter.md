# External agents and the R.A.I.N. adapter (`mesh-agent/1`)

Any deliberative system can join the mesh as an agent by speaking a small line-delimited JSON protocol on stdin and stdout. That includes a R.A.I.N. system, a planner, a learned policy, or a person behind a script. Its proposals are treated exactly like a built-in agent's. It is validated, judged if judgment is on, decided by policy, and recorded. It never talks to the kernel and holds no authority.

Status: the interface is implemented and tested (`ExternalProcessAgent`, `examples/rain_agent_stub.py`, and the `external-agent` scenario and golden fixture). A R.A.I.N. implementation behind it is **proposed**, not included.

## The adapter boundary

Every agent, built in or external, implements `MeshAgentAdapter` (`crates/mesh-lab/src/agents.rs`):

| Method | Direction | Content |
| --- | --- | --- |
| `observe(observation)` | mesh → agent | What its sensor delivered this tick: own position, goal, known hazards, neighbors, quality, timestamp. It may be stale, corrupted, or missing under faults. |
| `propose(context)` | mesh → agent → mesh | The published constraints (max step, min separation, allowed actions). The agent returns an **intent** (action, target, priority, rationale) or nothing. |
| `explain()` | agent → mesh | Free text recorded with the proposal. Never used for authority. |
| `receive_outcome(outcome)` | mesh → agent | What the kernel did: outcome, committed or not, reason codes, committed position |

An intent is not a command. The runner turns it into a proposal with an id and timestamps, and from there the kernel decides.

## Protocol

One JSON object per line. `->` is mesh to agent, `<-` is agent to mesh.

```text
-> {"type":"hello","protocol":"mesh-agent/1","agent_id":"rain_01"}
<- {"protocol":"mesh-agent/1","name":"rain-stub","deterministic":true}
-> {"type":"observe","observation":{"observation_id":"obs-000013","tick":0,"observed_at":1700000000000,
     "own_position":{"x":-55.0,"y":2.0,"z":0.0},"goal":{"x":55.0,"y":5.0,"z":0.0},
     "known_hazards":[{"id":"knoll","center":{"x":0.0,"y":0.0,"z":0.0},"radius":10.0}],
     "neighbors":[...],"quality":"GOOD"}}                                   (no reply)
-> {"type":"propose","context":{"tick":0,"now_ms":1700000000000,
     "constraints":{"max_step":8.0,"min_separation":3.0,"allowed_actions":["MOVE","PATROL","INSPECT","STANDBY","HOLD"]}}}
<- {"intent":{"action_type":"MOVE","target":{"x":-49.1,"y":1.2,"z":0.0},"priority":1,
     "rationale":"toward goal; repelled by knoll (gap 11.2)"},"explanation":"..."}
   or {"intent":null,"explanation":"no usable observation; holding"}
-> {"type":"outcome","outcome":{"proposal_id":"prop-00000-rain_01","outcome":"committed","committed":true,
     "reasons":["ELIGIBLE_PASS"],"committed_position":{"x":-49.1,"y":1.2,"z":0.0}}}   (no reply)
-> {"type":"shutdown"}
```

- The process is started from the scenario's `command`, for example `[python3, examples/rain_agent_stub.py]`. The HTTP API cannot change that command (`docs/security.md`).
- A reply that does not arrive within `timeout_ms` (15 s for the handshake) counts as no proposal. It is recorded as a protocol fault, not silently ignored. A line over 64 KiB ends the connection (every later turn then times out), and stderr is discarded.
- `deterministic` is the agent's own claim. Replay does not rely on it: every turn of an external agent (intent, explanation, faults) is recorded in `intents.jsonl` and substituted on replay, so the run replays exactly **without starting the process**. The `external-agent` golden fixture verifies this in CI with no Python.

## Writing an adapter

1. Copy `examples/rain_agent_stub.py`. It is a deterministic potential-field agent: it is attracted to its goal, repelled by hazards within reach, and more conservative after rejections.
2. Replace `deliberate()` with your reasoner. Keep the protocol.
3. Add an agent with `behavior: external` and your `command` to a scenario, then `mesh run` it.
4. Read `mesh explain <run> <proposal-id>` for your agent's decisions. The kernel will reject what it should, with reasons your agent receives in `outcome.reasons`.

An external agent is untrusted input to the kernel and trusted code on your machine: it runs with your privileges, unsandboxed. Run only agents you trust.
