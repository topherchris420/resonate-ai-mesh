# Resonate AI Mesh. Every target works offline and without credentials.

MESH := cargo run --quiet --release -p mesh-lab --bin mesh --
CANONICAL := scenarios/perturbed-mesh.yaml
SCRATCH := $(or $(TMPDIR),/tmp)/resonate-ai-mesh-check

.PHONY: help demo build serve cockpit test test-rust test-python test-web lint golden determinism \
        verify-exports check experiments export audit clean

help: ## List targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*## "}{printf "  %-16s %s\n", $$1, $$2}'

demo: ## Record, explain, replay, and branch the Perturbed Mesh (no keys, no network)
	$(MESH) demo

build: ## Build the mesh binary and the cockpit
	cargo build --release -p mesh-lab
	pnpm install --frozen-lockfile
	pnpm --filter c2-dashboard build

serve: ## Run mesh serve on 127.0.0.1:7878
	$(MESH) serve

cockpit: ## mesh serve plus the cockpit dev server (http://localhost:3000)
	cargo build --release -p mesh-lab
	@trap 'kill 0' INT TERM EXIT; \
	./target/release/mesh serve & \
	pnpm --filter c2-dashboard dev

test: test-rust test-python test-web ## All tests

test-rust:
	cargo test --workspace --all-targets

test-python:
	python3 -m pytest -q

test-web:
	pnpm --filter c2-dashboard test

lint: ## Formatting, clippy, ESLint, and type checks
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	pnpm --filter c2-dashboard lint
	pnpm --filter c2-dashboard typecheck
	pnpm --filter @pordenone/shared-types exec tsc --noEmit -p .

golden: ## Replay the committed golden recordings
	$(MESH) golden verify

determinism: ## The canonical run twice from scratch: identical head hashes
	rm -rf $(SCRATCH) && mkdir -p $(SCRATCH)
	$(MESH) run $(CANONICAL) --seed 42 --out $(SCRATCH)/a --json > $(SCRATCH)/a.json
	$(MESH) run $(CANONICAL) --seed 42 --out $(SCRATCH)/b --json > $(SCRATCH)/b.json
	@python3 -c "import json,sys; a,b=(json.load(open(f'$(SCRATCH)/{n}.json'))['head_hash'] for n in 'ab'); print('head', a); sys.exit(a!=b)"
	cmp $(SCRATCH)/a/events.jsonl $(SCRATCH)/b/events.jsonl
	$(MESH) replay $(SCRATCH)/a

verify-exports: ## Independently verify every committed bundle, and replay the cockpit export
	python3 tools/verify_bundle.py fixtures/golden/*/ apps/c2-dashboard/public/demo/runs/*/
	for run in apps/c2-dashboard/public/demo/runs/*/; do $(MESH) replay $$run || exit 1; done

check: lint test golden determinism verify-exports ## Everything CI runs, except the dependency audit

experiments: ## Every experiment manifest at full repetitions
	for manifest in experiments/*/manifest.yaml; do $(MESH) experiment run $$manifest --force || exit 1; done
	$(MESH) claims check

export: ## Regenerate the cockpit's static export from real runs
	$(MESH) export-web

audit: ## Known vulnerabilities in Rust and production npm dependencies
	cargo audit
	pnpm audit --prod

clean: ## Remove run artifacts and generated code
	rm -rf artifacts gen $(SCRATCH)
