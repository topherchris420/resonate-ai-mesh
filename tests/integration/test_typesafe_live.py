"""Manual check of the live TypeSafe System One response shape.

Skipped unless MESH_LIVE_TESTS=1 and TYPESAFE_API_KEY are both set, so the
default test run and CI never make network calls. The mesh's own bounded
remote-judgment path is exercised with `mesh run ... --allow-network`; see
docs/jev.md.
"""

import json
import os
import urllib.request

import pytest


pytestmark = pytest.mark.skipif(
    os.environ.get("MESH_LIVE_TESTS") != "1" or not os.environ.get("TYPESAFE_API_KEY"),
    reason="live network test: set MESH_LIVE_TESTS=1 and TYPESAFE_API_KEY to run",
)


def test_live_typesafe_system_one_shape():
    api_key = os.environ["TYPESAFE_API_KEY"]
    body = {
        "model": os.environ.get("JUDGMENT_MODEL", "jev-latest"),
        "state": {
            "simulation_label": "SIMULATED",
            "proposal": {
                "proposal_id": "prop_live",
                "agent_id": "agent_alpha",
                "action_type": "PATROL",
                "priority": 1,
            },
        },
        "questions": {
            "contradiction_present": {
                "type": "noul",
                "instructions": "The supplied evidence meaningfully contradicts the proposal.",
            }
        },
    }
    request = urllib.request.Request(
        "https://api.typesafe.ai/v1/systemone",
        data=json.dumps(body).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.loads(response.read().decode("utf-8"))
    assert str(payload.get("model", "")).startswith("jev-")
    answer = payload["answers"]["contradiction_present"]
    assert answer["type"] == "noul"
    assert 0.0 <= float(answer["noul"]) <= 1.0
    assert "confidence" not in answer or answer.get("confidence") is None
