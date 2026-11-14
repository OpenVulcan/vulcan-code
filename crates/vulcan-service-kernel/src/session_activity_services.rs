//! Session activity services: shell jobs and subagents shown in the activity panel.
//! 会话活动服务：活动面板中显示的 shell 任务与子代理。
//!
//! Shell jobs are listed from the job runtime and the session's job index; their output is read from the
//! rolling job log. Subagents are listed from the authoritative subagent registry with their latest tool
//! calls and reply lines. Clearing a list only stores a time marker in the application database; nothing is
//! deleted. Every text taken from command output or tool arguments is redacted like a tool result.
//! Shell 任务从任务运行时与会话任务索引列出，其输出从滚动任务日志读取。子代理从权威子代理注册表列出，并附带
//! 最近的工具调用与回复行。清理列表只在应用数据库中保存时间标记，不删除任何内容。取自命令输出或工具参数的
//! 每段文本都与工具结果一样经过遮蔽。

use super::*;
use crate::session_runtime_registry_v1::SessionRuntimeRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use vulcan_agent_runtime::tooling::native::{
    canonical_shell_job_session_id, shell_job_recent_lines_with_filter,
};
use vulcan_host_tools::shell_job::{
    SHELL_JOB_READ_DEFAULT_LINES, SHELL_JOB_READ_MAX_LINES, SHELL_JOBS_DIRECTORY,
    SHELL_OUTPUT_TAIL_BUDGET_BYTES, ShellJobEventKind, ShellJobLineRow, ShellJobLogReader,
    ShellJobOrigin, ShellJobRowKind, ShellJobRuntime, ShellJobSummary,
};
use vulcan_service_api::{
    ClearSessionActivityRequest, SessionActivityRequest, SessionActivityScope,
    SessionActivitySnapshot, ShellCommandBackgroundRequest, ShellJobChangedEvent, ShellJobList,
    ShellJobListRequest, ShellJobOutputPage, ShellJobOutputRequest, ShellJobOutputRow,
    ShellJobTargetRequest, ShellJobView, SubagentActivity, SubagentActivityTool,
};
use vulcan_session_core::v1::event::{ConversationEvent, EventBody};
use vulcan_workspace_store::workspace_store::{
    SessionActivityMarkScope, SessionActivityMarks, SessionV1StorageLocator,
    load_session_activity_marks, locate_embedded_task_session_v1, mark_session_activity_cleared,
};

/// Latest output lines carried by each running job in a list.
/// 列表中每个运行中任务携带的最近输出行数。
const SHELL_JOB_ACTIVITY_RECENT_LINES: usize = 3;

/// Byte budget of one output page read by the activity panel.
/// 活动面板读取的一页输出的字节预算。
const SHELL_JOB_OUTPUT_PAGE_BUDGET_BYTES: usize = 4 * SHELL_OUTPUT_TAIL_BUDGET_BYTES;

/// Latest tool calls shown for each subagent.
/// 每个子代理显示的最近工具调用数。
const SUBAGENT_ACTIVITY_RECENT_TOOLS: usize = 2;

/// Latest reply lines shown for each subagent.
/// 每个子代理显示的最近回复行数。
const SUBAGENT_ACTIVITY_RECENT_OUTPUT_LINES: usize = 2;

/// Longest tool argument summary or reply line, in characters.
/// 工具参数摘要或回复行的最大长度（字符数）。
const SUBAGENT_ACTIVITY_TEXT_CHARS: usize = 160;

/// Complete Turns read from a closed subagent's stored history; its latest activity is in the last one.
/// 从已关闭子代理的存储历史中读取的完整回合数；其最近活动位于最后一个回合中。
const SUBAGENT_ACTIVITY_TURN_WINDOW: usize = 1;

/// Tool arguments, in order of preference, that summarize a tool call.
/// 按优先顺序排列、用于概括一次工具调用的工具参数。
const SUBAGENT_ACTIVITY_SUMMARY_ARGUMENTS: &[&str] = &[
    "title",
    "purpose",
    "description",
    "command",
    "path",
    "file_path",
    "pattern",
    "query",
    "url",
    "action",
];

