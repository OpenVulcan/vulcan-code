//! User-visible agent profile loading, persistence, and legacy migration helpers.
//! 面向用户的 Agent 配置加载、持久化与旧配置迁移辅助模块。

use crate::agent_runtime::config::{
    DEFAULT_WORKER_AGENT_ID, EXPLORE_AGENT_ID, LUA_SPECIALIST_AGENT_ID, MEMORY_SPECIALIST_AGENT_ID,
    PLAN_AGENT_ID, WORKSPACE_COORDINATOR_AGENT_ID,
};
use crate::provider_config::{PersistedAgentEntry, ProviderConfigurationFile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use vulcan_session_core::ExecutionRuntimePolicyRequest;

/// Stable synthetic picker identifier representing the system-default model-routed agent.
/// 表示系统默认模型路由代理的稳定合成选择标识。
pub const SYSTEM_DEFAULT_AGENT_CHOICE_ID: &str = "__system_default__";

/// Legacy built-in worker agent identifier retained only for migration filtering.
/// 仅用于迁移过滤的旧版内建工作代理标识。
const LEGACY_DEFAULT_WORKER_AGENT_ID: &str = "default-worker";

/// Legacy built-in Lua specialist agent identifier retained only for migration filtering.
/// 仅用于迁移过滤的旧版内建 Lua 专家代理标识。
const LEGACY_LUA_SPECIALIST_AGENT_ID: &str = "lua-specialist";

/// Legacy built-in memory specialist agent identifier retained only for migration filtering.
/// 仅用于迁移过滤的旧版内建记忆专家代理标识。
const LEGACY_MEMORY_SPECIALIST_AGENT_ID: &str = "memory-specialist";

/// Legacy built-in coordinator agent identifier retained only for migration filtering.
/// 仅用于迁移过滤的旧版内建协调代理标识。
const LEGACY_WORKSPACE_COORDINATOR_AGENT_ID: &str = "workspace-coordinator";

/// Relative directory name that stores user-authored agent markdown files.
/// 存放用户自定义 Agent Markdown 文件的相对目录名称。
const USER_AGENT_DIRECTORY_NAME: &str = "agents";

/// File name used by the user-agent selection state persisted in the user directory.
/// 保存在用户目录中的 Agent 选择状态文件名。
const AGENT_PROFILE_STATE_FILE_NAME: &str = "agent-config.json";

/// Minimal persisted state used to remember the currently selected user agent.
/// 用于记住当前已选用户 Agent 的最小持久化状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfileState {
    /// Optional selected user-agent identifier; `None` means the system default should be used.
    /// 可选的已选用户 Agent 标识；`None` 表示使用系统默认。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_agent_id: Option<String>,
}

/// One parsed user-authored agent profile loaded from the user directory.
/// 从用户目录载入的一条用户自定义 Agent 配置。
#[derive(Debug, Clone)]
pub struct UserAgentProfile {
    /// Stable user-agent identifier derived from frontmatter or file name.
    /// 从前置元数据或文件名派生出的稳定用户 Agent 标识。
    pub id: String,
    /// Human-readable agent name shown in the picker.
    /// 展示在选择器中的人类可读 Agent 名称。
    pub name: String,
    /// Optional concise description shown in management surfaces.
    /// 展示在管理界面中的可选简短描述。
    pub description: String,
    /// Markdown body used as the primary agent prompt.
    /// 作为主 Agent 提示词使用的 Markdown 正文。
    pub prompt: String,
    /// Optional model override in `provider/model` form.
    /// 以 `provider/model` 形式声明的可选模型覆盖。
    pub model: Option<String>,
    /// Optional temperature override.
    /// 可选的温度参数覆盖值。
    pub temperature: Option<f32>,
    /// Optional max-turn override.
    /// 可选的最大轮次数覆盖值。
    pub max_turns: Option<u16>,
    /// Prompt skill identifiers bound to this agent profile.
    /// 绑定到该 Agent 配置的提示词技能标识集合。
    pub skill_ids: Vec<String>,
    /// Native tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的原生工具标识集合。
    pub native_tools: Vec<String>,
    /// MCP tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的 MCP 工具标识集合。
    pub mcp_tools: Vec<String>,
    /// Optional shared tool-policy identifier.
    /// 可选的共享工具策略标识。
    pub tool_policy: Option<String>,
    /// Optional task-session runtime feature overrides.
    /// 可选的子代理运行时能力覆盖。
    pub runtime_policy: ExecutionRuntimePolicyRequest,
}

