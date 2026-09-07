//! Private HTTPS inference responses for semantic-index and conversation interleaving tests.
//! 供语义索引与对话交错测试使用的私有 HTTPS 推理响应。

use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use vulcan_provider_adapters::{
    CredentialSecretEnvelope, CredentialSecretResolver, CredentialSecretStore,
};

/// Exact hostname already declared by the registered VCE offline configuration fixture.
/// 已注册 VCE 离线配置夹具声明的精确主机名。
const FIXTURE_HOST: &str = "vce-readiness.cn-beijing.maas.aliyuncs.com";
/// Exact built-in China compatible API host used by native embedding routes without an origin requirement.
/// 不要求工作空间来源的原生向量路线使用的精确内置中国兼容 API 主机。
const DECLARED_API_HOST: &str = "dashscope.aliyuncs.com";

/// Immutable fake credential source; it never reads or writes user secrets.
/// 不可变假凭据来源，绝不读取或写入用户秘密。
pub(super) struct StreamFixtureSecrets;

#[async_trait::async_trait]
impl CredentialSecretResolver for StreamFixtureSecrets {
    /// Resolve only the exact fixture reference and return a non-production key.
    /// 只解析精确夹具引用，返回非生产密钥。
    async fn read_optional_secret(
        &self,
        reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        if reference != "vce-readiness-no-secret" {
            return Err("Unexpected fixture credential".into());
        }
        Ok(Some(CredentialSecretEnvelope::new_api_key(
            "vce-local-only".into(),
        )?))
    }
}

#[async_trait::async_trait]
impl CredentialSecretStore for StreamFixtureSecrets {
    /// Reject reference creation with the supplied envelope; returns an immutable-store error.
    /// 拒绝使用给定信封创建引用，返回不可变存储错误。
    async fn create_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Immutable VCE fixture".into())
    }
    /// Reject replacement of any revision; returns an immutable-store error.
    /// 拒绝替换任何修订，返回不可变存储错误。
    async fn replace_secret(
        &self,
        _reference: &str,
        _revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Immutable VCE fixture".into())
    }
    /// Reject deletion of the given reference; returns an immutable-store error.
    /// 拒绝删除给定引用，返回不可变存储错误。
    async fn delete_secret(&self, _reference: &str) -> Result<(), String> {
        Err("Immutable VCE fixture".into())
    }
    /// Reject restoring a reference; returns an immutable-store error.
    /// 拒绝恢复引用，返回不可变存储错误。
    async fn restore_secret(
        &self,
        _reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err("Immutable VCE fixture".into())
    }
}

/// Explicit barriers and captured real requests, shared only by this fixture's connections.
/// 明确屏障及捕获的真实请求，仅由本夹具连接共享。
pub(super) struct StreamSignals {
    /// Count actual embedding HTTP requests without retaining their text or headers.
    /// 统计实际向量 HTTP 请求次数，不保留其文本或请求头。
    pub(super) embedding_count: AtomicUsize,
    /// Arm exactly one embedding request after initialization.
    /// 初始化后明确暂停恰好一次向量请求。
    pub(super) pause_embedding: AtomicBool,
    /// Buffered proof that the selected physical embedding request arrived.
    /// 已选物理向量请求到达的缓存证明。
    pub(super) embedding_entered: Semaphore,
    /// Permission to finish the selected embedding request.
    /// 完成已选向量请求的许可。
    pub(super) release_embedding: Semaphore,
    /// Release tool-call chunks only after the host has projected the initial text delta.
    /// 只有宿主投影初始文本增量后才释放工具调用分块。
    pub(super) release_tools: Semaphore,
    /// Number of actual chat HTTP requests, including the post-tool request.
    /// 实际聊天 HTTP 请求数，包括工具执行后请求。
    pub(super) chat_count: AtomicUsize,
    /// Ordered complete chat request bodies; no credential headers are retained.
    /// 有序完整聊天请求正文，不保留凭据头。
    pub(super) chats: Mutex<Vec<Value>>,
}

/// Owned loopback server and a client trusting only its additional test certificate.
/// 持有的回环服务器及只额外信任其测试证书的客户端。
pub(super) struct PrivateModelServer {
    /// Production-runtime-compatible HTTP client with exact DNS override and no ambient proxy.
    /// 可供生产运行时使用的 HTTP 客户端，精确覆盖 DNS 且不使用环境代理。
    pub(super) http: reqwest::Client,
    /// Explicit interleaving barriers and actual wire-body evidence.
    /// 明确交错屏障及实际传输正文证据。
    pub(super) signals: Arc<StreamSignals>,
    /// Parent owns the connection task set, so aborting it aborts all remaining test sockets.
    /// 父任务持有连接任务集合，中止父任务即中止全部剩余测试套接字。
    task: Option<tokio::task::JoinHandle<()>>,
}