/// What the shell job services need from the service state for one session.
/// shell 任务服务针对一个会话从服务状态中所需的内容。
struct ShellJobSessionContext {
    /// Job owner identity in canonical form.
    /// 规范形式的任务所有者身份。
    owner: String,
    /// Directory of the session's job index and logs.
    /// 会话任务索引与日志所在目录。
    jobs_dir: PathBuf,
    /// Application database.
    /// 应用数据库。
    host_runtime: HostRuntimeHandle,
    /// Redaction registry, when the agent runtime is ready.
    /// 遮蔽注册表（代理运行时就绪时）。
    task_runtime: Option<Arc<TaskRuntime>>,
}

impl ShellJobSessionContext {
    /// Redacts registered secrets from one text; a broken registry fails closed.
    /// 从一段文本中遮蔽已登记的密钥；注册表损坏时闭锁失败。
    fn redact(&self, text: &str) -> Result<String, String> {
        match &self.task_runtime {
            Some(task_runtime) => task_runtime.redact_sensitive_text(text),
            None => Err(String::from("Shell job output redaction is not ready.")),
        }
    }

    /// Loads the owning environment before projecting persisted job text.
    /// 在投影持久化任务文本前，加载任务归属会话的环境。
    async fn prepare_redaction(&self, state: &SharedServiceBridgeState) -> Result<(), String> {
        let task_runtime = self
            .task_runtime
            .as_deref()
            .ok_or_else(|| String::from("Shell job output redaction is not ready."))?;
        crate::environment_variables::register_activity_environment_secrets(
            state,
            &self.owner,
            task_runtime,
        )
        .await
    }
}

/// Resolves the job owner, job directory, database and redaction registry of one session.
/// 解析一个会话的任务所有者、任务目录、数据库与遮蔽注册表。
async fn shell_job_session_context(
    state: &SharedServiceBridgeState,
    session_id: &str,
) -> Result<ShellJobSessionContext, String> {
    let (_, locator, _) = resolve_exact_locator_and_registry(state, session_id).await?;
    let (host_runtime, task_runtime) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        (
            guard.controller_runtime.clone().ok_or_else(|| {
                String::from("The service is still starting; storage is not ready.")
            })?,
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.task_runtime)),
        )
    };
    Ok(ShellJobSessionContext {
        owner: canonical_shell_job_session_id(session_id),
        jobs_dir: locator.session_directory().join(SHELL_JOBS_DIRECTORY),
        host_runtime,
        task_runtime,
    })
}

/// Current time in Unix milliseconds.
/// 当前时间（Unix 毫秒）。
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Lists the shell jobs of one session, running first, with the latest lines of running jobs.
/// 列出一个会话的 shell 任务，运行中的在前，并附带运行中任务的最近输出行。
pub(crate) async fn service_list_shell_jobs(
    state: SharedServiceBridgeState,
    request: ShellJobListRequest,
) -> Result<ShellJobList, String> {
    let context = shell_job_session_context(&state, &request.session_id).await?;
    context.prepare_redaction(&state).await?;
    let marks = load_session_activity_marks(&context.host_runtime, &context.owner).await?;
    let runtime = ShellJobRuntime::global();
    let summaries = runtime
        .list_session_jobs(&context.owner, &context.jobs_dir)
        .map_err(|error| format!("Failed to list the shell jobs: {error}"))?;
    let mut running = Vec::new();
    let mut finished = Vec::new();
    let mut cleared_count = 0;
    for summary in summaries {
        let cleared = shell_job_is_cleared(&summary, &marks);
        if cleared {
            cleared_count += 1;
            if !request.include_cleared {
                continue;
            }
        }
        if summary.running {
            running.push(summary);
        } else {
            finished.push((summary, cleared));
        }
    }
    let finished_count = finished.len();
    if let Some(limit) = request.finished_limit {
        finished.truncate(limit);
    }
    let mut jobs = Vec::with_capacity(running.len() + finished.len());
    for summary in running {
        let recent_lines = match runtime.job(&context.owner, &summary.job_id) {
            Some(job) => {
                shell_job_recent_lines_with_filter(&job, SHELL_JOB_ACTIVITY_RECENT_LINES, |line| {
                    context.redact(line).map_err(std::io::Error::other)
                })
                .map_err(|error| format!("Failed to read the shell job output: {error}"))?
            }
            None => Vec::new(),
        };
        jobs.push(shell_job_view(&context, summary, recent_lines, false)?);
    }
    for (summary, cleared) in finished {
        jobs.push(shell_job_view(&context, summary, Vec::new(), cleared)?);
    }
    Ok(ShellJobList {
        session_id: request.session_id,
        running_count: runtime.running_jobs(&context.owner).len(),
        background_running_count: runtime.running_background_count(&context.owner),
        jobs,
        finished_count,
        cleared_count,
        recent_line_limit: SHELL_JOB_ACTIVITY_RECENT_LINES,
    })
}

