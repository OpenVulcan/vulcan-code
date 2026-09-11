//! Bounded host execution channel for synchronous LuaSkills database callbacks.
//! 面向同步 LuaSkills 数据库回调的有界宿主执行通道。
//!
//! LuaSkills installs its SQLite and LanceDB providers as **synchronous** process-global callbacks
//! (`Arc<dyn Fn(&RuntimeSqliteProviderRequest) -> Result<Value, String> + Send + Sync>` and its LanceDB
//! counterpart), while every in-process database facility this host owns is asynchronous. Something has to
//! run a future to completion on behalf of a callback that cannot await, which is what this channel does.
//! LuaSkills 把它的 SQLite 与 LanceDB provider 安装为**同步**的进程级回调，而本宿主拥有的每个进程内数据库设施
//! 都是异步的。必须有人替无法 await 的回调把 future 运行到完成，这正是本通道的职责。
//!
//! The calling-thread question was settled with evidence before this module was written, and the answer
//! fixes the mechanism. The pinned `luaskills 0.5.5` source spawns eleven plain `std::thread`s and
//! references `tokio` in exactly three places, all three inside `host/controller.rs` — the external-controller
//! host that this migration retires. The VM, engine, provider and callback paths therefore run on plain
//! threads with **no ambient Tokio runtime**, which rules out `tokio::task::block_in_place` (it needs a
//! multi-thread runtime context that the callback thread does not have) and rules out constructing a
//! runtime inside a callback (the nested-runtime construction the plan forbids). What remains is the one
//! supported direction: capture the host's [`Handle`] once and call [`Handle::block_on`] from a thread that
//! has no runtime context. This module never constructs a runtime; it cannot, because a [`Handle`] cannot
//! be obtained without one.
//! 调用线程问题在本模块编写之前就以证据确定，而答案决定了机制。锁定的 `luaskills 0.5.5` 源码派生十一个普通
//! `std::thread`，并且只在三处引用 `tokio`，三处全在 `host/controller.rs`——即本迁移要退役的外部控制器
//! 宿主。因此 VM、引擎、provider 与回调路径都运行在**无 ambient Tokio 运行时**的普通线程
//! 上，这排除了 `tokio::task::block_in_place`（它需要回调线程并不具备的多线程运行时上下文），也排除了在回调
//! 内部构造运行时（方案禁止的嵌套运行时构造）。剩下的是唯一受支持的方向：一次性捕获宿主的 [`Handle`]，并从无
//! 运行时上下文的线程调用 [`Handle::block_on`]。本模块绝不构造运行时；它也无法构造，因为没有运行时就无法获得
//! [`Handle`]。
//!
//! Four properties are deliberate, and each exists because its absence has a specific failure mode:
//! 四项性质是刻意的，每一项的存在都因为缺失它有特定的失败模式：
//!
//! - **Bounded.** The request queue has a fixed capacity and a fixed worker count, and submission uses
//!   `try_send`. A callback therefore never waits for a queue slot; when the host is saturated it receives
//!   an explicit saturation error. Bounding the wait is the point: an unbounded queue would turn a slow
//!   database into unbounded memory, and a blocking send would turn it into an unbounded wait inside a Lua
//!   VM thread.
//! - **有界。** 请求队列容量固定、工作线程数固定，且提交使用 `try_send`。因此回调绝不等待队列空位；当宿主饱和
//!   时它会收到明确的饱和错误。约束等待正是要点：无界队列会把慢数据库变成无界内存，而阻塞式发送会把慢数据库变成
//!   Lua VM 线程内的无界等待。
//! - **Closable.** Closing stops accepting work and joins the workers, so a callback that arrives after host
//!   shutdown receives a closed-channel error rather than a hang, and no thread outlives the host. The
//!   channel must be closed before the runtime it borrowed a [`Handle`] from is dropped. **A runtime
//!   handle does not keep its runtime alive**, so the reverse also holds and is the sharper hazard: dropping
//!   the runtime while jobs are queued cancels them. A test for the provider dispatch found this by
//!   dropping the runtime it had built, and the design is unchanged — the wiring must keep the runtime alive.
//!   channel must be closed before the runtime it borrowed a [`Handle`] from is dropped, which is why
//!   [`HostExecutionChannel::close`] is public and why dropping the channel closes it.
//! - **可关闭。** 关闭会停止接收工作并 join 工作线程，因此宿主关闭后到达的回调会收到已关闭错误而非挂起，且没有
//!   线程会比宿主活得更久。通道必须在它所借用 [`Handle`] 的运行时被丢弃之前关闭，这正是
//!   [`HostExecutionChannel::close`] 公开、且丢弃通道即关闭它的原因。
//! - **Panic-isolated.** A job that panics is caught, reported as an error, and does not kill its worker, so
//!   one bad statement cannot take the remaining callbacks down with it.
//! - **panic 隔离。** panic 的作业会被捕获并作为错误报告，且不会杀死其工作线程，因此一条坏语句不会连带摧毁其余
//!   回调。
//! - **Timeout-bounded.** A callback waits at most its configured call timeout for a result, so a stuck
//!   statement reports itself instead of hanging a Lua VM thread forever.
//! - **超时有界。** 回调最多等待其配置的调用超时，因此卡住的语句会自我报告，而不是永久挂起 Lua VM 线程。

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;

