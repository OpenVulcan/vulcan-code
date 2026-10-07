//! Loopback-only production viewer fixture with synthetic source authority and the real static bundle.
//! 仅回环的正式查看器夹具，使用合成源码权威及真实静态包。
//! It never opens user sessions, discovers a running Core, or calls a model.
//! 绝不打开用户会话、发现正在运行的 Core 或调用模型。

use axum::{
    Json, Router,
    extract::{OriginalUri, State},
    response::{IntoResponse, Response},
    routing::post,
};
use std::{path::PathBuf, sync::Arc};
use tokio::net::TcpListener;
use vulcan_service_api::{
    HtmlAnswerSourceIdentity, HtmlAnswerViewError, HtmlAnswerViewLinkRequest,
    build_webui_static_response,
};
use vulcan_service_host::html_answers::{
    HtmlAnswerContent, HtmlAnswerRuntime, HtmlAnswerRuntimeConfig,
};
use vulcan_session_core::v1::{
    ApiCallId, AssistantSemanticStatus, AssistantTextEvent, ConversationEvent, EventBody,
    EventVisibility, Seq,
    html_answers::{HtmlAnswerAvailability, project_html_answers},
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
    /// Explicit built WebUI directory provided by the test runner.
    /// 测试运行器提供的显式已构建 WebUI 目录。
    static_root: PathBuf,
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
    let HtmlAnswerAvailability::Ready { artifact_id, .. } = descriptor.availability else {
        return Err("fixture source must be valid".into());
    };
    let identity = HtmlAnswerSourceIdentity {
        session_id: session,
        parent_session_id: None,
        window_generation: 1,
        event_seq: event.seq.value(),
        event_revision: event.event_revision,
        artifact_id,
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
    let fixture = Fixture {
        runtime: runtime.clone(),
        source: identity,
        static_root,
    };
    let router = Router::new()
        .route("/fixture/issue", post(issue))
        .fallback(static_asset)
        .with_state(fixture)
        .merge(runtime.viewer_router());
    println!("{}", serde_json::json!({ "viewer": origin }));
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

/// Serve only the explicit test bundle through the same path validation as the production host.
/// 仅通过与正式宿主相同的路径校验提供显式测试静态包。
async fn static_asset(State(fixture): State<Fixture>, OriginalUri(uri): OriginalUri) -> Response {
    let response = build_webui_static_response(
        Some(&fixture.static_root),
        uri.path(),
        "no-store",
        "Not found",
    );
    let mut builder = Response::builder().status(response.status);
    for (key, value) in response.headers {
        builder = builder.header(key, value);
    }
    builder
        .body(axum::body::Body::from(response.body))
        .expect("validated fixture static response")
}
