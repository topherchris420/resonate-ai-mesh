#!/usr/bin/env python3
"""A minimal deliberative agent speaking the mesh-agent/1 protocol.

This is the integration boundary for a R.A.I.N. system (or any external
reasoner). The process reads one JSON object per line on stdin and answers
`hello` and `propose` with one JSON object per line on stdout. It never talks
to the kernel: its intents become proposals that the Pordenone kernel
validates and decides on like any other agent's.

Deliberation here is deliberately simple and deterministic (a repulsive
potential around known hazards), so runs that use it replay exactly. A real
R.A.I.N. adapter would replace `deliberate` and keep the protocol.

Protocol (see docs/rain-adapter.md):
  -> {"type":"hello","protocol":"mesh-agent/1","agent_id":"rain_01"}
  <- {"protocol":"mesh-agent/1","name":"rain-stub","deterministic":true}
  -> {"type":"observe","observation":{...}}          (no reply)
  -> {"type":"propose","context":{...}}
  <- {"intent":{...}|null,"explanation":"..."}
  -> {"type":"outcome","outcome":{...}}               (no reply)
  -> {"type":"shutdown"}
"""

import json
import math
import sys

PROTOCOL = "mesh-agent/1"


class Deliberator:
    def __init__(self):
        self.observation = None
        self.last_outcome = None
        self.rejections = 0

    def deliberate(self, context):
        obs = self.observation
        if obs is None or obs.get("quality") != "GOOD" or obs.get("goal") is None:
            return None, "no usable observation; holding"
        x, y = obs["own_position"]["x"], obs["own_position"]["y"]
        gx, gy = obs["goal"]["x"], obs["goal"]["y"]
        dx, dy = gx - x, gy - y
        distance = math.hypot(dx, dy)
        if distance < 0.75:
            return None, "at goal"
        # Attraction to the goal plus repulsion from hazards within reach.
        fx, fy = dx / distance, dy / distance
        reasons = []
        for zone in obs.get("known_hazards", []):
            cx, cy, r = zone["center"]["x"], zone["center"]["y"], zone["radius"]
            hx, hy = x - cx, y - cy
            gap = math.hypot(hx, hy) - r
            if gap < 12.0:
                weight = 2.5 / max(gap, 0.5)
                fx += weight * hx / max(math.hypot(hx, hy), 1e-9)
                fy += weight * hy / max(math.hypot(hx, hy), 1e-9)
                reasons.append(f"repelled by {zone['id']} (gap {gap:.1f})")
        norm = math.hypot(fx, fy) or 1.0
        # Be more conservative after rejections.
        step = min(distance, context["constraints"]["max_step"] * (0.6 if self.rejections else 0.8))
        tx = round(x + fx / norm * step, 4)
        ty = round(y + fy / norm * step, 4)
        intent = {
            "action_type": "MOVE",
            "target": {"x": tx, "y": ty, "z": 0.0},
            "priority": 1,
            "rationale": "potential-field step" + (": " + "; ".join(reasons) if reasons else ""),
        }
        return intent, f"distance to goal {distance:.1f}; step {step:.1f}"

    def handle(self, message):
        kind = message.get("type")
        if kind == "hello":
            return {"protocol": PROTOCOL, "name": "rain-stub", "deterministic": True}
        if kind == "observe":
            self.observation = message.get("observation")
            return None
        if kind == "propose":
            intent, explanation = self.deliberate(message["context"])
            return {"intent": intent, "explanation": explanation}
        if kind == "outcome":
            self.last_outcome = message.get("outcome")
            if self.last_outcome and not self.last_outcome.get("committed"):
                self.rejections += 1
            return None
        if kind == "shutdown":
            raise SystemExit(0)
        return None


def main():
    agent = Deliberator()
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            reply = agent.handle(json.loads(line))
        except SystemExit:
            return
        except Exception as error:  # report and keep serving
            reply = {"intent": None, "explanation": f"agent error: {error}"}
        if reply is not None:
            sys.stdout.write(json.dumps(reply) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