/// Default worker count for one host execution channel.
/// 单个宿主执行通道的默认工作线程数。
///
/// Four matches the host's already-declared parallel database ceiling, so the channel cannot outrun the
/// database boundary it submits into.
/// 四与宿主已声明的并行数据库上限一致，因此该通道无法超出它所提交到的数据库边界。
pub const HOST_EXECUTION_CHANNEL_WORKERS: usize = 4;

/// Default queue capacity for one host execution channel.
/// 单个宿主执行通道的默认队列容量。
pub const HOST_EXECUTION_CHANNEL_QUEUE_CAPACITY: usize = 64;

/// Default maximum time one callback waits for its job to finish.
/// 单个回调等待其作业完成的最大时长默认值。
pub const HOST_EXECUTION_CHANNEL_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// One queued unit of work: it owns everything it needs, including where to send its result.
/// 一个排队的工作单元：它拥有自己所需的一切，包括把结果发送到哪里。
type ChannelJob = Box<dyn FnOnce() + Send + 'static>;

/// Bounded, closable channel that runs asynchronous database work for synchronous callbacks.
/// 为同步回调运行异步数据库工作的有界可关闭通道。
///
/// The context is cloned into every job, so a channel is parameterised by whatever the callbacks need in
/// order to reach their databases — for example an `Arc<LocalDatabaseRuntime>`, or a structure holding that
/// plus a LanceDB engine.
/// 上下文会被克隆进每个作业，因此通道以其回调触达数据库所需之物为参数——例如
/// `Arc<LocalDatabaseRuntime>`，或持有它并附加 LanceDB 引擎的结构。
pub struct HostExecutionChannel<C> {
    context: C,
    /// The host runtime handle this channel drives futures on; it is cloned into every job.
    /// 本通道用于驱动 future 的宿主运行时句柄；它会被克隆进每个作业。
    handle: Handle,
    /// Dropped to stop the workers; `None` after the channel is closed.
    /// 丢弃以停止工作线程；通道关闭后为 `None`。
    sender: Option<SyncSender<ChannelJob>>,
    workers: Vec<std::thread::JoinHandle<()>>,
    closed: Arc<AtomicBool>,
    call_timeout: Duration,
}

