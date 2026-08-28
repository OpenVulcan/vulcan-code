use super::*;

/// Clears process-local tool correlations after the authoritative V1 API reaches a terminal boundary.
/// 权威 V1 API 到达终态边界后，清理进程内工具关联。
///
/// # Parameters
/// 参数
/// `runtime_state` is the process-local adapter whose correlation maps must not survive the run.
/// `runtime_state` 是进程内适配状态，其关联表不得跨越本次运行继续存在。
pub(super) fn clear_open_tool_correlations(runtime_state: &mut SessionRuntimeStateEntry) {
    runtime_state.tool_request_record_ids.clear();
    runtime_state.tool_call_record_ids.clear();
}
