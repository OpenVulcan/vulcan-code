//! Permission gate for model tool calls: classifies each call, applies the session permission
//! mode's deterministic rules, and decides whether the call proceeds.
//! 模型工具调用的权限闸门：对每次调用分类，按会话权限模式的确定性规则判断是否执行。
//!
//! Layers run in order: mode gate, deterministic rules, then (when configured) the judge
//! model, a reviewing LLM, and finally the user. Every layer either decides or hands over;
//! errors never allow a call.
//! 各层依次运行：模式闸门、确定性规则，然后（配置时）判定模型、复核 LLM，最后是用户。
//! 每层要么作出决定，要么交给下一层；错误绝不放行。

pub mod command_rules;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use vulcan_host_tools::file_runtime::{apply_patch_write_paths, path_resolves_inside_workspace};
pub use vulcan_permission_judge::JudgeLabel;
use vulcan_session_core::v1::config::PermissionMode;
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationContext, VulcanToolSourceKind,
    tool_is_allowed_in_plan_with_args,
};

use crate::agent_runtime::question_runtime::ToolApprovalPrompt;
use crate::model::SessionId;
use crate::tooling::native::{
    APPLY_PATCH_TOOL_ID, FILE_EDIT_TOOL_ID, FILE_TOOL_ID, FILE_WRITE_TOOL_ID,
    SHELL_COMMAND_TOOL_ID, TERMINAL_SESSION_TOOL_ID,
};
pub use command_rules::HighRiskCategory;

/// Model-visible explanation returned when the user denies an escalated call.
/// 用户拒绝升级调用时返回给模型的说明。
pub const USER_DENIAL: &str = "The user denied permission for this call, so it was not executed. Do not retry it or work around the denial. Continue with other work, or stop and explain what you intended to do.";

/// Model-visible prefix of the explanation returned when the reviewing LLM denies a call.
/// 复核 LLM 拒绝调用时返回给模型的说明前缀。
pub const REVIEW_DENIAL_PREFIX: &str = "The permission reviewer denied this call, so it was not executed. Do not retry it or work around the denial. Reviewer's reason: ";

/// Model-visible explanation returned when the session permission mode cannot be read.
/// 无法读取会话权限模式时返回给模型的说明。
pub const MODE_UNAVAILABLE_DENIAL: &str = "The session permission mode could not be read, so this call was not executed. Tell the user that tool permissions are unavailable.";

/// `file` tool actions that only read.
/// 仅读取的 `file` 工具动作。
const FILE_READ_ACTIONS: &[&str] = &["list", "stat"];

/// Host services the permission gate needs from the service kernel.
/// 权限闸门需要服务内核提供的宿主能力。
#[async_trait::async_trait]
pub trait ToolPermissionHost: Send + Sync {
    /// Returns the live permission mode of `session_id`, read at call time.
    /// 返回 `session_id` 在调用时的实时权限模式。
    fn permission_mode(&self, session_id: SessionId) -> Result<PermissionMode, String>;

    /// Waits for the user's decision on one escalated call; `true` runs it once.
    /// 等待用户对一次升级调用的决定；`true` 表示执行一次。
    /// An error means the call cannot continue, exactly like a failed or abandoned Ask.
    /// 错误表示调用无法继续，与失败或被放弃的 Ask 完全相同。
    async fn request_approval(
        &self,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        arguments: Value,
        prompt: ToolApprovalPrompt,
    ) -> Result<bool, String>;

    /// Scores one escalated call with the configured judge model; `Ok(None)` means no judge is
    /// configured. An error means the judge is configured but could not score this call.
    /// 用已配置的判定模型为一次升级调用评分；`Ok(None)` 表示未配置判定器。错误表示已配置判定器
    /// 但无法为本次调用评分。
    async fn judge(&self, _call: JudgeCall) -> Result<Option<JudgeLabel>, String> {
        Ok(None)
    }

    /// Reviews one escalated call with the configured reviewing LLM; `Ok(None)` means no reviewer
    /// is configured. An error means the reviewer is configured but could not review this call.
    /// 用已配置的复核 LLM 复核一次升级调用；`Ok(None)` 表示未配置复核器。错误表示已配置复核器
    /// 但无法复核本次调用。
    async fn review(&self, _call: ReviewCall) -> Result<Option<LlmReview>, String> {
        Ok(None)
    }
}

