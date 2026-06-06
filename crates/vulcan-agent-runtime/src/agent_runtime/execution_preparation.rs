//! Execution-preparation builder for the compose runtime foundation.
//! compose 运行时基础架构使用的执行准备构建模块。

use crate::agent_runtime::execution_models::{
    ExecutionPreparation, ExecutionPreparationRequest, ExecutionPromptPurpose,
};
use crate::agent_runtime::provider::resolve_active_provider_blueprint_with_ai_tools;
use crate::agent_runtime::provider_stream_projection::truncate_preview;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::conversation_protocol::collect_host_visible_attachments;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeToolBinding;
use crate::provider_config::{AiToolKind, PersistedAiToolConfiguration, ProviderConfigurationFile};
use crate::vulcan_tools_bridge::{build_vulcan_tools_manager, resolve_ai_tool_fallback_aliases};
use crate::workspace_path::normalize_workspace_path_text;
use luaskills::RuntimeEntryDescriptor;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use vulcan_luaskills_prompt::{LuaSkillPromptEntry, render_luaskills_prompt_segment};
use vulcan_prompt_skills::{
    PromptSkillsProjectConfig, PromptSkillsSessionConfig, render_prompt_skills_instructions,
    resolve_prompt_skills,
};
use vulcan_session_core::{
    ToolExposureSnapshot, normalized_tool_selector_set, tool_selector_matches,
};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{ResolvedToolDefinition, ResolvedToolSet, VulcanToolSourceKind};

use vulcan_luaskills_prompt::LUASKILL_HELP_TOOL_NAME;

/// All AI capability kinds that can be selected by request-scoped tool exposure.
/// 可被请求级工具可见性选择的全部 AI 能力类型。
const ALL_AI_TOOL_KINDS: [AiToolKind; 11] = [
    AiToolKind::WebSearch,
    AiToolKind::UnderstandImage,
    AiToolKind::FileSearch,
    AiToolKind::CodeInterpreter,
    AiToolKind::ImageGeneration,
    AiToolKind::ComputerUse,
    AiToolKind::VideoGeneration,
    AiToolKind::SpeechSynthesis,
    AiToolKind::MusicGeneration,
    AiToolKind::MusicCover,
    AiToolKind::AudioGeneration,
];

/// Request-effective tool exposure selected before building the unified tools manager.
/// 构建统一工具管理器之前选定的请求级生效工具可见性。
#[derive(Clone)]
pub(crate) struct ExecutionToolExposureSelection {
    /// Native Rust tool identifiers visible to the current request.
    /// 当前请求可见的 Rust 原生工具标识集合。
    pub native_tool_ids: Vec<String>,
    /// Direct LuaSkills function tools visible to the current request.
    /// 当前请求可见的直接 LuaSkills 函数工具集合。
    pub luaskills_tools: Vec<RuntimeEntryDescriptor>,
    /// Implicit LuaSkills dispatcher entries visible to the current request.
    /// 当前请求可见的隐式 LuaSkills 调度入口集合。
    pub luaskills_dispatch_tools: Vec<RuntimeEntryDescriptor>,
    /// Prompt entries paired with implicit LuaSkills dispatcher calls.
    /// 与隐式 LuaSkills 调度调用配套的提示词入口集合。
    pub luaskill_prompt_entries: Vec<LuaSkillPromptEntry>,
    /// MCP tool bindings visible to the current request.
    /// 当前请求可见的 MCP 工具绑定集合。
    pub mcp_tools: Vec<McpRuntimeToolBinding>,
    /// AI capability configuration after request-scoped allow and deny overrides.
    /// 应用请求级允许与禁用覆盖后的 AI 能力配置。
    pub ai_tool_configuration: PersistedAiToolConfiguration,
    /// Normalized selector state used for final public tool filtering.
    /// 用于最终公开工具过滤的规范化选择器状态。
    selectors: ToolExposureSelectorState,
}

