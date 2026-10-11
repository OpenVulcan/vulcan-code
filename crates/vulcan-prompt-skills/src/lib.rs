//! System and user prompt skill discovery, installation, policy resolution, and rendering.
//! 系统级与用户级提示词技能发现、安装、策略解析与渲染。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use vulcan_runtime_assets::{text_decode::decode_complete_user_text, user_vulcan_root_for_home};

mod install;

#[cfg(test)]
mod tests;

pub use install::{
    embedded_system_skill_names, install_embedded_system_prompt_skills,
    install_embedded_system_prompt_skills_to_root, install_system_prompt_skills_from_source,
};

/// Canonical prompt skill manifest file name.
/// 规范提示词技能清单文件名。
pub const SKILL_FILE_NAME: &str = "SKILL.md";

/// Default directory name used below the user's home directory.
/// 用户主目录下使用的默认目录名。
pub const DEFAULT_USER_SKILL_DIR: &str = "code/skills";

/// Directory name that stores host-managed system prompt skills.
/// 存放宿主管理系统提示词技能的目录名。
pub const SYSTEM_SKILL_DIR_NAME: &str = ".system";

/// VulcanCode-specific skill interface metadata file name in one skill root.
/// 单个技能根目录下的 VulcanCode 专用技能界面元数据文件名。
pub const VULCAN_SKILL_METADATA_FILE_NAME: &str = "vulcan.yaml";

/// Host-managed catalog metadata directory below the user skill root.
/// 用户技能根目录下由宿主管理的目录元数据目录。
pub const VULCAN_SKILL_CATALOG_DIR_NAME: &str = ".vulcan";

/// Host-managed logical catalog file name.
/// 宿主管理的逻辑目录文件名。
pub const VULCAN_SKILL_CATALOG_FILE_NAME: &str = "catalog.yaml";

/// Required schema version for package-local Vulcan metadata.
/// 技能包内 Vulcan 元数据要求的结构版本。
pub const VULCAN_SKILL_METADATA_SCHEMA_VERSION: u32 = 3;

/// Required schema version for the host-managed logical catalog.
/// 宿主管理逻辑目录要求的结构版本。
pub const VULCAN_SKILL_CATALOG_SCHEMA_VERSION: u32 = 1;

/// Monotonic suffix used to isolate concurrent filesystem transactions.
/// 用于隔离并发文件系统事务的单调递增后缀。
static PROMPT_SKILL_TRANSACTION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Process-start timestamp that keeps transaction names unique when a restarted process reuses a pid.
/// 进程启动时间戳；在重启后的进程复用 pid 时保持事务名称唯一。
static PROMPT_SKILL_TRANSACTION_EPOCH: std::sync::LazyLock<u128> = std::sync::LazyLock::new(|| {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default()
});

/// Builds one unique transaction token from the pid, process-start timestamp, and sequence.
/// 由 pid、进程启动时间戳与序号构建一个唯一事务标记。
///
/// Orphans left by a crashed process are never reused or deleted automatically because they may
/// hold the only copy of a previous package.
/// 崩溃进程留下的孤儿目录不会被复用或自动删除，因为其中可能保存着旧技能包的唯一副本。
fn next_prompt_skill_transaction_token() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        *PROMPT_SKILL_TRANSACTION_EPOCH,
        PROMPT_SKILL_TRANSACTION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// Origin scope of one prompt skill.
/// 单个提示词技能的来源作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptSkillScope {
    /// Host-managed system skill installed under `.system`.
    /// 安装在 `.system` 下的宿主管理系统技能。
    System,
    /// User-installed skill under the ordinary user skill root.
    /// 位于普通用户技能根目录下的用户安装技能。
    User,
    /// Read-only skill contributed by an enabled Vulcan plugin.
    /// 由已启用 Vulcan 插件贡献的只读技能。
    Plugin,
}

impl PromptSkillScope {
    /// Returns a compact English label for diagnostics.
    /// 返回用于诊断信息的紧凑英文标签。
    pub const fn en_label(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Plugin => "plugin",
        }
    }
}

/// One parsed prompt skill ready for runtime filtering and rendering.
/// 一条已解析且可用于运行时过滤与渲染的提示词技能。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkill {
    /// Stable skill name from frontmatter.
    /// 来自 frontmatter 的稳定技能名称。
    pub name: String,
    /// Model-facing trigger description from frontmatter.
    /// 来自 frontmatter 的面向模型触发描述。
    pub description: String,
    /// Optional license identifier or bundled license reference from the standard frontmatter.
    /// 来自标准 frontmatter 的可选协议标识或包内协议文件引用。
    pub license: Option<String>,
    /// Optional environment compatibility statement from the standard frontmatter.
    /// 来自标准 frontmatter 的可选环境兼容性说明。
    pub compatibility: Option<String>,
    /// Standard extension metadata preserved as string key-value pairs.
    /// 以字符串键值对保留的标准扩展元数据。
    pub metadata: BTreeMap<String, String>,
    /// Absolute or caller-provided path to the skill markdown file.
    /// 指向技能 Markdown 文件的绝对路径或调用方提供路径。
    pub skill_file: PathBuf,
    /// Directory that owns the skill markdown file and bundled resources.
    /// 拥有技能 Markdown 文件与附带资源的目录。
    pub base_dir: PathBuf,
    /// Root scope that supplied this prompt skill.
    /// 提供该提示词技能的根作用域。
    pub scope: PromptSkillScope,
    /// Optional UI-facing metadata loaded from root-level `vulcan.yaml`.
    /// 从根目录 `vulcan.yaml` 载入的可选界面展示元数据。
    pub interface: PromptSkillInterfaceMetadata,
    /// Logical catalog metadata resolved for this skill.
    /// 为当前技能解析得到的逻辑目录元数据。
    pub catalog: PromptSkillCatalogMetadata,
    /// Auditable origin metadata resolved for this skill.
    /// 为当前技能解析得到的可审计来源元数据。
    pub origin: PromptSkillOriginMetadata,
    /// Frozen resource summary discovered while loading the skill package.
    /// 加载技能包时发现并冻结的资源摘要。
    pub resources: PromptSkillResourceSummary,
    /// Markdown body without YAML frontmatter.
    /// 去除 YAML frontmatter 后的 Markdown 正文。
    pub body: String,
}

impl PromptSkill {
    /// Returns whether this skill has a non-empty user-visible description.
    /// 返回当前技能是否具有非空的用户可见描述。
    pub fn has_description(&self) -> bool {
        !self.description.trim().is_empty()
    }

    /// Returns the owning plugin identifier for a plugin-contributed skill.
    /// 返回插件贡献技能的所属插件标识。
    ///
    /// # Returns
    /// 返回值
    /// The stable plugin identifier, or `None` for non-plugin skills.
    /// 稳定插件标识；非插件技能返回 `None`。
    pub fn plugin_id(&self) -> Option<&str> {
        if self.scope != PromptSkillScope::Plugin {
            return None;
        }
        self.name.split_once(':').map(|(plugin_id, _)| plugin_id)
    }

    /// Returns the package-local skill name without the plugin namespace.
    /// 返回不含插件命名空间的包内技能名称。
    ///
    /// # Returns
    /// 返回值
    /// The package-local name for plugin skills or the canonical name for other scopes.
    /// 插件技能返回包内名称，其他作用域返回规范名称。
    pub fn local_name(&self) -> &str {
        if self.scope == PromptSkillScope::Plugin {
            return self
                .name
                .split_once(':')
                .map(|(_, local_name)| local_name)
                .unwrap_or(self.name.as_str());
        }
        self.name.as_str()
    }
}

/// Stable host-owned category identifier for plugin-contributed components.
/// 插件贡献组件使用的稳定宿主分类标识。
pub const PLUGIN_CATEGORY_ID: &str = "plugins";

/// Optional UI-facing metadata for one prompt skill.
/// 单个提示词技能的可选界面展示元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PromptSkillInterfaceMetadata {
    /// Human-facing display name shown in skill lists.
    /// 技能列表中显示的人类可读名称。
    pub display_name: Option<String>,
    /// Short human-facing description shown in compact cards.
    /// 紧凑卡片中显示的人类可读短描述。
    pub short_description: Option<String>,
    /// Optional emoji rendered by the same marker logic as agents.
    /// 由智能体同款标记逻辑渲染的可选表情图标。
    pub emoji: Option<String>,
    /// Optional hexadecimal marker color rendered by the skill list.
    /// 由技能列表渲染的可选十六进制标记颜色。
    pub color: Option<String>,
    /// Localized display metadata keyed by canonical BCP 47 locale tags.
    /// 按规范 BCP 47 语言标签索引的本地化展示元数据。
    pub locales: BTreeMap<String, PromptSkillLocaleMetadata>,
}

/// Localized human-facing metadata for one prompt skill.
/// 单个提示词技能的本地化人类可读元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PromptSkillLocaleMetadata {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    pub display_name: Option<String>,
    /// Optional localized compact description.
    /// 可选本地化紧凑说明。
    pub short_description: Option<String>,
}

/// Logical catalog metadata for one prompt skill.
/// 单个提示词技能的逻辑目录元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PromptSkillCatalogMetadata {
    /// Stable primary category identifier.
    /// 稳定的主分类标识。
    pub category_id: String,
    /// Stable searchable tags.
    /// 稳定的可搜索标签。
    pub tags: Vec<String>,
}

/// Auditable origin metadata for one prompt skill.
/// 单个提示词技能的可审计来源元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PromptSkillOriginMetadata {
    /// Stable origin kind.
    /// 稳定来源类型。
    pub kind: String,
    /// Optional human-facing source label.
    /// 可选人类可读来源标签。
    pub label: Option<String>,
    /// Optional source URL.
    /// 可选来源链接。
    pub url: Option<String>,
    /// Optional source version.
    /// 可选来源版本。
    pub version: Option<String>,
    /// Optional immutable source revision.
    /// 可选不可变来源修订号。
    pub revision: Option<String>,
    /// Whether lifecycle updates are managed by the host.
    /// 生命周期更新是否由宿主管理。
    pub managed: bool,
    /// Optional content integrity digest.
    /// 可选内容完整性摘要。
    pub integrity: Option<String>,
    /// Optional installation timestamp.
    /// 可选安装时间戳。
    pub installed_at: Option<String>,
    /// Optional last synchronization timestamp.
    /// 可选最近同步时间戳。
    pub updated_at: Option<String>,
}

/// Frozen resource summary for one standard Agent Skills package.
/// 单个标准 Agent Skills 技能包的冻结资源摘要。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillResourceSummary {
    /// Number of files below `scripts`.
    /// `scripts` 目录下的文件数量。
    pub scripts: usize,
    /// Number of files below `references`.
    /// `references` 目录下的文件数量。
    pub references: usize,
    /// Number of files below `assets`.
    /// `assets` 目录下的文件数量。
    pub assets: usize,
    /// Deterministic bounded sample of package-relative support-file paths.
    /// 确定且有界的包内相对支持文件路径样本。
    #[serde(default)]
    pub sampled_files: Vec<PathBuf>,
}

/// One logical Agent Skills category.
/// 一个逻辑 Agent Skills 分类。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PromptSkillCategory {
    /// Stable category identifier.
    /// 稳定分类标识。
    pub category_id: String,
    /// Default human-facing category name.
    /// 默认人类可读分类名称。
    pub name: String,
    /// Optional default category description.
    /// 可选默认分类说明。
    pub description: Option<String>,
    /// Human-facing category emoji.
    /// 人类可读分类图标。
    pub emoji: String,
    /// Human-facing category color.
    /// 人类可读分类颜色。
    pub color: String,
    /// Stable display order.
    /// 稳定展示顺序。
    pub order: i32,
    /// Whether the category ships with the host.
    /// 当前分类是否由宿主内置。
    pub builtin: bool,
    /// Localized category metadata keyed by locale tag.
    /// 按语言标签索引的本地化分类元数据。
    pub locales: BTreeMap<String, PromptSkillCategoryLocale>,
}

/// Localized human-facing category metadata.
/// 本地化人类可读分类元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PromptSkillCategoryLocale {
    /// Localized category name.
    /// 本地化分类名称。
    pub name: String,
    /// Optional localized category description.
    /// 可选本地化分类说明。
    pub description: Option<String>,
}

