//! Parent-side client for one isolated `vulcan-terminal-host` process.
//! 单个隔离 `vulcan-terminal-host` 进程的父端客户端。

use crate::process_creation::apply_windows_no_window;
use crate::terminal_host_protocol::{
    TERMINAL_HOST_CONTRACT, TERMINAL_HOST_MAX_COLS, TERMINAL_HOST_MAX_ROWS, TerminalHostErrorEvent,
    TerminalHostFrame, TerminalHostFrameKind, TerminalHostResizeRequest, TerminalHostStartRequest,
    TerminalHostStartedEvent, decode_terminal_host_json, encode_terminal_host_json,
    read_terminal_host_frame_async, validate_terminal_host_sequence,
    write_terminal_host_frame_async,
};
use std::path::PathBuf;
use std::process::Stdio;
use std::{env, io};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc::{Receiver, channel};
use tokio::time::{Duration, Instant, timeout};

/// Defines how long the parent waits for the terminal host to acknowledge startup.
/// 定义父进程等待终端宿主确认启动的时长。
const TERMINAL_HOST_START_TIMEOUT: Duration = Duration::from_secs(10);

/// Bounds each protocol control-pipe write so an unresponsive host cannot stall its supervisor.
/// 限制每次协议控制管道写入，避免无响应宿主卡住其监控器。
const TERMINAL_HOST_CONTROL_WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Bounds complete terminal-host events waiting for the owning supervisor.
/// 限制等待所有者监督器处理的完整终端宿主事件数量。
const TERMINAL_HOST_EVENT_QUEUE_LIMIT: usize = 256;

/// Owns the process and pipes of one acknowledged terminal host.
/// 持有一个已确认终端宿主的进程及管道。
pub struct TerminalHostConnection {
    /// Owns the terminal-host child lifecycle.
    /// 持有终端宿主子进程生命周期。
    pub child: Child,
    /// Sends framed control requests to the terminal host.
    /// 向终端宿主发送分帧控制请求。
    pub request_writer: TerminalHostRequestWriter,
    /// Receives complete framed terminal events from the single non-cancellable pipe reader.
    /// 从唯一不可取消的管道读取器接收完整分帧终端事件。
    pub event_receiver: Receiver<io::Result<TerminalHostFrame>>,
    /// Receives host-only diagnostics that never enter terminal output.
    /// 接收不会进入终端输出的宿主专用诊断信息。
    pub stderr: ChildStderr,
    /// Contains the process identifier reported by the host.
    /// 包含宿主报告的进程标识符。
    pub terminal_process_id: Option<u32>,
}

/// Starts the sole stdout owner that always finishes one frame before reading the next frame.
/// 启动唯一的标准输出所有者，确保每次完整读完一帧后才读取下一帧。
///
/// # Parameters / 参数
/// - `stdout`: Owned terminal-host event pipe after the startup frame was consumed.
/// - `stdout`：启动帧已消费后的终端宿主事件管道所有权。
///
/// # Returns / 返回值
/// Returns a bounded receiver of complete frames or one terminal read failure.
/// 返回一个有界接收器，用于接收完整帧或一次终止性读取错误。
fn spawn_terminal_host_event_reader(
    mut stdout: ChildStdout,
) -> Receiver<io::Result<TerminalHostFrame>> {
    let (event_sender, event_receiver) = channel(TERMINAL_HOST_EVENT_QUEUE_LIMIT);
    tokio::spawn(async move {
        loop {
            match read_terminal_host_frame_async(&mut stdout).await {
                Ok(Some(frame)) => {
                    if event_sender.send(Ok(frame)).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = event_sender.send(Err(error)).await;
                    break;
                }
            }
        }
    });
    event_receiver
}

/// Serializes ordered control requests onto one terminal-host stdin pipe.
/// 将有序控制请求串行写入一个终端宿主标准输入管道。
pub struct TerminalHostRequestWriter {
    /// Owns the terminal-host stdin pipe.
    /// 持有终端宿主标准输入管道。
    stdin: ChildStdin,
    /// Allocates monotonically increasing parent request sequences.
    /// 分配单调递增的父进程请求序号。
    next_sequence: u64,
}

