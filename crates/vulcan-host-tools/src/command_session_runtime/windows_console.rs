use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum WindowsConsoleHelperRequest {
    /// Starts the target console program inside the helper.
    /// 在 helper 内启动目标控制台程序。
    Start {
        /// Program executable path or name.
        /// 程序可执行文件路径或名称。
        program: String,
        /// Program argument list.
        /// 程序参数列表。
        args: Vec<String>,
        /// Working directory for the target process.
        /// 目标进程的工作目录。
        cwd: String,
        /// Initial screen row count.
        /// 初始屏幕行数。
        rows: u16,
        /// Initial screen column count.
        /// 初始屏幕列数。
        cols: u16,
        /// Optional initial stdin text.
        /// 可选初始标准输入文本。
        initial_stdin: Option<String>,
        /// Whether the helper should append the interactive newline to initial stdin.
        /// helper 是否应向初始标准输入追加交互换行。
        initial_stdin_append_newline: bool,
        /// Interactive newline used by the selected shell.
        /// 所选 shell 使用的交互换行。
        interactive_newline: String,
    },
    /// Sends ordered terminal input events.
    /// 发送保序终端输入事件。
    Input {
        /// Ordered input events.
        /// 保序输入事件。
        events: Vec<CommandSessionInputEvent>,
    },
    /// Writes raw text as terminal input.
    /// 将原始文本作为终端输入写入。
    Text {
        /// Text payload.
        /// 文本载荷。
        text: String,
    },
    /// Resizes the helper terminal.
    /// 调整 helper 终端大小。
    Resize {
        /// Desired row count.
        /// 期望行数。
        rows: u16,
        /// Desired column count.
        /// 期望列数。
        cols: u16,
    },
    /// Stops the helper and target process.
    /// 停止 helper 与目标进程。
    Stop,
}

/// One event received from the Windows console helper.
/// 从 Windows 控制台 helper 接收的一条事件。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum WindowsConsoleHelperEvent {
    /// Target process started successfully.
    /// 目标进程已成功启动。
    Started {
        /// Target process identifier.
        /// 目标进程标识。
        pid: u32,
    },
    /// Text output emitted by the helper.
    /// helper 发出的文本输出。
    Output {
        /// Output stream label.
        /// 输出流标签。
        stream: String,
        /// Output text payload.
        /// 输出文本载荷。
        text: String,
    },
    /// Current terminal screen frame.
    /// 当前终端屏幕帧。
    Frame {
        /// Stable frame sequence.
        /// 稳定帧序号。
        seq: u64,
        /// Frame row count.
        /// 帧行数。
        rows: u16,
        /// Frame column count.
        /// 帧列数。
        cols: u16,
        /// Renderable screen text with optional ANSI SGR styling.
        /// 可渲染屏幕文本，可包含 ANSI SGR 样式。
        text: String,
    },
    /// Target process reached a terminal status.
    /// 目标进程到达终态。
    Exited {
        /// Optional target exit code.
        /// 可选目标退出码。
        exit_code: Option<i32>,
        /// Whether the helper stopped the target by request.
        /// helper 是否按请求停止了目标。
        stopped: bool,
    },
    /// Helper reported an error.
    /// helper 报告错误。
    Error {
        /// Error message.
        /// 错误消息。
        message: String,
    },
}

