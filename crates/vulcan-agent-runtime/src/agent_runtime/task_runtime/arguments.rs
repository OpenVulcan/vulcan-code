use super::*;

/// Parses one raw JSON payload into strongly typed child-agent arguments.
/// 将一份原始 JSON 载荷解析为强类型子代理 TaskSession 参数。
pub(super) fn parse_task_tool_arguments(args: Value) -> Result<TaskToolInvocationArgs, String> {
    let mut parsed: TaskToolInvocationArgs = serde_json::from_value(args)
        .map_err(|error| format!("Failed to parse child-agent arguments: {error}"))?;
    parsed.agent_mode = normalize_subagent_mode(parsed.agent_mode.as_deref())?;
    if parsed.agent_mode.as_deref() == Some(SUBAGENT_MODE_EXPERT)
        && parsed.subagent_profile_id.is_none()
    {
        return Err(String::from(
            "agent_mode `expert` requires subagent_profile_id.",
        ));
    }
    Ok(parsed)
}

/// Resolves the stable task identifier and resume flag for the current invocation.
/// 为当前调用解析稳定任务标识与续跑标志。
pub(super) fn resolve_task_session_id(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    args: &TaskToolInvocationArgs,
) -> Result<ResolvedTaskSessionIdentifier, String> {
    let (root_session_id, direct_parent, current_depth) = {
        let sessions = task_runtime.sessions.read().map_err(|_| {
            String::from("The subagent task registry is locked and cannot inspect existing tasks.")
        })?;
        let current_record = sessions.get(&current_session_id);
        let root_session_id = current_record
            .map(|record| record.origin_session_id)
            .unwrap_or(current_session_id);
        let direct_parent = current_record
            .map(|_| ParentSessionRef::TaskSession {
                root_session_id: SessionNodeId::from(root_session_id),
                parent_task_session_id: SessionNodeId::from(current_session_id),
            })
            .unwrap_or(ParentSessionRef::MainSession {
                root_session_id: SessionNodeId::from(root_session_id),
            });
        let current_depth = current_record.map(|record| record.depth).unwrap_or(0);
        (root_session_id, direct_parent, current_depth)
    };

    if let Some(task_id_text) = args.task_id.as_ref() {
        let public_task_id = normalize_public_task_identifier(task_id_text)?;
        let archived_task_ids = archived_subagent_task_ids_for_public_lookup(task_runtime)?;
        let mut matching_records = task_runtime
            .sessions
            .read()
            .map_err(|_| {
                String::from(
                    "The subagent task registry is locked and cannot inspect existing tasks.",
                )
            })?
            .values()
            .filter(|record| {
                record.public_task_id == public_task_id
                    && task_record_belongs_to_direct_parent(record, current_session_id)
                    && !task_record_is_archived_for_public_lookup(record, &archived_task_ids)
            })
            .cloned()
            .collect::<Vec<_>>();
        matching_records.sort_by(|left, right| {
            // Prefer the newest open replacement when a closed public id was recreated.
            // 已关闭公开标识被重建时，优先恢复最新的未关闭替代记录。
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| right.task_id.cmp(&left.task_id))
        });
        let resumed_record = matching_records.into_iter().next();
        if let Some(record) = resumed_record {
            return Ok(ResolvedTaskSessionIdentifier {
                task_id: record.task_id,
                public_task_id,
                is_resumed: true,
                persisted_metadata: None,
            });
        }

        if let Some(metadata) = task_runtime.persisted_task_metadata_for_public_id(
            root_session_id,
            direct_parent.clone(),
            &public_task_id,
        )? && metadata.status != TaskSessionStatus::Archived
        {
            return Ok(ResolvedTaskSessionIdentifier {
                task_id: Uuid::from(metadata.task_session_id),
                public_task_id,
                is_resumed: true,
                persisted_metadata: Some(metadata),
            });
        }

        return Ok(ResolvedTaskSessionIdentifier {
            task_id: Uuid::new_v4(),
            public_task_id,
            is_resumed: false,
            persisted_metadata: None,
        });
    }

    if current_depth + 1 > task_runtime.max_depth() {
        return Err(format!(
            "The current task chain has reached the maximum depth `{}` and cannot create another subtask.",
            task_runtime.max_depth()
        ));
    }

    let generated_task_id = Uuid::new_v4();
    Ok(ResolvedTaskSessionIdentifier {
        task_id: generated_task_id,
        public_task_id: generated_task_id.to_string(),
        is_resumed: false,
        persisted_metadata: None,
    })
}