/// Builds one validated execution-preparation object from the current runtime foundation and GUI request.
/// 根据当前运行时基础架构与 GUI 请求构建一份通过校验的执行准备对象。
pub(crate) fn prepare_execution(
    foundation: &RuntimeFoundation,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    config: &ProviderConfigurationFile,
    request: &ExecutionPreparationRequest,
) -> Result<ExecutionPreparation, String> {
    let active_agent = match request.execution_agent_id.as_deref() {
        Some(agent_id) => foundation
            .agent_definition(agent_id)
            .ok_or_else(|| format!("执行代理 `{agent_id}` 未能在运行时基础架构中解析到。"))?,
        None => foundation
            .active_agent_definition()
            .ok_or_else(|| String::from("当前激活代理未能在运行时基础架构中解析到。"))?,
    };

    if !foundation.has_built_agent(&active_agent.id) {
        return Err(format!(
            "代理 `{}` 当前尚未成功构建，无法启动执行。",
            active_agent.name
        ));
    }

    let tool_exposure = resolve_execution_tool_exposure(foundation, &active_agent.id, request);
    let model_blueprint = resolve_active_provider_blueprint_with_ai_tools(
        config,
        request.reasoning_effort,
        &tool_exposure.ai_tool_configuration,
    )?;
    let mut warnings = Vec::new();
    let workspace_path = normalize_workspace_path_text(&request.workspace_path);
    let conversation_attachments = collect_host_visible_attachments(&request.conversation_state);
    let prompt_skills_global_config =
        crate::skills::prompt::global_config_from_persisted(&config.prompt_skills);
    let prompt_skill_user_root = Some(vulcan_prompt_skills::configured_user_skill_root(
        &prompt_skills_global_config,
    ));
    let prompt_skill_catalog =
        vulcan_prompt_skills::load_prompt_skills(&prompt_skills_global_config);
    for error in &prompt_skill_catalog.errors {
        warnings.push(format!(
            "提示词技能 `{}` 载入失败：{}",
            error.path.display(),
            error.message
        ));
    }
    let prompt_skills = resolve_prompt_skills(
        &prompt_skill_catalog,
        &prompt_skills_global_config,
        &prompt_skills_project_config_from_request(request),
        &prompt_skills_session_config_from_request(request),
    );
    let implicit_luaskills = tool_exposure.luaskills_dispatch_tools.clone();
    let implicit_luaskill_prompt_entries = tool_exposure.luaskill_prompt_entries.clone();
    let prompt_skill_developer_segment = build_skill_developer_segment(
        request.prompt_purpose,
        &prompt_skills,
        &implicit_luaskill_prompt_entries,
    );
    let enabled_prompt_skill_names = vulcan_prompt_skills::enabled_skill_names(&prompt_skills);
    let ai_tool_fallback_aliases = resolve_ai_tool_fallback_aliases(&model_blueprint);
    let project_vmm = if request.runtime_policy.vmm_enabled {
        request.project_vmm.clone()
    } else {
        Default::default()
    };
    let session_vmm = if request.runtime_policy.vmm_enabled {
        request.session_vmm.clone()
    } else {
        Default::default()
    };
    let system_prompt_hook_blocks = if request.runtime_policy.hooks_enabled {
        request.system_prompt_hook_blocks.clone()
    } else {
        Vec::new()
    };
    let tools_manager = build_vulcan_tools_manager(
        foundation.task_runtime(),
        foundation.question_runtime(),
        foundation.native_tool_catalog().clone(),
        luaskills_runtime,
        (!workspace_path.is_empty()).then(|| PathBuf::from(workspace_path.clone())),
        conversation_attachments.clone(),
        tool_exposure.native_tool_ids.clone(),
        tool_exposure.luaskills_tools.clone(),
        implicit_luaskills,
        tool_exposure.mcp_tools.clone(),
        tool_exposure.ai_tool_configuration.clone(),
        ai_tool_fallback_aliases.clone(),
        enabled_prompt_skill_names,
        prompt_skill_user_root.clone(),
        implicit_luaskill_prompt_entries.clone(),
        project_vmm.clone(),
        session_vmm.clone(),
    );
    let mut resolved_tools = tools_manager.resolve_for_execution_mode(&request.execution_mode);
    filter_resolved_tool_set_by_tool_exposure(&mut resolved_tools, &tool_exposure);
    let prompt_skill_tool_available = request.prompt_purpose.supports_user_turn_tooling()
        && render_prompt_skills_instructions(&prompt_skills).is_some();
    if !prompt_skill_tool_available {
        remove_resolved_tool(&mut resolved_tools, "skill");
    }
    if request.prompt_purpose.disables_provider_tools() {
        if !resolved_tools.descriptors.is_empty() {
            warnings.push(format!(
                "内部 `{}` 执行不允许工具调用，本轮已隐藏 {} 个工具。",
                request.prompt_purpose.diagnostics_label(),
                resolved_tools.descriptors.len()
            ));
        }
        resolved_tools.descriptors.clear();
        resolved_tools.definitions.clear();
    }

    if workspace_path.is_empty() {
        warnings.push(String::from(
            "当前工作区路径为空，后续原生工具可能无法提供完整上下文。",
        ));
    }

    if request.prompt.trim().is_empty() {
        warnings.push(String::from(
            "当前提示词为空，本次准备计划仅验证运行时链路。",
        ));
    }

    if let Some(reasoning_note) = model_blueprint.reasoning.note.as_ref() {
        warnings.push(reasoning_note.clone());
    }

    push_missing_ai_tool_fallback_warnings(
        &mut warnings,
        &ai_tool_fallback_aliases,
        &resolved_tools,
    );

    if !model_blueprint.supports_tool_call() && !resolved_tools.descriptors.is_empty() {
        warnings.push(format!(
            "当前模型 `{}` 未声明工具调用能力，本轮已自动隐藏 {} 个工具。",
            model_blueprint.model_name,
            resolved_tools.descriptors.len()
        ));
        resolved_tools.descriptors.clear();
        resolved_tools.definitions.clear();
    }

    let temperature = if model_blueprint.supports_temperature() {
        request.temperature
    } else {
        if request.temperature.is_some() {
            warnings.push(format!(
                "当前模型 `{}` 未声明显式温度能力，本轮将自动回退到模型默认采样设置。",
                model_blueprint.model_name
            ));
        }
        None
    };

    let mcp_overview = foundation.mcp_overview();

    Ok(ExecutionPreparation {
        session_id: request.session_id,
        session_title: request.session_title.clone(),
        agent_id: active_agent.id.clone(),
        agent_name: active_agent.name.clone(),
        agent_kind: active_agent.kind.label().to_string(),
        agent_system_prompt_override: request.agent_system_prompt_override.clone(),
        model_blueprint,
        prompt_preview: truncate_preview(request.prompt.trim(), 180),
        workspace_path,
        task_session_store_root: request.task_session_store_root.clone(),
        conversation_attachments,
        context_history_count: request.conversation_state.messages.len(),
        execution_mode: request.execution_mode.clone(),
        temperature,
        prompt_configuration: config.prompt.clone(),
        prompt_purpose: request.prompt_purpose,
        prompt_skills,
        prompt_skill_user_root,
        implicit_luaskill_prompt_entries,
        prompt_skill_developer_segment,
        skill_names: foundation.skill_names_for_agent(&active_agent.id),
        tool_descriptors: resolved_tools.descriptors,
        delegate_agents: active_agent.delegates.clone(),
        enabled_mcp_servers: mcp_overview.active_summary_rows().to_vec(),
        warnings,
        provider_request_preview_markdown: String::new(),
        provider_request_snapshot_json: None,
        provider_wire_request_json: None,
        context_budget: None,
        context_assembly: None,
        context_baseline: None,
        context_request_mode: None,
        context_injection: request.context_injection.clone(),
        system_prompt_hook_blocks,
        project_vmm,
        session_vmm,
    })
}

