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
        .or_else(|| normalize_optional_task_label(args.subagent_profile_id.as_deref()));
    let mut invocation_config = task_invocation_config_from_args(&args);
    let fork_turns = effective_spawn_agent_fork_turns(
        invocation_config.subagent_mode.as_deref(),
        invocation_config.subagent_profile_id.as_deref(),
        args.fork_turns.as_deref(),
    )?;
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
            let mut initial_snapshot = initial_spawn_snapshot_for_fork_turns(
                task_runtime,
                current_session_id,
                Some(fork_turns.as_str()),
            )?;
            append_mirror_subagent_status_prompt_if_needed(
                &mut initial_snapshot,
                task_id,
                &invocation_config,
            );
            attach_subagent_runtime_debug_extra(
                &mut invocation_config,
                &fork_turns,
                initial_snapshot.messages.len(),
                false,
            );
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
                    snapshot: initial_snapshot.clone(),
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
                initial_snapshot,
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

/// Attaches non-secret runtime debug metadata to the invocation extra payload.
/// 将不含敏感提示词正文的运行时调试元数据附加到调用扩展载荷。
///
/// # Parameters
/// 参数
/// `invocation_config` is the invocation being persisted for this task.
/// `invocation_config` 是即将为该任务持久化的调用配置。
/// `fork_turns` is the effective fork selector used for the initial snapshot.
/// `fork_turns` 是构建初始快照时使用的有效派生选择器。
/// `snapshot_message_count` is the number of messages in the child initial snapshot.
/// `snapshot_message_count` 是子代理初始快照中的消息数量。
/// `is_resumed` indicates whether this metadata was produced for a resumed task.
/// `is_resumed` 表示该元数据是否来自续跑任务。
fn attach_subagent_runtime_debug_extra(
    invocation_config: &mut TaskInvocationConfig,
    fork_turns: &str,
    snapshot_message_count: usize,
    is_resumed: bool,
) {
    let agent_id = invocation_config
        .subagent_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let agent_kind = if agent_id.is_some() {
        "agent_profile"
    } else {
        "parent_clone"
    };
    let prompt_source = agent_id
        .map(|value| format!("agent_md:{value}"))
        .unwrap_or_else(|| String::from("core"));
    let debug = json!({
        "agent_kind": agent_kind,
        "agent_mode": invocation_config.subagent_mode,
        "agent_id": invocation_config.subagent_profile_id,
        "prompt_source": prompt_source,
        "fork_turns": fork_turns,
        "initial_snapshot_messages": snapshot_message_count,
        "inherit_parent_config": invocation_config.inherit_parent_config,
        "is_resumed": is_resumed,
    });
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

/// Appends the hidden mirror-mode user prompt to a newly forked child snapshot.
/// 向新 fork 的子快照追加镜像模式隐藏用户提示词。
///
/// # Parameters
/// 参数
/// `snapshot` is the child snapshot being prepared.
/// `snapshot` 是正在准备的子快照。
/// `task_id` is the child task session identifier.
/// `task_id` 是子任务会话标识。
/// `invocation_config` carries the resolved invocation mode.
/// `invocation_config` 携带已解析的调用模式。
fn append_mirror_subagent_status_prompt_if_needed(
    snapshot: &mut CoreConversationSnapshot,
    task_id: SessionId,
    invocation_config: &TaskInvocationConfig,
) {
    if invocation_config.subagent_mode.as_deref() != Some(SUBAGENT_MODE_MIRROR) {
        return;
    }
    snapshot.messages.push(CoreConversationMessage {
        id: format!("mirror-subagent-status-{task_id}"),
        payload: CoreConversationPayload::UserText {
            text: mirror_subagent_status_prompt(),
            attachments: Vec::new(),
        },
    });
}

/// Returns the hidden user prompt appended to mirror subagents.
/// 返回追加给镜像子代理的隐藏用户提示词。
fn mirror_subagent_status_prompt() -> String {
    String::from(
        "You are now running as a mirror subagent, not the root agent. You inherited the parent conversation context and may see the same tool schemas for cache stability and situational awareness. You may use ordinary available tools when they help complete the delegated task, but you must not use subagent orchestration tools such as spawning, listing, waiting for, interrupting, or messaging agents. If a subagent orchestration tool call is attempted, the host will reject it because the current execution is a mirror subagent. Complete the requested analysis from the inherited context and return your result to the parent agent.",
    )
}

/// Builds the initial child snapshot for one Vulcan spawn fork selector.
/// 根据 Vulcan 的 spawn 派生选择器构建子代理初始快照。
///
/// # Parameters
/// 参数
/// `task_runtime` provides access to the registered parent runtime context.
/// `task_runtime` 提供对已注册父级运行时上下文的访问。
/// `current_session_id` is the parent session that invoked spawn_agent.
/// `current_session_id` 是调用 spawn_agent 的父会话。
/// `fork_turns` is the normalized optional fork selector.
/// `fork_turns` 是已规范化的可选派生选择器。
///
/// # Returns
/// 返回值
/// A snapshot suitable for the child controller launch.
/// 适合子控制器启动的快照。
pub(super) fn initial_spawn_snapshot_for_fork_turns(
    task_runtime: &TaskRuntime,
    current_session_id: SessionId,
    fork_turns: Option<&str>,
) -> Result<CoreConversationSnapshot, String> {
    let fork_turns = normalize_spawn_agent_fork_turns(fork_turns)?;
    let Some(fork_turns) = fork_turns.as_deref() else {
        return Ok(CoreConversationSnapshot::default());
    };
    if fork_turns == "none" {
        return Ok(CoreConversationSnapshot::default());
    }
    let parent_snapshot = parent_snapshot_for_spawn(task_runtime, current_session_id)?;
    if fork_turns == "all" {
        return Ok(snapshot_for_child_agent_fork(parent_snapshot));
    }
    let turn_count = fork_turns.parse::<usize>().map_err(|_| {
        String::from("fork_turns must be `none`, `all`, or a positive integer string")
    })?;
    Ok(truncate_snapshot_to_last_user_turns(
        parent_snapshot,
        turn_count,
    ))
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
    task_runtime
        .execution_context_for_session(current_session_id)
        .map(|context| context.parent_snapshot)
        .ok_or_else(|| {
            format!(
                "spawn_agent fork requires a registered parent runtime context for session `{current_session_id}`."
            )
        })
}

/// Builds a child-agent fork snapshot without provider continuation state.
/// 构建不携带 Provider continuation 状态的子代理 fork 快照。
///
/// # Parameters
/// 参数
/// `snapshot` is the parent snapshot selected for the fork.
/// `snapshot` 是为 fork 选择的父级快照。
///
/// # Returns
/// 返回值
/// A snapshot containing only conversation messages.
/// 仅包含会话消息的快照。
fn snapshot_for_child_agent_fork(snapshot: CoreConversationSnapshot) -> CoreConversationSnapshot {
    CoreConversationSnapshot {
        messages: snapshot.messages,
        records: Vec::new(),
        continuation: Default::default(),
    }
}

/// Truncates a snapshot to the suffix beginning at the last N user-authored turns.
/// 将快照裁剪为从最后 N 个用户回合开始的后缀。
///
/// # Parameters
/// 参数
/// `snapshot` is the parent snapshot to truncate.
/// `snapshot` 是需要裁剪的父级快照。
/// `turn_count` is the positive count of user turns to preserve.
/// `turn_count` 是需要保留的正数用户回合数量。
///
/// # Returns
/// 返回值
/// A truncated snapshot with provider continuation cleared.
/// 清空 Provider continuation 后的裁剪快照。
pub(super) fn truncate_snapshot_to_last_user_turns(
    snapshot: CoreConversationSnapshot,
    turn_count: usize,
) -> CoreConversationSnapshot {
    if turn_count == 0 || snapshot.messages.is_empty() {
        return CoreConversationSnapshot::default();
    }
    let mut user_turns_seen = 0usize;
    let mut start_index = 0usize;
    for (index, message) in snapshot.messages.iter().enumerate().rev() {
        if matches!(message.payload, CoreConversationPayload::UserText { .. }) {
            user_turns_seen += 1;
            if user_turns_seen == turn_count {
                start_index = index;
                break;
            }
        }
    }
    let messages = if user_turns_seen < turn_count {
        snapshot.messages
    } else {
        snapshot.messages[start_index..].to_vec()
    };
    CoreConversationSnapshot {
        messages,
        records: Vec::new(),
        continuation: Default::default(),
    }
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
    let subagent = subagent_override.or_else(|| metadata.description.clone());

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
