# Architecture Overview: Pordenone

Pordenone is a unified cognitive cyber-physical command-and-control (C2) research platform integrating 8 domain-specific repositories behind clean, versioned interfaces.

## Conceptual Pipeline

```mermaid
graph TD
    A[Human / Sensor Inputs] --> B[CIRCLE Operator-State Layer]
    B --> C[Telemetry Normalization]
    C --> D[NEXUS Event Bus]
    D --> E[Pordenone Kernel Engine]
    E --> F[Deterministic Epistemic Validation]
    F -->|fail| R[Reject and publish]
    F -->|pass| TJ[Typed Judgment Fabric]
    TJ --> P[Deterministic Judgment Policy]
    P -->|PASS or judgment disabled| G[Commit authoritative state]
    P -->|REVISE / HUMAN_REVIEW / UNAVAILABLE| W[Withhold commit]
    G --> H[DRR Adaptive Dynamics and spatial state]
    W --> H
    H --> J[C2 Dashboard]
```

## Core Components

1. **Kernel Engine (`crates/kernel-core`)**
   - Implements `Observe -> Propose -> Validate -> Judge -> Policy -> Commit or withhold -> Publish`.
   - Ensures no agent proposal mutates authoritative state until epistemic validation succeeds.
   - Calls typed judgment only after deterministic validation passes, then applies `JudgmentPolicy` before commit.
   - Manages adaptive policy level adjustments (`NORMAL`, `ELEVATED`, `HIGH`, `CRITICAL`).

2. **Epistemic Validator (`crates/epistemic-validator`)**
   - Evaluates proposals for coordinate feasibility, non-contradiction, state consistency, and provenance.
   - Returns structured `ValidationResult` explaining accepted vs rejected proposals.
   - Remains the deterministic gate. Typed judgment does not replace it.

3. **Typed Judgment (`crates/typed-judgment`)**
   - Provider-neutral `JudgmentProvider` with disabled, deterministic mock, TypeSafe, and recorded-replay implementations.
   - See [`typed-judgment.md`](typed-judgment.md).

4. **Event Bus (`crates/event-bus`)**
   - Asynchronous in-memory event bus providing fan-out broadcast, correlation tracking, and structured tracing.

5. **Biometric Telemetry Pipeline (`services/biometric-pipeline`)**
   - Adapts CIRCLE physiological models.
   - Labels all data as `SIMULATED` vs `LIVE`.

6. **Simulation Engine (`services/sim-engine`)**
   - Adapts `ions-x-deep-emergence-lab` swarm dynamics and `dynamic-resonance-rooting` (DRR) adaptive state calculations.

7. **Spatial State (`crates/spatial-state`)**
   - Canonical 3D spatial index adapted from `lop-nur-twin` and `cognisync-terrain-weaver`.

8. **C2 Dashboard (`apps/c2-dashboard`)**
   - Next.js application with React Three Fiber 3D spatial canvas, real-time operator panel, agent panel, event feed, and validation inspector.
