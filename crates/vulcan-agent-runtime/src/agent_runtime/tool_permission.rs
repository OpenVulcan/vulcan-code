//! Permission gate for model tool calls: classifies each call, applies the session permission
//! mode's deterministic rules, and decides whether the call proceeds.
//! 模型工具调用的权限闸门：对每次调用分类，按会话权限模式的确定性规则判断是否执行。
//!
//! Layers run in order: mode gate, deterministic rules, then (when configured) the judge
//! model, a reviewing LLM, and finally the user. Every layer either decides or hands over;
//! errors never allow a call.
//! 各层依次运行：模式闸门、确定性规则，然后（配置时）判定模型、复核 LLM，最后是用户。
//! 每层要么作出决定，要么交给下一层；错误绝不放行。

pub(crate) mod builtin_luaskills;
pub mod command_rules;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;
use vulcan_host_tools::file_runtime::{
    apply_patch_write_paths, path_resolves_inside_workspace, path_targets_git_metadata,
};
use vulcan_host_tools::shell_host::HostCommandShell;
pub use vulcan_permission_judge::JudgeLabel;
use vulcan_session_core::v1::config::{AccessConfig, PermissionMode};
use vulcan_session_core::v1::ids::TurnId;
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanToolInvocationContext, VulcanToolSourceKind,
    text_looks_read_only, tool_is_allowed_in_plan_with_args,
};

use crate::agent_runtime::question_runtime::{ApprovalAnswer, ToolApprovalPrompt};
use crate::model::SessionId;
use crate::tooling::native::{
    APPLY_PATCH_TOOL_ID, CONFIGURATION_LIST_ACTION, DECLARE_INTENT_TOOL_ID, FEEDBACK_DRAFT_ACTION,
    FEEDBACK_TOOL_ID, FILE_EDIT_TOOL_ID, FILE_TOOL_ID, FILE_WRITE_TOOL_ID,
    LUASKILLS_DEBUG_STATUS_ACTION, LUASKILLS_DEBUG_TOOL_ID, MAX_PERMISSION_REASON_CHARS,
    MCP_CALL_TOOL_ID, MCP_CONFIG_TOOL_ID, MCP_CONNECT_TOOL_ID, PROVIDER_CONFIG_TOOL_ID,
    SHELL_COMMAND_TOOL_ID, TERMINAL_SESSION_TOOL_ID, declared_intent, session_mcp_command_line,
};
pub use command_rules::HighRiskCategory;

/// Model-visible explanation returned when the user denies an escalated call.
/// 用户拒绝升级调用时返回给模型的说明。
pub const USER_DENIAL: &str = "The user denied permission for this call, so it was not executed. Do not retry it or work around the denial. Continue with other work, or stop and explain what you intended to do.";

/// Model-visible explanation returned when nobody answers an approval before its wait runs out.
/// 审批等待时间耗尽前无人回答时返回给模型的说明。
pub const APPROVAL_EXPIRED_DENIAL: &str = "Nobody answered the approval prompt for this call before its wait ran out, so it was not executed. Do not retry it now. Continue with other work, or stop and explain what you intended to do so the user can approve it later.";

/// Model-visible prefix of the explanation returned when the reviewing LLM denies a call.
/// 复核 LLM 拒绝调用时返回给模型的说明前缀。
pub const REVIEW_DENIAL_PREFIX: &str = "The permission reviewer denied this call, so it was not executed. Do not retry it or work around the denial. Reviewer's reason: ";

/// Model-visible explanation returned when the session permission mode cannot be read.
/// 无法读取会话权限模式时返回给模型的说明。
pub const MODE_UNAVAILABLE_DENIAL: &str = "The session permission mode could not be read, so this call was not executed. Tell the user that tool permissions are unavailable.";

/// Denial of a configuration change in an unattended session, written for the agent.
/// 无人值守会话中配置变更被拒绝的说明，写给代理。
pub const CONFIGURATION_UNATTENDED_DENIAL: &str = "Configuration changes need the user's approval and no user can give it in an unattended session, so this change was not executed. Do not retry it. Describe the change you intended in your final answer so the user can make it later, and continue with the rest of the task.";

/// Consecutive denials of one identical call an unattended session reviews; the next identical
/// call is denied without review, so the agent cannot wear the reviewer down.
/// 无人值守会话对同一调用连续拒绝时复核的次数；下一次相同调用不经复核直接拒绝，避免代理磨损复核器。
pub const UNATTENDED_REVIEWED_REPEATS: u32 = 2;

/// Guidance after an unattended denial that a stated reason could still change: the call stated
/// none, the reviewer read it, and the reviewer may clear it. Only this text invites
/// `declare_intent`, and only after it may that tool be called.
/// 无人值守拒绝后、说明理由仍可能改变结论时的指引：调用未说明理由、复核器已读过且复核器可以放行。
/// 只有这段文字邀请使用 `declare_intent`，也只有在它之后才可调用该工具。
const UNATTENDED_RETRY_GUIDANCE: &str = "No user is watching this unattended session. If the call is necessary and safe, you may retry it once with your reason: state it in the `title` of `shell_command` or the `purpose` of `terminal_session`, or call `declare_intent` right before retrying any other tool. Otherwise take another approach, or skip this step and continue with the rest of the task.";

/// Guidance after an unattended denial that no stated reason can change: the reviewer already read
/// one, could not review, or may not clear the call.
/// 无人值守拒绝后、任何理由都无法改变结论时的指引：复核器已读过理由、无法复核或不得放行该调用。
const UNATTENDED_MOVE_ON_GUIDANCE: &str = "No user is watching this unattended session, and stating a reason will not change this decision. Do not retry this call. Take another approach, or skip this step and leave it for the user's later instructions while you finish the rest of the task.";

/// Model-visible refusal of a `declare_intent` call that no denial invited.
/// 未经拒绝邀请的 `declare_intent` 调用所收到的模型可见拒绝。
pub const DECLARE_INTENT_UNINVITED: &str = "declare_intent is only available right after a tool call was denied with a message that tells you to use it. It is not a note-taking tool. No intent was recorded.";

/// `file` tool actions that only read.
/// 仅读取的 `file` 工具动作。
const FILE_READ_ACTIONS: &[&str] = &["list", "stat"];

/// Host services the permission gate needs from the service kernel.
/// 权限闸门需要服务内核提供的宿主能力。
#[async_trait::async_trait]
pub trait ToolPermissionHost: Send + Sync {
    /// Returns the live access selections of `session_id`, read at call time.
    /// 返回 `session_id` 在调用时的实时授权选择。
    fn access(&self, session_id: SessionId) -> Result<AccessConfig, String>;

