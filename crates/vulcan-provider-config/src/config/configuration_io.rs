use super::*;
use std::io::Read;

#[cfg(test)]
mod media_delivery_tests;

/// Maximum bytes accepted for any persisted Vulcan Code configuration document.
/// 任一持久化 Vulcan Code 配置文档允许接受的最大字节数。
const PERSISTED_CONFIGURATION_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Returns whether a persisted boolean is false so default values stay omitted from JSON.
/// 判断持久化布尔值是否为 false，以便默认值继续从 JSON 中省略。
fn is_false(value: &bool) -> bool {
    !*value
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
    /// Whether bounded grouped Provider request and raw-response traces are written per session.
    /// 是否按会话写入有界分组 Provider 请求与原始返回追踪。
    #[serde(
        default,
        rename = "providerRequestTraceEnabled",
        skip_serializing_if = "is_false"
    )]
    pub provider_request_trace_enabled: bool,
    /// Retains failed session requests and raw responses in a separate bounded trace ring.
    /// 将失败的会话请求与原始响应保存在独立的有界追踪环中。
    #[serde(
        default,
        rename = "providerErrorTraceEnabled",
        skip_serializing_if = "is_false"
    )]
    pub provider_error_trace_enabled: bool,
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
    /// Default tool-group folding preference; absence preserves the existing enabled default.
    /// 工具分组默认折叠偏好；缺省时保留原有的默认开启行为。
    #[serde(
        default,
        rename = "toolCollapseDefaultEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_collapse_default_enabled: Option<bool>,
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
    /// Persisted prompt-composer configuration.
    /// 持久化的提示词组合配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedPromptConfiguration::is_default"
    )]
    pub prompt: PersistedPromptConfiguration,
    /// System-level permission judge settings and model registry.
    /// 系统级权限判定设置与模型登记表。
    #[serde(
        default,
        rename = "permissionJudge",
        skip_serializing_if = "PersistedPermissionJudgeConfiguration::is_default"
    )]
    pub permission_judge: PersistedPermissionJudgeConfiguration,
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
    let home = dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    runtime_assets::user_vulcan_root_for_home(&home).join("vulcan-code.json")
}

/// Returns the sole standalone public AI connection file path.
/// 返回唯一的独立公共 AI 连接文件路径。
///
/// # Returns
/// 返回值
/// `<user-vulcan-root>/code/ai-connections.toml`.
/// 返回 `<user-vulcan-root>/code/ai-connections.toml`。
pub fn public_ai_connections_file_path() -> PathBuf {
    let home = dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    runtime_assets::user_vulcan_code_root_for_home(&home).join("ai-connections.toml")
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
    load_public_ai_connections_file_at_path(&path)
}

/// Loads one public AI connection file from an exact path and its co-owned manifest store.
/// 从精确路径及其共同持有的 Manifest 存储加载公共 AI 连接文件。
///
/// # Parameters
/// 参数
/// `path` is the central connection document whose parent owns the standalone manifest store.
/// `path` 是中央连接文档，其父目录持有独立 Manifest 存储。
///
/// # Returns
/// 返回值
/// The hydrated current document, or an explicit error for an unsupported document.
/// 已补水的当前文档；不受支持的文档返回明确错误。
pub fn load_public_ai_connections_file_at_path(
    path: &Path,
) -> Result<PublicAiConnectionsFile, String> {
    load_public_ai_connections_file_from_path(path)
}