/// Complete canonical package content accepted by the user-skill writer.
/// 用户技能写入器接受的完整规范技能包内容。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptSkillPackageInput {
    /// Optional original name when renaming an existing user package.
    /// 重命名现有用户技能包时使用的可选原名称。
    pub original_name: Option<String>,
    /// Optional host-resolved source directory whose bundled resources seed the staged package.
    /// 其附带资源用于初始化暂存技能包的可选宿主解析来源目录。
    pub resource_source_dir: Option<PathBuf>,
    /// Standard canonical skill name.
    /// 标准规范技能名称。
    pub name: String,
    /// Model-facing trigger description.
    /// 面向模型的触发说明。
    pub description: String,
    /// Optional license identifier or bundled file reference.
    /// 可选协议标识或包内文件引用。
    pub license: Option<String>,
    /// Optional environment compatibility statement.
    /// 可选环境兼容性说明。
    pub compatibility: Option<String>,
    /// Standard string extension metadata.
    /// 标准字符串扩展元数据。
    pub metadata: BTreeMap<String, String>,
    /// Markdown instructions without YAML frontmatter.
    /// 不含 YAML frontmatter 的 Markdown 指令正文。
    pub body: String,
    /// Complete UI-facing metadata.
    /// 完整界面展示元数据。
    pub interface: PromptSkillInterfaceMetadata,
    /// Logical catalog metadata.
    /// 逻辑目录元数据。
    pub catalog: PromptSkillCatalogMetadata,
    /// Auditable provenance assigned by the host management boundary.
    /// 由宿主管理边界分配的可审计来源信息。
    pub origin: PromptSkillOriginMetadata,
}

/// One non-fatal loading issue captured while scanning prompt skills.
/// 扫描提示词技能时捕获的一条非致命加载问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillLoadError {
    /// Path related to the loading issue.
    /// 与加载问题相关的路径。
    pub path: PathBuf,
    /// Human-readable diagnostic message.
    /// 人类可读诊断消息。
    pub message: String,
}

/// Complete scan result for one prompt skill root.
/// 单个提示词技能根目录的完整扫描结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillCatalog {
    /// User skill root that was scanned.
    /// 已扫描的用户技能根目录。
    pub user_root: PathBuf,
    /// Host-managed system skill root that was scanned.
    /// 已扫描的宿主管理系统技能根目录。
    pub system_root: PathBuf,
    /// Parsed skills sorted by stable name.
    /// 按稳定名称排序的已解析技能集合。
    pub skills: Vec<PromptSkill>,
    /// All parsed source records before same-name scope resolution.
    /// 同名作用域解析前的全部已解析来源记录。
    pub records: Vec<PromptSkill>,
    /// Logical categories available to the Agent Skills page.
    /// Agent Skills 页面可用的逻辑分类。
    pub categories: Vec<PromptSkillCategory>,
    /// Non-fatal loading errors.
    /// 非致命加载错误集合。
    pub errors: Vec<PromptSkillLoadError>,
}

impl PromptSkillCatalog {
    /// Returns one skill by exact name.
    /// 按精确名称返回单个技能。
    pub fn skill(&self, name: &str) -> Option<&PromptSkill> {
        self.skills.iter().find(|skill| skill.name == name)
    }

    /// Returns one exact source record by its canonical `scope:name` key.
    /// 按规范 `scope:name` 键返回一条精确来源记录。
    pub fn source_record(&self, skill_key: &str) -> Option<&PromptSkill> {
        // scope_and_name decomposes the only accepted source-key structure.
        // scope_and_name 分解唯一允许的来源键结构。
        let (scope, name) = skill_key.split_once(':')?;
        // scope is resolved only from the two real package roots.
        // scope 仅从两个真实技能包根目录解析。
        let scope = match scope {
            "system" => PromptSkillScope::System,
            "user" => PromptSkillScope::User,
            "plugin" => PromptSkillScope::Plugin,
            _ => return None,
        };
        self.records
            .iter()
            .find(|skill| skill.scope == scope && skill.name == name)
    }

    /// Merges validated plugin skills into this effective catalog snapshot.
    /// 将已验证的插件技能合并到当前生效目录快照。
    ///
    /// # Parameters
    /// 参数
    /// `skills` contains namespaced plugin records from one immutable plugin catalog.
    /// `skills` 包含来自同一不可变插件目录的命名空间插件记录。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit, or a deterministic scope or identifier conflict diagnostic.
    /// 返回空值，或确定性的作用域及标识冲突诊断。
    pub fn merge_plugin_skills(
        &mut self,
        skills: impl IntoIterator<Item = PromptSkill>,
    ) -> Result<(), String> {
        let skills = skills.into_iter().collect::<Vec<_>>();
        let mut known_names = self
            .records
            .iter()
            .map(|skill| skill.name.clone())
            .collect::<BTreeSet<_>>();
        for skill in &skills {
            if skill.scope != PromptSkillScope::Plugin {
                return Err(format!(
                    "Prompt skill `{}` cannot enter the plugin catalog without plugin scope.",
                    skill.name
                ));
            }
            if skill.catalog.category_id != PLUGIN_CATEGORY_ID {
                return Err(format!(
                    "Plugin prompt skill `{}` must belong to category `{PLUGIN_CATEGORY_ID}`.",
                    skill.name
                ));
            }
            let Some((plugin_id, local_name)) = skill.name.split_once(':') else {
                return Err(format!(
                    "Plugin prompt skill `{}` must use a pluginId:localName identifier.",
                    skill.name
                ));
            };
            if !valid_category_id(plugin_id)
                || !valid_category_id(local_name)
                || local_name.contains(':')
            {
                return Err(format!(
                    "Plugin prompt skill `{}` has an invalid plugin namespace or local name.",
                    skill.name
                ));
            }
            if !known_names.insert(skill.name.clone()) {
                return Err(format!(
                    "Plugin prompt skill identifier `{}` conflicts with an existing catalog record.",
                    skill.name
                ));
            }
        }
        // Commit only after the complete incoming batch satisfies every invariant.
        // 仅在完整输入批次满足全部不变量后提交。
        self.records.extend(skills.iter().cloned());
        self.skills.extend(skills);
        self.records.sort_by(|left, right| {
            left.scope
                .cmp(&right.scope)
                .then_with(|| left.name.cmp(&right.name))
        });
        self.skills
            .sort_by(|left, right| left.name.cmp(&right.name));
        Ok(())
    }
}

/// Prompt skills roots and default per-skill configuration.
/// 提示词技能根目录与默认单技能配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillsGlobalConfig {
    /// Optional user skill root override.
    /// 可选用户技能根目录覆盖值。
    pub user_skill_root: Option<PathBuf>,
    /// Optional system skill root override.
    /// 可选系统技能根目录覆盖值。
    pub system_skill_root: Option<PathBuf>,
    /// Default state used when no per-skill override exists.
    /// 不存在单技能覆盖时使用的默认状态。
    pub default_skill_state: PromptSkillState,
    /// Persisted per-skill state overrides.
    /// 持久化单技能状态覆盖。
    pub skill_overrides: BTreeMap<String, PromptSkillState>,
}

impl Default for PromptSkillsGlobalConfig {
    /// Builds the default prompt skills roots and per-skill configuration.
    /// 构建默认提示词技能根目录与单技能配置。
    fn default() -> Self {
        Self {
            user_skill_root: None,
            system_skill_root: None,
            default_skill_state: PromptSkillState::Enabled,
            skill_overrides: BTreeMap::new(),
        }
    }
}

/// Result of one system prompt-skill installation pass.
/// 单次系统提示词技能安装过程的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemPromptSkillsInstallReport {
    /// Destination system prompt-skill root.
    /// 系统提示词技能目标根目录。
    pub system_root: PathBuf,
    /// Whether files were rewritten during this install pass.
    /// 本次安装过程中是否重写了文件。
    pub installed: bool,
    /// Embedded resource fingerprint stored in the marker.
    /// 写入标记文件的内置资源指纹。
    pub fingerprint: String,
    /// Installed top-level skill directory names.
    /// 已安装的顶层技能目录名称。
    pub skill_names: Vec<String>,
}

/// Project-level prompt skill overrides.
/// 项目级提示词技能覆盖配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillsProjectConfig {
    /// Project per-skill state overrides.
    /// 项目单技能状态覆盖。
    pub skill_overrides: BTreeMap<String, PromptSkillState>,
}

/// Session-level prompt skill overrides.
/// 会话级提示词技能覆盖配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillsSessionConfig {
    /// Session per-skill state overrides.
    /// 会话单技能状态覆盖。
    pub skill_overrides: BTreeMap<String, PromptSkillState>,
}

/// Effective on/off state for one prompt skill.
/// 单个提示词技能的生效开关状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptSkillState {
    /// The skill is visible and loadable.
    /// 当前技能可见且可加载。
    Enabled,
    /// The skill is hidden and cannot be loaded.
    /// 当前技能隐藏且不可加载。
    Disabled,
}

impl PromptSkillState {
    /// Returns whether the state enables a skill.
    /// 返回该状态是否启用技能。
    pub const fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// One skill after layered policy resolution.
/// 经过分层策略解析后的单个技能。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPromptSkill {
    /// Parsed prompt skill metadata and content.
    /// 已解析的提示词技能元数据与正文。
    pub skill: PromptSkill,
    /// Final effective state for the current scope.
    /// 当前作用域下的最终生效状态。
    pub state: PromptSkillState,
    /// Source that supplied the final state.
    /// 提供最终状态的来源。
    pub source: PromptSkillStateSource,
}

impl ResolvedPromptSkill {
    /// Returns whether the resolved skill is enabled.
    /// 返回解析后的技能是否启用。
    pub const fn is_enabled(&self) -> bool {
        self.state.is_enabled()
    }
}

/// Source tier that decided one skill state.
/// 决定单个技能状态的来源层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptSkillStateSource {
    /// The owning plugin lifecycle published this skill as enabled.
    /// 所属插件生命周期将当前技能发布为启用状态。
    Plugin,
    /// The session-level override decided the state.
    /// 会话级覆盖决定了该状态。
    Session,
    /// The project-level override decided the state.
    /// 项目级覆盖决定了该状态。
    Project,
    /// The global per-skill override decided the state.
    /// 全局单技能覆盖决定了该状态。
    GlobalOverride,
    /// The global default decided the state.
    /// 全局默认值决定了该状态。
    GlobalDefault,
}

/// Resolves the default user prompt skill root.
/// 解析默认用户提示词技能根目录。
pub fn default_user_skill_root() -> PathBuf {
    let user_vulcan_root = vulcan_runtime_assets::user_home_dir()
        .map(|home| user_vulcan_root_for_home(&home))
        .unwrap_or_else(|| user_vulcan_root_for_home(Path::new(".")));
    user_vulcan_root.join(DEFAULT_USER_SKILL_DIR)
}

/// Resolves the default host-managed system prompt skill root.
/// 解析默认宿主管理系统提示词技能根目录。
pub fn default_system_skill_root() -> PathBuf {
    default_user_skill_root().join(SYSTEM_SKILL_DIR_NAME)
}

/// Resolves the configured user prompt skill root.
/// 解析配置后的用户提示词技能根目录。
pub fn configured_user_skill_root(config: &PromptSkillsGlobalConfig) -> PathBuf {
    config
        .user_skill_root
        .clone()
        .unwrap_or_else(default_user_skill_root)
}

/// Resolves the host-managed system prompt skill root for one user root.
/// 根据给定用户根目录解析宿主管理系统提示词技能根目录。
pub fn configured_system_skill_root(config: &PromptSkillsGlobalConfig) -> PathBuf {
    config
        .system_skill_root
        .clone()
        .unwrap_or_else(default_system_skill_root)
}

/// Loads prompt skills from the configured system and user directories.
/// 从配置后的系统目录与用户目录共同加载提示词技能。
pub fn load_prompt_skills(config: &PromptSkillsGlobalConfig) -> PromptSkillCatalog {
    let user_root = configured_user_skill_root(config);
    let system_root = configured_system_skill_root(config);
    load_prompt_skills_from_roots(system_root, user_root)
}

/// Loads prompt skills from one user root.
/// 从单个用户根目录加载提示词技能。
pub fn load_prompt_skills_from_user_root(user_root: PathBuf) -> PromptSkillCatalog {
    load_prompt_skills_from_roots(PathBuf::new(), user_root)
}