    /// In-memory per-session state of the gate: declared intents and repeated denials.
    /// `None` keeps no state, so intents are dropped and repeats are never limited.
    /// 闸门的会话级内存状态：已声明意图与重复拒绝。`None` 不保存状态，意图被丢弃，重复也不受限。
    fn memory(&self) -> Option<&PermissionMemory> {
        None
    }

    /// Waits for the user's decision on one escalated call; the host may deny it unanswered once
    /// the user's approval wait runs out.
    /// 等待用户对一次升级调用的决定；用户设定的审批等待时间耗尽后，宿主可以在无人回答时拒绝它。
    /// An error means the call cannot continue, exactly like a failed or abandoned Ask.
    /// 错误表示调用无法继续，与失败或被放弃的 Ask 完全相同。
    async fn request_approval(
        &self,
        session_id: SessionId,
        context: VulcanToolInvocationContext,
        tool_name: String,
        arguments: Value,
        prompt: ToolApprovalPrompt,
    ) -> Result<ApprovalAnswer, String>;

    /// Scores one escalated call with the configured judge model; `Ok(None)` means no judge is
    /// configured. An error means the judge is configured but could not score this call.
    /// 用已配置的判定模型为一次升级调用评分；`Ok(None)` 表示未配置判定器。错误表示已配置判定器
    /// 但无法为本次调用评分。
    async fn judge(&self, _call: JudgeCall) -> Result<Option<JudgeVerdict>, JudgeFailure> {
        Ok(None)
    }

    /// Reviews one escalated call with the configured reviewing LLM; `Ok(None)` means no reviewer
    /// is configured. An error means the reviewer is configured but could not review this call.
    /// 用已配置的复核 LLM 复核一次升级调用；`Ok(None)` 表示未配置复核器。错误表示已配置复核器
    /// 但无法复核本次调用。
    async fn review(&self, _call: ReviewCall) -> Result<Option<LlmReview>, String> {
        Ok(None)
    }

    /// Describes the configuration change one call would make, for the user to approve. An error
    /// means the call is invalid or not allowed for this session; it is denied without asking.
    /// 描述一次调用将做出的配置变更，供用户批准。错误表示该调用无效或本会话不允许，直接拒绝而不询问。
    async fn configuration_preview(
        &self,
        _session_id: SessionId,
        _tool_name: &str,
        _args: &Value,
    ) -> Result<ConfigurationPreview, String> {
        Err(String::from(
            "Configuration changes are not available in this runtime.",
        ))
    }

    /// Keeps one finished decision that the judge or the reviewer took part in; it must not
    /// block, and a failure to keep it never changes the decision.
    /// 保存一次判定器或复核器参与的已完成决定；不得阻塞，保存失败也绝不改变决定。
    fn record(&self, _record: PermissionRecord) {}
}

/// What one judgement read and produced, kept for the decision log.
/// 一次判定读取与产生的内容，供判定记录保存。
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeTrace {
    /// Judge backend key, `local`, `cloud` or `api`.
    /// 判定后端键，`local`、`cloud` 或 `api`。
    pub backend: String,
    /// Local model name, cloud model id or API model id.
    /// 本机模型名称、云端模型标识或 API 模型标识。
    pub model: String,
    /// Policy version of the model.
    /// 模型的策略版本。
    pub policy_version: String,
    /// Category choices the judge read, by category.
    /// 判定器读取的各类别档位。
    pub settings: BTreeMap<String, String>,
    /// Call text the judge read.
    /// 判定器读取的调用文本。
    pub rendered: String,
    /// Raw local scores in label order.
    /// 按标签顺序排列的本机原始分数。
    pub logits: Option<[f32; 3]>,
    /// API probability of the allow label.
    /// API allow 标签的概率。
    pub allow_probability: Option<f32>,
    /// Sequence length the local model read or refused.
    /// 本机模型读取或拒绝的序列长度。
    pub input_tokens: Option<u32>,
    /// Checkpoint digest the cloud judge endpoint reported.
    /// 云端端点报告的检查点摘要。
    pub checkpoint_sha256: Option<String>,
}

/// One scored judgement.
/// 一次已评分的判定。
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeVerdict {
    /// Decision after the threshold rule.
    /// 应用门槛规则后的判定。
    pub label: JudgeLabel,
    /// What the judge read and produced.
    /// 判定器读取与产生的内容。
    pub trace: JudgeTrace,
}

/// A configured judge that could not score a call.
/// 已配置但无法为调用评分的判定器。
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeFailure {
    /// Why the call could not be scored.
    /// 无法评分的原因。
    pub reason: String,
    /// What the judge read, when it got that far.
    /// 判定器已读取的内容（若已进行到该步）。
    pub trace: Option<JudgeTrace>,
}

/// Which layer decided a recorded call, and how.
/// 已记录调用由哪一层决定以及如何决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionOutcome {
    /// The judge cleared the call.
    /// 判定器放行。
    JudgeAllowed,
    /// The reviewer cleared the call.
    /// 复核器放行。
    ReviewAllowed,
    /// The reviewer denied the call.
    /// 复核器拒绝。
    ReviewDenied,
    /// The user allowed the call once.
    /// 用户允许本次调用。
    UserAllowed,
    /// The user denied the call.
    /// 用户拒绝。
    UserDenied,
    /// Nobody answered the approval before its wait ran out.
    /// 审批等待时间耗尽前无人回答。
    ApprovalExpired,
    /// The approval could not finish.
    /// 审批无法完成。
    Failed,
    /// Unattended mode denied a call the reviewer did not allow, since no user can approve it.
    /// 无人值守模式拒绝复核器未放行的调用，因为没有用户可以批准。
    UnattendedDenied,
}

impl PermissionOutcome {
    /// Stable key shared by the decision log and clients.
    /// 判定记录与客户端共用的稳定键。
    pub fn key(self) -> &'static str {
        match self {
            Self::JudgeAllowed => "judge_allowed",
            Self::ReviewAllowed => "review_allowed",
            Self::ReviewDenied => "review_denied",
            Self::UserAllowed => "user_allowed",
            Self::UserDenied => "user_denied",
            Self::ApprovalExpired => "approval_expired",
            Self::Failed => "failed",
            Self::UnattendedDenied => "unattended_denied",
        }
    }
}

