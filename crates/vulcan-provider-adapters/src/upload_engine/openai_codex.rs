//! OpenAI Codex hosted-file handshake behind the `signed_object_upload` primitive.
//!
//! The engine creates one provider file record, streams the original bytes to the provider-issued
//! signed blob without any credential header, then finalizes the record until the provider reports
//! `success`. The publishable result is the plain provider file id consumed by an image route.

use super::*;
use tokio::sync::oneshot;

/// Use case fixed by the official Codex files request.
const OPENAI_FILE_USE_CASE: &str = "codex";

/// Hard protocol limit for one hosted file, fixed by the official client.
const OPENAI_FILE_UPLOAD_LIMIT_BYTES: u64 = 512 * 1024 * 1024;

/// Bounded wait for repeated `retry` finalization answers, in milliseconds.
const OPENAI_FILE_FINALIZE_TIMEOUT_MS: u64 = 30_000;

/// Bounded wait for repeated `retry` finalization answers.
const OPENAI_FILE_FINALIZE_TIMEOUT: Duration =
    Duration::from_millis(OPENAI_FILE_FINALIZE_TIMEOUT_MS);

/// Delay between bounded finalization answers.
const OPENAI_FILE_FINALIZE_RETRY_DELAY: Duration = Duration::from_millis(250);

/// Fixed buffer keeps transfer memory independent of the file size.
const RAW_TRANSFER_CHUNK_BYTES: usize = 64 * 1024;

/// One owned raw upload body with its typed source-completion proof.
struct PreparedRawUploadBody {
    /// Original bytes, hard-capped by the admitted length.
    body: futures_util::stream::BoxStream<'static, Result<Vec<u8>, MediaUploadFailure>>,
    /// Source outcome published only after every admitted byte was exposed.
    completion: oneshot::Receiver<Result<(), MediaUploadFailure>>,
}

/// Created provider file record returned by the files endpoint.
#[derive(Debug, Deserialize)]
struct CreatedOpenAiFile {
    /// Opaque provider file identifier consumed by the route.
    file_id: String,
    /// Provider-issued signed blob destination.
    upload_url: String,
}

/// Finalization result returned by the `uploaded` endpoint.
#[derive(Debug, Deserialize)]
struct FinalizedOpenAiFile {
    /// `success`, `retry`, or a terminal provider failure.
    status: String,
    /// Provider download URL, required on success but never retained.
    #[serde(default)]
    download_url: Option<String>,
}

/// Uploads one resource through the OpenAI Codex files handshake.
///
/// `context` freezes the HTTP client, credential projection, progress and cancellation; `channel`
/// owns the create endpoint and reference encoding; `resource` owns the exact source bytes.
///
/// Returns the provider file id reference, or a typed source/transport/response failure.
pub(super) async fn upload_openai_codex_file(
    context: &UploadExecutionContext<'_>,
    channel: &crate::UploadChannelDeclaration,
    resource: &UploadResource,
) -> Result<UploadedResourceReference, MediaUploadFailure> {
    // A Codex image route consumes exactly the provider file id; other encodings are unusable.
    if channel
        .reference_encoding
        .is_some_and(|encoding| encoding != UploadReferenceEncoding::ProviderFileId)
    {
        return Err(MediaUploadFailure::InvalidContract);
    }
    // Both control-plane calls are POST; another method cannot reach the fixed handshake.
    if !channel
        .method
        .as_deref()
        .is_some_and(|method| method.eq_ignore_ascii_case("POST"))
    {
        return Err(MediaUploadFailure::InvalidContract);
    }
    if resource.byte_len > OPENAI_FILE_UPLOAD_LIMIT_BYTES {
        return Err(MediaUploadFailure::SourceTooLarge {
            actual_bytes: resource.byte_len,
            max_bytes: OPENAI_FILE_UPLOAD_LIMIT_BYTES,
        });
    }
    let create_url = channel_origin_url(channel, context.fixture_loopback_origin)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    // Projection headers belong to the control plane only; they never reach the signed blob.
    let headers = upload_headers(context.entry, context.credential_profile, context.secret)
        .map_err(|_| MediaUploadFailure::InvalidContract)?;
    let file_name = openai_codex_file_name(resource)?;
    let created =
        create_openai_codex_file(context, &create_url, &headers, file_name, resource.byte_len)
            .await?;
    let file_id = validate_openai_codex_file_id(created.file_id.as_str())?;
    let blob_url =
        openai_codex_blob_url(context.fixture_loopback_origin, created.upload_url.as_str())?;
    transfer_openai_codex_blob(context, &blob_url, resource).await?;
    finalize_openai_codex_file(context, channel, &create_url, &headers, file_id).await
}

