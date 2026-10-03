# Security, privacy, and threat model

Resonate AI Mesh is research software that runs on a researcher's machine. It controls no hardware. Its security goals are narrow and specific:

1. **Authority.** Nothing except the kernel's single mutation path changes authoritative state: not an agent, a judge, a network client, or a recording.
2. **Integrity of evidence.** A recording that was altered after the fact is detected, and replay cannot be steered by the network.
3. **Honest labeling.** Simulated data is never presented as live, and a person's data is minimized and never sent anywhere by default.
4. **Safe local service.** `mesh serve` cannot be used from another website, and cannot be used from the network without a token.

## Assets and trust boundaries

| Boundary | Trusted side | Untrusted side | Control |
| --- | --- | --- | --- |
| Agent → kernel | validator, policy, state | every agent, built-in or external | Agents return intents only. Every intent passes 11 deterministic checks, and only `policy::authorize` can produce a `CommitAuthorization`. |
| Judge → kernel | policy | judge output, including a remote model | A judge returns an advisory envelope and holds no handle to state. Judgment is consulted only after validation passes. Failures (`UNAVAILABLE`) withhold and never commit. |
| Person → kernel | policy | approvals | An approval is re-validated against current state before it commits. |
| Process → `/ingest` | kernel | any WebSocket client | Admits only `human_state` and `observation` events. Schema-checked, 64 KiB per message, 50 messages per second per connection. `LIVE` only from sources in `MESH_LIVE_SOURCES`. The datum's mode must equal the envelope's. |
| Network → `mesh serve` | server | browsers, local processes, remote clients | See below. |
| Recording → reader | the verifier | the bundle file | Hash chain with genesis = hash of the configuration, file digests, and three independent verifiers (`docs/provenance.md`) |
| Kernel → TypeSafe | kernel | the remote API | Opt-in per run (`--allow-network`). Minimized evidence package. Typed failures. The key is never logged. |
| External agent process | the runner | the process | Line-delimited JSON, 64 KiB line limit, per-reply timeout. The process gets no state handle and its stderr is discarded. **It runs with your user's privileges and is not sandboxed.** |

## `mesh serve`

| Control | Default |
| --- | --- |
| Bind address | `127.0.0.1:7878`. A non-loopback bind is refused unless `MESH_API_TOKEN` is set. |
| Authentication | With a token, every `/api`, `/ws`, and `/ingest` request needs `Authorization: Bearer <token>` (or `?token=` for browser WebSockets). Comparison is constant-time. `/health` and `/metrics` are open and expose no credentials or data. |
| Cross-origin HTTP | CORS allows only `--allow-origin` (default `http://localhost:3000` and `http://127.0.0.1:3000`). JSON bodies are required, so a cross-site "simple" request is rejected with 415 before any handler runs. |
| Cross-origin WebSocket | CORS does not cover WebSocket upgrades. `/ws` and `/ingest` refuse any browser `Origin` outside the allow-list. |
| Configuration overrides | `POST /api/runs`, `/api/live/start`, and `/api/runs/:id/counterfactual` accept overrides, but the resolved scenario must not change which program any agent runs, and must not point a recorded judge at a file (403). |
| Paths | Run ids are validated before any path is built. Only allow-listed bundle and experiment files are served. Scenarios and manifests are addressed by id within the repository. |
| Size and rate | 256 KiB request bodies, 4 KiB client messages on `/ws`, ingest limits as above. One live session at a time. Experiment repetitions are capped at 1000. |
| Network from runs | API-started runs never get `allow_network`, so a networked judge cannot be reached through the API. |

**Residual risks.** Any local process can use a loopback server without a token. Set `MESH_API_TOKEN` on shared machines. An authenticated client can start CPU-heavy experiments. A token passed as `?token=` can appear in proxy logs; prefer the header.

## Issues found and fixed in this audit

| Finding | Impact | Fix |
| --- | --- | --- |
| API overrides could redefine an agent as an external process with any command | Command execution for any client able to reach the API | The resolved scenario is compared with its source, and any change to a process-launching agent is refused. Unit-tested against every override spelling, and probed against a live server. |
| WebSocket upgrades were not origin-checked | Any website could read live events or inject simulated human-state data into a loopback server | `Origin` allow-list on `/ws` and `/ingest`, with a unit test and a live probe |
| `/health` reported whether judgment credentials were present | Minor disclosure on token-protected deployments | Removed. The authenticated capabilities endpoint still reports it. |
| Cockpit on Next.js 14 with 27 advisories (2 critical) | The deployed page used none of the affected features, but it shipped known-vulnerable code | Next 15.5.27, React 19, postcss 8.5.28, vitest 4 |
| A static export described the exporting machine (credential presence, absolute paths) | Disclosure on a public page | The export now describes itself. A test asserts that no `/home/` or `/tmp/` paths and no credential status reach it. |
| Human-state signal loss silently disabled gating (`hold_last`) | Fail-open behavior under a sensor fault | The default is now `assume_high` (fail closed). The fault-injection experiment measures both behaviors. |

## Dependency audit (at the time of writing)

- `cargo audit`: 0 vulnerabilities and 0 warnings across 242 crates.
- `pnpm audit --prod`: no known vulnerabilities.
- `pnpm audit` including dev dependencies: one advisory, `braces` (stack exhaustion on adversarial glob patterns) via `eslint-config-next` → `fast-glob`. No patched release exists. It affects only lint runs over untrusted patterns. Accepted.

CI runs `cargo audit` and `pnpm audit --prod` on every push.

## Secrets

- `TYPESAFE_API_KEY` is read from the environment only when a TypeSafe judge is built, and never logged. Errors omit the key and the response body. The code tests check that the key never appears in judge errors or envelopes.
- Bundles, exports, fixtures, and logs were scanned for the key during this audit: 795 files, 0 occurrences.
- Nothing in CI needs credentials. The live TypeSafe test runs only with `MESH_LIVE_TESTS=1`.

## Privacy (human-state data)

- All human-state data in this repository is SIMULATED, produced by `sim.operator_load.v1` or the Python pipeline, and labeled as such on every event and datum.
- A remote judge never receives raw physiological signals, identities, names, credentials, transcripts, or parameter blobs. The evidence package carries a few derived indices from an allow-list, and lists the names of the fields it excluded. See `docs/jev.md` and `docs/human-state.md`.
- No telemetry leaves the machine unless a run explicitly enables a networked judge.

## Out of scope

Physical actuation (not implemented), multi-user access control (one token, one operator), signed bundles (not implemented; hashes prove internal consistency, not authorship), and sandboxing of external agent processes. Run only agents you trust.
