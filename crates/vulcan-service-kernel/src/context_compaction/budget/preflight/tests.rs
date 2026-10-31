//! Pre-dispatch budget regressions using real segmented persistence without provider calls.
//! 使用真实分段持久化、无供应商调用的分派前预算回归。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission,
};
use vulcan_session_core::v1::{
    ClientActionId, ClientInputSubmission, ContinuationEligibility,
    context::LockedBaselineProjection,
    document::{ApiCompletion, CompactionCommitInput, PrepareAndStartApiInput},
    event::{
        ApiCallStatus, ApiEndReason, ApiRequestSnapshot, AssistantSemanticStatus, TurnOutcome,
    },
    ids::ApiCallId,
    stats::UsageSnapshot,
};

/// Owns an isolated durable session and its frozen pre-dispatch inputs.
/// 持有隔离的可靠会话及其冻结的分派前输入。
struct Fixture {
    /// Keeps temporary storage alive for every reload.
    /// 为每次重载保持临时存储存活。
    _directory: tempfile::TempDir,
    /// Registers exactly this session's production handle.
    /// 只登记本会话的生产句柄。
    state: SharedServiceBridgeState,
    /// Owns durable mutations and full-history reads.
    /// 持有可靠变更及完整历史读取。
    handle: Arc<SessionHandle>,
    /// Freezes the route without credentials or a transport.
    /// 在不持有凭据或传输器的情况下冻结路由。
    route: ProviderRuntimeBlueprint,
    /// Models a low character estimate under the selected window.
    /// 模拟低于所选窗口的字符估算。
    budget: ContextBudgetSnapshot,
}

