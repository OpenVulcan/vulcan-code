use super::*;

/// Prepares one task execution request from the current invocation and the stored registry state.
/// 根据当前调用与已存储注册表状态准备一份任务执行请求。
pub(super) fn prepare_task_execution_request(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    resolved_identifier: ResolvedTaskSessionIdentifier,
    args: TaskToolInvocationArgs,
) -> Result<TaskExecutionRequest, String> {
    let ResolvedTaskSessionIdentifier {
        task_id,
        public_task_id,
        is_resumed,
        persisted_metadata,
    } = resolved_identifier;
    // execution_id establishes record ownership before any mutable preparation state is committed.
    // execution_id 在提交任何可变准备状态前建立记录所有权。
    let execution_id = Uuid::new_v4().to_string();
    // transaction_locks serialize child admission with both its direct parent tree and its own outcomes.
    // transaction_locks 将子节点准入与其直接父树及自身结果提交串行化。
    let transaction_locks =
        task_runtime.ordered_task_execution_state_locks(&[current_session_id, task_id])?;
    // transaction_guards keep session and registry preparation inside one ordered identity boundary.
    // transaction_guards 将会话与注册表准备保持在同一个有序身份边界内。
    let mut transaction_guards = Vec::with_capacity(transaction_locks.len());
    for transaction_lock in &transaction_locks {
        transaction_guards.push(transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot prepare the execution request.",
            )
        })?);
    }
    let normalized_description = normalize_optional_task_label(args.description.as_deref());
    let normalized_subagent = normalize_optional_task_label(args.subagent.as_deref())
        .or_else(|| normalize_optional_task_label(args.subagent_profile_id.as_deref()));
    let mut invocation_config = task_invocation_config_from_args(&args);
    let prompt = args.prompt;
    // The exact prior task record is the rollback image if registry admission rejects this preparation.
    // 如果注册表准入拒绝本次准备，精确的旧任务记录就是回滚镜像。
    let previous_task_record;
    let (
        parent_task_id,
        root_session_id,
        depth,
        snapshot,
        controller_config,
        description,
        subagent,
        invocation_config,
        display_title,
    ) = {
        let mut sessions = task_runtime.sessions.write().map_err(|_| {
            String::from(
                "The task session registry is locked and cannot prepare the execution request.",
            )
        })?;
        previous_task_record = sessions.get(&task_id).cloned();
        if let Some(record) = sessions.get_mut(&task_id) {
            if is_resumed {
                record.updated_at = current_unix_millis_text();
                if record.description.is_none() {
                    record.description = normalized_description.clone();
                }
                if record.subagent.is_none() {
                    record.subagent = normalized_subagent.clone();
                }
                record.invocation_config = merge_task_invocation_config_for_resume(
                    record.invocation_config.clone(),
                    invocation_config.clone(),
                );
                if task_invocation_changes_controller_config(&invocation_config) {
                    record.controller_config = None;
                }
            }
            record.last_prompt = prompt.clone();
            record.last_status = String::from("created");
            record.execution_id = Some(execution_id.clone());
            record.stop_reason = None;
            let description = record
                .description
                .clone()
                .or_else(|| normalized_description.clone());
            let subagent = record
                .subagent
                .clone()
                .or_else(|| normalized_subagent.clone());
            let display_title = build_task_display_title(
                description.as_deref(),
                subagent.as_deref(),
                &public_task_id,
                &prompt,
                task_id,
            );
            record.display_title = display_title.clone();
            (
                record.parent_task_id,
                record.origin_session_id,
                record.depth,
                record.snapshot.clone(),
                record.controller_config.clone(),
                description,
                subagent,
                record.invocation_config.clone(),
                display_title,
            )
        } else if is_resumed {
            let metadata = persisted_metadata.ok_or_else(|| {
                format!(
                    "Task `{public_task_id}` was marked as resumable but no in-memory or persisted TaskSession record was available."
                )
            })?;
            let context = task_runtime
                .load_persisted_task_context(metadata.root_session_id, metadata.task_session_id)?;
            let mut record = task_session_record_from_persisted_metadata(
                &metadata,
                context.as_ref(),
                prompt.clone(),
                invocation_config.clone(),
                normalized_description.clone(),
                normalized_subagent.clone(),
            )?;
            record.updated_at = current_unix_millis_text();
            if task_invocation_changes_controller_config(&invocation_config) {
                record.controller_config = None;
            }
            let description = record
                .description
                .clone()
                .or_else(|| normalized_description.clone());
            let subagent = record
                .subagent
                .clone()
                .or_else(|| normalized_subagent.clone());
            let display_title = build_task_display_title(
                description.as_deref(),
                subagent.as_deref(),
                &public_task_id,
                &prompt,
                task_id,
            );
            record.display_title = display_title.clone();
            record.description = description.clone();
            record.subagent = subagent.clone();
            record.last_prompt = prompt.clone();
            record.last_status = String::from("created");
            record.execution_id = Some(execution_id.clone());
            record.stop_reason = None;
            let parent_task_id = record.parent_task_id;
            let root_session_id = record.origin_session_id;
            let depth = record.depth;
            let snapshot = record.snapshot.clone();
            let controller_config = record.controller_config.clone();
            let invocation_config = record.invocation_config.clone();
            sessions.insert(task_id, record);
            (
                parent_task_id,
                root_session_id,
                depth,
                snapshot,
                controller_config,
                description,
                subagent,
                invocation_config,
                display_title,
            )
        } else {
            let current_depth = sessions
                .get(&current_session_id)
                .map(|record| record.depth)
                .unwrap_or(0);
            let depth = if current_depth == 0 {
                1
            } else {
                current_depth + 1
            };
            if depth > task_runtime.max_depth() {
                return Err(format!(
                    "The current task chain has reached the maximum depth `{}` and cannot create another subtask.",
                    task_runtime.max_depth()
                ));
            }
            let parent_task_id = sessions
                .get(&current_session_id)
                .map(|record| record.task_id);
            let root_session_id = sessions
                .get(&current_session_id)
                .map(|record| record.origin_session_id)
                .unwrap_or(current_session_id);
            let display_title = build_task_display_title(
                normalized_description.as_deref(),
                normalized_subagent.as_deref(),
                &public_task_id,
                &prompt,
                task_id,
            );
            let timestamp = current_unix_millis_text();
            let initial_context_source = task_initial_context_source(&invocation_config);
            let initial_snapshot = initial_spawn_snapshot_for_context_source(
                task_runtime,
                current_session_id,
                initial_context_source,
            )?;
            let agent_prompt_snapshot =
                parent_agent_prompt_snapshot_for_spawn(task_runtime, current_session_id)?;
            attach_subagent_runtime_debug_extra(&mut invocation_config);
            sessions.insert(
                task_id,
                TaskSessionRecord {
                    task_id,
                    public_task_id: public_task_id.clone(),
                    origin_session_id: root_session_id,
                    parent_task_id,
                    depth,
                    description: normalized_description.clone(),
                    subagent: normalized_subagent.clone(),
                    invocation_config: invocation_config.clone(),
                    display_title: display_title.clone(),
                    controller_config: None,
                    agent_prompt_snapshot: agent_prompt_snapshot.clone(),
                    snapshot: initial_snapshot.clone(),
                    last_prompt: prompt.clone(),
                    last_result_text: String::new(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    last_status: String::from("created"),
                    execution_id: Some(execution_id.clone()),
                    stop_reason: None,
                },
            );
            (
                parent_task_id,
                root_session_id,
                depth,
                initial_snapshot,
                None,
                normalized_description.clone(),
                normalized_subagent.clone(),
                invocation_config.clone(),
                display_title,
            )
        }
    };

    if let Err(error) = task_runtime.upsert_prepared_task_registry_entry(task_id) {
        let mut sessions = task_runtime.sessions.write().map_err(|_| {
            format!(
                "{error} The task session registry is locked and cannot roll back the rejected registry preparation."
            )
        })?;
        if let Some(previous_task_record) = previous_task_record {
            sessions.insert(task_id, previous_task_record);
        } else {
            sessions.remove(&task_id);
        }
        return Err(error);
    }

    Ok(TaskExecutionRequest {
        current_session_id,
        root_session_id,
        task_id,
        execution_id,
        public_task_id,
        prompt,
        description,
        subagent,
        invocation_config,
        display_title,
        parent_task_id,
        depth,
        snapshot,
        controller_config,
        is_resumed,
    })
}

