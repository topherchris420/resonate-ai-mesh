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

# Run Frontend Dashboard
pnpm --filter c2-dashboard dev
```

### Containerized Environment

Launch all services using Docker Compose:
```bash
docker compose up --build
```
