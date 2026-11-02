//! Real loopback coverage for the OpenAI Codex files signing primitive.
//!
//! The fixture speaks plain HTTP/1.1 on a closed loopback origin, so the production create,
//! signed-blob transfer and finalize requests are exercised without any external provider.

use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Exact source bytes written by the fixture handshake helper.
const FIXTURE_SOURCE_BYTES: &[u8] = b"codex-image-bytes";

/// One captured fixture request with its exact wire facts.
#[derive(Debug)]
struct CapturedRequest {
    /// HTTP method used by the production client.
    method: String,
    /// Request target including any query string.
    target: String,
    /// Lowercased header name and trimmed value pairs.
    headers: Vec<(String, String)>,
    /// Exact declared request body bytes.
    body: Vec<u8>,
}

impl CapturedRequest {
    /// Returns the exact value of one lowercased header, when present.
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Reads one bounded fixture request; the peer is the local test client only.
async fn read_fixture_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        let mut chunk = [0_u8; 1024];
        let read = stream.read(&mut chunk).await.expect("fixture request read");
        assert!(read > 0, "fixture request ended before its headers");
        bytes.extend_from_slice(&chunk[..read]);
        assert!(bytes.len() <= 64 * 1024, "fixture headers stay bounded");
    };
    let header = String::from_utf8(bytes[..header_end].to_vec()).expect("ASCII fixture headers");
    let mut lines = header.lines();
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    let method = request_line.next().unwrap_or_default().to_string();
    let target = request_line.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect::<Vec<_>>();
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .map(|(_, value)| value.parse::<usize>().expect("valid content length"))
        .unwrap_or(0);
    let mut body = bytes[header_end..].to_vec();
    while body.len() < length {
        let mut chunk = [0_u8; 4096];
        let read = stream.read(&mut chunk).await.expect("fixture body read");
        assert!(read > 0, "fixture body ended before its declared length");
        body.extend_from_slice(&chunk[..read]);
    }
    assert_eq!(
        body.len(),
        length,
        "fixture body matches its declared length"
    );
    CapturedRequest {
        method,
        target,
        headers,
        body,
    }
}

/// Serves one scripted response per accepted connection and returns every captured request.
fn spawn_fixture_server(
    listener: TcpListener,
    responses: Vec<(u16, String)>,
) -> tokio::task::JoinHandle<Vec<CapturedRequest>> {
    tokio::spawn(async move {
        let mut captured = Vec::new();
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().await.expect("fixture accept");
            captured.push(read_fixture_request(&mut stream).await);
            let mut response = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(body.as_bytes());
            stream.write_all(&response).await.expect("fixture response");
            stream.flush().await.expect("fixture flush");
        }
        captured
    })
}

/// Builds one Codex files channel exactly as a manifest declaration would describe it.
fn codex_channel() -> UploadChannelDeclaration {
    let mut channel = channel(
        UploadPrimitiveKind::SignedObjectUpload,
        vec![String::from("image/png")],
        Some(4096),
        Some(1),
    );
    channel.signing_primitive = Some(crate::UploadSigningPrimitiveKind::OpenAiCodexFiles);
    channel.endpoint_path = Some(String::from("/backend-api/files"));
    // The fixed handshake returns its own fields, so declared pointers must stay unused.
    channel.response_id_pointer = None;
    channel.response_url_pointer = None;
    channel.reference_encoding = Some(UploadReferenceEncoding::ProviderFileId);
    channel.reference_scope = crate::UploadReferenceScope::Credential;
    channel.progress_supported = true;
    channel.cancellation_supported = true;
    channel.timeout_ms = Some(5_000);
    channel
}

/// Builds one exact Codex credential projection with a verified account identity.
fn codex_secret() -> crate::CredentialExecutionSecret {
    crate::CredentialSecretEnvelope::new(crate::CredentialSecretMaterial::OpenAiCodexOAuth {
        tokens: crate::OAuthTokenSet {
            access_token: String::from("test-access-token"),
            refresh_token: String::from("test-refresh-token"),
            issued_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            scopes: Default::default(),
            account_subject: None,
            email: None,
        },
        id_token: None,
        account_id: Some(String::from("account_test")),
    })
    .expect("codex OAuth credential should validate")
    .execution_secret()
    .expect("codex execution secret should resolve")
}

