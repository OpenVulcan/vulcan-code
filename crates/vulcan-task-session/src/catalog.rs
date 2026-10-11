//! Subagent catalog loading and profile conversion.
//! 子代理目录加载与配置转换。

mod capability_policy;
mod directory_scan;
mod path_rules;
mod sidecar_protocol;

use crate::{atomic_file::write_file_atomically, profile::SubagentProfile};
use capability_policy::{
    SubagentPromptSkillPolicySidecar, SubagentToolExposureSidecar, SubagentToolPolicySidecar,
    normalize_prompt_skills, normalize_tool_exposure,
};
use directory_scan::{catalog_directory_entry, scannable_catalog_directory};
pub use path_rules::display_subagent_category_id;
use path_rules::{
    SUBAGENT_DEFINITION_EXTENSION, YAML_EXTENSION, canonical_id_for_file,
    category_id_for_directory, definition_file_stem, directory_is_top_level_category,
    package_id_for_directory, path_has_markdown_extension, safe_relative_agent_path,
    safe_relative_directory_path,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sidecar_protocol::{
    SUBAGENT_SIDECAR_SCHEMA_VERSION, SubagentCategoryLocaleSidecar, SubagentCategorySidecar,
    SubagentDefinitionLocaleSidecar, SubagentDefinitionSidecar, SubagentOriginSidecar,
    collect_sidecar_files, find_category_sidecar, find_definition_sidecar, read_category_sidecar,
    read_definition_sidecar, write_yaml_sidecar_atomically,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use vulcan_session_core::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, ContextCompactionPolicyRequest,
    ExecutionRuntimePolicyRequest, PromptSkillSnapshot, ProviderModelSnapshot,
    ToolExposureSnapshot, apply_prompt_skill_capability_policy,
};

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

/// Default agent emoji used when no metadata supplies one.
/// 没有元数据时使用的默认智能体图标。
const DEFAULT_AGENT_EMOJI: &str = "🤖";

/// Default agent color used when no metadata supplies one.
/// 没有元数据时使用的默认智能体颜色。
const DEFAULT_AGENT_COLOR: &str = "gray";

/// Stable host-owned category identifier for plugin-contributed agents.
/// 插件贡献智能体使用的稳定宿主分类标识。
pub const PLUGIN_SUBAGENT_CATEGORY_ID: &str = "plugins";

pub use vulcan_session_core::task_config::{
    SUBAGENT_MODE_DEFAULT, SUBAGENT_MODE_EXPERT, SUBAGENT_MODE_MIRROR,
};

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
    /// Read-only definition contributed by an enabled Vulcan plugin.
    /// 由已启用 Vulcan 插件贡献的只读定义。
    Plugin,
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

impl SubagentDefinitionSource {
    /// Returns the owning plugin identifier for a plugin definition.
    /// 返回插件定义的所属插件标识。
    ///
    /// # Returns
    /// 返回值
    /// The stable plugin identifier, or `None` for non-plugin definitions.
    /// 稳定插件标识；非插件定义返回 `None`。
    pub fn plugin_id(&self) -> Option<&str> {
        (self.scope == SubagentDefinitionScope::Plugin)
            .then_some(self.package_id.as_deref())
            .flatten()
    }
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
    /// Exact system definition ids captured during the same catalog load generation.
    /// 在同一次目录加载代次中捕获的精确系统定义标识集合。
    #[serde(default)]
    pub system_definition_ids: BTreeSet<String>,
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
            // Plugin agents are callable only through their canonical pluginId:localName identity.
            // 插件智能体只能通过其规范 pluginId:localName 标识调用。
            .filter(|definition| definition.source.scope != SubagentDefinitionScope::Plugin)
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

    /// Merges validated plugin agents into this effective catalog snapshot.
    /// 将已验证的插件智能体合并到当前生效目录快照。
    ///
    /// # Parameters
    /// 参数
    /// `definitions` contains namespaced plugin records from one immutable plugin catalog.
    /// `definitions` 包含来自同一不可变插件目录的命名空间插件记录。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit, or a deterministic scope, category, or identifier conflict diagnostic.
    /// 返回空值，或确定性的作用域、分类及标识冲突诊断。
    pub fn merge_plugin_definitions(
        &mut self,
        definitions: impl IntoIterator<Item = SubagentDefinition>,
    ) -> Result<(), String> {
        let definitions = definitions.into_iter().collect::<Vec<_>>();
        let mut known_ids = self
            .definitions
            .iter()
            .map(|definition| definition.id.clone())
            .collect::<BTreeSet<_>>();
        for definition in &definitions {
            if definition.source.scope != SubagentDefinitionScope::Plugin {
                return Err(format!(
                    "Subagent `{}` cannot enter the plugin catalog without plugin scope.",
                    definition.id
                ));
            }
            if definition.source.category_id.as_deref() != Some(PLUGIN_SUBAGENT_CATEGORY_ID) {
                return Err(format!(
                    "Plugin subagent `{}` must belong to category `{PLUGIN_SUBAGENT_CATEGORY_ID}`.",
                    definition.id
                ));
            }
            let plugin_id = definition.source.plugin_id().ok_or_else(|| {
                format!(
                    "Plugin subagent `{}` must retain its owning plugin id.",
                    definition.id
                )
            })?;
            let expected_prefix = format!("{plugin_id}:");
            if definition
                .id
                .strip_prefix(&expected_prefix)
                .is_none_or(str::is_empty)
            {
                return Err(format!(
                    "Plugin subagent `{}` must use its owning plugin namespace `{plugin_id}`.",
                    definition.id
                ));
            }
            if !definition.enabled {
                return Err(format!(
                    "Plugin subagent `{}` cannot be disabled independently.",
                    definition.id
                ));
            }
            if definition.prompt_skills.policy.mode != CapabilityPolicyMode::AllowOnly {
                return Err(format!(
                    "Plugin subagent `{}` must use an allow-only prompt skill policy.",
                    definition.id
                ));
            }
            if let Some(foreign_selector) =
                definition
                    .prompt_skills
                    .policy
                    .selectors
                    .iter()
                    .find(|selector| {
                        selector
                            .strip_prefix(&expected_prefix)
                            .is_none_or(|local_name| {
                                local_name.is_empty() || local_name.contains(':')
                            })
                    })
            {
                return Err(format!(
                    "Plugin subagent `{}` references foreign or invalid skill `{foreign_selector}`.",
                    definition.id
                ));
            }
            if !known_ids.insert(definition.id.clone()) {
                return Err(format!(
                    "Plugin subagent identifier `{}` conflicts with an existing catalog record.",
                    definition.id
                ));
            }
        }
        // Commit the category and definitions only after the whole batch validates.
        // 仅在整个批次校验通过后提交分类与定义。
        if !self
            .categories
            .iter()
            .any(|category| category.id == PLUGIN_SUBAGENT_CATEGORY_ID)
        {
            self.categories.push(plugin_subagent_category());
        }
        self.definitions.extend(definitions);
        self.definitions
            .sort_by(|left, right| left.id.cmp(&right.id));
        self.categories.sort_by(|left, right| {
            left.order
                .unwrap_or(i32::MAX)
                .cmp(&right.order.unwrap_or(i32::MAX))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(())
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
        system_definition_ids: BTreeSet::new(),
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
    // systemDefinitionIds freezes the exact system layer before user packages or overrides mutate the merged map.
    // systemDefinitionIds 在用户包或覆盖修改合并映射前冻结精确系统层。
    catalog.system_definition_ids = by_id.keys().cloned().collect();

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
    apply_user_overrides(
        &user_root.join(USER_OVERRIDES_DIR_NAME),
        &mut by_id,
        &mut catalog.errors,
    );
    catalog.definitions = by_id.into_values().collect();
    catalog.categories = categories.into_values().collect();
    catalog
}

/// Loads every read-only subagent definition contributed by one plugin package.
/// 加载单个插件包贡献的全部只读子代理定义。
///
/// # Parameters
/// 参数
/// - `plugin_id`: Stable owning plugin identifier.
/// - `plugin_id`：稳定的所属插件标识。
/// - `agents_root`: Conventional plugin `agents/` directory.
/// - `agents_root`：插件约定的 `agents/` 目录。
///
/// # Returns
/// 返回值
/// A plugin-scoped catalog whose definitions use namespaced identifiers and one host category.
/// 返回使用命名空间标识及唯一宿主分类的插件作用域目录。
pub fn load_plugin_subagent_catalog(plugin_id: &str, agents_root: PathBuf) -> SubagentCatalog {
    let mut by_id = BTreeMap::<String, SubagentDefinition>::new();
    let mut ignored_categories = BTreeMap::<String, SubagentCategoryDefinition>::new();
    let mut catalog = SubagentCatalog {
        user_root: PathBuf::new(),
        system_root: agents_root.clone(),
        definitions: Vec::new(),
        system_definition_ids: BTreeSet::new(),
        categories: vec![plugin_subagent_category()],
        errors: Vec::new(),
    };
    scan_subagent_definition_root(
        &agents_root,
        &agents_root,
        SubagentDefinitionScope::Plugin,
        Some(plugin_id),
        &mut by_id,
        &mut ignored_categories,
        &mut catalog.errors,
    );
    catalog.definitions = by_id.into_values().collect();
    catalog
}

/// Builds the immutable host category used by every plugin-contributed agent.
/// 构建所有插件贡献智能体共用的不可变宿主分类。
///
/// # Returns
/// 返回值
/// The bilingual plugin category definition.
/// 双语插件分类定义。
pub fn plugin_subagent_category() -> SubagentCategoryDefinition {
    SubagentCategoryDefinition {
        id: String::from(PLUGIN_SUBAGENT_CATEGORY_ID),
        name: String::from("Plugins"),
        description: None,
        locales: BTreeMap::from([
            (
                String::from("en-US"),
                SubagentCategoryLocaleDefinition {
                    name: Some(String::from("Plugins")),
                    description: None,
                },
            ),
            (
                String::from("zh-CN"),
                SubagentCategoryLocaleDefinition {
                    name: Some(String::from("Plugins")),
                    description: None,
                },
            ),
        ]),
        emoji: String::from("🧩"),
        color: String::from("#6366F1"),
        order: Some(110),
        file: None,
    }
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
    let raw = read_subagent_definition_text(definition_file)?;
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
    let explicit_plugin_skills = parsed.skills.clone();
    let explicit_prompt_skill_policy =
        parsed.prompt_skills.is_some() || sidecar.prompt_skill_policy.is_some();
    if explicit_plugin_skills.is_some() && scope != SubagentDefinitionScope::Plugin {
        return Err(SubagentCatalogLoadError {
            path: definition_file.to_path_buf(),
            message: String::from(
                "Subagent frontmatter field `skills` is reserved for plugin-contributed definitions.",
            ),
        });
    }
    if explicit_plugin_skills.is_some() && explicit_prompt_skill_policy {
        return Err(SubagentCatalogLoadError {
            path: definition_file.to_path_buf(),
            message: String::from(
                "Plugin subagent fields `skills` and `promptSkills` cannot be declared together.",
            ),
        });
    }
    let declared_package_id = non_empty_owned(sidecar.package_id.clone());
    if scope == SubagentDefinitionScope::Plugin {
        let owning_plugin_id = package_id
            .as_deref()
            .ok_or_else(|| SubagentCatalogLoadError {
                path: definition_file.to_path_buf(),
                message: String::from(
                    "Plugin subagent definition is missing its owning plugin id.",
                ),
            })?;
        if declared_package_id
            .as_deref()
            .is_some_and(|declared| declared != owning_plugin_id)
        {
            return Err(SubagentCatalogLoadError {
                path: definition_file.to_path_buf(),
                message: format!(
                    "Plugin subagent packageId must match its owning plugin `{owning_plugin_id}`."
                ),
            });
        }
    }
    // Plugin ownership is supplied by the trusted package loader and cannot be replaced by sidecar metadata.
    // 插件所有权由可信包加载器提供，不能被 sidecar 元数据替换。
    let effective_package_id = if scope == SubagentDefinitionScope::Plugin {
        package_id
    } else {
        declared_package_id.or(package_id)
    };
    let declared_enabled = sidecar.enabled.or(parsed.enabled);
    if scope == SubagentDefinitionScope::Plugin && declared_enabled == Some(false) {
        return Err(SubagentCatalogLoadError {
            path: definition_file.to_path_buf(),
            message: String::from(
                "Plugin subagent enabled state is owned by the plugin and cannot be disabled individually.",
            ),
        });
    }
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
    let mut tool_exposure = if sidecar.tool_exposure.has_override() {
        sidecar.tool_exposure.into_snapshot()
    } else {
        parsed.tool_exposure
    };
    if let Some(tool_policy) = sidecar.tool_policy {
        tool_exposure.policy = tool_policy.into_snapshot();
    }
    normalize_tool_exposure(&mut tool_exposure);
    let mut prompt_skills = parsed.prompt_skills.unwrap_or_default();
    if let Some(prompt_skill_policy) = sidecar.prompt_skill_policy {
        prompt_skills.policy = prompt_skill_policy.into_snapshot();
    }
    normalize_prompt_skills(&mut prompt_skills);
    if scope == SubagentDefinitionScope::Plugin {
        let plugin_id =
            effective_package_id
                .as_deref()
                .ok_or_else(|| SubagentCatalogLoadError {
                    path: definition_file.to_path_buf(),
                    message: String::from(
                        "Plugin subagent definition is missing its owning plugin id.",
                    ),
                })?;
        let selectors = match explicit_plugin_skills {
            Some(local_skill_names) => {
                let mut unique_local_names = BTreeSet::new();
                if local_skill_names.is_empty() {
                    return Err(SubagentCatalogLoadError {
                        path: definition_file.to_path_buf(),
                        message: String::from(
                            "Plugin subagent field `skills` must not be empty when declared.",
                        ),
                    });
                }
                let mut selectors = Vec::new();
                for local_name in local_skill_names {
                    if local_name != local_name.trim()
                        || local_name.is_empty()
                        || local_name.contains(':')
                        || local_name.contains('/')
                        || local_name.contains('\\')
                    {
                        return Err(SubagentCatalogLoadError {
                            path: definition_file.to_path_buf(),
                            message: format!(
                                "Plugin subagent skill reference `{local_name}` must be one canonical package-local skill name."
                            ),
                        });
                    }
                    if !unique_local_names.insert(local_name.clone()) {
                        return Err(SubagentCatalogLoadError {
                            path: definition_file.to_path_buf(),
                            message: format!(
                                "Plugin subagent skill reference `{local_name}` is declared more than once."
                            ),
                        });
                    }
                    selectors.push(format!("{plugin_id}:{local_name}"));
                }
                selectors
            }
            None if explicit_prompt_skill_policy => {
                if prompt_skills.policy.mode != CapabilityPolicyMode::AllowOnly {
                    return Err(SubagentCatalogLoadError {
                        path: definition_file.to_path_buf(),
                        message: String::from(
                            "Plugin subagent promptSkills policy must use allow_only mode.",
                        ),
                    });
                }
                let expected_prefix = format!("{plugin_id}:");
                for selector in &prompt_skills.policy.selectors {
                    let local_name = selector.strip_prefix(&expected_prefix).ok_or_else(|| {
                        SubagentCatalogLoadError {
                            path: definition_file.to_path_buf(),
                            message: format!(
                                "Plugin subagent skill selector `{selector}` must belong to plugin `{plugin_id}`."
                            ),
                        }
                    })?;
                    if local_name.is_empty() || local_name.contains(':') {
                        return Err(SubagentCatalogLoadError {
                            path: definition_file.to_path_buf(),
                            message: format!(
                                "Plugin subagent skill selector `{selector}` is not a canonical same-plugin skill id."
                            ),
                        });
                    }
                }
                prompt_skills.policy.selectors.clone()
            }
            None => Vec::new(),
        };
        apply_prompt_skill_capability_policy(
            &mut prompt_skills,
            CapabilityPolicySnapshot {
                mode: CapabilityPolicyMode::AllowOnly,
                selectors,
            },
        );
    }
    let system_prompt = parsed
        .system_prompt
        .and_then(non_empty_string)
        .or_else(|| non_empty_text(body).map(str::to_string));
    let source_key = source_key_for_definition(scope, effective_package_id.as_deref(), &id);
    Ok(SubagentDefinition {
        id,
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
        enabled: if scope == SubagentDefinitionScope::Plugin {
            true
        } else {
            declared_enabled.unwrap_or(true)
        },
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

/// Atomically writes one definition's user-owned Markdown and YAML sidecar as complete files.
/// 将单个定义的用户 Markdown 与 YAML sidecar 分别作为完整文件原子写入。
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
    ensure_non_plugin_definition_avoids_reserved_category(
        &path,
        SubagentDefinitionScope::User,
        &definition.id,
        definition.source.package_id.as_deref(),
    )?;
    let body = subagent_definition_to_markdown(definition)?;
    write_catalog_document_atomically(&path, body.as_bytes(), "user subagent definition")?;
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

/// Atomically writes one user-owned override sidecar without copying the prompt Markdown.
/// 原子写入一个用户拥有的覆盖 sidecar，而不复制提示词 Markdown。
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

/// Atomically writes one system-backed metadata override without copying the prompt Markdown.
/// 原子写入一个系统智能体元数据覆盖，而不复制提示词 Markdown。
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
    let sidecar = SubagentDefinitionSidecar {
        schema_version: SUBAGENT_SIDECAR_SCHEMA_VERSION,
        enabled: Some(definition.enabled),
        system_prompt_append: definition.system_prompt_append.clone(),
        tool_exposure: SubagentToolExposureSidecar::from_snapshot(&definition.tool_exposure),
        tool_policy: SubagentToolPolicySidecar::from_snapshot(&definition.tool_exposure.policy),
        prompt_skill_policy: SubagentPromptSkillPolicySidecar::from_snapshot(
            &definition.prompt_skills.policy,
        ),
        ..SubagentDefinitionSidecar::default()
    };
    write_yaml_sidecar_atomically(&path, &sidecar, "subagent metadata override")
}

/// Atomically writes one minimal user-owned enabled-state override sidecar.
/// 原子写入一个只包含启停状态的用户拥有覆盖 sidecar。
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
    let sidecar = SubagentDefinitionSidecar {
        schema_version: SUBAGENT_SIDECAR_SCHEMA_VERSION,
        enabled: Some(enabled),
        ..SubagentDefinitionSidecar::default()
    };
    write_yaml_sidecar_atomically(&path, &sidecar, "subagent enabled override")
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

/// Publishes one complete catalog document through the crate-wide atomic file writer.
/// 通过 crate 级原子文件写入器发布一个完整目录文档。
///
/// # Parameters
/// 参数
/// `path` is the final catalog document path.
/// `path` 是最终目录文档路径。
/// `contents` contains the complete next file image.
/// `contents` 包含下一版本文件的完整内容。
/// `artifact_kind` identifies the document in persistence diagnostics.
/// `artifact_kind` 在持久化诊断中标识文档。
///
/// # Returns
/// 返回值
/// `Ok(())` after atomic publication, otherwise a path-aware catalog load error.
/// 原子发布完成后返回 `Ok(())`，否则返回包含路径的目录加载错误。
fn write_catalog_document_atomically(
    path: &Path,
    contents: &[u8],
    artifact_kind: &str,
) -> Result<(), SubagentCatalogLoadError> {
    write_file_atomically(path, contents, artifact_kind).map_err(|message| {
        SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message,
        }
    })
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
    if !scannable_catalog_directory(root, "Subagent catalog root", errors) {
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
    for entry_result in entries {
        // entry follows the shared observable-error policy before any path classification.
        // entry 在任何路径分类前遵循共享的可观察错误策略。
        let Some((entry, file_type)) =
            catalog_directory_entry(entry_result, root, "subagent catalog", errors)
        else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            if scope != SubagentDefinitionScope::Plugin {
                register_category_if_needed(&path, base_root, package_id, categories, errors);
            }
            scan_subagent_definition_root(
                &path, base_root, scope, package_id, by_id, categories, errors,
            );
            continue;
        }
        if !file_type.is_file()
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
    for entry_result in entries {
        // entry preserves category traversal failures while allowing readable siblings to continue.
        // entry 保留分类遍历失败，同时允许可读同级项继续处理。
        let Some((entry, file_type)) =
            catalog_directory_entry(entry_result, root, "system subagent category", errors)
        else {
            continue;
        };
        let path = entry.path();
        if !file_type.is_dir() {
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
    if !scannable_catalog_directory(package_root, "Subagent package root", errors) {
        return;
    }
    let entries = match fs::read_dir(package_root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(SubagentCatalogLoadError {
                path: package_root.to_path_buf(),
                message: format!("Failed to read subagent package root: {error}"),
            });
            return;
        }
    };
    for entry_result in entries {
        // entry preserves package traversal failures while allowing readable packages to continue.
        // entry 保留包遍历失败，同时允许可读包继续处理。
        let Some((entry, file_type)) =
            catalog_directory_entry(entry_result, package_root, "subagent package", errors)
        else {
            continue;
        };
        let package_dir = entry.path();
        if !file_type.is_dir() {
            continue;
        }
        // Resolves a portable package id before it becomes part of every definition id.
        // 在包标识成为所有定义标识的一部分前解析其可移植形式。
        let Some(package_id) = package_id_for_directory(&package_dir) else {
            errors.push(SubagentCatalogLoadError {
                path: package_dir,
                message: String::from("Subagent package directory must have a portable id."),
            });
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
    if !scannable_catalog_directory(overrides_root, "Subagent override root", errors) {
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
    let raw = read_subagent_definition_text(definition_file)?;
    let relative_id = canonical_id_for_file(base_root, definition_file).ok_or_else(|| {
        SubagentCatalogLoadError {
            path: definition_file.to_path_buf(),
            message: String::from("Subagent definition file must have a valid relative path."),
        }
    })?;
    let id = match (scope, package_id) {
        (SubagentDefinitionScope::Plugin, Some(plugin_id)) => {
            format!("{plugin_id}:{relative_id}")
        }
        (_, Some(package_id)) => format!("{package_id}/{relative_id}"),
        (_, None) => relative_id,
    };
    let category_id = if scope == SubagentDefinitionScope::Plugin {
        Some(String::from(PLUGIN_SUBAGENT_CATEGORY_ID))
    } else {
        display_subagent_category_id(&id, package_id)
    };
    ensure_non_plugin_definition_avoids_reserved_category(definition_file, scope, &id, package_id)?;
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

/// Reads one complete user-authored subagent Markdown definition as UTF-8 or GB18030.
/// 将一个完整的用户子智能体 Markdown 定义读取为 UTF-8 或 GB18030。
///
/// Parameters: `definition_file` identifies the exact Markdown source.
/// Returns: normalized Unicode text or a path-aware catalog loading error.
/// 参数：`definition_file` 标识精确的 Markdown 来源。
/// 返回值：规范化后的 Unicode 文本，或包含路径信息的目录加载错误。
fn read_subagent_definition_text(
    definition_file: &Path,
) -> Result<String, SubagentCatalogLoadError> {
    let bytes = fs::read(definition_file).map_err(|error| SubagentCatalogLoadError {
        path: definition_file.to_path_buf(),
        message: format!("Failed to read subagent definition: {error}"),
    })?;
    vulcan_runtime_assets::text_decode::decode_complete_user_text(&bytes)
        .map(|decoded| decoded.content)
        .map_err(|error| SubagentCatalogLoadError {
            path: definition_file.to_path_buf(),
            message: format!("Failed to decode subagent definition as UTF-8 or GB18030: {error}"),
        })
}

/// Rejects non-plugin definitions that attempt to occupy the host-owned plugin category.
/// 拒绝尝试占用宿主拥有插件分类的非插件定义。
///
/// # Parameters
/// 参数
/// `path` identifies the rejected definition in diagnostics.
/// `path` 在诊断中标识被拒绝的定义。
/// `scope` is the catalog scope supplying the definition.
/// `scope` 是提供定义的目录作用域。
/// `id` is the canonical definition identifier.
/// `id` 是规范定义标识。
/// `package_id` identifies an optional user package namespace.
/// `package_id` 标识可选的用户包命名空间。
///
/// # Returns
/// 返回值
/// Unit for allowed categories, or a structured reserved-category error.
/// 分类允许时返回空值，否则返回结构化保留分类错误。
fn ensure_non_plugin_definition_avoids_reserved_category(
    path: &Path,
    scope: SubagentDefinitionScope,
    id: &str,
    package_id: Option<&str>,
) -> Result<(), SubagentCatalogLoadError> {
    if scope != SubagentDefinitionScope::Plugin
        && display_subagent_category_id(id, package_id).as_deref()
            == Some(PLUGIN_SUBAGENT_CATEGORY_ID)
    {
        return Err(SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!(
                "Subagent category `{PLUGIN_SUBAGENT_CATEGORY_ID}` is reserved for plugin-contributed definitions."
            ),
        });
    }
    Ok(())
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
    if id == PLUGIN_SUBAGENT_CATEGORY_ID {
        errors.push(SubagentCatalogLoadError {
            path: directory.to_path_buf(),
            message: format!(
                "Subagent category `{PLUGIN_SUBAGENT_CATEGORY_ID}` is reserved for plugin-contributed definitions."
            ),
        });
        return;
    }
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
    let sidecar = SubagentDefinitionSidecar {
        schema_version: SUBAGENT_SIDECAR_SCHEMA_VERSION,
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
        tool_exposure: SubagentToolExposureSidecar::from_snapshot(&definition.tool_exposure),
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
    };
    write_yaml_sidecar_atomically(path, &sidecar, "subagent sidecar")
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
    if sidecar.tool_exposure.has_override() {
        definition.tool_exposure = sidecar.tool_exposure.into_snapshot();
    }
    if let Some(tool_policy) = sidecar.tool_policy {
        definition.tool_exposure.policy = tool_policy.into_snapshot();
    }
    if let Some(prompt_skill_policy) = sidecar.prompt_skill_policy {
        definition.prompt_skills.policy = prompt_skill_policy.into_snapshot();
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
        SubagentDefinitionScope::Plugin => format!("plugin:{id}"),
    }
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
    /// Optional prompt skill override snapshot whose presence is preserved for conflict validation.
    /// 保留显式存在信息以供冲突校验的可选提示词技能覆盖快照。
    #[serde(default)]
    prompt_skills: Option<PromptSkillSnapshot>,
    /// Optional package-local skill names attached by a plugin definition.
    /// 插件定义附加的可选包内技能名称。
    #[serde(default)]
    skills: Option<Vec<String>>,
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

    /// Verifies a GBK subagent Markdown definition is decoded before frontmatter parsing.
    /// 验证 GBK 子智能体 Markdown 定义会在解析前置元数据前完成解码。
    #[test]
    fn parses_gbk_subagent_definition_file() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-gbk-definition-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("GBK subagent root should create");
        let path = root.join("gbk-reviewer.md");
        let source = "---\nname: 中文审查员\n---\n检查服务端用户等级规则。";
        let (encoded, _, had_errors) = encoding_rs::GBK.encode(source);
        assert!(!had_errors);
        fs::write(&path, encoded.as_ref()).expect("GBK subagent definition should write");

        let definition = parse_subagent_definition_file(&path, SubagentDefinitionScope::User)
            .expect("GBK subagent definition should parse");

        assert_eq!(definition.id, "gbk-reviewer");
        assert_eq!(definition.name, "中文审查员");
        assert_eq!(
            definition.system_prompt.as_deref(),
            Some("检查服务端用户等级规则。")
        );
        fs::remove_dir_all(root).expect("GBK subagent root should be removable");
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

    /// Verifies structured user definitions cannot replace system definitions by string id.
    /// 验证结构化用户定义不能按字符串标识替换系统定义。
    #[test]
    fn structured_user_definition_conflict_preserves_system_definition_by_id() {
        // Owns the isolated filesystem fixture for the catalog loading scenario.
        // 持有目录加载场景使用的隔离文件系统夹具。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-catalog-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        // Represents the canonical system definition root consumed by the loader.
        // 表示加载器读取的规范系统定义根目录。
        let system_root = root.join("system");
        // Represents the user catalog root that owns definitions, packages, and overrides.
        // 表示拥有 definitions、packages 与 overrides 的用户目录根。
        let user_root = root.join("user");
        fs::create_dir_all(&system_root).expect("system root");
        fs::create_dir_all(user_root.join("definitions")).expect("user definitions root");
        fs::write(
            system_root.join("reviewer.md"),
            "---\nname: System Reviewer\n---\nSystem.",
        )
        .expect("system file");
        fs::write(
            user_root.join("definitions").join("reviewer.md"),
            "---\nname: User Reviewer\nenabled: false\n---\nUser.",
        )
        .expect("user file");

        // Captures the merged catalog produced from only the canonical directory roots.
        // 捕获仅从规范目录根合并得到的目录结果。
        let catalog = load_subagent_catalog_from_roots(system_root, user_root);
        // Resolves the surviving system definition after rejecting the conflicting user entry.
        // 解析拒绝冲突用户条目后保留下来的系统定义。
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

    /// Verifies Markdown files outside structured user directories are not catalog inputs.
    /// 验证结构化用户目录之外的 Markdown 文件不会成为目录输入。
    #[test]
    fn ignores_markdown_files_outside_structured_user_directories() {
        // Owns the isolated filesystem fixture for the root layout contract.
        // 持有根目录布局契约使用的隔离文件系统夹具。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-root-layout-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        // Represents the user catalog root while intentionally omitting structured definitions.
        // 表示用户目录根，同时刻意不创建结构化 definitions 目录。
        let user_root = root.join("user");
        fs::create_dir_all(&user_root).expect("user root");
        fs::write(
            user_root.join("notes.md"),
            "---\nname: Root Notes\n---\nThis file is not an agent definition.",
        )
        .expect("user root notes");

        // Captures the catalog result when only an unrelated root Markdown file exists.
        // 捕获仅存在无关根目录 Markdown 文件时的目录结果。
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);

        assert!(catalog.definitions.is_empty());
        assert!(catalog.errors.is_empty());

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
            "schemaVersion: 1\nname: Sidecar Reviewer\nemoji: S\ncolor: blue\nsource:\n  kind: github\n  label: Example/reviewer\n  icon: github\n  url: https://example.test/reviewer\nlocales:\n  zh-CN:\n    name: 旁车审核专家\n    description: 本地化说明。\ntoolPolicy:\n  mode: allow_only\n  tools:\n    - shell_command\n",
        )
        .expect("sidecar file");
        fs::write(
            engineering_root.join("_category.yaml"),
            "schemaVersion: 1\nname: Engineering\nemoji: E\ncolor: blue\nlocales:\n  zh-CN:\n    name: 工程\n    description: 工程专家。\n",
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

    /// Verifies modern deny-only sidecar policies drop parent-owned child-agent tools.
    /// 验证现代仅禁用 sidecar 策略会移除父会话拥有的智能体禁用工具。
    #[test]
    fn sidecar_deny_only_tool_policy_drops_child_forbidden_tools() {
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
            "schemaVersion: 1\ntoolPolicy:\n  mode: deny_only\n  tools:\n    - spawn_agent\n",
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

    /// Verifies family-specific overrides survive a sidecar roundtrip without becoming global policy.
    /// 验证分族覆盖在 sidecar 往返后保持原作用域且不会转成全局策略。
    #[test]
    fn sidecar_roundtrip_keeps_family_overrides_out_of_global_tool_policy() {
        // Owns the isolated filesystem fixture for the sidecar roundtrip.
        // 持有 sidecar 往返场景使用的隔离文件系统夹具。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-family-policy-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        // Represents the canonical user catalog root used by the writer and loader.
        // 表示写入器与加载器共同使用的规范用户目录根。
        let user_root = root.join("user");
        // Carries only family-specific selectors and no unified global tool policy.
        // 仅携带分族选择器，不包含统一全局工具策略。
        let mut definition = parse_subagent_definition_text(
            &PathBuf::from("family-reviewer.md"),
            "---\nname: Family Reviewer\n---\nReview family tools.",
            SubagentDefinitionScope::User,
        )
        .expect("definition should parse");
        definition.tool_exposure.luaskills_overrides =
            serde_json::json!({"allow": ["vulcan-codekit"]});
        definition.tool_exposure.mcp_server_overrides =
            serde_json::json!({"allow": ["mcp__example__inspect"]});
        write_user_subagent_definition(&user_root, &definition).expect("definition should write");

        // Captures the catalog reloaded from the writer's current sidecar format.
        // 捕获从写入器当前 sidecar 格式重新加载的目录。
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);
        // Resolves the reloaded definition for policy and family override assertions.
        // 解析重新加载的定义以断言策略与分族覆盖。
        let parsed = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "family-reviewer")
            .expect("written definition should load");

        assert_eq!(parsed.tool_exposure.policy.mode, CapabilityPolicyMode::All);
        assert!(parsed.tool_exposure.policy.selectors.is_empty());
        assert!(parsed.tool_exposure.allowed_tools.is_empty());
        assert!(parsed.tool_exposure.disabled_tools.is_empty());
        assert_eq!(
            parsed.tool_exposure.luaskills_overrides,
            serde_json::json!({"allow": ["vulcan-codekit"]})
        );
        assert_eq!(
            parsed.tool_exposure.mcp_server_overrides,
            serde_json::json!({"allow": ["mcp__example__inspect"]})
        );

        fs::remove_dir_all(root).expect("test root should be removable");
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
            "schemaVersion: 1\nsource:\n  kind: github\n  label: Example/reviewer\n  icon: github\n  url: https://example.test/reviewer\nlocales:\n  zh-CN:\n    name: 系统审核专家\n    description: 系统本地化说明。\n",
        )
        .expect("system sidecar");
        fs::write(
            user_root
                .join("overrides")
                .join("engineering")
                .join("reviewer.yaml"),
            "schemaVersion: 1\nenabled: false\n",
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

    /// Verifies plugin agents require their complete namespaced identifier even when the local id is unique.
    /// 验证即使本地标识唯一，插件智能体仍必须使用完整命名空间标识。
    #[test]
    fn enabled_definition_requires_exact_plugin_agent_id() {
        // root owns the isolated conventional plugin agents directory.
        // root 持有隔离的约定式插件智能体目录。
        let root = std::env::temp_dir().join(format!(
            "vulcan-plugin-subagent-exact-id-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("plugin agents root");
        fs::write(
            root.join("reviewer.md"),
            "---\nname: Plugin Reviewer\n---\nReview plugin changes.",
        )
        .expect("plugin agent definition");
        // catalog applies the trusted package namespace before any runtime lookup.
        // catalog 在任何运行时查询前应用可信包命名空间。
        let catalog = load_plugin_subagent_catalog("content-plugin", root.clone());

        assert!(catalog.enabled_definition("reviewer").is_none());
        assert!(
            catalog
                .enabled_definition("content-plugin:reviewer")
                .is_some()
        );

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
            "schemaVersion: 1\nname: Game Development\nemoji: G\ncolor: purple\n",
        )
        .expect("top category sidecar");
        fs::write(
            nested_root.join("_category.yaml"),
            "schemaVersion: 1\nname: Blender\nemoji: B\ncolor: orange\n",
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

    /// Verifies non-portable package directories are reported and never produce definitions.
    /// 验证不可移植的包目录会被上报且不会产出定义。
    #[test]
    fn invalid_package_directory_is_reported_and_skipped() {
        // Owns the isolated filesystem fixture for invalid package loading.
        // 持有无效包加载场景使用的隔离文件系统夹具。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-invalid-package-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        // Represents the canonical user catalog root.
        // 表示规范用户目录根。
        let user_root = root.join("user");
        // Uses a Windows-reserved package name that is not portable.
        // 使用不可移植的 Windows 保留包名称。
        let package_root = user_root.join("packages").join("CON");
        fs::create_dir_all(&package_root).expect("invalid package root");
        fs::write(package_root.join("reviewer.md"), "Review.").expect("package definition");

        // Captures the complete catalog result including load issues.
        // 捕获包含加载问题的完整目录结果。
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);
        // Resolves the explicit package-directory validation issue.
        // 解析明确的包目录校验问题。
        let issue = catalog
            .errors
            .iter()
            .find(|error| error.path == package_root)
            .expect("invalid package directory should be reported");

        assert!(catalog.definitions.is_empty());
        assert_eq!(
            issue.message,
            "Subagent package directory must have a portable id."
        );

        fs::remove_dir_all(root).expect("test root should be removable");
    }

    /// Verifies existing package and override files are reported instead of treated as absent roots.
    /// 验证已存在的包文件与覆盖文件会被上报，而不会被当作缺失根目录。
    #[test]
    fn non_directory_package_and_override_roots_are_reported() {
        // root owns the isolated user catalog with deliberately invalid root entry types.
        // root 持有包含刻意无效根目录项类型的隔离用户目录。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-non-directory-roots-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        // user_root is the canonical catalog root passed to the production loader.
        // user_root 是传给生产加载器的规范目录根。
        let user_root = root.join("user");
        fs::create_dir_all(&user_root).expect("user catalog root");
        // package_root is a file where package discovery requires a directory.
        // package_root 是包发现要求目录位置上的普通文件。
        let package_root = user_root.join(USER_PACKAGES_DIR_NAME);
        fs::write(&package_root, b"not a package directory").expect("package root file");
        // overrides_root is a file where override discovery requires a directory.
        // overrides_root 是覆盖发现要求目录位置上的普通文件。
        let overrides_root = user_root.join(USER_OVERRIDES_DIR_NAME);
        fs::write(&overrides_root, b"not an override directory").expect("override root file");

        // catalog captures both type errors through the complete production scan sequence.
        // catalog 通过完整生产扫描顺序捕获两个类型错误。
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);
        // package_error identifies the invalid package root without conflating it with absence.
        // package_error 标识无效包根，而不会将其与路径缺失混淆。
        let package_error = catalog
            .errors
            .iter()
            .find(|error| error.path == package_root)
            .expect("package root type error should be reported");
        // override_error identifies the invalid override root through the same type policy.
        // override_error 通过相同类型策略标识无效覆盖根。
        let override_error = catalog
            .errors
            .iter()
            .find(|error| error.path == overrides_root)
            .expect("override root type error should be reported");

        assert_eq!(
            package_error.message,
            "Subagent package root exists but is not a directory."
        );
        assert_eq!(
            override_error.message,
            "Subagent override root exists but is not a directory."
        );

        fs::remove_dir_all(root).expect("test root should be removable");
    }

    /// Verifies recursive catalog scanning rejects a directory link back to an ancestor.
    /// 验证递归目录扫描会拒绝指回祖先目录的目录链接。
    #[cfg(unix)]
    #[test]
    fn recursive_catalog_scan_rejects_symbolic_link_cycle() {
        use std::os::unix::fs::symlink;

        // root owns a real catalog plus one link that would recurse back to the system root.
        // root 持有真实目录以及一个会递归指回系统根的链接。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-symlink-cycle-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        // system_root is the trusted root supplied explicitly by the caller.
        // system_root 是调用方显式提供的受信任根。
        let system_root = root.join("system");
        // category_root contains one valid definition and the malicious recursive link.
        // category_root 包含一个有效定义和恶意递归链接。
        let category_root = system_root.join("engineering");
        fs::create_dir_all(&category_root).expect("category root");
        fs::write(category_root.join("reviewer.md"), "Review.").expect("definition file");
        // loop_path points to the ancestor system root and would recurse forever if followed.
        // loop_path 指向祖先系统根；若跟随该链接将无限递归。
        let loop_path = category_root.join("loop");
        symlink(&system_root, &loop_path).expect("recursive directory symlink");

        // catalog must retain the valid sibling while reporting and skipping the link.
        // catalog 必须保留有效同级定义，同时上报并跳过该链接。
        let catalog = load_subagent_catalog_from_roots(system_root, PathBuf::new());
        // link_error identifies the exact rejected entry.
        // link_error 标识被拒绝的精确目录项。
        let link_error = catalog
            .errors
            .iter()
            .find(|error| error.path == loop_path)
            .expect("recursive link should be reported");

        assert_eq!(catalog.definitions.len(), 1);
        assert_eq!(catalog.definitions[0].id, "engineering/reviewer");
        assert_eq!(
            link_error.message,
            "Symbolic-link subagent catalog entry is not supported."
        );

        fs::remove_file(&loop_path).expect("recursive link should be removable");
        fs::remove_dir_all(root).expect("test root should be removable");
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
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);
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

    /// Verifies the user writer cannot persist an agent into the host-owned plugin category.
    /// 验证用户写入器不能将智能体持久化到宿主拥有的插件分类。
    #[test]
    fn write_user_definition_rejects_reserved_plugin_category() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-reserved-plugin-category-write-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let user_root = root.join("user");
        let mut definition = parse_subagent_definition_text(
            Path::new("reviewer.md"),
            "---\nname: Reviewer\n---\nReview.",
            SubagentDefinitionScope::User,
        )
        .expect("definition should parse");
        definition.id = String::from("plugins/reviewer");

        let error = write_user_subagent_definition(&user_root, &definition)
            .expect_err("reserved plugin category should be rejected");
        assert!(error.message.contains("reserved for plugin-contributed"));
        assert!(!user_root.join("definitions/plugins/reviewer.md").exists());
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies direct filesystem definitions cannot enter the host-owned plugin category.
    /// 验证直接写入文件系统的定义不能进入宿主拥有的插件分类。
    #[test]
    fn user_catalog_rejects_reserved_plugin_category() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-reserved-plugin-category-load-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let user_root = root.join("user");
        let reserved_root = user_root.join("definitions/plugins");
        fs::create_dir_all(&reserved_root).expect("reserved category directory should be created");
        fs::write(
            reserved_root.join("reviewer.md"),
            "---\nname: Reviewer\n---\nReview.",
        )
        .expect("reserved category definition should be written");

        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);
        assert!(catalog.definitions.is_empty());
        assert!(catalog.categories.is_empty());
        assert!(
            catalog
                .errors
                .iter()
                .any(|error| error.message.contains("reserved for plugin-contributed"))
        );
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies current tool and prompt-skill policy objects preserve their semantics after reload.
    /// 验证当前工具与提示词技能策略对象在重新加载后保持原有语义。
    #[test]
    fn write_user_definition_preserves_current_capability_policies() {
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
        definition.prompt_skills.policy = CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::DenyOnly,
            selectors: vec![String::from("skill-creator")],
        };

        write_user_subagent_definition(&user_root, &definition).expect("definition should write");
        let sidecar_path =
            user_subagent_sidecar_path(&user_root, "empty-tools").expect("sidecar path");
        let sidecar_text =
            fs::read_to_string(sidecar_path).expect("written sidecar should be readable");
        let catalog = load_subagent_catalog_from_roots(PathBuf::new(), user_root);
        let parsed = catalog
            .definitions
            .iter()
            .find(|definition| definition.id == "empty-tools")
            .expect("written definition should load");

        assert!(sidecar_text.contains("toolPolicy:"));
        assert!(sidecar_text.contains("mode: allow_only"));
        assert!(sidecar_text.contains("promptSkillPolicy:"));
        assert!(sidecar_text.contains("mode: deny_only"));
        assert!(sidecar_text.contains("skills:"));
        assert_eq!(
            parsed.tool_exposure.policy.mode,
            CapabilityPolicyMode::AllowOnly
        );
        assert!(parsed.tool_exposure.policy.selectors.is_empty());
        assert!(parsed.tool_exposure.allowed_tools.is_empty());
        assert!(parsed.tool_exposure.disabled_tools.is_empty());
        assert_eq!(
            parsed.prompt_skills.policy.mode,
            CapabilityPolicyMode::DenyOnly
        );
        assert_eq!(
            parsed.prompt_skills.policy.selectors,
            vec![String::from("skill-creator")]
        );
        assert!(parsed.prompt_skills.enabled.is_empty());
        assert_eq!(
            parsed.prompt_skills.disabled,
            vec![String::from("skill-creator")]
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies plugin agents use namespaced identifiers, one host category, and namespaced attached skills.
    /// 验证插件智能体使用命名空间标识、唯一宿主分类及命名空间化附加技能。
    #[test]
    fn plugin_agent_loader_namespaces_attached_skills() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-plugin-agent-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let agents_root = root.join("agents");
        fs::create_dir_all(agents_root.join("engineering"))
            .expect("plugin agent directory should be created");
        fs::write(
            agents_root.join("engineering/reviewer.md"),
            "---\nname: Plugin Reviewer\nskills:\n  - review-helper\n---\nReview the requested change.",
        )
        .expect("plugin agent should be written");

        let catalog = load_plugin_subagent_catalog("content-plugin", agents_root);
        assert!(catalog.errors.is_empty(), "{:?}", catalog.errors);
        assert_eq!(catalog.categories.len(), 1);
        assert_eq!(catalog.categories[0].id, PLUGIN_SUBAGENT_CATEGORY_ID);
        let definition = catalog
            .definitions
            .first()
            .expect("one plugin definition should load");
        assert_eq!(definition.id, "content-plugin:engineering/reviewer");
        assert_eq!(definition.source.scope, SubagentDefinitionScope::Plugin);
        assert_eq!(definition.source.plugin_id(), Some("content-plugin"));
        assert_eq!(
            definition.source.category_id.as_deref(),
            Some(PLUGIN_SUBAGENT_CATEGORY_ID)
        );
        assert_eq!(
            definition.prompt_skills.policy.mode,
            CapabilityPolicyMode::AllowOnly
        );
        assert_eq!(
            definition.prompt_skills.policy.selectors,
            vec![String::from("content-plugin:review-helper")]
        );
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies plugin sidecar metadata cannot replace the owning manifest identity.
    /// 验证插件 sidecar 元数据不能替换所属清单身份。
    #[test]
    fn plugin_agent_rejects_foreign_sidecar_package_id() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-plugin-agent-owner-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let agents_root = root.join("agents");
        fs::create_dir_all(&agents_root).expect("plugin agent directory should be created");
        fs::write(
            agents_root.join("reviewer.md"),
            "Review the requested change.",
        )
        .expect("plugin agent should be written");
        fs::write(
            agents_root.join("reviewer.yaml"),
            "schemaVersion: 1\npackageId: foreign-plugin\n",
        )
        .expect("plugin agent sidecar should be written");

        let catalog = load_plugin_subagent_catalog("content-plugin", agents_root);

        assert!(catalog.definitions.is_empty());
        assert_eq!(catalog.errors.len(), 1);
        assert!(
            catalog.errors[0]
                .message
                .contains("owning plugin `content-plugin`")
        );
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies a plugin agent without attached skills receives an empty allow-only policy.
    /// 验证未附加技能的插件智能体获得空的仅允许策略。
    #[test]
    fn plugin_agent_without_skills_cannot_inherit_global_skills() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-plugin-agent-empty-skills-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let agents_root = root.join("agents");
        fs::create_dir_all(&agents_root).expect("plugin agent directory should be created");
        fs::write(
            agents_root.join("reviewer.md"),
            "Review the requested change.",
        )
        .expect("plugin agent should be written");

        let catalog = load_plugin_subagent_catalog("content-plugin", agents_root);

        assert!(catalog.errors.is_empty(), "{:?}", catalog.errors);
        let definition = catalog
            .definitions
            .first()
            .expect("one plugin definition should load");
        assert_eq!(
            definition.prompt_skills.policy.mode,
            CapabilityPolicyMode::AllowOnly
        );
        assert!(definition.prompt_skills.policy.selectors.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies a failed plugin-agent batch leaves definitions and categories unchanged.
    /// 验证失败的插件智能体批次保持定义与分类不变。
    #[test]
    fn plugin_agent_merge_is_transactional() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-plugin-agent-transaction-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let agents_root = root.join("agents");
        fs::create_dir_all(&agents_root).expect("plugin agent directory should be created");
        fs::write(
            agents_root.join("reviewer.md"),
            "Review the requested change.",
        )
        .expect("plugin agent should be written");
        let source = load_plugin_subagent_catalog("content-plugin", agents_root);
        let valid = source
            .definitions
            .first()
            .expect("one plugin definition should load")
            .clone();
        let mut invalid = valid.clone();
        invalid.id = String::from("foreign-plugin:reviewer");
        let mut target = load_subagent_catalog_from_roots(PathBuf::new(), PathBuf::new());

        let result = target.merge_plugin_definitions([valid, invalid]);

        assert!(result.is_err());
        assert!(target.definitions.is_empty());
        assert!(target.categories.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies plugin agents reject foreign skill selectors and component-level disablement.
    /// 验证插件智能体拒绝外部技能选择器与组件级停用。
    #[test]
    fn plugin_agent_rejects_cross_plugin_skills_and_individual_disablement() {
        let root = std::env::temp_dir().join(format!(
            "vulcan-plugin-agent-policy-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        let agents_root = root.join("agents");
        fs::create_dir_all(&agents_root).expect("plugin agent directory should be created");
        fs::write(
            agents_root.join("foreign.md"),
            "---\npromptSkills:\n  policy:\n    mode: allow_only\n    selectors:\n      - foreign-plugin:review-helper\n  enabled: []\n  disabled: []\n---\nReview.",
        )
        .expect("foreign-skill plugin agent should be written");
        fs::write(
            agents_root.join("disabled.md"),
            "---\nenabled: false\n---\nReview.",
        )
        .expect("disabled plugin agent should be written");

        let catalog = load_plugin_subagent_catalog("content-plugin", agents_root);

        assert!(catalog.definitions.is_empty());
        assert_eq!(catalog.errors.len(), 2);
        assert!(
            catalog.errors.iter().any(|error| {
                error
                    .message
                    .contains("must belong to plugin `content-plugin`")
            }),
            "{:?}",
            catalog.errors
        );
        assert!(
            catalog
                .errors
                .iter()
                .any(|error| error.message.contains("cannot be disabled individually"))
        );
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies an explicitly default-valued promptSkills object still conflicts with the plugin skills shorthand.
    /// 验证显式为默认值的 promptSkills 对象仍会与插件 skills 简写发生冲突。
    #[test]
    fn plugin_agent_rejects_explicit_default_prompt_skills_with_shorthand() {
        // Empty selector lists are semantically default-valued but remain an explicit declaration.
        // 空选择器列表在取值上等于默认值，但仍属于显式声明。
        let result = parse_subagent_definition_text(
            Path::new("reviewer.md"),
            "---\nname: Reviewer\npromptSkills:\n  enabled: []\n  disabled: []\nskills:\n  - review-helper\n---\nReview.",
            SubagentDefinitionScope::Plugin,
        );

        assert!(result.is_err());
        assert!(
            result
                .expect_err("explicit promptSkills should conflict with skills shorthand")
                .message
                .contains("cannot be declared together")
        );
    }

    /// Verifies the plugin-only skills shorthand is rejected for ordinary user definitions.
    /// 验证仅供插件使用的 skills 简写会被普通用户定义拒绝。
    #[test]
    fn user_agent_rejects_plugin_skills_shorthand() {
        let result = parse_subagent_definition_text(
            Path::new("reviewer.md"),
            "---\nname: Reviewer\nskills:\n  - review-helper\n---\nReview.",
            SubagentDefinitionScope::User,
        );

        assert!(result.is_err());
        assert!(
            result
                .expect_err("user skills shorthand should fail")
                .message
                .contains("reserved for plugin-contributed definitions")
        );
    }
}
