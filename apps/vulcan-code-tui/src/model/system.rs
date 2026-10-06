// Local editor values and render-only job progress.
// 本地编辑器数值与仅用于渲染的任务进度。

#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Choice(String),
    Number(i64),
    Text(String),
}

impl SettingValue {
    pub fn as_bool(&self) -> bool {
        matches!(self, SettingValue::Bool(true))
    }

    pub fn as_number(&self) -> i64 {
        match self {
            SettingValue::Number(n) => *n,
            _ => 0,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            SettingValue::Choice(s) | SettingValue::Text(s) => s,
            _ => "",
        }
    }
}

/// A background job with progress, addressed by a stable id so live state can be re-queried.
/// 带进度的后台任务，以稳定 ID 寻址以便重新查询实时状态。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub label: String,
    /// 0..=100; `None` while the job has not reported progress.
    /// 0..=100；任务尚未报告进度时为 `None`。
    pub progress: Option<u8>,
    pub done: bool,
    pub failed: bool,
    pub detail: String,
}
