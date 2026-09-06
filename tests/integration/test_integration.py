import os
import sys
import pytest

def test_pipeline_to_sim_integration():
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../services/biometric-pipeline"))
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../services/sim-engine"))

    from biometric_generator import BiometricPipeline
    from emergence_sim import SwarmEmergenceSimulator
    from drr_adapter import DynamicResonanceRootingAdapter

    pipe = BiometricPipeline(seed=42)
    sim = SwarmEmergenceSimulator(num_agents=5, seed=42)
    drr = DynamicResonanceRootingAdapter()

    telemetry = pipe.generate_sample()
    sim_step = sim.step()

    adaptive_res = drr.evaluate_adaptive_state(
        cognitive_load=telemetry["cognitive_load"],
        environmental_entropy=sim_step["environmental_entropy"]
    )

    assert "state" in adaptive_res
    assert adaptive_res["confidence"] > 0.0
    assert len(sim_step["agents"]) == 5
