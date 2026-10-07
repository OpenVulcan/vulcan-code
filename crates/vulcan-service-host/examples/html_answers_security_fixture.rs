//! Synthetic loopback-only fixture for the real-browser HTML isolation regression suite.
//! 真实浏览器 HTML 隔离回归套件使用的纯合成回环夹具。
//! This has no Core token, session, filesystem or model access and is never a production entrypoint.
//! 此夹具没有 Core 令牌、会话、文件系统或模型访问能力，绝不是生产入口。

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Router,
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use tokio::net::TcpListener;
use vulcan_service_host::html_answers::HtmlAnswerOrigins;

/// Shared immutable policy and loopback-only forbidden destination, plus a request counter.
/// 共享的不可变策略和仅在回环监听的禁止目标，以及请求计数器。
#[derive(Clone)]
struct Fixture {
    /// Production policy implementation used for both response kinds.
    /// 两种响应共用的正式策略实现。
    origins: HtmlAnswerOrigins,
    /// Synthetic forbidden destination used to detect any attempted network escape.
    /// 用于检测任何网络逃逸尝试的合成禁止目标。
    sink: String,
    /// Number of requests that reached the forbidden listener.
    /// 到达禁止监听器的请求数量。
    requests: Arc<AtomicUsize>,
}

/// Start three ephemeral loopback listeners and print their non-sensitive addresses as JSON.
/// 启动三个临时回环监听器，并以 JSON 输出其非敏感地址。
/// No arguments; returns an I/O error if setup or serving fails.
/// 无参数；初始化或服务失败时返回 I/O 错误。
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let viewer = TcpListener::bind("127.0.0.1:0").await?;
    let renderer = TcpListener::bind("127.0.0.1:0").await?;
    let sink = TcpListener::bind("127.0.0.1:0").await?;
    let viewer_origin = format!("http://{}", viewer.local_addr()?);
    let render_origin = format!("http://{}", renderer.local_addr()?);
    let sink_origin = format!("http://{}", sink.local_addr()?);
    let fixture = Fixture {
        origins: HtmlAnswerOrigins::new(&viewer_origin, &render_origin)?,
        sink: sink_origin,
        requests: Arc::new(AtomicUsize::new(0)),
    };
    let viewer_router = Router::new()
        .route("/", get(viewer_html))
        .route("/viewer.js", get(viewer_script))
        .route("/observations", get(observations))
        .with_state(fixture.clone());
    let render_router = Router::new()
        .route("/content/{case}", get(generated_content))
        .route("/redirect", get(redirect))
        .with_state(fixture.clone());
    let sink_router = Router::new().fallback(forbidden).with_state(fixture);
    println!(
        "{}",
        serde_json::json!({"viewer": viewer_origin, "renderer": render_origin})
    );
    tokio::select! {
        result = axum::serve(viewer, viewer_router) => result?,
        result = axum::serve(renderer, render_router) => result?,
        result = axum::serve(sink, sink_router) => result?,
    }
    Ok(())
}

/// Serve the inert trusted fixture shell; no generated frame exists before the Open click.
/// 提供惰性的可信夹具外壳；点击打开前不存在生成框架。
async fn viewer_html(State(fixture): State<Fixture>) -> Response {
    (fixture.origins.viewer_headers(), "<!doctype html><html><head><meta charset=\"utf-8\"><title>HTML answer isolation fixture</title></head><body><button id=\"open\">Open</button><button id=\"close\">Close</button><div id=\"mount\"></div><script src=\"/viewer.js\"></script></body></html>").into_response()
}

/// Serve same-origin trusted control code that mounts one sandboxed render frame on demand.
/// 提供同源可信控制脚本，按需挂载一个沙箱渲染框架。
async fn viewer_script(State(fixture): State<Fixture>) -> Response {
    let origin = serde_json::to_string(fixture.origins.renderer()).expect("origin serializes");
    let script = format!(
        r#"
        const renderOrigin = {origin};
        document.getElementById('open').onclick = () => {{
            document.getElementById('mount').replaceChildren();
            const frame = document.createElement('iframe');
            frame.id = 'answer'; frame.sandbox = 'allow-scripts';
            frame.referrerPolicy = 'no-referrer';
            const requested = new URL(location.href).searchParams.get('case') || 'normal';
            frame.src = renderOrigin + (requested === 'redirect' ? '/redirect' : '/content/' + encodeURIComponent(requested));
            document.getElementById('mount').append(frame);
        }};
        document.getElementById('close').onclick = () => document.getElementById('mount').replaceChildren();
    "#
    );
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        script,
    )
        .into_response()
}

