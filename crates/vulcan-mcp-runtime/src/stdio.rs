//! Stdio MCP client transport backed by the official `rmcp` SDK.
//! 基于官方 `rmcp` SDK 的 stdio MCP 客户端传输。

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rmcp::model::{GetPromptResult, Prompt, ReadResourceResult, Resource};
use rmcp::service::{Peer, RoleClient, ServiceExt};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::broadcast;
use vulcan_host_tools::{
    output_decode::decode_shell_output_bytes, process_creation::apply_windows_no_window,
};

use crate::error::McpRuntimeError;
use crate::rmcp_support::{
    EventingClientHandler, call_rmcp_tool, get_rmcp_prompt, list_rmcp_prompts, list_rmcp_resources,
    list_rmcp_tools, read_rmcp_resource, spawn_rmcp_disconnect_watch,
};
use crate::shutdown::{TransportShutdown, observe_backend_close};
use crate::transport::{McpServerIdentity, McpToolSchema, McpTransport, McpTransportEvent};

mod child;
mod child_backend;
mod native_pipe;
mod pipe_completion;
use child_backend::ManagedChildBackend;
use pipe_completion::PipeCompletion;

#[cfg(test)]
mod resource_tests;
#[cfg(test)]
mod stderr_tests;

/// Maximum number of sanitized stderr chunks retained per stdio transport.
/// 每个 stdio 传输保留的最大脱敏 stderr 分片数量。
const MAX_STDERR_LOG_ENTRIES: usize = 200;
/// Maximum total UTF-8 bytes retained per stdio transport.
/// 每个 stdio 传输保留的最大 UTF-8 总字节数。
const MAX_STDERR_LOG_BYTES: usize = 64 * 1024;
/// Maximum bytes accepted for one unterminated stderr line before the entire line is discarded.
/// 单条未终止 stderr 行在整行被丢弃前允许的最大字节数。
const MAX_STDERR_LINE_BYTES: usize = 64 * 1024;

/// Production stdio MCP client transport.
/// 生产级 stdio MCP 客户端传输。
pub struct StdioTransport {
    /// Stable endpoint label surfaced to the host runtime.
    /// 暴露给宿主运行时的稳定端点标签。
    endpoint: String,
    /// Cloneable rmcp client peer used for schema discovery and calls.
    /// 用于 schema 发现与调用的可克隆 rmcp 客户端 peer。
    peer: Peer<RoleClient>,
    /// Owns the rmcp service and its single shared close operation.
    /// 拥有 rmcp 服务及其唯一共享关闭操作。
    shutdown: TransportShutdown,
    /// Broadcast channel used to surface tool refresh and disconnect events.
    /// 用于对外广播工具刷新与断连事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
    /// Bounded sanitized stderr diagnostics retained for host inspection.
    /// 为宿主检查保留的有界脱敏 stderr 诊断。
    recent_logs: Arc<Mutex<VecDeque<String>>>,
}

