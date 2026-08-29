//! Provider runtime context helpers for system preamble and turn-budget resolution.
//! 面向系统前导语与轮次预算解析的 Provider 运行时上下文辅助模块。

use crate::agent_runtime::config::RuntimeAgentDefinition;
use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPromptPurpose, persisted_personality_to_prompt_selection,
};
use crate::agent_runtime::prompt_composer_runtime::{
    record_runtime_prompt_composer_report, runtime_prompt_composer_engine,
};
use crate::agent_runtime::provider_prompt_loader::{
    load_provider_base_system_prompt, load_provider_base_system_prompt_with_source,
};
use crate::provider_config::{
    PersistedPromptConfiguration, load_project_configuration_file, project_configuration_file_path,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use vulcan_core::types::ProviderPromptLayers as CoreProviderPromptLayers;
use vulcan_hooks::{SystemPromptHookBaselineKind, SystemPromptHookBlock};
use vulcan_prompt_composer::{
    AdditionalPromptSegment, AgentPromptSnapshot, DEFAULT_PROJECT_RULES_FILE_NAME,
    EnvironmentContextSnapshot, ModelDescriptor, ProjectPromptFileCache, ProjectPromptSnapshot,
    ProjectRulesFileCache, ProjectRulesSnapshot, PromptAssemblyInput, PromptAssemblyMode,
    PromptAssemblyOutput, PromptBaselineKind, PromptBaselineProvenance, PromptSegment,
    PromptTarget, ProviderPromptSnapshot, SubagentSnapshot, ToolCatalogSnapshot, ToolDescriptor,
    build_context_compaction_developer_segment,
};
use vulcan_session_core::v1::context::{
    LockedBaselineProjection, LockedBaselineSegmentInput, LockedPromptTarget, LockedSourceKind,
};

/// Process-wide project-rules source cache shared by provider runtime requests.
/// Provider 运行时请求共享的进程级项目规则来源缓存。
static PROJECT_RULES_FILE_CACHE: OnceLock<Mutex<ProjectRulesFileCache>> = OnceLock::new();
/// Process-wide project-prompt source cache shared by provider runtime requests.
/// Provider 运行时请求共享的进程级项目提示词来源缓存。
static PROJECT_PROMPT_FILE_CACHE: OnceLock<Mutex<ProjectPromptFileCache>> = OnceLock::new();

/// Builds the effective layered runtime prompt sent to the real provider agent.
/// 构建发送给真实 Provider 代理的生效分层运行时提示词。
///
/// # Parameters
/// # 参数
/// - `preparation`: Validated execution preparation owning every runtime prompt source.
/// - `preparation`：拥有全部运行时提示来源的已验证执行准备。
/// - `active_agent`: Optional resolved agent definition used when no request snapshot overrides it.
/// - `active_agent`：请求快照未覆盖时使用的可选已解析代理定义。
///
/// # Returns
/// # 返回值
/// Returns complete prompt layers, or an exact project-source or composer diagnostic before provider launch.
/// 返回完整提示层；若项目来源或组合器失败，则在 Provider 启动前返回精确诊断。
pub(crate) fn build_runtime_prompt_layers_for_agent(
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
) -> Result<RuntimePromptLayers, String> {
    // input contains every validated project-owned and runtime-owned prompt source.
    // input 包含所有已验证的项目级与运行时级提示来源。
    let input = build_runtime_prompt_assembly_input(preparation, active_agent)?;
    // output must represent a successful composition; composer failures cannot silently drop prompt layers.
    // output 必须代表成功装配；组合器失败不能静默丢弃提示层。
    let output = runtime_prompt_composer_engine()
        .compose(&input)
        .map_err(|error| format!("Runtime prompt composition failed: {error}"))?;
    if active_agent.is_none() {
        record_runtime_prompt_composer_report(
            preparation.session_id.to_string(),
            preparation.session_title.clone(),
            "primary",
            preparation.model_blueprint.model_id.clone(),
            &output,
        );
    }

    RuntimePromptLayers::from_prompt_output(&output, preparation)
}

/// Runtime prompt layers plus the legacy fallback text used by old transport code.
/// 运行时提示词层，以及旧传输代码使用的兼容回退文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimePromptLayers {
    /// Legacy fallback system-preamble text.
    /// 旧版兼容系统前导语文本。
    pub(crate) fallback_text: String,
    /// Structured provider prompt layers.
    /// 结构化 Provider 提示词层。
    pub(crate) provider_layers: CoreProviderPromptLayers,
    /// Request-dynamic prompt layers that are not part of the locked context baseline.
    /// 不属于锁定上下文基线的请求动态提示词层。
    pub(crate) dynamic_prompt_layers: CoreProviderPromptLayers,
    /// Canonical context-generation baseline projected from producer-owned typed provenance.
    /// 根据生产者持有的强类型来源投影出的规范上下文代际基线。
    pub(crate) locked_baseline: LockedBaselineProjection,
}

impl RuntimePromptLayers {
    /// Build prompt layers from one prompt-composer output.
    /// 从一份提示词组合器输出构建提示词层。
    ///
    /// # Parameters
    /// # 参数
    /// - `output`: Successful structured prompt-composer output.
    /// - `output`：成功的结构化提示词组合器输出。
    /// - `preparation`: Validated execution preparation supplying hook blocks and provider resources.
    /// - `preparation`：提供 Hook 块与 Provider 资源的已验证执行准备。
    ///
    /// # Returns
    /// # 返回值
    /// Returns structured and rendered transport layers, or the exact resource failure needed by an empty-output fallback.
    /// 返回结构化与渲染后的传输层，或空输出回退所需资源的精确失败。
    fn from_prompt_output(
        output: &PromptAssemblyOutput,
        preparation: &ExecutionPreparation,
    ) -> Result<Self, String> {
        // provider_layers is the canonical structured projection produced by the prompt composer.
        // provider_layers 是提示词组合器生成的规范结构化投影。
        let mut provider_layers = render_runtime_prompt_layers(output);
        append_system_prompt_hook_region(
            &mut provider_layers,
            &preparation.system_prompt_hook_blocks,
        );
        // locked_baseline is derived from the same ordered typed segments that produced provider_layers.
        // locked_baseline 来自生成 provider_layers 的同一组有序强类型片段。
        let locked_baseline =
            project_locked_runtime_baseline(output, &preparation.system_prompt_hook_blocks)?;
        // dynamic_prompt_layers is filtered by producer-owned provenance, so the permit can
        // combine request-local instructions without duplicating the locked baseline.
        // dynamic_prompt_layers 按生产者持有的来源信息过滤，使许可可以合并请求级指令且不重复锁定基线。
        let dynamic_prompt_layers =
            render_request_dynamic_prompt_layers(output, &preparation.system_prompt_hook_blocks);
        // fallback_text supports text-only transports without inventing replacement prompt content.
        // fallback_text 支持纯文本传输，同时不虚构替代提示内容。
        let fallback_text = if provider_layers.is_empty() {
            build_legacy_runtime_system_preamble(preparation)?
        } else {
            provider_layers.render_fallback_system_prompt()
        };
        Ok(Self {
            fallback_text,
            provider_layers,
            dynamic_prompt_layers,
            locked_baseline,
        })
    }
}

