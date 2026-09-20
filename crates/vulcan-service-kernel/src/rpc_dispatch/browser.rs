//! Core browser session RPC routing.
//! Core 浏览器会话 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by the Core browser router.
/// Core 浏览器路由持有的精确公开 RPC 线协议名称。
pub(super) const RPC_METHODS: &[&str] = &[
    "browser.runtime.status",
    "browser.runtime.install",
    "browser.session.create",
    "browser.session.list",
    "browser.session.close",
    "browser.session.navigate",
    "browser.session.snapshot",
    "browser.lease.acquire",
    "browser.lease.release",
    "browser.lease.heartbeat",
    "browser.lease.takeover",
    "browser.operations.confirm",
    "browser.operations.unconfirmed",
    "browser.observe.subscribe",
    "browser.observe.unsubscribe",
    "browser.stream.ticket",
    "browser.stream.resume",
    "browser.input.dispatch",
    "browser.viewport.set",
    "browser.pages.list",
    "browser.page.create",
    "browser.page.close",
    "browser.page.capture",
    "browser.upload.stage",
    "browser.upload.attach",
    "browser.upload.list",
    "browser.download.list",
    "browser.dialog.list",
    "browser.dialog.respond",
    "browser.fileChooser.list",
    "browser.session.hide-windows",
    "browser.host.status",
];

