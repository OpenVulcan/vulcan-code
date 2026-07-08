//! Subagent catalog loading and profile conversion.
//! 子代理目录加载与配置转换。

use crate::profile::SubagentProfile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};
use vulcan_session_core::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, ContextCompactionPolicyRequest,
    ExecutionRuntimePolicyRequest, PromptSkillSnapshot, ProviderModelSnapshot,
    ToolExposureSnapshot,
};

/// File extension used by user-facing subagent definition files.
/// 面向用户的子代理定义文件使用的扩展名。
const SUBAGENT_DEFINITION_EXTENSION: &str = "md";

/// User-owned shareable definition directory below the agent catalog root.
/// 智能体目录根目录下用户拥有的可分享定义目录。
const USER_DEFINITIONS_DIR_NAME: &str = "definitions";

/// Downloaded package cache directory below the agent catalog root.
/// 智能体目录根目录下已下载包缓存目录。
const USER_PACKAGES_DIR_NAME: &str = "packages";

/// User-owned override directory below the agent catalog root.
/// 智能体目录根目录下用户拥有的覆盖目录。
const USER_OVERRIDES_DIR_NAME: &str = "overrides";

/// Category sidecar metadata file name.
/// 分类 sidecar 元数据文件名。
const CATEGORY_METADATA_FILE_NAME: &str = "_category";

/// YAML sidecar extension.
/// YAML sidecar 扩展名。
const YAML_EXTENSION: &str = "yaml";

/// Short YAML sidecar extension.
/// YAML sidecar 短扩展名。
const YML_EXTENSION: &str = "yml";

/// JSON sidecar extension.
/// JSON sidecar 扩展名。
const JSON_EXTENSION: &str = "json";

/// Default agent emoji used when no metadata supplies one.
/// 没有元数据时使用的默认智能体图标。
const DEFAULT_AGENT_EMOJI: &str = "🤖";

/// Default agent color used when no metadata supplies one.
/// 没有元数据时使用的默认智能体颜色。
const DEFAULT_AGENT_COLOR: &str = "gray";

/// Subagent mode keys accepted by catalog files and spawn_agent.
/// 目录文件与 spawn_agent 接受的子代理模式键。
pub const SUBAGENT_MODE_DEFAULT: &str = "default";
/// Mirror subagent mode key accepted by catalog files and spawn_agent.
/// 目录文件与 spawn_agent 接受的镜像子代理模式键。
pub const SUBAGENT_MODE_MIRROR: &str = "mirror";
/// Expert subagent mode key accepted by catalog files and spawn_agent.
/// 目录文件与 spawn_agent 接受的专家子代理模式键。
pub const SUBAGENT_MODE_EXPERT: &str = "expert";

/// Loaded catalog scope for one subagent definition.
/// 单个子代理定义的已加载目录作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentDefinitionScope {
    /// Definition shipped by the application resources.
    /// 随应用资源发布的定义。
    System,
    /// Definition stored in the user data directory.
    /// 存储在用户数据目录中的定义。
    User,
    /// Definition stored in a downloaded package cache.
    /// 存储在已下载包缓存中的定义。
    Package,
}

/// Declarative subagent execution mode.
/// 声明式子代理执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentDefinitionMode {
    /// Ordinary child agent mode.
    /// 普通子代理模式。
    Default,
    /// Mirror child agent mode.
    /// 镜像子代理模式。
    Mirror,
    /// Expert child agent mode.
    /// 专家子代理模式。
    Expert,
}

impl SubagentDefinitionMode {
    /// Returns the stable mode key.
    /// 返回稳定模式键。
    ///
    /// # Returns
    /// 返回值
    /// A snake_case mode identifier used in persisted snapshots.
    /// 用于持久化快照的 snake_case 模式标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => SUBAGENT_MODE_DEFAULT,
            Self::Mirror => SUBAGENT_MODE_MIRROR,
            Self::Expert => SUBAGENT_MODE_EXPERT,
        }
    }

    /// Parses one strict subagent mode key.
    /// 解析一个严格的子代理模式键。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the raw mode text.
    /// `value` 是原始模式文本。
    ///
    /// # Returns
    /// 返回值
    /// The parsed mode, or `None` when the key is unsupported.
    /// 解析出的模式；若键不受支持则返回 `None`。
    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim() {
            SUBAGENT_MODE_DEFAULT => Some(Self::Default),
            SUBAGENT_MODE_MIRROR => Some(Self::Mirror),
            SUBAGENT_MODE_EXPERT => Some(Self::Expert),
            _ => None,
        }
    }
}

impl Default for SubagentDefinitionMode {
    /// Builds the default subagent mode.
    /// 构建默认子代理模式。
    fn default() -> Self {
        Self::Default
    }
}

/// Filesystem and remote provenance for one loaded definition.
/// 单个已加载定义的文件系统与远程来源信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionSource {
    /// Scope that supplied this definition.
    /// 提供该定义的作用域。
    pub scope: SubagentDefinitionScope,
    /// Markdown file path that supplied this definition.
    /// 提供该定义的 Markdown 文件路径。
    pub file: PathBuf,
    /// Optional sidecar metadata path that supplied configuration for this definition.
    /// 为该定义提供配置的可选 sidecar 元数据路径。
    pub sidecar_file: Option<PathBuf>,
    /// Stable source key that distinguishes system, package, and user definitions.
    /// 区分系统、包与用户定义的稳定来源键。
    pub source_key: String,
    /// Canonical category path inferred from the definition directory.
    /// 从定义目录推导出的规范分类路径。
    pub category_id: Option<String>,
    /// Optional package id when the definition comes from a downloaded package.
    /// 定义来自已下载包时的可选包标识。
    pub package_id: Option<String>,
    /// Optional remote URL that originally supplied this user definition.
    /// 最初提供该用户定义的可选远程 URL。
    pub remote_url: Option<String>,
    /// Optional system definition id overridden by this user definition.
    /// 该用户定义覆盖的可选系统定义标识。
    pub override_of: Option<String>,
    /// Optional source hash supplied by the loader or remote cache.
    /// 加载器或远程缓存提供的可选来源哈希。
    pub source_hash: Option<String>,
    /// Optional source version string.
    /// 可选来源版本字符串。
    pub version: Option<String>,
    /// Optional remote ETag captured from the last successful fetch.
    /// 最近一次成功远程拉取捕获的可选 ETag。
    pub etag: Option<String>,
    /// Optional remote Last-Modified header captured from the last successful fetch.
    /// 最近一次成功远程拉取捕获的可选 Last-Modified 头。
    pub last_modified: Option<String>,
    /// Optional last-updated timestamp string.
    /// 可选最近更新时间戳字符串。
    pub updated_at: Option<String>,
}

/// One subagent or expert definition loaded from a Markdown file.
/// 从 Markdown 文件加载的单个子代理或专家定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinition {
    /// Stable string identifier, normally the Markdown file stem.
    /// 稳定字符串标识，通常为 Markdown 文件 stem。
    pub id: String,
    /// Human-facing display name.
    /// 面向人的显示名称。
    pub name: String,
    /// Optional human-facing description.
    /// 可选的人类可读说明。
    pub description: Option<String>,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    pub locales: BTreeMap<String, SubagentDefinitionLocaleDefinition>,
    /// Optional upstream or license source metadata.
    /// 可选上游或协议来源元数据。
    pub origin: Option<SubagentOriginMetadata>,
    /// Human-facing emoji used in catalog UIs.
    /// 目录界面使用的人类可读图标。
    pub emoji: String,
    /// Human-facing color token or color value used in catalog UIs.
    /// 目录界面使用的人类可读颜色标识或颜色值。
    pub color: String,
    /// Optional short tone or persona hint.
    /// 可选的简短语气或人格提示。
    pub vibe: Option<String>,
    /// Declarative execution mode.
    /// 声明式执行模式。
    pub mode: SubagentDefinitionMode,
    /// Optional base system prompt.
    /// 可选基础系统提示词。
    pub system_prompt: Option<String>,
    /// Optional appended system prompt.
    /// 可选追加系统提示词。
    pub system_prompt_append: Option<String>,
    /// Provider and model override snapshot.
    /// 供应商与模型覆盖快照。
    pub provider_model: ProviderModelSnapshot,
    /// Tool exposure override snapshot.
    /// 工具可见性覆盖快照。
    pub tool_exposure: ToolExposureSnapshot,
    /// Prompt skill override snapshot.
    /// 提示词技能覆盖快照。
    pub prompt_skills: PromptSkillSnapshot,
    /// Context compaction override snapshot.
    /// 上下文压缩覆盖快照。
    pub context_compaction: ContextCompactionPolicyRequest,
    /// Runtime policy override request.
    /// 运行时策略覆盖请求。
    pub runtime_policy: ExecutionRuntimePolicyRequest,
    /// Optional maximum nested task depth.
    /// 可选最大嵌套任务深度。
    pub max_depth: Option<u16>,
    /// Whether this definition may be exposed to models and spawn_agent.
    /// 该定义是否允许暴露给模型与 spawn_agent。
    pub enabled: bool,
    /// Loaded source metadata.
    /// 已加载来源元数据。
    pub source: SubagentDefinitionSource,
    /// Definition-owned extension payload.
    /// 定义拥有的扩展载荷。
    pub extra: Value,
}

/// Upstream or license source metadata for one subagent definition.
/// 单个子代理定义的上游或协议来源元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentOriginMetadata {
    /// Source kind such as github, system, or other.
    /// 来源类型，例如 github、system 或 other。
    pub kind: Option<String>,
    /// Optional human-facing source label.
    /// 可选的人类可读来源标签。
    pub label: Option<String>,
    /// Optional icon key used by WebUI.
    /// WebUI 使用的可选图标键。
    pub icon: Option<String>,
    /// Optional source URL.
    /// 可选来源链接。
    pub url: Option<String>,
}

/// Localized display metadata for one subagent definition.
/// 单个子代理定义的本地化展示元数据。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentDefinitionLocaleDefinition {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    pub name: Option<String>,
    /// Optional localized description.
    /// 可选本地化说明。
    pub description: Option<String>,
}

impl SubagentDefinition {
    /// Converts this definition into a task-session profile.
    /// 将当前定义转换为子代理会话配置。
    ///
    /// # Returns
    /// 返回值
    /// A profile carrying model, prompt, tool, skill, context, and runtime overrides.
    /// 携带模型、提示词、工具、技能、上下文与运行时覆盖的配置。
    pub fn to_profile(&self) -> SubagentProfile {
        let mut profile = SubagentProfile::new(self.id.clone(), self.name.clone());
        profile.description = self.description.clone();
        profile.inherit_parent_config = Some(false);
        profile.base_system_prompt = self.system_prompt.clone();
        profile.system_prompt_append = self.system_prompt_append.clone();
        profile.provider_model = self.provider_model.clone();
        profile.tool_exposure = self.tool_exposure.clone();
        profile.prompt_skills = self.prompt_skills.clone();
        profile.context_compaction = self.context_compaction.clone();
        profile.runtime_policy = self.runtime_policy;
        profile.max_depth = self.max_depth;
        profile.extra = self.extra.clone();
        profile
    }
}

/// One non-fatal subagent catalog loading issue.
/// 单个非致命子代理目录加载问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalogLoadError {
    /// Path related to the loading issue.
    /// 与加载问题相关的路径。
    pub path: PathBuf,
    /// Human-readable diagnostic message.
    /// 人类可读诊断信息。
    pub message: String,
}

/// Complete merged subagent catalog.
/// 完整合并后的子代理目录。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCatalog {
    /// User directory scanned for manual definitions and overrides.
    /// 已扫描的用户手动定义与覆盖目录。
    pub user_root: PathBuf,
    /// System directory scanned for application-integrated definitions.
    /// 已扫描的应用集成定义目录。
    pub system_root: PathBuf,
    /// Merged definitions sorted by stable string id.
    /// 按稳定字符串标识排序的合并定义。
    pub definitions: Vec<SubagentDefinition>,
    /// Parsed category metadata keyed by category path.
    /// 按分类路径索引的已解析分类元数据。
    pub categories: Vec<SubagentCategoryDefinition>,
    /// Non-fatal loading errors.
    /// 非致命加载错误集合。
    pub errors: Vec<SubagentCatalogLoadError>,
}

/// One catalog category loaded from a directory and optional `_category` sidecar.
/// 从目录和可选 `_category` sidecar 加载的单个目录分类。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCategoryDefinition {
    /// Stable category id inferred from the relative directory path.
    /// 从相对目录路径推导出的稳定分类标识。
    pub id: String,
    /// Human-facing category name.
    /// 面向用户的分类名称。
    pub name: String,
    /// Optional category description.
    /// 可选分类说明。
    pub description: Option<String>,
    /// Localized display metadata keyed by locale tag.
    /// 按语言标签索引的本地化展示元数据。
    pub locales: BTreeMap<String, SubagentCategoryLocaleDefinition>,
    /// Human-facing category emoji.
    /// 面向用户的分类图标。
    pub emoji: String,
    /// Human-facing category color token or value.
    /// 面向用户的分类颜色标识或颜色值。
    pub color: String,
    /// Optional category order.
    /// 可选分类排序值。
    pub order: Option<i32>,
    /// Category metadata source file.
    /// 分类元数据来源文件。
    pub file: Option<PathBuf>,
}

/// Localized display metadata for one catalog category.
/// 单个目录分类的本地化展示元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentCategoryLocaleDefinition {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    pub name: Option<String>,
    /// Optional localized description.
    /// 可选本地化说明。
    pub description: Option<String>,
}

