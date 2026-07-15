use super::*;
use std::collections::BTreeMap;

/// Summarizes one model-visible tool declared by a plugin package.
/// 汇总插件包声明的单个模型可见工具。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginToolSummary {
    /// Stable provider-visible tool identifier.
    /// 稳定的 Provider 可见工具标识。
    pub id: String,
    /// Atomic plugin-local tool group identifier.
    /// 插件内部原子工具组标识。
    pub group: String,
    /// English model-facing tool description.
    /// 面向模型的英文工具说明。
    pub description: String,
}

/// Summarizes one host hook declared by a manifest-driven plugin.
/// 汇总清单驱动插件声明的单个宿主 Hook。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginHookSummary {
    /// Stable plugin-local hook identifier.
    /// 稳定的插件内 Hook 标识。
    pub id: String,
    /// Exact host-owned hook point.
    /// 精确的宿主自有 Hook 点位。
    pub point: String,
}

/// Summarizes one dynamic WebUI mount declared by a plugin.
/// 汇总插件声明的单个动态 WebUI 挂载项。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginUiMountSummary {
    /// Stable plugin-local mount identifier.
    /// 稳定的插件内挂载标识。
    pub id: String,
    /// Stable host-owned mount point.
    /// 稳定的宿主自有挂载点。
    pub point: String,
    /// Default host navigation label declared by this mount.
    /// 此挂载点声明的默认宿主导航名称。
    pub label: String,
    /// Localized navigation labels keyed by locale identifier.
    /// 以语言标识为键的本地化导航名称。
    #[serde(default)]
    pub label_by_locale: BTreeMap<String, String>,
    /// Stable host-owned navigation icon identifier.
    /// 稳定的宿主导航图标标识。
    pub icon: String,
    /// Named ESM export containing the component factory.
    /// 包含组件工厂的 ESM 命名导出。
    pub export_name: String,
}

/// Describes manifest-owned extension surfaces unavailable to ordinary LuaSkill rows.
/// 描述普通 LuaSkill 行不具备的清单自有扩展面。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginExtensionSummary {
    /// Stable package origin such as bundled system or user installed.
    /// 稳定的包来源，例如系统内建或用户安装。
    pub origin: String,
    /// Exact host capabilities declared by the plugin manifest.
    /// 插件清单声明的精确宿主能力。
    pub capabilities: Vec<String>,
    /// Declared host hooks shown before installation authorization.
    /// 在安装授权前展示的已声明宿主 Hook。
    pub hooks: Vec<PluginHookSummary>,
    /// Host-callable business action identifiers.
    /// 宿主可调用的业务 Action 标识。
    pub actions: Vec<String>,
    /// Dynamic host UI mounts.
    /// 动态宿主界面挂载项。
    pub ui_mounts: Vec<PluginUiMountSummary>,
    /// Read-only Agent Skill identifiers contributed by this plugin.
    /// 当前插件贡献的只读 Agent Skill 标识。
    #[serde(default)]
    pub skills: Vec<String>,
    /// Read-only subagent identifiers contributed by this plugin.
    /// 当前插件贡献的只读子代理标识。
    #[serde(default)]
    pub agents: Vec<String>,
    /// Whether this plugin owns an executable Lua runtime.
    /// 当前插件是否拥有可执行 Lua 运行时。
    #[serde(default)]
    pub has_runtime: bool,
}

