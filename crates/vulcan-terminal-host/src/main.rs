//! Runs one isolated cross-platform pseudo terminal behind a strict framed protocol.
//! 在严格分帧协议之后运行一个隔离的跨平台伪终端。

mod process_job;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use process_job::TerminalProcessJob;
use std::io::{self, BufWriter, Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use vulcan_terminal_host_protocol::{
    TERMINAL_HOST_CONTRACT, TERMINAL_HOST_MAX_COLS, TERMINAL_HOST_MAX_ROWS, TerminalHostErrorEvent,
    TerminalHostExitedEvent, TerminalHostFrame, TerminalHostFrameKind, TerminalHostResizeRequest,
    TerminalHostStartRequest, TerminalHostStartedEvent, decode_terminal_host_json,
    encode_terminal_host_json, read_terminal_host_frame, validate_terminal_host_sequence,
    write_terminal_host_frame,
};

/// Bounds decoded parent controls awaiting the single PTY owner loop.
/// 限制等待单一 PTY 所有者循环处理的已解码父控制数量。
const TERMINAL_HOST_PARENT_CONTROL_QUEUE_LIMIT: usize = 128;

/// Contains one decoded parent request or a terminal protocol read failure.
/// 包含一个已解码的父进程请求或终端协议读取故障。
enum ParentRequestEvent {
    /// Carries one validated request frame.
    /// 承载一个已验证的请求帧。
    Frame(TerminalHostFrame),
    /// Marks clean parent-pipe closure.
    /// 标记父进程管道正常关闭。
    ParentDisconnected,
    /// Carries a malformed or failed request read.
    /// 承载格式错误或读取失败的请求。
    ReadFailed(String),
    /// Carries the terminal child's event-driven wait result.
    /// 承载终端子进程由事件驱动的等待结果。
    ChildExited(Result<Option<u32>, String>),
}

/// Serializes terminal-host events onto stdout without allowing interleaved frames.
/// 将终端宿主事件串行写入标准输出，避免数据帧交错。
struct TerminalHostEventWriter {
    /// Guards the shared buffered stdout writer.
    /// 保护共享的标准输出缓冲写入器。
    stdout: Mutex<BufWriter<io::Stdout>>,
    /// Allocates a monotonically increasing host-event sequence.
    /// 分配单调递增的宿主事件序号。
    next_sequence: AtomicU64,
}

impl TerminalHostEventWriter {
    /// Creates an event writer bound to the process protocol stdout.
    /// 创建绑定到进程协议标准输出的事件写入器。
    ///
    /// # Returns / 返回值
    /// Returns a writer whose first emitted event has sequence one.
    /// 返回首个事件序号为一的写入器。
    fn new() -> Self {
        Self {
            stdout: Mutex::new(BufWriter::new(io::stdout())),
            next_sequence: AtomicU64::new(1),
        }
    }

    /// Emits one raw event frame with the next host sequence.
    /// 使用下一个宿主序号发出一个原始事件帧。
    ///
    /// # Parameters / 参数
    /// - `kind`: Protocol event kind.
    /// - `payload`: JSON control data or raw terminal output bytes.
    /// - `kind`：协议事件类型。
    /// - `payload`：JSON 控制数据或原始终端输出字节。
    ///
    /// # Returns / 返回值
    /// Returns success after the complete frame is flushed to stdout.
    /// 完整数据帧刷新到标准输出后返回成功。
    fn emit(&self, kind: TerminalHostFrameKind, payload: Vec<u8>) -> io::Result<()> {
        // The stdout lock must cover sequence allocation and frame delivery so concurrent terminal
        // event writers cannot publish a later sequence before an earlier sequence.
        // 标准输出锁必须同时覆盖序号分配与数据帧交付，避免并发终端事件写入器先发布较大的序号。
        let mut stdout = self
            .stdout
            .lock()
            .map_err(|_| io::Error::other("terminal-host stdout lock was poisoned"))?;
        let sequence = self
            .next_sequence
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "terminal-host event sequence exhausted its valid range",
                )
            })?;
        let frame = TerminalHostFrame {
            kind,
            sequence,
            payload,
        };
        write_terminal_host_frame(&mut *stdout, &frame)
    }

    /// Serializes and emits one JSON control event.
    /// 序列化并发出一个 JSON 控制事件。
    ///
    /// # Parameters / 参数
    /// - `kind`: Protocol event kind.
    /// - `value`: Serializable control payload.
    /// - `kind`：协议事件类型。
    /// - `value`：可序列化的控制负载。
    ///
    /// # Returns / 返回值
    /// Returns success after JSON encoding and complete frame delivery.
    /// JSON 编码及完整数据帧交付后返回成功。
    fn emit_json<T: serde::Serialize>(
        &self,
        kind: TerminalHostFrameKind,
        value: &T,
    ) -> io::Result<()> {
        self.emit(kind, encode_terminal_host_json(value)?)
    }

    /// Emits a machine-readable terminal-host error event.
    /// 发出机器可读的终端宿主错误事件。
    ///
    /// # Parameters / 参数
    /// - `code`: Stable error category.
    /// - `message`: Human-readable diagnostic.
    /// - `code`：稳定错误类别。
    /// - `message`：人类可读诊断信息。
    ///
    /// # Returns / 返回值
    /// Returns success after the error event is delivered.
    /// 错误事件交付后返回成功。
    fn emit_error(&self, code: &str, message: impl Into<String>) -> io::Result<()> {
        self.emit_json(
            TerminalHostFrameKind::Error,
            &TerminalHostErrorEvent {
                code: code.to_owned(),
                message: message.into(),
            },
        )
    }
}