/// Loads one public-AI connection file from an exact path and its co-owned manifest store.
/// 从精确路径及其共同持有的 Manifest 存储加载公共 AI 连接文件。
///
/// # Parameters
/// 参数
/// `path` is the central connection document; its parent is the sole custom-manifest store root.
/// `path` 是中央连接文档；其父目录是唯一的自定义 Manifest 存储根。
///
/// # Returns
/// 返回值
/// The hydrated current document; the load never rewrites persisted bytes.
/// 返回已补水的当前文档；加载绝不改写持久化字节。
fn load_public_ai_connections_file_from_path(
    path: &Path,
) -> Result<PublicAiConnectionsFile, String> {
    // A durable custom-provider journal is replayed before the central file is inspected, so
    // hydration can never observe half of a prior cross-file commit.
    // 在检查中央文件前重放持久自定义供应商日志，使补水绝不观察到此前跨文件提交的一半。
    let store = crate::manifest_store::ManifestStoreLayout::new(
        &path.parent().map(Path::to_path_buf).unwrap_or_default(),
    );
    crate::custom_manifest::recover_custom_provider_transaction(&store, path)?;
    let exists = path.try_exists().map_err(|error| {
        format!(
            "Failed to probe the public AI connections file ({}): {error}",
            path.display()
        )
    })?;
    if !exists {
        return Ok(PublicAiConnectionsFile::default());
    }
    let text = read_bounded_configuration_text(path, "public AI connections file")?;
    let mut file = parse_public_ai_connections_text(&text).map_err(|error| {
        format!(
            "Failed to parse the public AI connections file ({}): {error}",
            path.display()
        )
    })?;
    // Standalone custom provider manifests own the provider bytes; every loader hydrates the
    // reference set back into the in-memory map so runtime reads keep one authoritative view.
    // 独立自定义供应商清单拥有供应商字节；所有加载路径都把引用集补水回内存映射，使运行时
    // 读取保持同一权威视图。
    hydrate_public_ai_custom_providers(&store, &mut file.configuration)?;
    Ok(file)
}

/// Hydrates the in-memory custom provider map from standalone manifest files.
/// 从独立清单文件为内存自定义供应商映射补水。
///
/// # Parameters
/// 参数
/// `store` is derived from the central file's parent; `configuration` is the parsed persisted
/// document that may carry only the reference set.
/// `store` 从中央文件父目录派生；`configuration` 是已解析的持久化文档，可能只携带引用集。
///
/// # Returns
/// 返回值
/// A hydration report after every referenced file loads; a missing file is a controlled failure
/// and descriptor-only sources are reported for transactional rewriting.
/// 每个被引用文件加载后返回补水报告；文件缺失时受控失败，仅描述符源会被报告以供事务重写。
fn hydrate_public_ai_custom_providers(
    store: &crate::manifest_store::ManifestStoreLayout,
    configuration: &mut vulcan_provider_adapters::PersistedAiConfiguration,
) -> Result<crate::custom_manifest::CustomProviderHydrationReport, String> {
    if configuration.custom_provider_file_refs.is_empty() {
        return Ok(crate::custom_manifest::CustomProviderHydrationReport::default());
    }
    crate::custom_manifest::load_custom_provider_manifests(store, configuration)
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
    if text.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "The public AI connections file exceeds the {PERSISTED_CONFIGURATION_MAX_BYTES}-byte limit."
        ));
    }
    let file: PublicAiConnectionsFile = toml::from_str(text).map_err(|error| error.to_string())?;
    if file.schema_version != 1 {
        return Err(format!(
            "Unsupported public AI connections file schema version {}; only version 1 is supported.",
            file.schema_version
        ));
    }
    // Custom providers are owned by standalone manifests; the central file carries only references.
    // 自定义供应商由独立 Manifest 持有；中央文件只携带引用。
    if !file.configuration.custom_providers.is_empty() {
        return Err(String::from(
            "The central public AI connections file must not embed custom provider definitions; it may only reference standalone manifests.",
        ));
    }
    file.configuration.validate_current_schema()?;
    Ok(file)
}