/// Creates one provider file record through `create_url`.
async fn create_openai_codex_file(
    context: &UploadExecutionContext<'_>,
    create_url: &reqwest::Url,
    headers: &HeaderMap,
    file_name: String,
    byte_len: u64,
) -> Result<CreatedOpenAiFile, MediaUploadFailure> {
    let body = serde_json::json!({
        "file_name": file_name,
        "file_size": byte_len,
        "use_case": OPENAI_FILE_USE_CASE,
    });
    let response = tokio::select! {
        response = context
            .http
            .post(create_url.clone())
            .headers(headers.clone())
            .json(&body)
            .send() => response.map_err(|error| {
            if context.cancellation.is_cancelled() {
                MediaUploadFailure::Cancelled
            } else {
                MediaUploadFailure::transport(MediaUploadStage::Signing, &error)
            }
        })?,
        _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
    };
    let text = read_upload_response(context, response, MediaUploadStage::Signing).await?;
    let payload = crate::provider_tool_execution::parse_bounded_provider_json(
        text.as_str(),
        "OpenAI file create",
    )
    .map_err(|_| MediaUploadFailure::InvalidResponse {
        stage: MediaUploadStage::Signing,
    })?;
    serde_json::from_value::<CreatedOpenAiFile>(payload).map_err(|_| {
        MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Signing,
        }
    })
}

/// Validates one provider file id before it becomes a URL path segment.
fn validate_openai_codex_file_id(candidate: &str) -> Result<String, MediaUploadFailure> {
    let valid = !candidate.is_empty()
        && candidate.len() <= 128
        && candidate
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if !valid {
        return Err(MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Signing,
        });
    }
    Ok(candidate.to_string())
}

/// Resolves the provider-visible file name without inventing an image format.
fn openai_codex_file_name(resource: &UploadResource) -> Result<String, MediaUploadFailure> {
    let candidate = resource
        .file_name
        .as_deref()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            resource
                .local_path
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            format!(
                "resource{}",
                temporary_upload_extension(resource.mime_type.as_str())
            )
        });
    let valid = candidate.len() <= 255
        && !candidate
            .chars()
            .any(|ch| ch.is_control() || ch == '/' || ch == '\\');
    if !valid {
        return Err(MediaUploadFailure::InvalidContract);
    }
    Ok(candidate)
}

/// Validates one provider-issued signed blob destination.
///
/// Production requires credential-free HTTPS. The closed fixture harness runs on a loopback HTTP
/// origin, so plain HTTP is accepted only for that exact origin. Redirects are never followed: the
/// frozen client installs `redirect::Policy::none()`, so a redirect answers as a non-success status.
pub(super) fn openai_codex_blob_url(
    fixture_loopback_origin: Option<&str>,
    candidate: &str,
) -> Result<reqwest::Url, MediaUploadFailure> {
    let invalid = || MediaUploadFailure::InvalidResponse {
        stage: MediaUploadStage::Signing,
    };
    let url = reqwest::Url::parse(candidate).map_err(|_| invalid())?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    if url.scheme() == "https" {
        return Ok(url);
    }
    if url.scheme() == "http"
        && fixture_loopback_origin.is_some_and(|origin| fixture_origin_allows(origin, &url))
    {
        return Ok(url);
    }
    Err(invalid())
}

/// Reports whether `origin` is the same loopback fixture authority as `candidate`.
fn fixture_origin_allows(origin: &str, candidate: &reqwest::Url) -> bool {
    let Ok(origin) = reqwest::Url::parse(origin) else {
        return false;
    };
    origin.scheme() == "http"
        && is_loopback_host(&origin)
        && origin.host_str() == candidate.host_str()
        && origin.port() == candidate.port()
}

/// Reports whether `url` names a loopback IP address rather than a DNS name.
fn is_loopback_host(url: &reqwest::Url) -> bool {
    url.host_str()
        .and_then(|host| {
            host.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .ok()
        })
        .is_some_and(|address| address.is_loopback())
}

/// Derives the finalization URL from the create endpoint and one validated file id.
pub(super) fn openai_codex_finalize_url(create_url: &reqwest::Url, file_id: &str) -> reqwest::Url {
    let mut finalize_url = create_url.clone();
    let path = format!(
        "{}/{file_id}/uploaded",
        create_url.path().trim_end_matches('/')
    );
    finalize_url.set_path(path.as_str());
    finalize_url
}

