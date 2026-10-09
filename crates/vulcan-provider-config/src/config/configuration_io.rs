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
    /// Records whether the workbench reasoning preference was explicitly chosen by the user.
    /// 记录工作台思考偏好是否由用户明确选择，用于区分旧版默认值与显式关闭。
    #[serde(
        default,
        rename = "workbenchReasoningEffortExplicit",
        skip_serializing_if = "is_false"
    )]
    pub workbench_reasoning_effort_explicit: bool,
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
/// The hydrated current document, after any recoverable legacy migration has completed.
/// 已完成可恢复旧版迁移后的补水当前文档。
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
/// The hydrated current document, after any legacy bytes are transactionally rewritten.
/// 返回已补水的当前文档；任何旧版字节均已通过事务完成重写。
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
    let exists = path
        .try_exists()
        .map_err(|error| format!("探测公共 AI 连接文件失败（{}）：{error}", path.display()))?;
    if !exists {
        return Ok(PublicAiConnectionsFile::default());
    }
    let text = read_bounded_configuration_text(path, "公共 AI 连接文件")?;
    let mut file = parse_public_ai_connections_text(&text)
        .map_err(|error| format!("解析公共 AI 连接文件失败（{}）：{error}", path.display()))?;
    let had_central_custom_providers = !file.configuration.custom_providers.is_empty();
    let schema_migrated = file.configuration.migrate_to_current_schema()?;
    // Standalone custom provider manifests own the provider bytes after migration; every
    // loader hydrates the reference set back into the in-memory map so runtime reads keep
    // the same authoritative view regardless of the caller (plan §R6.2).
    // 迁移后独立自定义供应商清单拥有供应商字节；所有加载路径都把引用集补水回内存映射，
    // 使运行时读取无论调用方是谁都保持相同的权威视图（方案 §R6.2）。
    let hydration_report = hydrate_public_ai_custom_providers(&store, &mut file.configuration)?;
    let migration_required = had_central_custom_providers
        || schema_migrated
        || !hydration_report.legacy_descriptor_provider_ids.is_empty();
    if migration_required && hydration_report.failure_total_count == 0 {
        // backupDigest pins the first complete pre-migration image before the journaled target
        // transaction can touch either the central document or standalone manifests.
        // backupDigest 在带日志的目标事务触碰中央文档或独立 Manifest 前，固定首次完整迁移前
        // 镜像。
        let backup_digest = crate::custom_manifest::ensure_custom_provider_migration_backup(
            &store,
            Some(text.as_str()),
        )?;
        // Existing standalone references are hydrated before this transaction, so a mixed
        // legacy/current document migrates the union and never drops an already externalized
        // provider. The central target and every provider file commit as one journaled image.
        // 在执行本事务前先补水现有独立引用，因此新旧混合文档迁移的是并集，绝不会丢失
        // 已外置供应商；中央目标与全部供应商文件作为一份带日志镜像共同提交。
        save_public_ai_connections_at_path(&file.configuration, path, &store)?;
        file.configuration =
            crate::custom_manifest::project_persisted_configuration(&file.configuration);
        hydrate_public_ai_custom_providers(&store, &mut file.configuration)?;
        crate::custom_manifest::mark_custom_provider_migration_complete(
            &store,
            backup_digest.as_str(),
        )?;
    } else if hydration_report.failure_total_count == 0
        && let Some(backup_digest) =
            crate::custom_manifest::custom_provider_migration_backup_digest(&store)?
    {
        // A crash after the data transaction but before the tiny marker write is reconciled only
        // after the current loader has proved that no legacy source remains.
        // 数据事务完成而微小标记写入前发生崩溃时，仅在当前加载器证明不存在旧来源后补写
        // 标记。
        crate::custom_manifest::mark_custom_provider_migration_complete(
            &store,
            backup_digest.as_str(),
        )?;
    }
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
            "公共 AI 连接文件超过 {PERSISTED_CONFIGURATION_MAX_BYTES} 字节上限。"
        ));
    }
    let file: PublicAiConnectionsFile = toml::from_str(text).map_err(|error| error.to_string())?;
    if file.schema_version != 1 {
        return Err(format!(
            "不支持公共 AI 连接文件 Schema 版本 {}，当前仅支持版本 1。",
            file.schema_version
        ));
    }
    // Optional old settings retain memory defaults, but explicit invalid media policies fail closed.
    // 旧版缺失设置保留内存默认值，但显式无效媒体策略必须拒绝。
    file.configuration
        .effective_media_delivery_policy()
        .validate()?;
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
    let file = fs::File::open(path)
        .map_err(|error| format!("读取{document_label}失败（{}）：{error}", path.display()))?;
    // metadata supplies the allocation and regular-file boundary from the opened handle itself.
    // metadata 从已打开句柄本身提供分配与普通文件边界。
    let metadata = file.metadata().map_err(|error| {
        format!(
            "读取{document_label}元数据失败（{}）：{error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "{document_label}路径不是普通文件（{}）。",
            path.display()
        ));
    }
    if metadata.len() > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "{document_label}超过 {PERSISTED_CONFIGURATION_MAX_BYTES} 字节上限（{}）。",
            path.display()
        ));
    }
    // bytes never preallocates beyond the already validated persisted-document ceiling.
    // bytes 的预分配绝不会超过已校验的持久化文档上限。
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    // limited detects concurrent growth with one sentinel byte instead of allocating the new size.
    // limited 用一个哨兵字节检测并发增长，而不会按新大小分配内存。
    let mut limited = file.take(PERSISTED_CONFIGURATION_MAX_BYTES.saturating_add(1));
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取{document_label}失败（{}）：{error}", path.display()))?;
    if bytes.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "{document_label}在读取期间超过 {PERSISTED_CONFIGURATION_MAX_BYTES} 字节上限（{}）。",
            path.display()
        ));
    }
    String::from_utf8(bytes).map_err(|error| {
        format!(
            "{document_label}不是有效 UTF-8（{}）：{error}",
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
        .map_err(|error| format!("序列化公共 AI 连接文件失败：{error}"))?;
    if content.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "公共 AI 连接文件超过 {PERSISTED_CONFIGURATION_MAX_BYTES} 字节上限。"
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
    let path_exists = path
        .try_exists()
        .map_err(|error| format!("探测项目配置文件失败（{}）：{}。", path.display(), error))?;
    if !path_exists {
        return Ok(ProjectVulcanCodeConfigurationFile::default());
    }

    // text is the exact persisted JSON document parsed below.
    // text 是下方执行解析的精确持久化 JSON 文档。
    let text = read_bounded_configuration_text(&path, "项目配置文件")?;
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
    if content.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "项目配置文件超过 {PERSISTED_CONFIGURATION_MAX_BYTES} 字节上限（{}）。",
            path.display()
        ));
    }
    crate::atomic_file::write_file_atomically(&path, content.as_bytes())
        .map_err(|error| format!("写入项目配置失败：{error}。"))
}