/// Summarizes one plugin or LuaSkill package.
/// 汇总单个插件或 LuaSkill 包。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginSummary {
    /// Stable plugin identifier.
    /// 稳定插件标识。
    pub id: String,
    /// Optional package version for manifest-driven plugins.
    /// 清单驱动插件的可选包版本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Plugin display name.
    /// 插件显示名称。
    pub name: String,
    /// Plugin kind such as LuaSkill or native Rust.
    /// 插件类型，例如 LuaSkill 或原生 Rust。
    pub kind: String,
    /// Plugin lifecycle status.
    /// 插件生命周期状态。
    pub status: String,
    /// Stable machine-readable runtime lifecycle state.
    /// 稳定的机器可读运行时生命周期状态。
    #[serde(default)]
    pub runtime_state: String,
    /// Plugin capability summary.
    /// 插件能力摘要。
    pub capability: String,
    /// Localized names keyed by locale identifier.
    /// 以语言标识为键的本地化名称。
    #[serde(default)]
    pub name_by_locale: BTreeMap<String, String>,
    /// Localized capability summaries keyed by locale identifier.
    /// 以语言标识为键的本地化能力摘要。
    #[serde(default)]
    pub capability_by_locale: BTreeMap<String, String>,
    /// Whether the plugin is currently active in the runtime catalog.
    /// 插件当前是否在运行时目录中处于活动状态。
    pub enabled: bool,
    /// Desired persisted enabled state independent from transient lifecycle state.
    /// 与临时生命周期状态独立的持久化期望启用状态。
    #[serde(default)]
    pub desired_enabled: bool,
    /// Whether the plugin assets required by the current runtime are available.
    /// 当前运行时所需插件资产是否可用。
    pub available: bool,
    /// Number of provider-visible tools declared by the plugin package.
    /// 插件包声明的 Provider 可见工具数量。
    #[serde(default)]
    pub tool_count: usize,
    /// Provider-visible tool metadata used by agent policy editors.
    /// 智能体策略编辑器使用的 Provider 可见工具元数据。
    #[serde(default)]
    pub tools: Vec<PluginToolSummary>,
    /// Number of host hook handlers declared by the plugin package.
    /// 插件包声明的宿主 hook 处理器数量。
    #[serde(default)]
    pub hook_count: usize,
    /// Number of read-only Agent Skills contributed by the plugin package.
    /// 插件包贡献的只读 Agent Skills 数量。
    #[serde(default)]
    pub skill_count: usize,
    /// Number of read-only subagents contributed by the plugin package.
    /// 插件包贡献的只读子代理数量。
    #[serde(default)]
    pub agent_count: usize,
    /// Number of currently retained managed sessions.
    /// 当前保留的受管会话数量。
    #[serde(default)]
    pub active_session_count: usize,
    /// Package, dependency, lifecycle, and cleanup diagnostics.
    /// 插件包、依赖、生命周期与清理诊断。
    #[serde(default)]
    pub diagnostics: Vec<String>,
    /// Stable lifecycle manager kind used by the WebUI action router.
    /// WebUI 动作路由使用的稳定生命周期管理类型。
    pub management_kind: String,
    /// Optional manifest-owned extension surfaces for real Vulcan plugins.
    /// 真实 Vulcan 插件可用的可选清单自有扩展面。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension: Option<PluginExtensionSummary>,
}

/// Returns one uncompressed plugin ESM module through the authenticated service boundary.
/// 通过已认证服务边界返回一个未压缩插件 ESM 模块。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginUiModuleSource {
    /// Exact owning plugin identifier.
    /// 精确的所属插件标识。
    pub plugin_id: String,
    /// Exact plugin-local mount identifier.
    /// 精确的插件内挂载标识。
    pub mount_id: String,
    /// Named ESM export containing the component factory.
    /// 包含组件工厂的 ESM 命名导出。
    pub export_name: String,
    /// Untouched UTF-8 ESM source read from the validated package.
    /// 从通过校验的插件包读取的原样 UTF-8 ESM 源码。
    pub source: String,
    /// Plugin-owned UI messages grouped by locale and stable message key.
    /// 按语言和稳定消息键组织的插件自有界面文案。
    #[serde(default)]
    pub messages_by_locale:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

/// Returns one bounded UTF-8 text asset declared by an enabled plugin package.
/// 返回一个由已启用插件包声明且有大小边界的 UTF-8 文本资源。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginTextAssetSource {
    /// Exact owning plugin identifier.
    /// 精确的所属插件标识。
    pub plugin_id: String,
    /// Exact package-relative path declared by the plugin manifest.
    /// 插件清单声明的精确包内相对路径。
    pub path: String,
    /// Untouched UTF-8 text read from the validated plugin package.
    /// 从通过校验的插件包读取的原样 UTF-8 文本。
    pub content: String,
}

