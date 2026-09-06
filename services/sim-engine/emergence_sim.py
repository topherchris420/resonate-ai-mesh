import numpy as np
import math
import time
from typing import Dict, Any, List

class SwarmEmergenceSimulator:
    def __init__(self, num_agents: int = 10, seed: int = 42, dt: float = 0.1):
        self.num_agents = num_agents
        self.seed = seed
        self.dt = dt
        self.step_count = 0
        np.random.seed(seed)

        self.positions = np.random.uniform(-500.0, 500.0, size=(num_agents, 3))
        self.velocities = np.random.uniform(-10.0, 10.0, size=(num_agents, 3))
        self.agent_ids = [f"agent_sim_{i:03d}" for i in range(num_agents)]

    def reset(self, seed: int = None):
        if seed is not None:
            self.seed = seed
        np.random.seed(self.seed)
        self.step_count = 0
        self.positions = np.random.uniform(-500.0, 500.0, size=(self.num_agents, 3))
        self.velocities = np.random.uniform(-10.0, 10.0, size=(self.num_agents, 3))

    def compute_entropy(self) -> float:
        speeds = np.linalg.norm(self.velocities, axis=1)
        mean_speed = np.mean(speeds) + 1e-6
        norm_speeds = speeds / mean_speed
        hist, _ = np.histogram(norm_speeds, bins=10, density=True)
        hist = hist[hist > 0]
        entropy = -np.sum(hist * np.log2(hist + 1e-9))
        return float(round(max(0.0, min(1.0, entropy / 4.0)), 4))

    def step(self) -> Dict[str, Any]:
        self.step_count += 1
        t = self.step_count * self.dt

        center_force = -0.01 * self.positions
        curl_force = np.stack([
            np.sin(0.1 * self.positions[:, 1]),
            np.cos(0.1 * self.positions[:, 0]),
            0.1 * np.sin(0.1 * self.positions[:, 2])
        ], axis=1)

        acceleration = center_force + curl_force
        self.velocities += acceleration * self.dt
        self.positions += self.velocities * self.dt

        entropy = self.compute_entropy()
        timestamp = int(time.time() * 1000)

        agents_data = []
        for i in range(self.num_agents):
            agents_data.append({
                "agent_id": self.agent_ids[i],
                "state": "EXECUTING",
                "capabilities": ["SWARM", "RECON"],
                "task_assignments": [f"PATROL_SECTOR_{i % 3}"],
                "priority": 1,
                "position": {
                    "x": round(float(self.positions[i, 0]), 2),
                    "y": round(float(self.positions[i, 1]), 2),
                    "z": round(float(self.positions[i, 2]), 2),
                },
                "velocity": {
                    "x": round(float(self.velocities[i, 0]), 2),
                    "y": round(float(self.velocities[i, 1]), 2),
                    "z": round(float(self.velocities[i, 2]), 2),
                },
                "confidence": 0.95,
                "timestamp": timestamp,
            })

        return {
            "step": self.step_count,
            "dt": self.dt,
            "environmental_entropy": entropy,
            "agents": agents_data,
            "timestamp": timestamp,
        }