/// Permission-only text carried with one call; neither persists.
/// 随一次调用携带、仅供权限使用的文本；两者都不持久化。
#[derive(Debug, Clone, Copy, Default)]
pub struct PermissionTexts<'a> {
    /// Argument text exactly as the model streamed it; restored calls have none.
    /// 模型流式发出的原样参数文本；恢复的调用没有该文本。
    pub raw_arguments: Option<&'a str>,
    /// Latest user-authored request of the conversation, when known.
    /// 会话中最近一条用户请求（已知时）。
    pub user_request: Option<&'a str>,
}

/// One escalated call as the reviewing LLM reads it.
/// 复核 LLM 所读取的一次升级调用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewCall {
    /// Workspace the call runs in; its project policy choices apply.
    /// 调用运行的工作区；适用其项目策略选择。
    pub workspace_root: Option<PathBuf>,
    /// Model-visible tool name.
    /// 模型可见的工具名称。
    pub tool_name: String,
    /// Streamed argument text, or the structured arguments as JSON for restored calls.
    /// 流式参数文本；恢复的调用使用结构化参数的 JSON。
    pub arguments: String,
    /// Latest user-authored request of the conversation, when known.
    /// 会话中最近一条用户请求（已知时）。
    pub user_request: Option<String>,
    /// Matched high-risk category, when any.
    /// 命中的高风险类别（如有）。
    pub high_risk: Option<HighRiskCategory>,
    /// Judge layer finding, when a judge is configured.
    /// 判定层结论（配置判定器时）。
    pub judge: Option<JudgeFinding>,
}

/// Decision of the reviewing LLM.
/// 复核 LLM 的决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    /// The call may run once.
    /// 调用可以执行一次。
    Allow,
    /// The user decides.
    /// 由用户决定。
    AskUser,
    /// The call must not run; the reason goes back to the model.
    /// 调用不得执行；原因返回给模型。
    Deny,
}

impl ReviewVerdict {
    /// Stable key shared by the structured output, projections, and clients.
    /// 结构化输出、投影与客户端共用的稳定键。
    pub fn key(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::AskUser => "ask_user",
            Self::Deny => "deny",
        }
    }
}

/// What the reviewing LLM returned for one call.
/// 复核 LLM 对一次调用的返回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmReview {
    /// Reviewer decision.
    /// 复核决定。
    pub verdict: ReviewVerdict,
    /// Reason in the user's language, shown on the approval card.
    /// 以用户语言写出的理由，显示在审批卡片上。
    pub reason: String,
    /// Whether the user lets the reviewer clear calls the judge blocked or a high-risk rule matched.
    /// 用户是否允许复核器放行判定器拦截或命中高风险规则的调用。
    pub may_allow_blocked: bool,
}

/// What the review layer found for a call it did not decide.
/// 复核层对未决定调用的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewFinding {
    /// The reviewer answered; an `Allow` appears when it was not permitted to clear the call.
    /// 复核器已回答；不允许其放行该调用时会出现 `Allow`。
    Reviewed(LlmReview),
    /// The reviewer is configured but could not review the call.
    /// 复核器已配置但无法复核调用。
    Unavailable(String),
}

/// One escalated call as the judge model reads it.
/// 判定模型所读取的一次升级调用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeCall {
    /// Workspace the call runs in; its project policy choices apply.
    /// 调用运行的工作区；适用其项目策略选择。
    pub workspace_root: Option<PathBuf>,
    /// Model-visible tool name.
    /// 模型可见的工具名称。
    pub tool_name: String,
    /// Argument text exactly as the model streamed it; restored calls have none.
    /// 模型流式发出的原样参数文本；恢复的调用没有该文本。
    pub raw_arguments: Option<String>,
}

/// What the judge layer found for a call it did not clear.
/// 判定层对未放行调用的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeFinding {
    /// The judge scored the call; `Allow` appears when a high-risk rule still needs the user.
    /// 判定器已评分；高风险规则仍需用户确认时会出现 `Allow`。
    Scored(JudgeLabel),
    /// The judge is configured but could not score the call.
    /// 判定器已配置但无法为调用评分。
    Unavailable(String),
}