/// Reads one persisted configuration document under a post-open byte ceiling.
/// 在打开后的字节上限内读取一份持久化配置文档。
///
/// # Parameters
/// 参数
/// `path` is the exact host-owned configuration path and `document_label` names it in diagnostics.
/// `path` 是宿主持有的精确配置路径，`document_label` 用于在诊断中命名该文档。
///
/// # Returns
/// 返回值
/// Exact UTF-8 text, or a controlled type, size, I/O, replacement, or encoding failure.
/// 返回精确 UTF-8 文本，或受控的类型、大小、I/O、替换及编码失败。
fn read_bounded_configuration_text(path: &Path, document_label: &str) -> Result<String, String> {
    // file freezes the opened object before metadata and body checks, avoiding path replacement drift.
    // file 在元数据与正文检查前冻结已打开对象，避免路径替换漂移。
    let file = fs::File::open(path).map_err(|error| {
        format!(
            "Failed to read the {document_label} ({}): {error}",
            path.display()
        )
    })?;
    // metadata supplies the allocation and regular-file boundary from the opened handle itself.
    // metadata 从已打开句柄本身提供分配与普通文件边界。
    let metadata = file.metadata().map_err(|error| {
        format!(
            "Failed to read metadata of the {document_label} ({}): {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "The {document_label} path is not a regular file ({}).",
            path.display()
        ));
    }
    if metadata.len() > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "The {document_label} exceeds the {PERSISTED_CONFIGURATION_MAX_BYTES}-byte limit ({}).",
            path.display()
        ));
    }
    // bytes never preallocates beyond the already validated persisted-document ceiling.
    // bytes 的预分配绝不会超过已校验的持久化文档上限。
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    // limited detects concurrent growth with one sentinel byte instead of allocating the new size.
    // limited 用一个哨兵字节检测并发增长，而不会按新大小分配内存。
    let mut limited = file.take(PERSISTED_CONFIGURATION_MAX_BYTES.saturating_add(1));
    limited.read_to_end(&mut bytes).map_err(|error| {
        format!(
            "Failed to read the {document_label} ({}): {error}",
            path.display()
        )
    })?;
    if bytes.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "The {document_label} exceeded the {PERSISTED_CONFIGURATION_MAX_BYTES}-byte limit while being read ({}).",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        format!(
            "The {document_label} is not valid UTF-8 ({}): {error}",
            path.display()
        )
    })
}

/// Atomically persists standalone custom manifests and their central reference document.
/// 原子持久化独立自定义清单及其中央引用文档。
///
/// # Parameters
/// 参数
/// `configuration` is the authoritative hydrated snapshot and `app_data_root` owns both
/// `ai-connections.toml` and the standalone manifest store used by the service runtime.
/// `configuration` 是权威已补水快照，`app_data_root` 同时持有 `ai-connections.toml` 与服务
/// 运行时使用的独立清单存储。
///
/// # Returns
/// 返回值
/// Success only after the recoverable cross-file transaction has fully committed.
/// 仅在可恢复跨文件事务完整提交后返回成功。
pub fn save_public_ai_connections_with_custom_manifests(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    app_data_root: &Path,
) -> Result<(), String> {
    let path = app_data_root.join("ai-connections.toml");
    let store = crate::manifest_store::ManifestStoreLayout::new(app_data_root);
    save_public_ai_connections_at_path(configuration, &path, &store)
}

/// Persists one hydrated configuration to an exact central path and matching manifest store.
/// 将一份已补水配置持久化到精确中央路径及匹配的 Manifest 存储。
///
/// # Parameters
/// 参数
/// `configuration` is authoritative, `path` owns the central document, and `store` owns every
/// standalone custom document participating in the same transaction.
/// `configuration` 是权威快照，`path` 持有中央文档，`store` 持有参与同一事务的全部独立
/// 自定义文档。
///
/// # Returns
/// 返回值
/// Success only after the complete target image is durable and its journal is removed.
/// 仅在完整目标镜像持久化且事务日志已移除后返回成功。
fn save_public_ai_connections_at_path(
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    path: &Path,
    store: &crate::manifest_store::ManifestStoreLayout,
) -> Result<(), String> {
    // Validate before serializing or starting the cross-file persistence transaction.
    // 在序列化或开始跨文件持久化事务之前完成校验。
    configuration.effective_media_delivery_policy().validate()?;
    let projected = crate::custom_manifest::project_persisted_configuration(configuration);
    let file = PublicAiConnectionsFile {
        schema_version: 1,
        configuration: projected,
    };
    let content = toml::to_string_pretty(&file)
        .map_err(|error| format!("Failed to serialize the public AI connections file: {error}"))?;
    if content.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "The public AI connections file exceeds the {PERSISTED_CONFIGURATION_MAX_BYTES}-byte limit."
        ));
    }
    crate::custom_manifest::persist_custom_provider_transaction(
        store,
        path,
        configuration,
        content.as_bytes(),
    )
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
    let path_exists = path.try_exists().map_err(|error| {
        format!(
            "Failed to probe the project configuration file ({}): {}.",
            path.display(),
            error
        )
    })?;
    if !path_exists {
        return Ok(ProjectVulcanCodeConfigurationFile::default());
    }

    // text is the exact persisted JSON document parsed below.
    // text 是下方执行解析的精确持久化 JSON 文档。
    let text = read_bounded_configuration_text(&path, "project configuration file")?;
    serde_json::from_str(&text).map_err(|error| {
        format!(
            "Failed to parse the project configuration file ({}): {}.",
            path.display(),
            error
        )
    })
}