impl SubagentCatalog {
    /// Returns one enabled definition by exact id or unique short id.
    /// 按精确标识或唯一短标识返回单个已启用定义。
    ///
    /// # Parameters
    /// 参数
    /// `id` is the string agent id, usually a Markdown file stem or category path.
    /// `id` 是字符串代理标识，通常为 Markdown 文件 stem 或分类路径。
    ///
    /// # Returns
    /// 返回值
    /// The enabled matching definition, or `None` when absent, disabled, or ambiguous.
    /// 匹配的已启用定义；若不存在、被禁用或存在歧义则返回 `None`。
    pub fn enabled_definition(&self, id: &str) -> Option<&SubagentDefinition> {
        let exact = self
            .definitions
            .iter()
            .find(|definition| definition.id == id && definition.enabled);
        if exact.is_some() {
            return exact;
        }

        self.unique_enabled_definition_by_short_id(id)
    }

    /// Returns whether the catalog contains any enabled definition with the id.
    /// 返回目录是否包含具有该标识的任一已启用定义。
    ///
    /// # Parameters
    /// 参数
    /// `id` is the string agent id to check.
    /// `id` 是需要检查的字符串代理标识。
    ///
    /// # Returns
    /// 返回值
    /// `true` when spawn_agent may resolve the id.
    /// 当 spawn_agent 可以解析该标识时返回 `true`。
    pub fn is_enabled(&self, id: &str) -> bool {
        self.enabled_definition(id).is_some()
    }

    /// Returns one enabled definition when the final id segment is unique.
    /// 当最终标识片段唯一时返回单个已启用定义。
    ///
    /// # Parameters
    /// 参数
    /// `short_id` is the legacy file-stem id supplied by callers.
    /// `short_id` 是调用方提供的旧版文件 stem 标识。
    ///
    /// # Returns
    /// 返回值
    /// The unique enabled definition, or `None` when missing or ambiguous.
    /// 唯一的已启用定义；缺失或存在歧义时返回 `None`。
    fn unique_enabled_definition_by_short_id(&self, short_id: &str) -> Option<&SubagentDefinition> {
        if short_id.contains('/') {
            return None;
        }

        let mut matches = self
            .definitions
            .iter()
            .filter(|definition| definition.enabled)
            .filter(|definition| {
                definition
                    .id
                    .rsplit('/')
                    .next()
                    .is_some_and(|final_segment| final_segment == short_id)
            });
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(first)
        }
    }
}

/// Loads subagent definitions from system and user roots.
/// 从系统根目录与用户根目录加载子代理定义。
///
/// # Parameters
/// 参数
/// `system_root` is the application-integrated definition directory.
/// `system_root` 是应用集成定义目录。
/// `user_root` is the user-owned definition and override directory.
/// `user_root` 是用户拥有的定义与覆盖目录。
///
/// # Returns
/// 返回值
/// A merged catalog where user definitions cannot replace shipped definitions by string id.
/// 一个不允许用户定义按字符串标识替换内置定义的合并目录。
pub fn load_subagent_catalog_from_roots(
    system_root: PathBuf,
    user_root: PathBuf,
) -> SubagentCatalog {
    let mut catalog = SubagentCatalog {
        user_root: user_root.clone(),
        system_root: system_root.clone(),
        definitions: Vec::new(),
        categories: Vec::new(),
        errors: Vec::new(),
    };
    catalog
        .errors
        .extend(ensure_user_subagent_category_directories(
            &system_root,
            &user_root,
        ));
    let mut by_id = BTreeMap::<String, SubagentDefinition>::new();
    let mut categories = BTreeMap::<String, SubagentCategoryDefinition>::new();
    scan_subagent_definition_root(
        &system_root,
        &system_root,
        SubagentDefinitionScope::System,
        None,
        &mut by_id,
        &mut categories,
        &mut catalog.errors,
    );

    if !user_root.as_os_str().is_empty() {
        let package_root = user_root.join(USER_PACKAGES_DIR_NAME);
        scan_package_roots(
            &package_root,
            &mut by_id,
            &mut categories,
            &mut catalog.errors,
        );
    }

    let user_definition_root = user_root.join(USER_DEFINITIONS_DIR_NAME);
    scan_subagent_definition_root(
        &user_definition_root,
        &user_definition_root,
        SubagentDefinitionScope::User,
        None,
        &mut by_id,
        &mut categories,
        &mut catalog.errors,
    );
    scan_legacy_flat_subagent_definition_root(
        &user_root,
        SubagentDefinitionScope::User,
        &mut by_id,
        &mut catalog.errors,
    );
    apply_user_overrides(
        &user_root.join(USER_OVERRIDES_DIR_NAME),
        &mut by_id,
        &mut catalog.errors,
    );
    catalog.definitions = by_id.into_values().collect();
    catalog.categories = categories.into_values().collect();
    catalog
}

/// Parses one subagent definition Markdown file.
/// 解析单个子代理定义 Markdown 文件。
///
/// # Parameters
/// 参数
/// `definition_file` is the Markdown file to parse.
/// `definition_file` 是需要解析的 Markdown 文件。
/// `scope` identifies the root that supplied the file.
/// `scope` 标识提供该文件的根目录。
///
/// # Returns
/// 返回值
/// A parsed subagent definition or a structured loading error.
/// 解析后的子代理定义或结构化加载错误。
pub fn parse_subagent_definition_file(
    definition_file: &Path,
    scope: SubagentDefinitionScope,
) -> Result<SubagentDefinition, SubagentCatalogLoadError> {
    let raw = fs::read_to_string(definition_file).map_err(|error| SubagentCatalogLoadError {
        path: definition_file.to_path_buf(),
        message: format!("Failed to read subagent definition: {error}"),
    })?;
    let id = if definition_file.is_relative() {
        canonical_id_for_file(Path::new(""), definition_file).ok_or_else(|| {
            SubagentCatalogLoadError {
                path: definition_file.to_path_buf(),
                message: String::from("Subagent definition file must have a valid relative path."),
            }
        })?
    } else {
        definition_file_stem(definition_file).ok_or_else(|| SubagentCatalogLoadError {
            path: definition_file.to_path_buf(),
            message: String::from("Subagent definition file must have a valid UTF-8 file stem."),
        })?
    };
    parse_subagent_definition_text_with_metadata(SubagentDefinitionParseRequest {
        definition_file,
        raw: &raw,
        scope,
        id,
        category_id: None,
        package_id: None,
        sidecar_file: None,
        sidecar: None,
    })
}

/// Parses one Markdown string into a subagent definition.
/// 将一段 Markdown 字符串解析为子代理定义。
///
/// # Parameters
/// 参数
/// `definition_file` is the logical source file used for diagnostics and id validation.
/// `definition_file` 是用于诊断与标识校验的逻辑来源文件。
/// `raw` is the Markdown text with YAML frontmatter.
/// `raw` 是包含 YAML frontmatter 的 Markdown 文本。
/// `scope` identifies the root that supplied the text.
/// `scope` 标识提供该文本的根目录。
///
/// # Returns
/// 返回值
/// A parsed subagent definition or a structured loading error.
/// 解析后的子代理定义或结构化加载错误。
pub fn parse_subagent_definition_text(
    definition_file: &Path,
    raw: &str,
    scope: SubagentDefinitionScope,
) -> Result<SubagentDefinition, SubagentCatalogLoadError> {
    let id = subagent_text_definition_id(definition_file)?;
    parse_subagent_definition_text_with_metadata(SubagentDefinitionParseRequest {
        definition_file,
        raw,
        scope,
        id,
        category_id: None,
        package_id: None,
        sidecar_file: None,
        sidecar: None,
    })
}

/// Resolves the stable id for a text-only subagent definition parse.
/// 解析纯文本智能体定义时使用的稳定标识。
///
/// # Parameters
/// 参数
/// `definition_file` is the logical or physical source file.
/// `definition_file` 是逻辑或物理来源文件。
///
/// # Returns
/// 返回值
/// A canonical id that preserves relative package/category paths while keeping absolute imports file-local.
/// 返回保留相对包与分类路径、但让绝对路径导入保持文件局部的规范标识。
fn subagent_text_definition_id(definition_file: &Path) -> Result<String, SubagentCatalogLoadError> {
    if definition_file.is_relative() {
        return canonical_id_for_file(Path::new(""), definition_file).ok_or_else(|| {
            SubagentCatalogLoadError {
                path: definition_file.to_path_buf(),
                message: String::from("Subagent definition file must have a valid relative path."),
            }
        });
    }
    definition_file_stem(definition_file).ok_or_else(|| SubagentCatalogLoadError {
        path: definition_file.to_path_buf(),
        message: String::from("Subagent definition file must have a valid UTF-8 file stem."),
    })
}

/// Request payload used to parse one subagent definition and its sidecar metadata.
/// 用于解析单个子代理定义及其 sidecar 元数据的请求载荷。
struct SubagentDefinitionParseRequest<'a> {
    /// Markdown file to parse.
    /// 需要解析的 Markdown 文件。
    definition_file: &'a Path,
    /// Markdown source text.
    /// Markdown 源文本。
    raw: &'a str,
    /// Catalog layer that supplied the definition.
    /// 提供该定义的目录层。
    scope: SubagentDefinitionScope,
    /// Canonical id inferred from the relative path.
    /// 从相对路径推导出的规范标识。
    id: String,
    /// Optional category path.
    /// 可选分类路径。
    category_id: Option<String>,
    /// Optional package id.
    /// 可选包标识。
    package_id: Option<String>,
    /// Optional sidecar path.
    /// 可选 sidecar 路径。
    sidecar_file: Option<PathBuf>,
    /// Optional parsed sidecar metadata.
    /// 可选已解析 sidecar 元数据。
    sidecar: Option<SubagentDefinitionSidecar>,
}

/// Parses one subagent definition request into its effective catalog definition.
/// 将单个子代理定义解析请求转换为生效目录定义。
fn parse_subagent_definition_text_with_metadata(
    request: SubagentDefinitionParseRequest<'_>,
) -> Result<SubagentDefinition, SubagentCatalogLoadError> {
    let SubagentDefinitionParseRequest {
        definition_file,
        raw,
        scope,
        id,
        category_id,
        package_id,
        sidecar_file,
        sidecar,
    } = request;
    let (frontmatter, body) = split_frontmatter(raw).unwrap_or(("", raw));
    let parsed = if frontmatter.trim().is_empty() {
        SubagentDefinitionFrontmatter::default()
    } else {
        serde_yaml::from_str::<SubagentDefinitionFrontmatter>(frontmatter).map_err(|error| {
            SubagentCatalogLoadError {
                path: definition_file.to_path_buf(),
                message: format!("Failed to parse subagent definition frontmatter: {error}"),
            }
        })?
    };
    let sidecar = sidecar.unwrap_or_default();
    let effective_package_id = non_empty_owned(sidecar.package_id.clone()).or(package_id);
    let name = non_empty_owned(sidecar.name)
        .or_else(|| non_empty_owned(parsed.name))
        .unwrap_or_else(|| display_name_from_id(&id));
    let description =
        non_empty_owned(sidecar.description).or_else(|| non_empty_owned(parsed.description));
    let emoji = non_empty_owned(sidecar.emoji)
        .or_else(|| non_empty_owned(parsed.emoji))
        .unwrap_or_else(|| String::from(DEFAULT_AGENT_EMOJI));
    let color = non_empty_owned(sidecar.color)
        .or_else(|| non_empty_owned(parsed.color))
        .unwrap_or_else(|| String::from(DEFAULT_AGENT_COLOR));
    let vibe = non_empty_owned(sidecar.vibe).or_else(|| non_empty_owned(parsed.vibe));
    let mut tool_exposure = if tool_exposure_snapshot_has_override(&sidecar.tool_exposure) {
        sidecar.tool_exposure
    } else {
        parsed.tool_exposure
    };
    if let Some(tool_policy) = sidecar.tool_policy {
        apply_tool_policy_to_exposure(&mut tool_exposure, tool_policy.into_snapshot());
    }
    normalize_tool_exposure(&mut tool_exposure);
    let mut prompt_skills = if !sidecar.prompt_skills.enabled.is_empty()
        || !sidecar.prompt_skills.disabled.is_empty()
    {
        sidecar.prompt_skills
    } else {
        parsed.prompt_skills
    };
    if let Some(prompt_skill_policy) = sidecar.prompt_skill_policy {
        apply_prompt_skill_policy(&mut prompt_skills, prompt_skill_policy.into_snapshot());
    }
    normalize_prompt_skills(&mut prompt_skills);
    let system_prompt = parsed
        .system_prompt
        .and_then(non_empty_string)
        .or_else(|| non_empty_text(body).map(str::to_string));
    let source_key = source_key_for_definition(scope, effective_package_id.as_deref(), &id);
    Ok(SubagentDefinition {
        id: id.clone(),
        name,
        description,
        locales: sidecar
            .locales
            .map(localized_definition_metadata)
            .unwrap_or_else(|| localized_definition_metadata(parsed.locales)),
        origin: normalized_origin_metadata(sidecar.source),
        emoji,
        color,
        vibe,
        mode: SubagentDefinitionMode::Expert,
        system_prompt,
        system_prompt_append: sidecar
            .system_prompt_append
            .and_then(non_empty_string)
            .or_else(|| parsed.system_prompt_append.and_then(non_empty_string)),
        provider_model: if sidecar.provider_model == ProviderModelSnapshot::default() {
            parsed.provider_model
        } else {
            sidecar.provider_model
        },
        tool_exposure,
        prompt_skills,
        context_compaction: if sidecar.context_compaction
            == ContextCompactionPolicyRequest::default()
        {
            parsed.context_compaction
        } else {
            sidecar.context_compaction
        },
        runtime_policy: if sidecar.runtime_policy == ExecutionRuntimePolicyRequest::default() {
            parsed.runtime_policy
        } else {
            sidecar.runtime_policy
        },
        max_depth: sidecar.max_depth.or(parsed.max_depth),
        enabled: sidecar.enabled.or(parsed.enabled).unwrap_or(true),
        source: SubagentDefinitionSource {
            scope,
            file: definition_file.to_path_buf(),
            sidecar_file,
            source_key,
            category_id,
            package_id: effective_package_id,
            remote_url: sidecar.remote_url.or(parsed.remote_url),
            override_of: sidecar.override_of.or(parsed.override_of),
            source_hash: sidecar.source_hash.or(parsed.source_hash),
            version: sidecar.version.or(parsed.version),
            etag: sidecar.etag.or(parsed.etag),
            last_modified: sidecar.last_modified.or(parsed.last_modified),
            updated_at: sidecar.updated_at.or(parsed.updated_at),
        },
        extra: sidecar.extra.or(parsed.extra).unwrap_or(Value::Null),
    })
}

