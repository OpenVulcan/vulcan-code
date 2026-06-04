//! Runtime-stream intervention hooks shared by host UI frontends.
//! 供宿主 UI 前端共享的运行时流干预钩子。

use crate::model::SessionId;
use std::sync::Arc;
use vulcan_core::controller::VulcanCoreController;

/// Host callback that may append replay-safe guidance before the next provider request.
/// 宿主回调，可在下一次 Provider 请求前追加可回放安全的引导。
///
/// # Parameters
/// 参数
/// `SessionId` identifies the session whose root stream reached a safe tool boundary.
/// `SessionId` 标识根流已经到达安全工具边界的会话。
/// `VulcanCoreController` is the controller snapshot owner that can accept an interjected user turn.
/// `VulcanCoreController` 是可接收插入用户回合的控制器快照拥有者。
///
/// # Returns
/// 返回值
/// `true` when the callback changed the controller snapshot and a checkpoint should be emitted.
/// 当回调修改了控制器快照且需要发出检查点时返回 `true`。
pub type RuntimePromptIntervention =
    Arc<dyn Fn(SessionId, &mut VulcanCoreController) -> bool + Send + Sync>;
