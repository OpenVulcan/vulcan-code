//! Request-scoped tool exposure selection and filtering.
//! 请求级工具可见性选择与过滤。

use crate::agent_runtime::execution_models::ExecutionPreparationRequest;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::mcp::McpRuntimeToolBinding;
use crate::provider_config::{AiToolKind, PersistedAiToolConfiguration};
use crate::tooling::native::{
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, TODOLIST_TOOL_ID, UPDATE_GOAL_TOOL_ID,
};
use luaskills::RuntimeEntryDescriptor;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_luaskills_prompt::{LUASKILL_HELP_TOOL_NAME, LuaSkillPromptEntry};
use vulcan_plugin_runtime::SystemPluginToolDescriptor;
use vulcan_session_core::{
    CapabilityPolicyMode, ToolExposureSnapshot, normalized_tool_selector_set, tool_selector_matches,
};
use vulcan_tools_luaskills_call::CALL_LUASKILL_TOOL_NAME;
use vulcan_tools_manager::{ResolvedToolDefinition, ResolvedToolSet, VulcanToolSourceKind};

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
    pub(crate) selectors: ToolExposureSelectorState,
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
    definitions.retain(|definition| resolved_tool_definition_is_visible(definition, tool_exposure));
}

/// Applies request-scoped public-tool selectors to descriptors and definitions together.
/// 将请求级公开工具选择器同时应用到工具描述与定义。
pub(crate) fn filter_resolved_tool_set_by_tool_exposure(
    resolved_tools: &mut ResolvedToolSet,
    tool_exposure: &ExecutionToolExposureSelection,
) {
    resolved_tools.descriptors.retain(|descriptor| {
        vulcan_tool_descriptor_is_visible(
            descriptor.id.as_str(),
            descriptor.name.as_str(),
            descriptor.description.as_str(),
            descriptor.source,
            tool_exposure,
        )
    });
    filter_resolved_tool_definitions_by_tool_exposure(
        &mut resolved_tools.definitions,
        tool_exposure,
    );
}

