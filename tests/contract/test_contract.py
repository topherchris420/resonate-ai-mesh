"""JSON-schema contracts, checked against real recordings.

Every event, decision, judgment, and human-state datum in the committed golden
bundles must validate against the published schemas, and so must what the
Python biometric pipeline sends to /ingest. A schema that drifts from the
implementation fails here.
"""

import copy
import json
import sys
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[2]
SCHEMAS = ROOT / "schemas" / "json"
GOLDEN = sorted(p for p in (ROOT / "fixtures" / "golden").iterdir() if (p / "events.jsonl").is_file())

sys.path.insert(0, str(ROOT / "services" / "biometric-pipeline"))
from biometric_generator import BiometricPipeline, human_state_envelope  # noqa: E402


_ALL = {p.stem: json.loads(p.read_text()) for p in SCHEMAS.glob("*.json")}
_REGISTRY = Registry().with_resources(
    (schema["$id"], Resource.from_contents(schema)) for schema in _ALL.values()
)


_VALIDATORS = {name: Draft202012Validator(schema, registry=_REGISTRY) for name, schema in _ALL.items()}


def validator(name):
    return _VALIDATORS[name]


def _errors(name, instance):
    return [f"{'/'.join(map(str, e.absolute_path))}: {e.message}" for e in validator(name).iter_errors(instance)]


def _jsonl(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


@pytest.mark.parametrize("name", sorted(_ALL))
def test_schemas_are_valid_2020_12(name):
    Draft202012Validator.check_schema(_ALL[name])


def test_recorded_events_match_schemas():
    events = [event for bundle in GOLDEN for event in _jsonl(bundle / "events.jsonl")]
    seen = {"human_state": 0, "judgment": 0}
    for event in events:
        assert not _errors("canonical-event", event), (event["event_id"], _errors("canonical-event", event))
        if event["event_type"] == "human_state":
            seen["human_state"] += 1
            assert not _errors("human-state-datum", event["payload"]["datum"]), event["event_id"]
        if event["event_type"] == "judgment":
            seen["judgment"] += 1
            assert not _errors("judgment-envelope", event["payload"]), (event["event_id"], _errors("judgment-envelope", event["payload"]))
    assert seen["judgment"] > 0 and seen["human_state"] > 0


@pytest.mark.parametrize("bundle", GOLDEN, ids=lambda p: p.name)
def test_recorded_decisions_match_schema(bundle):
    decisions = _jsonl(bundle / "decisions.jsonl")
    assert decisions
    for decision in decisions:
        errors = _errors("decision-record", decision)
        assert not errors, (decision["proposal_id"], errors)


def test_schema_rejects_commit_without_validation_pass():
    decision = _jsonl(GOLDEN[0] / "decisions.jsonl")
    committed = next(d for d in decision if d["committed"])
    forged = copy.deepcopy(committed)
    forged["validation"]["accepted"] = False
    assert _errors("decision-record", forged)


def test_judgment_sample_fixture_matches_schema():
    sample = json.loads((ROOT / "schemas" / "fixtures" / "judgment-envelope.sample.json").read_text())
    assert not _errors("judgment-envelope", sample)


def test_noul_answers_cannot_carry_confidence():
    sample = json.loads((ROOT / "schemas" / "fixtures" / "judgment-envelope.sample.json").read_text())
    noul = next(a for a in sample["answers"] if a["primitive"] == "noul")
    noul["confidence"] = 0.9
    assert _errors("judgment-envelope", sample)


def test_datum_replay_requires_original_mode():
    datum = {"metric": "operator_load_index", "value": 0.4, "unit": "index[0,1]", "timestamp": 1,
             "source": "sim.x", "mode": "REPLAY", "confidence": 0.9, "quality": "GOOD"}
    assert _errors("human-state-datum", datum)
    datum["original_mode"] = "SIMULATED"
    assert not _errors("human-state-datum", datum)
    datum["mode"] = "SIMULATED"
    assert _errors("human-state-datum", datum)


def test_biometric_pipeline_output_is_ingestible():
    pipeline = BiometricPipeline(seed=7)
    sequence = 0
    for _ in range(20):
        sample = pipeline.generate_sample()
        for datum in pipeline.to_datums(sample, ["operator_load_index", "heart_rate"]):
            assert not _errors("human-state-datum", datum), datum
            envelope = human_state_envelope(datum, "operator_alpha", sequence)
            sequence += 1
            assert not _errors("ingest-event", envelope), _errors("ingest-event", envelope)
            assert envelope["mode"] == "SIMULATED"


def test_ingest_schema_refuses_kernel_event_types():
    envelope = human_state_envelope(
        BiometricPipeline(seed=1).to_datums(BiometricPipeline(seed=1).generate_sample(), ["operator_load_index"])[0],
        "operator_alpha",
        0,
    )
    for forbidden in ["proposal", "validation", "judgment", "commitment", "state_transition"]:
        forged = dict(envelope, event_type=forbidden)
        assert _errors("ingest-event", forged), forbidden
