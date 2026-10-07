//! Loopback-only production viewer fixture with synthetic source authority and the real static bundle.
//! 仅回环的正式查看器夹具，使用合成源码权威及真实静态包。
//! It never opens user sessions, discovers a running Core, or calls a model.
//! 绝不打开用户会话、发现正在运行的 Core 或调用模型。

use axum::{
    Json, Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::net::TcpListener;
use vulcan_service_api::{
    ConversationEventRevisionFrontierV1, ConversationEventWindowV1Response,
    ConversationSessionConfigV1, HtmlAnswerRevokeRequest, HtmlAnswerSourceIdentity,
    HtmlAnswerViewError, HtmlAnswerViewLinkRequest, WebuiServiceRpcError,
    build_service_project_resource_response, webui_service_ready_discovery_snapshot,
};
use vulcan_service_host::html_answers::{
    HtmlAnswerContent, HtmlAnswerRuntime, HtmlAnswerRuntimeConfig,
};
use vulcan_service_host::{WebuiServiceHttpConfig, build_webui_service_router};
use vulcan_session_core::v1::{
    ApiCallId, AssistantSemanticStatus, AssistantTextEvent, ConversationEvent, EventBody,
    EventVisibility, Seq, SessionConfig,
    html_answers::{HtmlAnswerAvailability, HtmlAnswerEventProjection, project_html_answers},
    service_dto::ConversationEventWindowBatch,
};

/// Synthetic owner shared by fixture-only issuance and standard static serving.
/// 夹具专用签发与标准静态服务共用的合成归属。
#[derive(Clone)]
struct Fixture {
    /// Real bounded viewer runtime.
    /// 真实有界查看器运行时。
    runtime: Arc<HtmlAnswerRuntime>,
    /// Only source this test listener is allowed to authorize.
    /// 测试监听器唯一可以授权的源码。
    source: HtmlAnswerSourceIdentity,
    /// Canonical synthetic window, including the real Core parser's descriptors.
    /// 权威合成窗口，包含真实 Core 解析器的描述。
    window: serde_json::Value,
}

/// Start a temporary listener with the supplied built WebUI root; print only its public origin.
/// 用所提供的已构建 WebUI 根启动临时监听器；仅打印其公开来源。
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let static_root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("expected built WebUI directory")?,
    )
    .canonicalize()?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let session = uuid::Uuid::new_v4();
    let html = "<!doctype html><meta charset=\"utf-8\"><button id=\"run\" onclick=\"document.getElementById('value').textContent=Number(document.getElementById('value').textContent)+1\">Run</button><span id=\"value\">0</span>";
    let event = ConversationEvent {
        seq: Seq::try_new(3)?,
        turn_id: None,
        created_at_ms: 1,
        updated_at_ms: 2,
        event_revision: 1,
        visibility: EventVisibility::Provider,
        event: EventBody::AssistantText(AssistantTextEvent {
            api_call_id: ApiCallId::try_new(1)?,
            status: AssistantSemanticStatus::Completed,
            provider_source: None,
            text: format!(
                "```vulcan-html {{\"version\":1,\"title\":\"Synthetic answer\"}}\n{html}\n```\n"
            ),
        }),
    };
    let descriptor = project_html_answers(&session.to_string(), &event, true).remove(0);
    let HtmlAnswerAvailability::Ready {
        ref artifact_id, ..
    } = descriptor.availability
    else {
        return Err("fixture source must be valid".into());
    };
    let identity = HtmlAnswerSourceIdentity {
        session_id: session,
        parent_session_id: None,
        window_generation: 1,
        event_seq: event.seq.value(),
        event_revision: event.event_revision,
        artifact_id: artifact_id.clone(),
    };
    let selected = identity.clone();
    let runtime = HtmlAnswerRuntime::new(
        HtmlAnswerRuntimeConfig {
            viewer_origin: origin.clone(),
            renderer_origin: None,
            renderer_port: 0,
            static_root: Some(static_root.clone()),
        },
        Arc::new(move |source| {
            let matches = source == selected;
            Box::pin(async move {
                if !matches {
                    return Err(HtmlAnswerViewError::StaleSource);
                }
                Ok(HtmlAnswerContent {
                    title: "Synthetic answer".to_owned(),
                    source: format!("{html}\n"),
                })
            })
        }),
    )?;
    let mut hydration = ConversationEventWindowV1Response::hydration(
        ConversationEventWindowBatch::from_events(
            session.to_string(),
            1,
            std::slice::from_ref(&event),
        ),
        None,
        None,
        None,
        ConversationSessionConfigV1::from_session_config(&SessionConfig::default()),
        ConversationEventRevisionFrontierV1::try_new("synthetic", 1, 1)?,
    );
    hydration
        .html_answer_projections
        .push(HtmlAnswerEventProjection {
            seq: event.seq,
            event_revision: event.event_revision,
            answers: vec![descriptor.clone()],
        });
    let fixture = Fixture {
        runtime: runtime.clone(),
        source: identity,
        window: serde_json::json!({
            "owner": { "session_id": session, "window_generation": 1 }, "service_epoch": "synthetic",
            "memory_revision": 1, "durable_revision": 1, "events": [event], "older_before_seq": null,
            "has_more": false, "open_turn_id": null, "open_turn_start_seq": null, "session_config": null,
            "html_answer_projections": [{ "seq": event.seq, "event_revision": event.event_revision, "answers": [descriptor] }]
        }),
    };
    let rpc_runtime = runtime.clone();
    // The real transport exposes only this synthetic window and its viewing operations.
    // 真实传输仅暴露此合成窗口及其查看操作。
    let router = build_webui_service_router(WebuiServiceHttpConfig {
        discovery: Arc::new(Mutex::new(webui_service_ready_discovery_snapshot(
            listener.local_addr()?.port(),
            std::process::id(),
            "synthetic",
            "synthetic-fixture",
        ))),
        service_token: "synthetic-fixture".to_owned(),
        event_bus: tokio::sync::broadcast::channel(8).0,
        static_root: Some(static_root),
        html_answers: Some(runtime.clone()),
        rpc_dispatcher: Arc::new(move |method, params| {
            let runtime = rpc_runtime.clone();
            let hydration = hydration.clone();
            Box::pin(async move {
                if method == "service_session_event_hydration_v1" {
                    if params
                        .as_ref()
                        .and_then(|value| value.get("sessionId"))
                        .and_then(|value| value.as_str())
                        != Some(hydration.batch.session_id.as_str())
                    {
                        return Err(WebuiServiceRpcError::InvalidParams(
                            "unknown synthetic session".into(),
                        ));
                    }
                    return serde_json::to_value(hydration)
                        .map_err(|_| WebuiServiceRpcError::Service("serialization failed".into()));
                }
                let request = params
                    .and_then(|mut params| params.get_mut("request").map(serde_json::Value::take))
                    .ok_or_else(|| {
                        WebuiServiceRpcError::InvalidParams("request required".to_owned())
                    })?;
                match method.as_str() {
                    "service_create_html_answer_view_link" => {
                        let request: HtmlAnswerViewLinkRequest = serde_json::from_value(request)
                            .map_err(|_| {
                                WebuiServiceRpcError::InvalidParams("invalid request".to_owned())
                            })?;
                        let link = runtime.issue(request).await.map_err(fixture_error)?;
                        serde_json::to_value(link).map_err(|_| {
                            WebuiServiceRpcError::Service("serialization failed".to_owned())
                        })
                    }
                    "service_revoke_html_answer_view_link" => {
                        let request: HtmlAnswerRevokeRequest = serde_json::from_value(request)
                            .map_err(|_| {
                                WebuiServiceRpcError::InvalidParams("invalid request".to_owned())
                            })?;
                        runtime
                            .revoke(request.client_id, request.grant_id)
                            .map_err(fixture_error)?;
                        Ok(serde_json::Value::Null)
                    }
                    _ => Err(WebuiServiceRpcError::Service(
                        "unsupported fixture method".to_owned(),
                    )),
                }
            })
        }),
        session_resource_resolver: Arc::new(|_| {
            Box::pin(async {
                build_service_project_resource_response(b"", Some("text/plain"), false, None)
            })
        }),
        client_disconnected: Arc::new(|_| {}),
        status_provider: None,
        browser_stream: None,
        browser_download_resolver: None,
    })
    .merge(
        Router::new()
            .route("/fixture/issue", post(issue))
            .route(
                "/fixture/window",
                get(|State(fixture): State<Fixture>| async { Json(fixture.window) }),
            )
            .with_state(fixture),
    );
    println!(
        "{}",
        serde_json::json!({ "viewer": origin, "session": session })
    );
    let result = axum::serve(listener, router).await;
    runtime.shutdown().await;
    result?;
    Ok(())
}

/// Issue synthetic test capabilities only; fresh client IDs isolate per-document acceptance lifetimes.
/// 仅签发合成测试能力；新客户端身份隔离每个文档的验收生命周期。
async fn issue(State(fixture): State<Fixture>, Json(locale): Json<String>) -> Response {
    match fixture
        .runtime
        .issue(HtmlAnswerViewLinkRequest {
            client_id: uuid::Uuid::new_v4(),
            source: fixture.source,
            locale,
        })
        .await
    {
        Ok(link) => Json(link).into_response(),
        Err(error) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": error })),
        )
            .into_response(),
    }
}

/// Match the production string-error envelope without exposing source or capability data.
/// 匹配正式字符串错误信封，不暴露源码或能力数据。
fn fixture_error(error: HtmlAnswerViewError) -> WebuiServiceRpcError {
    WebuiServiceRpcError::Service(
        serde_json::to_value(error)
            .expect("enum serializes")
            .as_str()
            .expect("enum string")
            .to_owned(),
    )
}