/// One finished decision the judge or the reviewer took part in.
/// 一次判定器或复核器参与的已完成决定。
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionRecord {
    /// Session that made the call.
    /// 发起调用的会话。
    pub session_id: SessionId,
    /// When the call reached the judge layer.
    /// 调用到达判定层的时间。
    pub judged_at: SystemTime,
    /// Workspace the call runs in.
    /// 调用运行的工作区。
    pub workspace_root: Option<PathBuf>,
    /// Model-visible tool name.
    /// 模型可见的工具名称。
    pub tool_name: String,
    /// Streamed argument text, or the structured arguments as JSON for restored calls.
    /// 流式参数文本；恢复的调用使用结构化参数的 JSON。
    pub arguments: String,
    /// Matched high-risk category, when any.
    /// 命中的高风险类别（如有）。
    pub high_risk: Option<HighRiskCategory>,
    /// Judge layer finding, when a judge is configured.
    /// 判定层结论（配置判定器时）。
    pub judge: Option<JudgeFinding>,
    /// What the judge read and produced, when it got that far.
    /// 判定器读取与产生的内容（若已进行到该步）。
    pub judge_trace: Option<JudgeTrace>,
    /// Time the judge layer took.
    /// 判定层耗时。
    pub judge_elapsed: Option<Duration>,
    /// Review layer finding, when a reviewer is configured.
    /// 复核层结论（配置复核器时）。
    pub review: Option<ReviewFinding>,
    /// Time the review layer took.
    /// 复核层耗时。
    pub review_elapsed: Option<Duration>,
    /// Which layer decided, and how.
    /// 由哪一层决定以及如何决定。
    pub outcome: PermissionOutcome,
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
    /// Session that makes the call; the host reads its review prompt.
    /// 发起调用的会话；宿主读取其复核提示词。
    pub session_id: SessionId,
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
    /// `purpose` the agent gave on a command tool, when any.
    /// 代理在命令工具上给出的 `purpose`（如有）。
    pub purpose: Option<String>,
    /// Intent the agent declared just before this call, when any.
    /// 代理在本调用之前声明的意图（如有）。
    pub intent: Option<String>,
    /// Deny policy of an unattended session; `Some` means no user can be asked, so the reviewer
    /// must allow or deny.
    /// 无人值守会话的拒绝策略；`Some` 表示无法询问用户，复核器必须放行或拒绝。
    pub unattended: Option<String>,
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
        /// Launcher family frozen by the native tool; unknown interactive input must escalate.
        /// 原生工具固定的启动器类型；未知交互输入必须升级。
        shell: Option<HostCommandShell>,
    },
    /// An extension tool whose effects are unknown and not classified read-only.
    /// 效果未知且未被归类为只读的扩展工具。
    Opaque,
    /// Changes Vulcan's own configuration; outside full access only the user may approve it.
    /// 修改 Vulcan 自身配置；完全访问以外只有用户能批准。
    Configuration {
        /// The change shown to the user; the gate fills it from the host before asking.
        /// 展示给用户的变更；闸门在询问前从宿主取得并填入。
        preview: Option<ConfigurationPreview>,
    },
}

/// One configuration change as the user approves it. Kinds, actions, field and notice names are
/// stable keys that clients render in their own language; values are the configured data itself.
/// 用户批准时看到的一次配置变更。类型、动作、字段与提示名称是客户端以自身语言渲染的稳定键；
/// 值是配置数据本身。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationPreview {
    /// What is configured, such as `mcp_server` or `provider_connection`.
    /// 被配置的对象，例如 `mcp_server` 或 `provider_connection`。
    pub target_kind: String,
    /// Name of the configured item.
    /// 被配置条目的名称。
    pub target: String,
    /// What happens to it, such as `create`, `update` or `remove`.
    /// 对它执行的动作，例如 `create`、`update` 或 `remove`。
    pub action: String,
    /// Fields whose value changes.
    /// 值发生变化的字段。
    pub changes: Vec<ConfigurationFieldChange>,
    /// Consequences the user should know, such as activating a server for every session.
    /// 用户应当知道的后果，例如为所有会话启用某个服务器。
    pub notices: Vec<String>,
}

