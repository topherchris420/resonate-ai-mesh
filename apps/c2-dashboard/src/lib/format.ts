// Number formatting shared by every view. Mirrors fmt_value/fmt_delta in
// crates/mesh-lab/src/bundle.rs: precision follows magnitude, and a non-zero
// quantity is never printed as 0.

export function fmtValue(value: number | null | undefined): string {
  if (value === null || value === undefined || Number.isNaN(value)) return "—";
  if (Number.isInteger(value) && Math.abs(value) < 1e15) return String(Object.is(value, -0) ? 0 : value);
  const abs = Math.abs(value);
  if (abs < 0.0005) return value.toExponential(1);
  const text = abs >= 100 ? value.toFixed(1) : abs >= 1 ? value.toFixed(2) : value.toFixed(3);
  const trimmed = text.includes(".") ? text.replace(/0+$/, "").replace(/\.$/, "") : text;
  return trimmed === "-0" ? "0" : trimmed;
}

export function fmtDelta(delta: number | null | undefined): string {
  if (delta === null || delta === undefined || Number.isNaN(delta)) return "—";
  const text = fmtValue(delta);
  return delta > 0 ? `+${text}` : text;
}

export function fmtInterval(ci: [number, number] | null | undefined): string {
  return ci ? `[${fmtValue(ci[0])}, ${fmtValue(ci[1])}]` : "—";
}

export function shortHash(hash: string | null | undefined, length = 12): string {
  if (!hash) return "—";
  return hash.replace(/^sha256:/, "").slice(0, length);
}

export function titleCase(id: string): string {
  return id.replace(/[_-]+/g, " ").replace(/^\w/, (c) => c.toUpperCase());
}