/// Loads prompt skills from one system root and one user root.
/// 从单个系统根目录与单个用户根目录加载提示词技能。
pub fn load_prompt_skills_from_roots(
    system_root: PathBuf,
    user_root: PathBuf,
) -> PromptSkillCatalog {
    // Initialize one complete catalog transaction for both source scopes.
    // 为两个来源作用域初始化一次完整目录事务。
    let mut catalog = PromptSkillCatalog {
        user_root: user_root.clone(),
        system_root: system_root.clone(),
        skills: Vec::new(),
        records: Vec::new(),
        categories: default_prompt_skill_categories(),
        errors: Vec::new(),
    };
    // Preserve system records independently so user overrides never erase provenance.
    // 独立保留系统记录，确保用户覆盖不会抹除来源。
    let mut system_by_name = BTreeMap::<String, PromptSkill>::new();
    // Preserve user records independently for explicit effective-resolution behavior.
    // 独立保留用户记录，以实现显式生效解析行为。
    let mut user_by_name = BTreeMap::<String, PromptSkill>::new();
    scan_prompt_skill_root(
        &system_root,
        PromptSkillScope::System,
        &mut system_by_name,
        &mut catalog.errors,
    );
    scan_prompt_skill_root(
        &user_root,
        PromptSkillScope::User,
        &mut user_by_name,
        &mut catalog.errors,
    );
    // Load the host-managed logical catalog without mutating third-party packages.
    // 加载宿主管理逻辑目录，不修改第三方技能包。
    let host_catalog = load_host_prompt_skill_catalog(&user_root, &mut catalog.errors);
    merge_user_prompt_skill_categories(
        &mut catalog.categories,
        host_catalog.categories,
        &user_root,
        &mut catalog.errors,
    );
    apply_host_prompt_skill_overrides(
        &host_catalog.skills,
        &mut system_by_name,
        &mut user_by_name,
        &mut catalog.errors,
    );
    // Build the category lookup used to reject orphan assignments deterministically.
    // 构建用于确定性拒绝孤立归属的分类查找表。
    let category_ids = catalog
        .categories
        .iter()
        .map(|category| category.category_id.clone())
        .collect::<BTreeSet<_>>();
    normalize_prompt_skill_categories(
        system_by_name.values_mut(),
        &category_ids,
        &mut catalog.errors,
    );
    normalize_prompt_skill_categories(
        user_by_name.values_mut(),
        &category_ids,
        &mut catalog.errors,
    );
    // Retain every parsed source record in stable scope then name order.
    // 按稳定的作用域和名称顺序保留每条已解析来源记录。
    catalog.records.extend(system_by_name.values().cloned());
    catalog.records.extend(user_by_name.values().cloned());
    // Resolve one effective runtime record per standard name with the user scope winning.
    // 按用户作用域优先规则为每个标准名称解析一条运行时有效记录。
    let mut effective_by_name = system_by_name;
    effective_by_name.extend(user_by_name);
    catalog.skills = effective_by_name.into_values().collect();
    catalog
}

/// Loads every read-only prompt skill contributed by one plugin package.
/// 加载单个插件包贡献的全部只读提示词技能。
///
/// # Parameters
/// 参数
/// - `plugin_id`: Stable owning plugin identifier.
/// - `plugin_id`：稳定的所属插件标识。
/// - `plugin_name`: Human-facing owning plugin name.
/// - `plugin_name`：面向用户的所属插件名称。
/// - `plugin_version`: Owning plugin version.
/// - `plugin_version`：所属插件版本。
/// - `skills_root`: Conventional plugin `skills/` directory.
/// - `skills_root`：插件约定的 `skills/` 目录。
///
/// # Returns
/// 返回值
/// Returns the complete namespaced skill set, or every deterministic parse error.
/// 返回完整的命名空间技能集合，或全部确定性解析错误。
pub fn load_plugin_prompt_skills(
    plugin_id: &str,
    plugin_name: &str,
    plugin_version: &str,
    skills_root: &Path,
) -> Result<Vec<PromptSkill>, Vec<PromptSkillLoadError>> {
    let mut by_name = BTreeMap::<String, PromptSkill>::new();
    let mut errors = Vec::new();
    scan_prompt_skill_root(
        skills_root,
        PromptSkillScope::Plugin,
        &mut by_name,
        &mut errors,
    );
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut skills = by_name.into_values().collect::<Vec<_>>();
    for skill in &mut skills {
        let local_name = skill.name.clone();
        skill.name = format!("{plugin_id}:{local_name}");
        skill.catalog.category_id = String::from(PLUGIN_CATEGORY_ID);
        if skill.origin.kind == "plugin" {
            skill.origin.label = Some(plugin_name.to_string());
        }
        if skill.origin.version.is_none() {
            skill.origin.version = Some(plugin_version.to_string());
        }
        skill.origin.managed = true;
    }
    Ok(skills)
}

/// Returns the immutable default logical Agent Skills categories.
/// 返回不可变的默认逻辑 Agent Skills 分类。
pub fn default_prompt_skill_categories() -> Vec<PromptSkillCategory> {
    vec![
        builtin_prompt_skill_category("general", "General", "General", "✨", "#64748B", 10),
        builtin_prompt_skill_category(
            "engineering",
            "Engineering",
            "Engineering",
            "💻",
            "#2563EB",
            20,
        ),
        builtin_prompt_skill_category(
            "quality",
            "Testing & Quality",
            "Testing and quality",
            "✅",
            "#16A34A",
            30,
        ),
        builtin_prompt_skill_category(
            "operations",
            "Operations & Automation",
            "Operations and automation",
            "⚙️",
            "#EA580C",
            40,
        ),
        builtin_prompt_skill_category("data", "Data", "Data", "📊", "#0891B2", 50),
        builtin_prompt_skill_category(
            "documents",
            "Documents & Content",
            "Docs and content",
            "📄",
            "#7C3AED",
            60,
        ),
        builtin_prompt_skill_category(
            "design",
            "Design & Media",
            "Design and media",
            "🎨",
            "#DB2777",
            70,
        ),
        builtin_prompt_skill_category(
            "research",
            "Research",
            "Research and retrieval",
            "🔎",
            "#4F46E5",
            80,
        ),
        builtin_prompt_skill_category("security", "Security", "Security", "🛡️", "#DC2626", 90),
        builtin_prompt_skill_category(
            "integrations",
            "Integrations",
            "Integrations",
            "🔌",
            "#0D9488",
            100,
        ),
        builtin_prompt_skill_category(
            PLUGIN_CATEGORY_ID,
            "Plugins",
            "Plugins",
            "🧩",
            "#6366F1",
            110,
        ),
        builtin_prompt_skill_category(
            "uncategorized",
            "Uncategorized",
            "Unassigned",
            "📦",
            "#78716C",
            1000,
        ),
    ]
}

/// Builds one immutable bilingual default category.
/// 构建一个不可变的双语默认分类。
fn builtin_prompt_skill_category(
    category_id: &str,
    english_name: &str,
    chinese_name: &str,
    emoji: &str,
    color: &str,
    order: i32,
) -> PromptSkillCategory {
    // Build the localized display map for both first-class interface languages.
    // 为两种一级界面语言构建本地化展示映射。
    let locales = BTreeMap::from([
        (
            String::from("en-US"),
            PromptSkillCategoryLocale {
                name: english_name.to_string(),
                description: None,
            },
        ),
        (
            String::from("zh-CN"),
            PromptSkillCategoryLocale {
                name: chinese_name.to_string(),
                description: None,
            },
        ),
    ]);
    PromptSkillCategory {
        category_id: category_id.to_string(),
        name: english_name.to_string(),
        description: None,
        emoji: emoji.to_string(),
        color: color.to_string(),
        order,
        builtin: true,
        locales,
    }
}

/// Saves one user-owned Agent Skills package through a validated directory transaction.
/// 通过经过校验的目录事务保存一个用户拥有的 Agent Skills 技能包。
///
/// # Parameters
/// 参数
/// - `user_root`: User-owned Agent Skills root directory.
/// - `user_root`：用户拥有的 Agent Skills 根目录。
/// - `input`: Complete canonical package content.
/// - `input`：完整规范技能包内容。
///
/// # Returns
/// 返回值
/// The parsed committed package, or a deterministic validation or filesystem error.
/// 返回解析后的已提交技能包，或确定性的校验/文件系统错误。
pub fn save_user_prompt_skill_package(
    user_root: &Path,
    input: PromptSkillPackageInput,
) -> Result<PromptSkill, String> {
    if input.name != input.name.trim() {
        return Err(String::from(
            "Managed Agent Skill names must already be canonical and cannot contain surrounding whitespace.",
        ));
    }
    let target_name = input.name.clone();
    validate_prompt_skill_name(Path::new(SKILL_FILE_NAME), &target_name)
        .map_err(|error| error.message)?;
    let submitted_original_name = input.original_name.as_deref();
    if submitted_original_name.is_some_and(|value| value.is_empty() || value != value.trim()) {
        return Err(String::from(
            "Original Agent Skill names must already be canonical and cannot be empty or contain surrounding whitespace.",
        ));
    }
    let original_name = submitted_original_name
        .unwrap_or(target_name.as_str())
        .to_string();
    validate_prompt_skill_name(Path::new(SKILL_FILE_NAME), &original_name)
        .map_err(|error| error.message)?;
    validate_prompt_skill_package_input(&input)?;
    ensure_managed_prompt_skill_directory(user_root, "Agent Skills user root")?;
    let target_dir = user_root.join(&target_name);
    let original_dir = user_root.join(&original_name);
    let original_exists =
        managed_prompt_skill_directory_exists(&original_dir, "original user Agent Skill package")?;
    let target_exists = if target_dir == original_dir {
        original_exists
    } else {
        managed_prompt_skill_directory_exists(&target_dir, "target user Agent Skill package")?
    };
    if submitted_original_name.is_none() && target_exists {
        return Err(format!(
            "Agent Skill `{target_name}` already exists in the user catalog."
        ));
    }
    if submitted_original_name.is_some() && !original_exists {
        return Err(format!(
            "Original user Agent Skill `{original_name}` does not exist."
        ));
    }
    if target_name != original_name && target_exists {
        return Err(format!(
            "Agent Skill `{target_name}` already exists in the user catalog."
        ));
    }
    // host_catalog is the authoritative logical category and tag assignment table.
    // host_catalog 是权威逻辑分类与标签归属表。
    let mut host_catalog = read_host_prompt_skill_catalog_strict(user_root)?;
    // category_exists validates against immutable defaults and user-added category rows.
    // category_exists 同时针对不可变默认分类与用户附加分类校验。
    let category_exists = default_prompt_skill_categories()
        .iter()
        .any(|category| category.category_id == input.catalog.category_id)
        || host_catalog
            .categories
            .iter()
            .any(|category| category.category_id == input.catalog.category_id);
    if !category_exists {
        return Err(format!(
            "Agent Skills category `{}` does not exist.",
            input.catalog.category_id
        ));
    }
    if original_name != target_name {
        host_catalog.skills.remove(&original_name);
    }
    // assignment is the unique host row synchronized with the package transaction.
    // assignment 是与技能包事务同步的唯一宿主归属记录。
    let assignment = host_catalog.skills.entry(target_name.clone()).or_default();
    assignment.category_id = Some(input.catalog.category_id.clone());
    assignment.tags = Some(normalized_unique_strings(input.catalog.tags.clone()));
    let transaction_dir = prompt_skill_transaction_dir(user_root)?;
    let staged_dir = transaction_dir.join(&target_name);
    let backup_dir = transaction_dir.join(".backup");
    fs::create_dir(&transaction_dir)
        .map_err(|error| format!("Failed to create Agent Skills transaction directory: {error}"))?;
    // preserve_transaction keeps the transaction directory when it still holds the only copy of the previous package.
    // preserve_transaction 在事务目录仍持有旧技能包唯一副本时保留该目录。
    let mut preserve_transaction = false;
    let transaction_result = (|| {
        if original_exists {
            copy_prompt_skill_directory(&original_dir, &staged_dir)?;
        } else if let Some(source_dir) = input.resource_source_dir.as_deref() {
            copy_prompt_skill_directory(source_dir, &staged_dir)?;
        } else {
            fs::create_dir_all(&staged_dir)
                .map_err(|error| format!("Failed to create staged Agent Skill: {error}"))?;
        }
        write_prompt_skill_package_files(&staged_dir, &input, &target_name)?;
        parse_prompt_skill_file_with_scope(
            &staged_dir.join(SKILL_FILE_NAME),
            PromptSkillScope::User,
        )
        .map_err(|error| error.message)?;
        if original_exists {
            prompt_skill_resource_summary(&original_dir).map_err(|error| error.message)?;
            fs::rename(&original_dir, &backup_dir)
                .map_err(|error| format!("Failed to stage existing Agent Skill backup: {error}"))?;
        }
        if let Err(error) = fs::rename(&staged_dir, &target_dir) {
            let error = format!("Failed to commit Agent Skill `{target_name}`: {error}");
            if original_exists {
                return Err(restore_prompt_skill_backup(
                    &backup_dir,
                    &original_dir,
                    error,
                    &mut preserve_transaction,
                ));
            }
            return Err(error);
        }
        if let Err(error) = write_host_prompt_skill_catalog(user_root, &host_catalog) {
            if let Err(remove_error) = fs::remove_dir_all(&target_dir) {
                if original_exists {
                    preserve_transaction = true;
                    return Err(format!(
                        "{error}; removing the uncommitted Agent Skill `{target_name}` also failed ({remove_error}), the previous package is preserved at {}",
                        vulcan_runtime_assets::native_host_path_text_from_path(&backup_dir)
                    ));
                }
                return Err(format!(
                    "{error}; removing the uncommitted Agent Skill `{target_name}` also failed: {remove_error}"
                ));
            }
            if original_exists {
                return Err(restore_prompt_skill_backup(
                    &backup_dir,
                    &original_dir,
                    error,
                    &mut preserve_transaction,
                ));
            }
            return Err(error);
        }
        if original_exists {
            let _ = fs::remove_dir_all(&backup_dir);
        }
        parse_prompt_skill_file_with_scope(
            &target_dir.join(SKILL_FILE_NAME),
            PromptSkillScope::User,
        )
        .map_err(|error| error.message)
    })();
    if !preserve_transaction {
        let _ = fs::remove_dir_all(&transaction_dir);
    }
    transaction_result
}