/// One field of a configuration change; secrets never appear, only the names they are read from.
/// 配置变更中的一个字段；密钥从不出现，只出现其取值来源的名称。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationFieldChange {
    /// Stable field key.
    /// 稳定字段键。
    pub field: String,
    /// Value before the change; absent when the field was unset.
    /// 变更前的值；字段未设置时缺省。
    pub before: Option<String>,
    /// Value after the change; absent when the field is cleared.
    /// 变更后的值；字段被清除时缺省。
    pub after: Option<String>,
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
        VulcanToolSourceKind::Native
            if builtin_luaskills::is_dispatcher(&definition.descriptor.id) =>
        {
            // The dispatcher runs another LuaSkills entry, so it is classified as that entry.
            // 调度器运行另一个 LuaSkills 入口，因此按该入口分类。
            builtin_luaskills::classify_dispatch(args, |name| {
                if text_looks_read_only(name) {
                    PermissionSubject::Ungoverned
                } else {
                    PermissionSubject::Opaque
                }
            })
        }
        VulcanToolSourceKind::Native => {
            let mut subject = classify_native(&definition.descriptor.id, args)
                .unwrap_or(PermissionSubject::Opaque);
            if let PermissionSubject::Command { shell, .. } = &mut subject
                && (definition.descriptor.id == SHELL_COMMAND_TOOL_ID
                    || args.get("action").and_then(Value::as_str) == Some("start"))
            {
                *shell = command_shell(definition, args);
            }
            subject
        }
        VulcanToolSourceKind::Mcp
        | VulcanToolSourceKind::LuaSkills
        | VulcanToolSourceKind::VulcanPlugin => {
            // Built-in LuaSkills entries are known exactly and override the name heuristic.
            // 内置 LuaSkills 入口已精确已知，优先于名称启发式。
            if definition.descriptor.source == VulcanToolSourceKind::LuaSkills
                && let Some(subject) = builtin_luaskills::classify_entry(
                    definition.descriptor.origin().original_name(),
                    args,
                )
            {
                return subject;
            }
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
fn classify_native(tool_id: &str, args: &Value) -> Option<PermissionSubject> {
    // Read string fields without interpreting malformed input as a valid action.
    // 读取字符串字段，避免把格式错误的输入当成有效动作。
    let string_arg = |field: &str| args.get(field).and_then(Value::as_str).map(str::to_string);
    Some(match tool_id {
        // These tools mutate only explicitly enabled knowledge storage or user-directory memory,
        // or read native local text.
        // 这些工具仅变更显式启用的知识存储或用户目录记忆，或读取原生本地文本。
        "knowledge_list" | "knowledge_read" | "knowledge_search" | "knowledge_write"
        | "memory_outline" | "memory_search" | "memory_write" | "file_read" => {
            PermissionSubject::Ungoverned
        }
        // Preserve the declared behavior of existing runtime, collaboration, and host UI tools.
        // 保持现有运行时、协作和宿主界面工具的既定行为。
        "native_workspace_describe"
        | "native_registry_list_agents"
        | "native_registry_list_skills"
        | "native_mcp_list_servers"
        | "skill"
        | "todolist"
        | "submit_plan"
        | "get_goal"
        | "create_goal"
        | "update_goal"
        | DECLARE_INTENT_TOOL_ID
        | "env"
        | "shell_job"
        | "mcp_tools"
        | "runtime-config"
        | "vmm_memory_search"
        | "vmm_turn_details"
        | "vmm_memory_write"
        | "vmm_memory_delete"
        | "browser_list"
        | "browser_open"
        | "browser_select"
        | "browser_close"
        | "ask"
        | vulcan_tool_chain::TOOL_CHAIN_TOOL_ID
        | vulcan_luaskills_prompt::LUASKILL_HELP_TOOL_NAME
        | vulcan_subagent_core::SPAWN_AGENT_TOOL
        | vulcan_subagent_core::AGENT_ORCHESTRATION_INFO_TOOL
        | vulcan_subagent_core::SEND_MESSAGE_TOOL
        | vulcan_subagent_core::LIST_AGENTS_TOOL
        | vulcan_subagent_core::INTERRUPT_AGENT_TOOL
        | vulcan_subagent_core::CLOSE_AGENT_TOOL
        | vulcan_subagent_core::WAIT_AGENTS_RESULT_TOOL
        | "ask_question"
        | "ask_parent"
        | "reply_question"
        | "cancel_question"
        | "list_sessions"
        | "read_session"
        | "send_session_message"
        | "send_notification" => PermissionSubject::Ungoverned,
        FILE_TOOL_ID => {
            let action = args.get("action").and_then(Value::as_str);
            if action.is_some_and(|action| FILE_READ_ACTIONS.contains(&action)) {
                return Some(PermissionSubject::Ungoverned);
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
            Some(text) => PermissionSubject::Command { text, shell: None },
            None => PermissionSubject::Opaque,
        },
        TERMINAL_SESSION_TOOL_ID => classify_terminal(args),
        MCP_CONNECT_TOOL_ID => classify_mcp_connect(args),
        // A session MCP tool is classified like a configured one, by the called tool's name.
        // 会话 MCP 工具与已配置工具一样，按被调用工具的名称分类。
        MCP_CALL_TOOL_ID => match string_arg("tool") {
            Some(tool) if text_looks_read_only(&tool) => PermissionSubject::Ungoverned,
            _ => PermissionSubject::Opaque,
        },
        // Listing configuration reads it; every other action changes it, malformed ones included.
        // 列出配置只是读取；其他任何动作（包括格式错误的）都会修改配置。
        MCP_CONFIG_TOOL_ID | PROVIDER_CONFIG_TOOL_ID => match string_arg("action").as_deref() {
            Some(CONFIGURATION_LIST_ACTION) => PermissionSubject::Ungoverned,
            _ => PermissionSubject::Configuration { preview: None },
        },
        // Reading the installed state is local; installing downloads and installs an executable,
        // an extension effect, and so is any malformed call.
        // 读取安装状态是本地操作；安装会下载并安装可执行文件，属于扩展效果，格式错误的调用同样如此。
        LUASKILLS_DEBUG_TOOL_ID => match string_arg("action").as_deref() {
            Some(LUASKILLS_DEBUG_STATUS_ACTION) => PermissionSubject::Ungoverned,
            _ => PermissionSubject::Opaque,
        },
        // A draft only shows the user a report to review and send themself; linking a pull request
        // writes to the OpenVulcan server, an extension effect, and so is any malformed call.
        // 草稿只是向用户展示一份供其检查并亲自发送的报告；关联拉取请求会写入 OpenVulcan 服务端，属于
        // 扩展效果，格式错误的调用同样如此。
        FEEDBACK_TOOL_ID => match string_arg("action").as_deref() {
            Some(FEEDBACK_DRAFT_ACTION) => PermissionSubject::Ungoverned,
            _ => PermissionSubject::Opaque,
        },
        // A newly registered native tool needs an explicit classification before bypassing review.
        // 新注册的原生工具必须显式分类，才能跳过审批。
        _ => return None,
    })
}

/// Classifies one `mcp_connect` call: starting a stdio server runs its command line, and reaching
/// a remote server is an extension effect; inspecting or closing connections is session state.
/// 对一次 `mcp_connect` 调用分类：启动 stdio 服务会运行其命令行，访问远程服务属于扩展效果；
/// 查看或关闭连接属于会话状态。
fn classify_mcp_connect(args: &Value) -> PermissionSubject {
    if args.get("action").and_then(Value::as_str) != Some("connect") {
        return PermissionSubject::Ungoverned;
    }
    match args.get("transport").and_then(Value::as_str) {
        Some("stdio") => {
            let command = args.get("command").and_then(Value::as_str);
            let arguments: Option<Vec<String>> = match args.get("args") {
                None | Some(Value::Null) => Some(Vec::new()),
                Some(value) => serde_json::from_value(value.clone()).ok(),
            };
            match (command, arguments) {
                (Some(command), Some(arguments)) if !command.trim().is_empty() => {
                    PermissionSubject::Command {
                        text: session_mcp_command_line(command.trim(), &arguments),
                        shell: None,
                    }
                }
                _ => PermissionSubject::Opaque,
            }
        }
        _ => PermissionSubject::Opaque,
    }
}

/// Classifies one `terminal_session` call: starting a shell or typing text runs commands.
/// 对一次 `terminal_session` 调用分类：启动 shell 或输入文本会运行命令。
/// Key and chord inputs only submit or interrupt text that was itself already gated.
/// 按键与组合键输入只提交或中断已经过闸门的文本。
fn classify_terminal(args: &Value) -> PermissionSubject {
    match args.get("action").and_then(Value::as_str) {
        Some("start") => PermissionSubject::Command {
            shell: None,
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
                    shell: None,
                    text: typed.join("\n"),
                }
            }
        }
        _ => PermissionSubject::Ungoverned,
    }
}

