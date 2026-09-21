export type JudgmentDisposition =
  | "PENDING"
  | "PASS"
  | "REVISE"
  | "HUMAN_REVIEW"
  | "UNAVAILABLE"
  | "SKIPPED";

export type JudgmentProviderStatus =
  | "ok"
  | "disabled"
  | "timeout"
  | "unauthorized"
  | "forbidden"
  | "rate_limited"
  | "overloaded"
  | "server_error"
  | "malformed_response"
  | "network_error"
  | "unknown_model"
  | "invalid_request"
  | "missing_credentials"
  | "state_too_large"
  | "serialization_failure"
  | "unsupported_primitive"
  | "missing_fields";

export type JudgmentPrimitive = "choice" | "score" | "noul";

export type EvaluationMode = "LIVE" | "RECORDED_JUDGMENT" | "LIVE_REEVALUATION" | "DISABLED";

export interface JudgmentAnswer {
  question_id: string;
  primitive: JudgmentPrimitive;
  choice: string | null;
  score: number | null;
  noul: number | null;
  probabilities: Record<string, number>;
  confidence: number | null;
  legend: Record<string, string>;
}

export interface JudgmentEnvelope {
  schema_version: string;
  judgment_id: string;
  provider: string;
  model: string;
  provider_model_version: string;
  question_set_version: string;
  state_hash: string;
  state_schema_version: string;
  truncated: boolean;
  proposal_id: string;
  correlation_id: string;
  causation_id: string;
  answers: JudgmentAnswer[];
  disposition: JudgmentDisposition;
  reason_codes: string[];
  policy_version: string;
  requested_at: number;
  completed_at: number;
  latency_ms: number;
  provider_status: JudgmentProviderStatus;
  evaluation_mode: EvaluationMode;
  simulation_label: string;
  provider_request_id: string | null;
  input_tokens: number | null;
  output_tokens: number | null;
}

const DISPOSITIONS: readonly JudgmentDisposition[] = [
  "PENDING",
  "PASS",
  "REVISE",
  "HUMAN_REVIEW",
  "UNAVAILABLE",
  "SKIPPED",
];

const PROVIDER_STATUSES: readonly JudgmentProviderStatus[] = [
  "ok",
  "disabled",
  "timeout",
  "unauthorized",
  "forbidden",
  "rate_limited",
  "overloaded",
  "server_error",
  "malformed_response",
  "network_error",
  "unknown_model",
  "invalid_request",
  "missing_credentials",
  "state_too_large",
  "serialization_failure",
  "unsupported_primitive",
  "missing_fields",
];

const PRIMITIVES: readonly JudgmentPrimitive[] = ["choice", "score", "noul"];

const EVALUATION_MODES: readonly EvaluationMode[] = [
  "LIVE",
  "RECORDED_JUDGMENT",
  "LIVE_REEVALUATION",
  "DISABLED",
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isStringRecord(value: unknown): value is Record<string, string> {
  return isRecord(value) && Object.values(value).every((item) => typeof item === "string");
}

function isNumberRecord(value: unknown): value is Record<string, number> {
  return isRecord(value) && Object.values(value).every((item) => typeof item === "number");
}

function isJudgmentAnswer(value: unknown): value is JudgmentAnswer {
  if (!isRecord(value)) return false;
  return (
    typeof value.question_id === "string" &&
    PRIMITIVES.includes(value.primitive as JudgmentPrimitive) &&
    (value.choice === null || typeof value.choice === "string") &&
    (value.score === null || typeof value.score === "number") &&
    (value.noul === null || typeof value.noul === "number") &&
    isNumberRecord(value.probabilities) &&
    (value.confidence === null || typeof value.confidence === "number") &&
    isStringRecord(value.legend)
  );
}

export function isJudgmentEnvelope(value: unknown): value is JudgmentEnvelope {
  if (!isRecord(value)) return false;
  return (
    typeof value.schema_version === "string" &&
    typeof value.judgment_id === "string" &&
    typeof value.provider === "string" &&
    typeof value.model === "string" &&
    typeof value.provider_model_version === "string" &&
    typeof value.question_set_version === "string" &&
    typeof value.state_hash === "string" &&
    typeof value.state_schema_version === "string" &&
    typeof value.truncated === "boolean" &&
    typeof value.proposal_id === "string" &&
    typeof value.correlation_id === "string" &&
    typeof value.causation_id === "string" &&
    Array.isArray(value.answers) &&
    value.answers.every(isJudgmentAnswer) &&
    DISPOSITIONS.includes(value.disposition as JudgmentDisposition) &&
    Array.isArray(value.reason_codes) &&
    value.reason_codes.every((item) => typeof item === "string") &&
    typeof value.policy_version === "string" &&
    typeof value.requested_at === "number" &&
    typeof value.completed_at === "number" &&
    typeof value.latency_ms === "number" &&
    PROVIDER_STATUSES.includes(value.provider_status as JudgmentProviderStatus) &&
    EVALUATION_MODES.includes(value.evaluation_mode as EvaluationMode) &&
    typeof value.simulation_label === "string" &&
    (value.provider_request_id === null || typeof value.provider_request_id === "string") &&
    (value.input_tokens === null || typeof value.input_tokens === "number") &&
    (value.output_tokens === null || typeof value.output_tokens === "number")
  );
}

export function disabledJudgmentEnvelope(input: {
  proposal_id: string;
  correlation_id: string;
  causation_id: string;
  timestamp: number;
}): JudgmentEnvelope {
  return {
    schema_version: "pordenone.judgment.envelope.v1",
    judgment_id: `disabled_${input.proposal_id}`,
    provider: "disabled",
    model: "none",
    provider_model_version: "none",
    question_set_version: "pordenone.judgment.questions.v1",
    state_hash: "sha256:not-evaluated",
    state_schema_version: "pordenone.judgment.state.v1",
    truncated: false,
    proposal_id: input.proposal_id,
    correlation_id: input.correlation_id,
    causation_id: input.causation_id,
    answers: [],
    disposition: "SKIPPED",
    reason_codes: ["JUDGMENT_DISABLED"],
    policy_version: "pordenone.judgment.policy.v1",
    requested_at: input.timestamp,
    completed_at: input.timestamp,
    latency_ms: 0,
    provider_status: "disabled",
    evaluation_mode: "DISABLED",
    simulation_label: "SIMULATED",
    provider_request_id: null,
    input_tokens: null,
    output_tokens: null,
  };
}
