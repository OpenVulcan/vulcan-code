//! Bounded blocking execution for complete project-configuration transactions.
//! 完整项目配置事务的有界阻塞执行。

use std::sync::{Arc, LazyLock};
use tokio::sync::Semaphore;

/// Bound concurrent project configuration transactions across independent files.
/// 限制不同项目配置文件同时执行的事务数量。
const MAX_CONFIGURATION_IO_TASKS: usize = 2;
/// Process-owned capacity remains with each real worker through durable and runtime commit.
/// 进程级容量由实际工作线程持有直至持久化与运行时提交完成。
static ADMISSION: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(MAX_CONFIGURATION_IO_TASKS)));

/// Runs owned operation on bounded blocking workers; returns its result or an explicit task failure.
/// 在有界阻塞线程上执行自有 operation；返回操作结果或明确的任务失败。
pub(super) async fn run<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let permit = Arc::clone(&ADMISSION)
        .acquire_owned()
        .await
        .map_err(|error| format!("configuration I/O admission failed: {error}"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation()
    })
    .await
    .map_err(|error| format!("configuration transaction did not complete: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::super::{begin_project_configuration_transaction, mutate_project_configuration};
    use std::time::Duration;

    /// A cancelled waiter must not unlock a transaction that is still publishing its durable state.
    /// 等待方取消不能解锁仍在发布持久状态的事务。
    #[tokio::test]
    async fn cancellation_keeps_file_ownership_until_disk_and_runtime_commit_finish() {
        let root = tempfile::tempdir().expect("isolated configuration");
        let workspace = root.path().to_path_buf();
        let config_path =
            vulcan_provider_config::config::project_configuration_file_path(&workspace);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (committed_tx, committed_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            mutate_project_configuration(
                &workspace,
                "test",
                move |configuration| {
                    started_tx.send(()).expect("test owns start receiver");
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("test controls transaction release");
                    configuration.vmm.enabled = Some(false);
                    Ok(())
                },
                move |_| {
                    let _ = committed_tx.send(());
                    Ok(())
                },
            )
            .await
        });
        started_rx.await.expect("blocking transaction started");
        task.abort();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                begin_project_configuration_transaction(&config_path)
            )
            .await
            .is_err(),
            "cancellation must not release a running configuration transaction"
        );
        release_tx.send(()).expect("release blocking operation");
        tokio::time::timeout(Duration::from_secs(5), committed_rx)
            .await
            .expect("commit remains owned")
            .expect("commit delivered");
        let _guard = begin_project_configuration_transaction(&config_path)
            .await
            .expect("ownership released after commit");
        let loaded = vulcan_provider_config::config::load_project_configuration_file(root.path())
            .expect("durable value");
        assert_eq!(loaded.vmm.enabled, Some(false));
    }
}