/// Aggregate user-agent catalog composed from one selection state and the visible markdown profiles.
/// 由一份选择状态与可见 Markdown 配置组成的用户 Agent 目录聚合体。
#[derive(Debug, Clone, Default)]
pub struct AgentProfileCatalog {
    /// Persisted selection state remembered across launches.
    /// 跨启动周期保留的持久化选择状态。
    pub state: AgentProfileState,
    /// Visible user-authored agent profiles keyed by stable identifier.
    /// 以稳定标识为键保存的可见用户 Agent 配置集合。
    pub agents: BTreeMap<String, UserAgentProfile>,
}

/// Frontmatter schema accepted by one user-authored agent markdown file.
/// 单个用户自定义 Agent Markdown 文件接受的前置元数据结构。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserAgentFrontmatter {
    /// Optional explicit stable agent identifier.
    /// 可选的显式稳定 Agent 标识。
    pub id: Option<String>,
    /// Optional human-readable agent name.
    /// 可选的人类可读 Agent 名称。
    pub name: Option<String>,
    /// Optional concise agent description.
    /// 可选的简短 Agent 描述。
    pub description: Option<String>,
    /// Optional model override in `provider/model` form.
    /// 以 `provider/model` 形式声明的可选模型覆盖。
    pub model: Option<String>,
    /// Optional temperature override.
    /// 可选的温度参数覆盖值。
    pub temperature: Option<f32>,
    /// Optional max-turn override.
    /// 可选的最大轮次数覆盖值。
    pub max_turns: Option<u16>,
    /// Prompt skill identifiers bound to this agent profile.
    /// 绑定到该 Agent 配置的提示词技能标识集合。
    #[serde(default, alias = "skills", skip_serializing_if = "Vec::is_empty")]
    pub skill_ids: Vec<String>,
    /// Native tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的原生工具标识集合。
    #[serde(default, alias = "tools", skip_serializing_if = "Vec::is_empty")]
    pub native_tools: Vec<String>,
    /// MCP tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的 MCP 工具标识集合。
    #[serde(default, alias = "mcp", skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<String>,
    /// Optional shared tool-policy identifier.
    /// 可选的共享工具策略标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<String>,
    /// Optional task-session runtime feature overrides.
    /// 可选的子代理运行时能力覆盖。
    #[serde(default, skip_serializing_if = "runtime_policy_request_is_empty")]
    pub runtime_policy: ExecutionRuntimePolicyRequest,
}

/// Returns the user-scoped root directory that stores agent profile state and markdown files.
/// 返回保存 Agent 状态与 Markdown 文件的用户级根目录。
pub fn agent_profile_root_directory() -> PathBuf {
    let home = dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    home.join(".vulcan").join("code")
}

/// Returns the directory that stores user-authored agent markdown files.
/// 返回存放用户自定义 Agent Markdown 文件的目录。
pub fn agent_profiles_directory() -> PathBuf {
    agent_profile_root_directory().join(USER_AGENT_DIRECTORY_NAME)
}

/// Returns the persisted agent-profile state file path.
/// 返回持久化 Agent 配置状态文件路径。
pub fn agent_profile_state_file_path() -> PathBuf {
    agent_profile_root_directory().join(AGENT_PROFILE_STATE_FILE_NAME)
}

/// Loads the visible user-agent catalog from disk.
/// 从磁盘加载可见的用户 Agent 目录聚合体。
pub fn load_agent_profile_catalog() -> Result<AgentProfileCatalog, String> {
    let state = load_agent_profile_state()?;
    let agents = load_user_agent_profiles()?;

    Ok(AgentProfileCatalog { state, agents })
}

/// Returns whether one agent identifier is reserved by the built-in runtime or the synthetic system-default slot.
/// 返回某个 Agent 标识是否被内置运行时或系统默认合成槽位保留。
pub fn is_reserved_agent_identifier(agent_id: &str) -> bool {
    matches!(
        agent_id.trim(),
        SYSTEM_DEFAULT_AGENT_CHOICE_ID
            | DEFAULT_WORKER_AGENT_ID
            | LUA_SPECIALIST_AGENT_ID
            | MEMORY_SPECIALIST_AGENT_ID
            | WORKSPACE_COORDINATOR_AGENT_ID
            | PLAN_AGENT_ID
            | EXPLORE_AGENT_ID
    )
}

