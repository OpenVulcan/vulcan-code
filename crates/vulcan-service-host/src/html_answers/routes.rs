//! Trusted viewer routes and the separately bound, capability-only generated-content route.
//! 可信查看器路由及独立绑定、仅凭有限能力访问的生成内容路由。

use super::{
    runtime::{HtmlAnswerRuntime, PendingLease, RenderState},
    viewer_headers,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use std::{sync::Arc, time::Instant};
use uuid::Uuid;
use vulcan_service_api::HtmlAnswerViewError;
use vulcan_session_core::v1::html_answers::{
    HTML_ANSWER_VIEW_REQUEST_MAX_BYTES, html_answer_identity_is_valid,
};

/// Secret-bearing exchange request; the route identity binds the otherwise opaque ticket.
/// 携带秘密的兑换请求；路由身份绑定原本不透明的票据。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExchangeRequest {
    /// Single-use result capability from the URL fragment.
    /// 来自 URL 片段的单次结果能力。
    ticket: String,
    /// Session identity parsed by the trusted viewer from its canonical path.
    /// 可信查看器从规范路径解析的会话身份。
    session_id: Uuid,
    /// Full artifact identity parsed from that same path.
    /// 从同一路径解析的完整结果身份。
    artifact_id: String,
}

/// Viewer-owned lease release request, never coupled to the Core instance bearer token.
/// 查看器持有的租约释放请求，绝不关联 Core 实例持有者令牌。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReleaseRequest {
    /// Result-scoped bearer returned only to the trusted viewer.
    /// 仅返回给可信查看器的限定结果持有者秘密。
    lease_token: String,
}

/// Build viewer endpoints with no CORS allowance and bounded secret-bearing request bodies.
/// 构造不允许 CORS 且限制秘密请求正文大小的查看器端点。
pub(super) fn viewer_router(runtime: Arc<HtmlAnswerRuntime>) -> Router {
    Router::new()
        .route("/html-answers/{session_id}/{artifact_id}", get(viewer))
        .route("/v1/html-answers/exchange", post(exchange))
        .route("/v1/html-answers/release", post(release))
        .layer(DefaultBodyLimit::max(HTML_ANSWER_VIEW_REQUEST_MAX_BYTES))
        .with_state(runtime)
}

/// Build the isolated listener with exactly one content route and a rejecting fallback.
/// 构造只有一个内容路由、其他请求全部拒绝的隔离监听器。
pub(super) fn render_router(state: Arc<RenderState>) -> Router {
    Router::new()
        .route("/content/{lease}", get(content))
        .fallback(|| async { reject(HtmlAnswerViewError::Unavailable) })
        .with_state(state)
}

/// Serve the built trusted viewer without reading any session source or consuming a ticket.
/// 提供已构建的可信查看器，不读取任何会话源码，也不消费票据。
async fn viewer(
    State(runtime): State<Arc<HtmlAnswerRuntime>>,
    Path((session, artifact)): Path<(String, String)>,
) -> Response {
    if session.parse::<Uuid>().is_err() || !html_answer_identity_is_valid(&artifact) {
        return reject(HtmlAnswerViewError::InvalidRequest);
    }
    let Some(root) = &runtime.config.static_root else {
        return reject(HtmlAnswerViewError::Unavailable);
    };
    let path = root.join("html-answer-viewer.html");
    let html = match tokio::task::spawn_blocking(move || std::fs::read(path)).await {
        Ok(Ok(html)) => html,
        _ => return reject(HtmlAnswerViewError::Unavailable),
    };
    let renderer = runtime
        .shared
        .origins
        .get()
        .map(|origins| origins.renderer());
    (viewer_headers(renderer), html).into_response()
}

/// Require exactly one Origin header matching the configured trusted viewer; null and absent are rejected.
/// 要求恰好一个与配置可信查看器匹配的 Origin 头；拒绝 null 和缺失来源。
fn trusted_origin(headers: &HeaderMap, expected: &str) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    let first = values.next();
    first.is_some_and(|value| value.as_bytes() == expected.as_bytes()) && values.next().is_none()
}

/// Consume a ticket, validate current authority without holding locks, then commit its exact lease.
/// 消费票据，在不持锁时校验当前权威，然后提交精确租约。
async fn exchange(
    State(runtime): State<Arc<HtmlAnswerRuntime>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !trusted_origin(&headers, &runtime.config.viewer_origin) {
        return reject(HtmlAnswerViewError::InvalidRequest);
    }
    let request: ExchangeRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return reject(HtmlAnswerViewError::InvalidRequest),
    };
    match exchange_checked(&runtime, request).await {
        Ok(value) => json_response(value),
        Err(error) => reject(error),
    }
}

/// Return the selected page and opaque lease only after exact reservation and fresh source checks succeed.
/// 仅在精确预留及最新源码检查成功后返回所选页面和不透明租约。
async fn exchange_checked(
    runtime: &HtmlAnswerRuntime,
    request: ExchangeRequest,
) -> Result<serde_json::Value, HtmlAnswerViewError> {
    let (secret, grant) = runtime
        .shared
        .grants
        .lock()
        .map_err(|_| HtmlAnswerViewError::Unavailable)?
        .reserve_lease(
            &request.ticket,
            request.session_id,
            &request.artifact_id,
            Instant::now(),
        )?;
    let mut reservation = PendingLease {
        shared: runtime.shared.clone(),
        secret,
        committed: false,
    };
    let content = (runtime.shared.resolver)(grant.source).await?;
    let origins = runtime
        .shared
        .origins
        .get()
        .ok_or(HtmlAnswerViewError::Unavailable)?;
    let mut grants = runtime
        .shared
        .grants
        .lock()
        .map_err(|_| HtmlAnswerViewError::Unavailable)?;
    grants.prune(Instant::now());
    let lease = grants
        .leases
        .get_mut(&reservation.secret)
        .ok_or(HtmlAnswerViewError::Expired)?;
    lease.ready = true;
    reservation.committed = true;
    Ok(serde_json::json!({
        "contentUrl": format!("{}/content/{}", origins.renderer(), reservation.secret),
        "leaseToken": reservation.secret,
        "expiresInSeconds": lease.grant.expires.saturating_duration_since(Instant::now()).as_secs(),
        "title": content.title, "source": content.source, "locale": grant.locale,
    }))
}