impl PrivateModelServer {
    /// Bind private TLS and return its bounded inference fixture, or an explicit setup error.
    /// 绑定私有 TLS，返回有界推理夹具或明确准备错误。
    pub(super) async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
        // Never bind an external interface.
        // 绝不绑定外部接口。
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        // Pin the actual socket for DNS override.
        // 固定实际套接字用于 DNS 覆盖。
        let address = listener.local_addr()?;
        // Hostname validation remains enabled against this exact test certificate.
        // 主机名校验保持开启，并针对这张精确测试证书。
        let parameters =
            rcgen::CertificateParams::new(vec![FIXTURE_HOST.into(), DECLARED_API_HOST.into()])?;
        // Ephemeral key material remains inside the test process.
        // 临时密钥材料保留在测试进程内部。
        let key = rcgen::KeyPair::generate()?;
        // Only the generated certificate is added to the client trust roots.
        // 只将生成的证书加入客户端信任根。
        let certificate = parameters.self_signed(&key)?;
        // Convert the test certificate without disabling certificate verification.
        // 转换测试证书，不禁用证书校验。
        let root = reqwest::Certificate::from_der(certificate.der().as_ref())?;
        // Server authentication uses no client certificate or production identity.
        // 服务器认证不使用客户端证书或生产身份。
        let tls = tokio_rustls::rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![certificate.der().clone()],
                tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key.serialize_der())?,
            )?;
        // Every accepted connection uses the same isolated identity.
        // 每个已接纳连接使用同一个隔离身份。
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
        // The next-call barrier is unarmed until the test explicitly requests an update.
        // 下一调用屏障在测试明确请求更新前保持未启用。
        let signals = Arc::new(StreamSignals {
            embedding_count: AtomicUsize::new(0),
            pause_embedding: AtomicBool::new(false),
            embedding_entered: Semaphore::new(0),
            release_embedding: Semaphore::new(0),
            release_tools: Semaphore::new(0),
            chat_count: AtomicUsize::new(0),
            chats: Mutex::new(Vec::new()),
        });
        // Transfer shared barrier ownership to the listener task.
        // 将共享屏障所有权传入监听任务。
        let server_signals = signals.clone();
        // All child connections are owned and aborted together on shutdown or assertion unwind.
        // 全部子连接统一持有，在关闭或断言展开时共同中止。
        let task = tokio::spawn(async move {
            // JoinSet propagates socket panics and cannot leak detached network tasks.
            // JoinSet 传播套接字异常，不能泄漏游离网络任务。
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        // Accept only one loopback socket at a time while permitting concurrent requests.
                        // 每次接纳一个回环套接字，同时允许请求并发。
                        let (socket, _) = accepted.expect("private model accept");
                        // Each task owns its own TLS acceptor and a shared explicit barrier.
                        // 每个任务持有自己的 TLS 接收器及共享明确屏障。
                        let acceptor = acceptor.clone();
                        // Request observation is shared without retaining any state lock across I/O.
                        // 共享请求观察，但不让状态锁跨越输入输出。
                        let signals = server_signals.clone();
                        connections.spawn(async move {
                            tokio::time::timeout(Duration::from_secs(45), async move {
                                // TLS decoding occurs before any request body is interpreted.
                                // 解释任何请求正文之前先完成 TLS 解码。
                                let mut stream = acceptor.accept(socket).await.expect("private model TLS");
                                serve_request(&mut stream, &signals).await;
                            }).await.expect("private model request deadline");
                        });
                    }
                    result = connections.join_next(), if !connections.is_empty() => {
                        result.expect("connection result").expect("private model connection succeeds");
                    }
                }
            }
        });
        // Explicit DNS, trusted certificate, disabled redirect/proxy and a finite request timeout isolate traffic.
        // 明确 DNS、信任证书、禁用重定向／代理及有限请求超时隔离流量。
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .add_root_certificate(root)
            .resolve(FIXTURE_HOST, address)
            .resolve(DECLARED_API_HOST, address)
            .timeout(Duration::from_secs(40))
            .build()?;
        Ok(Self {
            http,
            signals,
            task: Some(task),
        })
    }

    /// Finish this private server, rejecting an unexpected earlier listener termination.
    /// 结束该私有服务器，拒绝非预期的提前监听终止。
    pub(super) async fn shutdown(mut self) {
        // Take the exact listener handle so normal shutdown can join it before Drop runs.
        // 取出精确监听句柄，使正常关闭可在析构前等待其结束。
        let task = self.task.take().expect("owned listener");
        assert!(
            !task.is_finished(),
            "private inference server failed before teardown"
        );
        task.abort();
        assert!(task.await.expect_err("listener was aborted").is_cancelled());
    }
}

