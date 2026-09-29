//! Cancellation-safe read buffering around the original native pipe owner.
//! 围绕原原生管道所有者提供取消安全的读取缓冲。

use super::{PIPE_IO_CHUNK_BYTES, PipeCompletion, PipeFailure, PipeOwner};
use std::{
    io::{self, Read},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};
use tokio::{
    io::{AsyncRead, ReadBuf},
    runtime::Handle,
    task::JoinHandle,
};

/// Keeps buffered bytes and at most one native read across cancelled receive futures.
/// 在接收 future 取消期间保留缓冲字节及最多一次原生读取。
pub(in crate::stdio) struct NativePipeReader<R> {
    /// The adapter and each running or queued operation retain the same actual resource.
    /// 适配器及每个运行或排队操作保留相同实际资源。
    owner: Arc<PipeOwner<R>>,
    /// The launch runtime supplies existing blocking execution capacity.
    /// 启动运行时提供既有阻塞执行容量。
    runtime: Handle,
    /// One pending read bounds in-flight operations independently of caller cancellation.
    /// 单个待读取独立于调用方取消限制在途操作。
    pending: Option<JoinHandle<io::Result<Vec<u8>>>>,
    /// Completed bytes survive cancellation and smaller subsequent read buffers.
    /// 已完成字节在取消及后续更小读取缓冲下继续保留。
    buffered: Vec<u8>,
    /// Only bytes before this offset have been delivered to the official decoder.
    /// 仅此偏移之前的字节已交给官方解码器。
    offset: usize,
    /// A confirmed native zero-byte read makes future reads return EOF without new work.
    /// 已确认原生零字节读取使后续读取直接返回 EOF，不创建新工作。
    eof: bool,
    /// Actual read or join errors remain visible on repeated polls.
    /// 实际读取或等待错误在重复轮询中保持可见。
    failure: Option<PipeFailure>,
}

impl<R: Read + Send + 'static> NativePipeReader<R> {
    /// Binds the original reader to one runtime and its independent actual-release receipt.
    /// 将原读取者绑定到一个运行时及其独立实际释放回执。
    /// Reader is consumed and runtime is explicit; returns the adapter and the original receipt.
    /// 消费 reader，runtime 明确给定；返回适配器及原回执。
    pub(in crate::stdio) fn new(reader: R, runtime: Handle) -> (Self, PipeCompletion) {
        // No OS read is scheduled until the official decoder first polls this adapter.
        // 官方解码器首次轮询此适配器前，不调度操作系统读取。
        let (owner, completion) = PipeOwner::new(reader);
        (
            Self {
                owner,
                runtime,
                pending: None,
                buffered: Vec::new(),
                offset: 0,
                eof: false,
                failure: None,
            },
            completion,
        )
    }
}

impl<R: Read + Send + 'static> AsyncRead for NativePipeReader<R> {
    /// Delivers preserved bytes or schedules one bounded native read while retaining actual ownership.
    /// 交付保留字节，或在保留实际所有权期间调度一次有界原生读取。
    /// Context registers the current observer; destination receives bytes without consuming them on cancellation.
    /// context 注册当前观察者；destination 接收字节，取消时不消费字节。
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        destination: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Only the adapter's state moves; the original native resource remains behind its stable Arc.
        // 仅移动适配器状态；原原生资源保留在稳定 Arc 后方。
        let this = self.get_mut();
        if destination.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        loop {
            if let Some(failure) = &this.failure {
                return Poll::Ready(Err(failure.error()));
            }
            if this.offset < this.buffered.len() {
                // A new receive future may offer a smaller destination than the cancelled predecessor.
                // 新接收 future 可能提供比已取消前驱更小的目标缓冲。
                let count = destination
                    .remaining()
                    .min(this.buffered.len() - this.offset);
                destination.put_slice(&this.buffered[this.offset..this.offset + count]);
                this.offset += count;
                return Poll::Ready(Ok(()));
            }
            if this.eof {
                return Poll::Ready(Ok(()));
            }
            if let Some(pending) = this.pending.as_mut() {
                // The stored join handle survives cancellation of the surrounding read future.
                // 保存的等待句柄在外层读取 future 取消后继续存续。
                let joined = ready!(Pin::new(pending).poll(cx));
                this.pending = None;
                // A lost operation is an error, never a synthetic EOF.
                // 丢失操作是错误，绝非合成 EOF。
                let result = joined
                    .map_err(|error| {
                        io::Error::other(format!("MCP native read worker failed: {error}"))
                    })
                    .and_then(|result| result);
                match result {
                    Ok(bytes) => {
                        this.eof = bytes.is_empty();
                        this.buffered = bytes;
                        this.offset = 0;
                    }
                    Err(error) => {
                        this.owner.record_failure(error.to_string());
                        this.failure = Some(PipeFailure::from_error(&error));
                        return Poll::Ready(Err(error));
                    }
                }
            } else {
                // The worker's reference pins the exact resource even if this adapter is destroyed.
                // 即使此适配器销毁，工作线程引用也固定精确资源。
                let owner = Arc::clone(&this.owner);
                // Bound each allocation and native request by the same sole byte limit.
                // 每次分配及原生请求使用同一唯一定义的字节上限。
                let count = destination.remaining().min(PIPE_IO_CHUNK_BYTES);
                this.pending = Some(this.runtime.spawn_blocking(move || {
                    // Initialized storage avoids imposing unsafe Read implementations on the adapter.
                    // 已初始化存储避免适配器要求不安全 Read 实现。
                    let mut bytes = vec![0_u8; count];
                    // Interrupted reads are retried inside the one operation without allocating another worker.
                    // 中断读取在同一操作内部重试，不分配另一个工作线程。
                    let count = owner.with_io(|reader| {
                        loop {
                            match reader.read(&mut bytes) {
                                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                                    continue;
                                }
                                result => break result,
                            }
                        }
                    })?;
                    bytes.truncate(count);
                    Ok(bytes)
                }));
            }
        }
    }
}

impl<R> Drop for NativePipeReader<R> {
    /// Cancels queued reads when the entire adapter leaves, while started blocking reads retain ownership.
    /// 整个适配器离开时取消排队读取，已开始的阻塞读取仍保留所有权。
    /// No inputs or return; abort never certifies release and cannot stop an already-running blocking call.
    /// 无输入或返回；abort 绝不证明释放，也不能停止已经运行的阻塞调用。
    fn drop(&mut self) {
        if let Some(pending) = &self.pending {
            pending.abort();
        }
    }
}
