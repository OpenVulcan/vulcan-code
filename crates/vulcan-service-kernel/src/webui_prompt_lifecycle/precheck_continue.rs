//! WebUI prompt lifecycle precheck continue helpers.
//! WebUI 提示词生命周期 precheck continue 辅助逻辑。

use uuid::Uuid;
use vulcan_service_api::{ServiceEvent, VmmDeploymentMode, WorkbenchDeltaEnvelope};
use vulcan_workbench_model::{SessionMessageEntry, SessionMessageKind};

use super::*;
use crate::state::{
    SessionTerminalPersistenceOwner, SessionTerminalPersistenceOwnerGuard,
    SessionTurnAdmissionReservationKind,
};
use crate::vmm_session_controller::open_registered_session_vmm_controller;
use crate::{
    append_webui_session_message_entry_to_state, apply_session_phase_to_state,
    emit_context_injection_changed_delta, emit_service_event_with_trace,
    owning_project_id_for_session, register_session_terminal_persistence_owner,
    sync_hydrated_session_dialogue_ready_marker,
};

/// Acquires native publication ownership only for an effectively enabled native prompt.
/// 只为实际启用原生记忆的提示词获取发布归属。
/// Takes the same admitted runtime and start as continuation; external and disabled paths remain untouched.
/// 接收与继续执行相同的准入运行时及起点；外置及禁用路径保持不变。
pub(crate) async fn prepare_native_recall_publication(
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    start: &SharedPromptTurnStart,
) -> Result<Option<crate::native_vmm::NativeVmmRecallPublication>, String> {
    if !start.vmm_runtime_ready
        || !vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
    {
        return Ok(None);
    }
    // Use the live hook projection, not the detached provider configuration captured before inference.
    // 使用活动 Hook 投影，而非推理前捕获的脱离状态 Provider 配置。
    let effective = agent_runtime
        .vmm_hook_runtime
        .resolve_publication_config(start.session_id, &start.workspace_path)?;
    if !effective.enabled || effective.backend_mode != vulcan_vmm::VmmBackendMode::Native {
        return Ok(None);
    }
    // Clone the native owner without retaining the service mutex over the asynchronous publication barrier.
    // 克隆原生归属方，不跨异步发布屏障持有服务互斥锁。
    let maintenance = state
        .lock()
        .map_err(|error| format!("锁定原生记忆发布状态失败：{error}"))?
        .native_vmm_maintenance
        .clone()
        .ok_or_else(|| "原生记忆发布运行时尚未就绪。".to_owned())?;
    maintenance
        .prepare_recall_publication(start.project_id.as_deref(), &start.session_id.to_string())
        .await
        .map(Some)
}

/// Immutable VMM service readiness captured before one prompt turn is admitted.
/// 在单个提示词回合准入前捕获的不可变 VMM 服务就绪事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WebuiVmmRuntimeReadiness {
    /// Whether the selected native or external backend is actually ready.
    /// 所选原生或外置后端是否真实就绪。
    pub(crate) service_ready: bool,
    /// Whether the ready process also exposes the complete PreCheck capability.
    /// 已就绪进程是否同时具备完整 PreCheck 能力。
    pub(crate) precheck_ready: bool,
}

/// Reads the only service-owned VMM readiness fact used by prompt admission.
/// 读取提示词准入使用的唯一服务级 VMM 就绪事实。
///
/// # Parameters
/// 参数
/// `state` owns the current Provider configuration and backend readiness owners.
/// `state` 持有当前 Provider 配置与后端就绪状态所有者。
///
/// # Returns
/// 返回值
/// Both fields are false unless the service process is fully ready.
/// 除非服务进程完全就绪，否则两个字段均为 false。
pub(crate) fn webui_vmm_runtime_readiness(
    state: &SharedServiceBridgeState,
) -> Result<WebuiVmmRuntimeReadiness, String> {
    let (global_layer, native_host_ai_ready) = {
        let guard = state
            .lock()
            .map_err(|error| format!("读取 VMM 服务就绪状态失败：{error}"))?;
        let Some(agent_runtime) = guard.agent_runtime.as_ref() else {
            return Ok(project_webui_vmm_runtime_readiness(false, false, false));
        };
        (
            agent_runtime.provider_configuration.vmm.clone(),
            guard
                .native_vmm_maintenance
                .as_ref()
                .is_some_and(|maintenance| maintenance.host_ai_status().ready),
        )
    };
    match crate::vmm::current_vmm_deployment_mode(&global_layer) {
        VmmDeploymentMode::External => {
            let service_ready = crate::vmm::current_external_vmm_service_ready(&global_layer);
            let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&global_layer, None, None);
            let precheck_ready = service_ready
                && effective.diagnose_precheck_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
            Ok(project_webui_vmm_runtime_readiness(
                service_ready,
                service_ready,
                precheck_ready,
            ))
        }
        VmmDeploymentMode::Native => {
            let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&global_layer, None, None);
            let service_ready = native_host_ai_ready
                && effective.diagnose_memory_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
            let precheck_ready = service_ready
                && effective.diagnose_precheck_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
            Ok(project_webui_vmm_runtime_readiness(
                service_ready,
                service_ready,
                precheck_ready,
            ))
        }
        VmmDeploymentMode::Disabled => Ok(project_webui_vmm_runtime_readiness(false, false, false)),
    }
}

/// Projects current lifecycle facts into strict prompt-admission readiness.
/// 将当前生命周期事实投影为严格的提示词准入就绪状态。
///
/// # Parameters
/// 参数
/// `process_ready` proves that the current service process or endpoint is live.
/// `process_ready` 证明当前服务进程或端点处于存活状态。
/// `scope_ready` proves that required service configuration is complete.
/// `scope_ready` 证明必需的服务配置已经完整。
/// `precheck_ready` proves that the selected backend exposes PreCheck.
/// `precheck_ready` 证明所选后端提供 PreCheck 能力。
///
/// # Returns
/// 返回值
/// Both capabilities remain unavailable unless service startup and configuration are complete.
/// 除非服务启动且配置完整，否则两项能力均保持不可用。
fn project_webui_vmm_runtime_readiness(
    process_ready: bool,
    scope_ready: bool,
    precheck_ready: bool,
) -> WebuiVmmRuntimeReadiness {
    // serviceReady requires both a live process and the exact configured user/project scope.
    // serviceReady 同时要求进程存活以及精确用户/项目作用域完成配置。
    let service_ready = process_ready && scope_ready;
    WebuiVmmRuntimeReadiness {
        service_ready,
        precheck_ready: service_ready && precheck_ready,
    }
}

/// Builds the shared precheck plan for a WebUI session using the service VMM layering rules.
/// 使用服务运行时的 VMM 分层规则为一个 WebUI 会话构建共享 precheck 计划。
///
/// # Parameters
/// 参数
/// `agent_runtime` owns provider configuration and hook runtime state.
/// `agent_runtime` 持有 Provider 配置与 hook 运行态。
/// `workspace_snapshot` contains session-scoped VMM overrides.
/// `workspace_snapshot` 包含会话级 VMM 覆盖配置。
/// `session_id` selects the session being prepared.
/// `session_id` 选择正在准备的会话。
/// `workspace_path` identifies the cached project VMM layer.
/// `workspace_path` 标识缓存中的项目 VMM 配置层。
///
/// # Returns
/// 返回值
/// A shared precheck plan matching the WebUI service shell.
/// 返回与 WebUI 服务外壳一致的共享 precheck 计划。
pub(crate) fn webui_shared_prompt_precheck_plan(
    agent_runtime: &AgentRuntimeBridge,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    workspace_path: &str,
    vmm_precheck_ready: bool,
) -> Result<SharedPromptPrecheckPlan, String> {
    let project_configuration_started_at = current_unix_millis_value();
    let project_layer = cached_project_vmm_layer(workspace_path);
    webui_trace(
        "precheck_plan_project_config_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(project_configuration_started_at)
        ),
    );
    let session_layer = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .map(|runtime_state| &runtime_state.vmm);
    let effective_vmm_config = vulcan_vmm::VmmEffectiveConfig::resolve(
        &agent_runtime.provider_configuration.vmm,
        Some(&project_layer),
        session_layer,
    );
    let hook_snapshot_started_at = current_unix_millis_value();
    let precheck_hook_snapshots = agent_runtime
        .lock_conversation_hooks()
        .map_err(|error| format!("会话 hook runtime 加锁失败：{error}"))?
        .precheck_hook_snapshots();
    webui_trace(
        "precheck_plan_hook_snapshots_ready",
        format!(
            "session={session_id} hooks={} elapsed_ms={}",
            precheck_hook_snapshots.len(),
            current_unix_millis_value().saturating_sub(hook_snapshot_started_at)
        ),
    );
    let plan = shared_prompt_precheck_plan(
        &effective_vmm_config,
        &precheck_hook_snapshots,
        vmm_precheck_ready,
    );
    // Record the effective precheck decision; the UI-ready flag alone does not prove dispatch.
    // 记录最终 PreCheck 决策；界面显示就绪本身不能证明已经派发。
    tracing::info!(
        "[webui-service][vmm-precheck] stage=plan session={} precheck_ready={} diagnostic={:?} hooks={} skip={}",
        session_id,
        vmm_precheck_ready,
        effective_vmm_config.diagnose_precheck_flow(true),
        precheck_hook_snapshots.len(),
        plan.should_skip_precheck,
    );
    Ok(plan)
}