impl Drop for PrivateModelServer {
    /// Abort owned connections even during assertion unwinding; no input or return value.
    /// 即使断言展开也中止持有连接；无输入和返回值。
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Decode one bounded actual HTTP request and serve only declared embeddings or chat completions.
/// 解码一个有界实际 HTTP 请求，只提供已声明向量或聊天补全。
/// Stream owns verified TLS; signals controls the index pause and captures transmitted model bodies.
/// 流持有已校验 TLS；信号控制索引暂停并捕获已传输模型正文。
async fn serve_request(
    stream: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    signals: &StreamSignals,
) {
    // Byte-exact framing avoids consuming request body bytes before bounding Content-Length.
    // 字节精确分帧避免在约束正文长度前消费正文。
    let mut header = Vec::new();
    // A one-byte buffer suffices for the bounded header terminator scan.
    // 单字节缓冲足以进行有界头终止符扫描。
    let mut byte = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        assert_eq!(
            stream.read(&mut byte).await.expect("read fixture header"),
            1
        );
        header.push(byte[0]);
        assert!(header.len() <= 16384, "bounded request headers");
    }
    // Headers are parsed but never retained in evidence or printed.
    // 解析头但不保留到证据或打印。
    let header = String::from_utf8(header).expect("ASCII HTTP headers");
    // Only the exact declared POST method is accepted.
    // 只接纳精确声明的 POST 方法。
    let path = header
        .lines()
        .next()
        .expect("request line")
        .strip_prefix("POST ")
        .expect("POST inference")
        .strip_suffix(" HTTP/1.1")
        .expect("HTTP 1.1 path");
    // Native inference and the protocol adapter both send bounded JSON with Content-Length.
    // 原生推理及协议适配器均通过正文长度发送有界 JSON。
    let length = header
        .lines()
        .find_map(|line| {
            // HTTP field names are case-insensitive by contract.
            // HTTP 字段名按契约不区分大小写。
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("body length"))
        })
        .expect("JSON content length");
    assert!(length <= 2_000_000, "tiny fixture request bound");
    // Own exactly the transmitted body bytes.
    // 精确持有已传输正文的字节。
    let mut bytes = vec![0; length];
    stream
        .read_exact(&mut bytes)
        .await
        .expect("read inference body");
    // Typed request discriminators below match the existing native/protocol wire contracts.
    // 下方请求判别字段匹配既有原生／协议传输契约。
    let request: Value = serde_json::from_slice(&bytes).expect("inference JSON");
    if path.ends_with("/embeddings") {
        signals.embedding_count.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request["model"], "text-embedding-v4");
        assert_eq!(request["dimensions"], 1024);
        if signals.pause_embedding.swap(false, Ordering::AcqRel) {
            signals.embedding_entered.add_permits(1);
            signals
                .release_embedding
                .acquire()
                .await
                .expect("release index response")
                .forget();
        }
        // Every input receives its own exact index and a deterministic non-zero dense vector.
        // 每个输入取得自己的精确序号及确定性非零稠密向量。
        let data: Vec<Value> = request["input"]
            .as_array()
            .expect("embedding text array")
            .iter()
            .enumerate()
            .map(|(index, _)| json!({"index":index,"embedding":vec![1.0_f32;1024]}))
            .collect();
        // JSON shape matches WireEmbeddingResponse rather than a test-only domain response.
        // JSON 形态匹配 WireEmbeddingResponse，而非仅测试领域响应。
        let body = json!({"model":"text-embedding-v4","data":data,"usage":{"prompt_tokens":8,"total_tokens":8}}).to_string();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.expect("embedding response");
    } else {
        assert!(
            path.ends_with("/chat/completions"),
            "unexpected fixture endpoint: {path}"
        );
        assert_eq!(request["model"], "qwen3.5-flash");
        assert_eq!(request["stream"], true);
        // Counter assigns the initial tool request and following result-consuming request.
        // 计数器区分初次工具请求及随后消费结果的请求。
        let ordinal = signals.chat_count.fetch_add(1, Ordering::SeqCst);
        signals
            .chats
            .lock()
            .expect("wire requests")
            .push(request.clone());
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n").await.expect("SSE headers");
        if ordinal == 0 || ordinal == 3 {
            // Verify the final Provider wire toolset contains both independently usable native tools.
            // 验证最终 Provider 传输工具集合包含两个可独立使用的原生工具。
            let tools = request["tools"].as_array().expect("provider tools");
            for name in ["codebase_retrieval", "file"] {
                assert!(
                    tools.iter().any(|tool| tool["function"]["name"] == name),
                    "missing wire tool {name}"
                );
            }
            send_delta(
                stream,
                json!({"role":"assistant","content":if ordinal == 0 {
                    "Inspecting the indexed source. "
                } else { "Continuing the indexed source checks. " }}),
                Value::Null,
            )
            .await;
            if ordinal == 0 {
                signals
                    .release_tools
                    .acquire()
                    .await
                    .expect("host observed incremental text")
                    .forget();
            }
            // Continued calls have fresh provider identities so earlier results cannot satisfy assertions.
            // 继续调用使用新的供应商身份，避免先前结果满足断言。
            let search_id = if ordinal == 0 {
                "call_vce_search"
            } else {
                "call_vce_continue_search"
            };
            // Bind the physical file result to the exact continued request as well.
            // 同样将物理文件结果绑定到精确继续请求。
            let file_id = if ordinal == 0 {
                "call_vce_file"
            } else {
                "call_vce_continue_file"
            };
            send_delta(stream, json!({"tool_calls":[
                {"index":0,"id":search_id,"type":"function","function":{"name":"codebase_retrieval","arguments":"{\"information_request\":\"generation_marker\"}"}},
                {"index":1,"id":file_id,"type":"function","function":{"name":"file","arguments":"{\"action\":\"read\",\"path\":\"lib.rs\"}"}}
            ]}), Value::Null).await;
            send_delta(stream, json!({}), json!("tool_calls")).await;
        } else if ordinal == 2 {
            // The host must actually stop this live response before its real Continue action can run.
            // 宿主必须实际停止此存活响应，才能运行真实继续动作。
            send_delta(
                stream,
                json!({"role":"assistant","content":"Awaiting user stop before continuation."}),
                Value::Null,
            )
            .await;
            // The owning JoinSet cancels this unfinished socket; its existing timeout still bounds failures.
            // 持有它的 JoinSet 取消此未结束套接字；既有超时仍限制失败等待。
            std::future::pending::<()>().await;
        } else {
            assert!(
                ordinal == 1 || ordinal == 4,
                "exactly two post-tool requests"
            );
            // These are actual tool outputs returned by the production dispatcher, not injected fixture results.
            // 这些是生产派发器返回的实际工具输出，不是夹具注入结果。
            let messages = request["messages"].as_array().expect("post-tool messages");
            // Select only this turn's exact call identities, never a prior turn's matching content.
            // 只选择本轮精确调用身份，绝不使用先前轮次的匹配内容。
            let (search_id, file_id) = if ordinal == 1 {
                ("call_vce_search", "call_vce_file")
            } else {
                ("call_vce_continue_search", "call_vce_continue_file")
            };
            for (id, marker) in [
                (search_id, "vce_old_generation"),
                (file_id, "vce_new_generation"),
            ] {
                // Tool-call identifiers correlate each actual result with its original invocation.
                // 工具调用标识将每个实际结果关联到原调用。
                let output = messages
                    .iter()
                    .find(|message| message["role"] == "tool" && message["tool_call_id"] == id)
                    .expect("exact returned tool output");
                assert!(
                    output["content"].to_string().contains(marker),
                    "tool result lacks {marker}"
                );
            }
            send_delta(
                stream,
                json!({"role":"assistant","content":if ordinal == 1 {
                "Search and file read completed while indexing."
            } else { "Continued search and file read completed while indexing." }}),
                Value::Null,
            )
            .await;
            send_delta(stream, json!({}), json!("stop")).await;
        }
        stream
            .write_all(b"data: [DONE]\n\n")
            .await
            .expect("SSE done");
    }
    stream.shutdown().await.expect("close fixture TLS");
}

/// Write one protocol-real chat delta and flush it so production can observe incremental text.
/// 写入一个真实协议聊天增量并刷新，使生产代码可以观察增量文本。
/// Delta and finish reason become the single choice; returns after the socket write completes.
/// 增量及结束原因组成唯一候选项；套接字写入完成后返回。
async fn send_delta(
    stream: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    delta: Value,
    finish: Value,
) {
    // Match the normal OpenAI-compatible chunk envelope consumed by the declared Alibaba protocol.
    // 匹配已声明阿里云协议消费的普通 OpenAI 兼容分块信封。
    let chunk = json!({"id":"vce-fixture-chat","object":"chat.completion.chunk","created":1,
        "model":"qwen3.5-flash","choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
    stream
        .write_all(format!("data: {chunk}\n\n").as_bytes())
        .await
        .expect("SSE chunk");
    stream.flush().await.expect("flush SSE chunk");
}