/// Loads the persisted agent selection state from disk.
/// 从磁盘加载持久化的 Agent 选择状态。
pub fn load_agent_profile_state() -> Result<AgentProfileState, String> {
    let path = agent_profile_state_file_path();
    if !path.exists() {
        return Ok(AgentProfileState::default());
    }

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("读取 Agent 状态配置失败：{}。", error))?;

    serde_json::from_str(&text).map_err(|error| format!("解析 Agent 状态配置失败：{}。", error))
}

/// Saves the persisted agent selection state back to disk.
/// 将持久化的 Agent 选择状态写回磁盘。
pub fn save_agent_profile_state(state: &AgentProfileState) -> Result<(), String> {
    let path = agent_profile_state_file_path();
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    fs::create_dir_all(&parent).map_err(|error| format!("创建 Agent 配置目录失败：{}。", error))?;

    let content = serde_json::to_string_pretty(state)
        .map_err(|error| format!("序列化 Agent 状态配置失败：{}。", error))?;

    fs::write(&path, content).map_err(|error| format!("写入 Agent 状态配置失败：{}。", error))
}

/// Canonicalizes one user-provided agent identifier into the stable slug format used on disk.
/// 将用户提供的 Agent 标识规范化为磁盘使用的稳定 slug 格式。
pub fn canonicalize_agent_identifier(raw: &str) -> String {
    slugify_identifier(raw)
}

/// Saves one user-authored agent profile into the user directory, optionally replacing one previous identifier.
/// 将单条用户自定义 Agent 配置保存到用户目录，并可选替换某个旧标识。
pub fn save_user_agent_profile(
    profile: &UserAgentProfile,
    previous_id: Option<&str>,
) -> Result<(), String> {
    if profile.id.trim().is_empty() {
        return Err(String::from("Agent 标识不能为空。"));
    }
    if is_reserved_agent_identifier(&profile.id) {
        return Err(String::from(
            "该 Agent 标识已被系统内置 Agent 保留，请更换为其它标识。",
        ));
    }

    fs::create_dir_all(agent_profiles_directory())
        .map_err(|error| format!("创建用户 Agent 目录失败：{}。", error))?;

    if let Some(previous_id) = previous_id
        .map(str::trim)
        .filter(|previous_id| !previous_id.is_empty() && *previous_id != profile.id)
    {
        let previous_path = user_agent_profile_file_path(previous_id);
        if previous_path.exists() {
            fs::remove_file(&previous_path).map_err(|error| {
                format!(
                    "移除旧 Agent 文件 `{}` 失败：{}。",
                    previous_path.display(),
                    error
                )
            })?;
        }
    }

    save_user_agent_profile_markdown(profile)
}

/// Deletes one user-authored agent profile markdown file from the user directory.
/// 从用户目录中删除单条用户自定义 Agent Markdown 文件。
pub fn delete_user_agent_profile(agent_id: &str) -> Result<(), String> {
    let normalized_id = canonicalize_agent_identifier(agent_id);
    if normalized_id.is_empty() {
        return Err(String::from("Agent 标识不能为空。"));
    }

    let path = user_agent_profile_file_path(&normalized_id);
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(&path)
        .map_err(|error| format!("删除 Agent 文件 `{}` 失败：{}。", path.display(), error))
}

/// Migrates legacy provider-bound agent definitions into the user-agent directory when needed.
/// 在需要时将旧版绑定在 Provider 配置中的 Agent 定义迁移到用户 Agent 目录。
pub fn migrate_legacy_provider_agents(
    config: &ProviderConfigurationFile,
    catalog: &mut AgentProfileCatalog,
) -> Result<bool, String> {
    let mut changed = false;

    for (agent_id, entry) in &config.legacy_agents {
        if is_reserved_agent_identifier(agent_id) || is_legacy_builtin_agent_id(agent_id) {
            continue;
        }

        if catalog.agents.contains_key(agent_id) {
            continue;
        }

        let profile = user_agent_profile_from_legacy_entry(agent_id, entry);
        save_user_agent_profile_markdown(&profile)?;
        catalog.agents.insert(profile.id.clone(), profile);
        changed = true;
    }

    if catalog.state.selected_agent_id.is_none() {
        if let Some(legacy_active_agent_id) = config
            .legacy_active_agent
            .as_deref()
            .filter(|agent_id| !is_legacy_builtin_agent_id(agent_id))
            .filter(|agent_id| catalog.agents.contains_key(*agent_id))
        {
            catalog.state.selected_agent_id = Some(legacy_active_agent_id.to_string());
            changed = true;
        }
    }

    if changed {
        save_agent_profile_state(&catalog.state)?;
    }

    Ok(changed)
}