/// Replaces a Continue request's retained process state with the complete durable V1 controller history.
/// 使用完整可靠的 V1 控制器历史替换 Continue 请求保留的进程状态。
///
/// # Parameters
/// 参数
/// `state` owns the registered V1 handle and `session_id` selects its exact session.
/// `state` 持有已注册 V1 句柄，`session_id` 选择其精确会话。
/// `request` is the request-local execution candidate whose legacy conversation state is non-authoritative.
/// `request` 是请求局部执行候选，其旧会话状态不具备权威性。
///
/// # Returns
/// 返回值
/// Success after the complete authoritative V1 window replaces the candidate, or an explicit projection error.
/// 完整权威 V1 窗口替换候选后返回成功；投影无效时返回明确错误。
fn synchronize_continue_controller_state_from_v1(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    request: &mut ExecutionPreparationRequest,
) -> Result<(), String> {
    // handle is the sole registered owner of this session's durable V1 document.
    // handle 是该会话可靠 V1 文档的唯一已注册所有者。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    // snapshot freezes the same complete Provider projection frontier used by physical preparation.
    // snapshot 冻结与物理准备相同的完整 Provider 投影前沿。
    let snapshot = handle
        .capture_controller_projection_snapshot()
        .map_err(|error| error.to_string())?;
    // projected_state is the sole request-local controller authority for this V1 Continue launch.
    // projected_state 是本次 V1 Continue 启动唯一的请求局部控制器权威。
    let projected_state =
        crate::agent_stream::provider_v1_sink::provider_projection_controller_state(&snapshot)?;
    request.conversation_state = projected_state;
    Ok(())
}

/// Continues a WebUI prompt turn after the shared precheck outcome is known.
/// 在共享 precheck 结果确定后继续 WebUI 提示词回合。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and diagnostics.
/// `app` 发出工作台快照与诊断信息。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `controller_runtime` provides the controller services needed by prompt preparation.
/// `controller_runtime` 提供提示词准备所需的控制器服务。
/// `agent_runtime` owns provider, hook, and AI tool configuration.
/// `agent_runtime` 持有 Provider、hook 与 AI 工具配置。
/// `start` is the prompt turn start produced by the shared reducer.
/// `start` 是共享 reducer 产出的提示词回合起点。
/// `expected_kernel_generation` binds all detached preparation writes to the kernel generation that accepted `start`.
/// `expected_kernel_generation` 将所有分离准备写入绑定到接纳 `start` 的 kernel 代际。
/// `profile_outcome` is the independently dispatched profile preparation result.
/// `profile_outcome` 是独立派发的画像准备结果。
/// `precheck_outcome` is either a real hook result or a skipped outcome.
/// `precheck_outcome` 是真实 hook 结果或跳过后的结果。
///
/// # Returns
/// 返回值
/// A provider execution request when the turn should continue, or `None` when stopped.
/// 当回合应继续时返回 Provider 执行请求；停止时返回 `None`。
// Prompt continuation carries independent profile, precheck and native memory publication outcomes.
// 提示词继续流程携带独立的配置、预检及原生记忆发布结果。
#[expect(
    clippy::too_many_arguments,
    reason = "Prompt continuation carries independent profile, precheck and native memory publication outcomes."
)]
pub(crate) fn continue_webui_prompt_turn_after_preparation_from_state(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    expected_kernel_generation: u64,
    profile_outcome: ConversationProfileOutcome,
    precheck_outcome: ConversationPrecheckOutcome,
    native_publication: Option<&crate::native_vmm::NativeVmmRecallPublication>,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    continue_webui_prompt_turn_after_preparation_for_purpose(
        app,
        state,
        controller_runtime,
        agent_runtime,
        start,
        expected_kernel_generation,
        profile_outcome,
        precheck_outcome,
        native_publication,
        PromptContinuationPurpose::Business,
    )
}

/// Distinguishes ordinary request preparation from a host-authorized auxiliary question execution.
/// 区分普通请求准备与宿主已授权的辅助答疑执行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptContinuationPurpose {
    /// Prepare the current business task, including its Goal context publication.
    /// 准备当前业务任务，包含其目标上下文发布。
    Business,
    /// Prepare auxiliary Q&A without publishing or resuming the business Goal.
    /// 准备辅助答疑，不发布或恢复业务目标。
    QuestionExecution,
}

