import { JudgmentEnvelope, ValidationResultPayload } from "@pordenone/shared-types";

export function answerById(judgment: JudgmentEnvelope, id: string) {
  return judgment.answers.find((answer) => answer.question_id === id);
}

export function formatProbability(value: number | undefined): string {
  if (value === undefined) return "—";
  return value.toFixed(2);
}

export function deterministicRows(validation: ValidationResultPayload) {
  const text = validation.contradictions.join(" ").toLowerCase();
  const mentions = (needle: string) => text.includes(needle);
  return [
    {
      label: "Coordinates valid",
      ok: !mentions("position") && !mentions("boundary") && !mentions("coordinate"),
    },
    { label: "Action permitted", ok: !mentions("action") },
    { label: "Freshness valid", ok: !mentions("stale") },
    { label: "State consistent", ok: validation.contradictions.length === 0 },
  ];
}

export function dispositionClass(disposition: string): string {
  switch (disposition) {
    case "PASS":
      return "bg-greenOk/20 text-greenOk border-greenOk/40";
    case "HUMAN_REVIEW":
      return "bg-amberAlert/20 text-amberAlert border-amberAlert/40";
    case "REVISE":
      return "bg-amberAlert/10 text-amberAlert border-amberAlert/30";
    case "SKIPPED":
      return "bg-[#21262d] text-[#c9d1d9] border-panelBorder";
    default:
      return "bg-redAlert/20 text-redAlert border-redAlert/40";
  }
}
