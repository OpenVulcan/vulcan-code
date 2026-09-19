//! Service bootstrap error helpers.
//! 服务引导 error 辅助逻辑。

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
        }
    }

    /// Projects one stable status failure without configuration or credential content.
    /// 投影一条不含配置或凭据内容的稳定状态失败。
    ///
    /// # Returns
    /// 返回值
    /// A machine-readable failure record owned by the protected status endpoint.
    /// 返回受保护状态端点持有的机器可读失败记录。
    pub(crate) fn service_status_failure(&self) -> vulcan_service_api::ServiceStatusFailure {
        vulcan_service_api::ServiceStatusFailure {
            stage: self.step_id.to_string(),
            code: format!("bootstrap_{}", self.step_id),
            message: self.message.clone(),
        }
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