/// Builds an effective AI-tool configuration after capability-level request overrides.
/// 根据能力级请求覆盖构建生效 AI 工具配置。
pub(crate) fn ai_tool_configuration_for_execution_request(
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
pub(crate) struct ToolExposureSelectorState {
    /// Whether global allow-list mode is active even when the selector set is empty.
    /// 即使选择器集合为空，全局仅允许模式是否仍处于激活状态。
    global_allow_active: bool,
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
    pub(crate) fn from_snapshot(snapshot: &ToolExposureSnapshot) -> Self {
        let luaskills_override = lua_selector_override_from_value(&snapshot.luaskills_overrides);
        let mcp_override = mcp_selector_override_from_value(&snapshot.mcp_server_overrides);
        let (policy_allowed, policy_disabled, global_allow_active) =
            selector_sets_from_policy(snapshot);
        Self {
            global_allow_active,
            global_allowed: policy_allowed,
            global_disabled: policy_disabled,
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

    /// Returns whether global allow-list mode is active for this request.
    /// 返回当前请求是否启用了全局仅允许模式。
    fn has_global_allow(&self) -> bool {
        self.global_allow_active
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
}

/// Builds global allow and deny selector sets from the unified policy or legacy lists.
/// 从统一策略或旧版列表构建全局允许与禁用选择器集合。
fn selector_sets_from_policy(
    snapshot: &ToolExposureSnapshot,
) -> (BTreeSet<String>, BTreeSet<String>, bool) {
    if snapshot.policy.mode == CapabilityPolicyMode::All && snapshot.policy.selectors.is_empty() {
        let allowed = normalized_tool_selector_set(&snapshot.allowed_tools);
        let allow_active = !allowed.is_empty();
        return (
            allowed,
            normalized_tool_selector_set(&snapshot.disabled_tools),
            allow_active,
        );
    }
    match snapshot.policy.mode {
        CapabilityPolicyMode::All => (BTreeSet::new(), BTreeSet::new(), false),
        CapabilityPolicyMode::AllowOnly => (
            normalized_tool_selector_set(&snapshot.policy.selectors),
            BTreeSet::new(),
            true,
        ),
        CapabilityPolicyMode::DenyOnly => (
            BTreeSet::new(),
            normalized_tool_selector_set(&snapshot.policy.selectors),
            false,
        ),
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
pub(crate) fn filter_native_tool_ids(
    tool_ids: Vec<String>,
    selectors: &ToolExposureSelectorState,
) -> Vec<String> {
    tool_ids
        .into_iter()
        .filter(|tool_id| {
            if native_tool_id_is_universal(tool_id) {
                return true;
            }
            if tool_selector_matches(&selectors.global_disabled, &[tool_id.as_str()]) {
                return false;
            }
            !selectors.has_global_allow()
                || tool_selector_matches(&selectors.global_allowed, &[tool_id.as_str()])
        })
        .collect()
}

/// Filters LuaSkills entries with global and LuaSkills-specific selectors.
/// 使用全局与 LuaSkills 专属选择器过滤 LuaSkills 入口。
pub(crate) fn filter_luaskills_entries(
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
            (!selectors.has_global_allow() && !selectors.has_luaskills_allow())
                || luaskills_entry_matches_any(entry, &selectors.global_allowed)
                || luaskills_entry_matches_any(entry, &selectors.luaskills_allowed)
        })
        .collect()
}

/// Filters LuaSkills prompt entries with global and LuaSkills-specific selectors.
/// 使用全局与 LuaSkills 专属选择器过滤 LuaSkills 提示词入口。
pub(crate) fn filter_luaskill_prompt_entries(
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
            (!selectors.has_global_allow() && !selectors.has_luaskills_allow())
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
            let no_allow = !selectors.has_global_allow()
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

/// Filters System Plugin tools by request selectors while preserving each declared atomic group.
/// 按请求选择器过滤系统插件工具，同时保留每个声明的原子工具组。
pub(crate) fn filter_system_plugin_tool_groups(
    tools: Vec<SystemPluginToolDescriptor>,
    tool_exposure: &ExecutionToolExposureSelection,
) -> Vec<SystemPluginToolDescriptor> {
    let mut groups = BTreeMap::<(String, String), Vec<SystemPluginToolDescriptor>>::new();
    for tool in tools {
        groups
            .entry((tool.plugin_id.clone(), tool.group.clone()))
            .or_default()
            .push(tool);
    }
    groups
        .into_values()
        .filter(|group| {
            group.iter().all(|tool| {
                vulcan_tool_descriptor_is_visible(
                    &tool.id,
                    &tool.id,
                    &tool.description,
                    VulcanToolSourceKind::SystemPlugin,
                    tool_exposure,
                )
            })
        })
        .flatten()
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
    tool_exposure: &ExecutionToolExposureSelection,
) -> bool {
    vulcan_tool_descriptor_is_visible(
        definition.descriptor.id.as_str(),
        definition.descriptor.name.as_str(),
        definition.descriptor.description.as_str(),
        definition.descriptor.source,
        tool_exposure,
    )
}

/// Returns whether a final public tool descriptor remains visible after request selectors.
/// 返回最终公开工具描述在请求选择器之后是否仍然可见。
pub(crate) fn vulcan_tool_descriptor_is_visible(
    id: &str,
    name: &str,
    description: &str,
    source: VulcanToolSourceKind,
    tool_exposure: &ExecutionToolExposureSelection,
) -> bool {
    let selectors = &tool_exposure.selectors;
    if source == VulcanToolSourceKind::Native && native_tool_id_is_universal(id) {
        return true;
    }
    if tool_selector_matches(&selectors.global_disabled, &[id, name, description]) {
        return false;
    }
    if !selectors.has_global_allow() {
        return true;
    }
    if tool_selector_matches(&selectors.global_allowed, &[id, name, description]) {
        return true;
    }
    match source {
        VulcanToolSourceKind::AiFallback => true,
        VulcanToolSourceKind::SystemPlugin => false,
        VulcanToolSourceKind::LuaSkills => selectors.has_luaskills_allow(),
        VulcanToolSourceKind::Mcp => selectors.has_mcp_allow(),
        VulcanToolSourceKind::Native => {
            (!tool_exposure.luaskills_dispatch_tools.is_empty()
                || !tool_exposure.luaskill_prompt_entries.is_empty())
                && (id == LUASKILL_HELP_TOOL_NAME || id == CALL_LUASKILL_TOOL_NAME)
        }
    }
}

/// Returns whether one native tool is exposed universally outside regular tool filters.
/// 返回单个原生工具是否在普通工具过滤之外全局暴露。
///
/// # Parameters
/// 参数
/// `tool_id` is the stable native tool identifier.
/// `tool_id` 是稳定的原生工具标识。
///
/// # Returns
/// 返回值
/// `true` when the tool should bypass request-scoped filtering.
/// 当工具应绕过请求级过滤时返回 `true`。
fn native_tool_id_is_universal(tool_id: &str) -> bool {
    matches!(
        tool_id,
        TODOLIST_TOOL_ID | GET_GOAL_TOOL_ID | CREATE_GOAL_TOOL_ID | UPDATE_GOAL_TOOL_ID
    )
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