/// Summarizes one settings section.
/// 汇总单个设置分组。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SettingSectionSummary {
    /// Stable settings section identifier.
    /// 稳定设置分组标识。
    pub id: String,
    /// Settings section display label.
    /// 设置分组显示名称。
    pub label: String,
    /// Settings section description.
    /// 设置分组描述。
    pub description: String,
    /// Representative setting items.
    /// 代表性设置项。
    pub items: Vec<String>,
}

/// Summarizes one model provider.
/// 汇总单个模型供应商。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub id: String,
    /// Provider display name.
    /// 供应商显示名称。
    pub name: String,
    /// Provider status text.
    /// 供应商状态文本。
    pub status: String,
    /// Whether this provider has a saved local configuration.
    /// 该供应商是否已经保存本地配置。
    #[serde(default)]
    pub configured: bool,
    /// Enabled model count text.
    /// 已启用模型数量文本。
    pub enabled_models: String,
    /// Whether this provider owns the currently active workbench model.
    /// 该供应商是否拥有当前激活的工作台模型。
    #[serde(default)]
    pub active: bool,
    /// Enabled model summaries exposed to the WebUI model picker.
    /// 暴露给 WebUI 模型选择器的已启用模型摘要。
    #[serde(default)]
    pub models: Vec<ProviderModelSummary>,
    /// Full catalog model summaries before enabled filtering.
    /// 启用过滤前的完整目录模型摘要。
    #[serde(default)]
    pub catalog_models: Vec<ProviderModelSummary>,
    /// Editable provider form fields projected from the provider catalog definition.
    /// 从供应商目录定义投影出的可编辑 Provider 表单字段。
    #[serde(default)]
    pub fields: Vec<ProviderFieldSummary>,
}

/// Summarizes one enabled model under a provider.
/// 汇总某个供应商下的单个已启用模型。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelSummary {
    /// Stable model identifier persisted with the provider id.
    /// 与供应商标识一起持久化的稳定模型标识。
    pub id: String,
    /// Human-readable model display name.
    /// 人类可读的模型显示名称。
    pub name: String,
    /// Whether this model is the current workbench model.
    /// 该模型是否为当前工作台模型。
    #[serde(default)]
    pub active: bool,
    /// Whether this model is enabled for the provider.
    /// 该模型是否已在该供应商下启用。
    #[serde(default)]
    pub enabled: bool,
    /// Optional lifecycle status from the provider catalog.
    /// 来自供应商目录的可选生命周期状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional model family from the provider catalog.
    /// 来自供应商目录的可选模型家族。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Optional context window limit.
    /// 可选上下文窗口上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input token limit.
    /// 可选输入 Token 上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output token limit.
    /// 可选输出 Token 上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<usize>,
    /// Optional reasoning capability flag.
    /// 可选推理能力标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Optional attachment capability flag.
    /// 可选附件能力标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Normalized input modality list.
    /// 归一化输入模态列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modalities: Vec<String>,
    /// Normalized output modality list.
    /// 归一化输出模态列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_modalities: Vec<String>,
    /// Optional reasoning profile JSON text.
    /// 可选推理档位配置 JSON 文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_profile_json: Option<String>,
    /// Whether this provider/model pair exposes backend-managed secondary routes.
    /// 该 Provider/模型组合是否暴露由后端管理的二级路由。
    #[serde(default)]
    pub supports_secondary_routes: bool,
    /// Active backend-managed secondary route, when one is selected.
    /// 当前已选中的后端二级路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_route: Option<ProviderModelSecondaryRouteSummary>,
}

/// Summarizes one backend-managed secondary route for a concrete provider/model pair.
/// 汇总某个具体 Provider/模型组合下的一条后端二级路由。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelSecondaryRouteSummary {
    /// Stable owning provider identifier.
    /// 稳定所属 Provider 标识。
    pub provider_id: String,
    /// Stable owning model identifier.
    /// 稳定所属模型标识。
    pub model_id: String,
    /// Stable route identifier forwarded to the provider request.
    /// 转发到 Provider 请求中的稳定路由标识。
    pub route_id: String,
    /// Compatibility alias for providers that call the route id a tag.
    /// 兼容将路由标识称为 tag 的 Provider。
    pub tag: String,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    pub provider_name: String,
    /// Human-readable endpoint name.
    /// 人类可读的端点名称。
    pub endpoint_name: String,
    /// Compact label rendered in picker triggers.
    /// 渲染在选择器触发器中的紧凑标签。
    pub label: String,
    /// Secondary detail line rendered in route lists.
    /// 渲染在路由列表中的二级详情文本。
    pub subtitle: String,
    /// Whether this route is the active persisted selection.
    /// 该路由是否为当前已持久化选择。
    #[serde(default)]
    pub active: bool,
}

