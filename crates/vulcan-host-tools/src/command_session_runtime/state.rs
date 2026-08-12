use super::*;

/// Mutable per-session process state that must stay synchronized across output readers and control actions.
/// 必须在输出读取器与控制动作之间保持同步的每会话可变进程状态。
#[derive(Debug)]
pub(super) struct CommandSessionMutableState {
    /// Current lifecycle status tracked by the runtime.
    /// 运行时当前跟踪的生命周期状态。
    pub(super) status: CommandSessionStatus,
    /// Optional exit code observed after the process exited.
    /// 进程退出后观测到的可选退出码。
    pub(super) exit_code: Option<i32>,
    /// Optional failure reason synthesized by the runtime.
    /// 由运行时综合生成的可选失败原因。
    pub(super) failure_reason: Option<String>,
    /// Accumulated decoded stdout byte count.
    /// 已累计的解码后标准输出字节数。
    pub(super) stdout_bytes: usize,
    /// Accumulated decoded stderr byte count.
    /// 已累计的解码后标准错误字节数。
    pub(super) stderr_bytes: usize,
    /// Highest assigned output sequence number.
    /// 当前已分配的最高输出序号。
    pub(super) last_seq: u64,
    /// Highest output sequence evicted from bounded retained history.
    /// 已从有界保留历史中淘汰的最高输出序号。
    pub(super) dropped_before_seq: u64,
    /// Highest output sequence number already exposed to at least one read caller.
    /// 已经暴露给至少一个读取调用方的最高输出序号。
    pub(super) highest_delivered_seq: u64,
    /// Highest output sequence number delivered through the AI-facing terminal-session cursor.
    /// 已经通过面向 AI 的终端会话游标交付的最高输出序号。
    pub(super) ai_delivered_seq: u64,
    /// Whether the latest stdin write left one open input line without a trailing newline.
    /// 最近一次标准输入写入是否留下了一条未以换行结束的开放输入行。
    pub(super) pending_stdin_line: bool,
    /// Latest screen frame sequence reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新屏幕帧序号。
    pub(super) screen_frame_seq: Option<u64>,
    /// Latest screen row count reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新屏幕行数。
    pub(super) screen_rows: Option<u16>,
    /// Latest screen column count reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新屏幕列数。
    pub(super) screen_cols: Option<u16>,
    /// Latest renderable screen text reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新可渲染屏幕内容。
    pub(super) screen_text: Option<String>,
    /// Buffered output chunks retained for future read calls.
    /// 为后续读取调用保留的缓冲输出分片。
    pub(super) chunks: VecDeque<CommandSessionOutputChunk>,
    /// Total UTF-8 bytes currently retained by output chunks.
    /// 输出分片当前保留的 UTF-8 总字节数。
    pub(super) retained_output_bytes: usize,
}

impl Default for CommandSessionMutableState {
    /// Builds one fresh running-state record for a newly spawned command session.
    /// 为新派生的命令会话构建一份初始运行态记录。
    fn default() -> Self {
        Self {
            status: CommandSessionStatus::Running,
            exit_code: None,
            failure_reason: None,
            stdout_bytes: 0,
            stderr_bytes: 0,
            last_seq: 0,
            dropped_before_seq: 0,
            highest_delivered_seq: 0,
            ai_delivered_seq: 0,
            pending_stdin_line: false,
            screen_frame_seq: None,
            screen_rows: None,
            screen_cols: None,
            screen_text: None,
            chunks: VecDeque::new(),
            retained_output_bytes: 0,
        }
    }
}

/// Shared mutable state plus one notification primitive owned by a command session record.
/// 由命令会话记录持有的共享可变状态与通知原语。
#[derive(Debug, Default)]
pub(super) struct CommandSessionSharedState {
    /// Mutable session process state protected for concurrent async access.
    /// 为并发异步访问加锁保护的可变会话进程状态。
    pub(super) mutable_state: Mutex<CommandSessionMutableState>,
    /// Notification primitive fired on new output or lifecycle transitions.
    /// 在出现新输出或生命周期切换时触发的通知原语。
    pub(super) change_notify: Notify,
}

