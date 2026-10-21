use super::*;
use vulcan_session_core::v1::child_execution::{ChildToolOrigin, ChildToolSourceKind};

/// Captures callable registrations with source identity instead of trusting reusable public names.
/// 捕获可调用注册及来源身份，不信任可能被复用的公开名称。
/// `descriptors` is the final prepared set; rejection inputs match the stream executor's contract.
/// `descriptors` 是最终准备集合；拒绝参数遵循流执行器契约。
/// Returns public IDs mapped to immutable origins, excluding request-rejected tools.
/// 返回公开 ID 到不可变来源的映射，排除请求拒绝的工具。
pub(super) fn capture_child_admitted_tools(
    descriptors: &[VulcanToolDescriptor],
    rejection_reason: Option<&str>,
    rejection_all: bool,
    rejection_ids: &[String],
) -> BTreeMap<String, ChildToolOrigin> {
    descriptors
        .iter()
        .filter(|descriptor| {
            rejection_reason.is_none()
                || (!rejection_all && !rejection_ids.contains(&descriptor.id))
        })
        .map(|descriptor| {
            // The source registry owns this identity before collision suffixes are allocated.
            // 来源注册表在分配冲突后缀之前持有此身份。
            let origin = descriptor.origin();
            let source_kind = match origin.source_kind() {
                VulcanToolSourceKind::Native => ChildToolSourceKind::Native,
                VulcanToolSourceKind::VulcanManaged => ChildToolSourceKind::VulcanManaged,
                VulcanToolSourceKind::VulcanPlugin => ChildToolSourceKind::VulcanPlugin,
                VulcanToolSourceKind::LuaSkills => ChildToolSourceKind::LuaSkills,
                VulcanToolSourceKind::Mcp => ChildToolSourceKind::Mcp,
                VulcanToolSourceKind::AiHostBinding => ChildToolSourceKind::AiHostBinding,
            };
            (
                descriptor.id.clone(),
                ChildToolOrigin {
                    source_kind,
                    namespace: origin.namespace().to_owned(),
                    original_name: origin.original_name().to_owned(),
                },
            )
        })
        .collect()
}

/// Message returned when a child agent invokes a session-level or parent-reserved tool.
/// 子智能体调用会话级或父级保留工具时返回的提示信息。
pub(super) const SUBAGENT_REJECTED_TOOL_REASON: &str = "The current execution is running in child-agent mode. This tool is reserved for the parent session and cannot be used here. Use available collaboration tools for a parent question or progress notification, and return the task result in the final response.";

/// Freezes resolved policy plus inherited settings from the actual constructed child request.
/// 从真实构造完成的子请求冻结已解析策略及继承设置。
/// `resolved` owns task policy; `request` owns normalized routing and merged skill switches.
/// `resolved` 持有任务策略；`request` 持有规范化路由和合并后的技能开关。
/// Returns a recoverable configuration or an explicit error if the request has no exact model.
/// 返回可恢复配置；请求没有精确模型时返回明确错误。
pub(super) fn capture_child_effective_config(
    resolved: &SessionEffectiveConfig,
    request: &ExecutionPreparationRequest,
) -> Result<SessionEffectiveConfig, String> {
    // Route fields must come from the normalized request, including qualified model selectors.
    // 路由字段必须来自规范化请求，包括带供应商限定的模型选择器。
    let provider_id = request
        .provider_id_override
        .as_ref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| String::from("Child profile capture requires an exact provider."))?;
    let model_id = request
        .model_id_override
        .as_ref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| String::from("Child profile capture requires an exact model."))?;
    // Keep the enum's declared wire value; Display is a translated UI label.
    // 保留枚举声明的传输值；Display 是翻译后的界面文案。
    let reasoning_value = serde_json::to_value(request.reasoning_effort)
        .map_err(|error| format!("Failed to encode child reasoning preference: {error}"))?;
    let reasoning = serde_json::from_value::<String>(reasoning_value)
        .map_err(|error| format!("Invalid child reasoning preference contract: {error}"))?;
    // Task-specific policy remains intact while inherited request values become explicit.
    // 保留任务专属策略，同时显式记录继承的请求值。
    let mut captured = resolved.clone();
    captured.provider_model = ProviderModelSnapshot {
        provider_id: Some(provider_id.clone()),
        model_id: Some(model_id.clone()),
        reasoning_effort: Some(reasoning),
        execution_mode: Some(request.execution_mode.clone()),
    };
    captured.tool_exposure = request.tool_exposure_override.clone();
    captured.prompt_skills.policy = request.prompt_skill_policy_override.clone();
    captured.prompt_skills.enabled.clear();
    captured.prompt_skills.disabled.clear();
    for (skill, state) in &request.session_prompt_skill_overrides {
        match state {
            PersistedPromptSkillState::Enabled => {
                captured.prompt_skills.enabled.push(skill.clone())
            }
            PersistedPromptSkillState::Disabled => {
                captured.prompt_skills.disabled.push(skill.clone())
            }
        }
    }
    Ok(captured)
}