/// Resolves all request-scoped tool exposure inputs through one shared preparation path.
/// 通过同一条共享准备路径解析全部请求级工具可见性输入。
pub(crate) fn resolve_execution_tool_exposure(
    foundation: &RuntimeFoundation,
    agent_id: &str,
    request: &ExecutionPreparationRequest,
) -> ExecutionToolExposureSelection {
    let selectors = ToolExposureSelectorState::from_snapshot(&request.tool_exposure_override);
    let native_tool_ids = filter_native_tool_ids(
        foundation.resolved_native_tools_for_agent(agent_id),
        &selectors,
    );
    let luaskills_tools = filter_luaskills_entries(
        foundation.resolved_luaskills_tools_for_agent(agent_id),
        &selectors,
    );
    let luaskills_dispatch_tools = filter_luaskills_entries(
        foundation.resolved_luaskills_dispatch_tools_for_agent(agent_id),
        &selectors,
    );
    let luaskill_prompt_entries = filter_luaskill_prompt_entries(
        foundation.resolved_luaskills_prompt_entries_for_agent(agent_id),
        &selectors,
    );
    let mcp_tools = filter_mcp_tool_bindings(
        foundation.resolved_mcp_tools_for_agent(agent_id),
        &selectors,
    );
    let ai_tool_configuration =
        ai_tool_configuration_for_execution_request(&request.ai_tool_configuration, &selectors);

    ExecutionToolExposureSelection {
        native_tool_ids,
        luaskills_tools,
        luaskills_dispatch_tools,
        luaskill_prompt_entries,
        mcp_tools,
        ai_tool_configuration,
        selectors,
    }
}

/// Applies the same final public-tool selector gate used by preparation and bootstrap.
/// 应用准备阶段与启动阶段共用的最终公开工具选择器闸门。
pub(crate) fn filter_resolved_tool_definitions_by_tool_exposure(
    definitions: &mut Vec<ResolvedToolDefinition>,
    tool_exposure: &ExecutionToolExposureSelection,
) {
    definitions.retain(|definition| {
        resolved_tool_definition_is_visible(definition, &tool_exposure.selectors)
    });
}

/// Applies request-scoped public-tool selectors to descriptors and definitions together.
/// 将请求级公开工具选择器同时应用到工具描述与定义。
fn filter_resolved_tool_set_by_tool_exposure(
    resolved_tools: &mut ResolvedToolSet,
    tool_exposure: &ExecutionToolExposureSelection,
) {
    resolved_tools.descriptors.retain(|descriptor| {
        vulcan_tool_descriptor_is_visible(
            descriptor.id.as_str(),
            descriptor.name.as_str(),
            descriptor.description.as_str(),
            descriptor.source,
            &tool_exposure.selectors,
        )
    });
    filter_resolved_tool_definitions_by_tool_exposure(
        &mut resolved_tools.definitions,
        tool_exposure,
    );
}

/// Builds an effective AI-tool configuration after capability-level request overrides.
/// 根据能力级请求覆盖构建生效 AI 工具配置。
fn ai_tool_configuration_for_execution_request(
    base: &PersistedAiToolConfiguration,
    selectors: &ToolExposureSelectorState,
) -> PersistedAiToolConfiguration {
    let mut configuration = base.clone();
    if !selectors.ai_allowed_raw.is_empty() {
        for kind in ALL_AI_TOOL_KINDS {
            if selectors.ai_kind_matches_allowed(kind) {
                configuration.disabled_capability_kinds.remove(&kind);
            } else {
                configuration.disabled_capability_kinds.insert(kind);
            }
        }
    }
    for kind in ALL_AI_TOOL_KINDS {
        if selectors.ai_kind_matches_disabled(kind) {
            configuration.disabled_capability_kinds.insert(kind);
        }
    }
    configuration
}

/// Normalized request-scoped selectors split by tool family.
/// 按工具族拆分的规范化请求级选择器。
#[derive(Clone, Debug, Default)]
struct ToolExposureSelectorState {
    /// Global public tool allow selectors.
    /// 全局公开工具允许选择器。
    global_allowed: BTreeSet<String>,
    /// Global public tool deny selectors.
    /// 全局公开工具禁用选择器。
    global_disabled: BTreeSet<String>,
    /// LuaSkills-specific allow selectors.
    /// LuaSkills 专属允许选择器。
    luaskills_allowed: BTreeSet<String>,
    /// LuaSkills-specific deny selectors.
    /// LuaSkills 专属禁用选择器。
    luaskills_disabled: BTreeSet<String>,
    /// MCP tool-specific allow selectors.
    /// MCP 工具专属允许选择器。
    mcp_tool_allowed: BTreeSet<String>,
    /// MCP tool-specific deny selectors.
    /// MCP 工具专属禁用选择器。
    mcp_tool_disabled: BTreeSet<String>,
    /// MCP server-specific allow selectors.
    /// MCP 服务专属允许选择器。
    mcp_server_allowed: BTreeSet<String>,
    /// MCP server-specific deny selectors.
    /// MCP 服务专属禁用选择器。
    mcp_server_disabled: BTreeSet<String>,
    /// Raw AI capability allow selectors used for semantic-key matching.
    /// 用于语义键匹配的原始 AI 能力允许选择器。
    ai_allowed_raw: Vec<String>,
    /// Raw AI capability deny selectors used for semantic-key matching.
    /// 用于语义键匹配的原始 AI 能力禁用选择器。
    ai_disabled_raw: Vec<String>,
}

