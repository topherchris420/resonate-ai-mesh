# Development

## Requirements

- Rust 1.88 or newer. CI checks 1.88 and the current stable.
- Node.js 20+ with pnpm 10 (`corepack enable`)
- Python 3.11+ with `pip install -r requirements-dev.txt`
- Optional: Docker, for `docker compose up`

No API keys are needed for anything except the optional TypeSafe Jev judge.

## Layout

```
crates/            Rust workspace: event-bus, epistemic-validator, spatial-state,
                   typed-judgment, kernel-core, telemetry-bridge, mesh-lab (the `mesh` binary)
apps/c2-dashboard  research cockpit (Next.js); public/demo is the committed static export
packages/shared-types  TypeScript mirrors of the recorded contracts
scenarios/         14 scenario files
experiments/       7 experiment manifests
claims/            10 research claims with their evidence checks
fixtures/golden/   4 committed recordings that CI replays (one judged by Jev)
fixtures/canonical/floats.json  cross-language float-formatting contract
schemas/json/, proto/  wire contracts, tested against recordings
services/biometric-pipeline  simulated human-state source for /ingest
tools/verify_bundle.py       independent bundle verifier (Python standard library)
examples/rain_agent_stub.py  reference mesh-agent/1 agent
docker/, docker-compose.yml  container images for mesh, cockpit, pipeline
```

## Everyday commands

```bash
make demo           # the canonical scenario, end to end
make check          # what CI runs: lint, tests, golden replay, determinism, export verification
make cockpit        # mesh serve + cockpit dev server
make experiments    # every manifest at full repetitions, then the claims check
make export         # regenerate apps/c2-dashboard/public/demo from real runs
make audit          # cargo audit + pnpm audit --prod
```

The binary is `target/release/mesh` (`cargo build --release -p mesh-lab`). `pnpm experiment run <manifest>`, `pnpm replay <run>`, and `pnpm mesh <args>` wrap it.

## Tests

| Suite | What it covers |
| --- | --- |
| `cargo test --workspace` | Unit and property tests for every crate, including the canonical JSON and hash chain (with float round-trip properties), the validator's checks and fail-closed behavior, judgment policy and provider failures, the typestate mutation path, statistics, and the server's override and origin guards. `crates/mesh-lab/tests/reproducibility.rs` runs every scenario twice and checks every invariant. It also checks that recorded runs replay exactly with zero network calls, that tampering is detected and blocks replay, that counterfactuals leave the original untouched, that a networked judge is never built without permission and is substituted on replay, that the golden fixtures still replay, that runs stay reproducible under arbitrary configuration overrides (property test), and that a live session with operator commands and an early stop replays exactly. |
| `pytest` | `tests/verifier` (the independent verifier against fixtures, the float contract, and tamper and forgery cases), `tests/contract` (JSON schemas and protobuf definitions against every recorded golden event), `services/biometric-pipeline` |
| `pnpm --filter c2-dashboard test` | The browser's canonical JSON and chain verification, the decision reconstruction (field by field against `decisions.jsonl`), the explain port (against `mesh explain`), and each view rendered from the real static export |

Live network tests are opt-in: `MESH_LIVE_TESTS=1 TYPESAFE_API_KEY=... pytest tests/integration`.

## Changing recorded behavior

A change to the simulator, agents, validator, judges, or policy changes recordings. `mesh golden verify` and the export replay in `make verify-exports` will report the first divergent event. If the change is intended:

```bash
mesh golden update        # regenerate fixtures/golden
make export               # regenerate the cockpit export
make experiments          # regenerate results, then re-read claims check output
```

Commit the regenerated fixtures with the change that caused them, and say in the commit message why the behavior changed.

## Updating dependencies

`fixtures/canonical/floats.json` pins `serde_json`'s float formatting. If a `serde_json` upgrade changes it, the event-bus test fails. Regenerate with `UPDATE_FIXTURES=1 cargo test -p event-bus` only if the new format is correct, and expect every hash in every recording to change.

## Deployment

The cockpit deploys as a static Next.js site (Vercel, `vercel.json`). It reads `public/demo` and needs no backend. For a full local stack:

```bash
MESH_API_TOKEN=$(openssl rand -hex 24) docker compose up --build
```