/// Returns whether a finished job ended before the user's last clear of the command list.
/// 返回已结束任务是否在用户上次清理命令列表之前结束。
fn shell_job_is_cleared(summary: &ShellJobSummary, marks: &SessionActivityMarks) -> bool {
    let Some(cleared_at) = marks.commands_cleared_at_ms else {
        return false;
    };
    if summary.running {
        return false;
    }
    // A lost job has no end time; its start time stands in for it.
    // 丢失的任务没有结束时间；以其启动时间代替。
    let ended_at = summary
        .outcome
        .as_ref()
        .map(|outcome| outcome.finished_at_ms)
        .filter(|finished_at| *finished_at > 0)
        .unwrap_or(summary.started_at_ms);
    ended_at <= cleared_at
}

/// Builds the user-facing view of one job; command and purpose are redacted like its output.
/// 构建一个任务面向用户的视图；命令与用途说明与其输出一样经过遮蔽。
fn shell_job_view(
    context: &ShellJobSessionContext,
    summary: ShellJobSummary,
    recent_lines: Vec<String>,
    cleared: bool,
) -> Result<ShellJobView, String> {
    let outcome = summary.outcome.as_ref();
    let status = if summary.running {
        String::from("running")
    } else {
        outcome
            .map_or("lost", |outcome| outcome.status.as_str())
            .to_string()
    };
    let duration_ms = match outcome {
        Some(outcome) if !summary.running => outcome.duration_ms,
        _ => now_unix_ms().saturating_sub(summary.started_at_ms),
    };
    Ok(ShellJobView {
        job_id: summary.job_id,
        command: context.redact(&summary.command)?,
        purpose: context.redact(&summary.purpose)?,
        shell: summary.shell,
        cwd: summary.cwd,
        started_at_ms: summary.started_at_ms,
        api_call_id: summary.origin.map(|origin| origin.api_call_id),
        tool_call_id: summary.origin.map(|origin| origin.tool_call_id),
        background: summary.background,
        running: summary.running,
        status,
        exit_code: outcome.and_then(|outcome| outcome.exit_code),
        duration_ms,
        finished_at_ms: outcome
            .map(|outcome| outcome.finished_at_ms)
            .filter(|finished_at| *finished_at > 0),
        failure_reason: outcome
            .and_then(|outcome| outcome.failure_reason.as_deref())
            .map(|reason| context.redact(reason))
            .transpose()?,
        total_lines: summary.log.map_or(0, |log| log.total_lines),
        total_bytes: summary.log.map_or(0, |log| log.total_bytes),
        log_available: summary.log.is_some(),
        last_output_at_ms: summary.last_output_at_ms,
        recent_lines,
        cleared,
    })
}

