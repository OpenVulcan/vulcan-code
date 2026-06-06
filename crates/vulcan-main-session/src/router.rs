use crate::task_entry::MainSessionTaskEntry;
use vulcan_session_core::{RootSessionId, SessionNodeRouter, SessionRouteError};

/// Main-session private router contract.
/// 主会话私有路由契约。
pub trait MainSessionRouter: SessionNodeRouter {
    /// Lists child task entries owned by one root main session.
    /// 列出某个根主会话拥有的子代理入口。
    fn list_root_task_entries(
        &self,
        root_session_id: RootSessionId,
    ) -> Result<Vec<MainSessionTaskEntry>, SessionRouteError>;
}
