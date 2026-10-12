//! System-level judge choices shared by persisted settings and the service API.
//! 持久化设置与服务 API 共享的系统级判定选择。

use serde::{Deserialize, Serialize};

/// Where calls are judged. A saved backend is what makes the judge configured, and so what makes
/// auto-approve mode available.
/// 判定调用的位置。已保存的后端即表示判定器已配置，自动审批模式也因此可用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeBackend {
    /// The active registered model in a local worker process.
    /// 在本机 worker 进程中运行的使用中登记模型。
    Local,
    /// The decision route saved for the permission judge, such as jev on TypeSafe or OpenRouter.
    /// 为权限判定保存的判定路由，例如 TypeSafe 或 OpenRouter 上的 jev。
    Cloud,
}
