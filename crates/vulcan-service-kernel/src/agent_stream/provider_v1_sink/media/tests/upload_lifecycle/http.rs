//! HTTPS loopback checks across actual V1 planning, upload, metadata and progress consumers.
//! 跨真实V1规划、上传、元数据及进度消费者的HTTPS回环检查。

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod cancelled_selection;
mod draft_handoff;
mod durable_completion;
mod shared_deadline;

/// Pauses a real HTTPS request after its body has arrived but before any provider reference is returned.
/// 在真实HTTPS请求正文到达后、任何供应商引用返回前暂停。
struct UploadReplyGate {
    /// Signals the exact completed request-body boundary to the test owner.
    /// 向测试所有者通知精确请求正文完成边界。
    request_received: tokio::sync::oneshot::Sender<()>,
    /// Releases the reply, or abandons it when dropped after client cancellation.
    /// 放行回复，或在客户端取消后被丢弃时放弃回复。
    release_reply: tokio::sync::oneshot::Receiver<()>,
}

/// Creates an isolated verified HTTPS endpoint returning exact `status` and `response_body`.
/// 创建返回精确`status`及`response_body`的隔离已验证HTTPS端点。
/// Returns the restricted client and one bounded server task that captures the actual provider-file request.
/// 返回受限客户端及一个有界服务器任务，后者捕获实际供应商文件请求。
/// Optional `reply_gate` controls the response boundary without weakening TLS or actual body validation.
/// 可选`reply_gate`控制响应边界，不削弱TLS或实际正文校验。
async fn https_upload_fixture(
    status: u16,
    response_body: &str,
    reply_gate: Option<UploadReplyGate>,
) -> Result<(reqwest::Client, tokio::task::JoinHandle<serde_json::Value>), Box<dyn std::error::Error>>
{
    // Preserve the existing test-process crypto provider, installing the same repository fixture default only if absent.
    // 保留测试进程既有加密提供者，仅在缺失时安装相同仓库夹具默认值。
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
    // No public interface or non-loopback target is opened.
    // 不打开公共接口或非回环目标。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    // Freeze the exact ephemeral socket before moving the listener into its task.
    // 在监听器移入任务之前冻结精确临时套接字。
    let address = listener.local_addr()?;
    // Match the already-declared offline fixture host, not a production hostname or a disabled TLS check.
    // 匹配已声明离线夹具主机，而非生产主机名，也不禁用TLS检查。
    let parameters = rcgen::CertificateParams::new(vec!["unreachable.invalid".into()])?;
    // Private fixture material never leaves this process or appears in failure text.
    // 私有夹具材料绝不离开此进程或出现在失败文案中。
    let key = rcgen::KeyPair::generate()?;
    // The exact generated certificate is the only additional client trust root.
    // 精确生成证书是客户端唯一额外信任根。
    let certificate = parameters.self_signed(&key)?;
    // The client still validates the hostname and peer certificate.
    // 客户端仍验证主机名及对端证书。
    let root = reqwest::Certificate::from_der(certificate.der().as_ref())?;
    // This TLS server serves only the isolated certificate and does not request client certificates.
    // 此TLS服务器只提供隔离证书，且不要求客户端证书。
    let tls = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![certificate.der().clone()],
            tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(key.serialize_der())?,
        )?;
    // The acceptor is owned by the single-request fixture task.
    // 接收器由单请求夹具任务持有。
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
    // Own only the small controlled response body in the asynchronous fixture.
    // 在异步夹具中仅持有小型受控响应正文。
    let body = response_body.as_bytes().to_vec();
    // The task cannot remain pending beyond its isolated five-second test deadline.
    // 任务不能超过其隔离五秒测试截止时间持续等待。
    let server = tokio::spawn(async move {
        tokio::time::timeout(std::time::Duration::from_secs(5), async move {
            // Exactly one transfer is expected; subsequent model calls are outside this test and never run.
            // 恰好预期一次传输；后续模型调用不在本测试内且绝不执行。
            let (socket, _) = listener.accept().await.expect("accept local upload");
            // Terminate TLS with the generated, hostname-matching certificate.
            // 使用生成且匹配主机名的证书终结TLS。
            let mut stream = acceptor.accept(socket).await.expect("local upload TLS");
            // Bound header framing independently of Content-Length.
            // 独立于Content-Length约束头部分帧。
            let mut header = Vec::new();
            // One-byte framing cannot consume any request-body bytes before its limit is checked.
            // 单字节分帧不能在限制检查前消费任何请求正文。
            let mut byte = [0_u8; 1];
            while !header.ends_with(b"\r\n\r\n") {
                assert_eq!(stream.read(&mut byte).await.expect("read upload header"), 1);
                header.push(byte[0]);
                assert!(header.len() <= 8192, "fixture headers are bounded");
            }
            // Inspect only the expected request line and framing; do not log credentials.
            // 只检查预期请求行及分帧；不记录凭据。
            let header = String::from_utf8(header).expect("HTTP ASCII headers");
            assert!(header.starts_with("POST /files HTTP/1.1\r\n"));
            // The production file-create primitive declares a bounded JSON request body.
            // 生产文件创建原语声明有界JSON请求正文。
            let length = header.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().expect("content length"))
            }).expect("file-create content length");
            assert!(length <= 8192, "tiny media request remains bounded");
            // The fixture reads the actual transmitted body before returning a controlled response.
            // 夹具在返回受控响应之前读取实际传输正文。
            let mut request = vec![0; length];
            stream.read_exact(&mut request).await.expect("file-create request body");
            if let Some(gate) = reply_gate {
                gate.request_received.send(()).expect("notify actual request body");
                if gate.release_reply.await.is_err() {
                    return serde_json::from_slice(&request).expect("cancelled real file-create JSON");
                }
            }
            // Return headers and body together so intentional error-body rejection cannot race a second write.
            // 一同返回头部及正文，使有意拒绝错误正文不能与第二次写入竞争。
            let mut response = format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
            response.extend_from_slice(&body);
            stream.write_all(&response).await.expect("write upload fixture response");
            let _ = stream.shutdown().await;
            serde_json::from_slice(&request).expect("real file-create JSON")
        }).await.expect("HTTPS upload fixture must terminate")
    });
    // DNS and trust are narrowed to this loopback; redirects and ambient proxies are disabled.
    // DNS及信任收窄到此回环；禁用重定向与环境代理。
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .add_root_certificate(root)
        .resolve("unreachable.invalid", address)
        .build()?;
    Ok((client, server))
}

