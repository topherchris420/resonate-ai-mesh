# Integration Matrix

The nine repositories below are a conceptual map. This repository does not vendor them, and it does not declare git submodules or path dependencies on them. Each idea is reimplemented locally. The "Reimplemented here" column names code in this repository, not an import from the named project.

TypeSafe Jev is separate from that map. It is an optional remote HTTP provider, and it stays disabled unless `JUDGMENT_ENABLED=true`.

| Conceptual source | Idea | Reimplemented here | Local interface |
| :--- | :--- | :--- | :--- |
| **`james_library`** | Agent proposal loop and epistemic checks | `crates/kernel-core` (`KernelEngine`) and `crates/epistemic-validator` (`EpistemicValidator`) | `ActionProposal`, `ValidationResult` |
| **`dynamic-resonance-rooting`** | Adaptive state from load and stability | `services/sim-engine/drr_adapter.py` (`DynamicResonanceRootingAdapter`) | `AdaptiveState` |
| **`ions-x-deep-emergence-lab`** | Multi-agent spatial simulation | `services/sim-engine/emergence_sim.py` (`SwarmEmergenceSimulator`) | `sim_engine.step()` |
| **`waveform-shift-quantum`** | Bounds a proposal must satisfy | Coordinate, freshness, priority, and action checks inside `EpistemicValidator::validate` | `ValidationResult` |
| **`circle`** | Human-state telemetry | `services/biometric-pipeline/biometric_generator.py` (`BiometricPipeline`). Samples are `SIMULATED` unless the process is started in `LIVE` mode | `OperatorStateTelemetry` |
| **`embedded-ai-validation-platform`** | Fault and sensor checks before commit | No hardware adapter is included. Proposals are accepted or rejected by `EpistemicValidator` and published on `crates/event-bus` | `CanonicalEventEnvelope` |
| **`lop-nur-twin`** | 3D spatial scene | `apps/c2-dashboard/src/components/SpatialCanvas.tsx`, a local procedural mesh | `SpatialCanvas` props |
| **`orpheus-resonance-protocol`** | Status display for human state and validation | `OperatorPanel.tsx` and `ValidationInspector.tsx`. These panels are written in this repository | `OperatorStateTelemetry`, `AdaptiveState`, `JudgmentEnvelope` |
| **`cognisync-terrain-weaver`** | Spatial index | `crates/spatial-state` (`SpatialIndex`) | `SpatialEntity` |
| **TypeSafe Jev** | Optional remote typed judgment | `crates/typed-judgment` (`TypeSafeJudgmentProvider`), off by default | `JudgmentEnvelope`, [`typed-judgment.md`](typed-judgment.md) |