/// Resolves the Windows console helper executable path located beside the current process.
/// 解析位于当前进程旁边的 Windows 控制台 helper 可执行文件路径。
#[cfg(windows)]
pub(super) fn resolve_windows_console_helper_path() -> Result<PathBuf, String> {
    if let Ok(explicit_path) = env::var("VULCAN_WINDOWS_CONSOLE_HELPER") {
        let explicit_path = PathBuf::from(explicit_path);
        if explicit_path.is_file() {
            return Ok(explicit_path);
        }
        return Err(format!(
            "VULCAN_WINDOWS_CONSOLE_HELPER points to `{}`, but that file does not exist.",
            explicit_path.display()
        ));
    }
    let current_exe = env::current_exe()
        .map_err(|error| format!("Failed to resolve the current executable path: {error}"))?;
    let helper_name = "vulcan-windows-console-helper.exe";
    let beside_current = current_exe.with_file_name(helper_name);
    if beside_current.is_file() {
        return Ok(beside_current);
    }
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
        .join(helper_name);
    if manifest_path.is_file() {
        return Ok(manifest_path);
    }
    Err(format!(
        "Windows console helper `{helper_name}` was not found beside `{}` or at `{}`. Build it with `cargo build -p vulcan-windows-console-helper`.",
        current_exe.display(),
        manifest_path.display()
    ))
}

/// Writes one JSONL request to the Windows console helper stdin pipe.
/// 向 Windows 控制台 helper 标准输入管道写入一条 JSONL 请求。
#[cfg(windows)]
pub(super) async fn write_windows_console_helper_request(
    stdin: &mut ChildStdin,
    request: &WindowsConsoleHelperRequest,
) -> Result<(), String> {
    let mut line = serde_json::to_vec(request)
        .map_err(|error| format!("Failed to serialize Windows console helper request: {error}"))?;
    line.push(b'\n');
    stdin
        .write_all(&line)
        .await
        .map_err(|error| format!("Failed to write Windows console helper request: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Failed to flush Windows console helper request: {error}"))
}

pub(super) fn helper_stream_label_to_output_stream(label: &str) -> CommandSessionOutputStream {
    if label.eq_ignore_ascii_case("stderr") {
        CommandSessionOutputStream::Stderr
    } else {
        CommandSessionOutputStream::Stdout
    }
}

/// Pumps Windows console helper stdout JSONL events into an internal channel.
/// 将 Windows 控制台 helper 的 stdout JSONL 事件泵入内部通道。
#[cfg(windows)]
pub(super) async fn pump_windows_console_helper_events(
    stdout: ChildStdout,
    event_sender: UnboundedSender<WindowsConsoleHelperEvent>,
    shared_state: Arc<CommandSessionSharedState>,
) {
    let mut lines = BufReader::new(stdout).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => match serde_json::from_str::<WindowsConsoleHelperEvent>(&line) {
                Ok(event) => {
                    let _ = event_sender.send(event);
                }
                Err(error) => {
                    shared_state
                        .append_output_chunk(
                            CommandSessionOutputStream::Stderr,
                            format!("[host] Failed to parse Windows console helper event: {error}: {line}\n"),
                        )
                        .await;
                }
            },
            Ok(None) => break,
            Err(error) => {
                shared_state
                    .append_output_chunk(
                        CommandSessionOutputStream::Stderr,
                        format!("[host] Failed to read Windows console helper stdout: {error}\n"),
                    )
                    .await;
                break;
            }
        }
    }
}