/// Actual HTTPS failures become precise safe V1 attachment explanations and one failed progress terminal.
/// 实际HTTPS失败转为精确安全V1附件说明及一次失败进度终态。
#[tokio::test]
async fn v1_https_upload_failures_reach_prompt_metadata_and_terminal_progress()
-> Result<(), Box<dyn std::error::Error>> {
    for (status, body, expected) in [
        (
            401,
            r#"{"error":"secret-fixture-marker C:/private/source.png"}"#,
            MediaUploadFailure::Http {
                stage: vulcan_provider_adapters::MediaUploadStage::Transfer,
                status: 401,
            },
        ),
        (
            413,
            "not-json",
            MediaUploadFailure::Http {
                stage: vulcan_provider_adapters::MediaUploadStage::Transfer,
                status: 413,
            },
        ),
        (200, "{}", MediaUploadFailure::MissingReference),
        (200, r#"{"id":""}"#, MediaUploadFailure::InvalidReference),
    ] {
        // Every failure starts with a new project source and genuinely new prompt intention.
        // 每次失败从新项目来源及真正新提示词意图开始。
        let project = tempfile::tempdir()?;
        // The production catalog must retain the original attachment without a successful receipt.
        // 生产目录必须保留原附件而不生成成功回执。
        let registry = Arc::new(ProjectResourceCatalogRegistry::default());
        // A real source is uploaded after all normal metadata and integrity admission.
        // 真实来源在全部正常元数据及完整性准入后上传。
        let entry = register(project.path(), &registry, b"abc")?;
        // Use the repository's existing test-only HTTP injection, not an upload implementation stub.
        // 使用仓库既有仅测试HTTP注入，而非上传实现桩。
        let (http, server) = https_upload_fixture(status, body, None).await?;
        // Record the same observer events consumed by the actual service bridge.
        // 记录实际服务桥消费的相同观察器事件。
        let events = Arc::new(std::sync::Mutex::new(
            Vec::<RuntimeMediaUploadProgressEvent>::new(),
        ));
        // The observer is installed before route resolution freezes its owner.
        // 观察器在路线解析冻结所有者之前安装。
        let observed = Arc::clone(&events);
        // Model, account and policy resolution still use the real runtime pipeline.
        // 模型、账号及策略解析仍使用真实运行时链路。
        let (context, _) = context_with_http(
            false,
            true,
            Some(Arc::new(move |event| observed.lock().unwrap().push(event))),
            Some(http),
        )
        .await?;
        // Exact prompt parameters are supplied by the canonical new-input constructor.
        // 精确提示词参数由规范新输入构造器提供。
        let input = snapshot(
            project.path(),
            entry.resource_id,
            vec![UserPromptParameters::for_new_resources(
                &[entry.resource_id],
            )],
        )?;
        // This runs the entire V1 media consumer, including the real HTTPS upload.
        // 此处运行整个V1媒体消费者，包括真实HTTPS上传。
        let result = prepare_v1_user_media(&input, Some(&context), Arc::clone(&registry)).await;
        // Await the exact server task; it proves the expected three bytes were transmitted once.
        // 等待精确服务器任务；它证明预期三字节被实际传输一次。
        let request = server.await?;
        assert_eq!(request["content_base64"], "YWJj");
        assert_eq!(request["mime_type"], "image/png");
        // Preparation failure retains the prompt attachment rather than reporting native success.
        // 准备失败保留提示词附件，而非报告原生成功。
        let prepared = result?;
        assert!(metadata(&prepared, 0).contains(&expected.to_string()));
        assert!(!metadata(&prepared, 0).contains("secret-fixture-marker"));
        assert!(!metadata(&prepared, 0).contains("C:/private"));
        assert_eq!(prepared.prompts[&0].attachments.len(), 1);
        // No reference completed, but the original upload choice must survive instead of becoming fresh intent.
        // 尚无引用完成，但原上传选择必须保留，不能变为新意图。
        assert!(matches!(
            prepared.bindings[0].origin.resources[0].delivery,
            UserPromptResourceDeliveryKind::UploadPending { .. }
        ));
        assert!(
            load_project_resource_catalog(project.path())?.entries[0]
                .provider_bindings
                .is_empty()
        );
        // The final user-visible diagnostic is generated from the same typed failure facts.
        // 最终用户可见诊断从相同强类型失败事实生成。
        let recorded = events.lock().unwrap();
        assert_eq!(
            recorded.first().unwrap().phase,
            RuntimeMediaUploadPhase::Preparing
        );
        assert_eq!(
            recorded.last().unwrap().phase,
            RuntimeMediaUploadPhase::Failed
        );
        assert_eq!(
            recorded.last().unwrap().error.as_deref(),
            Some(expected.to_string().as_str())
        );
        assert_eq!(
            recorded
                .iter()
                .filter(|event| matches!(
                    event.phase,
                    RuntimeMediaUploadPhase::Failed
                        | RuntimeMediaUploadPhase::Cancelled
                        | RuntimeMediaUploadPhase::Completed
                ))
                .count(),
            1
        );
    }
    Ok(())
}

/// A valid real HTTPS upload produces one immutable receipt, native content and a completed progress row.
/// 有效真实HTTPS上传生成一个不可变回执、原生内容及已完成进度行。
#[tokio::test]
async fn v1_https_upload_success_keeps_real_reference_and_original_source()
-> Result<(), Box<dyn std::error::Error>> {
    // All source and receipt ownership remains inside this isolated production project catalog.
    // 全部来源及回执所有权保留在此隔离生产项目目录内。
    let project = tempfile::tempdir()?;
    // The exact registry is shared by source lookup and append-only receipt publication.
    // 精确注册表由来源查询及仅追加回执发布共用。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // The same source identity is checked before and after the real upload.
    // 真实上传前后检查相同来源身份。
    let entry = register(project.path(), &registry, b"abc")?;
    // This response contains only a valid exact provider file identifier.
    // 此响应仅包含有效精确供应商文件标识。
    let (http, server) = https_upload_fixture(200, r#"{"id":"file-https-success"}"#, None).await?;
    // The production observer records a real, not cached, successful transfer.
    // 生产观察器记录真实而非缓存的成功传输。
    let events = Arc::new(std::sync::Mutex::new(
        Vec::<RuntimeMediaUploadProgressEvent>::new(),
    ));
    // Retain recorder ownership across the callback without borrowing the test stack.
    // 跨回调保留记录器所有权，不借用测试栈。
    let observed = Arc::clone(&events);
    // The frozen execution selects the declared provider-file upload interface.
    // 冻结执行选择声明的供应商文件上传接口。
    let (context, _) = context_with_http(
        false,
        true,
        Some(Arc::new(move |event| observed.lock().unwrap().push(event))),
        Some(http),
    )
    .await?;
    // Only this new prompt can freeze its first original delivery proof.
    // 只有此新提示词可以冻结其首次原投递证明。
    let input = snapshot(
        project.path(),
        entry.resource_id,
        vec![UserPromptParameters::for_new_resources(
            &[entry.resource_id],
        )],
    )?;
    // Preserve the actual result while joining the exact upload server before assertions.
    // 在断言前等待精确上传服务器，同时保留实际结果。
    let result = prepare_v1_user_media(&input, Some(&context), Arc::clone(&registry)).await;
    assert_eq!(server.await?["content_base64"], "YWJj");
    // Native media must carry the actual returned ID, never an invented file reference.
    // 原生媒体必须携带实际返回ID，绝非虚构文件引用。
    let prepared = result?;
    assert_eq!(native(&prepared, 0).id, "file-https-success");
    assert!(matches!(
        prepared.bindings[0].origin.resources[0].delivery,
        UserPromptResourceDeliveryKind::Uploaded { .. }
    ));
    assert_eq!(
        load_project_resource_catalog(project.path())?.entries[0]
            .provider_bindings
            .len(),
        1
    );
    // A completed transfer is distinct from a failed/cancelled row and accounts for its real source bytes.
    // 完成传输区别于失败/取消行，并核算其真实来源字节。
    let recorded = events.lock().unwrap();
    assert!(
        recorded.iter().any(|event| {
            event.phase == RuntimeMediaUploadPhase::Uploading && event.bytes_sent == 3
        }),
        "file-create uploads report actual raw bytes through the V1 progress bridge"
    );
    assert_eq!(
        recorded.last().unwrap().phase,
        RuntimeMediaUploadPhase::Completed
    );
    assert_eq!(recorded.last().unwrap().bytes_sent, 3);
    assert!(recorded.last().unwrap().error.is_none());
    Ok(())
}