/// Attaches non-secret creation metadata to the invocation extra payload.
/// 将不含敏感提示词正文的创建元数据附加到调用扩展载荷。
///
/// # Parameters
/// 参数
/// `invocation_config` is the invocation being persisted for this task.
/// `invocation_config` 是即将为该任务持久化的调用配置。
fn attach_subagent_runtime_debug_extra(invocation_config: &mut TaskInvocationConfig) {
    let agent_id = invocation_config
        .subagent_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let creation_mode = debug_creation_mode_for_invocation(invocation_config);
    let mut debug = serde_json::Map::new();
    debug.insert(String::from("creation_mode"), json!(creation_mode));
    if let Some(agent_id) = agent_id {
        debug.insert(String::from("agent_id"), json!(agent_id));
    }
    if debug_has_custom_system_prompt(invocation_config) {
        debug.insert(String::from("has_custom_system_prompt"), json!(true));
    }
    let debug = Value::Object(debug);
    match &mut invocation_config.extra {
        Value::Object(map) => {
            map.insert(String::from("_subagent_runtime_debug"), debug);
        }
        Value::Null => {
            invocation_config.extra = json!({
                "_subagent_runtime_debug": debug,
            });
        }
        previous => {
            let caller_extra = previous.clone();
            invocation_config.extra = json!({
                "_subagent_runtime_debug": debug,
                "_caller_extra": caller_extra,
            });
        }
    }
}

