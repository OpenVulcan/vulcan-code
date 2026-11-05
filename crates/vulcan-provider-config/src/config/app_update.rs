//! Saved in-app update settings.
//! 已保存的应用内更新设置。

use serde::{Deserialize, Serialize};

/// Download origins chosen by the user.
/// 用户选择的下载源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersistedAppUpdateRoute {
    /// The international origin, then the mainland China mirror.
    /// 先国际下载源，再中国大陆镜像。
    Auto,
    /// Only the international origin.
    /// 只用国际下载源。
    International,
    /// Only the mainland China mirror.
    /// 只用中国大陆镜像。
    China,
}

/// How often the service checks on its own, as chosen by the user.
/// 用户选择的服务自行检查频率。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersistedAppUpdateCheckInterval {
    /// Every hour.
    /// 每小时。
    OneHour,
    /// Every six hours.
    /// 每六小时。
    SixHours,
    /// Every twelve hours.
    /// 每十二小时。
    TwelveHours,
    /// Once a day.
    /// 每天一次。
    OneDay,
}

/// In-app update settings; an absent field keeps its default, which is applied only in memory.
/// 应用内更新设置；缺省字段保持默认值，默认值只在内存中补齐。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedAppUpdateConfiguration {
    /// Whether preview releases are offered as well; absence means no.
    /// 是否同时接收预览版；缺省表示否。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<bool>,
    /// Download origins; absence means automatic.
    /// 下载源；缺省表示自动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<PersistedAppUpdateRoute>,
    /// How often the service checks on its own; absence means every six hours.
    /// 服务自行检查的频率；缺省表示每六小时。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_interval: Option<PersistedAppUpdateCheckInterval>,
}

impl PersistedAppUpdateConfiguration {
    /// Returns whether nothing was chosen, so the field stays out of the saved file.
    /// 判断是否未做任何选择，使该字段不写入保存文件。
    pub fn is_default(&self) -> bool {
        self.preview.is_none() && self.route.is_none() && self.check_interval.is_none()
    }
}
