# Typed Judgment Fabric

Pordenone separates measurement, deterministic constraint checking, and bounded probabilistic judgment.

Typed judgment does not replace the epistemic validator, DRR, simulation, or operator policy. It answers a fixed set of atomic questions about evidence that has already passed deterministic checks. Pordenone code decides what those answers are allowed to do.

Remote judgment is off unless `JUDGMENT_ENABLED=true`. The platform remains fully usable without TypeSafe.

## Pipeline

```
SENSORS / AGENTS / SIMULATIONS
             ↓
      CANONICAL EVENTS
             ↓
        PORDENONE KERNEL
             ↓
   DETERMINISTIC VALIDATION
             ↓
       TYPED JUDGMENT
             ↓
    DETERMINISTIC POLICY
             ↓
       HUMAN REVIEW?
          ↙       ↘
        YES        NO
         ↓          ↓
      WITHHOLD    COMMIT
          \        /
           ↓      ↓
         EVENT BUS
             ↓
        C2 DASHBOARD
```

In Vers3Dynamics language: observe, relate, judge, adapt, observe again.

The kernel path is:

1. Observe a proposal and the minimum derived context.
2. Run `EpistemicValidator` first.
3. If deterministic validation fails, reject immediately. No provider is called.
4. If it passes, build a canonical evidence package and evaluate the versioned question set.
5. Apply `JudgmentPolicy`. The model does not select `PASS`, `REVISE`, `HUMAN_REVIEW`, or `UNAVAILABLE`.
6. Commit authoritative state only for `PASS`, or for `SKIPPED` when judgment is disabled.
7. Publish validation, judgment, and commitment events. A later human decision publishes `human_resolution`.

Only `KernelEngine` commits `agent_states` and the spatial index. A `JudgmentProvider` receives an owned evidence package and returns an envelope. It has no write handle to state, the event bus, configuration, the filesystem, the shell, or actuators.

## What judgment can and cannot establish

Deterministic truth stays in Pordenone code:

- coordinate bounds
- action allowlist
- stale timestamps
- priority constraints
- schema and state consistency

Bounded judgment is appropriate for questions such as:

- how well the supplied evidence supports the proposal
- how strong that evidence is
- whether the evidence meaningfully contradicts the proposal
- whether the proposal goes beyond the observations
- whether the ambiguity warrants human review

Judgment cannot establish that a deterministic constraint passed, choose a policy, or authorize actuation. Simulated research actions remain `MOVE`, `PATROL`, `INSPECT`, `STANDBY`, and `HOLD`.

## Question set `pordenone.judgment.questions.v1`

All five questions are sent together against the same immutable state. They do not refer to each other and they do not instruct the model to control the system.

| ID | Primitive | Question |
| --- | --- | --- |
| `proposal_support` | Choice | Given only the supplied evidence, how well is this proposal supported? Options: `supported`, `mixed`, `unsupported`, `insufficient_evidence`. |
| `evidence_quality` | Score | How strong is the supplied evidence for evaluating this proposal? Levels 0–3: insufficient, weak or indirect, adequate but limited, strong and directly relevant. |
| `contradiction_present` | Noul | The supplied evidence meaningfully contradicts the proposal. |
| `scope_violation` | Noul | The proposal goes materially beyond what the supplied observations establish. |
| `human_review` | Noul | The ambiguity, uncertainty, or potential consequence warrants explicit human review before state commitment. |

Choice and Score answers carry a probability distribution and a confidence value derived from that distribution. Noul answers are a single probability from 0 to 1 and have no confidence. Pordenone does not invent one.

## Evidence package `pordenone.judgment.state.v1`

The serializer builds the minimum package:

- proposal id, agent id, action type, target, priority
- bounded observation id, not an unstructured transcript
- deterministic feasibility, constraints checked, contradictions, and deterministic confidence
- optional DRR stability, resonance, and confidence
- derived operator features only: `cognitive_load`, `signal_quality`, `baseline_delta`, `cross_signal_coherence`, `state_confidence`, `is_simulated`
- derived spatial target and bounds flag
- explicit limitations, truncation, and simulation status
- correlation, causation, and schema versions

The canonical JSON is hashed as `sha256:<hex>`. The same logical state always produces the same hash. Truncation is recorded on the state and on the envelope. An incomplete package is never described as complete.

### Privacy boundary

Remote calls are opt-in. The evidence package excludes:

- raw EEG, PPG, EDA, and waveform buffers
- participant identity and names
- credentials, secrets, and configuration
- unrelated telemetry history, private memory, and transcripts
- proposal parameter blobs

Excluded field names may be listed. Their values are not sent. Operator `confidence` from existing telemetry is mapped to `state_confidence`. Simulated inputs are labeled `SIMULATED`, including when a live flag conflicts with a simulated marker.

TypeSafe states that Jev is not trained on customer requests. Zero data retention is an enterprise option, not the default. See the TypeSafe legal documents. Pordenone still minimizes what it sends.

## Policy `pordenone.judgment.policy.v1`

Thresholds live in `JudgmentPolicy`. Noul comparisons are strictly above the threshold.

