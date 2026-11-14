//! `shell_job`: list, inspect, read, search, wait for and stop the shell jobs of the session.
//! `shell_job`：列出、查看、读取、搜索、等待与停止会话的 shell 任务。
//!
//! Every action only reads job state or ends a job this session started, so the tool is not governed by the
//! permission pipeline. Results go through the same tool-result redaction as `shell_command`.
//! 每个动作只读取任务状态，或结束本会话启动的任务，因此本工具不受权限管线管辖。结果与 `shell_command`
//! 经过同样的工具结果脱敏。

use super::*;
use vulcan_host_tools::shell_job::{
    SHELL_JOB_READ_DEFAULT_LINES, SHELL_JOB_READ_MAX_LINES, SHELL_JOB_SEARCH_MAX_CONTEXT_LINES,
    SHELL_JOB_SEARCH_MAX_MATCHES, SHELL_JOB_WAIT_DEFAULT_MS, SHELL_JOB_WAIT_MAX_MS,
    SHELL_OUTPUT_TAIL_BUDGET_BYTES, ShellJobLogReader, ShellJobRuntime, ShellJobSummary,
    ShellJobWaitOutcome, render_numbered_rows, snapshot_process_tree,
};

/// Stable identifier of the shell job tool.
/// shell 任务工具的稳定标识。
pub const SHELL_JOB_TOOL_ID: &str = "shell_job";

/// Finished jobs listed after the running ones.
/// 在运行中任务之后列出的已结束任务数。
const SHELL_JOB_LIST_FINISHED_LIMIT: usize = 20;

/// Characters of a command shown in a listing.
/// 列表中显示的命令字符数。
const SHELL_JOB_LIST_COMMAND_CHARS: usize = 160;

/// Default context lines around a search match.
/// 搜索匹配前后的默认上下文行数。
const SHELL_JOB_SEARCH_DEFAULT_CONTEXT_LINES: usize = 2;

/// Builds the `shell_job` native tool.
/// 构建 `shell_job` 原生工具。
pub(crate) fn build_shell_job_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(SHELL_JOB_TOOL_ID),
        name: String::from(SHELL_JOB_TOOL_ID),
        description: shell_job_tool_description(),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };
    build_native_tool_definition(
        descriptor,
        shell_job_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_shell_job_tool(request).await })
        },
    )
}

/// AI-facing description of `shell_job`.
/// `shell_job` 面向 AI 的描述。
fn shell_job_tool_description() -> String {
    format!(
        "Inspect and control the shell jobs of this session. Every `shell_command` call is a job; its output (stdout and stderr together, in arrival order) is kept in a rolling log, so long output can be read here even after the command returned only its end.\n\
- `list`: running jobs first, then the {SHELL_JOB_LIST_FINISHED_LIMIT} most recent finished ones, each with its title.\n\
- `status`: one job's state, run time, output size, time of the latest output and, while it runs, its live processes with CPU time; use it to tell a stuck command from a busy one.\n\
- `read`: output rows with line numbers. Omit `start_line` for the end of the output; give it to page from that line. `N|` starts line N, `N+|` continues a long line split into rows.\n\
- `search`: regular-expression search over the whole log, with `context_lines` around each match (`N>|` marks matching rows).\n\
- `wait`: wait up to `timeout_ms` (default {SHELL_JOB_WAIT_DEFAULT_MS}, at most {SHELL_JOB_WAIT_MAX_MS}) for a running job to finish; returns early when the user sends a message.\n\
- `stop`: terminate a running job's process tree.\n\
You are told automatically when a background job finishes; you do not need to poll."
    )
}