impl Fixture {
    /// Creates an open root turn in isolated storage; takes no arguments and dispatches nothing.
    /// 在隔离存储中创建开放根回合；无参数，不分派请求。
    fn new() -> Self {
        // Production storage validates the same document and start/stat pairs as Release.
        // 生产存储校验与 Release 相同的文档及起始、统计配对。
        let directory = tempfile::tempdir().unwrap();
        // Session identity remains stable across a simulated restart.
        // 会话身份跨模拟重启保持稳定。
        let session_id = SessionId::new_v4();
        // Create through the supported test storage factory, not handcrafted JSON.
        // 经受支持的测试存储工厂创建，而非手工构造 JSON。
        let locator = vulcan_workspace_store::workspace_store::create_session_v1_storage_fixture(
            directory.path(),
            &session_id.to_string(),
            1,
        )
        .unwrap();
        // Load the authoritative initialized document.
        // 加载权威初始化文档。
        let loaded =
            vulcan_workspace_store::workspace_store::load_session_v1_full(&locator).unwrap();
        // Every fixture owns a separate registry and writer.
        // 每个夹具拥有独立注册表及写入器。
        let handle = SessionHandle::new(session_id, locator, loaded.document);
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("preflight-root").unwrap(),
                    "retained task",
                    Vec::new(),
                ),
                2,
            )
            .unwrap();
        // Use the ordinary service registry path without attaching a network runtime.
        // 使用普通服务注册表路径，不附加网络运行时。
        let service =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        service
            .session_runtime_registry
            .register(handle.clone())
            .unwrap();
        Self {
            _directory: directory,
            state: Arc::new(std::sync::Mutex::new(service)),
            handle,
            route: ProviderRuntimeBlueprint {
                provider_id: "preflight-provider".into(),
                service_entry_id: "api".into(),
                connection_id: "preflight-account".into(),
                provider_name: "Fixture".into(),
                runtime_label: "fixture".into(),
                model_id: "preflight-model".into(),
                model_name: "Fixture".into(),
                protocol_profile_id: "anthropic-messages.v1".into(),
                manifest_revision: 1,
                frozen_selection: None,
                route_revision: "fixture-route".into(),
                secondary_route: vulcan_provider_adapters::SecondaryRouteSelection::Automatic,
                public_capabilities: Default::default(),
                base_url: None,
                supports_remote_model_refresh: false,
                credential_source: "unavailable".into(),
                reasoning: Default::default(),
            },
            budget: ContextBudgetSnapshot {
                active_context_tokens: 1000,
                estimated_input_tokens: 1000,
                context_limit: Some(1_000_000),
                provider_request_hard_limit: Some(1_000_000),
                effective_context_window: Some(354_350),
                auto_compact_limit: Some(354_350),
                compaction_auto_enabled: true,
                baseline_matches_active_runtime: true,
                ..Default::default()
            },
        }
    }

    /// Starts a local API fact for `purpose` without dispatch; returns its exact identity.
    /// 为 `purpose` 创建本地 API 事实但不分派；返回其精确身份。
    fn start(&self, purpose: ApiPurposeKind) -> ApiCallId {
        // Capture the real current compaction frontier and complete durable history.
        // 捕获真实当前压缩前沿及完整可靠历史。
        let snapshot = self
            .handle
            .capture_controller_projection_snapshot()
            .unwrap();
        self.handle
            .prepare_and_start_api_durable(PrepareAndStartApiInput {
                purpose,
                request: self.request(),
                projection: vulcan_session_core::v1::event::ProviderProjectionProof {
                    segment_generation: snapshot.segment_generation,
                    document_revision: snapshot.document_revision,
                    retained_from_seq: snapshot.retained_from_seq,
                    through_seq: snapshot.through_seq,
                    fingerprint_sha256: "f".repeat(64),
                },
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: crate::current_v1_unix_millis().unwrap(),
            })
            .unwrap()
            .value
            .api_call_id
    }

    /// Returns immutable API route facts matching the frozen fixture route; takes no arguments.
    /// 返回与冻结夹具路由匹配的不可变 API 路由事实；无参数。
    fn request(&self) -> ApiRequestSnapshot {
        ApiRequestSnapshot {
            provider: self.route.provider_id.clone(),
            model: self.route.model_id.clone(),
            profile: Some(self.route.protocol_profile_id.clone()),
            connection_id: Some(self.route.connection_id.clone()),
            thinking_level: None,
            temperature: None,
        }
    }

    /// Completes `api` with explicit `status` and `usage`; returns no value and contacts no provider.
    /// 使用显式 `status` 及 `usage` 完成 `api`；无返回值，不联系供应商。
    fn complete(&self, api: ApiCallId, status: ApiCallStatus, usage: UsageSnapshot) {
        // Terminal fixtures obey the same status, reason, error and Continue contract as real APIs.
        // 终态夹具遵守与真实 API 相同的状态、原因、错误及 Continue 契约。
        let (reason, error, eligibility) = match status {
            ApiCallStatus::Completed => (
                ApiEndReason::CompletedSuccessfully,
                None,
                ContinuationEligibility::Forbidden,
            ),
            ApiCallStatus::Failed => (
                ApiEndReason::NetworkError,
                Some(vulcan_session_core::v1::event::RedactedApiError {
                    category: vulcan_session_core::v1::event::ApiErrorCategory::Network,
                    error_code: None,
                    message: "Fixture network failure".into(),
                }),
                ContinuationEligibility::Allowed,
            ),
            ApiCallStatus::Interrupted => (
                ApiEndReason::ProcessInterrupted,
                None,
                ContinuationEligibility::Allowed,
            ),
            ApiCallStatus::Cancelled => (
                ApiEndReason::Cancelled,
                None,
                ContinuationEligibility::Forbidden,
            ),
            _ => panic!("fixture requires a terminal API status"),
        };
        self.handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| {
                    document.complete_api_group(
                        api,
                        ApiCompletion {
                            request: self.request(),
                            usage,
                            status,
                            end_reason: Some(reason),
                            error,
                            continuation_eligibility: eligibility,
                            timing: Default::default(),
                            provider_source: None,
                            completed_at_ms: crate::current_v1_unix_millis().unwrap(),
                        },
                    )
                },
            )
            .unwrap();
    }

    /// Evaluates the exact production preflight reader; returns historical pressure without a new API start.
    /// 执行精确生产预检读取器；返回历史压力，不新增 API 起始。
    fn observed(&self) -> Option<ContextBudgetSnapshot> {
        observed_compaction_budget_before_provider_request(
            &self.state,
            self.handle.session_id(),
            &self.route,
            &self.budget,
        )
        .unwrap()
    }

    /// Replaces the process-local owner with a cold tail load; returns no value and preserves all bytes.
    /// 使用冷尾部加载替换进程内所有者；无返回值，保留全部字节。
    fn reload(&mut self) {
        // Tail loading cannot rely on a process-local previous-response cache.
        // 尾部加载不能依赖进程内上一响应缓存。
        let locator = self.handle.storage_locator().unwrap();
        // Reload through the same storage entry point as ordinary session activation.
        // 经普通会话激活使用的同一存储入口重载。
        let loaded =
            vulcan_workspace_store::workspace_store::load_session_v1_for_runtime(&locator).unwrap();
        self.handle = SessionHandle::new(self.handle.session_id(), locator, loaded.document);
        // A fresh registry models a new service process.
        // 全新注册表模拟新的服务进程。
        let service = crate::tests::test_support::test_service_state_with_parent_session(
            self.handle.session_id(),
        );
        service
            .session_runtime_registry
            .register(self.handle.clone())
            .unwrap();
        self.state = Arc::new(std::sync::Mutex::new(service));
    }
}

