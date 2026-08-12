use super::*;
use crate::terminal_host_client::{TerminalHostConnection, wait_for_terminal_host_stop_deadline};
use crate::terminal_host_protocol::{
    TerminalHostErrorEvent, TerminalHostExitedEvent, TerminalHostFrameKind,
    decode_terminal_host_json, validate_terminal_host_sequence,
};

/// Headless VT screen checkpoint synthesized from decoded PTY output.
/// 从解码后的 PTY 输出综合生成的无界面 VT 屏幕检查点。
pub(super) struct PtyScreenFrameState {
    /// Complete terminal parser used to preserve cursor, style, and alternate-screen state.
    /// 用于保留光标、样式与备用屏幕状态的完整终端解析器。
    parser: vt100::Parser,
    /// Current screen rows exposed through the snapshot contract.
    /// 通过快照合同暴露的当前屏幕行数。
    rows: u16,
    /// Current screen columns exposed through the snapshot contract.
    /// 通过快照合同暴露的当前屏幕列数。
    cols: u16,
    /// Monotonic frame sequence emitted when visible text changes.
    /// 可见文本变化时递增的帧序号。
    frame_seq: u64,
    /// Last emitted frame text used to suppress duplicate updates.
    /// 用于抑制重复更新的上一帧文本。
    last_frame_text: String,
}

impl PtyScreenFrameState {
    /// Creates one rolling legacy screen-frame state.
    /// 创建一份滚动的旧版屏幕帧状态。
    ///
    /// # Parameters / 参数
    /// - `rows`: Maximum retained screen rows.
    /// - `cols`: Reported screen columns.
    /// - `rows`：最大保留屏幕行数。
    /// - `cols`：报告的屏幕列数。
    ///
    /// # Returns / 返回值
    /// Returns an empty frame state with sequence zero.
    /// 返回序号为零的空帧状态。
    pub(super) fn new(rows: u16, cols: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows, cols, 0),
            rows,
            cols,
            frame_seq: 0,
            last_frame_text: String::new(),
        }
    }

    /// Pushes one decoded output fragment and returns a changed legacy screen frame.
    /// 推入一个已解码输出片段，并在变化时返回旧版屏幕帧。
    ///
    /// # Parameters / 参数
    /// - `text`: Decoded terminal output fragment.
    /// - `text`：已解码的终端输出片段。
    ///
    /// # Returns / 返回值
    /// Returns frame sequence, dimensions, and text only when visible content changed.
    /// 仅在可见内容变化时返回帧序号、尺寸和文本。
    pub(super) fn push_output(&mut self, text: &str) -> Option<(u64, u16, u16, String)> {
        if text.is_empty() {
            return None;
        }
        self.parser.process(text.as_bytes());
        if self.parser.screen().contents().trim().is_empty() {
            return None;
        }
        let frame_text =
            String::from_utf8_lossy(&self.parser.screen().state_formatted()).into_owned();
        if frame_text == self.last_frame_text {
            return None;
        }
        self.frame_seq = self.frame_seq.saturating_add(1);
        self.last_frame_text = frame_text.clone();
        Some((self.frame_seq, self.rows, self.cols, frame_text))
    }

    /// Resizes the headless VT checkpoint after the isolated PTY accepts a new grid.
    /// 隔离 PTY 接受新字符网格后调整无界面 VT 检查点尺寸。
    ///
    /// # Parameters / 参数
    /// - `rows`: Accepted positive terminal row count.
    /// - `cols`: Accepted positive terminal column count.
    /// - `rows`：已接受的大于零终端行数。
    /// - `cols`：已接受的大于零终端列数。
    fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
        self.rows = rows;
        self.cols = cols;
    }
}

/// Appends decoded PTY output and mirrors it into the legacy screen snapshot.
/// 追加解码后的 PTY 输出，并将其镜像到旧版屏幕快照。
///
/// The raw decoded chunk remains the authoritative AI output stream. The formatted VT checkpoint
/// initializes visible read-only xterm views without changing the AI cursor contract.
/// 原始解码分片仍是 AI 输出流的权威来源；格式化 VT 检查点用于初始化可见只读 xterm，且不改变 AI 游标合同。
///
/// # Parameters / 参数
/// - `shared_state`: Session state receiving output and frame updates.
/// - `screen_state`: Rolling legacy screen-frame accumulator.
/// - `text`: Decoded output fragment.
/// - `shared_state`：接收输出与帧更新的会话状态。
/// - `screen_state`：滚动的旧版屏幕帧累加器。
/// - `text`：已解码输出片段。
async fn append_pty_output_and_update_screen(
    shared_state: &Arc<CommandSessionSharedState>,
    screen_state: &mut PtyScreenFrameState,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    let screen_frame = screen_state.push_output(&text);
    shared_state
        .append_output_chunk_with_screen_frame(
            CommandSessionOutputStream::Stdout,
            text,
            screen_frame,
        )
        .await;
}

