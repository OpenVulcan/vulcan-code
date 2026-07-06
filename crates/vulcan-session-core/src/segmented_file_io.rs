//! Shared segmented-session file IO coordination.
//! 共享分段会话文件 IO 协调。

use std::sync::{Mutex, OnceLock};

/// Process-local mutex protecting segmented JSONL append read/modify/write operations.
/// 保护分段 JSONL 追加读改写操作的进程内互斥锁。
static SEGMENTED_APPEND_FILE_IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Returns the shared process-local lock for segmented JSONL append operations.
/// 返回分段 JSONL 追加操作共享的进程内锁。
///
/// # Returns
/// 返回值
/// A process-local mutex shared by main-session and task-session segmented stores.
/// 返回主会话与子任务会话分段存储共享的进程内互斥锁。
pub fn segmented_append_file_io_lock() -> &'static Mutex<()> {
    SEGMENTED_APPEND_FILE_IO_LOCK.get_or_init(|| Mutex::new(()))
}