/// Streams the exact source bytes to the signed blob without any credential header.
async fn transfer_openai_codex_blob(
    context: &UploadExecutionContext<'_>,
    blob_url: &reqwest::Url,
    resource: &UploadResource,
) -> Result<(), MediaUploadFailure> {
    let PreparedRawUploadBody {
        body,
        mut completion,
    } = prepare_raw_upload_body(
        resource,
        context.progress.clone(),
        context.cancellation.clone(),
    )
    .await?;
    // The signed destination receives the block type, a request id and the exact length only.
    let request = context
        .http
        .put(blob_url.clone())
        .header("x-ms-blob-type", "BlockBlob")
        .header("x-ms-client-request-id", uuid::Uuid::new_v4().to_string())
        .header(reqwest::header::CONTENT_LENGTH, resource.byte_len)
        .body(reqwest::Body::wrap_stream(body));
    let response = tokio::select! {
        response = request.send() => response.map_err(|error| {
            if context.cancellation.is_cancelled() {
                MediaUploadFailure::Cancelled
            } else if let Ok(Err(failure)) = completion.try_recv() {
                failure
            } else {
                MediaUploadFailure::transport(MediaUploadStage::Transfer, &error)
            }
        })?,
        _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
    };
    read_upload_response(context, response, MediaUploadStage::Transfer).await?;
    // An early 2xx cannot prove the source was consumed; wait for the typed completion.
    tokio::select! {
        biased;
        _ = context.cancellation.wait() => Err(MediaUploadFailure::Cancelled),
        result = completion => match result {
            Ok(inner) => inner,
            Err(_) => Err(MediaUploadFailure::SourceUnavailable),
        },
    }
}

/// Finalizes one created record until the provider reports `success`.
async fn finalize_openai_codex_file(
    context: &UploadExecutionContext<'_>,
    channel: &crate::UploadChannelDeclaration,
    create_url: &reqwest::Url,
    headers: &HeaderMap,
    file_id: String,
) -> Result<UploadedResourceReference, MediaUploadFailure> {
    let finalize_url = openai_codex_finalize_url(create_url, file_id.as_str());
    let started = tokio::time::Instant::now();
    loop {
        let response = tokio::select! {
            response = context
                .http
                .post(finalize_url.clone())
                .headers(headers.clone())
                .json(&serde_json::json!({}))
                .send() => response.map_err(|error| {
                if context.cancellation.is_cancelled() {
                    MediaUploadFailure::Cancelled
                } else {
                    MediaUploadFailure::transport(MediaUploadStage::Transfer, &error)
                }
            })?,
            _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
        };
        let text = read_upload_response(context, response, MediaUploadStage::Transfer).await?;
        let payload = crate::provider_tool_execution::parse_bounded_provider_json(
            text.as_str(),
            "OpenAI file finalize",
        )
        .map_err(|_| MediaUploadFailure::InvalidResponse {
            stage: MediaUploadStage::Transfer,
        })?;
        let finalized = serde_json::from_value::<FinalizedOpenAiFile>(payload).map_err(|_| {
            MediaUploadFailure::InvalidResponse {
                stage: MediaUploadStage::Transfer,
            }
        })?;
        match finalized.status.as_str() {
            "success" => {
                // The official client treats a success without a download URL as a failed upload.
                if finalized.download_url.is_none() {
                    return Err(MediaUploadFailure::InvalidResponse {
                        stage: MediaUploadStage::Transfer,
                    });
                }
                let reference = render_reference(channel, Some(file_id.clone()), None)?;
                return Ok(UploadedResourceReference {
                    reference,
                    encoding: UploadReferenceEncoding::ProviderFileId,
                    response_id: Some(file_id),
                    response_url: None,
                });
            }
            "retry" => {
                let remaining = OPENAI_FILE_FINALIZE_TIMEOUT.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    // The bounded readiness wait expired; report the bound that actually elapsed.
                    return Err(MediaUploadFailure::Timeout {
                        channel_id: channel.upload_channel_id.clone(),
                        timeout_ms: OPENAI_FILE_FINALIZE_TIMEOUT_MS,
                    });
                }
                tokio::select! {
                    _ = tokio::time::sleep(OPENAI_FILE_FINALIZE_RETRY_DELAY.min(remaining)) => {}
                    _ = context.cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
                }
            }
            _ => {
                return Err(MediaUploadFailure::InvalidResponse {
                    stage: MediaUploadStage::Transfer,
                });
            }
        }
    }
}

