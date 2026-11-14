//! Shell jobs: every one-shot shell command runs as a job owned by this runtime instead of by the tool call.
//! Shell 任务：每条一次性 shell 命令都作为由本运行时持有的任务运行，而不是由工具调用持有。
//!
//! A tool call attaches to its job in the foreground and waits; it may detach the job into the background,
//! after which the process keeps running and its output keeps flowing into the job log. The log stores the
//! decoded text in bounded rolling segments inside the owning session directory, so callers return only a
//! bounded tail and read the rest by line range or search.
//! 工具调用以前台方式附着到任务并等待；它可以把任务转到后台，此后进程继续运行，输出继续写入任务日志。
//! 日志把解码后的文本以有界滚动分段保存在所属会话目录中，因此调用方只返回有界尾部，其余内容按行范围或
//! 搜索读取。

mod diagnostics;
mod index;
mod log;
mod redaction;
mod runtime;
mod text;

#[cfg(test)]
mod tests;

pub use diagnostics::{ShellJobProcess, snapshot_process_tree};
pub use index::{ShellJobIndexError, ShellJobRecord, read_session_job_records};
pub use log::{ShellJobLineRange, ShellJobLogReader, ShellJobLogStats, ShellJobSearchMatch};
pub use redaction::ShellJobLogRedaction;
pub use runtime::{
    ShellJob, ShellJobCapturedStreams, ShellJobDirectoryResolver, ShellJobEvent, ShellJobEventKind,
    ShellJobFinal, ShellJobForeground, ShellJobNotice, ShellJobOrigin, ShellJobRuntime,
    ShellJobSpawn, ShellJobSpawnError, ShellJobState, ShellJobStatus, ShellJobSummary,
    ShellJobWaitOutcome, valid_job_id,
};
pub use text::{
    ShellJobLineRow, ShellJobRowKind, ShellJobTail, render_numbered_rows, render_plain_rows,
};

/// Lines one `shell_job` read returns when the caller gives no count.
/// 调用方未给出行数时，一次 `shell_job` 读取返回的行数。
pub const SHELL_JOB_READ_DEFAULT_LINES: usize = 200;

/// Most lines one `shell_job` read returns.
/// 一次 `shell_job` 读取最多返回的行数。
pub const SHELL_JOB_READ_MAX_LINES: usize = 2_000;

/// Wait time of `shell_job` `wait` when the caller gives none.
/// 调用方未指定时 `shell_job` `wait` 的等待时间。
pub const SHELL_JOB_WAIT_DEFAULT_MS: u64 = 60_000;

/// Bytes of raw stdout and of raw stderr kept in memory for a foreground call, from the start of each stream.
/// 前台调用在内存中保留的原始 stdout 与原始 stderr 字节数（各自从流的开头算起）。
///
/// Only tool-chain steps read these streams; the full output stays in the job log.
/// 只有工具链步骤读取这些流；完整输出保留在任务日志中。
pub const SHELL_JOB_STREAM_CAPTURE_BYTES: usize = 4 * 1024 * 1024;

/// Byte budget of the output tail a finished or detached command returns; shorter output returns in full.
/// 已结束或已转后台命令返回的输出尾部字节预算；更短的输出完整返回。
pub const SHELL_OUTPUT_TAIL_BUDGET_BYTES: usize = 32 * 1024;

/// Longest display row; a longer line is split into several rows at character boundaries.
/// 最长显示行；更长的行在字符边界处切分为多个显示行。
pub const SHELL_OUTPUT_LINE_WRAP_BYTES: usize = 2 * 1024;

/// Leading bytes of every job log that are always kept, since a command's opening lines often matter.
/// 每个任务日志始终保留的开头字节数，因为命令开头几行往往很重要。
pub const SHELL_JOB_LOG_HEAD_BYTES: u64 = 64 * 1024;

/// Size of one rolling body segment after the head.
/// 开头之后单个滚动正文分段的大小。
pub const SHELL_JOB_LOG_SEGMENT_BYTES: u64 = 4 * 1024 * 1024;

/// Upper bound of one job log kept on disk; the oldest body segments are dropped beyond it.
/// 单个任务日志在磁盘上保留的上限；超出时丢弃最早的正文分段。
pub const SHELL_JOB_LOG_LIMIT_BYTES: u64 = 64 * 1024 * 1024;

/// Upper bound of all job logs of one session; the logs of the oldest finished jobs are deleted beyond it.
/// 单个会话全部任务日志的上限；超出时删除最早已结束任务的日志。
pub const SHELL_JOB_SESSION_LOG_BUDGET_BYTES: u64 = 512 * 1024 * 1024;

/// Most background jobs one session may run at once when the model asks for background execution.
/// 模型请求后台执行时，单个会话可同时运行的后台任务上限。
pub const MAX_RUNNING_BACKGROUND_SHELL_JOBS: usize = 8;

/// Longest single `shell_job` wait; a longer job reports that it is still running.
/// 单次 `shell_job` 等待的最长时长；更久的任务报告仍在运行。
pub const SHELL_JOB_WAIT_MAX_MS: u64 = 600_000;

/// Most matches one log search returns.
/// 单次日志搜索返回的最多匹配数。
pub const SHELL_JOB_SEARCH_MAX_MATCHES: usize = 100;

/// Most context lines around one search match.
/// 单个搜索匹配前后的最多上下文行数。
pub const SHELL_JOB_SEARCH_MAX_CONTEXT_LINES: usize = 5;

/// Directory, inside one session directory, that holds the job index and the job logs.
/// 会话目录中保存任务索引与任务日志的目录。
pub const SHELL_JOBS_DIRECTORY: &str = "shell-jobs";
