// TypeScript mirrors of the mesh's recorded contracts. The JSON schemas in
// schemas/json/ are authoritative; tests/contract/ checks them against real
// recordings.

export type {
  EvaluationMode,
  JudgmentAnswer,
  JudgmentDisposition,
  JudgmentEnvelope,
  JudgmentPrimitive,
  JudgmentProviderStatus,
} from "./judgment";
export { disabledJudgmentEnvelope, isJudgmentEnvelope } from "./judgment";

/** Where a value came from. Unlabeled data defaults to SIMULATED. */
export type DataMode = "SIMULATED" | "LIVE" | "REPLAY";

export type SignalQuality = "GOOD" | "DEGRADED" | "INVALID" | "MISSING";

/** Event envelope v1.2.0 (schemas/json/canonical-event.json). */
export interface EventEnvelope {
  event_id: string;
  event_type: string;
  schema_version: string;
  /** Milliseconds; logical simulation time in experiments. */
  timestamp: number;
  source: string;
  subject_id: string;
  correlation_id: string;
  /** event_id of the direct cause. */
  causation_id: string;
  provenance: string;
  run_id: string;
  experiment_id: string;
  seq: number;
  tick: number | null;
  mode: DataMode;
  policy_version: string;
  software_version: string;
  /** A probabilistic or remote model produced or influenced this event. */
  ai_involved: boolean;
  payload: unknown;
}

/** A recorded event with its hash-chain links. */
export interface ChainedEvent extends EventEnvelope {
  prev_hash: string;
  hash: string;
}

/** schemas/json/human-state-datum.json. An operational index, not a clinical measure. */
export interface HumanStateDatum {
  metric: string;
  value: number;
  unit: string;
  timestamp: number;
  source: string;
  mode: DataMode;
  original_mode?: DataMode | null;
  confidence: number;
  quality: SignalQuality;
}

export interface CheckOutcome {
  check: string;
  status: "PASS" | "FAIL" | "SKIPPED" | "DISABLED";
  detail?: string;
  measured?: number;
  limit?: number;
}

/** Payload of a `validation` event. */
export interface ValidationPayload {
  proposal_id: string;
  agent_id: string;
  accepted: boolean;
  feasibility: number;
  contradictions: string[];
  /** Deterministic confidence, not a model output. */
  confidence: number;
  reasons: string[];
  provenance: string;
  timestamp: number;
  checks: CheckOutcome[];
  config_hash: string;
  observation_age_ms: number;
  validator_version: string;
}

/** x, y, z; a non-finite coordinate is recorded as null. */
export type Coords = [number | null, number | null, number | null];

/** One line of decisions.jsonl (schemas/json/decision-record.json). */
export interface DecisionRecord {
  tick: number;
  kind: "proposal" | "human_resolution";
  proposal_id: string;
  agent_id: string;
  correlation_id: string;
  action_type: string;
  target: Coords;
  from: Coords;
  priority: number;
  observation_id: string | null;
  observed_at: number | null;
  observation_age_ms: number;
  observation_quality: string;
  trigger_event_id: string;
  validation: {
    event_id: string;
    accepted: boolean;
    reasons: string[];
    failed_checks: string[];
    confidence: number;
  };
  judgment: {
    event_id: string;
    judgment_id: string;
    provider: string;
    model: string;
    disposition: string;
    reason_codes: string[];
    provider_status: string;
    latency_ms: number;
    model_involved: boolean;
    min_confidence: number | null;
  } | null;
  judgment_skip_reason: string | null;
  policy: {
    event_id: string;
    outcome: string;
    reason_codes: string[];
    basis: string | null;
    adaptive_level: string;
  };
  transition: {
    event_id: string;
    revision: number;
    before_hash: string;
    after_hash: string;
  } | null;
  committed: boolean;
  /** Ground truth from the independent geometric oracle; null = safe. */
  oracle_unsafe: string | null;
  rationale: string | null;
  duplicate_submission: boolean;
}

/** Events an external producer may send to /ingest. */
export const INGESTIBLE_EVENT_TYPES = ["human_state", "observation"] as const;
