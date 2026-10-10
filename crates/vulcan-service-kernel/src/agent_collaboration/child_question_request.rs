//! Dedicated request construction for embedded auxiliary execution.
//! 嵌入辅助执行的专用请求构建。

use super::*;
use vulcan_agent_runtime::agent_runtime::execution_models::ExecutionTurnStartMode;
use vulcan_agent_runtime::conversation_protocol::SessionConversationState;
use vulcan_agent_runtime::{ExecutionPreparationRequest, ExecutionPromptPurpose};
use vulcan_provider_config::config::{ProjectVulcanCodeConfigurationFile, ReasoningEffortLevel};
use vulcan_session_core::v1::child_execution::ChildExecutionProfile;

/// Builds a child QA request from the canonical document and currently available host ports.
/// 从规范文档及当前可用宿主端口构建子答疑请求。
/// `invocation` supplies services, `target` owns history, and `session_id` selects the admitted child.
/// `invocation` 提供服务，`target` 持有历史，`session_id` 选择已准入子会话。
/// Returns a request without root-session profile/recall side effects, or an explicit preparation error.
/// 返回没有根会话画像与召回副作用的请求，或明确准备错误。
pub(super) async fn prepare_child_question_request(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    session_id: Uuid,
    target: &ResolvedSession,
) -> Result<ExecutionPreparationRequest, String> {
    prepare_child_execution_request(app, invocation, session_id, target, None).await
}

/// Prepares a captured business request using the same current host ports as child QA.
/// 使用与子答疑相同的当前宿主端口准备已捕获业务请求。
/// The exact execution must still own a durable open child turn; no new input is submitted.
/// 精确执行必须仍持有可靠开放子回合；不提交新输入。
pub(super) async fn prepare_child_business_request(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    session_id: Uuid,
    target: &ResolvedSession,
    execution_id: &CollaborationExecutionId,
) -> Result<ExecutionPreparationRequest, String> {
    prepare_child_execution_request(app, invocation, session_id, target, Some(execution_id)).await
}

