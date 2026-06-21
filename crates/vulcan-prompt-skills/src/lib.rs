//! System and user prompt skill discovery, installation, policy resolution, and rendering.
//! 系统级与用户级提示词技能发现、安装、策略解析与渲染。

use include_dir::Dir;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Embedded VulcanCode-adapted system prompt skills.
/// 已内嵌且适配 VulcanCode 的系统提示词技能。
const SYSTEM_SKILLS_DIR: Dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/src/assets/system");

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

/// Marker file used to skip unchanged system skill installs.
/// 用于跳过未变化系统技能安装的标记文件。
const SYSTEM_SKILLS_MARKER_FILENAME: &str = ".vulcan-system-skills.marker";

/// Fingerprint salt for the current embedded system skill install shape.
/// 当前内置系统技能安装结构的指纹盐值。
const SYSTEM_SKILLS_MARKER_SALT: &str = "v1";

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
    /// Returns a compact Chinese label for GUI display.
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
    configured_user_skill_root(config).join(SYSTEM_SKILL_DIR_NAME)
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

/// Installs embedded system prompt skills into the configured `.system` root.
/// 将内置系统提示词技能安装到配置后的 `.system` 根目录。
pub fn install_embedded_system_prompt_skills(
    config: &PromptSkillsGlobalConfig,
) -> Result<SystemPromptSkillsInstallReport, PromptSkillLoadError> {
    install_embedded_system_prompt_skills_to_root(configured_system_skill_root(config))
}

/// Installs embedded system prompt skills into one explicit system root.
/// 将内置系统提示词技能安装到一个显式系统根目录。
pub fn install_embedded_system_prompt_skills_to_root(
    system_root: PathBuf,
) -> Result<SystemPromptSkillsInstallReport, PromptSkillLoadError> {
    let skill_names = embedded_system_skill_names();
    let fingerprint = embedded_system_skills_fingerprint();
    let marker_path = system_root.join(SYSTEM_SKILLS_MARKER_FILENAME);
    if system_root.is_dir() && read_marker(&marker_path).is_ok_and(|marker| marker == fingerprint) {
        return Ok(SystemPromptSkillsInstallReport {
            system_root,
            installed: false,
            fingerprint,
            skill_names,
        });
    }

    if system_root.exists() {
        fs::remove_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
            path: system_root.clone(),
            message: format!("Failed to remove old system prompt skills: {error}"),
        })?;
    }
    fs::create_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
        path: system_root.clone(),
        message: format!("Failed to create system prompt skills root: {error}"),
    })?;
    write_embedded_dir(&SYSTEM_SKILLS_DIR, &system_root)?;
    fs::write(&marker_path, format!("{fingerprint}\n")).map_err(|error| PromptSkillLoadError {
        path: marker_path,
        message: format!("Failed to write system prompt skills marker: {error}"),
    })?;

    Ok(SystemPromptSkillsInstallReport {
        system_root,
        installed: true,
        fingerprint,
        skill_names,
    })
}

/// Installs system prompt skills from one packaged filesystem resource root.
/// 从一个打包后的文件系统资源根目录安装系统提示词技能。
pub fn install_system_prompt_skills_from_source(
    source_root: PathBuf,
    system_root: PathBuf,
) -> Result<SystemPromptSkillsInstallReport, PromptSkillLoadError> {
    if !source_root.is_dir() {
        return Err(PromptSkillLoadError {
            path: source_root,
            message: String::from("System prompt skills source root is not a directory."),
        });
    }

    let skill_names = system_skill_names_from_source_root(&source_root)?;
    let fingerprint = system_skills_source_fingerprint(&source_root)?;
    let marker_path = system_root.join(SYSTEM_SKILLS_MARKER_FILENAME);
    if system_root.is_dir() && read_marker(&marker_path).is_ok_and(|marker| marker == fingerprint) {
        return Ok(SystemPromptSkillsInstallReport {
            system_root,
            installed: false,
            fingerprint,
            skill_names,
        });
    }

    if system_root.exists() {
        fs::remove_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
            path: system_root.clone(),
            message: format!("Failed to remove old system prompt skills: {error}"),
        })?;
    }
    fs::create_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
        path: system_root.clone(),
        message: format!("Failed to create system prompt skills root: {error}"),
    })?;
    copy_source_dir_contents(&source_root, &system_root)?;
    fs::write(&marker_path, format!("{fingerprint}\n")).map_err(|error| PromptSkillLoadError {
        path: marker_path,
        message: format!("Failed to write system prompt skills marker: {error}"),
    })?;

    Ok(SystemPromptSkillsInstallReport {
        system_root,
        installed: true,
        fingerprint,
        skill_names,
    })
}

