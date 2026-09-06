# Telemetry & IPC Protocols

Pordenone supports dual-layer communication: Protobuf high-performance IPC and WebSocket/JSON streaming.

## Interfaces

1. **Protobuf Contracts (`proto/`)**
   - `telemetry.proto`: Operator telemetry and adaptive state schemas.
   - `agent.proto`: Agent identity, state, vector positions, proposals.
   - `spatial.proto`: 3D spatial entity state and terrain sync.
   - `events.proto`: Envelope wrapper.

2. **WebSocket Telemetry Bridge (`crates/telemetry-bridge`)**
   - Port `8080` (`ws://0.0.0.0:8080/ws`)
   - Bi-directional JSON streaming of `CanonicalEventEnvelope`.
   - Used by `apps/c2-dashboard` and `services/biometric-pipeline`.

3. **Simulation Engine REST API (`services/sim-engine`)**
   - Port `8000` (`http://0.0.0.0:8000`)
   - Endpoints: `/health`, `/state`, `/step`, `/scenario/load`, `/drr/evaluate`.