/// Selects one backend-managed secondary route for a concrete provider/model pair.
/// 为某个具体 Provider/模型组合选择一条后端二级路由。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelSecondaryRouteSelection {
    /// Stable route identifier forwarded to the provider request.
    /// 转发到 Provider 请求中的稳定路由标识。
    pub route_id: String,
    /// Compatibility alias for providers that call the route id a tag.
    /// 兼容将路由标识称为 tag 的 Provider。
    #[serde(default)]
    pub tag: String,
    /// Human-readable upstream provider name.
    /// 人类可读的上游供应商名称。
    #[serde(default)]
    pub provider_name: String,
    /// Human-readable endpoint name.
    /// 人类可读的端点名称。
    #[serde(default)]
    pub endpoint_name: String,
}

/// Draft data used by the WebUI user-prompt edit dialog.
/// WebUI 用户提示词编辑弹窗使用的草稿数据。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserPromptEditDraftSummary {
    /// Stable conversation identifier.
    /// 稳定会话标识。
    pub conversation_id: String,
    /// Stable user prompt message identifier.
    /// 稳定用户提示词消息标识。
    pub message_id: String,
    /// Editable prompt text.
    /// 可编辑提示词文本。
    pub prompt: String,
    /// Attachments that will be replayed with the edited prompt.
    /// 将随编辑后提示词重新执行的附件。
    #[serde(default)]
    pub attachments: Vec<ComposerAttachmentSummary>,
    /// Whether the owning conversation is currently running.
    /// 所属会话当前是否正在运行。
    #[serde(default)]
    pub running: bool,
}

/// Summarizes one editable provider configuration field.
/// 汇总单个可编辑 Provider 配置字段。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderFieldSummary {
    /// Stable field key used by save requests.
    /// 保存请求使用的稳定字段键。
    pub key: String,
    /// User-facing field label.
    /// 面向用户显示的字段标签。
    pub label: String,
    /// Field placeholder text.
    /// 字段占位提示文本。
    pub placeholder: String,
    /// Stored field value.
    /// 已保存的字段值。
    pub value: String,
    /// Whether the field should be edited as a secret.
    /// 该字段是否应按密文字段编辑。
    pub secret: bool,
    /// Whether the field must be non-empty before saving.
    /// 保存前该字段是否必须非空。
    pub required: bool,
}

/// Summarizes one persisted MCP server configuration.
/// 汇总单个持久化 MCP 服务器配置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSummary {
    /// Stable MCP server identifier.
    /// 稳定 MCP 服务器标识。
    pub id: String,
    /// User-facing server name.
    /// 面向用户显示的服务器名称。
    pub name: String,
    /// Whether the server is enabled.
    /// 该服务器是否启用。
    pub enabled: bool,
    /// Transport key such as `stdio`, `http`, or `sse`.
    /// 传输键，例如 `stdio`、`http` 或 `sse`。
    pub transport: String,
    /// Command used for stdio servers.
    /// stdio 服务器使用的启动命令。
    pub command: String,
    /// Command arguments used for stdio servers.
    /// stdio 服务器使用的启动参数。
    pub args: Vec<String>,
    /// URL used for remote servers.
    /// 远端服务器使用的 URL。
    pub url: String,
    /// Bearer token used for remote servers.
    /// 远端服务器使用的 Bearer Token。
    pub bearer_token: String,
    /// Explicit request headers.
    /// 显式请求头。
    pub headers: Vec<KeyValueSummary>,
    /// Request headers resolved from environment variables.
    /// 从环境变量解析的请求头。
    pub headers_from_env: Vec<KeyValueSummary>,
    /// OAuth client identifier.
    /// OAuth 客户端标识。
    pub oauth_client_id: String,
    /// OAuth client secret.
    /// OAuth 客户端密钥。
    pub oauth_client_secret: String,
    /// OAuth scope list.
    /// OAuth scope 列表。
    pub oauth_scopes: Vec<String>,
    /// OAuth redirect URI.
    /// OAuth 回调 URI。
    pub oauth_redirect_uri: String,
    /// Working directory used by stdio servers.
    /// stdio 服务器使用的工作目录。
    pub cwd: String,
    /// Environment variables injected into stdio servers.
    /// 注入 stdio 服务器的环境变量。
    pub env: Vec<KeyValueSummary>,
    /// Environment variable names passed through to stdio servers.
    /// 透传给 stdio 服务器的环境变量名。
    pub env_passthrough: Vec<String>,
}

