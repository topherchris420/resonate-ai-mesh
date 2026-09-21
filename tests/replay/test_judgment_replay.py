import copy
import os
import socket
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../services"))

from judgment.replay import compare_reevaluation, local_mock_judgment_event, replay_recorded_judgment


def test_replay_uses_recorded_judgment_and_makes_no_network_request(monkeypatch):
    def reject_network(*_args, **_kwargs):
        raise AssertionError("replay opened a network socket")

    monkeypatch.setattr(socket, "socket", reject_network)
    event = local_mock_judgment_event(1_700_000_000_000)
    original = copy.deepcopy(event)
    replayed = replay_recorded_judgment(event)

    assert replayed["evaluation_mode"] == "RECORDED_JUDGMENT"
    assert replayed["answers"] == original["payload"]["judgment"]["answers"]
    assert replayed["disposition"] == "PASS"
    assert replayed["simulation_label"] == "SIMULATED"
    assert event == original
    assert original["payload"]["judgment"]["evaluation_mode"] == "LIVE"


def test_live_reevaluation_records_drift_without_overwriting():
    recorded_event = local_mock_judgment_event(1_700_000_000_000)
    recorded = recorded_event["payload"]["judgment"]
    live = copy.deepcopy(recorded)
    live["judgment_id"] = "jdg_reeval"
    live["provider_model_version"] = "jev-1.y"
    live["evaluation_mode"] = "LIVE_REEVALUATION"
    live["answers"][0]["choice"] = "mixed"
    before = copy.deepcopy(recorded)

    comparison = compare_reevaluation(recorded, live)
    assert comparison["drift_detected"] is True
    assert comparison["agreement"] is False
    support = next(item for item in comparison["questions"] if item["question_id"] == "proposal_support")
    assert support["left"] == "choice:supported"
    assert support["right"] == "choice:mixed"
    assert recorded == before
