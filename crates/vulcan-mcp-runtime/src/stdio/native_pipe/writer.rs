//! Bounded writes retain native ownership after accepted bytes outlive their asynchronous sender.
//! 有界写入在已接收字节比异步发送方存续更久时仍保留原生所有权。

use super::{PIPE_IO_CHUNK_BYTES, PipeCompletion, PipeFailure, PipeOwner};
use std::{
    future::Future,
    io::{self, Write},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};
use tokio::{io::AsyncWrite, runtime::Handle, task::JoinHandle};

/// One retained final-release wait makes repeated shutdown polls cancellation-safe.
/// 一个保留的最终释放等待，使重复关闭轮询取消安全。
type ReleaseWait = Pin<Box<dyn Future<Output = io::Result<()>> + Send>>;

/// Retains scheduling-failure evidence even when both the adapter and its observer disappear.
/// 即使适配器及观察者均消失，仍保留调度失败证据。
struct QueuedWrite<W> {
    /// The queued closure owns the exact native resource until execution or actual disposal.
    /// 排队闭包拥有精确原生资源，直至执行或实际销毁。
    owner: Arc<PipeOwner<W>>,
    /// Only entry into the blocking closure disarms the unexecuted-operation failure.
    /// 仅进入阻塞闭包才解除未执行操作失败标记。
    entered: bool,
}

impl<W> QueuedWrite<W> {
    /// Creates an armed operation before handing its closure to the runtime.
    /// 把闭包交给运行时前，创建已启用监测的操作。
    /// Owner identifies the original pipe; returns ownership with cancellation evidence enabled.
    /// owner 标识原管道；返回已启用取消证据的所有权。
    fn new(owner: Arc<PipeOwner<W>>) -> Self {
        Self {
            owner,
            entered: false,
        }
    }

    /// Records actual entry and executes the supplied native write or flush exactly once.
    /// 记录实际进入，并且仅一次执行给定原生写入或刷新。
    /// Operation borrows the original writer; returns its native result with failure retained by the owner.
    /// operation 借用原写入者；返回原生结果，失败由所有者保留。
    fn run(mut self, operation: impl FnOnce(&mut W) -> io::Result<()>) -> io::Result<()> {
        self.entered = true;
        self.owner.with_io(operation)
    }
}

impl<W> Drop for QueuedWrite<W> {
    /// Marks rejected or discarded work before releasing its native ownership.
    /// 释放原生所有权之前，标记被拒绝或丢弃的工作。
    /// No inputs or return; started operations retain their own actual I/O or poison evidence.
    /// 无输入或返回；已开始操作保留自身实际输入输出或中毒证据。
    fn drop(&mut self) {
        if !self.entered {
            self.owner.record_failure(String::from(
                "MCP native write operation was discarded before execution.",
            ));
        }
    }
}

/// Accepts at most one bounded chunk while the original native writer completes its operation.
/// 原原生写入者完成操作期间，最多接收一个有界分片。
pub(in crate::stdio) struct NativePipeWriter<W> {
    /// Shutdown takes this reference; an already-running operation retains its own exact owner.
    /// 关闭取走此引用；已运行操作保留自己的精确所有者。
    owner: Option<Arc<PipeOwner<W>>>,
    /// Blocking operations use the explicit launch runtime's existing worker capacity.
    /// 阻塞操作使用明确启动运行时的既有工作容量。
    runtime: Handle,
    /// Only one write or flush may be in flight for this pipe.
    /// 此管道仅允许一个写入或刷新在途。
    pending: Option<JoinHandle<io::Result<()>>>,
    /// Accepted bytes require a native flush before a flush observer can succeed.
    /// 已接收字节要求原生刷新，之后刷新观察才能成功。
    needs_flush: bool,
    /// Real failures remain sticky so later polls cannot pretend buffered bytes were delivered.
    /// 真实失败持续保留，防止后续轮询假称缓冲字节已交付。
    failure: Option<PipeFailure>,
    /// This observer retains evidence, never the actual writer.
    /// 此观察者保留证据，绝不保留实际写入者。
    completion: PipeCompletion,
    /// An explicit async shutdown retains its pending release observation across polls.
    /// 显式异步关闭跨轮询保留待完成释放观察。
    shutdown_wait: Option<ReleaseWait>,
    /// Successful shutdown stays idempotent without polling a completed future again.
    /// 成功关闭保持幂等，不再次轮询已完成 future。
    shutdown_complete: bool,
}