/// Resolves the model-facing creation mode for debug surfaces without changing runtime behavior.
/// 在不改变运行行为的前提下，为调试表面解析模型侧创建模式。
fn debug_creation_mode_for_invocation(invocation_config: &TaskInvocationConfig) -> &'static str {
    let runtime_agent_mode = debug_runtime_agent_mode_for_invocation(invocation_config);
    if invocation_config
        .subagent_profile_id
        .as_deref()
        .is_some_and(|agent_id| !agent_id.trim().is_empty())
        || runtime_agent_mode == SUBAGENT_MODE_EXPERT
    {
        return "agent";
    }
    if runtime_agent_mode == SUBAGENT_MODE_MIRROR {
        return "mirror";
    }
    if debug_has_custom_system_prompt(invocation_config) {
        return "custom";
    }
    "default"
}

/// Resolves the internal runtime mode used only to derive the public creation mode.
/// 解析仅用于推导公开创建模式的内部运行模式。
fn debug_runtime_agent_mode_for_invocation(invocation_config: &TaskInvocationConfig) -> &str {
    invocation_config
        .subagent_mode
        .as_deref()
        .map(str::trim)
        .filter(|mode| !mode.is_empty())
        .unwrap_or(SUBAGENT_MODE_DEFAULT)
}

/// Returns whether this invocation carries a replacement custom system prompt.
/// 返回本次调用是否携带替换型自定义系统提示词。
fn debug_has_custom_system_prompt(invocation_config: &TaskInvocationConfig) -> bool {
    invocation_config
        .system_prompt
        .as_deref()
        .is_some_and(|system_prompt| !system_prompt.trim().is_empty())
}

/// Resolves the initial runtime context source from one invocation configuration.
/// 从单次调用配置解析初始运行时上下文来源。
///
/// # Parameters
/// 参数
/// `invocation_config` carries the child-agent creation mode.
/// `invocation_config` 携带子智能体创建模式。
///
/// # Returns
/// 返回值
/// The explicit context source used by the new child runtime.
/// 新子智能体运行时使用的显式上下文来源。
fn task_initial_context_source(
    invocation_config: &TaskInvocationConfig,
) -> TaskInitialContextSource {
    if invocation_config.subagent_mode.as_deref() == Some(SUBAGENT_MODE_MIRROR) {
        return TaskInitialContextSource::CloneLatestRuntime;
    }
    TaskInitialContextSource::Fresh
}