/// Builds a request using the exact owner and explicitly selected preparation purpose.
/// 使用精确 owner 和明确的准备用途构建请求。
///
/// The emitter, shared state, controller, agent runtime, start record, generation and preparation
/// outcomes have the same ownership as ordinary continuation; purpose controls Goal side effects.
/// 参数为发送器、共享状态、控制器、代理运行时、启动记录、代次和准备结果；用途控制目标副作用。
/// Returns the prepared request, no request for a stale owner, or an explicit preparation error.
/// 返回已准备请求、owner 已失效时的空值，或明确的准备错误。
// Prompt continuation carries independent profile, precheck and native memory publication outcomes.
// 提示词继续流程携带独立的配置、预检及原生记忆发布结果。
#[expect(
    clippy::too_many_arguments,
    reason = "Prompt continuation carries independent profile, precheck and native memory publication outcomes."
)]
pub(crate) fn continue_webui_prompt_turn_after_preparation_for_purpose(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    expected_kernel_generation: u64,
    profile_outcome: ConversationProfileOutcome,
    precheck_outcome: ConversationPrecheckOutcome,
    native_publication: Option<&crate::native_vmm::NativeVmmRecallPublication>,
    purpose: PromptContinuationPurpose,
) -> Result<Option<ExecutionPreparationRequest>, String> {
    let session_id = start.session_id;
    let continue_started_at = current_unix_millis_value();
    webui_after_precheck_trace(
        "after_precheck_01_continue_enter",
        format!("session={session_id} source={:?}", start.source),
    );
    webui_trace(
        "precheck_continue_entered",
        format!("session={session_id} source={:?}", start.source),
    );
    let runs_user_prompt_precheck = start.source.runs_user_prompt_precheck();
    let snapshot_lock_started_at = current_unix_millis_value();
    // capturedOwner makes the initial workspace read indivisible from exact generation validation.
    // capturedOwner 让初始工作区读取与精确代际校验不可分割。
    let Some((mut workspace_snapshot, current_kernel_generation)) =
        capture_webui_prompt_preparation_workspace(state, session_id, &start.preparation_turn_id)?
    else {
        webui_trace(
            "precheck_continue_owner_closed",
            format!(
                "session={session_id} expected_generation={expected_kernel_generation} turn_id={}",
                start.preparation_turn_id
            ),
        );
        return Ok(None);
    };
    if current_kernel_generation != expected_kernel_generation {
        webui_trace(
            "precheck_continue_generation_replaced",
            format!(
                "session={session_id} expected_generation={expected_kernel_generation} current_generation={current_kernel_generation} turn_id={}",
                start.preparation_turn_id
            ),
        );
        return Ok(None);
    }
    webui_trace(
        "precheck_continue_snapshot_loaded",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(snapshot_lock_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_02_snapshot_loaded",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(snapshot_lock_started_at)
        ),
    );
    if workspace_snapshot.session_run_phases.get(&session_id) == Some(&SessionRunPhase::Stopped) {
        if let Ok(mut guard) = state.lock() {
            stop_webui_session_kernel_generation(&mut guard, session_id);
        }
        return Ok(None);
    }
    ensure_webui_session_runtime_entry(
        &mut workspace_snapshot,
        session_id,
        Some(&start.preparation_turn_id),
    )?;
    webui_after_precheck_trace(
        "after_precheck_03_runtime_state_ready",
        format!(
            "session={session_id} elapsed_ms_since_continue={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
        ),
    );
    let project_configuration_started_at = current_unix_millis_value();
    let project_configuration = load_project_configuration(&start.workspace_path);
    webui_after_precheck_trace(
        "after_precheck_05_project_config_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(project_configuration_started_at)
        ),
    );
    // Effective disable revokes session leases before pending events bind to the current user message.
    // 生效禁用会在待处理事件绑定当前用户消息之前撤销会话租约。
    let effective_vmm = agent_runtime
        .vmm_hook_runtime
        .resolve_publication_config(session_id, &start.workspace_path)?;
    let effective_vmm_enabled = effective_vmm.enabled;
    let effective_vmm_backend = effective_vmm.backend_mode;
    // vmmDomainActive cannot be inferred from configuration or retained session state.
    // vmmDomainActive 不能从配置或保留的会话状态反向推断。
    let vmm_domain_active = vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
        && start.vmm_runtime_ready
        && effective_vmm_enabled;
    // The continuation consumes this admitted domain fact instead of raw service readiness, so a
    // project that turns its memory off continues without VMM state rather than demanding an epoch.
    // 续接消费这一已准入的领域事实而非原始服务就绪状态，使关闭记忆的项目不带 VMM 状态继续，而不是要求上下文纪元。
    let vmm_service_ready = start.vmm_runtime_ready;
    let mut start = start;
    start.vmm_runtime_ready = vmm_domain_active;
    if vmm_domain_active
        && effective_vmm_backend == vulcan_vmm::VmmBackendMode::Native
        && native_publication.is_none()
    {
        return Err("原生记忆配置在提示词发布前发生变化，请重新发起。".to_owned());
    }
    // Freeze the current profile identity independently of the returned profile effect.
    // 独立于传回画像效果，冻结当前画像身份。
    let vmm_profile_read_source = if vmm_domain_active {
        match effective_vmm_backend {
            vulcan_vmm::VmmBackendMode::Native => Some(
                native_publication
                    .ok_or_else(|| "原生画像发布缺少当前来源屏障。".to_owned())?
                    .profile_read_source()?,
            ),
            vulcan_vmm::VmmBackendMode::ExternalGrpc => Some(
                vulcan_vmm::VmmMemoryReadSource::new(
                    vulcan_vmm::VmmMemorySource::external(
                        &vulcan_vmm::VmmTransportConfig::from_effective(&effective_vmm),
                        effective_vmm
                            .user_id
                            .ok_or_else(|| "外置画像发布缺少记忆用户 ID。".to_owned())?,
                        effective_vmm
                            .project_id
                            .ok_or_else(|| "外置画像发布缺少记忆项目 ID。".to_owned())?,
                    )
                    .map_err(|error| error.to_string())?,
                    Vec::new(),
                )
                .map_err(|error| error.to_string())?,
            ),
        }
    } else {
        None
    };
    let runtime_reasoning_effort = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| runtime_state.active_reasoning_effort);
    // Keep the preparation identity for idempotent publication; Goal context is projected from V1 events.
    // 保留准备身份用于幂等发布；目标上下文从 V1 事件投影。
    let prompt_turn_id = start.preparation_turn_id.clone();
    let shared_continue_started_at = current_unix_millis_value();
    // vmm_command_id makes the full Provider-boundary transition durably idempotent.
    // vmm_command_id 使完整 Provider 边界转换具备持久幂等性。
    let vmm_command_id = format!("provider_boundary:{prompt_turn_id}");
    // vmm_command_timestamp is shared by domain changes, receipt, and durable publication.
    // vmm_command_timestamp 由领域变更、回执与持久发布共享。
    let vmm_command_timestamp = current_unix_millis_text();
    // continuation_result crosses the serialized controller closure without exposing mutable VMM state.
    // continuation_result 跨越串行控制器闭包传递，且不暴露可变 VMM 状态。
    let mut continuation_result = None;
    // continuation_input is cloned because an idempotent command receipt skips the mutator and
    // therefore needs a second request-building pass against the already committed VMM state.
    // continuation_input 需要克隆，因为幂等命令回执会跳过变更器，必须基于已提交 VMM 状态
    // 再构建一次请求。
    // Capture the authoritative lineage without using a retained workbench compaction mirror.
    // 捕获权威沿革，不使用保留的工作台压缩镜像。
    let vmm_context_epoch = if vmm_domain_active {
        let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
            state, session_id,
        )?;
        let (generation, compaction_id) = handle
            .read(|document| {
                (
                    document.segment_generation(),
                    document
                        .runtime_lock
                        .current_compaction_id
                        .map(|id| id.value().to_string()),
                )
            })
            .map_err(|error| format!("Failed to read VMM host context epoch: {error}"))?;
        Some(vulcan_vmm::VmmContextEpoch::new(generation, compaction_id)?)
    } else {
        None
    };
    let mut continuation_input = SharedPromptTurnContinueInput {
        vmm_context_epoch,
        vmm_profile_read_source,
        start,
        profile_outcome,
        precheck_outcome,
        project_configuration,
        runtime_reasoning_effort,
        provider_reasoning_effort: active_reasoning_selection_effort(
            &agent_runtime.provider_configuration,
        ),
        patch_visible_user_prompt_record: runs_user_prompt_precheck,
    };
    // Validate current native nodes before cloning replay input or taking the Hook configuration read guard.
    // 在克隆重放输入或取得 Hook 配置读守卫之前校验当前原生节点。
    if let Some(publication) = native_publication {
        // The admitted candidate is the exact runtime row that shared continuation will consume.
        // 已准入候选就是共享续接即将消费的准确运行态行。
        let runtime = workspace_snapshot
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| {
                format!("Native profile publication requires session `{session_id}` runtime state.")
            })?;
        if publication.reconcile_profile(runtime, &mut continuation_input.profile_outcome)? {
            // Repair the early Hook cache write without acquiring a write lock inside its publication read guard.
            // 修复早先 Hook 缓存写入，不在其发布读守卫内获取写锁。
            agent_runtime
                .vmm_hook_runtime
                .store_profile_sync_state(session_id, runtime.vmm_profile_sync.clone())?;
        }
    }
    let mut replay_continuation_input = continuation_input.clone();
    let continuation = if vmm_domain_active {
        // vmmController is opened only after every service/configuration prerequisite is true.
        // 仅当全部服务与配置前提成立后才打开 vmmController。
        let vmm_controller =
            open_registered_session_vmm_controller(state, session_id, &vmm_command_timestamp)?;
        // Freeze the admitted source before moving the preparation input into the durable mutator.
        // 在将准备输入移入持久变更器前，冻结已准入来源。
        let publication_source = continuation_input
            .vmm_profile_read_source
            .clone()
            .ok_or_else(|| "Active VMM publication requires an exact read source.".to_owned())?;
        // Resolve the workspace before taking the configuration read guard; no service lock is acquired inside it.
        // 在取得配置读守卫前解析工作区；守卫内部不再取得服务锁。
        let publication_workspace = continuation_input.start.workspace_path.clone();
        // Keep configuration stable through both the original commit and duplicate-receipt reconciliation.
        // 在首次提交和重复回执协调期间始终保持配置稳定。
        agent_runtime.vmm_hook_runtime.with_current_profile_publication(
            session_id,
            &publication_workspace,
            &publication_source,
            || -> Result<_, String> {
                // vmmCommit owns both shared prompt VMM mutation and durable publication under one session lock.
                // vmmCommit 在同一会话锁内拥有共享提示词 VMM 变更与持久发布。
                let vmm_commit = vmm_controller.execute_command(
                    &vmm_command_id,
                    &vmm_command_timestamp,
                    |candidate| {
                        // Recheck resident and delayed incoming recall before binding pending events to the user message.
                        // 在向用户消息绑定待处理事件前，复查驻留召回及迟到传入召回。
                        if let Some(publication) = native_publication {
                            publication.reconcile(
                                candidate,
                                &mut continuation_input.precheck_outcome,
                                &vmm_command_timestamp,
                            )?;
                        }
                        // continuation is built against the exact isolated controller candidate.
                        // continuation 基于精确的隔离控制器候选状态构建。
                        let continuation = continue_shared_prompt_turn_after_preparation(
                            &mut workspace_snapshot,
                            candidate,
                            continuation_input,
                        )?;
                        continuation_result = Some(continuation);
                        Ok(())
                    },
                )?;
                // A duplicate receipt rebuilds only the transient request from committed state.
                // 重复回执只会从已提交状态重建临时请求。
                Ok(if vmm_commit.applied {
                    continuation_result.ok_or_else(|| {
                        format!(
                            "Core VMM Provider-boundary command `{vmm_command_id}` committed without a continuation result."
                        )
                    })?
                } else {
                    if continuation_result.is_some() {
                        return Err(format!(
                            "Core VMM Provider-boundary command `{vmm_command_id}` returned an invalid duplicate continuation result."
                        ));
                    }
                    webui_trace(
                        "precheck_continue_vmm_command_replayed",
                        format!("session={session_id} command_id={vmm_command_id}"),
                    );
                    // Persist changed native visibility through its own controller command before rebuilding
                    // the transient request; the original provider receipt remains untouched.
                    // 在重建临时请求前，通过独立控制器命令持久化变化后的原生可见性；
                    // 原始 Provider 回执保持不变。
                    if let Some(publication) = native_publication {
                        publication.reconcile_replayed_boundary(
                            &vmm_controller,
                            &mut replay_continuation_input.precheck_outcome,
                            &vmm_command_timestamp,
                        )?;
                    } else {
                        // A remote source change also needs durable invalidation when the original command cannot rerun.
                        // 原命令不能重跑时，远端来源变化同样需要持久失效处理。
                        vmm_controller.reconcile_read_source(
                            replay_continuation_input.vmm_profile_read_source.as_ref(),
                            &vmm_command_timestamp,
                        )?;
                    };
                    // Reconcile lineage durably even though the original boundary receipt cannot run again.
                    // 即使原始边界回执不能再次执行，也要持久协调沿革。
                    let mut replay_vmm_state = vmm_controller.reconcile_context_epoch(
                        replay_continuation_input.vmm_context_epoch.as_ref().ok_or_else(||
                            "Active VMM replay requires the exact host context epoch.".to_owned())?,
                        &publication_source,
                        &vmm_command_timestamp,
                    )?;
                    continue_shared_prompt_turn_after_preparation(
                        &mut workspace_snapshot,
                        &mut replay_vmm_state,
                        replay_continuation_input,
                    )
                    .map_err(|error| {
                        format!(
                            "Core VMM Provider-boundary command `{vmm_command_id}` was already committed, but continuation reconstruction failed: {error}"
                        )
                    })?
                })
            },
        )??
    } else {
        // An unconfigured or stopped service bypasses every VMM controller and uses empty local state.
        // 未完成配置或已停止的服务会绕过全部 VMM Controller，并使用空的本地状态继续普通请求。
        webui_trace(
            "precheck_continue_vmm_bypassed",
            format!(
                "session={session_id} service_ready={vmm_service_ready} effective_enabled={effective_vmm_enabled}"
            ),
        );
        let mut inactive_vmm_state = vulcan_vmm::VmmSessionMemoryState::default();
        continue_shared_prompt_turn_after_preparation(
            &mut workspace_snapshot,
            &mut inactive_vmm_state,
            replay_continuation_input,
        )
        .map_err(|error| format!("Build prompt without VMM failed: {error}"))?
    };
    webui_trace(
        "precheck_continue_request_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(shared_continue_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_06_shared_continue_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(shared_continue_started_at)
        ),
    );
    if let Some(detail) = continuation.precheck_failure_detail.as_deref() {
        tracing::warn!("[webui-service] precheck hook failed but request will continue: {detail}");
        if let Err(error) = push_webui_service_diagnostic(
            state,
            format!("warning · Precheck hook 执行失败：{detail}"),
        ) {
            tracing::warn!("[webui-service] record precheck warning failed: {error}");
        }
    }
    if let Some(detail) = continuation.profile_failure_detail.as_deref() {
        tracing::warn!("[webui-service] profile hook failed but request will continue: {detail}");
        if let Err(error) = push_webui_service_diagnostic(
            state,
            format!("warning · 画像准备 Hook 执行失败：{detail}"),
        ) {
            tracing::warn!("[webui-service] record profile warning failed: {error}");
        }
    }
    let mut request = continuation.request;
    if request.continuation_action_id.is_some() {
        // Continue never mixes retained legacy state with V1 history; V1 owns the complete controller window.
        // Continue 绝不混合保留的旧状态与 V1 历史；完整控制器窗口由 V1 持有。
        synchronize_continue_controller_state_from_v1(state, session_id, &mut request)?;
    }
    // Live VMM readiness is captured at turn admission so model-visible memory tools cannot outlive
    // a stopped or failed transport. Child tasks inherit the same immutable request fact.
    // 在回合准入时捕获 VMM 实时就绪状态，避免模型可见记忆工具在传输停止或失败后继续暴露；子任务继承同一不可变请求事实。
    request.vmm_runtime_ready = vmm_domain_active;
    // Main-session requests consume V1 configuration before construction, including Goal and queued continuations.
    // 主会话请求在构建前消费 V1 配置，包括目标续跑和排队续跑，不能再次从全局选择或旧镜像取模型。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
        .ok_or_else(|| format!("Session `{session_id}` has no admitted runtime state."))?;
    crate::session_request_config_v1::synchronize_v1_dispatch_model_selection(
        handle.as_ref(),
        agent_runtime.public_ai_runtime.as_ref(),
        runtime_state,
    )?;
    request.provider_id_override = runtime_state.active_provider_id.clone();
    request.model_id_override = runtime_state.active_model_id.clone();
    request.connection_id_override = runtime_state.active_connection_id.clone();
    request.provider_route_lease_identity = runtime_state.provider_route_lease.clone();
    if request.provider_id_override.is_some() {
        request.reasoning_effort = runtime_state
            .active_reasoning_effort
            .unwrap_or(vulcan_provider_config::config::ReasoningEffortLevel::Off);
        request.temperature = runtime_state.active_temperature;
    }
    request.controller_runtime = Some(controller_runtime.clone());
    // Explicit media tools resolve logical resource addresses through the same V1 session registry as dispatch.
    // 显式媒体工具通过与分派相同的V1会话注册表解析逻辑资源地址。
    request.media_resource_runtime =
        Some(crate::session_media_resources_v1::session_v1_media_resource_runtime(state)?);
    // Native Goal tools receive the exact request session's V1 domain port; no SQLite store is exposed.
    // 原生 Goal 工具取得精确请求会话的 V1 领域端口；不再暴露 SQLite 存储。
    request.native_goal_runtime = match purpose {
        PromptContinuationPurpose::Business => Some(
            crate::session_goals::session_v1_native_goal_runtime(state.clone(), app.clone()),
        ),
        PromptContinuationPurpose::QuestionExecution => None,
    };
    // Collaboration tools receive a host-owned port and derive the sender from each actual tool call.
    // 协作工具取得宿主端口，并从每次真实工具调用确定发送方身份。
    request.native_agent_collaboration_runtime = Some(
        crate::agent_collaboration::session_v1_native_agent_collaboration_runtime(
            state.clone(),
            app.clone(),
        ),
    );
    // Browser tools are registered only while their exact plugin is enabled; each call still
    // checks the live generation and derives ownership from the trusted execution session.
    // 浏览器工具仅在精确插件启用时注册；每次调用仍核对实时代次并从可信执行会话确定归属。
    request.native_browser_runtime = crate::browser::native_browser_runtime(state.clone())?;
    // vceProjectReady binds tool visibility to both the exact project and the current global model contract.
    // vceProjectReady 将工具可见性同时绑定到精确项目与当前全局模型契约。
    let vce_project_ready = request
        .project_id
        .as_deref()
        .is_some_and(|project_id| crate::vce::vce_project_tool_enabled(state, project_id));
    // vceRetrievalHandler freezes the current process manager while retaining a live fail-closed callback.
    // vceRetrievalHandler 冻结当前进程管理器，同时保留实时失败关闭回调。
    request.vce_retrieval_handler = if vce_project_ready {
        let manager = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .vce_runtime
            .clone();
        manager.map(|manager| crate::vce::vce_retrieval_handler(state.clone(), manager))
    } else {
        None
    };
    if request.vce_retrieval_handler.is_none()
        && !request
            .tool_request_hidden_tool_ids
            .iter()
            .any(|tool_id| tool_id == vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID)
    {
        request.tool_request_hidden_tool_ids.push(String::from(
            vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID,
        ));
    }
    let goal_context_started_at = current_unix_millis_value();
    match purpose {
        PromptContinuationPurpose::Business => {
            // Goal state owns durable context events; usage publication belongs only to business preparation.
            // 目标状态持有可靠上下文事件；用量发布只属于业务准备。
            let (session_goal_context, goal_turn_usage_context) =
                session_goal_prompt_parts_from_state(state, session_id)?;
            request.session_goal_context = session_goal_context;
            persist_goal_turn_usage_context(handle.as_ref(), goal_turn_usage_context.as_deref())?;
        }
        PromptContinuationPurpose::QuestionExecution => {
            request.session_goal_context = None;
        }
    }
    webui_after_precheck_trace(
        "after_precheck_09_goal_context_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(goal_context_started_at)
        ),
    );
    let plan_context_started_at = current_unix_millis_value();
    // V1 Plan entry and exit are projected from their original event positions exactly once.
    // V1 Plan 进入与退出只从原始事件位置投影一次。
    request.session_plan_context = None;
    webui_after_precheck_trace(
        "after_precheck_10_plan_context_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(plan_context_started_at)
        ),
    );
    let session_locator_started_at = current_unix_millis_value();
    // sessionRegistry is cloned without retaining the service-state lock across registry access.
    // sessionRegistry 在不让服务状态锁跨越注册表访问的前提下完成克隆。
    let session_registry = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .session_runtime_registry
        .clone();
    // sessionDirectory comes only from the registered V1 owner; the removed TaskSession store must
    // never receive the V1 root because it writes legacy metadata and transcript segments there.
    // sessionDirectory 只来自已注册的 V1 owner；已移除的 TaskSession 存储绝不能接收 V1 根目录，
    // 因为它会在该目录写入旧 metadata 与转录分段。
    let session_directory = session_registry
        .get(session_id)
        .map_err(|error| error.to_string())?
        .storage_locator()
        .map_err(|error| error.to_string())?
        .session_directory();
    request.session_storage_directory = Some(session_directory.clone());
    webui_after_precheck_trace(
        "after_precheck_11_v1_session_locator_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(session_locator_started_at)
        ),
    );
    // now timestamps the authoritative pre-provider workspace transition.
    // now 为权威的 Provider 调用前工作区转换提供时间戳。
    let now = current_unix_millis_text();
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.updated_at = now;
    }
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Running;
        session.focus = String::from("正在请求");
        session.last_updated_label = String::from("刚刚");
    }
    // vmmProjection retains the exact memory state after the detached slice moves into commit.
    // vmmProjection 在分离切片移入提交后继续保留精确记忆状态。
    let (vmm_layer, vmm_outbox, last_compaction_id) = {
        let runtime_state = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .ok_or_else(|| {
                format!("Prompt continuation is missing runtime state for session `{session_id}`.")
            })?;
        (
            runtime_state.vmm.clone(),
            runtime_state.vmm_writeback_outbox.clone(),
            runtime_state.last_compaction_id.clone(),
        )
    };
    // phase_commit_started_at measures the authoritative in-memory commit and its required projections.
    // phase_commit_started_at 测量权威内存提交及其必要投影的耗时。
    let phase_commit_started_at = current_unix_millis_value();
    let phase_committed =
        commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            app,
            state,
            workspace_snapshot,
            session_id,
            SessionRunPhase::Preparing,
            expected_kernel_generation,
            &prompt_turn_id,
        )?;
    if !phase_committed {
        webui_trace(
            "precheck_continue_phase_owner_closed",
            format!(
                "session={session_id} expected_generation={expected_kernel_generation} turn_id={prompt_turn_id}"
            ),
        );
        return Ok(None);
    }
    // The VMM Hook projection refreshes only after authoritative memory and required disk state agree.
    // VMM Hook 投影仅在权威内存与所需磁盘状态一致后刷新。
    // vmm_snapshot_started_at measures the derived Hook projection refresh.
    // vmm_snapshot_started_at 测量派生 Hook 投影刷新的耗时。
    let vmm_snapshot_started_at = current_unix_millis_value();
    agent_runtime.vmm_hook_runtime.replace_session_projection(
        session_id,
        &vmm_layer,
        &vmm_outbox,
        last_compaction_id.as_deref(),
    )?;
    webui_trace(
        "precheck_continue_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(vmm_snapshot_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_07_hook_snapshot_ready",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(vmm_snapshot_started_at)
        ),
    );
    if let Err(error) = emit_context_injection_changed_delta(app, state, session_id) {
        webui_trace(
            "context_injection_delta_emit_failed_after_commit",
            format!("session={session_id} error={error}"),
        );
    }
    webui_trace(
        "precheck_continue_phase_committed",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(phase_commit_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_15_phase_delta_committed",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(phase_commit_started_at)
        ),
    );
    if runs_user_prompt_precheck {
        webui_after_precheck_trace(
            "after_precheck_16_v1_owner_retained",
            format!("session={session_id}"),
        );
    } else {
        webui_after_precheck_trace(
            "after_precheck_16_snapshot_persist_skipped",
            format!("session={session_id}"),
        );
    }
    webui_trace(
        "precheck_continue_finished",
        format!(
            "session={session_id} elapsed_ms={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
        ),
    );
    webui_after_precheck_trace(
        "after_precheck_17_continue_finished",
        format!(
            "session={session_id} elapsed_ms_since_continue={}",
            current_unix_millis_value().saturating_sub(continue_started_at)
        ),
    );
    Ok(Some(request))
}