/// Reads a marker file and normalizes its trailing whitespace.
/// 读取标记文件并规范化尾部空白。
fn read_marker(path: &Path) -> Result<String, PromptSkillLoadError> {
    Ok(fs::read_to_string(path)
        .map_err(|error| PromptSkillLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to read system prompt skills marker: {error}"),
        })?
        .trim()
        .to_string())
}

/// Returns embedded system skill names in stable order.
/// 返回稳定顺序的内置系统技能名称。
pub fn embedded_system_skill_names() -> Vec<String> {
    let mut names = SYSTEM_SKILLS_DIR
        .dirs()
        .filter_map(|dir| dir.path().file_name()?.to_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Returns packaged system skill names from one filesystem root in stable order.
/// 从一个文件系统根目录返回稳定顺序的打包系统技能名称。
fn system_skill_names_from_source_root(root: &Path) -> Result<Vec<String>, PromptSkillLoadError> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| PromptSkillLoadError {
        path: root.to_path_buf(),
        message: format!("Failed to read system prompt skills source root: {error}"),
    })? {
        let entry = entry.map_err(|error| PromptSkillLoadError {
            path: root.to_path_buf(),
            message: format!("Failed to read system prompt skill source entry: {error}"),
        })?;
        let path = entry.path();
        if path.is_dir()
            && path.join(SKILL_FILE_NAME).is_file()
            && let Some(name) = path.file_name().and_then(|name| name.to_str())
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// Builds one fingerprint for all embedded system skill resources.
/// 为所有内置系统技能资源构建一个指纹。
fn embedded_system_skills_fingerprint() -> String {
    let mut items = Vec::new();
    collect_fingerprint_items(&SYSTEM_SKILLS_DIR, &mut items);
    items.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    let mut hasher = DefaultHasher::new();
    SYSTEM_SKILLS_MARKER_SALT.hash(&mut hasher);
    for (path, contents_hash) in items {
        path.hash(&mut hasher);
        contents_hash.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

/// Builds one fingerprint for a packaged system skill source directory.
/// 为一个打包系统技能源目录构建指纹。
fn system_skills_source_fingerprint(root: &Path) -> Result<String, PromptSkillLoadError> {
    let mut items = Vec::new();
    collect_source_fingerprint_items(root, root, &mut items)?;
    items.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    let mut hasher = DefaultHasher::new();
    SYSTEM_SKILLS_MARKER_SALT.hash(&mut hasher);
    for (path, contents_hash) in items {
        path.hash(&mut hasher);
        contents_hash.hash(&mut hasher);
    }
    Ok(format!("{:x}", hasher.finish()))
}

/// Collects relative embedded resource paths and file content hashes.
/// 收集内置资源相对路径与文件内容哈希。
fn collect_fingerprint_items(dir: &Dir<'_>, items: &mut Vec<(String, Option<u64>)>) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(subdir) => {
                items.push((subdir.path().to_string_lossy().to_string(), None));
                collect_fingerprint_items(subdir, items);
            }
            include_dir::DirEntry::File(file) => {
                let mut file_hasher = DefaultHasher::new();
                file.contents().hash(&mut file_hasher);
                items.push((
                    file.path().to_string_lossy().to_string(),
                    Some(file_hasher.finish()),
                ));
            }
        }
    }
}

/// Collects relative filesystem resource paths and file content hashes.
/// 收集文件系统资源相对路径与文件内容哈希。
fn collect_source_fingerprint_items(
    root: &Path,
    current: &Path,
    items: &mut Vec<(String, Option<u64>)>,
) -> Result<(), PromptSkillLoadError> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| PromptSkillLoadError {
            path: current.to_path_buf(),
            message: format!("Failed to read system prompt skill source directory: {error}"),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PromptSkillLoadError {
            path: current.to_path_buf(),
            message: format!("Failed to read system prompt skill source entry: {error}"),
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            items.push((relative_path, None));
            collect_source_fingerprint_items(root, &path, items)?;
        } else if path.is_file() {
            let contents = fs::read(&path).map_err(|error| PromptSkillLoadError {
                path: path.clone(),
                message: format!("Failed to read system prompt skill source file: {error}"),
            })?;
            let mut file_hasher = DefaultHasher::new();
            contents.hash(&mut file_hasher);
            items.push((relative_path, Some(file_hasher.finish())));
        }
    }

    Ok(())
}

