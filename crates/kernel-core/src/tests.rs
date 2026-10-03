use super::*;
use async_trait::async_trait;
use epistemic_validator::Vector3;
use proptest::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;
use typed_judgment::{
    DeterministicMockJudgmentProvider, Disposition, EvidenceHeuristicJudge, MockScenario,
};

const T0: i64 = 1_700_000_000_000;

fn proposal(id: &str, action: &str, x: f64, correlation: &str) -> ActionProposal {
    ActionProposal {
        proposal_id: id.into(),
        agent_id: "agent_001".into(),
        action_type: action.into(),
        parameters_json: "{\"note\":\"do not send raw parameters\"}".into(),
        target_position: Vector3::new(x, 200.0, 0.0),
        priority: 1,
        timestamp: T0,
        correlation_id: correlation.into(),
        source_observation: "obs_001".into(),
        observed_at: Some(T0),
    }
}

fn agent(x: f64) -> AuthoritativeAgentState {
    AuthoritativeAgentState::idle("agent_001", Vector3::new(x, 0.0, 0.0), T0)
}

struct CountingProvider {
    calls: AtomicUsize,
    seen: StdMutex<String>,
    inner: Box<dyn JudgmentProvider>,
}

#[async_trait]
impl JudgmentProvider for CountingProvider {
    async fn evaluate(&self, request: &JudgmentRequest) -> JudgmentEnvelope {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.seen.lock().expect("seen") = request.prepared.canonical_json.clone();
        self.inner.evaluate(request).await
    }

    fn descriptor(&self) -> ProviderDescriptor {
        self.inner.descriptor()
    }
}

fn counting(scenario: MockScenario) -> Arc<CountingProvider> {
    counting_with(Box::new(DeterministicMockJudgmentProvider { scenario }))
}

fn counting_with(inner: Box<dyn JudgmentProvider>) -> Arc<CountingProvider> {
    Arc::new(CountingProvider {
        calls: AtomicUsize::new(0),
        seen: StdMutex::new(String::new()),
        inner,
    })
}

fn deterministic(provider: Option<Arc<dyn JudgmentProvider>>) -> (KernelEngine, Arc<ManualClock>) {
    let clock = Arc::new(ManualClock::new(T0));
    let mut builder = KernelEngine::builder(EventBus::new(256))
        .clock(clock.clone())
        .ids(Arc::new(SequentialIds::new()));
    if let Some(provider) = provider {
        builder = builder.judgment(provider, JudgmentPolicy::default());
    }
    (builder.build().unwrap(), clock)
}

fn types(events: &[EventEnvelope]) -> Vec<&str> {
    events
        .iter()
        .map(|event| event.event_type.as_str())
        .collect()
}

#[tokio::test]
async fn commit_and_rejection_lifecycle() {
    let (kernel, _) = deterministic(None);
    kernel.register_agent(agent(0.0)).await.unwrap();
    let ok = kernel
        .process_action_proposal_detailed(proposal("prop_valid", "MOVE", 100.0, "corr_001"))
        .await;
    assert!(ok.validation.accepted);
    assert!(ok.committed);
    let state = kernel.get_agent("agent_001").await.unwrap();
    assert_eq!(state.status, AgentStatus::Executing);
    assert_eq!(state.position.x, 100.0);
    assert_eq!(
        types(&ok.events),
        ["validation", "commitment", "state_transition"]
    );

    let bad = kernel
        .process_action_proposal_detailed(proposal("prop_invalid", "MOVE", 999_999.0, "corr_002"))
        .await;
    assert!(!bad.validation.accepted);
    assert_eq!(bad.decision.outcome, PolicyOutcome::RejectedDeterministic);
    assert_eq!(types(&bad.events), ["validation", "commitment"]);
    assert_eq!(
        kernel.get_agent("agent_001").await.unwrap().position.x,
        100.0
    );
}