/// Spawns independent profile preparation plus the optional user-precheck continuation chain.
/// 为一个 WebUI 提示词回合启动独立画像准备及可选用户 precheck 续接链路。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and is forwarded to the stream task.
/// `app` 发出工作台快照，并继续传递给流任务。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
/// `expected_kernel_generation` is the immutable generation captured before asynchronous preparation dispatch.
/// `expected_kernel_generation` 是异步准备派发前捕获的不可变代际。
/// `run_precheck` decides whether the user-recall line dispatches or returns a skipped outcome.
/// `run_precheck` 决定用户召回线路是执行派发还是返回跳过结果。
pub(crate) fn spawn_webui_prompt_prepare_continue_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    expected_kernel_generation: u64,
    run_precheck: bool,
) -> Result<(), String> {
    let session_id = start.session_id;
    // Register before spawning so an accepted task never loses ownership during handoff.
    // 在生成任务前登记，避免已接纳任务在交接期间丢失所有权。
    let dispatch_owner =
        crate::agent_collaboration::dispatch_owner::DispatchOwner::acquire(&state, session_id)?;
    // preparation_turn_id remains available for exact VMM failure cleanup after start moves into continuation.
    // preparation_turn_id 在 start 移入续接流程后仍用于精确的 VMM 失败清理。
    let preparation_turn_id = start.preparation_turn_id.clone();
    webui_trace(
        "prompt_prepare_task_spawn",
        format!("session={session_id} run_precheck={run_precheck}"),
    );
    spawn_service_task(async move {
        // Cancellation and completion both release the preparation owner.
        // 取消与完成都会释放准备所有者。
        let _dispatch_owner = dispatch_owner;
        // The accepted turn, not navigation or policy publication, owns one cancellable prewarm batch.
        // 已接纳回合拥有单个可取消预热批次，导航及策略发布不拥有触发。
        if let Err(error) = crate::plugin_runtime_services::automatic_prewarm::spawn_for_turn(
            &state,
            agent_runtime.vulcan_plugin_runtime.clone(),
            session_id,
            &preparation_turn_id,
            expected_kernel_generation,
        ) {
            tracing::warn!("[plugin-prewarm] session={session_id} trigger rejected: {error}");
        }
        let preparation_started_at = current_unix_millis_value();
        webui_trace(
            "prompt_prepare_task_run_start",
            format!("session={session_id} run_precheck={run_precheck}"),
        );
        // profile_request always runs, while precheck_request remains governed only by user-recall policy.
        // profile_request 始终运行，而 precheck_request 仅受用户召回策略控制。
        let profile_request = start.profile_request.clone();
        let precheck_request = start.precheck_request.clone();
        // profile_outcome and precheck_outcome retain separate reports even when they execute concurrently.
        // profile_outcome 与 precheck_outcome 即使并发执行也保留相互独立的报告。
        let (profile_outcome, precheck_outcome) = if run_precheck {
            tokio::join!(profile_request.run(), precheck_request.run())
        } else {
            (profile_request.run().await, precheck_request.skip())
        };
        // Keep handler failures visible even when the dispatch policy allows the model turn to continue.
        // 即使派发策略允许模型回合继续，也让处理器失败可见。
        match precheck_outcome.report.as_ref() {
            Ok(report) => tracing::info!(
                "[webui-service][vmm-precheck] stage=hook_result session={} run={} completed={:?} failures={}",
                session_id,
                run_precheck,
                report.completed_hooks,
                report.failures.len(),
            ),
            Err(error) => tracing::warn!(
                "[webui-service][vmm-precheck] stage=hook_result session={} run={} dispatch_error={}",
                session_id,
                run_precheck,
                error,
            ),
        }
        if let Ok(report) = precheck_outcome.report.as_ref() {
            for failure in &report.failures {
                tracing::warn!(
                    "[webui-service][vmm-precheck] stage=hook_failure session={} hook={} error={}",
                    session_id,
                    failure.hook_id,
                    failure.message,
                );
            }
        }
        webui_after_precheck_trace(
            "after_precheck_00_outcome_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(preparation_started_at)
            ),
        );
        webui_trace(
            "prompt_prepare_task_run_finished",
            format!(
                "session={session_id} run_precheck={run_precheck} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(preparation_started_at)
            ),
        );
        let continue_started_at = current_unix_millis_value();
        // Acquire the short native barrier after AI preparation and before capturing the final workspace owner.
        // 在 AI 准备之后、捕获最终工作区归属之前获取短时原生屏障。
        let native_publication =
            prepare_native_recall_publication(&state, &agent_runtime, &start).await;
        match native_publication.and_then(|publication| {
            continue_webui_prompt_turn_after_preparation_from_state(
                &app,
                &state,
                &controller_runtime,
                &agent_runtime,
                start,
                expected_kernel_generation,
                profile_outcome,
                precheck_outcome,
                publication.as_ref(),
            )
        }) {
            Ok(Some(mut request)) => {
                // Discover owner-scoped Playwright schemas before the Provider sees this turn's
                // tool catalog; an empty browser uses a non-connectable template until AI opens one.
                // 在 Provider 看到本回合工具目录前发现所有者范围 Playwright Schema；没有浏览器时先用
                // 不可连接的模板，直至 AI 打开浏览器。
                match crate::browser::prepare_browser_mcp_bindings(state.clone(), session_id).await
                {
                    Ok(bindings) => request.supplemental_mcp_tools = bindings,
                    Err(error) => {
                        tracing::warn!("[webui-service] prepare browser MCP failed: {error}");
                        if let Err(cleanup_error) = mark_webui_prompt_preparation_failed(
                            &app,
                            &state,
                            &agent_runtime,
                            session_id,
                            &preparation_turn_id,
                            &error,
                        )
                        .await
                        {
                            tracing::warn!(
                                "[webui-service] cleanup failed browser MCP preparation failed: {cleanup_error}"
                            );
                        }
                        return;
                    }
                }
                webui_after_precheck_trace(
                    "after_precheck_18_stream_spawn_call",
                    format!(
                        "session={session_id} elapsed_ms_since_continue_call={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
                webui_trace(
                    "prompt_prepare_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=true elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
                spawn_agent_stream_task(
                    app,
                    state,
                    controller_runtime,
                    agent_runtime,
                    request,
                    expected_kernel_generation,
                    preparation_turn_id,
                )
            }
            Ok(None) => {
                webui_trace(
                    "prompt_prepare_task_continue_finished",
                    format!(
                        "session={session_id} will_spawn_stream=false elapsed_ms={}",
                        current_unix_millis_value().saturating_sub(continue_started_at)
                    ),
                );
            }
            Err(error) => {
                tracing::warn!(
                    "[webui-service] continue WebUI prompt after preparation failed: {error}"
                );
                if let Err(cleanup_error) = mark_webui_prompt_preparation_failed(
                    &app,
                    &state,
                    &agent_runtime,
                    session_id,
                    &preparation_turn_id,
                    &error,
                )
                .await
                {
                    tracing::warn!(
                        "[webui-service] cleanup failed WebUI prompt preparation failed: {cleanup_error}"
                    );
                }
            }
        }
    });
    Ok(())
}

/// Starts precheck dispatch or immediately continues with a skipped precheck using shared semantics.
/// 使用共享语义启动 precheck 调度，或在需要跳过时立即继续。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and runtime stream events.
/// `app` 发出工作台快照与运行时流事件。
/// `state` owns retained service state.
/// `state` 持有保留的服务状态。
/// `controller_runtime` provides workspace persistence.
/// `controller_runtime` 提供工作区持久化能力。
/// `agent_runtime` owns provider and hook runtimes.
/// `agent_runtime` 持有 Provider 与 hook 运行时。
/// `start` is the first-stage prompt turn.
/// `start` 是第一阶段提示词回合。
/// `precheck_plan` decides whether handlers should run.
/// `precheck_plan` 决定是否需要运行处理器。
///
/// # Returns
/// 返回值
/// Whether the independent prompt-preparation continuation task was started.
/// 是否已经启动独立提示词准备续接任务。
pub(crate) fn start_webui_precheck_or_continue(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    start: SharedPromptTurnStart,
    precheck_plan: SharedPromptPrecheckPlan,
) -> Result<bool, String> {
    let session_id = start.session_id;
    // expectedKernelGeneration is captured before any phase mutation so Stop can close every later detached write.
    // expectedKernelGeneration 在任何阶段变更前捕获，使 Stop 能关闭所有后续分离写入。
    let Some(expected_kernel_generation) =
        capture_webui_prompt_preparation_generation(state, session_id, &start.preparation_turn_id)?
    else {
        tracing::info!(
            "[webui-service][vmm-precheck] stage=dispatch session={} result=owner_closed",
            session_id,
        );
        webui_trace(
            "precheck_dispatch_owner_closed",
            format!("session={session_id} turn_id={}", start.preparation_turn_id),
        );
        return Ok(false);
    };
    tracing::info!(
        "[webui-service][vmm-precheck] stage=dispatch session={} source={:?} has_hooks={} skip={}",
        session_id,
        start.source,
        precheck_plan.has_precheck_hooks,
        precheck_plan.should_skip_precheck,
    );
    webui_trace(
        "precheck_dispatch_entered",
        format!(
            "session={} has_hooks={} skip={} labels={}",
            session_id,
            precheck_plan.has_precheck_hooks,
            precheck_plan.should_skip_precheck,
            precheck_plan.progress_labels.len()
        ),
    );
    if precheck_plan.should_skip_precheck || !precheck_plan.has_precheck_hooks {
        // Profile preparation remains asynchronous and mandatory even when user recall is skipped.
        // 即使跳过用户召回，画像准备仍保持异步且必须执行。
        spawn_webui_prompt_prepare_continue_task(
            app.clone(),
            state.clone(),
            controller_runtime.clone(),
            agent_runtime.clone(),
            start,
            expected_kernel_generation,
            false,
        )?;
        webui_trace(
            "precheck_dispatch_skipped_profile_task_spawned",
            format!("session={session_id}"),
        );
        return Ok(true);
    }
    // Emits the memory-recall phase only after the shared plan confirms dispatch will run.
    // 仅在共享计划确认会调度 precheck 后才发出记忆召回阶段。
    let mut workspace_snapshot = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::MemoryRecall);
    let phase_committed =
        commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            app,
            state,
            workspace_snapshot,
            session_id,
            SessionRunPhase::MemoryRecall,
            expected_kernel_generation,
            &start.preparation_turn_id,
        )?;
    if !phase_committed {
        return Ok(false);
    }
    spawn_webui_prompt_prepare_continue_task(
        app.clone(),
        state.clone(),
        controller_runtime.clone(),
        agent_runtime.clone(),
        start,
        expected_kernel_generation,
        true,
    )?;
    webui_trace(
        "precheck_dispatch_spawned_task",
        format!("session={session_id}"),
    );
    Ok(true)
}