/// Builds the initial child snapshot for one explicit context source.
/// 根据显式上下文来源构建子智能体初始快照。
///
/// # Parameters
/// 参数
/// `task_runtime` provides access to the registered parent runtime context.
/// `task_runtime` 提供对已注册父级运行时上下文的访问。
/// `current_session_id` is the parent session that invoked spawn_agent.
/// `current_session_id` 是调用 spawn_agent 的父会话。
/// `context_source` selects whether the child starts fresh or clones the latest runtime state.
/// `context_source` 选择子智能体是全新启动还是克隆最新运行时状态。
///
/// # Returns
/// 返回值
/// A snapshot suitable for the child controller launch.
/// 适合子控制器启动的快照。
pub(super) fn initial_spawn_snapshot_for_context_source(
    task_runtime: &TaskRuntime,
    current_session_id: SessionId,
    context_source: TaskInitialContextSource,
) -> Result<CoreConversationSnapshot, String> {
    match context_source {
        TaskInitialContextSource::Fresh => Ok(CoreConversationSnapshot::default()),
        TaskInitialContextSource::CloneLatestRuntime => {
            parent_snapshot_for_spawn(task_runtime, current_session_id)
        }
    }
}

/// Reads the parent snapshot registered for the active runtime context.
/// 读取当前运行时上下文注册的父级快照。
///
/// # Parameters
/// 参数
/// `task_runtime` owns execution contexts keyed by session id.
/// `task_runtime` 持有按会话标识索引的执行上下文。
/// `current_session_id` is the session whose context should be inspected.
/// `current_session_id` 是需要检查上下文的会话。
///
/// # Returns
/// 返回值
/// The parent snapshot, or an explicit error when the context is unavailable.
/// 父级快照；若上下文不可用则返回显式错误。
fn parent_snapshot_for_spawn(
    task_runtime: &TaskRuntime,
    current_session_id: SessionId,
) -> Result<CoreConversationSnapshot, String> {
    // parent_context is the exact registered context whose snapshot belongs to this fork.
    // parent_context 是本次 fork 所属快照对应的精确已注册上下文。
    let parent_context = task_runtime.execution_context_for_session(current_session_id)?;
    parent_context
        .map(|context| context.parent_snapshot)
        .ok_or_else(|| {
            format!(
                "spawn_agent fork requires a registered parent runtime context for session `{current_session_id}`."
            )
        })
}

/// Reads the fixed parent agent prompt snapshot captured for the active runtime context.
/// 读取当前运行时上下文捕获的固定父级代理提示词快照。
///
/// # Parameters
/// 参数
/// `task_runtime` owns execution contexts keyed by session id.
/// `task_runtime` 持有按会话标识索引的执行上下文。
/// `current_session_id` is the parent session whose fixed prompt should be reused.
/// `current_session_id` 是需要复用固定提示词的父级会话。
///
/// # Returns
/// 返回值
/// The optional fixed agent prompt snapshot, or an explicit execution-context registry error.
/// 返回可选固定代理提示词快照，或明确的执行上下文注册表错误。
fn parent_agent_prompt_snapshot_for_spawn(
    task_runtime: &TaskRuntime,
    current_session_id: SessionId,
) -> Result<Option<ExecutionAgentPromptSnapshot>, String> {
    Ok(task_runtime
        .execution_context_for_session(current_session_id)?
        .and_then(|context| context.agent_prompt_snapshot))
}

