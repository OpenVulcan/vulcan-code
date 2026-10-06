//! Real subprocess regressions distinguish borrowed validation cancellation from controller abandonment.
//! 真实子进程回归区分借用校验取消与控制对象被放弃。
use super::*;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;

/// Polls an actual pinned future once and requires a pending result without a timing assumption.
/// 轮询实际固定 Future 一次，并要求等待结果，不依赖计时假设。
/// Future is the borrowed operation under test; returns after proving it reached a cancellable wait.
/// future 为待测借用操作；证明其已进入可取消等待后返回。
async fn assert_pending_once<F: Future>(mut future: Pin<&mut F>) {
    // Use the executor's real task context so the tested operation installs its normal wake registrations.
    // 使用执行器的真实任务上下文，使待测操作安装正常唤醒注册。
    poll_fn(|context| {
        assert!(
            future.as_mut().poll(context).is_pending(),
            "the original stalled operation must reach an actual pending wait"
        );
        Poll::Ready(())
    })
    .await;
}

/// Checks the exact native PID written by the existing immutable candidate fixture.
/// 检查现有不可变候选夹具记录的精确原生 PID。
/// Marker contains one completed child startup record; returns whether the operating system still sees that process.
/// marker 包含一条完整子进程启动记录；返回操作系统是否仍可见该进程。
fn original_process_is_alive(marker: &std::path::Path) -> bool {
    // Read only the fixture's actual startup record, whose completed newline was already verified by its parent helper.
    // 仅读取夹具实际启动记录，其完成换行已由父级辅助函数验证。
    let bytes = fs::read_to_string(marker).expect("original child startup record");
    // Select the sole record by the fixture invariant rather than an evolving catalog position.
    // 按夹具唯一记录约束选择，不使用会演进的目录位置。
    let record: Value = serde_json::from_str(
        bytes
            .lines()
            .next()
            .expect("expected original startup record may have changed"),
    )
    .expect("original startup JSON");
    // The child itself supplies its native PID; no runtime diagnostic is treated as process existence.
    // 子进程自身提供原生 PID；不把运行时诊断当作进程存在证据。
    let pid = record["pid"]
        .as_u64()
        .expect("actual child PID must be an unsigned integer")
        .to_string();
    // Signal zero queries process existence without stopping, restarting or sending protocol work to it.
    // 信号零查询进程是否存在，不停止、重启或向其发送协议工作。
    let output = std::process::Command::new("node")
        .args(["-e", "process.kill(Number(process.argv[1]), 0)", &pid])
        .output()
        .expect("Node.js is required by the existing native candidate fixture");
    output.status.success()
}