impl CommandSessionSharedState {
    /// Appends one decoded output chunk and wakes pending readers.
    /// 追加一条解码后的输出分片并唤醒等待中的读取方。
    pub(super) async fn append_output_chunk(
        &self,
        stream: CommandSessionOutputStream,
        text: String,
    ) {
        if text.is_empty() {
            return;
        }
        let mut state = self.mutable_state.lock().await;
        append_output_chunk_to_state(&mut state, stream, text);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Atomically appends one decoded output chunk and its optional terminal screen checkpoint.
    /// 以原子方式追加一条解码输出分片及其可选终端屏幕检查点。
    ///
    /// # Parameters / 参数
    /// - `stream`: Logical output stream assigned to the decoded PTY bytes.
    /// - `text`: Decoded output text retained for incremental readers.
    /// - `screen_frame`: Optional sequence, rows, columns, and formatted VT checkpoint.
    /// - `stream`：分配给已解码 PTY 字节的逻辑输出流。
    /// - `text`：为增量读取方保留的解码输出文本。
    /// - `screen_frame`：可选的序号、行数、列数与格式化 VT 检查点。
    pub(super) async fn append_output_chunk_with_screen_frame(
        &self,
        stream: CommandSessionOutputStream,
        text: String,
        screen_frame: Option<(u64, u16, u16, String)>,
    ) {
        if text.is_empty() && screen_frame.is_none() {
            return;
        }
        let mut state = self.mutable_state.lock().await;
        if !text.is_empty() {
            append_output_chunk_to_state(&mut state, stream, text);
        }
        if let Some((sequence, rows, cols, frame_text)) = screen_frame {
            state.screen_frame_seq = Some(sequence);
            state.screen_rows = Some(rows);
            state.screen_cols = Some(cols);
            state.screen_text = Some(frame_text);
        }
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Records one runtime-generated failure reason without immediately overriding the running status.
    /// 记录一条运行时生成的失败原因，而不立即覆盖运行状态。
    pub(super) async fn remember_failure_reason(&self, reason: String) {
        let mut state = self.mutable_state.lock().await;
        if state.failure_reason.is_none() {
            state.failure_reason = Some(reason);
        }
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Updates whether the latest stdin submission left one open line and wakes pending readers.
    /// 更新最近一次标准输入提交是否留下开放输入行，并唤醒等待中的读取方。
    pub(super) async fn set_pending_stdin_line(&self, pending_stdin_line: bool) {
        let mut state = self.mutable_state.lock().await;
        state.pending_stdin_line = pending_stdin_line;
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Stores the latest terminal screen frame and wakes pending readers.
    /// 保存最新终端屏幕帧并唤醒等待中的读取方。
    #[cfg(test)]
    pub(super) async fn update_screen_frame(&self, seq: u64, rows: u16, cols: u16, text: String) {
        let mut state = self.mutable_state.lock().await;
        state.screen_frame_seq = Some(seq);
        state.screen_rows = Some(rows);
        state.screen_cols = Some(cols);
        state.screen_text = Some(text);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Marks the session as host-stopped before process reaping completes, so UI stop/close paths never wait on incompatible terminal I/O forever.
    /// 在进程回收完成前先把会话标记为宿主停止，确保 UI 停止/关闭路径不会永久等待不兼容的终端 I/O。
    pub(super) async fn mark_stop_requested(&self) {
        let mut state = self.mutable_state.lock().await;
        mark_stop_requested_state(&mut state);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Finalizes the session after the child process has reached a terminal state.
    /// 在子进程到达终态后完成会话收尾。
    pub(super) async fn finalize_process_exit(
        &self,
        stop_requested: bool,
        exit_code: Option<i32>,
        wait_error: Option<String>,
        process_succeeded: bool,
    ) {
        let mut state = self.mutable_state.lock().await;
        finalize_process_exit_state(
            &mut state,
            stop_requested,
            exit_code,
            wait_error,
            process_succeeded,
        );
        drop(state);
        self.change_notify.notify_waiters();
    }
}

/// Appends one decoded output chunk into a mutable session state.
/// 将一条解码输出分片追加进可变会话状态。
pub(super) fn append_output_chunk_to_state(
    state: &mut CommandSessionMutableState,
    stream: CommandSessionOutputStream,
    text: String,
) {
    let bytes = text.len();
    match stream {
        CommandSessionOutputStream::Stdout => {
            state.stdout_bytes = state.stdout_bytes.saturating_add(bytes);
        }
        CommandSessionOutputStream::Stderr => {
            state.stderr_bytes = state.stderr_bytes.saturating_add(bytes);
        }
    }
    if can_coalesce_output_chunk(
        state.chunks.back(),
        state.highest_delivered_seq,
        stream,
        bytes,
    ) {
        if let Some(last_chunk) = state.chunks.back_mut() {
            last_chunk.text.push_str(&text);
        }
    } else {
        state.last_seq = state.last_seq.saturating_add(1);
        let seq = state.last_seq;
        state
            .chunks
            .push_back(CommandSessionOutputChunk { seq, stream, text });
    }
    state.retained_output_bytes = state.retained_output_bytes.saturating_add(bytes);
    while state.chunks.len() > COMMAND_SESSION_RETAINED_CHUNK_LIMIT
        || state.retained_output_bytes > COMMAND_SESSION_RETAINED_BYTE_LIMIT
    {
        let Some(dropped) = state.chunks.pop_front() else {
            break;
        };
        state.retained_output_bytes = state
            .retained_output_bytes
            .saturating_sub(dropped.text.len());
        state.dropped_before_seq = dropped.seq;
    }
}

/// Marks one mutable session state as host-stopped.
/// 将一份可变会话状态标记为宿主停止。
pub(super) fn mark_stop_requested_state(state: &mut CommandSessionMutableState) {
    if state.status == CommandSessionStatus::Running {
        state.status = CommandSessionStatus::Stopped;
        state.exit_code = None;
        state.failure_reason = None;
        state.pending_stdin_line = false;
    }
}

/// Finalizes one mutable session state after its process reaches a terminal state.
/// 在进程到达终态后完成一份可变会话状态收尾。
pub(super) fn finalize_process_exit_state(
    state: &mut CommandSessionMutableState,
    stop_requested: bool,
    exit_code: Option<i32>,
    wait_error: Option<String>,
    process_succeeded: bool,
) {
    state.exit_code = exit_code;
    if let Some(error) = wait_error {
        state.status = CommandSessionStatus::Failed;
        state.failure_reason = Some(error);
    } else if state.status == CommandSessionStatus::Failed && state.failure_reason.is_some() {
        // Preserve the earlier runtime failure reason instead of overwriting it with a kill or exit-code summary.
        // 保留更早记录的运行时失败原因，而不是用 kill 或退出码摘要把它覆盖掉。
    } else if stop_requested {
        state.status = CommandSessionStatus::Stopped;
        state.failure_reason = None;
    } else if process_succeeded {
        state.status = CommandSessionStatus::Completed;
        state.failure_reason = None;
    } else {
        state.status = CommandSessionStatus::Failed;
        if state.failure_reason.is_none() {
            state.failure_reason = Some(match exit_code {
                Some(code) => format!("The command session exited with status code {code}."),
                None => String::from("The command session failed without returning an exit code."),
            });
        }
    }
}

/// Reports whether one output chunk already ends on a stable line boundary.
/// 判断一条输出分片是否已经稳定结束在行边界上。
pub(super) fn output_chunk_ends_with_line_boundary(text: &str) -> bool {
    text.ends_with(['\n', '\r'])
}

/// Reports whether one new fragment may be merged into the last undelivered chunk without breaking read-cursor stability.
/// 判断一条新碎片是否可以安全合并进最后一个未投递分片，而不破坏读取游标稳定性。
pub(super) fn can_coalesce_output_chunk(
    last_chunk: Option<&CommandSessionOutputChunk>,
    highest_delivered_seq: u64,
    incoming_stream: CommandSessionOutputStream,
    incoming_bytes: usize,
) -> bool {
    let Some(last_chunk) = last_chunk else {
        return false;
    };
    last_chunk.seq > highest_delivered_seq
        && last_chunk.stream == incoming_stream
        && !output_chunk_ends_with_line_boundary(&last_chunk.text)
        && last_chunk.text.len().saturating_add(incoming_bytes)
            <= COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES
}
