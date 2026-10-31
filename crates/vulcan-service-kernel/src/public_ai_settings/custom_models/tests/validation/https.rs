//! One-request TLS fixture with synthetic credentials and an explicit response barrier.
//! 使用合成凭据与显式回复屏障的单请求 TLS 夹具。

use super::CustomProviderProtocol;
use async_trait::async_trait;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;
use vulcan_provider_adapters::{
    CredentialSecretEnvelope, CredentialSecretResolver, CredentialSecretStore,
};

/// Reserved test hostname resolved exclusively to the owned loopback listener.
/// 仅解析到自有回环监听器的保留测试主机名。
pub(super) const HOST: &str = "model-validation.example.test";
/// One shared bound for this fixture's network exchange and response barrier.
/// 本夹具网络交换与回复屏障共用的单一时间上限。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Bounds captured validation request bytes without retaining any authentication headers.
/// 限制捕获的验证请求字节数，不保留任何认证请求头。
const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// Selects the declared transport response independently from production encoders.
/// 独立于生产编码器选择声明的传输回复。
#[derive(Clone, Copy)]
pub(super) enum Reply {
    /// Successful stream with visible assistant text.
    /// 包含可见助手正文的成功流。
    Visible,
    /// Successful terminal stream with no visible assistant text.
    /// 没有可见助手正文的成功终态流。
    Empty,
    /// Explicit upstream authentication rejection.
    /// 明确的上游认证拒绝。
    Rejected,
}

/// Resolves only the fixture's synthetic API key; mutations are always rejected.
/// 仅解析夹具的合成 API Key，始终拒绝修改。
pub(super) struct Secrets;

#[async_trait]
impl CredentialSecretResolver for Secrets {
    /// Reads `reference` only when it belongs to this fixture; returns an isolated key envelope.
    /// 仅在 `reference` 属于本夹具时读取，返回隔离密钥封装。
    async fn read_optional_secret(
        &self,
        reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        if reference != "validation-secret" {
            return Err("Unknown validation fixture credential.".into());
        }
        Ok(Some(CredentialSecretEnvelope::new_api_key(
            "local-validation-only".into(),
        )?))
    }
}

#[async_trait]
impl CredentialSecretStore for Secrets {
    /// Rejects creating `_secret` at `_reference`; returns an immutable-store error.
    /// 拒绝在 `_reference` 创建 `_secret`，返回不可变存储错误。
    async fn create_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Validation fixture secrets are immutable.".into())
    }
    /// Rejects replacing `_reference` at `_revision` with `_secret`; returns an error.
    /// 拒绝用 `_secret` 替换 `_reference` 的 `_revision` 修订，返回错误。
    async fn replace_secret(
        &self,
        _reference: &str,
        _revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Validation fixture secrets are immutable.".into())
    }
    /// Rejects deleting `_reference`; returns an immutable-store error.
    /// 拒绝删除 `_reference`，返回不可变存储错误。
    async fn delete_secret(&self, _reference: &str) -> Result<(), String> {
        Err("Validation fixture secrets are immutable.".into())
    }
    /// Rejects restoring `_secret` at `_reference`; returns an immutable-store error.
    /// 拒绝在 `_reference` 还原 `_secret`，返回不可变存储错误。
    async fn restore_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Validation fixture secrets are immutable.".into())
    }
}

/// Safe request evidence retained after headers and body have been validated and discarded.
/// 校验并丢弃请求头与正文后保留的安全请求证据。
pub(super) struct ObservedRequest {
    /// Model selected by the actual production request encoder.
    /// 实际生产请求编码器选择的模型。
    pub model: String,
    /// Whether the production encoder requested streaming.
    /// 生产编码器是否请求流式输出。
    pub streaming: bool,
}

