"""Simulated human-state source for Resonate AI Mesh.

Every value produced here is SIMULATED. The generator models an
operator-load *index* and a few physiological-looking channels so that the
mesh's privacy boundary and labeling can be exercised; none of it is a
measurement of a person, and none of it is a diagnosis.

LIVE mode requires a sensor adapter. This repository does not include one, so
asking for LIVE data fails instead of emitting values that would look like
real measurements.
"""

import math
import random
import time
from typing import Any, Dict, List, Optional

SOURCE = "sim.biometric_pipeline.v2"
METRIC_UNITS = {
    "operator_load_index": "index[0,1]",
    "heart_rate": "bpm (simulated)",
    "hrv": "ms (simulated)",
    "arousal": "index[0,1]",
    "stress": "index[0,1]",
}


class LiveSourceUnavailable(RuntimeError):
    """Raised when LIVE data is requested but no sensor adapter exists."""


class BiometricPipeline:
    def __init__(
        self,
        operator_id: str = "operator_alpha",
        mode: str = "SIMULATED",
        sample_rate_hz: float = 10.0,
        seed: int = 42,
    ):
        mode = mode.upper()
        if mode == "LIVE":
            raise LiveSourceUnavailable(
                "LIVE mode needs a hardware sensor adapter; none is implemented in this "
                "repository. Use SIMULATED, or implement an adapter that emits LIVE datums "
                "from a source registered with MESH_LIVE_SOURCES."
            )
        if mode != "SIMULATED":
            raise ValueError(f"unknown mode {mode!r}; expected SIMULATED")
        if sample_rate_hz <= 0:
            raise ValueError("sample_rate_hz must be positive")
        self.operator_id = operator_id
        self.mode = mode
        self.sample_rate_hz = sample_rate_hz
        self.seed = seed
        self.step_count = 0
        self.rng = random.Random(seed)

    def generate_sample(self, timestamp: Optional[int] = None) -> Dict[str, Any]:
        """One simulated sample. Values are labeled SIMULATED and carry provenance."""
        if timestamp is None:
            timestamp = int(time.time() * 1000)
        t = self.step_count / self.sample_rate_hz
        self.step_count += 1

        def jitter(scale: float) -> float:
            return self.rng.uniform(-scale, scale)

        load = 0.45 + 0.3 * math.sin(0.05 * t) + 0.1 * math.sin(0.2 * t) + jitter(0.02)
        load = min(1.0, max(0.0, load))
        stress = min(1.0, max(0.0, 0.2 + 0.6 * load + jitter(0.02)))
        return {
            "operator_id": self.operator_id,
            "timestamp": timestamp,
            "mode": self.mode,
            "is_simulated": True,
            "source": SOURCE,
            "provenance": f"{SOURCE}:seed_{self.seed}",
            "values": {
                "operator_load_index": round(load, 4),
                "heart_rate": round(min(180.0, max(40.0, 72.0 + 15.0 * load + jitter(0.5))), 2),
                "hrv": round(min(150.0, max(10.0, 60.0 + 25.0 * math.cos(0.04 * t) + jitter(1.0))), 2),
                "arousal": round(min(1.0, max(0.0, 0.5 + 0.2 * math.cos(0.08 * t) + jitter(0.02))), 4),
                "stress": round(stress, 4),
            },
            "confidence": round(min(1.0, max(0.0, 0.9 - 0.3 * stress + jitter(0.01))), 4),
        }

    def to_datums(self, sample: Dict[str, Any], metrics: Optional[List[str]] = None) -> List[Dict[str, Any]]:
        """HumanStateDatum objects (the mesh's schema) for selected metrics."""
        chosen = metrics or ["operator_load_index"]
        datums = []
        for metric in chosen:
            datums.append(
                {
                    "metric": metric,
                    "value": sample["values"][metric],
                    "unit": METRIC_UNITS[metric],
                    "timestamp": sample["timestamp"],
                    "source": sample["source"],
                    "mode": sample["mode"],
                    "confidence": sample["confidence"],
                    "quality": "GOOD",
                }
            )
        return datums


def human_state_envelope(datum: Dict[str, Any], subject_id: str, sequence: int) -> Dict[str, Any]:
    """A canonical event envelope that the mesh /ingest endpoint admits."""
    event_id = f"bio-{sequence:08d}"
    return {
        "event_id": event_id,
        "event_type": "human_state",
        "schema_version": "1.2.0",
        "timestamp": datum["timestamp"],
        "source": datum["source"],
        "subject_id": subject_id,
        "correlation_id": event_id,
        "causation_id": event_id,
        "provenance": f"{datum['source']}:{datum['metric']}",
        "mode": datum["mode"],
        "payload": {"datum": datum},
    }
