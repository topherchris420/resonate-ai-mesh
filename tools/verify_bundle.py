#!/usr/bin/env python3
"""Independent verifier for Resonate AI Mesh run bundles.

This file shares no code with the Rust implementation. It re-derives, from
the bundle files alone and using only the Python standard library:

* the chain genesis: sha256 of the canonical JSON of ``manifest.json``;
* every link of the event hash chain in ``events.jsonl``;
* the file digests listed in ``provenance.json``;
* the counts, head hash, and genesis claimed in ``replay.json``;
* structural guarantees of the execution law, read from the events:
  every committed state transition names a passing validation of the same
  proposal, judgment only follows a passing validation, state revisions and
  state hashes form an unbroken sequence, and nothing in a simulated run
  except a person's own actions is labeled LIVE.

It does not re-execute the simulation. ``mesh replay`` does that; this tool
answers a narrower question: was this record tampered with, and is it
internally consistent?

Usage::

    python3 tools/verify_bundle.py <bundle-dir> [<bundle-dir> ...]
    python3 tools/verify_bundle.py --json <bundle-dir>

Exit status is 0 when every bundle verifies, 1 otherwise.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from decimal import Decimal
from pathlib import Path
from typing import Any, Dict, List, Optional

BUNDLE_FORMAT = "resonate-ai-mesh.run-bundle.v1"
HUMAN_ACTION_EVENTS = {"operator_command", "human_resolution"}
MODES = {"SIMULATED", "LIVE", "REPLAY"}


# --------------------------------------------------------------------------
# Canonical JSON, matching serde_json + ryu output byte for byte.
# --------------------------------------------------------------------------

def _escape_string(text: str) -> str:
    out = ['"']
    for ch in text:
        code = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == "\\":
            out.append("\\\\")
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\b":
            out.append("\\b")
        elif ch == "\f":
            out.append("\\f")
        elif code < 0x20:
            out.append("\\u%04x" % code)
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


def format_float(value: float) -> str:
    """Render a finite float the way serde_json does.

    Python's ``repr`` and serde_json both produce the shortest digit string
    that round-trips; they differ only in layout, which is reproduced here.
    ``fixtures/canonical/floats.json``, written by the Rust test suite, pins
    the layout so a dependency upgrade that changes it fails CI on both sides.
    """
    if not math.isfinite(value):
        raise ValueError("non-finite number in canonical JSON")
    if value == 0.0:
        return "-0.0" if math.copysign(1.0, value) < 0 else "0.0"
    sign = "-" if value < 0 else ""
    digits_tuple = Decimal(repr(abs(value))).as_tuple()
    digits = "".join(str(d) for d in digits_tuple.digits).rstrip("0") or "0"
    # Exponent of the last significant digit after stripping trailing zeros.
    k = digits_tuple.exponent + (len(digits_tuple.digits) - len(digits))
    length = len(digits)
    kk = length + k  # 10^(kk-1) <= |value| < 10^kk
    if 0 <= k and kk <= 16:
        body = digits + "0" * k + ".0"
    elif 0 < kk <= 16:
        body = digits[:kk] + "." + digits[kk:]
    elif -5 < kk <= 0:
        body = "0." + "0" * (-kk) + digits
    else:
        exponent = kk - 1
        mantissa = digits if length == 1 else digits[0] + "." + digits[1:]
        body = mantissa + "e" + ("+" if exponent > 0 else "") + str(exponent)
    return sign + body


def canonical_json(value: Any) -> str:
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return format_float(value)
    if isinstance(value, str):
        return _escape_string(value)
    if isinstance(value, list):
        return "[" + ",".join(canonical_json(item) for item in value) + "]"
    if isinstance(value, dict):
        items = sorted(value.items(), key=lambda kv: kv[0])
        return "{" + ",".join(_escape_string(k) + ":" + canonical_json(v) for k, v in items) + "}"
    raise TypeError(f"unsupported JSON value {type(value).__name__}")


def sha256_tagged(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def link_hash(prev_hash: str, event: Dict[str, Any]) -> str:
    return sha256_tagged((prev_hash + "\n" + canonical_json(event)).encode("utf-8"))


# --------------------------------------------------------------------------
# Verification
# --------------------------------------------------------------------------

class Report:
    def __init__(self, bundle: Path) -> None:
        self.bundle = str(bundle)
        self.checks: List[Dict[str, Any]] = []
        self.facts: Dict[str, Any] = {}

    def check(self, name: str, ok: bool, detail: str = "") -> bool:
        self.checks.append({"check": name, "ok": bool(ok), "detail": detail})
        return ok

    @property
    def ok(self) -> bool:
        return all(c["ok"] for c in self.checks)

    def as_dict(self) -> Dict[str, Any]:
        return {"bundle": self.bundle, "ok": self.ok, "facts": self.facts, "checks": self.checks}


def _load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def _load_jsonl(path: Path) -> List[Dict[str, Any]]:
    rows = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                rows.append(json.loads(line))
    return rows


def verify_bundle(bundle: Path) -> Report:
    report = Report(bundle)
    try:
        manifest = _load_json(bundle / "manifest.json")
        provenance = _load_json(bundle / "provenance.json")
        replay = _load_json(bundle / "replay.json")
        events = _load_jsonl(bundle / "events.jsonl")
        decisions = _load_jsonl(bundle / "decisions.jsonl")
    except (OSError, ValueError) as error:
        report.check("bundle_readable", False, str(error))
        return report
    report.check("bundle_readable", True)
    report.check(
        "bundle_format",
        provenance.get("bundle_format") == BUNDLE_FORMAT,
        str(provenance.get("bundle_format")),
    )

    # Files on disk match the digests recorded at save time.
    mismatched = []
    for name, expected in sorted(provenance.get("file_hashes", {}).items()):
        path = bundle / name
        actual = sha256_tagged(path.read_bytes()) if path.is_file() else "missing"
        if actual != expected:
            mismatched.append(name)
    report.check("file_hashes", not mismatched, ", ".join(mismatched))

    # Genesis: the hash of the resolved run configuration.
    genesis = sha256_tagged(canonical_json(manifest).encode("utf-8"))
    report.facts["genesis_hash"] = genesis
    report.check(
        "genesis_is_manifest_hash",
        genesis == provenance.get("run_config_hash") == replay.get("genesis_hash"),
        f"computed {genesis}",
    )

    # The hash chain, recomputed link by link.
    head = genesis
    broken: Optional[str] = None
    for index, line in enumerate(events):
        body = dict(line)
        prev_hash = body.pop("prev_hash", None)
        recorded = body.pop("hash", None)
        if prev_hash != head:
            broken = f"event #{index} ({body.get('event_id')}) prev_hash does not match the previous event"
            break
        computed = link_hash(head, body)
        if computed != recorded:
            broken = f"event #{index} ({body.get('event_id')}) hash does not match its content"
            break
        head = computed
    report.facts["head_hash"] = head if broken is None else None
    report.facts["events"] = len(events)
    report.check("event_chain", broken is None, broken or "")
    if broken is not None:
        return report
    report.check("head_hash_matches_replay", head == replay.get("head_hash"), head)

    # Counts claimed by replay.json.
    counts: Dict[str, int] = {"events": len(events), "decisions": len(decisions)}
    for event in events:
        key = "events." + event["event_type"]
        counts[key] = counts.get(key, 0) + 1
    claimed = replay.get("counts", {})
    differing = sorted(k for k in set(counts) | set(claimed) if counts.get(k) != claimed.get(k))
    report.check("counts_match_replay", not differing, ", ".join(differing))
    report.check("event_count", replay.get("event_count") == len(events))

    # Envelope basics.
    run_id = manifest.get("run_id")
    bad_seq = [i for i, e in enumerate(events) if e.get("seq") != i]
    report.check("seq_contiguous", not bad_seq, f"first gap at {bad_seq[0]}" if bad_seq else "")
    foreign = [e["event_id"] for e in events if e.get("run_id") != run_id]
    report.check("single_run_id", not foreign, ", ".join(foreign[:5]))
    bad_mode = [e["event_id"] for e in events if e.get("mode") not in MODES]
    report.check("mode_values", not bad_mode, ", ".join(bad_mode[:5]))
    ids = [e["event_id"] for e in events]
    report.check("unique_event_ids", len(ids) == len(set(ids)))

    by_id = {e["event_id"]: e for e in events}

    # Execution law, from the record.
    commits = [
        e for e in events
        if e["event_type"] == "state_transition" and e["payload"].get("mutation") == "commit"
    ]
    unauthorized = []
    for commit in commits:
        proposal_id = commit["payload"].get("proposal_id")
        names_pass = any(
            by_id.get(ref, {}).get("event_type") == "validation"
            and by_id[ref]["payload"].get("accepted") is True
            and by_id[ref]["payload"].get("proposal_id") == proposal_id
            for ref in commit["payload"].get("authorized_by", [])
        )
        if not names_pass:
            unauthorized.append(commit["event_id"])
    report.facts["commits"] = len(commits)
    report.check(
        "commit_requires_validation_pass",
        not unauthorized,
        ", ".join(unauthorized[:5]),
    )

    misplaced = [
        j["event_id"] for j in events
        if j["event_type"] == "judgment"
        and not (
            by_id.get(j["causation_id"], {}).get("event_type") == "validation"
            and by_id[j["causation_id"]]["payload"].get("accepted") is True
        )
    ]
    report.check("judgment_only_after_validation_pass", not misplaced, ", ".join(misplaced[:5]))

    # Every authoritative mutation continues the previous state hash.
    transitions = [e for e in events if e["event_type"] == "state_transition"]
    gaps = []
    for before, after in zip(transitions, transitions[1:]):
        if after["payload"].get("before_hash") != before["payload"].get("after_hash"):
            gaps.append(after["event_id"])
        if after["payload"].get("revision") != before["payload"].get("revision", 0) + 1:
            gaps.append(after["event_id"])
    report.check("state_hash_sequence", not gaps, ", ".join(sorted(set(gaps))[:5]))

    mislabeled = [
        e["event_id"] for e in events
        if (e.get("mode") == "LIVE" and e["event_type"] not in HUMAN_ACTION_EVENTS)
        or (e["event_type"] == "human_state" and e["payload"].get("datum", {}).get("mode") != e.get("mode"))
    ]
    report.check("data_mode_labels", not mislabeled, ", ".join(mislabeled[:5]))

    # The decision log agrees with the event log.
    committed_ids = {c["payload"].get("proposal_id") for c in commits}
    decision_commits = {d["proposal_id"] for d in decisions if d.get("committed")}
    report.check(
        "decisions_agree_with_events",
        committed_ids == decision_commits,
        f"{len(committed_ids ^ decision_commits)} proposal ids differ",
    )
    missing_refs = [
        d["proposal_id"] for d in decisions
        if d.get("validation", {}).get("event_id") not in by_id
    ]
    report.check("decision_event_refs", not missing_refs, ", ".join(missing_refs[:5]))
    return report


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("bundles", nargs="+", type=Path)
    parser.add_argument("--json", action="store_true", help="print machine-readable results")
    args = parser.parse_args(argv)
    reports = [verify_bundle(path) for path in args.bundles]
    if args.json:
        print(json.dumps([r.as_dict() for r in reports], indent=2))
    else:
        for report in reports:
            status = "OK  " if report.ok else "FAIL"
            facts = report.facts
            print(
                f"{status} {report.bundle}: {facts.get('events', 0)} events, "
                f"{facts.get('commits', 0)} commits, head {str(facts.get('head_hash'))[:19]}"
            )
            for check in report.checks:
                if not check["ok"]:
                    print(f"     - {check['check']}: {check['detail']}")
    return 0 if all(r.ok for r in reports) else 1


if __name__ == "__main__":
    sys.exit(main())
