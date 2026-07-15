use super::*;

/// Normalizes one persisted transcript turn-page size to the supported fixed presets.
/// 将一条持久化转录轮次分页大小归一化到受支持的固定档位。
pub fn normalize_session_transcript_turn_page_size(value: Option<usize>) -> usize {
    let normalized = value.unwrap_or(SESSION_TRANSCRIPT_TURN_PAGE_SIZE_DEFAULT);
    if SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS.contains(&normalized) {
        normalized
    } else {
        SESSION_TRANSCRIPT_TURN_PAGE_SIZE_DEFAULT
    }
}

/// Workbench thinking-output display mode persisted for the WebUI.
/// 为 WebUI 持久化的工作台思考输出展示模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkbenchThinkingDisplayMode {
    /// Shows the live thinking block in the popup and then leaves it inline after streaming ends.
    /// 在弹窗中展示实时思考块，并在流式结束后保留到主输出中。
    PopupOnly,
    /// Shows every thinking block directly in the main transcript.
    /// 将所有思考块直接展示在主会话转录中。
    InlineOutput,
    /// Shows the first thinking block inline while keeping the live popup for later thinking blocks.
    /// 首段思考内联展示，同时为后续思考块保留实时弹窗。
    FirstInlineWithPopup,
    /// Shows only the first thinking block inline and hides later thinking blocks.
    /// 仅内联展示首段思考，并隐藏后续思考块。
    FirstInlineOnly,
}

impl Default for WorkbenchThinkingDisplayMode {
    /// Returns the default thinking display mode used when no user configuration exists.
    /// 返回没有用户配置时使用的默认思考展示模式。
    fn default() -> Self {
        Self::FirstInlineWithPopup
    }
}