impl ToolExposureSelectorState {
    /// Builds normalized selector state from one persisted exposure snapshot.
    /// 从一份持久化可见性快照构建规范化选择器状态。
    fn from_snapshot(snapshot: &ToolExposureSnapshot) -> Self {
        let luaskills_override = lua_selector_override_from_value(&snapshot.luaskills_overrides);
        let mcp_override = mcp_selector_override_from_value(&snapshot.mcp_server_overrides);
        Self {
            global_allowed: normalized_tool_selector_set(&snapshot.allowed_tools),
            global_disabled: normalized_tool_selector_set(&snapshot.disabled_tools),
            luaskills_allowed: normalized_tool_selector_set(&luaskills_override.allowed),
            luaskills_disabled: normalized_tool_selector_set(&luaskills_override.disabled),
            mcp_tool_allowed: normalized_tool_selector_set(&mcp_override.allowed),
            mcp_tool_disabled: normalized_tool_selector_set(&mcp_override.disabled),
            mcp_server_allowed: normalized_tool_selector_set(&mcp_override.allowed_servers),
            mcp_server_disabled: normalized_tool_selector_set(&mcp_override.disabled_servers),
            ai_allowed_raw: snapshot.allowed_ai_capabilities.clone(),
            ai_disabled_raw: snapshot.disabled_ai_capabilities.clone(),
        }
    }

    /// Returns whether any AI allow selector matches the semantic kind.
    /// 返回是否存在 AI 允许选择器命中该语义能力类型。
    fn ai_kind_matches_allowed(&self, kind: AiToolKind) -> bool {
        ai_tool_kind_matches_any_selector(kind, &self.ai_allowed_raw)
    }

    /// Returns whether any AI deny selector matches the semantic kind.
    /// 返回是否存在 AI 禁用选择器命中该语义能力类型。
    fn ai_kind_matches_disabled(&self, kind: AiToolKind) -> bool {
        ai_tool_kind_matches_any_selector(kind, &self.ai_disabled_raw)
    }

    /// Returns whether LuaSkills-specific allow selectors are active for this request.
    /// 返回当前请求是否启用了 LuaSkills 专属允许选择器。
    fn has_luaskills_allow(&self) -> bool {
        !self.luaskills_allowed.is_empty()
    }

    /// Returns whether MCP-specific allow selectors are active for this request.
    /// 返回当前请求是否启用了 MCP 专属允许选择器。
    fn has_mcp_allow(&self) -> bool {
        !self.mcp_tool_allowed.is_empty() || !self.mcp_server_allowed.is_empty()
    }

    /// Returns whether AI-specific allow selectors are active for this request.
    /// 返回当前请求是否启用了 AI 专属允许选择器。
    fn has_ai_allow(&self) -> bool {
        !self.ai_allowed_raw.is_empty()
    }
}

/// Flexible allow and deny selector payload extracted from JSON overrides.
/// 从 JSON 覆盖中提取出的灵活允许与禁用选择器载荷。
#[derive(Default)]
struct SelectorOverride {
    /// Tool or entry allow selectors.
    /// 工具或入口允许选择器。
    allowed: Vec<String>,
    /// Tool or entry deny selectors.
    /// 工具或入口禁用选择器。
    disabled: Vec<String>,
    /// MCP server allow selectors.
    /// MCP 服务允许选择器。
    allowed_servers: Vec<String>,
    /// MCP server deny selectors.
    /// MCP 服务禁用选择器。
    disabled_servers: Vec<String>,
}

/// Extracts LuaSkills selector overrides from a flexible JSON payload.
/// 从灵活 JSON 载荷中提取 LuaSkills 选择器覆盖。
fn lua_selector_override_from_value(value: &Value) -> SelectorOverride {
    let mut selectors = SelectorOverride::default();
    collect_json_selectors(
        value,
        &[
            "allow",
            "allowed",
            "enabled",
            "include",
            "tools",
            "entries",
            "skills",
            "luaskills",
        ],
        &["deny", "denied", "disabled", "disable", "exclude"],
        &mut selectors.allowed,
        &mut selectors.disabled,
    );
    selectors
}

/// Extracts MCP tool and server selector overrides from a flexible JSON payload.
/// 从灵活 JSON 载荷中提取 MCP 工具与服务选择器覆盖。
fn mcp_selector_override_from_value(value: &Value) -> SelectorOverride {
    let mut selectors = SelectorOverride::default();
    match value {
        Value::String(_) | Value::Array(_) => {
            append_json_string_values(&mut selectors.allowed_servers, value)
        }
        Value::Object(map) => {
            for key in [
                "allow",
                "allowed",
                "enabled",
                "include",
                "tools",
                "mcp_tools",
            ] {
                if let Some(value) = map.get(key) {
                    append_json_string_values(&mut selectors.allowed, value);
                }
            }
            for key in ["deny", "denied", "disabled", "disable", "exclude"] {
                if let Some(value) = map.get(key) {
                    append_json_string_values(&mut selectors.disabled, value);
                }
            }
            for key in ["servers", "server", "server_ids", "mcp_servers"] {
                if let Some(value) = map.get(key) {
                    append_json_string_values(&mut selectors.allowed_servers, value);
                }
            }
            for key in ["disabled_servers", "deny_servers", "exclude_servers"] {
                if let Some(value) = map.get(key) {
                    append_json_string_values(&mut selectors.disabled_servers, value);
                }
            }
        }
        _ => {}
    }
    selectors
}

/// Collects generic tool selectors from a flexible JSON object, string, or array.
/// 从灵活 JSON 对象、字符串或数组中收集通用工具选择器。
fn collect_json_selectors(
    value: &Value,
    allowed_keys: &[&str],
    disabled_keys: &[&str],
    allowed: &mut Vec<String>,
    disabled: &mut Vec<String>,
) {
    match value {
        Value::String(_) | Value::Array(_) => append_json_string_values(allowed, value),
        Value::Object(map) => {
            for key in allowed_keys {
                if let Some(value) = map.get(*key) {
                    append_json_string_values(allowed, value);
                }
            }
            for key in disabled_keys {
                if let Some(value) = map.get(*key) {
                    append_json_string_values(disabled, value);
                }
            }
        }
        _ => {}
    }
}