impl StdioTransport {
    /// Spawns with host authority retained by the actual process and each native pipe.
    /// 启动时由实际进程及每条原生管道保留宿主权威。
    /// Endpoint identifies diagnostics; program, args, cwd, env and inherit_environment define the exact launch.
    /// endpoint 标识诊断；program、args、cwd、env 及 inherit_environment 定义精确启动。
    /// Resources is consumed before native work starts; its original receipt survives cancellation of initialization.
    /// 原生工作开始前消费 resources；初始化取消后，其原回执继续存续。
    /// Returns a connected transport or the original launch or handshake error; errors alone do not prove release.
    /// 返回已连接传输或原启动及握手错误；错误本身不能证明释放。
    pub async fn spawn_with_resources(
        endpoint: impl Into<String>,
        program: impl AsRef<std::ffi::OsStr>,
        args: &[&str],
        cwd: Option<&Path>,
        env: &BTreeMap<String, String>,
        inherit_environment: bool,
        resources: crate::McpTransportResources,
    ) -> Result<Self, McpRuntimeError> {
        // Own the diagnostic label independently of borrowed launch arguments.
        // 独立于借用启动参数，持有诊断标签。
        let endpoint = endpoint.into();
        // Preserve the exact native executable identity across initialization awaits.
        // 跨初始化等待保留精确原生可执行文件身份。
        let program = program.as_ref().to_owned();
        // Owned arguments preserve the caller's declared ordering and values.
        // 自有参数保留调用方声明的顺序及值。
        let argv: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
        // The existing builder remains the sole command normalization authority.
        // 既有构造器保持命令规范化的唯一权威。
        let cmd = build_child_command(&program, &argv, cwd, env, inherit_environment);
        // sensitiveValues contains only exact injected values and is consumed solely by the stderr redactor.
        // sensitiveValues 仅包含精确注入值，并且只由 stderr 脱敏器消费。
        let mut sensitive_values = argv
            .iter()
            .chain(env.values())
            .filter(|value| !value.is_empty())
            .cloned()
            .collect::<Vec<_>>();
        sensitive_values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        sensitive_values.dedup();
        // The native diagnostic owner shares only sanitized bounded output with observers.
        // 原生诊断所有者仅向观察者共享有界脱敏输出。
        let recent_logs = Arc::new(Mutex::new(VecDeque::new()));
        // This clone moves into the native reader, which outlives the caller's executor when needed.
        // 此克隆移入原生读取者，必要时比调用方执行器存续更久。
        let capture_logs = Arc::clone(&recent_logs);
        // Process and all three native pipe owners exist before protocol initialization begins.
        // 协议初始化开始前，进程及三条原生管道的所有者均已建立。
        let transport = ManagedChildBackend::spawn_with_resources(cmd, resources, move |stderr| {
            spawn_sanitized_stderr_capture(stderr, sensitive_values, capture_logs)
        })
        .await
        .map_err(|error| McpRuntimeError::transport(format!("mcp.spawn: {error}")))?;
        // Preserve the actual backend close result independently of the official service join.
        // 独立于官方服务任务等待，保留实际后端关闭结果。
        let (transport, backend_close) = observe_backend_close(transport, "mcp.shutdown");
        // Bounded events belong to this transport's existing public notification channel.
        // 有界事件属于此传输既有公开通知通道。
        let (event_sender, _) = broadcast::channel(32);
        // The official service receives the existing host event handler.
        // 官方服务接收既有宿主事件处理器。
        let handler = Arc::new(EventingClientHandler::new(event_sender.clone()));
        // Initialization may be cancelled while actual native owners retain host leases.
        // 初始化可以取消，同时实际原生所有者保留宿主租约。
        let service = handler
            .serve(transport)
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.connect: {error}")))?;
        // The connected peer exposes protocol operations without taking native ownership.
        // 已连接对端暴露协议操作，不接管原生所有权。
        let peer = service.peer().clone();
        spawn_rmcp_disconnect_watch(
            endpoint.clone(),
            "stdio",
            peer.clone(),
            event_sender.clone(),
        );

        Ok(Self {
            endpoint,
            peer,
            shutdown: TransportShutdown::for_service("mcp.shutdown", service, backend_close),
            event_sender,
            recent_logs,
        })
    }

    /// Spawn one child program and connect over stdio.
    /// 启动一个子进程程序并通过 stdio 建立连接。
    ///
    /// # Parameters
    /// 参数
    /// `endpoint` identifies diagnostics, `program`, `args`, and `cwd` define the process,
    /// `env` contains explicit values, and `inherit_environment` controls visibility of all remaining host variables.
    /// `endpoint` 标识诊断，`program`、`args` 与 `cwd` 定义进程，
    /// `env` 包含显式值，`inherit_environment` 控制其余全部宿主变量的可见性。
    ///
    /// # Returns
    /// 返回值
    /// Returns a connected managed transport or the exact bounded launch/initialize failure.
    /// 返回已连接的受管传输，或精确且有限的启动/初始化失败。
    pub async fn spawn(
        endpoint: impl Into<String>,
        program: impl AsRef<std::ffi::OsStr>,
        args: &[&str],
        cwd: Option<&Path>,
        env: &BTreeMap<String, String>,
        inherit_environment: bool,
    ) -> Result<Self, McpRuntimeError> {
        // Ordinary launches follow the same actual resource ownership without plugin-specific leases.
        // 普通启动遵循相同实际资源所有权，不携带插件专用租约。
        let (resources, _) = crate::McpTransportResources::new(());
        Self::spawn_with_resources(
            endpoint,
            program,
            args,
            cwd,
            env,
            inherit_environment,
            resources,
        )
        .await
    }
}

