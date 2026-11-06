//! Live output tail of running `shell_command` calls, for display only.
//! 正在运行的 `shell_command` 调用的实时输出尾部，仅用于显示。
//!
//! Every `shell_command` call runs as a shell job that records its output in a rolling log. A command card
//! finds the job by the exact tool call that started it and reads the most recent lines from that log, so the
//! tail keeps working after the command moved to the background. Nothing here reaches the model.
//! 每次 `shell_command` 调用都作为把输出记录到滚动日志的 shell 任务运行。命令卡按启动它的精确工具调用找到
//! 任务，并从该日志读取最近若干行，因此命令转到后台后尾部仍然可用。这里的内容从不交给模型。

use std::sync::Arc;

use vulcan_host_tools::shell_job::{
    SHELL_OUTPUT_TAIL_BUDGET_BYTES, ShellJob, ShellJobOrigin, ShellJobRowKind, ShellJobRuntime,
};

/// Number of most recent lines a live tail shows.
/// 实时尾部显示的最近行数。
pub const SHELL_COMMAND_LIVE_TAIL_LINES: usize = 20;

/// Writes a UUID session id in its canonical form so job owners, lookups and signals agree.
/// 将 UUID 形式的会话标识写为规范形式，使任务所有者、查找与信号保持一致。
pub fn canonical_shell_job_session_id(session_id: &str) -> String {
    uuid::Uuid::parse_str(session_id).map_or_else(|_| session_id.to_owned(), |id| id.to_string())
}

/// Identity of one `shell_command` call: owner session, API call and tool call.
/// 一次 `shell_command` 调用的身份：所属会话、API 调用与工具调用。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShellCommandLiveKey {
    /// Owner session identity as text.
    /// 文本形式的所属会话身份。
    pub session_id: String,
    /// API call that issued the tool call.
    /// 发出该工具调用的 API 调用。
    pub api_call_id: u64,
    /// Tool call inside that API call.
    /// 该 API 调用内的工具调用。
    pub tool_call_id: u64,
}

impl ShellCommandLiveKey {
    /// Builds a key, writing a UUID session id in its canonical form so callers and lookups agree.
    /// 构建键；UUID 形式的会话标识写为规范形式，使登记方与查找方一致。
    pub fn new(session_id: &str, api_call_id: u64, tool_call_id: u64) -> Self {
        Self {
            session_id: canonical_shell_job_session_id(session_id),
            api_call_id,
            tool_call_id,
        }
    }
}

/// Live view of one command's job.
/// 一个命令任务的实时视图。
#[derive(Debug, Clone)]
pub struct ShellCommandLiveRun {
    /// The job.
    /// 该任务。
    job: Arc<ShellJob>,
}

impl ShellCommandLiveRun {
    /// Returns the most recent lines of the output.
    /// 返回输出中最近的若干行。
    pub fn recent_lines(&self) -> Vec<String> {
        shell_job_recent_lines(&self.job, SHELL_COMMAND_LIVE_TAIL_LINES)
    }

    /// Subscribes to changes, including the end of the run.
    /// 订阅变化，包括运行结束。
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.job.subscribe_output()
    }

    /// Returns whether the command is still running.
    /// 返回命令是否仍在运行。
    pub fn is_running(&self) -> bool {
        self.job.outcome().is_none()
    }

    /// Job identity.
    /// 任务身份。
    pub fn job_id(&self) -> &str {
        self.job.id()
    }
}

/// Returns the most recent display lines of one job's output as they show on screen, oldest first.
/// 返回一个任务输出中最近的若干显示行（按屏幕上的显示效果），从旧到新。
pub fn shell_job_recent_lines(job: &ShellJob, max_lines: usize) -> Vec<String> {
    let Some(reader) = job.log_reader() else {
        return Vec::new();
    };
    let rows = reader
        .tail_rows(max_lines, SHELL_OUTPUT_TAIL_BUDGET_BYTES)
        .unwrap_or_default();
    rows.iter()
        .filter(|row| row.kind != ShellJobRowKind::Separator)
        .map(|row| screen_text(&row.text))
        .collect()
}

/// Text left on screen by a line: a carriage return redraws the line (progress bars), so only its last
/// segment shows.
/// 一行在屏幕上留下的文本：回车会重绘当前行（进度条），因此只显示最后一段。
fn screen_text(text: &str) -> String {
    text.rsplit('\r').next().unwrap_or(text).to_owned()
}

/// Looks up the job of one command call.
/// 查找一次命令调用的任务。
///
/// # Returns / 返回值
/// Returns `None` when the call started no job in this process or its job is no longer kept in memory.
/// 该调用未在本进程启动任务，或其任务已不在内存中时返回 `None`。
pub fn shell_command_live_run(key: &ShellCommandLiveKey) -> Option<ShellCommandLiveRun> {
    ShellJobRuntime::global()
        .job_for_origin(
            &key.session_id,
            ShellJobOrigin {
                api_call_id: key.api_call_id,
                tool_call_id: key.tool_call_id,
            },
        )
        .map(|job| ShellCommandLiveRun { job })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carriage_return_keeps_the_redrawn_segment() {
        assert_eq!(screen_text("10%\r50%\r100%"), "100%");
        assert_eq!(screen_text("plain"), "plain");
    }

    #[test]
    fn uuid_session_ids_are_canonical() {
        let id = "6F9619FF-8B86-D011-B42D-00C04FC964FF";
        assert_eq!(
            canonical_shell_job_session_id(id),
            "6f9619ff-8b86-d011-b42d-00c04fc964ff"
        );
        assert_eq!(canonical_shell_job_session_id("direct"), "direct");
    }
}
