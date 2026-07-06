//! System and user prompt skill discovery, installation, policy resolution, and rendering.
//! 系统级与用户级提示词技能发现、安装、策略解析与渲染。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

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
pub const DEFAULT_USER_SKILL_DIR: &str = ".vulcan/skills";

/// Directory name that stores host-managed system prompt skills.
/// 存放宿主管理系统提示词技能的目录名。
pub const SYSTEM_SKILL_DIR_NAME: &str = ".system";

/// VulcanCode-specific skill interface metadata file name in one skill root.
/// 单个技能根目录下的 VulcanCode 专用技能界面元数据文件名。
pub const VULCAN_SKILL_METADATA_FILE_NAME: &str = "vulcan.yaml";

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
}

impl PromptSkillScope {
    /// Returns a compact Chinese label for WebUI display.
    /// 返回用于界面展示的紧凑中文标签。
    pub const fn zh_label(self) -> &'static str {
        match self {
            Self::System => "系统",
            Self::User => "用户",
        }
    }

    /// Returns a compact English label for diagnostics.
    /// 返回用于诊断信息的紧凑英文标签。
    pub const fn en_label(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
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
    pub interface: Option<PromptSkillInterfaceMetadata>,
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
}

/// Optional UI-facing metadata for one prompt skill.
/// 单个提示词技能的可选界面展示元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillInterfaceMetadata {
    /// Human-facing display name shown in skill lists.
    /// 技能列表中显示的人类可读名称。
    pub display_name: Option<String>,
    /// Short human-facing description shown in compact cards.
    /// 紧凑卡片中显示的人类可读短描述。
    pub short_description: Option<String>,
    /// Optional small icon path relative to the skill directory.
    /// 相对于技能目录的可选小图标路径。
    pub icon_small: Option<PathBuf>,
    /// Optional large icon path relative to the skill directory.
    /// 相对于技能目录的可选大图标路径。
    pub icon_large: Option<PathBuf>,
    /// Optional brand color string for future UI styling.
    /// 供后续界面样式使用的可选品牌色字符串。
    pub brand_color: Option<String>,
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
}

/// Global prompt skills configuration.
/// 全局提示词技能配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSkillsGlobalConfig {
    /// Whether prompt skills are globally enabled.
    /// 是否全局启用提示词技能。
    pub enabled: bool,
    /// Optional user skill root override.
    /// 可选用户技能根目录覆盖值。
    pub user_skill_root: Option<PathBuf>,
    /// Optional system skill root override.
    /// 可选系统技能根目录覆盖值。
    pub system_skill_root: Option<PathBuf>,
    /// Default state used when no per-skill override exists.
    /// 不存在单技能覆盖时使用的默认状态。
    pub default_skill_state: PromptSkillState,
    /// Global per-skill state overrides.
    /// 全局单技能状态覆盖。
    pub skill_overrides: BTreeMap<String, PromptSkillState>,
}