/// Writes one definition into a user-owned Markdown file.
/// 将单个定义写入用户拥有的 Markdown 文件。
///
/// # Parameters
/// 参数
/// `user_root` is the user subagent catalog directory.
/// `user_root` 是用户子代理目录。
/// `definition` is the definition snapshot that should be persisted.
/// `definition` 是需要持久化的定义快照。
///
/// # Returns
/// 返回值
/// The file path that was written.
/// 返回已写入的文件路径。
pub fn write_user_subagent_definition(
    user_root: &Path,
    definition: &SubagentDefinition,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    let path = user_subagent_definition_path(user_root, &definition.id)?;
    let parent = path.parent().unwrap_or(user_root);
    fs::create_dir_all(parent).map_err(|error| SubagentCatalogLoadError {
        path: parent.to_path_buf(),
        message: format!("Failed to create user subagent catalog root: {error}"),
    })?;
    let body = subagent_definition_to_markdown(definition)?;
    let mut file = fs::File::create(&path).map_err(|error| SubagentCatalogLoadError {
        path: path.clone(),
        message: format!("Failed to create user subagent definition: {error}"),
    })?;
    file.write_all(body.as_bytes())
        .map_err(|error| SubagentCatalogLoadError {
            path: path.clone(),
            message: format!("Failed to write user subagent definition: {error}"),
        })?;
    write_user_subagent_sidecar(user_root, definition)?;
    Ok(path)
}

/// Returns the user-owned Markdown path for one definition id.
/// 返回单个定义标识对应的用户 Markdown 路径。
///
/// # Parameters
/// 参数
/// `user_root` is the user subagent catalog directory.
/// `user_root` 是用户子代理目录。
/// `id` is the stable subagent definition id.
/// `id` 是稳定子代理定义标识。
///
/// # Returns
/// 返回值
/// A safe direct-child Markdown file path or a structured validation error.
/// 返回安全的直接子级 Markdown 文件路径，或结构化校验错误。
pub fn user_subagent_definition_path(
    user_root: &Path,
    id: &str,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    Ok(user_root
        .join(USER_DEFINITIONS_DIR_NAME)
        .join(safe_relative_agent_path(id, SUBAGENT_DEFINITION_EXTENSION)?))
}

/// Returns the user-owned YAML sidecar path for one definition id.
/// 返回单个定义标识对应的用户 YAML sidecar 路径。
///
/// # Parameters
/// 参数
/// `user_root` is the user subagent catalog directory.
/// `user_root` 是用户子代理目录。
/// `id` is the stable subagent definition id.
/// `id` 是稳定子代理定义标识。
///
/// # Returns
/// 返回值
/// A safe relative sidecar path rooted in the user definitions directory.
/// 位于用户定义目录中的安全相对 sidecar 路径。
pub fn user_subagent_sidecar_path(
    user_root: &Path,
    id: &str,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    Ok(user_root
        .join(USER_DEFINITIONS_DIR_NAME)
        .join(safe_relative_agent_path(id, YAML_EXTENSION)?))
}

/// Returns the user-owned override YAML path for one definition id.
/// 返回单个定义标识对应的用户覆盖 YAML 路径。
///
/// # Parameters
/// 参数
/// `user_root` is the user subagent catalog directory.
/// `user_root` 是用户子代理目录。
/// `id` is the stable subagent definition id.
/// `id` 是稳定子代理定义标识。
///
/// # Returns
/// 返回值
/// A safe relative sidecar path rooted in the user override directory.
/// 位于用户覆盖目录中的安全相对 sidecar 路径。
pub fn user_subagent_override_path(
    user_root: &Path,
    id: &str,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    Ok(user_root
        .join(USER_OVERRIDES_DIR_NAME)
        .join(safe_relative_agent_path(id, YAML_EXTENSION)?))
}

/// Writes one user-owned override sidecar without copying the prompt Markdown.
/// 写入一个用户拥有的覆盖 sidecar，而不复制提示词 Markdown。
///
/// # Parameters
/// 参数
/// `user_root` is the user subagent catalog directory.
/// `user_root` 是用户子代理目录。
/// `definition` is the effective definition snapshot to persist as overrides.
/// `definition` 是需要作为覆盖持久化的生效定义快照。
///
/// # Returns
/// 返回值
/// The override sidecar path that was written.
/// 返回已写入的覆盖 sidecar 路径。
pub fn write_user_subagent_override(
    user_root: &Path,
    definition: &SubagentDefinition,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    let path = user_subagent_override_path(user_root, &definition.id)?;
    write_subagent_sidecar_to_path(&path, definition)
}

/// Writes one system-backed metadata override without copying the prompt Markdown.
/// 写入一个系统智能体元数据覆盖，而不复制提示词 Markdown。
///
/// # Parameters
/// 参数
/// `user_root` is the user subagent catalog directory.
/// `user_root` 是用户子代理目录。
/// `definition` is the effective system-backed definition with editable metadata already applied.
/// `definition` 是已经应用可编辑元数据后的系统智能体生效定义。
///
/// # Returns
/// 返回值
/// The override sidecar path that was written.
/// 返回已写入的覆盖 sidecar 路径。
pub fn write_user_subagent_metadata_override(
    user_root: &Path,
    definition: &SubagentDefinition,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    let path = user_subagent_override_path(user_root, &definition.id)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| SubagentCatalogLoadError {
        path: parent.to_path_buf(),
        message: format!("Failed to create user subagent metadata override root: {error}"),
    })?;
    let sidecar = SubagentDefinitionSidecar {
        schema_version: Some(1),
        enabled: Some(definition.enabled),
        system_prompt_append: definition.system_prompt_append.clone(),
        tool_exposure: sidecar_tool_exposure_from_definition(&definition.tool_exposure),
        tool_policy: SubagentToolPolicySidecar::from_snapshot(&definition.tool_exposure.policy),
        prompt_skill_policy: SubagentPromptSkillPolicySidecar::from_snapshot(
            &definition.prompt_skills.policy,
        ),
        ..SubagentDefinitionSidecar::default()
    };
    let yaml = serde_yaml::to_string(&sidecar).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to serialize subagent metadata override: {error}"),
    })?;
    let mut file = fs::File::create(&path).map_err(|error| SubagentCatalogLoadError {
        path: path.clone(),
        message: format!("Failed to create user subagent metadata override: {error}"),
    })?;
    file.write_all(yaml.as_bytes())
        .map_err(|error| SubagentCatalogLoadError {
            path: path.clone(),
            message: format!("Failed to write user subagent metadata override: {error}"),
        })?;
    Ok(path)
}

/// Writes one minimal user-owned enabled-state override sidecar.
/// 写入一个只包含启停状态的用户拥有覆盖 sidecar。
///
/// # Parameters
/// 参数
/// `user_root` is the user subagent catalog directory.
/// `user_root` 是用户子代理目录。
/// `id` is the stable subagent definition id.
/// `id` 是稳定子代理定义标识。
/// `enabled` is the model exposure state to persist.
/// `enabled` 是需要持久化的模型暴露状态。
///
/// # Returns
/// 返回值
/// The override sidecar path that was written.
/// 返回已写入的覆盖 sidecar 路径。
pub fn write_user_subagent_enabled_override(
    user_root: &Path,
    id: &str,
    enabled: bool,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    let path = user_subagent_override_path(user_root, id)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| SubagentCatalogLoadError {
        path: parent.to_path_buf(),
        message: format!("Failed to create user subagent override root: {error}"),
    })?;
    let sidecar = SubagentDefinitionSidecar {
        schema_version: Some(1),
        enabled: Some(enabled),
        ..SubagentDefinitionSidecar::default()
    };
    let yaml = serde_yaml::to_string(&sidecar).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to serialize subagent enabled override: {error}"),
    })?;
    let mut file = fs::File::create(&path).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to create user subagent enabled override: {error}"),
    })?;
    file.write_all(yaml.as_bytes())
        .map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to write user subagent enabled override: {error}"),
        })?;
    Ok(path)
}

/// Ensures the user definition directory contains the system category directory tree.
/// 确保用户定义目录包含系统分类目录树。
///
/// # Parameters
/// 参数
/// `system_root` is the application-integrated definition directory.
/// `system_root` 是应用集成定义目录。
/// `user_root` is the user-owned agent catalog root.
/// `user_root` 是用户拥有的智能体目录根。
///
/// # Returns
/// 返回值
/// Non-fatal directory creation errors that should be surfaced with catalog load issues.
/// 返回应随目录加载问题展示的非致命目录创建错误。
pub fn ensure_user_subagent_category_directories(
    system_root: &Path,
    user_root: &Path,
) -> Vec<SubagentCatalogLoadError> {
    let mut errors = Vec::new();
    if user_root.as_os_str().is_empty() {
        return errors;
    }
    let user_definition_root = user_root.join(USER_DEFINITIONS_DIR_NAME);
    create_subagent_directory(&user_definition_root, &mut errors);
    if system_root.as_os_str().is_empty() || !system_root.is_dir() {
        return errors;
    }
    mirror_system_top_level_category_directories(
        system_root,
        system_root,
        &user_definition_root,
        &mut errors,
    );
    errors
}

/// Serializes one definition into the catalog Markdown file format.
/// 将单个定义序列化为目录 Markdown 文件格式。
///
/// # Parameters
/// 参数
/// `definition` is the subagent definition to serialize.
/// `definition` 是需要序列化的子代理定义。
///
/// # Returns
/// 返回值
/// Markdown text with YAML frontmatter that can be parsed by this catalog loader.
/// 返回可被当前目录加载器解析的带 YAML frontmatter 的 Markdown 文本。
pub fn subagent_definition_to_markdown(
    definition: &SubagentDefinition,
) -> Result<String, SubagentCatalogLoadError> {
    let frontmatter = SubagentMarkdownFrontmatter {
        name: Some(definition.name.clone()),
        description: definition.description.clone(),
        emoji: Some(definition.emoji.clone()),
        color: Some(definition.color.clone()),
        vibe: definition.vibe.clone(),
    };
    let yaml = serde_yaml::to_string(&frontmatter).map_err(|error| SubagentCatalogLoadError {
        path: definition.source.file.clone(),
        message: format!("Failed to serialize subagent definition frontmatter: {error}"),
    })?;
    let body = definition
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    Ok(format!("---\n{yaml}---\n{body}\n"))
}

/// Scans one root directory recursively for Markdown definition files.
/// 递归扫描单个根目录中的 Markdown 定义文件。
fn scan_subagent_definition_root(
    root: &Path,
    base_root: &Path,
    scope: SubagentDefinitionScope,
    package_id: Option<&str>,
    by_id: &mut BTreeMap<String, SubagentDefinition>,
    categories: &mut BTreeMap<String, SubagentCategoryDefinition>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    if root.as_os_str().is_empty() || !root.exists() {
        return;
    }
    if !root.is_dir() {
        errors.push(SubagentCatalogLoadError {
            path: root.to_path_buf(),
            message: String::from("Subagent catalog root exists but is not a directory."),
        });
        return;
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(SubagentCatalogLoadError {
                path: root.to_path_buf(),
                message: format!("Failed to read subagent catalog root: {error}"),
            });
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            register_category_if_needed(&path, base_root, package_id, categories, errors);
            scan_subagent_definition_root(
                &path, base_root, scope, package_id, by_id, categories, errors,
            );
            continue;
        }
        if !path.is_file()
            || !path_has_markdown_extension(&path)
            || path.file_stem().and_then(|stem| stem.to_str()) == Some(CATEGORY_METADATA_FILE_NAME)
        {
            continue;
        }
        match parse_subagent_definition_file_in_root(&path, base_root, scope, package_id) {
            Ok(definition) => {
                if user_definition_conflicts_with_existing_system_or_package(
                    &definition,
                    by_id,
                    errors,
                ) {
                    continue;
                }
                by_id.insert(definition.id.clone(), definition);
            }
            Err(error) => errors.push(error),
        }
    }
}

/// Mirrors system top-level category directories into the user definition directory.
/// 将系统顶层分类目录镜像到用户定义目录。
fn mirror_system_top_level_category_directories(
    root: &Path,
    base_root: &Path,
    user_definition_root: &Path,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(SubagentCatalogLoadError {
                path: root.to_path_buf(),
                message: format!("Failed to read system subagent category directory: {error}"),
            });
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !directory_is_top_level_category(base_root, &path) {
            continue;
        }
        if let Some(category_id) = category_id_for_directory(base_root, &path) {
            match safe_relative_directory_path(&category_id) {
                Ok(relative_path) => {
                    create_subagent_directory(&user_definition_root.join(relative_path), errors);
                }
                Err(error) => errors.push(error),
            }
        }
    }
}

/// Creates one subagent directory and records non-fatal errors.
/// 创建单个子代理目录并记录非致命错误。
fn create_subagent_directory(path: &Path, errors: &mut Vec<SubagentCatalogLoadError>) {
    if let Err(error) = fs::create_dir_all(path) {
        errors.push(SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to create user subagent category directory: {error}"),
        });
    }
}