/// Resolves a native launch from its explicit selector or the same frozen default its executor uses.
/// 根据显式选择或执行器使用的同一固定默认值解析原生启动类型。
/// `definition` owns the generated schema; `args` supplies the selector. Invalid or absent facts return None.
/// `definition` 持有生成的 Schema；`args` 提供选择器。事实缺失或无效时返回 None。
fn command_shell(definition: &ResolvedToolDefinition, args: &Value) -> Option<HostCommandShell> {
    // The schema default is derived from the executor's HostCommandShellSupport snapshot.
    // Schema 默认值由执行器的 HostCommandShellSupport 快照派生。
    let selector = match args.get("shell") {
        None | Some(Value::Null) => "auto",
        Some(Value::String(selector)) => selector.trim(),
        _ => return None,
    };
    if selector.is_empty() || selector.eq_ignore_ascii_case("auto") {
        definition.parameters_schema["properties"]["shell"]["default"]
            .as_str()
            .and_then(HostCommandShell::parse)
    } else {
        HostCommandShell::parse(selector)
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
        PermissionSubject::Command { text, shell } => {
            if let Some(category) = command_rules::high_risk_category(text) {
                RuleOutcome::Escalate {
                    high_risk: Some(category),
                }
            } else if !text.trim().is_empty()
                && shell.is_some_and(|shell| command_rules::is_known_read_only(text, shell))
            {
                RuleOutcome::Allow
            } else {
                RuleOutcome::Escalate { high_risk: None }
            }
        }
        PermissionSubject::FileWrite { paths } => {
            let inside = matches!(
                mode,
                PermissionMode::WorkspaceWrite | PermissionMode::AutoApprove
            ) && workspace_root.is_some_and(|root| {
                // Git metadata names commands Git runs, so writing it is not a workspace edit.
                // Git 元数据指定 Git 会运行的命令，因此写入它不属于工作区编辑。
                paths.iter().all(|path| {
                    path_resolves_inside_workspace(root, path).unwrap_or(false)
                        && !path_targets_git_metadata(root, path).unwrap_or(true)
                })
            });
            if inside {
                RuleOutcome::Allow
            } else {
                RuleOutcome::Escalate { high_risk: None }
            }
        }
        PermissionSubject::Opaque | PermissionSubject::Configuration { .. } => {
            RuleOutcome::Escalate { high_risk: None }
        }
    }
}

/// What the judge layer produced for one call.
/// 判定层对一次调用的产出。
#[derive(Default)]
struct JudgeRun {
    /// Finding; `None` when no judge is configured or the mode skips it.
    /// 结论；未配置判定器或模式跳过时为 `None`。
    finding: Option<JudgeFinding>,
    /// What the judge read and produced.
    /// 判定器读取与产生的内容。
    trace: Option<JudgeTrace>,
    /// Time the layer took, when it ran.
    /// 该层运行时的耗时。
    elapsed: Option<Duration>,
}

/// Returns the argument text the judge and reviewer read for a call that will run with `args`.
/// 返回将以 `args` 运行的调用供判定器与复核者读取的参数文本。
/// The streamed text is kept when it encodes exactly `args`; when the final arguments differ, they
/// replace it so the layers judge what will run. Restored calls without streamed text stay `None`.
/// 流式文本精确编码 `args` 时保留；最终参数不同时以其替换，使各层判断实际将运行的内容。
/// 没有流式文本的恢复调用保持 `None`。
fn executed_raw_arguments(raw_arguments: Option<&str>, args: &Value) -> Option<String> {
    let raw = raw_arguments?;
    if serde_json::from_str::<Value>(raw).is_ok_and(|streamed| &streamed == args) {
        Some(raw.to_string())
    } else {
        Some(args.to_string())
    }
}

/// Asks the judge about one escalated call.
/// 就一次升级调用询问判定器。
async fn judge_layer(
    host: &Arc<dyn ToolPermissionHost>,
    definition: &ResolvedToolDefinition,
    raw_arguments: Option<&str>,
    workspace_root: Option<&Path>,
) -> JudgeRun {
    let call = JudgeCall {
        workspace_root: workspace_root.map(Path::to_path_buf),
        tool_name: definition.descriptor.id.clone(),
        raw_arguments: raw_arguments.map(str::to_string),
    };
    let started = Instant::now();
    let (finding, trace) = match host.judge(call).await {
        Ok(Some(verdict)) => (
            Some(JudgeFinding::Scored(verdict.label)),
            Some(verdict.trace),
        ),
        Ok(None) => (None, None),
        Err(failure) => (
            Some(JudgeFinding::Unavailable(failure.reason)),
            failure.trace,
        ),
    };
    JudgeRun {
        elapsed: finding.as_ref().map(|_| started.elapsed()),
        finding,
        trace,
    }
}

/// Asks the host's reviewing LLM about one call; `None` when no reviewer is configured.
/// 请宿主的复核 LLM 复核一次调用；未配置复核器时返回 `None`。
async fn review_layer(
    host: &Arc<dyn ToolPermissionHost>,
    call: ReviewCall,
) -> (Option<ReviewFinding>, Option<Duration>) {
    let started = Instant::now();
    let finding = match host.review(call).await {
        Ok(review) => review.map(ReviewFinding::Reviewed),
        Err(reason) => Some(ReviewFinding::Unavailable(reason)),
    };
    let elapsed = finding.as_ref().map(|_| started.elapsed());
    (finding, elapsed)
}

/// What the layers after the judge decided for one escalated call.
/// 判定层之后各层对一次升级调用的决定。
struct EscalatedDecision {
    verdict: ToolPermissionVerdict,
    outcome: PermissionOutcome,
    review: Option<ReviewFinding>,
    review_elapsed: Option<Duration>,
    /// The denial text invited `declare_intent`; the same flag chose that text.
    /// 拒绝文字邀请了 `declare_intent`；该文字也由同一标志选定。
    invites_intent: bool,
}

/// Per-session gate state kept only in memory: the invitation to declare an intent, the intent
/// declared for the next call, and the streak of consecutive unattended denials of one identical
/// call.
/// 仅保存在内存中的会话级闸门状态：声明意图的邀请、为下一次调用声明的意图，以及无人值守模式下
/// 同一调用的连续拒绝。
#[derive(Debug, Default)]
pub struct PermissionMemory {
    sessions: Mutex<HashMap<SessionId, SessionMemory>>,
}

/// Gate state of one session.
/// 一个会话的闸门状态。
#[derive(Debug, Default)]
struct SessionMemory {
    /// Intent for the next tool call, consumed by that call whatever it is.
    /// 下一次工具调用的意图；无论该调用是什么都会被其消费。
    intent: Option<String>,
    /// Set by a denial whose text invited `declare_intent`, under that user request; the next
    /// gated call or one declaration ends it.
    /// 由文字邀请了 `declare_intent` 的拒绝设置，绑定该用户请求；下一次受管控调用或一次声明结束它。
    invitation: Option<IntentInvitation>,
    /// Consecutive unattended denials of one identical call.
    /// 无人值守模式下同一调用的连续拒绝。
    denials: Option<DenialStreak>,
}

/// User request a call is made under: its turn and text, so a later turn sending the same text
/// (such as "continue") starts afresh.
/// 调用所属的用户请求：其回合与文本，因此后续回合发送相同文本（如“继续”）时重新开始。
#[derive(Debug, Clone, Copy)]
struct RequestScope<'a> {
    turn_id: TurnId,
    user_request: Option<&'a str>,
}

/// Stored form of a [`RequestScope`].
/// [`RequestScope`] 的存储形式。
#[derive(Debug)]
struct StoredRequestScope {
    turn_id: TurnId,
    user_request: Option<String>,
}

impl StoredRequestScope {
    /// Copies one scope for storage.
    /// 复制一个作用域以便存储。
    fn of(scope: RequestScope<'_>) -> Self {
        Self {
            turn_id: scope.turn_id,
            user_request: scope.user_request.map(str::to_string),
        }
    }

