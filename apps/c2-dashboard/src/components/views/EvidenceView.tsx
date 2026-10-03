"use client";

import React from "react";
import { Badge, CopyCommand, Empty, ErrorNote, Loading, Panel, Table, type Tone } from "@/components/ui";
import { shortHash } from "@/lib/format";
import type { MeshSource } from "@/lib/source";
import type { ClaimResult } from "@/lib/types";
import { useAsync } from "@/lib/useRun";

const LEVEL_TONE: Record<string, Tone> = { ok: "commit", note: "accent", warning: "withhold", error: "reject" };
const OUTCOME_TONE: Record<string, Tone> = { holds: "commit", consistent: "commit", fails: "reject", inconsistent: "reject", incomplete: "withhold", missing: "withhold" };

function Claim({ claim }: { claim: ClaimResult }) {
  return (
    <Panel
      title={claim.id}
      actions={
        <>
          <Badge title="Written by a person in the claim file">declared: {claim.declared_status}</Badge>
          <Badge tone={OUTCOME_TONE[claim.evidence_status] ?? "neutral"} title="Computed from recorded experiment summaries">
            evidence: {claim.evidence_status}
          </Badge>
          <Badge tone={LEVEL_TONE[claim.level] ?? "neutral"}>{claim.level}</Badge>
        </>
      }
    >
      <p className="text-[13px]">{claim.claim}</p>
      <p className="mt-1 text-[12px] text-muted">{claim.message}</p>
      {claim.evidence.length > 0 && (
        <Table className="mt-3" head={["experiment", "evidence", "role", "outcome", "detail", "summary"]}>
          {claim.evidence.map((e, i) => (
            <tr key={i}>
              <td className="px-2 py-1 font-mono">{e.experiment}</td>
              <td className="px-2 py-1">{e.description}</td>
              <td className="px-2 py-1 text-muted">{e.role}</td>
              <td className="px-2 py-1">
                <Badge tone={OUTCOME_TONE[e.outcome] ?? "neutral"}>{e.outcome}</Badge>
              </td>
              <td className="px-2 py-1 text-muted">{e.detail}</td>
              <td className="px-2 py-1 font-mono text-[11px] text-muted" title={e.summary_hash ?? undefined}>
                {shortHash(e.summary_hash, 8)}
              </td>
            </tr>
          ))}
        </Table>
      )}
      {claim.limitations.length > 0 && (
        <ul className="mt-3 list-disc pl-4 text-[12px] text-muted">
          {claim.limitations.map((l) => (
            <li key={l}>{l}</li>
          ))}
        </ul>
      )}
      <p className="mt-2 font-mono text-[11px] text-muted/70">{claim.file}</p>
    </Panel>
  );
}

export default function EvidenceView({ source }: { source: MeshSource }) {
  const claims = useAsync(() => source.claims(), [source]);
  if (claims.loading && !claims.value) return <Loading what="claims" />;
  const list = claims.value ?? [];
  const count = (level: string) => list.filter((c) => c.level === level).length;
  return (
    <div className="space-y-4">
      <Panel title="Claims and evidence">
        <p className="max-w-3xl">
          Each claim is written by a person, with the status they believe it has and the experiment evidence it rests on. The checker re-reads the recorded experiment summaries and reports
          whether that evidence is consistent with the declared status. It never changes a claim. A claim declared supported whose evidence is incomplete or inconsistent is an error;
          evidence that agrees with a provisional claim is only a note, because the status stays as declared until a person changes it.
        </p>
        <div className="mt-3 flex flex-wrap gap-2">
          <Badge tone="commit">{count("ok")} ok</Badge>
          <Badge tone="accent">{count("note")} notes</Badge>
          <Badge tone="withhold">{count("warning")} warnings</Badge>
          <Badge tone="reject">{count("error")} errors</Badge>
        </div>
        <div className="mt-3">
          <CopyCommand command="mesh claims check" />
        </div>
      </Panel>
      <ErrorNote error={claims.error} />
      {list.length === 0 && !claims.error ? <Empty title="No claims recorded" /> : list.map((c) => <Claim key={c.id} claim={c} />)}
    </div>
  );
}