/// Returns measured over-line context including cache creation and output; takes no arguments.
/// 返回包含缓存创建及输出的超线实测上下文；无参数。
fn over_line_usage() -> UsageSnapshot {
    UsageSnapshot {
        input_tokens: Some(358_817),
        output_tokens: Some(868),
        uncached_input_tokens: Some(320_458),
        cache_read_tokens: Some(38_359),
        cache_write_tokens: Some(306_389),
        ..Default::default()
    }
}

/// Success and failure both compact before a new prompt's first API, including after a cold reopen.
/// 成功及失败均在新提示词首个 API 前触发压缩，包含冷重载后。
/// Takes no arguments; asserts no request is started by the check and no history is changed.
/// 无参数；断言检查不启动请求，也不改变历史。
#[test]
fn preflight_observed_usage_survives_failure_and_cold_new_turn() {
    for status in [
        ApiCallStatus::Completed,
        ApiCallStatus::Failed,
        ApiCallStatus::Interrupted,
        ApiCallStatus::Cancelled,
    ] {
        // Each status uses an independent real session store.
        // 每种状态使用独立真实会话存储。
        let mut fixture = Fixture::new();
        // This completed API predates the next user prompt.
        // 此终态 API 早于下一条用户提示词。
        let api = fixture.start(ApiPurposeKind::MainGeneration);
        fixture.complete(api, status, over_line_usage());
        fixture
            .handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.close_turn(
                        TurnOutcome::Completed,
                        0,
                        crate::current_v1_unix_millis().unwrap(),
                    )
                },
            )
            .unwrap();
        fixture
            .handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("preflight-new-input").unwrap(),
                    "continue task",
                    Vec::new(),
                ),
                crate::current_v1_unix_millis().unwrap(),
            )
            .unwrap();
        fixture.reload();
        // Snapshot the durable frontier before preflight to prove no probe request is required.
        // 预检前记录可靠前沿，证明不需要探测请求。
        let before = fixture
            .handle
            .read(|document| document.memory_revision())
            .unwrap();
        assert!(!fixture.budget.should_compact());
        // The observed total uses each cache/read/write dimension only once.
        // 实测总量只计算一次各缓存读取及创建维度。
        let observed = fixture
            .observed()
            .expect("same-context usage must survive restart");
        assert_eq!(observed.active_context_tokens, 359_685);
        assert!(
            observed.should_compact(),
            "terminal status must not disable preflight: {status:?}"
        );
        assert_eq!(
            fixture
                .handle
                .read(|document| document.memory_revision())
                .unwrap(),
            before
        );
        assert!(
            fixture
                .handle
                .read(|document| document.active_api().is_none())
                .unwrap()
        );
    }
}