#[tokio::test]
async fn a_resubmitted_proposal_is_rejected_as_a_duplicate() {
    let (kernel, _) = deterministic(None);
    kernel.register_agent(agent(0.0)).await.unwrap();
    let first = kernel
        .process_action_proposal_detailed(proposal("prop_dup", "MOVE", 3.0, "corr"))
        .await;
    assert!(first.committed);
    let second = kernel
        .process_action_proposal_detailed(proposal("prop_dup", "MOVE", 3.0, "corr"))
        .await;
    assert!(!second.committed);
    assert_eq!(second.validation.reasons, vec!["DUPLICATE_PROPOSAL"]);
}

#[tokio::test]
async fn unknown_agents_are_rejected_and_never_auto_registered() {
    let (kernel, _) = deterministic(None);
    let before = kernel.state_hash().await;
    let outcome = kernel
        .process_action_proposal_detailed(proposal("prop_ghost", "MOVE", 1.0, "corr"))
        .await;
    assert!(!outcome.committed);
    assert!(outcome
        .validation
        .reasons
        .contains(&"AGENT_NOT_REGISTERED".to_string()));
    assert!(kernel.get_agent("agent_001").await.is_none());
    assert_eq!(kernel.state_hash().await, before);
}

#[tokio::test]
async fn deterministic_rejection_does_not_call_judgment_or_move_state() {
    let counter = counting(MockScenario::Supported);
    let (kernel, _) = deterministic(Some(counter.clone()));
    kernel.register_agent(agent(5.0)).await.unwrap();
    let outcome = kernel
        .process_action_proposal_detailed(proposal("prop_bad", "MOVE", 999_999.0, "corr_bad"))
        .await;
    assert!(!outcome.validation.accepted);
    assert!(outcome.judgment.is_none());
    assert!(!outcome.committed);
    assert_eq!(counter.calls.load(Ordering::SeqCst), 0);
    assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 5.0);
    assert!(outcome
        .events
        .iter()
        .all(|event| event.event_type != "judgment"));
}

#[tokio::test]
async fn valid_proposal_reaches_provider_and_commits_on_pass_with_a_causal_chain() {
    let counter = counting(MockScenario::Supported);
    let (kernel, _) = deterministic(Some(counter.clone()));
    kernel.register_agent(agent(0.0)).await.unwrap();
    let outcome = kernel
        .process_proposal(
            proposal("prop_ok", "PATROL", 12.0, "corr_ok"),
            "evt-proposal",
        )
        .await;
    assert!(outcome.validation.accepted);
    assert_eq!(counter.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        outcome.judgment.as_ref().unwrap().disposition,
        Disposition::Pass
    );
    assert!(outcome.committed);
    assert_eq!(outcome.decision.basis, Some(CommitBasis::JudgmentPass));
    assert_eq!(
        kernel.get_agent("agent_001").await.unwrap().position.x,
        12.0
    );
    let [validation, judgment, commitment, transition] = &outcome.events[..] else {
        panic!("unexpected events {:?}", types(&outcome.events));
    };
    assert_eq!(validation.event_type, "validation");
    assert_eq!(validation.causation_id, "evt-proposal");
    assert_eq!(judgment.causation_id, validation.event_id);
    assert_eq!(commitment.causation_id, judgment.event_id);
    assert_eq!(transition.causation_id, commitment.event_id);
    assert!(outcome
        .events
        .iter()
        .all(|event| event.correlation_id == "corr_ok"));
    let applied = outcome.transition.unwrap();
    assert_eq!(
        applied.authorized_by,
        vec![validation.event_id.clone(), commitment.event_id.clone()]
    );
    assert_ne!(applied.before_hash, applied.after_hash);
    assert!(!judgment.ai_involved, "a deterministic mock is not a model");
}

