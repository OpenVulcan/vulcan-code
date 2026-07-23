//! Deterministic user-visible agent profile loading and runtime form helpers.
//! 确定性的用户可见 Agent 配置加载与运行时表单辅助模块。

use crate::agent_runtime::config::{
    DEFAULT_WORKER_AGENT_ID, EXPLORE_AGENT_ID, LUA_SPECIALIST_AGENT_ID, MEMORY_SPECIALIST_AGENT_ID,
    PLAN_AGENT_ID, WORKSPACE_COORDINATOR_AGENT_ID,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use vulcan_runtime_assets::text_decode::decode_complete_user_text;
use vulcan_session_core::ExecutionRuntimePolicyRequest;

/// Relative directory name that stores user-authored agent markdown files.
/// 存放用户自定义 Agent Markdown 文件的相对目录名称。
const USER_AGENT_DIRECTORY_NAME: &str = "agents";

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
    /// Vulcan Plugin tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的 Vulcan 插件工具标识集合。
    pub plugin_tools: Vec<String>,
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

/// Aggregate user-agent catalog composed from the visible Markdown profiles.
/// 由可见 Markdown 配置组成的用户 Agent 目录聚合体。
#[derive(Debug, Clone, Default)]
pub struct AgentProfileCatalog {
    /// Visible user-authored agent profiles keyed by stable identifier.
    /// 以稳定标识为键保存的可见用户 Agent 配置集合。
    pub agents: BTreeMap<String, UserAgentProfile>,
}

/// Frontmatter schema accepted by one user-authored agent markdown file.
/// 单个用户自定义 Agent Markdown 文件接受的前置元数据结构。
#[derive(Debug, Clone, Default, Deserialize)]
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
    #[serde(default, alias = "skills")]
    pub skill_ids: Vec<String>,
    /// Native tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的原生工具标识集合。
    #[serde(default, alias = "tools")]
    pub native_tools: Vec<String>,
    /// Vulcan Plugin tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的 Vulcan 插件工具标识集合。
    #[serde(default)]
    pub plugin_tools: Vec<String>,
    /// MCP tool identifiers directly exposed by this profile.
    /// 该配置直接暴露的 MCP 工具标识集合。
    #[serde(default, alias = "mcp")]
    pub mcp_tools: Vec<String>,
    /// Optional shared tool-policy identifier.
    /// 可选的共享工具策略标识。
    pub tool_policy: Option<String>,
    /// Optional task-session runtime feature overrides.
    /// 可选的子代理运行时能力覆盖。
    #[serde(default)]
    pub runtime_policy: ExecutionRuntimePolicyRequest,
}

/// Returns the user-scoped root directory that stores Agent profile Markdown files.
/// 返回保存 Agent 配置 Markdown 文件的用户级根目录。
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

/// Loads the visible user-agent catalog from disk.
/// 从磁盘加载可见的用户 Agent 目录聚合体。
pub fn load_agent_profile_catalog() -> Result<AgentProfileCatalog, String> {
    let agents = load_user_agent_profiles()?;

    Ok(AgentProfileCatalog { agents })
}

/// Returns whether one Agent identifier is reserved by the built-in runtime.
/// 返回某个 Agent 标识是否被内置运行时保留。
pub fn is_reserved_agent_identifier(agent_id: &str) -> bool {
    matches!(
        agent_id.trim(),
        DEFAULT_WORKER_AGENT_ID
            | LUA_SPECIALIST_AGENT_ID
            | MEMORY_SPECIALIST_AGENT_ID
            | WORKSPACE_COORDINATOR_AGENT_ID
            | PLAN_AGENT_ID
            | EXPLORE_AGENT_ID
    )
}

/// Loads every visible user-authored agent profile from the user directory.
/// 从用户目录加载所有可见的用户自定义 Agent 配置。
fn load_user_agent_profiles() -> Result<BTreeMap<String, UserAgentProfile>, String> {
    // root is the single product-owned source directory for user-authored Agent markdown.
    // root 是产品持有的用户自定义 Agent Markdown 唯一来源目录。
    let root = agent_profiles_directory();
    load_user_agent_profiles_from_directory(&root)
}