/// Resolves a fresh subagent task identifier for spawn_agent without resuming completed work.
/// 为 spawn_agent 解析全新的任务会话标识，避免复用已完成任务。
pub(super) fn resolve_new_spawn_task_session_id(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    args: &TaskToolInvocationArgs,
) -> Result<ResolvedTaskSessionIdentifier, String> {
    let (root_session_id, direct_parent, current_depth) = {
        let sessions = task_runtime.sessions.read().map_err(|_| {
            String::from("The subagent task registry is locked and cannot inspect existing tasks.")
        })?;
        let current_record = sessions.get(&current_session_id);
        let root_session_id = current_record
            .map(|record| record.origin_session_id)
            .unwrap_or(current_session_id);
        let direct_parent = current_record
            .map(|_| ParentSessionRef::TaskSession {
                root_session_id: SessionNodeId::from(root_session_id),
                parent_task_session_id: SessionNodeId::from(current_session_id),
            })
            .unwrap_or(ParentSessionRef::MainSession {
                root_session_id: SessionNodeId::from(root_session_id),
            });
        let current_depth = current_record.map(|record| record.depth).unwrap_or(0);
        (root_session_id, direct_parent, current_depth)
    };

    if current_depth + 1 > task_runtime.max_depth() {
        return Err(format!(
            "The current task chain has reached the maximum depth `{}` and cannot create another subtask.",
            task_runtime.max_depth()
        ));
    }

    let task_id = Uuid::new_v4();
    let public_task_id = if let Some(task_id_text) = args.task_id.as_ref() {
        let requested_public_task_id = normalize_public_task_identifier(task_id_text)?;
        reject_existing_spawn_public_task_identifier(
            task_runtime,
            root_session_id,
            direct_parent,
            current_session_id,
            &requested_public_task_id,
        )?
    } else {
        task_id.to_string()
    };

    Ok(ResolvedTaskSessionIdentifier {
        task_id,
        public_task_id,
        is_resumed: false,
        persisted_metadata: None,
    })
}

/// Returns whether a task record belongs to the currently invoking direct parent.
/// 返回一条任务记录是否属于当前发起调用的直接父级。
pub(super) fn task_record_belongs_to_direct_parent(
    record: &TaskSessionRecord,
    current_session_id: SessionId,
) -> bool {
    record
        .parent_task_id
        .map(|parent_task_id| parent_task_id == current_session_id)
        .unwrap_or(record.origin_session_id == current_session_id)
}

/// Returns archived task ids known by the native subagent registry.
/// 返回原生子代理注册表已知的已归档任务标识集合。
pub(super) fn archived_subagent_task_ids_for_public_lookup(
    task_runtime: &Arc<TaskRuntime>,
) -> Result<BTreeSet<SessionId>, String> {
    Ok(task_runtime
        .subagent_registry_snapshot()?
        .list_all()
        .into_iter()
        .filter(|entry| entry.status == SubagentStatus::Archived)
        .map(|entry| SessionId::from(entry.session_id))
        .collect())
}

/// Returns whether a task record should be ignored by public-id lookup because it is closed.
/// 返回任务记录是否因已关闭而应被公开标识查找忽略。
pub(super) fn task_record_is_archived_for_public_lookup(
    record: &TaskSessionRecord,
    archived_task_ids: &BTreeSet<SessionId>,
) -> bool {
    record.last_status == status_name_for_task_session_status(TaskSessionStatus::Archived)
        || archived_task_ids.contains(&record.task_id)
}