impl<C> HostExecutionChannel<C> {
    /// Stops accepting work and waits for the workers to finish, without requiring context bounds.
    /// 停止接收工作并等待工作线程结束，且不要求上下文约束。
    ///
    /// This is defined without trait bounds because `Drop` cannot require them, and dropping the channel
    /// performs exactly this shutdown.
    /// 本方法不设 trait 约束，因为 `Drop` 无法要求它们，而丢弃通道执行的恰是此关闭流程。
    ///
    /// # Parameters / 参数
    /// This method takes no parameters.
    /// 本方法不接收参数。
    ///
    /// # Returns / 返回值
    /// Returns no value. Calling it more than once is harmless.
    /// 不返回值。多次调用是无害的。
    fn shutdown(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
        // Dropping the sender ends every worker's receive loop, so the joins below terminate.
        // 丢弃发送方会结束每个工作线程的接收循环，因此下面的 join 会终止。
        self.sender = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl<C> HostExecutionChannel<C>
where
    C: Clone + Send + Sync + 'static,
{
    /// Starts one channel with the default worker count, queue capacity and call timeout.
    /// 以默认工作线程数、队列容量与调用超时启动一个通道。
    ///
    /// # Parameters / 参数
    /// `handle` is the host runtime handle captured at bootstrap; `context` is cloned into every job.
    /// `handle` 是引导时捕获的宿主运行时句柄；`context` 会被克隆进每个作业。
    ///
    /// # Returns / 返回值
    /// Returns a started channel whose workers are already waiting for work.
    /// 返回一个已启动的通道，其工作线程已在等待工作。
    pub fn start(handle: Handle, context: C) -> Self {
        Self::start_with_limits(
            handle,
            context,
            HOST_EXECUTION_CHANNEL_WORKERS,
            HOST_EXECUTION_CHANNEL_QUEUE_CAPACITY,
            HOST_EXECUTION_CHANNEL_CALL_TIMEOUT,
        )
    }

    /// Starts one channel with explicit limits.
    /// 以显式上限启动一个通道。
    ///
    /// # Parameters / 参数
    /// `workers` is the fixed worker count, `queue_capacity` the fixed request-queue capacity, and
    /// `call_timeout` the maximum time one callback waits for its job.
    /// `workers` 是固定工作线程数，`queue_capacity` 是固定请求队列容量，`call_timeout` 是单个回调等待其作业的
    /// 最大时长。
    ///
    /// # Returns / 返回值
    /// Returns a started channel. Zero workers produce a channel that reports saturation for every call,
    /// which is a usable state rather than a panic.
    /// 返回已启动的通道。零工作线程会产生对每次调用都报告饱和的通道，这是可用状态而非 panic。
    pub fn start_with_limits(
        handle: Handle,
        context: C,
        workers: usize,
        queue_capacity: usize,
        call_timeout: Duration,
    ) -> Self {
        // A capacity of zero would make `sync_channel` a rendezvous channel and defeat `try_send`, so the
        // smallest useful capacity is one.
        // 容量为零会把 `sync_channel` 变成会合通道并破坏 `try_send`，因此最小可用容量为一。
        let capacity = queue_capacity.max(1);
        let (sender, receiver) = sync_channel::<ChannelJob>(capacity);
        // `std::sync::mpsc::Receiver` is single-consumer, so the workers share one under a mutex and hold it
        // only while taking a job, never while running it.
        // `std::sync::mpsc::Receiver` 是单消费者的，因此各工作线程在互斥锁下共享同一个接收端，且只在取作业时
        // 持锁，绝不在运行作业时持锁。
        let receiver = Arc::new(Mutex::new(receiver));
        let closed = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::with_capacity(workers);
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            let closed = Arc::clone(&closed);
            // Each worker is a plain thread, which is exactly the context a LuaSkills callback already
            // runs in, so no worker needs a runtime context of its own.
            // 每个工作线程都是普通线程，而 LuaSkills 回调本就在这种上下文中运行，因此没有工作线程需要自己的
            // 运行时上下文。
            let worker = std::thread::Builder::new()
                .name(format!("vulcan-luaskills-db-{index}"))
                .spawn(move || worker_loop(&receiver, &closed));
            match worker {
                Ok(handle) => handles.push(handle),
                // A thread that cannot be created is reported by the saturation path rather than by a panic,
                // so a partial worker set is still a usable channel.
                // 无法创建的线程由饱和路径而非 panic 报告，因此部分工作线程集仍是可用通道。
                Err(_) => break,
            }
        }
        Self {
            context,
            handle,
            sender: Some(sender),
            workers: handles,
            closed,
            call_timeout,
        }
    }

    /// Runs one asynchronous job to completion and returns its result.
    /// 把一个异步作业运行至完成并返回其结果。
    ///
    /// # Parameters / 参数
    /// `label` names the operation in diagnostics; `job` builds the future from the channel context.
    /// `label` 在诊断中命名该操作；`job` 从通道上下文构建 future。
    ///
    /// # Returns / 返回值
    /// Returns the job's value, or an explicit error when the channel is closed, saturated, the job panicked,
    /// the job failed, or the call exceeded its timeout. The value is generic because the SQLite provider
    /// returns JSON while the LanceDB provider returns metadata beside a byte payload.
    /// 返回作业的值，或在通道已关闭、饱和、作业 panic、作业失败或调用超过其超时时返回明确错误。该值之所以泛型，
    /// 是因为 SQLite provider 返回 JSON，而 LanceDB provider 返回元数据并附带字节载荷。
    ///
    /// The timeout bounds **this caller's wait**, not the job. A submitted job is never cancelled, which is
    /// deliberate — it is the same property `host_database_providers::shutdown` documents, where shutdown must not
    /// cancel work another holder is still awaiting. A caller must therefore read a timeout as an **unknown**
    /// outcome rather than a failed one: a write that reports a timeout may still complete, and the error text says
    /// so.
    /// 该超时限定的是**本调用方的等待**，而不是作业。已提交的作业绝不被取消，这是刻意的——它与
    /// `host_database_providers::shutdown` 所记录的性质相同，即关闭不得取消其他持有者仍在等待的工作。因此调用方
    /// 必须把超时读作**未知**结果而非失败结果：报告超时的写入仍可能完成，而错误文本已如此说明。
    pub fn run<T, F, Fut>(&self, label: &str, job: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(C) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, String>> + Send + 'static,
    {
        let sender = self
            .sender
            .as_ref()
            .filter(|_| !self.closed.load(Ordering::SeqCst))
            .ok_or_else(|| {
                format!("LuaSkills host execution channel is closed; `{label}` cannot run.")
            })?;
        let context = self.context.clone();
        let handle = self.handle.clone();
        // The result channel is bounded to exactly one message because a job reports exactly once.
        // 结果通道的界恰为一条消息，因为每个作业恰好报告一次。
        let (result_sender, result_receiver) = sync_channel::<Result<T, String>>(1);
        let queued: ChannelJob = Box::new(move || {
            // Driving the future happens here, on a worker thread with no runtime context, which is the
            // direction Tokio supports and the reason no runtime is nested.
            // 驱动 future 在此发生，位于无运行时上下文的工作线程上，这是 Tokio 支持的方向，也是不嵌套运行时的
            // 原因。
            let outcome =
                std::panic::catch_unwind(AssertUnwindSafe(|| handle.block_on(job(context))));
            let outcome = match outcome {
                Ok(result) => result,
                Err(payload) => Err(format!(
                    "LuaSkills host execution channel job panicked: {}",
                    panic_message(payload.as_ref())
                )),
            };
            // A receiver that is already gone means the caller timed out; the result is simply dropped.
            // 接收方已消失意味着调用方已超时；结果直接丢弃即可。
            let _ = result_sender.send(outcome);
        });
        match sender.try_send(queued) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                return Err(format!(
                    "LuaSkills host execution channel is saturated; `{label}` was rejected rather than queued."
                ));
            }
            Err(TrySendError::Disconnected(_)) => {
                return Err(format!(
                    "LuaSkills host execution channel has no worker; `{label}` cannot run."
                ));
            }
        }
        match result_receiver.recv_timeout(self.call_timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(format!(
                "LuaSkills host execution channel call `{label}` exceeded its {} second timeout and was not \
cancelled; the operation may still complete.",
                self.call_timeout.as_secs_f64()
            )),
        }
    }

