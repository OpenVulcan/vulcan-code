//! A native worker owns the real child until exit, independently of any Tokio observer.
//! 原生工作线程持有真实子进程直至退出，独立于任何 Tokio 观察者。

use std::io;
use std::process::{ChildStderr, ChildStdin, ChildStdout, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::{oneshot, watch};

/// Sole direct-child grace interval after the protocol writer has been closed.
/// 协议写入端关闭后，直接子进程唯一宽限时间。
const CHILD_EXIT_GRACE: Duration = Duration::from_secs(3);
/// Bounds native exit polling and retries without requiring an asynchronous timer driver.
/// 限制原生退出轮询及重试频率，无需异步计时驱动。
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Monotonic stop requests distinguish normal protocol close from loss of its owner.
/// 单调停止请求区分正常协议关闭与所有者丢失。
#[repr(u8)]
enum StopRequest {
    /// Keep the child running while its transport remains owned.
    /// 传输仍被持有时保持子进程运行。
    Running = 0,
    /// Allow the protocol-close grace before forced termination.
    /// 强制终止前允许协议关闭宽限。
    Graceful = 1,
    /// Cancellation or destruction requests immediate termination.
    /// 取消或销毁请求立即终止。
    Immediate = 2,
}

/// Actual pipe handles transferred out of the native child owner exactly once.
/// 仅一次从原生子进程所有者移交的实际管道句柄。
pub(super) struct ChildPipes {
    /// Protocol input retains the standard library's exact child handle.
    /// 协议输入保留标准库的精确子进程句柄。
    pub(super) stdin: ChildStdin,
    /// Protocol output is consumed by the official asynchronous codec.
    /// 协议输出由官方异步编解码器消费。
    pub(super) stdout: ChildStdout,
    /// Diagnostic output remains separate from protocol messages.
    /// 诊断输出与协议消息保持分离。
    pub(super) stderr: ChildStderr,
}

/// Observes one actual child exit without owning or cancelling the worker.
/// 观察单个实际子进程退出，不拥有或取消工作线程。
#[derive(Clone)]
pub(super) struct ChildCompletion {
    /// Success is published only after the child has exited and its process handle was dropped.
    /// 仅在子进程退出且进程句柄释放后发布成功。
    result: watch::Receiver<Option<Result<(), String>>>,
}

impl ChildCompletion {
    /// Waits for confirmed exit or a reported cleanup failure; observer cancellation is harmless.
    /// 等待确认退出或已报告清理失败；观察者取消不影响清理。
    /// No inputs; an error never proves exit and the native owner continues its cleanup attempts.
    /// 无输入；错误绝不证明退出，原生所有者会继续尝试清理。
    pub(super) async fn wait(&self) -> io::Result<()> {
        // Each observer receives the retained result without consuming another observer's receipt.
        // 每个观察者读取保留结果，不消费其他观察者的回执。
        let mut receiver = self.result.clone();
        loop {
            if let Some(result) = receiver.borrow().clone() {
                return result.map_err(io::Error::other);
            }
            receiver.changed().await.map_err(|_| {
                io::Error::other("MCP child supervisor ended without confirming process exit.")
            })?;
        }
    }
}

/// Requests child shutdown while the independent native worker owns the actual process.
/// 请求子进程关闭，而实际进程由独立原生工作线程持有。
pub(super) struct ManagedChild {
    /// Stop requests only escalate, so graceful close cannot undo cancellation.
    /// 停止请求只升级，正常关闭不能撤销取消。
    requested: Arc<AtomicU8>,
    /// A bounded wakeup channel never carries repeated work or allocates per stop request.
    /// 有界唤醒通道不携带重复工作，也不按停止请求分配空间。
    wake: mpsc::SyncSender<()>,
    /// Shared final evidence remains observable after the requesting owner leaves.
    /// 请求所有者离开后，共享最终证据仍可被观察。
    completion: ChildCompletion,
}

impl ManagedChild {
    /// Spawns on a native worker before handing pipes to the asynchronous protocol adapter.
    /// 先在原生工作线程启动，再把管道交给异步协议适配器。
    /// Command preserves exact arguments, environment and flags; returns the owner and original pipes.
    /// command 保留精确参数、环境及标志；返回所有者和原管道。
    pub(super) async fn spawn(command: Command) -> io::Result<(Self, ChildPipes)> {
        // The owner exists before spawn is scheduled, covering cancellation of the first await.
        // 调度启动前所有者已存在，覆盖首次等待的取消。
        let requested = Arc::new(AtomicU8::new(StopRequest::Running as u8));
        // Only one wakeup is needed; the monotonic atomic value carries the actual request.
        // 仅需一次唤醒；单调原子值携带实际请求。
        let (wake, wake_receiver) = mpsc::sync_channel(1);
        // Receipts do not rely on an executor to be published by the native owner.
        // 原生所有者发布回执不依赖执行器。
        let (result, result_receiver) = watch::channel(None);
        // Pipe publication fails if the initialization observer has already disappeared.
        // 初始化观察者已消失时，管道发布失败。
        let (ready, ready_receiver) = oneshot::channel();
        // This guard requests immediate cleanup if thread creation or pipe observation fails.
        // 线程创建或管道观察失败时，此保护请求立即清理。
        let owner = Self {
            requested: Arc::clone(&requested),
            wake,
            completion: ChildCompletion {
                result: result_receiver,
            },
        };
        std::thread::Builder::new()
            .name(String::from("mcp-child-owner"))
            .spawn(move || supervise(command, requested, wake_receiver, ready, result))?;
        // No detached Tokio cleanup is needed when this future is cancelled.
        // 此 future 取消时无需派生 Tokio 清理。
        let pipes = ready_receiver.await.map_err(|_| {
            io::Error::other("MCP child supervisor ended before publishing child pipes.")
        })??;
        Ok((owner, pipes))
    }

    /// Requests graceful exit once and waits on the original independent completion receipt.
    /// 请求正常退出，并等待原独立完成回执。
    /// No inputs; returns confirmed exit or a cleanup error without cancelling native ownership.
    /// 无输入；返回确认退出或清理错误，不取消原生所有权。
    pub(super) async fn close(&self) -> io::Result<()> {
        self.start_close();
        self.completion.wait().await
    }

    /// Starts the grace timer before protocol shutdown can block on a full child pipe.
    /// 在协议关闭可能阻塞于已满子进程管道前启动宽限计时。
    /// No inputs or result; subsequent waits observe this same request.
    /// 无输入或返回；后续等待观察相同请求。
    pub(super) fn start_close(&self) {
        self.request(StopRequest::Graceful);
    }

    /// Escalates the exact child's stop state and wakes its sole native owner.
    /// 升级精确子进程的停止状态，并唤醒其唯一原生所有者。
    /// Request is monotonic; a full wake channel already contains the necessary notification.
    /// 请求单调；已满的唤醒通道已包含所需通知。
    fn request(&self, request: StopRequest) {
        self.requested.fetch_max(request as u8, Ordering::AcqRel);
        match self.wake.try_send(()) {
            Ok(())
            | Err(mpsc::TrySendError::Full(()))
            | Err(mpsc::TrySendError::Disconnected(())) => {}
        }
    }
}

impl Drop for ManagedChild {
    /// Requests immediate cleanup without moving the actual process onto the caller's executor.
    /// 请求立即清理，不将实际进程移交给调用方执行器。
    /// No inputs or result; the native owner retains the child until exit is confirmed.
    /// 无输入或返回；原生所有者保留子进程，直至确认退出。
    fn drop(&mut self) {
        self.request(StopRequest::Immediate);
    }
}

/// Owns spawn, stop and reap on one native thread; never reports an unconfirmed exit as success.
/// 在单个原生线程拥有启动、停止和回收；绝不把未确认退出报告为成功。
/// Command and channels describe one child only; this function returns after spawn failure or actual exit.
/// command 与通道仅描述一个子进程；本函数在启动失败或实际退出后返回。
fn supervise(
    mut command: Command,
    requested: Arc<AtomicU8>,
    wake: mpsc::Receiver<()>,
    ready: oneshot::Sender<io::Result<ChildPipes>>,
    result: watch::Sender<Option<Result<(), String>>>,
) {
    // Standard-library spawning transfers process ownership without registering it with Tokio.
    // 标准库启动移交进程所有权，不向 Tokio 注册该进程。
    let mut child = match command
        .as_std_mut()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    // A failed pipe handoff still leaves the actual child owned by this thread for cleanup.
    // 管道移交失败后，实际子进程仍由本线程持有并清理。
    let pipes = match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
        (Some(stdin), Some(stdout), Some(stderr)) => Ok(ChildPipes {
            stdin,
            stdout,
            stderr,
        }),
        _ => Err(io::Error::other(
            "MCP child is missing a requested stdio pipe.",
        )),
    };
    if pipes.is_err() || ready.is_closed() {
        requested.fetch_max(StopRequest::Immediate as u8, Ordering::AcqRel);
    }
    if ready.send(pipes).is_err() {
        requested.fetch_max(StopRequest::Immediate as u8, Ordering::AcqRel);
    }
    // One deadline belongs to graceful close; later cancellation may escalate it immediately.
    // 一个期限属于正常关闭；之后取消可立即升级。
    let mut closing_at = None;
    // Successful termination is sent once; failed attempts retain the child for another attempt.
    // 成功终止仅发送一次；失败尝试保留子进程以便再次尝试。
    let mut termination_sent = false;
    // Keep the first actual cleanup failure even if a later retry reaps the process.
    // 即使之后重试回收进程，也保留首个实际清理失败。
    let mut failure = None;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                drop(child);
                result.send_replace(Some(failure.map_or(Ok(()), Err)));
                return;
            }
            Ok(None) => {}
            Err(error) => {
                // The retained diagnostic is copied to observers without releasing native ownership.
                // 向观察者复制保留诊断，不释放原生所有权。
                let diagnostic = failure
                    .get_or_insert_with(|| {
                        format!("MCP child exit could not be confirmed: {error}")
                    })
                    .clone();
                result.send_replace(Some(Err(diagnostic)));
                requested.fetch_max(StopRequest::Immediate as u8, Ordering::AcqRel);
            }
        }
        // Read the monotonic request after polling to avoid killing an already reaped child.
        // 轮询后读取单调请求，避免终止已经回收的子进程。
        let stop = requested.load(Ordering::Acquire);
        if stop != StopRequest::Running as u8 {
            // The native clock survives loss of the caller's runtime and timer driver.
            // 原生时钟在调用方运行时及计时驱动丢失后继续有效。
            let started = closing_at.get_or_insert_with(Instant::now);
            if !termination_sent
                && (stop == StopRequest::Immediate as u8 || started.elapsed() >= CHILD_EXIT_GRACE)
            {
                match child.kill() {
                    Ok(()) => termination_sent = true,
                    Err(error) => {
                        // A natural exit racing termination is confirmed through the owned child itself.
                        // 与终止竞争的自然退出通过所持子进程自身确认。
                        if let Ok(Some(_)) = child.try_wait() {
                            drop(child);
                            result.send_replace(Some(failure.map_or(Ok(()), Err)));
                            return;
                        }
                        // Preserve the first failure while this worker retains and retries the actual process.
                        // 工作线程保留并重试实际进程期间，保留首个失败。
                        let diagnostic = failure
                            .get_or_insert_with(|| format!("MCP child termination failed: {error}"))
                            .clone();
                        result.send_replace(Some(Err(diagnostic)));
                    }
                }
            }
        }
        match wake.recv_timeout(CHILD_POLL_INTERVAL) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                requested.fetch_max(StopRequest::Immediate as u8, Ordering::AcqRel);
                // A disconnected channel returns immediately; preserve the same bounded polling interval.
                // 已断开通道立即返回；保留相同有界轮询间隔。
                std::thread::sleep(CHILD_POLL_INTERVAL);
            }
        }
    }
}

#[cfg(test)]
mod tests;