#[tokio::test]
async fn revise_human_review_and_unavailable_do_not_commit() {
    for scenario in [
        MockScenario::Unsupported,
        MockScenario::HumanReview,
        MockScenario::LowConfidence,
        MockScenario::Timeout,
        MockScenario::Malformed,
        MockScenario::Disabled,
    ] {
        let (kernel, _) = deterministic(Some(counting(scenario)));
        kernel.register_agent(agent(0.0)).await.unwrap();
        let outcome = kernel
            .process_action_proposal_detailed(proposal("prop_hold", "INSPECT", 4.0, "corr"))
            .await;
        assert!(outcome.validation.accepted);
        assert!(!outcome.committed, "{scenario:?} committed");
        assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 0.0);
    }
}

#[tokio::test]
async fn human_review_requires_an_explicit_decision() {
    let (kernel, _) = deterministic(Some(counting(MockScenario::HumanReview)));
    kernel.register_agent(agent(0.0)).await.unwrap();
    let outcome = kernel
        .process_action_proposal_detailed(proposal("prop_human", "HOLD", 6.0, "corr_human"))
        .await;
    assert_eq!(outcome.decision.outcome, PolicyOutcome::AwaitingHumanReview);
    assert!(!outcome.committed);
    assert_eq!(kernel.pending_reviews().await, vec!["prop_human"]);
    assert!(kernel
        .resolve_human_review(HumanReviewDecision {
            proposal_id: "missing".into(),
            approve: true,
            operator_ref: "operator".into(),
            note: String::new(),
            simulated_operator: false,
        })
        .await
        .is_err());
    let rejected = kernel
        .resolve_human_review(HumanReviewDecision {
            proposal_id: "prop_human".into(),
            approve: false,
            operator_ref: "operator".into(),
            note: "withhold".into(),
            simulated_operator: false,
        })
        .await
        .unwrap();
    assert!(!rejected.committed);
    assert!(rejected
        .decision
        .reason_codes
        .contains(&"HUMAN_DECISION_REJECT".to_string()));
    assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 0.0);
    assert!(kernel.pending_reviews().await.is_empty());

    let (kernel, _) = deterministic(Some(counting(MockScenario::HumanReview)));
    kernel.register_agent(agent(0.0)).await.unwrap();
    kernel
        .process_action_proposal_detailed(proposal("prop_human", "HOLD", 6.0, "corr_human"))
        .await;
    let approved = kernel
        .resolve_human_review(HumanReviewDecision {
            proposal_id: "prop_human".into(),
            approve: true,
            operator_ref: "operator".into(),
            note: "reviewed".into(),
            simulated_operator: true,
        })
        .await
        .unwrap();
    assert!(approved.committed);
    assert_eq!(approved.decision.basis, Some(CommitBasis::HumanApproved));
    assert_eq!(
        types(&approved.events),
        [
            "human_resolution",
            "validation",
            "commitment",
            "state_transition"
        ]
    );
    assert_eq!(approved.events[0].mode, DataMode::Simulated);
    assert_eq!(
        approved.judgment.unwrap().disposition,
        Disposition::HumanReview,
        "the model disposition is never rewritten"
    );
    assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 6.0);
}

#[tokio::test]
async fn human_approval_cannot_override_a_failed_recheck() {
    let (kernel, _) = deterministic(Some(counting(MockScenario::HumanReview)));
    kernel.register_agent(agent(0.0)).await.unwrap();
    kernel
        .process_action_proposal_detailed(proposal("prop_h", "MOVE", 6.0, "corr"))
        .await;
    kernel
        .declare_hazard(
            HazardZone {
                id: "late_hazard".into(),
                center: Vector3::new(6.0, 200.0, 0.0),
                radius: 3.0,
            },
            "test",
        )
        .await
        .unwrap();
    let outcome = kernel
        .resolve_human_review(HumanReviewDecision {
            proposal_id: "prop_h".into(),
            approve: true,
            operator_ref: "operator".into(),
            note: String::new(),
            simulated_operator: false,
        })
        .await
        .unwrap();
    assert!(!outcome.committed);
    assert_eq!(
        outcome.decision.outcome,
        PolicyOutcome::RejectedDeterministic
    );
    assert!(outcome
        .decision
        .reason_codes
        .contains(&"HUMAN_APPROVAL_BLOCKED".to_string()));
    assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 0.0);
}