/// Saves one project-local Vulcan Code configuration file back to disk.
/// 将单个项目级 Vulcan Code 配置文件写回磁盘。
pub fn save_project_configuration_file(
    workspace_path: &Path,
    config: &ProjectVulcanCodeConfigurationFile,
) -> Result<(), String> {
    let path = project_configuration_file_path(workspace_path);
    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("Failed to serialize the project configuration: {}.", error))?;
    if content.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "The project configuration file exceeds the {PERSISTED_CONFIGURATION_MAX_BYTES}-byte limit ({}).",
            path.display()
        ));
    }
    crate::atomic_file::write_file_atomically(&path, content.as_bytes())
        .map_err(|error| format!("Failed to write the project configuration: {error}."))
}

/// Loads the persisted provider configuration file from disk.
/// 从磁盘加载持久化的供应商配置文件。
pub fn load_configuration_file() -> Result<ProviderConfigurationFile, String> {
    let path = configuration_file_path();

    // path_exists preserves access/probe failures instead of silently replacing them with defaults.
    // path_exists 保留访问与探测失败，不会静默用默认值替代这些失败。
    let path_exists = path.try_exists().map_err(|error| {
        format!(
            "Failed to probe the provider configuration file ({}): {error}.",
            path.display()
        )
    })?;
    if !path_exists {
        return Ok(ProviderConfigurationFile::default());
    }

    let text = read_bounded_configuration_text(&path, "provider configuration file")?;

    serde_json::from_str(&text).map_err(|error| {
        format!(
            "Failed to parse the provider configuration file: {}.",
            error
        )
    })
}

/// Saves the provider configuration file back to disk, creating parent directories when needed.
/// 将供应商配置文件写回磁盘，并在需要时创建父目录。
/// `config` is the complete configuration; returns success only after atomic persistence.
/// `config` 是完整配置；仅在原子持久化成功后返回成功。
pub fn save_configuration_file(config: &ProviderConfigurationFile) -> Result<(), String> {
    save_configuration_file_at_path(config, &configuration_file_path())
}

/// Saves the complete configuration to an explicit path using the standard bounded atomic writer.
/// 使用标准的受限原子写入器，将完整配置保存到明确指定的路径。
/// `config` supplies the document and `path` identifies its exact destination, including the filename.
/// `config` 提供文档，`path` 指定包含文件名在内的精确目标位置。
/// Returns success after persistence, or the serialization, size-limit, or filesystem error.
/// 持久化成功后返回成功，否则返回序列化、大小限制或文件系统错误。
pub fn save_configuration_file_at_path(
    config: &ProviderConfigurationFile,
    path: &Path,
) -> Result<(), String> {
    // Share serialization and bounds with production so isolated tests exercise the real writer.
    // 与生产共享序列化与大小限制，让隔离测试执行真实写入器。
    let content = serde_json::to_string_pretty(config)
        .map_err(|error| format!("Failed to serialize the provider configuration: {}.", error))?;
    if content.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "The provider configuration file exceeds the {PERSISTED_CONFIGURATION_MAX_BYTES}-byte limit ({}).",
            path.display()
        ));
    }
    crate::atomic_file::write_file_atomically(path, content.as_bytes())
        .map_err(|error| format!("Failed to write the provider configuration: {error}."))
}

#[cfg(test)]
mod public_ai_connections_tests {
    use std::collections::BTreeSet;

    use super::{
        PERSISTED_CONFIGURATION_MAX_BYTES, ProviderConfigurationFile, PublicAiConnectionsFile,
        ReasoningEffortLevel, load_project_configuration_file,
        load_public_ai_connections_file_at_path, load_public_ai_connections_file_from_path,
        parse_public_ai_connections_text, save_public_ai_connections_at_path,
    };

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
        assert!(error.contains("schema version 2"));
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