/// What one tool call would affect, as far as permissions are concerned.
/// 就权限而言，一次工具调用会影响什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionSubject {
    /// Reads, session-internal state, network, or browser; never gated.
    /// 读取、会话内部状态、网络或浏览器；从不受闸门约束。
    Ungoverned,
    /// Writes to these raw paths, exactly as the model wrote them.
    /// 写入这些原始路径（与模型书写完全一致）。
    FileWrite {
        /// Raw paths; relative ones resolve against the workspace root.
        /// 原始路径；相对路径相对工作区根目录解析。
        paths: Vec<String>,
    },
    /// Runs shell text, either a one-shot command or text typed into a terminal.
    /// 运行 shell 文本：一次性命令或输入终端的文本。
    Command {
        /// Command text; empty means an interactive shell with no initial command.
        /// 命令文本；为空表示没有初始命令的交互式 shell。
        text: String,
    },
    /// An extension tool whose effects are unknown and not classified read-only.
    /// 效果未知且未被归类为只读的扩展工具。
    Opaque,
}

/// Outcome of the deterministic rule layer.
/// 确定性规则层的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOutcome {
    /// The call runs without further checks.
    /// 调用直接执行，不再检查。
    Allow,
    /// The call needs a later layer's decision.
    /// 调用需要后续层作出决定。
    Escalate {
        /// Matched high-risk category; a judge model may never allow these alone.
        /// 命中的高风险类别；判定模型不能单独放行。
        high_risk: Option<HighRiskCategory>,
    },
}

/// Final decision of the permission gate for one call.
/// 权限闸门对一次调用的最终决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPermissionVerdict {
    /// Run the tool.
    /// 执行工具。
    Proceed,
    /// Do not run the tool; the text is returned to the model as the result.
    /// 不执行工具；文本作为结果返回给模型。
    Deny(String),
    /// The approval could not finish; the call fails with this error, like a failed Ask.
    /// 审批无法完成；调用以此错误失败，与失败的 Ask 相同。
    Fail(String),
}

/// Classifies one tool call by what it would affect.
/// 按影响对象对一次工具调用分类。
/// Malformed arguments of a writing tool classify as `Opaque`, never as `Ungoverned`.
/// 写入类工具的参数格式错误时归为 `Opaque`，绝不归为 `Ungoverned`。
pub fn classify_tool_call(definition: &ResolvedToolDefinition, args: &Value) -> PermissionSubject {
    match definition.descriptor.source {
        VulcanToolSourceKind::Native => classify_native(&definition.descriptor.id, args),
        VulcanToolSourceKind::Mcp
        | VulcanToolSourceKind::LuaSkills
        | VulcanToolSourceKind::VulcanPlugin => {
            // Extension tools reuse the Plan-mode read-only classification.
            // 扩展工具复用计划模式的只读分类。
            if tool_is_allowed_in_plan_with_args(definition, args) {
                PermissionSubject::Ungoverned
            } else {
                PermissionSubject::Opaque
            }
        }
        VulcanToolSourceKind::VulcanManaged | VulcanToolSourceKind::AiHostBinding => {
            PermissionSubject::Ungoverned
        }
    }
}

/// Classifies one native tool call by its canonical id and arguments.
/// 按规范标识与参数对一次原生工具调用分类。
fn classify_native(tool_id: &str, args: &Value) -> PermissionSubject {
    let string_arg = |field: &str| args.get(field).and_then(Value::as_str).map(str::to_string);
    match tool_id {
        FILE_TOOL_ID => {
            let action = args.get("action").and_then(Value::as_str);
            if action.is_some_and(|action| FILE_READ_ACTIONS.contains(&action)) {
                return PermissionSubject::Ungoverned;
            }
            let paths: Vec<String> = [string_arg("path"), string_arg("destination")]
                .into_iter()
                .flatten()
                .collect();
            if paths.is_empty() {
                PermissionSubject::Opaque
            } else {
                PermissionSubject::FileWrite { paths }
            }
        }
        FILE_WRITE_TOOL_ID | FILE_EDIT_TOOL_ID => match string_arg("path") {
            Some(path) => PermissionSubject::FileWrite { paths: vec![path] },
            None => PermissionSubject::Opaque,
        },
        APPLY_PATCH_TOOL_ID => {
            match string_arg("patch").map(|patch| apply_patch_write_paths(&patch)) {
                Some(Ok(paths)) if !paths.is_empty() => PermissionSubject::FileWrite { paths },
                _ => PermissionSubject::Opaque,
            }
        }
        SHELL_COMMAND_TOOL_ID => match string_arg("command") {
            Some(text) => PermissionSubject::Command { text },
            None => PermissionSubject::Opaque,
        },
        TERMINAL_SESSION_TOOL_ID => classify_terminal(args),
        _ => PermissionSubject::Ungoverned,
    }
}