/// Restores one staged Agent Skill backup and keeps the transaction when restoration fails.
/// 恢复一个已暂存的 Agent Skill 备份，恢复失败时保留事务目录。
///
/// # Parameters
/// 参数
/// `backup_dir` is the staged copy of the previous package.
/// `backup_dir` 是旧技能包的暂存副本。
/// `original_dir` is the package location being restored.
/// `original_dir` 是需要恢复的技能包位置。
/// `error` is the commit failure that triggered the rollback.
/// `error` 是触发回滚的提交失败信息。
/// `preserve_transaction` is set when the backup must stay on disk.
/// `preserve_transaction` 在备份必须保留在磁盘上时被置位。
///
/// # Returns
/// 返回值
/// Returns the error reported to the caller, including the backup location when restoration fails.
/// 返回报告给调用方的错误；恢复失败时包含备份位置。
fn restore_prompt_skill_backup(
    backup_dir: &Path,
    original_dir: &Path,
    error: String,
    preserve_transaction: &mut bool,
) -> String {
    match fs::rename(backup_dir, original_dir) {
        Ok(()) => error,
        Err(restore_error) => {
            *preserve_transaction = true;
            format!(
                "{error}; restoring the previous Agent Skill package also failed ({restore_error}), the previous package is preserved at {}",
                vulcan_runtime_assets::native_host_path_text_from_path(backup_dir)
            )
        }
    }
}

/// Deletes one exact user-owned Agent Skills package without touching system packages.
/// 删除一个精确的用户 Agent Skills 技能包且不触碰系统技能包。
pub fn delete_user_prompt_skill_package(user_root: &Path, name: &str) -> Result<(), String> {
    if name != name.trim() {
        return Err(String::from(
            "Managed Agent Skill names must already be canonical and cannot contain surrounding whitespace.",
        ));
    }
    validate_prompt_skill_name(Path::new(SKILL_FILE_NAME), name).map_err(|error| error.message)?;
    let target_dir = user_root.join(name);
    if !managed_prompt_skill_directory_exists(user_root, "Agent Skills user root")?
        || !managed_prompt_skill_directory_exists(&target_dir, "user Agent Skill package")?
    {
        return Err(format!("User Agent Skill `{name}` does not exist."));
    }
    prompt_skill_resource_summary(&target_dir).map_err(|error| error.message)?;
    // catalog is the strict assignment table synchronized with package deletion.
    // catalog 是与技能包删除同步的严格归属表。
    let mut catalog = read_host_prompt_skill_catalog_strict(user_root)?;
    catalog.skills.remove(name);
    // transaction_dir retains the package until catalog persistence succeeds.
    // transaction_dir 在目录持久化成功前保留技能包。
    let transaction_dir = prompt_skill_transaction_dir(user_root)?;
    let removed_dir = transaction_dir.join("removed");
    fs::create_dir(&transaction_dir)
        .map_err(|error| format!("Failed to create Agent Skills transaction directory: {error}"))?;
    fs::rename(&target_dir, &removed_dir)
        .map_err(|error| format!("Failed to remove user Agent Skill `{name}`: {error}"))?;
    if let Err(error) = write_host_prompt_skill_catalog(user_root, &catalog) {
        let mut preserve_transaction = false;
        let error = restore_prompt_skill_backup(
            &removed_dir,
            &target_dir,
            error,
            &mut preserve_transaction,
        );
        if !preserve_transaction {
            let _ = fs::remove_dir_all(&transaction_dir);
        }
        return Err(error);
    }
    let _ = fs::remove_dir_all(&transaction_dir);
    Ok(())
}

/// Saves or replaces one user-defined logical category in the host catalog.
/// 在宿主目录中保存或替换一个用户定义逻辑分类。
pub fn save_user_prompt_skill_category(
    user_root: &Path,
    mut category: PromptSkillCategory,
) -> Result<(), String> {
    if category.category_id == PLUGIN_CATEGORY_ID {
        return Err(String::from(
            "The `plugins` prompt skill category is host-owned and read-only.",
        ));
    }
    validate_prompt_skill_category(&category)?;
    if default_prompt_skill_categories()
        .iter()
        .any(|builtin| builtin.category_id == category.category_id)
    {
        return Err(format!(
            "Built-in category `{}` is immutable.",
            category.category_id
        ));
    }
    category.builtin = false;
    let mut catalog = read_host_prompt_skill_catalog_strict(user_root)?;
    catalog
        .categories
        .retain(|item| item.category_id != category.category_id);
    catalog.categories.push(category);
    catalog
        .categories
        .sort_by_key(|item| (item.order, item.category_id.clone()));
    write_host_prompt_skill_catalog(user_root, &catalog)
}

/// Deletes one user-defined category and moves its assigned skills to `uncategorized`.
/// 删除一个用户定义分类，并将其已归属技能迁移到 `uncategorized`。
pub fn delete_user_prompt_skill_category(
    system_root: &Path,
    user_root: &Path,
    category_id: &str,
) -> Result<(), String> {
    if category_id == PLUGIN_CATEGORY_ID {
        return Err(String::from(
            "The `plugins` prompt skill category is host-owned and cannot be deleted.",
        ));
    }
    validate_prompt_skill_name(Path::new(VULCAN_SKILL_CATALOG_FILE_NAME), category_id)
        .map_err(|error| error.message)?;
    if default_prompt_skill_categories()
        .iter()
        .any(|category| category.category_id == category_id)
    {
        return Err(format!("Built-in category `{category_id}` is immutable."));
    }
    // loaded is the exact merged catalog used to validate assignment identity.
    // loaded 是用于校验归属身份的精确合并目录。
    let loaded = load_prompt_skills_from_roots(system_root.to_path_buf(), user_root.to_path_buf());
    if !loaded
        .categories
        .iter()
        .any(|category| category.category_id == category_id && !category.builtin)
    {
        return Err(format!(
            "User Agent Skills category `{category_id}` does not exist."
        ));
    }
    let assigned_names = loaded
        .skills
        .iter()
        .filter(|skill| skill.catalog.category_id == category_id)
        .map(|skill| skill.name.clone())
        .collect::<Vec<_>>();
    // catalog is the strict user-root assignment table committed atomically below.
    // catalog 是下方原子提交的严格用户根目录归属表。
    let mut catalog = read_host_prompt_skill_catalog_strict(user_root)?;
    catalog
        .categories
        .retain(|category| category.category_id != category_id);
    for name in assigned_names {
        catalog.skills.entry(name).or_default().category_id = Some(String::from("uncategorized"));
    }
    write_host_prompt_skill_catalog(user_root, &catalog)
}

/// Assigns one effective skill to an exact logical category through host metadata.
/// 通过宿主元数据将一个有效技能归入精确逻辑分类。
pub fn set_user_prompt_skill_category(
    system_root: &Path,
    user_root: &Path,
    name: &str,
    category_id: &str,
) -> Result<(), String> {
    if category_id == PLUGIN_CATEGORY_ID {
        return Err(String::from(
            "The `plugins` prompt skill category cannot receive manual assignments.",
        ));
    }
    validate_prompt_skill_name(Path::new(SKILL_FILE_NAME), name).map_err(|error| error.message)?;
    let available =
        load_prompt_skills_from_roots(system_root.to_path_buf(), user_root.to_path_buf());
    if available.skill(name).is_none() {
        return Err(format!("Agent Skill `{name}` does not exist."));
    }
    if !available
        .categories
        .iter()
        .any(|category| category.category_id == category_id)
    {
        return Err(format!(
            "Agent Skills category `{category_id}` does not exist."
        ));
    }
    let mut catalog = read_host_prompt_skill_catalog_strict(user_root)?;
    catalog
        .skills
        .entry(name.to_string())
        .or_default()
        .category_id = Some(category_id.to_string());
    write_host_prompt_skill_catalog(user_root, &catalog)
}

/// Persists one managed skill assignment in the user-root logical catalog table.
/// 在用户技能根目录的逻辑目录表中持久化一个受管技能归属。
///
/// Parameters: system and user roots, optional renamed source name, target name, category, and tags.
/// 参数：系统与用户根目录、可选重命名来源名称、目标名称、分类与标签。
/// Returns: success after the strict catalog file is atomically replaced.
/// 返回值：严格目录文件完成原子替换后返回成功。
pub fn save_user_prompt_skill_catalog_assignment(
    system_root: &Path,
    user_root: &Path,
    original_name: Option<&str>,
    name: &str,
    category_id: &str,
    tags: Vec<String>,
) -> Result<(), String> {
    validate_prompt_skill_name(Path::new(SKILL_FILE_NAME), name).map_err(|error| error.message)?;
    if let Some(original_name) = original_name {
        validate_prompt_skill_name(Path::new(SKILL_FILE_NAME), original_name)
            .map_err(|error| error.message)?;
    }
    let loaded = load_prompt_skills_from_roots(system_root.to_path_buf(), user_root.to_path_buf());
    if !loaded.skills.iter().any(|skill| skill.name == name) {
        return Err(format!("Agent Skill `{name}` does not exist."));
    }
    if !loaded
        .categories
        .iter()
        .any(|category| category.category_id == category_id)
    {
        return Err(format!(
            "Agent Skills category `{category_id}` does not exist."
        ));
    }
    let mut catalog = read_host_prompt_skill_catalog_strict(user_root)?;
    if let Some(original_name) = original_name.filter(|original_name| *original_name != name) {
        catalog.skills.remove(original_name);
    }
    // metadata is the unique assignment row keyed by the canonical package name.
    // metadata 是以规范技能包名称为键的唯一归属记录。
    let metadata = catalog.skills.entry(name.to_string()).or_default();
    metadata.category_id = Some(category_id.to_string());
    metadata.tags = Some(normalized_unique_strings(tags));
    write_host_prompt_skill_catalog(user_root, &catalog)
}

/// Validates all package input fields that are not covered by the standard parser.
/// 校验标准解析器未覆盖的全部技能包输入字段。
fn validate_prompt_skill_package_input(input: &PromptSkillPackageInput) -> Result<(), String> {
    if input.description.trim().is_empty() || input.description.chars().count() > 1024 {
        return Err(String::from(
            "Agent Skill description must contain 1 to 1024 characters.",
        ));
    }
    if input
        .compatibility
        .as_deref()
        .is_some_and(|value| value.chars().count() > 500)
    {
        return Err(String::from(
            "Agent Skill compatibility must not exceed 500 characters.",
        ));
    }
    if input.origin.kind.trim().is_empty() {
        return Err(String::from(
            "Managed Agent Skill origin kind cannot be empty.",
        ));
    }
    if input
        .interface
        .emoji
        .as_deref()
        .is_some_and(|emoji| emoji.trim().is_empty())
    {
        return Err(String::from("Agent Skill emoji cannot be empty."));
    }
    if input
        .interface
        .color
        .as_deref()
        .is_some_and(|color| !valid_hex_color(color.trim()))
    {
        return Err(String::from(
            "Agent Skill color must be a hexadecimal RGB or RGBA color.",
        ));
    }
    Ok(())
}

