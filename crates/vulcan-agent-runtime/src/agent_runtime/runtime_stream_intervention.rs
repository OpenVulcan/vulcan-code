//! Runtime-stream intervention hooks shared by host UI frontends.
//! 供宿主 UI 前端共享的运行时流干预钩子。

use crate::model::SessionId;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use vulcan_core::controller::VulcanCoreController;

/// Typed host failure raised while applying a replay-safe runtime prompt intervention.
/// 应用可回放安全的运行时提示词干预时产生的类型化宿主错误。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimePromptInterventionError {
    /// A session-scoped exact model selection must replace the frozen route before the next Provider request.
    /// 会话级精确模型选择必须在下一次 Provider 请求前替换当前冻结路由。
    ProviderRouteSwitchRequired {
        /// Human-readable control-boundary diagnostic for the service-owned restart.
        /// 供服务层重启使用的人类可读控制边界诊断。
        message: String,
    },
    /// Shared retained state could not be locked or accessed.
    /// 无法锁定或访问共享保留状态。
    StateAccess {
        /// Human-readable source error.
        /// 可读的来源错误。
        message: String,
    },
    /// Required retained state was not initialized at the safe boundary.
    /// 安全边界处所需的保留状态尚未初始化。
    StateUnavailable {
        /// Human-readable missing-state description.
        /// 可读的缺失状态说明。
        message: String,
    },
    /// One queued host input violated the intervention protocol.
    /// 某条排队宿主输入违反了干预协议。
    InvalidInput {
        /// Human-readable protocol error.
        /// 可读的协议错误。
        message: String,
    },
    /// A detached draft no longer matched the authoritative queue invariant.
    /// 分离草稿不再满足权威队列不变式。
    InvariantViolation {
        /// Human-readable invariant failure.
        /// 可读的不变式失败信息。
        message: String,
    },
    /// Provider-visible and display projections could not be committed consistently.
    /// Provider 可见投影与显示投影无法一致提交。
    ProjectionConflict {
        /// Human-readable projection failure.
        /// 可读的投影失败信息。
        message: String,
    },
    /// The original operation failed and restoring drained host input also failed.
    /// 原操作失败且恢复已取出的宿主输入也失败。
    RecoveryFailure {
        /// Original operation failure.
        /// 原始操作失败信息。
        operation_error: String,
        /// Recovery operation failure.
        /// 恢复操作失败信息。
        recovery_error: String,
    },
}

impl Display for RuntimePromptInterventionError {
    /// Formats one typed intervention error for Core and user-facing diagnostics.
    /// 为 Core 与用户可见诊断格式化一条类型化干预错误。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProviderRouteSwitchRequired { message } => {
                write!(
                    formatter,
                    "Provider route switch is required before the next request: {message}"
                )
            }
            Self::StateAccess { message } => {
                write!(
                    formatter,
                    "Runtime intervention state access failed: {message}"
                )
            }
            Self::StateUnavailable { message } => {
                write!(
                    formatter,
                    "Runtime intervention state is unavailable: {message}"
                )
            }
            Self::InvalidInput { message } => {
                write!(
                    formatter,
                    "Runtime intervention input is invalid: {message}"
                )
            }
            Self::InvariantViolation { message } => {
                write!(
                    formatter,
                    "Runtime intervention invariant failed: {message}"
                )
            }
            Self::ProjectionConflict { message } => {
                write!(
                    formatter,
                    "Runtime intervention projection failed: {message}"
                )
            }
            Self::RecoveryFailure {
                operation_error,
                recovery_error,
            } => write!(
                formatter,
                "Runtime intervention failed and input recovery also failed: operation={operation_error}; recovery={recovery_error}"
            ),
        }
    }
}

impl std::error::Error for RuntimePromptInterventionError {}

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
/// Returns whether the controller changed, or an error that must stop the provider loop.
/// 返回控制器是否发生变化，或必须停止 Provider 循环的错误。
pub type RuntimePromptIntervention = Arc<
    dyn Fn(SessionId, &mut VulcanCoreController) -> Result<bool, RuntimePromptInterventionError>
        + Send
        + Sync,
>;
