"""Protobuf definitions agree with what the mesh actually records.

The protos are compiled with grpc_tools into a temporary directory, then the
committed golden events are parsed with the generated types. Unknown JSON
fields are errors, so a field recorded by the Rust code but missing from the
proto fails this test.
"""

import importlib
import json
import subprocess
import sys
from pathlib import Path

import pytest

pytest.importorskip("grpc_tools")
from google.protobuf import json_format  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
GOLDEN = sorted(p for p in (ROOT / "fixtures" / "golden").iterdir() if (p / "events.jsonl").is_file())


@pytest.fixture(scope="module")
def pb(tmp_path_factory):
    out = tmp_path_factory.mktemp("proto")
    protos = sorted(str(p.relative_to(ROOT)) for p in (ROOT / "proto").glob("*.proto"))
    subprocess.run(
        [sys.executable, "-m", "grpc_tools.protoc", "-I.", f"--python_out={out}", *protos],
        cwd=ROOT,
        check=True,
    )
    sys.path.insert(0, str(out))
    try:
        yield {
            "events": importlib.import_module("proto.events_pb2"),
            "telemetry": importlib.import_module("proto.telemetry_pb2"),
            "judgment": importlib.import_module("proto.judgment_pb2"),
        }
    finally:
        sys.path.remove(str(out))
        for name in [m for m in sys.modules if m.startswith("proto.") or m == "proto"]:
            del sys.modules[name]


def _events():
    for bundle in GOLDEN:
        for line in (bundle / "events.jsonl").read_text().splitlines():
            if line.strip():
                yield json.loads(line)


def test_every_recorded_envelope_parses(pb):
    count = 0
    for event in _events():
        message = json_format.ParseDict(event, pb["events"].CanonicalEventEnvelope())
        assert message.event_id == event["event_id"]
        assert message.seq == event["seq"]
        assert pb["telemetry"].DataMode.Name(message.mode) == event["mode"]
        assert message.hash == event["hash"]
        count += 1
    assert count > 1000


def test_typed_payloads_parse(pb):
    kinds = {"human_state": 0, "validation": 0, "adaptive_level": 0}
    for event in _events():
        kind = event["event_type"]
        if kind == "human_state":
            datum = json_format.ParseDict(event["payload"]["datum"], pb["telemetry"].HumanStateDatum())
            assert pb["telemetry"].DataMode.Name(datum.mode) == event["mode"]
        elif kind == "validation":
            result = json_format.ParseDict(event["payload"], pb["events"].ValidationResultPayload())
            assert result.accepted == event["payload"]["accepted"]
            assert len(result.checks) == len(event["payload"]["checks"])
        elif kind == "adaptive_level":
            json_format.ParseDict(event["payload"], pb["telemetry"].AdaptiveLevelPayload())
        else:
            continue
        kinds[kind] += 1
    assert all(kinds.values()), kinds


def _schema_properties(name):
    return set(json.loads((ROOT / "schemas" / "json" / f"{name}.json").read_text())["properties"])


def test_judgment_proto_covers_the_json_schema(pb):
    # The judgment proto predates the JSON form: maps are repeated key/value
    # messages and optional numbers use has_* flags, so it is compared by field
    # name rather than parsed.
    envelope_fields = set(pb["judgment"].JudgmentEnvelope.DESCRIPTOR.fields_by_name)
    assert _schema_properties("judgment-envelope") <= envelope_fields
    answer_schema = json.loads((ROOT / "schemas" / "json" / "judgment-envelope.json").read_text())
    answer_fields = set(answer_schema["properties"]["answers"]["items"]["properties"])
    assert answer_fields <= set(pb["judgment"].JudgmentAnswer.DESCRIPTOR.fields_by_name)


def test_envelope_proto_covers_the_json_schema(pb):
    fields = set(pb["events"].CanonicalEventEnvelope.DESCRIPTOR.fields_by_name)
    assert _schema_properties("canonical-event") <= fields
    datum_fields = set(pb["telemetry"].HumanStateDatum.DESCRIPTOR.fields_by_name)
    assert _schema_properties("human-state-datum") == datum_fields


def test_unlabeled_data_defaults_to_simulated(pb):
    assert pb["telemetry"].DataMode.Name(0) == "SIMULATED"
    assert pb["telemetry"].DataMode.Name(pb["events"].CanonicalEventEnvelope().mode) == "SIMULATED"