/// Missing usage and auxiliary failures retain prior business evidence; known new usage replaces it.
/// 缺失用量及辅助失败保留此前业务证据；新的已知用量替换旧证据。
/// Takes no arguments; returns no value and tests both an empty and a populated usage history.
/// 无参数、无返回值，覆盖无用量历史及已有用量历史。
#[test]
fn preflight_missing_usage_does_not_become_zero_or_use_summary_usage() {
    // Start without any measured token dimensions.
    // 从尚无任何实测 Token 维度开始。
    let fixture = Fixture::new();
    fixture.complete(
        fixture.start(ApiPurposeKind::MainGeneration),
        ApiCallStatus::Failed,
        UsageSnapshot::default(),
    );
    assert!(fixture.observed().is_none());
    fixture.complete(
        fixture.start(ApiPurposeKind::RetryGeneration),
        ApiCallStatus::Failed,
        over_line_usage(),
    );
    fixture.complete(
        fixture.start(ApiPurposeKind::RetryGeneration),
        ApiCallStatus::Failed,
        UsageSnapshot::default(),
    );
    fixture.complete(
        fixture.start(ApiPurposeKind::Compaction),
        ApiCallStatus::Failed,
        UsageSnapshot {
            input_tokens: Some(7),
            ..Default::default()
        },
    );
    assert!(fixture.observed().unwrap().should_compact());
    fixture.complete(
        fixture.start(ApiPurposeKind::RetryGeneration),
        ApiCallStatus::Completed,
        UsageSnapshot {
            input_tokens: Some(100),
            output_tokens: Some(20),
            ..Default::default()
        },
    );
    assert!(!fixture.observed().unwrap().should_compact());
}

/// Committed compaction retires prior pressure and its own usage; the new context starts a fresh baseline.
/// 已提交压缩使旧压力及摘要自身用量失效；新上下文建立新基线。
/// Takes no arguments; returns no value and exercises the production compaction commit.
/// 无参数、无返回值，执行生产压缩提交。
#[test]
fn preflight_successful_compaction_does_not_compact_the_summary_again() {
    // The original business context has already crossed the soft threshold.
    // 原业务上下文已经超过软阈值。
    let mut fixture = Fixture::new();
    fixture.complete(
        fixture.start(ApiPurposeKind::MainGeneration),
        ApiCallStatus::Completed,
        over_line_usage(),
    );
    // The summary API can itself have a large input without describing the next business context.
    // 摘要 API 自身可有大量输入，但不代表下一次业务上下文。
    let summary_api = fixture.start(ApiPurposeKind::Compaction);
    // Produce a real summary event before committing the compaction node.
    // 提交压缩节点前产生真实摘要事件。
    let summary_seq = fixture
        .handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                document.append_assistant_text(
                    summary_api,
                    "summary",
                    AssistantSemanticStatus::Completed,
                    crate::current_v1_unix_millis().unwrap(),
                )
            },
        )
        .unwrap()
        .value
        .into_value();
    fixture.complete(summary_api, ApiCallStatus::Completed, over_line_usage());
    fixture
        .handle
        .record_compaction(
            CompactionCommitInput::new(
                summary_api,
                summary_seq,
                summary_seq,
                "summary",
                LockedBaselineProjection::empty(),
            ),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    fixture.reload();
    assert!(fixture.observed().is_none());
    fixture.complete(
        fixture.start(ApiPurposeKind::MainGeneration),
        ApiCallStatus::Failed,
        UsageSnapshot::default(),
    );
    assert!(fixture.observed().is_none());
    fixture.complete(
        fixture.start(ApiPurposeKind::MainGeneration),
        ApiCallStatus::Completed,
        UsageSnapshot {
            input_tokens: Some(200),
            ..Default::default()
        },
    );
    assert!(!fixture.observed().unwrap().should_compact());
}