/// Root provider configuration file saved in the Vulcan home directory.
/// 保存到 Vulcan 家目录中的根级供应商配置文件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfigurationFile {
    /// Currently selected global model in `provider/model` form.
    /// 当前以 `provider/model` 形式保存的全局激活模型。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Remembered workbench reasoning preference selected by the user before any model-specific capability downgrade is applied.
    /// 在应用任意模型级能力回退前，由用户选定并记忆的工作台思考偏好。
    #[serde(
        rename = "workbenchReasoningEffort",
        skip_serializing_if = "Option::is_none"
    )]
    pub workbench_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Remembered workbench thinking-output display mode selected by the user.
    /// 由用户选择并记忆的工作台思考输出展示模式。
    #[serde(
        rename = "workbenchThinkingDisplayMode",
        skip_serializing_if = "Option::is_none"
    )]
    pub workbench_thinking_display_mode: Option<WorkbenchThinkingDisplayMode>,
    /// Remembered WebUI locale identifier selected by the user.
    /// 由用户选择并记忆的WebUI语言标识。
    #[serde(rename = "uiLocale", skip_serializing_if = "Option::is_none")]
    pub ui_locale: Option<String>,
    /// Remembered startup transcript page size expressed in complete user turns.
    /// 以完整用户轮次表示的已记忆启动转录分页大小。
    #[serde(
        rename = "sessionTranscriptTurnPageSize",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_transcript_turn_page_size: Option<usize>,
    /// Persisted provider retry behavior used by real provider dispatches.
    /// 真实 Provider 分发使用的持久化请求重试行为。
    #[serde(
        default,
        skip_serializing_if = "PersistedProviderRetryConfiguration::is_default"
    )]
    pub retry: PersistedProviderRetryConfiguration,
    /// Remembered default command shell selected by the user to override host-side `auto` resolution.
    /// 由用户选择并记忆、用于覆盖宿主侧 `auto` 解析的默认命令 shell。
    #[serde(
        rename = "defaultCommandShell",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_command_shell: Option<String>,
    /// Persisted Git commit dialog behavior.
    /// 持久化的 Git 提交弹窗行为。
    #[serde(
        default,
        rename = "gitCommit",
        skip_serializing_if = "PersistedGitCommitConfiguration::is_default"
    )]
    pub git_commit: PersistedGitCommitConfiguration,
    /// Remembered OpenRouter endpoint route selections keyed by the exact OpenRouter model identifier.
    /// 以精确 OpenRouter 模型标识为键记忆的 OpenRouter 端点路由选择集合。
    #[serde(
        rename = "openrouterEndpointRoutes",
        default,
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub openrouter_endpoint_routes: BTreeMap<String, OpenRouterEndpointRouteSelection>,
    /// Persisted provider map keyed by provider identifier.
    /// 以供应商标识为键保存的供应商映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider: BTreeMap<String, PersistedProviderEntry>,
    /// Persisted skill definitions keyed by stable skill id.
    /// 以稳定技能标识为键保存的技能定义集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skills: BTreeMap<String, PersistedSkillEntry>,
    /// Persisted tool policy definitions keyed by stable policy id.
    /// 以稳定策略标识为键保存的工具策略定义集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_policies: BTreeMap<String, PersistedToolPolicyEntry>,
    /// Host-managed LuaSkills visibility overrides persisted independently from package lifecycle state.
    /// 与技能包生命周期状态独立持久化的宿主管理 LuaSkills 可见性覆盖配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedLuaSkillsConfiguration::is_default"
    )]
    pub luaskills: PersistedLuaSkillsConfiguration,
    /// Host-managed Prompt Skills configuration persisted independently from LuaSkills.
    /// 与 LuaSkills 独立持久化的宿主管理提示词技能配置。
    #[serde(
        default,
        rename = "promptSkills",
        skip_serializing_if = "PersistedPromptSkillsConfiguration::is_default"
    )]
    pub prompt_skills: PersistedPromptSkillsConfiguration,
    /// Legacy embedded AI tool routing and compatibility configuration kept only as a migration fallback for the standalone AI capability file.
    /// 仅作为独立 AI 能力配置文件迁移回退而保留的旧版内嵌 AI 工具路由与兼容配置。
    #[serde(
        default,
        rename = "aiTools",
        skip_serializing_if = "PersistedAiToolConfiguration::is_default"
    )]
    pub ai_tools: PersistedAiToolConfiguration,
    /// Global VMM memory integration configuration.
    /// 全局 VMM 记忆集成配置。
    #[serde(default, skip_serializing_if = "VmmConfigLayer::is_empty")]
    pub vmm: VmmConfigLayer,
    /// Persisted MCP server configuration block.
    /// 持久化的 MCP 服务器配置块。
    #[serde(default, skip_serializing_if = "PersistedMcpConfiguration::is_empty")]
    pub mcp: PersistedMcpConfiguration,
    /// Persisted controller bootstrap configuration.
    /// 持久化的控制器引导配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedControllerConfiguration::is_default"
    )]
    pub controller: PersistedControllerConfiguration,
    /// Persisted prompt-composer configuration.
    /// 持久化的提示词组合配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedPromptConfiguration::is_default"
    )]
    pub prompt: PersistedPromptConfiguration,
}

/// Loads the supported provider catalog derived from the split provider resources.
/// 加载从分离供应商资源目录派生出来的受支持供应商目录。
pub fn supported_providers() -> &'static [SupportedProviderDefinition] {
    static PROVIDERS: OnceLock<Vec<SupportedProviderDefinition>> = OnceLock::new();

    PROVIDERS.get_or_init(load_supported_providers)
}

/// Returns the persisted Vulcan configuration file path.
/// 返回持久化 Vulcan 配置文件路径。
pub fn configuration_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_path_buf()
    });

    home.join(".vulcan").join("vulcan-code.json")
}

/// Returns the project-local Vulcan Code configuration file path for one workspace.
/// 返回单个工作区的项目级 Vulcan Code 配置文件路径。
pub fn project_configuration_file_path(workspace_path: &Path) -> PathBuf {
    workspace_path
        .join(".vulcan")
        .join("code")
        .join("config.json")
}

