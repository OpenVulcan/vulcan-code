//! Live output tail of running `shell_command` calls, for display only.
//! 正在运行的 `shell_command` 调用的实时输出尾部，仅用于显示。
//!
//! A running command registers here under the exact tool call that started it and every captured chunk is
//! also appended, in arrival order across stdout and stderr, to a small bounded tail. The tail is never
//! persisted and never reaches the model; the complete output still flows through the normal result. The
//! registration ends when the call returns, so an unknown key simply means the command is no longer running.
//! 正在运行的命令以启动它的精确工具调用为键登记于此；每个捕获到的数据块还会按 stdout 与 stderr 的到达顺序
//! 追加到一段有界的小尾部。尾部从不持久化，也从不交给模型；完整输出仍走正常结果。调用返回时登记结束，因此
//! 未知键仅表示命令已不在运行。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use vulcan_host_tools::output_decode::decode_shell_output_bytes;

/// Number of most recent lines a live tail shows.
/// 实时尾部显示的最近行数。
pub const SHELL_COMMAND_LIVE_TAIL_LINES: usize = 20;

/// Bytes of most recent combined output kept per running command; enough for the shown lines of ordinary
/// output, while a single very long line is cut to its end.
/// 每个运行中命令保留的最近合并输出字节数；足以容纳普通输出的显示行数，超长单行只保留其末尾。
const SHELL_COMMAND_LIVE_TAIL_BYTES: usize = 16 * 1024;

/// Identity of one running `shell_command` call: owner session, API call and tool call.
/// 一次运行中 `shell_command` 调用的身份：所属会话、API 调用与工具调用。
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
        let session_id = uuid::Uuid::parse_str(session_id)
            .map_or_else(|_| session_id.to_owned(), |id| id.to_string());
        Self {
            session_id,
            api_call_id,
            tool_call_id,
        }
    }
}

/// Bounded output tail of one running command plus a change counter for waiters.
/// 一个运行中命令的有界输出尾部，以及供等待方使用的变化计数。
pub struct ShellCommandLiveRun {
    /// Most recent combined output bytes.
    /// 最近的合并输出字节。
    tail: Mutex<Vec<u8>>,
    /// Counts appended chunks and the end of the run, waking waiters on each.
    /// 记录追加的数据块与运行结束，每次都会唤醒等待方。
    changes: tokio::sync::watch::Sender<u64>,
    /// Set once the call has returned.
    /// 调用返回后置位。
    ended: std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for ShellCommandLiveRun {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellCommandLiveRun")
            .finish_non_exhaustive()
    }
}

impl ShellCommandLiveRun {
    /// Appends one captured chunk and drops bytes beyond the bound.
    /// 追加一个捕获到的数据块，并丢弃超出上限的字节。
    pub(crate) fn append(&self, chunk: &[u8]) {
        {
            let mut tail = self.tail.lock().unwrap_or_else(PoisonError::into_inner);
            tail.extend_from_slice(chunk);
            if tail.len() > SHELL_COMMAND_LIVE_TAIL_BYTES {
                let excess = tail.len() - SHELL_COMMAND_LIVE_TAIL_BYTES;
                tail.drain(..excess);
            }
        }
        self.changes
            .send_modify(|count| *count = count.wrapping_add(1));
    }

    /// Returns the most recent lines of the tail.
    /// 返回尾部中最近的若干行。
    pub fn recent_lines(&self) -> Vec<String> {
        let tail = self
            .tail
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        recent_lines_of(&tail, SHELL_COMMAND_LIVE_TAIL_BYTES)
    }

    /// Subscribes to changes, including the end of the run.
    /// 订阅变化，包括运行结束。
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changes.subscribe()
    }

    /// Returns whether the command is still running.
    /// 返回命令是否仍在运行。
    pub fn is_running(&self) -> bool {
        !self.ended.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Splits a byte tail into its last lines, dropping a first line the bound may have cut.
/// 将字节尾部拆分为最后若干行，并丢弃可能被上限截断的首行。
///
/// # Parameters / 参数
/// - `tail`: Most recent combined output bytes.
/// - `bound`: Size at which the tail starts dropping old bytes.
/// - `tail`：最近的合并输出字节。
/// - `bound`：尾部开始丢弃旧字节时的大小。
fn recent_lines_of(tail: &[u8], bound: usize) -> Vec<String> {
    // A full tail may start inside a line or a multi-byte character; start after its first newline.
    // 满载的尾部可能从某行或多字节字符中间开始；从其第一个换行之后开始。
    let start = if tail.len() >= bound {
        tail.iter()
            .position(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1)
    } else {
        0
    };
    let text = decode_shell_output_bytes(&tail[start..]);
    let mut lines = text
        .split('\n')
        // A carriage return redraws the line (progress bars), so only its last segment is on screen.
        // 回车会重绘当前行（进度条），因此屏幕上只剩最后一段。
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            line.rsplit('\r').next().unwrap_or(line).to_owned()
        })
        .collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    let skip = lines.len().saturating_sub(SHELL_COMMAND_LIVE_TAIL_LINES);
    lines.split_off(skip)
}