/// Serve one generated test page through the actual production content response function.
/// 通过实际正式内容响应函数提供一个生成测试页。
async fn generated_content(State(fixture): State<Fixture>, Path(case): Path<String>) -> Response {
    let forbidden =
        serde_json::to_string(&format!("{}/escape", fixture.sink)).expect("fixture URL serializes");
    let script = match case.as_str() {
        "navigate" => format!("document.getElementById('run').onclick = () => location.assign({forbidden});"),
        "data" => "document.getElementById('run').onclick = () => location.assign('data:text/html,<h1>escaped</h1>');".to_owned(),
        "top" => format!("document.getElementById('run').onclick = () => {{ try {{ top.location = {forbidden}; }} catch {{ document.body.dataset.blocked = 'true'; }} }};"),
        "normal" => format!(r#"
            const result = {{ scripts: true, desktop: typeof window.vulcanDesktop, node: typeof require, process: typeof process }};
            for (const [name, read] of Object.entries({{ parentDom: () => parent.document.body, cookie: () => document.cookie, storage: () => localStorage.length }})) {{
                try {{ read(); result[name] = 'allowed'; }} catch {{ result[name] = 'blocked'; }}
            }}
            try {{ eval('1 + 1'); result.eval = 'allowed'; }} catch {{ result.eval = 'blocked'; }}
            document.getElementById('run').onclick = () => {{ document.getElementById('value').textContent = '1'; }};
            fetch({forbidden}).catch(() => {{ document.body.dataset.fetchBlocked = 'true'; }});
            try {{ new WebSocket({forbidden}.replace('http:', 'ws:')); }} catch {{}}
            try {{ new Worker({forbidden}); }} catch {{}}
            const image = new Image(); image.src = {forbidden}; document.body.append(image);
            const frame = document.createElement('iframe'); frame.src = {forbidden}; document.body.append(frame);
            const style = document.createElement('style'); style.textContent = 'body {{ background-image: url(' + {forbidden} + ') }}'; document.head.append(style);
            try {{ result.popup = window.open({forbidden}) === null ? 'blocked' : 'allowed'; }}
            catch {{ result.popup = 'blocked'; }}
            document.body.dataset.result = JSON.stringify(result);
        "#),
        "meta" => String::new(),
        _ => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    let meta = if case == "meta" {
        format!(
            "<meta http-equiv=\"refresh\" content=\"0;url={}/escape\">",
            fixture.sink
        )
    } else {
        String::new()
    };
    fixture.origins.content_response(format!("<!doctype html><html><head><meta charset=\"utf-8\">{meta}</head><body><button id=\"run\">Interact</button><span id=\"value\">0</span><script>{script}</script></body></html>"))
}

/// Redirect within the allowed frame origin toward the forbidden destination for CSP verification.
/// 从允许框架来源重定向到禁止目标，以验证 CSP。
async fn redirect(State(fixture): State<Fixture>) -> Redirect {
    Redirect::temporary(&format!("{}/redirect-escape", fixture.sink))
}

/// Count any request that escapes isolation; respond inertly without logging its contents.
/// 统计任何逃逸隔离的请求；惰性响应且不记录其内容。
async fn forbidden(State(fixture): State<Fixture>) -> &'static str {
    fixture.requests.fetch_add(1, Ordering::SeqCst);
    "Forbidden destination reached"
}

/// Return only the synthetic forbidden-request count for the acceptance harness.
/// 只向验收工具返回合成禁止请求数量。
async fn observations(State(fixture): State<Fixture>) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({"forbidden_requests": fixture.requests.load(Ordering::SeqCst)}))
}