/// Builds one task-session execution request and provider configuration from a shared parent runtime context.
/// 从共享父级运行时上下文构建一份子代理会话执行请求与供应商配置。
///
/// # Parameters
/// 参数
/// `context` provides the shared runtime foundation and parent request template.
/// `context` 提供共享运行时基础与父请求模板。
/// `invocation` provides the isolated task identity, prompt, and canonical snapshot.
/// `invocation` 提供隔离任务身份、提示词与规范快照。
/// `resolved_config` provides the immutable effective task configuration.
/// `resolved_config` 提供不可变的任务有效配置。
///
/// # Returns
/// 返回值
/// Returns the task provider configuration and validated request, or the exact snapshot conversion failure.
/// 返回任务 Provider 配置与经过校验的请求，或返回准确的快照转换失败。
pub(super) fn build_task_runtime_preparation_request(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<(ProviderConfigurationFile, ExecutionPreparationRequest), String> {
    build_task_runtime_preparation_with_history(
        context,
        invocation,
        resolved_config,
        core_snapshot_to_session_conversation_state(&invocation.snapshot)?,
    )
}

/// Builds only child route and prompt controls when its durable host owns summary history.
/// 当可靠宿主持有摘要历史时，仅构建子路由及提示控制。
/// The original context, invocation and configuration retain identity; no resident history is read or substituted.
/// 原上下文、调用与配置保留身份；不读取或替代驻留历史。
/// Returns preparation controls whose history is supplied later by the host's actual durable plan.
/// 返回准备控制，其历史随后由宿主真实可靠规划提供。
pub(super) fn build_task_runtime_compaction_controls(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<(ProviderConfigurationFile, ExecutionPreparationRequest), String> {
    build_task_runtime_preparation_with_history(
        context,
        invocation,
        resolved_config,
        vulcan_context::SessionConversationState::default(),
    )
}

/// Applies original child configuration to explicitly selected history and returns a validated preparation.
/// 将原子配置应用到明确选定的历史，并返回经过校验的准备。
/// `conversation_state` is resident execution history or an empty control-only carrier for durable host planning.
/// `conversation_state` 是驻留执行历史，或供可靠宿主规划使用的无历史控制载体。
fn build_task_runtime_preparation_with_history(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
    conversation_state: vulcan_context::SessionConversationState,
) -> Result<(ProviderConfigurationFile, ExecutionPreparationRequest), String> {
    if let Some(profile) = &context.recovered_child_profile {
        if profile.task_id.as_str() != invocation.task_id.to_string()
            || profile.business_execution_id != invocation.execution_id
            || profile.invocation_config != invocation.invocation_config
            || &profile.resolved_config != resolved_config
        {
            return Err(String::from(
                "Recovered child preparation no longer matches its original execution.",
            ));
        }
        // Re-preparation and compaction retain the child's exact route and tool ceiling.
        // 重新准备与压缩保留子代理精确路由和工具上限。
        let mut request = context.request_template.clone();
        // Current host authorization is already intersected into this captured request.
        // 当前宿主授权已经与捕获请求求交，不能再次用原访问上限覆盖它。
        request.prompt.clear();
        request.conversation_state = conversation_state;
        request.turn_start_mode = Some(
            crate::agent_runtime::execution_models::ExecutionTurnStartMode::RestoredCheckpoint,
        );
        return Ok((context.config.clone(), request));
    }
    let mut config = context.config.clone();
    apply_task_provider_model_override(
        &mut config,
        &resolved_config.effective_config.provider_model,
    );
    let mut request = context.request_template.clone();
    request.session_id = invocation.task_id;
    if request
        .knowledge_access
        .as_ref()
        .is_none_or(|access| access.library_ids.is_empty())
    {
        request.controller_runtime = None;
    }
    request.execution_scope = ExecutionScope::TaskSession {
        root_session_id: SessionNodeId::from(invocation.root_session_id),
        task_session_id: SessionNodeId::from(invocation.task_id),
    };
    request.runtime_policy = resolved_config.runtime_policy;
    request.execution_agent_id = context.request_template.execution_agent_id.clone();
    request.agent_prompt_snapshot =
        task_record_agent_prompt_snapshot(&context.foundation.task_runtime(), invocation.task_id)?
            .or_else(|| context.agent_prompt_snapshot.clone());
    let public_model_override = provider_model_selector_from_snapshot(
        &resolved_config.effective_config.provider_model,
        &context.config,
    );
    if let Some((provider_id, model_id)) = public_model_override {
        // The parent level may be unsupported by another model; keep it only where the target accepts it.
        // 父级档位可能不被其他模型支持；仅在目标模型接受时保留。
        let inherits_parent_model = request.provider_id_override.as_deref()
            == Some(provider_id.as_str())
            && request.model_id_override.as_deref() == Some(model_id.as_str());
        if !inherits_parent_model {
            request.reasoning_effort = resolve_child_model_reasoning_effort(
                &context.foundation,
                &provider_id,
                &model_id,
                request.reasoning_effort,
            )?;
        }
        request.provider_id_override = Some(provider_id);
        request.model_id_override = Some(model_id);
        // A child provider/model pair cannot reuse the parent's credential connection.
        // 子任务更换供应商/模型后不得继续复用父任务凭据连接。
        request.connection_id_override = None;
    }
    request.subagent_prompt_context =
        build_task_subagent_prompt_context(invocation, resolved_config);
    // Child persistence is owned by the registered V1 SessionHandle, never by a legacy task directory.
    // 子会话持久化由已注册的 V1 SessionHandle 拥有，绝不再使用旧任务目录。
    request.session_storage_directory = None;
    request.session_title = invocation.display_title.clone();
    request.prompt = invocation.prompt.clone();
    request.conversation_state = conversation_state;
    request.context_compaction_policy = Some(task_context_compaction_policy_from_snapshot(
        &resolved_config.effective_config.context_compaction,
    ));
    request.context_budget_override = task_context_budget_override_from_snapshot(
        &resolved_config.effective_config.context_compaction,
    );
    request.tool_exposure_override = resolved_config.effective_config.tool_exposure.clone();
    clear_parent_request_runtime_state_for_task_session(&mut request);
    let mirror_subagent =
        resolved_config.effective_config.subagent_mode.as_deref() == Some(SUBAGENT_MODE_MIRROR);
    apply_child_tool_policy(&mut request, mirror_subagent);
    if let Some(execution_mode) = resolved_config
        .effective_config
        .provider_model
        .execution_mode
        .as_deref()
        .and_then(|value| normalize_optional_task_label(Some(value)))
    {
        request.execution_mode = execution_mode;
    }
    if let Some(temperature) = resolved_config
        .effective_config
        .extra
        .get("temperature")
        .and_then(Value::as_f64)
    {
        request.temperature = Some(temperature.clamp(0.0, 2.0) as f32);
    }
    apply_task_prompt_skill_snapshot(
        &mut request,
        &resolved_config.effective_config.prompt_skills,
    );
    Ok((config, request))
}

/// Clears parent-turn runtime state that must not leak into an isolated TaskSession request.
/// 清理不得泄漏进隔离 TaskSession 请求的父回合运行态。
///
/// # Parameters
/// 参数
/// `request` is the cloned parent request being rewritten into a child-agent request.
/// `request` 是正在改写为子智能体请求的父级请求克隆。
fn clear_parent_request_runtime_state_for_task_session(request: &mut ExecutionPreparationRequest) {
    request.conversation_hook_turn_id = None;
    // A manual Continue and a restart continuation bind only the parent turn; a child that inherited
    // them would claim a Continue its own history never requested.
    // 手动 Continue 与重启续接只绑定父回合；子代理若继承它们，会声明其自身历史从未请求过的 Continue。
    request.continuation_action_id = None;
    request.provider_request_continuation = None;
    // A newly delegated child captures its own tools; a restored parent's ceiling belongs only to that parent.
    // 新委派子代理捕获自己的工具；恢复父代理的权限上限仅属于该父代理。
    request.captured_child_tool_admission = None;
    // A TaskSession is an independent lease owner. The parent's persisted identity is scoped to
    // another session id, even when the child inherits the same provider and model.
    // TaskSession 是独立租约所有者。即使子任务继承相同供应商与模型，父级持久化身份也只
    // 作用于另一个会话 ID。
    request.provider_route_lease_identity = None;
    request.tool_request_rejection_reason = None;
    request.tool_request_rejection_all_tools = false;
    request.tool_request_rejection_tool_ids.clear();
    request.tool_request_hidden_tool_ids.clear();
    request.system_prompt_hook_blocks.clear();
    request.context_injection = None;
    // A task session owns a new compaction window and cannot reuse the parent prefill baseline.
    // 子任务会话拥有新的压缩窗口，不能复用父会话前缀基线。
    request.context_window_prefill_input_tokens = None;
    request.boundary_events.clear();
    request.runtime_system_updates.clear();
    request.session_goal_context = None;
    request.session_plan_context = None;
    request.embedded_turn_usage_segments.clear();
}

/// Converts the resolved TaskSession context budget into a request-level input budget override.
/// 将已解析的 TaskSession 上下文预算转换为请求级输入预算覆盖。
///
/// # Parameters
/// 参数
/// `snapshot` is the resolved TaskSession context-compaction policy snapshot.
/// `snapshot` 是已解析的 TaskSession 上下文压缩策略快照。
///
/// # Returns
/// 返回值
/// A non-zero input-token budget when the resolved snapshot defines one.
/// 当解析后的快照定义了非零预算时返回输入 Token 预算。
pub(super) fn task_context_budget_override_from_snapshot(
    snapshot: &ContextCompactionPolicySnapshot,
) -> Option<usize> {
    snapshot
        .context_budget
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
}

/// Reads the fixed agent prompt snapshot attached to a TaskSession record.
/// 读取附着在 TaskSession 记录上的固定代理提示词快照。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the in-memory task registry.
/// `task_runtime` 持有内存任务注册表。
/// `task_id` is the stable TaskSession identifier being launched.
/// `task_id` 是正在启动的稳定 TaskSession 标识。
///
/// # Returns
/// 返回值
/// The stored fixed prompt snapshot, a legitimate absence, or an explicit registry error.
/// 返回已保存的固定提示词快照、合法缺失状态或明确的注册表错误。
fn task_record_agent_prompt_snapshot(
    task_runtime: &TaskRuntime,
    task_id: SessionId,
) -> Result<Option<ExecutionAgentPromptSnapshot>, String> {
    // sessions is the authoritative task-record registry guard for this lookup.
    // sessions 是本次查询所使用的权威任务记录注册表守卫。
    let sessions = task_runtime.sessions.read().map_err(|_| {
        format!(
            "The task-session registry is locked and cannot read the fixed agent prompt snapshot for task {task_id}."
        )
    })?;
    Ok(sessions
        .get(&task_id)
        .and_then(|record| record.agent_prompt_snapshot.clone()))
}

/// Builds the structured prompt context for one child-agent execution.
/// 为一次子智能体执行构建结构化提示词上下文。
///
/// # Parameters
/// 参数
/// `invocation` is the prepared child-agent execution request.
/// `invocation` 是已准备好的子智能体执行请求。
/// `resolved_config` is the effective child-agent session configuration.
/// `resolved_config` 是生效后的子智能体会话配置。
///
/// # Returns
/// 返回值
/// A prompt context consumed by the shared runtime prompt composer.
/// 供共享运行时提示词组合器消费的提示词上下文。
fn build_task_subagent_prompt_context(
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Option<ExecutionSubagentPromptContext> {
    let mirror_subagent =
        resolved_config.effective_config.subagent_mode.as_deref() == Some(SUBAGENT_MODE_MIRROR);
    let identity = invocation
        .subagent
        .as_deref()
        .or(invocation.description.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("subagent")
        .to_string();
    let delegated_task = invocation
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Continue the delegated task.")
        .to_string();
    Some(ExecutionSubagentPromptContext {
        identity,
        delegated_task,
        delegation: Some(child_agent_delegation_prompt(mirror_subagent)),
        functional_system_prompt: invocation.controller_config.as_ref().and_then(|base| {
            task_functional_system_prompt(base, &invocation.invocation_config, resolved_config)
        }),
    })
}

/// Applies the hard child-agent tool policy to one execution request.
/// 对单个执行请求应用智能体硬禁用工具策略。
///
/// # Parameters
/// 参数
/// `request` is the child execution request being prepared.
/// `request` 是正在准备的智能体执行请求。
/// `preserve_parent_tool_shape` indicates clone or mirror runtime parity.
/// `preserve_parent_tool_shape` 表示克隆或镜像运行时需要保持父级工具形状。
fn apply_child_tool_policy(
    request: &mut ExecutionPreparationRequest,
    preserve_parent_tool_shape: bool,
) {
    let forbidden_tool_ids = vulcan_session_core::child_agent_forbidden_tool_ids();
    if preserve_parent_tool_shape {
        reject_registered_child_tools(request, forbidden_tool_ids);
    } else {
        hide_child_tools(request, forbidden_tool_ids);
    }
}

/// Builds the hidden child-agent delegation guidance used by the shared prompt composer.
/// 构建共享提示词组合器使用的隐式智能体委托说明。
///
/// # Parameters
/// 参数
/// `preserve_parent_tool_shape` indicates whether forbidden tools remain registered for runtime parity.
/// `preserve_parent_tool_shape` 表示禁用工具是否因运行时对齐而保留注册。
///
/// # Returns
/// 返回值
/// A model-facing child-agent instruction string.
/// 面向模型的智能体说明文本。
fn child_agent_delegation_prompt(preserve_parent_tool_shape: bool) -> String {
    let policy_sentence = if preserve_parent_tool_shape {
        "The following parent-session tools may remain registered for context parity but are unavailable in child-agent mode and will return a policy rejection if called:"
    } else {
        "The following parent-session tools are unavailable in child-agent mode and should not be called:"
    };
    format!(
        "Run as a child agent for the delegated task. Return the final result to the parent agent; do not assume you are the root conversation. The host task_start envelope identifies the real parent session in source and your session in target. Use ask_parent for a temporary question, send_notification with that parent session ID for progress that needs no reply, and reply_question to answer a received question. Sending returns immediately; unanswered questions and notifications do not block task completion. Call a wait tool only when you choose to wait. A final task result is separate from a question answer and needs no acknowledgement. {policy_sentence} {}.",
        child_forbidden_tool_prompt_list()
    )
}

/// Returns the stable prompt-facing forbidden child-agent tool list.
/// 返回面向提示词的稳定智能体禁用工具列表。
///
/// # Returns
/// 返回值
/// A comma-separated list of child-agent forbidden tool ids.
/// 逗号分隔的智能体禁用工具标识列表。
fn child_forbidden_tool_prompt_list() -> String {
    vulcan_session_core::child_agent_forbidden_tool_ids().join(", ")
}

/// Adds child-forbidden tool ids to the request hidden list without changing global policies.
/// 将智能体禁用工具加入请求隐藏列表，同时不改变全局工具策略。
///
/// # Parameters
/// 参数
/// `request` is the mutable child execution preparation request.
/// `request` 是可变的智能体执行准备请求。
/// `tool_ids` are public tool ids that should not be registered for the child turn.
/// `tool_ids` 是本次智能体回合不应注册的公开工具标识。
fn hide_child_tools(request: &mut ExecutionPreparationRequest, tool_ids: &[&str]) {
    for tool_id in tool_ids {
        if !request
            .tool_request_hidden_tool_ids
            .iter()
            .any(|existing| existing == tool_id)
        {
            request
                .tool_request_hidden_tool_ids
                .push((*tool_id).to_string());
        }
    }
}

/// Adds child-forbidden tool ids to the request rejection list without hiding their schemas.
/// 将子智能体禁止执行的工具加入请求拒绝列表，同时不隐藏它们的 Schema。
///
/// # Parameters
/// 参数
/// `request` is the mutable child execution preparation request.
/// `request` 是可变的子智能体执行准备请求。
/// `tool_ids` are model-visible tool ids that should fail when invoked by the child.
/// `tool_ids` 是对子智能体可见但调用时应失败的工具标识。
fn reject_registered_child_tools(request: &mut ExecutionPreparationRequest, tool_ids: &[&str]) {
    request.tool_request_rejection_reason = Some(SUBAGENT_REJECTED_TOOL_REASON.to_string());
    request.tool_request_rejection_all_tools = false;
    for tool_id in tool_ids {
        if !request
            .tool_request_rejection_tool_ids
            .iter()
            .any(|existing| existing == tool_id)
        {
            request
                .tool_request_rejection_tool_ids
                .push((*tool_id).to_string());
        }
    }
}

/// Applies one TaskSession provider/model snapshot to a mutable provider configuration clone.
/// 将一份 TaskSession 供应商与模型快照应用到可变供应商配置副本上。
pub(super) fn apply_task_provider_model_override(
    _config: &mut ProviderConfigurationFile,
    _provider_model: &ProviderModelSnapshot,
) {
}

/// Applies TaskSession prompt-skill overrides to the child execution request.
/// 将 TaskSession 提示词技能覆盖应用到子代理执行请求。
pub(super) fn apply_task_prompt_skill_snapshot(
    request: &mut ExecutionPreparationRequest,
    prompt_skills: &PromptSkillSnapshot,
) {
    // Preserve the complete policy so allow-only agents cannot inherit unrelated global skills.
    // 保留完整策略，避免仅允许模式的智能体继承无关全局技能。
    request.prompt_skill_policy_override = prompt_skills.policy.clone();
    for skill in &prompt_skills.enabled {
        request
            .session_prompt_skill_overrides
            .insert(skill.clone(), PersistedPromptSkillState::Enabled);
    }
    for skill in &prompt_skills.disabled {
        request
            .session_prompt_skill_overrides
            .insert(skill.clone(), PersistedPromptSkillState::Disabled);
    }
}

/// One resolved child request and tool snapshot, retained unchanged until document admission.
/// 一份已解析子请求及工具快照，原样保留到文档准入。
pub(super) struct PreparedTaskRuntimeExecution {
    /// Serializable settings available before the child document has been published.
    /// 子文档发布之前即可使用的可序列化设置。
    pub(super) creation: Option<collaboration_lifecycle::ChildExecutionProfileCaptureRequest>,
    /// Exact provider configuration that produced the prepared request.
    /// 生成已准备请求的精确供应商配置。
    config: ProviderConfigurationFile,
    /// Runtime request retaining executable host ports outside the serialized creation source.
    /// 在可序列化创建来源之外保留可执行宿主端口的运行时请求。
    request: ExecutionPreparationRequest,
    /// Tool registrations and provider selection resolved once for this admission.
    /// 为本次准入一次性解析的工具注册及供应商选择。
    prepared: crate::agent_runtime::execution_preparation::PreparedExecution,
}

/// Resolves child settings without requiring a canonical child file or performing host capture.
/// 解析子设置，不要求规范子文件存在，也不执行宿主捕获。
/// Inputs are the actual runtime context and invocation; returns one heap-owned preparation.
/// 输入为真实运行时上下文及调用；返回一份由堆持有的准备结果。
pub(super) fn prepare_task_runtime_execution(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<Box<PreparedTaskRuntimeExecution>, String> {
    let (config, request) =
        build_task_runtime_preparation_request(context, invocation, resolved_config)?;
    // Resolve the final tool set before capture; bootstrap and provider execution still wait for persistence.
    // 捕获前解析最终工具集合；启动及 Provider 执行仍等待持久化完成。
    let prepared = prepare_execution_with_tool_snapshot(
        &context.foundation,
        context.luaskills_runtime.clone(),
        &config,
        &request,
    )?;
    // Recovery already owns a profile; ordinary host-backed preparation exposes its creation source.
    // 恢复已经持有配置；普通宿主准备暴露自身创建来源。
    let creation = if context.recovered_child_profile.is_none()
        && request.native_agent_collaboration_runtime.is_some()
    {
        let prompt = request.subagent_prompt_context.as_ref().ok_or_else(|| {
            String::from("Child execution preparation is missing its prompt context.")
        })?;
        Some(
            collaboration_lifecycle::ChildExecutionProfileCaptureRequest {
                invocation_config: invocation.invocation_config.clone(),
                resolved_config: resolved_config.clone(),
                runtime_identity:
                    vulcan_session_core::v1::child_execution::ChildExecutionRuntimeIdentity {
                        public_task_id: invocation.public_task_id.clone(),
                        display_title: invocation.display_title.clone(),
                        description: invocation.description.clone(),
                        subagent: invocation.subagent.clone(),
                        depth: u32::try_from(invocation.depth).map_err(|_| {
                            String::from(
                                "Child admission depth exceeds the durable identity range.",
                            )
                        })?,
                    },
                task_id: invocation.task_id,
                execution_id: invocation.execution_id.clone(),
                root_session_id: invocation.root_session_id,
                parent_session_id: invocation
                    .parent_task_id
                    .unwrap_or(invocation.root_session_id),
                effective_config: capture_child_effective_config(
                    &resolved_config.effective_config,
                    &request,
                )?,
                runtime_policy: request.runtime_policy,
                execution_agent_id: Some(prepared.preparation.agent_id.clone()),
                connection_id: prepared.preparation.model_blueprint.connection_id.clone(),
                temperature: prepared.preparation.temperature,
                request_overrides: capture_child_request_overrides(&request)?,
                admitted_tools: capture_child_admitted_tools(
                    &prepared.preparation.tool_descriptors,
                    prepared
                        .preparation
                        .tool_request_rejection_reason
                        .as_deref(),
                    prepared.preparation.tool_request_rejection_all_tools,
                    &prepared.preparation.tool_request_rejection_tool_ids,
                ),
                fixed_agent_prompt: request
                    .agent_prompt_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.text.clone()),
                prompt_context:
                    vulcan_session_core::v1::child_execution::ChildExecutionPromptContext {
                        identity: prompt.identity.clone(),
                        delegated_task: prompt.delegated_task.clone(),
                        delegation: prompt.delegation.clone(),
                        functional_system_prompt: prompt.functional_system_prompt.clone(),
                    },
            },
        )
    } else {
        None
    };
    Ok(Box::new(PreparedTaskRuntimeExecution {
        creation,
        config,
        request,
        prepared,
    }))
}

/// Captures the prepared profile after canonical admission and assembles the same request for dispatch.
/// 规范准入后捕获已准备配置，并将同一请求组装用于分发。
/// The host context supplies runtime owners; returns a launch without executing its provider loop.
/// 宿主上下文提供运行时所有者；返回启动结果，不执行其供应商循环。
pub(super) async fn launch_prepared_task_runtime_execution(
    context: &TaskExecutionRuntimeContext,
    execution: Box<PreparedTaskRuntimeExecution>,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    // Move the complete snapshot together so capture and dispatch cannot resolve different settings.
    // 整体移出完整快照，确保捕获和分发不会解析出不同设置。
    let PreparedTaskRuntimeExecution {
        creation,
        config,
        request,
        prepared,
    } = *execution;
    if let Some(creation) = creation {
        let port = request
            .native_agent_collaboration_runtime
            .as_ref()
            .ok_or_else(|| String::from("Prepared child creation has no profile capture host."))?;
        port.capture_child_execution_profile(creation).await?;
    }
    let bootstrap = bootstrap_runtime_stream(
        &context.foundation,
        context.luaskills_runtime.clone(),
        &request,
        prepared.preparation,
        prepared.tool_snapshot,
    )
    .await?;
    let child_context = TaskExecutionRuntimeContext::new(
        config,
        Arc::clone(&context.foundation),
        context.luaskills_runtime.clone(),
        request.clone(),
    )
    .with_parent_snapshot(bootstrap.core_snapshot.clone());
    build_runtime_stream_launch_payload(&request, bootstrap, Some(child_context))
}

/// Builds one child runtime-stream launch through the same preparation and bootstrap bridge used by main sessions.
/// 通过主会话使用的同一准备与启动桥构建一份子代理运行时流启动载荷。
/// Returns the prepared launch after durable host capture; provider execution remains separate.
/// 可靠宿主捕获后返回已准备启动结果；供应商执行仍独立进行。
pub(crate) async fn prepare_task_runtime_stream_launch(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    let execution = prepare_task_runtime_execution(context, invocation, resolved_config)?;
    Box::pin(launch_prepared_task_runtime_execution(context, execution)).await
}

/// Builds one child runtime-stream launch and performs an in-process preflight compaction when the first preparation exceeds the budget.
/// 构建子代理运行时流启动载荷，并在首次准备超过预算时执行进程内前置压缩。
pub(crate) async fn prepare_task_runtime_stream_launch_with_preflight_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: &TaskExecutionRuntimeContext,
    invocation: &mut TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    let launch = prepare_task_runtime_stream_launch(context, invocation, resolved_config).await?;
    if !resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled
        || !task_launch_should_compact(&launch)
    {
        return Ok(launch);
    }

    if let Some(compacted_snapshot) = run_task_session_context_compaction(
        task_runtime,
        context.clone(),
        invocation.clone(),
        resolved_config.clone(),
        crate::agent_runtime::prompt_compaction::ChildTaskCompactionPhase::ActiveBusiness,
    )
    .await?
    {
        invocation.snapshot = compacted_snapshot;
        return prepare_task_runtime_stream_launch(context, invocation, resolved_config).await;
    }

    Ok(launch)
}

/// Returns whether a prepared child launch should compact its context before provider execution.
/// 返回一份已准备的子代理启动载荷是否应在 Provider 执行前压缩上下文。
pub(super) fn task_launch_should_compact(
    launch: &crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch,
) -> bool {
    launch
        .preparation
        .context_budget
        .as_ref()
        .is_some_and(|context_budget| context_budget.should_compact())
}

/// Converts a terminal runtime report back into the TaskSession outcome shape expected by persistence and parent cards.
/// 将终态运行时报告转换回持久化与父级卡片期望的 TaskSession 结果形态。
pub(super) fn task_execution_outcome_from_report(
    invocation: &TaskExecutionRequest,
    controller_config: CoreControllerConfig,
    resolved_config: TaskSessionResolvedConfig,
    report: ExecutionRunReport,
) -> Result<TaskExecutionOutcome, String> {
    if let Some(error) = report.error {
        return Err(error);
    }
    let tool_calls_processed = report
        .controller_events
        .iter()
        .filter(|event| matches!(event.kind, vulcan_core::types::CoreEventKind::ToolFinished))
        .count();
    let snapshot = match report.conversation_state.as_ref() {
        Some(conversation_state) => build_core_conversation_snapshot(conversation_state)?,
        None => invocation.snapshot.clone(),
    };
    let final_output_text = report
        .assistant_output
        .filter(|output| !output.trim().is_empty())
        .or_else(|| extract_final_assistant_text(&snapshot))
        .unwrap_or_else(|| {
            String::from(
                "The task completed, but it did not produce reusable final assistant text.",
            )
        });
    Ok(TaskExecutionOutcome {
        task_id: invocation.task_id,
        controller_config,
        resolved_config,
        final_assistant_message_id: extract_final_assistant_message_id(&snapshot),
        snapshot,
        provider_turns_executed: report.provider_request_count,
        tool_calls_processed,
        final_output_text,
        execution_status: TaskExecutionStatus::Completed,
        usage_segments: Vec::new(),
    })
}
