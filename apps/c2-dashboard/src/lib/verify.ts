// Hash-chain verification in the browser, with WebCrypto SHA-256.
//
// The same checks as tools/verify_bundle.py, restricted to what proves the
// record was not altered: the genesis is the hash of manifest.json, every
// event's hash covers its content and its predecessor's hash, and the head
// matches replay.json. This does not re-execute the run; `mesh replay` does.

import { canonicalJson, parseJsonPreservingNumbers, type JsonValue } from "./canonical";

export type Digest = (data: Uint8Array<ArrayBuffer>) => Promise<Uint8Array>;

export function webCryptoDigest(): Digest | null {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return null;
  return async (data) => new Uint8Array(await subtle.digest("SHA-256", data));
}

const encoder = new TextEncoder();

async function sha256Tagged(digest: Digest, text: string): Promise<string> {
  const bytes = await digest(encoder.encode(text));
  let hex = "sha256:";
  for (const byte of bytes) hex += byte.toString(16).padStart(2, "0");
  return hex;
}

export interface ChainVerification {
  ok: boolean;
  events: number;
  genesis: string;
  genesisMatchesManifest: boolean;
  head: string | null;
  headMatchesReplay: boolean | null;
  /** First broken link, with a human-readable reason. */
  broken: { index: number; eventId: string; reason: string } | null;
  millis: number;
}

export async function verifyChain(
  manifestText: string,
  eventsText: string,
  recorded: { genesis_hash?: string; head_hash?: string } | null,
  digest: Digest,
  onProgress?: (done: number, total: number) => void,
): Promise<ChainVerification> {
  const started = typeof performance !== "undefined" ? performance.now() : Date.now();
  const genesis = await sha256Tagged(digest, canonicalJson(parseJsonPreservingNumbers(manifestText)));
  const lines = eventsText.split("\n").filter((line) => line.trim().length > 0);
  const parsed = lines.map((line) => parseJsonPreservingNumbers(line) as { [key: string]: JsonValue });

  let broken: ChainVerification["broken"] = null;
  // Each link depends only on recorded values, so hashes are computed in
  // parallel batches and then checked in order.
  const computed: string[] = new Array(parsed.length);
  const batch = 256;
  for (let start = 0; start < parsed.length; start += batch) {
    const slice = parsed.slice(start, start + batch);
    const hashes = await Promise.all(
      slice.map((event) => {
        const { prev_hash: prev, hash: _hash, ...body } = event;
        void _hash;
        return sha256Tagged(digest, `${typeof prev === "string" ? prev : ""}\n${canonicalJson(body)}`);
      }),
    );
    hashes.forEach((hash, offset) => {
      computed[start + offset] = hash;
    });
    onProgress?.(Math.min(start + batch, parsed.length), parsed.length);
  }
  let head = genesis;
  for (let index = 0; index < parsed.length; index++) {
    const event = parsed[index];
    const eventId = typeof event.event_id === "string" ? event.event_id : `#${index}`;
    if (event.prev_hash !== head) {
      broken = { index, eventId, reason: "prev_hash does not match the previous event" };
      break;
    }
    if (computed[index] !== event.hash) {
      broken = { index, eventId, reason: "hash does not match the event's content" };
      break;
    }
    head = computed[index];
  }
  const finished = typeof performance !== "undefined" ? performance.now() : Date.now();
  const genesisMatchesManifest = recorded?.genesis_hash ? recorded.genesis_hash === genesis : true;
  const headMatchesReplay = broken || !recorded?.head_hash ? null : recorded.head_hash === head;
  return {
    ok: broken === null && genesisMatchesManifest && headMatchesReplay !== false,
    events: parsed.length,
    genesis,
    genesisMatchesManifest,
    head: broken ? null : head,
    headMatchesReplay,
    broken,
    millis: Math.round(finished - started),
  };
}