    /// Whether `scope` is the same user request.
    /// `scope` 是否为同一用户请求。
    fn matches(&self, scope: RequestScope<'_>) -> bool {
        self.turn_id == scope.turn_id && self.user_request.as_deref() == scope.user_request
    }
}

/// User request under which a denial invited `declare_intent`.
/// 拒绝邀请 `declare_intent` 时所属的用户请求。
#[derive(Debug)]
struct IntentInvitation {
    scope: StoredRequestScope,
}

/// Consecutive unattended denials of one identical call under one user request.
/// 同一用户请求下同一调用的连续无人值守拒绝。
#[derive(Debug)]
struct DenialStreak {
    fingerprint: String,
    scope: StoredRequestScope,
    count: u32,
}

impl PermissionMemory {
    /// Runs `change` on one session's state and drops the entry once it holds nothing.
    /// 对一个会话的状态执行 `change`，状态为空时移除条目。
    fn with<T>(&self, session_id: SessionId, change: impl FnOnce(&mut SessionMemory) -> T) -> T {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = sessions.entry(session_id).or_default();
        let value = change(entry);
        if entry.intent.is_none() && entry.invitation.is_none() && entry.denials.is_none() {
            sessions.remove(&session_id);
        }
        value
    }

    /// Whether the last gated call under `user_request` was denied with an invitation to declare.
    /// `user_request` 下最近一次受管控调用是否以邀请声明的方式被拒绝。
    fn invited(&self, session_id: SessionId, scope: RequestScope<'_>) -> bool {
        self.with(session_id, |memory| {
            memory
                .invitation
                .as_ref()
                .is_some_and(|invitation| invitation.scope.matches(scope))
        })
    }

    /// Keeps `intent` for the next tool call and spends the invitation, so one denial allows one
    /// declaration.
    /// 为下一次工具调用保存 `intent` 并用掉邀请，因此一次拒绝只允许一次声明。
    fn declare_intent(&self, session_id: SessionId, intent: String) {
        self.with(session_id, |memory| {
            memory.intent = Some(intent);
            memory.invitation = None;
        });
    }

    /// Records whether the latest gated call ended in an inviting denial; anything else ends the
    /// invitation.
    /// 记录最近一次受管控调用是否以邀请式拒绝结束；其他结果都会结束邀请。
    fn set_invitation(&self, session_id: SessionId, invited: Option<RequestScope<'_>>) {
        self.with(session_id, |memory| {
            memory.invitation = invited.map(|scope| IntentInvitation {
                scope: StoredRequestScope::of(scope),
            });
        });
    }

    /// Takes the intent declared for this call.
    /// 取出为本次调用声明的意图。
    fn take_intent(&self, session_id: SessionId) -> Option<String> {
        self.with(session_id, |memory| memory.intent.take())
    }

    /// Consecutive denials of this identical call under the same user request.
    /// 同一用户请求下本相同调用的连续拒绝次数。
    fn denials(&self, session_id: SessionId, fingerprint: &str, scope: RequestScope<'_>) -> u32 {
        self.with(session_id, |memory| match &memory.denials {
            Some(streak) if streak.fingerprint == fingerprint && streak.scope.matches(scope) => {
                streak.count
            }
            _ => 0,
        })
    }

    /// Counts one more denial of this call; a different call starts a new streak.
    /// 为本调用多计一次拒绝；不同调用开始新的连续计数。
    fn note_denial(&self, session_id: SessionId, fingerprint: &str, scope: RequestScope<'_>) {
        let count = self.denials(session_id, fingerprint, scope) + 1;
        self.with(session_id, |memory| {
            memory.denials = Some(DenialStreak {
                fingerprint: fingerprint.to_string(),
                scope: StoredRequestScope::of(scope),
                count,
            });
        });
    }

    /// Ends the streak because a call ran.
    /// 因有调用执行而结束连续计数。
    fn clear_denials(&self, session_id: SessionId) {
        self.with(session_id, |memory| memory.denials = None);
    }
}

/// Identity of one call for repeat counting: the tool and every argument except the reason the
/// agent attached, so a retry that only rewords its `title` or `purpose` counts as the same call,
/// while new file content or another working directory is a new call.
/// 用于重复计数的调用身份：工具及除代理附加理由外的全部参数，因此只改写 `title` 或 `purpose` 的
/// 重试视为同一调用，而新的文件内容或另一个工作目录则是新调用。
fn call_fingerprint(tool_name: &str, args: &Value) -> String {
    let mut args = args.clone();
    if let (Some(object), Some(field)) = (args.as_object_mut(), command_reason_field(tool_name)) {
        object.remove(field);
    }
    format!("{tool_name}\n{args}")
}

/// Argument that carries the agent's reason on a command tool: the `title` of `shell_command` and
/// the optional `purpose` of `terminal_session`.
/// 命令工具中承载代理理由的参数：`shell_command` 的 `title` 与 `terminal_session` 的可选 `purpose`。
fn command_reason_field(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        SHELL_COMMAND_TOOL_ID => Some("title"),
        TERMINAL_SESSION_TOOL_ID => Some("purpose"),
        _ => None,
    }
}

/// Non-blank reason of a command tool call, cut to the reviewer's limit.
/// 命令工具调用中非空白的理由，截断到复核器上限。
fn command_purpose(tool_id: &str, args: &Value) -> Option<String> {
    let purpose = args.get(command_reason_field(tool_id)?)?.as_str()?.trim();
    (!purpose.is_empty()).then(|| purpose.chars().take(MAX_PERMISSION_REASON_CHARS).collect())
}