/// Writes canonical SKILL.md and vulcan.yaml files into one staged directory.
/// 将规范 SKILL.md 与 vulcan.yaml 写入一个暂存目录。
fn write_prompt_skill_package_files(
    directory: &Path,
    input: &PromptSkillPackageInput,
    target_name: &str,
) -> Result<(), String> {
    let frontmatter = PromptSkillFrontmatter {
        name: target_name.to_string(),
        description: input.description.trim().to_string(),
        license: normalize_optional_string(input.license.clone()),
        compatibility: normalize_optional_string(input.compatibility.clone()),
        metadata: input.metadata.clone(),
    };
    let yaml = serde_yaml::to_string(&frontmatter)
        .map_err(|error| format!("Failed to serialize Agent Skill frontmatter: {error}"))?;
    let skill_text = format!("---\n{yaml}---\n{}", input.body.trim_start());
    fs::write(directory.join(SKILL_FILE_NAME), skill_text)
        .map_err(|error| format!("Failed to write staged SKILL.md: {error}"))?;
    let metadata = PromptSkillAgentMetadata {
        schema_version: VULCAN_SKILL_METADATA_SCHEMA_VERSION,
        interface: PromptSkillInterfaceMetadataRaw {
            display_name: input.interface.display_name.clone(),
            short_description: input.interface.short_description.clone(),
            emoji: input.interface.emoji.clone(),
            color: input.interface.color.clone(),
            locales: input.interface.locales.clone(),
        },
        catalog: input.catalog.clone(),
        origin: input.origin.clone(),
    };
    let yaml = serde_yaml::to_string(&metadata)
        .map_err(|error| format!("Failed to serialize Agent Skill metadata: {error}"))?;
    fs::write(directory.join(VULCAN_SKILL_METADATA_FILE_NAME), yaml)
        .map_err(|error| format!("Failed to write staged vulcan.yaml: {error}"))
}

/// Recursively copies one package while rejecting symbolic links.
/// 递归复制一个技能包并拒绝符号链接。
fn copy_prompt_skill_directory(source: &Path, destination: &Path) -> Result<(), String> {
    let source_metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("Failed to inspect existing Agent Skill directory: {error}"))?;
    if metadata_is_link_like(&source_metadata) || !source_metadata.is_dir() {
        return Err(format!(
            "Managed Agent Skill package source must be a regular directory and cannot be a symbolic link or Windows reparse point: {}",
            vulcan_runtime_assets::native_host_path_text_from_path(source)
        ));
    }
    fs::create_dir_all(destination)
        .map_err(|error| format!("Failed to create staged package directory: {error}"))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("Failed to read existing Agent Skill: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("Failed to inspect existing Agent Skill: {error}"))?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)
            .map_err(|error| format!("Failed to inspect package entry: {error}"))?;
        if metadata_is_link_like(&metadata) {
            return Err(format!(
                "Symbolic links and Windows reparse points are not permitted in managed Agent Skill packages: {}",
                vulcan_runtime_assets::native_host_path_text_from_path(&entry_path)
            ));
        }
        let next_destination = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_prompt_skill_directory(&entry_path, &next_destination)?;
        } else if metadata.is_file() {
            fs::copy(entry_path, next_destination)
                .map_err(|error| format!("Failed to copy Agent Skill resource: {error}"))?;
        } else {
            return Err(String::from(
                "Managed Agent Skill package entries must be regular files or directories.",
            ));
        }
    }
    Ok(())
}

/// Builds one verified isolated transaction directory path below host-only metadata.
/// 在仅宿主使用的元数据下构建一个经过校验的隔离事务目录路径。
fn prompt_skill_transaction_dir(user_root: &Path) -> Result<PathBuf, String> {
    let metadata_root = user_root.join(VULCAN_SKILL_CATALOG_DIR_NAME);
    ensure_managed_prompt_skill_directory(&metadata_root, "Agent Skills metadata directory")?;
    let transactions_root = metadata_root.join("transactions");
    ensure_managed_prompt_skill_directory(
        &transactions_root,
        "Agent Skills transactions directory",
    )?;
    let transaction_dir = transactions_root.join(next_prompt_skill_transaction_token());
    if fs::symlink_metadata(&transaction_dir).is_ok() {
        return Err(format!(
            "Agent Skills transaction path already exists: {}",
            vulcan_runtime_assets::native_host_path_text_from_path(&transaction_dir)
        ));
    }
    Ok(transaction_dir)
}

/// Ensures one managed prompt-skill directory is regular and not link-like.
/// 确保一个受管提示词技能目录为常规目录且不是链接类路径。
fn ensure_managed_prompt_skill_directory(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata_is_link_like(&metadata) || !metadata.is_dir() {
                return Err(format!(
                    "{label} must be a regular directory and cannot be a symbolic link or Windows reparse point: {}",
                    vulcan_runtime_assets::native_host_path_text_from_path(path)
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|error| {
                format!(
                    "Failed to create {label} `{}`: {error}",
                    vulcan_runtime_assets::native_host_path_text_from_path(path)
                )
            })?;
            let metadata = fs::symlink_metadata(path)
                .map_err(|error| format!("Failed to verify created {label}: {error}"))?;
            if metadata_is_link_like(&metadata) || !metadata.is_dir() {
                return Err(format!(
                    "Created {label} is not a regular directory: {}",
                    vulcan_runtime_assets::native_host_path_text_from_path(path)
                ));
            }
        }
        Err(error) => {
            return Err(format!(
                "Failed to inspect {label} `{}`: {error}",
                vulcan_runtime_assets::native_host_path_text_from_path(path)
            ));
        }
    }
    Ok(())
}

/// Returns whether one managed prompt-skill directory exists after strict inspection.
/// 在严格检查后返回一个受管提示词技能目录是否存在。
fn managed_prompt_skill_directory_exists(path: &Path, label: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link_like(&metadata) || !metadata.is_dir() => Err(format!(
            "{label} must be a regular directory and cannot be a symbolic link or Windows reparse point: {}",
            vulcan_runtime_assets::native_host_path_text_from_path(path)
        )),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "Failed to inspect {label} `{}`: {error}",
            vulcan_runtime_assets::native_host_path_text_from_path(path)
        )),
    }
}

/// Returns whether one managed regular file exists after strict no-link inspection.
/// 在严格无链接检查后返回一个受管常规文件是否存在。
fn managed_prompt_skill_file_exists(path: &Path, label: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link_like(&metadata) || !metadata.is_file() => Err(format!(
            "{label} must be a regular file and cannot be a symbolic link or Windows reparse point: {}",
            vulcan_runtime_assets::native_host_path_text_from_path(path)
        )),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "Failed to inspect {label} `{}`: {error}",
            vulcan_runtime_assets::native_host_path_text_from_path(path)
        )),
    }
}

/// Reads the strict host catalog or creates an empty current-schema document.
/// 读取严格宿主目录或创建空的当前结构文档。
fn read_host_prompt_skill_catalog_strict(
    user_root: &Path,
) -> Result<HostPromptSkillCatalog, String> {
    if !managed_prompt_skill_directory_exists(user_root, "Agent Skills user root")? {
        return Ok(HostPromptSkillCatalog {
            schema_version: VULCAN_SKILL_CATALOG_SCHEMA_VERSION,
            ..HostPromptSkillCatalog::default()
        });
    }
    let directory = user_root.join(VULCAN_SKILL_CATALOG_DIR_NAME);
    if !managed_prompt_skill_directory_exists(&directory, "Agent Skills metadata directory")? {
        return Ok(HostPromptSkillCatalog {
            schema_version: VULCAN_SKILL_CATALOG_SCHEMA_VERSION,
            ..HostPromptSkillCatalog::default()
        });
    }
    let path = directory.join(VULCAN_SKILL_CATALOG_FILE_NAME);
    if !managed_prompt_skill_file_exists(&path, "Agent Skills catalog")? {
        return Ok(HostPromptSkillCatalog {
            schema_version: VULCAN_SKILL_CATALOG_SCHEMA_VERSION,
            ..HostPromptSkillCatalog::default()
        });
    }
    let raw = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read Agent Skills catalog `{}`: {error}",
            vulcan_runtime_assets::native_host_path_text_from_path(&path)
        )
    })?;
    let catalog: HostPromptSkillCatalog = serde_yaml::from_str(&raw).map_err(|error| {
        format!(
            "Failed to parse Agent Skills catalog `{}`: {error}",
            vulcan_runtime_assets::native_host_path_text_from_path(&path)
        )
    })?;
    if catalog.schema_version != VULCAN_SKILL_CATALOG_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported Agent Skills catalog schema version {}.",
            catalog.schema_version
        ));
    }
    Ok(catalog)
}

/// Atomically replaces the strict host catalog file.
/// 原子替换严格宿主目录文件。
fn write_host_prompt_skill_catalog(
    user_root: &Path,
    catalog: &HostPromptSkillCatalog,
) -> Result<(), String> {
    ensure_managed_prompt_skill_directory(user_root, "Agent Skills user root")?;
    let directory = user_root.join(VULCAN_SKILL_CATALOG_DIR_NAME);
    ensure_managed_prompt_skill_directory(&directory, "Agent Skills metadata directory")?;
    let target = directory.join(VULCAN_SKILL_CATALOG_FILE_NAME);
    // An existing catalog must be a regular file; links are never replaced.
    // 已有目录必须是普通文件；链接永不被替换。
    managed_prompt_skill_file_exists(&target, "Agent Skills catalog")?;
    let temporary = directory.join(format!(
        "catalog-{}.yaml.tmp",
        next_prompt_skill_transaction_token()
    ));
    if fs::symlink_metadata(&temporary).is_ok() {
        return Err(format!(
            "Agent Skills catalog transaction path already exists: {}",
            vulcan_runtime_assets::native_host_path_text_from_path(&temporary)
        ));
    }
    let yaml = serde_yaml::to_string(catalog)
        .map_err(|error| format!("Failed to serialize Agent Skills catalog: {error}"))?;
    let mut temporary_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Failed to create Agent Skills catalog transaction: {error}"))?;
    std::io::Write::write_all(&mut temporary_file, yaml.as_bytes())
        .map_err(|error| format!("Failed to write Agent Skills catalog transaction: {error}"))?;
    temporary_file
        .sync_all()
        .map_err(|error| format!("Failed to flush Agent Skills catalog transaction: {error}"))?;
    drop(temporary_file);
    // Rename replaces the catalog atomically on every platform (MoveFileExW with
    // MOVEFILE_REPLACE_EXISTING on Windows), so readers see the old or the new catalog and never
    // a missing one, which would read as empty.
    // 重命名在所有平台上原子替换目录（Windows 上为带 MOVEFILE_REPLACE_EXISTING 的 MoveFileExW），
    // 读取方只会看到旧目录或新目录，绝不会看到缺失（缺失会被读作空目录）。
    if let Err(error) = fs::rename(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Failed to commit Agent Skills catalog: {error}"));
    }
    Ok(())
}

/// Validates one user-defined category against the strict catalog contract.
/// 按严格目录契约校验一个用户定义分类。
fn validate_prompt_skill_category(category: &PromptSkillCategory) -> Result<(), String> {
    validate_prompt_skill_name(
        Path::new(VULCAN_SKILL_CATALOG_FILE_NAME),
        category.category_id.as_str(),
    )
    .map_err(|error| error.message)?;
    if category.name.trim().is_empty() {
        return Err(String::from("Agent Skills category name cannot be empty."));
    }
    if category.emoji.trim().is_empty() || category.color.trim().is_empty() {
        return Err(String::from(
            "Agent Skills category emoji and color are required.",
        ));
    }
    if !valid_hex_color(category.color.trim()) {
        return Err(String::from(
            "Agent Skills category color must be a hexadecimal RGB or RGBA color.",
        ));
    }
    if let Some(locale) = category
        .locales
        .keys()
        .find(|locale| !valid_bcp47_locale_tag(locale))
    {
        return Err(format!(
            "Agent Skills category locale `{locale}` is not a valid BCP 47 language tag."
        ));
    }
    if category
        .locales
        .values()
        .any(|metadata| metadata.name.trim().is_empty())
    {
        return Err(String::from(
            "Every localized Agent Skills category name must be non-empty.",
        ));
    }
    Ok(())
}

