//! Session-scoped AI capability configuration handlers.
//! 会话级 AI 能力配置处理器。

use super::*;

/// WebUI 输入区弹窗发送的会话级 AI 能力配置保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SessionAiCapabilityConfigurationSaveRequest {
    /// Owning project id used only as a fallback when no conversation id exists.
    /// 所属项目标识，仅在会话标识不存在时作为回退使用。
    project_id: Option<String>,
    /// Conversation id whose addon AI capability namespace should be updated.
    /// 需要更新附属 AI 能力命名空间的会话标识。
    conversation_id: Option<String>,
    /// Complete disabled capability kind list from the dialog draft.
    /// 弹窗草稿中的完整禁用能力类型列表。
    disabled_ai_capability_kinds: Vec<String>,
    /// Complete disabled target selector list from the dialog draft.
    /// 弹窗草稿中的完整禁用目标选择器列表。
    disabled_ai_capability_target_selectors: Vec<String>,
    /// Complete target order overrides grouped by capability kind.
    /// 按能力类型分组的完整目标顺序覆盖。
    ai_capability_target_order_overrides: BTreeMap<String, Vec<String>>,
}

/// Saves the current session AI capability dialog draft into the session addon document.
/// 将当前会话 AI 能力快捷面板草稿保存到会话附属状态文档。
pub(super) async fn service_save_session_ai_capability_configuration(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionAiCapabilityConfigurationSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    let session_id = request
        .conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .ok_or_else(|| String::from("当前草稿会话尚未创建，暂时不能保存会话级 AI 能力配置。"))?;
    let requested_project_id = request
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let disabled_kinds =
        parse_session_ai_capability_disabled_kinds(request.disabled_ai_capability_kinds)?;
    let disabled_target_selectors =
        parse_ai_capability_target_selectors(Some(request.disabled_ai_capability_target_selectors));
    let target_order_overrides = parse_ai_capability_target_order_overrides(Some(
        request.ai_capability_target_order_overrides,
    ))?;
    let (controller_runtime, workspace_snapshot, provider_configuration, previous_configuration) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let default_ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if !workspace_snapshot_contains_regular_session(workspace_snapshot, session_id) {
            return Err(format!("未找到要保存 AI 能力配置的会话：{session_id}"));
        }
        if let Some(project_id) = requested_project_id
            && owning_project_id_for_session(workspace_snapshot, session_id).as_deref()
                != Some(project_id)
        {
            return Err(String::from(
                "当前会话与请求项目不匹配，已拒绝保存 AI 能力配置。",
            ));
        }
        let previous_configuration = session_ai_tool_configuration_or_default(
            workspace_snapshot,
            session_id,
            &default_ai_tool_configuration,
        )
        .clone();
        (
            controller_runtime,
            workspace_snapshot.clone(),
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| runtime.provider_configuration.clone())
                .unwrap_or_default(),
            previous_configuration,
        )
    };
    let mut next_configuration = previous_configuration.clone();
    apply_session_ai_capability_dialog_draft(
        &mut next_configuration,
        &disabled_kinds,
        &disabled_target_selectors,
        &target_order_overrides,
    )?;
    let model_capabilities = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .and_then(|runtime_state| {
            runtime_state_base_model_capabilities(runtime_state, &provider_configuration)
        })
        .or_else(|| active_provider_configuration_model_capabilities(&provider_configuration));
    let capability_update_injection = session_ai_capability_configuration_update_injection(
        &previous_configuration,
        &next_configuration,
        model_capabilities.as_ref(),
    );
    let updated_at = current_unix_millis_text();
    let mut addon_state = SessionAddonState::new(session_id);
    let _ = addon_state.set_ai_tool_configuration(&next_configuration, updated_at.clone());
    invalidate_webui_session_addon_persist_version(
        &state,
        session_id,
        SessionAddonNamespace::AiCapabilities,
    )?;
    let addon_persist_lock = webui_workspace_persist_lock();
    let _addon_persist_guard = addon_persist_lock.lock().await;
    let persisted_addon_state = persist_workspace_session_addon_state_update(
        controller_runtime,
        workspace_snapshot,
        session_id,
        addon_state,
    )
    .await?;
    invalidate_runtime_prompt_tool_visibility_cache();
    let workspace_snapshot =
        with_workspace_mutation_and_preserved_navigation_full_rebuild_for_cold_path(
            &app,
            &state,
            |workspace| {
                let runtime_state = workspace
                    .session_runtime_state
                    .entry(session_id)
                    .or_insert_with(|| SessionRuntimeStateEntry {
                        updated_at: updated_at.clone(),
                        ..SessionRuntimeStateEntry::default()
                    });
                runtime_state.ai_tool_configuration = Some(next_configuration.clone());
                if let Some(injection) = capability_update_injection.clone() {
                    runtime_state
                        .context_injection_state
                        .apply_insert(injection, &updated_at);
                }
                runtime_state.updated_at = updated_at.clone();
            },
        )?;
    emit_session_addon_changed_event(
        &app,
        session_id,
        &[SessionAddonNamespace::AiCapabilities],
        persisted_addon_state.revision,
        latest_session_addon_updated_at(&persisted_addon_state),
    );
    let _ = workspace_snapshot;
    current_workbench_snapshot_from_state(&state)
}