/// Builds the prompt-composer input used by the runtime bridge.
/// 构建运行时桥接层使用的提示词组合输入。
///
/// # Parameters
/// # 参数
/// - `preparation`: Validated execution preparation containing workspace-owned prompt configuration.
/// - `preparation`：包含工作区自有提示配置的已验证执行准备。
/// - `active_agent`: Optional resolved agent definition used as a prompt fallback source.
/// - `active_agent`：作为提示词回退来源使用的可选已解析代理定义。
///
/// # Returns
/// # 返回值
/// Returns an assembly input only after every configured project source loads successfully.
/// 仅在所有已配置项目来源均成功加载后返回装配输入。
fn build_runtime_prompt_assembly_input(
    preparation: &ExecutionPreparation,
    active_agent: Option<&RuntimeAgentDefinition>,
) -> Result<PromptAssemblyInput, String> {
    // provider_prompt is loaded from the single effective model-mapped resource file.
    // provider_prompt 从唯一生效的模型映射资源文件加载。
    let provider_prompt = load_provider_base_system_prompt_with_source()?;
    let agent_prompt = build_request_agent_prompt_snapshot(preparation)
        .or_else(|| active_agent.and_then(build_agent_prompt_snapshot));
    let environment_context = EnvironmentContextSnapshot::new(format!(
        "Current workspace: {}\nCurrent execution mode: {}",
        preparation.workspace_path, preparation.execution_mode
    ));
    let mut custom_developer_segments =
        build_prompt_purpose_developer_segments(preparation.prompt_purpose);
    if let Some(segment) = preparation.session_plan_context.clone() {
        custom_developer_segments.push(AdditionalPromptSegment::dynamic(
            "Session Plan Context",
            segment,
            "Runtime session plan context",
        ));
    }
    custom_developer_segments.extend(preparation.additional_developer_segments.clone());

    let subagent_prompt_context = preparation.subagent_prompt_context.as_ref();
    Ok(PromptAssemblyInput {
        mode: prompt_assembly_mode_for_preparation(preparation),
        model: ModelDescriptor {
            family: preparation
                .model_blueprint
                .protocol_profile_key()
                .to_string(),
            model_id: preparation.model_blueprint.model_id.clone(),
            supports_native_personality: true,
        },
        provider_prompt: Some(
            ProviderPromptSnapshot::new(provider_prompt.text)
                .with_source_label(provider_prompt.source_label),
        ),
        agent_prompt,
        personality: persisted_personality_to_prompt_selection(
            &preparation.prompt_configuration.personality,
        ),
        output_style: preparation.output_style_snapshot.clone(),
        project_rules: load_workspace_project_rules_snapshot(&preparation.workspace_path)?,
        environment_context: Some(environment_context),
        // Session goals are delivered through append-only user turns so goal creation never rewrites the provider prefix.
        // 会话目标通过追加式用户回合投递，确保目标创建不会改写 Provider 前缀。
        task_goal: None,
        tool_catalog: ToolCatalogSnapshot::new(
            preparation
                .tool_descriptors
                .iter()
                .map(|tool| ToolDescriptor {
                    id: tool.id.clone(),
                    title: tool.name.clone(),
                    description: tool.description.clone(),
                    help_text: tool.help_text.clone(),
                    schema_fingerprint: None,
                })
                .collect(),
        ),
        subagent: subagent_prompt_context.map(|context| SubagentSnapshot {
            identity: context.identity.clone(),
            task: context.delegated_task.clone(),
            delegation: context.delegation.clone(),
            functional_system_prompt: context.functional_system_prompt.clone(),
        }),
        custom_developer_segments,
        custom_user_segments: load_workspace_project_user_prompt(preparation)?
            .into_iter()
            .collect(),
    })
}

/// Selects the prompt-composer mode for one prepared execution.
/// 为一次已准备执行选择提示词组合模式。
fn prompt_assembly_mode_for_preparation(preparation: &ExecutionPreparation) -> PromptAssemblyMode {
    if let Some(context) = preparation.subagent_prompt_context.as_ref() {
        if context.uses_functional_prompt() {
            return PromptAssemblyMode::SubagentFunctional;
        }
        return PromptAssemblyMode::SubagentClone;
    }
    match preparation.prompt_purpose {
        ExecutionPromptPurpose::UserTurn
        | ExecutionPromptPurpose::ExecutionRetry
        | ExecutionPromptPurpose::ContextCompactionCachePreserving => PromptAssemblyMode::Primary,
        ExecutionPromptPurpose::ContextCompaction => PromptAssemblyMode::ContextCompactionMinimal,
        ExecutionPromptPurpose::GitCommitSummary => PromptAssemblyMode::ContextCompactionMinimal,
    }
}

/// Builds developer-side runtime instructions required by a special prompt purpose.
/// 构建特殊提示词用途所需的开发者侧运行时指令。
fn build_prompt_purpose_developer_segments(
    purpose: ExecutionPromptPurpose,
) -> Vec<AdditionalPromptSegment> {
    match purpose {
        ExecutionPromptPurpose::UserTurn
        | ExecutionPromptPurpose::ExecutionRetry
        | ExecutionPromptPurpose::ContextCompactionCachePreserving => Vec::new(),
        ExecutionPromptPurpose::ContextCompaction => vec![AdditionalPromptSegment::dynamic(
            "Context Compaction Contract",
            build_context_compaction_developer_segment(),
            "Runtime context-compaction purpose",
        )],
        ExecutionPromptPurpose::GitCommitSummary => vec![AdditionalPromptSegment::dynamic(
            "Git Commit Summary Contract",
            build_git_commit_summary_developer_segment(),
            "Runtime Git commit-summary purpose",
        )],
    }
}

/// Builds the fixed developer instruction used by internal Git commit-message summaries.
/// 构建内部 Git 提交信息总结使用的固定开发者指令。
fn build_git_commit_summary_developer_segment() -> String {
    String::from(
        "You generate Git commit messages for a local desktop client. Do not call tools. Use only the repository change data in the user message. Output only the final commit message text.",
    )
}

/// Builds an agent prompt snapshot with a useful runtime source label.
/// 构建带有可用运行时来源标签的 Agent 提示词快照。
fn build_agent_prompt_snapshot(agent: &RuntimeAgentDefinition) -> Option<AgentPromptSnapshot> {
    let text = agent.system_prompt.trim();
    if text.is_empty() {
        return None;
    }
    Some(
        AgentPromptSnapshot::new(text.to_string()).with_source_label(format!(
            "Runtime agent `{}` ({}) system prompt",
            agent.id, agent.name
        )),
    )
}

/// Builds a request-scoped fixed agent prompt snapshot when one is present.
/// 当请求携带固定代理提示词快照时构建请求级代理提示词快照。
fn build_request_agent_prompt_snapshot(
    preparation: &ExecutionPreparation,
) -> Option<AgentPromptSnapshot> {
    let snapshot = preparation.agent_prompt_snapshot.as_ref()?;
    let text = snapshot.text.trim();
    if text.is_empty() {
        return None;
    }
    Some(AgentPromptSnapshot::new(text.to_string()).with_source_label(snapshot.source_label()))
}

/// Renders one prompt-composer output into structured provider prompt layers.
/// 将一份提示词组合器输出渲染为结构化 Provider 提示词层。
fn render_runtime_prompt_layers(output: &PromptAssemblyOutput) -> CoreProviderPromptLayers {
    CoreProviderPromptLayers {
        system: output.render_system_text(),
        developer: output.render_developer_text(),
        tool_guidance: output.render_tool_metadata_text(),
        runtime_context: output.render_user_text(),
    }
}

