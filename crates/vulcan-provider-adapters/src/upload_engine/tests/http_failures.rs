//! Real loopback HTTP failures exercise production upload primitives without external providers.
//! 真实回环HTTP失败覆盖生产上传原语，不访问外部供应商。

use super::*;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

/// Reads one fixture `stream` request bounded by `max_bytes`, preserving actual method and body bytes.
/// 读取一个受`max_bytes`约束的夹具`stream`请求，保留实际方法及正文字节。
/// Supports the exact GET policy and length-declared POST requests produced by the tested primitives.
/// 支持被测原语产生的精确GET策略请求及声明长度的POST请求。
async fn read_fixture_request(stream: &mut TcpStream, max_bytes: usize) -> Vec<u8> {
    // Each fixture supplies its own small trusted bound, never a length chosen by the peer.
    // 每个夹具提供自己的小型可信限制，绝不使用对端选择的长度。
    let mut bytes = Vec::new();
    loop {
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            // Headers come from the real HTTP client and are inspected without logging credentials.
            // 头部来自真实HTTP客户端，检查时不记录凭据。
            let header = std::str::from_utf8(&bytes[..end]).expect("ASCII request headers");
            // GET has no body; tested POST primitives always declare their complete content length.
            // GET没有正文；被测POST原语始终声明完整内容长度。
            let length = if header.starts_with("GET ") {
                0
            } else {
                assert!(header.starts_with("POST "));
                header
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().expect("valid content length"))
                    })
                    .expect("bounded fixture POST must carry Content-Length")
            };
            assert!(end + 4 + length <= max_bytes, "fixture frame is bounded");
            if bytes.len() >= end + 4 + length {
                return bytes;
            }
        }
        // Never allocate based on arbitrary peer input.
        // 绝不依据任意对端输入分配内存。
        let mut chunk = [0; 1024];
        // The enclosing fixture deadline bounds this real socket read.
        // 外围夹具截止时间约束此次真实套接字读取。
        let count = stream.read(&mut chunk).await.expect("fixture request read");
        assert!(count > 0, "request ended before its declared body");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(
            bytes.len() <= max_bytes,
            "fixture request cannot grow without a bound"
        );
    }
}

/// Runs one isolated real upload using `primitive` against controlled `status` and `body`.
/// 使用`primitive`对受控`status`及`body`执行一次隔离真实上传。
/// `stall_body` sends only response headers; returns typed production outcome and exact received request bytes.
/// `stall_body`仅发送响应头；返回强类型生产结果及实际收到的请求字节。
async fn run_http_fixture(
    primitive: UploadPrimitiveKind,
    status: u16,
    body: &str,
    stall_body: bool,
) -> Result<
    (
        Result<UploadedResourceReference, MediaUploadFailure>,
        Vec<u8>,
    ),
    Box<dyn std::error::Error>,