/// Captures the atomic in-memory boundary committed before prompt-preparation cleanup performs external work.
/// 捕获提示词准备清理执行外部工作前原子提交的内存边界。
#[derive(Debug)]
pub(crate) struct PromptPreparationFailureBoundaryCommit {
    /// Exact owner that keeps Continue closed until the terminal snapshot is durable.
    /// 在终态快照可靠保存前保持 Continue 关闭的精确所有者。
    terminal_persistence_owner: SessionTerminalPersistenceOwner,
    /// Optional ordered phase delta produced by the same terminal-state transaction.
    /// 由同一终态事务产出的可选有序阶段增量。
    phase_event: Option<WorkbenchDeltaEnvelope>,
    /// Project identity required to terminate the exact Hook turn.
    /// 终止精确 Hook 回合所需的项目身份。
    owning_project_id: Option<String>,
    /// Non-fatal diagnostics accumulated while forcing the boundary closed.
    /// 强制关闭边界时累计的非致命诊断。
    diagnostics: Vec<String>,
}

/// Atomically stops one preparation turn, registers its persistence owner, and commits a fail-closed terminal draft.
/// 原子停止一个准备回合、登记其持久化所有者，并提交失败关闭的终态草稿。
///
/// # Parameters
/// 参数
/// `state` owns the exact kernel, owner tables, retained workspace, and visible workbench projection.
/// `state` 持有精确 kernel、所有者表、保留工作区与可见工作台投影。
/// `session_id` and `failed_turn_id` identify the preparation turn being terminalized.
/// `session_id` 与 `failed_turn_id` 标识正在终态化的准备回合。
/// `error` is the controlled failure diagnostic.
/// `error` 是受控失败诊断。
///
/// # Returns
/// 返回值
/// The exact persistence owner and detached facts needed by the asynchronous settlement phase.
/// 返回异步结算阶段所需的精确持久化所有者与分离事实。
pub(crate) fn commit_prompt_preparation_failure_boundary(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    failed_turn_id: &str,
    error: &str,
) -> Result<PromptPreparationFailureBoundaryCommit, String> {
    // guard serializes owner validation, kernel stop, retry closure, and retained-state replacement.
    // guard 串行化所有者校验、kernel 停止、重试关闭与保留状态替换。
    let mut guard = state
        .lock()
        .map_err(|lock_error| format!("failed to lock service state: {lock_error}"))?;
    if guard.active_stream_tasks.contains_key(&session_id) {
        return Err(format!(
            "Prompt preparation cleanup rejected an active stream owner for session `{session_id}`."
        ));
    }
    if let Some(reservation) = guard.session_turn_admission_reservations.get(&session_id)
        && reservation.kind != SessionTurnAdmissionReservationKind::PromptSubmission
    {
        return Err(format!(
            "Prompt preparation cleanup conflicts with {} for session `{session_id}`.",
            reservation.kind.diagnostic_label()
        ));
    }
    // active_kernel_turn_id proves this cleanup cannot stop a replacement turn.
    // active_kernel_turn_id 证明当前清理不会停止替代回合。
    let active_kernel_turn_id = guard
        .session_runtime_kernels
        .get(&session_id)
        .and_then(|kernel| kernel.active_turn().map(|turn| turn.turn_id.clone()));
    if active_kernel_turn_id
        .as_deref()
        .is_some_and(|turn_id| turn_id != failed_turn_id)
    {
        return Err(format!(
            "Prompt preparation cleanup turn mismatch for session `{session_id}`: failed=`{failed_turn_id}` active=`{}`.",
            active_kernel_turn_id.as_deref().unwrap_or_default()
        ));
    }
    if let Some(existing_owner) = guard.session_terminal_persistence_owners.get(&session_id) {
        return Err(format!(
            "Prompt preparation cleanup conflicts with terminal owner `{existing_owner:?}` for session `{session_id}`."
        ));
    }
    // terminal_persistence_owner uses a unique transaction identity to avoid generation-zero ABA cleanup.
    // terminal_persistence_owner 使用唯一事务身份，避免 generation-zero ABA 清理。
    let terminal_persistence_owner =
        SessionTerminalPersistenceOwner::PromptPreparation(Uuid::new_v4());
    if guard.workspace_snapshot.is_none() {
        if active_kernel_turn_id.is_some() {
            register_session_terminal_persistence_owner(
                &mut guard,
                session_id,
                terminal_persistence_owner.clone(),
            )?;
            stop_webui_session_kernel_generation(&mut guard, session_id);
            return Err(format!(
                "Prompt preparation workspace is unavailable for session `{session_id}`; the kernel was stopped and terminal owner `{terminal_persistence_owner:?}` was retained."
            ));
        }
        return Err(String::from("workspace snapshot is not ready"));
    }
    register_session_terminal_persistence_owner(
        &mut guard,
        session_id,
        terminal_persistence_owner.clone(),
    )?;
    stop_webui_session_kernel_generation(&mut guard, session_id);
    // workspace_snapshot starts from the kernel-stopped authoritative state mirrored by the same lock.
    // workspace_snapshot 从同一锁内已镜像 kernel 停止事实的权威状态开始。
    let mut workspace_snapshot = guard
        .workspace_snapshot
        .clone()
        .ok_or_else(|| String::from("workspace snapshot disappeared after kernel stop"))?;
    // now is shared by the error record, ledger boundary, and retained session metadata.
    // now 由错误记录、账本边界与保留会话元数据共享。
    let now = current_unix_millis_text();
    // diagnostics records every degraded projection while terminal ownership remains closed.
    // diagnostics 记录终态所有权保持关闭期间的每个降级投影。
    let mut diagnostics = Vec::new();
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        runtime_state.active_precheck_turn_id = None;
        // failure_entry creates immediate, durable feedback for asynchronous post-ACK failures.
        // failure_entry 为 ACK 后异步失败创建即时且可靠的反馈。
        let failure_entry = SessionMessageEntry::new(
            SessionMessageKind::Error,
            "请求准备失败",
            format!("本轮请求未能继续，已安全停止。原因：{error}"),
        );
        if let Err(projection_error) =
            append_webui_session_message_entry_to_state(runtime_state, session_id, failure_entry)
        {
            diagnostics.push(format!(
                "Prompt preparation failure message projection failed: {projection_error}"
            ));
        }
        runtime_state.updated_at = now.clone();
    } else {
        diagnostics.push(String::from(
            "Prompt preparation failure runtime state is unavailable.",
        ));
    }
    if workspace_snapshot
        .session_runtime_state
        .contains_key(&session_id)
    {
        sync_hydrated_session_dialogue_ready_marker(&mut workspace_snapshot, session_id);
    }
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Stopped);
    if let Some(session) = workspace_snapshot
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Blocked;
        session.focus = String::from("提示词准备结果无法应用，已停止本轮请求。");
        session.last_updated_label = String::from("刚刚");
    }
    // owning_project_id is derived before the committed workspace replaces shared state.
    // owning_project_id 在已提交工作区替换共享状态前派生。
    let owning_project_id = owning_project_id_for_session(&workspace_snapshot, session_id);
    guard.workspace_snapshot = Some(workspace_snapshot);
    // phase_event is best-effort because the owner marker and stopped kernel already fail closed.
    // phase_event 为尽力投影，因为所有者标记与已停止 kernel 已经失败关闭。
    let phase_event =
        match apply_session_phase_to_state(&mut guard, session_id, SessionRunPhase::Stopped) {
            Ok(event) => Some(event),
            Err(phase_error) => {
                diagnostics.push(format!(
                    "Prompt preparation stopped-phase projection failed: {phase_error}"
                ));
                None
            }
        };
    Ok(PromptPreparationFailureBoundaryCommit {
        terminal_persistence_owner,
        phase_event,
        owning_project_id,
        diagnostics,
    })
}

