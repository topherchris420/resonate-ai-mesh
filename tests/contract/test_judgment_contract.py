import json
import os

FIXTURE = os.path.join(
    os.path.dirname(__file__),
    "../../schemas/fixtures/judgment-envelope.sample.json",
)


def test_judgment_fixture_matches_shared_contract():
    with open(FIXTURE, "r", encoding="utf-8") as handle:
        envelope = json.load(handle)

    required = [
        "schema_version",
        "judgment_id",
        "provider",
        "model",
        "provider_model_version",
        "question_set_version",
        "state_hash",
        "state_schema_version",
        "truncated",
        "proposal_id",
        "correlation_id",
        "causation_id",
        "answers",
        "disposition",
        "reason_codes",
        "policy_version",
        "requested_at",
        "completed_at",
        "latency_ms",
        "provider_status",
        "evaluation_mode",
        "simulation_label",
    ]
    for field in required:
        assert field in envelope

    assert envelope["schema_version"] == "pordenone.judgment.envelope.v1"
    assert envelope["disposition"] == "PASS"
    assert envelope["provider"] == "typesafe"
    assert envelope["provider_model_version"] == "jev-1.13.0"
    assert envelope["simulation_label"] == "SIMULATED"
    noul = next(answer for answer in envelope["answers"] if answer["question_id"] == "contradiction_present")
    assert noul["primitive"] == "noul"
    assert noul["noul"] == 0.07
    assert noul["confidence"] is None
    rendered = json.dumps(envelope)
    for forbidden in ["eeg", "ppg", "eda", "RAW_EEG", "api_key"]:
        assert forbidden not in rendered


def test_canonical_event_still_accepts_judgment_as_a_payload():
    schema_path = os.path.join(os.path.dirname(__file__), "../../schemas/json/canonical-event.json")
    with open(schema_path, "r", encoding="utf-8") as handle:
        schema = json.load(handle)
    assert "payload" in schema["required"]
    assert schema["properties"]["event_type"]["type"] == "string"