> {
    // A loopback listener is the only network target for this test.
    // 回环监听器是本测试唯一网络目标。
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    // The existing closed fixture-origin hook validates a loopback IP and explicit port.
    // 既有封闭夹具源钩子验证回环IP及显式端口。
    let origin = format!("http://{}", listener.local_addr()?);
    // Own the small response body in the task without borrowing the caller's stack.
    // 在任务内持有小型响应正文，不借用调用方栈。
    let response_body = body.as_bytes().to_vec();
    // This bounded fixture task terminates after the exact single request or client close.
    // 此有界夹具任务在精确单个请求或客户端关闭后终结。
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async move {
            // Accept only the upload being tested; no user service is involved.
            // 仅接收被测上传；不涉及用户服务。
            let (mut stream, _) = listener.accept().await.expect("fixture accept");
            // Keep bytes for assertions, not for console output.
            // 保留字节用于断言，而非控制台输出。
            let request = read_fixture_request(&mut stream, 16 * 1024).await;
            // Stalled bodies deliberately remain incomplete after their HTTP status is available.
            // 停滞正文在HTTP状态已可用之后故意保持未完成。
            let length = if stall_body { 4096 } else { response_body.len() };
            // Write complete responses together so status-first rejection cannot race a second body write.
            // 完整响应一起写入，避免状态优先拒绝与第二次正文写入竞争。
            let mut response = format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {length}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n").into_bytes();
            if !stall_body {
                response.extend_from_slice(&response_body);
            }
            stream.write_all(&response).await.expect("fixture response");
            if stall_body {
                // Rejection or timeout must drop the response and close this connection.
                // 拒绝或超时必须丢弃响应并关闭此连接。
                let mut byte = [0];
                assert_eq!(stream.read(&mut byte).await.expect("client closes stalled response"), 0);
            }
            request
        }).await.expect("fixture must terminate within five seconds")
    });
    // Real local sources stay inside an automatically removed temporary directory.
    // 真实本地来源保持在自动移除的临时目录内。
    let directory = tempfile::tempdir()?;
    // Every primitive consumes the same exact original three bytes.
    // 每种原语消费相同精确原始三字节。
    let path = directory.path().join("source.png");
    tokio::fs::write(&path, b"abc").await?;
    // The selected channel, not a provider name, determines the production upload primitive.
    // 由选中通道而非供应商名称决定生产上传原语。
    let mut channel = channel(primitive, vec!["image/png".into()], Some(1024), Some(1));
    channel.timeout_ms = Some(1000);
    channel.response_url_pointer = None;
    if primitive == UploadPrimitiveKind::SignedObjectUpload {
        channel.signing_primitive = Some(crate::UploadSigningPrimitiveKind::TemporaryOssPolicy);
        channel.method = Some("GET".into());
    }
    // Multipart and signed primitives require a real file; file-create reads the same owned bytes.
    // Multipart和签名原语要求真实文件；文件创建读取相同所有字节。
    let mut source = resource("image/png", 3);
    source.inline_bytes = None;
    source.local_path = Some(tokio::fs::canonicalize(&path).await?);
    source.file_name = Some("source.png".into());
    // These are isolated non-user credentials and routes from existing upload tests.
    // 这些是来自既有上传测试的隔离非用户凭据及路线。
    let cancellation = MediaUploadCancellation::new();
    // Execute the real deadline, HTTP client, response reader and reference validator.
    // 执行真实截止时间、HTTP客户端、响应读取器及引用验证器。
    let outcome = materialize_upload(
        UploadExecutionContext {
            http: &reqwest::Client::builder().no_proxy().build()?,
            entry: &dummy_entry(),
            secret: &dummy_secret(),
            credential_profile: &dummy_credential_profile(),
            progress: None,
            cancellation: &cancellation,
            model_id: Some("fixture-model"),
            fixture_loopback_origin: Some(&origin),
        },
        &channel,
        &source,
    )
    .await;
    Ok((outcome, server.await?))
}

/// Actual multipart and file-create statuses are preserved without reading or echoing provider diagnostics.
/// 保留实际Multipart及文件创建状态，不读取或回显供应商诊断。
#[tokio::test]
async fn upload_http_errors_preserve_status_without_error_body_leakage()
-> Result<(), Box<dyn std::error::Error>> {
    for primitive in [
        UploadPrimitiveKind::MultipartFile,
        UploadPrimitiveKind::ProviderFileCreate,
    ] {
        for status in [401, 403, 413, 429, 500] {
            // Deliberately sensitive-looking response text must never enter the typed failure.
            // 故意类似敏感信息的响应文本绝不能进入强类型失败。
            let (outcome, request) = run_http_fixture(
                primitive,
                status,
                r#"{"error":"secret-fixture-marker C:/private/source.png"}"#,
                false,
            )
            .await?;
            // The HTTP response, not its body text, owns the asserted status.
            // 被断言状态归HTTP响应而非其正文文本所有。
            let failure = outcome.unwrap_err();
            assert_eq!(
                failure,
                MediaUploadFailure::Http {
                    stage: MediaUploadStage::Transfer,
                    status
                }
            );
            assert!(!failure.to_string().contains("secret-fixture-marker"));
            assert!(!format!("{failure:?}").contains("private/source"));
            assert!(request.starts_with(b"POST /v1/files HTTP/1.1\r\n"));
        }
    }
    Ok(())
}