/// Emits an immediate message-window patch after the failure record enters retained workspace state.
/// 在失败记录进入保留工作区状态后立即发出消息窗口补丁。
///
/// # Parameters
/// 参数
/// `app` emits the ordered patch, `state` owns its authoritative revision, and `session_id` selects the visible conversation.
/// `app` 发出有序补丁，`state` 持有其权威修订，`session_id` 选择可见会话。
///
/// # Returns
/// 返回值
/// Success when no focused surface needs a patch or the exact patch was emitted.
/// 没有聚焦界面需要补丁或精确补丁已发出时返回成功。
fn emit_prompt_preparation_failure_message_window_delta(
    _app: &ServiceEventEmitter,
    _state: &SharedServiceBridgeState,
    _session_id: SessionId,
) -> Result<(), String> {
    // Preparation failures are durable V1 display/control events and need no Workbench transcript rebuild.
    // 准备失败属于持久化 V1 显示/控制事件，不需要重建 Workbench 转录。
    Ok(())
}

/// Marks a WebUI prompt turn as stopped when either preparation outcome cannot be applied.
/// 当 WebUI 提示词回合无法应用任一准备线路结果时，将其标记为已停止。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `agent_runtime` provides projection configuration for the emitted workbench.
/// `agent_runtime` 提供发出工作台快照所需的投影配置。
/// `session_id` selects the failed prompt turn.
/// `session_id` 选择失败的提示词回合。
/// `failed_turn_id` identifies the exact logical VMM owner that must be abandoned.
/// `failed_turn_id` 标识必须放弃的精确 VMM 逻辑所有者。
/// `error` describes the continuation failure.
/// `error` 描述 continuation 失败原因。
///
/// # Returns
/// 返回值
/// `Ok(())` after the failed state is projected and persisted.
/// 失败状态完成投影与持久化后返回 `Ok(())`。
pub(crate) async fn mark_webui_prompt_preparation_failed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    failed_turn_id: &str,
    error: &str,
) -> Result<(), String> {
    // boundary_commit atomically closes execution ownership before any external cleanup can fail.
    // boundary_commit 在任何外部清理可能失败前原子关闭执行所有权。
    let boundary_commit =
        commit_prompt_preparation_failure_boundary(state, session_id, failed_turn_id, error)?;
    // terminal_persistence_guard keeps every replacement submission and Continue claim blocked until durable settlement.
    // terminal_persistence_guard 在可靠结算前阻止所有替代提交与 Continue 领取。
    let terminal_persistence_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        boundary_commit.terminal_persistence_owner,
    )?;
    if let Some(phase_event) = boundary_commit.phase_event {
        emit_service_event_with_trace(
            app,
            "workbench_phase_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(phase_event)),
        );
    }
    // diagnostics retains degraded external cleanup and projection results without reopening execution.
    // diagnostics 保留降级的外部清理与投影结果，同时不重新开放执行。
    let mut diagnostics = boundary_commit.diagnostics;
    if let Err(message_projection_error) =
        emit_prompt_preparation_failure_message_window_delta(app, state, session_id)
    {
        diagnostics.push(format!(
            "Prompt preparation failure message delta failed: {message_projection_error}"
        ));
    }
    // cleanup_updated_at provides one stable command timestamp for the idempotent VMM abandonment.
    // cleanup_updated_at 为幂等 VMM 放弃命令提供一个稳定时间戳。
    let cleanup_updated_at = current_unix_millis_text();
    if let Err(vmm_error) =
        abandon_failed_webui_prompt_vmm_turn(state, session_id, failed_turn_id, &cleanup_updated_at)
    {
        diagnostics.push(format!(
            "Prompt preparation VMM cleanup failed: {vmm_error}"
        ));
    }
    match agent_runtime.lock_conversation_hooks() {
        Ok(mut hook_runtime) => {
            hook_runtime.terminate_turn(session_id, boundary_commit.owning_project_id, error, 0);
        }
        Err(hook_error) => {
            diagnostics.push(format!(
                "Prompt preparation Hook cleanup failed: {hook_error}"
            ));
        }
    }
    // V1 execution controls and API terminal facts are the only continuation authority; no retry sidecar is rewritten here.
    // V1 执行控制与 API 终态事实是唯一 Continue 权威；此处不再改写重试 sidecar。
    crate::persist_v1_execution_failure_boundary(
        state,
        session_id,
        "prompt_preparation_failed",
        Some(error.to_owned()),
    )
    .await
    .map_err(|persist_error| {
        format!(
            "Prompt preparation terminal persistence failed: {persist_error}; terminal owner retained"
        )
    })?;
    terminal_persistence_guard.release()?;
    // Asynchronous preparation failure has no provider terminal callback to return its task result.
    // 异步准备失败没有供应商终态回调来回传任务结果。
    // Delivery runs after releasing terminal ownership and keeps failed receipts available for retry.
    // 投递在释放终态归属后执行，失败回执仍可重试。
    if let Err(delivery_error) =
        crate::agent_collaboration::reconcile_task_results_for_session(state, session_id).await
    {
        tracing::warn!(session_id = %session_id, %delivery_error, "Preparation failure task result return remains pending");
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics.join("; "))
    }
}