/// Renders only request-dynamic prompt segments from one composer output.
/// 仅渲染一份组合器输出中的请求动态提示词片段。
///
/// # Parameters
/// # 参数
/// - `output`: Successful prompt-composer output with producer-owned baseline provenance.
/// - `output`：包含生产者持有基线来源信息的成功提示词组合器输出。
/// - `hook_blocks`: Ordered system hook blocks from the same request assembly.
/// - `hook_blocks`：来自同一次请求装配的有序系统 Hook 块。
///
/// # Returns
/// # 返回值
/// Returns layers containing only segments whose producers did not mark them as locked baseline sources.
/// 返回仅包含生产者未标记为锁定基线来源的提示词层。
fn render_request_dynamic_prompt_layers(
    output: &PromptAssemblyOutput,
    hook_blocks: &[SystemPromptHookBlock],
) -> CoreProviderPromptLayers {
    let mut layers = CoreProviderPromptLayers {
        system: render_dynamic_prompt_segments(&output.system_segments),
        developer: render_dynamic_prompt_segments(&output.developer_segments),
        tool_guidance: render_dynamic_prompt_segments(&output.tool_metadata_segments),
        runtime_context: render_dynamic_prompt_segments(&output.user_segments),
    };
    let hook_region = render_system_prompt_hook_region(
        &hook_blocks
            .iter()
            .filter(|block| block.baseline_provenance.is_none())
            .cloned()
            .collect::<Vec<_>>(),
    );
    append_prompt_text(&mut layers.system, &hook_region);
    layers
}