/// Dispatches one RPC method owned by the Core browser domain.
/// 分发 Core 浏览器领域持有的单个 RPC 方法。
///
/// # Parameters
/// 参数
/// `state` owns service data, `method` is the exact wire name, and `params` is the normalized object.
/// `state` 持有服务数据，`method` 是精确线协议名称，`params` 是归一化对象。
///
/// # Returns
/// 返回值
/// Returns `Handled` with the service result, `Unknown` for another domain, or a parameter parsing error.
/// 返回携带服务结果的 `Handled`、表示其他领域的 `Unknown`，或参数解析错误。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, String> {
    if !RPC_METHODS.contains(&method) {
        return Ok(ServiceRpcDispatch::Unknown);
    }
    if matches!(method, "browser.runtime.status" | "browser.runtime.install") {
        if params.as_object().is_none_or(|fields| !fields.is_empty()) {
            return Err(String::from(
                "invalid params for browser runtime operation: empty object required",
            ));
        }
        // runtime is a Core-wide package capability; no conversation browser identity exists yet.
        // runtime 是 Core 全局安装组件能力；此时尚不存在会话浏览器身份。
        let manager = crate::browser::service_browser_manager(&state)?;
        if method == "browser.runtime.install" {
            return Ok(ServiceRpcDispatch::Handled(
                manager
                    .start_runtime_installation()
                    .and_then(|task| serde_json::to_value(task).map_err(|error| error.to_string())),
            ));
        }
        return Ok(ServiceRpcDispatch::Handled(Ok(serde_json::to_value(
            manager.runtime_status()?,
        )
        .map_err(|error| error.to_string())?)));
    }
    // owner_session_id is the selected VulcanCode conversation, separate from the Core browser id.
    // owner_session_id 是选中的 VulcanCode 会话，与 Core 浏览器标识分离。
    let owner_session_id = params
        .get("ownerSessionId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && *value == value.trim())
        .ok_or_else(|| format!("invalid params for {method}: ownerSessionId is required"))?
        .to_owned();
    // browser_params removes the routing identity before the strict typed request is decoded.
    // browser_params 在严格类型化请求解码前移除路由身份。
    let mut browser_params = params
        .as_object()
        .cloned()
        .ok_or_else(|| format!("invalid params for {method}: object required"))?;
    browser_params.remove("ownerSessionId");
    // Every non-list operation is confined to the browser owned by this conversation.
    // 除列表外的每个操作都限制在该会话所拥有的浏览器内。
    if !matches!(method, "browser.session.create" | "browser.session.list") {
        // session_id is the Core browser identity present in every browser operation request.
        // session_id 是每个浏览器操作请求中携带的 Core 浏览器身份。
        let session_id = browser_params
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("invalid params for {method}: sessionId is required"))?;
        crate::browser::service_browser_manager(&state)?
            .require_owner(session_id, &owner_session_id)?;
    }
    // params now contains only the original typed method arguments.
    // params 现在只包含原有的类型化方法参数。
    let params = &Value::Object(browser_params);
    let result = match method {
        "browser.session.create" => rpc_json_result(
            crate::browser::service_browser_session_create(
                state,
                parse_param(params, method)?,
                owner_session_id,
            )
            .await,
        ),
        "browser.session.list" => rpc_json_result(
            crate::browser::service_browser_session_list(state, owner_session_id).await,
        ),
        "browser.session.close" => rpc_json_result(
            crate::browser::service_browser_session_close(state, parse_param(params, method)?)
                .await,
        ),
        "browser.page.close" => rpc_json_result(
            crate::browser::service_browser_page_close(state, parse_param(params, method)?).await,
        ),
        "browser.session.navigate" => rpc_json_result(
            crate::browser::service_browser_session_navigate(state, parse_param(params, method)?)
                .await,
        ),
        "browser.session.snapshot" => rpc_json_result(
            crate::browser::service_browser_session_snapshot(state, parse_param(params, method)?)
                .await,
        ),
        "browser.lease.acquire" => rpc_json_result(
            crate::browser::service_browser_lease_acquire(state, parse_param(params, method)?)
                .await,
        ),
        "browser.lease.release" => rpc_json_result(
            crate::browser::service_browser_lease_release(state, parse_param(params, method)?)
                .await,
        ),
        "browser.lease.heartbeat" => rpc_json_result(
            crate::browser::service_browser_lease_heartbeat(state, parse_param(params, method)?)
                .await,
        ),
        "browser.lease.takeover" => rpc_json_result(
            crate::browser::service_browser_lease_takeover(state, parse_param(params, method)?)
                .await,
        ),
        "browser.operations.confirm" => rpc_json_result(
            crate::browser::service_browser_operations_confirm(state, parse_param(params, method)?)
                .await,
        ),
        "browser.operations.unconfirmed" => rpc_json_result(
            crate::browser::service_browser_operations_unconfirmed(
                state,
                parse_param(params, method)?,
            )
            .await,
        ),
        "browser.observe.subscribe" => rpc_json_result(
            crate::browser::service_browser_observe_subscribe(state, parse_param(params, method)?)
                .await,
        ),
        "browser.observe.unsubscribe" => rpc_json_result(
            crate::browser::service_browser_observe_unsubscribe(
                state,
                parse_param(params, method)?,
            )
            .await,
        ),
        "browser.stream.ticket" => rpc_json_result(
            crate::browser::service_browser_stream_ticket(state, parse_param(params, method)?)
                .await,
        ),
        "browser.stream.resume" => rpc_json_result(
            crate::browser::service_browser_stream_resume(state, parse_param(params, method)?)
                .await,
        ),
        "browser.input.dispatch" => rpc_json_result(
            crate::browser::service_browser_input_dispatch(state, parse_param(params, method)?)
                .await,
        ),
        "browser.viewport.set" => rpc_json_result(
            crate::browser::service_browser_viewport_set(state, parse_param(params, method)?).await,
        ),
        "browser.pages.list" => rpc_json_result(
            crate::browser::service_browser_pages_list(state, parse_param(params, method)?).await,
        ),
        "browser.host.status" => rpc_json_result(
            crate::browser::service_browser_host_status(state, parse_param(params, method)?).await,
        ),
        "browser.page.create" => rpc_json_result(
            crate::browser::service_browser_page_create(state, parse_param(params, method)?).await,
        ),
        "browser.page.capture" => rpc_json_result(
            crate::browser::service_browser_page_capture(state, parse_param(params, method)?).await,
        ),
        "browser.upload.stage" => rpc_json_result(
            crate::browser::service_browser_upload_stage(state, parse_param(params, method)?).await,
        ),
        "browser.upload.attach" => rpc_json_result(
            crate::browser::service_browser_upload_attach(state, parse_param(params, method)?)
                .await,
        ),
        "browser.upload.list" => rpc_json_result(
            crate::browser::service_browser_upload_list(state, parse_param(params, method)?).await,
        ),
        "browser.download.list" => rpc_json_result(
            crate::browser::service_browser_download_list(state, parse_param(params, method)?)
                .await,
        ),
        "browser.dialog.list" => rpc_json_result(
            crate::browser::service_browser_dialog_list(state, parse_param(params, method)?).await,
        ),
        "browser.dialog.respond" => rpc_json_result(
            crate::browser::service_browser_dialog_respond(state, parse_param(params, method)?)
                .await,
        ),
        "browser.fileChooser.list" => rpc_json_result(
            crate::browser::service_browser_file_chooser_list(state, parse_param(params, method)?)
                .await,
        ),
        "browser.session.hide-windows" => rpc_json_result(
            crate::browser::service_browser_hide_windows(state, parse_param(params, method)?).await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}

/// Deserializes one complete strict browser request object.
/// 反序列化一个完整严格浏览器请求对象。
fn parse_param<T: serde::de::DeserializeOwned>(params: &Value, method: &str) -> Result<T, String> {
    serde_json::from_value(params.clone())
        .map_err(|error| format!("invalid params for {method}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use vulcan_agent_runtime::model::SessionId;

    /// Verifies component discovery does not invent a browser or require a conversation.
    /// 验证组件查询不会虚构浏览器，也不要求预先存在会话。
    #[tokio::test]
    async fn browser_runtime_status_reports_missing_optional_component() {
        // state has no configured host runtime, matching a service without optional Chromium.
        // state 没有配置宿主运行时，对应未安装可选 Chromium 的服务。
        let state = Arc::new(Mutex::new(
            crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4()),
        ));
        let status = dispatch(
            state.clone(),
            "browser.runtime.status",
            &serde_json::json!({}),
        )
        .await
        .expect("dispatch status");
        assert!(
            matches!(status, ServiceRpcDispatch::Handled(Ok(value)) if value["ready"] == false && value["canInstall"] == false)
        );
        let invalid = dispatch(
            state,
            "browser.runtime.status",
            &serde_json::json!({ "ownerSessionId": "conversation-a" }),
        )
        .await;
        assert!(invalid.unwrap_err().contains("empty object required"));
    }
}
