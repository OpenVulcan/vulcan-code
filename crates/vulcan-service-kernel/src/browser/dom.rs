use super::*;
use serde_json::{Value, json};

/// Selects one page and the last rendered revision for conditional DOM reads.
/// 指定页面和已渲染的修订号，供条件式 DOM 读取。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DomSnapshotRequest {
    session_id: String,
    page_id: String,
    revision: Option<String>,
}

/// Selects a previously published resource and byte offset for one page.
/// 指定页面已发布的资源和字节偏移量。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DomAssetRequest {
    session_id: String,
    page_id: String,
    asset_id: String,
    offset: u32,
}

/// Reads one conditional DOM snapshot through the owned Chromium target.
/// 通过已归属的 Chromium 目标读取一次条件式 DOM 快照。
pub(crate) async fn service_browser_dom_snapshot(
    state: SharedServiceBridgeState,
    request: DomSnapshotRequest,
) -> Result<Value, String> {
    read_page_projection(
        state,
        request.session_id,
        request.page_id,
        "page.dom.snapshot",
        json!({ "revision": request.revision }),
    )
    .await
}

/// Reads one bounded chunk of a resource captured by the owned Chromium target.
/// 读取已归属 Chromium 目标捕获的资源的一个有限分块。
pub(crate) async fn service_browser_dom_asset(
    state: SharedServiceBridgeState,
    request: DomAssetRequest,
) -> Result<Value, String> {
    if request.asset_id.len() != 64
        || !request
            .asset_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(String::from(BROWSER_ERROR_INVALID_REQUEST));
    }
    read_page_projection(
        state,
        request.session_id,
        request.page_id,
        "page.dom.asset",
        json!({ "assetId": request.asset_id, "offset": request.offset }),
    )
    .await
}

/// Resolves a Core page to its exact host target and attaches the generation used by input.
/// 将 Core 页面解析为确切宿主目标，并附上输入所用的代次。
async fn read_page_projection(
    state: SharedServiceBridgeState,
    session_id: String,
    page_id: String,
    operation: &'static str,
    mut params: Value,
) -> Result<Value, String> {
    let manager = browser_manager_from_state(&state)?;
    manager
        .run_blocking(&session_id.clone(), move |manager| {
            let gate = manager.dispatch_gate(&session_id)?;
            let _dispatch = gate
                .lock()
                .map_err(|error| format!("failed to lock browser dispatch: {error}"))?;
            let page = manager
                .list_pages(&session_id)?
                .into_iter()
                .find(|page| page.page_id == page_id)
                .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
            let projection = manager.session_projection(&session_id)?;
            if page.host_generation != projection.host_generation {
                return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
            }
            params["targetId"] = json!(page.target_id);
            let outcome = manager.request_host_with_timeout(
                &session_id,
                operation,
                params,
                Duration::from_millis(750),
            )?;
            let response = match outcome {
                BrowserHostRequestOutcome::Completed(value) => value,
                BrowserHostRequestOutcome::Failed { code, message } => {
                    return Err(format!("{code}: {message}"));
                }
                BrowserHostRequestOutcome::Unknown { reason } => {
                    return Err(format!("{BROWSER_ERROR_INVALID_REQUEST}: {reason}"));
                }
            };
            manager.sync_host_events(&session_id)?;
            let current = manager.session_projection(&session_id)?;
            if response.get("targetId").and_then(Value::as_str) != Some(page.target_id.as_str())
                || current.host_generation != projection.host_generation
                || current.page_generation != projection.page_generation
                || current.viewport_generation != projection.viewport_generation
            {
                return Err(String::from(BROWSER_ERROR_STALE_GENERATION));
            }
            let result = response
                .get("result")
                .ok_or_else(|| String::from(BROWSER_ERROR_INVALID_REQUEST))?;
            Ok(json!({
                "pageId": page_id,
                "pageGeneration": projection.page_generation.to_string(),
                "viewportGeneration": projection.viewport_generation.to_string(),
                "result": result,
            }))
        })
        .await
}
