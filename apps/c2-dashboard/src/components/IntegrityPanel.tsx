"use client";

import React, { useEffect, useState } from "react";
import { shortHash } from "@/lib/format";
import type { MeshSource } from "@/lib/source";
import type { ReplayReport } from "@/lib/types";
import type { LoadedRun } from "@/lib/useRun";
import { verifyChain, webCryptoDigest, type ChainVerification } from "@/lib/verify";
import { Badge, Button, CopyCommand, ErrorNote, Mono } from "./ui";

/** Check the record itself in the browser, and re-execute it when a server is available. */
export default function IntegrityPanel({ run, source, cliTarget }: { run: LoadedRun; source: MeshSource; cliTarget: string }) {
  const [check, setCheck] = useState<ChainVerification | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [replay, setReplay] = useState<ReplayReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const digest = typeof window === "undefined" ? null : webCryptoDigest();

  useEffect(() => {
    setCheck(null);
    setReplay(null);
    setError(null);
  }, [run.id]);

  const verify = async () => {
    if (!digest || !run.manifestText || !run.eventsText) return;
    setError(null);
    setProgress(0);
    try {
      setCheck(await verifyChain(run.manifestText, run.eventsText, run.replay, digest, (done, total) => setProgress(done / total)));
    } catch (e) {
      setError(e);
    } finally {
      setProgress(null);
    }
  };

  const reexecute = async () => {
    if (!source.server) return;
    setBusy(true);
    setError(null);
    try {
      setReplay((await source.server.replay(run.id)).report);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="space-y-3 text-[12px]">
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        <dt className="text-muted">events</dt>
        <dd className="tabular-nums">{run.replay?.event_count ?? run.model.events.length}</dd>
        <dt className="text-muted">genesis</dt>
        <dd>
          <Mono title={run.replay?.genesis_hash}>{shortHash(run.replay?.genesis_hash)}</Mono> <span className="text-muted">= sha256(manifest)</span>
        </dd>
        <dt className="text-muted">head</dt>
        <dd>
          <Mono title={run.replay?.head_hash}>{shortHash(run.replay?.head_hash)}</Mono>
        </dd>
        <dt className="text-muted">substituted</dt>
        <dd>{run.replay?.substituted.length ? run.replay.substituted.join(", ") : "nothing (fully re-executable)"}</dd>
      </dl>

      <div className="flex flex-wrap gap-2">
        <Button tone="primary" onClick={verify} disabled={!digest || progress !== null} title={digest ? undefined : "WebCrypto needs a secure context (https or localhost)"}>
          {progress !== null ? `Verifying… ${Math.round(progress * 100)}%` : "Verify hash chain in this browser"}
        </Button>
        {source.server && (
          <Button onClick={reexecute} disabled={busy}>
            {busy ? "Re-executing…" : "Re-execute on server"}
          </Button>
        )}
      </div>
      {!digest && <p className="text-muted">In-browser verification needs WebCrypto, which browsers only provide over https or on localhost.</p>}

      {check && (
        <div className={`rounded border px-3 py-2 ${check.ok ? "border-commit/40 bg-commit/5" : "border-reject/40 bg-reject/10"}`}>
          {check.ok ? (
            <p className="text-commit">
              Intact: {check.events} links recomputed with SHA-256 in {check.millis} ms. The genesis is the hash of manifest.json and the head matches replay.json.
            </p>
          ) : (
            <p className="text-reject">
              {check.broken
                ? `Broken at event #${check.broken.index} (${check.broken.eventId}): ${check.broken.reason}.`
                : !check.genesisMatchesManifest
                  ? "manifest.json does not hash to the recorded genesis."
                  : "The recomputed head does not match replay.json."}
            </p>
          )}
          <p className="mt-1 text-muted">
            This proves the record was not altered. It does not re-run the simulation{source.server ? "; re-execution does." : "; `mesh replay` does."}
          </p>
        </div>
      )}

      {replay && (
        <div className={`rounded border px-3 py-2 ${replay.verified ? "border-commit/40 bg-commit/5" : "border-reject/40 bg-reject/10"}`}>
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={replay.verified ? "commit" : "reject"}>{replay.verified ? "replay verified" : "replay diverged"}</Badge>
            <Badge tone={replay.network_calls === 0 ? "commit" : "reject"}>{replay.network_calls} network calls</Badge>
            {replay.head_exact_match && <Badge tone="commit">head hash identical</Badge>}
          </div>
          <p className="mt-1 text-text/85">
            {replay.events_matching} events re-executed and compared one by one.
            {replay.normalized_fields.length > 0 && ` Normalized: ${replay.normalized_fields.join(", ")}.`}
          </p>
          {replay.first_divergence && (
            <p className="mt-1 text-reject">
              First divergence at event #{replay.first_divergence.index}:{" "}
              {replay.first_divergence.differences.map((d) => `${d.path}: ${JSON.stringify(d.left)} → ${JSON.stringify(d.right)}`).join("; ")}
            </p>
          )}
          {replay.error && <p className="mt-1 text-reject">{replay.error}</p>}
        </div>
      )}
      <ErrorNote error={error} />
      {!source.server && <CopyCommand command={`mesh replay ${cliTarget}`} />}
    </div>
  );
}
