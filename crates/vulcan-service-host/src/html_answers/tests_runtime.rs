//! Behavioral capability, cancellation, HTTP isolation and listener-lifetime regressions.
//! 能力、取消、HTTP 隔离及监听器生命周期的行为回归。

use super::grants::{Grant, Grants};
use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tower::ServiceExt;
use uuid::Uuid;
use vulcan_service_api::{
    HtmlAnswerSourceIdentity, HtmlAnswerViewError, HtmlAnswerViewLinkRequest,
};
use vulcan_session_core::v1::html_answers::*;

/// Draft source stays copyable while only the compiled representation reaches the isolated response.
/// 草稿源码保持可复制，只有编译表示进入隔离响应。
#[test]
fn html_answers_draft_source_and_rendered_body_remain_distinct() {
    let source = "```ui-details More\ncontent\n```\n";
    let content = HtmlAnswerContent::from_source(
        "draft".into(),
        source.into(),
        Some(templates::HtmlAnswerTemplate::Document),
    )
    .unwrap();
    assert_eq!(content.source, source);
    assert!(content.into_html().contains("<details>"));
    let legacy = HtmlAnswerContent::from_source("raw".into(), "<p>old</p>".into(), None).unwrap();
    assert_eq!(legacy.into_html(), "<p>old</p>");
}

/// Create a synthetic exact identity with no access to any real session or model.
/// 创建不访问真实会话或模型的合成精确身份。
fn source_identity() -> HtmlAnswerSourceIdentity {
    HtmlAnswerSourceIdentity {
        session_id: Uuid::new_v4(),
        parent_session_id: None,
        window_generation: 1,
        event_seq: 3,
        event_revision: 2,
        artifact_id: "a".repeat(HTML_ANSWER_ID_HEX_LENGTH),
    }
}

/// Create one grant expiring by the actual Core ticket policy at `now`.
/// 创建在 `now` 起按实际 Core 票据策略到期的一份授权。
fn grant(client: Uuid, now: Instant) -> Grant {
    Grant {
        id: Uuid::new_v4(),
        client,
        source: source_identity(),
        locale: "en".to_owned(),
        expires: now + Duration::from_secs(HTML_ANSWER_TICKET_SECONDS),
    }
}

/// Exact per-client capacity, one-use exchange, monotonic expiry and isolated revocation are enforced.
/// 精确执行客户端容量、单次兑换、单调到期及隔离撤销。
#[test]
fn html_answers_grants_bound_expire_and_revoke_capabilities() {
    let now = Instant::now();
    let client = Uuid::new_v4();
    let mut table = Grants::default();
    for _ in 0..HTML_ANSWER_MAX_CLIENT_TICKETS {
        table.issue(grant(client, now), now).unwrap();
    }
    assert!(matches!(
        table.issue(grant(client, now), now),
        Err(HtmlAnswerViewError::Capacity)
    ));
    let selected = grant(Uuid::new_v4(), now);
    let ticket = table.issue(selected.clone(), now).unwrap();
    assert!(matches!(
        table.reserve_lease(&ticket, Uuid::new_v4(), &selected.source.artifact_id, now),
        Err(HtmlAnswerViewError::InvalidRequest)
    ));
    let (lease, _) = table
        .reserve_lease(
            &ticket,
            selected.source.session_id,
            &selected.source.artifact_id,
            now,
        )
        .unwrap();
    assert!(lease != ticket, "lease must use an independent secret");
    assert!(matches!(
        table.reserve_lease(
            &ticket,
            selected.source.session_id,
            &selected.source.artifact_id,
            now
        ),
        Err(HtmlAnswerViewError::Expired)
    ));
    table.revoke(Uuid::new_v4(), selected.id);
    assert!(table.leases.contains_key(&lease));
    table.revoke(selected.client, selected.id);
    assert!(!table.leases.contains_key(&lease));
    table.prune(now + Duration::from_secs(HTML_ANSWER_TICKET_SECONDS));
    assert!(table.tickets.is_empty());
    for _ in 0..HTML_ANSWER_MAX_CLIENT_LEASES {
        let selected = grant(client, now);
        let ticket = table.issue(selected.clone(), now).unwrap();
        table
            .reserve_lease(
                &ticket,
                selected.source.session_id,
                &selected.source.artifact_id,
                now,
            )
            .unwrap();
    }
    let selected = grant(client, now);
    let ticket = table.issue(selected.clone(), now).unwrap();
    assert!(matches!(
        table.reserve_lease(
            &ticket,
            selected.source.session_id,
            &selected.source.artifact_id,
            now
        ),
        Err(HtmlAnswerViewError::Capacity)
    ));
    table.prune(now + Duration::from_secs(HTML_ANSWER_LEASE_SECONDS));
    assert!(table.leases.is_empty());
    table.close();
    assert!(matches!(
        table.issue(grant(client, now), now),
        Err(HtmlAnswerViewError::Closed)
    ));
}

