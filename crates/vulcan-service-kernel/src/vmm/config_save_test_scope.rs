//! Task-local scheduling control for real global VMM preflight and commit boundaries.
//! 真实全局 VMM 预检及提交边界的任务局部调度控制。

use std::sync::Arc;

/// Coordinates one explicitly scoped save at the selected real service boundary.
/// 在选定的真实服务边界协调一次明确设置作用域的保存。
#[derive(Default)]
pub(crate) struct GlobalSavePause {
    /// Signals that the real save reached the scoped preflight or commit boundary.
    /// 表示真实保存已经到达作用域指定的预检或提交边界。
    pub(crate) entered: tokio::sync::Notify,
    /// Releases only the scoped save so a test can complete another save first.
    /// 只放行当前作用域的保存，使测试可以先完成另一次保存。
    pub(crate) release: tokio::sync::Notify,
}

tokio::task_local! {
    /// Optional coordination owned by the exact test task, never inherited by peer saves.
    /// 精确测试任务持有的可选协调状态，其他保存任务不会继承。
    pub(crate) static GLOBAL_SAVE_COMMIT_PAUSE: Arc<GlobalSavePause>;
    /// Optional coordination after preflight but before acquiring the configuration transaction.
    /// 预检完成后、获取配置事务前的可选协调状态。
    pub(crate) static GLOBAL_SAVE_PREFLIGHT_PAUSE: Arc<GlobalSavePause>;
}

/// Pause only an explicitly scoped test save after preflight and before any durable mutation.
/// 仅在预检结束、任何持久修改发生之前暂停明确设置作用域的测试保存。
/// No input or result; unscoped saves proceed normally and no production state is changed.
/// 无输入输出；未设置作用域的保存正常继续，不改变任何生产状态。
pub(crate) async fn pause_after_preflight() {
    // The borrowed task-local state must be released before waiting on another real service call.
    // 等待另一次真实服务调用前，必须释放借用的任务局部状态。
    if let Ok(pause) = GLOBAL_SAVE_PREFLIGHT_PAUSE.try_with(Arc::clone) {
        pause.entered.notify_one();
        pause.release.notified().await;
    }
}

/// Pause only an explicitly scoped test save after the actual configuration transaction returns.
/// 仅在真实配置事务返回后暂停明确设置作用域的测试保存。
/// No input or result; unscoped saves return immediately without altering their scheduling.
/// 无输入输出；未设置作用域的保存立即返回，不改变其调度。
pub(crate) async fn pause_after_commit() {
    // Clone before awaiting so no task-local borrow or service/configuration lock crosses the pause.
    // 等待前克隆，确保暂停期间不跨越任务局部借用或服务及配置锁。
    if let Ok(pause) = GLOBAL_SAVE_COMMIT_PAUSE.try_with(Arc::clone) {
        pause.entered.notify_one();
        pause.release.notified().await;
    }
}