/// Appends string or string-array selector values while trimming empty entries.
/// 追加字符串或字符串数组选择器值，同时裁剪空白条目。
fn append_json_string_values(target: &mut Vec<String>, value: &Value) {
    match value {
        Value::String(text) => push_trimmed_selector(target, text),
        Value::Array(items) => {
            for item in items {
                if let Value::String(text) = item {
                    push_trimmed_selector(target, text);
                }
            }
        }
        _ => {}
    }
}

/// Pushes one trimmed selector if it is not empty.
/// 在选择器非空时追加裁剪后的值。
fn push_trimmed_selector(target: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
        target.push(trimmed.to_string());
    }
}

/// Filters native tool identifiers with global request selectors.
/// 使用全局请求选择器过滤原生工具标识。
fn filter_native_tool_ids(
    tool_ids: Vec<String>,
    selectors: &ToolExposureSelectorState,
) -> Vec<String> {
    tool_ids
        .into_iter()
        .filter(|tool_id| {
            if tool_selector_matches(&selectors.global_disabled, &[tool_id.as_str()]) {
                return false;
            }
            selectors.global_allowed.is_empty()
                || tool_selector_matches(&selectors.global_allowed, &[tool_id.as_str()])
        })
        .collect()
}

/// Filters LuaSkills entries with global and LuaSkills-specific selectors.
/// 使用全局与 LuaSkills 专属选择器过滤 LuaSkills 入口。
fn filter_luaskills_entries(
    entries: Vec<RuntimeEntryDescriptor>,
    selectors: &ToolExposureSelectorState,
) -> Vec<RuntimeEntryDescriptor> {
    entries
        .into_iter()
        .filter(|entry| {
            if luaskills_entry_matches_any(entry, &selectors.global_disabled)
                || luaskills_entry_matches_any(entry, &selectors.luaskills_disabled)
            {
                return false;
            }
            (selectors.global_allowed.is_empty() && selectors.luaskills_allowed.is_empty())
                || luaskills_entry_matches_any(entry, &selectors.global_allowed)
                || luaskills_entry_matches_any(entry, &selectors.luaskills_allowed)
        })
        .collect()
}

/// Filters LuaSkills prompt entries with global and LuaSkills-specific selectors.
/// 使用全局与 LuaSkills 专属选择器过滤 LuaSkills 提示词入口。
fn filter_luaskill_prompt_entries(
    entries: Vec<LuaSkillPromptEntry>,
    selectors: &ToolExposureSelectorState,
) -> Vec<LuaSkillPromptEntry> {
    entries
        .into_iter()
        .filter(|entry| {
            if lua_prompt_entry_matches_any(entry, &selectors.global_disabled)
                || lua_prompt_entry_matches_any(entry, &selectors.luaskills_disabled)
            {
                return false;
            }
            (selectors.global_allowed.is_empty() && selectors.luaskills_allowed.is_empty())
                || lua_prompt_entry_matches_any(entry, &selectors.global_allowed)
                || lua_prompt_entry_matches_any(entry, &selectors.luaskills_allowed)
        })
        .collect()
}

/// Filters MCP bindings with global, MCP-tool, and MCP-server selectors.
/// 使用全局、MCP 工具与 MCP 服务选择器过滤 MCP 绑定。
fn filter_mcp_tool_bindings(
    tools: Vec<McpRuntimeToolBinding>,
    selectors: &ToolExposureSelectorState,
) -> Vec<McpRuntimeToolBinding> {
    tools
        .into_iter()
        .filter(|tool| {
            if mcp_tool_binding_matches_any(tool, &selectors.global_disabled)
                || mcp_tool_binding_matches_any(tool, &selectors.mcp_tool_disabled)
                || tool.server_id().is_some_and(|server_id| {
                    tool_selector_matches(&selectors.mcp_server_disabled, &[server_id])
                })
            {
                return false;
            }
            let no_allow = selectors.global_allowed.is_empty()
                && selectors.mcp_tool_allowed.is_empty()
                && selectors.mcp_server_allowed.is_empty();
            no_allow
                || mcp_tool_binding_matches_any(tool, &selectors.global_allowed)
                || mcp_tool_binding_matches_any(tool, &selectors.mcp_tool_allowed)
                || tool.server_id().is_some_and(|server_id| {
                    tool_selector_matches(&selectors.mcp_server_allowed, &[server_id])
                })
        })
        .collect()
}

/// Returns whether one LuaSkills runtime entry matches a normalized selector set.
/// 返回单个 LuaSkills 运行时入口是否命中规范化选择器集合。
fn luaskills_entry_matches_any(
    entry: &RuntimeEntryDescriptor,
    selectors: &BTreeSet<String>,
) -> bool {
    tool_selector_matches(
        selectors,
        &[
            entry.canonical_name.as_str(),
            entry.skill_id.as_str(),
            entry.local_name.as_str(),
            entry.root_name.as_str(),
            entry.description.as_str(),
        ],
    )
}

/// Returns whether one LuaSkills prompt entry matches a normalized selector set.
/// 返回单个 LuaSkills 提示词入口是否命中规范化选择器集合。
fn lua_prompt_entry_matches_any(entry: &LuaSkillPromptEntry, selectors: &BTreeSet<String>) -> bool {
    let help_flow = entry.help_flow.as_deref().unwrap_or_default();
    tool_selector_matches(
        selectors,
        &[entry.name.as_str(), entry.description.as_str(), help_flow],
    )
}