/// Validates process arguments and runs the single-terminal host lifecycle.
/// 验证进程参数并运行单终端宿主生命周期。
///
/// # Returns / 返回值
/// Returns a successful exit code only after a clean terminal-host shutdown.
/// 仅在终端宿主正常关闭后返回成功退出码。
fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments != ["--contract", TERMINAL_HOST_CONTRACT] {
        eprintln!("vulcan-terminal-host requires: --contract {TERMINAL_HOST_CONTRACT}");
        return ExitCode::from(2);
    }
    match run_terminal_host() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vulcan-terminal-host failed: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Owns one PTY from the first start request until child exit or parent disconnect.
/// 从首个启动请求起持有一个 PTY，直至子进程退出或父进程断开。
///
/// # Returns / 返回值
/// Returns success after emitting the final exit event; returns an I/O failure otherwise.
/// 发出最终退出事件后返回成功；否则返回 I/O 故障。
fn run_terminal_host() -> io::Result<()> {
    let event_writer = Arc::new(TerminalHostEventWriter::new());
    let mut stdin = io::stdin().lock();
    let start_frame = read_terminal_host_frame(&mut stdin)?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "parent disconnected before terminal start",
        )
    })?;
    if start_frame.kind != TerminalHostFrameKind::Start {
        let message = format!(
            "first terminal-host request must be Start, received {:?}",
            start_frame.kind
        );
        let _ = event_writer.emit_error("invalid_start_sequence", &message);
        return Err(io::Error::new(io::ErrorKind::InvalidData, message));
    }
    let mut parent_sequence = 0;
    validate_terminal_host_sequence(&mut parent_sequence, start_frame.sequence)?;
    let start_request: TerminalHostStartRequest = decode_terminal_host_json(&start_frame.payload)
        .map_err(|error| {
        terminal_host_startup_error(
            &event_writer,
            "invalid_start_request",
            format!("failed to decode terminal start request: {error}"),
        )
    })?;
    validate_start_request(&start_request).map_err(|error| {
        terminal_host_startup_error(&event_writer, "invalid_start_request", error.to_string())
    })?;
    drop(stdin);

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: start_request.rows,
            cols: start_request.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| {
            terminal_host_startup_error(
                &event_writer,
                "pty_create_failed",
                format!("failed to create pseudo terminal: {error}"),
            )
        })?;
    let mut command = CommandBuilder::new(&start_request.program);
    command.args(&start_request.args);
    command.cwd(&start_request.cwd);
    let mut child = pair.slave.spawn_command(command).map_err(|error| {
        terminal_host_startup_error(
            &event_writer,
            "terminal_child_spawn_failed",
            format!("failed to spawn terminal child: {error}"),
        )
    })?;
    drop(pair.slave);
    let child_process_id = child.process_id();
    let terminal_process_job = match TerminalProcessJob::attach(child_process_id) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(terminal_host_startup_error(
                &event_writer,
                "terminal_process_isolation_failed",
                format!("failed to isolate terminal process tree: {error}"),
            ));
        }
    };
    let reader = pair.master.try_clone_reader().map_err(|error| {
        terminal_host_startup_error(
            &event_writer,
            "pty_reader_create_failed",
            format!("failed to clone PTY reader: {error}"),
        )
    })?;
    let writer = pair.master.take_writer().map_err(|error| {
        terminal_host_startup_error(
            &event_writer,
            "pty_writer_create_failed",
            format!("failed to take PTY writer: {error}"),
        )
    })?;
    let terminal_writer = Arc::new(Mutex::new(writer));
    let terminal_screen = Arc::new(Mutex::new(vt100::Parser::new(
        start_request.rows,
        start_request.cols,
        0,
    )));

    event_writer.emit_json(
        TerminalHostFrameKind::Started,
        &TerminalHostStartedEvent {
            process_id: child_process_id,
        },
    )?;

    let output_event_writer = Arc::clone(&event_writer);
    let output_terminal_writer = Arc::clone(&terminal_writer);
    let output_terminal_screen = Arc::clone(&terminal_screen);
    let output_thread = std::thread::Builder::new()
        .name(String::from("vulcan-terminal-host-output"))
        .spawn(move || {
            pump_terminal_output(
                reader,
                output_terminal_writer,
                output_terminal_screen,
                output_event_writer,
            )
        })
        .map_err(|error| io::Error::other(format!("failed to start PTY output thread: {error}")))?;

    let (terminal_event_sender, terminal_event_receiver) =
        mpsc::sync_channel(TERMINAL_HOST_PARENT_CONTROL_QUEUE_LIMIT);
    let parent_request_sender = terminal_event_sender.clone();
    let request_thread = std::thread::Builder::new()
        .name(String::from("vulcan-terminal-host-control"))
        .spawn(move || pump_parent_requests(parent_request_sender, parent_sequence))
        .map_err(|error| {
            io::Error::other(format!("failed to start parent request thread: {error}"))
        })?;

    let child_exit_sender = terminal_event_sender.clone();
    let wait_thread = std::thread::Builder::new()
        .name(String::from("vulcan-terminal-host-wait"))
        .spawn(move || {
            let result = child.wait().map_err(|error| error.to_string());
            let _ = child_exit_sender.send(ParentRequestEvent::ChildExited(
                result.map(|status| Some(status.exit_code())),
            ));
        })
        .map_err(|error| io::Error::other(format!("failed to start child wait thread: {error}")))?;
    drop(terminal_event_sender);

    let mut stopped_by_request = false;
    let mut parent_disconnected = false;
    let exit_code = loop {
        match terminal_event_receiver.recv() {
            Ok(ParentRequestEvent::Frame(frame)) => {
                if handle_parent_request(
                    frame,
                    &*pair.master,
                    &terminal_writer,
                    &terminal_screen,
                    &terminal_process_job,
                )? {
                    stopped_by_request = true;
                }
            }
            Ok(ParentRequestEvent::ParentDisconnected) => {
                if !parent_disconnected {
                    parent_disconnected = true;
                    stopped_by_request = true;
                    request_terminal_stop(&terminal_process_job)?;
                }
            }
            Ok(ParentRequestEvent::ReadFailed(error)) => {
                event_writer.emit_error("parent_protocol_read_failed", &error)?;
                stopped_by_request = true;
                request_terminal_stop(&terminal_process_job)?;
            }
            Ok(ParentRequestEvent::ChildExited(Ok(status))) => break status,
            Ok(ParentRequestEvent::ChildExited(Err(error))) => {
                event_writer.emit_error("child_wait_failed", &error)?;
                break None;
            }
            Err(_) => {
                event_writer.emit_error(
                    "terminal_event_channel_disconnected",
                    "terminal host event channel disconnected before child exit",
                )?;
                stopped_by_request = true;
                request_terminal_stop(&terminal_process_job)?;
                break None;
            }
        }
    };

    // Close the owning PTY master and process tree before waiting for the cloned output reader.
    // 在等待克隆的输出读取器前关闭 PTY 主端所有权与进程树。
    drop(pair.master);
    drop(terminal_process_job);
    drop(terminal_writer);
    drop(terminal_screen);
    let _ = output_thread.join();
    // The request thread can remain blocked on stdin until this process exits, so detach its handle explicitly.
    // 请求线程可能一直阻塞在 stdin，直到当前进程退出，因此明确分离其句柄。
    drop(request_thread);
    let _ = wait_thread.join();
    event_writer.emit_json(
        TerminalHostFrameKind::Exited,
        &TerminalHostExitedEvent {
            exit_code,
            stopped_by_request,
        },
    )?;
    Ok(())
}

