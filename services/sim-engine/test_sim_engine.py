import pytest
from emergence_sim import SwarmEmergenceSimulator
from drr_adapter import DynamicResonanceRootingAdapter

def test_swarm_emergence_determinism():
    s1 = SwarmEmergenceSimulator(num_agents=5, seed=42)
    s2 = SwarmEmergenceSimulator(num_agents=5, seed=42)

    res1 = s1.step()
    res2 = s2.step()

    assert res1["step"] == res2["step"] == 1
    assert res1["environmental_entropy"] == res2["environmental_entropy"]
    assert res1["agents"][0]["position"] == res2["agents"][0]["position"]

def test_drr_adapter_evaluation():
    drr = DynamicResonanceRootingAdapter()

    normal = drr.evaluate_adaptive_state(cognitive_load=0.2, environmental_entropy=0.1)
    assert normal["state"] == "NORMAL"
    assert normal["stability"] > 0.7

    critical = drr.evaluate_adaptive_state(cognitive_load=0.9, environmental_entropy=0.8)
    assert critical["state"] == "CRITICAL"
    assert critical["stability"] < 0.5