/// Constructs child-owned settings and current host ports for an explicitly selected execution purpose.
/// 为明确选择的执行用途构建子自有设置与当前宿主端口。
/// None selects auxiliary execution; a business identity is validated against full durable history.
/// None 选择辅助执行；业务身份按照完整可靠历史校验。
async fn prepare_child_execution_request(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    session_id: Uuid,
    target: &ResolvedSession,
    business_execution: Option<&CollaborationExecutionId>,
) -> Result<ExecutionPreparationRequest, String> {
    // The registered target is the only source of execution settings and title.
    // 已登记目标是执行设置与标题的唯一来源。
    let (profile, generation, title) = target
        .handle
        .read(|document| {
            (
                document.metadata.child_execution_profile.clone(),
                document.segment_generation(),
                document.metadata.title.clone(),
            )
        })
        .map_err(|error| error.to_string())?;
    // A missing profile cannot inherit the main session's executable configuration.
    // 缺少配置记录时不能继承主会话可执行配置。
    let profile = profile.ok_or_else(|| {
        String::from("Child question execution has no captured business profile.")
    })?;
    // Capture the full persisted projection, including sealed segments and the newly admitted question.
    // 捕获完整持久投影，包括封存分段及新准入问题。
    let snapshot = target
        .handle
        .capture_controller_projection_snapshot()
        .map_err(|error| error.to_string())?;
    // Projection keeps native tool/media identities and does not synthesize a new user prompt.
    // 投影保留原生工具与媒体身份，不伪造新的用户提示词。
    let conversation_state =
        crate::agent_stream::provider_v1_sink::provider_projection_controller_state(&snapshot)?;
    // Missing project files use declared defaults; malformed or inaccessible files remain errors.
    // 缺失项目文件使用声明默认值；格式错误或不可访问文件仍返回错误。
    let project_configuration = vulcan_provider_config::config::load_project_configuration_file(
        &target.locator.project_directory(),
    )?;
    // Pure construction restores only the captured child settings before host ports are attached.
    // 纯构建在附加宿主端口前仅恢复捕获的子设置。
    let mut request = match business_execution {
        None => build_child_question_request(
            session_id,
            title,
            &target.locator,
            &profile,
            generation,
            conversation_state,
            project_configuration,
        )?,
        Some(execution_id) => {
            let mut request = build_child_base_request(
                session_id,
                title,
                &target.locator,
                &profile,
                generation,
                conversation_state,
                project_configuration,
                ExecutionPromptPurpose::AgentTask,
            )?;
            target.handle.read_full_durable(|document| {
                vulcan_agent_runtime::agent_runtime::task_runtime::restore_child_business_request(
                    &mut request, document, execution_id,
                )
            }).map_err(|error| error.to_string())??;
            request
        }
    };
    request.controller_runtime = Some(invocation.controller_runtime.clone());
    request.media_resource_runtime = Some(
        crate::session_media_resources_v1::session_v1_media_resource_runtime(&invocation.state)?,
    );
    request.native_agent_collaboration_runtime = Some(
        session_v1_native_agent_collaboration_runtime(invocation.state.clone(), app.clone()),
    );
    request.native_browser_runtime =
        crate::browser::native_browser_runtime(invocation.state.clone())?;
    request.vmm_runtime_ready =
        crate::webui_prompt_lifecycle::webui_vmm_runtime_readiness(&invocation.state)?
            .service_ready;
    // VCE remains owned by its current service API; this caller only binds the project-authorized port.
    // VCE 仍归当前服务 API 持有；此调用方只绑定项目已授权端口。
    if crate::vce::vce_project_tool_enabled(&invocation.state, target.locator.project_id()) {
        // Release the state lock before constructing the callback or discovering browser bindings.
        // 在构建回调或发现浏览器绑定之前释放状态锁。
        let manager = invocation
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .vce_runtime
            .clone();
        request.vce_retrieval_handler = manager
            .map(|manager| crate::vce::vce_retrieval_handler(invocation.state.clone(), manager));
    }
    if request.vce_retrieval_handler.is_none() {
        request
            .tool_request_hidden_tool_ids
            .push(vulcan_context_engine::CODEBASE_RETRIEVAL_TOOL_ID.to_owned());
    }
    request.supplemental_mcp_tools =
        crate::browser::prepare_browser_mcp_bindings(invocation.state.clone(), session_id).await?;
    Ok(request)
}

#[cfg(test)]
mod tests;

/// Constructs an auxiliary request from exact storage, history, profile and current project configuration.
/// 按精确存储、历史、配置记录与当前项目配置构建辅助请求。
/// `generation` validates profile ownership; the result contains no parent template or business input.
/// `generation` 校验配置归属；返回结果不包含父请求模板或业务输入。
fn build_child_question_request(
    session_id: Uuid,
    title: String,
    locator: &SessionV1StorageLocator,
    profile: &ChildExecutionProfile,
    generation: u64,
    conversation_state: SessionConversationState,
    project_configuration: ProjectVulcanCodeConfigurationFile,
) -> Result<ExecutionPreparationRequest, String> {
    let mut request = build_child_base_request(
        session_id,
        title,
        locator,
        profile,
        generation,
        conversation_state,
        project_configuration,
        ExecutionPromptPurpose::QuestionExecution,
    )?;
    vulcan_agent_runtime::agent_runtime::task_runtime::restore_child_question_request(
        &mut request,
        profile,
        generation,
    )?;
    Ok(request)
}