/// Emits one startup failure before returning the same diagnostic to process stderr.
/// 在向进程标准错误返回同一诊断前发出一次启动失败事件。
///
/// # Parameters / 参数
/// - `event_writer`: Framed protocol writer available before the start acknowledgement.
/// - `code`: Stable machine-readable startup error category.
/// - `message`: Exact human-readable failure diagnostic.
/// - `event_writer`：启动确认前可用的分帧协议写入器。
/// - `code`：稳定且机器可读的启动错误类别。
/// - `message`：精确的人类可读故障诊断。
///
/// # Returns / 返回值
/// Returns an I/O error carrying the emitted diagnostic for normal error propagation.
/// 返回携带已发出诊断的 I/O 错误，以供常规错误传播。
fn terminal_host_startup_error(
    event_writer: &TerminalHostEventWriter,
    code: &str,
    message: String,
) -> io::Error {
    let _ = event_writer.emit_error(code, message.clone());
    io::Error::other(message)
}

/// Rejects invalid terminal dimensions and empty executable/cwd values before spawning.
/// 在派生进程前拒绝无效终端尺寸以及空的可执行文件或工作目录。
///
/// # Parameters / 参数
/// - `request`: Decoded terminal start contract.
/// - `request`：已解码的终端启动契约。
///
/// # Returns / 返回值
/// Returns success only when all mandatory fields are valid.
/// 仅当所有必填字段有效时返回成功。
fn validate_start_request(request: &TerminalHostStartRequest) -> io::Result<()> {
    if request.program.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "terminal program must not be empty",
        ));
    }
    if request.cwd.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "terminal working directory must not be empty",
        ));
    }
    if request.cols == 0
        || request.rows == 0
        || request.cols > TERMINAL_HOST_MAX_COLS
        || request.rows > TERMINAL_HOST_MAX_ROWS
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "terminal dimensions must be within 1..={TERMINAL_HOST_MAX_COLS} columns and 1..={TERMINAL_HOST_MAX_ROWS} rows"
            ),
        ));
    }
    Ok(())
}

