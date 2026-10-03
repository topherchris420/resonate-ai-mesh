// Shapes of the documents the cockpit reads: run bundles, experiment
// summaries, claim evaluations, capability reports, topology, and
// counterfactual comparisons. Field names follow the Rust serializers.

export type { ChainedEvent, DecisionRecord, EventEnvelope, ValidationPayload } from "@pordenone/shared-types";

export type Vec2 = [number, number];

export interface Hazard {
  id: string;
  center: Vec2;
  radius: number;
  appears_at_tick: number;
  retires_at_tick: number | null;
}

export interface PointOfInterest {
  id: string;
  position: Vec2;
}

export interface AgentSpec {
  id: string;
  behavior: string;
  start: Vec2;
  goal: string | null;
  speed: number;
  priority: number;
}

export interface FaultSpec {
  kind: string;
  target: string | null;
  at_tick: number;
  duration_ticks: number;
  magnitude: number;
}

export interface Scenario {
  id: string;
  description: string;
  ticks: number;
  dt_ms: number;
  arena_half_extent: number;
  hazards: Hazard[];
  points_of_interest: PointOfInterest[];
  agents: AgentSpec[];
  faults: FaultSpec[];
  kernel: {
    validator: { max_stale_ms: number; max_step: number; min_separation: number; disabled_checks: string[] };
    judgment: { provider: string; profile?: string | null };
    policy: { adaptive: { enabled: boolean; on_signal_loss: string; thresholds: number[] } };
  };
}

export interface RunManifest {
  run_id: string;
  experiment_id: string;
  condition_id: string;
  repetition: number;
  seed: number;
  scenario: Scenario;
  overrides: Record<string, unknown>;
}

export interface ReplayInfo {
  run_id: string;
  seed: number;
  genesis_hash: string;
  head_hash: string;
  event_count: number;
  final_state_hash: string;
  ticks_run: number;
  termination: string;
  substituted: string[];
  counts: Record<string, number>;
  policy_versions: string[];
}

export interface ResonanceDimension {
  value: number | null;
  basis: number;
  note: string | null;
}

export interface InvariantResult {
  name: string;
  holds: boolean;
  checked: number;
  violations: string[];
  description: string;
}

export interface MetricsDoc {
  run_id: string;
  values: Record<string, number | null>;
  resonance: Record<string, ResonanceDimension>;
  rejection_reasons: Record<string, number>;
  withhold_reasons: Record<string, number>;
  invariants: InvariantResult[];
}

export interface MetricDefinition {
  id: string;
  unit: string;
  definition: string;
}

export interface Provenance {
  software_version: string;
  git_commit: string;
  git_dirty: boolean | null;
  rustc: string | null;
  command: string[];
  started_at_wall: string;
  run_config_hash: string;
  validator_version: string;
  kernel_policy_version: string;
  judgment_policy_version: string;
  judge: { name: string; model: string; kind: string; networked: boolean } | null;
  agents: { agent_id: string; adapter: string; deterministic: boolean; networked: boolean }[];
  parent: { run_id: string; head_hash: string; overrides: Record<string, unknown>; label: string } | null;
  file_hashes: Record<string, string>;
}

export interface EventSummary {
  seq: number;
  event_id: string;
  event_type: string;
  tick: number | null;
  subject_id: string;
  source: string;
}

export interface Divergence {
  index: number;
  left: EventSummary | null;
  right: EventSummary | null;
  differences: { path: string; left: unknown; right: unknown }[];
}

export interface ReplayReport {
  run_id: string;
  verified: boolean;
  integrity: { ok: boolean; chain_ok: boolean; problems: string[]; files_checked: number; files_matching: number };
  counts: { label: string; recorded: number; replayed: number }[];
  events_matching: number;
  head_recorded: string;
  head_replayed: string;
  head_exact_match: boolean;
  metric_differences: string[];
  network_calls: number;
  substituted: string[];
  normalized_fields: string[];
  first_divergence: Divergence | null;
  error: string | null;
}

export interface TimelineComparison {
  left_run: string;
  right_run: string;
  overrides: Record<string, unknown>;
  left_events: number;
  right_events: number;
  first_divergent_event: Divergence | null;
  first_state_divergence_tick: number | null;
  first_decision_divergence_tick: number | null;
  state_by_tick: { tick: number; left: string | null; right: string | null }[];
  decision_changes: {
    proposal_id: string;
    agent_id: string;
    tick: number;
    left: string | null;
    right: string | null;
    left_decided_at: number | null;
    right_decided_at: number | null;
  }[];
  metric_deltas: { metric: string; left: number | null; right: number | null; delta: number | null }[];
}