/// Runs the production handshake against `build_responses` and returns its typed facts.
async fn run_codex_handshake<F>(
    build_responses: F,
) -> Result<
    (
        Result<UploadedResourceReference, MediaUploadFailure>,
        Vec<CapturedRequest>,
        u64,
    ),
    Box<dyn std::error::Error>,
>
where
    F: FnOnce(&str) -> Vec<(u16, String)>,
{
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let server = spawn_fixture_server(listener, build_responses(origin.as_str()));

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("diagram.png");
    tokio::fs::write(&path, FIXTURE_SOURCE_BYTES).await?;
    let mut source = resource("image/png", FIXTURE_SOURCE_BYTES.len() as u64);
    source.inline_bytes = None;
    source.local_path = Some(tokio::fs::canonicalize(&path).await?);
    source.file_name = Some(String::from("diagram.png"));
    source.source_identity = Some(crate::PreparedMediaSourceIdentity {
        byte_len: FIXTURE_SOURCE_BYTES.len() as u64,
        content_sha256: hex::encode(Sha256::digest(FIXTURE_SOURCE_BYTES)),
    });

    let channel = codex_channel();
    let mut entry = dummy_entry();
    entry.authentication = crate::AuthenticationProjection::OpenAiCodexBearer;
    let profile = dummy_credential_profile();
    let secret = codex_secret();
    let cancellation = MediaUploadCancellation::new();
    let http = reqwest::Client::builder().no_proxy().build()?;
    let progress = Arc::new(AtomicU64::new(0));
    let progress_recorder = Arc::clone(&progress);
    let callback: crate::MediaUploadProgressCallback = Arc::new(move |sent, _total| {
        progress_recorder.store(sent, Ordering::Relaxed);
    });

    let outcome = materialize_upload(
        UploadExecutionContext {
            http: &http,
            entry: &entry,
            secret: &secret,
            credential_profile: &profile,
            progress: Some(callback),
            cancellation: &cancellation,
            model_id: Some("codex-model"),
            fixture_loopback_origin: Some(origin.as_str()),
        },
        &channel,
        &source,
    )
    .await;
    // A short bound keeps a broken sequence from hanging the suite on a missing request.
    let captured = match tokio::time::timeout(Duration::from_secs(5), server).await {
        Ok(result) => result?,
        Err(_) => return Err("fixture server missed a scripted request".into()),
    };
    Ok((outcome, captured, progress.load(Ordering::Relaxed)))
}

/// Verifies the complete handshake publishes only the file id and never leaks credentials.
#[tokio::test]
async fn codex_files_handshake_publishes_file_id_without_blob_credentials()
-> Result<(), Box<dyn std::error::Error>> {
    let (outcome, captured, progress) = run_codex_handshake(|origin| {
        vec![
            (
                200,
                serde_json::json!({
                    "file_id": "file_codex123",
                    "upload_url": format!("{origin}/blob/file_codex123?sig=secret"),
                })
                .to_string(),
            ),
            (201, String::new()),
            (200, serde_json::json!({ "status": "retry" }).to_string()),
            (
                200,
                serde_json::json!({
                    "status": "success",
                    "download_url": format!("{origin}/download/file_codex123"),
                    "file_name": "diagram.png",
                })
                .to_string(),
            ),
        ]
    })
    .await?;

    let reference = outcome.expect("handshake should publish a reference");
    assert_eq!(reference.reference, "file_codex123");
    assert_eq!(reference.encoding, UploadReferenceEncoding::ProviderFileId);
    assert_eq!(reference.response_id.as_deref(), Some("file_codex123"));
    assert!(
        reference.response_url.is_none(),
        "signed provider URLs are never retained"
    );
    assert_eq!(progress, FIXTURE_SOURCE_BYTES.len() as u64);
    let expected_length = FIXTURE_SOURCE_BYTES.len().to_string();

    assert_eq!(
        captured.len(),
        4,
        "create, blob, retry and success requests"
    );
    let create = &captured[0];
    assert_eq!(create.method, "POST");
    assert_eq!(create.target, "/backend-api/files");
    assert_eq!(
        create.header("authorization"),
        Some("Bearer test-access-token")
    );
    assert_eq!(create.header("chatgpt-account-id"), Some("account_test"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&create.body)?,
        serde_json::json!({
            "file_name": "diagram.png",
            "file_size": FIXTURE_SOURCE_BYTES.len() as u64,
            "use_case": "codex",
        })
    );

    let blob = &captured[1];
    assert_eq!(blob.method, "PUT");
    assert_eq!(blob.target, "/blob/file_codex123?sig=secret");
    assert_eq!(blob.header("x-ms-blob-type"), Some("BlockBlob"));
    assert_eq!(
        blob.header("content-length"),
        Some(expected_length.as_str())
    );
    assert_eq!(blob.body, FIXTURE_SOURCE_BYTES.to_vec());
    assert!(
        blob.header("authorization").is_none(),
        "the signed blob must not receive provider credentials"
    );
    assert!(
        blob.header("chatgpt-account-id").is_none(),
        "the signed blob must not receive the account identity"
    );

    for finalize in &captured[2..] {
        assert_eq!(finalize.method, "POST");
        assert_eq!(finalize.target, "/backend-api/files/file_codex123/uploaded");
        assert_eq!(
            finalize.header("authorization"),
            Some("Bearer test-access-token")
        );
        assert_eq!(finalize.header("chatgpt-account-id"), Some("account_test"));
        assert_eq!(finalize.body, b"{}".to_vec());
    }
    Ok(())
}