/// Loads the persisted provider configuration file from disk.
/// 从磁盘加载持久化的供应商配置文件。
pub fn load_configuration_file() -> Result<ProviderConfigurationFile, String> {
    let path = configuration_file_path();

    // path_exists preserves access/probe failures instead of silently replacing them with defaults.
    // path_exists 保留访问与探测失败，不会静默用默认值替代这些失败。
    let path_exists = path
        .try_exists()
        .map_err(|error| format!("探测供应商配置文件失败（{}）：{error}。", path.display()))?;
    if !path_exists {
        return Ok(ProviderConfigurationFile::default());
    }

    let text = read_bounded_configuration_text(&path, "供应商配置文件")?;

    serde_json::from_str(&text).map_err(|error| format!("解析供应商配置文件失败：{}。", error))
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
        .map_err(|error| format!("序列化供应商配置失败：{}。", error))?;
    if content.len() as u64 > PERSISTED_CONFIGURATION_MAX_BYTES {
        return Err(format!(
            "供应商配置文件超过 {PERSISTED_CONFIGURATION_MAX_BYTES} 字节上限（{}）。",
            path.display()
        ));
    }
    crate::atomic_file::write_file_atomically(path, content.as_bytes())
        .map_err(|error| format!("写入供应商配置失败：{error}。"))
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

    /// Checked-in descriptor-only schema used to verify the real startup migration path.
    /// 用于验证真实启动迁移路径的签入式仅描述符 Schema。
    const LEGACY_CUSTOM_PROVIDER_FIXTURE: &str =
        include_str!("../../tests/fixtures/custom-provider-descriptor-v0.json");

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
        assert!(error.contains("字节上限"));
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
        assert!(error.contains("项目配置文件超过"));
        assert!(error.contains("字节上限"));
    }

    /// Verifies startup reads and rewrites custom manifests below the connection file's parent.
    /// 验证启动从连接文件父目录下读取并重写自定义 Manifest。
    #[test]
    fn startup_hydrates_matching_store_and_rewrites_legacy_descriptor() {
        // directory isolates both the central file and its co-owned manifest store.
        // directory 隔离中央文件及其共同持有的 Manifest 存储。
        let directory = tempfile::tempdir().expect("temporary root should create");
        // app_data_root, central_path, and store reproduce the production directory topology.
        // app_data_root、central_path 与 store 复现生产目录拓扑。
        let app_data_root = directory.path().join("code");
        let central_path = app_data_root.join("ai-connections.toml");
        let store = crate::manifest_store::ManifestStoreLayout::new(&app_data_root);
        // provider_id is the exact identity embedded in the historical fixture.
        // provider_id 是历史夹具内嵌的精确身份。
        let provider_id = "custom:fixture-provider";
        store
            .write_custom_manifest(provider_id, LEGACY_CUSTOM_PROVIDER_FIXTURE.as_bytes())
            .expect("legacy descriptor should write beside the central file");

        // persisted carries only the standalone reference, matching a post-migration restart.
        // persisted 只携带独立引用，匹配迁移后的重启状态。
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
            .expect("startup hydration and migration should succeed");
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

        // migrated_value proves the descriptor-only source was transactionally replaced by v1.
        // migrated_value 证明仅描述符源已通过事务替换为 v1。
        let migrated_text = store
            .read_custom_manifest(provider_id)
            .expect("migrated document should be readable")
            .expect("migrated document should remain present");
        let migrated_value: serde_json::Value =
            serde_json::from_str(&migrated_text).expect("migrated document should be JSON");
        assert_eq!(
            migrated_value
                .get("schema_version")
                .and_then(serde_json::Value::as_str),
            Some(crate::custom_manifest::CUSTOM_PROVIDER_MANIFEST_SCHEMA_VERSION)
        );
        assert!(migrated_value.get("provider_manifest").is_some());
        assert!(!directory.path().join("provider-manifests").exists());

        // backup and marker prove successful migration retained its exact first before-image.
        // backup 与 marker 证明成功迁移保留了精确首次前镜像。
        let backup_path = crate::custom_manifest::custom_provider_migration_backup_path(&store);
        let backup_value: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&backup_path).expect("migration backup should remain durable"),
        )
        .expect("migration backup should decode");
        assert_eq!(
            backup_value
                .get("central_configuration")
                .and_then(serde_json::Value::as_str),
            Some(central_text.as_str())
        );
        assert_eq!(
            backup_value
                .pointer("/custom_manifests/custom:fixture-provider")
                .and_then(serde_json::Value::as_str),
            Some(LEGACY_CUSTOM_PROVIDER_FIXTURE)
        );
        assert!(crate::custom_manifest::custom_provider_migration_marker_path(&store).exists());

        // restarted proves the rewritten document survives a second full load without migration.
        // restarted 证明重写文档可在无需再次迁移的第二次完整加载中恢复。
        let restarted = load_public_ai_connections_file_from_path(&central_path)
            .expect("rewritten document should survive restart");
        assert!(
            restarted
                .configuration
                .custom_providers
                .contains_key(provider_id)
        );
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
        // healthyProviderId is the exact identity embedded in the frozen legacy fixture.
        // healthyProviderId 是冻结历史夹具内嵌的精确身份。
        let healthy_provider_id = "custom:fixture-provider";
        // corruptProviderId is the independently referenced provider that must be isolated.
        // corruptProviderId 是必须独立隔离的被引用供应商。
        let corrupt_provider_id = "custom:corrupt-provider";
        store
            .write_custom_manifest(
                healthy_provider_id,
                LEGACY_CUSTOM_PROVIDER_FIXTURE.as_bytes(),
            )
            .expect("healthy legacy provider should write");
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
        // centralText is the exact before-image that failed migration must preserve.
        // centralText 是失败迁移必须保留的精确前镜像。
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
            Some(LEGACY_CUSTOM_PROVIDER_FIXTURE)
        );
        assert!(!crate::custom_manifest::custom_provider_migration_backup_path(&store).exists());
        assert!(!crate::custom_manifest::custom_provider_migration_marker_path(&store).exists());

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

    /// Verifies legacy reasoning values remain non-explicit while a new user choice round-trips with provenance.
    /// 验证旧版思考值保持非显式状态，而新的用户选择会连同来源标记完成往返。
    #[test]
    fn workbench_reasoning_choice_round_trips_with_explicit_provenance() {
        let legacy: ProviderConfigurationFile =
            serde_json::from_str(r#"{"workbenchReasoningEffort":"none"}"#)
                .expect("legacy provider configuration should parse");
        assert_eq!(
            legacy.workbench_reasoning_effort,
            Some(ReasoningEffortLevel::None)
        );
        assert!(!legacy.workbench_reasoning_effort_explicit);

        let explicit = ProviderConfigurationFile {
            workbench_reasoning_effort: Some(ReasoningEffortLevel::Max),
            workbench_reasoning_effort_explicit: true,
            ..ProviderConfigurationFile::default()
        };
        let serialized = serde_json::to_string(&explicit)
            .expect("explicit provider configuration should serialize");
        assert!(serialized.contains("\"workbenchReasoningEffortExplicit\":true"));
        let restored: ProviderConfigurationFile = serde_json::from_str(serialized.as_str())
            .expect("explicit provider configuration should restore");
        assert_eq!(
            restored.workbench_reasoning_effort,
            Some(ReasoningEffortLevel::Max)
        );
        assert!(restored.workbench_reasoning_effort_explicit);
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