/// Rejects duplicate public child-agent ids for spawn_agent.
/// 拒绝 spawn_agent 使用重复的公开子代理标识。
pub(super) fn reject_existing_spawn_public_task_identifier(
    task_runtime: &Arc<TaskRuntime>,
    root_session_id: SessionId,
    direct_parent: ParentSessionRef,
    current_session_id: SessionId,
    requested_public_task_id: &str,
) -> Result<String, String> {
    if public_task_identifier_exists_for_parent(
        task_runtime,
        root_session_id,
        direct_parent.clone(),
        current_session_id,
        requested_public_task_id,
    )? {
        return Err(format!(
            "Child agent `{requested_public_task_id}` already exists for this parent. Use send_message for an existing open agent or close_agent before reusing the name."
        ));
    }

    Ok(requested_public_task_id.to_string())
}

/// Checks whether a public child-agent id already belongs to the direct parent.
/// 检查公开子代理标识是否已经属于当前直接父会话。
pub(super) fn public_task_identifier_exists_for_parent(
    task_runtime: &Arc<TaskRuntime>,
    root_session_id: SessionId,
    direct_parent: ParentSessionRef,
    current_session_id: SessionId,
    public_task_id: &str,
) -> Result<bool, String> {
    let archived_task_ids = archived_subagent_task_ids_for_public_lookup(task_runtime)?;
    let exists_in_memory = task_runtime
        .sessions
        .read()
        .map_err(|_| {
            String::from("The subagent task registry is locked and cannot inspect existing tasks.")
        })?
        .values()
        .any(|record| {
            record.public_task_id == public_task_id
                && task_record_belongs_to_direct_parent(record, current_session_id)
                && !task_record_is_archived_for_public_lookup(record, &archived_task_ids)
        });
    if exists_in_memory {
        return Ok(true);
    }

    Ok(task_runtime
        .persisted_task_metadata_for_public_id(root_session_id, direct_parent, public_task_id)?
        .is_some_and(|metadata| metadata.status != TaskSessionStatus::Archived))
}

/// Normalizes one caller-facing task identifier while preserving the original textual identity used for future resume lookups.
/// 在保留未来续跑查找所需原始文本身份的前提下，规范化一条调用方面向任务标识。
pub(super) fn normalize_public_task_identifier(task_id_text: &str) -> Result<String, String> {
    let normalized_task_id = task_id_text.trim();
    if normalized_task_id.is_empty() {
        return Err(String::from("`task_id` cannot be empty."));
    }
    if normalized_task_id.contains('/') {
        return Err(String::from(
            "`task_id` must be one path segment and cannot contain `/`.",
        ));
    }
    Ok(normalized_task_id.to_string())
}

/// Trims one optional task-facing label and drops it when only whitespace remains.
/// 裁剪一条可选任务标签，并在仅剩空白时将其丢弃。
pub(super) fn normalize_optional_task_label(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Builds the task invocation config shared with the subagent runtime layer.
/// 构建与 TaskSession 层共享的任务调用配置。
pub(super) fn task_invocation_config_from_args(
    args: &TaskToolInvocationArgs,
) -> TaskInvocationConfig {
    let mut config = TaskInvocationConfig::main(args.prompt.clone());
    config.subagent_mode = args.agent_mode.clone();
    config.subagent_profile_id = args.subagent_profile_id.clone();
    config.inherit_parent_config = args.inherit_parent_config;
    config.system_prompt = normalize_optional_task_label(args.system_prompt.as_deref());
    config.system_prompt_append =
        normalize_optional_task_label(args.system_prompt_append.as_deref());
    config.provider_model = provider_model_snapshot_from_args(args);
    config.tool_exposure = tool_exposure_snapshot_from_args(args);
    config.prompt_skills = prompt_skill_snapshot_from_args(args);
    config.context_compaction =
        context_compaction_request_from_value(args.context_compaction.as_ref());
    config.runtime_policy = runtime_policy_request_from_value(args.runtime_policy.as_ref());
    config.extra = args.extra.clone().unwrap_or(Value::Null);
    config
}

/// Builds the provider/model override snapshot from child-agent arguments.
/// 从子代理 TaskSession 参数构建供应商与模型覆盖快照。
pub(super) fn provider_model_snapshot_from_args(
    args: &TaskToolInvocationArgs,
) -> ProviderModelSnapshot {
    let mut snapshot = ProviderModelSnapshot::default();
    if let Some(provider_model) = args.provider_model.as_ref() {
        apply_provider_model_value(&mut snapshot, provider_model);
    }
    if let Some(provider) = normalize_optional_task_label(args.provider.as_deref()) {
        snapshot.provider_id = Some(provider);
    }
    if let Some(model) = normalize_optional_task_label(args.model.as_deref()) {
        if let Ok((provider_id, model_id)) = split_provider_model_selector(&model) {
            snapshot.provider_id = Some(provider_id);
            snapshot.model_id = Some(model_id);
        } else {
            snapshot.model_id = Some(model);
        }
    }
    if let Some(execution_mode) = normalize_optional_task_label(args.execution_mode.as_deref()) {
        snapshot.execution_mode = Some(execution_mode);
    }
    snapshot
}

/// Applies a flexible provider/model JSON value into a provider/model snapshot.
/// 将灵活的供应商与模型 JSON 值应用到供应商与模型快照。
pub(super) fn apply_provider_model_value(snapshot: &mut ProviderModelSnapshot, value: &Value) {
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            if let Ok((provider_id, model_id)) = split_provider_model_selector(text) {
                snapshot.provider_id = Some(provider_id);
                snapshot.model_id = Some(model_id);
            } else {
                snapshot.model_id = Some(text.to_string());
            }
        }
        Value::Object(map) => {
            snapshot.provider_id = string_field(map, &["provider_id", "provider"])
                .or_else(|| snapshot.provider_id.clone());
            snapshot.model_id =
                string_field(map, &["model_id", "model"]).or_else(|| snapshot.model_id.clone());
            snapshot.execution_mode = string_field(map, &["execution_mode", "mode"])
                .or_else(|| snapshot.execution_mode.clone());
        }
        _ => {}
    }
}