/// Owns one request task and transport; dropping the fixture cancels its listener on assertion failure.
/// 持有单次请求任务与传输，断言失败时丢弃夹具会取消监听器。
pub(super) struct Server {
    /// Client pinned to the private certificate and loopback address.
    /// 固定到私有证书与回环地址的客户端。
    pub http: reqwest::Client,
    /// Delivers safe evidence before the response is released.
    /// 回复放行前交付安全证据。
    pub received: oneshot::Receiver<ObservedRequest>,
    /// Releases the response only after routing-isolation assertions.
    /// 仅在路由隔离断言完成后放行回复。
    pub release: Option<oneshot::Sender<()>>,
    /// Owned listener task whose failures are observed by the test.
    /// 由测试观察失败的自有监听任务。
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Server {
    /// Starts one private TLS endpoint for `protocol` and `reply`; returns its client and barriers.
    /// 为 `protocol` 与 `reply` 启动私有 TLS 端点，返回客户端与屏障。
    pub(super) async fn start(protocol: CustomProviderProtocol, reply: Reply) -> Self {
        // Concurrent TLS fixtures may already have installed the same process-wide crypto provider.
        // 并发 TLS 夹具可能已经安装同一进程级加密提供器。
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
        // Use an OS-assigned local port and verify a certificate for the exact test hostname.
        // 使用系统分配的本地端口，并校验精确测试主机名的证书。
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        // Pin DNS to the assigned socket rather than resolving a public hostname.
        // 将 DNS 固定到分配的套接字，避免解析公共主机。
        let address = listener.local_addr().unwrap();
        // Generate an ephemeral key used only by this listener.
        // 生成仅由此监听器使用的临时密钥。
        let key = rcgen::KeyPair::generate().unwrap();
        // The certificate authenticates only the exact fixture hostname.
        // 证书仅认证精确的夹具主机名。
        let certificate = rcgen::CertificateParams::new(vec![HOST.into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        // Trust this generated certificate without disabling verification.
        // 信任此生成证书，同时保持证书校验。
        let root = reqwest::Certificate::from_der(certificate.der().as_ref()).unwrap();
        // The server uses the same TLS implementation exercised by production clients.
        // 服务端使用生产客户端所验证的同一 TLS 实现。
        let tls = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.der().clone()],
                tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key.serialize_der())
                    .unwrap(),
            )
            .unwrap();
        // Share only the immutable TLS configuration with the listener task.
        // 仅向监听任务共享不可变 TLS 配置。
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        // The test observes admission before the server is allowed to return any text.
        // 测试在服务端允许返回正文之前观察准入。
        let (observed, received) = oneshot::channel();
        // Release is a one-use barrier, so the request cannot accidentally complete before assertions.
        // release 是一次性屏障，防止请求在断言前意外完成。
        let (release, released) = oneshot::channel();
        // The owned task accepts exactly one request and reports every transport failure.
        // 自有任务精确接收一个请求，并报告每项传输失败。
        let task = tokio::spawn(async move {
            tokio::time::timeout(REQUEST_TIMEOUT, async {
                // The socket belongs only to this loopback listener.
                // 此套接字仅属于本回环监听器。
                let (socket, _) = listener.accept().await.unwrap();
                // Complete TLS before parsing HTTP request framing.
                // 先完成 TLS，再解析 HTTP 请求分帧。
                let mut stream = acceptor.accept(socket).await.unwrap();
                serve(&mut stream, protocol, reply, observed, released).await;
            })
            .await
            .expect("bounded validation request");
        });
        // Neither proxies nor redirects can send the fake credential outside the local endpoint.
        // 代理与重定向均不能把假凭据发送到本地端点之外。
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(root)
            .resolve(HOST, address)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap();
        Self {
            http,
            received,
            release: Some(release),
            task: Some(task),
        }
    }

    /// Joins the single request task and exposes any transport assertion failure; consumes the fixture.
    /// 等待单次请求任务并暴露传输断言失败，同时消费夹具。
    pub(super) async fn finish(mut self) {
        self.task.take().unwrap().await.unwrap();
    }
}

