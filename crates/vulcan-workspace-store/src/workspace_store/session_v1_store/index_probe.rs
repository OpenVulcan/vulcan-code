//! Bounded metadata probes reuse already-read locators without a second SQL lookup.
//! 有界元数据检查复用已读取的定位行，不再次查询 SQL。

use super::{
    SessionIndexV1Row, SessionV1StoreError, exact_metadata_file_exists, storage_locator_from_row,
};
use crate::model::SessionId;
use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
};
use tokio::sync::Semaphore;

/// Limit simultaneous session storage operations, including complete metadata passes.
/// 限制同时执行的会话存储操作，包括完整元数据检查。
const MAX_SESSION_STORAGE_IO_TASKS: usize = 2;
/// Share the capacity across full index scans and individual storage operations.
/// 完整索引扫描与单次存储操作共享容量。
static STORAGE_IO_TASKS: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(MAX_SESSION_STORAGE_IO_TASKS)));

pub(in super::super) struct InspectedRow {
    pub session_id: SessionId,
    pub row: SessionIndexV1Row,
    pub metadata_exists: Result<bool, SessionV1StoreError>,
}

pub(in super::super) async fn inspect(
    data_root: PathBuf,
    rows: Vec<(SessionId, SessionIndexV1Row)>,
) -> Result<Vec<InspectedRow>, String> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    run_io(move || {
        rows.into_iter()
            .map(|(session_id, row)| {
                let metadata_exists = storage_locator_from_row(&data_root, row.clone())
                    .map_err(SessionV1StoreError::from)
                    .and_then(|locator| exact_metadata_file_exists(&locator));
                InspectedRow {
                    session_id,
                    row,
                    metadata_exists,
                }
            })
            .collect()
    })
    .await
}

/// Keep the shared disk budget until real I/O ends, even when an async caller stops waiting.
/// 即使异步调用方不再等待，也在真实 I/O 结束前保留共享磁盘预算。
pub(super) async fn run_io<T: Send + 'static>(
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let permit = Arc::clone(&STORAGE_IO_TASKS)
        .acquire_owned()
        .await
        .map_err(|error| format!("session storage I/O admission failed: {error}"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation()
    })
    .await
    .map_err(|error| {
        format!("session storage I/O did not complete; outcome must be rechecked: {error}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Cancelled waiters cannot admit more disk work until the real workers finish.
    /// 取消等待方后，必须等待实际线程结束才能准入更多磁盘工作。
    #[tokio::test]
    async fn cancelled_storage_waiters_keep_the_shared_disk_budget() {
        // Saturate the declared capacity with independently controlled operations.
        // 使用可独立控制的操作占用全部声明容量。
        let mut releases = Vec::new();
        for _ in 0..MAX_SESSION_STORAGE_IO_TASKS {
            let (started_tx, started_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let waiter = tokio::spawn(run_io(move || {
                started_tx.send(()).expect("worker started");
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .expect("release blocking disk fixture");
            }));
            started_rx.await.expect("executor remains responsive");
            waiter.abort();
            assert!(
                waiter
                    .await
                    .expect_err("cancel waiting task")
                    .is_cancelled()
            );
            releases.push(release_tx);
        }
        // A new caller remains pending even though all prior asynchronous waiters were cancelled.
        // 即使全部先前异步等待方均已取消，新的调用方仍须等待。
        let mut next = tokio::spawn(run_io(|| ()));
        let remained_queued = tokio::time::timeout(Duration::from_millis(30), &mut next)
            .await
            .is_err();
        for release in releases {
            release.send(()).expect("release actual worker");
        }
        assert!(
            remained_queued,
            "cancelled waiters must not release active disk capacity"
        );
        tokio::time::timeout(Duration::from_secs(5), next)
            .await
            .expect("capacity recovered")
            .expect("join new caller")
            .expect("disk operation completed");
    }
}
