// @vitest-environment node
import { describe, expect, it } from "vitest";
import expected from "./__fixtures__/explain.rust.json";
import { explain } from "./explain";
import { parseJsonl } from "./model";
import { goldenBundles, read } from "./testdata";
import type { ChainedEvent, Explanation } from "./types";

const events = parseJsonl<ChainedEvent>(read(goldenBundles().find((d) => d.endsWith("perturbed-mesh"))!, "events.jsonl"));

describe("explain (port of crates/mesh-lab/src/explain.rs)", () => {
  it.each(Object.entries(expected as Record<string, Explanation>))("walks the same causal chain as `mesh explain` for %s", (target, rust) => {
    const ours = explain(events, target)!;
    expect(ours.correlation_id).toBe(rust.correlation_id);
    expect(ours.chain.map((l) => l.event_id)).toEqual(rust.chain.map((l) => l.event_id));
    expect(ours.state_changed).toBe(rust.state_changed);
    expect(ours.ai_involved).toBe(rust.ai_involved);
    // Summaries match except where Rust prints a float like 1.0 that JSON.parse returns as 1.
    ours.chain.forEach((link, i) => expect(link.summary.replace(/\.0\b/g, "")).toBe(rust.chain[i].summary.replace(/\.0\b/g, "")));
  });

  it("returns null for an unknown target", () => {
    expect(explain(events, "prop-does-not-exist")).toBeNull();
  });
});
