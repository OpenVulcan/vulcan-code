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
    let normalized_description = normalize_optional_task_label(args.description.as_deref());
    let normalized_subagent = normalize_optional_task_label(args.subagent.as_deref())
        .or_else(|| normalize_optional_task_label(args.subagent_profile_id.as_deref()))
        .or_else(|| normalize_optional_task_label(args.profile.as_deref()));
    let invocation_config = task_invocation_config_from_args(&args);
    let prompt = args.prompt;
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
                    snapshot: CoreConversationSnapshot::default(),
                    last_prompt: prompt.clone(),
                    last_result_text: String::new(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    last_status: String::from("created"),
                },
            );
            (
                parent_task_id,
                root_session_id,
                depth,
                CoreConversationSnapshot::default(),
                None,
                normalized_description.clone(),
                normalized_subagent.clone(),
                invocation_config.clone(),
                display_title,
            )
        }
    };

    Ok(TaskExecutionRequest {
        current_session_id,
        root_session_id,
        task_id,
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

/// Builds one task-session record with the provided metadata and execution state.
/// 使用给定元数据与执行状态构建一条任务会话记录。
pub(super) fn build_task_session_record(
    task_id: SessionId,
    public_task_id: String,
    origin_session_id: SessionId,
    parent_task_id: Option<SessionId>,
    depth: usize,
    description: Option<String>,
    subagent: Option<String>,
    invocation_config: TaskInvocationConfig,
    display_title: String,
    controller_config: Option<CoreControllerConfig>,
    snapshot: CoreConversationSnapshot,
    last_prompt: String,
    last_status: String,
) -> TaskSessionRecord {
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
        snapshot,
        last_prompt,
        last_result_text: String::new(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        last_status,
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
    let subagent = subagent_override
        .or_else(|| metadata.subagent_profile_id.clone())
        .or_else(|| invocation_config.subagent_profile_id.clone());

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
        snapshot,
        last_prompt: prompt,
        last_result_text: metadata.last_result_summary.clone().unwrap_or_default(),
        created_at: metadata.created_at.to_string(),
        updated_at: metadata.updated_at.to_string(),
        last_status: status_name_for_task_session_status(metadata.status).to_string(),
    })
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
    Ok(build_core_conversation_snapshot(&conversation_state))
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
    let system_prompt_layers = if context.system_prompt_layers.is_empty() {
        effective_config.system_prompt_layers.clone()
    } else {
        context.system_prompt_layers.clone()
    };
    let system_prompt = system_prompt_layers.join("\n\n");
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
        prompt_layers: CoreProviderPromptLayers::from_system_prompt(system_prompt.clone()),
        system_prompt,
        temperature,
        max_turns,
        additional_params: None,
        context_injection: None,
    })
}
