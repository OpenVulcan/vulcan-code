//! Provider-neutral reasoning preference.
//! 供应商无关的推理偏好。

use crate::provider_config::ReasoningEffortLevel;

/// Captures the host reasoning preference attached to one immutable public execution.
/// 捕获附着到一次不可变公共执行的宿主推理偏好。
#[derive(Debug, Clone)]
pub struct ProviderRuntimeReasoningResolution {
    /// Stores the workbench reasoning effort requested by the user.
    /// 保存用户通过工作台请求的推理强度。
    pub requested_effort: ReasoningEffortLevel,
    /// Describes the public configuration layer that supplied this preference.
    /// 描述提供该偏好的公共配置层。
    pub source_label: String,
    /// Stores an optional diagnostic note without changing request routing.
    /// 保存不改变请求路由的可选诊断说明。
    pub note: Option<String>,
}

impl Default for ProviderRuntimeReasoningResolution {
    /// Builds a preference that leaves all provider behavior unchanged.
    /// 构建一个不改变任何供应商行为的偏好。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A provider-default reasoning resolution.
    /// 返回遵循供应商默认值的推理解析结果。
    fn default() -> Self {
        Self {
            requested_effort: ReasoningEffortLevel::Off,
            source_label: String::from("public contract default"),
            note: None,
        }
    }
}

impl ProviderRuntimeReasoningResolution {
    /// Returns one compact human-readable diagnostic label.
    /// 返回一个紧凑的人类可读诊断标签。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The selected effort, source, and optional note.
    /// 返回选定强度、来源与可选说明。
    pub fn summary_label(&self) -> String {
        let mut label = format!(
            "推理：{} · {}",
            self.requested_effort.compact_label(),
            self.source_label
        );
        if let Some(note) = self
            .note
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            label.push_str(" · ");
            label.push_str(note);
        }
        label
    }
}
