// @vitest-environment node
import { describe, expect, it } from "vitest";
import { canonicalJson, formatFloat, parseJsonPreservingNumbers } from "./canonical";
import { REPO, read } from "./testdata";

function bitsToDouble(hex: string): number {
  const view = new DataView(new ArrayBuffer(8));
  view.setBigUint64(0, BigInt(`0x${hex}`));
  return view.getFloat64(0);
}

describe("canonical JSON", () => {
  it("formats every pinned double exactly like serde_json", () => {
    const cases = JSON.parse(read(REPO, "fixtures/canonical/floats.json")) as { bits: string; json: string }[];
    expect(cases.length).toBeGreaterThan(400);
    for (const c of cases) expect(formatFloat(bitsToDouble(c.bits)), c.bits).toBe(c.json);
  });

  it("keeps the integer/float distinction that JSON.parse loses", () => {
    const value = parseJsonPreservingNumbers('{"b":100.0,"a":100,"c":[1e-7,-0.0,2.5e+20]}');
    expect(canonicalJson(value)).toBe('{"a":100,"b":100.0,"c":[1e-7,-0.0,2.5e+20]}');
  });

  it("sorts keys and escapes strings like serde_json", () => {
    const value = parseJsonPreservingNumbers('{"z":"line\\nbreak \\"q\\" \\u0001 é","a":{"y":null,"x":true}}');
    expect(canonicalJson(value)).toBe('{"a":{"x":true,"y":null},"z":"line\\nbreak \\"q\\" \\u0001 é"}');
  });

  it("rejects malformed input", () => {
    expect(() => parseJsonPreservingNumbers('{"a":1,}')).toThrow();
    expect(() => parseJsonPreservingNumbers("[1] 2")).toThrow();
  });
});