#[tokio::test]
async fn disabled_judgment_commits_on_deterministic_pass_without_a_judgment_event() {
    let (kernel, _) = deterministic(None);
    kernel.register_agent(agent(0.0)).await.unwrap();
    let outcome = kernel
        .process_action_proposal_detailed(proposal("prop_off", "MOVE", 3.0, "corr_off"))
        .await;
    assert!(outcome.committed);
    assert!(outcome.judgment.is_none());
    assert_eq!(outcome.decision.basis, Some(CommitBasis::DeterministicOnly));
    let summary = outcome.decision.judgment.unwrap();
    assert!(!summary.consulted);
    assert_eq!(summary.skip_reason.as_deref(), Some("JUDGMENT_DISABLED"));
    assert!(kernel.judgment_descriptor().is_none());
    assert!(outcome
        .events
        .iter()
        .all(|event| event.event_type != "judgment"));
}

#[tokio::test]
async fn provider_cannot_mutate_state_or_receive_raw_biosignals() {
    let counter = counting(MockScenario::Unsupported);
    let (kernel, _) = deterministic(Some(counter.clone()));
    kernel
        .set_operator_telemetry(serde_json::json!({
            "operator_load_index": 0.25,
            "is_simulated": true,
            "eeg": "RAW_EEG_BUFFER_9f3a",
            "api_key": "super-secret-key"
        }))
        .await;
    kernel.register_agent(agent(8.0)).await.unwrap();
    let outcome = kernel
        .process_action_proposal_detailed(proposal("prop_private", "MOVE", 9.0, "corr"))
        .await;
    assert!(!outcome.committed);
    assert_eq!(kernel.get_agent("agent_001").await.unwrap().position.x, 8.0);
    assert_eq!(counter.calls.load(Ordering::SeqCst), 1);
    let seen = counter.seen.lock().expect("seen").clone();
    assert!(!seen.contains("RAW_EEG_BUFFER_9f3a"));
    assert!(!seen.contains("super-secret-key"));
    assert!(!seen.contains("do not send raw parameters"));
    assert!(seen.contains("SIMULATED"));
    assert!(seen.contains("0.25"));
}

#[tokio::test]
async fn state_too_large_does_not_call_the_provider() {
    let counter = counting(MockScenario::Supported);
    let kernel = KernelEngine::with_limits(
        EventBus::new(16),
        counter.clone(),
        JudgmentPolicy::default(),
        StateLimits {
            max_bytes: 8,
            max_text_chars: 8,
        },
    );
    kernel.register_agent(agent(0.0)).await.unwrap();
    let mut p = proposal("prop_big", "MOVE", 1.0, "corr_big");
    let now = chrono::Utc::now().timestamp_millis();
    p.timestamp = now;
    p.observed_at = Some(now);
    let outcome = kernel.process_action_proposal_detailed(p).await;
    assert_eq!(counter.calls.load(Ordering::SeqCst), 0);
    assert!(!outcome.committed);
    let judgment = outcome.judgment.unwrap();
    assert_eq!(judgment.provider_status, ProviderStatus::StateTooLarge);
    assert_eq!(judgment.disposition, Disposition::Unavailable);
}

