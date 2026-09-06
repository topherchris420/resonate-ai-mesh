import time
import math
import random
from typing import Dict, Any, Optional, Iterator

class BiometricPipeline:
    def __init__(self, operator_id: str = "operator_alpha", mode: str = "SIMULATED", sample_rate_hz: float = 10.0, seed: int = 42):
        self.operator_id = operator_id
        self.mode = mode.upper()
        self.sample_rate_hz = sample_rate_hz
        self.is_simulated = (self.mode != "LIVE")
        self.seed = seed
        self.step_count = 0
        self.rng = random.Random(seed)

    def generate_sample(self, timestamp: Optional[int] = None) -> Dict[str, Any]:
        if timestamp is None:
            timestamp = int(time.time() * 1000)

        t = self.step_count / self.sample_rate_hz
        self.step_count += 1

        if self.is_simulated:
            base_load = 0.45 + 0.3 * math.sin(0.05 * t) + 0.1 * math.sin(0.2 * t)
            cognitive_load = max(0.0, min(1.0, base_load + self.rng.uniform(-0.02, 0.02)))
            arousal = max(0.0, min(1.0, 0.5 + 0.2 * math.cos(0.08 * t) + self.rng.uniform(-0.02, 0.02)))
            hrv = max(10.0, min(150.0, 60.0 + 25.0 * math.cos(0.04 * t) + self.rng.uniform(-1.0, 1.0)))
            heart_rate = max(40.0, min(180.0, 72.0 + 15.0 * cognitive_load + self.rng.uniform(-0.5, 0.5)))
            attention = max(0.0, min(1.0, 0.8 - 0.2 * cognitive_load + self.rng.uniform(-0.01, 0.01)))
            stress = max(0.0, min(1.0, 0.2 + 0.6 * cognitive_load + self.rng.uniform(-0.02, 0.02)))
            confidence = max(0.0, min(1.0, 0.9 - 0.3 * stress + self.rng.uniform(-0.01, 0.01)))
            sensor_provenance = f"CIRCLE:SIMULATION:synthetic_v1:seed_{self.seed}"
        else:
            cognitive_load = 0.5
            arousal = 0.5
            hrv = 65.0
            heart_rate = 72.0
            attention = 0.85
            stress = 0.3
            confidence = 0.95
            sensor_provenance = "CIRCLE:LIVE:hardware_sensor_v1"

        return {
            "operator_id": self.operator_id,
            "cognitive_load": round(cognitive_load, 4),
            "arousal": round(arousal, 4),
            "hrv": round(hrv, 2),
            "heart_rate": round(heart_rate, 2),
            "attention": round(attention, 4),
            "stress": round(stress, 4),
            "confidence": round(confidence, 4),
            "timestamp": timestamp,
            "sensor_provenance": sensor_provenance,
            "is_simulated": self.is_simulated,
        }

    def stream(self, count: int = 100) -> Iterator[Dict[str, Any]]:
        for _ in range(count):
            yield self.generate_sample()
            time.sleep(1.0 / self.sample_rate_hz)