/// Splits one canonical provider/model selector into provider and model identifiers.
/// 将一个规范 provider/model 选择器拆分为供应商与模型标识。
///
/// # Parameters
/// 参数
/// `selector` is the model-visible selector returned by agent_orchestration_info.
/// `selector` 是 agent_orchestration_info 返回给模型使用的选择器。
///
/// # Returns
/// 返回值
/// The provider id and model id pair when the selector is valid.
/// 选择器有效时返回供应商标识与模型标识。
pub(super) fn split_provider_model_selector(selector: &str) -> Result<(String, String), String> {
    let selector = selector.trim();
    let Some((provider_id, model_id)) = selector.split_once('/') else {
        return Err(String::from(
            "model must use a provider/model selector returned by agent_orchestration_info.",
        ));
    };
    let provider_id = normalize_optional_task_label(Some(provider_id)).ok_or_else(|| {
        String::from("model selector must include a non-empty provider id before `/`.")
    })?;
    let model_id = normalize_optional_task_label(Some(model_id)).ok_or_else(|| {
        String::from("model selector must include a non-empty model id after `/`.")
    })?;
    Ok((provider_id, model_id))
}

/// Resolves a provider/model snapshot into a canonical selector.
/// 将供应商与模型快照解析为规范选择器。
///
/// # Parameters
/// 参数
/// `snapshot` is the provider/model snapshot to inspect.
/// `snapshot` 是需要检查的供应商与模型快照。
/// `session_config` carries the active session model fallback.
/// `session_config` 携带当前会话模型回退值。
///
/// # Returns
/// 返回值
/// A provider/model pair when the snapshot selects a concrete model.
/// 快照选择了具体模型时返回供应商与模型标识。
pub(super) fn provider_model_selector_from_snapshot(
    snapshot: &ProviderModelSnapshot,
    _session_config: &ProviderConfigurationFile,
) -> Option<(String, String)> {
    let model_id = snapshot.model_id.as_deref()?.trim();
    if model_id.is_empty() {
        return None;
    }
    if let Ok((provider_id, model_id)) = split_provider_model_selector(model_id) {
        return Some((provider_id, model_id));
    }
    let provider_id = snapshot
        .provider_id
        .as_deref()
        .and_then(|value| normalize_optional_task_label(Some(value)))?;
    Some((provider_id, model_id.to_string()))
}