/// Reads one page of a job's output: its end, the lines before a line, or the lines from a line.
/// 读取任务输出的一页：末尾、某行之前的若干行，或从某行开始的若干行。
pub(crate) async fn service_read_shell_job_output(
    state: SharedServiceBridgeState,
    request: ShellJobOutputRequest,
) -> Result<ShellJobOutputPage, String> {
    let context = shell_job_session_context(&state, &request.session_id).await?;
    context.prepare_redaction(&state).await?;
    let runtime = ShellJobRuntime::global();
    let reader = runtime
        .open_log(&context.owner, &context.jobs_dir, &request.job_id)
        .map_err(|error| {
            format!(
                "Failed to open the output of shell job `{}`: {error}",
                request.job_id
            )
        })?
        .ok_or_else(|| format!("This session has no shell job `{}`.", request.job_id))?;
    let max_lines = request
        .max_lines
        .unwrap_or(SHELL_JOB_READ_DEFAULT_LINES)
        .clamp(1, SHELL_JOB_READ_MAX_LINES);
    let running = runtime
        .job(&context.owner, &request.job_id)
        .is_some_and(|job| job.outcome().is_none());
    let total_lines = reader.stats().total_lines;
    // Filter the logical line before wrapping so display boundaries cannot split a match.
    // 在折行前过滤逻辑行，避免显示边界截断匹配。
    let filter_log_line = |line: &str| context.redact(line).map_err(std::io::Error::other);
    let (rows, next_line) = match (request.before_line, request.from_line) {
        (Some(_), Some(_)) => {
            return Err(String::from(
                "Give either `beforeLine` or `fromLine`, not both.",
            ));
        }
        (Some(before_line), None) => (
            read_rows_before(&reader, before_line, max_lines, &filter_log_line)?,
            Some(before_line).filter(|line| *line <= total_lines),
        ),
        (None, Some(from_line)) => {
            let range = reader
                .read_lines_with_filter(
                    from_line.max(1),
                    max_lines,
                    SHELL_JOB_OUTPUT_PAGE_BUDGET_BYTES,
                    filter_log_line,
                )
                .map_err(|error| format!("Failed to read the shell job output: {error}"))?;
            (range.rows, range.next_line)
        }
        (None, None) => (
            reader
                .tail_rows_with_line_filter(
                    max_lines,
                    SHELL_JOB_OUTPUT_PAGE_BUDGET_BYTES,
                    filter_log_line,
                )
                .map_err(|error| format!("Failed to read the shell job output: {error}"))?,
            None,
        ),
    };
    let has_earlier = rows
        .first()
        .is_some_and(|row| row.line > 1 || row.continuation);
    let rows = rows
        .into_iter()
        .filter(|row| row.kind != ShellJobRowKind::Separator)
        .map(shell_job_output_row)
        .collect();
    Ok(ShellJobOutputPage {
        job_id: request.job_id,
        rows,
        has_earlier,
        next_line,
        total_lines,
        running,
    })
}

/// Reads up to `max_lines` lines that end right before `before_line`, within the page byte budget.
/// 在单页字节预算内读取紧接在 `before_line` 之前结束的至多 `max_lines` 行。
///
/// When the budget stops a read before it reaches `before_line`, the start moves closer until the rows
/// end right before it, so scrolling up never skips lines.
/// 预算使读取在到达 `before_line` 之前停止时，起点向其靠近，直到显示行恰好在其之前结束，使向上翻页不跳过任何行。
fn read_rows_before(
    reader: &ShellJobLogReader,
    before_line: u64,
    max_lines: usize,
    filter: &impl Fn(&str) -> std::io::Result<String>,
) -> Result<Vec<ShellJobLineRow>, String> {
    if before_line <= 1 {
        return Ok(Vec::new());
    }
    let mut first = before_line.saturating_sub(max_lines as u64).max(1);
    loop {
        let wanted = usize::try_from(before_line - first).unwrap_or(max_lines);
        let range = reader
            .read_lines_with_filter(first, wanted, SHELL_JOB_OUTPUT_PAGE_BUDGET_BYTES, filter)
            .map_err(|error| format!("Failed to read the shell job output: {error}"))?;
        let reached = range.next_line.is_none_or(|next| next >= before_line);
        if reached || first + 1 >= before_line {
            return Ok(range.rows);
        }
        first += (before_line - first).div_ceil(2);
    }
}

