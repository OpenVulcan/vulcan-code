//! Loopback HTTPS fixture for the actual child summary transport.
//! 实际子摘要传输使用的回环 HTTPS 夹具。

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vulcan_provider_adapters::{
    CredentialSecretEnvelope, CredentialSecretResolver, CredentialSecretStore,
};

mod question;

/// Closed response scenarios served exclusively through the private TLS endpoint.
/// 仅通过私有 TLS 端点提供的封闭响应场景。
#[derive(Clone, Copy)]
enum ResponseMode {
    /// Ordinary local summary.
    /// 普通本地摘要。
    Summary,
    /// Model tool reply followed by a final answer for auxiliary question execution.
    /// 辅助问答执行的模型工具回复及后续最终答复。
    Question,
}

/// The declared Model Studio workspace host redirected exclusively to loopback.
/// 声明的 Model Studio 工作空间主机，仅重定向到回环。
pub(super) const HOST: &str = "compaction.cn-beijing.maas.aliyuncs.com";

/// Isolated immutable fake API-key secret, unrelated to user credentials.
/// 隔离的不可变假 API 密钥秘密，与用户凭据无关。
pub(super) struct Secrets;

#[async_trait::async_trait]
impl CredentialSecretResolver for Secrets {
    /// Resolves only `reference` belonging to this fixture; returns its fake API key.
    /// 仅解析属于本夹具的 `reference`；返回假 API 密钥。
    async fn read_optional_secret(
        &self,
        reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        if reference != "compaction-test-secret" {
            return Err("Unknown compaction fixture credential.".into());
        }
        Ok(Some(CredentialSecretEnvelope::new_api_key(
            "compaction-local-only".into(),
        )?))
    }
}

#[async_trait::async_trait]
impl CredentialSecretStore for Secrets {
    /// Rejects creation for any `reference` and `secret`; returns an immutable-store error.
    /// 拒绝任何 `reference` 和 `secret` 的创建；返回不可变存储错误。
    async fn create_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Immutable compaction fixture.".into())
    }
    /// Rejects replacing `reference`; returns an immutable-store error.
    /// 拒绝替换 `reference`；返回不可变存储错误。
    async fn replace_secret(
        &self,
        _reference: &str,
        _revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Immutable compaction fixture.".into())
    }
    /// Rejects deletion of `reference`; returns an immutable-store error.
    /// 拒绝删除 `reference`；返回不可变存储错误。
    async fn delete_secret(&self, _reference: &str) -> Result<(), String> {
        Err("Immutable compaction fixture.".into())
    }
    /// Rejects restoring `secret`; returns an immutable-store error.
    /// 拒绝恢复 `secret`；返回不可变存储错误。
    async fn restore_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Immutable compaction fixture.".into())
    }
}

/// Owns bounded real TLS requests and their JSON bodies, without retaining authorization headers.
/// 持有有界真实 TLS 请求及 JSON 正文，不保留授权请求头。
pub(super) struct Server {
    /// Exact trusted and DNS-pinned transport installed in the real provider runtime.
    /// 安装到真实供应商运行时的精确信任及 DNS 固定传输。
    pub(super) http: reqwest::Client,
    /// Actually transmitted requests, in arrival order.
    /// 实际传输的请求，按到达顺序保存。
    pub(super) requests: Arc<Mutex<Vec<serde_json::Value>>>,
    /// Owning task, aborted on unwind and explicitly joined on successful completion.
    /// 持有任务，展开时中止，正常完成时明确等待退出。
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Server {
    /// Starts private TLS returning `summary`; returns isolated transport.
    /// 启动返回`summary`的私有 TLS；返回隔离传输。
    pub(super) async fn start(summary: &str) -> Self {
        Self::start_mode(summary, ResponseMode::Summary).await
    }

    /// Starts a real Chat model that replies to the tagged question using its advertised tool.
    /// 启动通过已暴露工具回复带标签问题的真实 Chat 模型。
    pub(super) async fn start_question_answerer() -> Self {
        Self::start_mode("", ResponseMode::Question).await
    }

    /// Starts the isolated endpoint for `mode` and optional `summary`; returns its transport and observations.
    /// 为 `mode` 及可选 `summary` 启动隔离端点；返回传输及观察记录。
    async fn start_mode(summary: &str, mode: ResponseMode) -> Self {
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
        // Bind no external interface and retain the OS-assigned private port.
        // 不绑定外部接口，保留操作系统分配的私有端口。
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        // Only this address can receive requests for the configured host.
        // 只有此地址能够接收配置主机的请求。
        let address = listener.local_addr().unwrap();
        // Keep certificate verification active for the declared workspace and standard API host.
        // 为声明的工作空间和标准 API 主机保持证书校验。
        let parameters = rcgen::CertificateParams::new(vec![
            HOST.into(),
            "dashscope.aliyuncs.com".into(),
        ])
        .unwrap();
        // Generate isolated ephemeral server credentials.
        // 生成隔离的临时服务器凭据。
        let key = rcgen::KeyPair::generate().unwrap();
        // This certificate alone is trusted in addition to normal roots.
        // 除普通根外，仅信任此证书。
        let certificate = parameters.self_signed(&key).unwrap();
        // Convert the generated certificate for reqwest.
        // 转换生成证书供 reqwest 使用。
        let root = reqwest::Certificate::from_der(certificate.der().as_ref()).unwrap();
        // Use the actual TLS parser on both ends of the loopback socket.
        // 回环套接字两端使用实际 TLS 解析器。
        let tls = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.der().clone()],
                tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key.serialize_der())
                    .unwrap(),
            )
            .unwrap();
        // Each request is sequential and owned by this listener task.
        // 每个请求顺序执行，由此监听任务持有。
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        // Capture JSON only, never secret-bearing headers.
        // 仅捕获 JSON，绝不捕获含秘密的请求头。
        let requests = Arc::new(Mutex::new(Vec::new()));
        // Share observations with the owning test.
        // 与所属测试共享观察记录。
        let observed = requests.clone();
        // Keep model output independent from the real request encoder.
        // 保持模型输出独立于真实请求编码器。
        let summary = summary.to_owned();
        // The listener owns the in-flight request even when the test unwinds.
        // 即使测试展开，监听器仍持有正在执行的请求。
        let task = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                tokio::time::timeout(Duration::from_secs(30), async {
                    // Decode real TLS before inspecting HTTP framing.
                    // 检查 HTTP 分帧前解码真实 TLS。
                    let mut stream = acceptor.accept(socket).await.unwrap();
                    serve(&mut stream, &observed, &summary, mode).await;
                })
                .await
                .expect("bounded summary request");
            }
        });
        // Disable proxy and redirects so fixture credentials cannot leave the pinned endpoint.
        // 禁用代理和重定向，确保夹具凭据不会离开固定端点。
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(root)
            .resolve(HOST, address)
            .resolve("dashscope.aliyuncs.com", address)
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap();
        Self {
            http,
            requests,
            task: Some(task),
        }
    }

    /// Stops and joins the listener; no request or assertion panic may be hidden.
    /// 停止并等待监听器退出；不得隐藏请求或断言异常。
    pub(super) async fn shutdown(mut self) {
        let task = self.task.take().unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
}

