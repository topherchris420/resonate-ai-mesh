import { describe, expect, it } from "vitest";
import { fmtDelta, fmtValue } from "./format";

// The same cases as format_tests in crates/mesh-lab/src/bundle.rs.
describe("number formatting", () => {
  it("matches the Rust report formatting", () => {
    expect(fmtValue(null)).toBe("—");
    expect(fmtValue(3)).toBe("3");
    expect(fmtValue(-0)).toBe("0");
    expect(fmtValue(182.24)).toBe("182.2");
    expect(fmtValue(-2.789)).toBe("-2.79");
    expect(fmtValue(0.2125)).toBe("0.212");
    expect(fmtValue(0.00012)).toBe("1.2e-4");
    expect(fmtValue(-0.00012)).toBe("-1.2e-4");
  });

  it("signs deltas", () => {
    expect(fmtDelta(12.06)).toBe("+12.06");
    expect(fmtDelta(-3.9)).toBe("-3.9");
    expect(fmtDelta(0)).toBe("0");
  });
});