/// Continuously decodes framed parent requests until the parent pipe closes.
/// 持续解码父进程分帧请求，直至父进程管道关闭。
///
/// # Parameters / 参数
/// - `sender`: Channel that transfers requests to the PTY owner loop.
/// - `last_sequence`: Last validated request sequence from the startup handshake.
/// - `sender`：向 PTY 所有者循环传递请求的通道。
/// - `last_sequence`：启动握手期间最后校验通过的请求序号。
fn pump_parent_requests(sender: SyncSender<ParentRequestEvent>, mut last_sequence: u64) {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    loop {
        match read_terminal_host_frame(&mut stdin) {
            Ok(Some(frame)) => {
                if let Err(error) =
                    validate_terminal_host_sequence(&mut last_sequence, frame.sequence)
                {
                    let _ = sender.send(ParentRequestEvent::ReadFailed(error.to_string()));
                    return;
                }
                if sender.send(ParentRequestEvent::Frame(frame)).is_err() {
                    return;
                }
            }
            Ok(None) => {
                let _ = sender.send(ParentRequestEvent::ParentDisconnected);
                return;
            }
            Err(error) => {
                let _ = sender.send(ParentRequestEvent::ReadFailed(error.to_string()));
                return;
            }
        }
    }
}

/// Applies one parent control request to the owned terminal.
/// 将一个父进程控制请求应用到所拥有的终端。
///
/// # Parameters / 参数
/// - `frame`: Validated parent request frame.
/// - `master`: PTY master used for resizing.
/// - `terminal_writer`: Serialized PTY input writer.
/// - `terminal_screen`: Headless terminal state used for accurate status replies.
/// - `terminal_process_job`: Operating-system process-tree isolation guard.
/// - `frame`：已验证的父进程请求帧。
/// - `master`：用于调整尺寸的 PTY 主端。
/// - `terminal_writer`：串行化的 PTY 输入写入器。
/// - `terminal_screen`：用于准确状态响应的无界面终端状态。
/// - `terminal_process_job`：操作系统进程树隔离守卫。
///
/// # Returns / 返回值
/// Returns `true` when the request asked the terminal to stop.
/// 当请求要求终止终端时返回 `true`。
fn handle_parent_request(
    frame: TerminalHostFrame,
    master: &(dyn portable_pty::MasterPty + Send),
    terminal_writer: &Arc<Mutex<Box<dyn Write + Send>>>,
    terminal_screen: &Arc<Mutex<vt100::Parser>>,
    terminal_process_job: &TerminalProcessJob,
) -> io::Result<bool> {
    match frame.kind {
        TerminalHostFrameKind::InputBytes => {
            write_terminal_input(terminal_writer, &frame.payload)?;
            Ok(false)
        }
        TerminalHostFrameKind::Resize => {
            let resize: TerminalHostResizeRequest = decode_terminal_host_json(&frame.payload)?;
            if resize.cols == 0
                || resize.rows == 0
                || resize.cols > TERMINAL_HOST_MAX_COLS
                || resize.rows > TERMINAL_HOST_MAX_ROWS
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "terminal resize must be within 1..={TERMINAL_HOST_MAX_COLS} columns and 1..={TERMINAL_HOST_MAX_ROWS} rows"
                    ),
                ));
            }
            master
                .resize(PtySize {
                    rows: resize.rows,
                    cols: resize.cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|error| io::Error::other(format!("failed to resize PTY: {error}")))?;
            terminal_screen
                .lock()
                .map_err(|_| io::Error::other("terminal screen lock was poisoned"))?
                .screen_mut()
                .set_size(resize.rows, resize.cols);
            Ok(false)
        }
        TerminalHostFrameKind::Stop => {
            request_terminal_stop(terminal_process_job)?;
            Ok(true)
        }
        unexpected => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected parent terminal-host frame: {unexpected:?}"),
        )),
    }
}