/// Initialization fields for creating one fresh task-session record.
/// 用于创建一条全新任务会话记录的初始化字段。
pub(super) struct TaskSessionRecordInit {
    /// Stable internal task-session identifier.
    /// 稳定的内部任务会话标识。
    pub(super) task_id: SessionId,
    /// Caller-facing public task identifier.
    /// 面向调用方的公开任务标识。
    pub(super) public_task_id: String,
    /// Root controller session that owns the task tree.
    /// 拥有任务树的根控制器会话。
    pub(super) origin_session_id: SessionId,
    /// Optional parent task-session identifier for nested tasks.
    /// 嵌套任务的可选父任务会话标识。
    pub(super) parent_task_id: Option<SessionId>,
    /// Task depth inside the task tree.
    /// 任务在任务树中的深度。
    pub(super) depth: usize,
    /// Optional task description supplied by the invocation.
    /// 调用提供的可选任务描述。
    pub(super) description: Option<String>,
    /// Optional selected subagent label.
    /// 可选的已选择子代理标签。
    pub(super) subagent: Option<String>,
    /// Resolved invocation configuration used to run or resume the task.
    /// 用于运行或恢复任务的已解析调用配置。
    pub(super) invocation_config: TaskInvocationConfig,
    /// Display title shown in task surfaces.
    /// 任务界面中展示的标题。
    pub(super) display_title: String,
    /// Optional controller configuration captured for the task.
    /// 为该任务捕获的可选控制器配置。
    pub(super) controller_config: Option<CoreControllerConfig>,
    /// Conversation snapshot attached to the task record.
    /// 附加到任务记录上的会话快照。
    pub(super) snapshot: CoreConversationSnapshot,
    /// Last prompt used to launch or inspect the task.
    /// 用于启动或检查任务的最近提示词。
    pub(super) last_prompt: String,
    /// Last known execution status text.
    /// 最近已知的执行状态文本。
    pub(super) last_status: String,
}