/// Scans downloaded package roots.
/// 扫描已下载包根目录。
fn scan_package_roots(
    package_root: &Path,
    by_id: &mut BTreeMap<String, SubagentDefinition>,
    categories: &mut BTreeMap<String, SubagentCategoryDefinition>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    if package_root.as_os_str().is_empty() || !package_root.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(package_root) else {
        errors.push(SubagentCatalogLoadError {
            path: package_root.to_path_buf(),
            message: String::from("Failed to read subagent package root."),
        });
        return;
    };
    for entry in entries.flatten() {
        let package_dir = entry.path();
        if !package_dir.is_dir() {
            continue;
        }
        let Some(package_id) = package_dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        scan_subagent_definition_root(
            &package_dir,
            &package_dir,
            SubagentDefinitionScope::Package,
            Some(&package_id),
            by_id,
            categories,
            errors,
        );
    }
}

/// Scans legacy flat user definitions directly below the user root.
/// 扫描用户根目录直下的旧版平铺定义。
fn scan_legacy_flat_subagent_definition_root(
    root: &Path,
    scope: SubagentDefinitionScope,
    by_id: &mut BTreeMap<String, SubagentDefinition>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    if root.as_os_str().is_empty() || !root.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        errors.push(SubagentCatalogLoadError {
            path: root.to_path_buf(),
            message: String::from("Failed to read legacy subagent catalog root."),
        });
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !path_has_markdown_extension(&path) {
            continue;
        }
        match parse_subagent_definition_file(&path, scope) {
            Ok(definition) => {
                if user_definition_conflicts_with_existing_system_or_package(
                    &definition,
                    by_id,
                    errors,
                ) {
                    continue;
                }
                by_id.insert(definition.id.clone(), definition);
            }
            Err(error) => errors.push(error),
        }
    }
}

/// Returns whether a user definition is ignored because it would shadow a shipped definition.
/// 返回用户定义是否因会遮蔽内置定义而被忽略。
fn user_definition_conflicts_with_existing_system_or_package(
    definition: &SubagentDefinition,
    by_id: &BTreeMap<String, SubagentDefinition>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) -> bool {
    if definition.source.scope != SubagentDefinitionScope::User {
        return false;
    }
    let Some(existing) = by_id.get(&definition.id) else {
        return false;
    };
    if existing.source.scope == SubagentDefinitionScope::User {
        return false;
    }
    errors.push(SubagentCatalogLoadError {
        path: definition.source.file.clone(),
        message: format!(
            "User subagent definition `{}` is ignored because it conflicts with an existing system or package definition.",
            definition.id
        ),
    });
    true
}

/// Applies user override sidecars to already loaded definitions.
/// 将用户覆盖 sidecar 应用到已加载定义。
fn apply_user_overrides(
    overrides_root: &Path,
    by_id: &mut BTreeMap<String, SubagentDefinition>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    if overrides_root.as_os_str().is_empty() || !overrides_root.is_dir() {
        return;
    }
    let mut sidecars = Vec::new();
    collect_sidecar_files(overrides_root, &mut sidecars, errors);
    for sidecar_file in sidecars {
        let Some(id) = canonical_id_for_file(overrides_root, &sidecar_file) else {
            continue;
        };
        let sidecar = match read_definition_sidecar(&sidecar_file) {
            Ok(sidecar) => sidecar,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
        if let Some(definition) = by_id.get_mut(&id) {
            apply_sidecar_override(definition, sidecar_file, sidecar);
        }
    }
}

/// Parses one definition file using its relative path as the canonical id.
/// 使用相对路径作为规范标识解析单个定义文件。
fn parse_subagent_definition_file_in_root(
    definition_file: &Path,
    base_root: &Path,
    scope: SubagentDefinitionScope,
    package_id: Option<&str>,
) -> Result<SubagentDefinition, SubagentCatalogLoadError> {
    let raw = fs::read_to_string(definition_file).map_err(|error| SubagentCatalogLoadError {
        path: definition_file.to_path_buf(),
        message: format!("Failed to read subagent definition: {error}"),
    })?;
    let relative_id = canonical_id_for_file(base_root, definition_file).ok_or_else(|| {
        SubagentCatalogLoadError {
            path: definition_file.to_path_buf(),
            message: String::from("Subagent definition file must have a valid relative path."),
        }
    })?;
    let id = package_id
        .map(|package_id| format!("{package_id}/{relative_id}"))
        .unwrap_or(relative_id);
    let category_id = display_subagent_category_id(&id, package_id);
    let sidecar_file = find_definition_sidecar(definition_file);
    let sidecar = match sidecar_file.as_deref() {
        Some(path) => Some(read_definition_sidecar(path)?),
        None => None,
    };
    parse_subagent_definition_text_with_metadata(SubagentDefinitionParseRequest {
        definition_file,
        raw: &raw,
        scope,
        id,
        category_id,
        package_id: package_id.map(str::to_string),
        sidecar_file,
        sidecar,
    })
}

/// Registers category metadata for one directory when a category id is available.
/// 当目录具有分类标识时登记该目录的分类元数据。
fn register_category_if_needed(
    directory: &Path,
    base_root: &Path,
    package_id: Option<&str>,
    categories: &mut BTreeMap<String, SubagentCategoryDefinition>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    if !directory_is_top_level_category(base_root, directory) {
        return;
    }
    let Some(raw_id) = category_id_for_directory(base_root, directory) else {
        return;
    };
    let id = package_id
        .map(|package_id| format!("{package_id}/{raw_id}"))
        .unwrap_or(raw_id);
    if categories.contains_key(&id) {
        return;
    }
    let sidecar_file = find_category_sidecar(directory);
    let sidecar = match sidecar_file.as_deref() {
        Some(path) => match read_category_sidecar(path) {
            Ok(sidecar) => sidecar,
            Err(error) => {
                errors.push(error);
                SubagentCategorySidecar::default()
            }
        },
        None => SubagentCategorySidecar::default(),
    };
    categories.insert(
        id.clone(),
        SubagentCategoryDefinition {
            id: id.clone(),
            name: non_empty_owned(sidecar.name).unwrap_or_else(|| display_name_from_id(&id)),
            description: non_empty_owned(sidecar.description),
            locales: localized_category_definitions(sidecar.locales),
            emoji: non_empty_owned(sidecar.emoji)
                .unwrap_or_else(|| String::from(DEFAULT_AGENT_EMOJI)),
            color: non_empty_owned(sidecar.color)
                .unwrap_or_else(|| String::from(DEFAULT_AGENT_COLOR)),
            order: sidecar.order,
            file: sidecar_file,
        },
    );
}

/// Normalizes category sidecar locale metadata.
/// 规范化分类 sidecar 本地化元数据。
fn localized_category_definitions(
    locales: BTreeMap<String, SubagentCategoryLocaleSidecar>,
) -> BTreeMap<String, SubagentCategoryLocaleDefinition> {
    locales
        .into_iter()
        .filter_map(|(locale, metadata)| {
            let locale_key = locale.trim().to_string();
            if locale_key.is_empty() {
                return None;
            }
            let name = non_empty_owned(metadata.name);
            let description = non_empty_owned(metadata.description);
            if name.is_none() && description.is_none() {
                return None;
            }
            Some((
                locale_key,
                SubagentCategoryLocaleDefinition { name, description },
            ))
        })
        .collect()
}

/// Normalizes definition sidecar locale metadata.
/// 规范化定义 sidecar 本地化元数据。
fn localized_definition_metadata(
    locales: BTreeMap<String, SubagentDefinitionLocaleSidecar>,
) -> BTreeMap<String, SubagentDefinitionLocaleDefinition> {
    locales
        .into_iter()
        .filter_map(|(locale, metadata)| {
            let locale_key = locale.trim().to_string();
            if locale_key.is_empty() {
                return None;
            }
            let name = non_empty_owned(metadata.name);
            let description = non_empty_owned(metadata.description);
            if name.is_none() && description.is_none() {
                return None;
            }
            Some((
                locale_key,
                SubagentDefinitionLocaleDefinition { name, description },
            ))
        })
        .collect()
}

/// Converts definition locale metadata into serializable sidecar metadata.
/// 将定义本地化元数据转换为可序列化的 sidecar 元数据。
fn sidecar_locales_from_definition(
    locales: &BTreeMap<String, SubagentDefinitionLocaleDefinition>,
) -> BTreeMap<String, SubagentDefinitionLocaleSidecar> {
    locales
        .iter()
        .filter_map(|(locale, metadata)| {
            let locale_key = locale.trim().to_string();
            if locale_key.is_empty() {
                return None;
            }
            if metadata.name.is_none() && metadata.description.is_none() {
                return None;
            }
            Some((
                locale_key,
                SubagentDefinitionLocaleSidecar {
                    name: metadata.name.clone(),
                    description: metadata.description.clone(),
                },
            ))
        })
        .collect()
}

/// Converts definition locales into an optional sidecar field.
/// 将定义本地化信息转换为可选 sidecar 字段。
///
/// # Parameters
/// 参数
/// `locales` is the resolved definition locale map.
/// `locales` 是已解析的定义本地化映射。
///
/// # Returns
/// 返回值
/// `Some` when at least one locale has content; otherwise `None` so overrides stay incremental.
/// 存在至少一个有效语言项时返回 `Some`；否则返回 `None` 以保持覆盖增量语义。
fn non_empty_sidecar_locales_from_definition(
    locales: &BTreeMap<String, SubagentDefinitionLocaleDefinition>,
) -> Option<BTreeMap<String, SubagentDefinitionLocaleSidecar>> {
    let locales = sidecar_locales_from_definition(locales);
    (!locales.is_empty()).then_some(locales)
}

/// Normalizes optional origin metadata from sidecar input.
/// 规范化 sidecar 输入中的可选来源元数据。
fn normalized_origin_metadata(
    source: Option<SubagentOriginSidecar>,
) -> Option<SubagentOriginMetadata> {
    let source = source?;
    let kind = non_empty_owned(source.kind);
    let label = non_empty_owned(source.label);
    let icon = non_empty_owned(source.icon);
    let url = non_empty_owned(source.url);
    if kind.is_none() && label.is_none() && icon.is_none() && url.is_none() {
        return None;
    }
    Some(SubagentOriginMetadata {
        kind,
        label,
        icon,
        url,
    })
}

/// Converts origin metadata into serializable sidecar metadata.
/// 将来源元数据转换为可序列化的 sidecar 元数据。
fn sidecar_origin_from_definition(
    origin: Option<&SubagentOriginMetadata>,
) -> Option<SubagentOriginSidecar> {
    let origin = origin?;
    if origin.kind.is_none()
        && origin.label.is_none()
        && origin.icon.is_none()
        && origin.url.is_none()
    {
        return None;
    }
    Some(SubagentOriginSidecar {
        kind: origin.kind.clone(),
        label: origin.label.clone(),
        icon: origin.icon.clone(),
        url: origin.url.clone(),
    })
}

/// Writes one user-owned YAML sidecar for a definition.
/// 为定义写入一个用户拥有的 YAML sidecar。
fn write_user_subagent_sidecar(
    user_root: &Path,
    definition: &SubagentDefinition,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    let path = user_subagent_sidecar_path(user_root, &definition.id)?;
    write_subagent_sidecar_to_path(&path, definition)
}

/// Writes one definition sidecar to a specific path.
/// 将单个定义 sidecar 写入指定路径。
fn write_subagent_sidecar_to_path(
    path: &Path,
    definition: &SubagentDefinition,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| SubagentCatalogLoadError {
        path: parent.to_path_buf(),
        message: format!("Failed to create user subagent sidecar root: {error}"),
    })?;
    let sidecar = SubagentDefinitionSidecar {
        schema_version: Some(1),
        name: Some(definition.name.clone()),
        description: definition.description.clone(),
        locales: non_empty_sidecar_locales_from_definition(&definition.locales),
        source: sidecar_origin_from_definition(definition.origin.as_ref()),
        emoji: Some(definition.emoji.clone()),
        color: Some(definition.color.clone()),
        vibe: definition.vibe.clone(),
        enabled: Some(definition.enabled),
        system_prompt_append: definition.system_prompt_append.clone(),
        provider_model: definition.provider_model.clone(),
        tool_exposure: sidecar_tool_exposure_from_definition(&definition.tool_exposure),
        tool_policy: SubagentToolPolicySidecar::from_snapshot(&definition.tool_exposure.policy),
        prompt_skill_policy: SubagentPromptSkillPolicySidecar::from_snapshot(
            &definition.prompt_skills.policy,
        ),
        context_compaction: definition.context_compaction.clone(),
        runtime_policy: definition.runtime_policy,
        max_depth: definition.max_depth,
        package_id: definition.source.package_id.clone(),
        override_of: definition.source.override_of.clone(),
        remote_url: definition.source.remote_url.clone(),
        source_hash: definition.source.source_hash.clone(),
        version: definition.source.version.clone(),
        etag: definition.source.etag.clone(),
        last_modified: definition.source.last_modified.clone(),
        updated_at: definition.source.updated_at.clone(),
        extra: (definition.extra != Value::Null).then_some(definition.extra.clone()),
        ..SubagentDefinitionSidecar::default()
    };
    let yaml = serde_yaml::to_string(&sidecar).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to serialize subagent sidecar: {error}"),
    })?;
    let mut file = fs::File::create(path).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to create user subagent sidecar: {error}"),
    })?;
    file.write_all(yaml.as_bytes())
        .map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to write user subagent sidecar: {error}"),
        })?;
    Ok(path.to_path_buf())
}

/// Builds the sidecar legacy exposure payload after extracting unified tool policy.
/// 在抽离统一工具策略后构建 sidecar 旧版可见性载荷。
fn sidecar_tool_exposure_from_definition(snapshot: &ToolExposureSnapshot) -> ToolExposureSnapshot {
    let mut sidecar_snapshot = snapshot.clone();
    sidecar_snapshot.policy = CapabilityPolicySnapshot::default();
    sidecar_snapshot.allowed_tools.clear();
    sidecar_snapshot.disabled_tools.clear();
    sidecar_snapshot
}