impl Default for PromptSkillsGlobalConfig {
    /// Builds the default globally enabled prompt skills configuration.
    /// 构建默认全局启用的提示词技能配置。
    fn default() -> Self {
        Self {
            enabled: true,
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
    /// The global total switch disabled every skill.
    /// 全局总开关关闭了所有技能。
    GlobalDisabled,
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
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_USER_SKILL_DIR)
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

/// Loads prompt skills from the configured user directory only.
/// 仅从配置后的用户目录加载提示词技能。
pub fn load_user_prompt_skills(config: &PromptSkillsGlobalConfig) -> PromptSkillCatalog {
    load_prompt_skills_from_user_root(configured_user_skill_root(config))
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
    let mut catalog = PromptSkillCatalog {
        user_root: user_root.clone(),
        system_root: system_root.clone(),
        skills: Vec::new(),
        errors: Vec::new(),
    };

    let mut by_name = BTreeMap::<String, PromptSkill>::new();
    scan_prompt_skill_root(
        &system_root,
        PromptSkillScope::System,
        &mut by_name,
        &mut catalog.errors,
    );
    scan_prompt_skill_root(
        &user_root,
        PromptSkillScope::User,
        &mut by_name,
        &mut catalog.errors,
    );

    catalog.skills = by_name.into_values().collect();
    catalog
}

/// Scans one direct-child prompt skill root and records non-fatal load errors.
/// 扫描单个直接子级提示词技能根目录并记录非致命加载错误。
fn scan_prompt_skill_root(
    root: &Path,
    scope: PromptSkillScope,
    by_name: &mut BTreeMap<String, PromptSkill>,
    errors: &mut Vec<PromptSkillLoadError>,
) {
    if root.as_os_str().is_empty() || !root.exists() {
        return;
    }
    if !root.is_dir() {
        errors.push(PromptSkillLoadError {
            path: root.to_path_buf(),
            message: String::from("Prompt skill root exists but is not a directory."),
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

    for entry in entries.flatten() {
        let skill_dir = entry.path();
        if !skill_dir.is_dir() {
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
    let raw = fs::read_to_string(skill_file).map_err(|error| PromptSkillLoadError {
        path: skill_file.to_path_buf(),
        message: format!("Failed to read SKILL.md: {error}"),
    })?;
    parse_prompt_skill_text_with_scope(skill_file, &raw, scope)
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
    let (frontmatter, body) = split_frontmatter(raw).ok_or_else(|| PromptSkillLoadError {
        path: skill_file.to_path_buf(),
        message: String::from("SKILL.md must start with YAML frontmatter."),
    })?;
    let parsed: PromptSkillFrontmatter =
        serde_yaml::from_str(frontmatter).map_err(|error| PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: format!("Failed to parse skill frontmatter: {error}"),
        })?;
    let name = parsed.name.trim().to_string();
    if name.is_empty() {
        return Err(PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: String::from("Skill frontmatter field `name` must not be empty."),
        });
    }
    let description = parsed.description.unwrap_or_default().trim().to_string();
    if description.is_empty() {
        return Err(PromptSkillLoadError {
            path: skill_file.to_path_buf(),
            message: String::from("Skill frontmatter field `description` must not be empty."),
        });
    }
    let base_dir = skill_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let interface = load_prompt_skill_interface_metadata(&base_dir)?;
    Ok(PromptSkill {
        name,
        description,
        skill_file: skill_file.to_path_buf(),
        base_dir,
        scope,
        interface,
        body: body.trim().to_string(),
    })
}

/// Loads optional skill interface metadata from root-level `vulcan.yaml`.
/// 从根目录 `vulcan.yaml` 载入可选技能界面信息。
fn load_prompt_skill_interface_metadata(
    base_dir: &Path,
) -> Result<Option<PromptSkillInterfaceMetadata>, PromptSkillLoadError> {
    let metadata_path = base_dir.join(VULCAN_SKILL_METADATA_FILE_NAME);
    if metadata_path.is_file() {
        return parse_prompt_skill_interface_metadata_file(&metadata_path).map(Some);
    }
    Ok(None)
}

/// Parses one skill interface metadata file into UI-facing fields.
/// 将单个技能界面元数据文件解析为面向界面的字段。
fn parse_prompt_skill_interface_metadata_file(
    metadata_path: &Path,
) -> Result<PromptSkillInterfaceMetadata, PromptSkillLoadError> {
    let raw = fs::read_to_string(metadata_path).map_err(|error| PromptSkillLoadError {
        path: metadata_path.to_path_buf(),
        message: format!("Failed to read skill interface metadata: {error}"),
    })?;
    let parsed: PromptSkillAgentMetadata =
        serde_yaml::from_str(&raw).map_err(|error| PromptSkillLoadError {
            path: metadata_path.to_path_buf(),
            message: format!("Failed to parse skill interface metadata: {error}"),
        })?;
    Ok(parsed.interface.into_metadata())
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
            let (state, source) = resolve_prompt_skill_state(&skill.name, global, project, session);
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
    if !global.enabled {
        return (
            PromptSkillState::Disabled,
            PromptSkillStateSource::GlobalDisabled,
        );
    }
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
            resolved.skill.skill_file.display()
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
    let base_dir = skill.base_dir.display().to_string();
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
            xml_escape(&file.display().to_string())
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

/// YAML frontmatter shape supported by prompt skills.
/// 提示词技能支持的 YAML frontmatter 结构。
#[derive(Debug, Deserialize)]
struct PromptSkillFrontmatter {
    /// Stable skill name.
    /// 稳定技能名称。
    name: String,
    /// Model-facing skill description.
    /// 面向模型的技能描述。
    description: Option<String>,
}

/// YAML shape for one skill agent metadata file.
/// 单个技能 agent 元数据文件的 YAML 结构。
#[derive(Debug, Deserialize)]
struct PromptSkillAgentMetadata {
    /// UI-facing interface metadata block.
    /// 面向界面的展示元数据块。
    #[serde(default)]
    interface: PromptSkillInterfaceMetadataRaw,
}

/// Raw optional interface metadata fields from YAML.
/// 从 YAML 读取的原始可选界面元数据字段。
#[derive(Debug, Default, Deserialize)]
struct PromptSkillInterfaceMetadataRaw {
    /// Optional human-facing display name.
    /// 可选人类可读展示名称。
    display_name: Option<String>,
    /// Optional short human-facing description.
    /// 可选人类可读短描述。
    short_description: Option<String>,
    /// Optional small icon path.
    /// 可选小图标路径。
    icon_small: Option<String>,
    /// Optional large icon path.
    /// 可选大图标路径。
    icon_large: Option<String>,
    /// Optional brand color string.
    /// 可选品牌色字符串。
    brand_color: Option<String>,
}

impl PromptSkillInterfaceMetadataRaw {
    /// Converts raw YAML fields into normalized interface metadata.
    /// 将原始 YAML 字段转换为规范化界面元数据。
    fn into_metadata(self) -> PromptSkillInterfaceMetadata {
        PromptSkillInterfaceMetadata {
            display_name: normalize_optional_string(self.display_name),
            short_description: normalize_optional_string(self.short_description),
            icon_small: normalize_optional_string(self.icon_small).map(PathBuf::from),
            icon_large: normalize_optional_string(self.icon_large).map(PathBuf::from),
            brand_color: normalize_optional_string(self.brand_color),
        }
    }
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