/// Writes and flushes raw input bytes to the terminal child.
/// 向终端子进程写入并刷新原始输入字节。
///
/// # Parameters / 参数
/// - `terminal_writer`: Serialized PTY input writer.
/// - `bytes`: Exact bytes to deliver.
/// - `terminal_writer`：串行化的 PTY 输入写入器。
/// - `bytes`：需要原样交付的字节。
///
/// # Returns / 返回值
/// Returns success after every input byte has been flushed.
/// 所有输入字节刷新后返回成功。
fn write_terminal_input(
    terminal_writer: &Arc<Mutex<Box<dyn Write + Send>>>,
    bytes: &[u8],
) -> io::Result<()> {
    let mut writer = terminal_writer
        .lock()
        .map_err(|_| io::Error::other("PTY input lock was poisoned"))?;
    writer.write_all(bytes)?;
    writer.flush()
}

/// Requests deterministic termination of the isolated operating-system process tree.
/// 请求确定性终止隔离的操作系统进程树。
///
/// # Parameters / 参数
/// - `terminal_process_job`: Platform-specific process-tree ownership guard.
/// - `terminal_process_job`：平台专用的进程树所有权守卫。
///
/// # Returns / 返回值
/// Returns success after the platform accepts termination of every assigned descendant.
/// 平台接受终止全部已绑定后代进程后返回成功。
fn request_terminal_stop(terminal_process_job: &TerminalProcessJob) -> io::Result<()> {
    terminal_process_job
        .terminate()
        .map_err(|error| io::Error::other(format!("failed to stop terminal process tree: {error}")))
}

