//! Manager-owned executor for plugin supervision, independent of every caller's runtime.
//! 由管理器持有的插件监督执行器，独立于任何调用方运行时。
//!
//! Monitors, preparation and drain supervisors outlive the request that started them. Spawning
//! them on the caller's runtime ties their progress to that caller: a caller that later blocks its
//! own thread, or a short-lived runtime that is dropped, would stall or abort supervision that
//! other requests depend on. Every such task therefore runs here.
//! 监控、准备与排空监督者的生命周期长于启动它们的请求。若派生在调用方运行时上，其进度就会
//! 绑定到该调用方：调用方随后阻塞自身线程，或短命运行时被丢弃，都会拖住或中止其他请求所依赖
//! 的监督。因此所有此类任务都在这里运行。

use std::future::Future;
use std::sync::Mutex;

use tokio::runtime::{Handle, Runtime, RuntimeFlavor};

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};

/// Worker threads serving supervision; tasks are short polls and blocking work uses the pool.
/// 服务监督的工作线程数；任务均为短轮询，阻塞工作使用阻塞线程池。
const SUPERVISOR_WORKER_THREADS: usize = 2;

/// Lazily started runtime owned by one manager and shut down with it.
/// 由单个管理器持有、按需启动并随其关闭的运行时。
pub(super) struct PluginSupervisor(Mutex<Option<Runtime>>);

impl PluginSupervisor {
    /// Creates an idle supervisor; no thread starts until the first task is submitted.
    /// 创建空闲监督器；首个任务提交前不启动任何线程。
    pub(super) fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Returns the supervision handle, starting the runtime on first use.
    /// 返回监督句柄，首次使用时启动运行时。
    fn handle(&self) -> Result<Handle, VulcanPluginRuntimeError> {
        self.start(None)
    }

    /// Starts the runtime with a bounded blocking pool so a test can hold preparation work.
    /// 以有界阻塞线程池启动运行时，使测试可以挂起准备工作。
    ///
    /// Fails if supervision already started, since the limit could no longer apply.
    /// 监督已启动时失败，因为上限已无法生效。
    #[cfg(test)]
    pub(super) fn start_with_blocking_limit(
        &self,
        limit: usize,
    ) -> Result<Handle, VulcanPluginRuntimeError> {
        if self
            .0
            .lock()
            .map(|runtime| runtime.is_some())
            .unwrap_or(true)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin supervisor already started.",
            ));
        }
        self.start(Some(limit))
    }

    /// Blocks every supervisor worker for `hold`, returning once all of them are occupied.
    /// 让每个监督工作线程阻塞 `hold`，在全部被占用后返回。
    ///
    /// Tests use it to delay queued supervision deterministically; the caller's thread waits.
    /// 测试借此确定性地延迟已排队监督；调用方线程会等待。
    #[cfg(test)]
    pub(super) fn occupy_workers(
        &self,
        hold: std::time::Duration,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let handle = self.handle()?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(SUPERVISOR_WORKER_THREADS + 1));
        for _ in 0..SUPERVISOR_WORKER_THREADS {
            let barrier = std::sync::Arc::clone(&barrier);
            // A worker blocked at the barrier cannot take a second blocker, so each holds one.
            // 阻塞在屏障处的工作线程不能领取第二个阻塞任务，因此每个线程恰好持有一个。
            handle.spawn(async move {
                barrier.wait();
                std::thread::sleep(hold);
            });
        }
        barrier.wait();
        Ok(())
    }

    /// Returns the running handle or starts the runtime, optionally bounding its blocking pool.
    /// 返回运行中句柄或启动运行时，可选地限制其阻塞线程池。
    fn start(&self, blocking_limit: Option<usize>) -> Result<Handle, VulcanPluginRuntimeError> {
        let mut runtime = self
            .0
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin supervisor lock is poisoned."))?;
        if let Some(runtime) = runtime.as_ref() {
            return Ok(runtime.handle().clone());
        }
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        builder
            .worker_threads(SUPERVISOR_WORKER_THREADS)
            .thread_name("vulcan-plugin-supervisor")
            .enable_all();
        if let Some(limit) = blocking_limit {
            builder.max_blocking_threads(limit);
        }
        let started = builder.build().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Plugin supervisor failed to start: {error}"))
        })?;
        let handle = started.handle().clone();
        *runtime = Some(started);
        Ok(handle)
    }
}

