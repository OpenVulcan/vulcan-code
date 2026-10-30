//! Continue admission, controller recovery, and cross-protocol host-handoff regressions.
//! Continue 准入、控制器恢复以及跨协议宿主交接回归测试。

use super::*;
use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
use crate::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryTracker;
use crate::agent_runtime::provider_v1_bridge::{ProviderV1LifecycleFacts, ProviderV1RequestFacts};
use async_trait::async_trait;
use std::sync::Mutex;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::tool::{AllowAllToolPolicy, VulcanToolExecutor};
use vulcan_core::types::{
    ControllerRunResult, CoreError, ProviderRequestSnapshot, ProviderTurnResponse,
    ToolInvocationRequest, ToolInvocationResult,
};
use vulcan_session_core::v1::event::ApiPurposeKind;

/// Offline adapter that binds real telemetry permits and records the controller's exact history.
/// 绑定真实遥测许可并记录控制器精确历史的离线适配器。
struct ContinuedAdapter {
    /// Telemetry survives the simulated transport failure until the host captures its handoff.
    /// 遥测在模拟传输失败后持续保留，直到宿主捕获交接状态。
    tracker: Mutex<ProviderRequestTelemetryTracker>,
    /// One permitted outcome; a second dispatch is a test failure.
    /// 唯一获准的结果；再次分派即测试失败。
    response: Mutex<Option<Result<ProviderTurnResponse, CoreError>>>,
    /// Exact physical requests observed at dispatch.
    /// 分派时观测到的精确物理请求。
    requests: Mutex<Vec<ProviderRequestSnapshot>>,
    /// Typed action bindings prepared before each durable permit.
    /// 每次可靠许可前准备的强类型动作绑定。
    facts: Mutex<Vec<ProviderV1RequestFacts>>,
}