/// Streams PTY output to the parent and answers terminal cursor-position queries locally.
/// 将 PTY 输出流式发送给父进程，并在本地响应终端光标位置查询。
///
/// # Parameters / 参数
/// - `reader`: Blocking PTY output reader.
/// - `terminal_writer`: PTY input writer used for terminal status replies.
/// - `terminal_screen`: Headless parser used to track the exact cursor position.
/// - `event_writer`: Framed host output destination.
/// - `reader`：阻塞式 PTY 输出读取器。
/// - `terminal_writer`：用于终端状态响应的 PTY 输入写入器。
/// - `terminal_screen`：用于跟踪精确光标位置的无界面解析器。
/// - `event_writer`：分帧宿主输出目标。
fn pump_terminal_output(
    mut reader: Box<dyn Read + Send>,
    terminal_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    terminal_screen: Arc<Mutex<vt100::Parser>>,
    event_writer: Arc<TerminalHostEventWriter>,
) {
    let mut buffer = [0_u8; 8192];
    let mut pending_query_prefix = Vec::new();
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                if !pending_query_prefix.is_empty() {
                    let _ = event_writer.emit(
                        TerminalHostFrameKind::OutputBytes,
                        std::mem::take(&mut pending_query_prefix),
                    );
                }
                return;
            }
            Ok(bytes_read) => {
                let (visible_output, replies) = match terminal_screen.lock() {
                    Ok(mut screen) => filter_terminal_status_queries(
                        &mut screen,
                        &mut pending_query_prefix,
                        &buffer[..bytes_read],
                    ),
                    Err(_) => {
                        let _ = event_writer.emit_error(
                            "terminal_screen_lock_poisoned",
                            "terminal screen lock was poisoned",
                        );
                        return;
                    }
                };
                if !replies.is_empty()
                    && let Err(error) = write_terminal_input(&terminal_writer, &replies)
                {
                    let _ =
                        event_writer.emit_error("terminal_status_reply_failed", error.to_string());
                    return;
                }
                if !visible_output.is_empty()
                    && event_writer
                        .emit(TerminalHostFrameKind::OutputBytes, visible_output)
                        .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                let _ = event_writer.emit_error("terminal_output_read_failed", error.to_string());
                return;
            }
        }
    }
}

