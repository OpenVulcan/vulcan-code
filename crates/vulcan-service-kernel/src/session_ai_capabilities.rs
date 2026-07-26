//! Session-scoped AI capability configuration handlers.
//! 会话级 AI 能力配置处理器。

use super::*;
use vulcan_provider_config::config::PersistedAiCapabilityTargetBinding;
use vulcan_service_api::AiCapabilitySummary;

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
    /// Complete exact target bindings grouped by capability kind.
    /// 按能力类型分组的完整精确目标绑定。
    ai_capability_target_bindings: BTreeMap<String, String>,
}

/// Loaded state required to save one session-scoped AI capability draft.
/// 保存单个会话级 AI 能力草稿所需的已加载状态。
struct SessionAiCapabilitySaveContext {
    /// Controller runtime used to persist the session addon document.
    /// 用于持久化会话附属状态文档的控制器运行时。
    controller_runtime: ControllerRuntimeHandle,
    /// Workspace snapshot validated for the target session.
    /// 已针对目标会话完成校验的工作区快照。
    workspace_snapshot: WorkspaceSnapshot,
    /// Provider configuration used to resolve model capabilities.
    /// 用于解析模型能力的供应商配置。
    provider_configuration: ProviderConfigurationFile,
    /// Previous effective AI tool configuration for the session.
    /// 该会话先前生效的 AI 工具配置。
    previous_configuration: PersistedAiToolConfiguration,
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
    let target_bindings =
        parse_ai_capability_target_bindings(Some(request.ai_capability_target_bindings))?;
    let SessionAiCapabilitySaveContext {
        controller_runtime,
        workspace_snapshot,
        provider_configuration,
        previous_configuration,
    } = load_session_ai_capability_save_context(&state, session_id, requested_project_id)?;
    let mut next_configuration = previous_configuration.clone();
    apply_session_ai_capability_dialog_draft(
        &mut next_configuration,
        &disabled_kinds,
        &disabled_target_selectors,
        &target_bindings,
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

/// Loads and validates the shared state needed by a session AI capability save request.
/// 加载并校验会话 AI 能力保存请求所需的共享状态。
///
/// # Parameters
/// 参数
/// `state` owns the service bridge runtime and workspace snapshot.
/// `state` 持有服务桥接运行态与工作区快照。
/// `session_id` identifies the session whose addon document will be updated.
/// `session_id` 标识要更新附属状态文档的会话。
/// `requested_project_id` optionally constrains the session to one owning project.
/// `requested_project_id` 可选约束会话必须属于指定项目。
///
/// # Returns
/// 返回值
/// The validated save context used by persistence and runtime projection.
/// 返回持久化与运行态投影使用的已校验保存上下文。
fn load_session_ai_capability_save_context(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    requested_project_id: Option<&str>,
) -> Result<SessionAiCapabilitySaveContext, String> {
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
    let provider_configuration = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| runtime.provider_configuration.clone())
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
    Ok(SessionAiCapabilitySaveContext {
        controller_runtime,
        workspace_snapshot: workspace_snapshot.clone(),
        provider_configuration,
        previous_configuration,
    })
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
        let Some(previous) = ai_capability_summary_for_kind(&previous_capabilities, kind) else {
            continue;
        };
        let Some(next) = ai_capability_summary_for_kind(&next_capabilities, kind) else {
            continue;
        };
        if let Some(change) = session_ai_capability_change_line(
            kind,
            previous,
            next,
            previous_configuration,
            next_configuration,
        ) {
            changes.push(change);
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

/// Finds one AI capability summary by semantic capability kind.
/// 按语义能力类型查找单个 AI 能力摘要。
///
/// # Parameters
/// 参数
/// `capabilities` contains the summaries projected for one configuration.
/// `capabilities` 包含单个配置投影出的能力摘要。
/// `kind` identifies the capability summary to find.
/// `kind` 标识要查找的能力摘要。
///
/// # Returns
/// 返回值
/// The matching capability summary, or `None` when the summary is absent.
/// 返回匹配的能力摘要；摘要不存在时返回 `None`。
fn ai_capability_summary_for_kind(
    capabilities: &[AiCapabilitySummary],
    kind: AiToolKind,
) -> Option<&AiCapabilitySummary> {
    capabilities
        .iter()
        .find(|capability| capability.kind == kind.key())
}

/// Builds one model-facing change line for a session AI capability update.
/// 为会话 AI 能力更新构建一条面向模型的变化行。
///
/// # Parameters
/// 参数
/// `kind` identifies the semantic capability being compared.
/// `kind` 标识正在比较的语义能力。
/// `previous` is the previous projected capability summary.
/// `previous` 是先前投影出的能力摘要。
/// `next` is the next projected capability summary.
/// `next` 是下一份投影出的能力摘要。
/// `previous_configuration` is the configuration before the dialog draft was saved.
/// `previous_configuration` 是弹窗草稿保存前的配置。
/// `next_configuration` is the configuration after the dialog draft was applied.
/// `next_configuration` 是弹窗草稿应用后的配置。
///
/// # Returns
/// 返回值
/// A formatted change line, or `None` when the capability has no model-visible change.
/// 返回格式化变化行；该能力没有模型可见变化时返回 `None`。
fn session_ai_capability_change_line(
    kind: AiToolKind,
    previous: &AiCapabilitySummary,
    next: &AiCapabilitySummary,
    previous_configuration: &PersistedAiToolConfiguration,
    next_configuration: &PersistedAiToolConfiguration,
) -> Option<String> {
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
    (!details.is_empty())
        .then(|| format!("- `{}` ({}): {}", next.kind, next.label, details.join("; ")))
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
    target_bindings: &BTreeMap<AiToolKind, String>,
) -> Result<(), String> {
    configuration.disabled_capability_kinds = disabled_kinds.clone();
    for vendor in &mut configuration.vendors {
        for binding in &mut vendor.targets {
            apply_session_ai_capability_target_enabled_draft(
                vendor.vendor_id.as_str(),
                binding,
                disabled_target_selectors,
            );
        }
    }
    for capability_kind in WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS {
        set_ai_capability_selected_target_for_kind(configuration, capability_kind, None)?;
    }
    for (capability_kind, selected_target) in target_bindings {
        set_ai_capability_selected_target_for_kind(
            configuration,
            *capability_kind,
            Some(selected_target.clone()),
        )?;
    }
    configuration.normalize();
    Ok(())
}

/// Applies disabled target selectors from one session dialog draft to one binding.
/// 将单个会话弹窗草稿中的禁用目标选择器应用到一个 binding。
///
/// # Parameters
/// 参数
/// `vendor_id` is the persisted vendor id owning the binding.
/// `vendor_id` 是拥有该 binding 的持久化供应商标识。
/// `binding` is the mutable target binding being updated.
/// `binding` 是正在更新的可变目标 binding。
/// `disabled_target_selectors` contains the dialog-submitted disabled target selectors.
/// `disabled_target_selectors` 包含弹窗提交的禁用目标选择器。
fn apply_session_ai_capability_target_enabled_draft(
    vendor_id: &str,
    binding: &mut PersistedAiCapabilityTargetBinding,
    disabled_target_selectors: &BTreeSet<String>,
) {
    if !binding.capability_kind.supports_workbench_quick_toggle() {
        return;
    }
    let selector = session_ai_capability_binding_selector(vendor_id, binding);
    binding.enabled = !disabled_target_selectors
        .iter()
        .any(|disabled| disabled.eq_ignore_ascii_case(&selector));
}

/// Builds the session dialog selector for one persisted AI capability target binding.
/// 为单个持久化 AI 能力目标 binding 构建会话弹窗选择器。
///
/// # Parameters
/// 参数
/// `vendor_id` is the persisted vendor id owning the binding.
/// `vendor_id` 是拥有该 binding 的持久化供应商标识。
/// `binding` is the target binding whose alias forms the selector suffix.
/// `binding` 是提供选择器后缀别名的目标 binding。
///
/// # Returns
/// 返回值
/// The normalized `vendor_id/target_alias` selector used by session dialog drafts.
/// 返回会话弹窗草稿使用的规范化 `vendor_id/target_alias` 选择器。
fn session_ai_capability_binding_selector(
    vendor_id: &str,
    binding: &PersistedAiCapabilityTargetBinding,
) -> String {
    format!(
        "{}/{}",
        vendor_id.trim().to_ascii_lowercase(),
        binding.target_alias()
    )
}