/// Decides whether one tool call proceeds, reading the session's live access selections.
/// 读取会话实时授权选择，决定一次工具调用是否执行。
/// In auto-approve mode an escalated call is first scored by the judge, which may clear it
/// unless a high-risk rule matched. The reviewing LLM then sees what the judge did not clear: it
/// may deny, or clear calls that are neither judge-blocked nor high-risk (those only when the user
/// allows it). Everything else goes to the user, except in unattended mode, where no user is
/// asked and everything the reviewer did not allow is denied with guidance for the agent.
/// Read-only and workspace-write escalate straight to the user; only auto-approve may be
/// unattended, so unattended access under any other mode is treated as unreadable.
/// 在自动审批模式下，升级调用先由判定器评分；除非命中高风险规则，判定器可放行。复核 LLM 随后
/// 查看判定器未放行的调用：它可以拒绝，也可以放行既未被判定器拦截也未命中高风险的调用（这两类
/// 仅在用户允许时可放行）。其余全部交给用户；无人值守模式下不询问用户，复核器未放行的调用一律
/// 拒绝并附带给代理的指引。只读与工作区修改模式直接交给用户；只有自动审批可以无人值守，其他模式
/// 下出现无人值守视为授权不可读。
/// `declare_intent` proceeds only right after a denial whose text invited it, under the same user
/// request; its intent is kept for the next call, which consumes it whatever it is.
/// `declare_intent` 只有紧接在文字邀请了它的拒绝之后、且在同一用户请求下才会执行；其意图为下一次
/// 调用保存，无论下一次调用是什么都会消费它。
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
    let native = definition.descriptor.source == VulcanToolSourceKind::Native;
    let scope = RequestScope {
        turn_id: context.turn_id,
        user_request: texts.user_request,
    };
    if native && definition.descriptor.id == DECLARE_INTENT_TOOL_ID {
        let Some(memory) = host
            .memory()
            .filter(|memory| memory.invited(session_id, scope))
        else {
            return ToolPermissionVerdict::Deny(String::from(DECLARE_INTENT_UNINVITED));
        };
        // An invalid intent keeps the invitation; the tool itself reports what is wrong.
        // 无效意图保留邀请；由工具自身报告问题。
        if let Some(intent) = declared_intent(args) {
            memory.declare_intent(session_id, intent);
        }
        return ToolPermissionVerdict::Proceed;
    }
    let intent = host
        .memory()
        .and_then(|memory| memory.take_intent(session_id));
    let access = match host.access(session_id) {
        Ok(access) => access,
        Err(_) => return ToolPermissionVerdict::Deny(String::from(MODE_UNAVAILABLE_DENIAL)),
    };
    let mode = access.permission;
    let subject = classify_tool_call(definition, args);
    let RuleOutcome::Escalate { high_risk } = evaluate_rules(mode, &subject, workspace_root) else {
        return ToolPermissionVerdict::Proceed;
    };
    if access.unattended.is_some() && mode != PermissionMode::AutoApprove {
        return ToolPermissionVerdict::Deny(String::from(MODE_UNAVAILABLE_DENIAL));
    }
    if matches!(subject, PermissionSubject::Configuration { .. }) {
        return admit_configuration_change(
            host,
            session_id,
            definition,
            args,
            context,
            mode,
            access.unattended.is_some(),
        )
        .await;
    }
    let unattended = access.unattended.map(|config| config.deny_policy);
    let purpose = if native {
        command_purpose(&definition.descriptor.id, args)
    } else {
        None
    };
    let fingerprint = call_fingerprint(&definition.descriptor.id, args);
    if unattended.is_some() {
        let memory = host.memory();
        if let Some(memory) = memory {
            memory.set_invitation(session_id, None);
        }
        let repeated = memory.map_or(0, |memory| memory.denials(session_id, &fingerprint, scope));
        if repeated >= UNATTENDED_REVIEWED_REPEATS {
            return ToolPermissionVerdict::Deny(format!(
                "This identical call was already denied {UNATTENDED_REVIEWED_REPEATS} times in a row, so it was denied again without review. Take another approach, or skip this step and leave it for the user's later instructions while you finish the rest of the task."
            ));
        }
    }
    let judged_at = SystemTime::now();
    let raw_arguments = executed_raw_arguments(texts.raw_arguments, args);
    let judge = if mode == PermissionMode::AutoApprove {
        judge_layer(host, definition, raw_arguments.as_deref(), workspace_root).await
    } else {
        JudgeRun::default()
    };
    let arguments = raw_arguments.unwrap_or_else(|| args.to_string());
    let attended = unattended.is_none();
    let decision = decide_escalated(
        host,
        session_id,
        definition,
        args,
        EscalatedCall {
            arguments: &arguments,
            user_request: texts.user_request,
            context,
            workspace_root,
            mode,
            subject,
            high_risk,
            judge: judge.finding.clone(),
            purpose,
            intent,
            unattended,
        },
    )
    .await;
    if let Some(memory) = host.memory() {
        memory.set_invitation(session_id, decision.invites_intent.then_some(scope));
        if !attended {
            match decision.verdict {
                ToolPermissionVerdict::Proceed => memory.clear_denials(session_id),
                ToolPermissionVerdict::Deny(_) | ToolPermissionVerdict::Fail(_) => {
                    memory.note_denial(session_id, &fingerprint, scope);
                }
            }
        }
    }
    if judge.finding.is_some() || decision.review.is_some() {
        host.record(PermissionRecord {
            session_id,
            judged_at,
            workspace_root: workspace_root.map(Path::to_path_buf),
            tool_name: definition.descriptor.id.clone(),
            arguments,
            high_risk,
            judge: judge.finding,
            judge_trace: judge.trace,
            judge_elapsed: judge.elapsed,
            review: decision.review,
            review_elapsed: decision.review_elapsed,
            outcome: decision.outcome,
        });
    }
    decision.verdict
}

/// One escalated call as the layers after the judge read it.
/// 判定层之后各层所读取的一次升级调用。
struct EscalatedCall<'a> {
    arguments: &'a str,
    user_request: Option<&'a str>,
    context: &'a VulcanToolInvocationContext,
    workspace_root: Option<&'a Path>,
    mode: PermissionMode,
    subject: PermissionSubject,
    high_risk: Option<HighRiskCategory>,
    judge: Option<JudgeFinding>,
    purpose: Option<String>,
    intent: Option<String>,
    unattended: Option<String>,
}

/// Why an unattended call the reviewer did not allow was denied, written for the agent, and
/// whether that text invites `declare_intent`. It invites a reason only when one could change the
/// outcome: the call stated none, the reviewer read it, and the reviewer may clear it.
/// 写给代理的、复核器未放行的无人值守调用被拒绝的原因，以及该文字是否邀请 `declare_intent`。只有
/// 理由可能改变结论时才邀请：调用未说明理由、复核器已读过且复核器可以放行。
fn unattended_denial(
    review: Option<&ReviewFinding>,
    blocked: bool,
    explained: bool,
) -> (String, bool) {
    let invites = !explained
        && matches!(review, Some(ReviewFinding::Reviewed(reviewed))
            if !blocked || reviewed.may_allow_blocked);
    let guidance = if invites {
        UNATTENDED_RETRY_GUIDANCE
    } else {
        UNATTENDED_MOVE_ON_GUIDANCE
    };
    let cause = match review {
        Some(ReviewFinding::Reviewed(reviewed)) if reviewed.verdict == ReviewVerdict::Deny => {
            let denial = format!(
                "The permission reviewer denied this call, so it was not executed. Reviewer's reason: {}\n\n{guidance}",
                reviewed.reason
            );
            return (denial, invites);
        }
        Some(ReviewFinding::Reviewed(reviewed)) if blocked => format!(
            "the call matched a high-risk rule or the judge blocked it, which the reviewer may not clear (reviewer's note: {})",
            reviewed.reason
        ),
        Some(ReviewFinding::Reviewed(reviewed)) => format!(
            "the reviewer did not allow it (reviewer's note: {})",
            reviewed.reason
        ),
        Some(ReviewFinding::Unavailable(reason)) => {
            format!("the permission reviewer could not review it: {reason}")
        }
        None => String::from("no permission reviewer is configured"),
    };
    let denial = format!(
        "This call needs approval and no user can give it in an unattended session, so it was not executed: {cause}.\n\n{guidance}"
    );
    (denial, invites)
}

