//! Service bootstrap error helpers.
//! 服务引导 error 辅助逻辑。

use vulcan_service_api::StartupRecoverySnapshot;

#[derive(Debug, Clone)]
pub(crate) struct ServiceBootstrapError {
    /// Stable startup step identifier shown in the boot overlay.
    /// 启动遮罩中展示的稳定步骤标识。
    pub(crate) step_id: &'static str,
    /// Human-readable startup step label used in logs.
    /// 日志中使用的人类可读启动步骤标签。
    pub(crate) step_label: &'static str,
    /// Concrete failure message produced by the failed subsystem.
    /// 失败子系统产出的具体失败信息。
    pub(crate) message: String,
    /// Optional classified recovery that requires an explicit user action.
    /// 需要用户明确操作的可选已分类恢复方案。
    pub(crate) recovery: Option<StartupRecoverySnapshot>,
}

impl ServiceBootstrapError {
    /// Builds one startup step-scoped error.
    /// 构建一条带启动阶段的错误。
    pub(crate) fn new(
        step_id: &'static str,
        step_label: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            step_id,
            step_label,
            message: message.into(),
            recovery: None,
        }
    }

    /// Attaches one exact user-authorized recovery to this startup failure.
    /// 为该启动失败附加一项精确的用户授权恢复操作。
    ///
    /// # Parameters
    /// 参数
    /// `recovery` is the service-classified destructive recovery scope.
    /// `recovery` 是服务端分类后的破坏性恢复范围。
    ///
    /// # Returns
    /// 返回值
    /// The same startup error carrying the recovery projection.
    /// 返回携带恢复投影的同一启动错误。
    pub(crate) fn with_recovery(mut self, recovery: StartupRecoverySnapshot) -> Self {
        self.recovery = Some(recovery);
        self
    }
}

/// Wraps a subsystem result with one startup step scope.
/// 为子系统结果附加一个启动阶段作用域。
pub(crate) fn bootstrap_step<T>(
    result: Result<T, String>,
    step_id: &'static str,
    step_label: &'static str,
) -> Result<T, ServiceBootstrapError> {
    result.map_err(|error| ServiceBootstrapError::new(step_id, step_label, error))
}
