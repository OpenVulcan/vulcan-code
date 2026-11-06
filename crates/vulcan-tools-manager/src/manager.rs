use super::*;

/// Model-facing answer for a call that Plan mode blocks before it can run.
/// 计划模式在调用执行前拦截时返回给模型的回答。
pub const PLAN_MODE_BLOCKED_MESSAGE: &str = "Plan Mode is active. This tool call is not allowed because it may mutate workspace, repository, Git, or external state. Continue by refining the plan and wait for the user to leave Plan Mode before implementing changes.";

/// Host-owned tool manager that normalizes candidates, resolves mode-specific surfaces, and dispatches invocations.
/// 宿主自有工具管理器，负责归一化候选工具、解析模式化工具面并调度调用。
#[derive(Clone, Default)]
pub struct VulcanToolsManager {
    /// Candidate tools aggregated from host-owned native, LuaSkills, MCP, or future expert sources.
    /// 从宿主自有原生、LuaSkills、MCP 或未来专家来源聚合得到的候选工具集合。
    candidates: Vec<VulcanToolCandidate>,
    /// Workspace root that relative tool paths resolve against, when the execution has one.
    /// 相对工具路径解析所依据的工作区根目录（执行存在工作区时）。
    workspace_root: Option<std::path::PathBuf>,
}

impl VulcanToolsManager {
    /// Builds one unified tool manager from the current host-owned candidate surfaces.
    /// 根据当前宿主自有候选工具面构建统一工具管理器。
    pub fn new(candidates: Vec<VulcanToolCandidate>) -> Self {
        Self {
            candidates: normalize_candidate_public_tool_names(candidates),
            workspace_root: None,
        }
    }

    /// Records the workspace root that the candidates' tools resolve relative paths against.
    /// 记录候选工具解析相对路径所依据的工作区根目录。
    pub fn with_workspace_root(mut self, workspace_root: Option<std::path::PathBuf>) -> Self {
        self.workspace_root = workspace_root;
        self
    }

    /// Returns the workspace root shared by this manager's tools, when any.
    /// 返回本管理器工具共享的工作区根目录（如有）。
    pub fn workspace_root(&self) -> Option<&std::path::Path> {
        self.workspace_root.as_deref()
    }

    /// Resolves the final AI-visible tool set for one execution-mode label.
    /// 为单条执行模式标签解析最终 AI 可见工具集合。
    pub fn resolve_for_execution_mode(&self, _execution_mode_label: &str) -> ResolvedToolSet {
        let mut descriptors = Vec::new();
        let mut definitions = Vec::new();

        for candidate in self
            .candidates
            .iter()
            .filter(|candidate| candidate.ai_visible)
        {
            descriptors.push(candidate.descriptor.clone());
            definitions.push(ResolvedToolDefinition {
                descriptor: candidate.descriptor.clone(),
                parameters_schema: candidate.parameters_schema.clone(),
                plan_policy: candidate.plan_policy.clone(),
                request_streaming: candidate.request_streaming.clone(),
                invoke: candidate.invoke.clone(),
            });
        }

        ResolvedToolSet {
            descriptors,
            definitions,
        }
    }

    /// Dispatches one resolved tool call through its host-owned invocation closure.
    /// 通过宿主自有调用闭包调度单个已解析工具调用。
    pub async fn invoke_tool(
        &self,
        tool: &ResolvedToolDefinition,
        args: Value,
        context: VulcanToolInvocationContext,
        execution_mode_label: &str,
    ) -> Result<VulcanToolInvocationResult, String> {
        if Self::plan_mode_blocks(tool, &args, execution_mode_label) {
            return Ok(VulcanToolInvocationResult::from_model_output(
                Value::String(String::from(PLAN_MODE_BLOCKED_MESSAGE)),
            ));
        }
        (tool.invoke)(args, context).await
    }

    /// Returns whether Plan mode blocks this exact call; such calls never reach permission checks.
    /// 返回计划模式是否拦截这次精确调用；被拦截的调用不会进入权限检查。
    pub fn plan_mode_blocks(
        tool: &ResolvedToolDefinition,
        args: &Value,
        execution_mode_label: &str,
    ) -> bool {
        matches!(
            VulcanToolMode::from_execution_mode_label(execution_mode_label),
            VulcanToolMode::Plan
        ) && !tool_is_allowed_in_plan_with_args(tool, args)
    }
}
/// Returns whether one resolved tool may execute while Plan mode is active.
/// 返回单个已解析工具在计划模式激活时是否允许执行。
#[cfg(test)]
pub(super) fn tool_is_allowed_in_plan(tool: &ResolvedToolDefinition) -> bool {
    tool_is_allowed_in_plan_with_args(tool, &Value::Null)
}

/// Returns whether one resolved tool and its exact arguments may execute in Plan mode.
/// 返回单个已解析工具及其精确参数是否允许在计划模式中执行。
/// The permission gate reuses this as the read-only classification of opaque extension tools.
/// 权限闸门复用此判断作为不透明扩展工具的只读分类。
pub fn tool_is_allowed_in_plan_with_args(tool: &ResolvedToolDefinition, args: &Value) -> bool {
    match &tool.plan_policy {
        VulcanPlanToolPolicy::Hidden => false,
        VulcanPlanToolPolicy::Visible => true,
        VulcanPlanToolPolicy::HeuristicText(text) => text_looks_read_only(text),
        VulcanPlanToolPolicy::HeuristicStringArgument { field } => args
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(text_looks_read_only),
        VulcanPlanToolPolicy::StringArgumentAllowlist {
            field,
            allowed_values,
        } => args
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| allowed_values.iter().any(|allowed| allowed == value)),
    }
}

