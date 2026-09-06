import os
import sys
import subprocess
import pytest

def test_deterministic_session_replay(tmp_path):
    record_script = os.path.join(os.path.dirname(__file__), "../../scripts/record-session")
    replay_script = os.path.join(os.path.dirname(__file__), "../../scripts/replay-session")
    session_file = str(tmp_path / "recording.json")

    python_bin = sys.executable

    rec_res = subprocess.run([python_bin, record_script, session_file], capture_output=True, text=True)
    assert rec_res.returncode == 0, f"Recording failed: {rec_res.stderr}"
    assert os.path.exists(session_file)

    rep_res = subprocess.run([python_bin, replay_script, session_file], capture_output=True, text=True)
    assert rep_res.returncode == 0, f"Replay failed: {rep_res.stderr}"
    assert "REPLAY SUCCESSFUL" in rep_res.stdout