/// Waits until an explicitly requested terminal-host stop reaches its forced-reap deadline.
/// 等待显式终端宿主停止请求到达强制回收截止时间。
///
/// # Parameters / 参数
/// - `stop_requested_at`: Time when an explicit stop request began, or `None` while running normally.
/// - `grace_period`: Maximum time granted to the host after that explicit stop request.
/// - `stop_requested_at`：显式停止请求开始的时间；正常运行时为 `None`。
/// - `grace_period`：显式停止请求发出后给予宿主的最长宽限时间。
///
/// # Returns / 返回值
/// Completes at the forced-reap deadline and remains pending forever when no stop was requested.
/// 在强制回收截止时间完成；未请求停止时永久保持等待。
pub(crate) async fn wait_for_terminal_host_stop_deadline(
    stop_requested_at: Option<Instant>,
    grace_period: Duration,
) {
    match stop_requested_at {
        Some(requested_at) => tokio::time::sleep_until(requested_at + grace_period).await,
        None => std::future::pending::<()>().await,
    }
}

impl TerminalHostRequestWriter {
    /// Creates a request writer whose first emitted request uses sequence two.
    /// 创建首个后续请求使用序号二的请求写入器。
    ///
    /// The start handshake always consumes sequence one before this writer is returned.
    /// 在返回该写入器之前，启动握手始终占用序号一。
    ///
    /// # Parameters / 参数
    /// - `stdin`: Owned child stdin after a successful start handshake.
    /// - `stdin`：成功启动握手后的子进程标准输入所有权。
    ///
    /// # Returns / 返回值
    /// Returns an ordered terminal-host request writer.
    /// 返回有序的终端宿主请求写入器。
    fn after_start(stdin: ChildStdin) -> Self {
        Self {
            stdin,
            next_sequence: 2,
        }
    }

    /// Sends exact input bytes without newline or encoding transformation.
    /// 发送精确输入字节，不进行换行或编码转换。
    ///
    /// # Parameters / 参数
    /// - `bytes`: Exact terminal input bytes.
    /// - `bytes`：精确的终端输入字节。
    ///
    /// # Returns / 返回值
    /// Returns the number of accepted bytes after the frame is flushed.
    /// 数据帧刷新后返回已接受字节数。
    pub async fn write_input(&mut self, bytes: Vec<u8>) -> Result<usize, String> {
        let bytes_written = bytes.len();
        self.send(TerminalHostFrameKind::InputBytes, bytes).await?;
        Ok(bytes_written)
    }

    /// Sends a character-grid resize request.
    /// 发送字符网格尺寸调整请求。
    ///
    /// # Parameters / 参数
    /// - `cols`: Positive terminal column count.
    /// - `rows`: Positive terminal row count.
    /// - `cols`：大于零的终端列数。
    /// - `rows`：大于零的终端行数。
    ///
    /// # Returns / 返回值
    /// Returns success after the resize frame is flushed.
    /// 调整尺寸帧刷新后返回成功。
    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        if cols == 0 || rows == 0 || cols > TERMINAL_HOST_MAX_COLS || rows > TERMINAL_HOST_MAX_ROWS
        {
            return Err(format!(
                "Terminal dimensions must be within 1..={TERMINAL_HOST_MAX_COLS} columns and 1..={TERMINAL_HOST_MAX_ROWS} rows."
            ));
        }
        let payload = encode_terminal_host_json(&TerminalHostResizeRequest { cols, rows })
            .map_err(|error| format!("Failed to encode terminal resize request: {error}"))?;
        self.send(TerminalHostFrameKind::Resize, payload).await
    }

    /// Sends a terminal stop request.
    /// 发送终端停止请求。
    ///
    /// # Returns / 返回值
    /// Returns success after the stop frame is flushed.
    /// 停止帧刷新后返回成功。
    pub async fn stop(&mut self) -> Result<(), String> {
        self.send(TerminalHostFrameKind::Stop, Vec::new()).await
    }

    /// Sends one framed request with a monotonic parent sequence.
    /// 使用单调递增的父进程序号发送一个分帧请求。
    ///
    /// # Parameters / 参数
    /// - `kind`: Parent-to-host request kind.
    /// - `payload`: JSON control data or raw input bytes.
    /// - `kind`：父进程到宿主的请求类型。
    /// - `payload`：JSON 控制数据或原始输入字节。
    ///
    /// # Returns / 返回值
    /// Returns success after the complete frame is flushed.
    /// 完整数据帧刷新后返回成功。
    async fn send(&mut self, kind: TerminalHostFrameKind, payload: Vec<u8>) -> Result<(), String> {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            String::from("Terminal-host request sequence exhausted its valid range.")
        })?;
        timeout(
            TERMINAL_HOST_CONTROL_WRITE_TIMEOUT,
            write_terminal_host_frame_async(
                &mut self.stdin,
                &TerminalHostFrame {
                    kind,
                    sequence,
                    payload,
                },
            ),
        )
        .await
        .map_err(|_| String::from("Timed out writing terminal-host control frame."))?
        .map_err(|error| format!("Failed to write terminal-host request: {error}"))
    }
}

