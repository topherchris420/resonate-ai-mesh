from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Dict, Any, List, Optional
import os

from emergence_sim import SwarmEmergenceSimulator
from drr_adapter import DynamicResonanceRootingAdapter

app = FastAPI(title="Pordenone Sim Engine", version="0.1.0")

seed = int(os.getenv("SEED", "42"))
sim = SwarmEmergenceSimulator(num_agents=10, seed=seed)
drr = DynamicResonanceRootingAdapter()

class StepRequest(BaseModel):
    steps: Optional[int] = 1

class ScenarioRequest(BaseModel):
    seed: int
    num_agents: int = 10
    dt: float = 0.1

class DRREvaluateRequest(BaseModel):
    cognitive_load: float
    environmental_entropy: Optional[float] = None

@app.get("/health")
def health():
    return {"status": "ok", "service": "sim-engine", "seed": sim.seed, "step": sim.step_count}

@app.get("/state")
def get_state():
    entropy = sim.compute_entropy()
    adaptive_state = drr.evaluate_adaptive_state(0.45, entropy)
    return {
        "sim_step": sim.step_count,
        "num_agents": sim.num_agents,
        "environmental_entropy": entropy,
        "adaptive_state": adaptive_state,
    }

@app.post("/step")
def step(req: StepRequest):
    last_state = {}
    for _ in range(req.steps):
        last_state = sim.step()
    return last_state

@app.post("/scenario/load")
def load_scenario(req: ScenarioRequest):
    global sim
    sim = SwarmEmergenceSimulator(num_agents=req.num_agents, seed=req.seed, dt=req.dt)
    return {"status": "scenario_loaded", "seed": req.seed, "num_agents": req.num_agents}

@app.post("/drr/evaluate")
def evaluate_drr(req: DRREvaluateRequest):
    entropy = req.environmental_entropy if req.environmental_entropy is not None else sim.compute_entropy()
    res = drr.evaluate_adaptive_state(req.cognitive_load, entropy)
    return res

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
