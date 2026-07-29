//! Provider-neutral continuation cursors.
//! 供应商无关的延续游标。

use serde::{Deserialize, Serialize};

/// Carries one proven provider continuation boundary into a physical request.
/// 将一个已经证明安全的供应商延续边界带入物理请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiContinuation {
    /// Exact provider identifier that owns the upstream cursor.
    /// 持有上游游标的精确供应商标识。
    pub provider_id: String,
    /// Exact protocol profile that produced the cursor.
    /// 产出游标的精确协议配置。
    pub protocol_profile_id: String,
    /// Exact model identifier that produced the cursor.
    /// 产出游标的精确模型标识。
    pub model_id: String,
    /// Upstream response or assistant-message identifier.
    /// 上游响应或助手消息标识。
    pub provider_message_id: String,
    /// Exact local canonical message represented by the cursor.
    /// 游标所表示的精确本地规范消息。
    pub covered_local_message_id: String,
    /// Number of public messages already represented by the upstream cursor.
    /// 已经由上游游标表示的公共消息数量。
    pub covered_message_count: usize,
}

/// Carries provider continuation metadata emitted by a physical response.
/// 承载一个物理响应发出的供应商延续元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiContinuationUpdate {
    /// Upstream response or assistant-message identifier.
    /// 上游响应或助手消息标识。
    pub provider_message_id: String,
}