export interface ChainLink {
  seq: number;
  event_id: string;
  event_type: string;
  tick: number | null;
  source: string;
  causation_id: string;
  summary: string;
  ai_involved: boolean;
  mode: string;
  policy_version: string;
}

export interface Explanation {
  target: string;
  correlation_id: string;
  chain: ChainLink[];
  state_changed: boolean;
  ai_involved: boolean;
}

export interface Capability {
  id: string;
  statement: string;
  available: boolean;
  reason: string | null;
  command: string | null;
}

export interface CapabilityReport {
  context: "cli" | "server" | "static-export";
  orientation: string[];
  status: { human_state_input: string; remote_judgment: string; physical_control: string; data_mode: string };
  capabilities: Capability[];
  scenarios: string[];
  experiments: string[];
  latest_run: string | null;
}

export interface ClaimEvidence {
  description: string;
  detail: string;
  experiment: string;
  outcome: string;
  role: string;
  summary_hash: string | null;
}

export interface ClaimResult {
  id: string;
  claim: string;
  file: string;
  declared_status: string;
  evidence_status: string;
  level: string;
  message: string;
  evidence: ClaimEvidence[];
  limitations: string[];
}

export interface MetricStats {
  n: number;
  mean: number | null;
  median: number | null;
  sd: number | null;
  variance: number | null;
  min: number | null;
  max: number | null;
  ci95: [number, number] | null;
}

export interface PairedComparison {
  metric: string;
  baseline: string;
  treatment: string;
  n_pairs: number;
  baseline_mean: number | null;
  treatment_mean: number | null;
  mean_difference: number | null;
  median_difference: number | null;
  sd_difference: number | null;
  ci95_t: [number, number] | null;
  ci95_bootstrap: [number, number] | null;
  cohens_dz: number | null;
  hedges_g: number | null;
  pairs_increased: number;
  pairs_decreased: number;
  pairs_equal: number;
}

export interface ConditionSummary {
  id: string;
  description: string;
  status: string;
  skip_reason: string | null;
  overrides: Record<string, unknown>;
  runs: number;
  metrics: Record<string, MetricStats>;
  failure_rates: Record<string, unknown>;
}

export interface PredictionVerdict {
  metric: string;
  direction: string;
  baseline: string;
  treatment: string;
  status: string;
  detail: string;
}

export interface ExperimentSummary {
  version: string;
  experiment_id: string;
  title: string;
  question: string;
  hypothesis: { statement: string; prediction: Omit<PredictionVerdict, "status" | "detail"> | null };
  manifest_hash: string;
  scenario_id: string;
  base_seed: number;
  repetitions: number;
  dependent_variables: string[];
  conditions: ConditionSummary[];
  comparisons: PairedComparison[];
  prediction: PredictionVerdict | null;
  invariants: { name: string; expected: boolean; runs_checked: number; runs_violated: number; violating_runs: string[] }[];
  unexpected: string[];
  limitations: string[];
  software_version: string;
}

export interface ManifestInfo {
  id: string;
  title: string;
  question: string;
  hypothesis: string;
  prediction: Omit<PredictionVerdict, "status" | "detail"> | null;
  conditions: { id: string; description: string; set: Record<string, unknown>; requires: string[] }[];
  repetitions: number;
  path?: string;
  has_summary?: boolean;
}

export interface ScenarioInfo {
  id: string;
  description: string;
  agents: number;
  ticks: number;
  faults: string[];
  judgment: string;
  path?: string;
}

export interface TopologyNode {
  id: string;
  kind: string;
  label: string;
  capabilities: string[];
  inputs: string[];
  outputs: string[];
  schemas: string[];
  trust_boundary: string;
  latency_ms: number | null;
  provenance: string;
  health: string;
  deterministic: boolean;
  networked: boolean;
  may_mutate_state: boolean;
  data_mode: string | null;
}

export interface Topology {
  version: string;
  run_id: string | null;
  nodes: TopologyNode[];
  edges: { from: string; to: string; kind: string; schema: string }[];
}

export interface RunListing {
  id: string;
  label: string;
  kind: "recorded" | "counterfactual" | "live";
  parent: string | null;
  hasDivergence: boolean;
  events?: number;
}