/// Registry of running commands in this process.
/// 本进程中运行中命令的注册表。
static LIVE_RUNS: OnceLock<Mutex<HashMap<ShellCommandLiveKey, Arc<ShellCommandLiveRun>>>> =
    OnceLock::new();

/// Returns the process-local registry.
/// 返回进程内注册表。
fn live_runs() -> &'static Mutex<HashMap<ShellCommandLiveKey, Arc<ShellCommandLiveRun>>> {
    LIVE_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Looks up one running command.
/// 查找一个运行中的命令。
///
/// # Returns / 返回值
/// Returns `None` when the command has finished or never ran in this process.
/// 命令已结束或从未在本进程运行时返回 `None`。
pub fn shell_command_live_run(key: &ShellCommandLiveKey) -> Option<Arc<ShellCommandLiveRun>> {
    live_runs()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(key)
        .cloned()
}

/// Registration that removes its run when dropped, so every return path of the call ends it.
/// 在释放时移除其运行记录的登记，使调用的每条返回路径都会结束它。
pub(crate) struct ShellCommandLiveRegistration {
    /// Registered key.
    /// 已登记的键。
    key: ShellCommandLiveKey,
    /// Registered run.
    /// 已登记的运行记录。
    run: Arc<ShellCommandLiveRun>,
}

impl ShellCommandLiveRegistration {
    /// Registers one running command.
    /// 登记一个运行中的命令。
    pub(crate) fn register(key: ShellCommandLiveKey) -> Self {
        let (changes, _) = tokio::sync::watch::channel(0);
        let run = Arc::new(ShellCommandLiveRun {
            tail: Mutex::new(Vec::new()),
            changes,
            ended: std::sync::atomic::AtomicBool::new(false),
        });
        live_runs()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), Arc::clone(&run));
        Self { key, run }
    }

    /// Returns the run the output readers append to.
    /// 返回输出读取任务追加到的运行记录。
    pub(crate) fn run(&self) -> Arc<ShellCommandLiveRun> {
        Arc::clone(&self.run)
    }
}

impl Drop for ShellCommandLiveRegistration {
    fn drop(&mut self) {
        let mut runs = live_runs().lock().unwrap_or_else(PoisonError::into_inner);
        // Only remove our own run; a later registration under the same key stays.
        // 只移除自己的运行记录；之后以同一键登记的记录保留。
        if runs
            .get(&self.key)
            .is_some_and(|run| Arc::ptr_eq(run, &self.run))
        {
            runs.remove(&self.key);
        }
        drop(runs);
        // Waiters may still hold the run, so the end is a flag plus one last wake rather than a closed channel.
        // 等待方可能仍持有运行记录，因此结束以标志加一次唤醒表示，而不是关闭通道。
        self.run
            .ended
            .store(true, std::sync::atomic::Ordering::Release);
        self.run
            .changes
            .send_modify(|count| *count = count.wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(tool_call_id: u64) -> ShellCommandLiveKey {
        ShellCommandLiveKey {
            session_id: String::from("live-tail-test-session"),
            api_call_id: 1,
            tool_call_id,
        }
    }

    #[test]
    fn keeps_only_the_most_recent_lines() {
        let registration = ShellCommandLiveRegistration::register(key(1));
        let run = registration.run();
        for index in 0..(SHELL_COMMAND_LIVE_TAIL_LINES + 5) {
            run.append(format!("line {index}\r\n").as_bytes());
        }
        let lines = run.recent_lines();
        assert_eq!(lines.len(), SHELL_COMMAND_LIVE_TAIL_LINES);
        assert_eq!(
            lines.last().map(String::as_str),
            Some(format!("line {}", SHELL_COMMAND_LIVE_TAIL_LINES + 4).as_str())
        );
    }

    #[test]
    fn carriage_return_keeps_the_redrawn_segment() {
        assert_eq!(
            recent_lines_of(b"start\n10%\r50%\r100%\n", usize::MAX),
            vec![String::from("start"), String::from("100%")]
        );
    }

    #[test]
    fn a_full_tail_drops_its_cut_first_line() {
        let mut tail = vec![b'x'; 8];
        tail.extend_from_slice(b"\nwhole\n");
        assert_eq!(
            recent_lines_of(&tail, tail.len()),
            vec![String::from("whole")]
        );
    }

    #[test]
    fn the_bound_holds_for_long_output() {
        let registration = ShellCommandLiveRegistration::register(key(2));
        let run = registration.run();
        run.append(&vec![b'a'; SHELL_COMMAND_LIVE_TAIL_BYTES * 2]);
        assert_eq!(
            run.tail.lock().unwrap().len(),
            SHELL_COMMAND_LIVE_TAIL_BYTES
        );
    }

    #[tokio::test]
    async fn ending_the_call_unregisters_and_wakes_waiters() {
        let registration = ShellCommandLiveRegistration::register(key(3));
        let run = registration.run();
        let mut receiver = run.subscribe();
        assert!(shell_command_live_run(&key(3)).is_some());
        drop(registration);
        assert!(shell_command_live_run(&key(3)).is_none());
        assert!(receiver.changed().await.is_ok());
        assert!(!run.is_running());
    }
}