impl Drop for PluginSupervisor {
    /// Stops supervision without waiting, which is valid even on the supervisor's own worker.
    /// 不等待地停止监督，即使在监督器自身工作线程上执行也合法。
    fn drop(&mut self) {
        let runtime = self
            .0
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(runtime) = runtime {
            runtime.shutdown_background();
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Returns the manager-owned supervision executor, starting it on first use.
    /// 返回管理器持有的监督执行器，首次使用时启动。
    pub(super) fn supervisor_handle(&self) -> Result<Handle, VulcanPluginRuntimeError> {
        self.state.supervisor.handle()
    }

    /// Runs `future` to completion from synchronous code, whatever runtime the caller is on.
    /// 从同步代码运行 `future` 至完成，无论调用方处于何种运行时。
    ///
    /// The future runs on the supervisor, so the caller's thread only waits. Inside a multi-thread
    /// runtime the wait is wrapped in `block_in_place`, handing the worker's other tasks to peers.
    /// A panicking future is returned as an error instead of unwinding into the caller.
    /// future 在监督器上运行，调用方线程只负责等待。位于多线程运行时内时，等待包裹在
    /// `block_in_place` 中，把该工作线程的其他任务移交给同伴；future panic 以错误返回，
    /// 不会展开到调用方。
    pub fn block_on_supervised<F>(&self, future: F) -> Result<F::Output, VulcanPluginRuntimeError>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.supervisor_handle()?.spawn(async move {
            let _ = sender.send(future.await);
        });
        // A panic drops the sender during unwinding, which the receiver observes as disconnection.
        // panic 会在展开时丢弃发送端，接收端将其观测为断开。
        let wait = move || {
            receiver
                .recv()
                .map_err(|_| VulcanPluginRuntimeError::new("Supervised plugin task panicked."))
        };
        match Handle::try_current().map(|handle| handle.runtime_flavor()) {
            Ok(RuntimeFlavor::MultiThread) => tokio::task::block_in_place(wait),
            _ => wait(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a content-only manager whose supervisor starts on demand.
    /// 构建一个仅含内容能力、监督器按需启动的管理器。
    fn manager(root: &std::path::Path) -> VulcanPluginRuntimeManager {
        VulcanPluginRuntimeManager::without_runtime(
            root.join("plugins"),
            root.join("user-plugins"),
            root.join("bridge"),
            root.join("distribution"),
            root.join("state"),
            root.join("runtime"),
        )
    }

    /// The sync bridge must finish without a runtime and from both runtime flavors, including a
    /// current-thread caller whose own thread is blocked while it waits.
    /// 同步桥必须在无运行时与两种运行时内完成，包括等待期间自身线程被阻塞的单线程调用方。
    #[test]
    fn block_on_supervised_completes_in_every_caller_context() {
        let root = tempfile::tempdir().expect("create supervisor test root");
        let manager = manager(root.path());
        assert_eq!(manager.block_on_supervised(async { 1 }).ok(), Some(1));
        for mut builder in [
            tokio::runtime::Builder::new_current_thread(),
            tokio::runtime::Builder::new_multi_thread(),
        ] {
            let runtime = builder.enable_all().build().expect("build caller runtime");
            runtime.block_on(async {
                // Supervision spawned by an earlier async request must keep running while this
                // caller blocks, which a caller-bound task on a current-thread runtime cannot.
                // 早先异步请求派生的监督必须在此调用方阻塞时继续运行；绑定在单线程调用方运行时上的
                // 任务做不到这一点。
                let (ready, observed) = tokio::sync::oneshot::channel();
                manager
                    .supervisor_handle()
                    .expect("start supervisor")
                    .spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        let _ = ready.send(());
                    });
                let waited = manager
                    .block_on_supervised(async move { observed.await.is_ok() })
                    .expect("supervised wait");
                assert!(waited);
            });
        }
    }

    /// A panicking supervised future is reported to the synchronous caller as an error.
    /// 受监督 future 的 panic 以错误形式报告给同步调用方。
    #[test]
    fn block_on_supervised_reports_panics() {
        let root = tempfile::tempdir().expect("create supervisor test root");
        let manager = manager(root.path());
        let panicked = manager.block_on_supervised(async {
            panic!("supervised panic fixture");
        });
        assert!(panicked.is_err());
        assert_eq!(manager.block_on_supervised(async { 2 }).ok(), Some(2));
    }

    /// Dropping the last manager handle from a supervision task must not panic the runtime.
    /// 在监督任务中丢弃最后一个管理器句柄不得令运行时 panic。
    #[test]
    fn manager_can_be_released_from_its_own_supervisor() {
        let root = tempfile::tempdir().expect("create supervisor test root");
        let manager = manager(root.path());
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let owned = manager.clone();
        manager
            .supervisor_handle()
            .expect("start supervisor")
            .spawn(async move {
                drop(owned);
                let _ = sender.send(());
            });
        drop(manager);
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("supervision task completes after releasing the manager");
    }
}