/// JSON Schema of the `shell_job` arguments.
/// `shell_job` 参数的 JSON Schema。
fn shell_job_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["list", "status", "read", "search", "wait", "stop"],
                "description": "Operation to perform."
            },
            "job_id": {
                "type": "string",
                "description": "Job identity such as `job_0123456789ab`, from a `shell_command` result or `list`. Required for every action except `list`."
            },
            "start_line": {
                "type": "integer",
                "minimum": 1,
                "description": "`read` only: first line to return, 1-based. Omit it to read the end of the output."
            },
            "max_lines": {
                "type": "integer",
                "minimum": 1,
                "maximum": SHELL_JOB_READ_MAX_LINES,
                "description": format!("`read` only: most lines to return; default {SHELL_JOB_READ_DEFAULT_LINES}. The result also stays within {SHELL_OUTPUT_TAIL_BUDGET_BYTES} bytes.")
            },
            "pattern": {
                "type": "string",
                "description": "`search` only: regular expression (Rust syntax) matched against each output line."
            },
            "context_lines": {
                "type": "integer",
                "minimum": 0,
                "maximum": SHELL_JOB_SEARCH_MAX_CONTEXT_LINES,
                "description": format!("`search` only: lines shown before and after each match; default {SHELL_JOB_SEARCH_DEFAULT_CONTEXT_LINES}.")
            },
            "max_matches": {
                "type": "integer",
                "minimum": 1,
                "maximum": SHELL_JOB_SEARCH_MAX_MATCHES,
                "description": format!("`search` only: most matches to return; default {SHELL_JOB_SEARCH_MAX_MATCHES}.")
            },
            "timeout_ms": {
                "type": "integer",
                "minimum": 1,
                "maximum": SHELL_JOB_WAIT_MAX_MS,
                "description": format!("`wait` only: longest wait in milliseconds; default {SHELL_JOB_WAIT_DEFAULT_MS}.")
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// Parsed `shell_job` arguments.
/// 解析后的 `shell_job` 参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShellJobToolArgs {
    /// Operation.
    /// 操作。
    action: String,
    /// Target job.
    /// 目标任务。
    job_id: Option<String>,
    /// First line of a read.
    /// 读取的起始行。
    start_line: Option<u64>,
    /// Most lines of a read.
    /// 读取的最多行数。
    max_lines: Option<usize>,
    /// Search pattern.
    /// 搜索模式。
    pattern: Option<String>,
    /// Search context lines.
    /// 搜索上下文行数。
    context_lines: Option<usize>,
    /// Most search matches.
    /// 最多搜索匹配数。
    max_matches: Option<usize>,
    /// Wait time.
    /// 等待时间。
    timeout_ms: Option<u64>,
}

/// Parses shell-job arguments and enforces the advertised bounds before touching any job.
/// 解析 shell 任务参数，并在访问任务前执行已声明的参数边界。
fn parse_shell_job_arguments(arguments: Value) -> Result<ShellJobToolArgs, NativeToolError> {
    let args: ShellJobToolArgs = serde_json::from_value(arguments).map_err(|error| {
        NativeToolError::new(format!("Failed to parse shell_job arguments: {error}"))
    })?;
    if args.start_line == Some(0) {
        return Err(NativeToolError::new(
            "The shell_job start_line must be at least 1.",
        ));
    }
    if args
        .max_lines
        .is_some_and(|value| !(1..=SHELL_JOB_READ_MAX_LINES).contains(&value))
    {
        return Err(NativeToolError::new(format!(
            "The shell_job max_lines must be between 1 and {SHELL_JOB_READ_MAX_LINES}."
        )));
    }
    if args
        .max_matches
        .is_some_and(|value| !(1..=SHELL_JOB_SEARCH_MAX_MATCHES).contains(&value))
    {
        return Err(NativeToolError::new(format!(
            "The shell_job max_matches must be between 1 and {SHELL_JOB_SEARCH_MAX_MATCHES}."
        )));
    }
    if args
        .context_lines
        .is_some_and(|value| value > SHELL_JOB_SEARCH_MAX_CONTEXT_LINES)
    {
        return Err(NativeToolError::new(format!(
            "The shell_job context_lines must be at most {SHELL_JOB_SEARCH_MAX_CONTEXT_LINES}."
        )));
    }
    if args
        .timeout_ms
        .is_some_and(|value| !(1..=SHELL_JOB_WAIT_MAX_MS).contains(&value))
    {
        return Err(NativeToolError::new(format!(
            "The shell_job timeout_ms must be between 1 and {SHELL_JOB_WAIT_MAX_MS}."
        )));
    }
    Ok(args)
}