/// Projects one already filtered display row for the client.
/// 将已过滤的显示行投影给客户端。
fn shell_job_output_row(row: ShellJobLineRow) -> ShellJobOutputRow {
    ShellJobOutputRow {
        line: row.line,
        kind: match row.kind {
            ShellJobRowKind::Gap => String::from("gap"),
            _ => String::from("text"),
        },
        continuation: row.continuation,
        text: row.text,
    }
}

/// Stops one running job of a session.
/// 停止某会话的一个运行中任务。
///
/// # Returns / 返回值
/// Returns `false` when the job is not running in this process.
/// 任务不在本进程中运行时返回 `false`。
pub(crate) async fn service_stop_shell_job(
    state: SharedServiceBridgeState,
    request: ShellJobTargetRequest,
) -> Result<bool, String> {
    let context = shell_job_session_context(&state, &request.session_id).await?;
    let Some(job) = ShellJobRuntime::global().job(&context.owner, &request.job_id) else {
        return Ok(false);
    };
    if job.outcome().is_some() {
        return Ok(false);
    }
    job.request_stop("The user stopped this job.");
    Ok(true)
}

/// Moves the running foreground command of one tool call to the background, as an interjection would.
/// 将一个工具调用中运行的前台命令转到后台，效果与插队相同。
///
/// # Returns / 返回值
/// Returns `false` when the call runs no foreground command in this process.
/// 该调用在本进程中没有运行前台命令时返回 `false`。
pub(crate) async fn service_background_shell_command(
    request: ShellCommandBackgroundRequest,
) -> Result<bool, String> {
    let owner = canonical_shell_job_session_id(&request.session_id);
    let Some(job) = ShellJobRuntime::global().job_for_origin(
        &owner,
        ShellJobOrigin {
            api_call_id: request.api_call_id,
            tool_call_id: request.tool_call_id,
        },
    ) else {
        return Ok(false);
    };
    if job.outcome().is_some() {
        return Ok(false);
    }
    Ok(job.detach(false))
}