/// Removes complete cursor-position queries while preserving split query prefixes across reads.
/// 移除完整的光标位置查询，同时跨读取保留被拆分的查询前缀。
///
/// # Parameters / 参数
/// - `pending_prefix`: Incomplete query prefix retained from the previous read.
/// - `input`: Newly read PTY bytes.
/// - `pending_prefix`：上一次读取保留的不完整查询前缀。
/// - `input`：新读取的 PTY 字节。
///
/// # Returns / 返回值
/// Returns visible output bytes and exact device-status replies in request order.
/// 返回可见输出字节及按请求顺序排列的精确设备状态响应。
fn filter_terminal_status_queries(
    parser: &mut vt100::Parser,
    pending_prefix: &mut Vec<u8>,
    input: &[u8],
) -> (Vec<u8>, Vec<u8>) {
    const CURSOR_QUERY: &[u8] = b"\x1b[6n";
    const STATUS_QUERY: &[u8] = b"\x1b[5n";
    let mut combined = std::mem::take(pending_prefix);
    combined.extend_from_slice(input);
    let mut visible_output = Vec::with_capacity(combined.len());
    let mut replies = Vec::new();
    let mut cursor = 0;
    let mut segment_start = 0;
    while cursor < combined.len() {
        let remaining = &combined[cursor..];
        let reply = if remaining.starts_with(CURSOR_QUERY) {
            Some(CURSOR_QUERY)
        } else if remaining.starts_with(STATUS_QUERY) {
            Some(STATUS_QUERY)
        } else {
            None
        };
        if let Some(query) = reply {
            let visible_segment = &combined[segment_start..cursor];
            parser.process(visible_segment);
            visible_output.extend_from_slice(visible_segment);
            if query == CURSOR_QUERY {
                let (row, col) = parser.screen().cursor_position();
                replies.extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
            } else {
                replies.extend_from_slice(b"\x1b[0n");
            }
            cursor += query.len();
            segment_start = cursor;
            continue;
        }
        if remaining.len() < CURSOR_QUERY.len()
            && (CURSOR_QUERY.starts_with(remaining) || STATUS_QUERY.starts_with(remaining))
        {
            let visible_segment = &combined[segment_start..cursor];
            parser.process(visible_segment);
            visible_output.extend_from_slice(visible_segment);
            pending_prefix.extend_from_slice(remaining);
            segment_start = combined.len();
            break;
        }
        cursor += 1;
    }
    if segment_start < combined.len() {
        let visible_segment = &combined[segment_start..];
        parser.process(visible_segment);
        visible_output.extend_from_slice(visible_segment);
    }
    (visible_output, replies)
}

#[cfg(test)]
mod tests {
    use super::filter_terminal_status_queries;

    /// Verifies a cursor query split across PTY reads is removed exactly once.
    /// 验证跨 PTY 读取拆分的光标查询只被移除一次。
    #[test]
    fn split_terminal_status_query_is_buffered_and_counted() {
        let mut pending = Vec::new();
        let mut parser = vt100::Parser::new(24, 80, 0);
        let (first_output, first_replies) =
            filter_terminal_status_queries(&mut parser, &mut pending, b"before\x1b[");
        assert_eq!(first_output, b"before");
        assert!(first_replies.is_empty());
        assert_eq!(pending, b"\x1b[");

        let (second_output, second_replies) =
            filter_terminal_status_queries(&mut parser, &mut pending, b"6nafter");
        assert_eq!(second_output, b"after");
        assert_eq!(second_replies, b"\x1b[1;7R");
        assert!(pending.is_empty());
    }

    /// Verifies status and cursor reports preserve request order and use the parsed cursor.
    /// 验证状态与光标报告保持请求顺序，并使用已解析的光标位置。
    #[test]
    fn terminal_status_replies_use_current_parsed_state() {
        let mut pending = Vec::new();
        let mut parser = vt100::Parser::new(24, 80, 0);
        let (visible, replies) = filter_terminal_status_queries(
            &mut parser,
            &mut pending,
            b"abc\x1b[6n\r\nx\x1b[5n\x1b[6n",
        );
        assert_eq!(visible, b"abc\r\nx");
        assert_eq!(replies, b"\x1b[1;4R\x1b[0n\x1b[2;2R");
        assert!(pending.is_empty());
    }
}