/// Runs one `shell_job` call.
/// 执行一次 `shell_job` 调用。
async fn execute_shell_job_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let owner = ShellCommandJobContext::of_request(&request)
        .owner_session_id()
        .to_string();
    let args = parse_shell_job_arguments(request.arguments)?;
    // Historical output may be the first tool result after a restart. Register the
    // owning environment before any stored text reaches the normal result filter.
    // 历史输出可能是重启后的首个工具结果，须在进入常规结果过滤器前登记归属会话环境。
    if request.invocation_context.is_some() || request.session_id.is_some() {
        // Scoped calls must have both a validated session and its redaction host.
        // 有作用域的调用必须同时具备已校验的会话与脱敏宿主。
        let session_id = request.session_id.ok_or_else(|| {
            NativeToolError::new("Shell job output requires a valid session identity.")
        })?;
        let environment = request
            .environment
            .as_ref()
            .ok_or_else(|| NativeToolError::new("Shell job output redaction is not ready."))?;
        environment
            .effective(session_id)
            .await
            .map_err(NativeToolError::new)?;
    }
    let runtime = ShellJobRuntime::global();
    let jobs_dir = runtime.jobs_directory(&owner);
    let job_id = || {
        args.job_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                NativeToolError::new(format!(
                    "The `job_id` argument is required for the `{}` action.",
                    args.action
                ))
            })
    };
    let (text, payload) = match args.action.as_str() {
        "list" => {
            let jobs = runtime
                .list_session_jobs(&owner, &jobs_dir)
                .map_err(|error| NativeToolError::new(error.to_string()))?;
            (
                render_job_list(&jobs),
                json!({ "action": "list", "jobs": jobs.len() }),
            )
        }
        "status" => {
            let job_id = job_id()?;
            let summary = find_summary(runtime, &owner, &jobs_dir, job_id)?;
            let mut text = render_job_status(&summary);
            if summary.running
                && let Some(pid) = runtime
                    .job(&owner, job_id)
                    .and_then(|job| job.root_process_id())
            {
                match snapshot_process_tree(pid).await {
                    Ok(processes) if !processes.is_empty() => {
                        text.push_str("\nProcesses (pid, parent, CPU time, name):");
                        for process in processes {
                            text.push_str(&format!(
                                "\n  {} {} {} ms {}",
                                process.pid, process.parent_pid, process.cpu_time_ms, process.name
                            ));
                        }
                    }
                    Ok(_) => text.push_str("\nProcesses: the root process has already exited."),
                    Err(error) => text.push_str(&format!("\nProcesses: {error}")),
                }
            }
            (
                text,
                json!({ "action": "status", "job_id": job_id, "running": summary.running }),
            )
        }
        "read" => {
            let job_id = job_id()?;
            let reader = open_reader(runtime, &owner, &jobs_dir, job_id)?;
            let max_lines = args.max_lines.unwrap_or(SHELL_JOB_READ_DEFAULT_LINES);
            let text = match args.start_line {
                None => {
                    let tail = reader
                        .tail(SHELL_OUTPUT_TAIL_BUDGET_BYTES)
                        .map_err(io_error)?;
                    let mut rows = tail.rows;
                    let lines_of_rows =
                        |rows: &[vulcan_host_tools::shell_job::ShellJobLineRow]| {
                            let mut lines: Vec<u64> = rows.iter().map(|row| row.line).collect();
                            lines.dedup();
                            lines
                        };
                    let lines = lines_of_rows(&rows);
                    if lines.len() > max_lines {
                        let first_kept = lines[lines.len() - max_lines];
                        rows.retain(|row| row.line >= first_kept);
                    }
                    format!(
                        "Job {job_id}: {} lines, {} bytes in total; showing the end.\n{}",
                        tail.total_lines,
                        tail.total_bytes,
                        render_numbered_rows(&rows)
                    )
                }
                Some(start_line) => {
                    let range = reader
                        .read_lines(start_line, max_lines, SHELL_OUTPUT_TAIL_BUDGET_BYTES)
                        .map_err(io_error)?;
                    let mut text = format!(
                        "Job {job_id}: {} lines in total.\n{}",
                        range.total_lines,
                        render_numbered_rows(&range.rows)
                    );
                    if range.rows.is_empty() {
                        text.push_str("(no lines at or after this line)");
                    }
                    if let Some(next) = range.next_line {
                        text.push_str(&format!("\n[more: continue with start_line {next}]"));
                    }
                    text
                }
            };
            (text, json!({ "action": "read", "job_id": job_id }))
        }
        "search" => {
            let job_id = job_id()?;
            let pattern = args
                .pattern
                .as_deref()
                .filter(|pattern| !pattern.is_empty())
                .ok_or_else(|| {
                    NativeToolError::new("The `pattern` argument is required for `search`.")
                })?;
            let regex = regex::Regex::new(pattern).map_err(|error| {
                NativeToolError::new(format!("Invalid search pattern: {error}"))
            })?;
            let reader = open_reader(runtime, &owner, &jobs_dir, job_id)?;
            let found = reader
                .search(
                    &regex,
                    args.context_lines
                        .unwrap_or(SHELL_JOB_SEARCH_DEFAULT_CONTEXT_LINES),
                    args.max_matches.unwrap_or(SHELL_JOB_SEARCH_MAX_MATCHES),
                    SHELL_OUTPUT_TAIL_BUDGET_BYTES,
                )
                .map_err(io_error)?;
            let mut text = format!("Job {job_id}: {} matching lines.", found.match_count);
            if found.truncated {
                text.push_str(" Not all are shown; narrow the pattern or lower `context_lines`.");
            }
            if !found.rows.is_empty() {
                text.push('\n');
                text.push_str(&render_numbered_rows(&found.rows));
            }
            (
                text,
                json!({ "action": "search", "job_id": job_id, "matches": found.match_count }),
            )
        }
        "wait" => {
            let job_id = job_id()?;
            let Some(job) = runtime.job(&owner, job_id) else {
                let summary = find_summary(runtime, &owner, &jobs_dir, job_id)?;
                return Ok(shell_job_result(
                    render_job_status(&summary),
                    json!({ "action": "wait", "job_id": job_id, "running": false, "purpose": summary.purpose }),
                ));
            };
            let budget = args.timeout_ms.unwrap_or(SHELL_JOB_WAIT_DEFAULT_MS);
            let mut interjection = runtime.subscribe_interjection(&owner);
            let waited = job
                .wait_interruptible(Some(Duration::from_millis(budget)), &mut interjection)
                .await;
            let note = match &waited {
                ShellJobWaitOutcome::Finished(_) => {
                    // The model sees the outcome here, so the later notice would only repeat it.
                    // 模型在此看到结果，之后的通知只会重复它。
                    runtime.discard_notice(&owner, job_id);
                    "The job finished."
                }
                ShellJobWaitOutcome::StillRunning => "The wait ended; the job is still running.",
                ShellJobWaitOutcome::Interjected => {
                    "The user sent a message, so the wait ended early; the job is still running."
                }
                ShellJobWaitOutcome::Detached => {
                    "The job moved to the background and is still running."
                }
            };
            let mut text = format!("{note}\n{}", render_job_status(&job.summary()));
            append_tail(&mut text, job.log_reader());
            (
                text,
                json!({
                    "action": "wait",
                    "job_id": job_id,
                    "running": job.outcome().is_none(),
                }),
            )
        }
        "stop" => {
            let job_id = job_id()?;
            let Some(job) = runtime
                .job(&owner, job_id)
                .filter(|job| job.outcome().is_none())
            else {
                let summary = find_summary(runtime, &owner, &jobs_dir, job_id)?;
                return Ok(shell_job_result(
                    format!("The job is not running.\n{}", render_job_status(&summary)),
                    json!({ "action": "stop", "job_id": job_id, "running": false, "purpose": summary.purpose }),
                ));
            };
            job.request_stop("Stopped with shell_job.");
            let outcome = job.wait().await;
            runtime.discard_notice(&owner, job_id);
            let mut text = format!(
                "The job ended with status {}.\n{}",
                outcome.status.as_str(),
                render_job_status(&job.summary())
            );
            append_tail(&mut text, job.log_reader());
            (
                text,
                json!({ "action": "stop", "job_id": job_id, "running": false }),
            )
        }
        other => {
            return Err(NativeToolError::new(format!(
                "Unknown shell_job action `{other}`; use list, status, read, search, wait or stop."
            )));
        }
    };
    let mut payload = payload;
    // The card names the job by its purpose, so every payload that names a job carries it.
    // 卡片以用途称呼任务，因此每个指明任务的载荷都附带用途。
    if let Some(job_id) = payload.get("job_id").and_then(Value::as_str)
        && let Ok(summary) = find_summary(runtime, &owner, &jobs_dir, job_id)
    {
        payload["purpose"] = Value::String(summary.purpose);
    }
    Ok(shell_job_result(text, payload))
}