/// Classifies one `terminal_session` call: starting a shell or typing text runs commands.
/// 对一次 `terminal_session` 调用分类：启动 shell 或输入文本会运行命令。
/// Key and chord inputs only submit or interrupt text that was itself already gated.
/// 按键与组合键输入只提交或中断已经过闸门的文本。
fn classify_terminal(args: &Value) -> PermissionSubject {
    match args.get("action").and_then(Value::as_str) {
        Some("start") => PermissionSubject::Command {
            text: args
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        Some("send") => {
            let typed: Vec<&str> = args
                .get("inputs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|input| {
                    matches!(
                        input.get("kind").and_then(Value::as_str),
                        Some("text" | "paste")
                    )
                })
                .filter_map(|input| input.get("text").and_then(Value::as_str))
                .collect();
            if typed.is_empty() {
                PermissionSubject::Ungoverned
            } else {
                PermissionSubject::Command {
                    text: typed.join("\n"),
                }
            }
        }
        _ => PermissionSubject::Ungoverned,
    }
}

/// Applies the deterministic rules of `mode` to one classified call.
/// 对一次已分类调用应用 `mode` 的确定性规则。
/// `workspace_root` bounds direct file writes; without one, every file write escalates.
/// `workspace_root` 限定可直接写入的范围；没有工作区时所有文件写入都需升级。
pub fn evaluate_rules(
    mode: PermissionMode,
    subject: &PermissionSubject,
    workspace_root: Option<&Path>,
) -> RuleOutcome {
    if mode == PermissionMode::FullAccess {
        return RuleOutcome::Allow;
    }
    match subject {
        PermissionSubject::Ungoverned => RuleOutcome::Allow,
        PermissionSubject::Command { text } => {
            if let Some(category) = command_rules::high_risk_category(text) {
                RuleOutcome::Escalate {
                    high_risk: Some(category),
                }
            } else if !text.trim().is_empty() && command_rules::is_known_read_only(text) {
                RuleOutcome::Allow
            } else {
                RuleOutcome::Escalate { high_risk: None }
            }
        }
        PermissionSubject::FileWrite { paths } => {
            let inside = mode == PermissionMode::WorkspaceWrite
                && workspace_root.is_some_and(|root| {
                    paths
                        .iter()
                        .all(|path| path_resolves_inside_workspace(root, path).unwrap_or(false))
                });
            if inside {
                RuleOutcome::Allow
            } else {
                RuleOutcome::Escalate { high_risk: None }
            }
        }
        PermissionSubject::Opaque => RuleOutcome::Escalate { high_risk: None },
    }
}

/// Asks the judge about one escalated call; `None` when no judge is configured.
/// 就一次升级调用询问判定器；未配置判定器时返回 `None`。
async fn judge_layer(
    host: &Arc<dyn ToolPermissionHost>,
    definition: &ResolvedToolDefinition,
    raw_arguments: Option<&str>,
    workspace_root: Option<&Path>,
) -> Option<JudgeFinding> {
    let call = JudgeCall {
        workspace_root: workspace_root.map(Path::to_path_buf),
        tool_name: definition.descriptor.id.clone(),
        raw_arguments: raw_arguments.map(str::to_string),
    };
    match host.judge(call).await {
        Ok(label) => label.map(JudgeFinding::Scored),
        Err(reason) => Some(JudgeFinding::Unavailable(reason)),
    }
}