/// Issuance budgets replenish over elapsed time, and client churn cannot grow the rate table unbounded.
/// 签发预算按经过时间补充，客户端更替不能让速率表无界增长。
#[test]
fn html_answers_rate_limits_are_bounded_and_replenish() {
    let now = Instant::now();
    let client = Uuid::new_v4();
    let mut table = Grants::default();
    for _ in 0..HTML_ANSWER_TICKET_BURST {
        table.admit_issue(client, now).unwrap();
    }
    assert_eq!(
        table.admit_issue(client, now),
        Err(HtmlAnswerViewError::RateLimited)
    );
    let refill = Duration::from_secs_f64(60.0 / f64::from(HTML_ANSWER_TICKETS_PER_MINUTE));
    table.admit_issue(client, now + refill).unwrap();
    assert_eq!(
        table.admit_issue(client, now + refill),
        Err(HtmlAnswerViewError::RateLimited)
    );
    for _ in 1..HTML_ANSWER_MAX_TICKETS {
        table.admit_issue(Uuid::new_v4(), now + refill).unwrap();
    }
    assert_eq!(
        table.admit_issue(Uuid::new_v4(), now + refill),
        Err(HtmlAnswerViewError::Capacity)
    );
    table
        .admit_issue(Uuid::new_v4(), now + refill + Duration::from_secs(60))
        .unwrap();
}

/// Supply a synthetic built viewer and caller-owned resolver, returning their scoped runtime.
/// 提供合成已构建查看器和调用方自有解析器，返回作用域运行时。
fn runtime(resolver: HtmlAnswerSourceResolver) -> (tempfile::TempDir, Arc<HtmlAnswerRuntime>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("html-answer-viewer.html"),
        "<!doctype html><title>Viewer</title>",
    )
    .unwrap();
    let runtime = HtmlAnswerRuntime::new(
        HtmlAnswerRuntimeConfig {
            viewer_origin: "http://127.0.0.1:7101".to_owned(),
            renderer_origin: None,
            renderer_port: 0,
            static_root: Some(root.path().to_owned()),
        },
        resolver,
    )
    .unwrap();
    (root, runtime)
}

/// Send one secret-bearing POST through actual routing; return only its response to the test owner.
/// 通过实际路由发送一个携带秘密的 POST；仅向测试所有者返回响应。
async fn post_json(router: Router, path: &str, origin: &str, body: serde_json::Value) -> Response {
    router
        .oneshot(
            Request::post(path)
                .header("origin", origin)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Decode an intentionally bounded response without including a credential in failure diagnostics.
/// 解码刻意有界的响应，不在失败诊断中包含凭据。
async fn json_body(response: Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), HTML_ANSWER_MAX_TOTAL_BYTES)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("valid JSON response")
}