/// Loads every visible user-authored agent profile from the user directory.
/// 从用户目录加载所有可见的用户自定义 Agent 配置。
fn load_user_agent_profiles() -> Result<BTreeMap<String, UserAgentProfile>, String> {
    let root = agent_profiles_directory();
    if !root.exists() {
        return Ok(BTreeMap::new());
    }

    let mut profiles = BTreeMap::new();
    let mut files = Vec::new();
    collect_markdown_files(&root, &mut files)?;

    for path in files {
        let profile = load_user_agent_profile(&path)?;
        if is_reserved_agent_identifier(&profile.id) {
            continue;
        }
        profiles.insert(profile.id.clone(), profile);
    }

    Ok(profiles)
}

/// Recursively collects markdown files below one user-agent directory.
/// 递归收集某个用户 Agent 目录下的 Markdown 文件。
fn collect_markdown_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "读取 Agent 目录 `{}` 失败：{}。",
            directory.display(),
            error
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "枚举 Agent 目录 `{}` 时读取条目失败：{}。",
                directory.display(),
                error
            )
        })?;
        let path = entry.path();

        if path.is_dir() {
            collect_markdown_files(&path, files)?;
            continue;
        }

        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            files.push(path);
        }
    }

    Ok(())
}

/// Loads one user-authored agent markdown file into one structured profile.
/// 将单个用户自定义 Agent Markdown 文件加载为结构化配置。
fn load_user_agent_profile(path: &Path) -> Result<UserAgentProfile, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("读取 Agent 文件 `{}` 失败：{}。", path.display(), error))?;
    let (frontmatter, prompt) = split_frontmatter_and_body(&text, path)?;
    if prompt.trim().is_empty() {
        return Err(format!(
            "Agent 文件 `{}` 缺少有效提示词正文。",
            path.display()
        ));
    }

    let file_stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("agent");
    let agent_id = frontmatter
        .id
        .as_deref()
        .map(slugify_identifier)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| slugify_identifier(file_stem));
    let agent_name = frontmatter
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| file_stem.to_string());

    Ok(UserAgentProfile {
        id: agent_id,
        name: agent_name,
        description: frontmatter.description.unwrap_or_default(),
        prompt,
        model: trim_to_option(frontmatter.model.as_deref()),
        temperature: frontmatter.temperature,
        max_turns: frontmatter.max_turns,
        skill_ids: normalize_string_list(frontmatter.skill_ids),
        native_tools: normalize_string_list(frontmatter.native_tools),
        mcp_tools: normalize_string_list(frontmatter.mcp_tools),
        tool_policy: trim_to_option(frontmatter.tool_policy.as_deref()),
        runtime_policy: frontmatter.runtime_policy,
    })
}

/// Splits one markdown file into optional YAML frontmatter and body text.
/// 将单个 Markdown 文件拆分为可选 YAML 前置元数据与正文。
fn split_frontmatter_and_body(
    text: &str,
    path: &Path,
) -> Result<(UserAgentFrontmatter, String), String> {
    let normalized = text.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Ok((
            UserAgentFrontmatter::default(),
            normalized.trim().to_string(),
        ));
    };

    let Some((frontmatter_text, body)) = rest.split_once("\n---\n") else {
        return Err(format!(
            "Agent 文件 `{}` 的 YAML frontmatter 缺少结束分隔符 `---`。",
            path.display()
        ));
    };

    let frontmatter: UserAgentFrontmatter =
        serde_yaml::from_str(frontmatter_text).map_err(|error| {
            format!(
                "解析 Agent 文件 `{}` 的 YAML frontmatter 失败：{}。",
                path.display(),
                error
            )
        })?;

    Ok((frontmatter, body.trim().to_string()))
}

