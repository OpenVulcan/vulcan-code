use super::*;
use crate::process_creation::new_background_std_command;

pub(super) async fn write_command_session_stdin(
    child_stdin: &mut ChildStdin,
    bytes: &[u8],
) -> Result<usize, String> {
    match timeout(
        Duration::from_millis(COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS),
        child_stdin.write_all(bytes),
    )
    .await
    {
        Ok(Ok(())) => Ok(bytes.len()),
        Ok(Err(error)) => Err(format!("Failed to write command-session stdin: {error}")),
        Err(_) => Err(format!(
            "Timed out writing command-session stdin after {COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS}ms; Windows console-input TUI programs such as msvcrt.getch() cannot consume pipe stdin."
        )),
    }
}

/// Requests child-process termination, using a Windows process-tree kill when available before falling back to Tokio's child kill primitive.
/// 请求终止子进程；在 Windows 上优先杀进程树，然后回退到 Tokio 子进程终止原语。
pub(super) fn request_command_session_child_stop(child: &mut Child) -> Result<(), String> {
    #[cfg(windows)]
    {
        if let Some(process_id) = child.id() {
            let mut taskkill = new_background_std_command("taskkill");
            taskkill.args(["/PID", &process_id.to_string(), "/T", "/F"]);
            let taskkill_status = taskkill.status();
            if let Ok(status) = taskkill_status
                && status.success()
            {
                return Ok(());
            }
        }
    }
    child
        .start_kill()
        .map_err(|error| format!("Failed to stop the command-session process: {error}"))
}

pub(super) async fn pump_command_session_output<R>(
    mut reader: R,
    stream: CommandSessionOutputStream,
    shared_state: Arc<CommandSessionSharedState>,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    // Incremental shell-output decoder that preserves split multibyte characters across read chunks.
    // 在多次读取分片之间保留拆分多字节字符的增量 shell 输出解码器。
    let mut output_decoder = ShellOutputStreamDecoder::new();
    let mut buffer = vec![0_u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                let flushed_text = output_decoder.finish();
                shared_state.append_output_chunk(stream, flushed_text).await;
                break;
            }
            Ok(read_bytes) => {
                let text = output_decoder.push_bytes(&buffer[..read_bytes]);
                shared_state.append_output_chunk(stream, text).await;
            }
            Err(error) => {
                let flushed_text = output_decoder.finish();
                shared_state.append_output_chunk(stream, flushed_text).await;
                shared_state
                    .append_output_chunk(
                        CommandSessionOutputStream::Stderr,
                        format!(
                            "[host] Failed to read command session {}: {error}\n",
                            stream.as_str()
                        ),
                    )
                    .await;
                break;
            }
        }
    }
}

/// Runs the background process supervisor that owns the child process lifecycle and stdin control channel.
/// 运行后台进程监管器，持有子进程生命周期与标准输入控制通道。
pub(super) struct CommandSessionSupervisorInput {
    /// Runtime record that owns status, output buffers, and control metadata.
    /// 持有状态、输出缓冲与控制元数据的运行时记录。
    pub record: Arc<CommandSessionRecord>,
    /// Spawned child process owned by the supervisor.
    /// 由监管器持有的已启动子进程。
    pub child: Child,
    /// Optional writable stdin channel captured from the child process.
    /// 从子进程捕获的可选标准输入写入通道。
    pub stdin: Option<ChildStdin>,
    /// Captured stdout reader.
    /// 已捕获的标准输出读取器。
    pub stdout: ChildStdout,
    /// Captured stderr reader.
    /// 已捕获的标准错误读取器。
    pub stderr: ChildStderr,
    /// Control channel used by runtime API calls to write or stop the session.
    /// 运行时 API 写入或停止会话时使用的控制通道。
    pub control_receiver: Receiver<CommandSessionControlMessage>,
    /// Optional initial stdin payload submitted immediately after start.
    /// 启动后立即提交的可选初始标准输入内容。
    pub initial_stdin: Option<String>,
    /// Whether the supervisor appends the interactive newline to initial stdin.
    /// 监管器是否为初始标准输入追加交互式换行。
    pub initial_stdin_append_newline: bool,
}

