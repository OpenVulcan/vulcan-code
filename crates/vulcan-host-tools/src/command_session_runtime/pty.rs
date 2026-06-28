use super::*;

pub(super) async fn write_pty_command_session_stdin(
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    bytes: Vec<u8>,
) -> Result<usize, String> {
    let byte_count = bytes.len();
    let write_task = tokio::task::spawn_blocking(move || {
        let mut writer = writer
            .lock()
            .map_err(|_| String::from("The PTY stdin writer lock is poisoned."))?;
        writer
            .write_all(&bytes)
            .map_err(|error| format!("Failed to write PTY stdin: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("Failed to flush PTY stdin: {error}"))?;
        Ok::<usize, String>(byte_count)
    });
    timeout(
        Duration::from_millis(COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS),
        write_task,
    )
    .await
    .map_err(|_| {
        format!("Timed out writing PTY stdin after {COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS}ms.")
    })?
    .map_err(|error| format!("The PTY stdin writer task failed: {error}"))?
}

/// Requests PTY child termination through a cloned child killer.
/// 通过克隆出的子进程终止器请求 PTY 子进程停止。
pub(super) async fn request_pty_command_session_child_stop(
    killer: Arc<StdMutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut killer = killer
            .lock()
            .map_err(|_| String::from("The PTY child killer lock is poisoned."))?;
        killer
            .kill()
            .map_err(|error| format!("Failed to stop the PTY command-session process: {error}"))
    })
    .await
    .map_err(|error| format!("The PTY child killer task failed: {error}"))?
}

/// Rolling renderable screen frame synthesized from PTY output bytes.
/// 从 PTY 输出字节综合出的滚动可渲染屏幕帧。
pub(super) struct PtyScreenFrameState {
    /// Screen rows retained for WebUI frame display.
    /// WebUI 帧显示保留的屏幕行数。
    rows: u16,
    /// Screen columns reported for WebUI frame display.
    /// WebUI 帧显示报告的屏幕列数。
    cols: u16,
    /// Completed visible lines retained in the rolling frame.
    /// 滚动帧中保留的已完成可见行。
    lines: Vec<String>,
    /// Current mutable line that has not ended with a newline.
    /// 尚未以换行结束的当前可变行。
    current_line: String,
    /// Monotonic frame sequence emitted when visible text changes.
    /// 可见文本变化时递增的帧序号。
    frame_seq: u64,
    /// Last emitted frame text used to suppress duplicate updates.
    /// 用于抑制重复更新的上一帧文本。
    last_frame_text: String,
}

impl PtyScreenFrameState {
    /// Creates one default rolling PTY screen frame state.
    /// 创建一份默认滚动 PTY 屏幕帧状态。
    pub(super) fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            lines: Vec::new(),
            current_line: String::new(),
            frame_seq: 0,
            last_frame_text: String::new(),
        }
    }

    /// Pushes one decoded PTY output fragment and returns a new frame when visible text changed.
    /// 推入一段已解码 PTY 输出，并在可见文本变化时返回新帧。
    pub(super) fn push_output(&mut self, text: &str) -> Option<(u64, u16, u16, String)> {
        if text.is_empty() {
            return None;
        }
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '\x1b' => {
                    self.current_line.push(ch);
                    if chars.peek() == Some(&'[') {
                        self.current_line.push(chars.next().unwrap_or('['));
                        for next in chars.by_ref() {
                            self.current_line.push(next);
                            if next.is_ascii_alphabetic() || matches!(next, '~' | '@') {
                                break;
                            }
                        }
                    }
                }
                '\r' => {
                    self.current_line.clear();
                }
                '\n' => {
                    self.finish_current_line();
                }
                '\t' => {
                    self.current_line.push_str("    ");
                }
                ch if ch.is_control() => {}
                ch => {
                    self.current_line.push(ch);
                }
            }
        }
        let frame_text = self.frame_text();
        if frame_text.trim().is_empty() || frame_text == self.last_frame_text {
            return None;
        }
        self.frame_seq = self.frame_seq.saturating_add(1);
        self.last_frame_text = frame_text.clone();
        Some((self.frame_seq, self.rows, self.cols, frame_text))
    }

    /// Moves the current line into the retained line buffer.
    /// 将当前行移动到保留行缓冲区。
    pub(super) fn finish_current_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.current_line));
        let max_lines = self.rows.saturating_sub(1).max(1) as usize;
        if self.lines.len() > max_lines {
            let remove_count = self.lines.len() - max_lines;
            self.lines.drain(0..remove_count);
        }
    }

    /// Builds the current renderable frame text.
    /// 构建当前可渲染帧文本。
    pub(super) fn frame_text(&self) -> String {
        let mut lines = self.lines.clone();
        if !self.current_line.is_empty() {
            lines.push(self.current_line.clone());
        }
        lines.join("\n")
    }
}

/// Appends PTY output to the chunk buffer and mirrors it into the live WebUI screen frame.
/// 将 PTY 输出追加到分片缓冲区，并镜像到 WebUI 实时屏幕帧。
pub(super) fn append_pty_output_and_update_screen(
    shared_state: &Arc<CommandSessionSharedState>,
    screen_state: &mut PtyScreenFrameState,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    shared_state.append_output_chunk_blocking(CommandSessionOutputStream::Stdout, text.clone());
    if let Some((seq, rows, cols, frame_text)) = screen_state.push_output(&text) {
        shared_state.update_screen_frame_blocking(seq, rows, cols, frame_text);
    }
}