/// Abandons the exact failed prompt turn through the session's sole Core VMM controller.
/// 通过会话唯一的 Core VMM Controller 放弃精确的失败提示词回合。
///
/// # Parameters
/// 参数
/// `state` owns the process-wide session controller registry.
/// `state` 持有进程级会话 Controller Registry。
/// `session_id` selects the exact session controller.
/// `session_id` 选择精确的会话 Controller。
/// `failed_turn_id` is the logical turn whose active ownership must be cleared.
/// `failed_turn_id` 是必须清除活动所有权的逻辑回合。
/// `updated_at` is the shared failure-cleanup timestamp.
/// `updated_at` 是共享的失败清理时间戳。
///
/// # Returns
/// 返回值
/// Success when no VMM turn was registered or the exact active turn is durably abandoned.
/// 未登记 VMM 回合或精确活动回合已持久放弃时返回成功。
fn abandon_failed_webui_prompt_vmm_turn(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    failed_turn_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    if !webui_vmm_runtime_readiness(state)?.service_ready {
        return Ok(());
    }
    // controller is the sole authority for active VMM logical-turn ownership.
    // controller 是 VMM 活动逻辑回合所有权的唯一权威。
    let controller = open_registered_session_vmm_controller(state, session_id, updated_at)?;
    // active_turn_id distinguishes failures before the Provider-boundary command from committed failures.
    // active_turn_id 区分 Provider 边界命令前的失败与已经提交的失败。
    let active_turn_id = controller.snapshot()?.active_turn_id;
    let Some(active_turn_id) = active_turn_id else {
        return Ok(());
    };
    if active_turn_id != failed_turn_id {
        return Err(format!(
            "Failed prompt turn `{failed_turn_id}` does not own Core VMM active turn `{active_turn_id}` for session `{session_id}`."
        ));
    }
    // command_id makes repeated cleanup harmless after the durable abandonment succeeds.
    // command_id 使持久放弃成功后的重复清理不产生副作用。
    let command_id = format!("preparation_failed:{failed_turn_id}");
    controller.execute_command(&command_id, updated_at, |candidate| {
        candidate.abandon_active_turn(failed_turn_id, updated_at)?;
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Verifies configured capabilities never imply startup before the service process is ready.
    /// 验证在服务进程就绪前，已配置能力绝不能反向证明服务已启动。
    #[test]
    fn configured_but_stopped_vmm_is_not_ready_for_any_prompt_path() {
        let readiness = project_webui_vmm_runtime_readiness(false, true, true);

        assert!(!readiness.service_ready);
        assert!(!readiness.precheck_ready);
    }

    /// Verifies PreCheck remains unavailable until both process and capability readiness are true.
    /// 验证只有进程与能力就绪同时为 true 时 PreCheck 才可用。
    #[test]
    fn vmm_precheck_requires_ready_process_and_ready_capability() {
        assert!(!project_webui_vmm_runtime_readiness(true, true, false).precheck_ready);

        let readiness = project_webui_vmm_runtime_readiness(true, true, true);
        assert!(readiness.service_ready);
        assert!(readiness.precheck_ready);
    }

    /// Verifies a live process without configured scope is still treated as not started.
    /// 验证进程虽存活但作用域未完成配置时仍按未启动处理。
    #[test]
    fn vmm_process_without_configured_scope_is_not_service_ready() {
        let readiness = project_webui_vmm_runtime_readiness(true, false, true);

        assert!(!readiness.service_ready);
        assert!(!readiness.precheck_ready);
    }

    /// Verifies a stale ready workbench projection cannot start VMM paths before Agent Runtime exists.
    /// 验证 Agent Runtime 尚不存在时，过期的工作台就绪投影不能启动任何 VMM 链路。
    #[test]
    fn stale_workbench_readiness_cannot_replace_current_service_startup() {
        let session_id = Uuid::new_v4();
        let mut service_state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        service_state.workbench.vmm_memory.runtime.process_ready = true;
        service_state.workbench.vmm_memory.runtime.scope_ready = true;
        service_state.workbench.vmm_memory.runtime.precheck_ready = true;
        let state: SharedServiceBridgeState = Arc::new(Mutex::new(service_state));

        let readiness = webui_vmm_runtime_readiness(&state)
            .expect("readiness should fail closed during startup");

        assert!(!readiness.service_ready);
        assert!(!readiness.precheck_ready);
    }
}