/// Converts one legacy persisted agent entry into one user-authored profile snapshot.
/// 将单条旧版持久化 Agent 条目转换为用户自定义配置快照。
fn user_agent_profile_from_legacy_entry(
    agent_id: &str,
    entry: &PersistedAgentEntry,
) -> UserAgentProfile {
    UserAgentProfile {
        id: agent_id.to_string(),
        name: entry.name.clone(),
        description: entry.description.clone().unwrap_or_default(),
        prompt: entry.system_prompt.clone().unwrap_or_default(),
        model: trim_to_option(entry.model.as_deref()),
        temperature: entry.temperature,
        max_turns: entry.max_turns,
        skill_ids: Vec::new(),
        native_tools: Vec::new(),
        mcp_tools: Vec::new(),
        tool_policy: None,
        runtime_policy: ExecutionRuntimePolicyRequest::default(),
    }
}

/// Saves one user-authored agent profile into the user markdown directory.
/// 将单条用户自定义 Agent 配置保存到用户 Markdown 目录。
fn save_user_agent_profile_markdown(profile: &UserAgentProfile) -> Result<(), String> {
    let path = user_agent_profile_file_path(&profile.id);

    let frontmatter = UserAgentFrontmatter {
        id: Some(profile.id.clone()),
        name: Some(profile.name.clone()),
        description: trim_to_option(Some(profile.description.as_str())),
        model: profile.model.clone(),
        temperature: profile.temperature,
        max_turns: profile.max_turns,
        skill_ids: profile.skill_ids.clone(),
        native_tools: profile.native_tools.clone(),
        mcp_tools: profile.mcp_tools.clone(),
        tool_policy: profile.tool_policy.clone(),
        runtime_policy: profile.runtime_policy,
    };
    let frontmatter_text = serde_yaml::to_string(&frontmatter)
        .map_err(|error| format!("序列化用户 Agent frontmatter 失败：{}。", error))?;
    let content = format!(
        "---\n{}---\n\n{}\n",
        frontmatter_text,
        profile.prompt.trim()
    );

    fs::write(&path, content)
        .map_err(|error| format!("写入用户 Agent 文件 `{}` 失败：{}。", path.display(), error))
}

/// Returns the markdown file path used by one user-authored agent identifier.
/// 返回某个用户自定义 Agent 标识对应的 Markdown 文件路径。
fn user_agent_profile_file_path(agent_id: &str) -> PathBuf {
    agent_profiles_directory().join(format!("{agent_id}.md"))
}

/// Returns whether one legacy agent identifier belongs to the built-in runtime seed set.
/// 返回某个旧版 Agent 标识是否属于内建运行时种子集合。
fn is_legacy_builtin_agent_id(agent_id: &str) -> bool {
    matches!(
        agent_id,
        LEGACY_DEFAULT_WORKER_AGENT_ID
            | LEGACY_LUA_SPECIALIST_AGENT_ID
            | LEGACY_MEMORY_SPECIALIST_AGENT_ID
            | LEGACY_WORKSPACE_COORDINATOR_AGENT_ID
    )
}