/// Pumps one PTY reader stream into the shared command-session output buffer.
/// 将一条 PTY 读取流持续泵入共享命令会话输出缓冲区。
pub(super) fn pump_pty_command_session_output(
    mut reader: Box<dyn Read + Send>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    shared_state: Arc<CommandSessionSharedState>,
) {
    let mut output_decoder = ShellOutputStreamDecoder::new();
    let mut screen_state =
        PtyScreenFrameState::new(DEFAULT_PTY_SCREEN_ROWS, DEFAULT_PTY_SCREEN_COLS);
    let mut buffer = vec![0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                let flushed_text = output_decoder.finish();
                let flushed_text = answer_pty_terminal_status_queries(flushed_text, &writer);
                append_pty_output_and_update_screen(&shared_state, &mut screen_state, flushed_text);
                break;
            }
            Ok(read_bytes) => {
                let text = output_decoder.push_bytes(&buffer[..read_bytes]);
                let text = answer_pty_terminal_status_queries(text, &writer);
                append_pty_output_and_update_screen(&shared_state, &mut screen_state, text);
            }
            Err(error) => {
                let flushed_text = output_decoder.finish();
                let flushed_text = answer_pty_terminal_status_queries(flushed_text, &writer);
                append_pty_output_and_update_screen(&shared_state, &mut screen_state, flushed_text);
                shared_state.append_output_chunk_blocking(
                    CommandSessionOutputStream::Stderr,
                    format!("[host] Failed to read PTY command session: {error}\n"),
                );
                break;
            }
        }
    }
}

/// Answers minimal ANSI terminal status queries required by ConPTY-hosted console programs.
/// 应答 ConPTY 承载的控制台程序所需的最小 ANSI 终端状态查询。
pub(super) fn answer_pty_terminal_status_queries(
    text: String,
    writer: &Arc<StdMutex<Box<dyn Write + Send>>>,
) -> String {
    if !text.contains("\x1b[6n") {
        return text;
    }
    if let Ok(mut writer) = writer.lock() {
        let _ = writer.write_all(b"\x1b[1;1R");
        let _ = writer.flush();
    }
    text.replace("\x1b[6n", "")
}

pub(super) async fn run_pty_command_session_supervisor(
    record: Arc<CommandSessionRecord>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    mut control_receiver: UnboundedReceiver<CommandSessionControlMessage>,
    initial_stdin: Option<String>,
    initial_stdin_append_newline: bool,
) {
    let writer = Arc::new(StdMutex::new(writer));
    let killer = Arc::new(StdMutex::new(child.clone_killer()));
    let reader_state = Arc::clone(&record.shared_state);
    let reader_writer = Arc::clone(&writer);
    std::thread::spawn(move || {
        pump_pty_command_session_output(reader, reader_writer, reader_state)
    });

    let mut wait_task = tokio::task::spawn_blocking(move || {
        let wait_result = child.wait();
        match wait_result {
            Ok(status) => (Some(status.exit_code() as i32), None, status.success()),
            Err(error) => (
                None,
                Some(format!(
                    "Failed to wait for the PTY command session to exit: {error}"
                )),
                false,
            ),
        }
    });

    let mut stop_requested = false;
    if let Some(initial_stdin) = initial_stdin {
        let initial_stdin = if initial_stdin_append_newline {
            format!("{initial_stdin}{}", record.interactive_newline.as_str())
        } else {
            initial_stdin
        };
        match write_pty_command_session_stdin(
            Arc::clone(&writer),
            initial_stdin.clone().into_bytes(),
        )
        .await
        {
            Ok(_) => {
                record
                    .shared_state
                    .set_pending_stdin_line(stdin_payload_leaves_pending_line(&initial_stdin))
                    .await;
            }
            Err(error) => {
                record.shared_state.remember_failure_reason(error).await;
                let _ = request_pty_command_session_child_stop(Arc::clone(&killer)).await;
                record.shared_state.mark_stop_requested().await;
                stop_requested = true;
            }
        }
    }

    let (exit_code, wait_error, process_succeeded) = loop {
        tokio::select! {
            wait_result = &mut wait_task => {
                match wait_result {
                    Ok(result) => break result,
                    Err(error) => {
                        break (
                            None,
                            Some(format!("The PTY command-session wait task failed: {error}")),
                            false,
                        );
                    }
                }
            }
            maybe_message = control_receiver.recv() => {
                match maybe_message {
                    Some(CommandSessionControlMessage::Write { bytes, responder }) => {
                        let write_result = match write_pty_command_session_stdin(Arc::clone(&writer), bytes.clone()).await {
                            Ok(bytes_written) => {
                                record
                                    .shared_state
                                    .set_pending_stdin_line(stdin_payload_bytes_leave_pending_line(&bytes))
                                    .await;
                                Ok(bytes_written)
                            }
                            Err(error) => Err(error),
                        };
                        let _ = responder.send(write_result);
                    }
                    Some(CommandSessionControlMessage::InputEvents { responder, .. }) => {
                        let _ = responder.send(Err(String::from(
                            "Structured terminal input events are only supported by the Windows native console backend.",
                        )));
                    }
                    Some(CommandSessionControlMessage::Resize { responder, .. }) => {
                        let _ = responder.send(Err(String::from(
                            "Terminal resize is not yet supported by the current PTY runtime.",
                        )));
                    }
                    Some(CommandSessionControlMessage::Stop { responder }) => {
                        stop_requested = true;
                        let stop_result = request_pty_command_session_child_stop(Arc::clone(&killer)).await;
                        if stop_result.is_ok() {
                            record.shared_state.mark_stop_requested().await;
                        }
                        let _ = responder.send(stop_result);
                    }
                    None => {
                        stop_requested = true;
                        let _ = request_pty_command_session_child_stop(Arc::clone(&killer)).await;
                        record.shared_state.mark_stop_requested().await;
                    }
                }
            }
        }
    };

    record
        .shared_state
        .finalize_process_exit(stop_requested, exit_code, wait_error, process_succeeded)
        .await;
}
