//! Regression tests for shared transport-close ownership and observation.
//! 共享传输关闭所有权与观测的回归测试。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::sync::oneshot;

use super::{ShutdownResult, TransportShutdown};
use crate::McpRuntimeError;

/// A controlled close future with explicit entry, release, and resource-retirement signals.
/// 带明确进入、释放和资源退役信号的受控关闭任务。
struct ControlledClose {
    /// Owner shared only with test observers, never with its own close future.
    /// 仅与测试观察者共享的所有者，不与自身关闭任务共享。
    operation: Arc<TransportShutdown>,
    /// Entry proves the worker has acquired its real resource.
    /// 进入信号证明工作任务已取得真实资源。
    entered: oneshot::Receiver<()>,
    /// Explicit release keeps cleanup pending independently of the observer.
    /// 显式释放让清理能够独立于观察者保持等待。
    release: oneshot::Sender<()>,
    /// Retirement proves the worker has dropped the resource after release.
    /// 退役信号证明工作任务在放行后已丢弃资源。
    retired: oneshot::Receiver<()>,
    /// Weak resource handle detects premature destruction without extending its lifetime.
    /// 弱资源句柄检测过早析构，不延长其生命周期。
    resource: Weak<()>,
    /// Counter proves all observers share exactly one execution.
    /// 计数器证明所有观察者仅共享一次执行。
    starts: Arc<AtomicUsize>,
}

/// Builds a gated close operation that returns the supplied terminal result.
/// 构建返回指定终态结果的门控关闭操作。
///
/// The result is owned by the worker; returns all observation and release controls.
/// 结果由工作任务持有；返回全部观测及放行控制。
fn controlled_close(result: ShutdownResult) -> ControlledClose {
    // The future is the sole strong owner of the resource under test.
    // 此任务是被测资源唯一的强所有者。
    let resource = Arc::new(());
    let weak_resource = Arc::downgrade(&resource);
    // Independent one-shot channels make progress assertions deterministic.
    // 独立单次通道使进展断言确定化。
    let (entered_tx, entered) = oneshot::channel();
    let (release, release_rx) = oneshot::channel();
    let (retired_tx, retired) = oneshot::channel();
    // Counts execution rather than observation.
    // 统计执行次数而不是观测次数。
    let starts = Arc::new(AtomicUsize::new(0));
    let worker_starts = Arc::clone(&starts);
    let operation = Arc::new(TransportShutdown::new("test.close", async move {
        worker_starts.fetch_add(1, Ordering::SeqCst);
        entered_tx.send(()).expect("entry observer is retained");
        release_rx
            .await
            .expect("test must explicitly release cleanup");
        drop(resource);
        retired_tx
            .send(())
            .expect("retirement observer is retained");
        result
    }));
    ControlledClose {
        operation,
        entered,
        release,
        retired,
        resource: weak_resource,
        starts,
    }
}

/// Waits for a deterministic worker signal within a finite regression-test deadline.
/// 在有限回归测试截止时间内等待确定性工作信号。
///
/// The receiver identifies the phase; returns after delivery and fails on interruption or timeout.
/// 接收端标识阶段；收到信号后返回，中断或超时则失败。
async fn signal(receiver: oneshot::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(5), receiver)
        .await
        .expect("close worker must progress")
        .expect("close worker must publish its signal");
}

/// A second concurrent observer cannot report success while the original close is pending.
/// 原关闭仍在进行时，第二个并发观察者不能报告成功。
/// No inputs or return value; checks one execution and the eventual common result.
/// 无输入与返回值；校验单次执行及最终公共结果。
#[tokio::test]
async fn concurrent_observers_wait_for_the_same_close() {
    // The first observer starts cleanup and remains pending at the explicit gate.
    // 第一个观察者启动清理，并在显式门控处保持等待。
    let controlled = controlled_close(Ok(()));
    let first_owner = Arc::clone(&controlled.operation);
    let first = tokio::spawn(async move { first_owner.wait().await });
    signal(controlled.entered).await;
    // A later observer must time out instead of interpreting a taken service slot as completion.
    // 后到观察者必须超时，不能把已取走的服务槽位解释为完成。
    let pending = controlled
        .operation
        .wait_for(Duration::from_millis(10))
        .await
        .expect_err("concurrent close must remain pending");
    assert!(pending.to_string().contains("cleanup remains pending"));
    assert!(!first.is_finished());
    assert!(controlled.resource.upgrade().is_some());
    assert_eq!(controlled.starts.load(Ordering::SeqCst), 1);
    controlled.release.send(()).expect("release live worker");
    signal(controlled.retired).await;
    first.await.expect("observer join").expect("close result");
    controlled
        .operation
        .wait()
        .await
        .expect("same completed result");
    assert_eq!(controlled.starts.load(Ordering::SeqCst), 1);
    assert!(controlled.resource.upgrade().is_none());
}