impl Drop for Server {
    /// Cancels an unjoined listener on unwind; returns nothing and touches no external process.
    /// 展开时取消未等待的监听器，无返回值且不触碰外部进程。
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Receives bounded JSON on `stream`, signals `observed`, waits for `released`, and emits `reply`.
/// 从 `stream` 接收有界 JSON，通知 `observed`，等待 `released` 后发送 `reply`。
/// `protocol` fixes the expected URL and wire framing; returns after writing one complete response.
/// `protocol` 固定预期 URL 与线协议分帧，完整写出一个响应后返回。
async fn serve(
    stream: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    protocol: CustomProviderProtocol,
    reply: Reply,
    observed: oneshot::Sender<ObservedRequest>,
    released: oneshot::Receiver<()>,
) {
    // Parse framing without logging or retaining secret-bearing headers.
    // 解析分帧，但不记录或保留含秘密的请求头。
    let mut header = Vec::new();
    // Single-byte reads preserve the exact header/body boundary.
    // 单字节读取保留精确请求头与正文边界。
    let mut byte = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).await.unwrap();
        header.push(byte[0]);
        assert!(header.len() < MAX_REQUEST_BYTES);
    }
    // Decode headers only in memory; never retain or print them.
    // 仅在内存解码请求头，绝不保留或打印。
    let header = String::from_utf8(header).unwrap();
    // The declared protocol uniquely determines the endpoint being exercised.
    // 声明协议唯一确定正在验证的端点。
    let path = match protocol {
        CustomProviderProtocol::AnthropicMessages => "/v1/messages",
        CustomProviderProtocol::OpenAiChat => "/v1/chat/completions",
        CustomProviderProtocol::OpenAiResponses => "/v1/responses",
        _ => panic!("unsupported validation fixture protocol"),
    };
    assert_eq!(
        header.lines().next().unwrap(),
        format!("POST {path} HTTP/1.1")
    );
    // The encoder supplies a fixed body length, bounded together with its headers.
    // 编码器提供固定正文长度，并与请求头一起受限。
    let length = header
        .lines()
        .find_map(|line| {
            // Header names are case-insensitive while values retain their exact framing meaning.
            // 请求头名称不区分大小写，值保留精确分帧含义。
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    assert!(header.len() + length < MAX_REQUEST_BYTES);
    // Read exactly the declared body without accepting trailing bytes as another request.
    // 精确读取声明正文，不将尾部字节接受为另一个请求。
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    // Observe canonical encoder fields rather than a test-authored surrogate request.
    // 观察规范编码器字段，而非测试编写的替代请求。
    let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
    observed
        .send(ObservedRequest {
            model: request["model"].as_str().unwrap().into(),
            streaming: request["stream"].as_bool().unwrap(),
        })
        .unwrap_or_else(|_| panic!("test must observe the validation request"));
    released.await.unwrap();
    // Each response is authored independently of the production serializer and must pass its real decoder.
    // 每个回复独立于生产序列化器编写，必须通过真实解码器。
    let (status, content_type, response) = if matches!(reply, Reply::Rejected) {
        ("401 Unauthorized", "application/json", json!({"error":{"type":"authentication_error","message":"Synthetic rejected credential"}}).to_string())
    } else {
        (
            "200 OK",
            "text/event-stream",
            response_body(protocol, matches!(reply, Reply::Visible)),
        )
    };
    stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

/// Returns a complete SSE response for `protocol`, including assistant text only when `visible` is true.
/// 返回 `protocol` 的完整 SSE 回复，仅在 `visible` 为 true 时包含助手正文。
fn response_body(protocol: CustomProviderProtocol, visible: bool) -> String {
    // Wire events close every content item before their terminal response event.
    // 线协议事件在终态响应事件前闭合全部内容项目。
    let mut events = Vec::new();
    match protocol {
        CustomProviderProtocol::AnthropicMessages => {
            events
                .push(json!({"type":"message_start","message":{"id":"msg_validation","usage":{}}}));
            if visible {
                events.push(json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}));
                events.push(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"OK"}}));
                events.push(json!({"type":"content_block_stop","index":0}));
            }
            events.push(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}));
            events.push(json!({"type":"message_stop"}));
        }
        CustomProviderProtocol::OpenAiResponses => {
            if visible {
                events.push(json!({"type":"response.output_text.delta","item_id":"message","output_index":0,"content_index":0,"delta":"OK"}));
                events.push(json!({"type":"response.output_item.done","output_index":0,"item":{"type":"message","id":"message","role":"assistant","content":[{"type":"output_text","text":"OK"}]}}));
            }
            events.push(json!({"type":"response.completed","response":{"id":"response_validation","usage":{}}}));
        }
        CustomProviderProtocol::OpenAiChat => {
            if visible {
                events.push(json!({"id":"validation","choices":[{"index":0,"delta":{"role":"assistant","content":"OK"},"finish_reason":null}]}));
            }
            events.push(json!({"id":"validation","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}));
        }
        _ => panic!("unsupported validation fixture protocol"),
    }
    // SSE framing is shared across protocols; the Chat sentinel is only transport termination.
    // 各协议共用 SSE 分帧，Chat 哨兵仅表示传输终止。
    let mut response = events
        .into_iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>();
    if protocol == CustomProviderProtocol::OpenAiChat {
        response.push_str("data: [DONE]\n\n");
    }
    response
}