/// Loads one project-local Vulcan Code configuration file from disk.
/// 从磁盘加载单个项目级 Vulcan Code 配置文件。
///
/// # Parameters
/// # 参数
/// - `workspace_path`: Exact workspace directory that owns `.vulcan/code/config.json`.
/// - `workspace_path`：拥有 `.vulcan/code/config.json` 的精确工作区目录。
///
/// # Returns
/// # 返回值
/// Returns defaults only for a confirmed missing file, and preserves probing, reading, and parsing failures with the exact path.
/// 仅在确认文件缺失时返回默认值，并以精确路径保留探测、读取与解析失败。
pub fn load_project_configuration_file(
    workspace_path: &Path,
) -> Result<ProjectVulcanCodeConfigurationFile, String> {
    // path is the sole project-local configuration source defined by the persistence contract.
    // path 是持久化契约定义的唯一项目本地配置来源。
    let path = project_configuration_file_path(workspace_path);
    // path_exists distinguishes a missing optional configuration from an inaccessible path.
    // path_exists 区分缺失的可选配置与不可访问路径。
    let path_exists = path
        .try_exists()
        .map_err(|error| format!("探测项目配置文件失败（{}）：{}。", path.display(), error))?;
    if !path_exists {
        return Ok(ProjectVulcanCodeConfigurationFile::default());
    }

    // text is the exact persisted JSON document parsed below.
    // text 是下方执行解析的精确持久化 JSON 文档。
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取项目配置文件失败（{}）：{}。", path.display(), error))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("解析项目配置文件失败（{}）：{}。", path.display(), error))
}

/// Saves one project-local Vulcan Code configuration file back to disk.
/// 将单个项目级 Vulcan Code 配置文件写回磁盘。
pub fn save_project_configuration_file(
    workspace_path: &Path,
    config: &ProjectVulcanCodeConfigurationFile,
) -> Result<(), String> {
    let path = project_configuration_file_path(workspace_path);
    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("序列化项目配置失败：{}。", error))?;
    crate::atomic_file::write_file_atomically(&path, content.as_bytes())
        .map_err(|error| format!("写入项目配置失败：{error}。"))
}