/// Applies a parsed sidecar as an override to an existing definition.
/// 将已解析 sidecar 作为覆盖应用到现有定义。
fn apply_sidecar_override(
    definition: &mut SubagentDefinition,
    sidecar_file: PathBuf,
    sidecar: SubagentDefinitionSidecar,
) {
    let original_scope = definition.source.scope;
    if let Some(name) = non_empty_owned(sidecar.name) {
        definition.name = name;
    }
    if let Some(description) = non_empty_owned(sidecar.description) {
        definition.description = Some(description);
    }
    if let Some(locales) = sidecar.locales {
        definition.locales = localized_definition_metadata(locales);
    }
    if sidecar.source.is_some() {
        definition.origin = normalized_origin_metadata(sidecar.source);
    }
    if let Some(emoji) = non_empty_owned(sidecar.emoji) {
        definition.emoji = emoji;
    }
    if let Some(color) = non_empty_owned(sidecar.color) {
        definition.color = color;
    }
    if let Some(vibe) = non_empty_owned(sidecar.vibe) {
        definition.vibe = Some(vibe);
    }
    if let Some(enabled) = sidecar.enabled {
        definition.enabled = enabled;
    }
    if let Some(system_prompt_append) = sidecar.system_prompt_append.and_then(non_empty_string) {
        definition.system_prompt_append = Some(system_prompt_append);
    }
    if tool_exposure_snapshot_has_override(&sidecar.tool_exposure) {
        definition.tool_exposure = sidecar.tool_exposure;
    }
    if let Some(tool_policy) = sidecar.tool_policy {
        apply_tool_policy_to_exposure(&mut definition.tool_exposure, tool_policy.into_snapshot());
        normalize_tool_exposure(&mut definition.tool_exposure);
    }
    if !sidecar.prompt_skills.enabled.is_empty() || !sidecar.prompt_skills.disabled.is_empty() {
        definition.prompt_skills = sidecar.prompt_skills;
    }
    if let Some(prompt_skill_policy) = sidecar.prompt_skill_policy {
        apply_prompt_skill_policy(
            &mut definition.prompt_skills,
            prompt_skill_policy.into_snapshot(),
        );
        normalize_prompt_skills(&mut definition.prompt_skills);
    }
    if sidecar.provider_model != ProviderModelSnapshot::default() {
        definition.provider_model = sidecar.provider_model;
    }
    if sidecar.context_compaction != ContextCompactionPolicyRequest::default() {
        definition.context_compaction = sidecar.context_compaction;
    }
    if sidecar.runtime_policy != ExecutionRuntimePolicyRequest::default() {
        definition.runtime_policy = sidecar.runtime_policy;
    }
    if sidecar.max_depth.is_some() {
        definition.max_depth = sidecar.max_depth;
    }
    definition.source.sidecar_file = Some(sidecar_file);
    definition.source.override_of = sidecar.override_of.or_else(|| {
        (original_scope == SubagentDefinitionScope::System).then(|| definition.id.clone())
    });
    definition.source.version = sidecar
        .version
        .or_else(|| definition.source.version.clone());
    definition.source.updated_at = sidecar
        .updated_at
        .or_else(|| definition.source.updated_at.clone());
    if let Some(extra) = sidecar.extra {
        definition.extra = extra;
    }
    normalize_tool_exposure(&mut definition.tool_exposure);
    normalize_prompt_skills(&mut definition.prompt_skills);
}

/// Reads one definition sidecar from YAML, YML, or JSON.
/// 从 YAML、YML 或 JSON 读取单个定义 sidecar。
fn read_definition_sidecar(
    path: &Path,
) -> Result<SubagentDefinitionSidecar, SubagentCatalogLoadError> {
    let raw = fs::read_to_string(path).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to read subagent sidecar: {error}"),
    })?;
    if path_has_json_extension(path) {
        serde_json::from_str(&raw).map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to parse subagent JSON sidecar: {error}"),
        })
    } else {
        serde_yaml::from_str(&raw).map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to parse subagent YAML sidecar: {error}"),
        })
    }
}

/// Reads one category sidecar from YAML, YML, or JSON.
/// 从 YAML、YML 或 JSON 读取单个分类 sidecar。
fn read_category_sidecar(path: &Path) -> Result<SubagentCategorySidecar, SubagentCatalogLoadError> {
    let raw = fs::read_to_string(path).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to read subagent category sidecar: {error}"),
    })?;
    if path_has_json_extension(path) {
        serde_json::from_str(&raw).map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to parse subagent category JSON sidecar: {error}"),
        })
    } else {
        serde_yaml::from_str(&raw).map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to parse subagent category YAML sidecar: {error}"),
        })
    }
}

/// Returns the first matching sidecar path for one Markdown definition.
/// 返回单个 Markdown 定义的第一个匹配 sidecar 路径。
fn find_definition_sidecar(definition_file: &Path) -> Option<PathBuf> {
    let base = definition_file.with_extension("");
    [YAML_EXTENSION, YML_EXTENSION, JSON_EXTENSION]
        .iter()
        .map(|extension| base.with_extension(extension))
        .find(|path| path.is_file())
}

/// Returns the first matching category sidecar in one directory.
/// 返回某个目录中的第一个匹配分类 sidecar。
fn find_category_sidecar(directory: &Path) -> Option<PathBuf> {
    [YAML_EXTENSION, YML_EXTENSION, JSON_EXTENSION]
        .iter()
        .map(|extension| directory.join(format!("{CATEGORY_METADATA_FILE_NAME}.{extension}")))
        .find(|path| path.is_file())
}

/// Recursively collects sidecar files from one root.
/// 从一个根目录递归收集 sidecar 文件。
fn collect_sidecar_files(
    root: &Path,
    sidecars: &mut Vec<PathBuf>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        errors.push(SubagentCatalogLoadError {
            path: root.to_path_buf(),
            message: String::from("Failed to read override sidecar directory."),
        });
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sidecar_files(&path, sidecars, errors);
        } else if path_has_sidecar_extension(&path)
            && path.file_stem().and_then(|stem| stem.to_str()) != Some(CATEGORY_METADATA_FILE_NAME)
        {
            sidecars.push(path);
        }
    }
}

/// Converts the legacy three-state map into a mode-plus-selector policy.
/// 将旧版三态映射转换为规则加选择器策略。
fn policy_snapshot_from_state_map(
    policy: &BTreeMap<String, SubagentPolicyState>,
) -> CapabilityPolicySnapshot {
    let mut allowed = Vec::new();
    let mut disabled = Vec::new();
    for (id, state) in policy {
        match state {
            SubagentPolicyState::Allow => allowed.push(id.clone()),
            SubagentPolicyState::Deny => disabled.push(id.clone()),
            SubagentPolicyState::Default => {}
        }
    }
    policy_snapshot_from_legacy_lists(allowed, disabled)
}

/// Converts legacy allow and deny lists into a deterministic unified policy.
/// 将旧版允许与禁止列表转换为确定性的统一策略。
fn policy_snapshot_from_legacy_lists(
    allowed: Vec<String>,
    disabled: Vec<String>,
) -> CapabilityPolicySnapshot {
    vulcan_session_core::capability_policy_from_selector_lists(allowed, disabled)
}

/// Applies a unified tool policy to the legacy exposure lists used by older call sites.
/// 将统一工具策略应用到旧调用点仍使用的可见性列表。
fn apply_tool_policy_to_exposure(
    snapshot: &mut ToolExposureSnapshot,
    policy: CapabilityPolicySnapshot,
) {
    vulcan_session_core::apply_tool_capability_policy(snapshot, policy);
}

/// Applies a unified prompt-skill policy to the legacy prompt-skill lists.
/// 将统一提示词技能策略应用到旧提示词技能列表。
fn apply_prompt_skill_policy(snapshot: &mut PromptSkillSnapshot, policy: CapabilityPolicySnapshot) {
    vulcan_session_core::apply_prompt_skill_capability_policy(snapshot, policy);
}

/// Normalizes one tool exposure snapshot in place.
/// 原地规范化一个工具可见性快照。
fn normalize_tool_exposure(snapshot: &mut ToolExposureSnapshot) {
    snapshot.allowed_tools = normalized_string_list(std::mem::take(&mut snapshot.allowed_tools));
    snapshot.disabled_tools = normalized_string_list(std::mem::take(&mut snapshot.disabled_tools));
    snapshot.allowed_ai_capabilities =
        normalized_string_list(std::mem::take(&mut snapshot.allowed_ai_capabilities));
    snapshot.disabled_ai_capabilities =
        normalized_string_list(std::mem::take(&mut snapshot.disabled_ai_capabilities));
    if snapshot.policy.mode == CapabilityPolicyMode::All && snapshot.policy.selectors.is_empty() {
        let (legacy_allowed, legacy_disabled) = legacy_tool_policy_lists_from_exposure(snapshot);
        let policy = policy_snapshot_from_legacy_lists(legacy_allowed, legacy_disabled);
        apply_tool_policy_to_exposure(snapshot, policy);
    } else {
        let policy = std::mem::take(&mut snapshot.policy);
        apply_tool_policy_to_exposure(snapshot, policy);
    }
    vulcan_session_core::sanitize_child_agent_tool_exposure(snapshot);
}

/// Collects legacy generic, LuaSkills, and MCP tool selectors for policy migration.
/// 收集旧版通用、LuaSkills 与 MCP 工具选择器以执行策略迁移。
fn legacy_tool_policy_lists_from_exposure(
    snapshot: &ToolExposureSnapshot,
) -> (Vec<String>, Vec<String>) {
    let mut allowed = snapshot.allowed_tools.clone();
    let mut disabled = snapshot.disabled_tools.clone();
    append_legacy_luaskills_policy_selectors(
        &snapshot.luaskills_overrides,
        &mut allowed,
        &mut disabled,
    );
    append_legacy_mcp_tool_policy_selectors(
        &snapshot.mcp_server_overrides,
        &mut allowed,
        &mut disabled,
    );
    (allowed, disabled)
}

/// Appends legacy LuaSkills tool selectors from a flexible JSON payload.
/// 从灵活 JSON 载荷追加旧版 LuaSkills 工具选择器。
fn append_legacy_luaskills_policy_selectors(
    value: &Value,
    allowed: &mut Vec<String>,
    disabled: &mut Vec<String>,
) {
    append_legacy_json_policy_selectors(
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
        allowed,
        disabled,
    );
}

/// Appends legacy MCP tool selectors while leaving server selectors as advanced compatibility data.
/// 追加旧版 MCP 工具选择器，同时将服务器选择器保留为高级兼容数据。
fn append_legacy_mcp_tool_policy_selectors(
    value: &Value,
    allowed: &mut Vec<String>,
    disabled: &mut Vec<String>,
) {
    append_legacy_json_policy_selectors(
        value,
        &[
            "allow",
            "allowed",
            "enabled",
            "include",
            "tools",
            "mcp_tools",
        ],
        &["deny", "denied", "disabled", "disable", "exclude"],
        allowed,
        disabled,
    );
}

/// Appends string selectors from legacy JSON strings, arrays, or objects.
/// 从旧版 JSON 字符串、数组或对象追加字符串选择器。
fn append_legacy_json_policy_selectors(
    value: &Value,
    allowed_keys: &[&str],
    disabled_keys: &[&str],
    allowed: &mut Vec<String>,
    disabled: &mut Vec<String>,
) {
    match value {
        Value::String(_) | Value::Array(_) => append_legacy_json_string_values(allowed, value),
        Value::Object(map) => {
            for key in allowed_keys {
                if let Some(value) = map.get(*key) {
                    append_legacy_json_string_values(allowed, value);
                }
            }
            for key in disabled_keys {
                if let Some(value) = map.get(*key) {
                    append_legacy_json_string_values(disabled, value);
                }
            }
        }
        _ => {}
    }
}

/// Appends string or string-array JSON values into one selector list.
/// 将字符串或字符串数组 JSON 值追加到选择器列表。
fn append_legacy_json_string_values(target: &mut Vec<String>, value: &Value) {
    match value {
        Value::String(text) => {
            if let Some(value) = non_empty_text(text) {
                target.push(value.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                if let Value::String(text) = item
                    && let Some(value) = non_empty_text(text)
                {
                    target.push(value.to_string());
                }
            }
        }
        _ => {}
    }
}

/// Returns whether any field in a tool exposure snapshot carries an explicit override.
/// 返回工具可见性快照中是否存在任何显式覆盖字段。
fn tool_exposure_snapshot_has_override(snapshot: &ToolExposureSnapshot) -> bool {
    snapshot.policy.mode != CapabilityPolicyMode::All
        || !snapshot.policy.selectors.is_empty()
        || !snapshot.allowed_tools.is_empty()
        || !snapshot.disabled_tools.is_empty()
        || !snapshot.allowed_ai_capabilities.is_empty()
        || !snapshot.disabled_ai_capabilities.is_empty()
        || !snapshot.luaskills_overrides.is_null()
        || !snapshot.mcp_server_overrides.is_null()
}

/// Returns whether one legacy tool exposure payload is empty for sidecar serialization.
/// 返回旧版工具可见性载荷在 sidecar 序列化中是否为空。
fn tool_exposure_snapshot_is_empty(snapshot: &ToolExposureSnapshot) -> bool {
    snapshot.policy.mode == CapabilityPolicyMode::All
        && snapshot.policy.selectors.is_empty()
        && snapshot.allowed_tools.is_empty()
        && snapshot.disabled_tools.is_empty()
        && snapshot.allowed_ai_capabilities.is_empty()
        && snapshot.disabled_ai_capabilities.is_empty()
        && snapshot.luaskills_overrides.is_null()
        && snapshot.mcp_server_overrides.is_null()
}

/// Normalizes one prompt skill snapshot in place.
/// 原地规范化一个提示词技能快照。
fn normalize_prompt_skills(snapshot: &mut PromptSkillSnapshot) {
    snapshot.enabled = normalized_string_list(std::mem::take(&mut snapshot.enabled));
    snapshot.disabled = normalized_string_list(std::mem::take(&mut snapshot.disabled));
    if snapshot.policy.mode == CapabilityPolicyMode::All && snapshot.policy.selectors.is_empty() {
        let policy =
            policy_snapshot_from_legacy_lists(snapshot.enabled.clone(), snapshot.disabled.clone());
        apply_prompt_skill_policy(snapshot, policy);
    } else {
        let policy = std::mem::take(&mut snapshot.policy);
        apply_prompt_skill_policy(snapshot, policy);
    }
}

/// Returns whether one legacy prompt skill payload is empty for sidecar serialization.
/// 返回旧版提示词技能载荷在 sidecar 序列化中是否为空。
fn prompt_skill_snapshot_is_empty(snapshot: &PromptSkillSnapshot) -> bool {
    snapshot.policy.mode == CapabilityPolicyMode::All
        && snapshot.policy.selectors.is_empty()
        && snapshot.enabled.is_empty()
        && snapshot.disabled.is_empty()
}

/// Normalizes string lists by trimming empties and preserving first occurrence order.
/// 通过裁剪空值并保留首次出现顺序来规范化字符串列表。
fn normalized_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let Some(trimmed) = non_empty_string(value) else {
            continue;
        };
        if !normalized.iter().any(|existing| existing == &trimmed) {
            normalized.push(trimmed);
        }
    }
    normalized
}

