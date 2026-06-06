use crate::task_entry::MainSessionTaskEntry;
use vulcan_session_core::{RootSessionId, SessionNodeStore, SessionStoreError};

/// Main-session private storage contract.
/// 主会话私有存储契约。
pub trait MainSessionStore: SessionNodeStore {
    /// Appends one child task entry to the parent main session index.
    /// 向父级主会话索引追加一个子代理入口。
    fn append_task_entry(&self, entry: MainSessionTaskEntry) -> Result<(), SessionStoreError>;

    /// Lists task entries owned by one root main session.
    /// 列出某个根主会话拥有的子代理入口。
    fn list_task_entries(
        &self,
        root_session_id: RootSessionId,
    ) -> Result<Vec<MainSessionTaskEntry>, SessionStoreError>;
}