/// Opens `resource` and frames one capped raw body with progress, cancellation and completion proof.
async fn prepare_raw_upload_body(
    resource: &UploadResource,
    progress: Option<MediaUploadProgressCallback>,
    cancellation: MediaUploadCancellation,
) -> Result<PreparedRawUploadBody, MediaUploadFailure> {
    if cancellation.is_cancelled() {
        return Err(MediaUploadFailure::Cancelled);
    }
    let expected_identity = resource.source_identity.clone();
    if expected_identity
        .as_ref()
        .is_some_and(|identity| identity.byte_len != resource.byte_len)
    {
        return Err(MediaUploadFailure::SourceChanged);
    }
    // Inline bytes take precedence; otherwise the exact frozen handle supplies the bytes.
    let mut source: Box<dyn tokio::io::AsyncRead + Unpin + Send> = if let Some(bytes) =
        &resource.inline_bytes
    {
        if bytes.len() as u64 != resource.byte_len {
            return Err(MediaUploadFailure::SourceChanged);
        }
        Box::new(std::io::Cursor::new(bytes.clone()))
    } else {
        let path = resource
            .local_path
            .as_ref()
            .ok_or(MediaUploadFailure::SourceUnavailable)?;
        let file = tokio::select! {
            biased;
            _ = cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
            result = tokio::fs::File::open(path) => {
                result.map_err(|_| MediaUploadFailure::SourceUnavailable)?
            }
        };
        let metadata = tokio::select! {
            biased;
            _ = cancellation.wait() => return Err(MediaUploadFailure::Cancelled),
            result = file.metadata() => result.map_err(|_| MediaUploadFailure::SourceUnavailable)?,
        };
        if !metadata.is_file() || metadata.len() != resource.byte_len {
            return Err(MediaUploadFailure::SourceChanged);
        }
        Box::new(file)
    };
    let total_bytes = resource.byte_len;
    let (completion_sender, completion) = oneshot::channel();
    // The typed completion survives reqwest's body-error wrapper without parsing messages.
    let encoded: futures_util::stream::BoxStream<'static, Result<Vec<u8>, MediaUploadFailure>> =
        async_stream::try_stream! {
            let mut buffer = vec![0_u8; RAW_TRANSFER_CHUNK_BYTES];
            let mut digest = Sha256::new();
            let mut sent = 0_u64;
            while sent < total_bytes {
                if cancellation.is_cancelled() {
                    Err(MediaUploadFailure::Cancelled)?;
                }
                let length = (total_bytes - sent).min(RAW_TRANSFER_CHUNK_BYTES as u64) as usize;
                let read = tokio::select! {
                    biased;
                    _ = cancellation.wait() => Err(MediaUploadFailure::Cancelled),
                    result = source.read(&mut buffer[..length]) => {
                        result.map_err(|_| MediaUploadFailure::SourceUnavailable)
                    }
                }?;
                if read == 0 {
                    Err(MediaUploadFailure::SourceChanged)?;
                }
                sent = sent.saturating_add(read as u64);
                digest.update(&buffer[..read]);
                if let Some(callback) = &progress {
                    callback(sent, Some(total_bytes));
                }
                yield buffer[..read].to_vec();
            }
            // One extra byte detects growth without exceeding the admitted length on the wire.
            let mut extra = [0_u8; 1];
            let extra_bytes = tokio::select! {
                biased;
                _ = cancellation.wait() => Err(MediaUploadFailure::Cancelled),
                result = source.read(&mut extra) => {
                    result.map_err(|_| MediaUploadFailure::SourceUnavailable)
                }
            }?;
            if extra_bytes != 0
                || expected_identity
                    .is_some_and(|identity| hex::encode(digest.finalize()) != identity.content_sha256)
            {
                Err(MediaUploadFailure::SourceChanged)?;
            }
        }
        .boxed();
    // With an exact Content-Length the client stops polling after the last byte, so each chunk is
    // held back until the next one or the validated end arrives: the completion proof is published
    // before the final byte is handed over.
    let body = stream! {
        tokio::pin!(encoded);
        let mut pending: Option<Vec<u8>> = None;
        while let Some(chunk) = encoded.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Some(previous) = pending.replace(bytes) {
                        yield Ok(previous);
                    }
                }
                Err(failure) => {
                    let _ = completion_sender.send(Err(failure.clone()));
                    yield Err(failure);
                    return;
                }
            }
        }
        let _ = completion_sender.send(Ok(()));
        if let Some(last) = pending {
            yield Ok(last);
        }
    };
    Ok(PreparedRawUploadBody {
        body: body.boxed(),
        completion,
    })
}