/// Builds one task-session record with the provided metadata and execution state.
/// 使用给定元数据与执行状态构建一条任务会话记录。
///
/// # Parameters
/// 参数
/// `init` contains every identity, ownership, display, snapshot, and status field for the new record.
/// `init` 包含新记录所需的全部标识、归属、展示、快照与状态字段。
///
/// # Returns
/// 返回值
/// A fresh in-memory task-session record with synchronized created and updated timestamps.
/// 返回创建与更新时间戳一致的全新内存任务会话记录。
pub(super) fn build_task_session_record(init: TaskSessionRecordInit) -> TaskSessionRecord {
    let TaskSessionRecordInit {
        task_id,
        public_task_id,
        origin_session_id,
        parent_task_id,
        depth,
        description,
        subagent,
        invocation_config,
        display_title,
        controller_config,
        snapshot,
        last_prompt,
        last_status,
    } = init;
    let timestamp = current_unix_millis_text();
    TaskSessionRecord {
        task_id,
        public_task_id,
        origin_session_id,
        parent_task_id,
        depth,
        description,
        subagent,
        invocation_config,
        display_title,
        controller_config,
        agent_prompt_snapshot: None,
        snapshot,
        last_prompt,
        last_result_text: String::new(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        last_status,
        execution_id: None,
        stop_reason: None,
    }
}

/// Rebuilds one in-memory task record from persisted TaskSession metadata and current context.
/// 从持久化 TaskSession 元数据与当前上下文重建一条内存任务记录。
pub(super) fn task_session_record_from_persisted_metadata(
    metadata: &TaskSessionMetadata,
    context: Option<&SessionContextDocument>,
    prompt: String,
    incoming_invocation: TaskInvocationConfig,
    description_override: Option<String>,
    subagent_override: Option<String>,
) -> Result<TaskSessionRecord, String> {
    let stored_invocation =
        task_invocation_config_from_requested_config(&metadata.requested_config, prompt.clone());
    let invocation_config =
        merge_task_invocation_config_for_resume(stored_invocation, incoming_invocation);
    let snapshot = context
        .map(task_conversation_snapshot_from_context)
        .transpose()?
        .unwrap_or_default();
    let controller_config = context.and_then(|context| {
        task_controller_config_from_context(context, &metadata.effective_config)
    });
    let parent_task_id = match metadata.parent {
        ParentSessionRef::TaskSession {
            parent_task_session_id,
            ..
        } => Some(Uuid::from(parent_task_session_id)),
        ParentSessionRef::MainSession { .. } | ParentSessionRef::None => None,
    };
    let subagent = subagent_override.or_else(|| task_metadata_subagent_label(metadata));

    Ok(TaskSessionRecord {
        task_id: Uuid::from(metadata.task_session_id),
        public_task_id: metadata.public_task_id.clone(),
        origin_session_id: Uuid::from(metadata.root_session_id),
        parent_task_id,
        depth: metadata.depth as usize,
        description: description_override.or_else(|| metadata.description.clone()),
        subagent,
        invocation_config,
        display_title: metadata.display_title.clone(),
        controller_config,
        agent_prompt_snapshot: metadata
            .agent_prompt_snapshot
            .as_ref()
            .and_then(|text| ExecutionAgentPromptSnapshot::fixed_parent_runtime(text.as_str())),
        snapshot,
        last_prompt: prompt,
        last_result_text: metadata.last_result_summary.clone().unwrap_or_default(),
        created_at: metadata.created_at.to_string(),
        updated_at: metadata.updated_at.to_string(),
        last_status: status_name_for_task_session_status(metadata.status).to_string(),
        execution_id: None,
        stop_reason: metadata.stop_reason.clone(),
    })
}

/// Restores the display subagent label available from persisted task metadata.
/// 从持久化任务元数据恢复可用的展示子代理标签。
///
/// # Parameters
/// 参数
/// `metadata` is the persisted task-session metadata being projected into runtime state.
/// `metadata` 是正被投影到运行时状态中的持久化任务会话元数据。
///
/// # Returns
/// 返回值
/// The normalized profile identifier when the persisted task was backed by a catalog profile.
/// 当持久化任务由目录配置支持时返回归一化后的配置标识。
pub(super) fn task_metadata_subagent_label(metadata: &TaskSessionMetadata) -> Option<String> {
    normalize_optional_task_label(metadata.subagent_profile_id.as_deref())
}

/// Recreates one task invocation config from the persisted requested configuration snapshot.
/// 从已持久化的请求配置快照重建一份任务调用配置。
pub(super) fn task_invocation_config_from_requested_config(
    requested_config: &SessionRequestedConfig,
    prompt: String,
) -> TaskInvocationConfig {
    TaskInvocationConfig {
        prompt,
        subagent_profile_id: requested_config.subagent_profile_id.clone(),
        subagent_mode: requested_config.subagent_mode.clone(),
        inherit_parent_config: Some(requested_config.inherit_parent_config),
        system_prompt: requested_config.system_prompt.clone(),
        system_prompt_append: requested_config.system_prompt_append.clone(),
        provider_model: requested_config.provider_model.clone(),
        tool_exposure: requested_config.tool_exposure.clone(),
        prompt_skills: requested_config.prompt_skills.clone(),
        context_compaction: requested_config.context_compaction.clone(),
        runtime_policy: requested_config.runtime_policy,
        extra: requested_config.extra.clone(),
    }
}

/// Converts a persisted task context document into a core conversation snapshot.
/// 将已持久化的任务上下文文档转换为 core 会话快照。
pub(super) fn task_conversation_snapshot_from_context(
    context: &SessionContextDocument,
) -> Result<CoreConversationSnapshot, String> {
    let conversation_state: SessionConversationState =
        serde_json::from_value(context.conversation_state.clone()).map_err(|error| {
            format!(
                "TaskSession `{}` has an unreadable persisted conversation state: {error}",
                context.node_id
            )
        })?;
    build_core_conversation_snapshot(&conversation_state)
}

/// Rebuilds a minimal controller config from persisted task context and effective config.
/// 从已持久化的任务上下文与有效配置重建最小控制器配置。
pub(super) fn task_controller_config_from_context(
    context: &SessionContextDocument,
    effective_config: &SessionEffectiveConfig,
) -> Option<CoreControllerConfig> {
    let provider_id = context
        .provider_id
        .clone()
        .or_else(|| effective_config.provider_model.provider_id.clone())?;
    let model_id = context
        .model_id
        .clone()
        .or_else(|| effective_config.provider_model.model_id.clone())?;
    let temperature = effective_config
        .extra
        .get("temperature")
        .and_then(Value::as_f64)
        .map(|value| value.clamp(0.0, 2.0));
    let max_turns = effective_config
        .extra
        .get("max_turns")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(usize::MAX - 1);

    Some(CoreControllerConfig {
        provider_id,
        model_id,
        prompt_layers: CoreProviderPromptLayers::default(),
        system_prompt: String::new(),
        temperature,
        max_turns,
        additional_params: None,
        context_injection: None,
    })
}
