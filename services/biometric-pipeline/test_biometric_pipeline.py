import pytest
from biometric_generator import BiometricPipeline

def test_biometric_generator_simulated():
    pipeline = BiometricPipeline(operator_id="test_op", mode="SIMULATED", seed=100)
    sample = pipeline.generate_sample()

    assert sample["operator_id"] == "test_op"
    assert sample["is_simulated"] is True
    assert 0.0 <= sample["cognitive_load"] <= 1.0
    assert 0.0 <= sample["arousal"] <= 1.0
    assert 10.0 <= sample["hrv"] <= 150.0
    assert "SIMULATION" in sample["sensor_provenance"]

def test_biometric_generator_determinism():
    p1 = BiometricPipeline(seed=123)
    p2 = BiometricPipeline(seed=123)

    s1 = p1.generate_sample(timestamp=1000)
    s2 = p2.generate_sample(timestamp=1000)

    assert s1 == s2
