# Developer Guide

## Environment Setup

### Local Prerequisites
- Rust 1.80+ (`cargo`)
- Node.js 20+ & `pnpm`
- Python 3.11+
- Protobuf Compiler (`protoc`)

### Building and Testing

```bash
# Install Node dependencies
pnpm install

# Build TypeScript types
pnpm --filter @pordenone/shared-types run build

# Build Rust workspace
cargo build --workspace

# Run Rust tests
cargo test --workspace

# Run Python tests
PYTHONPATH=services/biometric-pipeline:services/sim-engine pytest services/ tests/

# Run dashboard tests (session model, panels, and the judgment fixture)
pnpm --filter c2-dashboard test

# Run Frontend Dashboard
pnpm --filter c2-dashboard dev
```

### Typed judgment

Remote judgment is off by default. CI does not need `TYPESAFE_API_KEY`.

```bash
# Optional live provider
export JUDGMENT_ENABLED=true
export JUDGMENT_PROVIDER=typesafe
export JUDGMENT_MODEL=jev-latest
export TYPESAFE_API_KEY=...
```

`JUDGMENT_TIMEOUT_MS` defaults to `10000`. `JUDGMENT_MINIMUM_CONFIDENCE` defaults to `0.70`. See [`typed-judgment.md`](typed-judgment.md).

### Containerized Environment

Launch all services using Docker Compose:
```bash
docker compose up --build
```