/// Returns a non-empty trimmed owned string.
/// 返回非空的已裁剪字符串。
fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Returns a non-empty trimmed owned string from an optional value.
/// 从可选值返回非空的已裁剪字符串。
fn non_empty_owned(value: Option<String>) -> Option<String> {
    value.and_then(non_empty_string)
}

/// Builds a human display name from one canonical id.
/// 从规范标识构建人类可读显示名。
fn display_name_from_id(id: &str) -> String {
    id.rsplit('/')
        .next()
        .unwrap_or(id)
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds a stable source key for one effective definition.
/// 为单个生效定义构建稳定来源键。
fn source_key_for_definition(
    scope: SubagentDefinitionScope,
    _package_id: Option<&str>,
    id: &str,
) -> String {
    match scope {
        SubagentDefinitionScope::System => format!("system:{id}"),
        SubagentDefinitionScope::User => format!("user:{id}"),
        SubagentDefinitionScope::Package => format!("package:{id}"),
    }
}

/// Returns the display category id for one canonical agent id.
/// 返回单个规范智能体标识对应的展示分类标识。
pub fn display_subagent_category_id(id: &str, package_id: Option<&str>) -> Option<String> {
    let package_id = package_id.map(str::trim).filter(|value| !value.is_empty());
    let relative_id = package_id
        .and_then(|package_id| id.strip_prefix(&format!("{package_id}/")))
        .unwrap_or(id);
    let (top_level_category, _) = relative_id.split_once('/')?;
    let top_level_category = top_level_category.trim();
    if top_level_category.is_empty() {
        return None;
    }
    package_id
        .map(|package_id| format!("{package_id}/{top_level_category}"))
        .or_else(|| Some(top_level_category.to_string()))
}

/// Returns one category id for a directory relative to the catalog root.
/// 返回一个目录相对目录根的分类标识。
fn category_id_for_directory(base_root: &Path, directory: &Path) -> Option<String> {
    relative_path_id(base_root, directory)
}

/// Returns whether one directory is a first-level display category.
/// 返回一个目录是否为第一层展示分类目录。
fn directory_is_top_level_category(base_root: &Path, directory: &Path) -> bool {
    let Ok(relative) = directory.strip_prefix(base_root) else {
        return false;
    };
    let mut normal_count = 0usize;
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return false;
        };
        if value.to_str().is_none_or(|part| part.trim().is_empty()) {
            return false;
        }
        normal_count += 1;
        if normal_count > 1 {
            return false;
        }
    }
    normal_count == 1
}

/// Returns a canonical id for a file relative to one catalog root.
/// 返回文件相对目录根的规范标识。
fn canonical_id_for_file(base_root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(base_root).ok()?;
    let without_extension = relative.with_extension("");
    path_to_canonical_id(&without_extension)
}

/// Converts a relative path into a slash-separated canonical id.
/// 将相对路径转换为斜杠分隔的规范标识。
fn relative_path_id(base_root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(base_root).ok()?;
    path_to_canonical_id(relative)
}

/// Converts a safe relative path into a canonical id.
/// 将安全相对路径转换为规范标识。
fn path_to_canonical_id(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return None;
        };
        let part = value.to_str()?.trim();
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        parts.push(part.to_string());
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Builds a safe relative directory path from a canonical category id.
/// 从规范分类标识构建安全相对目录路径。
fn safe_relative_directory_path(id: &str) -> Result<PathBuf, SubagentCatalogLoadError> {
    let id = id.trim();
    if id.is_empty() {
        return Err(SubagentCatalogLoadError {
            path: PathBuf::new(),
            message: format!("Invalid subagent category id `{id}`."),
        });
    }
    let mut path = PathBuf::new();
    for part in id.split('/') {
        let part = part.trim();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('\\')
            || part.contains(':')
        {
            return Err(SubagentCatalogLoadError {
                path: PathBuf::from(id),
                message: format!("Invalid subagent category id `{id}`."),
            });
        }
        path.push(part);
    }
    Ok(path)
}

/// Builds a safe relative file path from a canonical id and extension.
/// 从规范标识和扩展名构建安全相对文件路径。
fn safe_relative_agent_path(
    id: &str,
    extension: &str,
) -> Result<PathBuf, SubagentCatalogLoadError> {
    let id = id.trim();
    if id.is_empty() {
        return Err(SubagentCatalogLoadError {
            path: PathBuf::new(),
            message: format!("Invalid subagent definition id `{id}`."),
        });
    }
    let mut path = PathBuf::new();
    for part in id.split('/') {
        let part = part.trim();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('\\')
            || part.contains(':')
        {
            return Err(SubagentCatalogLoadError {
                path: PathBuf::new(),
                message: format!("Invalid subagent definition id `{id}`."),
            });
        }
        path.push(part);
    }
    path.set_extension(extension);
    Ok(path)
}

/// Returns the UTF-8 stem used as the stable agent id.
/// 返回用作稳定代理标识的 UTF-8 文件 stem。
fn definition_file_stem(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .map(str::to_string)
}

/// Returns whether a path has the supported Markdown extension.
/// 返回路径是否具有受支持的 Markdown 扩展名。
fn path_has_markdown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(SUBAGENT_DEFINITION_EXTENSION))
}

/// Returns whether a path has a supported sidecar extension.
/// 返回路径是否具有受支持的 sidecar 扩展名。
fn path_has_sidecar_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case(YAML_EXTENSION)
                || extension.eq_ignore_ascii_case(YML_EXTENSION)
                || extension.eq_ignore_ascii_case(JSON_EXTENSION)
        })
}

/// Returns whether a path has a JSON extension.
/// 返回路径是否具有 JSON 扩展名。
fn path_has_json_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(JSON_EXTENSION))
}

/// Returns trimmed text only when it is non-empty.
/// 仅在文本非空时返回裁剪后的文本。
fn non_empty_text(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Splits YAML frontmatter from Markdown body.
/// 从 Markdown 正文中拆分 YAML frontmatter。
fn split_frontmatter(raw: &str) -> Option<(&str, &str)> {
    let rest = raw.strip_prefix("---")?;
    let rest = rest
        .strip_prefix('\n')
        .or_else(|| rest.strip_prefix("\r\n"))?;
    let marker = rest.find("\n---").or_else(|| rest.find("\r\n---"))?;
    let frontmatter = &rest[..marker];
    let body_start = marker
        + if rest[marker..].starts_with("\r\n---") {
            "\r\n---".len()
        } else {
            "\n---".len()
        };
    let body = rest[body_start..]
        .strip_prefix("\r\n")
        .or_else(|| rest[body_start..].strip_prefix('\n'))
        .unwrap_or(&rest[body_start..]);
    Some((frontmatter, body))
}

/// YAML frontmatter for one subagent Markdown definition.
/// 单个子代理 Markdown 定义的 YAML frontmatter。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubagentDefinitionFrontmatter {
    /// Optional display name.
    /// 可选显示名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional description.
    /// 可选说明。
    #[serde(default)]
    description: Option<String>,
    /// Optional display emoji.
    /// 可选展示图标。
    #[serde(default)]
    emoji: Option<String>,
    /// Optional display color.
    /// 可选展示颜色。
    #[serde(default)]
    color: Option<String>,
    /// Optional short tone or persona hint.
    /// 可选简短语气或人格提示。
    #[serde(default)]
    vibe: Option<String>,
    /// Optional localized display metadata.
    /// 可选本地化展示元数据。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    locales: BTreeMap<String, SubagentDefinitionLocaleSidecar>,
    /// Optional enabled state.
    /// 可选启用状态。
    #[serde(default)]
    enabled: Option<bool>,
    /// Optional base system prompt.
    /// 可选基础系统提示词。
    #[serde(default)]
    system_prompt: Option<String>,
    /// Optional appended system prompt.
    /// 可选追加系统提示词。
    #[serde(default)]
    system_prompt_append: Option<String>,
    /// Provider and model override snapshot.
    /// 供应商与模型覆盖快照。
    #[serde(default)]
    provider_model: ProviderModelSnapshot,
    /// Tool exposure override snapshot.
    /// 工具可见性覆盖快照。
    #[serde(default)]
    tool_exposure: ToolExposureSnapshot,
    /// Prompt skill override snapshot.
    /// 提示词技能覆盖快照。
    #[serde(default)]
    prompt_skills: PromptSkillSnapshot,
    /// Context compaction override snapshot.
    /// 上下文压缩覆盖快照。
    #[serde(default)]
    context_compaction: ContextCompactionPolicyRequest,
    /// Runtime policy override request.
    /// 运行时策略覆盖请求。
    #[serde(default)]
    runtime_policy: ExecutionRuntimePolicyRequest,
    /// Optional maximum nested task depth.
    /// 可选最大嵌套任务深度。
    #[serde(default)]
    max_depth: Option<u16>,
    /// Optional remote source URL.
    /// 可选远程来源 URL。
    #[serde(default)]
    remote_url: Option<String>,
    /// Optional system definition id overridden by this definition.
    /// 该定义覆盖的可选系统定义标识。
    #[serde(default)]
    override_of: Option<String>,
    /// Optional source hash.
    /// 可选来源哈希。
    #[serde(default)]
    source_hash: Option<String>,
    /// Optional source version.
    /// 可选来源版本。
    #[serde(default)]
    version: Option<String>,
    /// Optional remote ETag.
    /// 可选远程 ETag。
    #[serde(default)]
    etag: Option<String>,
    /// Optional remote Last-Modified header.
    /// 可选远程 Last-Modified 头。
    #[serde(default)]
    last_modified: Option<String>,
    /// Optional update timestamp.
    /// 可选更新时间戳。
    #[serde(default)]
    updated_at: Option<String>,
    /// Optional extension payload.
    /// 可选扩展载荷。
    #[serde(default)]
    extra: Option<Value>,
}

/// Definition sidecar upstream or license source metadata.
/// 定义 sidecar 上游或协议来源元数据。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubagentOriginSidecar {
    /// Source kind such as github, system, or other.
    /// 来源类型，例如 github、system 或 other。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    /// Optional human-facing source label.
    /// 可选的人类可读来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    /// Optional icon key used by WebUI.
    /// WebUI 使用的可选图标键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    /// Optional source URL.
    /// 可选来源链接。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

/// Definition sidecar localized metadata.
/// 定义 sidecar 本地化元数据。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubagentDefinitionLocaleSidecar {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// Optional localized description.
    /// 可选本地化说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

/// Shareable YAML frontmatter written back to Markdown files.
/// 写回 Markdown 文件的可分享 YAML frontmatter。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubagentMarkdownFrontmatter {
    /// Optional display name.
    /// 可选显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// Optional description.
    /// 可选说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Optional display emoji.
    /// 可选展示图标。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    emoji: Option<String>,
    /// Optional display color.
    /// 可选展示颜色。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    /// Optional short tone or persona hint.
    /// 可选简短语气或人格提示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vibe: Option<String>,
}