#[tokio::test]
async fn replay_uses_the_recorded_envelope_and_makes_no_provider_call() {
    let (seed, _) = deterministic(Some(counting(MockScenario::Supported)));
    seed.register_agent(agent(0.0)).await.unwrap();
    let seeded = seed
        .process_action_proposal_detailed(proposal("prop_replay", "MOVE", 15.0, "corr"))
        .await;
    let recorded = seeded.judgment.unwrap();
    assert_eq!(recorded.disposition, Disposition::Pass);

    let provider = Arc::new(RecordedJudgmentProvider::new(vec![recorded.clone()]));
    let kernel = KernelEngine::builder(EventBus::new(16))
        .clock(Arc::new(ManualClock::new(T0)))
        .ids(Arc::new(SequentialIds::new()))
        .replay_judgments(provider.clone(), JudgmentPolicy::default())
        .build()
        .unwrap();
    kernel.register_agent(agent(0.0)).await.unwrap();
    let outcome = kernel
        .process_action_proposal_detailed(proposal("prop_replay", "MOVE", 15.0, "corr"))
        .await;
    assert!(outcome.committed);
    let replayed = outcome.judgment.unwrap();
    assert_eq!(replayed.evaluation_mode, EvaluationMode::RecordedJudgment);
    assert_eq!(replayed.judgment_id, recorded.judgment_id);
    assert_eq!(replayed.answers, recorded.answers);
    assert_eq!(provider.network_call_count(), 0);
    assert_eq!(
        provider.stored("prop_replay").unwrap().evaluation_mode,
        EvaluationMode::Live,
        "the stored envelope is not overwritten"
    );
}

#[tokio::test]
async fn replay_refuses_a_recording_for_different_evidence() {
    let (seed, _) = deterministic(Some(counting(MockScenario::Supported)));
    seed.register_agent(agent(0.0)).await.unwrap();
    let recorded = seed
        .process_action_proposal_detailed(proposal("prop_r", "MOVE", 15.0, "corr"))
        .await
        .judgment
        .unwrap();
    let provider = Arc::new(RecordedJudgmentProvider::new(vec![recorded]));
    let kernel = KernelEngine::builder(EventBus::new(16))
        .clock(Arc::new(ManualClock::new(T0)))
        .replay_judgments(provider, JudgmentPolicy::default())
        .build()
        .unwrap();
    kernel.register_agent(agent(0.0)).await.unwrap();
    // Same id, different target: the evidence hash differs.
    let outcome = kernel
        .process_action_proposal_detailed(proposal("prop_r", "MOVE", 16.0, "corr"))
        .await;
    assert!(!outcome.committed);
    assert_eq!(
        outcome.judgment.unwrap().reason_codes,
        vec!["RECORDED_JUDGMENT_STATE_MISMATCH"]
    );
}

#[tokio::test]
async fn adaptive_gating_defers_background_work_under_high_operator_load() {
    let (kernel, _) = deterministic(None);
    kernel.register_agent(agent(0.0)).await.unwrap();
    let datum = HumanStateDatum::simulated(
        "operator_load_index",
        0.7,
        "index[0,1]",
        T0,
        "test",
        0.9,
        SignalQuality::Good,
    );
    let event = kernel.observe_human_state(&datum, "evt-hs").await.unwrap();
    assert_eq!(event.event_type, "adaptive_level");
    assert_eq!(event.payload["level"], "HIGH");
    let mut background = proposal("prop_bg", "PATROL", 2.0, "corr");
    background.priority = 0;
    let deferred = kernel.process_action_proposal_detailed(background).await;
    assert!(!deferred.committed);
    assert!(deferred
        .decision
        .reason_codes
        .contains(&"OPERATOR_LOAD_DEFERRAL".to_string()));
    let foreground = kernel
        .process_action_proposal_detailed(proposal("prop_fg", "PATROL", 2.0, "corr"))
        .await;
    assert!(foreground.committed);
    assert!(
        kernel
            .observe_human_state(&datum, "evt-hs2")
            .await
            .is_none(),
        "no change, no event"
    );
}

