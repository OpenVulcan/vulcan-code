//! Official MCP framing with a separately supervised actual child process.
//! 官方 MCP 分帧与独立监督的实际子进程。

use super::child::ManagedChild;
use rmcp::RoleClient;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::{Transport, async_rw::AsyncRwTransport};
use std::io;
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};

/// Delegates protocol behavior while native ownership survives cancellation and executor loss.
/// 委托协议行为，同时使原生所有权在取消及执行器丢失后存续。
pub(super) struct ManagedChildBackend {
    /// The official codec owns the exact transferred protocol pipes.
    /// 官方编解码器拥有精确移交的协议管道。
    wire: AsyncRwTransport<RoleClient, ChildStdout, ChildStdin>,
    /// Dropping the protocol backend requests cleanup by the independent actual process owner.
    /// 释放协议后端时，请求独立实际进程所有者清理。
    child: ManagedChild,
}

impl ManagedChildBackend {
    /// Launches the exact command and converts its owned pipes using Tokio's supported adapters.
    /// 启动精确命令，并使用 Tokio 支持的适配器转换其自有管道。
    /// Command supplies all launch authority; errors retain independent cleanup of any spawned child.
    /// command 提供全部启动权威；出错后已启动子进程仍保留独立清理。
    pub(super) async fn spawn(command: Command) -> io::Result<(Self, ChildStderr)> {
        // Native ownership is established before any fallible pipe conversion.
        // 任何可能失败的管道转换前，先建立原生所有权。
        let (child, pipes) = ManagedChild::spawn(command).await?;
        // Each conversion consumes precisely the handle delivered by the original child.
        // 每次转换精确消费原子进程交付的句柄。
        let stdin = ChildStdin::from_std(pipes.stdin)?;
        // The official reader keeps framing and compatibility behavior unchanged.
        // 官方读取者保持分帧与兼容行为不变。
        let stdout = ChildStdout::from_std(pipes.stdout)?;
        // Stderr stays outside the protocol stream and follows the existing redaction path.
        // 标准错误保持在协议流之外，沿用既有脱敏路径。
        let stderr = ChildStderr::from_std(pipes.stderr)?;
        Ok((
            Self {
                wire: AsyncRwTransport::new(stdout, stdin),
                child,
            },
            stderr,
        ))
    }
}

impl Transport<RoleClient> for ManagedChildBackend {
    /// Preserve the concrete I/O error boundary expected by the official client.
    /// 保持官方客户端预期的具体输入输出错误边界。
    type Error = io::Error;

    /// Sends one official message through its original framed writer; returns the original result.
    /// 通过原分帧写入者发送单个官方消息；返回原结果。
    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        self.wire.send(item)
    }

    /// Receives one official message or EOF without introducing another protocol parser.
    /// 接收单个官方消息或文件结束，不引入另一套协议解析器。
    /// No inputs; returns the original decoder's next message.
    /// 无输入；返回原解码器的下一消息。
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.wire.receive().await
    }

    /// Closes protocol output and waits for actual platform process-scope exit under the same native owner.
    /// 关闭协议输出，并在同一原生所有者下等待实际平台进程范围退出。
    /// No inputs; preserves both protocol and process cleanup diagnostics on failure.
    /// 无输入；失败时保留协议及进程清理诊断。
    async fn close(&mut self) -> io::Result<()> {
        self.child.start_close();
        // The timer is already running if flushing the protocol pipe needs forced child exit.
        // 协议管道刷新需要强制子进程退出时，计时已经开始。
        let wire = self.wire.close().await;
        // Waiting cannot cancel the native operation; dropping this backend escalates the same child.
        // 等待不能取消原生操作；释放此后端会升级相同子进程的停止请求。
        let child = self.child.close().await;
        match (wire, child) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(wire), Err(child)) => Err(io::Error::other(format!(
                "Protocol close failed: {wire}; child cleanup failed: {child}"
            ))),
        }
    }
}