/// Loads the strict host-managed logical catalog or records one explicit diagnostic.
/// 加载严格的宿主管理逻辑目录，失败时记录一条明确诊断。
fn load_host_prompt_skill_catalog(
    user_root: &Path,
    errors: &mut Vec<PromptSkillLoadError>,
) -> HostPromptSkillCatalog {
    let user_root_exists =
        match managed_prompt_skill_directory_exists(user_root, "Agent Skills user root") {
            Ok(exists) => exists,
            Err(message) => {
                errors.push(PromptSkillLoadError {
                    path: user_root.to_path_buf(),
                    message,
                });
                return HostPromptSkillCatalog::default();
            }
        };
    if !user_root_exists {
        return HostPromptSkillCatalog {
            schema_version: VULCAN_SKILL_CATALOG_SCHEMA_VERSION,
            ..HostPromptSkillCatalog::default()
        };
    }
    // Resolve the unique host catalog path below the user skill root.
    // 解析用户技能根目录下唯一的宿主目录路径。
    let metadata_dir = user_root.join(VULCAN_SKILL_CATALOG_DIR_NAME);
    let catalog_path = metadata_dir.join(VULCAN_SKILL_CATALOG_FILE_NAME);
    let metadata_dir_exists = match managed_prompt_skill_directory_exists(
        &metadata_dir,
        "Agent Skills metadata directory",
    ) {
        Ok(exists) => exists,
        Err(message) => {
            errors.push(PromptSkillLoadError {
                path: metadata_dir,
                message,
            });
            return HostPromptSkillCatalog::default();
        }
    };
    if !metadata_dir_exists {
        return HostPromptSkillCatalog {
            schema_version: VULCAN_SKILL_CATALOG_SCHEMA_VERSION,
            ..HostPromptSkillCatalog::default()
        };
    }
    let catalog_exists =
        match managed_prompt_skill_file_exists(&catalog_path, "Agent Skills catalog") {
            Ok(exists) => exists,
            Err(message) => {
                errors.push(PromptSkillLoadError {
                    path: catalog_path,
                    message,
                });
                return HostPromptSkillCatalog::default();
            }
        };
    if !catalog_exists {
        return HostPromptSkillCatalog {
            schema_version: VULCAN_SKILL_CATALOG_SCHEMA_VERSION,
            ..HostPromptSkillCatalog::default()
        };
    }
    // Read the host catalog as UTF-8 YAML.
    // 以 UTF-8 YAML 读取宿主目录。
    let raw = match fs::read_to_string(&catalog_path) {
        Ok(raw) => raw,
        Err(error) => {
            errors.push(PromptSkillLoadError {
                path: catalog_path,
                message: format!("Failed to read Agent Skills catalog metadata: {error}"),
            });
            return HostPromptSkillCatalog::default();
        }
    };
    // Parse only the current strict host catalog schema.
    // 仅解析当前严格宿主目录结构。
    let parsed = match serde_yaml::from_str::<HostPromptSkillCatalog>(&raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            errors.push(PromptSkillLoadError {
                path: catalog_path,
                message: format!("Failed to parse Agent Skills catalog metadata: {error}"),
            });
            return HostPromptSkillCatalog::default();
        }
    };
    if parsed.schema_version != VULCAN_SKILL_CATALOG_SCHEMA_VERSION {
        errors.push(PromptSkillLoadError {
            path: catalog_path,
            message: format!(
                "Agent Skills catalog schema_version must equal {}.",
                VULCAN_SKILL_CATALOG_SCHEMA_VERSION
            ),
        });
        return HostPromptSkillCatalog::default();
    }
    parsed
}

/// Merges strict user categories after immutable built-in categories.
/// 在不可变内置分类之后合并严格用户分类。
fn merge_user_prompt_skill_categories(
    categories: &mut Vec<PromptSkillCategory>,
    user_categories: Vec<PromptSkillCategory>,
    user_root: &Path,
    errors: &mut Vec<PromptSkillLoadError>,
) {
    // Track every stable category identifier already admitted.
    // 跟踪每个已经接纳的稳定分类标识。
    let mut category_ids = categories
        .iter()
        .map(|category| category.category_id.clone())
        .collect::<BTreeSet<_>>();
    for mut category in user_categories {
        category.category_id = category.category_id.trim().to_string();
        let validation_error = validate_prompt_skill_category(&category).err();
        if validation_error.is_some() || !category_ids.insert(category.category_id.clone()) {
            errors.push(PromptSkillLoadError {
                path: user_root
                    .join(VULCAN_SKILL_CATALOG_DIR_NAME)
                    .join(VULCAN_SKILL_CATALOG_FILE_NAME),
                message: validation_error.unwrap_or_else(|| {
                    format!(
                        "User Agent Skills category id `{}` duplicates an existing category.",
                        category.category_id
                    )
                }),
            });
            continue;
        }
        category.builtin = false;
        categories.push(category);
    }
    categories.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then_with(|| left.category_id.cmp(&right.category_id))
    });
}

/// Applies host-managed overrides to the effective source record for each standard name.
/// 将宿主管理覆盖应用到每个标准名称的有效来源记录。
fn apply_host_prompt_skill_overrides(
    overrides: &BTreeMap<String, HostPromptSkillOverride>,
    system_by_name: &mut BTreeMap<String, PromptSkill>,
    user_by_name: &mut BTreeMap<String, PromptSkill>,
    errors: &mut Vec<PromptSkillLoadError>,
) {
    for (name, metadata_override) in overrides {
        // Select the same deterministic effective source used by runtime resolution.
        // 选择与运行时解析一致的确定性有效来源。
        let skill = user_by_name
            .get_mut(name)
            .or_else(|| system_by_name.get_mut(name));
        let Some(skill) = skill else {
            errors.push(PromptSkillLoadError {
                path: PathBuf::from(name),
                message: format!(
                    "Agent Skills catalog override references missing skill `{name}`."
                ),
            });
            continue;
        };
        if let Some(interface) = metadata_override.interface.clone() {
            skill.interface = interface;
        }
        if let Some(category_id) = metadata_override.category_id.as_deref() {
            skill.catalog.category_id = category_id.trim().to_string();
        }
        if let Some(tags) = metadata_override.tags.clone() {
            skill.catalog.tags = normalized_unique_strings(tags);
        }
        if let Some(origin) = metadata_override.origin.clone() {
            skill.origin = origin;
        }
    }
}

/// Reassigns invalid logical categories to the deterministic uncategorized category.
/// 将无效逻辑分类重新归入确定性的未分配目录。
fn normalize_prompt_skill_categories<'a>(
    skills: impl Iterator<Item = &'a mut PromptSkill>,
    category_ids: &BTreeSet<String>,
    errors: &mut Vec<PromptSkillLoadError>,
) {
    for skill in skills {
        if category_ids.contains(&skill.catalog.category_id) {
            continue;
        }
        errors.push(PromptSkillLoadError {
            path: skill.base_dir.clone(),
            message: format!(
                "Skill `{}` references missing category `{}` and was assigned to `uncategorized`.",
                skill.name, skill.catalog.category_id
            ),
        });
        skill.catalog.category_id = String::from("uncategorized");
    }
}

/// Returns whether one logical category identifier follows the stable id contract.
/// 返回一个逻辑分类标识是否符合稳定标识契约。
fn valid_category_id(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Scans one direct-child prompt skill root and records non-fatal load errors.
/// 扫描单个直接子级提示词技能根目录并记录非致命加载错误。
fn scan_prompt_skill_root(
    root: &Path,
    scope: PromptSkillScope,
    by_name: &mut BTreeMap<String, PromptSkill>,
    errors: &mut Vec<PromptSkillLoadError>,
) {
    if root.as_os_str().is_empty() {
        return;
    }
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            errors.push(PromptSkillLoadError {
                path: root.to_path_buf(),
                message: format!("Failed to inspect prompt skill root: {error}"),
            });
            return;
        }
    };
    if metadata_is_link_like(&root_metadata) || !root_metadata.is_dir() {
        errors.push(PromptSkillLoadError {
            path: root.to_path_buf(),
            message: String::from(
                "Prompt skill root must be a regular directory and cannot be a symbolic link or Windows reparse point.",
            ),
        });
        return;
    }

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(PromptSkillLoadError {
                path: root.to_path_buf(),
                message: format!("Failed to read prompt skill root: {error}"),
            });
            return;
        }
    };

    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(PromptSkillLoadError {
                    path: root.to_path_buf(),
                    message: format!("Failed to enumerate prompt skill root entry: {error}"),
                });
                continue;
            }
        };
        let skill_dir = entry.path();
        let entry_metadata = match fs::symlink_metadata(&skill_dir) {
            Ok(metadata) => metadata,
            Err(error) => {
                errors.push(PromptSkillLoadError {
                    path: skill_dir,
                    message: format!("Failed to inspect prompt skill directory entry: {error}"),
                });
                continue;
            }
        };
        if metadata_is_link_like(&entry_metadata) {
            errors.push(PromptSkillLoadError {
                path: skill_dir,
                message: String::from(
                    "Symbolic links and Windows reparse points are not permitted as prompt skill directories.",
                ),
            });
            continue;
        }
        if !entry_metadata.is_dir() {
            continue;
        }
        if scope == PromptSkillScope::User
            && skill_dir
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == SYSTEM_SKILL_DIR_NAME)
        {
            continue;
        }
        let skill_file = skill_dir.join(SKILL_FILE_NAME);
        if !skill_file.is_file() {
            continue;
        }
        match parse_prompt_skill_file_with_scope(&skill_file, scope) {
            Ok(skill) => {
                by_name.insert(skill.name.clone(), skill);
            }
            Err(error) => errors.push(error),
        }
    }
}

/// Parses one SKILL.md file into a prompt skill.
/// 将单个 SKILL.md 文件解析为提示词技能。
pub fn parse_prompt_skill_file(skill_file: &Path) -> Result<PromptSkill, PromptSkillLoadError> {
    parse_prompt_skill_file_with_scope(skill_file, PromptSkillScope::User)
}

/// Parses one SKILL.md file into a prompt skill with an explicit source scope.
/// 使用显式来源作用域将单个 SKILL.md 文件解析为提示词技能。
pub fn parse_prompt_skill_file_with_scope(
    skill_file: &Path,
    scope: PromptSkillScope,
) -> Result<PromptSkill, PromptSkillLoadError> {
    // Inspect the instruction file without following package-controlled links.
    // 在不跟随技能包可控链接的情况下检查指令文件。
    let skill_metadata =
        fs::symlink_metadata(skill_file).map_err(|error| PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: format!("Failed to inspect SKILL.md: {error}"),
        })?;
    if metadata_is_link_like(&skill_metadata) || !skill_metadata.is_file() {
        return Err(PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: String::from(
                "SKILL.md must be a regular file and cannot be a symbolic link or Windows reparse point.",
            ),
        });
    }
    let raw_bytes = fs::read(skill_file).map_err(|error| PromptSkillLoadError {
        path: skill_file.to_path_buf(),
        message: format!("Failed to read SKILL.md: {error}"),
    })?;
    let raw = decode_complete_user_text(&raw_bytes).map_err(|error| PromptSkillLoadError {
        path: skill_file.to_path_buf(),
        message: format!("Failed to decode SKILL.md as UTF-8 or GB18030: {error}"),
    })?;
    parse_prompt_skill_text_with_scope(skill_file, &raw.content, scope)
}

/// Parses one prompt skill markdown string with YAML frontmatter.
/// 解析一段带 YAML frontmatter 的提示词技能 Markdown 文本。
pub fn parse_prompt_skill_text(
    skill_file: &Path,
    raw: &str,
) -> Result<PromptSkill, PromptSkillLoadError> {
    parse_prompt_skill_text_with_scope(skill_file, raw, PromptSkillScope::User)
}

