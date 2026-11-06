//! Append-only job index of one session: `shell-jobs/index.jsonl`.
//! 单个会话的追加式任务索引：`shell-jobs/index.jsonl`。
//!
//! Every line is one versioned event; reading merges the events of each job in start order. Only version 1
//! exists, and any other version is refused. A torn last line from an interrupted write is ignored.
//! 每行是一个带版本的事件；读取时按启动顺序合并每个任务的事件。目前只有版本 1，其他版本一律拒绝。
//! 中断写入留下的不完整最后一行会被忽略。

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::runtime::{ShellJobFinal, ShellJobOrigin};

/// Version of the index line format.
/// 索引行格式的版本。
const SHELL_JOB_INDEX_VERSION: u32 = 1;

/// File name of the index inside the session's job directory.
/// 会话任务目录中索引的文件名。
pub(crate) const SHELL_JOB_INDEX_FILE: &str = "index.jsonl";

/// One index line.
/// 一个索引行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ShellJobIndexLine {
    /// A job started.
    /// 一个任务已启动。
    Started {
        /// Line format version.
        /// 行格式版本。
        version: u32,
        /// Job identity.
        /// 任务身份。
        job_id: String,
        /// Command as the model wrote it.
        /// 模型书写的命令。
        command: String,
        /// Description of what the command is for.
        /// 命令用途说明。
        purpose: String,
        /// Shell label.
        /// Shell 标签。
        shell: String,
        /// Working directory.
        /// 工作目录。
        cwd: String,
        /// Start time in Unix milliseconds.
        /// 启动时间（Unix 毫秒）。
        started_at_ms: u64,
        /// Tool call that started the job, when one did.
        /// 启动该任务的工具调用（如有）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<ShellJobOrigin>,
        /// Whether the job was started in the background.
        /// 任务是否以后台方式启动。
        background: bool,
    },
    /// A foreground job moved to the background.
    /// 一个前台任务转到了后台。
    Backgrounded {
        /// Line format version.
        /// 行格式版本。
        version: u32,
        /// Job identity.
        /// 任务身份。
        job_id: String,
        /// Time in Unix milliseconds.
        /// 时间（Unix 毫秒）。
        at_ms: u64,
    },
    /// A job finished.
    /// 一个任务已结束。
    Finished {
        /// Line format version.
        /// 行格式版本。
        version: u32,
        /// Job identity.
        /// 任务身份。
        job_id: String,
        /// Final outcome.
        /// 最终结果。
        outcome: ShellJobFinal,
    },
}

impl ShellJobIndexLine {
    /// Line format version.
    /// 行格式版本。
    fn version(&self) -> u32 {
        match self {
            Self::Started { version, .. }
            | Self::Backgrounded { version, .. }
            | Self::Finished { version, .. } => *version,
        }
    }

    /// Current line format version.
    /// 当前行格式版本。
    pub(crate) fn current_version() -> u32 {
        SHELL_JOB_INDEX_VERSION
    }
}

/// One job as recorded in a session's index.
/// 会话索引中记录的一个任务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobRecord {
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Command as the model wrote it.
    /// 模型书写的命令。
    pub command: String,
    /// Description of what the command is for.
    /// 命令用途说明。
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
    /// Tool call that started the job, when one did.
    /// 启动该任务的工具调用（如有）。
    pub origin: Option<ShellJobOrigin>,
    /// True when the job ran in the background, from the start or after detaching.
    /// 任务曾在后台运行（启动即后台或之后转入）时为 true。
    pub background: bool,
    /// Final outcome; none while running, or when the service stopped before the job finished.
    /// 最终结果；运行中，或服务在任务结束前停止时为 none。
    pub outcome: Option<ShellJobFinal>,
}

/// Failure to read a session's job index.
/// 读取会话任务索引失败。
#[derive(Debug)]
pub enum ShellJobIndexError {
    /// The index could not be read.
    /// 无法读取索引。
    Io(io::Error),
    /// One line is not a valid index line.
    /// 某一行不是有效的索引行。
    Malformed {
        /// One-based line number.
        /// 从 1 开始的行号。
        line: usize,
        /// Parser message.
        /// 解析器消息。
        message: String,
    },
    /// One line uses a version this build does not know.
    /// 某一行使用了本构建不认识的版本。
    UnsupportedVersion {
        /// One-based line number.
        /// 从 1 开始的行号。
        line: usize,
        /// Version found.
        /// 发现的版本。
        version: u32,
    },
}

impl std::fmt::Display for ShellJobIndexError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "Failed to read the shell job index: {error}"),
            Self::Malformed { line, message } => {
                write!(
                    formatter,
                    "Shell job index line {line} is malformed: {message}"
                )
            }
            Self::UnsupportedVersion { line, version } => write!(
                formatter,
                "Shell job index line {line} uses unsupported version {version}; this build reads version {SHELL_JOB_INDEX_VERSION}."
            ),
        }
    }
}

impl std::error::Error for ShellJobIndexError {}

/// Appends one line to the index in `jobs_dir`.
/// 向 `jobs_dir` 中的索引追加一行。
pub(crate) fn append_index_line(jobs_dir: &Path, line: &ShellJobIndexLine) -> io::Result<()> {
    fs::create_dir_all(jobs_dir)?;
    let mut text = serde_json::to_string(line).map_err(io::Error::other)?;
    text.push('\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(jobs_dir.join(SHELL_JOB_INDEX_FILE))?;
    file.write_all(text.as_bytes())
}

/// Reads the jobs recorded in `jobs_dir`, in start order; a missing index means no jobs.
/// 按启动顺序读取 `jobs_dir` 中记录的任务；索引不存在表示没有任务。
pub fn read_session_job_records(
    jobs_dir: &Path,
) -> Result<Vec<ShellJobRecord>, ShellJobIndexError> {
    let text = match fs::read_to_string(jobs_dir.join(SHELL_JOB_INDEX_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ShellJobIndexError::Io(error)),
    };
    let terminated = text.ends_with('\n');
    let lines: Vec<&str> = text.lines().collect();
    let mut records: Vec<ShellJobRecord> = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let parsed = match serde_json::from_str::<ShellJobIndexLine>(raw) {
            Ok(parsed) => parsed,
            // A torn final line from an interrupted append carries no committed event.
            // 中断追加留下的不完整最后一行不含已提交事件。
            Err(_) if index + 1 == lines.len() && !terminated => break,
            Err(error) => {
                return Err(ShellJobIndexError::Malformed {
                    line: index + 1,
                    message: error.to_string(),
                });
            }
        };
        if parsed.version() != SHELL_JOB_INDEX_VERSION {
            return Err(ShellJobIndexError::UnsupportedVersion {
                line: index + 1,
                version: parsed.version(),
            });
        }
        match parsed {
            ShellJobIndexLine::Started {
                job_id,
                command,
                purpose,
                shell,
                cwd,
                started_at_ms,
                origin,
                background,
                ..
            } => records.push(ShellJobRecord {
                job_id,
                command,
                purpose,
                shell,
                cwd,
                started_at_ms,
                origin,
                background,
                outcome: None,
            }),
            ShellJobIndexLine::Backgrounded { job_id, .. } => {
                if let Some(record) = records.iter_mut().find(|record| record.job_id == job_id) {
                    record.background = true;
                }
            }
            ShellJobIndexLine::Finished {
                job_id, outcome, ..
            } => {
                if let Some(record) = records.iter_mut().find(|record| record.job_id == job_id) {
                    record.outcome = Some(outcome);
                }
            }
        }
    }
    Ok(records)
}