/// Wraps text and a host payload into a tool result.
/// 把文本与宿主载荷包装为工具结果。
fn shell_job_result(text: String, payload: Value) -> VulcanToolInvocationResult {
    VulcanToolInvocationResult::from_model_output(Value::String(text)).with_host_result(
        VulcanToolHostResult {
            kind: String::from(SHELL_JOB_TOOL_ID),
            payload,
        },
    )
}

/// Maps a log read failure to a tool error.
/// 将日志读取失败映射为工具错误。
fn io_error(error: std::io::Error) -> NativeToolError {
    NativeToolError::new(format!("Failed to read the job output: {error}"))
}

/// Finds one job of the session among live and recorded jobs.
/// 在运行中与已记录的任务中查找会话的一个任务。
fn find_summary(
    runtime: &ShellJobRuntime,
    owner: &str,
    jobs_dir: &Path,
    job_id: &str,
) -> Result<ShellJobSummary, NativeToolError> {
    if let Some(job) = runtime.job(owner, job_id) {
        return Ok(job.summary());
    }
    runtime
        .list_session_jobs(owner, jobs_dir)
        .map_err(|error| NativeToolError::new(error.to_string()))?
        .into_iter()
        .find(|summary| summary.job_id == job_id)
        .ok_or_else(|| unknown_job(job_id))
}