/// Returns whether one MCP binding matches a normalized selector set.
/// 返回单个 MCP 绑定是否命中规范化选择器集合。
fn mcp_tool_binding_matches_any(
    tool: &McpRuntimeToolBinding,
    selectors: &BTreeSet<String>,
) -> bool {
    let origin_tool_id = tool.origin_tool_id();
    let server_id = tool.server_id().unwrap_or_default();
    let tool_name = tool.origin_tool_name().unwrap_or_default();
    tool_selector_matches(
        selectors,
        &[
            tool.id(),
            tool.name(),
            tool.description(),
            origin_tool_id,
            server_id,
            tool_name,
        ],
    )
}

/// Returns whether a final resolved public tool remains visible after request selectors.
/// 返回最终解析出的公开工具在请求选择器之后是否仍然可见。
fn resolved_tool_definition_is_visible(
    definition: &ResolvedToolDefinition,
    selectors: &ToolExposureSelectorState,
) -> bool {
    vulcan_tool_descriptor_is_visible(
        definition.descriptor.id.as_str(),
        definition.descriptor.name.as_str(),
        definition.descriptor.description.as_str(),
        definition.descriptor.source,
        selectors,
    )
}

/// Returns whether a final public tool descriptor remains visible after request selectors.
/// 返回最终公开工具描述在请求选择器之后是否仍然可见。
fn vulcan_tool_descriptor_is_visible(
    id: &str,
    name: &str,
    description: &str,
    source: VulcanToolSourceKind,
    selectors: &ToolExposureSelectorState,
) -> bool {
    if tool_selector_matches(&selectors.global_disabled, &[id, name, description]) {
        return false;
    }
    if selectors.global_allowed.is_empty() {
        return true;
    }
    if tool_selector_matches(&selectors.global_allowed, &[id, name, description]) {
        return true;
    }
    match source {
        VulcanToolSourceKind::LuaSkills => selectors.has_luaskills_allow(),
        VulcanToolSourceKind::Mcp => selectors.has_mcp_allow(),
        VulcanToolSourceKind::AiFallback => selectors.has_ai_allow(),
        VulcanToolSourceKind::Native => {
            selectors.has_luaskills_allow()
                && (id == LUASKILL_HELP_TOOL_NAME || id == CALL_LUASKILL_TOOL_NAME)
        }
    }
}

/// Returns whether one AI capability kind matches any user selector.
/// 返回某个 AI 能力类型是否命中任一用户选择器。
fn ai_tool_kind_matches_any_selector(kind: AiToolKind, selectors: &[String]) -> bool {
    selectors
        .iter()
        .any(|selector| ai_tool_kind_matches_selector(kind, selector))
}

/// Returns whether one AI capability kind matches one user selector.
/// 返回某个 AI 能力类型是否命中单个用户选择器。
fn ai_tool_kind_matches_selector(kind: AiToolKind, selector: &str) -> bool {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.eq_ignore_ascii_case(kind.key())
        || trimmed == kind.label()
        || normalized_tool_selector_set(&[trimmed.to_string()])
            .iter()
            .next()
            .is_some_and(|normalized| normalized == kind.key())
}

/// Builds the AI-visible skill developer segment only for user-turn-like requests.
/// 仅为类似用户回合的请求构建 AI 可见的技能开发者片段。
fn build_skill_developer_segment(
    purpose: ExecutionPromptPurpose,
    prompt_skills: &[vulcan_prompt_skills::ResolvedPromptSkill],
    implicit_luaskills: &[LuaSkillPromptEntry],
) -> Option<String> {
    if !purpose.supports_user_turn_tooling() {
        return None;
    }

    let mut segments = Vec::new();
    if let Some(segment) = render_prompt_skills_instructions(prompt_skills) {
        segments.push(segment);
    }
    if let Some(segment) = render_luaskills_prompt_segment(implicit_luaskills) {
        segments.push(segment);
    }
    (!segments.is_empty()).then(|| segments.join("\n\n"))
}

/// Builds project-level prompt-skill resolver input from one preparation request.
/// 从单条执行准备请求构建项目级提示词技能解析输入。
fn prompt_skills_project_config_from_request(
    request: &ExecutionPreparationRequest,
) -> PromptSkillsProjectConfig {
    PromptSkillsProjectConfig {
        skill_overrides: request
            .project_prompt_skills
            .skill_overrides
            .iter()
            .map(|(name, state)| {
                (
                    name.clone(),
                    crate::skills::prompt::state_from_persisted(*state),
                )
            })
            .collect(),
    }
}

/// Builds session-level prompt-skill resolver input from one preparation request.
/// 从单条执行准备请求构建会话级提示词技能解析输入。
fn prompt_skills_session_config_from_request(
    request: &ExecutionPreparationRequest,
) -> PromptSkillsSessionConfig {
    PromptSkillsSessionConfig {
        skill_overrides: request
            .session_prompt_skill_overrides
            .iter()
            .map(|(name, state)| {
                (
                    name.clone(),
                    crate::skills::prompt::state_from_persisted(*state),
                )
            })
            .collect(),
    }
}

/// Removes one resolved tool by public descriptor id from both descriptors and definitions.
/// 按公开描述符标识从描述与定义两侧移除一个已解析工具。
fn remove_resolved_tool(resolved_tools: &mut vulcan_tools_manager::ResolvedToolSet, tool_id: &str) {
    resolved_tools
        .descriptors
        .retain(|descriptor| descriptor.id != tool_id);
    resolved_tools
        .definitions
        .retain(|definition| definition.descriptor.id != tool_id);
}