    /// Verifies an oversized central connection document is rejected before body allocation.
    /// 验证超大中央连接文档会在分配正文前被拒绝。
    #[test]
    fn oversized_public_ai_connection_file_is_rejected_before_reading() {
        let directory = tempfile::tempdir().expect("temporary root should create");
        let path = directory.path().join("ai-connections.toml");
        let file = std::fs::File::create(&path).expect("sparse fixture should create");
        file.set_len(PERSISTED_CONFIGURATION_MAX_BYTES + 1)
            .expect("sparse fixture should resize");

        let error = load_public_ai_connections_file_from_path(&path)
            .expect_err("oversized central document must fail closed");
        assert!(error.contains("-byte limit"));
    }

    /// Verifies an oversized project configuration is rejected before body allocation.
    /// 验证超大项目配置会在分配正文前被拒绝。
    #[test]
    fn oversized_project_configuration_file_is_rejected_before_reading() {
        // workspace owns the exact project-local configuration topology used by production.
        // workspace 持有生产使用的精确项目本地配置拓扑。
        let workspace = tempfile::tempdir().expect("temporary workspace should create");
        // path resolves through the production path builder rather than a test-only alias.
        // path 通过生产路径构建器解析，而不是使用测试专属别名。
        let path = super::project_configuration_file_path(workspace.path());
        std::fs::create_dir_all(path.parent().expect("configuration parent should exist"))
            .expect("configuration parent should create");
        // file is sparse so the boundary test remains fast while preserving the exact metadata size.
        // file 使用稀疏表示，使边界测试保持快速并保留精确元数据大小。
        let file = std::fs::File::create(&path).expect("sparse fixture should create");
        file.set_len(PERSISTED_CONFIGURATION_MAX_BYTES + 1)
            .expect("sparse fixture should resize");

        // error proves allocation is rejected before the sparse body can be consumed.
        // error 证明稀疏正文被消费前即拒绝分配。
        let error = load_project_configuration_file(workspace.path())
            .expect_err("oversized project configuration must fail closed");
        assert!(error.contains("project configuration file exceeds"));
        assert!(error.contains("-byte limit"));
    }

    /// Verifies startup reads custom manifests below the connection file's parent without rewriting them.
    /// 验证启动从连接文件父目录下读取自定义 Manifest 且不改写它们。
    #[test]
    fn startup_hydrates_matching_store_without_rewriting() {
        // directory isolates both the central file and its co-owned manifest store.
        // directory 隔离中央文件及其共同持有的 Manifest 存储。
        let directory = tempfile::tempdir().expect("temporary root should create");
        // app_data_root, central_path, and store reproduce the production directory topology.
        // app_data_root、central_path 与 store 复现生产目录拓扑。
        let app_data_root = directory.path().join("code");
        let central_path = app_data_root.join("ai-connections.toml");
        let store = crate::manifest_store::ManifestStoreLayout::new(&app_data_root);
        let provider_id = "custom:fixture-provider";
        let document_text = crate::custom_manifest::tests::document_text(provider_id);
        store
            .write_custom_manifest(provider_id, document_text.as_bytes())
            .expect("current document should write beside the central file");

        // persisted carries only the standalone reference.
        // persisted 只携带独立引用。
        let mut persisted = PublicAiConnectionsFile::default();
        persisted
            .configuration
            .custom_provider_file_refs
            .insert(provider_id.to_string());
        let central_text =
            toml::to_string_pretty(&persisted).expect("central document should serialize");
        std::fs::write(&central_path, &central_text).expect("central document should write");

        // loaded proves hydration used the exact store associated with central_path.
        // loaded 证明补水使用了与 central_path 关联的精确存储。
        let loaded = load_public_ai_connections_file_at_path(&central_path)
            .expect("startup hydration should succeed");
        assert!(
            loaded
                .configuration
                .custom_providers
                .contains_key(provider_id)
        );
        assert!(
            loaded
                .configuration
                .custom_provider_manifests
                .contains_key(provider_id)
        );
        assert!(!directory.path().join("provider-manifests").exists());
        assert_eq!(
            std::fs::read_to_string(&central_path).expect("central document should read"),
            central_text
        );
        assert_eq!(
            store
                .read_custom_manifest(provider_id)
                .expect("standalone document should read")
                .as_deref(),
            Some(document_text.as_str())
        );
    }

