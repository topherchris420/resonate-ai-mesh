import math
import time
from typing import Dict, Any

class DynamicResonanceRootingAdapter:
    def __init__(self, initial_state: str = "NORMAL", baseline_resonance: float = 1.0):
        self.state = initial_state
        self.resonance = baseline_resonance
        self.stability = 0.95
        self.adaptation_rate = 0.05
        self.step_count = 0

    def evaluate_adaptive_state(self, cognitive_load: float, environmental_entropy: float, timestamp: int = None) -> Dict[str, Any]:
        if timestamp is None:
            timestamp = int(time.time() * 1000)

        self.step_count += 1
        self.resonance = round(1.0 + 0.5 * math.sin(0.1 * self.step_count) - 0.2 * cognitive_load + 0.1 * environmental_entropy, 4)
        self.stability = round(max(0.0, min(1.0, 1.0 - 0.4 * cognitive_load - 0.3 * environmental_entropy)), 4)
        self.adaptation_rate = round(max(0.01, min(1.0, 0.05 + 0.2 * abs(cognitive_load - environmental_entropy))), 4)

        if cognitive_load > 0.85 or self.stability < 0.3:
            self.state = "CRITICAL"
        elif cognitive_load > 0.65 or self.stability < 0.6:
            self.state = "HIGH"
        elif cognitive_load > 0.4:
            self.state = "ELEVATED"
        else:
            self.state = "NORMAL"

        confidence = round(max(0.1, min(1.0, self.stability * 0.9 + 0.1)), 4)

        return {
            "state": self.state,
            "stability": self.stability,
            "resonance": self.resonance,
            "adaptation_rate": self.adaptation_rate,
            "confidence": confidence,
            "timestamp": timestamp,
        }