/// Applies the judge finding, then the reviewer, then the user to one escalated call; an
/// unattended call never reaches the user.
/// 对一次升级调用依次应用判定结论、复核器与用户；无人值守调用绝不交给用户。
async fn decide_escalated(
    host: &Arc<dyn ToolPermissionHost>,
    session_id: SessionId,
    definition: &ResolvedToolDefinition,
    args: &Value,
    call: EscalatedCall<'_>,
) -> EscalatedDecision {
    let EscalatedCall {
        arguments,
        user_request,
        context,
        workspace_root,
        mode,
        subject,
        high_risk,
        judge,
        purpose,
        intent,
        unattended,
    } = call;
    if high_risk.is_none() && judge == Some(JudgeFinding::Scored(JudgeLabel::Allow)) {
        return EscalatedDecision {
            verdict: ToolPermissionVerdict::Proceed,
            outcome: PermissionOutcome::JudgeAllowed,
            review: None,
            review_elapsed: None,
            invites_intent: false,
        };
    }
    let explained = purpose.is_some() || intent.is_some();
    let attended = unattended.is_none();
    let (review, review_elapsed) = if mode == PermissionMode::AutoApprove {
        let call = ReviewCall {
            session_id,
            workspace_root: workspace_root.map(Path::to_path_buf),
            tool_name: definition.descriptor.id.clone(),
            arguments: arguments.to_string(),
            user_request: user_request.map(str::to_string),
            high_risk,
            judge: judge.clone(),
            purpose,
            intent,
            unattended,
        };
        review_layer(host, call).await
    } else {
        (None, None)
    };
    let decided = |verdict, outcome, review| EscalatedDecision {
        verdict,
        outcome,
        review,
        review_elapsed,
        invites_intent: false,
    };
    let blocked = high_risk.is_some() || judge == Some(JudgeFinding::Scored(JudgeLabel::Block));
    if let Some(ReviewFinding::Reviewed(reviewed)) = &review {
        match reviewed.verdict {
            ReviewVerdict::Allow if !blocked || reviewed.may_allow_blocked => {
                return decided(
                    ToolPermissionVerdict::Proceed,
                    PermissionOutcome::ReviewAllowed,
                    review,
                );
            }
            ReviewVerdict::Deny if attended => {
                let denial = format!("{REVIEW_DENIAL_PREFIX}{}", reviewed.reason);
                return decided(
                    ToolPermissionVerdict::Deny(denial),
                    PermissionOutcome::ReviewDenied,
                    review,
                );
            }
            ReviewVerdict::Deny => {
                let (denial, invites) = unattended_denial(review.as_ref(), blocked, explained);
                return EscalatedDecision {
                    invites_intent: invites,
                    ..decided(
                        ToolPermissionVerdict::Deny(denial),
                        PermissionOutcome::ReviewDenied,
                        review,
                    )
                };
            }
            ReviewVerdict::Allow | ReviewVerdict::AskUser => {}
        }
    }
    if !attended {
        let (denial, invites) = unattended_denial(review.as_ref(), blocked, explained);
        return EscalatedDecision {
            invites_intent: invites,
            ..decided(
                ToolPermissionVerdict::Deny(denial),
                PermissionOutcome::UnattendedDenied,
                review,
            )
        };
    }
    let prompt = ToolApprovalPrompt {
        mode,
        subject,
        high_risk,
        judge,
        review: review.clone(),
        deadline_unix_ms: None,
    };
    let answer = host
        .request_approval(
            session_id,
            context.clone(),
            definition.descriptor.id.clone(),
            args.clone(),
            prompt,
        )
        .await;
    let (verdict, outcome) = user_verdict(answer);
    decided(verdict, outcome, review)
}

/// What the user's answer to an approval means for the call.
/// 用户对审批的回答对调用意味着什么。
fn user_verdict(
    answer: Result<ApprovalAnswer, String>,
) -> (ToolPermissionVerdict, PermissionOutcome) {
    match answer {
        Ok(ApprovalAnswer::Allowed) => (
            ToolPermissionVerdict::Proceed,
            PermissionOutcome::UserAllowed,
        ),
        Ok(ApprovalAnswer::Denied) => (
            ToolPermissionVerdict::Deny(String::from(USER_DENIAL)),
            PermissionOutcome::UserDenied,
        ),
        Ok(ApprovalAnswer::Expired) => (
            ToolPermissionVerdict::Deny(String::from(APPROVAL_EXPIRED_DENIAL)),
            PermissionOutcome::ApprovalExpired,
        ),
        Err(error) => (
            ToolPermissionVerdict::Fail(error),
            PermissionOutcome::Failed,
        ),
    }
}

/// Asks the user about one configuration change, skipping the judge and the reviewer: changing
/// what Vulcan itself runs is the user's decision alone, so an unattended session cannot do it.
/// The host first previews the change; an invalid or refused change is denied without asking.
/// 就一次配置变更询问用户，跳过判定器与复核器：修改 Vulcan 自身运行的内容只能由用户决定，因此
/// 无人值守会话不能执行。宿主先预览变更；无效或被拒绝的变更不询问直接拒绝。
async fn admit_configuration_change(
    host: &Arc<dyn ToolPermissionHost>,
    session_id: SessionId,
    definition: &ResolvedToolDefinition,
    args: &Value,
    context: &VulcanToolInvocationContext,
    mode: PermissionMode,
    unattended: bool,
) -> ToolPermissionVerdict {
    if let Some(memory) = host.memory() {
        memory.set_invitation(session_id, None);
    }
    if unattended {
        return ToolPermissionVerdict::Deny(String::from(CONFIGURATION_UNATTENDED_DENIAL));
    }
    let preview = match host
        .configuration_preview(session_id, &definition.descriptor.id, args)
        .await
    {
        Ok(preview) => preview,
        Err(error) => {
            return ToolPermissionVerdict::Deny(format!(
                "This configuration change was not executed: {error}"
            ));
        }
    };
    let prompt = ToolApprovalPrompt {
        mode,
        subject: PermissionSubject::Configuration {
            preview: Some(preview),
        },
        high_risk: None,
        judge: None,
        review: None,
        deadline_unix_ms: None,
    };
    let answer = host
        .request_approval(
            session_id,
            context.clone(),
            definition.descriptor.id.clone(),
            args.clone(),
            prompt,
        )
        .await;
    user_verdict(answer).0
}