/// Asks the host's reviewing LLM about one call; `None` when no reviewer is configured.
/// 请宿主的复核 LLM 复核一次调用；未配置复核器时返回 `None`。
async fn review_layer(
    host: &Arc<dyn ToolPermissionHost>,
    call: ReviewCall,
) -> Option<ReviewFinding> {
    match host.review(call).await {
        Ok(review) => review.map(ReviewFinding::Reviewed),
        Err(reason) => Some(ReviewFinding::Unavailable(reason)),
    }
}

/// Decides whether one tool call proceeds, reading the session's live permission mode.
/// 读取会话实时权限模式，决定一次工具调用是否执行。
/// In workspace-write mode an escalated call is first scored by the judge, which may clear it
/// unless a high-risk rule matched. The reviewing LLM then sees what the judge did not clear: it
/// may deny, or clear calls that are neither judge-blocked nor high-risk (those only when the user
/// allows it). Everything else goes to the user. Read-only escalates every write straight to the
/// user.
/// 在工作区写入模式下，升级调用先由判定器评分；除非命中高风险规则，判定器可放行。复核 LLM 随后
/// 查看判定器未放行的调用：它可以拒绝，也可以放行既未被判定器拦截也未命中高风险的调用（这两类
/// 仅在用户允许时可放行）。其余全部交给用户。只读模式会把所有写入直接交给用户。
/// Without a host (embedded or test runtimes) no permission policy is configured and calls proceed.
/// 没有宿主时（嵌入式或测试运行时）未配置权限策略，调用直接执行。
pub async fn admit_tool_call(
    host: Option<&Arc<dyn ToolPermissionHost>>,
    session_id: SessionId,
    definition: &ResolvedToolDefinition,
    args: &Value,
    texts: PermissionTexts<'_>,
    context: &VulcanToolInvocationContext,
    workspace_root: Option<&Path>,
) -> ToolPermissionVerdict {
    let Some(host) = host else {
        return ToolPermissionVerdict::Proceed;
    };
    let mode = match host.permission_mode(session_id) {
        Ok(mode) => mode,
        Err(_) => return ToolPermissionVerdict::Deny(String::from(MODE_UNAVAILABLE_DENIAL)),
    };
    let subject = classify_tool_call(definition, args);
    let RuleOutcome::Escalate { high_risk } = evaluate_rules(mode, &subject, workspace_root) else {
        return ToolPermissionVerdict::Proceed;
    };
    let judge = if mode == PermissionMode::WorkspaceWrite {
        judge_layer(host, definition, texts.raw_arguments, workspace_root).await
    } else {
        None
    };
    if high_risk.is_none() && judge == Some(JudgeFinding::Scored(JudgeLabel::Allow)) {
        return ToolPermissionVerdict::Proceed;
    }
    let review = if mode == PermissionMode::WorkspaceWrite {
        let call = ReviewCall {
            workspace_root: workspace_root.map(Path::to_path_buf),
            tool_name: definition.descriptor.id.clone(),
            arguments: texts
                .raw_arguments
                .map_or_else(|| args.to_string(), str::to_string),
            user_request: texts.user_request.map(str::to_string),
            high_risk,
            judge: judge.clone(),
        };
        review_layer(host, call).await
    } else {
        None
    };
    if let Some(ReviewFinding::Reviewed(review)) = &review {
        let blocked = high_risk.is_some() || judge == Some(JudgeFinding::Scored(JudgeLabel::Block));
        match review.verdict {
            ReviewVerdict::Allow if !blocked || review.may_allow_blocked => {
                return ToolPermissionVerdict::Proceed;
            }
            ReviewVerdict::Deny => {
                return ToolPermissionVerdict::Deny(format!(
                    "{REVIEW_DENIAL_PREFIX}{}",
                    review.reason
                ));
            }
            ReviewVerdict::Allow | ReviewVerdict::AskUser => {}
        }
    }
    let prompt = ToolApprovalPrompt {
        mode,
        subject,
        high_risk,
        judge,
        review,
    };
    match host
        .request_approval(
            session_id,
            context.clone(),
            definition.descriptor.id.clone(),
            args.clone(),
            prompt,
        )
        .await
    {
        Ok(true) => ToolPermissionVerdict::Proceed,
        Ok(false) => ToolPermissionVerdict::Deny(String::from(USER_DENIAL)),
        Err(error) => ToolPermissionVerdict::Fail(error),
    }
}
