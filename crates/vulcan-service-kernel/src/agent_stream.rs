//! Agent stream task forwarding and runtime event projection.
//! Agent 流任务转发与运行时事件投影。

use super::*;

mod detached_subagent;
mod projection;
mod provider_telemetry;
pub(crate) mod provider_v1_sink;
mod runtime_coordination;
mod runtime_hook_dispatch;
mod stream_task;
mod workbench_sync;

pub(crate) use detached_subagent::*;
pub(crate) use projection::*;
pub(crate) use provider_telemetry::*;
pub(crate) use provider_v1_sink::*;
pub(crate) use runtime_coordination::*;
pub(crate) use runtime_hook_dispatch::*;
pub(crate) use stream_task::*;
pub(crate) use workbench_sync::*;

/// Returns the current Unix millisecond timestamp in the V1 domain width.
/// 以 V1 领域要求的位宽返回当前 Unix 毫秒时间戳。
///
/// # Returns / 返回值
///
/// The exact checked timestamp or an explicit range failure.
/// 经过检查的精确时间戳，越界时返回显式错误。
pub(crate) fn current_v1_unix_millis() -> Result<i64, String> {
    i64::try_from(current_unix_millis_value())
        .map_err(|_| String::from("Current Unix millisecond timestamp exceeds the V1 i64 range."))
}