/// Joins request-dynamic composer segments without reclassifying them from names or titles.
/// 拼接请求动态组合器片段，不根据名称或标题重新分类。
///
/// # Parameters
/// # 参数
/// - `segments`: Ordered prompt segments for one provider target.
/// - `segments`：一个 Provider 目标的有序提示词片段。
///
/// # Returns
/// # 返回值
/// Returns non-empty bodies whose authoritative baseline provenance is absent.
/// 返回权威基线来源为空的非空正文。
fn render_dynamic_prompt_segments(segments: &[PromptSegment]) -> String {
    segments
        .iter()
        .filter(|segment| segment.baseline_provenance.is_none())
        .map(|segment| segment.body.as_str())
        .filter(|body| !body.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Appends one non-empty prompt block with the canonical two-newline separator.
/// 使用规范的双换行分隔符追加一个非空提示词块。
///
/// # Parameters
/// # 参数
/// - `target`: Existing target prompt text.
/// - `target`：现有目标提示词文本。
/// - `addition`: Candidate prompt block to append.
/// - `addition`：待追加的候选提示词块。
fn append_prompt_text(target: &mut String, addition: &str) {
    if addition.trim().is_empty() {
        return;
    }
    if target.trim().is_empty() {
        *target = addition.to_string();
    } else {
        *target = format!("{}\n\n{}", target.trim_end(), addition);
    }
}

/// Projects producer-declared mutable baselines from the exact assembled prompt boundary.
/// 从精确装配完成的提示词边界投影生产者声明的可变基线。
fn project_locked_runtime_baseline(
    output: &PromptAssemblyOutput,
    hook_blocks: &[SystemPromptHookBlock],
) -> Result<LockedBaselineProjection, String> {
    let mut segments = Vec::new();
    for segment in &output.system_segments {
        push_locked_prompt_segment(&mut segments, segment)?;
    }
    for block in hook_blocks {
        push_locked_hook_segment(&mut segments, block)?;
    }
    for segment in &output.developer_segments {
        push_locked_prompt_segment(&mut segments, segment)?;
    }
    for segment in &output.user_segments {
        push_locked_prompt_segment(&mut segments, segment)?;
    }
    for segment in &output.tool_metadata_segments {
        if segment.baseline_provenance.is_some() {
            return Err(format!(
                "Prompt segment `{}` declared locked provenance for excluded tool metadata.",
                segment.node_id
            ));
        }
    }
    LockedBaselineProjection::project(segments)
        .map_err(|error| format!("Runtime locked-baseline projection failed: {error}"))
}

/// Appends one composer segment only when its authoritative producer marked it as a baseline.
/// 仅当权威生产者将组合器片段标记为基线时才追加该片段。
fn push_locked_prompt_segment(
    output: &mut Vec<LockedBaselineSegmentInput>,
    segment: &PromptSegment,
) -> Result<(), String> {
    let Some(provenance) = segment.baseline_provenance.as_ref() else {
        return Ok(());
    };
    let target = match segment.target {
        PromptTarget::System => LockedPromptTarget::System,
        PromptTarget::Developer => LockedPromptTarget::Developer,
        PromptTarget::User => LockedPromptTarget::User,
        PromptTarget::ToolMetadata => {
            return Err(format!(
                "Prompt segment `{}` cannot lock tool metadata.",
                segment.node_id
            ));
        }
        PromptTarget::DiagnosticsOnly => {
            return Err(format!(
                "Prompt segment `{}` cannot lock diagnostics-only content.",
                segment.node_id
            ));
        }
    };
    let kind = match provenance.kind {
        PromptBaselineKind::MemoryProfile => LockedSourceKind::MemoryProfile,
        PromptBaselineKind::AgentsMd => LockedSourceKind::AgentsMd,
        PromptBaselineKind::VulcanMd => LockedSourceKind::VulcanMd,
        PromptBaselineKind::AgentPersona => LockedSourceKind::AgentPersona,
        PromptBaselineKind::OutputStyle => LockedSourceKind::OutputStyle,
        PromptBaselineKind::PromptSkillInjection => LockedSourceKind::PromptSkillInjection,
        PromptBaselineKind::PluginInstructions => LockedSourceKind::PluginInstructions,
        PromptBaselineKind::BaselineHooks => LockedSourceKind::BaselineHooks,
    };
    output.push(LockedBaselineSegmentInput {
        kind,
        node_id: segment.node_id.clone(),
        target,
        source_label: Some(segment.source_label.clone()),
        source_id: Some(provenance.source_id.clone()),
        content: segment.body.clone(),
    });
    Ok(())
}

/// Appends one rendered hook block only when the hook producer declared stable baseline provenance.
/// 仅当 Hook 生产者声明稳定基线来源时才追加渲染后的 Hook 块。
fn push_locked_hook_segment(
    output: &mut Vec<LockedBaselineSegmentInput>,
    block: &SystemPromptHookBlock,
) -> Result<(), String> {
    let Some(provenance) = block.baseline_provenance.as_ref() else {
        return Ok(());
    };
    let Some(content) = render_system_prompt_hook_block(block) else {
        return Ok(());
    };
    let kind = match provenance.kind {
        SystemPromptHookBaselineKind::MemoryProfile => LockedSourceKind::MemoryProfile,
        SystemPromptHookBaselineKind::PluginInstructions => LockedSourceKind::PluginInstructions,
        SystemPromptHookBaselineKind::BaselineHooks => LockedSourceKind::BaselineHooks,
    };
    output.push(LockedBaselineSegmentInput {
        kind,
        node_id: provenance.node_id.clone(),
        target: LockedPromptTarget::System,
        source_label: block.source.clone(),
        source_id: Some(provenance.source_id.clone()),
        content,
    });
    Ok(())
}

/// Appends request-scoped hook blocks into the system prompt layer.
/// 将请求级 hook 块追加到系统提示词层。
fn append_system_prompt_hook_region(
    provider_layers: &mut CoreProviderPromptLayers,
    blocks: &[SystemPromptHookBlock],
) {
    let region = render_system_prompt_hook_region(blocks);
    if region.is_empty() {
        return;
    }
    if provider_layers.system.trim().is_empty() {
        provider_layers.system = region;
    } else {
        provider_layers.system = format!("{}\n\n{}", provider_layers.system.trim_end(), region);
    }
}

/// Renders request-scoped hook blocks inside one stable system prompt region.
/// 在稳定系统提示词区域内渲染请求级 hook 块。
fn render_system_prompt_hook_region(blocks: &[SystemPromptHookBlock]) -> String {
    let rendered_blocks = blocks
        .iter()
        .filter_map(render_system_prompt_hook_block)
        .collect::<Vec<_>>();
    if rendered_blocks.is_empty() {
        String::new()
    } else {
        format!(
            "<system_prompt_hooks>\n{}\n</system_prompt_hooks>",
            rendered_blocks.join("\n\n")
        )
    }
}

/// Renders one sanitized hook block for provider-visible system prompt text.
/// 将单个清洗后的 hook 块渲染为 Provider 可见系统提示词文本。
fn render_system_prompt_hook_block(block: &SystemPromptHookBlock) -> Option<String> {
    let tag = normalize_system_prompt_hook_tag(&block.tag)?;
    let content = block.content.trim();
    if content.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    let description = block.description.trim();
    if !description.is_empty() {
        lines.push(description.to_string());
    }
    lines.push(format!("Phase: {}", block.phase.as_str()));
    if let Some(source) = block
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Source: {source}"));
    }
    lines.push(String::new());
    lines.push(content.to_string());
    Some(format!("<{tag}>\n{}\n</{tag}>", lines.join("\n")))
}

/// Normalizes a hook tag so a malformed plugin tag cannot break the surrounding region.
/// 规范化 hook 标签，避免格式错误的插件标签破坏外围区域。
fn normalize_system_prompt_hook_tag(tag: &str) -> Option<String> {
    let normalized = tag
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect::<String>();
    (!normalized.is_empty()).then_some(normalized)
}

/// Loads one workspace-level project rules snapshot when `AGENTS.md` exists.
/// 当 `AGENTS.md` 存在时加载一份工作区级项目规则快照。
///
/// # Parameters
/// # 参数
/// - `workspace_path`: Persisted workspace path that may own the default `AGENTS.md` file.
/// - `workspace_path`：可能拥有默认 `AGENTS.md` 文件的持久化工作区路径。
///
/// # Returns
/// # 返回值
/// Returns `None` for a confirmed absent or blank rules file and preserves metadata or content failures.
/// 对确认缺失或空白的规则文件返回 `None`，并保留元数据或内容读取失败。
fn load_workspace_project_rules_snapshot(
    workspace_path: &str,
) -> Result<Option<ProjectRulesSnapshot>, String> {
    let Some(workspace) = resolve_workspace_directory(workspace_path)? else {
        return Ok(None);
    };
    // rules_path is the exact default source selected after workspace validation.
    // rules_path 是工作区验证后选择的精确默认来源。
    let rules_path = workspace.join(DEFAULT_PROJECT_RULES_FILE_NAME);
    // cache provides metadata-aware reuse without changing missing-file semantics.
    // cache 提供元数据感知复用，同时不改变文件缺失语义。
    let cache = PROJECT_RULES_FILE_CACHE.get_or_init(|| Mutex::new(ProjectRulesFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.load_file(&rules_path).map_err(|error| {
        format!(
            "Failed to load workspace project rules from `{}`: {error}",
            rules_path.display()
        )
    })
}

/// Invalidate the cached workspace-local project prompt snapshot.
/// 使缓存中的工作区本地项目提示词快照失效。
///
/// # Parameters
/// # 参数
/// - `workspace_path`: Persisted workspace directory that owns the cached prompt.
/// - `workspace_path`：拥有缓存提示词的持久化工作区目录。
/// - `project_prompt_path`: Configured project-prompt path relative to the workspace.
/// - `project_prompt_path`：相对于工作区的已配置项目提示词路径。
///
/// # Returns
/// # 返回值
/// Returns success after invalidation or intentional absence, and preserves workspace metadata failures.
/// 完成失效处理或来源有意缺失时返回成功，并保留工作区元数据失败。
pub fn invalidate_workspace_project_user_prompt_cache(
    workspace_path: &str,
    project_prompt_path: &str,
) -> Result<(), String> {
    let Some(prompt_path) = workspace_project_prompt_path(
        workspace_path,
        project_prompt_path.trim().trim_start_matches(['/', '\\']),
    )?
    else {
        return Ok(());
    };
    let cache = PROJECT_PROMPT_FILE_CACHE.get_or_init(|| Mutex::new(ProjectPromptFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.invalidate_file(prompt_path);
    Ok(())
}

/// Invalidate the cached workspace-level project rules snapshot.
/// 使缓存中的工作区级项目规则快照失效。
///
/// # Parameters
/// # 参数
/// - `workspace_path`: Persisted workspace directory that owns the default rules file.
/// - `workspace_path`：拥有默认规则文件的持久化工作区目录。
///
/// # Returns
/// # 返回值
/// Returns success after invalidation or intentional absence, and preserves workspace metadata failures.
/// 完成失效处理或来源有意缺失时返回成功，并保留工作区元数据失败。
pub fn invalidate_workspace_project_rules_prompt_cache(workspace_path: &str) -> Result<(), String> {
    let Some(workspace) = resolve_workspace_directory(workspace_path)? else {
        return Ok(());
    };
    let cache = PROJECT_RULES_FILE_CACHE.get_or_init(|| Mutex::new(ProjectRulesFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.invalidate_workspace_default(workspace);
    Ok(())
}

/// Resolves and validates the workspace directory used by project-owned prompt sources.
/// 解析并验证项目自有提示来源使用的工作区目录。
///
/// # Parameters
/// # 参数
/// - `workspace_path`: Persisted workspace path supplied by execution preparation.
/// - `workspace_path`：执行准备提供的持久化工作区路径。
///
/// # Returns
/// # 返回值
/// Returns `None` only for an empty, confirmed missing, or non-directory path, and preserves metadata failures.
/// 仅对空路径、已确认缺失路径或非目录路径返回 `None`，并保留元数据失败。
fn resolve_workspace_directory(workspace_path: &str) -> Result<Option<&Path>, String> {
    if workspace_path.trim().is_empty() {
        return Ok(None);
    }
    // workspace is the exact persisted path whose metadata determines prompt-source eligibility.
    // workspace 是其元数据决定提示来源资格的精确持久化路径。
    let workspace = Path::new(workspace_path);
    // metadata distinguishes a confirmed missing workspace from an inaccessible one.
    // metadata 区分已确认缺失的工作区与不可访问工作区。
    let metadata = match fs::metadata(workspace) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Failed to inspect workspace prompt source directory `{}`: {error}",
                workspace.display()
            ));
        }
    };
    if !metadata.is_dir() {
        return Ok(None);
    }
    Ok(Some(workspace))
}

/// Resolves one workspace-local project prompt path.
/// 解析单个工作区本地项目提示词路径。
///
/// # Parameters
/// # 参数
/// - `workspace_path`: Persisted workspace path that owns the configured prompt.
/// - `workspace_path`：拥有已配置提示词的持久化工作区路径。
/// - `project_prompt_path`: Normalized nonabsolute prompt path below the workspace.
/// - `project_prompt_path`：工作区下经过规范化的非绝对提示词路径。
///
/// # Returns
/// # 返回值
/// Returns the joined path when both inputs are valid, `None` for intentional absence, or a workspace metadata failure.
/// 两项输入有效时返回拼接路径；有意缺失时返回 `None`；工作区元数据失败时返回错误。
fn workspace_project_prompt_path(
    workspace_path: &str,
    project_prompt_path: &str,
) -> Result<Option<PathBuf>, String> {
    if project_prompt_path.trim().is_empty() {
        return Ok(None);
    }
    let Some(workspace) = resolve_workspace_directory(workspace_path)? else {
        return Ok(None);
    };
    Ok(Some(workspace.join(project_prompt_path)))
}

/// Loads the workspace-local user prompt configured for prompt composition.
/// 加载为提示词组合配置的工作区本地用户提示词。
///
/// # Parameters
/// # 参数
/// - `preparation`: Execution preparation containing the workspace and global prompt defaults.
/// - `preparation`：包含工作区与全局提示默认值的执行准备。
///
/// # Returns
/// # 返回值
/// Returns the formatted project prompt when enabled and nonblank, `None` when intentionally absent, or an exact load failure.
/// 启用且非空时返回格式化项目提示词；有意缺失时返回 `None`；加载失败时返回精确错误。
fn load_workspace_project_user_prompt(
    preparation: &ExecutionPreparation,
) -> Result<Option<AdditionalPromptSegment>, String> {
    // project_prompt_configuration is the validated effective project prompt selection.
    // project_prompt_configuration 是经过验证的生效项目提示词选择。
    let project_prompt_configuration = resolve_project_prompt_configuration(preparation)?;
    if !project_prompt_configuration.project_prompt_enabled {
        return Ok(None);
    }
    let Some(workspace) = resolve_workspace_directory(&preparation.workspace_path)? else {
        return Ok(None);
    };
    // relative_path is the configured workspace-local prompt path after separator normalization.
    // relative_path 是经过分隔符规范化的工作区本地提示词配置路径。
    let relative_path = project_prompt_configuration
        .project_prompt_path
        .trim()
        .trim_start_matches(['/', '\\']);
    if relative_path.is_empty() {
        return Ok(None);
    }
    // prompt_path is the exact configured source below the validated workspace directory.
    // prompt_path 是经过验证的工作区目录下精确配置的来源路径。
    let prompt_path = workspace.join(relative_path);
    let cache = PROJECT_PROMPT_FILE_CACHE.get_or_init(|| Mutex::new(ProjectPromptFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .load_file(&prompt_path)
        .map_err(|error| {
            format!(
                "Failed to load workspace project prompt from `{}`: {error}",
                prompt_path.display()
            )
        })
        .map(|snapshot| {
            snapshot.map(|snapshot: ProjectPromptSnapshot| {
                let source = snapshot.source;
                let text = snapshot.text.trim();
                AdditionalPromptSegment::locked(
                    "Project User Prompt",
                    format!("# Project User Prompt\nSource: {source}\n\n{text}"),
                    format!("Project prompt `{source}`"),
                    PromptBaselineProvenance::new(PromptBaselineKind::VulcanMd, source),
                )
            })
        })
}

/// Resolve the effective project-prompt configuration for one workspace.
/// 解析单个工作区的生效项目提示词配置。
///
/// # Parameters
/// # 参数
/// - `preparation`: Execution preparation supplying global defaults and the workspace path.
/// - `preparation`：提供全局默认值与工作区路径的执行准备。
///
/// # Returns
/// # 返回值
/// Returns global defaults with explicit project overrides, or the project configuration load failure.
/// 返回叠加明确项目覆盖后的全局默认值，或项目配置加载失败。
fn resolve_project_prompt_configuration(
    preparation: &ExecutionPreparation,
) -> Result<PersistedPromptConfiguration, String> {
    let Some(workspace) = resolve_workspace_directory(&preparation.workspace_path)? else {
        return Ok(preparation.prompt_configuration.clone());
    };
    // effective starts from the request snapshot and receives only explicit project overrides.
    // effective 从请求快照开始，仅接收明确的项目级覆盖。
    let mut effective = preparation.prompt_configuration.clone();
    if let Some(project_configuration) = load_workspace_project_prompt_configuration(workspace)? {
        effective.project_prompt_enabled = project_configuration.project_prompt_enabled;
        effective.project_prompt_path = project_configuration.project_prompt_path;
    }
    Ok(effective)
}

/// Load one workspace-local prompt configuration override when present.
/// 当存在时加载一个工作区本地提示词配置覆盖。
///
/// # Parameters
/// # 参数
/// - `workspace`: Validated workspace directory owning `.vulcan/code/config.json`.
/// - `workspace`：拥有 `.vulcan/code/config.json` 的已验证工作区目录。
///
/// # Returns
/// # 返回值
/// Returns the optional prompt subsection and preserves configuration I/O or JSON parsing failures.
/// 返回可选提示配置段，并保留配置 I/O 或 JSON 解析失败。
fn load_workspace_project_prompt_configuration(
    workspace: &Path,
) -> Result<Option<PersistedPromptConfiguration>, String> {
    // configuration_path is the exact project-local file consumed by the shared configuration loader.
    // configuration_path 是共享配置加载器消费的精确项目本地文件。
    let configuration_path = project_configuration_file_path(workspace);
    load_project_configuration_file(workspace)
        .map_err(|error| {
            format!(
                "Failed to load workspace project prompt configuration from `{}`: {error}",
                configuration_path.display()
            )
        })
        .map(|configuration| configuration.prompt)
}

/// Builds legacy transport text only when successful composition yields no structured layers.
/// 仅在成功组合未生成任何结构化提示层时构建旧传输文本。
///
/// # Parameters
/// # 参数
/// - `preparation`: Validated execution preparation selecting the exact provider prompt resource.
/// - `preparation`：选择精确 Provider 提示词资源的已验证执行准备。
///
/// # Returns
/// # 返回值
/// Returns rendered legacy transport text, or the exact provider prompt resource failure.
/// 返回渲染后的旧传输文本，或精确的 Provider 提示词资源失败。
fn build_legacy_runtime_system_preamble(
    preparation: &ExecutionPreparation,
) -> Result<String, String> {
    // provider_prompt comes from the same unique resource contract as structured composition.
    // provider_prompt 来自与结构化组合相同的唯一资源契约。
    let provider_prompt = load_provider_base_system_prompt()?;
    // sections preserves legacy transport ordering without supplying compatibility content.
    // sections 保留旧传输顺序，但不提供兼容性内容。
    let mut sections = vec![
        provider_prompt,
        format!("Current workspace: {}", preparation.workspace_path),
        format!("Current execution mode: {}", preparation.execution_mode),
    ];
    // hook_region is the validated runtime-owned system extension appended after base context.
    // hook_region 是追加在基础上下文之后的已验证运行时自有系统扩展。
    let hook_region = render_system_prompt_hook_region(&preparation.system_prompt_hook_blocks);
    if !hook_region.is_empty() {
        sections.push(hook_region);
    }
    Ok(sections.join("\n"))
}

/// Keeps the main provider tool loop unlimited regardless of legacy agent or prompt-purpose settings.
/// 无论旧版智能体或提示用途设置如何，均保持主 Provider 工具循环无限。
pub(crate) fn resolve_runtime_max_turns(
    _active_agent: Option<&RuntimeAgentDefinition>,
    _prompt_purpose: ExecutionPromptPurpose,
) -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    //! Provider runtime context tests.
    //! Provider 运行时上下文测试。

    use super::*;
    use crate::agent_runtime::execution_models::{
        ExecutionAgentPromptSnapshot, ExecutionSubagentPromptContext,
    };
    use crate::context_budget::ContextBudgetSnapshot;
    use crate::model::SessionId;
    use crate::provider_config::{
        PersistedPromptConfiguration, PersistedPromptPersonalitySelection,
        ProjectVulcanCodeConfigurationFile, ReasoningEffortLevel, save_project_configuration_file,
    };
    use std::fs;
    use std::path::PathBuf;
    use vulcan_context::{ContextAssembly, ContextBaseline};

    /// Verifies the project-rules loader reads a non-empty workspace AGENTS.md file.
    /// 验证项目规则加载器会读取非空的工作区 AGENTS.md 文件。
    #[test]
    fn project_rules_loader_reads_workspace_agents_file() {
        let root = unique_temp_dir("vulcan-project-rules-read");
        fs::create_dir_all(&root).expect("create temp workspace");
        let rules_path = root.join(DEFAULT_PROJECT_RULES_FILE_NAME);
        fs::write(&rules_path, "Rule A\n").expect("write project rules");

        let snapshot = load_workspace_project_rules_snapshot(&root.display().to_string())
            .expect("project rules load should succeed")
            .expect("project rules should exist");

        assert_eq!(snapshot.text, "Rule A");
        assert_eq!(snapshot.source, rules_path.display().to_string());

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies missing or empty project rules are not injected.
    /// 验证缺失或空白项目规则不会被注入。
    #[test]
    fn project_rules_loader_skips_missing_or_empty_agents_file() {
        let missing_root = unique_temp_dir("vulcan-project-rules-missing");
        fs::create_dir_all(&missing_root).expect("create missing workspace");
        assert!(
            load_workspace_project_rules_snapshot(&missing_root.display().to_string())
                .expect("missing project rules load should succeed")
                .is_none()
        );

        let empty_root = unique_temp_dir("vulcan-project-rules-empty");
        fs::create_dir_all(&empty_root).expect("create empty workspace");
        fs::write(empty_root.join(DEFAULT_PROJECT_RULES_FILE_NAME), " \n\t")
            .expect("write empty project rules");
        assert!(
            load_workspace_project_rules_snapshot(&empty_root.display().to_string())
                .expect("empty project rules load should succeed")
                .is_none()
        );

        let _ = fs::remove_dir_all(missing_root);
        let _ = fs::remove_dir_all(empty_root);
    }

    /// Verifies external watcher events can invalidate the shared project-rules cache.
    /// 验证外部监听事件可以使共享项目规则缓存失效。
    #[test]
    fn project_rules_loader_supports_explicit_cache_invalidation() {
        let root = unique_temp_dir("vulcan-project-rules-invalidate");
        fs::create_dir_all(&root).expect("create temp workspace");
        let rules_path = root.join(DEFAULT_PROJECT_RULES_FILE_NAME);
        fs::write(&rules_path, "Rule before\n").expect("write first rules");

        let first = load_workspace_project_rules_snapshot(&root.display().to_string())
            .expect("first project rules load should succeed")
            .expect("first project rules should exist");
        assert_eq!(first.text, "Rule before");

        fs::write(&rules_path, "Rule after\n").expect("write second rules");
        invalidate_workspace_project_rules_prompt_cache(&root.display().to_string())
            .expect("project rules cache invalidation should succeed");

        let second = load_workspace_project_rules_snapshot(&root.display().to_string())
            .expect("second project rules load should succeed")
            .expect("second project rules should exist");
        assert_eq!(second.text, "Rule after");

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies unreadable project-rules content aborts prompt assembly instead of disappearing.
    /// 验证无法读取的项目规则内容会中止提示装配，而不是静默消失。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if invalid UTF-8 rules are treated as absent.
    /// 此测试无返回值；若无效 UTF-8 规则被视为不存在则失败。
    #[test]
    fn runtime_prompt_layers_propagate_project_rules_read_failure() {
        // root isolates the invalid project-rules source from every other cache entry.
        // root 将无效项目规则来源与其他所有缓存条目隔离。
        let root = unique_temp_dir("vulcan-project-rules-invalid-utf8");
        fs::create_dir_all(&root).expect("create invalid-rules workspace");
        // rules_path is the exact default project-rules source selected by the runtime.
        // rules_path 是运行时选择的精确默认项目规则来源。
        let rules_path = root.join(DEFAULT_PROJECT_RULES_FILE_NAME);
        fs::write(&rules_path, [0xff, 0xfe, 0xfd]).expect("write invalid project rules");
        // preparation binds prompt assembly to the invalid workspace source.
        // preparation 将提示装配绑定到无效工作区来源。
        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration::default(),
        );

        // error must retain both the source category and exact file path.
        // error 必须同时保留来源类别与精确文件路径。
        let error = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect_err("invalid project rules should abort prompt assembly");

        assert!(error.contains("workspace project rules"));
        assert!(error.contains(&rules_path.display().to_string()));
        fs::remove_dir_all(root).expect("remove invalid-rules workspace");
    }

    /// Verifies workspace project prompts enter the runtime user-context layer.
    /// 验证工作区项目提示词会进入运行时用户上下文层。
    #[test]
    fn runtime_prompt_layers_include_workspace_project_prompt() {
        let root = unique_temp_dir("vulcan-project-user-prompt");
        let prompt_dir = root.join(".vulcan").join("code");
        fs::create_dir_all(&prompt_dir).expect("create project prompt directory");
        fs::write(prompt_dir.join("prompt.md"), "Project user instruction")
            .expect("write project prompt");

        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration::default(),
        );
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .provider_layers
                .runtime_context
                .contains("Project user instruction")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies workspace-local prompt configuration can override the global project prompt path.
    /// 验证工作区本地提示词配置可以覆盖全局项目提示词路径。
    #[test]
    fn runtime_prompt_layers_use_workspace_project_prompt_configuration() {
        let root = unique_temp_dir("vulcan-project-prompt-config");
        let prompt_dir = root.join(".vulcan").join("code");
        fs::create_dir_all(&prompt_dir).expect("create project prompt directory");
        fs::write(
            prompt_dir.join("custom-prompt.md"),
            "Project override instruction",
        )
        .expect("write project override prompt");
        fs::write(
            prompt_dir.join("config.json"),
            r#"{"prompt":{"projectPromptEnabled":true,"projectPromptPath":".vulcan/code/custom-prompt.md"}}"#,
        )
        .expect("write project prompt config");

        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration {
                project_prompt_enabled: false,
                ..PersistedPromptConfiguration::default()
            },
        );
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .provider_layers
                .runtime_context
                .contains("Project override instruction")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies malformed project configuration aborts prompt assembly before provider launch.
    /// 验证格式错误的项目配置会在 Provider 启动前中止提示装配。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if invalid JSON falls back to global prompt defaults.
    /// 此测试无返回值；若无效 JSON 降级为全局提示默认值则失败。
    #[test]
    fn runtime_prompt_layers_propagate_project_configuration_parse_failure() {
        // root isolates the malformed project configuration from other runtime tests.
        // root 将格式错误的项目配置与其他运行时测试隔离。
        let root = unique_temp_dir("vulcan-project-prompt-invalid-config");
        // configuration_path is the exact project configuration consumed by the shared loader.
        // configuration_path 是共享加载器消费的精确项目配置路径。
        let configuration_path = root.join(".vulcan").join("code").join("config.json");
        fs::create_dir_all(
            configuration_path
                .parent()
                .expect("project configuration should have a parent"),
        )
        .expect("create project configuration directory");
        fs::write(&configuration_path, b"{invalid-json")
            .expect("write invalid project configuration");
        // preparation binds prompt assembly to the malformed workspace configuration.
        // preparation 将提示装配绑定到格式错误的工作区配置。
        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration::default(),
        );

        // error must retain both the source category and workspace identity.
        // error 必须同时保留来源类别与工作区身份。
        let error = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect_err("invalid project configuration should abort prompt assembly");

        assert!(error.contains("project prompt configuration"));
        assert!(error.contains(&configuration_path.display().to_string()));
        fs::remove_dir_all(root).expect("remove invalid-config workspace");
    }

    /// Verifies unreadable configured project-prompt content aborts prompt assembly.
    /// 验证无法读取的已配置项目提示词内容会中止提示装配。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if invalid UTF-8 prompt content is treated as absent.
    /// 此测试无返回值；若无效 UTF-8 提示词内容被视为不存在则失败。
    #[test]
    fn runtime_prompt_layers_propagate_project_prompt_read_failure() {
        // root isolates the invalid project-prompt source from every other cache entry.
        // root 将无效项目提示词来源与其他所有缓存条目隔离。
        let root = unique_temp_dir("vulcan-project-prompt-invalid-utf8");
        // prompt_path is the default enabled project-prompt source selected by the test preparation.
        // prompt_path 是测试执行准备选择的默认已启用项目提示词来源。
        let prompt_path = root.join(".vulcan").join("code").join("prompt.md");
        fs::create_dir_all(
            prompt_path
                .parent()
                .expect("project prompt should have a parent"),
        )
        .expect("create project prompt directory");
        fs::write(&prompt_path, [0xff, 0xfe, 0xfd]).expect("write invalid project prompt");
        // preparation enables the default project prompt path for this workspace.
        // preparation 为当前工作区启用默认项目提示词路径。
        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration::default(),
        );

        // error must retain both the source category and exact file path.
        // error 必须同时保留来源类别与精确文件路径。
        let error = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect_err("invalid project prompt should abort prompt assembly");

        assert!(error.contains("workspace project prompt"));
        assert!(error.contains(&prompt_path.display().to_string()));
        fs::remove_dir_all(root).expect("remove invalid-prompt workspace");
    }

    /// Verifies workspace-local prompt configuration does not override the global personality choice.
    /// 验证工作区本地提示词配置不会覆盖全局人格选择。
    #[test]
    fn runtime_prompt_layers_keep_global_personality_when_project_prompt_overrides() {
        let root = unique_temp_dir("vulcan-project-prompt-personality");
        let prompt_dir = root.join(".vulcan").join("code");
        fs::create_dir_all(&prompt_dir).expect("create project prompt directory");
        fs::write(prompt_dir.join("prompt.md"), "Project prompt").expect("write project prompt");
        let project_config = ProjectVulcanCodeConfigurationFile {
            prompt_skills: Default::default(),
            prompt: Some(PersistedPromptConfiguration {
                personality: PersistedPromptPersonalitySelection::Default,
                output_style: Default::default(),
                project_prompt_enabled: true,
                project_prompt_path: String::from(".vulcan/code/prompt.md"),
            }),
            vmm: Default::default(),
        };
        save_project_configuration_file(&root, &project_config)
            .expect("write project prompt config");

        let preparation = build_test_preparation(
            root.display().to_string(),
            PersistedPromptConfiguration {
                personality: PersistedPromptPersonalitySelection::Disabled,
                ..PersistedPromptConfiguration::default()
            },
        );
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert_eq!(
            layers
                .provider_layers
                .system
                .matches("You optimize for team morale")
                .count(),
            0
        );
        assert!(
            layers
                .provider_layers
                .runtime_context
                .contains("Project prompt")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies persisted personality settings can disable personality injection.
    /// 验证持久化人格设置可以禁用人格注入。
    #[test]
    fn runtime_prompt_layers_respect_disabled_personality_setting() {
        let prompt_configuration = PersistedPromptConfiguration {
            personality: PersistedPromptPersonalitySelection::Disabled,
            ..PersistedPromptConfiguration::default()
        };
        let preparation = build_test_preparation(String::from("D:/missing"), prompt_configuration);
        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert_eq!(
            layers
                .provider_layers
                .system
                .matches("You optimize for team morale")
                .count(),
            0
        );
    }

    /// Verifies request-scoped agent prompt snapshots enter the shared runtime prompt composer.
    /// 验证请求级代理提示词快照会进入共享运行时提示词组合器。
    #[test]
    fn runtime_prompt_layers_use_request_scoped_agent_prompt_snapshot() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.agent_prompt_snapshot = ExecutionAgentPromptSnapshot::fixed_parent_runtime(
            "Task-specific reviewer instructions.",
        );

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .fallback_text
                .contains("Task-specific reviewer instructions.")
        );
    }

    /// Verifies active session goals never enter the global provider prompt layers.
    /// 验证活动会话目标绝不会进入 Provider 全局提示词层。
    #[test]
    fn runtime_prompt_layers_exclude_session_goal_context() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.session_goal_context = Some(
            crate::agent_runtime::execution_models::ExecutionSessionGoalContext {
                goal_id: String::from("goal-cache-stable"),
                prompt_context: String::from("GOAL_CONTEXT_MUST_STAY_OUT_OF_PREFIX"),
            },
        );

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            !layers
                .fallback_text
                .contains("GOAL_CONTEXT_MUST_STAY_OUT_OF_PREFIX")
        );
        assert!(
            !layers
                .provider_layers
                .runtime_context
                .contains("GOAL_CONTEXT_MUST_STAY_OUT_OF_PREFIX")
        );
    }

    /// Verifies child-agent prompt context enters the shared prompt composer directly.
    /// 验证子智能体提示词上下文会直接进入共享提示词组合器。
    #[test]
    fn runtime_prompt_layers_use_subagent_prompt_context() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.subagent_prompt_context = Some(ExecutionSubagentPromptContext {
            identity: String::from("reviewer"),
            delegated_task: String::from("Review code"),
            delegation: Some(String::from("Return findings to the parent.")),
            functional_system_prompt: None,
        });

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .provider_layers
                .developer
                .contains("Subagent identity: reviewer")
        );
        assert!(
            layers
                .provider_layers
                .developer
                .contains("Delegated task: Review code")
        );
        assert!(!layers.provider_layers.system.contains("Subagent identity"));
    }

    /// Verifies functional child-agent prompts use the functional subagent strategy.
    /// 验证功能型子智能体提示词会使用功能型子智能体策略。
    #[test]
    fn runtime_prompt_layers_use_functional_subagent_prompt() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.subagent_prompt_context = Some(ExecutionSubagentPromptContext {
            identity: String::from("custom-reviewer"),
            delegated_task: String::from("Review code"),
            delegation: Some(String::from("Return findings to the parent.")),
            functional_system_prompt: Some(String::from("You are a strict reviewer.")),
        });

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .provider_layers
                .system
                .contains("You are a strict reviewer.")
        );
        assert!(
            layers
                .provider_layers
                .developer
                .contains("Subagent identity: custom-reviewer")
        );
    }

    /// Verifies tool registration help reaches the runtime tool-guidance layer.
    /// 验证工具注册说明会进入运行时工具指导层。
    #[test]
    fn runtime_prompt_layers_include_registered_tool_help_text() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.tool_descriptors = vec![vulcan_tools_manager::VulcanToolDescriptor {
            id: String::from("custom_tool"),
            name: String::from("custom_tool"),
            description: String::from("Custom test tool"),
            help_text: Some(String::from("Use category 5 for project context.")),
            source: vulcan_tools_manager::VulcanToolSourceKind::Native,
        }];

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .provider_layers
                .tool_guidance
                .contains("Use category 5 for project context.")
        );
    }

    /// Verifies request-scoped system hook blocks are appended to the system layer.
    /// 验证请求级系统 hook 块会追加到系统层。
    #[test]
    fn runtime_prompt_layers_append_system_prompt_hook_region() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.system_prompt_hook_blocks = vec![SystemPromptHookBlock {
            tag: String::from("vmm_profile"),
            description: String::from("Profile details"),
            content: String::from("User prefers precise answers."),
            source: Some(String::from("test")),
            phase: vulcan_hooks::SystemPromptHookPhase::FirstSession,
            baseline_provenance: None,
        }];

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .provider_layers
                .system
                .contains("<system_prompt_hooks>")
        );
        assert!(layers.provider_layers.system.contains("<vmm_profile>"));
        assert!(
            layers
                .provider_layers
                .system
                .contains("User prefers precise answers.")
        );
        assert!(layers.fallback_text.contains("<system_prompt_hooks>"));
    }

    /// Verifies locked-baseline projection trusts only producer-owned typed provenance.
    /// 验证锁定基线投影只信任生产者持有的强类型来源信息。
    #[test]
    fn runtime_locked_baseline_does_not_infer_from_titles_or_hook_tags() {
        let mut output = PromptAssemblyOutput::default();
        output.system_segments = vec![
            PromptSegment::new(
                "dynamic-lookalike",
                "dynamic-cache",
                vulcan_prompt_composer::PromptNodeKind::System,
                PromptTarget::System,
                "AGENTS.md",
                "same-looking body",
            )
            .with_source_label("dynamic AGENTS.md lookalike"),
            PromptSegment::new(
                "authoritative-agents",
                "locked-cache",
                vulcan_prompt_composer::PromptNodeKind::System,
                PromptTarget::System,
                "AGENTS.md",
                "same-looking body",
            )
            .with_source_label("D:/repo/AGENTS.md")
            .with_baseline_provenance(PromptBaselineProvenance::new(
                PromptBaselineKind::AgentsMd,
                "D:/repo/AGENTS.md",
            )),
        ];
        let hooks = vec![
            SystemPromptHookBlock::new("vmm_profile", "dynamic lookalike"),
            SystemPromptHookBlock {
                tag: String::from("vmm_profile"),
                description: String::from("Profile details"),
                content: String::from("authoritative profile body"),
                source: Some(String::from("VMM profile full")),
                phase: vulcan_hooks::SystemPromptHookPhase::FirstSession,
                baseline_provenance: Some(vulcan_hooks::SystemPromptHookBaselineProvenance::new(
                    SystemPromptHookBaselineKind::MemoryProfile,
                    "hook.vmm-memory-profile",
                    "builtin.vmm.profile_prepare",
                )),
            },
        ];

        let projection = project_locked_runtime_baseline(&output, &hooks)
            .expect("typed provenance should project");
        let sources = projection.locked_sources();
        let dynamic_layers = render_request_dynamic_prompt_layers(&output, &hooks);

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].node_id, "authoritative-agents");
        assert_eq!(sources[0].kind, LockedSourceKind::AgentsMd);
        assert_eq!(sources[1].node_id, "hook.vmm-memory-profile");
        assert_eq!(sources[1].kind, LockedSourceKind::MemoryProfile);
        assert!(sources[1].content.contains("authoritative profile body"));
        assert_eq!(
            dynamic_layers.system.matches("same-looking body").count(),
            1,
            "the locked lookalike must not be duplicated into dynamic layers"
        );
        assert!(dynamic_layers.system.contains("dynamic lookalike"));
        assert!(!dynamic_layers.system.contains("authoritative profile body"));
    }

    /// Verifies context-compaction prompts use the prompt composer instead of a post-launch system suffix.
    /// 验证上下文压缩提示词会使用提示词组合器，而不是启动后追加系统提示词后缀。
    #[test]
    fn runtime_prompt_layers_for_context_compaction_use_developer_layer() {
        let mut preparation = build_test_preparation(
            String::from("D:/missing"),
            PersistedPromptConfiguration::default(),
        );
        preparation.prompt_purpose = ExecutionPromptPurpose::ContextCompaction;
        preparation.tool_descriptors = vec![vulcan_tools_manager::VulcanToolDescriptor {
            id: String::from("shell_command"),
            name: String::from("shell_command"),
            description: String::from("Run one shell command"),
            help_text: None,
            source: vulcan_tools_manager::VulcanToolSourceKind::Native,
        }];

        let layers = build_runtime_prompt_layers_for_agent(&preparation, None)
            .expect("runtime prompt layers should build");

        assert!(
            layers
                .provider_layers
                .developer
                .contains("Internal host task: summarize earlier conversation context")
        );
        assert!(
            !layers
                .provider_layers
                .tool_guidance
                .contains("shell_command")
        );
        assert!(layers.provider_layers.system.trim().is_empty());
    }

    /// Builds one minimal execution preparation for prompt-layer tests.
    /// 为提示词层测试构建一份最小执行准备。
    fn build_test_preparation(
        workspace_path: String,
        prompt_configuration: PersistedPromptConfiguration,
    ) -> ExecutionPreparation {
        ExecutionPreparation {
            session_id: SessionId::nil(),
            session_storage_directory: None,
            conversation_hook_turn_id: None,
            continuation_action_id: None,
            session_title: String::from("test"),
            agent_id: String::from("default"),
            agent_name: String::from("Default"),
            agent_kind: String::from("Primary"),
            agent_prompt_snapshot: None,
            subagent_prompt_context: None,
            model_blueprint: sample_model_blueprint(),
            prompt_preview: String::new(),
            workspace_path,
            conversation_attachments: Vec::new(),
            context_history_count: 0,
            execution_mode: String::from("Build"),
            temperature: None,
            prompt_configuration,
            output_style_snapshot: None,
            prompt_purpose: ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            implicit_luaskill_prompt_entries: Vec::new(),
            additional_developer_segments: Vec::new(),
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            provider_wire_request_json: None,
            context_budget: Option::<ContextBudgetSnapshot>::None,
            context_compaction_policy: None,
            context_budget_override: None,
            context_assembly: Option::<ContextAssembly>::None,
            context_baseline: Option::<ContextBaseline>::None,
            context_request_mode: None,
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            vmm_runtime_ready: false,
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
        }
    }

    /// Builds one real provider blueprint through the catalog resolver.
    /// 通过目录解析器构建一份真实供应商蓝图。
    fn sample_model_blueprint() -> crate::agent_runtime::provider::ProviderRuntimeBlueprint {
        crate::agent_runtime::provider::ProviderRuntimeBlueprint {
            provider_id: String::from("openrouter"),
            service_entry_id: String::from("public-api"),
            connection_id: String::from("test-connection"),
            provider_name: String::from("OpenRouter"),
            runtime_label: String::from("Vulcan Public AI Contract"),
            model_id: String::from("vendor/model"),
            model_name: String::from("Model"),
            protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID),
            manifest_revision: 0,
            frozen_selection: None,
            route_revision: String::from("sha256:test-route"),
            secondary_route: Default::default(),
            public_capabilities: vulcan_ai_contract::ModelCapabilities::default(),
            base_url: Some(String::from("https://openrouter.ai/api/v1")),
            supports_remote_model_refresh: true,
            credential_source: String::from("Account A"),
            reasoning:
                crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution {
                    requested_effort: ReasoningEffortLevel::Off,
                    source_label: String::from("public contract"),
                    note: None,
                },
        }
    }

    /// Builds one unique temporary directory for tests.
    /// 为测试构建一个唯一临时目录。
    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{suffix}"))
    }
}
