import ast
import pathlib

import pytest

from biometric_generator import (
    METRIC_UNITS,
    BiometricPipeline,
    LiveSourceUnavailable,
    human_state_envelope,
)

DATUM_KEYS = {"metric", "value", "unit", "timestamp", "source", "mode", "confidence", "quality"}


def test_service_entry_point_parses():
    # The previous main.py had a syntax error that no test caught.
    source = pathlib.Path(__file__).with_name("main.py").read_text()
    ast.parse(source)


def test_samples_are_labeled_simulated_with_provenance():
    pipeline = BiometricPipeline(operator_id="test_op", seed=100)
    sample = pipeline.generate_sample(timestamp=1000)
    assert sample["mode"] == "SIMULATED"
    assert sample["is_simulated"] is True
    assert sample["source"].startswith("sim.")
    assert 0.0 <= sample["values"]["operator_load_index"] <= 1.0
    assert 0.0 <= sample["confidence"] <= 1.0


def test_live_mode_fails_closed_instead_of_fabricating_data():
    with pytest.raises(LiveSourceUnavailable):
        BiometricPipeline(mode="LIVE")
    with pytest.raises(ValueError):
        BiometricPipeline(mode="REAL")


def test_generator_is_deterministic_for_a_seed():
    a = BiometricPipeline(seed=123)
    b = BiometricPipeline(seed=123)
    assert [a.generate_sample(timestamp=t) for t in range(20)] == [b.generate_sample(timestamp=t) for t in range(20)]


def test_datums_match_the_mesh_schema_and_stay_simulated():
    pipeline = BiometricPipeline(seed=7)
    for step in range(50):
        sample = pipeline.generate_sample(timestamp=step)
        for datum in pipeline.to_datums(sample, list(METRIC_UNITS)):
            assert set(datum) == DATUM_KEYS
            assert datum["mode"] == "SIMULATED"
            assert datum["quality"] == "GOOD"


def test_envelope_is_ingestible_shape():
    pipeline = BiometricPipeline(seed=1)
    datum = pipeline.to_datums(pipeline.generate_sample(timestamp=5))[0]
    envelope = human_state_envelope(datum, "operator_alpha", 3)
    assert envelope["event_type"] == "human_state"
    assert envelope["mode"] == datum["mode"] == "SIMULATED"
    assert envelope["payload"] == {"datum": datum}
    assert envelope["schema_version"] == "1.2.0"
