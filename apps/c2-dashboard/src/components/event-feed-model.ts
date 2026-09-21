import { CanonicalEventEnvelope } from "@pordenone/shared-types";

/** When alerts are aggregated, keep the first three telemetry events and every other type. */
export function selectDisplayEvents(
  events: CanonicalEventEnvelope[],
  aggregateAlerts: boolean
): CanonicalEventEnvelope[] {
  if (!aggregateAlerts) return events;
  const filtered: CanonicalEventEnvelope[] = [];
  let telemetryCount = 0;

  for (const evt of events) {
    if (evt.event_type === "telemetry") {
      telemetryCount++;
      if (telemetryCount <= 3) filtered.push(evt);
    } else {
      filtered.push(evt);
    }
  }
  return filtered;
}
