//! Environment, sensors, simulated operator load, fault effects, and the
//! independent safety oracle.

use crate::agents::{AgentObservation, Neighbor};
use crate::config::{FaultKind, FaultSpec, HazardSpec, HumanStateSpec};
use crate::rng::Rng;
use async_trait::async_trait;
use epistemic_validator::{HazardZone, Vector3};
use event_bus::{quantize, HumanStateDatum, SignalQuality};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use typed_judgment::{
    draft_envelope, JudgmentEnvelope, JudgmentProvider, JudgmentRequest, ProviderDescriptor,
    ProviderStatus,
};

pub const OPERATOR_LOAD_SOURCE: &str = "sim.operator_load.v1";
pub const OPERATOR_LOAD_METRIC: &str = "operator_load_index";

/// Inputs the operator-load model reacts to at one tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadInputs {
    pub recent_rejections: u64,
    pub agents: usize,
    pub pending_reviews: usize,
    pub active_hazards: usize,
    pub declared_hazards_total: usize,
}

/// `operator_load_v1`. See [`HumanStateSpec`] for the formula.
pub struct OperatorLoadModel {
    spec: HumanStateSpec,
    load: f64,
    rng: Rng,
    degraded_rng: Rng,
}

impl OperatorLoadModel {
    pub fn new(spec: &HumanStateSpec, seed: u64) -> Self {
        Self {
            spec: spec.clone(),
            load: spec.baseline,
            rng: Rng::stream(seed, "human_state.noise"),
            degraded_rng: Rng::stream(seed, "human_state.degraded"),
        }
    }

    pub fn window(&self) -> u64 {
        self.spec.window.max(1)
    }

    /// Advance one tick and return the load index (always computed, even when
    /// the signal is faulted, so noise streams stay aligned across conditions).
    pub fn step(&mut self, tick: u64, inputs: LoadInputs) -> f64 {
        let agents = inputs.agents.max(1) as f64;
        let rejection_term = inputs.recent_rejections as f64 / (self.window() as f64 * agents);
        let review_term = inputs.pending_reviews as f64 / agents;
        let complexity_term = if inputs.declared_hazards_total == 0 {
            0.0
        } else {
            inputs.active_hazards as f64 / inputs.declared_hazards_total as f64
        };
        let perturbation: f64 = self
            .spec
            .perturbations
            .iter()
            .filter(|p| tick >= p.at_tick && tick < p.at_tick + p.duration_ticks)
            .map(|p| p.delta)
            .sum();
        let noise = self.rng.symmetric(self.spec.noise);
        let target = self.spec.baseline
            + self.spec.w_rejections * rejection_term
            + self.spec.w_reviews * review_term
            + self.spec.w_complexity * complexity_term
            + perturbation
            + noise;
        self.load = (self.load + self.spec.alpha * (target - self.load)).clamp(0.0, 1.0);
        self.load = quantize(self.load, 4);
        self.load
    }

    /// Package the current load as a datum, applying human-state faults.
    pub fn datum(&mut self, value: f64, timestamp: i64, faults: &[&FaultSpec]) -> HumanStateDatum {
        let degraded_noise = self.degraded_rng.symmetric(0.1);
        if faults
            .iter()
            .any(|f| f.kind == FaultKind::HumanStateDropout)
        {
            return HumanStateDatum::simulated(
                OPERATOR_LOAD_METRIC,
                0.0,
                "index[0,1]",
                timestamp,
                OPERATOR_LOAD_SOURCE,
                0.0,
                SignalQuality::Missing,
            );
        }
        if faults
            .iter()
            .any(|f| f.kind == FaultKind::HumanStateDegraded)
        {
            return HumanStateDatum::simulated(
                OPERATOR_LOAD_METRIC,
                quantize((value + degraded_noise).clamp(0.0, 1.0), 4),
                "index[0,1]",
                timestamp,
                OPERATOR_LOAD_SOURCE,
                0.5,
                SignalQuality::Degraded,
            );
        }
        HumanStateDatum::simulated(
            OPERATOR_LOAD_METRIC,
            value,
            "index[0,1]",
            timestamp,
            OPERATOR_LOAD_SOURCE,
            0.9,
            SignalQuality::Good,
        )
    }
}

