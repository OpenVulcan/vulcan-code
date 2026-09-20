//! Stdio MCP client transport backed by the official `rmcp` SDK.
//! 基于官方 `rmcp` SDK 的 stdio MCP 客户端传输。

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rmcp::model::{GetPromptResult, Prompt, ReadResourceResult, Resource};
use rmcp::service::{Peer, RoleClient, RunningService, ServiceExt};
use rmcp::transport::TokioChildProcess;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::ChildStderr;
use tokio::process::Command;
use tokio::sync::{Mutex as AsyncMutex, broadcast};
use vulcan_host_tools::{
    output_decode::decode_shell_output_bytes, process_creation::apply_windows_no_window,
};

use crate::error::McpRuntimeError;
use crate::rmcp_support::{
    EventingClientHandler, call_rmcp_tool, get_rmcp_prompt, list_rmcp_prompts, list_rmcp_resources,
    list_rmcp_tools, read_rmcp_resource, spawn_rmcp_disconnect_watch,
};
use crate::transport::{McpServerIdentity, McpToolSchema, McpTransport, McpTransportEvent};

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
    /// Keeps the rmcp service alive for the lifetime of the transport.
    /// 在传输生命周期内保持 rmcp 服务存活。
    service: AsyncMutex<Option<RunningService<RoleClient, Arc<EventingClientHandler>>>>,
    /// Broadcast channel used to surface tool refresh and disconnect events.
    /// 用于对外广播工具刷新与断连事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
    /// Bounded sanitized stderr diagnostics retained for host inspection.
    /// 为宿主检查保留的有界脱敏 stderr 诊断。
    recent_logs: Arc<Mutex<VecDeque<String>>>,
}

impl StdioTransport {
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
        let endpoint = endpoint.into();
        let program = program.as_ref().to_owned();
        let argv: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
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
        let (transport, stderr) = TokioChildProcess::builder(cmd)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| McpRuntimeError::transport(format!("mcp.spawn: {error}")))?;
        // recentLogs is shared with the non-blocking stderr capture task and diagnostic readers.
        // recentLogs 与非阻塞 stderr 捕获任务及诊断读取方共享。
        let recent_logs = Arc::new(Mutex::new(VecDeque::new()));
        if let Some(stderr) = stderr {
            spawn_sanitized_stderr_capture(stderr, sensitive_values, Arc::clone(&recent_logs));
        }
        let (event_sender, _) = broadcast::channel(32);
        let handler = Arc::new(EventingClientHandler::new(event_sender.clone()));
        let service = handler
            .serve(transport)
            .await
            .map_err(|error| McpRuntimeError::transport(format!("mcp.connect: {error}")))?;
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
            service: AsyncMutex::new(Some(service)),
            event_sender,
            recent_logs,
        })
    }
}

/// Spawns a non-blocking bounded stderr capture task for one managed MCP child process.
/// 为单个受管 MCP 子进程启动非阻塞且有界的 stderr 捕获任务。
///
/// # Parameters
/// 参数
/// `stderr` is the child pipe, `sensitive_values` contains exact injected values, and `recent_logs` owns the bounded result.
/// `stderr` 是子进程管道，`sensitive_values` 包含精确注入值，`recent_logs` 持有有界结果。
fn spawn_sanitized_stderr_capture(
    mut stderr: ChildStderr,
    sensitive_values: Vec<String>,
    recent_logs: Arc<Mutex<VecDeque<String>>>,
) {
    tokio::spawn(async move {
        // buffer bounds every individual read so an unterminated child message cannot cause unbounded allocation.
        // buffer 限制每次独立读取，避免未终止的子进程消息导致无界分配。
        let mut buffer = [0_u8; 4096];
        // pending_line retains complete byte continuity so secrets split across reads cannot bypass redaction.
        // pending_line 保留完整字节连续性，防止跨读取分割的 Secret 绕过脱敏。
        let mut pending_line = Vec::new();
        // discarding_line suppresses every byte of one oversized line instead of exposing a partial suffix.
        // discarding_line 抑制单条超长行的全部字节，避免暴露部分后缀。
        let mut discarding_line = false;
        loop {
            let read = match stderr.read(&mut buffer).await {
                Ok(0) | Err(_) => {
                    consume_stderr_bytes(
                        &mut pending_line,
                        &mut discarding_line,
                        &[],
                        true,
                        &sensitive_values,
                        &recent_logs,
                    );
                    break;
                }
                Ok(read) => read,
            };
            consume_stderr_bytes(
                &mut pending_line,
                &mut discarding_line,
                &buffer[..read],
                false,
                &sensitive_values,
                &recent_logs,
            );
        }
    });
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
    // The command-level drop guard stops a child if initialize is cancelled before rmcp
    // returns a transport to the runtime owner.
    // 命令级析构保护在 initialize 返回传输给运行时所有者之前被取消时终止子进程。
    command.kill_on_drop(true);
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

    /// Gracefully closes rmcp, waits for child exit, and lets rmcp force-kill after its bounded grace period.
    /// 优雅关闭 rmcp、等待子进程退出，并允许 rmcp 在有限宽限期后强制终止。
    async fn shutdown(&self) -> Result<(), McpRuntimeError> {
        // service is taken exactly once so concurrent shutdown requests cannot race child cleanup.
        // service 仅被取出一次，避免并发关闭请求竞争子进程清理。
        let service = self.service.lock().await.take();
        let Some(mut service) = service else {
            return Ok(());
        };
        match tokio::time::timeout(std::time::Duration::from_secs(5), service.close()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(McpRuntimeError::transport(format!(
                "mcp.shutdown.join: {error}"
            ))),
            Err(_) => Err(McpRuntimeError::transport(
                "mcp.shutdown timed out after 5 seconds",
            )),
        }
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
