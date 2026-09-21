"use client";

import React from "react";
import { JudgmentEnvelope, ValidationResultPayload } from "@pordenone/shared-types";

interface ValidationInspectorProps {
  latestValidation: ValidationResultPayload | null;
  latestJudgment: JudgmentEnvelope | null;
  humanDecision: "approve" | "reject" | null;
  onDispatchProposal: (actionType: string, targetX: number, targetY: number) => void;
  onResolveHumanReview: (proposalId: string, decision: "approve" | "reject") => void;
}

function answerById(judgment: JudgmentEnvelope, id: string) {
  return judgment.answers.find((answer) => answer.question_id === id);
}

function formatProbability(value: number | undefined): string {
  if (value === undefined) return "—";
  return value.toFixed(2);
}

function deterministicRows(validation: ValidationResultPayload) {
  const text = validation.contradictions.join(" ").toLowerCase();
  const mentions = (needle: string) => text.includes(needle);
  return [
    {
      label: "Coordinates valid",
      ok: !mentions("position") && !mentions("boundary") && !mentions("coordinate"),
    },
    { label: "Action permitted", ok: !mentions("action") },
    { label: "Freshness valid", ok: !mentions("stale") },
    { label: "State consistent", ok: validation.contradictions.length === 0 },
  ];
}

function dispositionClass(disposition: string): string {
  switch (disposition) {
    case "PASS":
      return "bg-greenOk/20 text-greenOk border-greenOk/40";
    case "HUMAN_REVIEW":
      return "bg-amberAlert/20 text-amberAlert border-amberAlert/40";
    case "REVISE":
      return "bg-amberAlert/10 text-amberAlert border-amberAlert/30";
    case "SKIPPED":
      return "bg-[#21262d] text-[#c9d1d9] border-panelBorder";
    default:
      return "bg-redAlert/20 text-redAlert border-redAlert/40";
  }
}

