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

/// Versioned standalone public AI connection configuration.
/// 版本化独立公共 AI 连接配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicAiConnectionsFile {
    /// Exact standalone configuration schema version.
    /// 精确独立配置 Schema 版本。
    pub schema_version: u32,
    /// Persisted public AI connections and user choices.
    /// 持久化公共 AI 连接与用户选择。
    pub configuration: vulcan_provider_adapters::PersistedAiConfiguration,
}

impl Default for PublicAiConnectionsFile {
    /// Returns an empty first-version connection file.
    /// 返回空的第一版连接文件。
    fn default() -> Self {
        Self {
            schema_version: 1,
            configuration: vulcan_provider_adapters::PersistedAiConfiguration::default(),
        }
    }
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

/// Returns the sole standalone public AI connection file path.
/// 返回唯一的独立公共 AI 连接文件路径。
///
/// # Returns
/// 返回值
/// `~/.vulcan/code/ai-connections.toml`.
/// 返回 `~/.vulcan/code/ai-connections.toml`。
pub fn public_ai_connections_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_path_buf()
    });
    home.join(".vulcan")
        .join("code")
        .join("ai-connections.toml")
}

/// Loads the standalone version-one public AI connection file.
/// 加载独立的第一版公共 AI 连接文件。
///
/// # Returns
/// 返回值
/// An empty version-one file only when the exact path is missing.
/// 仅当精确路径缺失时返回空的第一版文件。
pub fn load_public_ai_connections_file() -> Result<PublicAiConnectionsFile, String> {
    let path = public_ai_connections_file_path();
    let exists = path
        .try_exists()
        .map_err(|error| format!("探测公共 AI 连接文件失败（{}）：{error}", path.display()))?;
    if !exists {
        return Ok(PublicAiConnectionsFile::default());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取公共 AI 连接文件失败（{}）：{error}", path.display()))?;
    parse_public_ai_connections_text(&text)
        .map_err(|error| format!("解析公共 AI 连接文件失败（{}）：{error}", path.display()))
}

/// Parses and validates one standalone public AI connection document.
/// 解析并校验一份独立公共 AI 连接文档。
///
/// # Parameters
/// 参数
/// `text` is the exact TOML document read from the authoritative path.
/// `text` 是从权威路径读取的精确 TOML 文档。
///
/// # Returns
/// 返回值
/// A version-one document, or an explicit schema/format error.
/// 返回第一版文档，或明确的 Schema/格式错误。
fn parse_public_ai_connections_text(text: &str) -> Result<PublicAiConnectionsFile, String> {
    let file: PublicAiConnectionsFile = toml::from_str(text).map_err(|error| error.to_string())?;
    if file.schema_version != 1 {
        return Err(format!(
            "不支持公共 AI 连接文件 Schema 版本 {}，当前仅支持版本 1。",
            file.schema_version
        ));
    }
    Ok(file)
}

/// Atomically saves the standalone version-one public AI connection file.
/// 原子保存独立的第一版公共 AI 连接文件。
///
/// # Parameters
/// 参数
/// `configuration` contains no secret material or runtime model catalogs.
/// `configuration` 不包含秘密材料或运行时模型目录。
pub fn save_public_ai_connections_file(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
) -> Result<(), String> {
    let path = public_ai_connections_file_path();
    let file = PublicAiConnectionsFile {
        schema_version: 1,
        configuration: configuration.clone(),
    };
    let content = toml::to_string_pretty(&file)
        .map_err(|error| format!("序列化公共 AI 连接文件失败：{error}"))?;
    crate::atomic_file::write_file_atomically(&path, content.as_bytes())
        .map_err(|error| format!("写入公共 AI 连接文件失败：{error}"))
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
    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("序列化供应商配置失败：{}。", error))?;
    crate::atomic_file::write_file_atomically(&path, content.as_bytes())
        .map_err(|error| format!("写入供应商配置失败：{error}。"))
}

#[cfg(test)]
mod public_ai_connections_tests {
    use super::{PublicAiConnectionsFile, parse_public_ai_connections_text};

    /// Verifies the standalone document round-trips without secrets or runtime catalogs.
    /// 验证独立文档往返时不包含秘密或运行时目录。
    #[test]
    fn standalone_public_ai_document_round_trips_without_runtime_state() {
        let expected = PublicAiConnectionsFile::default();
        let text = toml::to_string_pretty(&expected).expect("document must serialize");
        assert!(!text.contains("secret ="));
        assert!(!text.contains("catalog"));
        assert!(!text.contains("models"));
        let actual = parse_public_ai_connections_text(text.as_str())
            .expect("version-one document must parse");
        assert_eq!(actual.schema_version, expected.schema_version);
        assert_eq!(actual.configuration, expected.configuration);
    }

    /// Verifies unsupported schema revisions are rejected rather than migrated.
    /// 验证不受支持的 Schema 修订会被拒绝而不是迁移。
    #[test]
    fn unsupported_public_ai_schema_is_rejected() {
        let error = parse_public_ai_connections_text(
            "schema_version = 2\n\n[configuration.connection_graph]\n",
        )
        .expect_err("unsupported schema must fail");
        assert!(error.contains("Schema 版本 2"));
    }

    /// Verifies unknown fields are rejected instead of becoming compatibility aliases.
    /// 验证未知字段会被拒绝而不是变成兼容别名。
    #[test]
    fn unknown_public_ai_fields_are_rejected() {
        let error = parse_public_ai_connections_text(
            "schema_version = 1\nlegacy_provider = \"minimax\"\n\n[configuration.connection_graph]\n",
        )
        .expect_err("unknown fields must fail");
        assert!(error.contains("unknown field"));
    }
}
