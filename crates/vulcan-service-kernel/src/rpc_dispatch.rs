//! WebUI RPC method dispatch.
//! WebUI RPC 方法分发。

use super::*;

mod configuration;
mod conversation_runtime;
mod extensibility;
mod knowledge_base;
mod project_conversation;
mod workspace_io;

/// Typed ownership result returned by one domain RPC router.
/// 单个领域 RPC 路由返回的强类型所有权结果。
#[derive(Debug)]
pub(super) enum ServiceRpcDispatch {
    /// The domain owns the method and produced its serialized service result.
    /// 该领域持有此方法并产生了序列化服务结果。
    Handled(Result<Value, String>),
    /// The method belongs to another domain or is unknown.
    /// 此方法属于其他领域或未知。
    Unknown,
}

impl ServiceRpcDispatch {
    /// Returns whether a domain claimed exact ownership of the method.
    /// 返回某个领域是否声明了该方法的精确所有权。
    ///
    /// # Returns
    /// 返回值
    /// Returns true only for a handled service result, including service failures.
    /// 仅对已处理的服务结果返回 true，包括服务失败。
    fn is_handled(&self) -> bool {
        matches!(self, Self::Handled(_))
    }
}

/// Dispatches one normalized RPC through the ordered domain routers.
/// 通过有序领域路由分发单个归一化 RPC。
///
/// # Parameters
/// `app` emits frontend effects, `state` owns service data, `method` is the wire name, and `params` is normalized JSON.
/// `app` 发出前端副作用，`state` 持有服务数据，`method` 是线协议名称，`params` 是归一化 JSON。
///
/// # Returns
/// 返回值
/// Returns the first exact domain ownership result or `Unknown` after every router declines it.
/// 返回首个精确领域所有权结果，或在全部路由拒绝后返回 `Unknown`。
async fn dispatch_known_service_rpc(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, String> {
    // outcome is reused because each router either claims exact ownership or leaves it untouched.
    // outcome 被复用，因为每个路由要么声明精确所有权，要么保持未处理。
    let outcome = workspace_io::dispatch(app.clone(), state.clone(), method, params).await?;
    if outcome.is_handled() {
        return Ok(outcome);
    }
    let outcome = knowledge_base::dispatch(app.clone(), state.clone(), method, params).await?;
    if outcome.is_handled() {
        return Ok(outcome);
    }
    let outcome = extensibility::dispatch(app.clone(), state.clone(), method, params).await?;
    if outcome.is_handled() {
        return Ok(outcome);
    }
    let outcome =
        conversation_runtime::dispatch(app.clone(), state.clone(), method, params).await?;
    if outcome.is_handled() {
        return Ok(outcome);
    }
    let outcome = configuration::dispatch(app.clone(), state.clone(), method, params).await?;
    if outcome.is_handled() {
        return Ok(outcome);
    }
    project_conversation::dispatch(app, state, method, params).await
}

/// Dispatches one browser-facing service RPC while preserving submit tracing.
/// 在保留提交追踪的同时分发单个浏览器侧服务 RPC。
///
/// # Parameters
/// `app` emits frontend effects, `state` owns service data, `method` is the exact wire name, and `params` is optional raw JSON.
/// `app` 发出前端副作用，`state` 持有服务数据，`method` 是精确线协议名称，`params` 是可选原始 JSON。
///
/// # Returns
/// 返回值
/// Returns the serialized domain response or the canonical parameter, service, or unknown-method error.
/// 返回序列化领域响应，或标准参数、服务及未知方法错误。
pub(super) async fn dispatch_service_rpc(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    // params normalizes an omitted payload to the established empty object contract.
    // params 把缺失载荷归一化为既有空对象契约。
    let params = params.unwrap_or_else(|| json!({}));
    // rpc_started_at measures dispatch entry through serialized result handoff.
    // rpc_started_at 衡量从分发入口到序列化结果交接的耗时。
    let rpc_started_at = current_unix_millis_value();
    // traces_submit_rpc identifies the only RPC with detailed submission trace events.
    // traces_submit_rpc 标识唯一具有详细提交追踪事件的 RPC。
    let traces_submit_rpc = method == "service_submit_conversation_prompt";
    if traces_submit_rpc {
        webui_submit_trace("rpc_received", format!("method={method}"));
    }
    // rpc_result preserves service and unknown-method failures for the returning trace.
    // rpc_result 为返回追踪保留服务失败与未知方法失败。
    let rpc_result = match dispatch_known_service_rpc(app, state, &method, &params).await? {
        ServiceRpcDispatch::Handled(result) => result,
        ServiceRpcDispatch::Unknown => Err(unknown_service_rpc_error(&method)),
    };
    if traces_submit_rpc {
        webui_submit_trace(
            "rpc_returning",
            format!(
                "method={method} ok={} elapsed_ms={}",
                rpc_result.is_ok(),
                current_unix_millis_value().saturating_sub(rpc_started_at)
            ),
        );
    }
    rpc_result
}

/// Builds the canonical error for a method that no domain router owns.
/// 为没有任何领域路由持有的方法构建标准错误。
///
/// # Parameters
/// 参数
/// `method` is the exact unrecognized wire name supplied by the caller.
/// `method` 是调用方提供的精确未识别线协议名称。
///
/// # Returns
/// 返回值
/// Returns the stable unknown-method diagnostic used by the HTTP bridge.
/// 返回 HTTP 桥使用的稳定未知方法诊断。
fn unknown_service_rpc_error(method: &str) -> String {
    format!("unknown service RPC method: {method}")
}

/// Returns the complete unique RPC wire contract owned by all domain routers.
/// 返回全部领域路由持有的完整唯一 RPC 线协议契约。
///
/// # Returns
/// 返回值
/// Returns every exact public method name for protocol conformance tests.
/// 返回协议一致性测试使用的每个精确公开方法名称。
#[cfg(test)]
pub(crate) fn service_rpc_method_names() -> BTreeSet<&'static str> {
    [
        workspace_io::RPC_METHODS,
        extensibility::RPC_METHODS,
        conversation_runtime::RPC_METHODS,
        configuration::RPC_METHODS,
        project_conversation::RPC_METHODS,
        knowledge_base::RPC_METHODS,
    ]
    .into_iter()
    .flatten()
    .copied()
    .collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every migrated RPC wire name has one and only one domain owner.
    /// 验证每个迁移后的 RPC 线协议名称都有且仅有一个领域所有者。
    #[test]
    fn domain_rpc_method_ownership_is_complete_and_unique() {
        // methods combines all domain contracts without deriving ownership from implementation fallbacks.
        // methods 合并全部领域契约，不从实现兜底中推导所有权。
        let methods = service_rpc_method_names();
        // unique_methods proves no earlier router can shadow a later domain branch.
        // unique_methods 证明前置路由不会遮蔽后续领域分支。
        let declared_method_count = workspace_io::RPC_METHODS.len()
            + extensibility::RPC_METHODS.len()
            + conversation_runtime::RPC_METHODS.len()
            + configuration::RPC_METHODS.len()
            + project_conversation::RPC_METHODS.len();
        let declared_method_count = declared_method_count + knowledge_base::RPC_METHODS.len();
        assert_eq!(methods.len(), declared_method_count);
        assert!(methods.contains("service_submit_conversation_prompt"));
        assert!(methods.contains("service_set_vulcan_plugin_enabled"));
        assert!(methods.contains("service_update_local_plugin"));
        assert!(methods.contains("service_set_plugin_mcp_secret"));
        assert!(methods.contains("service_delete_plugin_mcp_secret"));
        assert!(methods.contains("workspace.diff.snapshot"));
        assert!(methods.contains("service_remove_project"));
        assert!(methods.contains("service_create_knowledge_library"));
        assert!(methods.contains("service_list_knowledge_sources"));
        assert!(methods.contains("service_import_knowledge_directory"));
        assert!(methods.contains("service_search_knowledge_library"));
        assert!(methods.contains("service_search_knowledge"));
        assert!(methods.contains("service_set_session_knowledge_libraries"));
    }

    /// Verifies unknown methods retain the browser bridge's canonical diagnostic.
    /// 验证未知方法保留浏览器桥的标准诊断。
    #[test]
    fn unknown_rpc_method_error_remains_stable() {
        assert_eq!(
            unknown_service_rpc_error("service_missing_method"),
            "unknown service RPC method: service_missing_method"
        );
    }
}
