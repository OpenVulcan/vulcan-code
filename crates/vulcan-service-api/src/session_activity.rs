//! Session activity: background shell jobs and subagents shown in the activity panel.
//! 会话活动：活动面板中显示的后台 shell 任务与子代理。

use super::*;

/// Lists the shell jobs of one session.
/// 列出一个会话的 shell 任务。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellJobListRequest {
    /// Session that owns the jobs.
    /// 拥有这些任务的会话。
    pub session_id: String,
    /// Also list finished jobs hidden by the user's last clear.
    /// 同时列出被用户上次清理隐藏的已结束任务。
    #[serde(default)]
    pub include_cleared: bool,
    /// Most finished jobs to return, newest first; all when absent. Running jobs are always returned.
    /// 最多返回的已结束任务数（从新到旧）；缺省时全部返回。运行中的任务总是返回。
    #[serde(default)]
    pub finished_limit: Option<usize>,
}

/// One shell job as shown to the user.
/// 展示给用户的一个 shell 任务。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellJobView {
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Command as the model wrote it.
    /// 模型书写的命令。
    pub command: String,
    /// What the command is for, as the model described it.
    /// 模型描述的命令用途。
    pub purpose: String,
    /// Shell label.
    /// Shell 标签。
    pub shell: String,
    /// Working directory.
    /// 工作目录。
    pub cwd: String,
    /// Start time in Unix milliseconds.
    /// 启动时间（Unix 毫秒）。
    pub started_at_ms: u64,
    /// API call of the tool call that started the job, when one did.
    /// 启动该任务的工具调用所属 API 调用（如有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_call_id: Option<u64>,
    /// Tool call that started the job, when one did.
    /// 启动该任务的工具调用（如有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<u64>,
    /// True when the job runs or ran in the background.
    /// 任务正在或曾在后台运行时为 true。
    pub background: bool,
    /// True while the process runs.
    /// 进程运行期间为 true。
    pub running: bool,
    /// `running`, or how the job ended: `completed`, `failed`, `timed_out`, `stopped` or `lost`.
    /// `running`，或任务的结束方式：`completed`、`failed`、`timed_out`、`stopped` 或 `lost`。
    pub status: String,
    /// Exit code when the process exited normally.
    /// 进程正常退出时的退出码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Run time so far, or the total run time once finished.
    /// 迄今的运行时长，结束后为总运行时长。
    pub duration_ms: u64,
    /// End time in Unix milliseconds once finished and known.
    /// 结束且已知时的结束时间（Unix 毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    /// Host-written explanation of a failure, timeout, stop or loss.
    /// 宿主写出的失败、超时、停止或丢失说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    /// Output lines so far.
    /// 迄今的输出行数。
    pub total_lines: u64,
    /// Output bytes so far.
    /// 迄今的输出字节数。
    pub total_bytes: u64,
    /// True while the output log can still be read.
    /// 输出日志仍可读取时为 true。
    pub log_available: bool,
    /// Time of the latest output in Unix milliseconds, when known.
    /// 最近一次输出的时间（Unix 毫秒），已知时提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_output_at_ms: Option<u64>,
    /// Latest output lines of a running job, oldest first; empty for finished jobs.
    /// 运行中任务的最近输出行，从旧到新；已结束任务为空。
    #[serde(default)]
    pub recent_lines: Vec<String>,
    /// True when the job finished before the user's last clear.
    /// 任务在用户上次清理之前结束时为 true。
    #[serde(default)]
    pub cleared: bool,
}

/// Shell jobs of one session, running jobs first.
/// 一个会话的 shell 任务，运行中的在前。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellJobList {
    /// Session that owns the jobs.
    /// 拥有这些任务的会话。
    pub session_id: String,
    /// Running jobs, then finished jobs newest first.
    /// 运行中的任务，随后是从新到旧的已结束任务。
    pub jobs: Vec<ShellJobView>,
    /// Running jobs of the session.
    /// 该会话运行中的任务数。
    pub running_count: usize,
    /// Running jobs of the session that run in the background.
    /// 该会话在后台运行的任务数。
    pub background_running_count: usize,
    /// Finished jobs that match the request, before the limit applied.
    /// 应用数量上限之前符合请求的已结束任务数。
    pub finished_count: usize,
    /// Finished jobs hidden by the user's last clear.
    /// 被用户上次清理隐藏的已结束任务数。
    pub cleared_count: usize,
    /// Number of latest output lines carried by each running job.
    /// 每个运行中任务携带的最近输出行数。
    pub recent_line_limit: usize,
}

/// Reads output rows of one shell job.
/// 读取一个 shell 任务的输出行。
///
/// Without `before_line` and `from_line` the request reads the end of the output.
/// 未提供 `before_line` 与 `from_line` 时读取输出末尾。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellJobOutputRequest {
    /// Session that owns the job.
    /// 拥有该任务的会话。
    pub session_id: String,
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Read the lines just before this one-based line, for scrolling up.
    /// 读取该从 1 开始行号之前紧邻的若干行，用于向上翻页。
    #[serde(default)]
    pub before_line: Option<u64>,
    /// Read lines starting at this one-based line, for scrolling down.
    /// 从该从 1 开始行号读取，用于向下翻页。
    #[serde(default)]
    pub from_line: Option<u64>,
    /// Most output lines to read; the service also bounds the bytes.
    /// 最多读取的输出行数；服务同时限制字节数。
    #[serde(default)]
    pub max_lines: Option<usize>,
}

