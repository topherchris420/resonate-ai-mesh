# Integration matrix

Several earlier Vers3Dynamics projects contributed ideas to this one. None of them is vendored, imported, or a submodule. Each idea is reimplemented here, and this table says where and how far it got. TypeSafe Jev is the only external service, and it is optional.

| Source idea | What it became here | Where | Status |
| --- | --- | --- | --- |
| `james_library`: agent proposal loop with epistemic checks | The Pordenone kernel: proposals, eleven deterministic checks, the single mutation path | `crates/kernel-core`, `crates/epistemic-validator` | implemented |
| `waveform-shift-quantum`: bounds a proposal must satisfy | Coordinate bounds, step size, hazard clearance (segment–circle), separation, freshness, clock skew | `epistemic-validator` | implemented |
| `embedded-ai-validation-platform`: fault and sensor checks before commit | 14 injectable fault kinds, fail-closed validation, the fault-injection experiment | `mesh-lab` (`sim.rs`, `experiments/fault-injection`) | implemented (simulated faults) |
| `ions-x-deep-emergence-lab`: multi-agent emergence | Deterministic multi-agent simulator with deliberately different behaviors; coordination measured by the resonance vector | `mesh-lab` (`sim.rs`, `agents.rs`, `metrics.rs`) | implemented; resonance vector experimental |
| `dynamic-resonance-rooting` (DRR): adaptive state from load | Operator-load levels and gating with fail-closed signal loss. The old "resonance" score had no operational definition and was removed. | `kernel-core/src/policy.rs` | implemented |
| `circle`: human-state telemetry | `HumanStateDatum` with mode and provenance; simulated sources; the hardened ingest gate | `event-bus/src/human_state.rs`, `telemetry-bridge`, `services/biometric-pipeline` | implemented (SIMULATED only) |
| `cognisync-terrain-weaver`: spatial index | Ordered spatial index used for observations and separation | `crates/spatial-state` | implemented (2D arena) |
| `lop-nur-twin`: spatial scene | The cockpit's 2D arena, drawn only from recorded events. The 3D procedural scene was removed: it showed invented terrain. | `apps/c2-dashboard/src/components/Arena.tsx` | implemented |
| `orpheus-resonance-protocol`: status display for human state and validation | The cockpit's decision inspector, causal timeline, and evidence views | `apps/c2-dashboard` | implemented |
| R.A.I.N.: deliberative reasoning | The `mesh-agent/1` adapter boundary and a reference agent | `agents.rs` (`ExternalProcessAgent`), `examples/rain_agent_stub.py` | interface implemented; R.A.I.N. proposed |
| TypeSafe Jev: typed judgment | Optional bounded judge with typed questions, minimized evidence, and recorded replay | `crates/typed-judgment` | optional; off unless a run asks for it |

Retired names: CIRCLE (the simulated human-state source), NEXUS (the event bus), DRR (operator-load gating).