impl Drop for Server {
    /// Cancels the owned socket task on assertion unwind; returns nothing.
    /// 断言展开时取消持有的套接字任务；无返回值。
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Reads a bounded `stream`, records JSON in `requests`, and emits `summary` using Chat SSE.
/// 读取有界 `stream`、向 `requests` 记录 JSON，并通过 Chat SSE 输出 `summary`。
async fn serve(
    stream: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    requests: &Mutex<Vec<serde_json::Value>>,
    summary: &str,
    mode: ResponseMode,
) {
    // Read exact framing without consuming body bytes as headers.
    // 精确读取分帧，不将正文字节作为请求头消费。
    let mut header = Vec::new();
    // A single-byte scan keeps the boundary unambiguous.
    // 单字节扫描保持边界明确。
    let mut byte = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).await.unwrap();
        header.push(byte[0]);
        assert!(header.len() <= 16384);
    }
    // Never persist or print these headers.
    // 绝不持久化或打印这些请求头。
    let header = String::from_utf8(header).unwrap();
    assert!(header.lines().next().unwrap().starts_with("POST "));
    // HTTP header field names are case-insensitive.
    // HTTP 请求头字段名称不区分大小写。
    let length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    assert!(length <= 2_000_000);
    // Receive exactly the declared JSON body.
    // 精确接收声明的 JSON 正文。
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.unwrap();
    assert!(
        header
            .lines()
            .next()
            .unwrap()
            .ends_with("/chat/completions HTTP/1.1")
    );
    requests
        .lock()
        .unwrap()
        .push(serde_json::from_slice(&body).unwrap());
    if matches!(mode, ResponseMode::Question) {
        question::respond(stream, requests).await;
        return;
    }
    // Produce semantic output and terminal usage through the real streaming parser.
    // 通过真实流式解析器产生语义输出和终态用量。
    let delta = serde_json::json!({"id":"summary-response","object":"chat.completion.chunk","model":"qwen3.5-flash","choices":[{"index":0,"delta":{"role":"assistant","content":summary},"finish_reason":null}]});
    // The physical response ends normally; this must not end the original business a second time.
    // 物理响应正常结束；不得再次结束原业务。
    let terminal = serde_json::json!({"id":"summary-response","object":"chat.completion.chunk","model":"qwen3.5-flash","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120}});
    // Explicit connection close avoids unrelated keepalive requests in this bounded fixture.
    // 明确关闭连接，避免此有界夹具出现无关保活请求。
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {delta}\n\ndata: {terminal}\n\ndata: [DONE]\n\n"
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    finish_response(stream).await;
}

/// Closes `stream` after a complete `Connection: close` response; returns nothing.
/// 在完整的 `Connection: close` 响应之后关闭 `stream`；无返回值。
/// The client may close first once it has read the whole response, so only a peer-closed error is
/// expected here; any other shutdown failure still fails the fixture.
/// 客户端读完整个响应后可能先行关闭，因此这里仅预期对端已关闭类错误；其他关闭失败仍使夹具失败。
async fn finish_response(stream: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>) {
    if let Err(error) = stream.shutdown().await {
        assert!(
            matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::UnexpectedEof
            ),
            "fixture response shutdown failed: {error}"
        );
    }
}