/// Returns whether a provider/model pair is currently selectable for execution.
/// 返回供应商与模型组合当前是否可被执行选择。
///
/// # Parameters
/// 参数
/// `session_config` is the current provider configuration snapshot.
/// `session_config` 是当前供应商配置快照。
/// `provider_id` is the provider identifier.
/// `provider_id` 是供应商标识。
/// `model_id` is the model identifier.
/// `model_id` 是模型标识。
///
/// # Returns
/// 返回值
/// `true` when the model exists in the current effective enabled model choices.
/// 当模型存在于当前有效启用模型列表中时返回 `true`。
pub(super) fn provider_model_selector_is_available(
    foundation: &RuntimeFoundation,
    provider_id: &str,
    model_id: &str,
) -> Result<bool, String> {
    let runtime = foundation
        .public_ai_runtime()
        .ok_or_else(|| String::from("The service has not installed the public AI runtime."))?;
    let configuration = runtime.configuration_snapshot()?;
    runtime.model_is_available(&configuration, provider_id, model_id)
}

/// Validates an explicit model override selected by a spawn_agent call.
/// 校验 spawn_agent 调用显式选择的模型覆盖。
///
/// # Parameters
/// 参数
/// `session_config` is the current provider configuration snapshot.
/// `session_config` 是当前供应商配置快照。
/// `provider_model` is the invocation-level provider/model override.
/// `provider_model` 是调用级供应商与模型覆盖。
///
/// # Returns
/// 返回值
/// `Ok(())` when no explicit model was selected or the selector is available.
/// 未显式选择模型或选择器可用时返回 `Ok(())`。
pub(super) fn validate_explicit_spawn_model_selector(
    session_config: &ProviderConfigurationFile,
    foundation: &RuntimeFoundation,
    provider_model: &ProviderModelSnapshot,
) -> Result<(), String> {
    let Some((provider_id, model_id)) =
        provider_model_selector_from_snapshot(provider_model, session_config)
    else {
        return Ok(());
    };
    if provider_model_selector_is_available(foundation, &provider_id, &model_id)? {
        return Ok(());
    }
    Err(format!(
        "model selector `{provider_id}/{model_id}` is not available. Call agent_orchestration_info and use one of the returned model selectors."
    ))
}

/// Clears unavailable profile-level fixed model overrides so execution falls back to the session model.
/// 清除不可用的配置级固定模型覆盖，使执行回退到会话模型。
///
/// # Parameters
/// 参数
/// `session_config` is the current provider configuration snapshot.
/// `session_config` 是当前供应商配置快照。
/// `resolved_config` is the resolved TaskSession configuration to adjust.
/// `resolved_config` 是需要调整的已解析 TaskSession 配置。
///
/// # Returns
/// 返回值
/// `true` when an unavailable fixed model was removed.
/// 当不可用固定模型被移除时返回 `true`。
pub(super) fn clear_unavailable_profile_model_override(
    session_config: &ProviderConfigurationFile,
    foundation: &RuntimeFoundation,
    resolved_config: &mut TaskSessionResolvedConfig,
) -> Result<bool, String> {
    let Some((provider_id, model_id)) = provider_model_selector_from_snapshot(
        &resolved_config.effective_config.provider_model,
        session_config,
    ) else {
        return Ok(false);
    };
    if foundation.public_ai_runtime().is_none() {
        resolved_config.effective_config.provider_model.provider_id = None;
        resolved_config.effective_config.provider_model.model_id = None;
        return Ok(true);
    }
    if provider_model_selector_is_available(foundation, &provider_id, &model_id)? {
        return Ok(false);
    }
    resolved_config.effective_config.provider_model.provider_id = None;
    resolved_config.effective_config.provider_model.model_id = None;
    Ok(true)
}

/// Builds the tool exposure override snapshot from child-agent TaskSession arguments.
/// 从子代理 TaskSession 参数构建工具可见性覆盖快照。
pub(super) fn tool_exposure_snapshot_from_args(
    args: &TaskToolInvocationArgs,
) -> ToolExposureSnapshot {
    let mut snapshot = ToolExposureSnapshot::default();
    if let Some(tools) = args.tools.as_ref() {
        apply_tool_selector_value(
            tools,
            &mut snapshot.allowed_tools,
            &mut snapshot.disabled_tools,
        );
    }
    if let Some(luaskills) = args.luaskills.as_ref() {
        snapshot.luaskills_overrides = luaskills.clone();
    }
    if let Some(mcp) = args.mcp.as_ref() {
        snapshot.mcp_server_overrides = mcp.clone();
    }
    dedupe_preserving_order(&mut snapshot.allowed_tools);
    dedupe_preserving_order(&mut snapshot.disabled_tools);
    snapshot.policy = capability_policy_from_legacy_lists(
        snapshot.allowed_tools.clone(),
        snapshot.disabled_tools.clone(),
    );
    snapshot
}