/// Observation timeouts preserve the original worker and its retained resources.
/// 观测超时保留原工作任务及其持有的资源。
/// No inputs or return value; the eventual completion remains available without restarting.
/// 无输入与返回值；最终结果仍可查询且不会重新启动。
#[tokio::test]
async fn timed_out_close_can_be_observed_again() {
    // A short caller budget does not limit the actual cleanup lifetime.
    // 较短调用预算不限制实际清理寿命。
    let controlled = controlled_close(Ok(()));
    controlled
        .operation
        .wait_for(Duration::from_millis(10))
        .await
        .expect_err("observer must time out at the gate");
    signal(controlled.entered).await;
    assert!(controlled.resource.upgrade().is_some());
    controlled.release.send(()).expect("release after timeout");
    controlled
        .operation
        .wait()
        .await
        .expect("observe original completion");
    signal(controlled.retired).await;
    assert_eq!(controlled.starts.load(Ordering::SeqCst), 1);
    assert!(controlled.resource.upgrade().is_none());
}

/// Cancelling the last observer and dropping its owner must not discard the worker's resource.
/// 取消最后一个观察者并丢弃其所有者，不能丢弃工作任务的资源。
/// No inputs or return value; explicitly releases and verifies the surviving worker.
/// 无输入与返回值；显式放行并验证仍存活的工作任务。
#[tokio::test]
async fn cancelled_observer_and_dropped_owner_preserve_cleanup() {
    // The observer owns a clone only until it is explicitly aborted.
    // 观察者仅在显式中止前持有一个克隆。
    let controlled = controlled_close(Ok(()));
    let observer_owner = Arc::clone(&controlled.operation);
    let observer = tokio::spawn(async move { observer_owner.wait().await });
    signal(controlled.entered).await;
    observer.abort();
    assert!(
        observer
            .await
            .expect_err("observer cancelled")
            .is_cancelled()
    );
    drop(controlled.operation);
    assert!(controlled.resource.upgrade().is_some());
    controlled
        .release
        .send(())
        .expect("cleanup survives all observers");
    signal(controlled.retired).await;
    assert_eq!(controlled.starts.load(Ordering::SeqCst), 1);
    assert!(controlled.resource.upgrade().is_none());
}

/// Destruction outside a Tokio context still schedules the retained close exactly once.
/// 在 Tokio 上下文之外析构时，仍须仅调度一次所保留的关闭。
/// No inputs or return value; checks the captured runtime and pending-owner drop path.
/// 无输入与返回值；校验捕获的执行器和待关闭所有者析构路径。
#[tokio::test]
async fn dropping_pending_owner_starts_cleanup_outside_runtime_context() {
    // Move only the owner to a plain thread; the runtime remains alive in this test.
    // 仅把所有者移至普通线程；当前测试中的执行器保持存活。
    let controlled = controlled_close(Ok(()));
    std::thread::spawn(move || drop(controlled.operation))
        .join()
        .expect("plain-thread destruction");
    signal(controlled.entered).await;
    assert!(controlled.resource.upgrade().is_some());
    controlled
        .release
        .send(())
        .expect("release drop-started cleanup");
    signal(controlled.retired).await;
    assert_eq!(controlled.starts.load(Ordering::SeqCst), 1);
    assert!(controlled.resource.upgrade().is_none());
}

/// A failed close remains failed for every later observer and is never silently retried.
/// 关闭失败对所有后续观察者仍为失败，且绝不静默重试。
/// No inputs or return value; checks the exact original error and one execution.
/// 无输入与返回值；校验精确原始错误及单次执行。
#[tokio::test]
async fn close_failure_is_retained_after_timeout() {
    // The delayed failure differs from the observation timeout.
    // 延迟发生的失败与观测超时不同。
    let controlled = controlled_close(Err(McpRuntimeError::transport("actual close failure")));
    controlled
        .operation
        .wait_for(Duration::from_millis(10))
        .await
        .expect_err("first observation timeout");
    signal(controlled.entered).await;
    controlled
        .release
        .send(())
        .expect("release failing cleanup");
    signal(controlled.retired).await;
    for _ in 0..2 {
        assert_eq!(
            controlled
                .operation
                .wait()
                .await
                .expect_err("close failed")
                .to_string(),
            "actual close failure"
        );
    }
    assert_eq!(controlled.starts.load(Ordering::SeqCst), 1);
}