/// Contains all ownership transferred to one command-session terminal-host supervisor.
/// 包含移交给一个命令会话终端宿主监控器的全部所有权。
pub(super) struct PtyCommandSessionSupervisorInput {
    /// Runtime record that owns status, output buffers, and control metadata.
    /// 持有状态、输出缓冲与控制元数据的运行时记录。
    pub record: Arc<CommandSessionRecord>,
    /// Isolated terminal-host process and its framed protocol pipes.
    /// 隔离的终端宿主进程及其分帧协议管道。
    pub connection: TerminalHostConnection,
    /// Receives runtime write, resize, and stop operations.
    /// 接收运行时写入、调整尺寸与停止操作。
    pub control_receiver: Receiver<CommandSessionControlMessage>,
    /// Optional initial stdin payload submitted immediately after start.
    /// 启动后立即提交的可选初始标准输入内容。
    pub initial_stdin: Option<String>,
    /// Indicates whether the interactive newline is appended to initial stdin.
    /// 指示是否为初始标准输入追加交互换行。
    pub initial_stdin_append_newline: bool,
}

/// Runs the supervisor that bridges one session record to one isolated terminal host.
/// 运行将一个会话记录连接到一个隔离终端宿主的监控器。
///
/// # Parameters / 参数
/// - `input`: Complete terminal-host and session ownership bundle.
/// - `input`：完整的终端宿主与会话所有权集合。
pub(super) async fn run_pty_command_session_supervisor(input: PtyCommandSessionSupervisorInput) {
    let PtyCommandSessionSupervisorInput {
        record,
        connection,
        mut control_receiver,
        initial_stdin,
        initial_stdin_append_newline,
    } = input;
    let TerminalHostConnection {
        mut child,
        mut request_writer,
        mut event_receiver,
        mut stderr,
        terminal_process_id: _,
    } = connection;

    let stderr_state = Arc::clone(&record.shared_state);
    let stderr_task = tokio::spawn(async move {
        let mut buffer = vec![0_u8; 4096];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) => break,
                Ok(bytes_read) => {
                    let diagnostic = String::from_utf8_lossy(&buffer[..bytes_read]);
                    stderr_state
                        .append_output_chunk(
                            CommandSessionOutputStream::Stderr,
                            format!("[terminal-host] {diagnostic}"),
                        )
                        .await;
                }
                Err(error) => {
                    stderr_state
                        .append_output_chunk(
                            CommandSessionOutputStream::Stderr,
                            format!("[terminal-host] Failed to read diagnostics: {error}\n"),
                        )
                        .await;
                    break;
                }
            }
        }
    });
    let mut wait_task = tokio::spawn(async move { child.wait().await });
    let mut output_decoder = ShellOutputStreamDecoder::new();
    let mut screen_state =
        PtyScreenFrameState::new(DEFAULT_PTY_SCREEN_ROWS, DEFAULT_PTY_SCREEN_COLS);
    let mut stdout_open = true;
    let mut terminal_exit: Option<TerminalHostExitedEvent> = None;
    let mut last_host_event_sequence = 1_u64;
    let mut stop_requested = false;
    let mut stop_requested_at: Option<Instant> = None;
    // host_wait_result retains host completion until every framed stdout event has been drained.
    // host_wait_result 保留宿主完成结果，直到所有 stdout 分帧事件均已排空。
    let mut host_wait_result: Option<std::io::Result<std::process::ExitStatus>> = None;

    if let Some(initial_stdin) = initial_stdin {
        let initial_stdin = if initial_stdin_append_newline {
            format!("{initial_stdin}{}", record.interactive_newline.as_str())
        } else {
            initial_stdin
        };
        match request_writer
            .write_input(initial_stdin.clone().into_bytes())
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
                stop_requested_at.get_or_insert_with(Instant::now);
                let _ = request_writer.stop().await;
                record.shared_state.mark_stop_requested().await;
                stop_requested = true;
            }
        }
    }

    let (exit_code, wait_error, process_succeeded) = loop {
        if !stdout_open && let Some(wait_result) = host_wait_result.take() {
            let flushed_text = output_decoder.finish();
            append_pty_output_and_update_screen(
                &record.shared_state,
                &mut screen_state,
                flushed_text,
            )
            .await;
            match wait_result {
                Ok(status) => {
                    if let Some(exit) = terminal_exit {
                        break (
                            exit.exit_code.map(|code| code as i32),
                            None,
                            exit.exit_code == Some(0),
                        );
                    }
                    break (
                        status.code(),
                        Some(String::from(
                            "The terminal host exited without reporting the terminal child exit state.",
                        )),
                        false,
                    );
                }
                Err(error) => {
                    break (
                        None,
                        Some(format!(
                            "Failed to wait for the terminal host process: {error}"
                        )),
                        false,
                    );
                }
            }
        }
        tokio::select! {
            frame_result = event_receiver.recv(), if stdout_open => {
                match frame_result {
                    Some(Ok(frame)) => if let Err(error) = validate_terminal_host_sequence(
                        &mut last_host_event_sequence,
                        frame.sequence,
                    ) {
                        stdout_open = false;
                        record.shared_state.remember_failure_reason(format!(
                            "Terminal-host event sequence validation failed: {error}"
                        )).await;
                        wait_task.abort();
                        let _ = (&mut wait_task).await;
                        host_wait_result = Some(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Terminal-host event sequence validation failed: {error}"),
                        )));
                        stop_requested = true;
                    } else {
                        match frame.kind {
                        TerminalHostFrameKind::OutputBytes => {
                            let text = output_decoder.push_bytes(&frame.payload);
                            append_pty_output_and_update_screen(
                                &record.shared_state,
                                &mut screen_state,
                                text,
                            )
                            .await;
                        }
                        TerminalHostFrameKind::Exited => {
                            match decode_terminal_host_json::<TerminalHostExitedEvent>(&frame.payload) {
                                Ok(exit) => {
                                    stop_requested = stop_requested || exit.stopped_by_request;
                                    terminal_exit = Some(exit);
                                }
                                Err(error) => {
                                    record.shared_state.remember_failure_reason(format!(
                                        "Failed to decode terminal-host exit event: {error}"
                                    )).await;
                                }
                            }
                        }
                        TerminalHostFrameKind::Error => {
                            match decode_terminal_host_json::<TerminalHostErrorEvent>(&frame.payload) {
                                Ok(error) => {
                                    record.shared_state.remember_failure_reason(format!(
                                        "Terminal host failed [{}]: {}",
                                        error.code, error.message
                                    )).await;
                                }
                                Err(error) => {
                                    record.shared_state.remember_failure_reason(format!(
                                        "Failed to decode terminal-host error event: {error}"
                                    )).await;
                                }
                            }
                        }
                        TerminalHostFrameKind::StateChanged => {}
                        unexpected => {
                            record.shared_state.remember_failure_reason(format!(
                                "Terminal host emitted an unexpected event: {unexpected:?}."
                            )).await;
                        }
                        }
                    },
                    None => stdout_open = false,
                    Some(Err(error)) => {
                        stdout_open = false;
                        record.shared_state.remember_failure_reason(format!(
                            "Failed to read terminal-host event: {error}"
                        )).await;
                    }
                }
            }
            wait_result = &mut wait_task, if host_wait_result.is_none() => {
                // The host writes Exited before process termination; defer finalization until pipe EOF drains it.
                // 宿主会在进程终止前写入 Exited；因此延迟结束，直到管道 EOF 将其排空。
                host_wait_result = Some(match wait_result {
                    Ok(result) => result,
                    Err(error) => Err(std::io::Error::other(format!(
                        "Terminal-host wait task failed: {error}"
                    ))),
                });
            }
            maybe_message = control_receiver.recv() => {
                match maybe_message {
                    Some(CommandSessionControlMessage::Write { bytes, responder }) => {
                        let write_result = match request_writer.write_input(bytes.clone()).await {
                            Ok(bytes_written) => {
                                record.shared_state.set_pending_stdin_line(
                                    stdin_payload_bytes_leave_pending_line(&bytes)
                                ).await;
                                Ok(bytes_written)
                            }
                            Err(error) => Err(error),
                        };
                        let _ = responder.send(write_result);
                    }
                    Some(CommandSessionControlMessage::Resize { rows, cols, responder }) => {
                        let result = request_writer.resize(cols, rows).await;
                        if result.is_ok() {
                            screen_state.resize(rows, cols);
                        }
                        let _ = responder.send(result);
                    }
                    Some(CommandSessionControlMessage::Stop { responder }) => {
                        stop_requested = true;
                        stop_requested_at.get_or_insert_with(Instant::now);
                        let stop_result = request_writer.stop().await;
                        if stop_result.is_ok() {
                            record.shared_state.mark_stop_requested().await;
                        }
                        let _ = responder.send(stop_result);
                    }
                    None => {
                        stop_requested = true;
                        stop_requested_at.get_or_insert_with(Instant::now);
                        let _ = request_writer.stop().await;
                        record.shared_state.mark_stop_requested().await;
                    }
                }
            },
            _ = wait_for_terminal_host_stop_deadline(
                stop_requested_at,
                TERMINAL_HOST_STOP_GRACE,
            ), if host_wait_result.is_none() => {
                let message = String::from(
                    "Terminal host did not exit within the 2-second stop grace period.",
                );
                record.shared_state.remember_failure_reason(message.clone()).await;
                stdout_open = false;
                wait_task.abort();
                let _ = (&mut wait_task).await;
                host_wait_result = Some(Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    message,
                )));
                stop_requested = true;
            }
        }
    };

    record
        .shared_state
        .finalize_process_exit(stop_requested, exit_code, wait_error, process_succeeded)
        .await;
    let _ = stderr_task.await;
}
