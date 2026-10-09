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

use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use vulcan_host_tools::file_runtime::{apply_patch_write_paths, path_resolves_inside_workspace};
use vulcan_session_core::v1::config::PermissionMode;
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationContext, VulcanToolSourceKind,
    tool_is_allowed_in_plan_with_args,
};

use crate::agent_runtime::question_runtime::ToolApprovalPrompt;
use crate::model::SessionId;
use crate::tooling::native::{
    APPLY_PATCH_TOOL_ID, FILE_EDIT_TOOL_ID, FILE_TOOL_ID, SHELL_COMMAND_TOOL_ID,
    TERMINAL_SESSION_TOOL_ID,
};
pub use command_rules::HighRiskCategory;

/// Model-visible explanation returned when the user denies an escalated call.
/// 用户拒绝升级调用时返回给模型的说明。
pub const USER_DENIAL: &str = "The user denied permission for this call, so it was not executed. Do not retry it or work around the denial. Continue with other work, or stop and explain what you intended to do.";

/// Model-visible explanation returned when the session permission mode cannot be read.
/// 无法读取会话权限模式时返回给模型的说明。
pub const MODE_UNAVAILABLE_DENIAL: &str = "The session permission mode could not be read, so this call was not executed. Tell the user that tool permissions are unavailable.";

/// `file` tool actions that only read.
/// 仅读取的 `file` 工具动作。
const FILE_READ_ACTIONS: &[&str] = &["read", "list", "stat"];

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
        FILE_EDIT_TOOL_ID => match string_arg("path") {
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

/// Decides whether one tool call proceeds, reading the session's live permission mode.
/// 读取会话实时权限模式，决定一次工具调用是否执行。
/// Escalated calls go to the user in every gated mode; read-only escalates every write.
/// 在所有受管控模式下升级调用都交给用户；只读模式会升级所有写入。
/// Without a host (embedded or test runtimes) no permission policy is configured and calls proceed.
/// 没有宿主时（嵌入式或测试运行时）未配置权限策略，调用直接执行。
pub async fn admit_tool_call(
    host: Option<&Arc<dyn ToolPermissionHost>>,
    session_id: SessionId,
    definition: &ResolvedToolDefinition,
    args: &Value,
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
    let prompt = ToolApprovalPrompt {
        mode,
        subject,
        high_risk,
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
