use super::*;

/// Builds one task controller configuration from the base host configuration and the task-specific metadata.
/// 根据基础宿主配置和任务专属元数据构建一份任务控制器配置。
pub(super) fn build_task_controller_config(
    base: &CoreControllerConfig,
    stored: Option<CoreControllerConfig>,
    _description: &Option<String>,
    _subagent: &Option<String>,
    _invocation_config: &TaskInvocationConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> CoreControllerConfig {
    let mut config = stored.unwrap_or_else(|| base.clone());
    clear_rendered_task_prompt_state(&mut config);
    apply_resolved_provider_model_to_controller(&mut config, base, resolved_config);
    apply_resolved_runtime_extra_to_controller(&mut config, resolved_config);
    config
}

/// Builds the controller snapshot that may be safely stored for later TaskSession resume.
/// 构建可安全保存并用于后续 TaskSession 续跑的控制器快照。
pub(super) fn task_controller_config_for_resume_snapshot(
    controller_config: &CoreControllerConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> CoreControllerConfig {
    let mut config = controller_config.clone();
    clear_rendered_task_prompt_state(&mut config);
    apply_resolved_provider_model_to_controller(&mut config, controller_config, resolved_config);
    apply_resolved_runtime_extra_to_controller(&mut config, resolved_config);
    config
}

/// Removes provider-rendered prompt material from a reusable TaskSession controller config.
/// 从可复用的 TaskSession 控制器配置中移除 Provider 已渲染提示词材料。
fn clear_rendered_task_prompt_state(config: &mut CoreControllerConfig) {
    config.system_prompt.clear();
    config.prompt_layers = CoreProviderPromptLayers::default();
    config.context_injection = None;
}

/// Applies provider/model overrides that can safely use the existing provider transport.
/// 应用可安全复用当前 Provider 传输层的供应商与模型覆盖。
pub(super) fn apply_resolved_provider_model_to_controller(
    config: &mut CoreControllerConfig,
    _base: &CoreControllerConfig,
    resolved_config: &TaskSessionResolvedConfig,
) {
    let provider_model = &resolved_config.effective_config.provider_model;
    let session_config = ProviderConfigurationFile::default();
    if let Some((provider_id, model_id)) =
        provider_model_selector_from_snapshot(provider_model, &session_config)
    {
        config.provider_id = provider_id;
        config.model_id = model_id;
    }
}

/// Applies runtime scalar overrides stored in the resolved TaskSession extra payload.
/// 应用已解析 TaskSession 额外载荷中存储的运行时标量覆盖。
pub(super) fn apply_resolved_runtime_extra_to_controller(
    config: &mut CoreControllerConfig,
    resolved_config: &TaskSessionResolvedConfig,
) {
    let Value::Object(extra) = &resolved_config.effective_config.extra else {
        return;
    };
    if let Some(temperature) = extra
        .get("temperature")
        .and_then(Value::as_f64)
        .map(|value| value.clamp(0.0, 2.0))
    {
        config.temperature = Some(temperature);
    }
    if let Some(max_turns) = extra
        .get("max_turns")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
    {
        config.max_turns = max_turns as usize;
    }
}

/// Builds the functional system prompt override used by the prompt composer.
/// 构建提示词组合器使用的功能性系统提示词覆盖。
pub(super) fn task_functional_system_prompt(
    _base: &CoreControllerConfig,
    _config: &TaskInvocationConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> Option<String> {
    let mut sections = resolved_config
        .functional_system_prompt_layers
        .iter()
        .filter_map(|layer| normalize_optional_task_label(Some(layer)))
        .collect::<Vec<_>>();
    dedupe_preserving_order(&mut sections);
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

/// Extracts the last visible assistant text from one canonical conversation snapshot.
/// 从一份规范会话快照中提取最后一条可见助手文本。
pub(super) fn extract_final_assistant_text(snapshot: &CoreConversationSnapshot) -> Option<String> {
    snapshot.messages.iter().rev().find_map(|message| {
        if let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &message.payload
            && !text.trim().is_empty()
        {
            return Some(text.clone());
        }
        None
    })
}

/// Extracts the last visible assistant message id from one canonical conversation snapshot.
/// 从一份规范会话快照中提取最后一条可见助手消息标识。
pub(super) fn extract_final_assistant_message_id(
    snapshot: &CoreConversationSnapshot,
) -> Option<String> {
    snapshot.messages.iter().rev().find_map(|message| {
        if let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &message.payload
            && !text.trim().is_empty()
        {
            return Some(message.id.clone());
        }
        None
    })
}