/// Trims one optional string and returns `None` when it becomes empty.
/// 裁剪单个可选字符串，并在结果为空时返回 `None`。
fn trim_to_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Parses a comma-or-newline separated profile list into stable unique entries.
/// 将逗号或换行分隔的配置列表解析为稳定去重条目。
pub fn parse_agent_profile_list_text(value: &str) -> Vec<String> {
    normalize_string_list(
        value
            .split([',', '\n', '\r'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Formats one profile list for compact form editing.
/// 将配置列表格式化为适合表单编辑的紧凑文本。
pub fn format_agent_profile_list_text(values: &[String]) -> String {
    values.join(", ")
}

/// Parses an optional runtime-policy YAML object from one profile form field.
/// 从配置表单字段解析可选运行时策略 YAML 对象。
pub fn parse_runtime_policy_request_text(
    value: &str,
) -> Result<ExecutionRuntimePolicyRequest, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(ExecutionRuntimePolicyRequest::default());
    }

    serde_yaml::from_str(trimmed).map_err(|error| format!("解析 runtimePolicy 失败：{}。", error))
}

/// Formats runtime-policy overrides as YAML, or an empty field when no override exists.
/// 将运行时策略覆盖格式化为 YAML；没有覆盖时返回空字段。
pub fn format_runtime_policy_request_text(policy: ExecutionRuntimePolicyRequest) -> String {
    if !policy.has_override() {
        return String::new();
    }

    serde_yaml::to_string(&policy)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Normalizes one string list by trimming empties and preserving first-seen order.
/// 通过裁剪空项并保留首次出现顺序来规范化字符串列表。
fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || normalized.iter().any(|item| item == trimmed) {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}

/// Returns whether a runtime-policy request has no explicit overrides.
/// 返回运行时策略请求是否没有任何显式覆盖。
fn runtime_policy_request_is_empty(policy: &ExecutionRuntimePolicyRequest) -> bool {
    !policy.has_override()
}

/// Builds a compact stable identifier from one user-facing agent label.
/// 根据某个人类可读 Agent 标签构建紧凑稳定标识。
fn slugify_identifier(raw: &str) -> String {
    let mut identifier = String::new();
    let mut last_was_separator = false;

    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            identifier.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator {
            identifier.push('-');
            last_was_separator = true;
        }
    }

    identifier.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        format_runtime_policy_request_text, is_reserved_agent_identifier, load_user_agent_profile,
        parse_agent_profile_list_text, parse_runtime_policy_request_text,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Verifies that the reserved-agent guard includes the built-in analysis entries.
    /// 验证保留 Agent 保护逻辑已包含内置分析类条目。
    #[test]
    fn reserved_agent_identifiers_cover_built_in_analysis_entries() {
        assert!(is_reserved_agent_identifier("plan"));
        assert!(is_reserved_agent_identifier("explore"));
        assert!(is_reserved_agent_identifier("__system_default__"));
        assert!(!is_reserved_agent_identifier("custom-agent"));
    }

    /// Verifies advanced profile frontmatter survives parsing into the runtime-facing profile model.
    /// 验证高级 profile 前置元数据会被解析进面向运行时的配置模型。
    #[test]
    fn user_agent_profile_frontmatter_loads_subagent_overrides() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("vulcan-agent-profile-{unique}.md"));
        let content = r#"---
id: review-agent
name: Review Agent
description: Reviews code changes
model: deepseek/deepseek-chat
temperature: 0.3
maxTurns: 8
skills:
  - regular.review
  - luaskills.bridge
tools:
  - shell_command
  - shell_command
mcp:
  - server.tool
toolPolicy: analysis-safe
runtimePolicy:
  hooksEnabled: true
  memoryRecallEnabled: true
  search_projection_enabled: true
---

Review code carefully.
"#;
        fs::write(&path, content).expect("profile fixture should write");

        let profile = load_user_agent_profile(&path).expect("profile should parse");
        let _ = fs::remove_file(&path);

        assert_eq!(profile.id, "review-agent");
        assert_eq!(
            profile.skill_ids,
            vec![
                String::from("regular.review"),
                String::from("luaskills.bridge")
            ]
        );
        assert_eq!(profile.native_tools, vec![String::from("shell_command")]);
        assert_eq!(profile.mcp_tools, vec![String::from("server.tool")]);
        assert_eq!(profile.tool_policy.as_deref(), Some("analysis-safe"));
        assert_eq!(profile.runtime_policy.hooks_enabled, Some(true));
        assert_eq!(profile.runtime_policy.memory_recall_enabled, Some(true));
        assert_eq!(profile.runtime_policy.search_projection_enabled, Some(true));
    }

    /// Verifies the shared form helpers preserve list and runtime-policy semantics.
    /// 验证共享表单辅助函数会保持列表与运行时策略语义。
    #[test]
    fn profile_form_helpers_parse_lists_and_runtime_policy() {
        let tools = parse_agent_profile_list_text("shell_command, vulcan_file_list\nshell_command");
        assert_eq!(
            tools,
            vec![
                String::from("shell_command"),
                String::from("vulcan_file_list")
            ]
        );

        let policy = parse_runtime_policy_request_text(
            "hooksEnabled: true\nmemoryWritebackEnabled: false\n",
        )
        .expect("runtime policy yaml should parse");
        assert_eq!(policy.hooks_enabled, Some(true));
        assert_eq!(policy.memory_writeback_enabled, Some(false));

        let formatted = format_runtime_policy_request_text(policy);
        assert!(formatted.contains("hooksEnabled"));
        assert!(formatted.contains("memoryWritebackEnabled"));
    }
}
