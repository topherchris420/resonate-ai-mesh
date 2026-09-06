import json
import os
import sys
import pytest

def test_canonical_envelope_contract():
    schema_path = os.path.join(os.path.dirname(__file__), "../../schemas/json/canonical-event.json")
    assert os.path.exists(schema_path), "Canonical JSON schema must exist"

    with open(schema_path, "r") as f:
        schema = json.load(f)

    required_fields = schema.get("required", [])
    expected = [
        "event_id", "event_type", "schema_version", "timestamp",
        "source", "subject_id", "correlation_id", "causation_id",
        "payload", "provenance"
    ]
    for field in expected:
        assert field in required_fields, f"Missing required field {field} in canonical JSON schema"

def test_python_telemetry_schema_matches_contract():
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../services/biometric-pipeline"))
    from biometric_generator import BiometricPipeline

    pipe = BiometricPipeline(seed=42)
    sample = pipe.generate_sample()

    keys = [
        "operator_id", "cognitive_load", "arousal", "hrv", "heart_rate",
        "attention", "stress", "confidence", "timestamp", "sensor_provenance", "is_simulated"
    ]
    for k in keys:
        assert k in sample, f"Missing telemetry field {k}"