    /// Stops accepting work and waits for the workers to finish.
    /// 停止接收工作并等待工作线程结束。
    ///
    /// # Parameters / 参数
    /// This method takes no parameters.
    /// 本方法不接收参数。
    ///
    /// # Returns / 返回值
    /// Returns no value. Calling it more than once is harmless.
    /// 不返回值。多次调用是无害的。
    pub fn close(&mut self) {
        self.shutdown();
    }

    /// Reports whether the channel has been closed.
    /// 报告通道是否已关闭。
    ///
    /// # Parameters / 参数
    /// This method takes no parameters.
    /// 本方法不接收参数。
    ///
    /// # Returns / 返回值
    /// Returns `true` after [`HostExecutionChannel::close`] or after the channel is dropped.
    /// 在 [`HostExecutionChannel::close`] 之后或通道被丢弃后返回 `true`。
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

impl<C> Drop for HostExecutionChannel<C> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Runs one worker's receive loop until the channel is closed.
/// 运行单个工作线程的接收循环直至通道关闭。
fn worker_loop(receiver: &Mutex<Receiver<ChannelJob>>, closed: &AtomicBool) {
    loop {
        if closed.load(Ordering::SeqCst) {
            break;
        }
        // The lock is released before the job runs, so jobs execute concurrently across workers.
        // 锁在作业运行前释放，因此作业在各工作线程间并发执行。
        let job = {
            let guard = match receiver.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.recv()
        };
        match job {
            Ok(job) => job(),
            // `recv` returns `Err` exactly when every sender is gone, which is how closing ends the loop.
            // 当所有发送方都消失时 `recv` 返回 `Err`，关闭正是以此结束循环。
            Err(_) => break,
        }
    }
}

/// Renders one panic payload into a single-line diagnostic.
/// 把一个 panic 载荷渲染为单行诊断。
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    String::from("unknown panic payload")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::mpsc::sync_channel;

