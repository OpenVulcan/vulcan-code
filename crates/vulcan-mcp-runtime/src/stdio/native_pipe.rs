//! Async protocol adapters retain the exact native resource through every blocking operation.
//! 异步协议适配器在每次阻塞操作期间保留精确原生资源。

use super::pipe_completion::PipeCompletion;
use std::{
    io,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

mod reader;
mod writer;
pub(super) use reader::NativePipeReader;
pub(super) use writer::NativePipeWriter;

#[cfg(test)]
mod tests;

/// Sole bound for bytes buffered by one native protocol I/O operation.
/// 单次原生协议输入输出操作缓冲字节数的唯一上限。
const PIPE_IO_CHUNK_BYTES: usize = 16 * 1024;

/// Owns the exact resource and first I/O failure while adapter and in-flight work share its lifetime.
/// 在适配器及在途工作共享生命周期期间，持有精确资源及首个输入输出失败。
struct PipeOwner<T> {
    /// Blocking operations serialize access to the one original resource.
    /// 阻塞操作串行访问唯一原资源。
    state: Mutex<PipeState<T>>,
    /// Final release is published only by the last actual owner's destructor.
    /// 最终释放仅由最后实际所有者的析构发布。
    result: watch::Sender<Option<Result<(), String>>>,
}

/// Keeps resource disposal separate from receipt publication in the final owner.
/// 在最终所有者中，将资源销毁与回执发布分开。
struct PipeState<T> {
    /// This is Some until the final destructor explicitly takes and drops the actual resource.
    /// 最终析构明确取走并释放实际资源之前，此值始终为 Some。
    resource: Option<T>,
    /// The first operation failure survives later observer cancellation and successful cleanup.
    /// 首个操作失败在后续观察取消及成功清理之后仍保留。
    failure: Option<String>,
}

impl<T> PipeOwner<T> {
    /// Creates actual ownership and an independent observer of final resource release.
    /// 创建实际所有权及最终资源释放的独立观察者。
    /// Resource is the original handle owner; returns a shared owner and its release receipt.
    /// resource 为原句柄所有者；返回共享所有者及其释放回执。
    fn new(resource: T) -> (Arc<Self>, PipeCompletion) {
        // The result channel never owns or prolongs the actual pipe.
        // 结果通道绝不拥有或延长实际管道。
        let (result, completion) = PipeCompletion::channel();
        (
            Arc::new(Self {
                state: Mutex::new(PipeState {
                    resource: Some(resource),
                    failure: None,
                }),
                result,
            }),
            completion,
        )
    }

    /// Executes one serialized native operation and retains its first real failure.
    /// 执行单次串行原生操作，并保留其首个真实失败。
    /// Operation borrows the exact resource; returns its original result without duplicating a handle.
    /// operation 借用精确资源；返回其原结果，不复制句柄。
    fn with_io<O>(&self, operation: impl FnOnce(&mut T) -> io::Result<O>) -> io::Result<O> {
        // Poisoned I/O state is never reused; only the destructor may recover it for disposal.
        // 中毒的输入输出状态绝不复用；仅析构可获取它用于销毁。
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("MCP native pipe owner was poisoned."))?;
        // Optional ownership permits explicit disposal before publishing the final receipt.
        // 可选所有权允许在发布最终回执之前显式销毁。
        let result = match state.resource.as_mut() {
            Some(resource) => operation(resource),
            None => Err(io::Error::other(
                "MCP native pipe resource was already released.",
            )),
        };
        if let Err(error) = &result {
            state
                .failure
                .get_or_insert_with(|| format!("MCP protocol pipe I/O failed: {error}"));
        }
        result
    }

    /// Retains a worker scheduling or join failure without attempting further native I/O.
    /// 保留工作线程调度或等待失败，不再尝试原生输入输出。
    /// Message is the actual runtime diagnostic; no result is returned and no resource is released.
    /// message 为实际运行时诊断；不返回结果，也不释放资源。
    fn record_failure(&self, message: String) {
        // Poison recovery here records failure only; it never accesses the underlying I/O resource.
        // 此处获取中毒状态仅记录失败，绝不访问底层输入输出资源。
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.failure.get_or_insert(message);
    }
}

impl<T> Drop for PipeOwner<T> {
    /// Drops the exact native resource before publishing any terminal release result.
    /// 发布任何终态释放结果前，释放精确原生资源。
    /// No inputs or return; exclusive final ownership permits poison recovery solely for disposal.
    /// 无输入或返回；排他最终所有权允许仅为销毁获取中毒状态。
    fn drop(&mut self) {
        // With no remaining Arc owners, get_mut cannot race a reader or writer.
        // 没有剩余 Arc 所有者时，get_mut 不会与读取者或写入者竞争。
        let state = match self.state.get_mut() {
            Ok(state) => state,
            Err(poisoned) => {
                // A panicking worker must not make disposal look like a clean operation.
                // 工作线程恐慌不能使销毁看起来像正常操作。
                let state = poisoned.into_inner();
                state
                    .failure
                    .get_or_insert_with(|| String::from("MCP native pipe owner was poisoned."));
                state
            }
        };
        if state.resource.is_none() {
            state.failure.get_or_insert_with(|| {
                String::from("MCP native pipe release evidence was missing.")
            });
        }
        drop(state.resource.take());
        self.result
            .send_replace(Some(state.failure.take().map_or(Ok(()), Err)));
    }
}

/// Stores an I/O failure so repeated polls cannot reinterpret it as EOF or successful flush.
/// 保存输入输出失败，防止重复轮询将其重新解释为 EOF 或成功刷新。
struct PipeFailure {
    /// Original error classification is preserved for subsequent polls.
    /// 后续轮询保留原错误分类。
    kind: io::ErrorKind,
    /// The original diagnostic remains bounded by the native operation's error.
    /// 原诊断受原生操作错误内容约束。
    message: String,
}

impl PipeFailure {
    /// Retains the supplied I/O classification and diagnostic.
    /// 保留所给输入输出分类及诊断。
    /// Error is borrowed and remains available for its first caller; returns repeatable error state.
    /// 借用 error，使其仍可交给首次调用方；返回可重复错误状态。
    fn from_error(error: &io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }

    /// Recreates the same classification and message for a later poll.
    /// 为后续轮询重新创建相同分类及消息。
    /// No inputs; returns an owned error without consuming the retained failure.
    /// 无输入；返回自有错误，不消费保留失败。
    fn error(&self) -> io::Error {
        io::Error::new(self.kind, self.message.clone())
    }
}
