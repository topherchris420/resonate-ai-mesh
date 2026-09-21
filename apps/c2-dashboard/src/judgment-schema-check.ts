import { isJudgmentEnvelope, type JudgmentEnvelope } from "@pordenone/shared-types";
import fixture from "../../../schemas/fixtures/judgment-envelope.sample.json";

function assertJudgmentEnvelope(value: unknown): JudgmentEnvelope {
  if (!isJudgmentEnvelope(value)) {
    throw new Error("judgment-envelope.sample.json does not satisfy JudgmentEnvelope");
  }
  return value;
}

/** Shared fixture checked against the TypeScript envelope contract. */
export const judgmentEnvelopeFixture: JudgmentEnvelope = assertJudgmentEnvelope(fixture);