/// Route transitions, disabled auto-compaction and prefix-only budgets do not inherit stale pressure.
/// 路由切换、关闭自动压缩及仅前缀预算不会继承陈旧压力。
/// Takes no arguments; returns no value and checks the actual frozen-policy guards.
/// 无参数、无返回值，检查实际冻结策略守卫。
#[test]
fn preflight_observed_pressure_respects_route_and_current_window_policy() {
    // Establish one usable original-route sample.
    // 建立一条可用的原路由样本。
    let mut fixture = Fixture::new();
    fixture.complete(
        fixture.start(ApiPurposeKind::MainGeneration),
        ApiCallStatus::Completed,
        over_line_usage(),
    );
    fixture.budget.compaction_auto_enabled = false;
    assert!(fixture.observed().is_none());
    fixture.budget.compaction_auto_enabled = true;
    fixture.budget.auto_compact_scope = vulcan_context::ContextWindowUsageScope::BodyAfterPrefix;
    fixture.budget.auto_compact_prefill_input_tokens = Some(358_817);
    fixture.budget.effective_context_window = Some(1_000_000);
    assert!(!fixture.observed().unwrap().should_compact());
    fixture.budget.auto_compact_scope = vulcan_context::ContextWindowUsageScope::Total;
    fixture.budget.auto_compact_limit = Some(400_000);
    assert!(!fixture.observed().unwrap().should_compact());
    fixture.budget.auto_compact_limit = Some(354_350);
    // A changed frozen route cannot inherit the old route's measured pressure.
    // 改变后的冻结路由不能继承旧路由的实测压力。
    let original = fixture.route.clone();
    for field in ["provider", "model", "profile", "connection"] {
        fixture.route = original.clone();
        match field {
            "provider" => fixture.route.provider_id = "other".into(),
            "model" => fixture.route.model_id = "other".into(),
            "profile" => fixture.route.protocol_profile_id = "other".into(),
            "connection" => fixture.route.connection_id = "other".into(),
            _ => unreachable!(),
        }
        assert!(
            fixture.observed().is_none(),
            "changed route dimension: {field}"
        );
    }
}

/// Editing a later prompt invalidates copied old-generation usage even when the old API stays in history.
/// 编辑后续提示词后，即使旧 API 仍在历史中，复制的旧代次用量也会失效。
/// Takes no arguments; returns no value and publishes a real edited storage generation.
/// 无参数、无返回值，发布真实编辑存储代次。
#[test]
fn preflight_edited_history_does_not_reuse_copied_usage() {
    use vulcan_session_core::v1::document::{EditGenerationInput, RootInputReplacement};
    // Keep a measured earlier turn before the editable prompt.
    // 在可编辑提示词之前保留一个已有实测用量的回合。
    let mut fixture = Fixture::new();
    fixture.complete(
        fixture.start(ApiPurposeKind::MainGeneration),
        ApiCallStatus::Completed,
        over_line_usage(),
    );
    fixture
        .handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.close_turn(
                    TurnOutcome::Completed,
                    0,
                    crate::current_v1_unix_millis().unwrap(),
                )
            },
        )
        .unwrap();
    // Locate the edit by its stable action identity rather than a hardcoded event position.
    // 按稳定动作身份定位编辑，不使用硬编码事件位置。
    let action = ClientActionId::try_new("preflight-editable").unwrap();
    fixture
        .handle
        .submit_root_input(
            ClientInputSubmission::new(action.clone(), "editable task", Vec::new()),
            crate::current_v1_unix_millis().unwrap(),
        )
        .unwrap();
    assert!(fixture.observed().unwrap().should_compact());
    // The real generation publisher validates and preserves the preceding API pair.
    // 真实代次发布器校验并保留之前的 API 配对。
    let (generation, target_seq) = fixture
        .handle
        .read(|document| {
            (
                document.segment_generation(),
                document
                    .events()
                    .iter()
                    .find_map(|event| match &event.event {
                        EventBody::UserInput(input) if input.client_action_id == action => {
                            Some(event.seq)
                        }
                        _ => None,
                    })
                    .expect("editable input must exist"),
            )
        })
        .unwrap();
    fixture
        .handle
        .publish_edit_generation(
            generation,
            EditGenerationInput {
                target_seq,
                replacement: RootInputReplacement::new("replacement task", Vec::new()),
                now_ms: crate::current_v1_unix_millis().unwrap(),
            },
        )
        .unwrap();
    fixture.reload();
    assert!(
        fixture
            .handle
            .read_full_durable(|document| document
                .events()
                .iter()
                .any(|event| matches!(event.event, EventBody::ApiStats(_))))
            .unwrap()
    );
    assert!(fixture.observed().is_none());
}
