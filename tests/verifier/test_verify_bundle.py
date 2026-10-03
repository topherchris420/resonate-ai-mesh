"""The independent bundle verifier against committed recordings.

These tests need no Rust toolchain: they read the golden bundles and the
float-formatting fixture that the Rust test suite pins.
"""

import json
import shutil
import struct
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))

import verify_bundle as vb  # noqa: E402

GOLDEN = sorted(p for p in (ROOT / "fixtures" / "golden").iterdir() if (p / "manifest.json").is_file())


def test_float_rendering_matches_rust():
    cases = json.loads((ROOT / "fixtures" / "canonical" / "floats.json").read_text())
    assert len(cases) > 400
    for case in cases:
        value = struct.unpack(">d", bytes.fromhex(case["bits"]))[0]
        assert vb.format_float(value) == case["json"], case
        # And the rendering parses back to the identical double.
        assert struct.pack(">d", json.loads(case["json"])) == bytes.fromhex(case["bits"])


def test_canonical_json_sorts_keys_and_escapes_like_serde():
    value = {"b": [1, 2.5, None], "a": {"y": "line\nbreak \"q\" \u0001 é", "x": True}}
    assert vb.canonical_json(value) == '{"a":{"x":true,"y":"line\\nbreak \\"q\\" \\u0001 é"},"b":[1,2.5,null]}'


def test_non_finite_numbers_are_rejected():
    with pytest.raises(ValueError):
        vb.canonical_json({"x": float("nan")})


@pytest.mark.parametrize("bundle", GOLDEN, ids=lambda p: p.name)
def test_golden_bundles_verify_independently(bundle):
    report = vb.verify_bundle(bundle)
    failures = [c for c in report.checks if not c["ok"]]
    assert report.ok, failures
    replay = json.loads((bundle / "replay.json").read_text())
    assert report.facts["head_hash"] == replay["head_hash"]
    assert report.facts["events"] == replay["event_count"]


@pytest.fixture
def bundle_copy(tmp_path):
    target = tmp_path / "bundle"
    shutil.copytree(ROOT / "fixtures" / "golden" / "perturbed-mesh", target)
    return target


def _rewrite_events(bundle, mutate):
    path = bundle / "events.jsonl"
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    mutate(rows)
    path.write_text("".join(json.dumps(row) + "\n" for row in rows))


def _failed(report):
    return {c["check"]: c["detail"] for c in report.checks if not c["ok"]}


def test_payload_edit_is_located(bundle_copy):
    def edit(rows):
        rows[23]["payload"]["accepted"] = not rows[23]["payload"].get("accepted", False)

    _rewrite_events(bundle_copy, edit)
    failed = _failed(vb.verify_bundle(bundle_copy))
    assert "event #23" in failed["event_chain"]
    assert "events.jsonl" in failed["file_hashes"]


def test_deleted_event_is_detected(bundle_copy):
    _rewrite_events(bundle_copy, lambda rows: rows.pop(100))
    failed = _failed(vb.verify_bundle(bundle_copy))
    assert "event #100" in failed["event_chain"]


def test_rehashed_forgery_still_fails_cross_checks(bundle_copy):
    """An attacker who recomputes the whole chain after removing a commit
    still disagrees with replay.json, the decision log, and file digests."""

    def forge(rows):
        victim = next(i for i, r in enumerate(rows)
                      if r["event_type"] == "state_transition" and r["payload"].get("mutation") == "commit")
        rows.pop(victim)
        head = rows[0]["prev_hash"]
        for seq, row in enumerate(rows):
            row["seq"] = seq
            row.pop("prev_hash")
            row.pop("hash")
            row["prev_hash"] = head
            head = vb.link_hash(head, {k: v for k, v in row.items() if k != "prev_hash"})
            row["hash"] = head

    _rewrite_events(bundle_copy, forge)
    failed = _failed(vb.verify_bundle(bundle_copy))
    assert "event_chain" not in failed
    assert {"head_hash_matches_replay", "counts_match_replay", "file_hashes",
            "decisions_agree_with_events", "state_hash_sequence"} <= set(failed)


def test_manifest_edit_breaks_genesis(bundle_copy):
    manifest = json.loads((bundle_copy / "manifest.json").read_text())
    manifest["seed"] = 43
    (bundle_copy / "manifest.json").write_text(json.dumps(manifest, indent=2))
    failed = _failed(vb.verify_bundle(bundle_copy))
    assert "genesis_is_manifest_hash" in failed
    assert "event_chain" in failed


def test_cli_exit_status(bundle_copy, capsys):
    assert vb.main([str(ROOT / "fixtures" / "golden" / "external-agent")]) == 0
    capsys.readouterr()
    _rewrite_events(bundle_copy, lambda rows: rows[5]["payload"].update({"x": 1}))
    assert vb.main(["--json", str(bundle_copy)]) == 1
    [result] = json.loads(capsys.readouterr().out)
    assert result["ok"] is False
    assert any(c["check"] == "event_chain" and not c["ok"] for c in result["checks"])