/// Runs the Windows console helper supervisor that owns helper lifecycle and JSONL control.
/// 运行 Windows 控制台 helper 监管器，持有 helper 生命周期与 JSONL 控制通道。
#[cfg(windows)]
pub(super) async fn run_windows_console_helper_supervisor(
    record: Arc<CommandSessionRecord>,
    mut child: Child,
    mut helper_stdin: ChildStdin,
    helper_stdout: ChildStdout,
    helper_stderr: ChildStderr,
    mut control_receiver: UnboundedReceiver<CommandSessionControlMessage>,
) {
    let (event_sender, mut event_receiver) = unbounded_channel();
    let stdout_state = Arc::clone(&record.shared_state);
    tokio::spawn(pump_windows_console_helper_events(
        helper_stdout,
        event_sender,
        stdout_state,
    ));
    tokio::spawn(pump_command_session_output(
        helper_stderr,
        CommandSessionOutputStream::Stderr,
        Arc::clone(&record.shared_state),
    ));

    let mut stop_requested = false;
    let mut target_exit_code = None;
    let mut target_succeeded = false;
    let mut wait_task = Box::pin(child.wait());

    loop {
        tokio::select! {
            wait_result = &mut wait_task => {
                match wait_result {
                    Ok(status) => {
                        if target_exit_code.is_none() {
                            target_exit_code = status.code();
                            target_succeeded = status.success();
                        }
                        break;
                    }
                    Err(error) => {
                        record.shared_state.remember_failure_reason(format!("Failed to wait for Windows console helper: {error}")).await;
                        break;
                    }
                }
            }
            maybe_event = event_receiver.recv() => {
                match maybe_event {
                    Some(WindowsConsoleHelperEvent::Started { pid }) => {
                        record.shared_state.append_output_chunk(
                            CommandSessionOutputStream::Stdout,
                            format!("[host] Windows console helper started target pid {pid}.\n"),
                        ).await;
                    }
                    Some(WindowsConsoleHelperEvent::Output { stream, text }) => {
                        record.shared_state.append_output_chunk(
                            helper_stream_label_to_output_stream(&stream),
                            text,
                        ).await;
                    }
                    Some(WindowsConsoleHelperEvent::Frame { seq, rows, cols, text }) => {
                        record.shared_state.update_screen_frame(seq, rows, cols, text.clone()).await;
                        record.shared_state.append_output_chunk(
                            CommandSessionOutputStream::Stdout,
                            format!("[screen #{seq}]\n{text}\n"),
                        ).await;
                    }
                    Some(WindowsConsoleHelperEvent::Exited { exit_code, stopped }) => {
                        stop_requested = stop_requested || stopped;
                        target_exit_code = exit_code;
                        target_succeeded = exit_code == Some(0);
                        break;
                    }
                    Some(WindowsConsoleHelperEvent::Error { message }) => {
                        record.shared_state.remember_failure_reason(message).await;
                    }
                    None => {
                        break;
                    }
                }
            }
            maybe_message = control_receiver.recv() => {
                match maybe_message {
                    Some(CommandSessionControlMessage::Write { bytes, responder }) => {
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let request = WindowsConsoleHelperRequest::Text { text };
                        let result = write_windows_console_helper_request(&mut helper_stdin, &request).await.map(|_| bytes.len());
                        if result.is_ok() {
                            record.shared_state.set_pending_stdin_line(stdin_payload_bytes_leave_pending_line(&bytes)).await;
                        }
                        let _ = responder.send(result);
                    }
                    Some(CommandSessionControlMessage::InputEvents { events, responder }) => {
                        let event_count = events.len();
                        let request = WindowsConsoleHelperRequest::Input { events };
                        let result = write_windows_console_helper_request(&mut helper_stdin, &request).await.map(|_| event_count);
                        if result.is_ok() {
                            record.shared_state.set_pending_stdin_line(false).await;
                        }
                        let _ = responder.send(result);
                    }
                    Some(CommandSessionControlMessage::Resize { rows, cols, responder }) => {
                        let request = WindowsConsoleHelperRequest::Resize { rows, cols };
                        let result = write_windows_console_helper_request(&mut helper_stdin, &request).await;
                        let _ = responder.send(result);
                    }
                    Some(CommandSessionControlMessage::Stop { responder }) => {
                        stop_requested = true;
                        let stop_result = write_windows_console_helper_request(&mut helper_stdin, &WindowsConsoleHelperRequest::Stop).await;
                        if stop_result.is_ok() {
                            record.shared_state.mark_stop_requested().await;
                        }
                        let _ = responder.send(stop_result);
                    }
                    None => {
                        stop_requested = true;
                        let _ = write_windows_console_helper_request(&mut helper_stdin, &WindowsConsoleHelperRequest::Stop).await;
                        record.shared_state.mark_stop_requested().await;
                    }
                }
            }
        }
    }

    record
        .shared_state
        .finalize_process_exit(stop_requested, target_exit_code, None, target_succeeded)
        .await;
    record.shared_state.change_notify.notify_waiters();
}
