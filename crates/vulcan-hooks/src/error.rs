use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display};

use crate::{HookFailurePolicy, HookId};

/// Failure details captured during hook dispatch.
/// hook 调度过程中捕获的失败详情。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookFailure {
    /// Hook id that failed.
    /// 失败的 hook 标识。
    pub hook_id: HookId,
    /// Failure policy active for that hook.
    /// 该 hook 生效的失败策略。
    pub policy: HookFailurePolicy,
    /// Human-readable failure message.
    /// 人类可读失败消息。
    pub message: String,
}

/// Error type used by hook registration and dispatch.
/// hook 注册和调度使用的错误类型。
#[derive(Debug)]
pub enum HookError {
    /// Registration metadata was invalid.
    /// 注册元数据无效。
    InvalidRegistration(String),
    /// Serialization or deserialization failed.
    /// 序列化或反序列化失败。
    Serialization(serde_json::Error),
    /// Handler returned a runtime error.
    /// 处理器返回运行时错误。
    Handler(String),
    /// Handler did not finish before its timeout.
    /// 处理器未在超时时间前完成。
    Timeout(HookId),
    /// Dispatch stopped because a stop-policy hook failed.
    /// 调度因 stop 策略 hook 失败而停止。
    DispatchStopped(Vec<HookFailure>),
}

impl HookError {
    /// Builds an invalid-registration error.
    /// 构建注册无效错误。
    pub fn invalid_registration(message: impl Into<String>) -> Self {
        Self::InvalidRegistration(message.into())
    }

    /// Builds a serialization error.
    /// 构建序列化错误。
    pub fn serialization(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }

    /// Builds a handler error.
    /// 构建处理器错误。
    pub fn handler(message: impl Into<String>) -> Self {
        Self::Handler(message.into())
    }

    /// Builds a timeout error.
    /// 构建超时错误。
    pub fn timeout(hook_id: impl Into<HookId>) -> Self {
        Self::Timeout(hook_id.into())
    }

    /// Builds a dispatch-stopped error.
    /// 构建调度停止错误。
    pub fn dispatch_stopped(failures: Vec<HookFailure>) -> Self {
        Self::DispatchStopped(failures)
    }
}

impl Display for HookError {
    /// Formats hook errors for diagnostics.
    /// 为诊断格式化 hook 错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRegistration(message) => {
                write!(formatter, "invalid hook registration: {message}")
            }
            Self::Serialization(error) => write!(formatter, "hook serialization failed: {error}"),
            Self::Handler(message) => write!(formatter, "hook handler failed: {message}"),
            Self::Timeout(hook_id) => write!(formatter, "hook timed out: {hook_id}"),
            Self::DispatchStopped(failures) => {
                write!(
                    formatter,
                    "hook dispatch stopped after {} failure(s)",
                    failures.len()
                )
            }
        }
    }
}

impl Error for HookError {
    /// Returns the nested serialization source when available.
    /// 当存在嵌套序列化来源时返回它。
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialization(error) => Some(error),
            _ => None,
        }
    }
}