/// Loads one deterministic Agent profile catalog from an explicit directory.
/// 从显式目录加载一份确定性的 Agent 配置目录。
///
/// # Parameters
/// 参数
/// `root` is the directory recursively searched for user-authored Markdown profiles.
/// `root` 是递归搜索用户自定义 Markdown 配置的目录。
///
/// # Returns
/// 返回值
/// A profile map when every file has a unique non-reserved identifier, otherwise an explicit configuration error.
/// 当每个文件都具有唯一且非保留标识时返回配置映射，否则返回明确配置错误。
fn load_user_agent_profiles_from_directory(
    root: &Path,
) -> Result<BTreeMap<String, UserAgentProfile>, String> {
    // root_metadata distinguishes a missing source from a broken or redirected symbolic-link source.
    // root_metadata 区分缺失来源与损坏或被重定向的符号链接来源。
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => {
            return Err(format!(
                "读取 Agent 根目录 `{}` 元数据失败：{}。",
                root.display(),
                error
            ));
        }
    };
    if root_metadata.file_type().is_symlink() {
        return Err(format!(
            "Agent 根目录 `{}` 不能是符号链接。",
            root.display()
        ));
    }
    if !root_metadata.is_dir() {
        return Err(format!("Agent 根路径 `{}` 不是目录。", root.display()));
    }

    // profiles is the validated runtime-facing catalog keyed by stable identifier.
    // profiles 是以稳定标识为键的已校验运行时目录。
    let mut profiles = BTreeMap::new();
    // profile_sources retains the exact first source path for deterministic duplicate diagnostics.
    // profile_sources 保留精确的首个来源路径，用于确定性的重复项诊断。
    let mut profile_sources = BTreeMap::<String, PathBuf>::new();
    // files is sorted because read_dir iteration order is explicitly unspecified across platforms.
    // files 会被排序，因为 read_dir 的迭代顺序在不同平台上明确未指定。
    let mut files = Vec::new();
    collect_markdown_files(root, &mut files)?;
    files.sort();

    // Each path is parsed exactly once after discovery order has been stabilized.
    // 每个路径在发现顺序稳定后只解析一次。
    for path in files {
        let profile = load_user_agent_profile(&path)?;
        if is_reserved_agent_identifier(&profile.id) {
            return Err(format!(
                "Agent 文件 `{}` 使用了系统保留标识 `{}`。",
                path.display(),
                profile.id
            ));
        }
        if let Some(previous_path) = profile_sources.get(&profile.id) {
            return Err(format!(
                "Agent 标识 `{}` 同时由文件 `{}` 与 `{}` 声明。",
                profile.id,
                previous_path.display(),
                path.display()
            ));
        }
        profile_sources.insert(profile.id.clone(), path);
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
        // file_type is read without following symbolic links so recursive discovery cannot escape or cycle.
        // file_type 在不跟随符号链接的情况下读取，使递归发现无法越界或形成循环。
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "读取 Agent 目录条目 `{}` 类型失败：{}。",
                entry.path().display(),
                error
            )
        })?;
        // path is the exact source surfaced in every validation diagnostic.
        // path 是所有校验诊断中展示的精确来源。
        let path = entry.path();

        if file_type.is_symlink() {
            return Err(format!(
                "Agent 配置路径 `{}` 不能是符号链接。",
                path.display()
            ));
        }

        if file_type.is_dir() {
            collect_markdown_files(&path, files)?;
            continue;
        }

        if file_type.is_file()
            && path
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
    let bytes = fs::read(path)
        .map_err(|error| format!("读取 Agent 文件 `{}` 失败：{}。", path.display(), error))?;
    let text = decode_complete_user_text(&bytes).map_err(|error| {
        format!(
            "Agent 文件 `{}` 不是有效 UTF-8 或 GB18030：{}。",
            path.display(),
            error
        )
    })?;
    let (frontmatter, prompt) = split_frontmatter_and_body(&text.content, path)?;
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
        plugin_tools: normalize_string_list(frontmatter.plugin_tools),
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
        load_user_agent_profiles_from_directory, parse_agent_profile_list_text,
        parse_runtime_policy_request_text,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Verifies that the reserved-agent guard includes the built-in analysis entries.
    /// 验证保留 Agent 保护逻辑已包含内置分析类条目。
    #[test]
    fn reserved_agent_identifiers_cover_built_in_analysis_entries() {
        assert!(is_reserved_agent_identifier("plan"));
        assert!(is_reserved_agent_identifier("explore"));
        assert!(is_reserved_agent_identifier("default-worker"));
        assert!(!is_reserved_agent_identifier("custom-agent"));
    }

    /// Verifies a user-authored GBK Agent Markdown file is decoded before frontmatter parsing.
    /// 验证用户编写的 GBK Agent Markdown 会在解析前置元数据前完成解码。
    #[test]
    fn user_agent_profile_decodes_gbk_markdown() {
        let root =
            tempfile::tempdir().expect("temporary GBK Agent profile directory should be created");
        let path = root.path().join("gbk-reviewer.md");
        let source = "---\nid: gbk-reviewer\nname: 中文审查员\n---\n\n检查服务端用户等级规则。\n";
        let (encoded, _, had_errors) = encoding_rs::GBK.encode(source);
        assert!(!had_errors);
        fs::write(&path, encoded.as_ref()).expect("GBK Agent profile should be written");

        let profile = load_user_agent_profile(&path).expect("GBK Agent profile should parse");

        assert_eq!(profile.id, "gbk-reviewer");
        assert_eq!(profile.name, "中文审查员");
        assert_eq!(profile.prompt, "检查服务端用户等级规则。");
    }

    /// Verifies duplicate canonical Agent identifiers fail deterministically instead of overwriting by read_dir order.
    /// 验证重复的规范 Agent 标识会确定性失败，而不是按 read_dir 顺序互相覆盖。
    #[test]
    fn user_agent_catalog_rejects_duplicate_identifiers_deterministically() {
        // root owns an automatically cleaned isolated profile hierarchy.
        // root 持有一套自动清理的隔离配置目录层级。
        let root =
            tempfile::tempdir().expect("temporary Agent profile directory should be created");
        // nested exercises recursive discovery instead of only direct children.
        // nested 覆盖递归发现，而不只覆盖直接子项。
        let nested = root.path().join("nested");
        fs::create_dir(&nested).expect("nested Agent profile directory should be created");
        // first_path and second_path intentionally declare the same canonical frontmatter id.
        // first_path 与 second_path 刻意声明相同的规范 frontmatter 标识。
        let first_path = root.path().join("first.md");
        let second_path = nested.join("second.md");
        fs::write(
            &first_path,
            "---\nid: duplicate-agent\n---\n\nFirst prompt.\n",
        )
        .expect("first duplicate profile should be written");
        fs::write(
            &second_path,
            "---\nid: duplicate-agent\n---\n\nSecond prompt.\n",
        )
        .expect("second duplicate profile should be written");

        // first_error and second_error prove repeated loads return the same ordered diagnostic.
        // first_error 与 second_error 证明重复加载会返回相同的有序诊断。
        let first_error = load_user_agent_profiles_from_directory(root.path())
            .expect_err("duplicate Agent identifiers should fail");
        let second_error = load_user_agent_profiles_from_directory(root.path())
            .expect_err("duplicate Agent identifiers should fail deterministically");

        assert_eq!(first_error, second_error);
        assert!(first_error.contains("duplicate-agent"));
        assert!(first_error.contains(&first_path.display().to_string()));
        assert!(first_error.contains(&second_path.display().to_string()));
    }

    /// Verifies a user profile cannot silently shadow or disappear behind a built-in Agent identifier.
    /// 验证用户配置不能通过内建 Agent 标识静默遮蔽或消失。
    #[test]
    fn user_agent_catalog_rejects_reserved_identifier() {
        // root owns an automatically cleaned isolated profile directory.
        // root 持有一个自动清理的隔离配置目录。
        let root =
            tempfile::tempdir().expect("temporary Agent profile directory should be created");
        // profile_path identifies the exact invalid user file expected in the diagnostic.
        // profile_path 标识诊断中预期出现的精确无效用户文件。
        let profile_path = root.path().join("plan.md");
        fs::write(&profile_path, "Plan prompt.\n")
            .expect("reserved Agent profile should be written");

        // error must surface the invalid source rather than silently skipping it.
        // error 必须暴露无效来源，而不是静默跳过。
        let error = load_user_agent_profiles_from_directory(root.path())
            .expect_err("reserved Agent identifier should fail");

        assert!(error.contains("plan"));
        assert!(error.contains(&profile_path.display().to_string()));
    }

    /// Verifies the catalog root must be an actual directory rather than an arbitrary filesystem entry.
    /// 验证配置目录根必须是真实目录，而不能是任意文件系统条目。
    #[test]
    fn user_agent_catalog_rejects_non_directory_root() {
        // root_file is an automatically cleaned regular file passed where a directory is required.
        // root_file 是一个自动清理的普通文件，被传入必须为目录的位置。
        let root_file = tempfile::NamedTempFile::new()
            .expect("temporary Agent profile root file should be created");

        // error must identify the exact invalid root instead of relying on a generic read_dir failure.
        // error 必须标识精确的无效根，而不是依赖通用 read_dir 失败。
        let error = load_user_agent_profiles_from_directory(root_file.path())
            .expect_err("non-directory Agent profile root should fail");

        assert!(error.contains("不是目录"));
        assert!(error.contains(&root_file.path().display().to_string()));
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
  databaseProjectionEnabled: true
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
        assert_eq!(
            profile.runtime_policy.database_projection_enabled,
            Some(true)
        );
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
