//! Official MCP framing with a separately supervised actual child process.
//! 官方 MCP 分帧与独立监督的实际子进程。

use super::child::ManagedChild;
use super::native_pipe::{NativePipeReader, NativePipeWriter};
use super::pipe_completion::PipeCompletion;
use rmcp::RoleClient;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::{Transport, async_rw::AsyncRwTransport};
use std::io;
use std::process::{ChildStderr, ChildStdin, ChildStdout};
use tokio::process::Command;

#[cfg(test)]
mod tests;

/// The official protocol transport operates on adapters that retain exact native ownership.
/// 官方协议传输工作于保留精确原生所有权的适配器。
type NativeProtocolWire = AsyncRwTransport<
    RoleClient,
    NativePipeReader<crate::resources::RetainedResource<ChildStdout>>,
    NativePipeWriter<crate::resources::RetainedResource<ChildStdin>>,
>;

/// Delegates protocol behavior while native ownership survives cancellation and executor loss.
/// 委托协议行为，同时使原生所有权在取消及执行器丢失后存续。
pub(super) struct ManagedChildBackend {
    /// The official codec owns the exact transferred protocol pipes.
    /// 官方编解码器拥有精确移交的协议管道。
    /// Close takes this value so pending OS operations become the final pipe owners.
    /// 关闭取走此值，使待完成操作系统操作成为最终管道所有者。
    wire: Option<NativeProtocolWire>,
    /// Dropping the protocol backend requests cleanup by the independent actual process owner.
    /// 释放协议后端时，请求独立实际进程所有者清理。
    child: ManagedChild,
    /// Final input-handle release is independent from writer-future completion.
    /// 最终输入句柄释放独立于写入 future 完成。
    stdin: PipeCompletion,
    /// Final output-handle release is independent from reader-future completion.
    /// 最终输出句柄释放独立于读取 future 完成。
    stdout: PipeCompletion,
    /// The independent diagnostic worker confirms its actual pipe release after native EOF or failure.
    /// 独立诊断工作线程在原生结束或失败后确认实际管道释放。
    stderr: PipeCompletion,
}

impl ManagedChildBackend {
    /// Transfers one launch's host leases into every actual process and pipe owner.
    /// 将一次启动的宿主租约移交给每个实际进程及管道所有者。
    /// Command and resources identify exact authority; capture_stderr consumes the lease-bound original pipe.
    /// command 与 resources 标识精确权威；capture_stderr 消费绑定租约的原管道。
    /// Returns the official codec backend after ownership transfer, or the original launch failure.
    /// 所有权移交后返回官方编解码后端，或返回原启动失败。
    pub(super) async fn spawn_with_resources(
        command: Command,
        resources: crate::McpTransportResources,
        capture_stderr: impl FnOnce(
            crate::resources::RetainedResource<ChildStderr>,
        ) -> io::Result<PipeCompletion>
        + Send,
    ) -> io::Result<Self> {
        // Fail before spawning when no explicit Tokio runtime can execute protocol operations.
        // 没有明确 Tokio 运行时可执行协议操作时，在启动进程前失败。
        let runtime = tokio::runtime::Handle::try_current().map_err(|error| {
            io::Error::other(format!(
                "MCP protocol pipes require a Tokio runtime: {error}"
            ))
        })?;
        // Native process ownership precedes every protocol and diagnostic resource handoff.
        // 原生进程所有权先于每次协议及诊断资源移交。
        let (child, pipes) = ManagedChild::spawn_with_resources(command, resources).await?;
        // Each conversion consumes precisely the handle delivered by the original child.
        // 每次转换精确消费原子进程交付的句柄。
        let (stdin, stdin_completion) =
            NativePipeWriter::new(pipes.resources.retain(pipes.stdin), runtime.clone());
        // The official reader keeps framing and compatibility behavior unchanged.
        // 官方读取者保持分帧与兼容行为不变。
        let (stdout, stdout_completion) =
            NativePipeReader::new(pipes.resources.retain(pipes.stdout), runtime);
        // The native diagnostic worker consumes the original handle, without a Tokio I/O wrapper.
        // 原生诊断工作线程消费原句柄，不经过 Tokio 输入输出包装。
        let stderr = capture_stderr(pipes.resources.retain(pipes.stderr))?;
        Ok(Self {
            wire: Some(AsyncRwTransport::new(stdout, stdin)),
            stdin: stdin_completion,
            stdout: stdout_completion,
            child,
            stderr,
        })
    }

    /// Launches the exact command and transfers diagnostic ownership before exposing the backend.
    /// 启动精确命令，并在暴露后端前移交诊断所有权。
    /// Command supplies launch authority; capture_stderr consumes the original pipe and returns its release receipt.
    /// command 提供启动权威；capture_stderr 消费原管道并返回其释放回执。
    /// Returns the backend only after all owners exist; failures retain native cleanup of any spawned child.
    /// 仅在全部所有者存在后返回后端；失败时已启动子进程保留原生清理。
    #[cfg(test)]
    pub(super) async fn spawn(
        command: Command,
        capture_stderr: impl FnOnce(
            crate::resources::RetainedResource<ChildStderr>,
        ) -> io::Result<PipeCompletion>
        + Send,
    ) -> io::Result<Self> {
        // Test fixtures use the same real ownership path with no external host resources.
        // 测试夹具使用相同真实所有权路径，不携带外部宿主资源。
        let (resources, _) = crate::McpTransportResources::new(());
        Self::spawn_with_resources(command, resources, capture_stderr).await
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
        // The official send future retains its own writer slot, preserving existing framing behavior.
        // 官方发送 future 保留自己的写入槽位，保持既有分帧行为。
        let send = self.wire.as_mut().map(|wire| wire.send(item));
        async move {
            match send {
                Some(send) => send.await,
                None => Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "MCP protocol transport is closed.",
                )),
            }
        }
    }

    /// Receives one official message or EOF without introducing another protocol parser.
    /// 接收单个官方消息或文件结束，不引入另一套协议解析器。
    /// No inputs; returns the original decoder's next message.
    /// 无输入；返回原解码器的下一消息。
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        match self.wire.as_mut() {
            Some(wire) => wire.receive().await,
            None => None,
        }
    }

    /// Closes protocol output and waits for actual platform process-scope exit under the same native owner.
    /// 关闭协议输出，并在同一原生所有者下等待实际平台进程范围退出。
    /// No inputs; success requires process-scope exit and the original release receipts for all three pipes.
    /// 无输入；成功要求进程范围退出及全部三条管道的原释放回执。
    async fn close(&mut self) -> io::Result<()> {
        self.child.start_close();
        // The native grace timer already runs while close waits for a sender holding the writer lock.
        // 关闭等待持有写入锁的发送方期间，原生宽限计时已经运行。
        let wire = match self.wire.as_mut() {
            Some(wire) => wire.close().await,
            None => Ok(()),
        };
        // Release adapter references before awaiting receipts; in-flight operations retain actual handles.
        // 等待回执前释放适配器引用；在途操作保留实际句柄。
        drop(self.wire.take());
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
        self.stdin.wait().await?;
        self.stdout.wait().await?;
        self.stderr.wait().await
    }
}