/// Spawns a native bounded stderr reader that owns its actual resource independently of Tokio.
/// 启动原生有界 stderr 读取者，独立于 Tokio 持有其实际资源。
///
/// # Parameters
/// 参数
/// `stderr` is the owned reader, `sensitive_values` contains exact injected values, and `recent_logs` owns the bounded result.
/// `stderr` 是自有读取者，`sensitive_values` 包含精确注入值，`recent_logs` 持有有界结果。
/// Production passes the original ChildStderr; tests can also exercise actual file I/O errors.
/// 生产传入原 ChildStderr；测试也可验证实际文件输入输出错误。
/// Returns the original release receipt, or the native thread-creation error with no pipe handoff.
/// 返回原释放回执，或尚未移交管道时的原生线程创建错误。
fn spawn_sanitized_stderr_capture(
    mut stderr: impl Read + Send + 'static,
    sensitive_values: Vec<String>,
    recent_logs: Arc<Mutex<VecDeque<String>>>,
) -> io::Result<PipeCompletion> {
    // The sole native publisher retains terminal evidence even if every current observer leaves.
    // 即使当前全部观察者离开，唯一原生发布者仍保留终态证据。
    let (result, completion) = PipeCompletion::channel();
    std::thread::Builder::new()
        .name(String::from("mcp-stderr-owner"))
        .spawn(move || {
            // Buffer bounds each independent read without relying on an executor's blocking I/O task.
            // 缓冲限制每次独立读取，不依赖执行器的阻塞输入输出任务。
            let mut buffer = [0_u8; 4096];
            // Byte continuity prevents secrets split across reads from bypassing the existing redactor.
            // 字节连续性防止跨读取分割的秘密绕过既有脱敏器。
            let mut pending_line = Vec::new();
            // Oversized lines are discarded in full, preserving the same established framing contract.
            // 超长行被整体丢弃，保持同一既有分帧契约。
            let mut discarding_line = false;
            // Distinguish real EOF from read failure; failed partial lines must not be published as complete.
            // 区分真实结束与读取失败；失败的部分行不能作为完整行发布。
            let outcome = loop {
                // The worker retains the exact standard-library pipe through every blocking read.
                // 工作线程在每次阻塞读取期间保留精确标准库管道。
                let read = match stderr.read(&mut buffer) {
                    Ok(0) => {
                        consume_stderr_bytes(
                            &mut pending_line,
                            &mut discarding_line,
                            &[],
                            true,
                            &sensitive_values,
                            &recent_logs,
                        );
                        break Ok(());
                    }
                    Ok(read) => read,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => break Err(format!("MCP stderr read failed: {error}")),
                };
                consume_stderr_bytes(
                    &mut pending_line,
                    &mut discarding_line,
                    &buffer[..read],
                    false,
                    &sensitive_values,
                    &recent_logs,
                );
            };
            // Closing the actual OS pipe precedes success publication, not just the observer's completion.
            // 在发布成功前关闭实际操作系统管道，而非仅结束观察者。
            drop(stderr);
            result.send_replace(Some(outcome));
        })?;
    Ok(completion)
}

/// Consumes arbitrary stderr byte chunks while preserving line boundaries across reads.
/// 消费任意 stderr 字节分片，同时跨读取保留行边界。
///
/// # Parameters
/// 参数
/// `pending_line` and `discarding_line` own framing state; `bytes` is the next chunk and `eof` finalizes it.
/// `pending_line` 与 `discarding_line` 持有分帧状态；`bytes` 是下一分片，`eof` 负责结束处理。
/// `sensitive_values` owns exact redaction needles and `recent_logs` receives only sanitized complete lines.
/// `sensitive_values` 持有精确脱敏匹配值，`recent_logs` 仅接收已脱敏的完整行。
fn consume_stderr_bytes(
    pending_line: &mut Vec<u8>,
    discarding_line: &mut bool,
    bytes: &[u8],
    eof: bool,
    sensitive_values: &[String],
    recent_logs: &Arc<Mutex<VecDeque<String>>>,
) {
    for byte in bytes {
        if matches!(byte, b'\r' | b'\n') {
            finish_stderr_line(pending_line, discarding_line, sensitive_values, recent_logs);
            continue;
        }
        if *discarding_line {
            continue;
        }
        if pending_line.len() == MAX_STDERR_LINE_BYTES {
            pending_line.clear();
            *discarding_line = true;
            continue;
        }
        pending_line.push(*byte);
    }
    if eof && (*discarding_line || !pending_line.is_empty()) {
        finish_stderr_line(pending_line, discarding_line, sensitive_values, recent_logs);
    }
}