| Condition | Disposition |
| --- | --- |
| Judgment disabled by kernel configuration | `SKIPPED` (deterministic pass may commit) |
| Provider timeout, auth failure, rate limit, malformed response, missing credentials, or state error | `UNAVAILABLE` |
| `proposal_support` is `unsupported`, `insufficient_evidence`, or `mixed` | `REVISE` |
| Evidence score below 2 | `REVISE` |
| `scope_violation` above 0.50 | `REVISE` |
| `contradiction_present` above 0.50 and at or below 0.75 | `REVISE` |
| `contradiction_present` above 0.75 | `HUMAN_REVIEW` |
| `human_review` above 0.50 | `HUMAN_REVIEW` |
| Choice or Score confidence missing or below `minimum_confidence` (default 0.70) | `HUMAN_REVIEW` |
| Supported, adequate evidence, nouls at or below thresholds, confidence at or above the minimum | `PASS` |

When several rules match, the stricter disposition wins and every reason code is kept. `UNAVAILABLE` and `HUMAN_REVIEW` are not rewritten into `PASS`. A provider that reports itself disabled during a live evaluation is treated as an invalid request, not as permission to commit.

`HUMAN_REVIEW` withholds the commit, keeps the proposal and envelope, and waits for `resolve_human_review`. Approval runs deterministic validation again. A failed recheck still does not commit. Rejection records `HUMAN_DECISION_REJECT`. Neither path changes the original model answers.

## Providers

`JudgmentProvider::evaluate(request) -> envelope` is the only integration surface.

| Provider | Role |
| --- | --- |
| `DisabledJudgmentProvider` | No remote call. Kernel disabled mode commits on deterministic pass. |
| `DeterministicMockJudgmentProvider` | Scripted answers for tests and offline comparison. |
| `TypeSafeJudgmentProvider` | `POST https://api.typesafe.ai/v1/systemone` using the current System One JSON shape. |
| `RecordedJudgmentProvider` | Replay. Returns the stored envelope and does not open a socket. |

Future local models or other System One models can implement the same trait. `compare_envelopes` records agreement per question. Disagreement is kept. It is not averaged.

TypeSafe request model alias is `jev-latest` unless `JUDGMENT_MODEL` or `TYPESAFE_DEFAULT_MODEL` is set. The response `model` field, currently a version such as `jev-1.13.0`, is stored as `provider_model_version`. The client does not retry a failure into success. Timeouts, DNS failures, `401`, `403`, `422`, `429`, `529`, and `5xx` become typed provider statuses. Error logs omit the API key and the response body.

## Events

Judgment does not change the required canonical envelope fields. New event types use schema `1.1.0`. Existing validation events stay on `1.0.0`.

```
observation → proposal → validation → judgment → commitment
```

Human review adds `human_resolution` before a later commitment. `causation_id` points at the previous event in that chain. `correlation_id` stays on the proposal.

`evaluation_mode` is `LIVE`, `RECORDED_JUDGMENT`, `LIVE_REEVALUATION`, or `DISABLED`.

## Replay

Session replay replays the recorded envelope and sets the copy's mode to `RECORDED_JUDGMENT`. It does not call TypeSafe and it does not overwrite the stored answers, disposition, or model version. A later live reevaluation is a separate comparison artifact with `drift_detected` when the primary answers differ.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `JUDGMENT_ENABLED` | `false` | Opt in to typed judgment. |
| `JUDGMENT_PROVIDER` | `disabled` | `disabled`, `mock`, or `typesafe`. |
| `JUDGMENT_MODEL` | `jev-latest` | Requested model alias. |
| `JUDGMENT_TIMEOUT_MS` | `10000` | Per-request timeout. |
| `JUDGMENT_MINIMUM_CONFIDENCE` | `0.70` | Choice/Score confidence floor. |
| `JUDGMENT_POLICY_VERSION` | `pordenone.judgment.policy.v1` | Policy stamp stored on envelopes. |
| `TYPESAFE_API_KEY` | unset | Read only when the provider is `typesafe`. |
| `TYPESAFE_BASE_URL` | `https://api.typesafe.ai` | API root. The path is `/v1/systemone`. |

Example:

```bash
JUDGMENT_ENABLED=true \
JUDGMENT_PROVIDER=typesafe \
JUDGMENT_MODEL=jev-latest \
TYPESAFE_API_KEY=... \
cargo run -p kernel-core
```

If judgment is enabled and the key or provider is missing, proposals that pass deterministic validation are withheld with `UNAVAILABLE`. They are not silently accepted.

A manual live check, not run in CI:

```bash
TYPESAFE_API_KEY=... cargo test -p typed-judgment --test live_typesafe -- --ignored
TYPESAFE_API_KEY=... pytest tests/integration/test_typesafe_live.py
```

## Dashboard

The validation inspector keeps three separate readings:

- deterministic checks and deterministic confidence
- typed answers, with Choice/Score confidence labeled separately from Noul probabilities
- the policy disposition

Disabled judgment shows an explicit off state instead of fabricated probabilities. `HUMAN_REVIEW` is a review state, not an error banner. Approve and reject record a human-resolution event and do not rewrite the model disposition.

## Failure behavior

Failures are typed, logged without secrets, and withheld. The kernel does not panic and does not fall back to `PASS` when a provider fails.
