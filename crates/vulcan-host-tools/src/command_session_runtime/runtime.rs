use super::*;

/// AI-facing command-session read request shared by read and peek paths.
/// 由读取与窥视路径共享的面向 AI 命令会话读取请求。
struct CommandSessionAiReadRequest {
    /// Session that owns the command session.
    /// 拥有该命令会话的父会话。
    owner_session_id: SessionId,
    /// Command session to read.
    /// 需要读取的命令会话。
    command_session_id: SessionId,
    /// Optional caller-provided output cursor.
    /// 调用方提供的可选输出游标。
    after_seq: Option<u64>,
    /// Whether reading starts from the beginning when no cursor is provided.
    /// 未提供游标时是否从开头读取。
    read_from_start: bool,
    /// Optional wait duration for new output.
    /// 等待新输出的可选时长。
    wait_for_output_ms: Option<u64>,
    /// Optional maximum number of chunks to return.
    /// 返回输出分片数量上限的可选值。
    max_chunks: Option<usize>,
    /// Whether the AI delivery cursor should advance after returning chunks.
    /// 返回分片后是否推进面向 AI 的交付游标。
    advance_ai_cursor: bool,
}

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
            next_admission_seq: AtomicU64::new(0),
        }
    }

    /// Starts one host-managed command session and returns its initial snapshot.
    /// 启动一条宿主管理命令会话并返回其初始快照。
    pub async fn start_session(
        self: &Arc<Self>,
        mut request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        // cwd crosses both the child-process and snapshot boundaries, so it must use one standard native spelling.
        // cwd 同时跨越子进程与快照边界，因此必须统一使用标准原生写法。
        request.cwd = vulcan_runtime_assets::native_host_path(&request.cwd);
        // launcher_program crosses the same process boundary and may originate from a canonical PATH probe.
        // launcher_program 跨越同一进程边界，并且可能来自规范化后的 PATH 探测。
        request.launcher_program =
            vulcan_runtime_assets::native_host_path_text(&request.launcher_program);
        match request.backend {
            CommandSessionBackend::Pipe => self.start_pipe_session(request).await,
            CommandSessionBackend::Pty => self.start_pty_session(request).await,
        }
    }

    /// Starts one pipe-backed host-managed command session and returns its initial snapshot.
    /// 启动一条管道后端的宿主管理命令会话并返回其初始快照。
    async fn start_pipe_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let mut command = new_background_tokio_command(&request.launcher_program);
        command.args(&request.launcher_args);
        command.current_dir(&request.cwd);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);
        // Stop requests must reach background descendants that inherited the session pipes.
        // 停止请求必须触达继承了会话管道的后台后代进程。
        crate::process_tree::isolate_child_process_tree(&mut command);
        // VULCAN_HOME keeps long-lived shell sessions aligned with the compiled user profile.
        // VULCAN_HOME 让长生命周期 shell 会话与当前编译用户配置保持一致。
        if std::env::var_os("VULCAN_HOME").is_none()
            && let Some(user_vulcan_root) = vulcan_runtime_assets::resolve_user_vulcan_root()
        {
            command.env(
                "VULCAN_HOME",
                vulcan_runtime_assets::native_host_path(&user_vulcan_root),
            );
        }
        apply_child_path_prefixes(&mut command, &request.path_prefixes)?;

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
        let (control_sender, control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            admission_seq: self.next_admission_seq.fetch_add(1, Ordering::Relaxed),
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

        self.register_session(&record)?;

        tokio::spawn(run_command_session_supervisor(
            CommandSessionSupervisorInput {
                record: Arc::clone(&record),
                child,
                stdin,
                stdout,
                stderr,
                control_receiver,
                initial_stdin: request.initial_stdin,
                initial_stdin_append_newline: request.initial_stdin_append_newline,
            },
        ));

        Ok(record.snapshot().await)
    }

    /// Starts one PTY-backed host-managed command session and returns its initial snapshot.
    /// 启动一条 PTY 后端的宿主管理命令会话并返回其初始快照。
    async fn start_pty_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let connection = crate::terminal_host_client::spawn_terminal_host(
            crate::terminal_host_protocol::TerminalHostStartRequest {
                program: request.launcher_program.clone(),
                args: request.launcher_args.clone(),
                cwd: request.cwd.clone(),
                cols: DEFAULT_PTY_SCREEN_COLS,
                rows: DEFAULT_PTY_SCREEN_ROWS,
            },
            &request.path_prefixes,
        )
        .await?;

        let command_session_id = Uuid::new_v4();
        let shared_state = Arc::new(CommandSessionSharedState::new_pty(
            DEFAULT_PTY_SCREEN_ROWS,
            DEFAULT_PTY_SCREEN_COLS,
        ));
        let (control_sender, control_receiver) = channel(COMMAND_SESSION_CONTROL_QUEUE_LIMIT);
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            admission_seq: self.next_admission_seq.fetch_add(1, Ordering::Relaxed),
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

        self.register_session(&record)?;

        tokio::spawn(run_pty_command_session_supervisor(
            PtyCommandSessionSupervisorInput {
                record: Arc::clone(&record),
                connection,
                control_receiver,
                initial_stdin: request.initial_stdin,
                initial_stdin_append_newline: request.initial_stdin_append_newline,
            },
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
        let max_chunks = bounded_command_session_read_chunks(max_chunks);
        let wait_for_output_ms = bounded_command_session_read_wait_ms(wait_for_output_ms);
        let wait_started_at = Instant::now();

        loop {
            let notified = record.shared_state.change_notify.notified();
            tokio::pin!(notified);
            // Register before reading output and lifecycle state because producers use notify_waiters.
            // 在读取输出与生命周期状态前完成注册，因为生产者使用 notify_waiters。
            notified.as_mut().enable();
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
            let _ = timeout(Duration::from_millis(remaining_ms), notified.as_mut()).await;
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
        send_command_session_control(
            &record.control_sender,
            CommandSessionControlMessage::Write {
                bytes,
                responder: sender,
            },
            "The command session supervisor has exited and cannot write more stdin.",
        )
        .await?;
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
        let bytes = encode_command_session_input_events(&events, &record.interactive_newline)?;
        self.write_session_bytes(owner_session_id, command_session_id, bytes)
            .await
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
        send_command_session_control(
            &record.control_sender,
            CommandSessionControlMessage::Resize {
                rows,
                cols,
                responder: sender,
            },
            "The command session supervisor has exited and cannot resize the terminal.",
        )
        .await?;
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
        send_command_session_control(
            &record.control_sender,
            CommandSessionControlMessage::Stop { responder: sender },
            "The command session supervisor has exited and cannot stop the process.",
        )
        .await?;
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
            tokio::pin!(notified);
            // Register before reading the terminal state so a fast process exit cannot be lost.
            // 在读取终态前完成注册，避免快速进程退出通知丢失。
            notified.as_mut().enable();
            // The status already reads `Stopped` after the request, so only reaping ends the wait.
            // 停止请求后状态已显示 `Stopped`，因此只有进程被回收才结束等待。
            let process_reaped = record.shared_state.process_reaped().await;
            let snapshot = record.snapshot().await;
            if process_reaped || wait_for_exit_ms == 0 {
                return Ok(snapshot);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_exit_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(snapshot);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified.as_mut()).await;
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
        request: CommandSessionAiReadRequest,
    ) -> Result<CommandSessionReadOutcome, String> {
        let record =
            self.session_for_owner(request.owner_session_id, request.command_session_id)?;
        let after_seq = if request.read_from_start {
            request.after_seq.unwrap_or(0)
        } else if let Some(after_seq) = request.after_seq {
            after_seq
        } else {
            record.ai_delivered_seq().await
        };
        let max_chunks = bounded_command_session_read_chunks(request.max_chunks);
        let wait_for_output_ms = bounded_command_session_read_wait_ms(request.wait_for_output_ms);
        let wait_started_at = Instant::now();

        loop {
            let notified = record.shared_state.change_notify.notified();
            tokio::pin!(notified);
            // Register before reading output and cursor state because producers use notify_waiters.
            // 在读取输出与游标状态前完成注册，因为生产者使用 notify_waiters。
            notified.as_mut().enable();
            let outcome = record
                .read_since(after_seq, max_chunks, request.advance_ai_cursor)
                .await;
            if !outcome.returned_chunks.is_empty()
                || outcome.snapshot.status != CommandSessionStatus::Running
                || wait_for_output_ms == 0
            {
                if request.advance_ai_cursor {
                    record.mark_ai_delivered(&outcome.returned_chunks).await;
                }
                return Ok(outcome);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_output_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(outcome);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified.as_mut()).await;
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
        self.read_session_for_ai_with_cursor_mode(CommandSessionAiReadRequest {
            owner_session_id,
            command_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
            advance_ai_cursor: true,
        })
        .await
    }

    /// Reads the newest retained output for the one-time visible-display bootstrap.
    /// 为可见显示层的一次性首次加载读取最新保留输出。
    ///
    /// # Parameters / 参数
    /// - `owner_session_id`: Session that owns the terminal.
    /// - `command_session_id`: Terminal session to display.
    /// - `max_chunks`: Maximum retained tail chunks returned once.
    /// - `owner_session_id`：拥有该终端的会话。
    /// - `command_session_id`：需要显示的终端会话。
    /// - `max_chunks`：一次返回的最大保留尾部分片数。
    ///
    /// # Returns / 返回值
    /// Returns the current snapshot and an ordered, bounded output tail.
    /// 返回当前快照以及有序且有界的输出尾部。
    pub async fn read_recent_session_for_display(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        max_chunks: usize,
    ) -> Result<CommandSessionReadOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        Ok(record.read_recent_for_display(max_chunks).await)
    }

    /// Waits until one visible display has new output or observes a lifecycle transition.
    /// 等待一个可见显示层出现新输出或观察到生命周期转换。
    ///
    /// This method uses the runtime notification primitive and does not poll.
    /// 此方法使用运行时通知原语，不执行轮询。
    ///
    /// # Parameters / 参数
    /// - `owner_session_id`: Session that owns the terminal.
    /// - `command_session_id`: Terminal session being displayed.
    /// - `display_cursor`: Output, lifecycle, screen-frame, and failure state already rendered.
    /// - `max_chunks`: Maximum output chunks returned in one notification.
    /// - `owner_session_id`：拥有该终端的会话。
    /// - `command_session_id`：正在显示的终端会话。
    /// - `display_cursor`：已经渲染的输出、生命周期、屏幕帧与故障状态。
    /// - `max_chunks`：单次通知返回的最大输出分片数。
    ///
    /// # Returns / 返回值
    /// Returns only after output/state differs from the caller's observed cursor and status.
    /// 仅在输出或状态与调用方观察到的游标及状态不同时返回。
    pub async fn wait_for_session_display_change(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        display_cursor: &CommandSessionDisplayCursor,
        max_chunks: usize,
    ) -> Result<CommandSessionReadOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        loop {
            let notified = record.shared_state.change_notify.notified();
            tokio::pin!(notified);
            // Register before reading state so notify_waiters cannot be lost between the read and await.
            // 在读取状态前完成注册，避免 notify_waiters 在读取与等待之间丢失。
            notified.as_mut().enable();
            let outcome = record
                // A visible cursor makes returned sequence ids immutable for future coalescing.
                // 可见游标会使已返回序号对后续合并保持不可变。
                .read_since(display_cursor.after_seq, max_chunks.max(1), true)
                .await;
            let running_now = outcome.snapshot.status == CommandSessionStatus::Running;
            if !outcome.returned_chunks.is_empty()
                || running_now != display_cursor.running_was_observed
                || outcome.snapshot.screen_frame_seq != display_cursor.screen_frame_seq
                || outcome.snapshot.failure_reason != display_cursor.failure_reason
            {
                return Ok(outcome);
            }
            notified.as_mut().await;
        }
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
        self.read_session_for_ai_with_cursor_mode(CommandSessionAiReadRequest {
            owner_session_id,
            command_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
            advance_ai_cursor: false,
        })
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

    /// Stores one started session and evicts the oldest finished sessions beyond the retention limit.
    /// 保存一条已启动会话，并淘汰超出保留上限的最早已结束会话。
    ///
    /// Running sessions are never evicted; a session whose state lock is busy counts as running.
    /// 运行中的会话从不淘汰；状态锁忙碌的会话按运行中处理。
    pub(super) fn register_session(
        &self,
        record: &Arc<CommandSessionRecord>,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The command session registry is locked and cannot store a new session.")
        })?;
        sessions.insert(record.command_session_id, Arc::clone(record));
        let finished = sessions
            .values()
            .filter_map(|candidate| {
                let state = candidate.shared_state.mutable_state.try_lock().ok()?;
                (state.status != CommandSessionStatus::Running)
                    .then_some((candidate.admission_seq, candidate.command_session_id))
            })
            .collect::<Vec<_>>();
        for command_session_id in
            finished_sessions_to_evict(finished, COMMAND_SESSION_RETAINED_FINISHED_LIMIT)
        {
            sessions.remove(&command_session_id);
        }
        Ok(())
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
