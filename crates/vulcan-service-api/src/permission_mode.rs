//! Session-owned permission mode commands and durable receipts.
//! 会话自有权限模式命令与可靠回执。

use serde::{Deserialize, Serialize};
use vulcan_session_core::v1::config::PermissionMode;

/// Permission mode update shared by WebUI and TUI.
/// WebUI 与 TUI 共用的权限模式更新请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionPermissionModeUpdateRequest {
    /// Exact existing regular session.
    /// 精确的已有普通会话。
    pub session_id: String,
    /// Mode read by the next tool call of this session.
    /// 本会话下一次工具调用读取的模式。
    pub mode: PermissionMode,
}

/// Durable permission mode result captured by the unique session writer.
/// 唯一会话写入器捕获的可靠权限模式结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPermissionModeUpdateResponse {
    /// Exact session owner of this response.
    /// 本响应的精确会话归属。
    pub session_id: String,
    /// Document revision whose durable publication completed before returning.
    /// 返回前已完成可靠发布的文档修订。
    pub revision: u64,
    /// Authoritative mode after the serialized operation.
    /// 串行操作后的权威模式。
    pub permission_mode: PermissionMode,
}