export default function ValidationInspector({
  latestValidation,
  latestJudgment,
  humanDecision,
  onDispatchProposal,
  onResolveHumanReview,
}: ValidationInspectorProps) {
  const [actionType, setActionType] = React.useState("PATROL");
  const [targetX, setTargetX] = React.useState(200);
  const [targetY, setTargetY] = React.useState(150);
  const support = latestJudgment ? answerById(latestJudgment, "proposal_support") : undefined;
  const evidence = latestJudgment ? answerById(latestJudgment, "evidence_quality") : undefined;
  const contradiction = latestJudgment ? answerById(latestJudgment, "contradiction_present") : undefined;
  const scope = latestJudgment ? answerById(latestJudgment, "scope_violation") : undefined;
  const human = latestJudgment ? answerById(latestJudgment, "human_review") : undefined;
  const awaitingReview = latestJudgment?.disposition === "HUMAN_REVIEW" && !humanDecision;

  return (
    <div className="bg-panel border border-panelBorder p-3 rounded flex flex-col gap-3 text-xs max-h-[62%] overflow-y-auto">
      <div className="font-bold tracking-wider text-cyanGlow uppercase border-b border-panelBorder pb-2">
        Validation & Judgment
      </div>

      <section className="bg-[#0d1117] p-2.5 rounded border border-cyanGlow/30 flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <span className="font-bold tracking-wider text-cyanGlow text-[10px]">DETERMINISTIC</span>
          <span className="text-[10px] text-[#8b949e]">Epistemic validation</span>
        </div>
        {latestValidation ? (
          <>
            <div className="flex justify-between items-center">
              <span className="text-white">Agent {latestValidation.agent_id}</span>
              <span
                className={`px-2 py-0.5 rounded font-bold text-[10px] border ${
                  latestValidation.accepted
                    ? "bg-greenOk/20 text-greenOk border-greenOk/40"
                    : "bg-redAlert/20 text-redAlert border-redAlert/40"
                }`}
              >
                {latestValidation.accepted ? "PASSED" : "REJECTED"}
              </span>
            </div>
            {deterministicRows(latestValidation).map((row) => (
              <div key={row.label} className="flex justify-between text-[11px]">
                <span className="text-[#8b949e]">{row.label}</span>
                <span className={row.ok ? "text-greenOk" : "text-redAlert"}>{row.ok ? "✓" : "✗"}</span>
              </div>
            ))}
            <div className="text-[10px] text-[#8b949e]">
              Feasibility {(latestValidation.feasibility * 100).toFixed(0)}% · deterministic confidence{" "}
              {(latestValidation.confidence * 100).toFixed(0)}%
            </div>
            {latestValidation.contradictions.length > 0 && (
              <div className="text-[10px] text-redAlert bg-red-950/20 p-1.5 rounded border border-red-800/40">
                {latestValidation.contradictions.join(" · ")}
              </div>
            )}
          </>
        ) : (
          <div className="text-[#8b949e] text-[11px] italic">No proposal has been validated yet.</div>
        )}
      </section>

      <section className="bg-[#0d1117] p-2.5 rounded border border-amberAlert/40 flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <span className="font-bold tracking-wider text-amberAlert text-[10px]">PROBABILISTIC</span>
          <span className="text-[10px] text-[#8b949e]">Typed judgment</span>
        </div>
        {!latestValidation ? (
          <div className="text-[#8b949e] text-[11px] italic">Waiting for a proposal.</div>
        ) : !latestValidation.accepted ? (
          <div className="text-[#c9d1d9] text-[11px]">
            Not requested. Deterministic validation failed, so no remote judgment was called.
          </div>
        ) : !latestJudgment || latestJudgment.answers.length === 0 ? (
          <div className="text-[#c9d1d9] text-[11px]">
            Typed judgment is off. Pordenone is using deterministic validation only.
            {latestJudgment?.simulation_label === "SIMULATED" && (
              <div className="mt-1 text-amberAlert">Simulation label: SIMULATED</div>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-1 text-[11px]">
            <JudgmentRow
              label="Support"
              value={(support?.choice ?? "—").toUpperCase()}
              detail={
                support
                  ? `p ${formatProbability(support.choice ? support.probabilities[support.choice] : undefined)} · confidence ${
                      support.confidence === null ? "absent" : formatProbability(support.confidence)
                    }`
                  : "missing"
              }
            />
            <JudgmentRow
              label="Evidence quality"
              value={evidence?.score === null || evidence?.score === undefined ? "—" : `${evidence.score.toFixed(2)} / 3`}
              detail={
                evidence
                  ? `confidence ${evidence.confidence === null ? "absent" : formatProbability(evidence.confidence)}`
                  : "missing"
              }
            />
            <JudgmentRow
              label="Contradiction"
              value={contradiction?.noul === null || contradiction?.noul === undefined ? "—" : contradiction.noul.toFixed(2)}
              detail="noul · no confidence"
            />
            <JudgmentRow
              label="Scope violation"
              value={scope?.noul === null || scope?.noul === undefined ? "—" : scope.noul.toFixed(2)}
              detail="noul · no confidence"
            />
            <JudgmentRow
              label="Human review"
              value={human?.noul === null || human?.noul === undefined ? "—" : human.noul.toFixed(2)}
              detail="noul · no confidence"
            />
            {latestJudgment.truncated && (
              <div className="text-amberAlert">Evidence package was truncated before evaluation.</div>
            )}
            {latestJudgment.simulation_label && (
              <div className="text-[10px] text-amberAlert">Simulation label: {latestJudgment.simulation_label}</div>
            )}
          </div>
        )}
      </section>

      <section className="bg-[#0d1117] p-2.5 rounded border border-white/20 flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <span className="font-bold tracking-wider text-white text-[10px]">POLICY</span>
          <span className="text-[10px] text-[#8b949e]">Deterministic interpretation</span>
        </div>
        {latestValidation && !latestValidation.accepted ? (
          <div className="text-[11px] text-redAlert">Deterministic rejection. Policy was not applied.</div>
        ) : latestJudgment ? (
          <>
            <div className="flex items-center justify-between">
              <span className={`px-2 py-0.5 rounded font-bold text-[10px] border ${dispositionClass(latestJudgment.disposition)}`}>
                {latestJudgment.disposition}
              </span>
              <span className="text-[10px] text-[#8b949e]">{latestJudgment.policy_version}</span>
            </div>
            <div className="text-[10px] text-[#c9d1d9]">{latestJudgment.reason_codes.join(" · ") || "No reason codes"}</div>
            {humanDecision && (
              <div className="text-[11px] text-white border border-panelBorder rounded p-1.5">
                Human decision recorded: {humanDecision.toUpperCase()}. The model disposition was not rewritten.
              </div>
            )}
            {awaitingReview && (
              <div className="flex gap-2">
                <button
                  onClick={() => onResolveHumanReview(latestJudgment.proposal_id, "approve")}
                  className="flex-1 bg-greenOk/20 border border-greenOk/40 text-greenOk font-bold py-1 rounded"
                >
                  Approve
                </button>
                <button
                  onClick={() => onResolveHumanReview(latestJudgment.proposal_id, "reject")}
                  className="flex-1 bg-redAlert/20 border border-redAlert/40 text-redAlert font-bold py-1 rounded"
                >
                  Reject
                </button>
              </div>
            )}
          </>
        ) : (
          <div className="text-[11px] text-[#8b949e]">No policy result yet.</div>
        )}
      </section>

      {latestJudgment && (
        <details className="text-[10px] text-[#8b949e]">
          <summary className="cursor-pointer text-cyanGlow">Provenance</summary>
          <div className="mt-1 space-y-0.5 font-mono">
            <div>provider {latestJudgment.provider}</div>
            <div>model {latestJudgment.model}</div>
            <div>provider model {latestJudgment.provider_model_version}</div>
            <div>mode {latestJudgment.evaluation_mode}</div>
            <div>questions {latestJudgment.question_set_version}</div>
            <div className="truncate">state {latestJudgment.state_hash}</div>
            <div>status {latestJudgment.provider_status}</div>
            <div>latency {latestJudgment.latency_ms} ms</div>
            {latestValidation && <div className="truncate">validation {latestValidation.provenance}</div>}
          </div>
        </details>
      )}

      <div className="border-t border-panelBorder pt-2 flex flex-col gap-2">
        <div className="font-bold text-[#8b949e] text-[10px]">Dispatch simulated proposal</div>
        <div className="grid grid-cols-3 gap-2">
          <select
            value={actionType}
            onChange={(e) => setActionType(e.target.value)}
            className="bg-[#0d1117] border border-panelBorder rounded p-1 text-[#c9d1d9]"
          >
            <option value="PATROL">PATROL</option>
            <option value="MOVE">MOVE</option>
            <option value="INSPECT">INSPECT</option>
            <option value="INVALID_OUT_OF_BOUNDS">INVALID BOUNDS</option>
          </select>
          <input
            type="number"
            value={targetX}
            onChange={(e) => setTargetX(Number(e.target.value))}
            placeholder="Target X"
            className="bg-[#0d1117] border border-panelBorder rounded p-1 text-[#c9d1d9]"
          />
          <input
            type="number"
            value={targetY}
            onChange={(e) => setTargetY(Number(e.target.value))}
            placeholder="Target Y"
            className="bg-[#0d1117] border border-panelBorder rounded p-1 text-[#c9d1d9]"
          />
        </div>
        <button
          onClick={() => {
            const x = actionType === "INVALID_OUT_OF_BOUNDS" ? 999999 : targetX;
            onDispatchProposal(actionType === "INVALID_OUT_OF_BOUNDS" ? "MOVE" : actionType, x, targetY);
          }}
          className="bg-cyanGlow/20 hover:bg-cyanGlow/30 border border-cyanGlow/50 text-cyanGlow font-bold py-1 px-3 rounded text-xs transition-colors"
        >
          Dispatch Proposal to Kernel
        </button>
      </div>
    </div>
  );
}

function JudgmentRow({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <div className="grid grid-cols-[7.5rem_1fr] gap-2">
      <span className="text-[#8b949e]">{label}</span>
      <span>
        <span className="text-white font-semibold">{value}</span>
        <span className="text-[#8b949e]"> · {detail}</span>
      </span>
    </div>
  );
}