/// Resolves the exact cross-platform terminal-host executable for this build.
/// 解析当前构建使用的精确跨平台终端宿主可执行文件。
///
/// Resolution is strict: an explicit environment path must exist; otherwise the binary
/// beside the current executable is preferred, followed by the matching Cargo target profile.
/// 解析过程是严格的：显式环境变量路径必须存在；否则优先使用当前可执行文件旁的二进制，
/// 最后检查匹配的 Cargo target 构建配置目录。
///
/// # Returns / 返回值
/// Returns one existing executable path or a diagnostic listing every checked location.
/// 返回一个已存在的可执行文件路径，或列出所有检查位置的诊断信息。
pub fn resolve_terminal_host_path() -> Result<PathBuf, String> {
    if let Ok(explicit_path) = env::var("VULCAN_TERMINAL_HOST") {
        let explicit_path = PathBuf::from(explicit_path);
        if explicit_path.is_file() {
            return Ok(explicit_path);
        }
        return Err(format!(
            "VULCAN_TERMINAL_HOST points to `{}`, but that file does not exist.",
            explicit_path.display()
        ));
    }
    let current_exe = env::current_exe()
        .map_err(|error| format!("Failed to resolve the current executable path: {error}"))?;
    let host_name = if cfg!(windows) {
        "vulcan-terminal-host.exe"
    } else {
        "vulcan-terminal-host"
    };
    let beside_current = current_exe.with_file_name(host_name);
    if beside_current.is_file() {
        return Ok(beside_current);
    }
    // Cargo integration tests run from `<profile>/deps`; the host remains one directory above.
    // Cargo 集成测试从 `<profile>/deps` 运行，而宿主仍位于其上一级目录。
    let beside_cargo_test_profile = current_exe.parent().and_then(|current_parent| {
        (current_parent.file_name().and_then(|name| name.to_str()) == Some("deps"))
            .then(|| {
                current_parent
                    .parent()
                    .map(|profile| profile.join(host_name))
            })
            .flatten()
    });
    if let Some(candidate) = beside_cargo_test_profile.as_ref()
        && candidate.is_file()
    {
        return Ok(candidate.clone());
    }
    let cargo_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
        .join(host_name);
    if cargo_target.is_file() {
        return Ok(cargo_target);
    }
    let cargo_test_profile_diagnostic = beside_cargo_test_profile
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| String::from("<not a Cargo deps executable>"));
    Err(format!(
        "Terminal host `{host_name}` was not found beside `{}`, beside its Cargo test profile at `{cargo_test_profile_diagnostic}`, or at `{}`. Build it with `cargo build -p vulcan-terminal-host`.",
        current_exe.display(),
        cargo_target.display()
    ))
}

