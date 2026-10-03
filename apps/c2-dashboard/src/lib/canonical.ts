// Canonical JSON, byte-compatible with the Rust recorder (serde_json) and the
// Python verifier (tools/verify_bundle.py).
//
// JSON.parse cannot be used for hashing: it turns 100.0 into 100 and loses
// the integer/float distinction that the canonical form preserves. This
// module parses numbers into tagged values instead.

export class JsonNumber {
  constructor(readonly text: string) {}

  get isFloat(): boolean {
    return /[.eE]/.test(this.text);
  }
}

export type JsonValue =
  | null
  | boolean
  | string
  | JsonNumber
  | JsonValue[]
  | { [key: string]: JsonValue };

/** Parse JSON keeping each number's lexeme. Throws on malformed input. */
export function parseJsonPreservingNumbers(text: string): JsonValue {
  let i = 0;
  const fail = (what: string): never => {
    throw new SyntaxError(`${what} at offset ${i}`);
  };
  const ws = () => {
    while (i < text.length && (text[i] === " " || text[i] === "\n" || text[i] === "\r" || text[i] === "\t")) i++;
  };
  const value = (): JsonValue => {
    ws();
    const ch = text[i];
    if (ch === "{") {
      i++;
      const obj: { [key: string]: JsonValue } = {};
      ws();
      if (text[i] === "}") {
        i++;
        return obj;
      }
      for (;;) {
        ws();
        if (text[i] !== '"') fail("expected key");
        const key = string();
        ws();
        if (text[i] !== ":") fail("expected ':'");
        i++;
        obj[key] = value();
        ws();
        if (text[i] === ",") {
          i++;
          continue;
        }
        if (text[i] === "}") {
          i++;
          return obj;
        }
        fail("expected ',' or '}'");
      }
    }
    if (ch === "[") {
      i++;
      const arr: JsonValue[] = [];
      ws();
      if (text[i] === "]") {
        i++;
        return arr;
      }
      for (;;) {
        arr.push(value());
        ws();
        if (text[i] === ",") {
          i++;
          continue;
        }
        if (text[i] === "]") {
          i++;
          return arr;
        }
        fail("expected ',' or ']'");
      }
    }
    if (ch === '"') return string();
    if (text.startsWith("true", i)) {
      i += 4;
      return true;
    }
    if (text.startsWith("false", i)) {
      i += 5;
      return false;
    }
    if (text.startsWith("null", i)) {
      i += 4;
      return null;
    }
    const match = /^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/.exec(text.slice(i, i + 64));
    if (!match) return fail("unexpected token");
    i += match[0].length;
    return new JsonNumber(match[0]);
  };
  const string = (): string => {
    const start = i;
    i++;
    while (i < text.length && text[i] !== '"') {
      if (text[i] === "\\") i++;
      i++;
    }
    if (i >= text.length) fail("unterminated string");
    i++;
    return JSON.parse(text.slice(start, i)) as string;
  };
  const result = value();
  ws();
  if (i !== text.length) fail("trailing characters");
  return result;
}

/** Shortest round-trip digits and exponent: |x| = digits × 10^exp. */
function decompose(x: number): { digits: string; exp: number } {
  const s = String(x);
  const e = s.indexOf("e");
  const mantissa = e >= 0 ? s.slice(0, e) : s;
  let exp = e >= 0 ? parseInt(s.slice(e + 1), 10) : 0;
  const dot = mantissa.indexOf(".");
  const frac = dot >= 0 ? mantissa.slice(dot + 1) : "";
  exp -= frac.length;
  const all = ((dot >= 0 ? mantissa.slice(0, dot) : mantissa) + frac).replace(/^0+/, "");
  const digits = all.replace(/0+$/, "") || "0";
  exp += all.length - digits.length;
  return { digits, exp };
}

/** Render a double the way serde_json does (pinned by fixtures/canonical/floats.json). */
export function formatFloat(value: number): string {
  if (!Number.isFinite(value)) throw new RangeError("non-finite number in canonical JSON");
  if (value === 0) return Object.is(value, -0) ? "-0.0" : "0.0";
  const sign = value < 0 ? "-" : "";
  const { digits, exp: k } = decompose(Math.abs(value));
  const length = digits.length;
  const kk = length + k;
  let body: string;
  if (k >= 0 && kk <= 16) {
    body = digits + "0".repeat(k) + ".0";
  } else if (kk > 0 && kk <= 16) {
    body = `${digits.slice(0, kk)}.${digits.slice(kk)}`;
  } else if (kk > -5 && kk <= 0) {
    body = `0.${"0".repeat(-kk)}${digits}`;
  } else {
    const exponent = kk - 1;
    const mantissa = length === 1 ? digits : `${digits[0]}.${digits.slice(1)}`;
    body = `${mantissa}e${exponent > 0 ? "+" : ""}${exponent}`;
  }
  return sign + body;
}

function escapeString(text: string): string {
  let out = '"';
  for (const ch of text) {
    const code = ch.codePointAt(0) ?? 0;
    if (ch === '"') out += '\\"';
    else if (ch === "\\") out += "\\\\";
    else if (ch === "\n") out += "\\n";
    else if (ch === "\r") out += "\\r";
    else if (ch === "\t") out += "\\t";
    else if (ch === "\b") out += "\\b";
    else if (ch === "\f") out += "\\f";
    else if (code < 0x20) out += `\\u${code.toString(16).padStart(4, "0")}`;
    else out += ch;
  }
  return `${out}"`;
}

/** Order keys by Unicode code point, which equals UTF-8 byte order (Rust's String order). */
function compareKeys(a: string, b: string): number {
  const left = Array.from(a);
  const right = Array.from(b);
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    const d = (left[i].codePointAt(0) ?? 0) - (right[i].codePointAt(0) ?? 0);
    if (d !== 0) return d;
  }
  return left.length - right.length;
}

const U64_MAX = BigInt("18446744073709551615");
const I64_MIN = -BigInt("9223372036854775808");

function renderNumber(n: JsonNumber): string {
  if (!n.isFloat) {
    const big = BigInt(n.text);
    // serde_json keeps integers that fit i64/u64; larger ones become doubles.
    if (big >= I64_MIN && big <= U64_MAX) return big.toString();
  }
  return formatFloat(Number(n.text));
}

export function canonicalJson(value: JsonValue): string {
  if (value === null) return "null";
  if (value === true) return "true";
  if (value === false) return "false";
  if (typeof value === "string") return escapeString(value);
  if (value instanceof JsonNumber) return renderNumber(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  const keys = Object.keys(value).sort(compareKeys);
  return `{${keys.map((k) => `${escapeString(k)}:${canonicalJson(value[k])}`).join(",")}}`;
}
