//! Official MCP framing with a separately supervised actual child process.
//! 官方 MCP 分帧与独立监督的实际子进程。

use super::child::ManagedChild;
use super::pipe_completion::PipeCompletion;
use rmcp::RoleClient;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::{Transport, async_rw::AsyncRwTransport};
use std::io;
use std::process::ChildStderr;
use tokio::process::{ChildStdin, ChildStdout, Command};

/// Delegates protocol behavior while native ownership survives cancellation and executor loss.
/// 委托协议行为，同时使原生所有权在取消及执行器丢失后存续。
pub(super) struct ManagedChildBackend {
    /// The official codec owns the exact transferred protocol pipes.
    /// 官方编解码器拥有精确移交的协议管道。
    wire: AsyncRwTransport<RoleClient, ChildStdout, ChildStdin>,
    /// Dropping the protocol backend requests cleanup by the independent actual process owner.
    /// 释放协议后端时，请求独立实际进程所有者清理。
    child: ManagedChild,
    /// The independent diagnostic worker confirms its actual pipe release after native EOF or failure.
    /// 独立诊断工作线程在原生结束或失败后确认实际管道释放。
    stderr: PipeCompletion,
}

impl ManagedChildBackend {
    /// Launches the exact command and transfers diagnostic ownership before exposing the backend.
    /// 启动精确命令，并在暴露后端前移交诊断所有权。
    /// Command supplies launch authority; capture_stderr consumes the original pipe and returns its release receipt.
    /// command 提供启动权威；capture_stderr 消费原管道并返回其释放回执。
    /// Returns the backend only after all owners exist; failures retain native cleanup of any spawned child.
    /// 仅在全部所有者存在后返回后端；失败时已启动子进程保留原生清理。
    pub(super) async fn spawn(
        command: Command,
        capture_stderr: impl FnOnce(ChildStderr) -> io::Result<PipeCompletion> + Send,
    ) -> io::Result<Self> {
        // Native ownership is established before any fallible pipe conversion.
        // 任何可能失败的管道转换前，先建立原生所有权。
        let (child, pipes) = ManagedChild::spawn(command).await?;
        // Each conversion consumes precisely the handle delivered by the original child.
        // 每次转换精确消费原子进程交付的句柄。
        let stdin = ChildStdin::from_std(pipes.stdin)?;
        // The official reader keeps framing and compatibility behavior unchanged.
        // 官方读取者保持分帧与兼容行为不变。
        let stdout = ChildStdout::from_std(pipes.stdout)?;
        // The native diagnostic worker consumes the original handle, without a Tokio I/O wrapper.
        // 原生诊断工作线程消费原句柄，不经过 Tokio 输入输出包装。
        let stderr = capture_stderr(pipes.stderr)?;
        Ok(Self {
            wire: AsyncRwTransport::new(stdout, stdin),
            child,
            stderr,
        })
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
    /// No inputs; successful protocol and process cleanup additionally requires actual stderr release.
    /// 无输入；协议及进程清理成功后，还必须等待实际标准错误释放。
    async fn close(&mut self) -> io::Result<()> {
        self.child.start_close();
        // The native grace timer already runs while close waits for a sender holding the writer lock.
        // 关闭等待持有写入锁的发送方期间，原生宽限计时已经运行。
        let wire = self.wire.close().await;
        // Waiting cannot cancel native ownership; backend destruction escalates the same scope.
        // 等待不能取消原生所有权；后端销毁升级相同范围的停止请求。
        let child = self.child.close().await;
        match (wire, child) {
            (Ok(()), Ok(())) => {}
            (Err(error), Ok(())) | (Ok(()), Err(error)) => return Err(error),
            (Err(wire), Err(child)) => {
                return Err(io::Error::other(format!(
                    "Protocol close failed: {wire}; child cleanup failed: {child}"
                )));
            }
        }
        // Native reader errors cannot become EOF success; cancelled observers retain the same receipt.
        // 原生读取错误不能变成结束成功；取消观察后仍保留相同回执。
        self.stderr.wait().await
    }
}
