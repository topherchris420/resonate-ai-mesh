# Pordenone: Unified Cognitive Cyber-Physical Command-and-Control Research Platform

Pordenone is a research monorepo integrating capabilities across 9 specialized domain repositories into a unified cognitive cyber-physical command-and-control (C2) architecture.

## Conceptual Pipeline

```
Human / Sensor Inputs
        ↓
CIRCLE Operator-State Layer
        ↓
Telemetry Normalization
        ↓
NEXUS Event Bus
        ↓
Pordenone Kernel
        ↓
Epistemic Validation (Observe → Propose → Validate → Commit → Publish)
        ↓
DRR Adaptive Dynamics
        ↓
Agent / Simulation Actions
        ↓
Spatial World State
        ↓
C2 Dashboard
```

## Repository Map & Architecture Matrix

See [`docs/integration-matrix.md`](docs/integration-matrix.md) for full mapping details:
- **Agent / Epistemic Kernel**: `james_library`
- **Adaptive Dynamics**: `dynamic-resonance-rooting`
- **Emergent Multi-Agent Simulation**: `ions-x-deep-emergence-lab`
- **Physical / Falsifiable Simulation**: `waveform-shift-quantum`
- **Human-State Sensing**: `circle`
- **Embedded AI Validation**: `embedded-ai-validation-platform`
- **3D GEOINT / Digital Twin**: `lop-nur-twin`
- **Tactical HUD / Telemetry**: `orpheus-resonance-protocol`
- **Spatial Sync**: `cognisync-terrain-weaver`

## Quickstart

### Prerequisites
- Rust 1.80+ (`cargo`)
- Node.js 20+ & `pnpm`
- Python 3.11+
- Protobuf Compiler (`protoc`)

### Development Commands
```bash
# Install Node dependencies
pnpm install

# Generate Protobuf bindings
npm run proto:generate

# Build Rust workspace
cargo build --workspace

# Run Rust tests
cargo test --workspace

# Run Python services tests
pytest services/ tests/

# Run frontend/dashboard dev server
pnpm dev
```

### Docker Compose
To launch the full containerized environment:
```bash
docker compose up --build
```

## Security & Safety Boundaries
- Defaults to **SIMULATION** mode and local execution.
- Unvalidated agent actions CANNOT mutate authoritative state.
- Simulated physiological telemetry is explicitly labeled as `SIMULATED`.
- No real-world autonomous weapon or irreversible physical actuation is implemented.