/// Builds explicit transient fields before purpose-specific source validation and restoration.
/// 在按用途校验来源及恢复之前构建明确瞬态字段。
/// Storage identity and profile generation must match; no caller template or credential is copied.
/// 存储身份与配置代际必须匹配；不复制调用方模板或凭据。
#[allow(clippy::too_many_arguments)]
fn build_child_base_request(
    session_id: Uuid,
    title: String,
    locator: &SessionV1StorageLocator,
    profile: &ChildExecutionProfile,
    generation: u64,
    conversation_state: SessionConversationState,
    project_configuration: ProjectVulcanCodeConfigurationFile,
    purpose: ExecutionPromptPurpose,
) -> Result<ExecutionPreparationRequest, String> {
    if locator.session_id() != session_id.to_string() {
        return Err(String::from(
            "Child question locator belongs to a different session.",
        ));
    }
    profile
        .validate_owner(&session_id.to_string(), generation)
        .map_err(|error| error.to_string())?;
    // Parse the durable root instead of copying whichever session happens to be selected in the UI.
    // 解析可靠根身份，不复制界面恰好选中的会话。
    let root_id = Uuid::parse_str(&profile.root_session_id).map_err(|error| error.to_string())?;
    // Every transient field is explicit; child restoration supplies the captured executable settings.
    // 每个瞬态字段均显式声明；子恢复逻辑提供捕获的可执行设置。
    let request = ExecutionPreparationRequest {
        session_id,
        vce_retrieval_handler: None,
        execution_scope: vulcan_session_core::ExecutionScope::TaskSession {
            root_session_id: root_id.into(),
            task_session_id: session_id.into(),
        },
        runtime_policy: profile.runtime_policy,
        conversation_hook_turn_id: None,
        continuation_action_id: None,
        provider_request_continuation: None,
        execution_agent_id: None,
        agent_prompt_snapshot: None,
        dedicated_session: None,
        provider_id_override: None,
        model_id_override: None,
        connection_id_override: None,
        provider_route_lease_identity: None,
        provider_tool_selections_override: None,
        provider_tool_media_understanding_selections_override: None,
        provider_tool_enabled_capabilities_override: None,
        provider_extension_target_sets_override: None,
        subagent_prompt_context: None,
        session_storage_directory: Some(locator.session_directory()),
        session_title: title,
        workspace_path: locator
            .project_directory()
            .to_str()
            .ok_or_else(|| String::from("Child workspace path is not valid UTF-8."))?
            .to_owned(),
        project_id: Some(locator.project_id().to_owned()),
        prompt: String::new(),
        output_style_snapshot: None,
        provider_visible_agent_messages: Vec::new(),
        turn_start_mode: Some(ExecutionTurnStartMode::RestoredCheckpoint),
        conversation_state,
        context_compaction_policy: None,
        context_compaction_window_node_id: None,
        context_budget_override: None,
        context_window_prefill_input_tokens: None,
        execution_mode: String::new(),
        temperature: None,
        reasoning_effort: ReasoningEffortLevel::Off,
        tool_exposure_override: Default::default(),
        captured_child_tool_admission: None,
        tool_request_rejection_reason: None,
        tool_request_rejection_all_tools: false,
        tool_request_rejection_tool_ids: Vec::new(),
        tool_request_hidden_tool_ids: Vec::new(),
        prompt_purpose: purpose,
        prompt_skill_policy_override: Default::default(),
        project_prompt_skills: project_configuration.prompt_skills,
        session_prompt_skill_overrides: Default::default(),
        context_injection: None,
        system_prompt_hook_blocks: Vec::new(),
        project_vmm: project_configuration.vmm,
        session_vmm: Default::default(),
        vmm_runtime_ready: false,
        boundary_events: Vec::new(),
        runtime_system_updates: Vec::new(),
        controller_runtime: None,
        native_goal_runtime: None,
        native_browser_runtime: None,
        native_agent_collaboration_runtime: None,
        supplemental_mcp_tools: Vec::new(),
        media_resource_runtime: None,
        knowledge_access: None,
        session_goal_context: None,
        session_plan_context: None,
        embedded_turn_usage_segments: Vec::new(),
    };
    Ok(request)
}
