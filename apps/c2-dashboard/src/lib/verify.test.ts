// @vitest-environment node
import { webcrypto } from "node:crypto";
import { describe, expect, it } from "vitest";
import { demoRuns, goldenBundles, read } from "./testdata";
import { verifyChain, type Digest } from "./verify";

const digest: Digest = async (data) => new Uint8Array(await webcrypto.subtle.digest("SHA-256", data));

async function verifyDir(dir: string, eventsOverride?: string) {
  const replay = JSON.parse(read(dir, "replay.json"));
  return verifyChain(read(dir, "manifest.json"), eventsOverride ?? read(dir, "events.jsonl"), replay, digest);
}

describe("browser chain verification", () => {
  it.each(goldenBundles())("verifies golden bundle %s", async (dir) => {
    const result = await verifyDir(dir);
    expect(result.broken).toBeNull();
    expect(result.ok).toBe(true);
    expect(result.head).toBe(JSON.parse(read(dir, "replay.json")).head_hash);
  });

  it("verifies every run the static cockpit ships", async () => {
    const runs = demoRuns();
    expect(runs.length).toBeGreaterThan(0);
    for (const dir of runs) expect((await verifyDir(dir)).ok, dir).toBe(true);
  });

  it("locates a tampered event", async () => {
    const dir = goldenBundles().find((d) => d.endsWith("perturbed-mesh"))!;
    const lines = read(dir, "events.jsonl").trim().split("\n");
    const victim = lines.findIndex((line) => line.includes('"accepted":true'));
    lines[victim] = lines[victim].replace('"accepted":true', '"accepted":false');
    const result = await verifyDir(dir, `${lines.join("\n")}\n`);
    expect(result.ok).toBe(false);
    expect(result.broken?.index).toBe(victim);
    expect(result.broken?.reason).toMatch(/content/);
  });

  it("treats a reformatted but numerically equal file as intact", async () => {
    // Canonical hashing is over values, not bytes: 1.0 written as 1.00 is the same double.
    const dir = goldenBundles().find((d) => d.endsWith("perturbed-mesh"))!;
    const events = read(dir, "events.jsonl").replace('"confidence":1.0,', '"confidence":1.00,');
    expect((await verifyDir(dir, events)).ok).toBe(true);
  });

  it("detects a wrong manifest", async () => {
    const dir = goldenBundles()[0];
    const replay = JSON.parse(read(dir, "replay.json"));
    const manifest = read(dir, "manifest.json").replace('"seed": 42', '"seed": 43');
    const result = await verifyChain(manifest, read(dir, "events.jsonl"), replay, digest);
    expect(result.ok).toBe(false);
    expect(result.broken?.index).toBe(0);
  });
});