/// One display row of shell job output.
/// shell 任务输出的一个显示行。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellJobOutputRow {
    /// One-based output line the row belongs to.
    /// 该行所属的从 1 开始的输出行号。
    pub line: u64,
    /// `text`, or `gap` for output dropped by the log size limit.
    /// `text`，或表示因日志大小上限被丢弃输出的 `gap`。
    pub kind: String,
    /// True when the row continues a long line split for display.
    /// 该行延续一条为显示而切分的长行时为 true。
    pub continuation: bool,
    /// Row text with registered secrets redacted.
    /// 已遮蔽登记密钥的行文本。
    pub text: String,
}

/// One page of shell job output rows.
/// 一页 shell 任务输出行。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellJobOutputPage {
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Rows in output order.
    /// 按输出顺序排列的显示行。
    pub rows: Vec<ShellJobOutputRow>,
    /// True when output lines exist before the first row.
    /// 第一行之前还有输出行时为 true。
    pub has_earlier: bool,
    /// One-based line after the last row when more output follows; absent at the end of the output.
    /// 后面还有输出时最后一行之后的从 1 开始行号；位于输出末尾时缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_line: Option<u64>,
    /// Output lines so far.
    /// 迄今的输出行数。
    pub total_lines: u64,
    /// True while the process runs.
    /// 进程运行期间为 true。
    pub running: bool,
}

/// Names one shell job of a session.
/// 指定会话中的一个 shell 任务。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellJobTargetRequest {
    /// Session that owns the job.
    /// 拥有该任务的会话。
    pub session_id: String,
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
}

/// Moves the running foreground command of one tool call to the background.
/// 将一个工具调用中运行的前台命令转到后台。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellCommandBackgroundRequest {
    /// Session that owns the tool call.
    /// 拥有该工具调用的会话。
    pub session_id: String,
    /// API call of the tool call.
    /// 该工具调用所属的 API 调用。
    pub api_call_id: u64,
    /// Tool call that runs the command.
    /// 运行该命令的工具调用。
    pub tool_call_id: u64,
}

/// Reports a job state change to every client so counts and lists refresh.
/// 向所有客户端报告任务状态变化，使数量与列表刷新。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellJobChangedEvent {
    /// Session that owns the job.
    /// 拥有该任务的会话。
    pub session_id: String,
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// `started`, `backgrounded` or `finished`.
    /// `started`、`backgrounded` 或 `finished`。
    pub change: String,
    /// Running jobs of the session after the change.
    /// 变化后该会话运行中的任务数。
    pub running_count: usize,
    /// Running jobs of the session that run in the background after the change.
    /// 变化后该会话在后台运行的任务数。
    pub background_running_count: usize,
}

/// Reads the subagent activity of one session.
/// 读取一个会话的子代理活动。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionActivityRequest {
    /// Root session whose subagents are listed.
    /// 列出其子代理的根会话。
    pub session_id: String,
    /// Also list finished subagents hidden by the user's last clear.
    /// 同时列出被用户上次清理隐藏的已结束子代理。
    #[serde(default)]
    pub include_cleared: bool,
}

/// One recent tool call of a subagent.
/// 子代理最近的一次工具调用。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentActivityTool {
    /// Tool name.
    /// 工具名称。
    pub tool_name: String,
    /// Short summary of the main argument, when one exists.
    /// 主要参数的简短摘要（如有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Lowercase tool call status such as `running` or `completed`.
    /// 小写工具调用状态，例如 `running` 或 `completed`。
    pub status: String,
}

/// One subagent with its latest activity.
/// 一个子代理及其最近活动。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentActivity {
    /// Navigation summary shared with the session tool panel.
    /// 与会话工具面板共享的导航摘要。
    pub task: SubagentTaskSummary,
    /// Creation time in Unix milliseconds.
    /// 创建时间（Unix 毫秒）。
    pub created_at_ms: u64,
    /// Last registry update in Unix milliseconds.
    /// 最近一次注册表更新时间（Unix 毫秒）。
    pub updated_at_ms: u64,
    /// True when the subagent ended before the user's last clear.
    /// 子代理在用户上次清理之前结束时为 true。
    pub cleared: bool,
    /// Latest tool calls, oldest first.
    /// 最近的工具调用，从旧到新。
    pub recent_tools: Vec<SubagentActivityTool>,
    /// Latest lines of the subagent's reply text, oldest first.
    /// 子代理回复正文的最近若干行，从旧到新。
    pub recent_output: Vec<String>,
}

/// Subagent activity of one session.
/// 一个会话的子代理活动。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionActivitySnapshot {
    /// Root session.
    /// 根会话。
    pub session_id: String,
    /// Running subagents first, then ended ones newest first.
    /// 运行中的子代理在前，随后是从新到旧的已结束子代理。
    pub subagents: Vec<SubagentActivity>,
    /// Ended subagents hidden by the user's last clear.
    /// 被用户上次清理隐藏的已结束子代理数。
    pub cleared_count: usize,
}

/// Which list of the activity panel a clear applies to.
/// 一次清理作用于活动面板的哪个列表。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionActivityScope {
    /// Ended subagents.
    /// 已结束的子代理。
    Subagents,
    /// Finished commands.
    /// 已结束的命令。
    Commands,
}

/// Hides the ended entries of one activity list; nothing is deleted.
/// 隐藏一个活动列表中已结束的条目；不删除任何内容。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClearSessionActivityRequest {
    /// Root session.
    /// 根会话。
    pub session_id: String,
    /// List to clear.
    /// 要清理的列表。
    pub scope: SessionActivityScope,
}