/// Sidecar metadata for one subagent definition.
/// 单个子代理定义的 sidecar 元数据。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubagentDefinitionSidecar {
    /// Optional schema version.
    /// 可选 schema 版本。
    #[serde(default)]
    schema_version: Option<u32>,
    /// Optional display name.
    /// 可选显示名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional description.
    /// 可选说明。
    #[serde(default)]
    description: Option<String>,
    /// Optional display emoji.
    /// 可选展示图标。
    #[serde(default)]
    emoji: Option<String>,
    /// Optional display color.
    /// 可选展示颜色。
    #[serde(default)]
    color: Option<String>,
    /// Optional short tone or persona hint.
    /// 可选简短语气或人格提示。
    #[serde(default)]
    vibe: Option<String>,
    /// Optional localized display metadata.
    /// 可选本地化展示元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    locales: Option<BTreeMap<String, SubagentDefinitionLocaleSidecar>>,
    /// Optional upstream or license source metadata.
    /// 可选上游或协议来源元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<SubagentOriginSidecar>,
    /// Optional enabled state.
    /// 可选启用状态。
    #[serde(default)]
    enabled: Option<bool>,
    /// Optional appended system prompt.
    /// 可选追加系统提示词。
    #[serde(default)]
    system_prompt_append: Option<String>,
    /// Provider and model override snapshot.
    /// 供应商与模型覆盖快照。
    #[serde(default)]
    provider_model: ProviderModelSnapshot,
    /// Legacy tool exposure snapshot.
    /// 旧版工具可见性快照。
    #[serde(default, skip_serializing_if = "tool_exposure_snapshot_is_empty")]
    tool_exposure: ToolExposureSnapshot,
    /// Unified tool policy, compatible with the legacy three-state map.
    /// 统一工具策略，兼容旧版三态映射。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_policy: Option<SubagentToolPolicySidecar>,
    /// Legacy prompt skill snapshot.
    /// 旧版提示词技能快照。
    #[serde(default, skip_serializing_if = "prompt_skill_snapshot_is_empty")]
    prompt_skills: PromptSkillSnapshot,
    /// Unified prompt skill policy, compatible with the legacy three-state map.
    /// 统一提示词技能策略，兼容旧版三态映射。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt_skill_policy: Option<SubagentPromptSkillPolicySidecar>,
    /// Context compaction override snapshot.
    /// 上下文压缩覆盖快照。
    #[serde(default)]
    context_compaction: ContextCompactionPolicyRequest,
    /// Runtime policy override request.
    /// 运行时策略覆盖请求。
    #[serde(default)]
    runtime_policy: ExecutionRuntimePolicyRequest,
    /// Optional maximum nested task depth.
    /// 可选最大嵌套任务深度。
    #[serde(default)]
    max_depth: Option<u16>,
    /// Optional user-facing package id.
    /// 可选的用户可见包标识。
    #[serde(default, rename = "packageId")]
    package_id: Option<String>,
    /// Optional remote source URL.
    /// 可选远程来源 URL。
    #[serde(default)]
    remote_url: Option<String>,
    /// Optional system definition id overridden by this metadata.
    /// 当前元数据覆盖的可选系统定义标识。
    #[serde(default)]
    override_of: Option<String>,
    /// Optional source hash.
    /// 可选来源哈希。
    #[serde(default)]
    source_hash: Option<String>,
    /// Optional source version.
    /// 可选来源版本。
    #[serde(default)]
    version: Option<String>,
    /// Optional remote ETag.
    /// 可选远程 ETag。
    #[serde(default)]
    etag: Option<String>,
    /// Optional remote Last-Modified header.
    /// 可选远程 Last-Modified 头。
    #[serde(default)]
    last_modified: Option<String>,
    /// Optional update timestamp.
    /// 可选更新时间戳。
    #[serde(default)]
    updated_at: Option<String>,
    /// Optional extension payload.
    /// 可选扩展载荷。
    #[serde(default)]
    extra: Option<Value>,
}

/// Unified tool policy sidecar with legacy map compatibility.
/// 兼容旧版映射的统一工具策略 sidecar。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum SubagentToolPolicySidecar {
    /// Modern mode-plus-list tool policy.
    /// 新版规则加列表工具策略。
    Modern(SubagentToolPolicyObject),
    /// Legacy per-tool state map.
    /// 旧版逐工具状态映射。
    Legacy(BTreeMap<String, SubagentPolicyState>),
}

impl SubagentToolPolicySidecar {
    /// Builds a sidecar policy from a normalized snapshot.
    /// 从规范化快照构建 sidecar 策略。
    fn from_snapshot(snapshot: &CapabilityPolicySnapshot) -> Option<Self> {
        if snapshot.mode == CapabilityPolicyMode::All && snapshot.selectors.is_empty() {
            return None;
        }
        Some(Self::Modern(SubagentToolPolicyObject {
            mode: snapshot.mode,
            tools: snapshot.selectors.clone(),
        }))
    }

    /// Converts this sidecar policy into the shared snapshot shape.
    /// 将当前 sidecar 策略转换为共享快照结构。
    fn into_snapshot(self) -> CapabilityPolicySnapshot {
        match self {
            Self::Modern(policy) => CapabilityPolicySnapshot {
                mode: policy.mode,
                selectors: policy.tools,
            },
            Self::Legacy(policy) => policy_snapshot_from_state_map(&policy),
        }
    }
}

/// Modern tool policy object serialized to YAML.
/// 序列化到 YAML 的新版工具策略对象。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubagentToolPolicyObject {
    /// Tool policy mode.
    /// 工具策略模式。
    #[serde(default)]
    mode: CapabilityPolicyMode,
    /// Tool selectors used by the mode.
    /// 当前模式使用的工具选择器。
    #[serde(default, alias = "selectors")]
    tools: Vec<String>,
}

/// Unified prompt-skill policy sidecar with legacy map compatibility.
/// 兼容旧版映射的统一提示词技能策略 sidecar。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum SubagentPromptSkillPolicySidecar {
    /// Modern mode-plus-list prompt skill policy.
    /// 新版规则加列表提示词技能策略。
    Modern(SubagentPromptSkillPolicyObject),
    /// Legacy per-skill state map.
    /// 旧版逐技能状态映射。
    Legacy(BTreeMap<String, SubagentPolicyState>),
}

impl SubagentPromptSkillPolicySidecar {
    /// Builds a sidecar policy from a normalized snapshot.
    /// 从规范化快照构建 sidecar 策略。
    fn from_snapshot(snapshot: &CapabilityPolicySnapshot) -> Option<Self> {
        if snapshot.mode == CapabilityPolicyMode::All && snapshot.selectors.is_empty() {
            return None;
        }
        Some(Self::Modern(SubagentPromptSkillPolicyObject {
            mode: snapshot.mode,
            skills: snapshot.selectors.clone(),
        }))
    }

    /// Converts this sidecar policy into the shared snapshot shape.
    /// 将当前 sidecar 策略转换为共享快照结构。
    fn into_snapshot(self) -> CapabilityPolicySnapshot {
        match self {
            Self::Modern(policy) => CapabilityPolicySnapshot {
                mode: policy.mode,
                selectors: policy.skills,
            },
            Self::Legacy(policy) => policy_snapshot_from_state_map(&policy),
        }
    }
}

/// Modern prompt skill policy object serialized to YAML.
/// 序列化到 YAML 的新版提示词技能策略对象。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubagentPromptSkillPolicyObject {
    /// Prompt skill policy mode.
    /// 提示词技能策略模式。
    #[serde(default)]
    mode: CapabilityPolicyMode,
    /// Prompt skill selectors used by the mode.
    /// 当前模式使用的提示词技能选择器。
    #[serde(default, alias = "selectors", alias = "tools")]
    skills: Vec<String>,
}

/// Three-state policy value for tools and prompt skills.
/// 工具与提示词技能使用的三态策略值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum SubagentPolicyState {
    /// Inherit the lower layer or system default.
    /// 继承下层或系统默认值。
    Default,
    /// Explicitly allow the capability at this layer.
    /// 在当前层显式允许该能力。
    Allow,
    /// Explicitly deny the capability at this layer.
    /// 在当前层显式禁用该能力。
    Deny,
}

/// Category sidecar metadata.
/// 分类 sidecar 元数据。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubagentCategorySidecar {
    /// Optional display name.
    /// 可选显示名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional description.
    /// 可选说明。
    #[serde(default)]
    description: Option<String>,
    /// Optional display emoji.
    /// 可选展示图标。
    #[serde(default)]
    emoji: Option<String>,
    /// Optional display color.
    /// 可选展示颜色。
    #[serde(default)]
    color: Option<String>,
    /// Optional display order.
    /// 可选展示顺序。
    #[serde(default)]
    order: Option<i32>,
    /// Optional localized display metadata.
    /// 可选本地化展示元数据。
    #[serde(default)]
    locales: BTreeMap<String, SubagentCategoryLocaleSidecar>,
}