/// Cancelling borrowed service validation preserves the same controller, native child and original transaction fence.
/// 取消借用服务校验保留同一控制对象、原生子进程及原事务隔离。
/// No inputs or return; only explicit original shutdown and actual release permit the original abort.
/// 无输入与返回值；仅显式原关闭及实际释放允许原事务中止。
#[tokio::test]
async fn retained_validation_cancellation_preserves_original_process_and_request() {
    // A declared stalled initialize request keeps readiness genuinely pending.
    // 声明的停滞初始化请求使就绪保持真实等待。
    let mut fixture = fixture("stall");
    // Copy original identity without acquiring another readiness or package lease.
    // 复制原身份，不获取其他就绪或插件包租约。
    let request_id = fixture.transaction.record().request_id();
    // Keep the service-owned controller outside the cancellable borrowed operation.
    // 将服务自有控制对象保留在可取消借用操作之外。
    let control = CandidatePluginMcpRuntime::start(fixture.prepared).expect("original admission");
    // Keep diagnostics and the original release receipt independently of validation.
    // 独立于校验保留诊断及原释放回执。
    let observer = control.observer();
    assert_started_once(&fixture.marker).await;
    assert!(original_process_is_alive(&fixture.marker));
    // Pin and actually poll the borrowed future before cancelling it by destruction.
    // 固定并实际轮询借用 Future，再通过销毁取消。
    let mut validation = Box::pin(control.validate_and_release_retained());
    assert_pending_once(validation.as_mut()).await;
    drop(validation);
    assert_eq!(control.observer().request_id(), request_id);
    assert_eq!(observer.request_id(), request_id);
    // A stale shutdown latch would make this readiness observation immediately fail rather than remain pending.
    // 过期关闭锁存会使此就绪观测立即失败，而不会保持等待。
    let mut readiness = Box::pin(observer.wait_until_ready(OBSERVE));
    assert_pending_once(readiness.as_mut()).await;
    drop(readiness);
    // Actual release must remain pending while the original child and controller still own the attempt.
    // 原子进程及控制对象仍拥有尝试时，实际释放必须保持等待。
    let mut release = Box::pin(observer.wait_until_released(OBSERVE));
    assert_pending_once(release.as_mut()).await;
    drop(release);
    assert_started_once(&fixture.marker).await;
    assert!(original_process_is_alive(&fixture.marker));
    assert_eq!(
        fixture
            .store
            .commit_activation_transaction(&mut fixture.transaction)
            .expect_err("borrowed cancellation must retain the original readiness lease")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        fixture
            .store
            .abort_activation_transaction(&mut fixture.transaction)
            .expect_err("borrowed cancellation cannot unlock the original transaction")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    control.shutdown();
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("explicit original shutdown releases actual owners");
    assert!(!original_process_is_alive(&fixture.marker));
    assert_started_once(&fixture.marker).await;
    fixture
        .store
        .abort_activation_transaction(&mut fixture.transaction)
        .expect("actual release permits the exact original abort with control still retained");
    assert_eq!(control.observer().request_id(), request_id);
}

/// Borrowed validation completes real readiness, shutdown and release without requiring the controller to be dropped.
/// 借用校验完成真实就绪、关闭及释放，不要求销毁控制对象。
/// No inputs or return; successful and failed native attempts preserve their original outcome and release the original fence.
/// 无输入与返回值；成功及失败原生尝试保留原结果，并释放原隔离。
#[tokio::test]
async fn retained_validation_ready_and_failure_release_while_controller_remains_alive() {
    // Each declared protocol mode receives a separate immutable request and native child.
    // 每个声明协议模式获得独立不可变请求及原生子进程。
    for mode in ["ready", "fail"] {
        // Keep this original transaction for a direct post-release abort.
        // 保留此原事务，以直接验证释放后的中止。
        let mut fixture = fixture(mode);
        // The retained controller is deliberately used again after the validation result.
        // 校验结果返回后刻意再次使用保留的控制对象。
        let control =
            CandidatePluginMcpRuntime::start(fixture.prepared).expect("original admission");
        // Preserve original diagnostics without retaining its actual package authority.
        // 保留原诊断，不保留实际插件包权威。
        let observer = control.observer();
        // Capture the real readiness outcome before validation requests shutdown.
        // 在校验请求关闭前捕获真实就绪结果。
        let original_readiness = observer.wait_until_ready(OBSERVE).await;
        assert_eq!(original_readiness.is_ok(), mode == "ready");
        // Each production validation boundary already uses its own authoritative readiness budget.
        // 每个生产校验边界已经使用其自身权威就绪预算。
        // A single shorter observer deadline cannot stand in for all three sequential validation boundaries.
        // 单个更短的观察期限不能替代三个连续校验边界。
        let result = control.validate_and_release_retained().await;
        assert_eq!(result.is_ok(), mode == "ready", "actual result: {result:?}");
        if let Err(original_error) = original_readiness {
            // Compare the actual original failure text so release cannot hide or replace readiness failure.
            // 比较实际原失败文本，确保释放不能隐藏或替换就绪失败。
            let validation_error = result.expect_err("original protocol failure stays observable");
            assert!(validation_error.contains("readiness:"));
            assert!(
                validation_error.contains(&original_error),
                "actual original failure must survive release: {validation_error}"
            );
        }
        observer
            .wait_until_released(OBSERVE)
            .await
            .expect("original resources really released");
        assert_started_once(&fixture.marker).await;
        assert!(!original_process_is_alive(&fixture.marker));
        assert_eq!(
            control.observer().request_id(),
            fixture.transaction.record().request_id()
        );
        fixture
            .store
            .abort_activation_transaction(&mut fixture.transaction)
            .expect("retained controller and observer must not retain the released package lease");
    }
}

/// Cancelling the original consuming helper still drops its controller and requests cleanup of the original native child.
/// 取消原消费式辅助入口仍销毁其控制对象，并请求清理原原生子进程。
/// No inputs or return; actual original release permits abort without replay or a second process start.
/// 无输入与返回值；实际原释放允许中止，不重放或二次启动进程。
#[tokio::test]
async fn consuming_validation_cancellation_still_shuts_down_original_process() {
    // An actual stalled child prevents the consuming future from completing before cancellation.
    // 真实停滞子进程阻止消费式 Future 在取消前完成。
    let mut fixture = fixture("stall");
    // This controller is consumed by the validating API rather than retained by the caller.
    // 此控制对象由校验接口消费，不由调用者保留。
    let control = CandidatePluginMcpRuntime::start(fixture.prepared).expect("original admission");
    // Observe the same original release after the consumed controller disappears.
    // 消费的控制对象消失后观测同一原释放。
    let observer = control.observer();
    assert_started_once(&fixture.marker).await;
    assert!(original_process_is_alive(&fixture.marker));
    // Actual first polling places controller ownership inside the suspended consuming future.
    // 实际首次轮询将控制所有权放入挂起的消费式 Future。
    let mut validation = Box::pin(control.validate_and_release());
    assert_pending_once(validation.as_mut()).await;
    assert_eq!(
        fixture
            .store
            .abort_activation_transaction(&mut fixture.transaction)
            .expect_err("actual native work must retain the request until cancellation cleanup")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(validation);
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("consuming cancellation still requests original actual cleanup");
    assert!(!original_process_is_alive(&fixture.marker));
    assert_started_once(&fixture.marker).await;
    fixture
        .store
        .abort_activation_transaction(&mut fixture.transaction)
        .expect("consuming cancellation releases the exact original transaction");
}