/// Finalizes one complete stderr line without ever emitting an oversized partial line.
/// 完成单条完整 stderr 行，且绝不输出超长行的局部内容。
///
/// # Parameters
/// 参数
/// Framing state selects either one fixed omission marker or one decoded and redacted complete line.
/// 分帧状态决定输出固定省略标记，或输出一条解码并脱敏后的完整行。
fn finish_stderr_line(
    pending_line: &mut Vec<u8>,
    discarding_line: &mut bool,
    sensitive_values: &[String],
    recent_logs: &Arc<Mutex<VecDeque<String>>>,
) {
    if *discarding_line {
        push_bounded_stderr_log(
            recent_logs,
            String::from("[stderr line omitted: exceeded 64 KiB]"),
        );
        pending_line.clear();
        *discarding_line = false;
        return;
    }
    if pending_line.is_empty() {
        return;
    }
    // decoded is created only after a complete bounded line is assembled.
    // decoded 仅在完整有界行组装完成后创建。
    let decoded = decode_shell_output_bytes(pending_line);
    let sanitized = redact_stderr_chunk(&decoded, sensitive_values);
    push_bounded_stderr_log(recent_logs, sanitized);
    pending_line.clear();
}

/// Replaces every exact injected value in one stderr chunk with a fixed redaction marker.
/// 将单个 stderr 分片中的每个精确注入值替换为固定脱敏标记。
///
/// # Parameters
/// 参数
/// `chunk` is decoded child output and `sensitive_values` contains exact argument and environment values.
/// `chunk` 是已解码子进程输出，`sensitive_values` 包含精确参数与环境变量值。
///
/// # Returns
/// 返回值
/// Returns a sanitized owned diagnostic chunk.
/// 返回脱敏后的自有诊断分片。
fn redact_stderr_chunk(chunk: &str, sensitive_values: &[String]) -> String {
    let mut sensitive_values = sensitive_values
        .iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    sensitive_values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    sensitive_values
        .into_iter()
        .fold(chunk.to_string(), |text, value| {
            text.replace(value, "[REDACTED]")
        })
}

/// Appends one sanitized stderr chunk while enforcing both entry and byte limits.
/// 追加单个脱敏 stderr 分片，同时强制执行条目与字节双重上限。
///
/// # Parameters
/// 参数
/// `recent_logs` owns the shared ring and `entry` is one already sanitized chunk.
/// `recent_logs` 持有共享环形缓冲，`entry` 是一个已经脱敏的分片。
fn push_bounded_stderr_log(recent_logs: &Arc<Mutex<VecDeque<String>>>, entry: String) {
    let mut logs = recent_logs
        .lock()
        .expect("stdio MCP diagnostic log buffer poisoned");
    logs.push_back(entry);
    while logs.len() > MAX_STDERR_LOG_ENTRIES
        || logs.iter().map(String::len).sum::<usize>() > MAX_STDERR_LOG_BYTES
    {
        logs.pop_front();
    }
}

/// Build one child-process command with optional working directory and environment overrides.
/// 构建一个带可选工作目录与环境变量覆盖的子进程命令。
///
/// # Parameters
/// 参数
/// `program`, `argv`, and `cwd` define the child, `env` contains explicit variables,
/// and `inherit_environment` decides whether the remaining host environment is cleared.
/// `program`、`argv` 与 `cwd` 定义子进程，`env` 包含显式变量，
/// `inherit_environment` 决定是否清除其余宿主环境。
///
/// # Returns
/// 返回值
/// Returns one configured Tokio command without spawning it.
/// 返回一条尚未启动的已配置 Tokio 命令。
fn build_child_command(
    program: &std::ffi::OsStr,
    argv: &[String],
    cwd: Option<&Path>,
    env: &BTreeMap<String, String>,
    inherit_environment: bool,
) -> Command {
    let mut command = Command::new(vulcan_runtime_assets::native_host_path(Path::new(program)));
    command.args(argv);
    if let Some(cwd) = cwd {
        command.current_dir(vulcan_runtime_assets::native_host_path(cwd));
    }
    if !inherit_environment {
        command.env_clear();
    }
    command.envs(env);
    apply_windows_no_window(&mut command);
    // ManagedChild transfers this standard command to a native process owner before initialization.
    // ManagedChild 在初始化前将此标准命令移交给原生进程所有者。
    command
}