/// Writes one embedded include-dir tree into the destination directory.
/// 将一个内置 include-dir 树写入目标目录。
fn write_embedded_dir(dir: &Dir<'_>, dest: &Path) -> Result<(), PromptSkillLoadError> {
    fs::create_dir_all(dest).map_err(|error| PromptSkillLoadError {
        path: dest.to_path_buf(),
        message: format!("Failed to create system prompt skills directory: {error}"),
    })?;

    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(subdir) => {
                write_embedded_dir(subdir, dest)?;
            }
            include_dir::DirEntry::File(file) => {
                let path = dest.join(file.path());
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| PromptSkillLoadError {
                        path: parent.to_path_buf(),
                        message: format!("Failed to create system prompt skill parent: {error}"),
                    })?;
                }
                fs::write(&path, file.contents()).map_err(|error| PromptSkillLoadError {
                    path,
                    message: format!("Failed to write system prompt skill file: {error}"),
                })?;
            }
        }
    }

    Ok(())
}

/// Copies direct filesystem source contents into the destination directory.
/// 将文件系统来源目录的直属内容复制到目标目录。
fn copy_source_dir_contents(source: &Path, dest: &Path) -> Result<(), PromptSkillLoadError> {
    for entry in fs::read_dir(source).map_err(|error| PromptSkillLoadError {
        path: source.to_path_buf(),
        message: format!("Failed to read system prompt skills source directory: {error}"),
    })? {
        let entry = entry.map_err(|error| PromptSkillLoadError {
            path: source.to_path_buf(),
            message: format!("Failed to read system prompt skill source entry: {error}"),
        })?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if source_path.is_dir() {
            copy_source_dir_recursive(&source_path, &dest_path)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &dest_path).map_err(|error| PromptSkillLoadError {
                path: source_path,
                message: format!("Failed to copy system prompt skill source file: {error}"),
            })?;
        }
    }
    Ok(())
}