/// Lists the subagents of one root session with their latest tool calls and reply lines.
/// 列出某根会话的子代理及其最近的工具调用与回复行。
pub(crate) async fn service_session_activity(
    state: SharedServiceBridgeState,
    request: SessionActivityRequest,
) -> Result<SessionActivitySnapshot, String> {
    let (root_session_id, root_locator, registry) =
        resolve_exact_locator_and_registry(&state, &request.session_id).await?;
    let (host_runtime, task_runtime, rows) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        (
            guard.controller_runtime.clone().ok_or_else(|| {
                String::from("The service is still starting; storage is not ready.")
            })?,
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| Arc::clone(&runtime.task_runtime)),
            subagent_activity_rows(&guard, root_session_id)?,
        )
    };
    let redact = |text: &str| match &task_runtime {
        Some(task_runtime) => task_runtime.redact_sensitive_text(text),
        None => Ok(text.to_string()),
    };
    let marks = load_session_activity_marks(
        &host_runtime,
        &canonical_shell_job_session_id(&request.session_id),
    )
    .await?;
    let parents: HashMap<SessionId, SessionId> = rows
        .iter()
        .map(|(_, entry)| {
            (
                entry.session_id.as_uuid(),
                entry.parent_session_id.as_uuid(),
            )
        })
        .collect();
    let mut locators: HashMap<SessionId, SessionV1StorageLocator> = HashMap::new();
    locators.insert(root_session_id, root_locator);
    let mut subagents = Vec::new();
    let mut cleared_count = 0;
    for (task, entry) in rows {
        let cleared = !task.is_running
            && marks
                .subagents_cleared_at_ms
                .is_some_and(|cleared_at| entry.updated_at_ms <= cleared_at);
        if cleared {
            cleared_count += 1;
            if !request.include_cleared {
                continue;
            }
        }
        let task_id = entry.session_id.as_uuid();
        let events = subagent_recent_events(&registry, &mut locators, &parents, task_id)
            .unwrap_or_else(|error| {
                tracing::warn!(%task_id, %error, "Subagent activity is unavailable");
                Vec::new()
            });
        let (recent_tools, recent_output) = subagent_recent_activity(&events);
        subagents.push(SubagentActivity {
            task,
            created_at_ms: entry.created_at_ms,
            updated_at_ms: entry.updated_at_ms,
            cleared,
            recent_tools: recent_tools
                .into_iter()
                .map(|tool| {
                    Ok(SubagentActivityTool {
                        summary: tool.summary.as_deref().map(redact).transpose()?,
                        ..tool
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            recent_output: recent_output
                .iter()
                .map(|line| redact(line))
                .collect::<Result<Vec<_>, _>>()?,
        });
    }
    subagents.sort_by(|left, right| {
        right
            .task
            .is_running
            .cmp(&left.task.is_running)
            .then_with(|| right.updated_at_ms.cmp(&left.updated_at_ms))
    });
    Ok(SessionActivitySnapshot {
        session_id: request.session_id,
        subagents,
        cleared_count,
    })
}

/// Reads the latest events of one subagent: from its live handle when loaded, else from storage.
/// 读取一个子代理的最近事件：已加载时取自其实时句柄，否则取自存储。
fn subagent_recent_events(
    registry: &SessionRuntimeRegistry,
    locators: &mut HashMap<SessionId, SessionV1StorageLocator>,
    parents: &HashMap<SessionId, SessionId>,
    task_id: SessionId,
) -> Result<Vec<ConversationEvent>, String> {
    if let Some(handle) = optional_registered_handle(registry, task_id)? {
        return handle
            .read(|document| document.events().to_vec())
            .map_err(|error| format!("Failed to read the live subagent session: {error}"));
    }
    let locator = subagent_locator(locators, parents, task_id)?;
    locator
        .load_recent_event_window(SUBAGENT_ACTIVITY_TURN_WINDOW)
        .map(|window| window.events)
        .map_err(|error| format!("Failed to load the subagent history: {error}"))
}

/// Locates one subagent's stored session inside its parent's, resolving parents first.
/// 在父会话的存储中定位一个子代理的存储会话，先解析父会话。
fn subagent_locator(
    locators: &mut HashMap<SessionId, SessionV1StorageLocator>,
    parents: &HashMap<SessionId, SessionId>,
    task_id: SessionId,
) -> Result<SessionV1StorageLocator, String> {
    // The chain from the task up to the first located ancestor, nearest first.
    // 从该任务向上到第一个已定位祖先的链，近者在前。
    let mut chain = Vec::new();
    let mut current = task_id;
    while !locators.contains_key(&current) {
        chain.push(current);
        current = *parents
            .get(&current)
            .ok_or_else(|| format!("Subagent {current} has no known parent session."))?;
        if chain.len() > parents.len() {
            return Err(String::from("The subagent parent chain loops."));
        }
    }
    for child in chain.into_iter().rev() {
        let parent = parents[&child];
        let parent_locator = locators[&parent].clone();
        let locator = locate_embedded_task_session_v1(&parent_locator, &child.to_string())
            .map_err(|error| format!("Failed to locate subagent {child}: {error}"))?;
        locators.insert(child, locator);
    }
    Ok(locators[&task_id].clone())
}

/// Picks the latest tool calls and the last lines of the latest reply text, both oldest first.
/// 选出最近的工具调用与最近回复正文的最后若干行，均从旧到新。
fn subagent_recent_activity(
    events: &[ConversationEvent],
) -> (Vec<SubagentActivityTool>, Vec<String>) {
    let mut tools = Vec::new();
    let mut output = Vec::new();
    for event in events.iter().rev() {
        match &event.event {
            EventBody::ToolCall(call) if tools.len() < SUBAGENT_ACTIVITY_RECENT_TOOLS => {
                tools.push(SubagentActivityTool {
                    tool_name: call.tool_name.clone(),
                    summary: serde_json::to_value(&call.input)
                        .ok()
                        .and_then(|input| tool_argument_summary(&input)),
                    status: serde_json::to_value(call.status)
                        .ok()
                        .and_then(|status| status.as_str().map(str::to_string))
                        .unwrap_or_default(),
                });
            }
            EventBody::AssistantText(text) if output.is_empty() => {
                output = text
                    .text
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .rev()
                    .take(SUBAGENT_ACTIVITY_RECENT_OUTPUT_LINES)
                    .map(|line| truncate_chars(line, SUBAGENT_ACTIVITY_TEXT_CHARS))
                    .collect();
                output.reverse();
            }
            _ => {}
        }
        if tools.len() >= SUBAGENT_ACTIVITY_RECENT_TOOLS && !output.is_empty() {
            break;
        }
    }
    tools.reverse();
    (tools, output)
}

/// Summarizes a tool call by its first present preferred string argument, on one line.
/// 以第一个存在的首选字符串参数单行概括一次工具调用。
fn tool_argument_summary(input: &Value) -> Option<String> {
    SUBAGENT_ACTIVITY_SUMMARY_ARGUMENTS.iter().find_map(|key| {
        let text = input.get(*key)?.as_str()?.trim();
        (!text.is_empty()).then(|| {
            truncate_chars(
                &text.split_whitespace().collect::<Vec<_>>().join(" "),
                SUBAGENT_ACTIVITY_TEXT_CHARS,
            )
        })
    })
}

/// Cuts text to at most `max_chars` characters, marking a cut with an ellipsis.
/// 将文本截为至多 `max_chars` 个字符，截断处以省略号标记。
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// Hides the ended entries of one activity list of a session; nothing is deleted.
/// 隐藏某会话一个活动列表中已结束的条目；不删除任何内容。
pub(crate) async fn service_clear_session_activity(
    state: SharedServiceBridgeState,
    request: ClearSessionActivityRequest,
) -> Result<bool, String> {
    let host_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("The service is still starting; storage is not ready."))?
    };
    let session_id = request
        .session_id
        .parse::<SessionId>()
        .map_err(|error| format!("Invalid session id `{}`: {error}", request.session_id))?;
    let scope = match request.scope {
        SessionActivityScope::Subagents => SessionActivityMarkScope::Subagents,
        SessionActivityScope::Commands => SessionActivityMarkScope::Commands,
    };
    mark_session_activity_cleared(&host_runtime, &session_id.to_string(), scope, now_unix_ms())
        .await?;
    Ok(true)
}

/// Forwards every shell job event to the clients with the owning session's current counts.
/// 将每个 shell 任务事件连同所属会话的当前数量转发给客户端。
pub(crate) fn install_shell_job_event_forwarding(
    runtime: &tokio::runtime::Handle,
    app: ServiceEventEmitter,
) {
    let mut events = ShellJobRuntime::global().subscribe_events();
    runtime.spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                // Counts travel with every event, so a skipped event is caught up by the next one.
                // 每个事件都带有数量，因此跳过的事件会由下一个事件补上。
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            let runtime = ShellJobRuntime::global();
            let change = match event.kind {
                ShellJobEventKind::Started => "started",
                ShellJobEventKind::Backgrounded => "backgrounded",
                ShellJobEventKind::Finished => "finished",
            };
            app.emit(ServiceEvent::ShellJobChanged(Box::new(
                ShellJobChangedEvent {
                    running_count: runtime.running_jobs(&event.owner_session_id).len(),
                    background_running_count: runtime
                        .running_background_count(&event.owner_session_id),
                    session_id: event.owner_session_id,
                    job_id: event.job_id,
                    change: String::from(change),
                },
            )));
        }
    });
}
