"use client";

import React, { useEffect, useRef } from "react";
import type { CapabilityReport } from "@/lib/types";
import { LAW } from "./DecisionInspector";
import { Button } from "./ui";

export type View = "live" | "replay" | "experiment" | "evidence" | "compare" | "topology" | "capabilities";

const STARTS: { view: View; title: string; text: string }[] = [
  { view: "replay", title: "Replay the Perturbed Mesh", text: "Five agents, a hazard that appears mid-run, an operator-load spike, a sensor dropout, and a corrupted reading. Inspect any decision." },
  { view: "compare", title: "Remove judgment and compare", text: "The same seed with the judgment stage disabled, and where the timelines part." },
  { view: "experiment", title: "Read an experiment", text: "Pre-registered predictions, paired repetitions, confidence intervals, effect sizes, raw data." },
  { view: "evidence", title: "Check the claims", text: "What the project claims, the status a person gave each claim, and whether the recorded evidence agrees." },
];

/** First-launch explanation, built from the capability report rather than marketing copy. */
export default function Orientation({ report, onClose, onGo }: { report: CapabilityReport | null; onClose: () => void; onGo: (view: View) => void }) {
  const dialog = useRef<HTMLDivElement>(null);
  useEffect(() => {
    dialog.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-ink/80 p-4 backdrop-blur-sm md:items-center" onClick={onClose}>
      <div
        ref={dialog}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby="orientation-title"
        className="w-full max-w-3xl rounded-xl border border-line bg-panel p-6 shadow-2xl outline-none"
        onClick={(e) => e.stopPropagation()}
      >
        <h1 id="orientation-title" className="text-[20px] font-semibold">
          Resonate AI Mesh
        </h1>
        <p className="text-muted">A research instrument for multi-agent decisions under the Pordenone kernel.</p>
        <ul className="mt-4 space-y-1 text-[13.5px]">
          {(report?.orientation ?? ["This is a deterministic multi-agent research environment."]).map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>

        <h2 className="mt-5 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted">The execution law every decision follows</h2>
        <ol className="mt-2 grid gap-1.5 md:grid-cols-2">
          {LAW.map((stage, i) => (
            <li key={stage.id} className="flex gap-2 text-[12.5px]">
              <span className="w-4 shrink-0 text-right text-muted">{i + 1}</span>
              <span>
                <span className="font-medium">{stage.label}.</span> <span className="text-text/80">{stage.text}</span>
              </span>
            </li>
          ))}
        </ol>
        <p className="mt-2 text-[12px] text-muted">
          No probabilistic model can override a failed deterministic check, no remote model can change authoritative state, and a person&apos;s decision is re-validated before it commits.
        </p>

        <h2 className="mt-5 text-[11px] font-semibold uppercase tracking-[0.08em] text-muted">Where to start</h2>
        <div className="mt-2 grid gap-2 md:grid-cols-2">
          {STARTS.map((s) => (
            <button
              key={s.view}
              type="button"
              onClick={() => onGo(s.view)}
              className="rounded-lg border border-line bg-raised/50 px-3 py-2.5 text-left hover:border-accent/50"
            >
              <div className="font-medium text-accent">{s.title}</div>
              <div className="text-[12px] text-text/80">{s.text}</div>
            </button>
          ))}
        </div>
        <div className="mt-5 flex items-center justify-between gap-3">
          <p className="text-[11px] text-muted">
            {report?.context === "static-export"
              ? "You are viewing a static export of recorded SIMULATED runs. Every number on these pages comes from those files."
              : "Connected to a local mesh server."}
          </p>
          <Button tone="primary" onClick={onClose}>
            Explore
          </Button>
        </div>
      </div>
    </div>
  );
}