/// A panicking worker cannot close the result channel and be mistaken for successful cleanup.
/// 工作任务恐慌导致结果通道关闭时，不能被误判为清理成功。
/// No inputs or return value; all subsequent observations report interruption.
/// 无输入与返回值；全部后续观测均报告执行中断。
#[tokio::test]
async fn interrupted_worker_is_never_reported_as_success() {
    // This deliberate task panic exercises the unpublished-result path.
    // 此处故意触发任务恐慌，验证结果尚未发布的路径。
    let operation = TransportShutdown::new("test.panicking.close", async {
        panic!("intentional cleanup task panic");
    });
    for _ in 0..2 {
        assert_eq!(
            operation
                .wait()
                .await
                .expect_err("interrupted close")
                .to_string(),
            "test.panicking.close interrupted before close completion"
        );
    }
}

/// Executor shutdown remains an explicit interruption when observed from a different runtime.
/// 在另一个执行器中观测时，原执行器关闭仍为明确中断。
/// No inputs or return value; verifies no implicit cleanup restart on a replacement executor.
/// 无输入与返回值；验证不会在替代执行器上隐式重启清理。
#[test]
fn runtime_shutdown_does_not_fabricate_a_completed_close() {
    // This runtime owns the real close worker and is explicitly destroyed below.
    // 此执行器拥有真实关闭工作任务，并在下方显式销毁。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("original runtime");
    // Count the worker's first poll independently of all observers.
    // 独立于全部观察者统计工作任务的首次轮询。
    let starts = Arc::new(AtomicUsize::new(0));
    let worker_starts = Arc::clone(&starts);
    let operation = runtime.block_on(async {
        TransportShutdown::new("test.runtime.close", async move {
            worker_starts.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<ShutdownResult>().await
        })
    });
    operation.request();
    runtime.block_on(tokio::task::yield_now());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    drop(runtime);

    // Observation uses another live executor, but ownership remains with the original operation.
    // 观测使用另一个存活执行器，但所有权仍属于原操作。
    let observer_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("observer runtime");
    assert_eq!(
        observer_runtime
            .block_on(operation.wait())
            .expect_err("original executor stopped")
            .to_string(),
        "test.runtime.close interrupted before close completion"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

/// Lifecycle ownership survives the production observation timeout and receives the actual final result.
/// 生命周期所有权跨越生产观测超时，并取得真正的最终结果。
/// No inputs or return value; verifies late success and late failure without restarting cleanup.
/// 无输入与返回值；验证迟到成功及迟到失败，不重新启动清理。
#[tokio::test]
async fn final_close_wait_survives_the_standard_observation_deadline() {
    for failed in [false, true] {
        // The gate remains closed throughout the original five-second observation window.
        // 门控在原五秒观测窗口内始终关闭。
        let expected = if failed {
            Err(McpRuntimeError::transport("late cleanup failure"))
        } else {
            Ok(())
        };
        let controlled = controlled_close(expected);
        // The runtime-style waiter owns the final result independently of the bounded observer.
        // 模拟运行时的等待者独立于有界观察者持有最终结果。
        let lifecycle_owner = Arc::clone(&controlled.operation);
        let lifecycle = tokio::spawn(async move { lifecycle_owner.wait_until_complete().await });
        signal(controlled.entered).await;
        assert!(
            controlled
                .operation
                .wait()
                .await
                .expect_err("production observer must time out")
                .to_string()
                .contains("cleanup remains pending")
        );
        assert!(!lifecycle.is_finished());
        assert!(controlled.resource.upgrade().is_some());
        controlled.release.send(()).expect("release actual cleanup");
        signal(controlled.retired).await;
        // Final failure is retained instead of being confused with the expired observation window.
        // 最终失败被保留，不与已经结束的观测窗口混淆。
        let result = lifecycle.await.expect("lifecycle observer join");
        if failed {
            assert_eq!(
                result.expect_err("late failure").to_string(),
                "late cleanup failure"
            );
        } else {
            result.expect("late successful close");
        }
        assert_eq!(controlled.starts.load(Ordering::SeqCst), 1);
        assert!(controlled.resource.upgrade().is_none());
    }
}