/// Opens the log of one job of the session.
/// 打开会话一个任务的日志。
fn open_reader(
    runtime: &ShellJobRuntime,
    owner: &str,
    jobs_dir: &Path,
    job_id: &str,
) -> Result<ShellJobLogReader, NativeToolError> {
    runtime
        .open_log(owner, jobs_dir, job_id)
        .map_err(|error| NativeToolError::new(error.to_string()))?
        .ok_or_else(|| unknown_job(job_id))
}

/// Error for a job this session does not have.
/// 本会话没有该任务时的错误。
fn unknown_job(job_id: &str) -> NativeToolError {
    NativeToolError::new(format!(
        "This session has no shell job `{job_id}`; use the `list` action to see its jobs."
    ))
}

/// Appends the output tail of a job, when it has any.
/// 追加任务的输出尾部（如有）。
fn append_tail(text: &mut String, reader: Option<ShellJobLogReader>) {
    let Some(tail) = reader.and_then(|reader| reader.tail(SHELL_OUTPUT_TAIL_BUDGET_BYTES).ok())
    else {
        return;
    };
    if tail.rows.is_empty() {
        return;
    }
    if tail.truncated {
        text.push_str("\n\n[output, end only]\n");
    } else {
        text.push_str("\n\n[output]\n");
    }
    text.push_str(render_numbered_rows(&tail.rows).trim_end());
}

/// One listing: running jobs, then the most recent finished ones.
/// 一份列表：运行中任务，随后是最近结束的任务。
fn render_job_list(jobs: &[ShellJobSummary]) -> String {
    if jobs.is_empty() {
        return String::from("This session has no shell jobs.");
    }
    let running: Vec<&ShellJobSummary> = jobs.iter().filter(|job| job.running).collect();
    let finished: Vec<&ShellJobSummary> = jobs
        .iter()
        .filter(|job| !job.running)
        .take(SHELL_JOB_LIST_FINISHED_LIMIT)
        .collect();
    let mut text = format!("Running: {}", running.len());
    for job in &running {
        text.push('\n');
        text.push_str(&render_job_line(job));
    }
    text.push_str(&format!(
        "\nFinished (most recent first): {}",
        finished.len()
    ));
    for job in &finished {
        text.push('\n');
        text.push_str(&render_job_line(job));
    }
    let older = jobs.iter().filter(|job| !job.running).count() - finished.len();
    if older > 0 {
        text.push_str(&format!("\n({older} older finished jobs not shown)"));
    }
    text
}