impl ContinuedAdapter {
    /// Builds an offline adapter from the host request and the sole planned provider outcome.
    /// 根据宿主请求和唯一预设 Provider 结果构建离线适配器。
    /// Returns an adapter with a fresh telemetry context, without shared services or credentials.
    /// 返回具有全新遥测上下文的适配器，不访问共享服务或凭据。
    fn new(
        request: &ExecutionPreparationRequest,
        response: Result<ProviderTurnResponse, CoreError>,
    ) -> Self {
        Self {
            tracker: Mutex::new(continuation_test_tracker(request)),
            response: Mutex::new(Some(response)),
            requests: Mutex::new(Vec::new()),
            facts: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl VulcanProviderAdapter for ContinuedAdapter {
    /// Canonical request captured from the real controller.
    /// 从真实控制器捕获的规范请求。
    type Request = ProviderRequestSnapshot;
    /// Normalized provider outcome consumed by the real controller.
    /// 由真实控制器消费的规范 Provider 结果。
    type Response = ProviderTurnResponse;

    /// Returns the fixture's provider identity; takes no arguments.
    /// 返回夹具的 Provider 身份；无参数。
    fn provider_id(&self) -> &str {
        "openai"
    }

    /// Returns the fixture's model identity; takes no arguments.
    /// 返回夹具的模型身份；无参数。
    fn model_id(&self) -> &str {
        "gpt-test"
    }

    /// Copies `snapshot` and `config` into a request; returns its unchanged canonical history.
    /// 将 `snapshot` 与 `config` 复制为请求；返回保留原始规范历史的请求。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &ControllerConfig,
    ) -> Result<Self::Request, CoreError> {
        Ok(ProviderRequestSnapshot {
            provider_id: config.provider_id.clone(),
            model_id: config.model_id.clone(),
            system_prompt: config.system_prompt.clone(),
            prompt_layers: config.prompt_layers.clone(),
            prompt: String::new(),
            prompt_in_history: true,
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: Vec::new(),
            additional_params: config.additional_params.clone(),
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        })
    }

    /// Confirms `request` through real telemetry, then returns the sole planned success or failure.
    /// 通过真实遥测确认 `request`，随后返回唯一预设的成功或失败结果。
    async fn execute_request(&self, request: Self::Request) -> Result<Self::Response, CoreError> {
        // tracker owns the same prepare/confirm/terminal boundary as the production stream.
        // tracker 持有与生产流相同的准备、确认和终态边界。
        let mut tracker = self.tracker.lock().unwrap();
        // prepared retains typed purpose and action evidence before consumption.
        // prepared 在动作消费前保留强类型用途与动作证据。
        let prepared = tracker.prepare_request(&request, None).unwrap();
        let Some(ProviderV1LifecycleFacts::RequestPrepared(facts)) = prepared.v1_lifecycle else {
            panic!("expected typed request preparation");
        };
        self.facts.lock().unwrap().push(facts);
        tracker.confirm_prepared_request(&request, None).unwrap();
        self.requests.lock().unwrap().push(request);
        // response permits exactly one dispatch, including the end_turn:false scenario.
        // response 仅允许一次分派，包括 end_turn:false 场景。
        let response = self.response.lock().unwrap().take().expect("one dispatch");
        if response.is_ok() {
            tracker
                .complete_success(
                    None,
                    crate::agent_runtime::provider_v1_bridge::test_provider_v1_source_input(),
                    0,
                )
                .unwrap();
        } else {
            tracker.complete_failure(
                true,
                1,
                3,
                0,
                "retryable_provider_transport_error",
                None,
                None,
            );
        }
        response
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for ContinuedAdapter {
    /// Executes `request` without deltas; `sink` stays empty so failures preserve the original tail.
    /// 执行 `request` 而不产生增量；`sink` 保持为空，使失败保留原有尾部。
    /// Returns the planned normalized response or provider failure.
    /// 返回预设规范响应或 Provider 失败。
    async fn execute_request_streaming<S>(
        &self,
        request: Self::Request,
        _sink: &mut S,
    ) -> Result<Self::Response, CoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        self.execute_request(request).await
    }
}

/// Rejects every tool invocation because assistant-tail recovery must not execute unresolved tools.
/// 拒绝全部工具调用，因为助手尾部恢复不得执行未解决的工具。
struct NoTools;

#[async_trait]
impl VulcanToolExecutor for NoTools {
    /// Rejects `request`; no tool result can be returned in these tool-free controller scenarios.
    /// 拒绝 `request`；这些无工具控制器场景不允许返回工具结果。
    async fn execute_tool(
        &self,
        _request: &ToolInvocationRequest,
    ) -> Result<ToolInvocationResult, CoreError> {
        panic!("assistant-tail recovery must not execute tools");
    }
}

/// Creates a normalized response with `text` and the explicit `end_turn` boundary; returns no tools.
/// 根据 `text` 和显式 `end_turn` 边界创建规范响应；不返回工具调用。
fn response(text: &str, end_turn: bool) -> ProviderTurnResponse {
    ProviderTurnResponse {
        assistant_text: text.into(),
        assistant_reasoning: String::new(),
        provider_message_id: None,
        provider_message_state: Some(serde_json::json!({
            "vulcan_ai": { "turn_boundary": { "end_turn": end_turn } }
        })),
        tool_calls: Vec::new(),
        continuation: None,
        usage: None,
    }
}

/// Seeds real host context before appending the retained `text`/`reasoning` assistant tail.
/// 在追加保留的 `text`／`reasoning` 助手尾部前播种真实宿主上下文。
/// Returns a matched request/preparation pair whose next bootstrap must not manufacture new input.
/// 返回相互匹配的请求与准备对象，下一次启动不得制造新的输入。
fn retained_request(
    text: &str,
    reasoning: &str,
) -> (ExecutionPreparationRequest, ExecutionPreparation) {
    // request owns an explicit manual Continue admission with no project filesystem access.
    // request 持有明确的手动 Continue 准入，不访问项目文件系统。
    let mut request = build_test_request();
    request.workspace_path.clear();
    request.turn_start_mode = Some(ExecutionTurnStartMode::RestoredCheckpoint);
    request.continuation_action_id = Some(
        vulcan_session_core::v1::ClientActionId::try_new("retained-assistant-continue").unwrap(),
    );
    // preparation mirrors the admission fields copied by execution preparation.
    // preparation 镜像执行准备复制的准入字段。
    let mut preparation = build_test_preparation();
    preparation.workspace_path.clear();
    preparation.continuation_action_id = request.continuation_action_id.clone();
    preparation.conversation_hook_turn_id = request.conversation_hook_turn_id.clone();
    // snapshot starts with real provider-visible input, before an interrupted assistant response.
    // snapshot 在助手响应中断前以真实 Provider 可见输入开始。
    let snapshot = CoreConversationSnapshot {
        messages: vec![CoreConversationMessage {
            id: "retained-user".into(),
            payload: CoreConversationPayload::UserText {
                text: "Continue the analysis".into(),
                attachments: Vec::new(),
            },
        }],
        ..Default::default()
    };
    request.conversation_state = core_snapshot_to_session_conversation_state(&snapshot).unwrap();
    // seeded materializes stable host context before the tail, avoiding a false pass from added input.
    // seeded 在尾部之前物化稳定宿主上下文，避免新增输入掩盖回归并使测试误通过。
    let mut seeded = build_runtime_bootstrap_core(&preparation, &request, None)
        .unwrap()
        .core_snapshot;
    seeded.messages.push(CoreConversationMessage {
        id: "retained-assistant".into(),
        payload: CoreConversationPayload::Assistant {
            text: text.into(),
            reasoning: reasoning.into(),
            provider_message_id: None,
            provider_message_state: None,
            usage: None,
            tool_calls: Vec::new(),
        },
    });
    request.conversation_state = core_snapshot_to_session_conversation_state(&seeded).unwrap();
    (request, preparation)
}

/// Runs the resolved launch mode through real bootstrap and controller APIs using offline `adapter`.
/// 使用离线 `adapter`，经真实启动和控制器 API 执行已解析的启动模式。
/// `request`/`preparation` supply host state; `intervention` models the next safe host boundary.
/// `request`／`preparation` 提供宿主状态；`intervention` 模拟下一安全宿主边界。
/// Returns the exact controller checkpoint and its success or typed failure.
/// 返回精确控制器检查点及其成功结果或强类型失败。
async fn run_retained<I>(
    request: &ExecutionPreparationRequest,
    preparation: &ExecutionPreparation,
    adapter: &ContinuedAdapter,
    intervention: I,
) -> (VulcanCoreController, Result<ControllerRunResult, CoreError>)
where
    I: FnMut(&mut VulcanCoreController) -> Result<bool, CoreError> + Send,
{
    // controller comes from the production bootstrap, including host-context synchronization.
    // controller 来自生产启动链路，其中包含宿主上下文同步。
    let mut controller = build_runtime_bootstrap_core(preparation, request, None)
        .unwrap()
        .core_controller;
    // result exercises the same public entrypoints selected by the production transport.
    // result 执行生产传输所选择的相同公开入口。
    let result = match runtime_stream_turn_start_mode_for_request(request) {
        RuntimeStreamTurnStartMode::ContinuedProviderTurn => {
            controller
                .run_restored_provider_loop_streaming_with_checkpoints_and_interventions(
                    adapter,
                    &NoTools,
                    &AllowAllToolPolicy,
                    None,
                    |_, _| {},
                    intervention,
                )
                .await
        }
        RuntimeStreamTurnStartMode::RestoredCheckpoint => {
            controller
                .run_seeded_turn_streaming_with_checkpoints_and_interventions(
                    adapter,
                    &NoTools,
                    &AllowAllToolPolicy,
                    |_, _| {},
                    intervention,
                )
                .await
        }
        RuntimeStreamTurnStartMode::UserPrompt => {
            panic!("retained input must not append a user prompt")
        }
    };
    (controller, result)
}

/// Captures a completed `adapter` handoff and updates both host request and preparation.
/// 捕获已结束 `adapter` 的交接状态，同时更新宿主请求与准备对象。
/// Returns no value; proves the consumed action stays absent while recovery remains admitted.
/// 无返回值；证明已消费动作保持为空，而恢复能力仍已准入。
fn adopt_handoff(
    request: &mut ExecutionPreparationRequest,
    preparation: &mut ExecutionPreparation,
    adapter: &ContinuedAdapter,
) {
    // continuation is captured only after the physical request reached its terminal state.
    // continuation 只在物理请求到达终态后捕获。
    let continuation = adapter
        .tracker
        .lock()
        .unwrap()
        .next_request_continuation_identity()
        .unwrap();
    assert!(continuation.allows_assistant_tail_restore);
    assert!(continuation.pending_continuation_action_id.is_none());
    assert_eq!(continuation.next_provider_attempt_index, 1);
    request.replace_retry_replay_with_provider_request_continuation(continuation);
    preparation.continuation_action_id = request.continuation_action_id.clone();
}

/// Verifies a replacement stream dispatches exactly `history` as ordinary generation.
/// 验证替代流以普通生成方式精确分派 `history`。
/// `adapter` contains completed offline dispatch facts; returns no value and never compares wording.
/// `adapter` 包含已完成离线分派事实；无返回值，不比较文案措辞。
fn assert_followup(adapter: &ContinuedAdapter, history: &[CoreConversationMessage]) {
    // requests contains the sole dispatch; matching the slice avoids structural position assumptions.
    // requests 包含唯一分派；通过切片匹配避免假定结构位置。
    let requests = adapter.requests.lock().unwrap();
    let [request] = requests.as_slice() else {
        panic!("expected exactly one followup dispatch");
    };
    assert_eq!(request.history, history);
    // facts must retain a fresh API identity without rebinding the consumed Continue action.
    // facts 必须保留新的 API 身份，不能重新绑定已消费的 Continue 动作。
    let facts = adapter.facts.lock().unwrap();
    let [facts] = facts.as_slice() else {
        panic!("expected exactly one prepared followup");
    };
    assert_eq!(facts.purpose, ApiPurposeKind::MainGeneration);
    assert!(facts.continuation_action_id.is_none());
}

/// Retry failures without new output preserve text-only and reasoning-only tails across host restarts.
/// 没有新增输出的重试失败在宿主重启后保留纯文本与纯推理尾部。
/// No inputs or return value; verifies route and policy interventions reach a new real controller request.
/// 无参数、无返回值；验证路由与策略介入后能够发出新的真实控制器请求。
#[tokio::test]
async fn continued_failure_host_restart_restores_assistant_tail() {
    // text/reasoning cover both permitted forms of interrupted semantic output.
    // text／reasoning 覆盖两种允许的中断语义输出形式。
    for (text, reasoning) in [("Partial answer", ""), ("", "Partial reasoning")] {
        // policy_refresh chooses the two intervention owners available before a retry dispatch.
        // policy_refresh 选择重试分派前可介入的两个状态归属方。
        for policy_refresh in [false, true] {
            // request/preparation begin with the same admitted assistant tail.
            // request／preparation 以同一已准入助手尾部开始。
            let (mut request, mut preparation) = retained_request(text, reasoning);
            // failed emits no semantic deltas after its durable Continue permit.
            // failed 在可靠 Continue 许可后不产生语义增量。
            let failed = ContinuedAdapter::new(
                &request,
                Err(CoreError::ProviderFailure {
                    provider_id: "openai".into(),
                    message: "offline retryable failure".into(),
                    details: None,
                }),
            );
            // controller/result retain the exact failure checkpoint for the automatic retry.
            // controller／result 为自动重试保留精确失败检查点。
            let (controller, result) =
                run_retained(&request, &preparation, &failed, |_| Ok(false)).await;
            assert!(matches!(result, Err(CoreError::ProviderFailure { .. })));
            request.conversation_state =
                core_snapshot_to_session_conversation_state(controller.snapshot()).unwrap();
            // retried/interrupted exercise a real retry-entry intervention before another dispatch.
            // retried／interrupted 在再次分派前执行真实重试入口介入。
            let (retried, interrupted) = run_retained(&request, &preparation, &failed, |_| {
                Err(if policy_refresh {
                    CoreError::ContextCompactionPolicyRefreshRequired {
                        message: "policy changed".into(),
                    }
                } else {
                    CoreError::ProviderRouteSwitchRequired {
                        message: "route changed".into(),
                    }
                })
            })
            .await;
            assert!(matches!(
                interrupted,
                Err(CoreError::ProviderRouteSwitchRequired { .. }
                    | CoreError::ContextCompactionPolicyRefreshRequired { .. })
            ));
            assert_eq!(retried.snapshot().messages, controller.snapshot().messages);
            assert_eq!(failed.requests.lock().unwrap().len(), 1);
            adopt_handoff(&mut request, &mut preparation, &failed);
            // resumed has a fresh tracker; old in-memory admission cannot mask lost recovery state.
            // resumed 使用全新跟踪器，旧内存中的准入不能掩盖恢复状态丢失。
            let resumed = ContinuedAdapter::new(&request, Ok(response("Finished", true)));
            // result must succeed with the consumed action absent and no inserted user message.
            // result 必须在已消费动作为空且没有插入用户消息的条件下成功。
            let (_, result) = run_retained(&request, &preparation, &resumed, |_| Ok(false)).await;
            assert_eq!(result.unwrap().provider_turns_executed, 1);
            assert_followup(&resumed, &controller.snapshot().messages);
        }
    }
}

/// An explicit nonterminal assistant response remains resumable after its Continue API succeeds.
/// 显式非终态助手响应在对应 Continue API 成功后仍可恢复。
/// No inputs or return value; verifies the controller's end_turn:false checkpoint and host handoff.
/// 无参数、无返回值；验证控制器的 end_turn:false 检查点与宿主交接。
#[tokio::test]
async fn continued_nonterminal_success_host_restart_restores_assistant_tail() {
    // request/preparation carry an already retained partial assistant response.
    // request／preparation 携带已经保留的助手部分响应。
    let (mut request, mut preparation) = retained_request("Partial answer", "");
    // progress explicitly requests another Provider turn without a tool-result input.
    // progress 明确要求再次执行 Provider 回合，而不提供工具结果输入。
    let progress = ContinuedAdapter::new(&request, Ok(response("Nonterminal progress", false)));
    // controller/result stop at the actual post-response intervention boundary.
    // controller／result 停在真实响应后介入边界。
    let (controller, result) = run_retained(&request, &preparation, &progress, |controller| {
        if controller.snapshot().messages.iter().any(|message| matches!(
            &message.payload, CoreConversationPayload::Assistant { text, .. } if text == "Nonterminal progress"
        )) {
            Err(CoreError::ProviderRouteSwitchRequired { message: "route changed after response".into() })
        } else {
            Ok(false)
        }
    }).await;
    assert!(matches!(
        result,
        Err(CoreError::ProviderRouteSwitchRequired { .. })
    ));
    request.conversation_state =
        core_snapshot_to_session_conversation_state(controller.snapshot()).unwrap();
    adopt_handoff(&mut request, &mut preparation, &progress);
    // resumed closes the admitted loop using a new ordinary API request.
    // resumed 使用新的普通 API 请求结束已准入循环。
    let resumed = ContinuedAdapter::new(&request, Ok(response("Finished", true)));
    // result confirms real controller acceptance after fresh bootstrap and telemetry construction.
    // result 确认全新启动和遥测构建后真实控制器仍然接受请求。
    let (_, result) = run_retained(&request, &preparation, &resumed, |_| Ok(false)).await;
    assert_eq!(result.unwrap().provider_turns_executed, 1);
    assert_followup(&resumed, &controller.snapshot().messages);
}

/// An ordinary host restart cannot acquire assistant-tail recovery merely by carrying a handoff.
/// 普通宿主重启不能仅因携带交接状态就获得助手尾部恢复能力。
/// No inputs or return value; verifies rejection before any provider dispatch.
/// 无参数、无返回值；验证在任何 Provider 分派之前拒绝。
#[tokio::test]
async fn ordinary_host_restart_rejects_unadmitted_assistant_tail() {
    // request/preparation remove the initial admission while retaining the same assistant history.
    // request／preparation 移除首次准入，同时保留同一助手历史。
    let (mut request, mut preparation) = retained_request("Partial answer", "");
    request.continuation_action_id = None;
    preparation.continuation_action_id = None;
    // continuation comes from a real ordinary stream and must retain its strict recovery boundary.
    // continuation 来自真实普通流，必须保留其严格恢复边界。
    let continuation = continuation_test_tracker(&request)
        .next_request_continuation_identity()
        .unwrap();
    assert!(!continuation.allows_assistant_tail_restore);
    request.replace_retry_replay_with_provider_request_continuation(continuation);
    // adapter must never receive this invalid seeded input.
    // adapter 绝不能接收到此无效播种输入。
    let adapter = ContinuedAdapter::new(&request, Ok(response("Unreachable", true)));
    // result is the real controller validation failure, rather than an enum-only assertion.
    // result 是真实控制器校验失败，而非只断言枚举取值。
    let (_, result) = run_retained(&request, &preparation, &adapter, |_| Ok(false)).await;
    assert!(matches!(result, Err(CoreError::InvalidState { .. })));
    assert!(adapter.requests.lock().unwrap().is_empty());
}

/// Builds fresh telemetry from exact `preparation` route facts and `request` handoff state.
/// 从精确的 `preparation` 路由事实与 `request` 交接状态构建全新遥测。
/// Returns an idle tracker through the production context factory, without reusing old state.
/// 经生产上下文工厂返回空闲跟踪器，不复用旧状态。
fn protocol_tracker(
    preparation: &ExecutionPreparation,
    request: &ExecutionPreparationRequest,
) -> ProviderRequestTelemetryTracker {
    // tracker receives only the immutable context built for this physical stream.
    // tracker 仅接收为此物理流构建的不可变上下文。
    let mut tracker = ProviderRequestTelemetryTracker::default();
    tracker.install_context(
        ProviderRequestTelemetryContext::from_preparation(
            preparation,
            request.provider_request_continuation.as_ref(),
        )
        .unwrap(),
    );
    tracker
}

/// Protocol switches preserve logical recovery and action consumption while replacing exact route facts.
/// 协议切换替换精确路由事实，同时保留逻辑恢复能力与动作消费状态。
/// No inputs or return value; covers every ordered profile pair before/after permit, with/without Continue.
/// 无参数、无返回值；覆盖各协议的有序组合、许可前后以及有无 Continue 的情况。
#[test]
fn protocol_switch_preserves_continue_consumption_and_recovery() {
    // profiles are supported protocol test cases, validated against the real registry below.
    // profiles 是受支持的协议测试用例，下面通过真实注册表校验。
    let profiles = [
        vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID,
        vulcan_llm_core::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
        vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
        vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID,
        vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
        vulcan_llm_core::GEMINI_INTERACTIONS_PROFILE_ID,
        vulcan_llm_core::GOOGLE_CODE_ASSIST_PROFILE_ID,
        vulcan_llm_core::COHERE_CHAT_PROFILE_ID,
        vulcan_llm_core::TYPESAFE_SYSTEMONE_PROFILE_ID,
        vulcan_llm_core::OPENVULCAN_NATIVE_PROFILE_ID,
    ];
    // registry rejects stale or unsupported profile identifiers instead of assuming catalog membership.
    // registry 拒绝过期或不受支持的协议标识，不假定目录成员关系。
    let registry = vulcan_llm_core::ProtocolRegistry::first_batch().unwrap();
    // snapshot is an offline physical request with no provider-specific cursor or shared credentials.
    // snapshot 是离线物理请求，不含 Provider 专属游标或共享凭据。
    let snapshot = ProviderRequestSnapshot {
        provider_id: "source-provider".into(),
        model_id: "source-model".into(),
        system_prompt: String::new(),
        prompt_layers: Default::default(),
        prompt: "Continue the analysis".into(),
        prompt_in_history: false,
        history: Vec::new(),
        temperature: None,
        max_turns: 1,
        tool_schemas: Vec::new(),
        additional_params: None,
        request_headers: None,
        continuation: Default::default(),
    };
    // source_profile/target_profile include same-profile restarts and both directions of every switch.
    // source_profile／target_profile 包含同协议重启及每次切换的两个方向。
    for source_profile in profiles {
        registry
            .require(source_profile)
            .expect("source profile test case may be outdated");
        for target_profile in profiles {
            registry
                .require(target_profile)
                .expect("target profile test case may be outdated");
            // admitted/confirmed distinguish recovery admission from one-shot API consumption.
            // admitted／confirmed 区分恢复准入与一次性 API 消费。
            for admitted in [false, true] {
                for confirmed in [false, true] {
                    // action is absent for ordinary requests; handoff presence alone grants no recovery.
                    // action 对普通请求为空；仅有交接状态不能授予恢复能力。
                    let action = admitted.then(|| {
                        vulcan_session_core::v1::ClientActionId::try_new("protocol-continue")
                            .unwrap()
                    });
                    // request carries logical state independently of either physical protocol route.
                    // request 独立于两条物理协议路由携带逻辑状态。
                    let mut request = build_test_request();
                    request.turn_start_mode = Some(ExecutionTurnStartMode::RestoredCheckpoint);
                    request.continuation_action_id = action.clone();
                    // preparation initially freezes the source route and initial admission.
                    // preparation 首先冻结来源路由与首次准入。
                    let mut preparation = build_test_preparation();
                    preparation.continuation_action_id = action.clone();
                    preparation.conversation_hook_turn_id =
                        request.conversation_hook_turn_id.clone();
                    preparation.model_blueprint.protocol_profile_id = source_profile.into();
                    preparation.model_blueprint.provider_id = snapshot.provider_id.clone();
                    preparation.model_blueprint.model_id = snapshot.model_id.clone();
                    preparation.model_blueprint.connection_id = "source-connection".into();
                    // source consumes the action only when its physical request obtains a durable permit.
                    // source 仅在物理请求获得可靠许可时消费动作。
                    let mut source = protocol_tracker(&preparation, &request);
                    source.prepare_request(&snapshot, None).unwrap();
                    if confirmed {
                        source.confirm_prepared_request(&snapshot, None).unwrap();
                        source.complete_failure(
                            true,
                            1,
                            3,
                            0,
                            "retryable_provider_transport_error",
                            None,
                            None,
                        );
                    } else {
                        source.reject_prepared_request();
                    }
                    // continuation is the sole cross-stream logical authority.
                    // continuation 是跨流的唯一逻辑权威。
                    let continuation = source.next_request_continuation_identity().unwrap();
                    assert_eq!(continuation.allows_assistant_tail_restore, admitted);
                    assert_eq!(
                        continuation.logical_turn_id,
                        request.conversation_hook_turn_id
                    );
                    request.replace_retry_replay_with_provider_request_continuation(
                        continuation.clone(),
                    );
                    preparation.continuation_action_id = request.continuation_action_id.clone();
                    preparation.model_blueprint.protocol_profile_id = target_profile.into();
                    preparation.model_blueprint.provider_id = "target-provider".into();
                    preparation.model_blueprint.model_id = "target-model".into();
                    preparation.model_blueprint.connection_id = "target-connection".into();
                    // target_snapshot replaces route identity without changing the canonical request body.
                    // target_snapshot 替换路由身份，不改变规范请求正文。
                    let mut target_snapshot = snapshot.clone();
                    target_snapshot.provider_id = preparation.model_blueprint.provider_id.clone();
                    target_snapshot.model_id = preparation.model_blueprint.model_id.clone();
                    // target must reconstruct its own first slot rather than inherit the old tracker.
                    // target 必须重建自己的首次槽位，不能沿用旧跟踪器。
                    let mut target = protocol_tracker(&preparation, &request);
                    assert_eq!(
                        target.next_request_continuation_identity().unwrap(),
                        continuation
                    );
                    // prepared verifies actual V1 facts and telemetry metadata for the target route.
                    // prepared 验证目标路由的实际 V1 事实与遥测元数据。
                    let prepared = target.prepare_request(&target_snapshot, None).unwrap();
                    assert_eq!(prepared.payload["attempt_index"], u32::from(confirmed));
                    assert_eq!(prepared.payload["protocol_profile_id"], target_profile);
                    // facts retain Continue only before the source stream's first durable permit.
                    // facts 仅在来源流首次可靠许可之前保留 Continue。
                    let Some(ProviderV1LifecycleFacts::RequestPrepared(facts)) =
                        prepared.v1_lifecycle
                    else {
                        panic!("expected typed target request preparation");
                    };
                    assert_eq!(facts.request.profile.as_deref(), Some(target_profile));
                    assert_eq!(facts.request.provider, "target-provider");
                    assert_eq!(facts.request.model, "target-model");
                    assert_eq!(
                        facts.request.connection_id.as_deref(),
                        Some("target-connection")
                    );
                    assert_eq!(
                        facts.continuation_action_id,
                        if confirmed { None } else { action }
                    );
                    assert_eq!(
                        facts.purpose,
                        if admitted && !confirmed {
                            ApiPurposeKind::Continue
                        } else {
                            ApiPurposeKind::MainGeneration
                        }
                    );
                    assert_eq!(
                        runtime_stream_turn_start_mode_for_request(&request),
                        if admitted {
                            RuntimeStreamTurnStartMode::ContinuedProviderTurn
                        } else {
                            RuntimeStreamTurnStartMode::RestoredCheckpoint
                        }
                    );
                }
            }
        }
    }
}
