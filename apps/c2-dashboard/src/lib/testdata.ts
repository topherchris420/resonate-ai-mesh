// Test helpers: read committed recordings from the repository.
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";

export const REPO = resolve(__dirname, "../../../..");
export const GOLDEN = join(REPO, "fixtures/golden");
export const DEMO = resolve(__dirname, "../../public/demo");

export function read(...parts: string[]): string {
  return readFileSync(join(...parts), "utf8");
}

export function goldenBundles(): string[] {
  return readdirSync(GOLDEN)
    .map((name) => join(GOLDEN, name))
    .filter((dir) => existsSync(join(dir, "events.jsonl")));
}

export function demoRuns(): string[] {
  const runs = join(DEMO, "runs");
  return existsSync(runs) ? readdirSync(runs).map((name) => join(runs, name)) : [];
}