/// Category sidecar localized metadata.
/// 分类 sidecar 本地化元数据。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubagentCategoryLocaleSidecar {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional localized description.
    /// 可选本地化说明。
    #[serde(default)]
    description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Verifies definitions default their id from the Markdown file stem.
    /// 验证定义默认从 Markdown 文件 stem 取得标识。
    #[test]
    fn parses_definition_id_from_file_stem() {
        let path = PathBuf::from("code-reviewer.md");
        let definition = parse_subagent_definition_text(
            &path,
            "---\nname: Code Reviewer\n---\nReview code.",
            SubagentDefinitionScope::User,
        )
        .expect("definition should parse");

        assert_eq!(definition.id, "code-reviewer");
        assert_eq!(definition.name, "Code Reviewer");
        assert_eq!(definition.mode, SubagentDefinitionMode::Expert);
        assert_eq!(definition.system_prompt.as_deref(), Some("Review code."));
    }

    /// Verifies relative logical Markdown paths preserve category prefixes in parsed ids.
    /// 验证相对逻辑 Markdown 路径会在解析标识中保留分类前缀。
    #[test]
    fn parses_definition_id_from_relative_logical_path() {
        let path = PathBuf::from("engineering/code-reviewer.md");
        let definition = parse_subagent_definition_text(
            &path,
            "---\nname: Code Reviewer\n---\nReview code.",
            SubagentDefinitionScope::User,
        )
        .expect("definition should parse");

        assert_eq!(definition.id, "engineering/code-reviewer");
        assert_eq!(definition.name, "Code Reviewer");
    }

    /// Verifies user definitions cannot replace system definitions by string id.
    /// 验证用户定义不能按字符串标识替换系统定义。
    #[test]
    fn user_definition_conflict_preserves_system_definition_by_id() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-catalog-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let system_root = root.join("system");
        let user_root = root.join("user");
        fs::create_dir_all(&system_root).expect("system root");
        fs::create_dir_all(&user_root).expect("user root");
        fs::write(
            system_root.join("reviewer.md"),
            "---\nname: System Reviewer\n---\nSystem.",
        )
        .expect("system file");
        fs::write(
            user_root.join("reviewer.md"),
            "---\nname: User Reviewer\nenabled: false\n---\nUser.",
        )
        .expect("user file");

        let catalog = load_subagent_catalog_from_roots(system_root, user_root);
        let definition = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "reviewer")
            .expect("merged definition");

        assert_eq!(definition.name, "System Reviewer");
        assert!(definition.enabled);
        assert!(catalog.enabled_definition("reviewer").is_some());
        assert!(
            catalog
                .errors
                .iter()
                .any(|error| error.message.contains("conflicts with an existing system"))
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies minimal Markdown without frontmatter can define an agent prompt.
    /// 验证不含 frontmatter 的最小 Markdown 可以定义智能体提示词。
    #[test]
    fn parses_minimal_markdown_without_frontmatter() {
        let definition = parse_subagent_definition_text(
            &PathBuf::from("plain-reviewer.md"),
            "Review the current change.",
            SubagentDefinitionScope::User,
        )
        .expect("minimal definition should parse");

        assert_eq!(definition.id, "plain-reviewer");
        assert_eq!(definition.name, "Plain Reviewer");
        assert_eq!(definition.emoji, DEFAULT_AGENT_EMOJI);
        assert_eq!(definition.color, DEFAULT_AGENT_COLOR);
        assert_eq!(
            definition.system_prompt.as_deref(),
            Some("Review the current change.")
        );
    }

    /// Verifies sidecar metadata overrides Markdown frontmatter and supports categorized ids.
    /// 验证 sidecar 元数据会覆盖 Markdown frontmatter 并支持分类标识。
    #[test]
    fn sidecar_overrides_markdown_metadata_for_categorized_definition() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-sidecar-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let system_root = root.join("system");
        let engineering_root = system_root.join("engineering");
        fs::create_dir_all(&engineering_root).expect("category root");
        fs::write(
            engineering_root.join("reviewer.md"),
            "---\nname: Markdown Reviewer\nemoji: M\n---\nReview.",
        )
        .expect("markdown file");
        fs::write(
            engineering_root.join("reviewer.yaml"),
            "schemaVersion: 1\nname: Sidecar Reviewer\nemoji: S\ncolor: blue\nsource:\n  kind: github\n  label: Example/reviewer\n  icon: github\n  url: https://example.test/reviewer\nlocales:\n  zh-CN:\n    name: 旁车审核专家\n    description: 本地化说明。\ntoolPolicy:\n  shell_command: allow\n",
        )
        .expect("sidecar file");
        fs::write(
            engineering_root.join("_category.yaml"),
            "name: Engineering\nemoji: E\ncolor: blue\nlocales:\n  zh-CN:\n    name: 工程\n    description: 工程专家。\n",
        )
        .expect("category sidecar");

        let catalog = load_subagent_catalog_from_roots(system_root, PathBuf::new());
        let definition = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "engineering/reviewer")
            .expect("categorized definition");

        assert_eq!(definition.name, "Sidecar Reviewer");
        assert_eq!(definition.emoji, "S");
        assert_eq!(definition.mode, SubagentDefinitionMode::Expert);
        assert_eq!(
            definition
                .locales
                .get("zh-CN")
                .and_then(|metadata| metadata.name.as_deref()),
            Some("旁车审核专家")
        );
        assert_eq!(
            definition
                .origin
                .as_ref()
                .and_then(|origin| origin.kind.as_deref()),
            Some("github")
        );
        assert_eq!(
            definition
                .origin
                .as_ref()
                .and_then(|origin| origin.url.as_deref()),
            Some("https://example.test/reviewer")
        );
        assert_eq!(
            definition.tool_exposure.allowed_tools,
            vec!["shell_command"]
        );
        assert_eq!(
            definition.tool_exposure.policy.mode,
            CapabilityPolicyMode::AllowOnly
        );
        assert_eq!(
            definition.tool_exposure.policy.selectors,
            vec!["shell_command"]
        );
        assert!(definition.tool_exposure.disabled_tools.is_empty());
        assert!(
            catalog
                .categories
                .iter()
                .any(|category| category.id == "engineering" && category.name == "Engineering")
        );
        let category = catalog
            .categories
            .iter()
            .find(|category| category.id == "engineering")
            .expect("engineering category should load");
        assert_eq!(
            category
                .locales
                .get("zh-CN")
                .and_then(|metadata| metadata.name.as_deref()),
            Some("工程")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies legacy deny-only sidecar maps drop parent-owned child-agent tools.
    /// 验证旧版仅禁用 sidecar 映射会移除父会话拥有的智能体禁用工具。
    #[test]
    fn sidecar_legacy_deny_tool_policy_drops_child_forbidden_tools() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-sidecar-deny-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let system_root = root.join("system");
        fs::create_dir_all(&system_root).expect("system root");
        fs::write(system_root.join("reviewer.md"), "Review.").expect("markdown file");
        fs::write(
            system_root.join("reviewer.yaml"),
            "schemaVersion: 1\ntoolPolicy:\n  spawn_agent: deny\n",
        )
        .expect("sidecar file");

        let catalog = load_subagent_catalog_from_roots(system_root, PathBuf::new());
        let definition = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "reviewer")
            .expect("definition");

        assert_eq!(
            definition.tool_exposure.policy.mode,
            CapabilityPolicyMode::All
        );
        assert!(definition.tool_exposure.policy.selectors.is_empty());
        assert!(definition.tool_exposure.allowed_tools.is_empty());
        assert!(definition.tool_exposure.disabled_tools.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies override sidecars can disable system definitions without copying Markdown.
    /// 验证覆盖 sidecar 可以在不复制 Markdown 的情况下禁用系统定义。
    #[test]
    fn override_sidecar_disables_system_definition_without_prompt_copy() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-override-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let system_root = root.join("system");
        let user_root = root.join("user");
        fs::create_dir_all(system_root.join("engineering")).expect("system category root");
        fs::create_dir_all(user_root.join("overrides").join("engineering"))
            .expect("override category root");
        fs::write(
            system_root.join("engineering").join("reviewer.md"),
            "---\nname: Reviewer\n---\nReview.",
        )
        .expect("system file");
        fs::write(
            system_root.join("engineering").join("reviewer.yaml"),
            "source:\n  kind: github\n  label: Example/reviewer\n  icon: github\n  url: https://example.test/reviewer\nlocales:\n  zh-CN:\n    name: 系统审核专家\n    description: 系统本地化说明。\n",
        )
        .expect("system sidecar");
        fs::write(
            user_root
                .join("overrides")
                .join("engineering")
                .join("reviewer.yaml"),
            "enabled: false\n",
        )
        .expect("override file");

        let catalog = load_subagent_catalog_from_roots(system_root, user_root);
        let definition = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "engineering/reviewer")
            .expect("system definition");

        assert!(!definition.enabled);
        assert_eq!(definition.source.scope, SubagentDefinitionScope::System);
        assert_eq!(
            definition
                .origin
                .as_ref()
                .and_then(|origin| origin.url.as_deref()),
            Some("https://example.test/reviewer")
        );
        assert_eq!(
            definition
                .locales
                .get("zh-CN")
                .and_then(|metadata| metadata.name.as_deref()),
            Some("系统审核专家")
        );
        assert!(catalog.enabled_definition("engineering/reviewer").is_none());

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies unique legacy short ids still resolve categorized definitions.
    /// 验证唯一旧版短标识仍可解析分类定义。
    #[test]
    fn enabled_definition_resolves_unique_short_id() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-short-id-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let system_root = root.join("system");
        let user_root = root.join("user");
        fs::create_dir_all(system_root.join("engineering")).expect("system category root");
        fs::write(
            system_root.join("engineering").join("reviewer.md"),
            "---\nname: Reviewer\n---\nReview.",
        )
        .expect("system file");

        let catalog = load_subagent_catalog_from_roots(system_root, user_root);
        let definition = catalog
            .enabled_definition("reviewer")
            .expect("unique short id should resolve");

        assert_eq!(definition.id, "engineering/reviewer");

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies ambiguous legacy short ids are rejected instead of guessed.
    /// 验证存在歧义的旧版短标识会被拒绝而不是猜测解析。
    #[test]
    fn enabled_definition_rejects_ambiguous_short_id() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-short-id-ambiguous-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let system_root = root.join("system");
        let user_root = root.join("user");
        fs::create_dir_all(system_root.join("engineering")).expect("engineering category root");
        fs::create_dir_all(system_root.join("architecture")).expect("architecture category root");
        fs::write(
            system_root.join("engineering").join("reviewer.md"),
            "---\nname: Engineering Reviewer\n---\nReview engineering.",
        )
        .expect("engineering file");
        fs::write(
            system_root.join("architecture").join("reviewer.md"),
            "---\nname: Architecture Reviewer\n---\nReview architecture.",
        )
        .expect("architecture file");

        let catalog = load_subagent_catalog_from_roots(system_root, user_root);

        assert!(catalog.enabled_definition("reviewer").is_none());
        assert!(catalog.enabled_definition("engineering/reviewer").is_some());

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies nested directories keep full ids while filtering under the top category.
    /// 验证深层目录保留完整标识但归入顶层分类筛选。
    #[test]
    fn nested_definition_uses_top_level_display_category() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-nested-category-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let system_root = root.join("system");
        let nested_root = system_root.join("game-development").join("blender");
        fs::create_dir_all(&nested_root).expect("nested category root");
        fs::write(
            system_root.join("game-development").join("_category.yaml"),
            "name: Game Development\nemoji: G\ncolor: purple\n",
        )
        .expect("top category sidecar");
        fs::write(
            nested_root.join("_category.yaml"),
            "name: Blender\nemoji: B\ncolor: orange\n",
        )
        .expect("nested category sidecar");
        fs::write(
            nested_root.join("asset-reviewer.md"),
            "---\nname: Asset Reviewer\n---\nReview game assets.",
        )
        .expect("nested definition");

        let catalog = load_subagent_catalog_from_roots(system_root, PathBuf::new());
        let definition = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "game-development/blender/asset-reviewer")
            .expect("nested definition should load");

        assert_eq!(
            definition.source.category_id.as_deref(),
            Some("game-development")
        );
        assert!(
            catalog
                .categories
                .iter()
                .any(|category| category.id == "game-development")
        );
        assert!(
            catalog
                .categories
                .iter()
                .all(|category| category.id != "game-development/blender")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies package definitions include the package id in their canonical id.
    /// 验证包定义会在规范标识中包含包标识。
    #[test]
    fn package_definitions_use_package_prefixed_ids() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-package-id-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let user_root = root.join("user");
        let alpha_root = user_root.join("packages").join("alpha").join("engineering");
        let beta_root = user_root.join("packages").join("beta").join("engineering");
        fs::create_dir_all(&alpha_root).expect("alpha package root");
        fs::create_dir_all(&beta_root).expect("beta package root");
        fs::write(
            alpha_root.join("reviewer.md"),
            "---\nname: Alpha Reviewer\n---\nReview alpha.",
        )
        .expect("alpha file");
        fs::write(
            beta_root.join("reviewer.md"),
            "---\nname: Beta Reviewer\n---\nReview beta.",
        )
        .expect("beta file");

        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);

        let alpha = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "alpha/engineering/reviewer")
            .expect("alpha package definition");
        let beta = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "beta/engineering/reviewer")
            .expect("beta package definition");

        assert_eq!(
            alpha.source.source_key,
            "package:alpha/engineering/reviewer"
        );
        assert_eq!(beta.source.source_key, "package:beta/engineering/reviewer");
        assert_eq!(alpha.source.package_id.as_deref(), Some("alpha"));
        assert_eq!(beta.source.package_id.as_deref(), Some("beta"));
        assert!(
            catalog
                .categories
                .iter()
                .any(|category| category.id == "alpha/engineering")
        );
        assert!(
            catalog
                .categories
                .iter()
                .any(|category| category.id == "beta/engineering")
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies user definition paths accept category ids and reject traversal.
    /// 验证用户定义路径允许分类标识并拒绝路径穿越。
    #[test]
    fn user_definition_path_accepts_categories_and_rejects_traversal() {
        let root = PathBuf::from("user-root");
        let category_path = user_subagent_definition_path(&root, "folder/reviewer")
            .expect("category ids should be accepted");

        assert_eq!(
            category_path,
            PathBuf::from("user-root")
                .join("definitions")
                .join("folder")
                .join("reviewer.md")
        );

        for id in ["", "../reviewer", r"folder\reviewer", "."] {
            let error = user_subagent_definition_path(&root, id)
                .expect_err("path-like ids should be rejected");
            assert!(error.message.contains("Invalid subagent definition id"));
        }
    }

    /// Verifies catalog loading creates only top-level user category directories.
    /// 验证目录加载只创建用户顶层分类目录。
    #[test]
    fn load_catalog_creates_user_category_directories_from_system_catalog() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-category-sync-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let system_root = root.join("system");
        let user_root = root.join("user");
        fs::create_dir_all(system_root.join("engineering").join("frontend"))
            .expect("system nested category root");
        fs::write(
            system_root.join("engineering").join("reviewer.md"),
            "---\nname: Reviewer\n---\nReview.",
        )
        .expect("system definition");

        let catalog = load_subagent_catalog_from_roots(system_root, user_root.clone());

        assert!(catalog.errors.is_empty());
        assert!(user_root.join("definitions").is_dir());
        assert!(user_root.join("definitions").join("engineering").is_dir());
        assert!(
            !user_root
                .join("definitions")
                .join("engineering")
                .join("frontend")
                .exists()
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies persisted user definitions roundtrip by the same string id.
    /// 验证持久化用户定义会按同一个字符串标识往返。
    #[test]
    fn write_user_definition_roundtrips_by_string_id() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-write-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let user_root = root.join("user");
        let source_file = PathBuf::from("code-reviewer.md");
        let mut definition = parse_subagent_definition_text(
            &source_file,
            "---\nname: Code Reviewer\nenabled: true\n---\nReview code.",
            SubagentDefinitionScope::User,
        )
        .expect("definition should parse");
        definition.source.file = user_root.join("code-reviewer.md");
        definition.source.remote_url = Some(String::from("https://example.test/code-reviewer.md"));
        definition.source.source_hash = Some(String::from("sha256-test"));
        definition.source.etag = Some(String::from("\"remote-etag\""));
        definition.source.last_modified = Some(String::from("Thu, 02 Jul 2026 00:00:00 GMT"));
        definition.source.package_id = Some(String::from("custom-pack"));
        definition.origin = Some(SubagentOriginMetadata {
            kind: Some(String::from("github")),
            label: Some(String::from("Example/code-reviewer")),
            icon: Some(String::from("github")),
            url: Some(String::from("https://example.test/code-reviewer")),
        });

        let written_path = write_user_subagent_definition(&user_root, &definition)
            .expect("definition should write");
        let written_markdown =
            fs::read_to_string(&written_path).expect("written markdown should be readable");
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root.clone());
        let parsed = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "code-reviewer")
            .expect("written definition should load");

        assert_eq!(
            written_path.file_stem().and_then(|stem| stem.to_str()),
            Some("code-reviewer")
        );
        assert_eq!(parsed.id, "code-reviewer");
        assert_eq!(parsed.mode, SubagentDefinitionMode::Expert);
        assert!(parsed.enabled);
        assert_eq!(
            parsed.source.remote_url.as_deref(),
            Some("https://example.test/code-reviewer.md")
        );
        assert_eq!(parsed.source.source_hash.as_deref(), Some("sha256-test"));
        assert_eq!(parsed.source.etag.as_deref(), Some("\"remote-etag\""));
        assert_eq!(
            parsed.source.last_modified.as_deref(),
            Some("Thu, 02 Jul 2026 00:00:00 GMT")
        );
        assert_eq!(parsed.source.package_id.as_deref(), Some("custom-pack"));
        assert_eq!(
            parsed
                .origin
                .as_ref()
                .and_then(|origin| origin.label.as_deref()),
            Some("Example/code-reviewer")
        );
        assert_eq!(
            parsed
                .origin
                .as_ref()
                .and_then(|origin| origin.url.as_deref()),
            Some("https://example.test/code-reviewer")
        );
        assert!(!written_markdown.contains("mode:"));
        assert!(!written_markdown.contains("enabled:"));
        assert!(!written_markdown.contains("remoteUrl:"));
        assert!(!written_markdown.contains("sourceHash:"));

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies persisted empty allow-only tool policy keeps the no-tool semantics after reload.
    /// 验证持久化的空仅允许工具策略在重新加载后仍保持无工具语义。
    #[test]
    fn write_user_definition_preserves_empty_allow_only_tool_policy() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-empty-allow-tool-policy-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let user_root = root.join("user");
        let source_file = PathBuf::from("empty-tools.md");
        let mut definition = parse_subagent_definition_text(
            &source_file,
            "---\nname: Empty Tools\nenabled: true\n---\nRun without regular tools.",
            SubagentDefinitionScope::User,
        )
        .expect("definition should parse");
        definition.tool_exposure.policy = CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::AllowOnly,
            selectors: Vec::new(),
        };

        write_user_subagent_definition(&user_root, &definition).expect("definition should write");
        let sidecar_path =
            user_subagent_sidecar_path(&user_root, "empty-tools").expect("sidecar path");
        let sidecar_text =
            fs::read_to_string(sidecar_path).expect("written sidecar should be readable");
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root.clone());
        let parsed = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "empty-tools")
            .expect("written definition should load");

        assert!(sidecar_text.contains("toolPolicy:"));
        assert!(sidecar_text.contains("mode: allow_only"));
        assert_eq!(
            parsed.tool_exposure.policy.mode,
            CapabilityPolicyMode::AllowOnly
        );
        assert!(parsed.tool_exposure.policy.selectors.is_empty());
        assert!(parsed.tool_exposure.allowed_tools.is_empty());
        assert!(parsed.tool_exposure.disabled_tools.is_empty());

        let _ = fs::remove_dir_all(root);
    }
}