/// Copies one filesystem source directory recursively into the destination directory.
/// 将一个文件系统来源目录递归复制到目标目录。
fn copy_source_dir_recursive(source: &Path, dest: &Path) -> Result<(), PromptSkillLoadError> {
    fs::create_dir_all(dest).map_err(|error| PromptSkillLoadError {
        path: dest.to_path_buf(),
        message: format!("Failed to create system prompt skill destination directory: {error}"),
    })?;
    copy_source_dir_contents(source, dest)
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
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates one isolated temporary directory for prompt skill tests.
    /// 为提示词技能测试创建一个隔离临时目录。
    fn temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vulcan-prompt-skills-{suffix}"));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    #[test]
    fn parses_skill_frontmatter_and_body() {
        let path = PathBuf::from("demo/SKILL.md");
        let skill = parse_prompt_skill_text(
            &path,
            "---\nname: demo\ndescription: Use for demo tasks.\n---\n\n# Demo\nBody",
        )
        .expect("skill should parse");
        assert_eq!(skill.name, "demo");
        assert_eq!(skill.description, "Use for demo tasks.");
        assert_eq!(skill.body, "# Demo\nBody");
        assert!(skill.interface.is_none());
    }

    #[test]
    fn loads_only_user_root_skill_directories() {
        let root = temp_dir();
        let skill_dir = root.join("demo");
        let system_dir = root.join(SYSTEM_SKILL_DIR_NAME);
        fs::create_dir_all(&skill_dir).expect("skill dir should be created");
        fs::create_dir_all(&system_dir).expect("system dir should be created");
        fs::write(
            skill_dir.join(SKILL_FILE_NAME),
            "---\nname: demo\ndescription: Use for demo tasks.\n---\n\n# Demo",
        )
        .expect("skill file should be written");
        fs::write(
            system_dir.join(SKILL_FILE_NAME),
            "---\nname: invalid\ndescription: Should be ignored.\n---\n\n# Invalid",
        )
        .expect("system root file should be written");
        fs::write(root.join(SKILL_FILE_NAME), "ignored").expect("root file should be written");

        let catalog = load_prompt_skills_from_user_root(root);
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].name, "demo");
        assert_eq!(catalog.skills[0].scope, PromptSkillScope::User);
        assert!(catalog.errors.is_empty());
    }

    #[test]
    fn loads_root_vulcan_interface_metadata() {
        let root = temp_dir();
        let skill_dir = root.join("demo");
        fs::create_dir_all(skill_dir.join("assets")).expect("assets dir should be created");
        fs::write(
            skill_dir.join(SKILL_FILE_NAME),
            "---\nname: demo\ndescription: Use for demo tasks.\n---\n\n# Demo",
        )
        .expect("skill file should be written");
        fs::write(
            skill_dir.join(VULCAN_SKILL_METADATA_FILE_NAME),
            "interface:\n  display_name: \"Vulcan Demo\"\n  short_description: \"Vulcan description\"\n  icon_small: \"./assets/demo.svg\"\n",
        )
        .expect("vulcan metadata should be written");

        let catalog = load_prompt_skills_from_user_root(root);
        let interface = catalog.skills[0]
            .interface
            .as_ref()
            .expect("interface metadata should load");
        assert_eq!(interface.display_name.as_deref(), Some("Vulcan Demo"));
        assert_eq!(
            interface.short_description.as_deref(),
            Some("Vulcan description")
        );
        assert_eq!(
            interface.icon_small.as_deref(),
            Some(Path::new("./assets/demo.svg"))
        );
    }

    #[test]
    fn loads_user_skill_over_system_skill_with_same_name() {
        let root = temp_dir();
        let system_root = root.join(SYSTEM_SKILL_DIR_NAME);
        let system_skill_dir = system_root.join("demo");
        let user_skill_dir = root.join("demo");
        fs::create_dir_all(&system_skill_dir).expect("system skill dir should be created");
        fs::create_dir_all(&user_skill_dir).expect("user skill dir should be created");
        fs::write(
            system_skill_dir.join(SKILL_FILE_NAME),
            "---\nname: demo\ndescription: System demo.\n---\n\n# System",
        )
        .expect("system skill should be written");
        fs::write(
            user_skill_dir.join(SKILL_FILE_NAME),
            "---\nname: demo\ndescription: User demo.\n---\n\n# User",
        )
        .expect("user skill should be written");

        let catalog = load_prompt_skills_from_roots(system_root, root);
        assert_eq!(catalog.skills.len(), 1);
        assert_eq!(catalog.skills[0].scope, PromptSkillScope::User);
        assert_eq!(catalog.skills[0].body, "# User");
    }

    #[test]
    fn installs_embedded_system_prompt_skills_with_marker_skip() {
        let root = temp_dir().join(SYSTEM_SKILL_DIR_NAME);
        let first = install_embedded_system_prompt_skills_to_root(root.clone())
            .expect("system skills should install");
        assert!(first.installed);
        assert!(root.join("skill-creator").join(SKILL_FILE_NAME).is_file());

        let second = install_embedded_system_prompt_skills_to_root(root)
            .expect("system skills should skip unchanged install");
        assert!(!second.installed);
        assert_eq!(first.fingerprint, second.fingerprint);
    }

    #[test]
    fn installs_system_prompt_skills_from_packaged_source() {
        let root = temp_dir();
        let source_root = root.join("packaged").join("prompt-skills").join("system");
        let source_skill_dir = source_root.join("packaged-skill");
        fs::create_dir_all(source_skill_dir.join("scripts"))
            .expect("source skill dir should be created");
        fs::write(
            source_skill_dir.join(SKILL_FILE_NAME),
            "---\nname: packaged-skill\ndescription: Use for packaged tests.\n---\n\n# Packaged",
        )
        .expect("source skill should be written");
        fs::write(source_skill_dir.join("scripts").join("helper.txt"), "ok")
            .expect("source resource should be written");

        let system_root = root.join(SYSTEM_SKILL_DIR_NAME);
        let first =
            install_system_prompt_skills_from_source(source_root.clone(), system_root.clone())
                .expect("packaged system skills should install");
        assert!(first.installed);
        assert_eq!(first.skill_names, vec![String::from("packaged-skill")]);
        assert!(
            system_root
                .join("packaged-skill")
                .join("scripts")
                .join("helper.txt")
                .is_file()
        );

        let second = install_system_prompt_skills_from_source(source_root, system_root)
            .expect("unchanged packaged system skills should skip");
        assert!(!second.installed);
        assert_eq!(first.fingerprint, second.fingerprint);
    }

    #[test]
    fn resolves_session_over_project_over_global() {
        let catalog = PromptSkillCatalog {
            user_root: PathBuf::from("."),
            system_root: PathBuf::from("./.system"),
            skills: vec![PromptSkill {
                name: String::from("demo"),
                description: String::from("Use for demo tasks."),
                skill_file: PathBuf::from("demo/SKILL.md"),
                base_dir: PathBuf::from("demo"),
                scope: PromptSkillScope::User,
                interface: None,
                body: String::from("# Demo"),
            }],
            errors: Vec::new(),
        };
        let mut global = PromptSkillsGlobalConfig::default();
        global
            .skill_overrides
            .insert(String::from("demo"), PromptSkillState::Disabled);
        let mut project = PromptSkillsProjectConfig::default();
        project
            .skill_overrides
            .insert(String::from("demo"), PromptSkillState::Enabled);
        let mut session = PromptSkillsSessionConfig::default();
        session
            .skill_overrides
            .insert(String::from("demo"), PromptSkillState::Disabled);

        let resolved = resolve_prompt_skills(&catalog, &global, &project, &session);
        assert_eq!(resolved[0].state, PromptSkillState::Disabled);
        assert_eq!(resolved[0].source, PromptSkillStateSource::Session);
    }

    #[test]
    fn renders_only_enabled_skills() {
        let skills = vec![
            ResolvedPromptSkill {
                skill: PromptSkill {
                    name: String::from("enabled"),
                    description: String::from("Use for enabled tasks."),
                    skill_file: PathBuf::from("enabled/SKILL.md"),
                    base_dir: PathBuf::from("enabled"),
                    scope: PromptSkillScope::User,
                    interface: None,
                    body: String::new(),
                },
                state: PromptSkillState::Enabled,
                source: PromptSkillStateSource::GlobalDefault,
            },
            ResolvedPromptSkill {
                skill: PromptSkill {
                    name: String::from("disabled"),
                    description: String::from("Use for disabled tasks."),
                    skill_file: PathBuf::from("disabled/SKILL.md"),
                    base_dir: PathBuf::from("disabled"),
                    scope: PromptSkillScope::User,
                    interface: None,
                    body: String::new(),
                },
                state: PromptSkillState::Disabled,
                source: PromptSkillStateSource::Session,
            },
        ];
        let rendered = render_prompt_skills_instructions(&skills).expect("instructions expected");
        assert!(rendered.contains("enabled"));
        assert!(!rendered.contains("disabled"));
        assert!(rendered.contains("<skills_instructions>"));
    }
}