#[async_trait]
impl McpTransport for StdioTransport {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn event_receiver(&self) -> broadcast::Receiver<McpTransportEvent> {
        self.event_sender.subscribe()
    }

    fn recent_logs(&self) -> Vec<String> {
        self.recent_logs
            .lock()
            .expect("stdio MCP diagnostic log buffer poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Returns the exact server identity negotiated by rmcp during initialize.
    /// 返回 rmcp 在 initialize 阶段协商得到的精确服务器身份。
    fn server_identity(&self) -> Option<McpServerIdentity> {
        self.peer.peer_info().and_then(|info| {
            info.server_info
                .as_ref()
                .map(|server_info| McpServerIdentity {
                    protocol_version: info.protocol_version.to_string(),
                    product: server_info.name.clone(),
                    version: server_info.version.clone(),
                })
        })
    }

    /// Observes the original rmcp close operation without cancelling it on timeout.
    /// 观测原 rmcp 关闭操作，不因观测超时而取消清理。
    /// Returns protocol, process-scope and all three actual pipe receipts; Unix descendants remain incomplete.
    /// 返回协议、进程范围及三条实际管道回执；Unix 后代仍待完整接入。
    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        self.shutdown.wait().await
    }

    /// Waits for the original rmcp close result while the lifecycle owner retains this transport.
    /// 生命周期所有者保留当前传输期间，等待原 rmcp 关闭结果。
    /// No inputs; returns terminal close success or failure without an observer timeout.
    /// 无输入；返回终态关闭成功或失败，不返回观察者超时。
    async fn shutdown_and_wait(&self) -> Result<(), McpRuntimeError> {
        self.shutdown.wait_until_complete().await
    }

    async fn list_tools(&self) -> Result<Vec<McpToolSchema>, McpRuntimeError> {
        list_rmcp_tools(&self.peer, "stdio").await
    }

    async fn list_prompts(&self) -> Result<Vec<Prompt>, McpRuntimeError> {
        list_rmcp_prompts(&self.peer, "stdio").await
    }

    async fn get_prompt(
        &self,
        name: &str,
        args: Value,
    ) -> Result<GetPromptResult, McpRuntimeError> {
        get_rmcp_prompt(&self.peer, "stdio", name, args).await
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpRuntimeError> {
        list_rmcp_resources(&self.peer, "stdio").await
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, McpRuntimeError> {
        read_rmcp_resource(&self.peer, "stdio", uri).await
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<Value, McpRuntimeError> {
        call_rmcp_tool(&self.peer, "stdio", name, args).await
    }

    /// Projects one completed call for its declared consumer without altering transport behavior.
    /// 为声明消费者投影一次已完成调用，不改变传输行为。
    async fn call_tool_with_audience(
        &self,
        name: &str,
        args: Value,
        audience: crate::McpToolResultAudience,
    ) -> Result<Value, McpRuntimeError> {
        crate::rmcp_support::call_rmcp_tool_with_audience(&self.peer, "stdio", name, args, audience)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_STDERR_LINE_BYTES, MAX_STDERR_LOG_BYTES, MAX_STDERR_LOG_ENTRIES, build_child_command,
        consume_stderr_bytes, push_bounded_stderr_log, redact_stderr_chunk,
    };
    use std::collections::{BTreeMap, VecDeque};
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    #[test]
    fn child_command_builder_preserves_args_cwd_and_env() {
        let cwd = PathBuf::from("D:/mock/mcp");
        let argv = vec![String::from("--flag"), String::from("value")];
        let mut env = BTreeMap::new();
        env.insert(String::from("VULCAN_MCP_MODE"), String::from("stdio"));

        let command = build_child_command(
            OsStr::new("mock-mcp"),
            &argv,
            Some(cwd.as_path()),
            &env,
            false,
        );
        let std_command = command.as_std();
        let args = std_command
            .get_args()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let env_entries = std_command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|entry| entry.to_string_lossy().to_string()),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(std_command.get_program(), OsStr::new("mock-mcp"));
        assert_eq!(args, vec![String::from("--flag"), String::from("value")]);
        assert_eq!(std_command.get_current_dir(), Some(cwd.as_path()));
        assert_eq!(
            env_entries,
            vec![(String::from("VULCAN_MCP_MODE"), Some(String::from("stdio")),)]
        );
    }

    /// Verifies Windows verbatim paths are converted before crossing the MCP child-process boundary.
    /// 验证 Windows 逐字路径在跨越 MCP 子进程边界前会转换为标准路径。
    #[cfg(windows)]
    #[test]
    fn child_command_builder_normalizes_windows_verbatim_paths() {
        let cwd = PathBuf::from(r"\\?\D:\openvulcan\workspace");
        let command = build_child_command(
            OsStr::new(r"\\?\D:\openvulcan\runtime\node.exe"),
            &[],
            Some(cwd.as_path()),
            &BTreeMap::new(),
            false,
        );
        let std_command = command.as_std();

        assert_eq!(
            std_command.get_program(),
            OsStr::new(r"D:\openvulcan\runtime\node.exe")
        );
        assert_eq!(
            std_command.get_current_dir(),
            Some(std::path::Path::new(r"D:\openvulcan\workspace"))
        );
    }

    /// Verifies exact injected argument and environment values are removed before stderr diagnostics are retained.
    /// 验证精确注入的参数与环境变量值会在保留 stderr 诊断前被移除。
    #[test]
    fn stderr_redaction_removes_exact_injected_values() {
        let sanitized = redact_stderr_chunk(
            "authorization=token-secret argument=--api-key=argument-secret",
            &[
                String::from("secret"),
                String::from("token-secret"),
                String::from("--api-key=argument-secret"),
            ],
        );
        assert_eq!(sanitized, "authorization=[REDACTED] argument=[REDACTED]");
    }

    /// Verifies stderr framing redacts a Secret split across arbitrary child-pipe reads.
    /// 验证 stderr 分帧会脱敏跨任意子进程管道读取分割的 Secret。
    #[test]
    fn stderr_framing_redacts_secrets_split_across_reads() {
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        let secrets = vec![String::from("token-secret")];
        let mut pending = Vec::new();
        let mut discarding = false;
        consume_stderr_bytes(
            &mut pending,
            &mut discarding,
            b"authorization=token-se",
            false,
            &secrets,
            &logs,
        );
        consume_stderr_bytes(
            &mut pending,
            &mut discarding,
            b"cret\n",
            false,
            &secrets,
            &logs,
        );
        assert_eq!(
            logs.lock()
                .expect("lock framed logs")
                .front()
                .map(String::as_str),
            Some("authorization=[REDACTED]")
        );
    }

    /// Verifies an oversized unterminated line is discarded in full so no Secret suffix can leak.
    /// 验证超长未终止行会被整体丢弃，确保不会泄漏 Secret 后缀。
    #[test]
    fn stderr_framing_discards_entire_oversized_line() {
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        let secrets = vec![String::from("token-secret")];
        let mut pending = Vec::new();
        let mut discarding = false;
        let mut oversized = vec![b'x'; MAX_STDERR_LINE_BYTES];
        oversized.extend_from_slice(b"token-secret\nvisible\n");
        consume_stderr_bytes(
            &mut pending,
            &mut discarding,
            &oversized,
            false,
            &secrets,
            &logs,
        );
        assert_eq!(
            logs.lock()
                .expect("lock oversized logs")
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                String::from("[stderr line omitted: exceeded 64 KiB]"),
                String::from("visible"),
            ]
        );
    }

    /// Verifies the stderr diagnostic ring enforces both its entry and total-byte limits.
    /// 验证 stderr 诊断环形缓冲会同时强制执行条目与总字节上限。
    #[test]
    fn stderr_log_ring_is_bounded_by_entries_and_bytes() {
        let logs = Arc::new(Mutex::new(VecDeque::new()));
        for index in 0..=MAX_STDERR_LOG_ENTRIES {
            push_bounded_stderr_log(&logs, format!("entry-{index}"));
        }
        {
            let retained = logs.lock().expect("lock stderr fixture");
            assert_eq!(retained.len(), MAX_STDERR_LOG_ENTRIES);
            assert_eq!(retained.front().map(String::as_str), Some("entry-1"));
        }
        for _ in 0..20 {
            push_bounded_stderr_log(&logs, "x".repeat(4096));
        }
        let retained = logs.lock().expect("lock byte-bounded stderr fixture");
        assert!(retained.len() <= MAX_STDERR_LOG_ENTRIES);
        assert!(retained.iter().map(String::len).sum::<usize>() <= MAX_STDERR_LOG_BYTES);
    }
}
