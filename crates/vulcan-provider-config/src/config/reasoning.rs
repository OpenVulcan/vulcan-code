//! Workbench reasoning preference independent from provider protocol details.
//! 与供应商协议细节无关的工作台推理偏好。

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unified reasoning-effort preference shared by workbench controls.
/// 工作台控件共享的统一推理强度偏好。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningEffortLevel {
    /// Keeps the provider or model default without an explicit override.
    /// 保留供应商或模型默认值，不发送显式覆盖。
    Off,
    /// Explicitly requests no reasoning when the active model exposes that level.
    /// 当当前模型公开该档位时，显式请求不进行推理。
    None,
    /// Requests provider-managed adaptive reasoning.
    /// 请求由供应商管理的自适应推理。
    Adaptive,
    /// Requests the smallest provider-declared reasoning preference.
    /// 请求供应商声明的最小推理档位。
    Minimal,
    /// Requests a lightweight reasoning preference.
    /// 请求轻量推理偏好。
    Low,
    /// Requests a balanced reasoning preference.
    /// 请求平衡推理偏好。
    Medium,
    /// Requests a deeper reasoning preference.
    /// 请求深度推理偏好。
    High,
    /// Requests the strongest workbench reasoning preference.
    /// 请求工作台最强推理偏好。
    XHigh,
    /// Requests a provider-declared maximum reasoning preference.
    /// 请求供应商声明的最大推理档位。
    Max,
    /// Requests a provider-specific ultra reasoning preference.
    /// 请求供应商特有的极限推理档位。
    Ultra,
}

impl ReasoningEffortLevel {
    /// Lists all workbench reasoning preferences in ascending order.
    /// 按升序列出全部工作台推理偏好。
    pub const ALL: [Self; 10] = [
        Self::Off,
        Self::None,
        Self::Adaptive,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
        Self::Ultra,
    ];

    /// Returns the stable preference rank.
    /// 返回稳定的偏好排序值。
    pub const fn rank(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::None => 1,
            Self::Adaptive => 2,
            Self::Minimal => 3,
            Self::Low => 4,
            Self::Medium => 5,
            Self::High => 6,
            Self::XHigh => 7,
            Self::Max => 8,
            Self::Ultra => 9,
        }
    }

    /// Returns the compact Chinese display label.
    /// 返回紧凑中文显示标签。
    pub const fn compact_label(self) -> &'static str {
        match self {
            Self::Off => "默认",
            Self::None => "关闭",
            Self::Adaptive => "自适应",
            Self::Minimal => "最小",
            Self::Low => "低",
            Self::Medium => "中",
            Self::High => "高",
            Self::XHigh => "超高",
            Self::Max => "最大",
            Self::Ultra => "极限",
        }
    }
}

impl fmt::Display for ReasoningEffortLevel {
    /// Writes the compact user-facing label.
    /// 写入紧凑的用户可见标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.compact_label())
    }
}