/// A non-success response with a never-finishing body is still its original HTTP error, not a later timeout.
/// 正文永不完成的非成功响应仍是原HTTP错误，而非较晚超时。
#[tokio::test]
async fn upload_http_status_precedes_stalled_error_body() -> Result<(), Box<dyn std::error::Error>>
{
    for primitive in [
        UploadPrimitiveKind::MultipartFile,
        UploadPrimitiveKind::ProviderFileCreate,
    ] {
        // The server cannot complete the declared body; status-only rejection must close it immediately.
        // 服务器无法完成声明正文；仅状态拒绝必须立即关闭它。
        let (outcome, _) = run_http_fixture(primitive, 413, "", true).await?;
        assert_eq!(
            outcome.unwrap_err(),
            MediaUploadFailure::Http {
                stage: MediaUploadStage::Transfer,
                status: 413
            }
        );
    }
    Ok(())
}

/// Successful HTTP responses distinguish missing references, invalid reference values and malformed JSON.
/// 成功HTTP响应区分缺失引用、无效引用值及畸形JSON。
#[tokio::test]
async fn upload_http_success_requires_the_declared_reference()
-> Result<(), Box<dyn std::error::Error>> {
    for primitive in [
        UploadPrimitiveKind::MultipartFile,
        UploadPrimitiveKind::ProviderFileCreate,
    ] {
        for (body, expected) in [
            ("{}", MediaUploadFailure::MissingReference),
            (r#"{"id":""}"#, MediaUploadFailure::InvalidReference),
            (r#"{"id":42}"#, MediaUploadFailure::InvalidReference),
            (
                "not-json",
                MediaUploadFailure::InvalidResponse {
                    stage: MediaUploadStage::Transfer,
                },
            ),
        ] {
            // Each response is actually decoded by the production reference reader.
            // 每个响应都由生产引用读取器实际解码。
            let (outcome, _) = run_http_fixture(primitive, 200, body, false).await?;
            assert_eq!(outcome.unwrap_err(), expected);
        }
        // Valid references still reach the existing successful materialization result.
        // 有效引用仍进入既有成功物化结果。
        let (outcome, _) =
            run_http_fixture(primitive, 200, r#"{"id":"file-valid"}"#, false).await?;
        assert_eq!(outcome?.reference, "file-valid");
    }
    Ok(())
}

/// Signing failures retain their stage and cannot start the object-transfer phase.
/// 签名失败保留其阶段，且不能启动对象传输阶段。
#[tokio::test]
async fn upload_http_signing_failure_keeps_its_own_stage() -> Result<(), Box<dyn std::error::Error>>
{
    // A real policy request is rejected before any object URL or signature is available.
    // 真实策略请求在任何对象URL或签名可用之前被拒绝。
    let (outcome, request) = run_http_fixture(
        UploadPrimitiveKind::SignedObjectUpload,
        401,
        "not-json",
        false,
    )
    .await?;
    assert_eq!(
        outcome.unwrap_err(),
        MediaUploadFailure::Http {
            stage: MediaUploadStage::Signing,
            status: 401
        }
    );
    assert!(
        request.starts_with(b"GET /v1/files?action=getPolicy&model=fixture-model HTTP/1.1\r\n")
    );
    Ok(())
}

/// A successful status with a stalled body remains bounded by the selected channel's actual total timeout.
/// 状态成功但正文停滞仍受选中通道实际总超时约束。
#[tokio::test]
async fn upload_http_success_body_uses_total_timeout() -> Result<(), Box<dyn std::error::Error>> {
    // The timeout must carry the exact selected declaration rather than an inferred HTTP status.
    // 超时必须携带精确选中声明，而非推测HTTP状态。
    let (outcome, _) =
        run_http_fixture(UploadPrimitiveKind::ProviderFileCreate, 200, "", true).await?;
    assert_eq!(
        outcome.unwrap_err(),
        MediaUploadFailure::Timeout {
            channel_id: "test.channel".into(),
            timeout_ms: 1000
        }
    );
    Ok(())
}

/// A multi-chunk local source reaches real HTTP as one exact-length JSON request with raw-byte progress.
/// 多块本地来源以一个精确长度JSON请求到达真实HTTP，并报告原始字节进度。
#[tokio::test]
async fn file_create_http_streams_exact_json_and_raw_progress()
-> Result<(), Box<dyn std::error::Error>> {
    for length in [0_usize, 131_075, 600_001] {
        // The payload crosses several 48 KiB chunks and ends with nontrivial Base64 padding.
        // 负载跨越多个48KiB块，并以非平凡Base64填充结束。
        let bytes: Vec<u8> = (0..length).map(|index| (index % 251) as u8).collect();
        // Only this isolated loopback listener receives the request.
        // 只有此隔离回环监听器接收请求。
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        // Use the existing strict fixture-origin seam, with no network-authority changes.
        // 使用既有严格夹具源接缝，不改变网络权限。
        let origin = format!("http://{}", listener.local_addr()?);
        // The server consumes the actual declared Content-Length before acknowledging the uploaded file.
        // 服务端在确认上传文件前消费实际声明的Content-Length。
        let server = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), async move {
                // One real connection suffices; no user service is started.
                // 一个真实连接即可；不启动用户服务。
                let (mut stream, _) = listener.accept().await.expect("fixture accept");
                // The fixed test bound covers only the known fixture body plus headers.
                // 固定测试限制仅覆盖已知夹具正文及头部。
                let request = read_fixture_request(&mut stream, 1024 * 1024).await;
                // Keep the response small and complete, so source-body completion is the only new variable.
                // 保持响应小且完整，使来源正文完成成为唯一新变量。
                let body = br#"{"id":"file-streamed"}"#;
                // Headers and response body are sent atomically from the fixture's perspective.
                // 从夹具角度将头部及响应正文作为一次写入发送。
                let mut response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
                response.extend_from_slice(body);
                stream.write_all(&response).await.expect("fixture response");
                request
            }).await.expect("fixture bounded completion")
        });
        // Real source files are temporary and never alias workspace attachments.
        // 真实来源文件均为临时文件，绝不与工作区附件混用。
        let directory = tempfile::tempdir()?;
        // A canonical local entity is required; inline bytes are explicitly absent.
        // 要求规范本地实体；内联字节明确不存在。
        let path = directory.path().join("source.bin");
        tokio::fs::write(&path, &bytes).await?;
        // The canonical digest is verified before transfer and against the bytes actually streamed.
        // 在传输前及针对实际流式字节校验规范摘要。
        let source = UploadResource {
            source_identity: Some(crate::PreparedMediaSourceIdentity {
                byte_len: length as u64,
                content_sha256: hex::encode(Sha256::digest(&bytes)),
            }),
            file_name: Some("source.bin".into()),
            local_path: Some(tokio::fs::canonicalize(path).await?),
            inline_bytes: None,
            mime_type: "application/octet-stream".into(),
            byte_len: length as u64,
            remote_url: None,
            item_count: 1,
        };
        // No fabricated upload ceiling is needed to make a declared unbounded interface stream safely.
        // 无需捏造上传上限，即可让声明无限制的接口安全流式传输。
        let mut selected = channel(
            UploadPrimitiveKind::ProviderFileCreate,
            vec![],
            None,
            Some(1),
        );
        selected.response_url_pointer = None;
        selected.timeout_ms = Some(5_000);
        selected
            .fixed_fields
            .insert("purpose".into(), "文件\n\"\\".into());
        // Observe exact source byte counts from the real HTTP body's consumption.
        // 从真实HTTP正文消费观察精确来源字节数。
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        // Callback ownership is limited to this isolated request.
        // 回调所有权仅限此隔离请求。
        let recorded = std::sync::Arc::clone(&events);
        // Run production preflight, timeout, stream framing, response and reference validation together.
        // 一起运行生产预检、超时、流式封装、响应及引用校验。
        let result = materialize_upload(
            UploadExecutionContext {
                http: &reqwest::Client::builder().no_proxy().build()?,
                entry: &dummy_entry(),
                secret: &dummy_secret(),
                credential_profile: &dummy_credential_profile(),
                progress: Some(std::sync::Arc::new(move |sent, total| {
                    recorded.lock().expect("events").push((sent, total))
                })),
                cancellation: &MediaUploadCancellation::new(),
                model_id: None,
                fixture_loopback_origin: Some(&origin),
            },
            &selected,
            &source,
        )
        .await?;
        assert_eq!(result.reference, "file-streamed");
        // Parse only the actual received request, not a second independently constructed upload object.
        // 仅解析实际收到的请求，而非第二份独立构造的上传对象。
        let request = server.await?;
        // The tested reader already checked the content-length bound; verify complete framing and media bytes as well.
        // 被测读取器已检查内容长度限制；另外验证完整封装及媒体字节。
        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("HTTP headers");
        // Header names are HTTP case-insensitive.
        // HTTP头部名称不区分大小写。
        let header = std::str::from_utf8(&request[..header_end])?.to_ascii_lowercase();
        assert!(header.starts_with("post /v1/files "));
        assert!(header.contains("content-type: application/json"));
        assert!(!header.contains("transfer-encoding:"));
        // Inspect the exact declared byte length rather than relying on successful JSON parsing alone.
        // 检查精确声明字节长度，而非仅依赖JSON解析成功。
        let declared = header
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .expect("length")
            .trim()
            .parse::<usize>()?;
        assert_eq!(request.len() - header_end - 4, declared);
        // Reserved fields and user-declared purpose remain unchanged semantically.
        // 保留字段及用户声明用途在语义上保持不变。
        let json: Value = serde_json::from_slice(&request[header_end + 4..])?;
        assert_eq!(
            json,
            serde_json::json!({"mime_type":"application/octet-stream", "content_base64":base64::engine::general_purpose::STANDARD.encode(&bytes), "purpose":"文件\n\"\\"})
        );
        // Empty files emit no invented raw progress; nonempty files report bounded, monotonic raw counts.
        // 空文件不产生虚构原始进度；非空文件报告有界单调原始计数。
        let actual = events.lock().expect("events");
        if length == 0 {
            assert!(actual.is_empty());
        } else {
            // The larger fixture also proves file-upload framing does not inherit the conversation's 512 KiB inline limit.
            // 更大夹具还证明文件上传封装不会继承会话512KiB内联限制。
            let expected: Vec<_> = (1..=length.div_ceil(49_152))
                .map(|index| ((index * 49_152).min(length) as u64, Some(length as u64)))
                .collect();
            assert_eq!(actual.as_slice(), expected.as_slice());
        }
    }
    Ok(())
}