/// Runs the background process supervisor that owns the child process lifecycle and stdin control channel.
/// 运行后台进程监管器，持有子进程生命周期与标准输入控制通道。
pub(super) async fn run_command_session_supervisor(input: CommandSessionSupervisorInput) {
    let CommandSessionSupervisorInput {
        record,
        mut child,
        mut stdin,
        stdout,
        stderr,
        mut control_receiver,
        initial_stdin,
        initial_stdin_append_newline,
    } = input;
    let stdout_task = tokio::spawn(pump_command_session_output(
        stdout,
        CommandSessionOutputStream::Stdout,
        Arc::clone(&record.shared_state),
    ));
    let stderr_task = tokio::spawn(pump_command_session_output(
        stderr,
        CommandSessionOutputStream::Stderr,
        Arc::clone(&record.shared_state),
    ));

    let mut stop_requested = false;
    // stdin_writable separates an uncertain or cancelled pipe write from process lifecycle ownership.
    // stdin_writable 将不确定或已取消的管道写入与进程生命周期所有权分离。
    let mut stdin_writable = stdin.is_some();
    if let Some(initial_stdin) = initial_stdin {
        let initial_stdin = if initial_stdin_append_newline {
            format!("{initial_stdin}{}", record.interactive_newline.as_str())
        } else {
            initial_stdin
        };
        match stdin.as_mut() {
            Some(child_stdin) => {
                if let Err(error) =
                    write_command_session_stdin(child_stdin, initial_stdin.as_bytes()).await
                {
                    record
                        .shared_state
                        .remember_failure_reason(format!(
                            "Failed to write the initial command-session stdin: {error}"
                        ))
                        .await;
                    let _ = request_command_session_child_stop(&mut child);
                    record.shared_state.mark_stop_requested().await;
                    stop_requested = true;
                } else {
                    record
                        .shared_state
                        .set_pending_stdin_line(stdin_payload_leaves_pending_line(&initial_stdin))
                        .await;
                }
            }
            None => {
                record
                    .shared_state
                    .remember_failure_reason(String::from(
                        "The command session did not expose a writable stdin channel.",
                    ))
                    .await;
            }
        }
    }

    let mut control_channel_open = true;
    let (exit_code, wait_error, process_succeeded) = loop {
        if !control_channel_open {
            match child.wait().await {
                Ok(status) => break (status.code(), None, status.success()),
                Err(error) => {
                    break (
                        None,
                        Some(format!(
                            "Failed to wait for the command session to exit: {error}"
                        )),
                        false,
                    );
                }
            }
        }

        tokio::select! {
            wait_result = child.wait() => {
                match wait_result {
                    Ok(status) => break (status.code(), None, status.success()),
                    Err(error) => {
                        break (
                            None,
                            Some(format!("Failed to wait for the command session to exit: {error}")),
                            false,
                        );
                    }
                }
            }
            maybe_message = control_receiver.recv() => {
                match maybe_message {
                    Some(CommandSessionControlMessage::Write { bytes, responder }) => {
                        let write_result = match (stdin_writable, stdin.as_mut()) {
                            (true, Some(child_stdin)) => {
                                match write_command_session_stdin(child_stdin, &bytes).await {
                                    Ok(bytes_written) => {
                                        record
                                            .shared_state
                                            .set_pending_stdin_line(stdin_payload_bytes_leave_pending_line(&bytes))
                                            .await;
                                        Ok(bytes_written)
                                    }
                                    Err(error) => {
                                        // Cancellation can leave a partial pipe write, so reject later writes without
                                        // closing the handle or terminating a process that is still alive.
                                        // 取消操作可能留下部分管道写入，因此拒绝后续写入，但不关闭句柄或终止仍存活的进程。
                                        stdin_writable = false;
                                        record.shared_state.remember_failure_reason(format!(
                                            "Command-session stdin was disabled after a failed write: {error}"
                                        )).await;
                                        Err(error)
                                    }
                                }
                            }
                            _ => Err(String::from(
                                "The command session does not currently have a writable stdin channel.",
                            )),
                        };
                        let _ = responder.send(write_result);
                    }
                    Some(CommandSessionControlMessage::Resize { responder, .. }) => {
                        let _ = responder.send(Err(String::from(
                            "Terminal resize is not supported by pipe-backed command sessions.",
                        )));
                    }
                    Some(CommandSessionControlMessage::Stop { responder }) => {
                        stop_requested = true;
                        stdin_writable = false;
                        stdin = None;
                        let stop_result = request_command_session_child_stop(&mut child);
                        if stop_result.is_ok() {
                            record.shared_state.mark_stop_requested().await;
                        }
                        let _ = responder.send(stop_result);
                    }
                    None => {
                        control_channel_open = false;
                    }
                }
            }
        }
    };

    let _ = stdout_task.await;
    let _ = stderr_task.await;
    record
        .shared_state
        .finalize_process_exit(stop_requested, exit_code, wait_error, process_succeeded)
        .await;
    record.shared_state.change_notify.notify_waiters();
}