/// Summarizes one discovered MCP runtime tool.
/// 汇总单个已发现的 MCP 运行时工具。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSummary {
    /// Stable selector identifier accepted by runtime tool exposure policies.
    /// 运行时工具暴露策略可接受的稳定选择器标识。
    pub id: String,
    /// Owning MCP server identifier.
    /// 所属 MCP 服务器标识。
    pub server_id: String,
    /// Remote tool name.
    /// 远端工具名称。
    pub name: String,
    /// Human-readable remote tool description.
    /// 人类可读的远端工具描述。
    pub description: String,
    /// Original qualified MCP tool identifier.
    /// 原始限定 MCP 工具标识。
    pub origin_tool_id: String,
}

/// Summarizes one string key/value pair.
/// 汇总单个字符串键值对。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyValueSummary {
    /// Stable key.
    /// 稳定键。
    pub key: String,
    /// Stored value.
    /// 已保存值。
    pub value: String,
}

/// Summarizes the workbench reasoning selector resolved by the backend.
/// 汇总由后端解析出的工作台思考选择器。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningSelectionSummary {
    /// Dynamic options supported by the active provider and model.
    /// 当前激活 Provider 与模型支持的动态选项。
    pub options: Vec<ReasoningSelectionOptionSummary>,
    /// Effective selected reasoning effort value.
    /// 当前生效的已选思考档位值。
    pub selected_effort: String,
    /// Optional note explaining model-specific reconciliation.
    /// 可选说明，用于解释模型特定的档位协调结果。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Summarizes one reasoning selector option.
/// 汇总单个思考选择器选项。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningSelectionOptionSummary {
    /// Stable reasoning effort value sent back through the service router.
    /// 通过服务路由回传的稳定思考档位值。
    pub value: String,
    /// User-facing option label resolved from provider configuration.
    /// 从 Provider 配置解析出的面向用户选项标签。
    pub label: String,
}

#[cfg(test)]
mod plugin_summary_tests {
    use super::*;

    /// Verifies frontend-required plugin collections remain present when they are empty.
    /// 验证前端必需的插件集合在为空时仍会保留在序列化结果中。
    #[test]
    fn empty_plugin_collections_are_serialized() {
        let summary = PluginSummary {
            id: String::from("hook-only-plugin"),
            version: Some(String::from("1.0.0")),
            name: String::from("Hook-only Plugin"),
            kind: String::from("System Plugin"),
            status: String::from("已启用"),
            runtime_state: String::from("enabled"),
            capability: String::from("Host hook integration"),
            name_by_locale: BTreeMap::new(),
            capability_by_locale: BTreeMap::new(),
            enabled: true,
            desired_enabled: true,
            available: true,
            tool_count: 0,
            skill_count: 0,
            agent_count: 0,
            tools: Vec::new(),
            hook_count: 1,
            active_session_count: 0,
            diagnostics: Vec::new(),
            management_kind: String::from("systemPlugin"),
            extension: None,
        };
        let value = serde_json::to_value(summary).expect("serialize plugin summary");
        assert_eq!(value["tools"], serde_json::json!([]));
        assert_eq!(value["diagnostics"], serde_json::json!([]));
        assert_eq!(value["nameByLocale"], serde_json::json!({}));
        assert_eq!(value["capabilityByLocale"], serde_json::json!({}));
    }
}