/// Real routes enforce Origin, single use, iframe-only reads, current source checks and shutdown cleanup.
/// 实际路由强制检查来源、单次使用、仅框架读取、最新源码及关闭清理。
#[tokio::test]
async fn html_answers_routes_revalidate_authority_and_stop_the_listener() {
    let available = Arc::new(AtomicBool::new(true));
    let read_available = available.clone();
    let (_root, runtime) = runtime(Arc::new(move |_| {
        let available = read_available.clone();
        Box::pin(async move {
            if available.load(Ordering::SeqCst) {
                HtmlAnswerContent::from_source(
                    "result".to_owned(),
                    "<button>run</button>".to_owned(),
                    None,
                )
            } else {
                Err(HtmlAnswerViewError::StaleSource)
            }
        })
    }));
    assert!(runtime.shared.origins.get().is_none());
    let selected = source_identity();
    let request = HtmlAnswerViewLinkRequest {
        client_id: Uuid::new_v4(),
        source: selected.clone(),
        locale: "en".to_owned(),
    };
    let link = runtime.issue(request.clone()).await.unwrap();
    let ticket = url::Url::parse(&link.viewer_url)
        .unwrap()
        .fragment()
        .unwrap()
        .strip_prefix("ticket=")
        .unwrap()
        .to_owned();
    let exchange = serde_json::json!({ "ticket": ticket, "sessionId": selected.session_id, "artifactId": selected.artifact_id });
    let wrong = post_json(
        runtime.viewer_router(),
        "/v1/html-answers/exchange",
        "null",
        exchange.clone(),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);
    assert!(wrong.headers().get("access-control-allow-origin").is_none());
    let response = post_json(
        runtime.viewer_router(),
        "/v1/html-answers/exchange",
        &runtime.config.viewer_origin,
        exchange.clone(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let lease = json_body(response).await;
    assert_eq!(
        post_json(
            runtime.viewer_router(),
            "/v1/html-answers/exchange",
            &runtime.config.viewer_origin,
            exchange
        )
        .await
        .status(),
        StatusCode::GONE
    );
    let url = url::Url::parse(lease["contentUrl"].as_str().unwrap()).unwrap();
    let renderer = routes::render_router(runtime.shared.clone());
    let direct = renderer
        .clone()
        .oneshot(Request::get(url.path()).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(direct.status(), StatusCode::FORBIDDEN);
    let iframe_request = || {
        Request::get(url.path())
            .header("sec-fetch-dest", "iframe")
            .header("sec-fetch-mode", "navigate")
            .body(Body::empty())
            .unwrap()
    };
    let rendered = renderer.clone().oneshot(iframe_request()).await.unwrap();
    assert_eq!(rendered.status(), StatusCode::OK);
    assert!(
        rendered.headers()["permissions-policy"]
            .to_str()
            .unwrap()
            .split(", ")
            .any(|directive| directive == "clipboard-write=()")
    );
    assert!(
        super::viewer_headers(None)["permissions-policy"]
            .to_str()
            .unwrap()
            .split(", ")
            .any(|directive| directive == "clipboard-write=(self)")
    );
    assert!(
        rendered.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox allow-scripts")
    );
    for forbidden in ["/rpc", "/health", "/asset/test", "/events"] {
        assert_ne!(
            renderer
                .clone()
                .oneshot(Request::get(forbidden).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    available.store(false, Ordering::SeqCst);
    assert_eq!(
        renderer.oneshot(iframe_request()).await.unwrap().status(),
        StatusCode::GONE
    );
    assert!(runtime.shared.grants.lock().unwrap().leases.is_empty());
    runtime.shutdown().await;
    assert!(
        tokio::net::TcpStream::connect((url.host_str().unwrap(), url.port().unwrap()))
            .await
            .is_err()
    );
    assert!(matches!(
        runtime.issue(request).await,
        Err(HtmlAnswerViewError::Closed)
    ));
    assert!(runtime.shared.grants.lock().unwrap().tickets.is_empty());
}

/// Cancellation during a source read consumes the ticket but releases the pending lease capacity.
/// 源码读取期间取消会消费票据，但会释放待确认租约容量。
#[tokio::test]
async fn html_answers_cancelled_exchange_reclaims_its_reservation() {
    let block = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(tokio::sync::Notify::new());
    let resolver_block = block.clone();
    let resolver_entered = entered.clone();
    let (_root, runtime) = runtime(Arc::new(move |_| {
        let block = resolver_block.clone();
        let entered = resolver_entered.clone();
        Box::pin(async move {
            if block.load(Ordering::SeqCst) {
                entered.notify_one();
                std::future::pending::<()>().await;
            }
            HtmlAnswerContent::from_source("page".to_owned(), "<p>hello</p>".to_owned(), None)
        })
    }));
    let selected = source_identity();
    let link = runtime
        .issue(HtmlAnswerViewLinkRequest {
            client_id: Uuid::new_v4(),
            source: selected.clone(),
            locale: "en".to_owned(),
        })
        .await
        .unwrap();
    let ticket = url::Url::parse(&link.viewer_url)
        .unwrap()
        .fragment()
        .unwrap()
        .strip_prefix("ticket=")
        .unwrap()
        .to_owned();
    block.store(true, Ordering::SeqCst);
    let router = runtime.viewer_router();
    let origin = runtime.config.viewer_origin.clone();
    let task = tokio::spawn(async move {
        post_json(router, "/v1/html-answers/exchange", &origin,
        serde_json::json!({ "ticket": ticket, "sessionId": selected.session_id, "artifactId": selected.artifact_id })).await
    });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    assert_eq!(runtime.shared.grants.lock().unwrap().leases.len(), 1);
    task.abort();
    let _ = task.await;
    assert!(runtime.shared.grants.lock().unwrap().leases.is_empty());
    assert!(runtime.shared.grants.lock().unwrap().tickets.is_empty());
    runtime.shutdown().await;
}