/// One listing line.
/// 一行列表。
fn render_job_line(job: &ShellJobSummary) -> String {
    let state = match &job.outcome {
        None if job.background => String::from("running in background"),
        None => String::from("running in foreground"),
        Some(outcome) => match outcome.exit_code {
            Some(code) => format!("{} (exit {code})", outcome.status.as_str()),
            None => outcome.status.as_str().to_string(),
        },
    };
    let command: String = job
        .command
        .chars()
        .take(SHELL_JOB_LIST_COMMAND_CHARS)
        .collect();
    let ellipsis = if job.command.chars().count() > SHELL_JOB_LIST_COMMAND_CHARS {
        "…"
    } else {
        ""
    };
    format!(
        "- {} [{state}] {} — `{command}{ellipsis}`",
        job.job_id, job.purpose
    )
}

/// Status of one job.
/// 一个任务的状态。
fn render_job_status(job: &ShellJobSummary) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let mut lines = vec![
        format!("Job: {}", job.job_id),
        format!("Title: {}", job.purpose),
        format!("Command: {}", job.command),
        format!("Shell: {}", job.shell),
        format!("Working directory: {}", job.cwd),
    ];
    match &job.outcome {
        None => {
            lines.push(format!(
                "Status: running in the {}",
                if job.background {
                    "background"
                } else {
                    "foreground"
                }
            ));
            lines.push(format!(
                "Running for: {} ms",
                now.saturating_sub(job.started_at_ms)
            ));
        }
        Some(outcome) => {
            lines.push(format!("Status: {}", outcome.status.as_str()));
            lines.push(format!("Duration: {} ms", outcome.duration_ms));
            if let Some(code) = outcome.exit_code {
                lines.push(format!("Exit code: {code}"));
            }
            if let Some(reason) = &outcome.failure_reason {
                lines.push(format!("Reason: {reason}"));
            }
        }
    }
    match &job.log {
        Some(log) => lines.push(format!(
            "Output: {} lines, {} bytes",
            log.total_lines, log.total_bytes
        )),
        None => lines.push(String::from("Output: the log is no longer available")),
    }
    if let Some(last) = job.last_output_at_ms {
        lines.push(format!(
            "Latest output: {} ms ago",
            now.saturating_sub(last)
        ));
    } else if job.running {
        lines.push(String::from("Latest output: none yet"));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks that execution enforces the ranges advertised for job reads, searches, and waits.
    /// 验证执行入口遵循任务读取、搜索和等待所声明的参数范围。
    #[test]
    fn shell_job_enforces_advertised_ranges() {
        for arguments in [
            json!({"action": "read", "start_line": 1, "max_lines": SHELL_JOB_READ_MAX_LINES}),
            json!({"action": "search", "context_lines": 0, "max_matches": SHELL_JOB_SEARCH_MAX_MATCHES}),
            json!({"action": "search", "context_lines": SHELL_JOB_SEARCH_MAX_CONTEXT_LINES}),
            json!({"action": "wait", "timeout_ms": SHELL_JOB_WAIT_MAX_MS}),
        ] {
            assert!(parse_shell_job_arguments(arguments).is_ok());
        }

        for arguments in [
            json!({"action": "read", "start_line": 0}),
            json!({"action": "read", "max_lines": 0}),
            json!({"action": "read", "max_lines": SHELL_JOB_READ_MAX_LINES + 1}),
            json!({"action": "search", "context_lines": SHELL_JOB_SEARCH_MAX_CONTEXT_LINES + 1}),
            json!({"action": "search", "max_matches": SHELL_JOB_SEARCH_MAX_MATCHES + 1}),
            json!({"action": "wait", "timeout_ms": 0}),
            json!({"action": "wait", "timeout_ms": SHELL_JOB_WAIT_MAX_MS + 1}),
        ] {
            assert!(parse_shell_job_arguments(arguments).is_err());
        }
    }
}
