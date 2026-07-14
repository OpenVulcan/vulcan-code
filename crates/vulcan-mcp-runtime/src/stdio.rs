//! Stdio MCP client transport backed by the official `rmcp` SDK.
//! 基于官方 `rmcp` SDK 的 stdio MCP 客户端传输。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{GetPromptResult, Prompt, ReadResourceResult, Resource};
use rmcp::service::{Peer, RoleClient, RunningService, ServiceExt};
use rmcp::transport::TokioChildProcess;
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::broadcast;
use vulcan_host_tools::process_creation::apply_windows_release_no_window;

use crate::error::McpRuntimeError;
use crate::rmcp_support::{
    EventingClientHandler, call_rmcp_tool, get_rmcp_prompt, list_rmcp_prompts, list_rmcp_resources,
    list_rmcp_tools, read_rmcp_resource, spawn_rmcp_disconnect_watch,
};
use crate::transport::{McpToolSchema, McpTransport, McpTransportEvent};

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
    _service: Arc<RunningService<RoleClient, Arc<EventingClientHandler>>>,
    /// Broadcast channel used to surface tool refresh and disconnect events.
    /// 用于对外广播工具刷新与断连事件的广播通道。
    event_sender: broadcast::Sender<McpTransportEvent>,
}

impl StdioTransport {
    /// Spawn one child program and connect over stdio.
    /// 启动一个子进程程序并通过 stdio 建立连接。
    pub async fn spawn(
        endpoint: impl Into<String>,
        program: impl AsRef<std::ffi::OsStr>,
        args: &[&str],
        cwd: Option<&Path>,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, McpRuntimeError> {
        let endpoint = endpoint.into();
        let program = program.as_ref().to_owned();
        let argv: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
        let cmd = build_child_command(&program, &argv, cwd, env);
        let transport = TokioChildProcess::new(cmd)
            .map_err(|error| McpRuntimeError::transport(format!("mcp.spawn: {error}")))?;
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
            _service: Arc::new(service),
            event_sender,
        })
    }
}

/// Build one child-process command with optional working directory and environment overrides.
/// 构建一个带可选工作目录与环境变量覆盖的子进程命令。
fn build_child_command(
    program: &std::ffi::OsStr,
    argv: &[String],
    cwd: Option<&Path>,
    env: &BTreeMap<String, String>,
) -> Command {
    let mut command = Command::new(vulcan_runtime_assets::native_host_path(Path::new(program)));
    command.args(argv);
    if let Some(cwd) = cwd {
        command.current_dir(vulcan_runtime_assets::native_host_path(cwd));
    }
    if !env.is_empty() {
        command.envs(env);
    }
    apply_windows_release_no_window(&mut command);
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
    use super::build_child_command;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn child_command_builder_preserves_args_cwd_and_env() {
        let cwd = PathBuf::from("D:/mock/mcp");
        let argv = vec![String::from("--flag"), String::from("value")];
        let mut env = BTreeMap::new();
        env.insert(String::from("VULCAN_MCP_MODE"), String::from("stdio"));

        let command = build_child_command(OsStr::new("mock-mcp"), &argv, Some(cwd.as_path()), &env);
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
}