/// Loads the shared controller bootstrap configuration from the runtime configuration directory.
/// 从运行时配置目录加载共享控制器引导配置。
pub fn load_controller_configuration_file() -> Result<PersistedControllerConfiguration, String> {
    let path = runtime_assets::resolve_controller_config_file_path();

    if !path.exists() {
        return Ok(PersistedControllerConfiguration::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取控制器配置文件失败：{}。", error))?;

    serde_json::from_str(&text).map_err(|error| format!("解析控制器配置文件失败：{}。", error))
}

/// Loads the persisted provider configuration file from disk.
/// 从磁盘加载持久化的供应商配置文件。
pub fn load_configuration_file() -> Result<ProviderConfigurationFile, String> {
    let path = configuration_file_path();

    if !path.exists() {
        return Ok(ProviderConfigurationFile::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取供应商配置文件失败：{}。", error))?;

    serde_json::from_str(&text).map_err(|error| format!("解析供应商配置文件失败：{}。", error))
}

/// Saves the provider configuration file back to disk, creating parent directories when needed.
/// 将供应商配置文件写回磁盘，并在需要时创建父目录。
pub fn save_configuration_file(config: &ProviderConfigurationFile) -> Result<(), String> {
    let path = configuration_file_path();
    let normalized_config = normalize_provider_configuration_for_save(config);
    let content = serde_json::to_string_pretty(&normalized_config)
        .map_err(|error| format!("序列化供应商配置失败：{}。", error))?;
    crate::atomic_file::write_file_atomically(&path, content.as_bytes())
        .map_err(|error| format!("写入供应商配置失败：{error}。"))
}

/// Resolves the supported provider definition by identifier.
/// 根据标识解析受支持的供应商定义。
pub fn supported_provider_by_id(provider_id: &str) -> Option<&'static SupportedProviderDefinition> {
    supported_providers()
        .iter()
        .find(|definition| definition.id == provider_id)
}

/// Builds picker choices from the supported provider catalog.
/// 从受支持供应商目录构建选择器选项。
pub fn supported_provider_choices() -> Vec<ProviderChoice> {
    supported_providers()
        .iter()
        .map(|definition| ProviderChoice {
            id: definition.id.clone(),
            name: definition.name.clone(),
        })
        .collect()
}

/// Resolves the full catalog model choices for one provider after merging embedded metadata and user overrides.
/// 合并内置元数据与用户覆盖后，为某个供应商解析完整目录模型列表。
pub fn catalog_model_choices(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> Vec<ModelChoice> {
    let mut embedded_models = supported_provider_by_id(provider_id)
        .map(|definition| definition.models.clone())
        .unwrap_or_default();
    apply_provider_reasoning_model_overrides(provider_id, &mut embedded_models);

    let mut catalog = embedded_models.clone();

    if let Some(entry) = config.provider.get(provider_id) {
        catalog = apply_model_overrides_to_catalog(catalog, &entry.model_overrides);
    }

    apply_provider_reasoning_model_overrides(provider_id, &mut catalog);
    catalog.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    catalog.dedup_by(|left, right| left.id == right.id);
    catalog
}

/// Resolves the effective enabled model choices for one provider by filtering the full catalog through explicit enabled-model preferences.
/// 通过显式启用模型偏好过滤完整目录，为某个供应商解析生效可用模型列表。
pub fn effective_model_choices(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> Vec<ModelChoice> {
    let catalog = catalog_model_choices(provider_id, config);
    let enabled_ids = resolved_enabled_model_ids(config.provider.get(provider_id), &catalog);

    catalog
        .into_iter()
        .filter(|model| enabled_ids.contains(&model.id))
        .collect()
}

/// Resolves the explicit enabled-model identifier set for one provider using persisted preferences or first-save defaults.
/// 使用持久化偏好或首次保存默认值，为某个供应商解析显式启用模型标识集合。
pub fn enabled_model_ids(
    provider_id: &str,
    config: &ProviderConfigurationFile,
) -> BTreeSet<String> {
    let catalog = catalog_model_choices(provider_id, config);
    resolved_enabled_model_ids(config.provider.get(provider_id), &catalog)
}

/// Parses the active provider/model tuple from the persisted global model string.
/// 从持久化的全局模型字符串中解析激活的供应商与模型二元组。
pub fn parse_active_model_reference(model: &str) -> Option<(&str, &str)> {
    let (provider_id, model_id) = model.split_once('/')?;
    Some((provider_id, model_id))
}

/// Returns the remembered OpenRouter endpoint route selection for one exact model identifier.
/// 返回某个精确模型标识已记忆的 OpenRouter 端点路由选择。
pub fn openrouter_endpoint_route_for_model(
    config: &ProviderConfigurationFile,
    model_id: &str,
) -> Option<OpenRouterEndpointRouteSelection> {
    config
        .openrouter_endpoint_routes
        .get(model_id)
        .cloned()
        .filter(|selection| !selection.is_empty())
}

/// Persists or clears one remembered OpenRouter endpoint route selection for the given model identifier.
/// 为指定模型标识持久化保存或清除一条已记忆的 OpenRouter 端点路由选择。
pub fn set_openrouter_endpoint_route_for_model(
    config: &mut ProviderConfigurationFile,
    model_id: impl Into<String>,
    selection: Option<OpenRouterEndpointRouteSelection>,
) {
    let model_id = model_id.into();
    if let Some(selection) = selection.filter(|selection| !selection.is_empty()) {
        config
            .openrouter_endpoint_routes
            .insert(model_id, selection);
    } else {
        config.openrouter_endpoint_routes.remove(model_id.as_str());
    }
}