/// Adds non-fatal diagnostics when a configured semantic AI fallback could not be exposed.
/// 当配置的语义 AI 回退工具无法暴露时追加非致命诊断。
fn push_missing_ai_tool_fallback_warnings(
    warnings: &mut Vec<String>,
    aliases: &[crate::vulcan_tools_bridge::AiToolFallbackAlias],
    resolved_tools: &vulcan_tools_manager::ResolvedToolSet,
) {
    for alias in aliases {
        let semantic_tool_name = alias.kind.key();
        let exposed = resolved_tools.descriptors.iter().any(|descriptor| {
            descriptor.source == vulcan_tools_manager::VulcanToolSourceKind::AiFallback
                && descriptor.id == semantic_tool_name
        });
        if !exposed {
            warnings.push(format!(
                "AI 专用工具 `{}` 已配置回退 `{}`，但本轮未找到可暴露的宿主工具。",
                alias.kind.label(),
                alias.fallback_tool_id
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ToolExposureSelectorState, ai_tool_configuration_for_execution_request,
        filter_luaskills_entries, filter_native_tool_ids,
        filter_resolved_tool_set_by_tool_exposure, prepare_execution,
    };
    use crate::agent_profile::AgentProfileCatalog;
    use crate::agent_runtime::config::PLAN_AGENT_ID;
    use crate::agent_runtime::execution_models::{
        ExecutionPreparationRequest, ExecutionPromptPurpose,
    };
    use crate::agent_runtime::runtime::RuntimeFoundation;
    use crate::conversation_protocol::SessionConversationState;
    use crate::mcp::configured_mcp_runtime_access;
    use crate::provider_config::{
        AiToolKind, PersistedAiToolConfiguration, PersistedProviderEntry, PersistedProviderOptions,
        ProjectPromptSkillsConfiguration, ProviderConfigurationFile, ReasoningEffortLevel,
    };
    use luaskills::RuntimeEntryDescriptor;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use uuid::Uuid;
    use vulcan_session_core::{
        ExecutionRuntimePolicy, ExecutionScope, SessionNodeId, ToolExposureSnapshot,
    };
    use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
    use vulcan_tools_manager::{
        ResolvedToolDefinition, ResolvedToolSet, VulcanToolDescriptor, VulcanToolInvocationResult,
        VulcanToolSourceKind,
    };
    use vulcan_vmm::VmmConfigLayer;

    /// Verifies request-scoped agent routing makes preparation use the requested agent instead of the global active worker.
    /// 验证请求级代理路由会让执行准备使用指定代理，而不是全局激活的默认工作代理。
    #[test]
    fn prepare_execution_uses_request_execution_agent_override() {
        let config = test_provider_config();
        let foundation = RuntimeFoundation::build_from_optional_mcp_access(
            &config,
            &AgentProfileCatalog::default(),
            None,
            Some(configured_mcp_runtime_access(&config)),
        );
        let mut request = test_execution_preparation_request();
        request.execution_agent_id = Some(String::from(PLAN_AGENT_ID));

        let preparation = prepare_execution(&foundation, None, &config, &request)
            .expect("preparation should route to the requested agent");

        assert_eq!(preparation.agent_id, PLAN_AGENT_ID);
        assert_eq!(preparation.agent_name, "方案代理");
        assert_eq!(preparation.agent_kind, "规划代理");
        assert!(preparation.delegate_agents.is_empty());
    }

    /// Verifies an unknown request-scoped execution agent fails instead of silently falling back to the active agent.
    /// 验证未知的请求级执行代理会失败，而不是静默回退到当前激活代理。
    #[test]
    fn prepare_execution_rejects_missing_request_execution_agent() {
        let config = test_provider_config();
        let foundation = RuntimeFoundation::build_from_optional_mcp_access(
            &config,
            &AgentProfileCatalog::default(),
            None,
            Some(configured_mcp_runtime_access(&config)),
        );
        let mut request = test_execution_preparation_request();
        request.execution_agent_id = Some(String::from("missing-agent"));

        let error = prepare_execution(&foundation, None, &config, &request)
            .expect_err("missing requested agent should fail");

        assert!(error.contains("missing-agent"));
        assert!(error.contains("未能在运行时基础架构中解析到"));
    }

    /// Verifies global request selectors filter native tool identifiers through the shared preparation helper.
    /// 验证全局请求选择器会通过共享准备辅助逻辑过滤原生工具标识。
    #[test]
    fn execution_tool_exposure_filters_native_ids_with_global_selectors() {
        let snapshot = ToolExposureSnapshot {
            allowed_tools: vec![
                String::from("shell command"),
                String::from("vulcan_file_read"),
            ],
            disabled_tools: vec![String::from("vulcan-file-read")],
            ..ToolExposureSnapshot::default()
        };
        let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);

        let filtered = filter_native_tool_ids(
            vec![
                String::from("shell_command"),
                String::from("vulcan_file_read"),
                String::from("vmm_memory_write"),
            ],
            &selectors,
        );

        assert_eq!(filtered, vec!["shell_command"]);
    }

    /// Verifies LuaSkills JSON overrides filter direct and implicit entries without TaskRuntime-side cloning.
    /// 验证 LuaSkills JSON 覆盖会过滤直接与隐式入口，而不依赖 TaskRuntime 侧克隆逻辑。
    #[test]
    fn execution_tool_exposure_filters_luaskills_with_family_overrides() {
        let snapshot = ToolExposureSnapshot {
            luaskills_overrides: json!({
                "enabled": ["vulcan-codekit"],
                "disabled": ["ast-detail"]
            }),
            ..ToolExposureSnapshot::default()
        };
        let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);
        let entries = vec![
            test_luaskills_entry("vulcan-codekit", "rg"),
            test_luaskills_entry("vulcan-codekit", "ast-detail"),
            test_luaskills_entry("vulcan-curl", "get"),
        ];

        let filtered = filter_luaskills_entries(entries, &selectors);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].canonical_name, "vulcan-codekit-rg");
    }

    /// Verifies AI capability selectors narrow the effective AI-tool configuration before provider routing.
    /// 验证 AI 能力选择器会在 Provider 路由之前收窄生效 AI 工具配置。
    #[test]
    fn execution_tool_exposure_applies_ai_capability_allow_and_deny() {
        let snapshot = ToolExposureSnapshot {
            allowed_ai_capabilities: vec![
                String::from("image-generation"),
                String::from("web_search"),
            ],
            disabled_ai_capabilities: vec![String::from("web search")],
            ..ToolExposureSnapshot::default()
        };
        let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);

        let configuration = ai_tool_configuration_for_execution_request(
            &PersistedAiToolConfiguration::default(),
            &selectors,
        );

        assert!(
            !configuration
                .disabled_capability_kinds
                .contains(&AiToolKind::ImageGeneration)
        );
        assert!(
            configuration
                .disabled_capability_kinds
                .contains(&AiToolKind::WebSearch)
        );
        assert!(
            configuration
                .disabled_capability_kinds
                .contains(&AiToolKind::MusicGeneration)
        );
    }

    /// Verifies LuaSkills family allow-list keeps the shared dispatcher even when global tool allow-list is also present.
    /// 验证 LuaSkills 族允许列表与全局工具允许列表并存时仍会保留共享调度器。
    #[test]
    fn execution_tool_exposure_keeps_luaskills_dispatcher_with_family_allow() {
        let snapshot = ToolExposureSnapshot {
            allowed_tools: vec![String::from("shell_command")],
            luaskills_overrides: json!({
                "enabled": ["vulcan-codekit"]
            }),
            ..ToolExposureSnapshot::default()
        };
        let selectors = ToolExposureSelectorState::from_snapshot(&snapshot);
        let mut resolved = ResolvedToolSet {
            descriptors: Vec::new(),
            definitions: vec![
                test_resolved_tool("shell_command", VulcanToolSourceKind::Native),
                test_resolved_tool(CALL_LUASKILL_TOOL_NAME, VulcanToolSourceKind::Native),
                test_resolved_tool("ask", VulcanToolSourceKind::Native),
            ],
        };
        let exposure = super::ExecutionToolExposureSelection {
            native_tool_ids: Vec::new(),
            luaskills_tools: Vec::new(),
            luaskills_dispatch_tools: Vec::new(),
            luaskill_prompt_entries: Vec::new(),
            mcp_tools: Vec::new(),
            ai_tool_configuration: PersistedAiToolConfiguration::default(),
            selectors,
        };

        filter_resolved_tool_set_by_tool_exposure(&mut resolved, &exposure);

        let remaining = resolved
            .definitions
            .iter()
            .map(|definition| definition.descriptor.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(remaining, vec!["shell_command", CALL_LUASKILL_TOOL_NAME]);
    }

    /// Builds one provider configuration that can resolve a provider blueprint without touching the network.
    /// 构建一份无需访问网络即可解析 Provider 蓝图的供应商配置。
    fn test_provider_config() -> ProviderConfigurationFile {
        let mut provider = BTreeMap::new();
        provider.insert(
            String::from("deepseek"),
            PersistedProviderEntry {
                name: Some(String::from("DeepSeek")),
                options: PersistedProviderOptions {
                    api_key: Some(String::from("test-key")),
                    ..PersistedProviderOptions::default()
                },
                ..PersistedProviderEntry::default()
            },
        );
        ProviderConfigurationFile {
            model: Some(String::from("deepseek/deepseek-v4-pro")),
            provider,
            ..ProviderConfigurationFile::default()
        }
    }

    /// Builds one minimal LuaSkills runtime entry descriptor for selector tests.
    /// 为选择器测试构建一条最小 LuaSkills 运行时入口描述。
    fn test_luaskills_entry(skill_id: &str, local_name: &str) -> RuntimeEntryDescriptor {
        RuntimeEntryDescriptor {
            canonical_name: format!("{skill_id}-{local_name}"),
            skill_id: skill_id.to_string(),
            local_name: local_name.to_string(),
            root_name: String::from("ROOT"),
            skill_dir: String::from("skill-dir"),
            description: String::from("test entry"),
            parameters: Vec::new(),
            input_schema: json!({
                "type": "object"
            }),
        }
    }

    /// Builds one minimal resolved tool definition for public selector tests.
    /// 为公开选择器测试构建一条最小已解析工具定义。
    fn test_resolved_tool(id: &str, source: VulcanToolSourceKind) -> ResolvedToolDefinition {
        ResolvedToolDefinition {
            descriptor: VulcanToolDescriptor {
                id: id.to_string(),
                name: id.to_string(),
                description: String::from("test tool"),
                help_text: None,
                source,
            },
            parameters_schema: json!({ "type": "object" }),
            invoke: Arc::new(|_, _| {
                Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!({}))) })
            }),
        }
    }

    /// Builds one minimal execution-preparation request for agent-routing tests.
    /// 为代理路由测试构建一份最小执行准备请求。
    fn test_execution_preparation_request() -> ExecutionPreparationRequest {
        let session_id = Uuid::nil();
        ExecutionPreparationRequest {
            session_id,
            execution_scope: ExecutionScope::MainSession {
                session_id: SessionNodeId::from(session_id),
            },
            runtime_policy: ExecutionRuntimePolicy::main_session(),
            execution_agent_id: None,
            agent_system_prompt_override: None,
            task_session_store_root: None,
            session_title: String::from("测试会话"),
            workspace_path: String::from(r"D:\workspace"),
            prompt: String::from("请规划当前任务"),
            conversation_state: SessionConversationState::default(),
            execution_mode: String::from("Build"),
            temperature: None,
            reasoning_effort: ReasoningEffortLevel::Off,
            ai_tool_configuration: PersistedAiToolConfiguration::default(),
            tool_exposure_override: Default::default(),
            prompt_purpose: ExecutionPromptPurpose::UserTurn,
            project_prompt_skills: ProjectPromptSkillsConfiguration::default(),
            session_prompt_skill_overrides: BTreeMap::new(),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: VmmConfigLayer::default(),
            session_vmm: VmmConfigLayer::default(),
        }
    }
}