/// Updates the selected session Plan mode state through the WebUI API.
/// 通过 WebUI API 更新当前会话的计划模式状态。
///
/// # Parameters
/// 参数
/// `app` emits addon-change notifications after persistence.
/// `app` 在持久化后发出附属状态变更通知。
/// `state` owns the retained workbench snapshot.
/// `state` 持有保留的工作台快照。
/// `request` carries the target conversation and desired Plan enablement.
/// `request` 携带目标会话与期望的计划模式启停状态。
///
/// # Returns
/// 返回值
/// Returns a persistent hidden user injection that clears on context compaction when model-visible capability changes exist.
/// 返回一条上下文压缩时清除的持久隐藏用户注入；若无模型可见变化则返回 `None`。
pub(super) fn session_ai_capability_configuration_update_injection(
    previous_configuration: &PersistedAiToolConfiguration,
    next_configuration: &PersistedAiToolConfiguration,
    model_capabilities: Option<&ResolvedModelCapabilities>,
) -> Option<InjectionInsert> {
    let previous_capabilities = ai_capabilities(previous_configuration, model_capabilities);
    let next_capabilities = ai_capabilities(next_configuration, model_capabilities);
    let mut changes = Vec::new();
    for kind in WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS {
        let Some(previous) = previous_capabilities
            .iter()
            .find(|capability| capability.kind == kind.key())
        else {
            continue;
        };
        let Some(next) = next_capabilities
            .iter()
            .find(|capability| capability.kind == kind.key())
        else {
            continue;
        };
        if ai_capability_has_native_path(kind, model_capabilities) {
            continue;
        }
        let previous_default = ai_capability_default_route(previous_configuration, kind);
        let next_default = ai_capability_default_route(next_configuration, kind);
        let mut details = Vec::new();
        if previous.enabled != next.enabled {
            let state = if next.enabled { "ENABLED" } else { "DISABLED" };
            details.push(format!(
                "state changed: {state}; native={}, bridged={}",
                next.native, next.bridged
            ));
        }
        if previous_default != next_default {
            details.push(format!(
                "default route changed: {} -> {}",
                format_ai_capability_default_route(previous_default.as_ref()),
                format_ai_capability_default_route(next_default.as_ref())
            ));
        }
        if !details.is_empty() {
            changes.push(format!(
                "- `{}` ({}): {}",
                next.kind,
                next.label,
                details.join("; ")
            ));
        }
    }
    if changes.is_empty() {
        return None;
    }
    let content = format!(
        "Runtime AI extension capability update for this session.\n\
         Treat this block as newer than any earlier conversation history about available tools.\n\
         The following session AI extension capability changes were just saved:\n{}\n\n\
         Operational rule: rely on the currently registered tool schema and this latest runtime update. \
         If a capability is listed as DISABLED or no matching tool is registered in the tool schema, do not claim you can call it. \
         This notice is persistent only for the current context window and must be forgotten after context compaction.",
        changes.join("\n")
    );
    let mut item = InjectionInsertItem::new(content);
    item.dedupe_key = Some(SESSION_AI_CAPABILITY_UPDATE_DEDUPE_KEY.to_string());
    item.compaction_policy = Some(InjectionCompactionPolicy::Clear);
    let mut insert = InjectionInsert::single(
        InjectionKind::Persistent,
        SESSION_AI_CAPABILITY_UPDATE_INJECTION_TAG,
        "Latest saved session AI extension capability changes. This notice clears on context compaction.",
        String::new(),
        None,
    );
    insert.items = vec![item];
    insert.compaction_policy = InjectionCompactionPolicy::Clear;
    insert.source_display = Some(String::from("Session AI capabilities"));
    insert.priority = -35;
    Some(insert)
}

