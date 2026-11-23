//! Local tool preparation for the request-context inspector.
//! 请求上下文检查面板的本地工具准备。

use crate::state::AgentRuntimeBridge;
use crate::{ServiceEventEmitter, SharedServiceBridgeState};
use std::sync::Arc;
use vulcan_agent_runtime::agent_runtime::{
    DedicatedExecutionProfile, ExecutionKnowledgeAccessContext, ExecutionPreparationRequest,
    execution_preparation::{RequestToolPreview, preview_request_tools},
    runtime::RuntimeFoundation,
};
use vulcan_session_core::{ExecutionRuntimePolicy, ExecutionScope, SessionNodeId};
use vulcan_workbench_model::{SessionId, SessionRuntimeStateEntry};
use vulcan_workspace_store::workspace_store::LoadedSessionV1;

/// Builds the locally available model-visible tool schemas and skill catalog for an indexed session.
/// 为已索引会话构建本地可解析的模型可见工具 Schema 和技能目录。
///
/// The loaded session owns the configuration; host state provides the published runtime
/// generation and local tool bindings. The caller passes the session's authorized library view.
/// 已加载会话持有配置，宿主状态提供已发布的运行代次及本地工具绑定。调用者提供会话的授权知识库视图。
/// Returns the inspection data before provider-specific wire encoding or a preparation error.
/// 返回供应商专用线协议编码之前的检查数据，或准备错误。
pub(crate) fn preview_session_tools(
    loaded: &LoadedSessionV1,
    session_id: SessionId,
    workspace_path: &str,
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &AgentRuntimeBridge,
    knowledge_access: Option<ExecutionKnowledgeAccessContext>,
) -> Result<RequestToolPreview, String> {
    // Resolve session-specific choices from the same durable document as the simulated messages.
    // 从模拟消息所用的同一份可靠会话文档解析会话选择。
    let metadata = &loaded.document.metadata;
    // Reuse the V1 projection that also populates the workbench's session adapter.
    // 复用同时填充工作台会话适配器的 V1 投影。
    let mut session = SessionRuntimeStateEntry::default();
    crate::session_model_projection_v1::apply_request_model_projection_v1(
        &mut session,
        &metadata.config.active_request,
        metadata.config.pending_request.as_ref(),
    );
    crate::session_request_config_v1::apply_provider_tool_selection_v1(
        &mut session,
        &metadata.config.selection.provider_tools,
    );
    // A pending route applies to the next assembled request without changing the saved session.
    // 待生效路由用于下一组装请求，不改变已保存会话。
    let pending_model = session.pending_model_selection.is_some();
    if let Some(pending) = session.pending_model_selection.take() {
        session.active_provider_id = Some(pending.provider_id);
        session.active_connection_id = Some(pending.connection_id);
        session.active_model_id = Some(pending.model_id);
    }

    // Dedicated sessions use the same tool cap and omit collaboration and goal ports.
    // 专职会话使用相同的工具权限上限，并省略协作和目标端口。
    let dedicated = loaded
        .document
        .dedicated_profile()
        .map(|profile| DedicatedExecutionProfile {
            tool_access: profile.tool_access,
            base_prompt: profile.base_prompt.clone(),
        });
    // Child execution restores its captured policy before resolving any tool exposure.
    // 子代理执行在解析工具可见性之前恢复捕获的策略。
    let child = metadata.child_execution_profile.as_ref();
    // Capture the project policy that participates in request-time tool exposure.
    // 捕获参与请求级工具可见性筛选的项目策略。
    let project = crate::webui_prompt_context::load_project_configuration(workspace_path);
    // Read the VMM host readiness without starting its runtime or invoking precheck hooks.
    // 读取 VMM 宿主就绪状态，无需启动其运行时或调用预检 Hook。
    let vmm_runtime_ready = dedicated.is_none()
        && child.is_none()
        && vulcan_vmm::VmmRuntimeFeaturePolicy::from_env().session_state_enabled
        && crate::webui_prompt_lifecycle::webui_vmm_runtime_readiness(state)?.service_ready;
    // Preserve the normal build default and use durable Plan mode when it is active.
    // 保留常规工作模式默认值，计划模式开启时采用其可靠状态。
    let execution_mode = if metadata.mode.plan.enabled {
        "plan"
    } else {
        "build"
    };
    // Read the same session allowlist as execution, without rewriting its sidecar.
    // 读取与执行相同的会话白名单，避免改写旁置文件。
    let subagent_model_policy = if dedicated.is_none() && child.is_none() {
        crate::subagent_models::preview_session_subagent_model_policy(&loaded.locator)?
    } else {
        None
    };
    // The tool preparer consumes the normal execution input and never launches it.
    // 工具准备器消费常规执行输入，且不会启动它。
    let request = ExecutionPreparationRequest {
        session_id,
        execution_scope: ExecutionScope::MainSession {
            session_id: SessionNodeId::from(session_id),
        },
        runtime_policy: ExecutionRuntimePolicy::main_session(),
        conversation_hook_turn_id: None,
        continuation_action_id: None,
        provider_request_continuation: None,
        execution_agent_id: None,
        agent_prompt_snapshot: None,
        provider_id_override: session.active_provider_id,
        model_id_override: session.active_model_id,
        connection_id_override: session.active_connection_id,
        subagent_model_policy,
        provider_route_lease_identity: None,
        provider_tool_selections_override: session.provider_tool_settings_initialized.then(|| {
            session
                .provider_tool_selections
                .into_iter()
                .filter(|(capability, _)| {
                    session
                        .provider_tool_enabled_capabilities
                        .contains(capability)
                })
                .collect()
        }),
        provider_tool_media_understanding_selections_override: session
            .provider_tool_settings_initialized
            .then_some(session.media_understanding_selections),
        provider_tool_enabled_capabilities_override: session
            .provider_tool_settings_initialized
            .then_some(session.provider_tool_enabled_capabilities),
        provider_extension_target_sets_override: session
            .provider_tool_settings_initialized
            .then_some(session.provider_extension_target_sets),
        provider_extension_modes_override: session
            .provider_tool_settings_initialized
            .then_some(session.provider_extension_modes),
        subagent_prompt_context: None,
        session_storage_directory: Some(loaded.locator.session_directory()),
        session_title: metadata.title.clone(),
        workspace_path: workspace_path.to_owned(),
        project_id: Some(loaded.locator.project_id().to_owned()),
        project_shell_guard: None,
        prompt: String::new(),
        output_style_snapshot: None,
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: None,
        conversation_state: Default::default(),
        context_compaction_policy: None,
        context_window_selection: metadata.config.context_window_selection,
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: execution_mode.to_owned(),
        temperature: session.active_temperature,
        session_request_options: metadata.config.selection.request_options.clone(),
        reasoning_effort: session
            .active_reasoning_effort
            .unwrap_or(vulcan_provider_config::config::ReasoningEffortLevel::Off),
        tool_exposure_override: Default::default(),
        captured_child_tool_admission: None,
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
        tool_request_hidden_tool_ids: Vec::new(),
        prompt_purpose: Default::default(),
        prompt_skill_policy_override: Default::default(),
        project_prompt_skills: project.prompt_skills,
        session_prompt_skill_overrides:
            crate::conversation_submission::runtime_prompt_skill_overrides(
                &metadata.config.selection.prompt_skill_overrides,
            ),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: project.vmm,
        session_vmm: vulcan_vmm::VmmConfigLayer {
            precheck_enabled: metadata.config.selection.vmm.precheck_enabled,
            memory_writeback_enabled: metadata.config.selection.vmm.memory_writeback_enabled,
            ..Default::default()
        },
        vmm_runtime_ready,
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        native_goal_runtime: None,
        native_browser_runtime: crate::browser::native_browser_runtime(state.clone())?,
        native_agent_collaboration_runtime: dedicated.is_none().then(|| {
            crate::agent_collaboration::session_v1_native_agent_collaboration_runtime(
                state.clone(),
                app.clone(),
            )
        }),
        dedicated_session: dedicated,
        supplemental_mcp_tools: Vec::new(),
        media_resource_runtime: Some(
            crate::session_media_resources_v1::session_v1_media_resource_runtime(state)?,
        ),
        knowledge_access,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    };
    // Reuse the published extension generation so opening the inspector cannot launch discovery.
    // 复用已发布的扩展代次，避免打开检查面板时启动发现。
    let foundation = RuntimeFoundation::build_from_extension_generation_with_runtimes_and_plugins(
        &runtime.provider_configuration,
        &runtime.agent_profile_catalog,
        runtime.luaskills_runtime.clone(),
        Some(runtime.vulcan_plugin_runtime.clone()),
        Arc::clone(&runtime.extension_generation),
        Arc::clone(&runtime.task_runtime),
        Arc::clone(&runtime.question_runtime),
        Arc::clone(&runtime.command_session_runtime),
    );
    preview_request_tools(
        foundation,
        Arc::clone(&runtime.public_ai_runtime),
        runtime.luaskills_runtime.clone(),
        &runtime.provider_configuration,
        request,
        pending_model && child.is_none(),
        child,
    )
}
