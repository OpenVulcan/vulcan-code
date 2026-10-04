//! Exact authenticated RPC names expose original prewarm receipts without accepting workspace paths.
//! 精确已认证 RPC 名称暴露原预热回执，不接受工作区路径。

use super::*;
use crate::plugin_runtime_services::prewarm::{
    self as service, PrewarmAction, PrewarmOwner, PrewarmPreparation, PrewarmReceiptRequest,
};

/// Dispatch method with strict object params; return original service evidence or an unknown-domain marker.
/// 使用严格对象 params 分发 method；返回原服务证据或未知领域标记。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    // Each mutation retains its explicit action; HTTP cancellation never becomes a target cancellation.
    // 每项变更保留其明确动作；HTTP 取消绝不转成目标取消。
    let result = match method {
        "service_plugin_prewarm_targets" => {
            // Retain exact parent and canonical controller identities from this read.
            // 保留此读取的精确父级及规范控制器身份。
            let request: PrewarmOwner = parse_params(params)?;
            rpc_json_result(service::catalog(state, request).await)
        }
        "service_prepare_plugin_prewarm" => {
            // Require one named request object instead of positional Serde input.
            // 要求单个具名请求对象，而非位置 Serde 输入。
            let params: RequestParams = parse_params(params)?;
            // Decode only browser-owned identities and intent, with no filesystem authority.
            // 仅解码浏览器拥有的身份及意图，不接收文件系统权威。
            let request: PrewarmPreparation = parse_params(&params.request)?;
            rpc_json_result(service::prepare(state, request).await)
        }
        "service_plugin_prewarm_status"
        | "service_start_plugin_prewarm"
        | "service_cancel_plugin_prewarm"
        | "service_recover_plugin_prewarm_cleanup"
        | "service_forget_plugin_prewarm" => {
            // All receipt actions use the caller's exact original invocation.
            // 全部回执动作使用调用方的精确原始调用。
            let request: PrewarmReceiptRequest = parse_params(params)?;
            // Resolve only the exact method already matched above, without a default mutation.
            // 仅解析上方已匹配的精确方法，不设置默认变更。
            let action = match method {
                "service_plugin_prewarm_status" => PrewarmAction::Status,
                "service_start_plugin_prewarm" => PrewarmAction::Start,
                "service_cancel_plugin_prewarm" => PrewarmAction::Cancel,
                "service_recover_plugin_prewarm_cleanup" => PrewarmAction::Recover,
                "service_forget_plugin_prewarm" => PrewarmAction::Forget,
                _ => unreachable!("receipt methods were exhaustively matched"),
            };
            rpc_json_result(service::control(state, request, action).await)
        }
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