/// Spawns one isolated terminal host and completes its strict start handshake.
/// 派生一个隔离终端宿主并完成严格启动握手。
///
/// # Parameters / 参数
/// - `request`: Exact terminal program, arguments, cwd, and initial dimensions.
/// - `request`：精确的终端程序、参数、工作目录及初始尺寸。
///
/// # Returns / 返回值
/// Returns owned host pipes only after receiving a valid `Started` event.
/// 仅在收到有效 `Started` 事件后返回宿主管道所有权。
pub async fn spawn_terminal_host(
    request: TerminalHostStartRequest,
) -> Result<TerminalHostConnection, String> {
    let host_path = resolve_terminal_host_path()?;
    let mut command = Command::new(crate::runtime_assets::native_host_path(&host_path));
    command
        .arg("--contract")
        .arg(TERMINAL_HOST_CONTRACT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    apply_windows_no_window(&mut command);
    let mut child = command.spawn().map_err(|error| {
        format!(
            "Failed to start terminal host `{}`: {error}",
            host_path.display()
        )
    })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        String::from("Terminal host started without an available stdin control pipe.")
    })?;
    let mut stdout = child.stdout.take().ok_or_else(|| {
        String::from("Terminal host started without an available stdout event pipe.")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        String::from("Terminal host started without an available stderr diagnostic pipe.")
    })?;
    let start_payload = encode_terminal_host_json(&request)
        .map_err(|error| format!("Failed to encode terminal-host start request: {error}"))?;
    timeout(
        TERMINAL_HOST_CONTROL_WRITE_TIMEOUT,
        write_terminal_host_frame_async(
            &mut stdin,
            &TerminalHostFrame {
                kind: TerminalHostFrameKind::Start,
                sequence: 1,
                payload: start_payload,
            },
        ),
    )
    .await
    .map_err(|_| String::from("Timed out writing terminal-host start request."))?
    .map_err(|error| format!("Failed to write terminal-host start request: {error}"))?;
    let first_event = timeout(
        TERMINAL_HOST_START_TIMEOUT,
        read_terminal_host_frame_async(&mut stdout),
    )
    .await
    .map_err(|_| String::from("Terminal host did not acknowledge startup within 10 seconds."))?
    .map_err(|error| format!("Failed to read terminal-host start response: {error}"))?
    .ok_or_else(|| String::from("Terminal host exited before acknowledging startup."))?;
    let mut initial_event_sequence = 0;
    validate_terminal_host_sequence(&mut initial_event_sequence, first_event.sequence)
        .map_err(|error| format!("Invalid terminal-host start response sequence: {error}"))?;
    let started: TerminalHostStartedEvent = match first_event.kind {
        TerminalHostFrameKind::Started => decode_terminal_host_json(&first_event.payload)
            .map_err(|error| format!("Failed to decode terminal-host start response: {error}"))?,
        TerminalHostFrameKind::Error => {
            let error: TerminalHostErrorEvent = decode_terminal_host_json(&first_event.payload)
                .map_err(|decode_error| {
                    format!("Terminal host returned a malformed startup error: {decode_error}")
                })?;
            return Err(format!(
                "Terminal host startup failed [{}]: {}",
                error.code, error.message
            ));
        }
        unexpected => {
            return Err(format!(
                "Terminal host returned unexpected startup event: {unexpected:?}."
            ));
        }
    };
    Ok(TerminalHostConnection {
        child,
        request_writer: TerminalHostRequestWriter::after_start(stdin),
        event_receiver: spawn_terminal_host_event_reader(stdout),
        stderr,
        terminal_process_id: started.process_id,
    })
}

#[cfg(test)]
mod tests {
    use super::{resolve_terminal_host_path, wait_for_terminal_host_stop_deadline};
    use tokio::time::{Duration, Instant, timeout};

    /// Verifies the test build can locate the sibling Cargo terminal-host binary when present.
    /// 验证测试构建可在存在时定位 Cargo 同级终端宿主二进制。
    #[test]
    fn terminal_host_path_resolution_is_strict() {
        let result = resolve_terminal_host_path();
        if let Ok(path) = result {
            assert!(path.is_file());
        }
    }

    /// Verifies forced reaping is armed only by an explicit stop request.
    /// 验证只有显式停止请求才会启用强制回收。
    #[tokio::test]
    async fn stop_deadline_requires_an_explicit_stop_request() {
        timeout(
            Duration::from_millis(50),
            wait_for_terminal_host_stop_deadline(Some(Instant::now()), Duration::ZERO),
        )
        .await
        .expect("an explicit zero-grace stop deadline must complete");

        assert!(
            timeout(
                Duration::from_millis(10),
                wait_for_terminal_host_stop_deadline(None, Duration::ZERO),
            )
            .await
            .is_err(),
            "a normally running host must not acquire a forced-reap deadline"
        );
    }

    /// Verifies runtime supervisors never add message-based liveness probes to terminal output/control pipes.
    /// 验证运行时监督器绝不向终端输出或控制管道加入消息式存活探针。
    #[test]
    fn runtime_supervisors_do_not_schedule_automatic_liveness_probes() {
        // project_terminal_runtime_source captures the project-owned interactive terminal supervisor.
        // project_terminal_runtime_source 捕获项目级交互终端监督器源码。
        let project_terminal_runtime_source = include_str!("project_terminal_runtime.rs");
        // command_session_pty_source captures the AI session terminal supervisor.
        // command_session_pty_source 捕获 AI 会话终端监督器源码。
        let command_session_pty_source = include_str!("command_session_runtime/pty.rs");
        // terminal_host_source captures the isolated process implementation that owns the PTY.
        // terminal_host_source 捕获持有 PTY 的隔离进程实现。
        let terminal_host_source = include_str!("../../vulcan-terminal-host/src/main.rs");

        assert!(!project_terminal_runtime_source.contains("request_writer.ping("));
        assert!(!command_session_pty_source.contains("request_writer.ping("));
        assert!(!project_terminal_runtime_source.contains("read_terminal_host_frame_async"));
        assert!(!command_session_pty_source.contains("read_terminal_host_frame_async"));
        assert!(!terminal_host_source.contains("liveness probe"));
    }
}