#[tokio::test]
async fn adaptive_gating_can_be_switched_off_for_ablation() {
    let kernel = KernelEngine::builder(EventBus::new(16))
        .policy(KernelPolicyConfig {
            adaptive: AdaptivePolicyConfig {
                enabled: false,
                ..AdaptivePolicyConfig::default()
            },
            ..KernelPolicyConfig::default()
        })
        .clock(Arc::new(ManualClock::new(T0)))
        .build()
        .unwrap();
    kernel.register_agent(agent(0.0)).await.unwrap();
    kernel.update_policy_for_cognitive_load(0.95).await;
    let mut background = proposal("prop_bg", "PATROL", 2.0, "corr");
    background.priority = 0;
    assert!(
        kernel
            .process_action_proposal_detailed(background)
            .await
            .committed
    );
}

#[tokio::test]
async fn invalid_human_state_drops_evidence_without_guessing_a_level() {
    let (kernel, _) = deterministic(None);
    let good = HumanStateDatum::simulated(
        "operator_load_index",
        0.5,
        "index[0,1]",
        T0,
        "t",
        0.9,
        SignalQuality::Good,
    );
    kernel.observe_human_state(&good, "e1").await;
    assert_eq!(kernel.adaptive_level().await, AdaptiveLevel::Elevated);
    let mut lost = good.clone();
    lost.quality = SignalQuality::Missing;
    lost.value = 0.0;
    let event = kernel.observe_human_state(&lost, "e2").await.unwrap();
    assert_eq!(event.payload["signal_usable"], false);
    assert_eq!(kernel.adaptive_level().await, AdaptiveLevel::Elevated);
}

#[tokio::test]
async fn routing_skips_judgment_for_stable_agents_only() {
    let counter = counting_with(Box::new(EvidenceHeuristicJudge));
    let kernel = KernelEngine::builder(EventBus::new(64))
        .judgment(counter.clone(), JudgmentPolicy::default())
        .policy(KernelPolicyConfig {
            routing: JudgmentRouting::OnAgentInstability {
                window: 4,
                threshold: 0.25,
            },
            ..KernelPolicyConfig::default()
        })
        .clock(Arc::new(ManualClock::new(T0)))
        .ids(Arc::new(SequentialIds::new()))
        .build()
        .unwrap();
    kernel.register_agent(agent(0.0)).await.unwrap();
    let first = kernel
        .process_action_proposal_detailed(proposal("p1", "MOVE", 1.0, "c"))
        .await;
    assert!(
        first.decision.judgment.as_ref().unwrap().consulted,
        "new agents are judged"
    );
    for (index, x) in [2.0, 3.0, 4.0].into_iter().enumerate() {
        kernel
            .process_action_proposal_detailed(proposal(&format!("p{}", index + 2), "MOVE", x, "c"))
            .await;
    }
    let calls_before = counter.calls.load(Ordering::SeqCst);
    let routed = kernel
        .process_action_proposal_detailed(proposal("p6", "MOVE", 5.0, "c"))
        .await;
    let summary = routed.decision.judgment.unwrap();
    assert!(!summary.consulted);
    assert_eq!(summary.skip_reason.as_deref(), Some("ROUTED_STABLE_AGENT"));
    assert_eq!(counter.calls.load(Ordering::SeqCst), calls_before);
    assert!(routed.committed);
}

async fn scripted_run(provider: Option<Arc<dyn JudgmentProvider>>) -> (Vec<EventEnvelope>, String) {
    let (kernel, clock) = deterministic(provider);
    let mut events = kernel.register_agent(agent(0.0)).await.unwrap().events;
    for step in 0..12 {
        clock.advance(250);
        let x = if step % 4 == 3 {
            50_000.0
        } else {
            step as f64 * 3.0
        };
        let mut p = proposal(&format!("prop_{step}"), "MOVE", x, &format!("corr_{step}"));
        p.timestamp = clock.now_ms();
        p.observed_at = Some(clock.now_ms() - 100);
        events.extend(kernel.process_action_proposal_detailed(p).await.events);
    }
    (events, kernel.state_hash().await)
}