    /// Verifies a central document embedding custom provider definitions is rejected.
    /// 验证内嵌自定义供应商定义的中央文档会被拒绝。
    #[test]
    fn central_document_with_inline_custom_providers_is_rejected() {
        let mut persisted = PublicAiConnectionsFile::default();
        persisted.configuration.custom_providers.insert(
            String::from("custom:inline"),
            crate::custom_manifest::tests::custom_descriptor("custom:inline"),
        );
        let text = toml::to_string_pretty(&persisted).expect("central document should serialize");
        assert!(parse_public_ai_connections_text(text.as_str()).is_err());
    }

    /// Verifies one corrupt custom source is preserved by healthy saves until explicit removal.
    /// 验证一个损坏自定义源会被健康保存保留，直至显式删除。
    #[test]
    fn startup_isolates_corrupt_custom_provider_until_explicit_removal() {
        // directory owns the exact central document and both standalone provider sources.
        // directory 持有精确中央文档与两个独立供应商源。
        let directory = tempfile::tempdir().expect("temporary root should create");
        // appDataRoot is the production-shaped persistence root under the isolated fixture.
        // appDataRoot 是隔离夹具下与生产形态一致的持久化根。
        let app_data_root = directory.path().join("code");
        // centralPath is the single authoritative public-AI configuration file.
        // centralPath 是唯一权威公共 AI 配置文件。
        let central_path = app_data_root.join("ai-connections.toml");
        // store owns the standalone custom-provider namespace associated with centralPath.
        // store 持有与 centralPath 关联的独立自定义供应商命名空间。
        let store = crate::manifest_store::ManifestStoreLayout::new(&app_data_root);
        // healthyProviderId owns one complete current standalone document.
        // healthyProviderId 持有一份完整的当前独立文档。
        let healthy_provider_id = "custom:fixture-provider";
        let healthy_document = crate::custom_manifest::tests::document_text(healthy_provider_id);
        // corruptProviderId is the independently referenced provider that must be isolated.
        // corruptProviderId 是必须独立隔离的被引用供应商。
        let corrupt_provider_id = "custom:corrupt-provider";
        store
            .write_custom_manifest(healthy_provider_id, healthy_document.as_bytes())
            .expect("healthy provider should write");
        store
            .write_custom_manifest(corrupt_provider_id, br#"{"schema_version":"broken"}"#)
            .expect("corrupt provider fixture should write");

        // persisted carries both authoritative standalone references without embedded fallback.
        // persisted 携带两个权威独立引用，不含内嵌回退。
        let mut persisted = PublicAiConnectionsFile::default();
        persisted.configuration.custom_provider_file_refs.extend([
            healthy_provider_id.to_string(),
            corrupt_provider_id.to_string(),
        ]);
        // centralText is the exact before-image that loading must preserve.
        // centralText 是加载必须保留的精确前镜像。
        let central_text =
            toml::to_string_pretty(&persisted).expect("central document should serialize");
        std::fs::write(&central_path, central_text.as_bytes())
            .expect("central document should write");

        // loaded keeps the healthy provider usable while exposing the exact isolated sibling.
        // loaded 使健康供应商保持可用，同时暴露精确隔离同级项。
        let mut loaded = load_public_ai_connections_file_from_path(&central_path)
            .expect("one corrupt provider must not fail the whole service configuration");
        assert!(
            loaded
                .configuration
                .custom_providers
                .contains_key(healthy_provider_id)
        );
        assert!(
            !loaded
                .configuration
                .custom_providers
                .contains_key(corrupt_provider_id)
        );
        assert_eq!(
            loaded
                .configuration
                .custom_provider_manifest_failure_total_count,
            1
        );
        assert_eq!(
            loaded.configuration.custom_provider_manifest_failures[0].provider_id,
            corrupt_provider_id
        );
        assert_eq!(
            std::fs::read_to_string(&central_path).expect("central before-image should remain"),
            central_text
        );
        assert_eq!(
            store
                .read_custom_manifest(healthy_provider_id)
                .expect("healthy source should remain readable")
                .as_deref(),
            Some(healthy_document.as_str())
        );

        loaded.configuration.usage_refresh_interval_seconds = 601;
        save_public_ai_connections_at_path(&loaded.configuration, &central_path, &store)
            .expect("a healthy setting save should preserve the isolated provider source");
        let saved_central_text =
            std::fs::read_to_string(&central_path).expect("saved central bytes should read");
        assert_ne!(saved_central_text, central_text);
        let saved = parse_public_ai_connections_text(saved_central_text.as_str())
            .expect("saved central document should remain valid");
        assert_eq!(saved.configuration.usage_refresh_interval_seconds, 601);
        assert_eq!(
            saved.configuration.custom_provider_file_refs,
            BTreeSet::from([
                healthy_provider_id.to_string(),
                corrupt_provider_id.to_string(),
            ])
        );
        assert_eq!(
            store
                .read_custom_manifest(corrupt_provider_id)
                .expect("healthy save must preserve corrupt source")
                .as_deref(),
            Some(r#"{"schema_version":"broken"}"#)
        );
        // healthySourceBeforeRemoval freezes the exact migrated sibling bytes for the delete check.
        // healthySourceBeforeRemoval 冻结删除校验所需的健康同级迁移后精确字节。
        let healthy_source_before_removal = store
            .read_custom_manifest(healthy_provider_id)
            .expect("healthy sibling should remain readable")
            .expect("healthy sibling should exist");

        // Explicit removal clears the exact reference and lets the same transaction delete only
        // that provider's source while preserving its healthy sibling.
        // 显式删除会清理精确引用，并让同一事务仅删除该供应商源，同时保留健康同级项。
        assert!(
            loaded
                .configuration
                .remove_custom_provider_owned_state(corrupt_provider_id)
                .is_none(),
            "an isolated provider intentionally has no hydrated descriptor"
        );
        save_public_ai_connections_at_path(&loaded.configuration, &central_path, &store)
            .expect("explicit isolated-provider removal should persist atomically");
        let removed_central_text =
            std::fs::read_to_string(&central_path).expect("removed central bytes should read");
        let removed = parse_public_ai_connections_text(removed_central_text.as_str())
            .expect("central document after removal should remain valid");
        assert_eq!(
            removed.configuration.custom_provider_file_refs,
            BTreeSet::from([healthy_provider_id.to_string()])
        );
        assert_eq!(
            store
                .read_custom_manifest(corrupt_provider_id)
                .expect("removed source lookup should succeed"),
            None
        );
        assert_eq!(
            store
                .read_custom_manifest(healthy_provider_id)
                .expect("healthy sibling should remain readable")
                .as_deref(),
            Some(healthy_source_before_removal.as_str())
        );
    }

    /// Verifies a stored workbench reasoning choice, including `none`, round-trips.
    /// 验证已存工作台思考选择（包括 `none`）可完成往返。
    #[test]
    fn workbench_reasoning_choice_round_trips() {
        let explicit = ProviderConfigurationFile {
            workbench_reasoning_effort: Some(ReasoningEffortLevel::None),
            ..ProviderConfigurationFile::default()
        };
        let serialized =
            serde_json::to_string(&explicit).expect("provider configuration should serialize");
        let restored: ProviderConfigurationFile = serde_json::from_str(&serialized)
            .expect("serialized provider configuration should parse");
        assert_eq!(
            restored.workbench_reasoning_effort,
            Some(ReasoningEffortLevel::None)
        );
    }

    /// Verifies bounded Provider wire tracing defaults off and persists only after explicit enablement.
    /// 验证有界 Provider 线上追踪默认关闭，并且仅在显式启用后持久化。
    #[test]
    fn provider_request_trace_switch_defaults_off_and_round_trips_when_enabled() {
        let default_configuration: ProviderConfigurationFile =
            serde_json::from_str("{}").expect("default provider configuration should parse");
        assert!(!default_configuration.provider_request_trace_enabled);
        let default_json = serde_json::to_string(&default_configuration)
            .expect("default provider configuration should serialize");
        assert!(!default_json.contains("providerRequestTraceEnabled"));

        let enabled_configuration = ProviderConfigurationFile {
            provider_request_trace_enabled: true,
            ..ProviderConfigurationFile::default()
        };
        let enabled_json = serde_json::to_string(&enabled_configuration)
            .expect("enabled provider trace configuration should serialize");
        assert!(enabled_json.contains("\"providerRequestTraceEnabled\":true"));
        let restored: ProviderConfigurationFile = serde_json::from_str(enabled_json.as_str())
            .expect("enabled provider trace configuration should restore");
        assert!(restored.provider_request_trace_enabled);
    }
}