/// Parses one prompt skill markdown string with YAML frontmatter and source scope.
/// 解析一段带 YAML frontmatter 与来源作用域的提示词技能 Markdown 文本。
pub fn parse_prompt_skill_text_with_scope(
    skill_file: &Path,
    raw: &str,
    scope: PromptSkillScope,
) -> Result<PromptSkill, PromptSkillLoadError> {
    // Split the standard frontmatter from the model-facing instruction body.
    // 从面向模型的说明正文中拆分标准 frontmatter。
    let (frontmatter, body) = split_frontmatter(raw).ok_or_else(|| PromptSkillLoadError {
        path: skill_file.to_path_buf(),
        message: String::from("SKILL.md must start with YAML frontmatter."),
    })?;
    // Parse the complete standard metadata contract without private top-level fields.
    // 按完整标准元数据契约解析，禁止私有顶层字段。
    let parsed: PromptSkillFrontmatter =
        serde_yaml::from_str(frontmatter).map_err(|error| PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: format!("Failed to parse skill frontmatter: {error}"),
        })?;
    // Reject non-canonical whitespace instead of silently rewriting package identity.
    // 拒绝非规范空白，不静默改写技能包身份。
    if parsed.name != parsed.name.trim() {
        return Err(PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: String::from(
                "Skill frontmatter field `name` must already be canonical and cannot contain surrounding whitespace.",
            ),
        });
    }
    // Validate the exact canonical model-facing skill name.
    // 校验精确的面向模型规范技能名称。
    let name = parsed.name;
    validate_prompt_skill_name(skill_file, &name)?;
    // Resolve the owning directory required by the Agent Skills standard.
    // 解析 Agent Skills 标准要求的所属目录。
    let base_dir =
        skill_file
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| PromptSkillLoadError {
                path: skill_file.to_path_buf(),
                message: String::from("SKILL.md must belong to a named skill directory."),
            })?;
    // Read the exact parent directory name for the standard identity check.
    // 读取用于标准身份校验的精确父目录名称。
    let directory_name = base_dir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PromptSkillLoadError {
            path: base_dir.clone(),
            message: String::from("Skill directory name must be valid UTF-8."),
        })?;
    if directory_name != name {
        return Err(PromptSkillLoadError {
            path: base_dir.clone(),
            message: format!(
                "Skill frontmatter field `name` must match parent directory `{directory_name}`."
            ),
        });
    }
    // Normalize and validate the model-facing trigger description.
    // 规范化并校验面向模型的触发说明。
    let description = parsed.description.trim().to_string();
    if description.is_empty() || description.chars().count() > 1024 {
        return Err(PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: String::from(
                "Skill frontmatter field `description` must contain 1 to 1024 characters.",
            ),
        });
    }
    // Normalize the optional standard compatibility statement.
    // 规范化可选标准兼容性说明。
    let compatibility = normalize_optional_string(parsed.compatibility);
    if compatibility
        .as_ref()
        .is_some_and(|value| value.chars().count() > 500)
    {
        return Err(PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: String::from(
                "Skill frontmatter field `compatibility` must not exceed 500 characters.",
            ),
        });
    }
    // Validate the complete package and freeze its resource summary before reading optional metadata.
    // 在读取可选元数据前校验完整技能包并冻结资源摘要。
    let resources = prompt_skill_resource_summary(&base_dir)?;
    // Load the single strict Vulcan metadata schema when the package declares it.
    // 在技能包声明时加载唯一严格的 Vulcan 元数据结构。
    let package_metadata = load_prompt_skill_metadata(&base_dir)?;
    // Resolve the interface metadata without retaining an optional legacy shape.
    // 解析界面元数据，不保留可选旧结构。
    let interface = package_metadata
        .as_ref()
        .map(|metadata| metadata.interface.clone().into_metadata())
        .unwrap_or_default();
    // Resolve explicit logical catalog metadata or the deterministic fallback category.
    // 解析显式逻辑目录元数据或确定性回退分类。
    let mut catalog = package_metadata
        .as_ref()
        .map(|metadata| metadata.catalog.clone())
        .unwrap_or_default();
    if catalog.category_id.trim().is_empty() {
        catalog.category_id = String::from("uncategorized");
    }
    catalog.tags = normalized_unique_strings(catalog.tags);
    // Resolve package origin or a deterministic scope-derived origin.
    // 解析技能包来源或由作用域确定的默认来源。
    let mut origin = package_metadata
        .map(|metadata| metadata.origin)
        .unwrap_or_default();
    if origin.kind.trim().is_empty() {
        origin.kind = match scope {
            PromptSkillScope::System => String::from("builtin"),
            PromptSkillScope::User => String::from("unknown"),
            PromptSkillScope::Plugin => String::from("plugin"),
        };
    }
    Ok(PromptSkill {
        name,
        description,
        license: normalize_optional_string(parsed.license),
        compatibility,
        metadata: parsed.metadata,
        skill_file: skill_file.to_path_buf(),
        base_dir,
        scope,
        interface,
        catalog,
        origin,
        resources,
        body: body.trim().to_string(),
    })
}

/// Loads optional strict Vulcan metadata from root-level `vulcan.yaml`.
/// 从根目录 `vulcan.yaml` 载入可选严格 Vulcan 元数据。
fn load_prompt_skill_metadata(
    base_dir: &Path,
) -> Result<Option<PromptSkillAgentMetadata>, PromptSkillLoadError> {
    // Build the only supported package-local Vulcan metadata path.
    // 构建唯一支持的技能包内 Vulcan 元数据路径。
    let metadata_path = base_dir.join(VULCAN_SKILL_METADATA_FILE_NAME);
    let metadata = match fs::symlink_metadata(&metadata_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(PromptSkillLoadError {
                path: metadata_path,
                message: format!("Failed to inspect Vulcan skill metadata: {error}"),
            });
        }
    };
    if metadata_is_link_like(&metadata) || !metadata.is_file() {
        return Err(PromptSkillLoadError {
            path: metadata_path,
            message: String::from(
                "Vulcan skill metadata must be a regular file and cannot be a symbolic link or Windows reparse point.",
            ),
        });
    }
    parse_prompt_skill_metadata_file(&metadata_path).map(Some)
}

/// Parses one strict package-local Vulcan metadata file.
/// 解析单个严格的技能包内 Vulcan 元数据文件。
fn parse_prompt_skill_metadata_file(
    metadata_path: &Path,
) -> Result<PromptSkillAgentMetadata, PromptSkillLoadError> {
    // Read the package-local metadata as UTF-8 YAML.
    // 以 UTF-8 YAML 读取技能包内元数据。
    let raw = fs::read_to_string(metadata_path).map_err(|error| PromptSkillLoadError {
        path: metadata_path.to_path_buf(),
        message: format!("Failed to read Vulcan skill metadata: {error}"),
    })?;
    // Parse only the current strict metadata schema.
    // 仅解析当前严格元数据结构。
    let parsed: PromptSkillAgentMetadata =
        serde_yaml::from_str(&raw).map_err(|error| PromptSkillLoadError {
            path: metadata_path.to_path_buf(),
            message: format!("Failed to parse Vulcan skill metadata: {error}"),
        })?;
    if parsed.schema_version != VULCAN_SKILL_METADATA_SCHEMA_VERSION {
        return Err(PromptSkillLoadError {
            path: metadata_path.to_path_buf(),
            message: format!(
                "Vulcan skill metadata schema_version must equal {}.",
                VULCAN_SKILL_METADATA_SCHEMA_VERSION
            ),
        });
    }
    validate_prompt_skill_interface_metadata(&parsed.interface, metadata_path)?;
    if !parsed.catalog.category_id.trim().is_empty()
        && !valid_category_id(parsed.catalog.category_id.trim())
    {
        return Err(PromptSkillLoadError {
            path: metadata_path.to_path_buf(),
            message: String::from(
                "Vulcan skill metadata catalog.category_id must be a canonical lowercase identifier.",
            ),
        });
    }
    Ok(parsed)
}

/// Validates strict interface localization and marker fields.
/// 校验严格界面本地化与标记字段。
fn validate_prompt_skill_interface_metadata(
    interface: &PromptSkillInterfaceMetadataRaw,
    metadata_path: &Path,
) -> Result<(), PromptSkillLoadError> {
    for locale in interface.locales.keys() {
        if !valid_bcp47_locale_tag(locale) {
            return Err(PromptSkillLoadError {
                path: metadata_path.to_path_buf(),
                message: format!(
                    "Vulcan skill metadata locale `{locale}` is not a valid BCP 47 language tag."
                ),
            });
        }
    }
    if interface
        .emoji
        .as_deref()
        .is_some_and(|emoji| emoji.trim().is_empty())
    {
        return Err(PromptSkillLoadError {
            path: metadata_path.to_path_buf(),
            message: String::from("Vulcan skill metadata interface.emoji cannot be empty."),
        });
    }
    if let Some(color) = interface.color.as_deref()
        && !valid_hex_color(color)
    {
        return Err(PromptSkillLoadError {
            path: metadata_path.to_path_buf(),
            message: String::from(
                "Vulcan skill metadata interface.color must be a hexadecimal RGB or RGBA color.",
            ),
        });
    }
    Ok(())
}