#[tokio::test]
async fn identical_inputs_produce_identical_events_and_state() {
    for provider in [
        None,
        Some(Arc::new(EvidenceHeuristicJudge) as Arc<dyn JudgmentProvider>),
    ] {
        let (first_events, first_hash) = scripted_run(provider.clone()).await;
        let (second_events, second_hash) = scripted_run(provider).await;
        assert_eq!(first_events, second_events);
        assert_eq!(first_hash, second_hash);
    }
}

fn arbitrary_proposal() -> impl Strategy<Value = ActionProposal> {
    (
        prop_oneof![
            -200.0f64..200.0,
            Just(f64::NAN),
            Just(f64::INFINITY),
            9_000.0f64..20_000.0
        ],
        -200.0f64..200.0,
        prop_oneof![Just("MOVE"), Just("HOLD"), Just("SELF_DESTRUCT"), Just("")],
        prop_oneof![Just("agent_001"), Just("agent_002"), Just("ghost")],
        -2i32..12,
        -40_000i64..2_000,
    )
        .prop_map(|(x, y, action, agent_id, priority, age)| ActionProposal {
            proposal_id: format!("prop_{x}_{y}"),
            agent_id: agent_id.into(),
            action_type: action.into(),
            parameters_json: "{}".into(),
            target_position: Vector3::new(x, y, 0.0),
            priority,
            timestamp: T0 + age.max(-1),
            correlation_id: "corr".into(),
            source_observation: "obs".into(),
            observed_at: Some(T0 + age),
        })
}

fn scenario() -> impl Strategy<Value = MockScenario> {
    prop_oneof![
        Just(MockScenario::Supported),
        Just(MockScenario::Unsupported),
        Just(MockScenario::HumanReview),
        Just(MockScenario::LowConfidence),
        Just(MockScenario::Timeout),
        Just(MockScenario::Contradiction),
    ]
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// A rejected proposal can never alter authoritative state, and judgment
    /// is never consulted for it, whatever the provider would have said.
    #[test]
    fn rejected_proposals_never_change_state_or_reach_judgment(
        proposals in proptest::collection::vec(arbitrary_proposal(), 1..12),
        scenario in scenario(),
        judged in any::<bool>(),
    ) {
        runtime().block_on(async {
            let counter = counting(scenario);
            let (kernel, _) = deterministic(judged.then(|| counter.clone() as Arc<dyn JudgmentProvider>));
            kernel.register_agent(agent(0.0)).await.unwrap();
            kernel.register_agent(AuthoritativeAgentState::idle("agent_002", Vector3::new(50.0, 0.0, 0.0), T0)).await.unwrap();
            for p in proposals {
                let before = kernel.state_hash().await;
                let calls_before = counter.calls.load(Ordering::SeqCst);
                let outcome = kernel.process_action_proposal_detailed(p).await;
                let after = kernel.state_hash().await;
                if !outcome.validation.accepted {
                    prop_assert_eq!(&before, &after);
                    prop_assert!(!outcome.committed);
                    prop_assert!(outcome.judgment.is_none());
                    prop_assert_eq!(counter.calls.load(Ordering::SeqCst), calls_before);
                }
                if outcome.committed {
                    prop_assert!(outcome.validation.accepted);
                    prop_assert!(outcome.decision.permits_commit());
                    let transition = outcome.transition.as_ref().unwrap();
                    prop_assert_eq!(&transition.before_hash, &before);
                    prop_assert_eq!(&transition.after_hash, &after);
                    // Every committed transition names the validation and decision events.
                    let validation_id = &outcome.events[0].event_id;
                    prop_assert!(transition.authorized_by.contains(validation_id));
                    if let Some(judgment) = &outcome.judgment {
                        prop_assert_eq!(judgment.disposition, Disposition::Pass);
                    }
                } else {
                    prop_assert_eq!(&before, &after);
                }
            }
            Ok(())
        })?;
    }
}