/// Revoke only the result lease supplied by this trusted-origin viewer; repeat release is harmless.
/// 仅撤销当前可信来源查看器提供的结果租约；重复释放无害。
async fn release(
    State(runtime): State<Arc<HtmlAnswerRuntime>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !trusted_origin(&headers, &runtime.config.viewer_origin) {
        return reject(HtmlAnswerViewError::InvalidRequest);
    }
    let request: ReleaseRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return reject(HtmlAnswerViewError::InvalidRequest),
    };
    let Ok(mut grants) = runtime.shared.grants.lock() else {
        return reject(HtmlAnswerViewError::Unavailable);
    };
    grants.leases.remove(&request.lease_token);
    json_response(serde_json::json!({ "released": true }))
}

/// Resolve each iframe read against current storage, then recheck lease authority before delivering bytes.
/// 每次 iframe 读取都对照当前存储解析，并在交付字节前重新检查租约权威。
async fn content(
    State(state): State<Arc<RenderState>>,
    Path(secret): Path<String>,
    headers: HeaderMap,
) -> Response {
    if headers
        .get("sec-fetch-dest")
        .and_then(|value| value.to_str().ok())
        != Some("iframe")
        || headers
            .get("sec-fetch-mode")
            .and_then(|value| value.to_str().ok())
            != Some("navigate")
    {
        return reject(HtmlAnswerViewError::InvalidRequest);
    }
    let (source, locale) = {
        let Ok(mut grants) = state.grants.lock() else {
            return reject(HtmlAnswerViewError::Unavailable);
        };
        grants.prune(Instant::now());
        let Some(lease) = grants.leases.get(&secret).filter(|lease| lease.ready) else {
            return reject(HtmlAnswerViewError::Expired);
        };
        (lease.grant.source.clone(), lease.grant.locale.clone())
    };
    let content = match (state.resolver)(source).await {
        Ok(content) => content,
        Err(error) => {
            if let Ok(mut grants) = state.grants.lock() {
                grants.leases.remove(&secret);
            }
            return content_rejection(&state, error, &locale);
        }
    };
    let Ok(mut grants) = state.grants.lock() else {
        return reject(HtmlAnswerViewError::Unavailable);
    };
    grants.prune(Instant::now());
    if !grants.leases.get(&secret).is_some_and(|lease| lease.ready) {
        return reject(HtmlAnswerViewError::Expired);
    }
    let Some(origins) = state.origins.get() else {
        return reject(HtmlAnswerViewError::Unavailable);
    };
    origins.content_response(content.into_html())
}

/// Show a localized failure inside the isolated frame when authority changes after exchange.
/// 兑换后权威发生变化时，在隔离框架内显示本地化失败。
/// No script or raw generated markup is included; only catalog text is HTML-escaped.
/// 不包含脚本或原始生成标记；仅对目录文案进行 HTML 转义。
fn content_rejection(state: &RenderState, error: HtmlAnswerViewError, locale: &str) -> Response {
    let key = match error {
        HtmlAnswerViewError::StaleSource => "htmlAnswers.stale",
        HtmlAnswerViewError::Expired => "htmlAnswers.expired",
        _ => "htmlAnswers.unavailable",
    };
    let mut translator = vulcan_i18n::Translator::new(&[&vulcan_i18n::WEBUI]);
    translator.set_locale(locale);
    let text = translator
        .text(key)
        .expect("shared HTML viewer labels exist");
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    let Some(origins) = state.origins.get() else {
        return reject(error);
    };
    let mut response = origins.content_response(format!("<!doctype html><html><head><meta charset=\"utf-8\"></head><body><p role=\"alert\">{escaped}</p></body></html>"));
    *response.status_mut() = StatusCode::GONE;
    response
}

/// Return credential-bearing JSON with no-store and no-referrer, without logging or formatting secrets.
/// 返回携带凭据的 JSON，并禁止缓存和来源引用；不记录或格式化秘密。
fn json_response(value: serde_json::Value) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// Map stable rejection codes to non-secret HTTP failures; never include request text or source bodies.
/// 将稳定拒绝码映射为不含秘密的 HTTP 失败；绝不包含请求文本或源码正文。
fn reject(error: HtmlAnswerViewError) -> Response {
    let status = match error {
        HtmlAnswerViewError::InvalidRequest => StatusCode::FORBIDDEN,
        HtmlAnswerViewError::Expired | HtmlAnswerViewError::StaleSource => StatusCode::GONE,
        HtmlAnswerViewError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        HtmlAnswerViewError::Capacity
        | HtmlAnswerViewError::Unavailable
        | HtmlAnswerViewError::Closed => StatusCode::SERVICE_UNAVAILABLE,
    };
    let mut response = json_response(serde_json::json!({ "error": error }));
    *response.status_mut() = status;
    response
}