impl<W: Write + Send + 'static> NativePipeWriter<W> {
    /// Creates an idle adapter and a receipt for the exact writer's eventual release.
    /// 创建空闲适配器及精确写入者最终释放的回执。
    /// Writer is consumed and runtime is explicit; no permanent I/O worker is started.
    /// 消费 writer，runtime 明确给定；不启动永久输入输出工作线程。
    pub(in crate::stdio) fn new(writer: W, runtime: Handle) -> (Self, PipeCompletion) {
        // The writer and external close barrier share only the immutable completion observation.
        // 写入者及外部关闭屏障仅共享不可变完成观察。
        let (owner, completion) = PipeOwner::new(writer);
        (
            Self {
                owner: Some(owner),
                runtime,
                pending: None,
                needs_flush: false,
                failure: None,
                completion: completion.clone(),
                shutdown_wait: None,
                shutdown_complete: false,
            },
            completion,
        )
    }

    /// Observes the one pending native operation and preserves any first failure.
    /// 观察唯一待完成原生操作，并保留任何首个失败。
    /// Context registers wakeup; returns pending or the actual operation result.
    /// context 注册唤醒；返回待完成或实际操作结果。
    fn poll_pending(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(failure) = &self.failure {
            return Poll::Ready(Err(failure.error()));
        }
        // No operation means this adapter owns the actual idle writer directly.
        // 无操作表示此适配器直接拥有实际空闲写入者。
        let Some(pending) = self.pending.as_mut() else {
            return Poll::Ready(Ok(()));
        };
        // The join remains stored until its result is observed; caller cancellation cannot remove it.
        // 等待句柄保存至结果被观察；调用方取消不能移除它。
        let joined = ready!(Pin::new(pending).poll(cx));
        self.pending = None;
        // A runtime failure never certifies successful native output.
        // 运行时失败绝不证明原生输出成功。
        let result = joined
            .map_err(|error| io::Error::other(format!("MCP native write worker failed: {error}")))
            .and_then(|result| result);
        if let Err(error) = &result {
            if let Some(owner) = &self.owner {
                owner.record_failure(error.to_string());
            }
            self.failure = Some(PipeFailure::from_error(error));
        }
        Poll::Ready(result)
    }
}

impl<W: Write + Send + 'static> AsyncWrite for NativePipeWriter<W> {
    /// Accepts one bounded chunk, preserving it in the native worker until the OS write finishes.
    /// 接收一个有界分片，并保留于原生工作线程直到操作系统写入结束。
    /// Bytes are acknowledged once accepted; flush observes delivery, matching buffered AsyncWrite semantics.
    /// 字节接收后即确认；flush 观察交付，遵循缓冲 AsyncWrite 语义。
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        // The resource itself remains at its stable shared address.
        // 资源本身留在稳定共享地址。
        let this = self.get_mut();
        ready!(this.poll_pending(cx))?;
        // Only explicit shutdown removes the adapter's native ownership.
        // 仅显式关闭移除适配器原生所有权。
        let Some(owner) = this.owner.as_ref() else {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "MCP protocol writer is closed.",
            )));
        };
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        // The operation owns at most this many copied bytes, independent of future message size.
        // 操作最多拥有这些复制字节，与后续消息大小无关。
        let count = bytes.len().min(PIPE_IO_CHUNK_BYTES);
        // Copying is bounded and completes before any accepted-byte count is returned.
        // 复制受界限约束，并在返回任何接收字节数前完成。
        let owned_bytes = bytes[..count].to_vec();
        // A detached in-flight task still owns the original resource until its native write returns.
        // 已脱离的在途任务仍拥有原资源，直到原生写入返回。
        let operation = QueuedWrite::new(Arc::clone(owner));
        this.pending = Some(
            this.runtime
                .spawn_blocking(move || operation.run(|writer| writer.write_all(&owned_bytes))),
        );
        this.needs_flush = true;
        Poll::Ready(Ok(count))
    }

    /// Waits for accepted writes and a single native flush, retaining work across cancellation.
    /// 等待已接收写入及一次原生刷新，跨取消保留工作。
    /// Context registers the observer; returns success only after the actual writer's flush completes.
    /// context 注册观察者；仅在实际写入者刷新完成后返回成功。
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Both write and flush use the same one-operation slot.
        // 写入及刷新使用同一个单操作槽位。
        let this = self.get_mut();
        loop {
            ready!(this.poll_pending(cx))?;
            if !this.needs_flush {
                return Poll::Ready(Ok(()));
            }
            // Shutdown always flushes before releasing this reference.
            // 关闭总是在释放此引用前刷新。
            let Some(owner) = this.owner.as_ref() else {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "MCP protocol writer closed before flushing.",
                )));
            };
            // The native operation retains exact ownership even if the flush observer disappears.
            // 即使刷新观察者消失，原生操作也保留精确所有权。
            let operation = QueuedWrite::new(Arc::clone(owner));
            this.pending = Some(
                this.runtime
                    .spawn_blocking(move || operation.run(Write::flush)),
            );
            this.needs_flush = false;
        }
    }

    /// Flushes, releases the actual writer reference, and waits for its original release receipt.
    /// 刷新、释放实际写入者引用，并等待其原释放回执。
    /// Context drives observation only; repeated or cancelled shutdown never duplicates a close operation.
    /// context 仅驱动观察；重复或取消关闭绝不重复关闭操作。
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        // Pinning does not pin the actual resource, which is owned behind Arc.
        // 固定适配器不固定实际资源，后者由 Arc 后方持有。
        let this = self.get_mut();
        if this.shutdown_complete {
            return Poll::Ready(Ok(()));
        }
        drop(this.owner.take());
        if this.shutdown_wait.is_none() {
            // Cloning a receipt cannot keep the native handle open.
            // 克隆回执不能使原生句柄保持打开。
            let completion = this.completion.clone();
            this.shutdown_wait = Some(Box::pin(async move { completion.wait().await }));
        }
        // The slot is initialized above and retained across every pending poll.
        // 此槽位已在上方初始化，并跨每次待完成轮询保留。
        let Some(wait) = this.shutdown_wait.as_mut() else {
            return Poll::Ready(Err(io::Error::other(
                "MCP writer shutdown receipt was not initialized.",
            )));
        };
        // Completion is remembered before the ready future can be polled again.
        // 在就绪 future 可能再次被轮询前，记住完成状态。
        let result = ready!(wait.as_mut().poll(cx));
        match &result {
            Ok(()) => this.shutdown_complete = true,
            Err(error) => this.failure = Some(PipeFailure::from_error(error)),
        }
        Poll::Ready(result)
    }
}