/// HTTP body failures retain their typed source cause, and an early successful response cannot bypass completion.
/// HTTP正文失败保留强类型来源原因，提前成功响应不能绕过完成校验。
#[tokio::test]
async fn file_create_http_source_failure_and_early_success_are_not_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    for early_response in [false, true] {
        // This fixture never waits for a valid complete JSON request from a deliberately damaged source.
        // 此夹具绝不等待故意损坏来源产生有效完整JSON请求。
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        // Both branches remain on the same strict loopback-only seam.
        // 两个分支均保持在相同严格仅回环接缝上。
        let origin = format!("http://{}", listener.local_addr()?);
        // The server either closes after the failed body or fabricates success immediately after the headers.
        // 服务端在正文失败后关闭，或在头部之后立即虚构成功。
        let server = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), async move {
                // A single real upload connection is owned by this bounded fixture.
                // 此有界夹具持有单个真实上传连接。
                let (mut stream, _) = listener.accept().await.expect("fixture accept");
                // Retain only bounded received bytes to detect the HTTP header boundary.
                // 仅保留有界收到字节以识别HTTP头部边界。
                let mut received = Vec::new();
                loop {
                    // Small fixed socket chunks keep the fixture independent of the declared request length.
                    // 小型固定套接字块使夹具独立于声明请求长度。
                    let mut chunk = [0_u8; 4096];
                    // Source failure may close or reset the socket; neither is an upload acknowledgement.
                    // 来源失败可能关闭或重置套接字；两者均不是上传确认。
                    let count = match stream.read(&mut chunk).await {
                        Ok(count) => count,
                        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => 0,
                        Err(error) => panic!("fixture socket failed: {error}"),
                    };
                    if count == 0 {
                        break;
                    }
                    received.extend_from_slice(&chunk[..count]);
                    assert!(received.len() < 3 * 1024 * 1024);
                    if early_response && received.windows(4).any(|window| window == b"\r\n\r\n") {
                        // This reference is valid syntactically but must never be returned for an incomplete source.
                        // 此引用语法有效，但绝不能为不完整来源返回。
                        let body = br#"{"id":"must-not-be-accepted"}"#;
                        // Send a complete response without consuming or validating the source body.
                        // 未消费或校验来源正文就发送完整响应。
                        let mut response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        response.extend_from_slice(body);
                        stream.write_all(&response).await.expect("early response");
                        break;
                    }
                }
            })
            .await
            .expect("fixture bounded completion");
        });
        // The original entity and proof agree during the production pre-upload hash check.
        // 在生产上传前哈希检查时，原实体与证明一致。
        let directory = tempfile::tempdir()?;
        // A multi-megabyte source cannot finish between the first prefix and the deliberate first-chunk mutation.
        // 多兆字节来源不能在首个前缀与故意首次分块修改之间完成。
        let bytes = vec![7_u8; 2 * 1024 * 1024];
        // Keep the exact source path available to a synchronous progress fault injector.
        // 将精确来源路径交给同步进度故障注入器。
        let path = directory.path().join("source.bin");
        tokio::fs::write(&path, &bytes).await?;
        // No mutable model argument supplies the host identity.
        // 宿主身份不由可变模型参数提供。
        let source = UploadResource {
            source_identity: Some(crate::PreparedMediaSourceIdentity {
                byte_len: bytes.len() as u64,
                content_sha256: hex::encode(Sha256::digest(&bytes)),
            }),
            file_name: Some("source.bin".into()),
            local_path: Some(tokio::fs::canonicalize(&path).await?),
            inline_bytes: None,
            mime_type: "application/octet-stream".into(),
            byte_len: bytes.len() as u64,
            remote_url: None,
            item_count: 1,
        };
        // The existing declaration defines the whole bounded transfer, including any early-response wait.
        // 既有声明定义整个有界传输，包括任何提前响应等待。
        let mut selected = channel(
            UploadPrimitiveKind::ProviderFileCreate,
            vec![],
            None,
            Some(1),
        );
        selected.response_url_pointer = None;
        selected.timeout_ms = Some(5_000);
        // Truncate only after at least one real source chunk is read, simulating a mutation after preflight.
        // 仅在至少读取一个真实来源块后截短，模拟预检后的修改。
        let progress: MediaUploadProgressCallback = std::sync::Arc::new(move |_, _| {
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .expect("fixture source")
                .set_len(0)
                .expect("fixture truncate");
        });
        // The actual materializer must reject the receipt, not merely fail a standalone test-only validator.
        // 必须由实际物化器拒绝回执，而非仅由独立测试校验器失败。
        let result = materialize_upload(
            UploadExecutionContext {
                http: &reqwest::Client::builder().no_proxy().build()?,
                entry: &dummy_entry(),
                secret: &dummy_secret(),
                credential_profile: &dummy_credential_profile(),
                progress: Some(progress),
                cancellation: &MediaUploadCancellation::new(),
                model_id: None,
                fixture_loopback_origin: Some(&origin),
            },
            &selected,
            &source,
        )
        .await;
        if early_response {
            assert!(
                matches!(
                    result,
                    Err(MediaUploadFailure::SourceChanged
                        | MediaUploadFailure::SourceUnavailable
                        | MediaUploadFailure::Transport { .. })
                ),
                "early HTTP success must not become an upload receipt: {result:?}"
            );
        } else {
            assert_eq!(result, Err(MediaUploadFailure::SourceChanged));
        }
        server.await?;
    }
    Ok(())
}