/// Returns the default route selected by the first enabled target for one capability.
/// 返回单个能力由首个已启用目标决定的默认路由。
///
/// # Parameters
/// 参数
/// `configuration` is the AI tool configuration to inspect.
/// `configuration` 是需要检查的 AI 工具配置。
/// `kind` selects the semantic capability.
/// `kind` 选择语义能力。
///
/// # Returns
/// 返回值
/// The first enabled route target and model id when a host route exists.
/// 存在宿主路由时返回首个已启用路由目标及模型标识。
pub(super) fn ai_capability_default_route(
    configuration: &PersistedAiToolConfiguration,
    kind: AiToolKind,
) -> Option<(String, String)> {
    configuration
        .resolved_route_targets_for_kind(kind)
        .into_iter()
        .next()
        .map(|target| (target.target_selector, target.model_id))
}

/// Formats one optional AI capability default route for model-facing notices.
/// 为面向模型的通知格式化单个可选 AI 能力默认路由。
///
/// # Parameters
/// 参数
/// `route` is the optional default route tuple.
/// `route` 是可选的默认路由元组。
///
/// # Returns
/// 返回值
/// A compact stable route label.
/// 返回紧凑稳定的路由标签。
pub(super) fn format_ai_capability_default_route(route: Option<&(String, String)>) -> String {
    route
        .map(|(target, model)| format!("`{target}` using model `{model}`"))
        .unwrap_or_else(|| String::from("none"))
}

/// Parses the complete disabled capability list submitted by the WebUI session dialog.
/// 解析 WebUI 会话弹窗中的完整禁用能力列表。
pub(super) fn parse_session_ai_capability_disabled_kinds(
    raw_kinds: Vec<String>,
) -> Result<BTreeSet<AiToolKind>, String> {
    let mut disabled_kinds = BTreeSet::new();
    for raw_kind in raw_kinds {
        let capability_kind = parse_ai_tool_kind(&raw_kind)?;
        if capability_kind.supports_workbench_quick_toggle() {
            disabled_kinds.insert(capability_kind);
        }
    }
    Ok(disabled_kinds)
}

/// Applies one WebUI session dialog draft to a persisted AI capability configuration.
/// 将 WebUI 会话弹窗草稿应用到持久化 AI 能力配置。
pub(super) fn apply_session_ai_capability_dialog_draft(
    configuration: &mut PersistedAiToolConfiguration,
    disabled_kinds: &BTreeSet<AiToolKind>,
    disabled_target_selectors: &BTreeSet<String>,
    target_order_overrides: &BTreeMap<AiToolKind, Vec<String>>,
) -> Result<(), String> {
    configuration.disabled_capability_kinds = disabled_kinds.clone();
    for vendor in &mut configuration.vendors {
        let vendor_id = vendor.vendor_id.trim().to_ascii_lowercase();
        for binding in &mut vendor.targets {
            if !binding.capability_kind.supports_workbench_quick_toggle() {
                continue;
            }
            let selector = format!("{}/{}", vendor_id, binding.target_alias());
            binding.enabled = !disabled_target_selectors
                .iter()
                .any(|disabled| disabled.eq_ignore_ascii_case(&selector));
        }
    }
    for (capability_kind, target_order) in target_order_overrides {
        set_ai_capability_target_order_for_kind(
            configuration,
            *capability_kind,
            target_order.clone(),
        )?;
    }
    configuration.normalize();
    Ok(())
}
