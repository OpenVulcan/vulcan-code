use super::*;

/// Builds one task controller configuration from the base host configuration and the task-specific metadata.
/// 根据基础宿主配置和任务专属元数据构建一份任务控制器配置。
pub(super) fn build_task_controller_config(
    base: &CoreControllerConfig,
    stored: Option<CoreControllerConfig>,
    description: &Option<String>,
    subagent: &Option<String>,
    invocation_config: &TaskInvocationConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> CoreControllerConfig {
    if let Some(stored) = stored
        && stored_task_controller_config_can_be_reused(base, invocation_config, resolved_config)
    {
        return stored;
    }

    let mut config = base.clone();
    apply_resolved_provider_model_to_controller(&mut config, base, resolved_config);
    apply_resolved_runtime_extra_to_controller(&mut config, resolved_config);
    let functional_system_prompt =
        task_functional_system_prompt(base, invocation_config, resolved_config);
    let inherit_parent_config = resolved_config.effective_config.inherit_parent_config;
    let include_base_provider_prompt =
        subagent_profile_uses_base_provider_prompt(resolved_config, inherit_parent_config);
    let mut system_prompt = build_subagent_clone_system_prompt(
        base,
        description,
        subagent,
        include_base_provider_prompt,
        functional_system_prompt.as_deref(),
    )
    .unwrap_or_else(|| {
        build_legacy_task_system_prompt(
            base,
            description,
            subagent,
            include_base_provider_prompt,
            functional_system_prompt.as_deref(),
        )
    });
    append_task_windows_path_guidance(&mut system_prompt);
    if !system_prompt.trim().is_empty() {
        config.system_prompt = system_prompt;
    }
    config
}

/// Returns whether a task-specific profile still needs the base provider prompt.
/// 返回任务专属画像是否仍需要基础 provider 提示词。
fn subagent_profile_uses_base_provider_prompt(
    resolved_config: &TaskSessionResolvedConfig,
    inherit_parent_config: bool,
) -> bool {
    inherit_parent_config
        || resolved_config
            .effective_config
            .subagent_profile_id
            .as_deref()
            .is_some_and(|profile_id| !profile_id.trim().is_empty())
}

/// Decides whether a stored controller configuration still matches the resolved task prompt contract.
/// 判断已存储的控制器配置是否仍匹配当前已解析的任务提示词契约。
///
/// # Parameters
/// 参数
/// `base` is the parent controller configuration used as the inherited prompt baseline.
/// `base` 是作为继承提示词基线的父级控制器配置。
/// `invocation_config` is the current task invocation request.
/// `invocation_config` 是当前任务调用请求。
/// `resolved_config` is the already resolved task-session configuration.
/// `resolved_config` 是已经解析完成的子代理会话配置。
///
/// # Returns
/// 返回值
/// `true` when the stored controller config can be reused without hiding task-specific prompts.
/// 当复用已存储控制器配置不会遮蔽任务专属提示词时返回 `true`。
fn stored_task_controller_config_can_be_reused(
    base: &CoreControllerConfig,
    invocation_config: &TaskInvocationConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> bool {
    if resolved_config
        .effective_config
        .subagent_profile_id
        .as_deref()
        .is_some_and(|profile_id| !profile_id.trim().is_empty())
    {
        return false;
    }
    if invocation_config.system_prompt.is_some() || invocation_config.system_prompt_append.is_some()
    {
        return false;
    }
    let parent_prompt = base.system_prompt.trim();
    !resolved_config
        .effective_config
        .system_prompt_layers
        .iter()
        .filter_map(|layer| normalize_optional_task_label(Some(layer)))
        .any(|layer| layer.trim() != parent_prompt)
}

/// Applies provider/model overrides that can safely use the existing provider transport.
/// 应用可安全复用当前 Provider 传输层的供应商与模型覆盖。
pub(super) fn apply_resolved_provider_model_to_controller(
    config: &mut CoreControllerConfig,
    base: &CoreControllerConfig,
    resolved_config: &TaskSessionResolvedConfig,
) {
    let provider_model = &resolved_config.effective_config.provider_model;
    let session_config = ProviderConfigurationFile {
        model: Some(format!("{}/{}", base.provider_id, base.model_id)),
        ..ProviderConfigurationFile::default()
    };
    if let Some((provider_id, model_id)) =
        provider_model_selector_from_snapshot(provider_model, &session_config)
        && provider_id == base.provider_id
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

/// Builds a routed subagent system prompt through the prompt composer.
/// 通过提示词组合器构建按配置路由的子代理系统提示词。
pub(super) fn build_subagent_clone_system_prompt(
    base: &CoreControllerConfig,
    description: &Option<String>,
    subagent: &Option<String>,
    include_base_provider_prompt: bool,
    functional_system_prompt: Option<&str>,
) -> Option<String> {
    let functional_system_prompt = functional_system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mode = if functional_system_prompt.is_some() {
        PromptAssemblyMode::SubagentFunctional
    } else {
        PromptAssemblyMode::SubagentClone
    };
    let output = runtime_prompt_composer_engine()
        .compose(&PromptAssemblyInput {
            mode,
            model: ModelDescriptor {
                family: base.provider_id.clone(),
                model_id: base.model_id.clone(),
                supports_native_personality: true,
            },
            provider_prompt: include_base_provider_prompt.then(|| {
                ProviderPromptSnapshot::new(base.system_prompt.clone())
                    .with_source_label("Parent runtime system prompt")
            }),
            agent_prompt: None,
            personality: PersonalitySelection::Disabled,
            project_rules: None,
            environment_context: None,
            task_goal: None,
            tool_catalog: ToolCatalogSnapshot::new(Vec::new()),
            subagent: Some(SubagentSnapshot {
                identity: subagent
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("subagent")
                    .to_string(),
                task: description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Continue the delegated task.")
                    .to_string(),
                delegation: Some(String::from(
                    "Use the parent agent's existing context and return a concise result that the parent can merge back into the main flow.",
                )),
                functional_system_prompt: functional_system_prompt.map(str::to_string),
            }),
            custom_developer_segments: Vec::new(),
            custom_user_segments: Vec::new(),
        })
        .ok()?;

    let prompt = render_subagent_prompt_output(&output);
    (!prompt.trim().is_empty()).then_some(prompt)
}

/// Renders clone-mode subagent composer output into the current single system-prompt channel.
/// 将分身模式子代理组合输出渲染到当前单一系统提示词通道。
pub(super) fn render_subagent_prompt_output(output: &PromptAssemblyOutput) -> String {
    let mut sections = Vec::new();
    push_task_prompt_section(&mut sections, output.render_system_text());
    push_titled_task_prompt_section(
        &mut sections,
        "Subagent Developer Instructions",
        output.render_developer_text(),
    );
    push_titled_task_prompt_section(
        &mut sections,
        "Subagent Tool Guidance",
        output.render_tool_metadata_text(),
    );
    push_titled_task_prompt_section(&mut sections, "Subagent Context", output.render_user_text());
    sections.join("\n\n")
}

/// Builds the legacy child-task prompt when composer assembly fails.
/// 当组合器装配失败时构建旧版子任务提示词。
pub(super) fn build_legacy_task_system_prompt(
    base: &CoreControllerConfig,
    description: &Option<String>,
    subagent: &Option<String>,
    include_base_provider_prompt: bool,
    functional_system_prompt: Option<&str>,
) -> String {
    let mut system_prompt = if include_base_provider_prompt {
        base.system_prompt.clone()
    } else {
        String::new()
    };
    if let Some(subagent) = subagent.as_ref().filter(|value| !value.trim().is_empty()) {
        system_prompt.push_str(&format!("\n\n[Subagent]\n{subagent}"));
    }
    if let Some(description) = description
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        system_prompt.push_str(&format!("\n\n[Task]\n{description}"));
    }
    if let Some(functional_system_prompt) = functional_system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        system_prompt.push_str(&format!(
            "\n\n[Subagent Functional System Prompt]\n{functional_system_prompt}"
        ));
    }
    system_prompt
}

/// Builds the functional system prompt override used by the prompt composer.
/// 构建提示词组合器使用的功能性系统提示词覆盖。
pub(super) fn task_functional_system_prompt(
    base: &CoreControllerConfig,
    config: &TaskInvocationConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> Option<String> {
    let mut sections = Vec::new();
    if config.system_prompt.is_some() {
        sections.extend(
            resolved_config
                .effective_config
                .system_prompt_layers
                .iter()
                .filter_map(|layer| normalize_optional_task_label(Some(layer))),
        );
    } else {
        let parent_prompt = base.system_prompt.trim();
        sections.extend(
            resolved_config
                .effective_config
                .system_prompt_layers
                .iter()
                .filter_map(|layer| normalize_optional_task_label(Some(layer)))
                .filter(|layer| layer.trim() != parent_prompt),
        );
    }
    dedupe_preserving_order(&mut sections);
    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

/// Pushes one raw child-task prompt section when it has visible text.
/// 当子任务原始提示词分节包含可见文本时推入该分节。
pub(super) fn push_task_prompt_section(sections: &mut Vec<String>, body: String) {
    let body = body.trim();
    if !body.is_empty() {
        sections.push(body.to_string());
    }
}

/// Pushes one titled child-task prompt section when it has visible text.
/// 当子任务带标题提示词分节包含可见文本时推入该分节。
pub(super) fn push_titled_task_prompt_section(
    sections: &mut Vec<String>,
    title: &str,
    body: String,
) {
    let body = body.trim();
    if !body.is_empty() {
        sections.push(format!("## {title}\n{body}"));
    }
}

/// Appends one Windows-specific filesystem path guidance block to the child task system prompt.
/// 为子任务系统提示词追加一段 Windows 专用文件系统路径指引。
pub(super) fn append_task_windows_path_guidance(system_prompt: &mut String) {
    if !cfg!(windows) || system_prompt.contains("[Filesystem Paths]") {
        return;
    }
    system_prompt.push_str("\n\n");
    system_prompt.push_str(WINDOWS_TASK_FILESYSTEM_PATH_GUIDANCE);
}

/// Extracts the last visible assistant text from one canonical conversation snapshot.
/// 从一份规范会话快照中提取最后一条可见助手文本。
pub(super) fn extract_final_assistant_text(snapshot: &CoreConversationSnapshot) -> Option<String> {
    snapshot.messages.iter().rev().find_map(|message| {
        if let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &message.payload {
            if !text.trim().is_empty() {
                return Some(text.clone());
            }
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
        if let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &message.payload {
            if !text.trim().is_empty() {
                return Some(message.id.clone());
            }
        }
        None
    })
}