/// Builds the prompt-skill override snapshot from child-agent TaskSession arguments.
/// 从子代理 TaskSession 参数构建提示词技能覆盖快照。
pub(super) fn prompt_skill_snapshot_from_args(
    args: &TaskToolInvocationArgs,
) -> PromptSkillSnapshot {
    let mut snapshot = PromptSkillSnapshot::default();
    if let Some(skills) = args.skills.as_ref() {
        apply_tool_selector_value(skills, &mut snapshot.enabled, &mut snapshot.disabled);
    }
    if let Some(prompt_skills) = args.prompt_skills.as_ref() {
        apply_tool_selector_value(prompt_skills, &mut snapshot.enabled, &mut snapshot.disabled);
    }
    dedupe_preserving_order(&mut snapshot.enabled);
    dedupe_preserving_order(&mut snapshot.disabled);
    snapshot.policy =
        capability_policy_from_legacy_lists(snapshot.enabled.clone(), snapshot.disabled.clone());
    snapshot
}

/// Builds the context-compaction override request from a flexible JSON payload.
/// 从灵活 JSON 载荷构建上下文压缩覆盖请求。
pub(super) fn context_compaction_request_from_value(
    value: Option<&Value>,
) -> ContextCompactionPolicyRequest {
    let mut request = ContextCompactionPolicyRequest::default();
    let Some(Value::Object(map)) = value else {
        return request;
    };
    request.context_budget = integer_field(map, &["context_budget", "budget"]);
    request.max_provider_requests =
        integer_field(map, &["max_provider_requests", "max_requests"]).map(|value| value as usize);
    request.auto_compaction_enabled = bool_field(
        map,
        &["auto_compaction_enabled", "auto_compaction", "enabled"],
    );
    request
}

/// Builds the runtime-policy override request from a flexible JSON payload.
/// 从灵活 JSON 载荷构建运行时策略覆盖请求。
pub(super) fn runtime_policy_request_from_value(
    value: Option<&Value>,
) -> ExecutionRuntimePolicyRequest {
    let mut request = ExecutionRuntimePolicyRequest::default();
    let Some(Value::Object(map)) = value else {
        return request;
    };
    request.hooks_enabled = bool_field(map, &["hooksEnabled", "hooks_enabled", "hooks"]);
    request.vmm_enabled = bool_field(map, &["vmmEnabled", "vmm_enabled", "vmm"]);
    request.memory_recall_enabled = bool_field(
        map,
        &[
            "memoryRecallEnabled",
            "memory_recall_enabled",
            "memory_recall",
            "recall",
        ],
    );
    request.memory_writeback_enabled = bool_field(
        map,
        &[
            "memoryWritebackEnabled",
            "memory_writeback_enabled",
            "memory_writeback",
            "writeback",
        ],
    );
    request.database_projection_enabled = bool_field(
        map,
        &[
            "databaseProjectionEnabled",
            "database_projection_enabled",
            "database_projection",
            "db_projection",
        ],
    );
    request
}

/// Applies a flexible selector value into allow and deny lists.
/// 将灵活选择器值应用到允许与禁用列表。
pub(super) fn apply_tool_selector_value(
    value: &Value,
    allowed: &mut Vec<String>,
    disabled: &mut Vec<String>,
) {
    match value {
        Value::String(_) | Value::Array(_) => append_string_values(allowed, value),
        Value::Object(map) => {
            for key in [
                "allow",
                "allowed",
                "enabled",
                "include",
                "tools",
                "native",
                "native_tools",
                "mcp_tools",
                "capabilities",
            ] {
                if let Some(value) = map.get(key) {
                    append_string_values(allowed, value);
                }
            }
            for key in ["deny", "denied", "disabled", "disable", "exclude"] {
                if let Some(value) = map.get(key) {
                    append_string_values(disabled, value);
                }
            }
        }
        _ => {}
    }
}

