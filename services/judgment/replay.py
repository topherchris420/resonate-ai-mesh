"""Replay recorded judgment envelopes without calling a remote provider.

Live reevaluation is a separate comparison. It never replaces the recorded envelope.
"""

import copy
from typing import Any, Dict, List


def replay_recorded_judgment(event: Dict[str, Any]) -> Dict[str, Any]:
    payload = event.get("payload") or {}
    recorded = payload.get("judgment")
    if not isinstance(recorded, dict):
        raise ValueError("judgment event is missing a recorded envelope")
    replayed = copy.deepcopy(recorded)
    replayed["evaluation_mode"] = "RECORDED_JUDGMENT"
    return replayed


def compare_reevaluation(recorded: Dict[str, Any], live: Dict[str, Any]) -> Dict[str, Any]:
    recorded_answers = {
        item.get("question_id"): _summary(item) for item in recorded.get("answers", [])
    }
    live_answers = {item.get("question_id"): _summary(item) for item in live.get("answers", [])}
    question_ids: List[str] = []
    for question_id in list(recorded_answers) + list(live_answers):
        if question_id not in question_ids:
            question_ids.append(question_id)
    questions = []
    for question_id in question_ids:
        left = recorded_answers.get(question_id, "missing")
        right = live_answers.get(question_id, "missing")
        questions.append(
            {
                "question_id": question_id,
                "agreement": left == right,
                "left": left,
                "right": right,
            }
        )
    agreement = all(item["agreement"] for item in questions) if questions else True
    return {
        "recorded_judgment_id": recorded.get("judgment_id"),
        "reevaluation_judgment_id": live.get("judgment_id"),
        "drift_detected": not agreement,
        "agreement": agreement,
        "questions": questions,
    }


def local_mock_judgment_event(timestamp: int, correlation_id: str = "corr_judgment") -> Dict[str, Any]:
    """Deterministic offline envelope used when a session is recorded without TypeSafe."""
    envelope = {
        "schema_version": "pordenone.judgment.envelope.v1",
        "judgment_id": "jdg_recorded_local",
        "provider": "deterministic_mock",
        "model": "deterministic-mock",
        "provider_model_version": "deterministic-mock",
        "question_set_version": "pordenone.judgment.questions.v1",
        "state_hash": "sha256:recorded-local",
        "state_schema_version": "pordenone.judgment.state.v1",
        "truncated": False,
        "proposal_id": "prop_recorded_local",
        "correlation_id": correlation_id,
        "causation_id": "val_recorded_local",
        "answers": [
            {
                "question_id": "proposal_support",
                "primitive": "choice",
                "choice": "supported",
                "score": None,
                "noul": None,
                "probabilities": {"supported": 0.91, "mixed": 0.09},
                "confidence": 0.88,
                "legend": {},
            },
            {
                "question_id": "contradiction_present",
                "primitive": "noul",
                "choice": None,
                "score": None,
                "noul": 0.07,
                "probabilities": {},
                "confidence": None,
                "legend": {},
            },
        ],
        "disposition": "PASS",
        "reason_codes": ["ELIGIBLE_PASS"],
        "policy_version": "pordenone.judgment.policy.v1",
        "requested_at": timestamp,
        "completed_at": timestamp,
        "latency_ms": 0,
        "provider_status": "ok",
        "evaluation_mode": "LIVE",
        "simulation_label": "SIMULATED",
        "provider_request_id": None,
        "input_tokens": None,
        "output_tokens": None,
    }
    return {
        "event_id": "rec_judgment_0",
        "event_type": "judgment",
        "schema_version": "1.1.0",
        "timestamp": timestamp,
        "source": "kernel_core",
        "subject_id": "agent_alpha",
        "correlation_id": correlation_id,
        "causation_id": "val_recorded_local",
        "provenance": "Judgment:deterministic_mock:pordenone.judgment.questions.v1",
        "payload": {"judgment": envelope},
    }


def _summary(answer: Dict[str, Any]) -> str:
    if answer.get("choice"):
        return f"choice:{answer['choice']}"
    if answer.get("score") is not None:
        return f"score:{answer['score']}"
    if answer.get("noul") is not None:
        return f"noul:{answer['noul']}"
    return "empty"