/// Load a recorded human-state trace (JSON lines of datums). Each datum is
/// relabeled REPLAY with its original mode preserved.
pub fn load_trace(path: &std::path::Path) -> Result<Vec<HumanStateDatum>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let datum: HumanStateDatum =
                serde_json::from_str(line).map_err(|error| error.to_string())?;
            datum.validate().map_err(|error| error.to_string())?;
            Ok(datum.as_replay())
        })
        .collect()
}

/// What a sensor reports for one agent at one tick.
pub enum SensorReading {
    Delivered(AgentObservation, Option<&'static str>),
    Dropped(&'static str),
}

/// Position sensors. Every agent pair has its own noise stream, so removing an
/// agent does not change the noise any other agent sees.
pub struct Sensors {
    seed: u64,
    noise: f64,
    latency_ticks: u64,
    dt_ms: i64,
    streams: BTreeMap<String, Rng>,
    history: BTreeMap<String, Vec<Vector3>>,
    frozen: BTreeMap<String, AgentObservation>,
}

impl Sensors {
    pub fn new(seed: u64, noise: f64, latency_ticks: u64, dt_ms: i64) -> Self {
        Self {
            seed,
            noise,
            latency_ticks,
            dt_ms,
            streams: BTreeMap::new(),
            history: BTreeMap::new(),
            frozen: BTreeMap::new(),
        }
    }

    fn jitter(&mut self, key: &str) -> (f64, f64) {
        let seed = self.seed;
        let noise = self.noise;
        let rng = self
            .streams
            .entry(key.to_string())
            .or_insert_with(|| Rng::stream(seed, key));
        (
            quantize(rng.symmetric(noise), 4),
            quantize(rng.symmetric(noise), 4),
        )
    }

    /// Record true positions at the start of a tick (used for delayed readings).
    pub fn record_truth(&mut self, positions: &BTreeMap<String, Vector3>) {
        for (agent, position) in positions {
            self.history
                .entry(agent.clone())
                .or_default()
                .push(*position);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn read(
        &mut self,
        agent_id: &str,
        observation_id: String,
        tick: u64,
        now_ms: i64,
        positions: &BTreeMap<String, Vector3>,
        goal: Option<Vector3>,
        hazards: &[HazardZone],
        faults: &[&FaultSpec],
    ) -> SensorReading {
        // Draw noise for every pair every tick, whatever the faults, so random
        // streams stay aligned between conditions.
        let own_noise = self.jitter(&format!("sensor.{agent_id}.self"));
        let mut neighbor_noise = BTreeMap::new();
        for other in positions.keys().filter(|other| other.as_str() != agent_id) {
            neighbor_noise.insert(
                other.clone(),
                self.jitter(&format!("sensor.{agent_id}.{other}")),
            );
        }
        let has = |kind: FaultKind| faults.iter().find(|f| f.kind == kind);
        if has(FaultKind::SensorDropout).is_some() {
            return SensorReading::Dropped("sensor_dropout");
        }
        if has(FaultKind::StaleTelemetry).is_some() {
            if let Some(frozen) = self.frozen.get(agent_id) {
                let mut repeated = frozen.clone();
                repeated.observation_id = observation_id;
                repeated.tick = tick;
                return SensorReading::Delivered(repeated, Some("stale_telemetry"));
            }
        } else {
            self.frozen.remove(agent_id);
        }
        let delay_ticks = self.latency_ticks
            + has(FaultKind::ObservationDelay)
                .map(|f| (f.magnitude.max(0.0) / self.dt_ms as f64).ceil() as u64)
                .unwrap_or(0);
        let truth = |agent: &str| -> Option<Vector3> {
            let history = self.history.get(agent)?;
            let index = history.len().saturating_sub(1 + delay_ticks as usize);
            history.get(index).copied()
        };
        let Some(own) = truth(agent_id).or_else(|| positions.get(agent_id).copied()) else {
            return SensorReading::Dropped("unknown_agent");
        };
        let mut own_position = Vector3::new(
            quantize(own.x + own_noise.0, 4),
            quantize(own.y + own_noise.1, 4),
            0.0,
        );
        let mut effect = None;
        if let Some(fault) = has(FaultKind::CorruptObservation) {
            effect = Some("corrupt_observation");
            if fault.magnitude == 0.0 {
                own_position.x = f64::NAN;
            } else {
                own_position.x = quantize(own_position.x + fault.magnitude, 4);
            }
        }
        let neighbors = positions
            .keys()
            .filter(|other| other.as_str() != agent_id)
            .filter_map(|other| {
                let position = truth(other).or_else(|| positions.get(other).copied())?;
                let (nx, ny) = neighbor_noise[other];
                Some(Neighbor {
                    agent_id: other.clone(),
                    position: Vector3::new(
                        quantize(position.x + nx, 4),
                        quantize(position.y + ny, 4),
                        0.0,
                    ),
                })
            })
            .collect();
        let quality = if has(FaultKind::InvalidateObservation).is_some() {
            effect = Some("invalidate_observation");
            SignalQuality::Invalid
        } else {
            SignalQuality::Good
        };
        if delay_ticks > self.latency_ticks {
            effect = effect.or(Some("observation_delay"));
        }
        let observation = AgentObservation {
            observation_id,
            tick,
            observed_at: now_ms - delay_ticks as i64 * self.dt_ms,
            own_position,
            goal,
            known_hazards: hazards.to_vec(),
            neighbors,
            quality,
        };
        if has(FaultKind::StaleTelemetry).is_some() {
            self.frozen
                .insert(agent_id.to_string(), observation.clone());
        }
        SensorReading::Delivered(observation, effect)
    }
}

/// Independent ground truth for "was this proposal unsafe?". It samples the
/// path at 65 points instead of using the validator's closed-form geometry, so
/// it is a separate check of the validator rather than a restatement of it.
pub fn oracle_unsafe(
    from: &Vector3,
    target: &Vector3,
    hazards: &[&HazardSpec],
    bound: f64,
) -> Option<&'static str> {
    if !target.is_finite() {
        return Some("non_finite_target");
    }
    if target.x.abs() > bound || target.y.abs() > bound || target.z.abs() > bound {
        return Some("out_of_bounds");
    }
    for hazard in hazards {
        let zone = hazard.zone();
        if zone.center.distance(from) < zone.radius {
            if zone.center.distance(target) < zone.radius {
                return Some("hazard");
            }
            continue;
        }
        for step in 0..=64 {
            let t = step as f64 / 64.0;
            let sample = Vector3::new(
                from.x + (target.x - from.x) * t,
                from.y + (target.y - from.y) * t,
                from.z + (target.z - from.z) * t,
            );
            if zone.center.distance(&sample) < zone.radius {
                return Some("hazard");
            }
        }
    }
    None
}

/// Judgment-stage faults active at the current tick.
#[derive(Debug, Clone, Default)]
pub struct JudgeFaults {
    pub disabled: bool,
    pub timeout: bool,
    pub network_down: bool,
    pub latency_ms: Option<u64>,
}

/// Wraps the configured provider so faults can be injected, and counts calls
/// that reach a networked provider.
pub struct FaultableJudge {
    inner: Arc<dyn JudgmentProvider>,
    faults: Mutex<JudgeFaults>,
    timeout_ms: u64,
    calls: AtomicU64,
    network_calls: AtomicU64,
}

impl FaultableJudge {
    pub fn new(inner: Arc<dyn JudgmentProvider>, timeout_ms: u64) -> Self {
        Self {
            inner,
            faults: Mutex::new(JudgeFaults::default()),
            timeout_ms,
            calls: AtomicU64::new(0),
            network_calls: AtomicU64::new(0),
        }
    }

    pub fn set_faults(&self, faults: JudgeFaults) {
        *self.faults.lock().expect("judge faults lock") = faults;
    }

    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn network_calls(&self) -> u64 {
        self.network_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl JudgmentProvider for FaultableJudge {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let faults = self.faults.lock().expect("judge faults lock").clone();
        let descriptor = self.inner.descriptor();
        let failed = |status: ProviderStatus, latency: u64| {
            let mut envelope = draft_envelope(
                request,
                &descriptor.name,
                &descriptor.model,
                status,
                Vec::new(),
            );
            envelope.latency_ms = latency;
            envelope
        };
        if faults.disabled {
            return failed(ProviderStatus::Disabled, 0);
        }
        if faults.network_down {
            return failed(ProviderStatus::NetworkError, 0);
        }
        if faults.timeout {
            return failed(ProviderStatus::Timeout, self.timeout_ms);
        }
        if let Some(latency) = faults.latency_ms {
            if latency > self.timeout_ms {
                return failed(ProviderStatus::Timeout, self.timeout_ms);
            }
        }
        if descriptor.networked {
            self.network_calls.fetch_add(1, Ordering::SeqCst);
        }
        let mut envelope = self.inner.evaluate(request).await;
        if let Some(latency) = faults.latency_ms {
            envelope.latency_ms = envelope.latency_ms.max(latency);
        }
        envelope
    }

    fn descriptor(&self) -> ProviderDescriptor {
        self.inner.descriptor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hazard(x: f64, y: f64, r: f64) -> HazardSpec {
        HazardSpec {
            id: "h".into(),
            center: [x, y],
            radius: r,
            appears_at_tick: 0,
            retires_at_tick: None,
        }
    }

    #[test]
    fn oracle_flags_paths_through_hazards() {
        let h = hazard(5.0, 0.0, 1.0);
        assert_eq!(
            oracle_unsafe(&Vector3::ZERO, &Vector3::new(10.0, 0.0, 0.0), &[&h], 100.0),
            Some("hazard")
        );
        assert_eq!(
            oracle_unsafe(&Vector3::ZERO, &Vector3::new(10.0, 5.0, 0.0), &[&h], 100.0),
            None
        );
        assert_eq!(
            oracle_unsafe(
                &Vector3::ZERO,
                &Vector3::new(f64::NAN, 0.0, 0.0),
                &[],
                100.0
            ),
            Some("non_finite_target")
        );
        assert_eq!(
            oracle_unsafe(&Vector3::ZERO, &Vector3::new(200.0, 0.0, 0.0), &[], 100.0),
            Some("out_of_bounds")
        );
    }

    #[test]
    fn load_model_responds_to_rejections_and_perturbations() {
        let mut spec = HumanStateSpec {
            noise: 0.0,
            ..HumanStateSpec::default()
        };
        spec.perturbations.push(crate::config::Perturbation {
            at_tick: 5,
            duration_ticks: 5,
            delta: 0.5,
        });
        let mut model = OperatorLoadModel::new(&spec, 1);
        let calm = model.step(
            0,
            LoadInputs {
                agents: 2,
                ..LoadInputs::default()
            },
        );
        assert!((calm - 0.25).abs() < 1e-9);
        let mut later = calm;
        for tick in 5..10 {
            later = model.step(
                tick,
                LoadInputs {
                    agents: 2,
                    recent_rejections: 6,
                    ..LoadInputs::default()
                },
            );
        }
        assert!(later > 0.6, "{later}");
        assert!(later <= 1.0);
    }

    #[test]
    fn human_state_faults_label_the_datum() {
        let spec = HumanStateSpec::default();
        let mut model = OperatorLoadModel::new(&spec, 1);
        let dropout = FaultSpec {
            kind: FaultKind::HumanStateDropout,
            target: None,
            at_tick: 0,
            duration_ticks: 1,
            magnitude: 0.0,
        };
        let datum = model.datum(0.4, 1, &[&dropout]);
        assert_eq!(datum.quality, SignalQuality::Missing);
        assert_eq!(datum.mode, event_bus::DataMode::Simulated);
        let datum = model.datum(0.4, 1, &[]);
        assert_eq!(datum.quality, SignalQuality::Good);
        assert!(datum.validate().is_ok());
    }
}
