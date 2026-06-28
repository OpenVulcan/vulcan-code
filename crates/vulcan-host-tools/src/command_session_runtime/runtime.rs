use super::*;

impl Default for CommandSessionRuntime {
    /// Builds one empty command-session runtime.
    /// 构建一份空的命令会话运行时。
    fn default() -> Self {
        Self::new()
    }
}

impl CommandSessionRuntime {
    /// Builds one empty command-session runtime.
    /// 构建一份空的命令会话运行时。
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Starts one host-managed command session and returns its initial snapshot.
    /// 启动一条宿主管理命令会话并返回其初始快照。
    pub async fn start_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        match request.backend {
            CommandSessionBackend::Pipe => self.start_pipe_session(request).await,
            CommandSessionBackend::Pty => self.start_pty_session(request).await,
            CommandSessionBackend::WindowsConsole => {
                self.start_windows_console_session(request).await
            }
        }
    }

    /// Starts one pipe-backed host-managed command session and returns its initial snapshot.
    /// 启动一条管道后端的宿主管理命令会话并返回其初始快照。
    async fn start_pipe_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let mut command = TokioCommand::new(&request.launcher_program);
        command.args(&request.launcher_args);
        command.current_dir(&request.cwd);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);

        let mut child = command.spawn().map_err(|error| {
            format!(
                "Failed to start the command session: program `{}` could not be executed: {error}",
                request.launcher_program
            )
        })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| {
            String::from("The command session started, but stdout could not be captured.")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            String::from("The command session started, but stderr could not be captured.")
        })?;

        let command_session_id = Uuid::new_v4();
        let shared_state = Arc::new(CommandSessionSharedState::default());
        let (control_sender, control_receiver) = unbounded_channel();
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            owner_session_id: request.owner_session_id,
            display_name: request.display_name,
            command: request.command,
            shell_label: request.shell_label,
            launcher_shell_label: request.launcher_shell_label,
            interactive_newline: request.interactive_newline,
            cwd: request.cwd,
            backend: CommandSessionBackend::Pipe,
            control_sender,
            shared_state: Arc::clone(&shared_state),
        });

        self.sessions
            .write()
            .map_err(|_| {
                String::from(
                    "The command session registry is locked and cannot store a new session.",
                )
            })?
            .insert(command_session_id, Arc::clone(&record));

        tokio::spawn(run_command_session_supervisor(
            Arc::clone(&record),
            child,
            stdin,
            stdout,
            stderr,
            control_receiver,
            request.initial_stdin,
            request.initial_stdin_append_newline,
        ));

        Ok(record.snapshot().await)
    }

    /// Starts one PTY-backed host-managed command session and returns its initial snapshot.
    /// 启动一条 PTY 后端的宿主管理命令会话并返回其初始快照。
    async fn start_pty_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to create a pseudo terminal: {error}"))?;
        let mut command = PtyCommandBuilder::new(&request.launcher_program);
        command.args(&request.launcher_args);
        command.cwd(&request.cwd);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Failed to start the PTY command session: {error}"))?;
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Failed to capture PTY output: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Failed to open PTY input: {error}"))?;

        let command_session_id = Uuid::new_v4();
        let shared_state = Arc::new(CommandSessionSharedState::default());
        let (control_sender, control_receiver) = unbounded_channel();
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            owner_session_id: request.owner_session_id,
            display_name: request.display_name,
            command: request.command,
            shell_label: request.shell_label,
            launcher_shell_label: request.launcher_shell_label,
            interactive_newline: request.interactive_newline,
            cwd: request.cwd,
            backend: CommandSessionBackend::Pty,
            control_sender,
            shared_state: Arc::clone(&shared_state),
        });

        self.sessions
            .write()
            .map_err(|_| {
                String::from(
                    "The command session registry is locked and cannot store a new session.",
                )
            })?
            .insert(command_session_id, Arc::clone(&record));

        tokio::spawn(run_pty_command_session_supervisor(
            Arc::clone(&record),
            pair.master,
            child,
            reader,
            writer,
            control_receiver,
            request.initial_stdin,
            request.initial_stdin_append_newline,
        ));

        Ok(record.snapshot().await)
    }

    /// Starts one Windows native console helper-backed command session and returns its initial snapshot.
    /// 启动一条 Windows 原生控制台 helper 后端的命令会话并返回其初始快照。
    async fn start_windows_console_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        #[cfg(not(windows))]
        {
            let _ = request;
            Err(String::from(
                "The Windows native console backend is only available on Windows.",
            ))
        }
        #[cfg(windows)]
        {
            self.start_windows_console_session_inner(request).await
        }
    }

    /// Starts one Windows helper-backed command session on Windows hosts.
    /// 在 Windows 宿主上启动一条 helper 后端命令会话。
    #[cfg(windows)]
    async fn start_windows_console_session_inner(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let helper_path = resolve_windows_console_helper_path()?;
        let mut helper = TokioCommand::new(&helper_path);
        helper.creation_flags(WINDOWS_CREATE_NO_WINDOW);
        helper.stdin(Stdio::piped());
        helper.stdout(Stdio::piped());
        helper.stderr(Stdio::piped());
        helper.kill_on_drop(true);
        let mut child = helper.spawn().map_err(|error| {
            format!(
                "Failed to start Windows console helper `{}`: {error}",
                helper_path.display()
            )
        })?;
        let mut helper_stdin = child.stdin.take().ok_or_else(|| {
            String::from("The Windows console helper started without a writable stdin pipe.")
        })?;
        let helper_stdout = child.stdout.take().ok_or_else(|| {
            String::from("The Windows console helper started without a readable stdout pipe.")
        })?;
        let helper_stderr = child.stderr.take().ok_or_else(|| {
            String::from("The Windows console helper started without a readable stderr pipe.")
        })?;
        let start_request = WindowsConsoleHelperRequest::Start {
            program: request.launcher_program.clone(),
            args: request.launcher_args.clone(),
            cwd: request.cwd.display().to_string(),
            rows: 24,
            cols: 100,
            initial_stdin: request.initial_stdin.clone(),
            initial_stdin_append_newline: request.initial_stdin_append_newline,
            interactive_newline: request.interactive_newline.clone(),
        };
        write_windows_console_helper_request(&mut helper_stdin, &start_request).await?;

        let command_session_id = Uuid::new_v4();
        let shared_state = Arc::new(CommandSessionSharedState::default());
        let (control_sender, control_receiver) = unbounded_channel();
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            owner_session_id: request.owner_session_id,
            display_name: request.display_name,
            command: request.command,
            shell_label: request.shell_label,
            launcher_shell_label: request.launcher_shell_label,
            interactive_newline: request.interactive_newline,
            cwd: request.cwd,
            backend: CommandSessionBackend::WindowsConsole,
            control_sender,
            shared_state: Arc::clone(&shared_state),
        });

        self.sessions
            .write()
            .map_err(|_| {
                String::from(
                    "The command session registry is locked and cannot store a new session.",
                )
            })?
            .insert(command_session_id, Arc::clone(&record));

        tokio::spawn(run_windows_console_helper_supervisor(
            Arc::clone(&record),
            child,
            helper_stdin,
            helper_stdout,
            helper_stderr,
            control_receiver,
        ));

        Ok(record.snapshot().await)
    }

    /// Reads buffered output from one owned command session, optionally waiting for fresh output when needed.
    /// 从一条归属命令会话读取缓冲输出，并在需要时可选等待新输出到达。
    pub async fn read_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
    ) -> Result<CommandSessionReadOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let after_seq = after_seq.unwrap_or(0);
        let max_chunks = max_chunks
            .unwrap_or(DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS)
            .max(1);
        let wait_for_output_ms = wait_for_output_ms.unwrap_or(DEFAULT_COMMAND_SESSION_READ_WAIT_MS);
        let wait_started_at = Instant::now();

        loop {
            let notified = record.shared_state.change_notify.notified();
            let outcome = record.read_since(after_seq, max_chunks, true).await;
            if !outcome.returned_chunks.is_empty()
                || outcome.snapshot.status != CommandSessionStatus::Running
                || wait_for_output_ms == 0
            {
                return Ok(outcome);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_output_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(outcome);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified).await;
        }
    }

    /// Writes one text payload into one owned running command session.
    /// 向一条归属且仍在运行的命令会话写入一段文本载荷。
    pub async fn write_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        text: String,
        append_newline: bool,
    ) -> Result<CommandSessionWriteOutcome, String> {
        let text = if append_newline {
            let record = self.session_for_owner(owner_session_id, command_session_id)?;
            format!("{text}{}", record.interactive_newline.as_str())
        } else {
            text
        };
        self.write_session_bytes(owner_session_id, command_session_id, text.into_bytes())
            .await
    }

    /// Writes one byte payload into one owned running command session.
    /// 向一条归属且仍在运行的命令会话写入一段字节载荷。
    pub async fn write_session_bytes(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        bytes: Vec<u8>,
    ) -> Result<CommandSessionWriteOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let snapshot = record.snapshot().await;
        if snapshot.status != CommandSessionStatus::Running {
            return Err(format!(
                "Command session `{}` is already in `{}` state and cannot accept more stdin.",
                command_session_id,
                snapshot.status.as_str()
            ));
        }

        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::Write {
                bytes,
                responder: sender,
            })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot write more stdin.",
                )
            })?;
        let bytes_written =
            timeout(
                Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
                receiver,
            )
            .await
            .map_err(|_| {
                String::from(
                    "Timed out waiting for the command session supervisor to acknowledge stdin; the process may be a Windows console-input TUI that is incompatible with pipe stdin.",
                )
            })?
            .map_err(|_| {
                String::from("The command session write acknowledgement channel is closed.")
            })??;

        Ok(CommandSessionWriteOutcome {
            snapshot: record.snapshot().await,
            bytes_written,
        })
    }

    /// Sends structured terminal input events into one owned running command session.
    /// 向一条归属且仍在运行的命令会话发送结构化终端输入事件。
    pub async fn write_session_input_events(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        events: Vec<CommandSessionInputEvent>,
    ) -> Result<CommandSessionWriteOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let snapshot = record.snapshot().await;
        if snapshot.status != CommandSessionStatus::Running {
            return Err(format!(
                "Command session `{}` is already in `{}` state and cannot accept more terminal input.",
                command_session_id,
                snapshot.status.as_str()
            ));
        }
        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::InputEvents {
                events,
                responder: sender,
            })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot send terminal input.",
                )
            })?;
        let accepted_count =
            timeout(
                Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
                receiver,
            )
            .await
            .map_err(|_| {
                String::from(
                    "Timed out waiting for the command session supervisor to acknowledge terminal input.",
                )
            })?
            .map_err(|_| {
                String::from("The command session input acknowledgement channel is closed.")
            })??;
        Ok(CommandSessionWriteOutcome {
            snapshot: record.snapshot().await,
            bytes_written: accepted_count,
        })
    }

    /// Resizes one owned running terminal session when its backend supports screen dimensions.
    /// 在后端支持屏幕尺寸时调整一条归属运行中终端会话的大小。
    pub async fn resize_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        rows: u16,
        cols: u16,
    ) -> Result<CommandSessionSnapshot, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::Resize {
                rows,
                cols,
                responder: sender,
            })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot resize the terminal.",
                )
            })?;
        timeout(
            Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
            receiver,
        )
        .await
        .map_err(|_| {
            String::from(
                "Timed out waiting for the command session supervisor to acknowledge resize.",
            )
        })?
        .map_err(|_| {
            String::from("The command session resize acknowledgement channel is closed.")
        })??;
        Ok(record.snapshot().await)
    }

    /// Stops one owned command session and waits briefly for the process to reach a terminal state.
    /// 停止一条归属命令会话，并在短时间内等待进程到达终态。
    pub async fn stop_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        wait_for_exit_ms: Option<u64>,
    ) -> Result<CommandSessionSnapshot, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let existing_snapshot = record.snapshot().await;
        if existing_snapshot.status != CommandSessionStatus::Running {
            return Ok(existing_snapshot);
        }

        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::Stop { responder: sender })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot stop the process.",
                )
            })?;
        let stop_acknowledged = timeout(
            Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
            receiver,
        )
        .await;
        match stop_acknowledged {
            Ok(Ok(Ok(()))) => {
                record.shared_state.mark_stop_requested().await;
            }
            Ok(Ok(Err(error))) => return Err(error),
            Ok(Err(_)) | Err(_) => {
                record.shared_state.mark_stop_requested().await;
                return Ok(record.snapshot().await);
            }
        }

        let wait_for_exit_ms = wait_for_exit_ms.unwrap_or(DEFAULT_COMMAND_SESSION_STOP_WAIT_MS);
        let wait_started_at = Instant::now();
        loop {
            let notified = record.shared_state.change_notify.notified();
            let snapshot = record.snapshot().await;
            if snapshot.status != CommandSessionStatus::Running || wait_for_exit_ms == 0 {
                return Ok(snapshot);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_exit_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(snapshot);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified).await;
        }
    }

    /// Returns one best-effort live snapshot for the requested owned command session without blocking on async state, so WebUI status surfaces can prefer current runtime state over historical transcript payloads.
    /// 在不阻塞异步状态锁的前提下返回目标归属命令会话的一份尽力而为实时快照，使 WebUI 状态界面可以优先展示当前运行时状态而不是历史转录载荷。
    pub fn try_snapshot_owned_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
    ) -> Option<CommandSessionSnapshot> {
        let record = self
            .sessions
            .read()
            .ok()?
            .get(&command_session_id)
            .cloned()?;
        if record.owner_session_id != owner_session_id {
            return None;
        }
        Some(
            record
                .try_snapshot()
                .unwrap_or_else(|| record.fallback_running_snapshot()),
        )
    }

    /// Reads buffered output with optional AI-facing cursor advancement.
    /// 按需读取缓冲输出，并可选推进面向 AI 的读取游标。
    async fn read_session_for_ai_with_cursor_mode(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        read_from_start: bool,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
        advance_ai_cursor: bool,
    ) -> Result<CommandSessionReadOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let after_seq = if read_from_start {
            after_seq.unwrap_or(0)
        } else if let Some(after_seq) = after_seq {
            after_seq
        } else {
            record.ai_delivered_seq().await
        };
        let max_chunks = max_chunks
            .unwrap_or(DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS)
            .max(1);
        let wait_for_output_ms = wait_for_output_ms.unwrap_or(DEFAULT_COMMAND_SESSION_READ_WAIT_MS);
        let wait_started_at = Instant::now();

        loop {
            let notified = record.shared_state.change_notify.notified();
            let outcome = record
                .read_since(after_seq, max_chunks, advance_ai_cursor)
                .await;
            if !outcome.returned_chunks.is_empty()
                || outcome.snapshot.status != CommandSessionStatus::Running
                || wait_for_output_ms == 0
            {
                if advance_ai_cursor {
                    record.mark_ai_delivered(&outcome.returned_chunks).await;
                }
                return Ok(outcome);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_output_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(outcome);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified).await;
        }
    }

    /// Reads buffered output for terminal_session and advances the AI-facing cursor by default.
    /// 为 terminal_session 读取缓冲输出，并默认推进面向 AI 的读取游标。
    pub async fn read_session_for_ai(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        read_from_start: bool,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
    ) -> Result<CommandSessionReadOutcome, String> {
        self.read_session_for_ai_with_cursor_mode(
            owner_session_id,
            command_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
            true,
        )
        .await
    }

    /// Peeks buffered output for terminal_session without advancing the AI-facing cursor.
    /// 窥视 terminal_session 缓冲输出，但不推进面向 AI 的读取游标。
    pub async fn peek_session_for_ai(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        read_from_start: bool,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
    ) -> Result<CommandSessionReadOutcome, String> {
        self.read_session_for_ai_with_cursor_mode(
            owner_session_id,
            command_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
            false,
        )
        .await
    }

    /// Returns one best-effort live snapshot for the requested command session identifier without enforcing ownership, so read-only WebUI projections can still recover the latest visible status when the owner id must be inferred from historical metadata.
    /// 在不强制校验归属的前提下返回目标命令会话标识的一份尽力而为实时快照，使只读 WebUI 投影在只能从历史元数据推断 owner id 时仍可恢复最新可见状态。
    pub fn try_snapshot_session_by_id(
        &self,
        command_session_id: SessionId,
    ) -> Option<CommandSessionSnapshot> {
        let record = self
            .sessions
            .read()
            .ok()?
            .get(&command_session_id)
            .cloned()?;
        Some(
            record
                .try_snapshot()
                .unwrap_or_else(|| record.fallback_running_snapshot()),
        )
    }

    /// Returns best-effort live snapshots for all retained command sessions.
    /// 返回当前保留的所有命令会话的尽力而为实时快照。
    pub fn try_snapshot_sessions(&self) -> Vec<CommandSessionSnapshot> {
        self.sessions
            .read()
            .map(|sessions| {
                sessions
                    .values()
                    .map(|record| {
                        record
                            .try_snapshot()
                            .unwrap_or_else(|| record.fallback_running_snapshot())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    /// Resolves one command-session record and enforces that the caller owns it.
    /// 解析一条命令会话记录并强制校验调用方是否拥有它。
    fn session_for_owner(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
    ) -> Result<Arc<CommandSessionRecord>, String> {
        let record = self
            .sessions
            .read()
            .map_err(|_| {
                String::from("The command session registry is locked and cannot read the session.")
            })?
            .get(&command_session_id)
            .cloned()
            .ok_or_else(|| format!("Command session `{command_session_id}` was not found."))?;
        if record.owner_session_id != owner_session_id {
            return Err(format!(
                "Command session `{}` does not belong to the current controller session, so cross-session access is rejected.",
                command_session_id
            ));
        }
        Ok(record)
    }
}
