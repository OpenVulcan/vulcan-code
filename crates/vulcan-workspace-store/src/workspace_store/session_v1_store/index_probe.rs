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

/// Limit concurrent complete index filesystem passes across workspaces in this process.
/// 限制本进程跨工作区同时执行的完整索引文件系统检查数量。
const MAX_WORKSPACE_METADATA_PASSES: usize = 2;
static METADATA_PASSES: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(MAX_WORKSPACE_METADATA_PASSES)));

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
    let permit = Arc::clone(&METADATA_PASSES)
        .acquire_owned()
        .await
        .map_err(|error| format!("workspace metadata admission failed: {error}"))?;
    tokio::task::spawn_blocking(move || {
        // Keep admission until the actual blocking work ends, even when its caller is cancelled.
        // 即使调用方取消，也在真实阻塞工作结束前保留准入占用。
        let _permit = permit;
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
    .map_err(|error| format!("workspace metadata inspection did not complete: {error}"))
}