/// Verifies one terminal finalization status fails closed instead of publishing a reference.
#[tokio::test]
async fn codex_files_terminal_finalize_status_is_rejected() -> Result<(), Box<dyn std::error::Error>>
{
    let (outcome, captured, _) = run_codex_handshake(|origin| {
        vec![
            (
                200,
                serde_json::json!({
                    "file_id": "file_terminal",
                    "upload_url": format!("{origin}/blob/file_terminal?sig=secret"),
                })
                .to_string(),
            ),
            (201, String::new()),
            (
                200,
                serde_json::json!({ "status": "error", "error_message": "provider rejected" })
                    .to_string(),
            ),
        ]
    })
    .await?;

    assert_eq!(
        outcome,
        Err(MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Transfer,
        })
    );
    assert_eq!(captured.len(), 3);
    Ok(())
}

/// Verifies an unusable provider file id is rejected before any signed blob request.
#[tokio::test]
async fn codex_files_rejects_file_id_that_could_escape_the_finalize_path()
-> Result<(), Box<dyn std::error::Error>> {
    let (outcome, captured, _) = run_codex_handshake(|origin| {
        vec![(
            200,
            serde_json::json!({
                "file_id": "../../escape",
                "upload_url": format!("{origin}/blob/escape?sig=secret"),
            })
            .to_string(),
        )]
    })
    .await?;

    assert_eq!(
        outcome,
        Err(MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Signing,
        })
    );
    assert_eq!(
        captured.len(),
        1,
        "no transfer starts for a rejected file id"
    );
    Ok(())
}

/// Verifies the signed blob destination policy stays closed outside the fixture harness.
#[test]
fn codex_blob_url_requires_production_https() {
    let blob_url = crate::upload_engine::openai_codex::openai_codex_blob_url;
    assert!(
        blob_url(None, "https://account.blob.example.invalid/file?sig=secret").is_ok(),
        "provider-issued HTTPS destinations stay usable"
    );
    for rejected in [
        "http://account.blob.example.invalid/file?sig=secret",
        "https://user:secret@account.blob.example.invalid/file",
        "https://account.blob.example.invalid/file#fragment",
        "account.blob.example.invalid/file",
    ] {
        assert!(
            blob_url(None, rejected).is_err(),
            "signed blob destination `{rejected}` must be rejected in production"
        );
    }
    // Plain HTTP stays available only for the exact loopback fixture authority.
    let origin = "http://127.0.0.1:9000";
    assert!(blob_url(Some(origin), "http://127.0.0.1:9000/blob?sig=1").is_ok());
    assert!(blob_url(Some(origin), "http://127.0.0.1:9001/blob?sig=1").is_err());
    assert!(blob_url(Some(origin), "http://localhost:9000/blob?sig=1").is_err());
}

/// Verifies the finalize URL is derived from the declared create endpoint only.
#[test]
fn codex_finalize_url_appends_one_file_id_segment() {
    let create =
        reqwest::Url::parse("https://chatgpt.com/backend-api/files").expect("fixed create URL");
    assert_eq!(
        crate::upload_engine::openai_codex::openai_codex_finalize_url(&create, "file_123").as_str(),
        "https://chatgpt.com/backend-api/files/file_123/uploaded"
    );
}