    /// Builds one current-thread runtime. The jobs below never need a driver, so no enable call is
    /// required and the smallest possible runtime is enough.
    /// 构建一个单线程运行时。下面的作业不需要驱动器，因此无需 enable 调用，最小的运行时即可。
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build current-thread runtime")
    }

    /// Verifies asynchronous work runs off the calling thread and receives the channel context.
    /// 验证异步工作脱离调用线程运行，并接收通道上下文。
    #[test]
    fn runs_asynchronous_work_from_a_plain_thread() {
        let runtime = runtime();
        let mut channel =
            HostExecutionChannel::start(runtime.handle().clone(), Arc::new(String::from("ctx")));
        let caller = std::thread::current().id();

        let value = channel
            .run("probe", move |context| async move {
                Ok(json!({
                    "context": context.as_str(),
                    "off_caller_thread": std::thread::current().id() != caller,
                }))
            })
            .expect("run probe job");

        assert_eq!(value["context"], "ctx");
        assert_eq!(value["off_caller_thread"], true);
        channel.close();
    }

    /// Verifies a callback arriving on a thread that already has a runtime context still completes.
    /// 验证在已有运行时上下文的线程上到达的回调仍能完成。
    ///
    /// `enter` gives this thread a context without driving the scheduler, which is exactly the condition
    /// under which driving the future on the calling thread would be illegal. The job completing proves the
    /// channel handed it to a worker instead.
    /// `enter` 在不为本线程驱动调度器的前提下赋予其上下文，而这正是"在调用线程上驱动 future 属非法"的条件。
    /// 作业完成即证明通道改为把它交给了工作线程。
    #[test]
    fn runs_when_the_calling_thread_already_has_a_runtime_context() {
        let runtime = runtime();
        let channel = HostExecutionChannel::start(runtime.handle().clone(), ());
        let _entered = runtime.handle().clone().enter();

        let value = channel
            .run("entered", |()| async { Ok(json!({ "ok": true })) })
            .expect("run job from a thread with a runtime context");

        assert_eq!(value["ok"], true);
    }

    /// Verifies a full queue is reported instead of waited on.
    /// 验证队列已满时会报告而非等待。
    #[test]
    fn saturation_is_reported_rather_than_waited_on() {
        let runtime = runtime();
        let channel = Arc::new(HostExecutionChannel::start_with_limits(
            runtime.handle().clone(),
            (),
            1,
            1,
            Duration::from_millis(100),
        ));
        let (progress_sender, progress_receiver) = sync_channel::<()>(1);
        let (hold_sender, hold_receiver) = sync_channel::<()>(1);
        let holding_channel = Arc::clone(&channel);
        let holder = std::thread::spawn(move || {
            // The holding call is expected to time out on its own while the worker remains inside the job,
            // because the test holds it longer than one call timeout on purpose. Only the worker's occupancy
            // matters here, so this result is deliberately not asserted.
            // 持有调用预期会自行超时，因为本测试刻意把它持有的时间超过一次调用超时，而此刻工作线程仍在作业内部。
            // 此处只有工作线程被占用这件事重要，因此该结果刻意不做断言。
            let _ = holding_channel.run("hold", move |()| async move {
                let _ = progress_sender.send(());
                let _ = hold_receiver.recv();
                Ok(json!({ "held": true }))
            });
        });
        progress_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("holding job must start");

        // The single worker is busy, so this submission is buffered and then times out. That timeout is what
        // establishes that the queue now holds one entry.
        // 唯一的工作线程正忙，因此这次提交被缓冲随后超时。正是该超时确立了队列中已有一条条目。
        let buffered = channel.run("buffered", |()| async { Ok(json!({ "buffered": true })) });
        assert!(
            buffered.is_err(),
            "a buffered job cannot have run while the only worker was held"
        );

        // With one worker busy and the single queue slot occupied, the next submission is rejected outright.
        // 在一个工作线程忙、唯一队列槽被占用的情况下，下一次提交被直接拒绝。
        let error = channel
            .run("overflow", |()| async { Ok(json!({ "overflow": true })) })
            .expect_err("a full channel must reject work");
        assert!(error.contains("saturated"), "unexpected error: {error}");

        let _ = hold_sender.send(());
        holder.join().expect("join holding job thread");
    }

    /// Verifies closing the channel rejects later callbacks with a closed-channel error.
    /// 验证关闭通道会使后续回调收到已关闭错误。
    #[test]
    fn closing_the_channel_rejects_late_calls() {
        let runtime = runtime();
        let mut channel = HostExecutionChannel::start(runtime.handle().clone(), ());
        channel.close();
        assert!(channel.is_closed());

        let error = channel
            .run("late", |()| async { Ok(json!({ "ok": true })) })
            .expect_err("closed channel must reject work");

        assert!(error.contains("closed"), "unexpected error: {error}");
    }

    /// Verifies a panicking job is reported and does not take its worker down.
    /// 验证 panic 的作业会被报告，且不会带走其工作线程。
    #[test]
    fn a_panicking_job_is_reported_and_the_worker_survives() {
        let runtime = runtime();
        let channel = HostExecutionChannel::start(runtime.handle().clone(), ());

        let error = channel
            // The panicking future never produces a value, so its type must be stated.
            // panic 的 future 从不产生值，因此必须显式给出其类型。
            .run::<serde_json::Value, _, _>("panics", |()| async { panic!("probe panic") })
            .expect_err("panicking job must report an error");
        assert!(error.contains("panicked"), "unexpected error: {error}");

        let value = channel
            .run("after", |()| async { Ok(json!({ "alive": true })) })
            .expect("worker must survive a panicking job");
        assert_eq!(value["alive"], true);
    }

    /// Verifies a job that exceeds its call timeout is reported rather than awaited forever.
    /// 验证超过调用超时的作业会被报告，而不是被永久等待。
    #[test]
    fn a_job_that_exceeds_the_call_timeout_is_reported() {
        let runtime = runtime();
        let mut channel = HostExecutionChannel::start_with_limits(
            runtime.handle().clone(),
            (),
            1,
            4,
            Duration::from_millis(50),
        );

        let error = channel
            .run("slow", |()| async {
                std::thread::sleep(Duration::from_millis(400));
                Ok(json!({ "finished": true }))
            })
            .expect_err("slow job must report a timeout");

        assert!(error.contains("timeout"), "unexpected error: {error}");
        // The timeout bounds the caller's wait, not the job: the worker went on to finish the sleep and return its
        // value. A caller must therefore treat a timeout as an unknown outcome, and the message has to say so.
        // 超时限定的是调用方的等待而非作业：工作线程继续完成睡眠并返回其值。因此调用方必须把超时视为未知结果，
        // 而消息必须说明这一点。
        assert!(
            error.contains("may still complete"),
            "a timeout must not read as a definite failure: {error}"
        );
        channel.close();
    }
}