/// Appends string or string-array values into a target list after trimming empty entries.
/// 将字符串或字符串数组裁剪后追加到目标列表，并跳过空值。
pub(super) fn append_string_values(target: &mut Vec<String>, value: &Value) {
    match value {
        Value::String(text) => {
            if let Some(text) = normalize_optional_task_label(Some(text)) {
                target.push(text);
            }
        }
        Value::Array(items) => {
            for item in items {
                append_string_values(target, item);
            }
        }
        _ => {}
    }
}

/// Reads the first non-empty string field from a JSON object.
/// 从 JSON 对象读取第一个非空字符串字段。
pub(super) fn string_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(Value::as_str)
            .and_then(|value| normalize_optional_task_label(Some(value)))
    })
}

/// Reads the first unsigned integer field from a JSON object.
/// 从 JSON 对象读取第一个无符号整数字段。
pub(super) fn integer_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(Value::as_u64))
}

/// Reads the first boolean field from a JSON object.
/// 从 JSON 对象读取第一个布尔字段。
pub(super) fn bool_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(Value::as_bool))
}

/// Deduplicates one string list while preserving the original order.
/// 在保留原始顺序的同时为字符串列表去重。
pub(super) fn dedupe_preserving_order(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

/// Builds a unified capability policy from legacy allow and deny lists.
/// 从旧版允许与禁止列表构建统一能力策略。
fn capability_policy_from_legacy_lists(
    allowed: Vec<String>,
    disabled: Vec<String>,
) -> CapabilityPolicySnapshot {
    vulcan_session_core::capability_policy_from_selector_lists(allowed, disabled)
}

/// Merges an incoming resume invocation without clearing stored explicit task configuration.
/// 合并续跑传入配置，同时避免清除已存储的显式任务配置。
pub(super) fn merge_task_invocation_config_for_resume(
    mut stored: TaskInvocationConfig,
    incoming: TaskInvocationConfig,
) -> TaskInvocationConfig {
    stored.prompt = incoming.prompt;
    if incoming.inherit_parent_config.is_some() {
        stored.inherit_parent_config = incoming.inherit_parent_config;
    }
    if incoming.system_prompt.is_some() {
        stored.system_prompt = incoming.system_prompt;
    }
    if incoming.system_prompt_append.is_some() {
        stored.system_prompt_append = incoming.system_prompt_append;
    }
    if provider_model_has_override(&incoming.provider_model) {
        stored.provider_model = incoming.provider_model;
    }
    if tool_exposure_has_override(&incoming.tool_exposure) {
        stored.tool_exposure = incoming.tool_exposure;
    }
    if prompt_skills_have_override(&incoming.prompt_skills) {
        stored.prompt_skills = incoming.prompt_skills;
    }
    if context_compaction_has_override(&incoming.context_compaction) {
        stored.context_compaction = incoming.context_compaction;
    }
    if incoming.runtime_policy.has_override() {
        stored.runtime_policy = incoming.runtime_policy;
    }
    if !incoming.extra.is_null() {
        stored.extra = incoming.extra;
    }
    stored
}

/// Returns whether a provider/model snapshot carries any explicit override.
/// 返回供应商与模型快照是否携带显式覆盖。
pub(super) fn provider_model_has_override(snapshot: &ProviderModelSnapshot) -> bool {
    snapshot.provider_id.is_some()
        || snapshot.model_id.is_some()
        || snapshot.execution_mode.is_some()
}

/// Returns whether a tool exposure snapshot carries any explicit override.
/// 返回工具可见性快照是否携带显式覆盖。
pub(super) fn tool_exposure_has_override(snapshot: &ToolExposureSnapshot) -> bool {
    snapshot.policy.mode != CapabilityPolicyMode::All
        || !snapshot.policy.selectors.is_empty()
        || !snapshot.allowed_tools.is_empty()
        || !snapshot.disabled_tools.is_empty()
        || !snapshot.luaskills_overrides.is_null()
        || !snapshot.mcp_server_overrides.is_null()
}

/// Returns whether a prompt-skill snapshot carries any explicit override.
/// 返回提示词技能快照是否携带显式覆盖。
pub(super) fn prompt_skills_have_override(snapshot: &PromptSkillSnapshot) -> bool {
    snapshot.policy.mode != CapabilityPolicyMode::All
        || !snapshot.policy.selectors.is_empty()
        || !snapshot.enabled.is_empty()
        || !snapshot.disabled.is_empty()
}

/// Returns whether a context-compaction request carries any explicit override.
/// 返回上下文压缩请求是否携带显式覆盖。
pub(super) fn context_compaction_has_override(request: &ContextCompactionPolicyRequest) -> bool {
    request.context_budget.is_some()
        || request.max_provider_requests.is_some()
        || request.auto_compaction_enabled.is_some()
}

/// Returns whether an incoming invocation explicitly changes controller-owned runtime shape.
/// 返回传入调用是否显式变更控制器拥有的运行时形态。
pub(super) fn task_invocation_changes_controller_config(config: &TaskInvocationConfig) -> bool {
    config.inherit_parent_config.is_some()
        || config.subagent_mode.is_some()
        || config.system_prompt.is_some()
        || config.system_prompt_append.is_some()
        || provider_model_has_override(&config.provider_model)
        || config.runtime_policy.has_override()
        || !config.extra.is_null()
}

/// Normalizes and validates one model-visible subagent creation mode.
/// 规范化并校验一个模型可见的子代理创建模式。
pub(super) fn normalize_subagent_creation_mode(value: Option<&str>) -> Result<String, String> {
    let Some(mode) = normalize_optional_task_label(value) else {
        return Ok(String::from("default"));
    };
    match mode.as_str() {
        "default" | "mirror" | "agent" | "custom" => Ok(mode),
        other => Err(format!(
            "creation_mode must be `default`, `mirror`, `agent`, or `custom`; received `{other}`."
        )),
    }
}

/// Builds one compact prompt preview that can serve as a last-resort task title fallback.
/// 构建一条紧凑的提示词预览文本，作为任务标题的最后兜底来源。
pub(super) fn build_task_prompt_preview(prompt: &str) -> Option<String> {
    let compact = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(prompt)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let compact = compact.trim();
    if compact.is_empty() {
        return None;
    }
    const MAX_PREVIEW_CHARS: usize = 48;
    let preview = compact.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
    if compact.chars().count() > MAX_PREVIEW_CHARS {
        return Some(format!("{preview}..."));
    }
    Some(preview)
}

/// Builds one stable human-readable task title from the best available task metadata.
/// 根据当前可用的最佳任务元数据构建一条稳定的人类可读任务标题。
pub(super) fn build_task_display_title(
    description: Option<&str>,
    subagent: Option<&str>,
    public_task_id: &str,
    prompt: &str,
    task_id: SessionId,
) -> String {
    let description = normalize_optional_task_label(description);
    let subagent = normalize_optional_task_label(subagent);
    match (description.as_deref(), subagent.as_deref()) {
        (Some(description), Some(subagent)) if description != subagent => {
            return format!("{description} ({subagent})");
        }
        (Some(description), _) => return description.to_string(),
        (_, Some(subagent)) => return subagent.to_string(),
        _ => {}
    }

    let normalized_public_task_id = public_task_id.trim();
    if !normalized_public_task_id.is_empty()
        && SessionId::parse_str(normalized_public_task_id).is_err()
    {
        return normalized_public_task_id.to_string();
    }
    if let Some(prompt_preview) = build_task_prompt_preview(prompt) {
        return prompt_preview;
    }
    if !normalized_public_task_id.is_empty() {
        return normalized_public_task_id.to_string();
    }
    format!("Subagent {}", task_id)
}

/// Normalizes and validates one optional subagent mode key.
/// 规范化并校验一个可选子代理模式键。
pub(super) fn normalize_subagent_mode(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(mode) = normalize_optional_task_label(value) else {
        return Ok(None);
    };
    match mode.as_str() {
        SUBAGENT_MODE_DEFAULT | SUBAGENT_MODE_MIRROR | SUBAGENT_MODE_EXPERT => Ok(Some(mode)),
        other => Err(format!(
            "agent_mode must be `default`, `mirror`, or `expert`; received `{other}`."
        )),
    }
}