/// Returns whether one locale tag follows the structural BCP 47 subtag grammar.
/// 返回一个语言标签是否遵循 BCP 47 子标签结构语法。
fn valid_bcp47_locale_tag(value: &str) -> bool {
    let mut subtags = value.split('-');
    let Some(language) = subtags.next() else {
        return false;
    };
    if !(2..=8).contains(&language.len())
        || !language.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    subtags.all(|subtag| {
        (1..=8).contains(&subtag.len()) && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

/// Returns whether one color uses the supported hexadecimal RGB or RGBA notation.
/// 返回一个颜色是否使用受支持的十六进制 RGB 或 RGBA 表示法。
fn valid_hex_color(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('#') else {
        return false;
    };
    matches!(digits.len(), 3 | 4 | 6 | 8) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Resolves prompt skills through global, project, and session state.
/// 通过全局、项目与会话状态解析提示词技能。
pub fn resolve_prompt_skills(
    catalog: &PromptSkillCatalog,
    global: &PromptSkillsGlobalConfig,
    project: &PromptSkillsProjectConfig,
    session: &PromptSkillsSessionConfig,
) -> Vec<ResolvedPromptSkill> {
    catalog
        .skills
        .iter()
        .cloned()
        .map(|skill| {
            // Enabled plugin skills enter this catalog only through the committed plugin snapshot.
            // 已启用插件技能只会通过已提交插件快照进入当前目录。
            let (state, source) = if skill.scope == PromptSkillScope::Plugin {
                (PromptSkillState::Enabled, PromptSkillStateSource::Plugin)
            } else {
                resolve_prompt_skill_state(&skill.name, global, project, session)
            };
            ResolvedPromptSkill {
                skill,
                state,
                source,
            }
        })
        .collect()
}

/// Resolves the effective state of one prompt skill.
/// 解析单个提示词技能的生效状态。
pub fn resolve_prompt_skill_state(
    name: &str,
    global: &PromptSkillsGlobalConfig,
    project: &PromptSkillsProjectConfig,
    session: &PromptSkillsSessionConfig,
) -> (PromptSkillState, PromptSkillStateSource) {
    if let Some(state) = session.skill_overrides.get(name).copied() {
        return (state, PromptSkillStateSource::Session);
    }
    if let Some(state) = project.skill_overrides.get(name).copied() {
        return (state, PromptSkillStateSource::Project);
    }
    if let Some(state) = global.skill_overrides.get(name).copied() {
        return (state, PromptSkillStateSource::GlobalOverride);
    }
    (
        global.default_skill_state,
        PromptSkillStateSource::GlobalDefault,
    )
}

/// Renders enabled prompt skills into a compact developer instruction block.
/// 将启用的提示词技能渲染为紧凑的开发者指令块。
pub fn render_prompt_skills_instructions(skills: &[ResolvedPromptSkill]) -> Option<String> {
    let enabled = skills
        .iter()
        .filter(|skill| skill.is_enabled() && skill.skill.has_description())
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return None;
    }

    let mut lines = vec![
        String::from("<skills_instructions>"),
        String::from("## Skills"),
        String::from(
            "A skill is a local SKILL.md instruction package. Use the `skill` tool to load full instructions only when the task matches a listed skill.",
        ),
        String::from("### Available skills"),
    ];
    for resolved in enabled {
        lines.push(format!(
            "- {}: {} (file: {})",
            resolved.skill.name,
            collapse_whitespace(&resolved.skill.description),
            vulcan_runtime_assets::native_host_path_text_from_path(&resolved.skill.skill_file)
        ));
    }
    lines.extend([
        String::from("### How to use skills"),
        String::from("- Match the user's task against the skill descriptions before loading a skill."),
        String::from("- Call `skill` with the exact skill name to load its full SKILL.md content and resource directory."),
        String::from("- Do not assume scripts or references until the skill content or its base directory has been loaded."),
        String::from("</skills_instructions>"),
    ]);
    Some(lines.join("\n"))
}

/// Renders one prompt skill as the output of the native skill tool.
/// 将单个提示词技能渲染为原生 skill 工具输出。
pub fn render_skill_tool_output(skill: &PromptSkill, sampled_files: &[PathBuf]) -> String {
    let base_dir = vulcan_runtime_assets::native_host_path_text_from_path(&skill.base_dir);
    let mut lines = vec![
        format!("<skill_content name=\"{}\">", xml_escape(&skill.name)),
        format!("# Skill: {}", skill.name),
        format!("Scope: {}", skill.scope.en_label()),
        String::new(),
        skill.body.trim().to_string(),
        String::new(),
        format!("Base directory for this skill: {base_dir}"),
        String::from(
            "Relative paths in this skill, such as scripts/, references/, or templates/, are relative to this base directory.",
        ),
        String::from("Note: file list is sampled."),
        String::new(),
        String::from("<skill_files>"),
    ];
    for file in sampled_files {
        lines.push(format!(
            "<file>{}</file>",
            xml_escape(&vulcan_runtime_assets::native_host_path_text_from_path(
                file,
            ))
        ));
    }
    lines.extend([
        String::from("</skill_files>"),
        String::from("</skill_content>"),
    ]);
    lines.join("\n")
}

/// Returns sorted enabled skill names for diagnostics and native tool errors.
/// 返回排序后的启用技能名，用于诊断与原生工具错误。
pub fn enabled_skill_names(skills: &[ResolvedPromptSkill]) -> Vec<String> {
    skills
        .iter()
        .filter(|skill| skill.is_enabled())
        .map(|skill| skill.skill.name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Host-managed logical Agent Skills catalog persisted outside skill packages.
/// 持久化在技能包之外的宿主管理逻辑 Agent Skills 目录。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostPromptSkillCatalog {
    /// Required host catalog schema version.
    /// 必填宿主目录结构版本。
    schema_version: u32,
    /// User-defined logical categories.
    /// 用户定义的逻辑分类。
    #[serde(default)]
    categories: Vec<PromptSkillCategory>,
    /// Per-skill display, catalog, and provenance overrides keyed by standard name.
    /// 按标准名称索引的单技能展示、目录与来源覆盖。
    #[serde(default)]
    skills: BTreeMap<String, HostPromptSkillOverride>,
}

/// Host-managed metadata override for one effective skill.
/// 单个有效技能的宿主管理元数据覆盖。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostPromptSkillOverride {
    /// Optional complete interface metadata replacement.
    /// 可选完整界面元数据替换。
    interface: Option<PromptSkillInterfaceMetadata>,
    /// Optional primary category replacement.
    /// 可选主分类替换。
    category_id: Option<String>,
    /// Optional complete searchable tag replacement.
    /// 可选完整搜索标签替换。
    tags: Option<Vec<String>>,
    /// Optional complete auditable origin replacement.
    /// 可选完整可审计来源替换。
    origin: Option<PromptSkillOriginMetadata>,
}

/// YAML frontmatter shape supported by prompt skills.
/// 提示词技能支持的 YAML frontmatter 结构。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptSkillFrontmatter {
    /// Stable skill name.
    /// 稳定技能名称。
    name: String,
    /// Model-facing skill description.
    /// 面向模型的技能描述。
    description: String,
    /// Optional license identifier or bundled file reference.
    /// 可选协议标识或包内文件引用。
    license: Option<String>,
    /// Optional environment compatibility statement.
    /// 可选环境兼容性说明。
    compatibility: Option<String>,
    /// Standard string extension metadata.
    /// 标准字符串扩展元数据。
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

/// YAML shape for one skill agent metadata file.
/// 单个技能 agent 元数据文件的 YAML 结构。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptSkillAgentMetadata {
    /// Required Vulcan metadata schema version.
    /// 必填 Vulcan 元数据结构版本。
    schema_version: u32,
    /// UI-facing interface metadata block.
    /// 面向界面的展示元数据块。
    #[serde(default)]
    interface: PromptSkillInterfaceMetadataRaw,
    /// Logical catalog metadata block.
    /// 逻辑目录元数据块。
    #[serde(default)]
    catalog: PromptSkillCatalogMetadata,
    /// Auditable origin metadata block.
    /// 可审计来源元数据块。
    #[serde(default)]
    origin: PromptSkillOriginMetadata,
}

/// Raw optional interface metadata fields from YAML.
/// 从 YAML 读取的原始可选界面元数据字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptSkillInterfaceMetadataRaw {
    /// Optional human-facing display name.
    /// 可选人类可读展示名称。
    display_name: Option<String>,
    /// Optional short human-facing description.
    /// 可选人类可读短描述。
    short_description: Option<String>,
    /// Optional emoji rendered by the skill marker.
    /// 由技能标记渲染的可选表情图标。
    emoji: Option<String>,
    /// Optional hexadecimal marker color.
    /// 可选十六进制标记颜色。
    color: Option<String>,
    /// Localized display metadata.
    /// 本地化展示元数据。
    #[serde(default)]
    locales: BTreeMap<String, PromptSkillLocaleMetadata>,
}

impl PromptSkillInterfaceMetadataRaw {
    /// Converts raw YAML fields into normalized interface metadata.
    /// 将原始 YAML 字段转换为规范化界面元数据。
    fn into_metadata(self) -> PromptSkillInterfaceMetadata {
        PromptSkillInterfaceMetadata {
            display_name: normalize_optional_string(self.display_name),
            short_description: normalize_optional_string(self.short_description),
            emoji: normalize_optional_string(self.emoji),
            color: normalize_optional_string(self.color),
            locales: self.locales,
        }
    }
}

/// Validates one canonical Agent Skills name against the open specification.
/// 按开放规范校验一个 Agent Skills 规范名称。
fn validate_prompt_skill_name(skill_file: &Path, name: &str) -> Result<(), PromptSkillLoadError> {
    // Count Unicode scalar values even though the accepted character set is ASCII.
    // 即使允许字符集为 ASCII，仍按 Unicode 标量值统计长度。
    let length = name.chars().count();
    // Check the complete canonical name contract in one deterministic predicate.
    // 使用单个确定性断言检查完整规范名称契约。
    let valid = (1..=64).contains(&length)
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-');
    if valid {
        return Ok(());
    }
    Err(PromptSkillLoadError {
        path: skill_file.to_path_buf(),
        message: String::from(
            "Skill frontmatter field `name` must contain 1 to 64 lowercase ASCII letters, digits, or single hyphens and must not start or end with a hyphen.",
        ),
    })
}

/// Returns stable trimmed unique strings while preserving first occurrence order.
/// 返回保持首次出现顺序的稳定裁剪去重字符串。
fn normalized_unique_strings(values: Vec<String>) -> Vec<String> {
    // Track normalized values already emitted by this pass.
    // 跟踪本次处理中已经输出的规范值。
    let mut seen = BTreeSet::<String>::new();
    // Retain meaningful normalized values only once.
    // 仅保留一次有意义的规范值。
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && seen.insert(value.clone()))
        .collect()
}

/// Maximum number of frozen support-file paths exposed in one skill detail response.
/// 单个技能详情响应中暴露的冻结支持文件路径最大数量。
const PROMPT_SKILL_RESOURCE_SAMPLE_LIMIT: usize = 12;

/// Inspects one complete skill package and freezes its resource summary without following links.
/// 检查一个完整技能包并在不跟随链接的情况下冻结其资源摘要。
fn prompt_skill_resource_summary(
    base_dir: &Path,
) -> Result<PromptSkillResourceSummary, PromptSkillLoadError> {
    let base_metadata = match fs::symlink_metadata(base_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PromptSkillResourceSummary::default());
        }
        Err(error) => {
            return Err(PromptSkillLoadError {
                path: base_dir.to_path_buf(),
                message: format!("Failed to inspect prompt skill package: {error}"),
            });
        }
    };
    if metadata_is_link_like(&base_metadata) || !base_metadata.is_dir() {
        return Err(PromptSkillLoadError {
            path: base_dir.to_path_buf(),
            message: String::from(
                "Prompt skill package must be a regular directory and cannot be a symbolic link or Windows reparse point.",
            ),
        });
    }
    let mut summary = PromptSkillResourceSummary::default();
    collect_prompt_skill_resources(base_dir, base_dir, &mut summary)?;
    summary.sampled_files.sort();
    summary
        .sampled_files
        .truncate(PROMPT_SKILL_RESOURCE_SAMPLE_LIMIT);
    Ok(summary)
}

/// Recursively validates package entries and accumulates counts plus package-relative samples.
/// 递归校验技能包目录项并累计数量及包内相对路径样本。
fn collect_prompt_skill_resources(
    base_dir: &Path,
    current_dir: &Path,
    summary: &mut PromptSkillResourceSummary,
) -> Result<(), PromptSkillLoadError> {
    let entries = fs::read_dir(current_dir).map_err(|error| PromptSkillLoadError {
        path: current_dir.to_path_buf(),
        message: format!("Failed to read prompt skill package directory: {error}"),
    })?;
    let mut entries = entries
        .map(|entry| {
            entry.map_err(|error| PromptSkillLoadError {
                path: current_dir.to_path_buf(),
                message: format!("Failed to enumerate prompt skill package entry: {error}"),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| PromptSkillLoadError {
            path: path.clone(),
            message: format!("Failed to inspect prompt skill package entry: {error}"),
        })?;
        if metadata_is_link_like(&metadata) {
            return Err(PromptSkillLoadError {
                path,
                message: String::from(
                    "Symbolic links and Windows reparse points are not permitted in prompt skill packages.",
                ),
            });
        }
        if metadata.is_dir() {
            collect_prompt_skill_resources(base_dir, &path, summary)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(PromptSkillLoadError {
                path,
                message: String::from(
                    "Prompt skill package entries must be regular files or directories.",
                ),
            });
        }
        let relative = path
            .strip_prefix(base_dir)
            .map(Path::to_path_buf)
            .map_err(|error| PromptSkillLoadError {
                path: path.clone(),
                message: format!("Prompt skill resource escaped its package root: {error}"),
            })?;
        match relative
            .components()
            .next()
            .and_then(|component| match component {
                std::path::Component::Normal(value) => value.to_str(),
                _ => None,
            }) {
            Some("scripts") => summary.scripts += 1,
            Some("references") => summary.references += 1,
            Some("assets") => summary.assets += 1,
            _ => {}
        }
        if relative != Path::new(SKILL_FILE_NAME) && prompt_skill_resource_is_visible(&relative) {
            summary.sampled_files.push(relative);
        }
    }
    Ok(())
}

/// Returns whether one package-relative resource path belongs in the model-facing sample.
/// 返回一个包内相对资源路径是否应进入面向模型的样本。
fn prompt_skill_resource_is_visible(relative: &Path) -> bool {
    relative.components().all(|component| match component {
        std::path::Component::Normal(value) => value
            .to_str()
            .is_some_and(|name| !name.starts_with('.') && !name.eq_ignore_ascii_case("target")),
        _ => false,
    })
}

/// Returns whether metadata represents a symbolic link or a Windows reparse point.
/// 返回元数据是否表示符号链接或 Windows 重解析点。
fn metadata_is_link_like(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || metadata_is_windows_reparse_point(metadata)
}

/// Returns whether metadata carries the Windows reparse-point attribute.
/// 返回元数据是否携带 Windows 重解析点属性。
#[cfg(windows)]
fn metadata_is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Reports no Windows reparse point on non-Windows platforms.
/// 在非 Windows 平台上报告不存在 Windows 重解析点。
#[cfg(not(windows))]
fn metadata_is_windows_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

/// Splits YAML frontmatter from a markdown file.
/// 从 Markdown 文件中拆分 YAML frontmatter。
fn split_frontmatter(raw: &str) -> Option<(&str, &str)> {
    let normalized = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let rest = normalized.strip_prefix("---")?;
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;
    let marker = rest.find("\n---")?;
    let frontmatter = &rest[..marker];
    let after_marker = &rest[marker + 4..];
    let body = after_marker
        .strip_prefix("\r\n")
        .or_else(|| after_marker.strip_prefix('\n'))
        .unwrap_or(after_marker);
    Some((frontmatter, body))
}

/// Collapses repeated whitespace for compact prompt rendering.
/// 折叠重复空白以便紧凑渲染提示词。
fn collapse_whitespace(text: &str) -> String {
    let mut collapsed = String::new();
    for word in text.split_whitespace() {
        if !collapsed.is_empty() {
            collapsed.push(' ');
        }
        collapsed.push_str(word);
    }
    collapsed
}

/// Trims one optional string and drops it when empty.
/// 修剪一个可选字符串并在为空时丢弃。
fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Escapes minimal XML text entities used by tool output wrappers.
/// 转义工具输出包装中使用的最小 XML 文本实体。
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
