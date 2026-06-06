use serde::{Deserialize, Serialize};
use vulcan_session_core::ExecutionRuntimePolicy;

/// Main-session runtime policy wrapper.
/// 主会话运行时策略包装器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainSessionRuntimePolicy {
    /// Shared runtime policy used by main-session execution.
    /// 主会话执行使用的共享运行时策略。
    pub runtime_policy: ExecutionRuntimePolicy,
}

impl Default for MainSessionRuntimePolicy {
    /// Builds the default main-session runtime policy.
    /// 构建默认主会话运行时策略。
    fn default() -> Self {
        Self {
            runtime_policy: ExecutionRuntimePolicy::main_session(),
        }
    }
}