/// Returns whether a tool name `text` reads as a read-only operation.
/// 返回工具名称 `text` 是否表现为只读操作。
///
/// The name is split into words at separators and camelCase boundaries, so a keyword inside an
/// unrelated word (`rg` in `merge`, `read` in `thread`, `get` in `target`) never counts. A name
/// that also carries an unambiguous write verb (`get_or_create`, `search_and_replace`) is not
/// read-only. The permission gate applies the same heuristic to implicit LuaSkills entries named in
/// a dispatch and to session MCP calls.
/// 名称按分隔符与驼峰边界拆分为单词，因此关键字出现在无关单词内部（`merge` 中的 `rg`、`thread` 中的
/// `read`、`target` 中的 `get`）时不计。名称同时带有明确写入动词（`get_or_create`、`search_and_replace`）
/// 时不视为只读。权限闸门对调度中指定的隐式 LuaSkills 入口与会话 MCP 调用应用同一启发式。
pub fn text_looks_read_only(text: &str) -> bool {
    let words = name_words(text);
    words.iter().any(|word| word_is_read_only_keyword(word))
        && !words
            .iter()
            .any(|word| PLAN_WRITE_VERBS.contains(&word.as_str()))
}

/// Splits one tool name into lowercase words at non-alphanumeric separators, camelCase
/// boundaries and acronym ends (`getHTTPStatus` → get, http, status).
/// 将一个工具名称在非字母数字分隔符、驼峰边界与缩写结尾处拆分为小写单词（`getHTTPStatus` → get、http、status）。
fn name_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    for part in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        let chars: Vec<char> = part.chars().collect();
        let mut current = String::new();
        for (index, &c) in chars.iter().enumerate() {
            let boundary = index > 0 && c.is_ascii_uppercase() && {
                let previous = chars[index - 1];
                previous.is_ascii_lowercase()
                    || previous.is_ascii_digit()
                    || (previous.is_ascii_uppercase()
                        && chars.get(index + 1).is_some_and(char::is_ascii_lowercase))
            };
            if boundary && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(c.to_ascii_lowercase());
        }
        if !current.is_empty() {
            words.push(current);
        }
    }
    words
}

/// Returns whether one lowercase word is a read-only keyword or a plain inflection of one.
/// Keywords in `PLAN_READ_ONLY_AFFIX_KEYWORDS` also count at the start or end of a run-together
/// word (`websearch`, `prefetch`, `eslint`); the others must stand alone, because they hide inside
/// unrelated words (`check` in `checkout`, `get` in `target`).
/// 返回一个小写单词是否为只读关键字或其简单屈折形式。`PLAN_READ_ONLY_AFFIX_KEYWORDS` 中的关键字在连写
/// 单词的开头或结尾也计入（`websearch`、`prefetch`、`eslint`）；其余关键字必须独立成词，因为它们会藏在
/// 无关单词中（`checkout` 中的 `check`、`target` 中的 `get`）。
fn word_is_read_only_keyword(word: &str) -> bool {
    PLAN_READ_ONLY_KEYWORDS.iter().any(|keyword| {
        word == *keyword
            || ["s", "es", "ing", "ed"]
                .iter()
                .any(|suffix| word.strip_suffix(suffix) == Some(keyword))
    }) || PLAN_READ_ONLY_AFFIX_KEYWORDS
        .iter()
        .any(|keyword| word.starts_with(keyword) || word.ends_with(keyword))
}

/// Read-only keywords that stay recognizable inside a run-together word.
/// 在连写单词中仍可可靠识别的只读关键字。
const PLAN_READ_ONLY_AFFIX_KEYWORDS: &[&str] = &[
    "search",
    "fetch",
    "query",
    "grep",
    "lint",
    "detail",
    "symbol",
    "reference",
    "describe",
    "typecheck",
];

/// Read-only keywords of the shared Plan-mode and permission name heuristic.
/// 计划模式与权限共享名称启发式的只读关键字。
const PLAN_READ_ONLY_KEYWORDS: &[&str] = &[
    "read",
    "list",
    "rg",
    "grep",
    "search",
    "find",
    "tree",
    "ast",
    "detail",
    "menu",
    "symbol",
    "hover",
    "reference",
    "fetch",
    "get",
    "query",
    "check",
    "lint",
    "typecheck",
    "test",
    "describe",
];

/// Unambiguous write verbs that keep a name with a read-only keyword out of the read-only class.
/// Matched as whole words only; nouns that double as verbs (`post`, `comment`, `commit`) are left
/// out so reading tools such as `get_post` stay read-only.
/// 明确的写入动词：名称即使含只读关键字，只要带有其一就不归为只读。只按整词匹配；兼作名词的动词
/// （`post`、`comment`、`commit`）不列入，使 `get_post` 等读取工具保持只读。
const PLAN_WRITE_VERBS: &[&str] = &[
    "create",
    "update",
    "delete",
    "remove",
    "write",
    "edit",
    "modify",
    "insert",
    "upsert",
    "add",
    "append",
    "merge",
    "rename",
    "replace",
    "overwrite",
    "truncate",
    "purge",
    "install",
    "uninstall",
    "deploy",
    "publish",
    "upload",
    "submit",
    "send",
    "revert",
    "rebase",
    "checkout",
    "reset",
    "kill",
    "cancel",
    "set",
    "apply",
    "clear",
    "move",
    "save",
    "transfer",
    "approve",
    "assign",
    "restore",
    "drop",
    "push",
];
