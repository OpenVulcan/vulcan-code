//! System-level judge choices shared by persisted settings and the service API.
//! 持久化设置与服务 API 共享的系统级判定选择。

use serde::{Deserialize, Serialize};

/// Whether the user switched the judge off or on; absence means not configured yet.
/// 用户关闭或启用了判定器；缺省表示尚未设置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeState {
    /// The judge layer is not used and its options stay hidden.
    /// 不使用判定层，且其选项保持隐藏。
    Off,
    /// The judge layer runs with the configured backend.
    /// 判定层使用已设置的后端运行。
    Enabled,
}

/// Where calls are judged.
/// 判定调用的位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeBackend {
    /// The active registered model in a local worker process.
    /// 在本机 worker 进程中运行的使用中登记模型。
    Local,
}
