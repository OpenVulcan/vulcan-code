//! Alibaba provider-native media tools and temporary resource materialization.
//! 阿里云供应商原生媒体工具与临时资源物化。

use crate::media::{MediaUploadCancellation, MediaUploadProgressCallback};
use crate::provider_task::{
    ProviderAsyncTaskPhase, ProviderAsyncTaskPoll, ProviderAsyncTaskRecoveryContext,
    ProviderAsyncTaskStart,
};
use crate::{
    CredentialExecutionSecret, ProviderToolAdapterKind, ProviderToolExecutionResult,
    ProviderToolGeneratedResource, ProviderToolNormalizedOutput, ProviderToolTarget, ServiceEntry,
};
use async_stream::stream;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

/// Maximum time spent waiting for one Alibaba asynchronous task.
/// 等待单个阿里云异步任务的最长时间。
const ALIBABA_TASK_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Polling cadence copied from the verified Alibaba task drivers.
/// 从已验证阿里云任务 Driver 复制的轮询间隔。
const ALIBABA_TASK_POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Trusted host-only local file descriptor inserted after AI-visible schema validation.
/// 在 AI 可见 Schema 校验后插入的受信任仅宿主本地文件描述符。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaHostFileDescriptor {
    /// Canonical absolute source path.
    /// 规范绝对来源路径。
    absolute_path: String,
    /// Exact detected MIME type.
    /// 精确检测 MIME 类型。
    mime_type: String,
    /// Exact byte length observed by the host.
    /// 宿主观测到的精确字节长度。
    size_bytes: u64,
}

/// Temporary Alibaba OSS policy response.
/// 阿里云临时 OSS 策略响应。
#[derive(Debug, Deserialize)]
struct AlibabaOssPolicyEnvelope {
    /// Signed upload policy fields.
    /// 已签名上传策略字段。
    data: AlibabaOssPolicy,
}

/// Signed multipart fields returned by the verified Alibaba upload endpoint.
/// 已验证阿里云上传端点返回的签名 Multipart 字段。
#[derive(Debug, Deserialize)]
struct AlibabaOssPolicy {
    /// Temporary form destination.
    /// 临时表单目标地址。
    upload_host: String,
    /// Provider-owned object prefix.
    /// 供应商持有的对象前缀。
    upload_dir: String,
    /// Temporary OSS access-key identifier.
    /// 临时 OSS Access Key 标识。
    oss_access_key_id: String,
    /// Signed form signature.
    /// 已签名表单签名。
    signature: String,
    /// Signed form policy.
    /// 已签名表单策略。
    policy: String,
    /// Required object ACL.
    /// 必需对象 ACL。
    x_oss_object_acl: String,
    /// Required overwrite protection.
    /// 必需覆盖保护。
    x_oss_forbid_overwrite: String,
}

/// Validates one trusted host file against its current canonical filesystem identity.
/// 针对当前规范文件系统身份校验一个受信任宿主文件。
///
/// # Parameters
/// 参数
/// `descriptor` was inserted by the host after local-path validation.
/// `descriptor` 由宿主在本地路径校验后插入。
///
/// # Returns
/// 返回值
/// The canonical path when identity and byte length still match.
/// 当身份与字节长度仍匹配时返回规范路径。
async fn validate_host_file(descriptor: &AlibabaHostFileDescriptor) -> Result<PathBuf, String> {
    if descriptor.size_bytes == 0 || descriptor.size_bytes > 2 * 1024 * 1024 * 1024 {
        return Err(String::from(
            "Alibaba host file size is outside the verified range.",
        ));
    }
    let path = PathBuf::from(descriptor.absolute_path.as_str());
    if !path.is_absolute() || descriptor.mime_type.trim().is_empty() {
        return Err(String::from("Alibaba host file descriptor is invalid."));
    }
    let canonical = tokio::fs::canonicalize(&path)
        .await
        .map_err(|error| format!("Alibaba host file cannot be resolved: {error}"))?;
    if canonical != path {
        return Err(String::from(
            "Alibaba host file path changed after materialization.",
        ));
    }
    let metadata = tokio::fs::metadata(&canonical)
        .await
        .map_err(|error| format!("Alibaba host file metadata is unavailable: {error}"))?;
    if !metadata.is_file() || metadata.len() != descriptor.size_bytes {
        return Err(String::from(
            "Alibaba host file changed after materialization.",
        ));
    }
    Ok(canonical)
}

/// Uploads one verified local file through Alibaba's temporary OSS policy flow.
/// 通过阿里云临时 OSS 策略流程上传一个已验证本地文件。
///
/// # Parameters
/// 参数
/// The selected target, credential, and descriptor are frozen for this invocation.
/// 所选目标、凭据与描述符在当前调用期间保持冻结。
///
/// # Returns
/// 返回值
/// A temporary `oss://` provider object handle.
/// 返回临时 `oss://` 供应商对象句柄。
async fn upload_provider_object(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    descriptor: &AlibabaHostFileDescriptor,
) -> Result<String, String> {
    let cancellation = MediaUploadCancellation::new();
    upload_alibaba_object(
        http,
        entry,
        target.upstream_target_id.as_str(),
        secret,
        descriptor,
        None,
        &cancellation,
    )
    .await
}

/// Uploads one user attachment through the verified Alibaba temporary OSS workflow.
/// 通过已验证的阿里云临时 OSS 流程上传一个用户附件。
///
/// # Parameters
/// 参数
/// `http`, `entry`, and `secret` identify the exact configured Alibaba account.
/// `http`、`entry` 与 `secret` 标识精确配置的阿里云账号。
/// `model_id` selects the model-specific upload policy, while the remaining values describe the
/// validated local attachment and its progress/cancellation controls.
/// `model_id` 选择模型对应的上传策略，其余参数描述已校验的本地附件及进度/取消控制。
///
/// # Returns
/// 返回值
/// The temporary `oss://` reference consumed by Alibaba model requests.
/// 阿里云模型请求消费的临时 `oss://` 引用。
pub(crate) async fn upload_user_media(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    model_id: &str,
    secret: &CredentialExecutionSecret,
    local_path: &Path,
    mime_type: &str,
    size_bytes: u64,
    progress: Option<MediaUploadProgressCallback>,
    cancellation: &MediaUploadCancellation,
) -> Result<String, String> {
    let descriptor = AlibabaHostFileDescriptor {
        absolute_path: local_path.to_string_lossy().into_owned(),
        mime_type: mime_type.to_string(),
        size_bytes,
    };
    upload_alibaba_object(
        http,
        entry,
        model_id,
        secret,
        &descriptor,
        progress,
        cancellation,
    )
    .await
}

/// Uploads one verified local file through Alibaba's temporary OSS policy flow.
/// 通过阿里云临时 OSS 策略流程上传一个已校验的本地文件。
async fn upload_alibaba_object(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    model_id: &str,
    secret: &CredentialExecutionSecret,
    descriptor: &AlibabaHostFileDescriptor,
    progress: Option<MediaUploadProgressCallback>,
    cancellation: &MediaUploadCancellation,
) -> Result<String, String> {
    if !matches!(
        entry.id.as_str(),
        "model-studio-cn" | "token-plan-personal-cn"
    ) {
        return Err(format!(
            "Alibaba service entry `{}` has no verified temporary-object upload contract.",
            entry.id
        ));
    }
    let canonical = validate_host_file(descriptor).await?;
    let policy_url = format!(
        "https://dashscope.aliyuncs.com/api/v1/uploads?action=getPolicy&model={}",
        percent_encode_component(model_id)
    );
    let policy_response = tokio::select! {
        response = http
        .get(policy_url)
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .send() => response
            .map_err(|error| format!("Alibaba OSS policy request failed: {error}"))?,
        _ = cancellation.wait() => return Err(String::from("media upload cancelled")),
    };
    let policy_status = policy_response.status();
    let policy_text = tokio::select! {
        result = crate::provider_tool_execution::read_bounded_provider_response(
            policy_response,
            "Alibaba OSS policy",
        ) => result?,
        _ = cancellation.wait() => return Err(String::from("media upload cancelled")),
    };
    if cancellation.is_cancelled() {
        return Err(String::from("media upload cancelled"));
    }
    if !policy_status.is_success() {
        return Err(format!(
            "Alibaba OSS policy failed: HTTP {} | {}",
            policy_status.as_u16(),
            policy_text
        ));
    }
    let policy: AlibabaOssPolicyEnvelope = serde_json::from_str(policy_text.as_str())
        .map_err(|error| format!("Alibaba OSS policy JSON is invalid: {error}"))?;
    validate_oss_policy(&policy.data)?;
    let digest = hash_host_file(canonical.as_path(), cancellation).await?;
    let file_name = format!(
        "resource-{}{}",
        &digest[..16],
        upload_extension(descriptor.mime_type.as_str())
    );
    let object_key = format!(
        "{}/{}",
        policy.data.upload_dir.trim_end_matches('/'),
        file_name
    );
    if object_key.starts_with('/') || object_key.contains("../") {
        return Err(String::from(
            "Alibaba OSS policy returned an unsafe object directory.",
        ));
    }
    let file = tokio::fs::File::open(canonical)
        .await
        .map_err(|error| format!("Alibaba upload source cannot be opened: {error}"))?;
    let descriptor_size_bytes = descriptor.size_bytes;
    let upload_cancellation = cancellation.clone();
    let upload_progress = progress.clone();
    let stream = stream! {
        let mut source = ReaderStream::new(file);
        let mut sent = 0_u64;
        while let Some(chunk) = source.next().await {
            if upload_cancellation.is_cancelled() {
                yield Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "media upload cancelled",
                ));
                return;
            }
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield Err(error);
                    return;
                }
            };
            sent = sent.saturating_add(chunk.len() as u64);
            if let Some(callback) = upload_progress.as_ref() {
                callback(sent, Some(descriptor_size_bytes));
            }
            yield Ok(chunk);
        }
        if sent != descriptor_size_bytes {
            yield Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Alibaba upload source length changed during transfer",
            ));
        }
    };
    let part = reqwest::multipart::Part::stream_with_length(
        reqwest::Body::wrap_stream(stream),
        descriptor_size_bytes,
    )
    .file_name(file_name)
    .mime_str(descriptor.mime_type.as_str())
    .map_err(|error| format!("Alibaba upload MIME type is invalid: {error}"))?;
    let form = reqwest::multipart::Form::new()
        .text("OSSAccessKeyId", policy.data.oss_access_key_id)
        .text("Signature", policy.data.signature)
        .text("policy", policy.data.policy)
        .text("x-oss-object-acl", policy.data.x_oss_object_acl)
        .text("x-oss-forbid-overwrite", policy.data.x_oss_forbid_overwrite)
        .text("key", object_key.clone())
        .text("success_action_status", "200")
        .part("file", part);
    let upload_response = tokio::select! {
        response = http
        .post(policy.data.upload_host)
        .multipart(form)
        .send() => response
            .map_err(|error| {
                if cancellation.is_cancelled() {
                    String::from("media upload cancelled")
                } else {
                    format!("Alibaba OSS upload failed: {error}")
                }
            })?,
        _ = cancellation.wait() => return Err(String::from("media upload cancelled")),
    };
    if cancellation.is_cancelled() {
        return Err(String::from("media upload cancelled"));
    }
    let upload_status = upload_response.status();
    let upload_text = tokio::select! {
        result = crate::provider_tool_execution::read_bounded_provider_response(
            upload_response,
            "Alibaba OSS upload",
        ) => result?,
        _ = cancellation.wait() => return Err(String::from("media upload cancelled")),
    };
    if cancellation.is_cancelled() {
        return Err(String::from("media upload cancelled"));
    }
    if !upload_status.is_success() {
        return Err(format!(
            "Alibaba OSS upload failed: HTTP {} | {}",
            upload_status.as_u16(),
            upload_text
        ));
    }
    Ok(format!("oss://{object_key}"))
}

/// Hashes one already validated local upload source without buffering it in memory.
/// 在不把文件缓冲进内存的情况下哈希一个已校验本地上传来源。
///
/// # Parameters
/// 参数
/// `path` is the canonical source path.
/// `path` 是规范来源路径。
///
/// # Returns
/// 返回值
/// Lowercase SHA-256 hexadecimal text.
/// 返回小写 SHA-256 十六进制文本。
async fn hash_host_file(
    path: &Path,
    cancellation: &MediaUploadCancellation,
) -> Result<String, String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| format!("Alibaba upload source cannot be opened: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        let read = file
            .read(buffer.as_mut_slice())
            .await
            .map_err(|error| format!("Alibaba upload source cannot be hashed: {error}"))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Validates all security-sensitive Alibaba OSS policy fields.
/// 校验所有安全敏感的阿里云 OSS 策略字段。
///
/// # Parameters
/// 参数
/// `policy` is the decoded provider response.
/// `policy` 是解码后的供应商响应。
///
/// # Returns
/// 返回值
/// Success only for complete HTTPS upload policies.
/// 仅对完整 HTTPS 上传策略返回成功。
fn validate_oss_policy(policy: &AlibabaOssPolicy) -> Result<(), String> {
    if [
        policy.upload_dir.as_str(),
        policy.oss_access_key_id.as_str(),
        policy.signature.as_str(),
        policy.policy.as_str(),
        policy.x_oss_object_acl.as_str(),
        policy.x_oss_forbid_overwrite.as_str(),
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(String::from("Alibaba OSS policy fields are incomplete."));
    }
    let upload_url = reqwest::Url::parse(policy.upload_host.as_str())
        .map_err(|error| format!("Alibaba OSS upload URL is invalid: {error}"))?;
    if upload_url.scheme() != "https"
        || upload_url.host_str().is_none()
        || !upload_url.username().is_empty()
        || upload_url.password().is_some()
        || upload_url.query().is_some()
        || upload_url.fragment().is_some()
    {
        return Err(String::from(
            "Alibaba OSS upload URL must be an HTTPS origin.",
        ));
    }
    Ok(())
}

/// Percent-encodes one URI path or query component.
/// 百分号编码一个 URI 路径或查询组件。
///
/// # Parameters
/// 参数
/// `value` is one provider identifier.
/// `value` 是一个供应商标识。
///
/// # Returns
/// 返回值
/// An ASCII component using only unreserved literals.
/// 返回仅使用非保留字面字符的 ASCII 组件。
fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(format!("%{byte:02X}").as_str());
        }
    }
    encoded
}

/// Computes one stable lowercase SHA-256 fingerprint for sensitive provider locators.
/// 为敏感供应商定位符计算稳定的小写 SHA-256 指纹。
///
/// # Parameters
/// 参数
/// `value` is observed in memory and is never persisted by this helper.
/// `value` 仅在内存中观测，此辅助函数不会持久化它。
///
/// # Returns
/// 返回值
/// A fixed-width hexadecimal digest.
/// 返回固定宽度十六进制摘要。
fn sha256_text(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// Maps one exact media MIME type to a safe temporary object extension.
/// 将一个精确媒体 MIME 类型映射为安全临时对象扩展名。
///
/// # Parameters
/// 参数
/// `mime_type` is detected by the trusted host.
/// `mime_type` 由受信任宿主检测。
///
/// # Returns
/// 返回值
/// A non-user-controlled extension.
/// 返回不受用户控制的扩展名。
fn upload_extension(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" => ".jpg",
        "image/png" => ".png",
        "image/webp" => ".webp",
        "audio/mpeg" => ".mp3",
        "audio/wav" | "audio/x-wav" => ".wav",
        "audio/aac" => ".aac",
        "audio/flac" => ".flac",
        "video/mp4" => ".mp4",
        "video/quicktime" => ".mov",
        _ => ".bin",
    }
}

/// Exact host-materialized Qwen Omni media item.
/// 精确的宿主物化 Qwen Omni 媒体项。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaOmniMediaInput {
    /// Semantic media kind.
    /// 语义媒体类别。
    kind: String,
    /// Exact MIME type.
    /// 精确 MIME 类型。
    mime_type: String,
    /// Bounded inline Data URL.
    /// 有界内联 Data URL。
    data_url: String,
}

/// Exact host-materialized Qwen Omni tool arguments.
/// 精确的宿主物化 Qwen Omni 工具参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaOmniMediaArguments {
    /// Closed analysis task.
    /// 封闭分析任务。
    task: String,
    /// Ordered inline media.
    /// 有序内联媒体。
    media: Vec<AlibabaOmniMediaInput>,
    /// Optional task instruction.
    /// 可选任务指令。
    #[serde(default)]
    instruction: Option<String>,
}

/// Executes one verified Qwen3.5 Omni media-analysis stream.
/// 执行一条已验证的 Qwen3.5 Omni 媒体分析流。
///
/// # Parameters
/// 参数
/// Exact route, credential, and host-materialized arguments are immutable.
/// 精确路由、凭据与宿主物化参数保持不可变。
///
/// # Returns
/// 返回值
/// Complete normalized analysis text.
/// 返回完整归一化分析文本。
pub(crate) async fn execute_alibaba_qwen_omni_media_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: AlibabaOmniMediaArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba Qwen Omni arguments are invalid: {error}"))?;
    let instruction = omni_instruction(arguments.task.as_str(), arguments.instruction.as_deref())?;
    if arguments.media.is_empty() || arguments.media.len() > 16 {
        return Err(String::from(
            "Alibaba Qwen Omni requires one through sixteen media inputs.",
        ));
    }
    let mut content = vec![serde_json::json!({"type": "text", "text": instruction})];
    for media in arguments.media {
        if !media
            .data_url
            .starts_with(format!("data:{};base64,", media.mime_type).as_str())
        {
            return Err(String::from(
                "Alibaba Qwen Omni media carrier does not match its MIME type.",
            ));
        }
        match media.kind.as_str() {
            "image" if media.mime_type.starts_with("image/") => {
                content.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": {"url": media.data_url}
                }));
            }
            "audio" if media.mime_type.starts_with("audio/") => {
                let format = chat_audio_format(media.mime_type.as_str())?;
                let data = media
                    .data_url
                    .split_once(',')
                    .map(|(_, data)| data)
                    .ok_or_else(|| String::from("Alibaba Qwen Omni audio Data URL is invalid."))?;
                content.push(serde_json::json!({
                    "type": "input_audio",
                    "input_audio": {"data": data, "format": format}
                }));
            }
            _ => {
                return Err(String::from(
                    "Alibaba Qwen Omni media kind and MIME type do not match.",
                ));
            }
        }
    }
    let response = http
        .post(crate::provider_tool_execution::provider_tool_url(
            entry, target,
        ))
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": target.upstream_target_id,
            "stream": true,
            "stream_options": {"include_usage": true},
            "messages": [{"role": "user", "content": content}]
        }))
        .send()
        .await
        .map_err(|error| format!("Alibaba Qwen Omni request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = crate::provider_tool_execution::read_bounded_provider_response(
            response,
            "Alibaba Qwen Omni media analysis",
        )
        .await?;
        return Err(format!(
            "Alibaba Qwen Omni request failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let text = decode_chat_text_stream(response).await?;
    Ok(ProviderToolExecutionResult {
        upstream_response_id: None,
        output: ProviderToolNormalizedOutput::Text(text),
    })
}

/// Returns one frozen Router-owned media-analysis instruction.
/// 返回一条冻结的 Router 持有媒体分析指令。
///
/// # Parameters
/// 参数
/// `task` and optional `instruction` are exact semantic inputs.
/// `task` 与可选 `instruction` 是精确语义输入。
///
/// # Returns
/// 返回值
/// The task-specific prompt or an explicit validation error.
/// 返回任务专属提示词或明确校验错误。
fn omni_instruction(task: &str, instruction: Option<&str>) -> Result<String, String> {
    let instruction = instruction.unwrap_or_default().trim();
    match task {
        "describe" => Ok(String::from(
            "Describe the supplied media faithfully. Distinguish direct observations from uncertainty.",
        )),
        "summarize" => Ok(String::from(
            "Summarize the supplied media faithfully. Preserve important sequence, entities, speech, and events.",
        )),
        "question_answer" if !instruction.is_empty() => Ok(format!(
            "Answer this question using only evidence from the supplied media:\n{instruction}"
        )),
        "extract" if !instruction.is_empty() => Ok(format!(
            "Extract the requested information from the supplied media:\n{instruction}"
        )),
        "question_answer" | "extract" => Err(String::from(
            "Alibaba Qwen Omni question_answer and extract require an instruction.",
        )),
        _ => Err(String::from(
            "Alibaba Qwen Omni analysis task is unsupported.",
        )),
    }
}

/// Maps one verified audio MIME type to Alibaba Chat's format discriminator.
/// 将一个已验证音频 MIME 类型映射到阿里云 Chat 格式判别值。
///
/// # Parameters
/// 参数
/// `mime_type` comes from trusted host media detection.
/// `mime_type` 来自受信任宿主媒体检测。
///
/// # Returns
/// 返回值
/// A documented Alibaba Chat audio format.
/// 返回文档化阿里云 Chat 音频格式。
fn chat_audio_format(mime_type: &str) -> Result<&'static str, String> {
    match mime_type {
        "audio/mpeg" | "audio/mp3" => Ok("mp3"),
        "audio/wav" | "audio/x-wav" => Ok("wav"),
        "audio/amr" => Ok("amr"),
        "audio/aac" | "audio/mp4" | "audio/x-m4a" => Ok("aac"),
        "audio/ogg" => Ok("ogg"),
        "audio/3gpp" => Ok("3gpp"),
        _ => Err(format!(
            "Alibaba Qwen Omni audio MIME type `{mime_type}` is unsupported."
        )),
    }
}

/// Decodes one strict OpenAI Chat SSE text stream.
/// 解码一条严格 OpenAI Chat SSE 文本流。
///
/// # Parameters
/// 参数
/// `response` is a successful streaming HTTP response.
/// `response` 是成功的流式 HTTP 响应。
///
/// # Returns
/// 返回值
/// Complete visible text only after one `[DONE]` terminal record.
/// 仅在收到一个 `[DONE]` 终止记录后返回完整可见文本。
async fn decode_chat_text_stream(response: reqwest::Response) -> Result<String, String> {
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();
    let mut output = String::new();
    let mut terminal = false;
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("OpenAI Chat stream could not be read: {error}"))?;
        pending.extend_from_slice(chunk.as_ref());
        while let Some(position) = pending.iter().position(|byte| *byte == b'\n') {
            let mut line = pending.drain(..=position).collect::<Vec<_>>();
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            if line.is_empty() || line.starts_with(b":") {
                continue;
            }
            let Some(data) = line.strip_prefix(b"data:") else {
                continue;
            };
            let data = String::from_utf8(data.to_vec())
                .map_err(|_| String::from("OpenAI Chat stream data is not UTF-8."))?;
            let data = data.trim();
            if data == "[DONE]" {
                terminal = true;
                continue;
            }
            if terminal {
                return Err(String::from(
                    "OpenAI Chat emitted data after its terminal event.",
                ));
            }
            let value: Value = serde_json::from_str(data)
                .map_err(|error| format!("OpenAI Chat stream JSON is invalid: {error}"))?;
            append_chat_delta_text(&mut output, &value)?;
        }
    }
    if !pending.iter().all(u8::is_ascii_whitespace) {
        return Err(String::from(
            "OpenAI Chat stream ended with an incomplete SSE record.",
        ));
    }
    if !terminal {
        return Err(String::from(
            "OpenAI Chat stream ended before the [DONE] terminal record.",
        ));
    }
    if output.is_empty() {
        return Err(String::from(
            "Alibaba Qwen Omni returned no visible analysis text.",
        ));
    }
    Ok(output)
}

/// Appends visible text from one exact OpenAI Chat delta envelope.
/// 从一个精确 OpenAI Chat Delta 信封追加可见文本。
///
/// # Parameters
/// 参数
/// `output` accumulates visible text and `value` is one decoded SSE object.
/// `output` 累积可见文本，`value` 是一个已解码 SSE 对象。
///
/// # Returns
/// 返回值
/// Success for recognized empty or textual deltas.
/// 对已识别空 Delta 或文本 Delta 返回成功。
fn append_chat_delta_text(output: &mut String, value: &Value) -> Result<(), String> {
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("OpenAI Chat stream event omitted choices."))?;
    for choice in choices {
        let Some(content) = choice.get("delta").and_then(|delta| delta.get("content")) else {
            continue;
        };
        if let Some(text) = content.as_str() {
            output.push_str(text);
            continue;
        }
        if let Some(parts) = content.as_array() {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = part.get("text").and_then(Value::as_str)
                {
                    output.push_str(text);
                }
            }
            continue;
        }
        if !content.is_null() {
            return Err(String::from(
                "OpenAI Chat stream content has an unsupported shape.",
            ));
        }
    }
    Ok(())
}

/// Exact Fun-ASR host-materialized arguments.
/// 精确 Fun-ASR 宿主物化参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaFunAsrArguments {
    /// Ordered local source descriptors.
    /// 有序本地来源描述符。
    sources: Vec<AlibabaHostFileDescriptor>,
    /// Optional single language hint.
    /// 可选单语言提示。
    #[serde(default)]
    language: Option<String>,
    /// Ordered channel identifiers.
    /// 有序声道标识。
    #[serde(default)]
    channel_ids: Vec<u64>,
    /// Speaker diarization switch.
    /// 说话人分离开关。
    #[serde(default)]
    diarization: bool,
    /// Optional expected speaker count.
    /// 可选预期说话人数。
    #[serde(default)]
    speaker_count: Option<u64>,
    /// Optional provider-managed vocabulary ID.
    /// 可选供应商管理词表 ID。
    #[serde(default)]
    vocabulary_id: Option<String>,
}

/// Exact common Alibaba asynchronous task envelope.
/// 精确通用阿里云异步任务信封。
#[derive(Debug, Deserialize)]
struct AlibabaTaskEnvelope {
    /// Provider request identifier.
    /// 供应商请求标识。
    #[serde(default)]
    request_id: String,
    /// Task observation.
    /// 任务观测。
    output: AlibabaTaskOutput,
    /// Optional provider error code.
    /// 可选供应商错误码。
    #[serde(default)]
    code: String,
}

/// Exact Alibaba asynchronous cancellation confirmation.
/// 精确阿里云异步取消确认。
#[derive(Debug, Deserialize)]
struct AlibabaTaskCancellationEnvelope {
    /// Provider request identifier.
    /// 供应商请求标识。
    #[serde(default)]
    request_id: String,
    /// Optional provider error code.
    /// 可选供应商错误码。
    #[serde(default)]
    code: String,
}

/// Exact Alibaba asynchronous task output.
/// 精确阿里云异步任务输出。
#[derive(Debug, Deserialize)]
struct AlibabaTaskOutput {
    /// Private provider task identifier.
    /// 私有供应商任务标识。
    task_id: String,
    /// Documented uppercase task state.
    /// 文档化大写任务状态。
    task_status: String,
    /// Fun-ASR per-file results.
    /// Fun-ASR 逐文件结果。
    #[serde(default)]
    results: Vec<AlibabaTaskResult>,
    /// Direct HappyHorse video URL.
    /// HappyHorse 直接视频 URL。
    #[serde(default)]
    video_url: String,
}

/// Exact Alibaba asynchronous task result item.
/// 精确阿里云异步任务结果项。
#[derive(Debug, Deserialize)]
struct AlibabaTaskResult {
    /// Provider-echoed Fun-ASR input locator.
    /// 供应商回显 Fun-ASR 输入定位符。
    #[serde(default)]
    file_url: String,
    /// File-level terminal state.
    /// 文件级终态。
    #[serde(default)]
    subtask_status: String,
    /// Temporary transcript sidecar URL.
    /// 临时转录 Sidecar URL。
    #[serde(default)]
    transcription_url: String,
    /// HappyHorse fallback resource URL.
    /// HappyHorse 回退资源 URL。
    #[serde(default)]
    url: String,
}

/// Executes one strict Fun-ASR upload, task, poll, and sidecar sequence.
/// 执行一条严格 Fun-ASR 上传、任务、轮询与 Sidecar 序列。
///
/// # Parameters
/// 参数
/// Exact route, credential, and host files are frozen for this invocation.
/// 精确路由、凭据与宿主文件在当前调用期间保持冻结。
///
/// # Returns
/// 返回值
/// Ordered complete transcript text or an explicit whole-request failure.
/// 返回有序完整转录文本或明确整请求失败。
pub(crate) async fn execute_alibaba_fun_asr_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let started = start_alibaba_fun_asr_task(http, entry, target, arguments, secret).await?;
    let deadline = tokio::time::Instant::now() + ALIBABA_TASK_TIMEOUT;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(String::from("Alibaba Fun-ASR timed out after 30 minutes."));
        }
        tokio::time::sleep(ALIBABA_TASK_POLL_INTERVAL).await;
        match poll_alibaba_fun_asr_task(
            http,
            entry,
            secret,
            started.provider_task_id.as_str(),
            &started.recovery,
        )
        .await?
        {
            ProviderAsyncTaskPoll::Pending(_) => {}
            ProviderAsyncTaskPoll::Completed(result) => return Ok(result),
            ProviderAsyncTaskPoll::Failed(error) => return Err(error),
            ProviderAsyncTaskPoll::Cancelled => {
                return Err(String::from("Alibaba Fun-ASR task was cancelled."));
            }
        }
    }
}

/// Starts one Fun-ASR task and returns only non-secret recovery metadata.
/// 启动一个 Fun-ASR 任务并仅返回非秘密恢复元数据。
///
/// # Parameters
/// 参数
/// Exact route, credential, and materialized host arguments are frozen for dispatch.
/// 精确路由、凭据与已物化宿主参数在分派期间保持冻结。
///
/// # Returns
/// 返回值
/// A provider-confirmed task identity and ordered source fingerprints.
/// 返回供应商确认的任务身份与有序来源指纹。
pub(crate) async fn start_alibaba_fun_asr_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderAsyncTaskStart, String> {
    let arguments: AlibabaFunAsrArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba Fun-ASR arguments are invalid: {error}"))?;
    validate_fun_asr_arguments(&arguments)?;
    let mut source_urls = Vec::with_capacity(arguments.sources.len());
    for source in &arguments.sources {
        source_urls.push(upload_provider_object(http, entry, target, secret, source).await?);
    }
    let mut parameters = serde_json::Map::new();
    parameters.insert(
        String::from("channel_id"),
        serde_json::json!(if arguments.channel_ids.is_empty() {
            vec![0_u64]
        } else {
            arguments.channel_ids.clone()
        }),
    );
    if let Some(language) = arguments.language {
        parameters.insert(
            String::from("language_hints"),
            serde_json::json!([language]),
        );
    }
    if arguments.diarization {
        parameters.insert(String::from("diarization_enabled"), Value::Bool(true));
    }
    if let Some(speaker_count) = arguments.speaker_count {
        parameters.insert(
            String::from("speaker_count"),
            Value::Number(speaker_count.into()),
        );
    }
    if let Some(vocabulary_id) = arguments.vocabulary_id {
        parameters.insert(String::from("vocabulary_id"), Value::String(vocabulary_id));
    }
    let response = http
        .post(crate::provider_tool_execution::provider_tool_url(
            entry, target,
        ))
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .header("x-dashscope-async", "enable")
        .header("x-dashscope-ossresource-resolve", "enable")
        .json(&serde_json::json!({
            "model": target.upstream_target_id,
            "input": {"file_urls": source_urls},
            "parameters": Value::Object(parameters)
        }))
        .send()
        .await
        .map_err(|error| format!("Alibaba Fun-ASR start failed: {error}"))?;
    let status = response.status();
    let body = crate::provider_tool_execution::read_bounded_provider_response(
        response,
        "Alibaba Fun-ASR start",
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "Alibaba Fun-ASR start failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let start: AlibabaTaskEnvelope = serde_json::from_str(body.as_str())
        .map_err(|error| format!("Alibaba Fun-ASR start JSON is invalid: {error}"))?;
    validate_queued_task(&start, "Alibaba Fun-ASR")?;
    Ok(ProviderAsyncTaskStart {
        provider_task_id: start.output.task_id,
        recovery: ProviderAsyncTaskRecoveryContext::FunAsr {
            source_url_sha256: source_urls
                .iter()
                .map(|source_url| sha256_text(source_url))
                .collect(),
        },
        cancellation_supported: false,
    })
}

/// Polls one Fun-ASR task once and normalizes a provider-confirmed terminal result.
/// 单次轮询一个 Fun-ASR 任务并归一化供应商确认的终态结果。
///
/// # Parameters
/// 参数
/// Exact route, credential, task identity, and source fingerprints remain immutable.
/// 精确路由、凭据、任务身份与来源指纹保持不可变。
///
/// # Returns
/// 返回值
/// A pending phase or ordered complete transcript text.
/// 返回等待阶段或有序完整转录文本。
pub(crate) async fn poll_alibaba_fun_asr_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
    task_id: &str,
    recovery: &ProviderAsyncTaskRecoveryContext,
) -> Result<ProviderAsyncTaskPoll, String> {
    let source_url_sha256 = match recovery {
        ProviderAsyncTaskRecoveryContext::FunAsr { source_url_sha256 } => source_url_sha256,
        ProviderAsyncTaskRecoveryContext::None => {
            return Err(String::from(
                "Alibaba Fun-ASR recovery metadata is missing.",
            ));
        }
    };
    let terminal = match poll_task_once(http, entry, secret, task_id, "Alibaba Fun-ASR").await? {
        AlibabaTaskPollObservation::Pending(phase) => {
            return Ok(ProviderAsyncTaskPoll::Pending(phase));
        }
        AlibabaTaskPollObservation::Succeeded(terminal) => terminal,
        AlibabaTaskPollObservation::Failed(error) => {
            return Ok(ProviderAsyncTaskPoll::Failed(error));
        }
        AlibabaTaskPollObservation::Cancelled => {
            return Ok(ProviderAsyncTaskPoll::Cancelled);
        }
    };
    if terminal.output.results.len() != source_url_sha256.len() {
        return Err(String::from(
            "Alibaba Fun-ASR result count differs from the submitted source count.",
        ));
    }
    let mut by_fingerprint = BTreeMap::new();
    for result in terminal.output.results {
        if result.file_url.trim().is_empty()
            || by_fingerprint
                .insert(sha256_text(result.file_url.as_str()), result)
                .is_some()
        {
            return Err(String::from(
                "Alibaba Fun-ASR returned an invalid or duplicate source result.",
            ));
        }
    }
    let mut transcripts = Vec::with_capacity(source_url_sha256.len());
    for source_fingerprint in source_url_sha256 {
        let result = by_fingerprint.remove(source_fingerprint).ok_or_else(|| {
            String::from("Alibaba Fun-ASR returned an unknown or missing source result.")
        })?;
        if result.subtask_status != "SUCCEEDED" || result.transcription_url.trim().is_empty() {
            return Err(String::from(
                "Alibaba Fun-ASR reported a file-level transcription failure.",
            ));
        }
        transcripts.push(fetch_fun_asr_transcript(http, result.transcription_url.as_str()).await?);
    }
    Ok(ProviderAsyncTaskPoll::Completed(
        ProviderToolExecutionResult {
            upstream_response_id: Some(task_id.to_string()),
            output: ProviderToolNormalizedOutput::Text(transcripts.join("\n\n")),
        },
    ))
}

/// Validates Fun-ASR cross-field constraints before any upload starts.
/// 在任何上传开始前校验 Fun-ASR 跨字段约束。
///
/// # Parameters
/// 参数
/// `arguments` is the typed host-materialized request.
/// `arguments` 是强类型宿主物化请求。
///
/// # Returns
/// 返回值
/// Success only for the exact verified request subset.
/// 仅对精确已验证请求子集返回成功。
fn validate_fun_asr_arguments(arguments: &AlibabaFunAsrArguments) -> Result<(), String> {
    if arguments.sources.is_empty() || arguments.sources.len() > 100 {
        return Err(String::from(
            "Alibaba Fun-ASR requires one through one hundred sources.",
        ));
    }
    if arguments.speaker_count.is_some() && !arguments.diarization {
        return Err(String::from(
            "Alibaba Fun-ASR speaker_count requires diarization=true.",
        ));
    }
    let mut channels = BTreeSet::new();
    if arguments
        .channel_ids
        .iter()
        .any(|channel| !channels.insert(*channel))
    {
        return Err(String::from("Alibaba Fun-ASR channel_ids must be unique."));
    }
    Ok(())
}

/// Validates one provider-confirmed queued task identity.
/// 校验一个供应商确认的排队任务身份。
///
/// # Parameters
/// 参数
/// `task` is the decoded start envelope and `operation` scopes diagnostics.
/// `task` 是已解码启动信封，`operation` 限定诊断信息。
///
/// # Returns
/// 返回值
/// Success only for a non-empty exact `PENDING` task.
/// 仅对非空精确 `PENDING` 任务返回成功。
fn validate_queued_task(task: &AlibabaTaskEnvelope, operation: &str) -> Result<(), String> {
    if task.request_id.trim().is_empty()
        || !task.code.trim().is_empty()
        || task.output.task_status != "PENDING"
        || task.output.task_id.trim().is_empty()
        || task.output.task_id.trim() != task.output.task_id
    {
        return Err(format!(
            "{operation} start did not return one valid queued task."
        ));
    }
    Ok(())
}

/// One strongly typed Alibaba task observation.
/// 一次强类型阿里云任务观测。
enum AlibabaTaskPollObservation {
    /// A documented non-terminal provider phase.
    /// 文档明确的供应商未终结阶段。
    Pending(ProviderAsyncTaskPhase),
    /// A provider-confirmed successful terminal envelope.
    /// 供应商确认成功的终态信封。
    Succeeded(AlibabaTaskEnvelope),
    /// A provider-confirmed terminal failure.
    /// 供应商确认的终态失败。
    Failed(String),
    /// A provider-confirmed cancellation.
    /// 供应商确认的取消。
    Cancelled,
}

/// Polls one Alibaba task exactly once without retrying or changing route affinity.
/// 对一个阿里云任务精确轮询一次，不重试也不改变路由亲和性。
///
/// # Parameters
/// 参数
/// Route, credential, task identifier, and diagnostic label are immutable.
/// 路由、凭据、任务标识与诊断标签保持不可变。
///
/// # Returns
/// 返回值
/// One pending phase, a successful terminal envelope, or an explicit terminal failure.
/// 返回一个等待阶段、成功终态信封或明确终态失败。
async fn poll_task_once(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
    task_id: &str,
    operation: &str,
) -> Result<AlibabaTaskPollObservation, String> {
    let response = http
        .get(format!(
            "{}/api/v1/tasks/{}",
            entry.base_url.trim_end_matches('/'),
            percent_encode_component(task_id)
        ))
        .bearer_auth(secret.expose_value())
        .send()
        .await
        .map_err(|error| format!("{operation} poll failed: {error}"))?;
    let status = response.status();
    let body =
        crate::provider_tool_execution::read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format!(
            "{operation} poll failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let observation: AlibabaTaskEnvelope = serde_json::from_str(body.as_str())
        .map_err(|error| format!("{operation} poll JSON is invalid: {error}"))?;
    if observation.request_id.trim().is_empty() || observation.output.task_id != task_id {
        return Err(format!("{operation} task correlation failed."));
    }
    match observation.output.task_status.as_str() {
        "PENDING" => Ok(AlibabaTaskPollObservation::Pending(
            ProviderAsyncTaskPhase::Queued,
        )),
        "RUNNING" => Ok(AlibabaTaskPollObservation::Pending(
            ProviderAsyncTaskPhase::Running,
        )),
        "SUCCEEDED" if observation.code.trim().is_empty() => {
            Ok(AlibabaTaskPollObservation::Succeeded(observation))
        }
        "FAILED" | "UNKNOWN" => Ok(AlibabaTaskPollObservation::Failed(format!(
            "{operation} ended with provider state `{}`.",
            observation.output.task_status
        ))),
        "CANCELED" => Ok(AlibabaTaskPollObservation::Cancelled),
        state => Err(format!(
            "{operation} returned unknown task state `{state}`."
        )),
    }
}

/// Requests cancellation of one provider-confirmed Alibaba task.
/// 请求取消一个供应商已确认的阿里云任务。
///
/// # Parameters
/// 参数
/// The exact immutable route, credential, task identity, and operation label are required.
/// 需要精确不可变路由、凭据、任务身份与操作标签。
///
/// # Returns
/// 返回值
/// Success only after a valid provider confirmation response.
/// 仅在收到有效供应商确认响应后返回成功。
pub(crate) async fn cancel_alibaba_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
    task_id: &str,
    operation: &str,
) -> Result<(), String> {
    let response = http
        .post(format!(
            "{}/api/v1/tasks/{}/cancel",
            entry.base_url.trim_end_matches('/'),
            percent_encode_component(task_id)
        ))
        .bearer_auth(secret.expose_value())
        .send()
        .await
        .map_err(|error| format!("{operation} cancellation failed: {error}"))?;
    let status = response.status();
    let body =
        crate::provider_tool_execution::read_bounded_provider_response(response, operation).await?;
    if !status.is_success() {
        return Err(format!(
            "{operation} cancellation failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let confirmation: AlibabaTaskCancellationEnvelope = serde_json::from_str(body.as_str())
        .map_err(|error| format!("{operation} cancellation JSON is invalid: {error}"))?;
    if confirmation.request_id.trim().is_empty() || !confirmation.code.trim().is_empty() {
        return Err(format!(
            "{operation} cancellation response did not confirm the exact task."
        ));
    }
    Ok(())
}

/// Fetches and normalizes one Fun-ASR transcript sidecar.
/// 获取并归一化一个 Fun-ASR 转录 Sidecar。
///
/// # Parameters
/// 参数
/// `url` is the provider-returned temporary HTTPS document.
/// `url` 是供应商返回的临时 HTTPS 文档。
///
/// # Returns
/// 返回值
/// Complete ordered channel text.
/// 返回完整有序声道文本。
async fn fetch_fun_asr_transcript(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| format!("Alibaba Fun-ASR transcript URL is invalid: {error}"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(String::from(
            "Alibaba Fun-ASR transcript URL must be HTTPS without user information.",
        ));
    }
    let response = http
        .get(parsed)
        .send()
        .await
        .map_err(|error| format!("Alibaba Fun-ASR transcript fetch failed: {error}"))?;
    let status = response.status();
    let body = crate::provider_tool_execution::read_bounded_provider_response(
        response,
        "Alibaba Fun-ASR transcript",
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "Alibaba Fun-ASR transcript fetch failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let value: Value = serde_json::from_str(body.as_str())
        .map_err(|error| format!("Alibaba Fun-ASR transcript JSON is invalid: {error}"))?;
    let transcripts = value
        .get("transcripts")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Alibaba Fun-ASR transcript omitted transcripts."))?;
    let texts = transcripts
        .iter()
        .map(|transcript| {
            transcript
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    String::from("Alibaba Fun-ASR transcript contains an empty channel.")
                })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if texts.is_empty() {
        return Err(String::from(
            "Alibaba Fun-ASR transcript contains no channel text.",
        ));
    }
    Ok(texts.join("\n"))
}

/// Exact HappyHorse common arguments after host materialization.
/// 宿主物化后的精确 HappyHorse 通用参数。
#[derive(Debug, Deserialize)]
struct AlibabaHappyHorseCommonArguments {
    /// Optional negative prompt.
    /// 可选负向提示词。
    #[serde(default)]
    negative_prompt: String,
    /// Optional resolution tier.
    /// 可选分辨率档位。
    #[serde(default)]
    resolution: String,
    /// Optional aspect ratio.
    /// 可选长宽比。
    #[serde(default)]
    aspect_ratio: String,
    /// Optional whole duration seconds.
    /// 可选整数时长秒。
    #[serde(default)]
    duration_seconds: Option<u64>,
    /// Optional seed.
    /// 可选种子。
    #[serde(default)]
    seed: Option<u64>,
    /// Optional prompt-extension switch.
    /// 可选提示词扩展开关。
    #[serde(default)]
    prompt_extend: Option<bool>,
    /// Optional watermark switch.
    /// 可选水印开关。
    #[serde(default)]
    watermark: Option<bool>,
}

/// Exact HappyHorse materialized reference.
/// 精确 HappyHorse 物化参考。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaHappyHorseReference {
    /// Image or video kind.
    /// 图片或视频类别。
    kind: String,
    /// Inline image Data URL or trusted video descriptor.
    /// 内联图片 Data URL 或受信任视频描述符。
    carrier: Value,
    /// Optional trusted voice descriptor.
    /// 可选受信任语音描述符。
    #[serde(default)]
    voice: Option<AlibabaHostFileDescriptor>,
}

/// Executes one HappyHorse asynchronous generation or edit request.
/// 执行一个 HappyHorse 异步生成或编辑请求。
///
/// # Parameters
/// 参数
/// Exact route, credential, and model-specific arguments remain immutable.
/// 精确路由、凭据与模型专属参数保持不可变。
///
/// # Returns
/// 返回值
/// One normalized MP4 resource.
/// 返回一个归一化 MP4 资源。
pub(crate) async fn execute_alibaba_happyhorse_video_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let started =
        start_alibaba_happyhorse_video_task(http, entry, target, arguments, secret).await?;
    let deadline = tokio::time::Instant::now() + ALIBABA_TASK_TIMEOUT;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(String::from(
                "Alibaba HappyHorse timed out after 30 minutes.",
            ));
        }
        tokio::time::sleep(ALIBABA_TASK_POLL_INTERVAL).await;
        match poll_alibaba_video_task(
            http,
            entry,
            secret,
            started.provider_task_id.as_str(),
            "Alibaba HappyHorse",
        )
        .await?
        {
            ProviderAsyncTaskPoll::Pending(_) => {}
            ProviderAsyncTaskPoll::Completed(result) => return Ok(result),
            ProviderAsyncTaskPoll::Failed(error) => return Err(error),
            ProviderAsyncTaskPoll::Cancelled => {
                return Err(String::from("Alibaba HappyHorse task was cancelled."));
            }
        }
    }
}

/// Starts one HappyHorse task without consuming its asynchronous result.
/// 启动一个 HappyHorse 任务但不消费其异步结果。
///
/// # Parameters
/// 参数
/// Exact route, credential, model, and materialized arguments are immutable.
/// 精确路由、凭据、模型与已物化参数保持不可变。
///
/// # Returns
/// 返回值
/// A provider-confirmed task identity and cancellation capability.
/// 返回供应商确认的任务身份与取消能力。
pub(crate) async fn start_alibaba_happyhorse_video_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderAsyncTaskStart, String> {
    let (input, parameters, used_provider_object) = match target.execution.adapter {
        ProviderToolAdapterKind::AlibabaHappyHorseVideoGenerate => {
            build_happyhorse_generation(http, entry, target, secret, arguments).await?
        }
        ProviderToolAdapterKind::AlibabaHappyHorseVideoEdit => {
            build_happyhorse_edit(http, entry, target, secret, arguments).await?
        }
        _ => unreachable!("HappyHorse executor only receives HappyHorse adapters"),
    };
    let mut request = http
        .post(crate::provider_tool_execution::provider_tool_url(
            entry, target,
        ))
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .header("x-dashscope-async", "enable");
    if used_provider_object {
        request = request.header("x-dashscope-ossresource-resolve", "enable");
    }
    let response = request
        .json(&serde_json::json!({
            "model": target.upstream_target_id,
            "input": input,
            "parameters": parameters
        }))
        .send()
        .await
        .map_err(|error| format!("Alibaba HappyHorse start failed: {error}"))?;
    let status = response.status();
    let body = crate::provider_tool_execution::read_bounded_provider_response(
        response,
        "Alibaba HappyHorse start",
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "Alibaba HappyHorse start failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let start: AlibabaTaskEnvelope = serde_json::from_str(body.as_str())
        .map_err(|error| format!("Alibaba HappyHorse start JSON is invalid: {error}"))?;
    validate_queued_task(&start, "Alibaba HappyHorse")?;
    Ok(ProviderAsyncTaskStart {
        provider_task_id: start.output.task_id,
        recovery: ProviderAsyncTaskRecoveryContext::None,
        cancellation_supported: true,
    })
}

/// Polls one HappyHorse or Wan video task and normalizes a terminal MP4 resource.
/// 轮询一个 HappyHorse 或 Wan 视频任务并归一化终态 MP4 资源。
///
/// # Parameters
/// 参数
/// Exact route, credential, task identity, and diagnostic label remain immutable.
/// 精确路由、凭据、任务身份与诊断标签保持不可变。
///
/// # Returns
/// 返回值
/// A pending phase or one normalized MP4 resource.
/// 返回等待阶段或一个归一化 MP4 资源。
pub(crate) async fn poll_alibaba_video_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
    task_id: &str,
    operation: &str,
) -> Result<ProviderAsyncTaskPoll, String> {
    let terminal = match poll_task_once(http, entry, secret, task_id, operation).await? {
        AlibabaTaskPollObservation::Pending(phase) => {
            return Ok(ProviderAsyncTaskPoll::Pending(phase));
        }
        AlibabaTaskPollObservation::Succeeded(terminal) => terminal,
        AlibabaTaskPollObservation::Failed(error) => {
            return Ok(ProviderAsyncTaskPoll::Failed(error));
        }
        AlibabaTaskPollObservation::Cancelled => {
            return Ok(ProviderAsyncTaskPoll::Cancelled);
        }
    };
    let video_url = if terminal.output.video_url.trim().is_empty() {
        terminal
            .output
            .results
            .first()
            .map(|result| result.url.trim())
            .unwrap_or_default()
    } else {
        terminal.output.video_url.trim()
    };
    validate_resource_url(video_url, operation)?;
    Ok(ProviderAsyncTaskPoll::Completed(
        ProviderToolExecutionResult {
            upstream_response_id: Some(task_id.to_string()),
            output: ProviderToolNormalizedOutput::Resources(vec![ProviderToolGeneratedResource {
                output_id: String::from("video-0"),
                kind: String::from("video"),
                mime_type: String::from("video/mp4"),
                bytes: None,
                download_url: Some(video_url.to_string()),
            }]),
        },
    ))
}

/// Builds one HappyHorse generation input and parameter pair.
/// 构建一个 HappyHorse 生成输入与参数对。
///
/// # Parameters
/// 参数
/// Route, credential, and model-specific JSON are frozen.
/// 路由、凭据与模型专属 JSON 保持冻结。
///
/// # Returns
/// 返回值
/// Input, parameters, and whether provider-object resolution is required.
/// 返回输入、参数及是否需要供应商对象解析。
async fn build_happyhorse_generation(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    arguments: Value,
) -> Result<(Value, Value, bool), String> {
    let object = arguments
        .as_object()
        .ok_or_else(|| String::from("Alibaba HappyHorse arguments must be an object."))?;
    let common: AlibabaHappyHorseCommonArguments =
        serde_json::from_value(happyhorse_common_value(object))
            .map_err(|error| format!("Alibaba HappyHorse parameters are invalid: {error}"))?;
    validate_happyhorse_common(&common)?;
    let prompt = object
        .get("prompt")
        .and_then(Value::as_str)
        .filter(|prompt| !prompt.trim().is_empty())
        .ok_or_else(|| String::from("Alibaba HappyHorse prompt is required."))?;
    let mut input = serde_json::json!({
        "prompt": prompt,
        "negative_prompt": common.negative_prompt
    });
    let mut used_provider_object = false;
    match target.upstream_target_id.as_str() {
        "happyhorse-1.1-t2v" => {
            if object.contains_key("first_frame_image")
                || object.contains_key("materialized_references")
            {
                return Err(String::from("HappyHorse t2v does not accept media input."));
            }
        }
        "happyhorse-1.1-i2v" => {
            let image = object
                .get("first_frame_image")
                .and_then(Value::as_str)
                .filter(|image| image.starts_with("data:image/"))
                .ok_or_else(|| {
                    String::from("HappyHorse i2v requires one materialized first frame.")
                })?;
            input
                .as_object_mut()
                .expect("constructed input object")
                .insert(
                    String::from("media"),
                    serde_json::json!([{"type": "first_frame", "url": image}]),
                );
        }
        "happyhorse-1.1-r2v" => {
            let references: Vec<AlibabaHappyHorseReference> = serde_json::from_value(
                object
                    .get("materialized_references")
                    .cloned()
                    .ok_or_else(|| String::from("HappyHorse r2v requires references."))?,
            )
            .map_err(|error| format!("HappyHorse references are invalid: {error}"))?;
            if references.is_empty() || references.len() > 9 {
                return Err(String::from(
                    "HappyHorse r2v requires one through nine references.",
                ));
            }
            let mut media = Vec::with_capacity(references.len());
            for reference in references {
                let (media_type, url, uploaded) = match reference.kind.as_str() {
                    "image" => {
                        let image = reference
                            .carrier
                            .as_str()
                            .filter(|image| image.starts_with("data:image/"))
                            .ok_or_else(|| {
                                String::from("HappyHorse image reference is invalid.")
                            })?;
                        ("reference_image", image.to_string(), false)
                    }
                    "video" => {
                        let descriptor: AlibabaHostFileDescriptor =
                            serde_json::from_value(reference.carrier).map_err(|error| {
                                format!("HappyHorse video reference is invalid: {error}")
                            })?;
                        (
                            "reference_video",
                            upload_provider_object(http, entry, target, secret, &descriptor)
                                .await?,
                            true,
                        )
                    }
                    _ => {
                        return Err(String::from("HappyHorse reference kind is unsupported."));
                    }
                };
                used_provider_object |= uploaded;
                let mut item = serde_json::json!({"type": media_type, "url": url});
                if let Some(voice) = reference.voice {
                    let voice_url =
                        upload_provider_object(http, entry, target, secret, &voice).await?;
                    item.as_object_mut()
                        .expect("constructed reference object")
                        .insert(String::from("reference_voice"), Value::String(voice_url));
                    used_provider_object = true;
                }
                media.push(item);
            }
            input
                .as_object_mut()
                .expect("constructed input object")
                .insert(String::from("media"), Value::Array(media));
        }
        _ => return Err(String::from("HappyHorse generation model is unsupported.")),
    }
    Ok((
        input,
        happyhorse_parameters_value(&common, None),
        used_provider_object,
    ))
}

/// Builds one HappyHorse video-edit input and parameter pair.
/// 构建一个 HappyHorse 视频编辑输入与参数对。
///
/// # Parameters
/// 参数
/// Route, credential, and host-materialized JSON are frozen.
/// 路由、凭据与宿主物化 JSON 保持冻结。
///
/// # Returns
/// 返回值
/// Input, parameters, and provider-object resolution requirement.
/// 返回输入、参数及供应商对象解析要求。
async fn build_happyhorse_edit(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    arguments: Value,
) -> Result<(Value, Value, bool), String> {
    if target.upstream_target_id != "happyhorse-1.0-video-edit" {
        return Err(String::from("HappyHorse edit model is unsupported."));
    }
    let object = arguments
        .as_object()
        .ok_or_else(|| String::from("Alibaba HappyHorse edit arguments must be an object."))?;
    let common: AlibabaHappyHorseCommonArguments =
        serde_json::from_value(happyhorse_common_value(object))
            .map_err(|error| format!("Alibaba HappyHorse edit parameters are invalid: {error}"))?;
    validate_happyhorse_common(&common)?;
    if common
        .duration_seconds
        .is_some_and(|duration| !(2..=10).contains(&duration))
    {
        return Err(String::from(
            "HappyHorse video-edit duration must be from two through ten seconds.",
        ));
    }
    let source: AlibabaHostFileDescriptor = serde_json::from_value(
        object
            .get("source_video")
            .cloned()
            .ok_or_else(|| String::from("HappyHorse edit source video is required."))?,
    )
    .map_err(|error| format!("HappyHorse edit source is invalid: {error}"))?;
    let source_url = upload_provider_object(http, entry, target, secret, &source).await?;
    let mut media = vec![serde_json::json!({"type": "video", "url": source_url})];
    if let Some(references) = object.get("reference_images").and_then(Value::as_array) {
        if references.len() > 4 {
            return Err(String::from(
                "HappyHorse video edit accepts at most four image references.",
            ));
        }
        for reference in references {
            let reference = reference
                .as_str()
                .filter(|value| value.starts_with("data:image/"))
                .ok_or_else(|| String::from("HappyHorse edit image reference is invalid."))?;
            media.push(serde_json::json!({"type": "reference_image", "url": reference}));
        }
    }
    let audio_mode = object
        .get("audio_mode")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok((
        serde_json::json!({
            "prompt": object.get("instruction").and_then(Value::as_str).unwrap_or_default(),
            "negative_prompt": common.negative_prompt,
            "media": media
        }),
        happyhorse_parameters_value(&common, audio_mode.as_deref()),
        true,
    ))
}

/// Extracts only common HappyHorse fields for strict typed decoding.
/// 提取仅通用 HappyHorse 字段用于严格强类型解码。
///
/// # Parameters
/// 参数
/// `object` is the model-specific argument map.
/// `object` 是模型专属参数映射。
///
/// # Returns
/// 返回值
/// A JSON object containing no model-specific media fields.
/// 返回不含模型专属媒体字段的 JSON 对象。
fn happyhorse_common_value(object: &serde_json::Map<String, Value>) -> Value {
    let mut common = serde_json::Map::new();
    for key in [
        "negative_prompt",
        "resolution",
        "aspect_ratio",
        "duration_seconds",
        "seed",
        "prompt_extend",
        "watermark",
    ] {
        if let Some(value) = object.get(key) {
            common.insert(String::from(key), value.clone());
        }
    }
    Value::Object(common)
}

/// Validates common HappyHorse parameter enumerations before dispatch.
/// 在分派前校验通用 HappyHorse 参数枚举。
///
/// # Parameters
/// 参数
/// `common` is the typed common parameter set.
/// `common` 是强类型通用参数集合。
///
/// # Returns
/// 返回值
/// Success only for the verified parameter subset.
/// 仅对已验证参数子集返回成功。
fn validate_happyhorse_common(common: &AlibabaHappyHorseCommonArguments) -> Result<(), String> {
    if !common.resolution.is_empty() && !matches!(common.resolution.as_str(), "720P" | "1080P") {
        return Err(String::from("HappyHorse resolution must be 720P or 1080P."));
    }
    if !common.aspect_ratio.is_empty()
        && !matches!(
            common.aspect_ratio.as_str(),
            "16:9" | "9:16" | "1:1" | "4:3" | "3:4"
        )
    {
        return Err(String::from("HappyHorse aspect ratio is unsupported."));
    }
    if common.duration_seconds == Some(0) {
        return Err(String::from(
            "HappyHorse duration must be a positive whole number of seconds.",
        ));
    }
    Ok(())
}

/// Projects verified HappyHorse controls into the provider parameter object.
/// 将已验证 HappyHorse 控制项投影为供应商参数对象。
///
/// # Parameters
/// 参数
/// `common` holds shared controls and `audio_mode` applies only to editing.
/// `common` 持有共享控制项，`audio_mode` 仅适用于编辑。
///
/// # Returns
/// 返回值
/// A provider-ready parameters object without null fields.
/// 返回不含 Null 字段的供应商就绪参数对象。
fn happyhorse_parameters_value(
    common: &AlibabaHappyHorseCommonArguments,
    audio_mode: Option<&str>,
) -> Value {
    let mut parameters = serde_json::Map::new();
    if !common.resolution.is_empty() {
        parameters.insert(
            String::from("resolution"),
            Value::String(common.resolution.clone()),
        );
    }
    if !common.aspect_ratio.is_empty() {
        parameters.insert(
            String::from("ratio"),
            Value::String(common.aspect_ratio.clone()),
        );
    }
    if let Some(duration) = common.duration_seconds {
        parameters.insert(String::from("duration"), Value::Number(duration.into()));
    }
    if let Some(seed) = common.seed {
        parameters.insert(String::from("seed"), Value::Number(seed.into()));
    }
    if let Some(prompt_extend) = common.prompt_extend {
        parameters.insert(String::from("prompt_extend"), Value::Bool(prompt_extend));
    }
    if let Some(watermark) = common.watermark {
        parameters.insert(String::from("watermark"), Value::Bool(watermark));
    }
    if let Some(audio_mode) = audio_mode {
        parameters.insert(
            String::from("audio_setting"),
            Value::String(audio_mode.to_string()),
        );
    }
    Value::Object(parameters)
}

/// Exact host-materialized Wan 2.7 video arguments.
/// 精确的宿主物化 Wan 2.7 视频参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaWanVideoArguments {
    /// Optional generation prompt.
    /// 可选生成提示词。
    #[serde(default)]
    prompt: String,
    /// Optional negative prompt.
    /// 可选负向提示词。
    #[serde(default)]
    negative_prompt: String,
    /// Optional first-frame image Data URL.
    /// 可选首帧图片 Data URL。
    #[serde(default)]
    first_frame: String,
    /// Optional last-frame image Data URL.
    /// 可选尾帧图片 Data URL。
    #[serde(default)]
    last_frame: String,
    /// Optional trusted local audio upload descriptor.
    /// 可选受信任本地音频上传描述符。
    #[serde(default)]
    audio_source: Option<AlibabaHostFileDescriptor>,
    /// Optional output resolution.
    /// 可选输出分辨率。
    #[serde(default)]
    resolution: String,
    /// Optional text-to-video aspect ratio.
    /// 可选文生视频长宽比。
    #[serde(default)]
    aspect_ratio: String,
    /// Optional whole output duration.
    /// 可选完整输出时长。
    #[serde(default)]
    duration_seconds: Option<u64>,
    /// Optional provider-native prompt rewriting switch.
    /// 可选供应商原生提示词改写开关。
    #[serde(default)]
    prompt_extend: Option<bool>,
    /// Optional AIGC watermark switch.
    /// 可选 AIGC 水印开关。
    #[serde(default)]
    watermark: Option<bool>,
    /// Optional deterministic provider seed.
    /// 可选供应商确定性种子。
    #[serde(default)]
    seed: Option<u64>,
}

/// Executes one strict Wan 2.7 asynchronous video lifecycle.
/// 执行一条严格的 Wan 2.7 异步视频生命周期。
///
/// # Parameters
/// 参数
/// Exact route, credential, and model-specific arguments remain immutable.
/// 精确路由、凭据与模型专属参数保持不可变。
///
/// # Returns
/// 返回值
/// One normalized MP4 resource after provider-confirmed completion.
/// 在供应商确认完成后返回一个归一化 MP4 资源。
pub(crate) async fn execute_alibaba_wan_video_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let started = start_alibaba_wan_video_task(http, entry, target, arguments, secret).await?;
    let deadline = tokio::time::Instant::now() + ALIBABA_TASK_TIMEOUT;
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(String::from(
                "Alibaba Wan video timed out after 30 minutes.",
            ));
        }
        tokio::time::sleep(ALIBABA_TASK_POLL_INTERVAL).await;
        match poll_alibaba_video_task(
            http,
            entry,
            secret,
            started.provider_task_id.as_str(),
            "Alibaba Wan video",
        )
        .await?
        {
            ProviderAsyncTaskPoll::Pending(_) => {}
            ProviderAsyncTaskPoll::Completed(result) => return Ok(result),
            ProviderAsyncTaskPoll::Failed(error) => return Err(error),
            ProviderAsyncTaskPoll::Cancelled => {
                return Err(String::from("Alibaba Wan video task was cancelled."));
            }
        }
    }
}

/// Starts one Wan video task without consuming its asynchronous result.
/// 启动一个 Wan 视频任务但不消费其异步结果。
///
/// # Parameters
/// 参数
/// Exact route, credential, model, and host-materialized arguments remain immutable.
/// 精确路由、凭据、模型与宿主物化参数保持不可变。
///
/// # Returns
/// 返回值
/// A provider-confirmed task identity and cancellation capability.
/// 返回供应商确认的任务身份与取消能力。
pub(crate) async fn start_alibaba_wan_video_task(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderAsyncTaskStart, String> {
    let arguments: AlibabaWanVideoArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba Wan video arguments are invalid: {error}"))?;
    let (input, parameters, used_provider_object) =
        build_wan_video_request(http, entry, target, secret, arguments).await?;
    let mut request = http
        .post(crate::provider_tool_execution::provider_tool_url(
            entry, target,
        ))
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .header("x-dashscope-async", "enable");
    if used_provider_object {
        request = request.header("x-dashscope-ossresource-resolve", "enable");
    }
    let response = request
        .json(&serde_json::json!({
            "model": target.upstream_target_id,
            "input": input,
            "parameters": parameters
        }))
        .send()
        .await
        .map_err(|error| format!("Alibaba Wan video start failed: {error}"))?;
    let status = response.status();
    let body = crate::provider_tool_execution::read_bounded_provider_response(
        response,
        "Alibaba Wan video start",
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "Alibaba Wan video start failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let start: AlibabaTaskEnvelope = serde_json::from_str(body.as_str())
        .map_err(|error| format!("Alibaba Wan video start JSON is invalid: {error}"))?;
    validate_queued_task(&start, "Alibaba Wan video")?;
    Ok(ProviderAsyncTaskStart {
        provider_task_id: start.output.task_id,
        recovery: ProviderAsyncTaskRecoveryContext::None,
        cancellation_supported: true,
    })
}

/// Builds one exact Wan 2.7 request body from host-materialized input.
/// 从宿主物化输入构建一个精确的 Wan 2.7 请求体。
///
/// # Parameters
/// 参数
/// The frozen target selects text-to-video or image-to-video and the credential
/// authorizes optional temporary audio upload.
/// 冻结目标选择文生视频或图生视频，凭据授权可选临时音频上传。
///
/// # Returns
/// 返回值
/// Provider input, parameters, and whether object resolution is required.
/// 返回供应商输入、参数以及是否需要对象解析。
async fn build_wan_video_request(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    secret: &CredentialExecutionSecret,
    arguments: AlibabaWanVideoArguments,
) -> Result<(Value, Value, bool), String> {
    validate_wan_video_arguments(target.upstream_target_id.as_str(), &arguments)?;
    let mut input = serde_json::Map::new();
    if !arguments.prompt.trim().is_empty() {
        input.insert(String::from("prompt"), Value::String(arguments.prompt));
    }
    if !arguments.negative_prompt.trim().is_empty() {
        input.insert(
            String::from("negative_prompt"),
            Value::String(arguments.negative_prompt),
        );
    }
    let mut used_provider_object = false;
    match target.upstream_target_id.as_str() {
        "wan2.7-t2v" => {
            if let Some(audio_source) = arguments.audio_source.as_ref() {
                let audio_url =
                    upload_provider_object(http, entry, target, secret, audio_source).await?;
                input.insert(String::from("audio_url"), Value::String(audio_url));
                used_provider_object = true;
            }
        }
        "wan2.7-i2v" => {
            let mut media = vec![serde_json::json!({
                "type": "first_frame",
                "url": arguments.first_frame
            })];
            if !arguments.last_frame.is_empty() {
                media.push(serde_json::json!({
                    "type": "last_frame",
                    "url": arguments.last_frame
                }));
            }
            if let Some(audio_source) = arguments.audio_source.as_ref() {
                let audio_url =
                    upload_provider_object(http, entry, target, secret, audio_source).await?;
                media.push(serde_json::json!({
                    "type": "driving_audio",
                    "url": audio_url
                }));
                used_provider_object = true;
            }
            input.insert(String::from("media"), Value::Array(media));
        }
        _ => return Err(String::from("Alibaba Wan video model is unsupported.")),
    }
    let mut parameters = serde_json::Map::new();
    if !arguments.resolution.is_empty() {
        parameters.insert(
            String::from("resolution"),
            Value::String(arguments.resolution),
        );
    }
    if !arguments.aspect_ratio.is_empty() {
        parameters.insert(String::from("ratio"), Value::String(arguments.aspect_ratio));
    }
    if let Some(duration) = arguments.duration_seconds {
        parameters.insert(String::from("duration"), Value::Number(duration.into()));
    }
    if let Some(prompt_extend) = arguments.prompt_extend {
        parameters.insert(String::from("prompt_extend"), Value::Bool(prompt_extend));
    }
    if let Some(watermark) = arguments.watermark {
        parameters.insert(String::from("watermark"), Value::Bool(watermark));
    }
    if let Some(seed) = arguments.seed {
        parameters.insert(String::from("seed"), Value::Number(seed.into()));
    }
    Ok((
        Value::Object(input),
        Value::Object(parameters),
        used_provider_object,
    ))
}

/// Validates one model-specific Wan 2.7 request before any paid dispatch.
/// 在任何计费分派前校验一个模型专属 Wan 2.7 请求。
///
/// # Parameters
/// 参数
/// `model_id` fixes request semantics and `arguments` contains typed host input.
/// `model_id` 固定请求语义，`arguments` 包含强类型宿主输入。
///
/// # Returns
/// 返回值
/// Success only for the reviewed closed parameter and media combinations.
/// 仅对已审核的封闭参数与媒体组合返回成功。
fn validate_wan_video_arguments(
    model_id: &str,
    arguments: &AlibabaWanVideoArguments,
) -> Result<(), String> {
    if !arguments.resolution.is_empty()
        && !matches!(arguments.resolution.as_str(), "720P" | "1080P")
    {
        return Err(String::from(
            "Alibaba Wan video resolution must be 720P or 1080P.",
        ));
    }
    if arguments
        .duration_seconds
        .is_some_and(|duration| !(2..=15).contains(&duration))
    {
        return Err(String::from(
            "Alibaba Wan video duration must be from two through fifteen seconds.",
        ));
    }
    if arguments.seed.is_some_and(|seed| seed > 2_147_483_647) {
        return Err(String::from(
            "Alibaba Wan video seed exceeds the supported range.",
        ));
    }
    match model_id {
        "wan2.7-t2v" => {
            if arguments.prompt.trim().is_empty() {
                return Err(String::from("Alibaba Wan text-to-video requires a prompt."));
            }
            if !arguments.first_frame.is_empty() || !arguments.last_frame.is_empty() {
                return Err(String::from(
                    "Alibaba Wan text-to-video does not accept frame input.",
                ));
            }
            if !arguments.aspect_ratio.is_empty()
                && !matches!(
                    arguments.aspect_ratio.as_str(),
                    "16:9" | "9:16" | "1:1" | "4:3" | "3:4"
                )
            {
                return Err(String::from(
                    "Alibaba Wan text-to-video aspect ratio is unsupported.",
                ));
            }
        }
        "wan2.7-i2v" => {
            if !arguments.first_frame.starts_with("data:image/") {
                return Err(String::from(
                    "Alibaba Wan image-to-video requires a materialized first frame.",
                ));
            }
            if !arguments.last_frame.is_empty() && !arguments.last_frame.starts_with("data:image/")
            {
                return Err(String::from(
                    "Alibaba Wan image-to-video last frame is invalid.",
                ));
            }
            if !arguments.aspect_ratio.is_empty() {
                return Err(String::from(
                    "Alibaba Wan image-to-video does not accept an aspect ratio.",
                ));
            }
        }
        _ => return Err(String::from("Alibaba Wan video model is unsupported.")),
    }
    Ok(())
}

/// Exact non-streaming Fun-Music request arguments.
/// 精确的非流式 Fun-Music 请求参数。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlibabaFunMusicArguments {
    /// Optional style and scene prompt.
    /// 可选风格与场景提示词。
    #[serde(default)]
    prompt: String,
    /// Optional exact lyrics.
    /// 可选精确歌词。
    #[serde(default)]
    lyrics: String,
    /// Instrumental-only switch.
    /// 纯音乐开关。
    #[serde(default)]
    is_instrumental: bool,
    /// Optional V1 vocal gender.
    /// 可选 V1 人声性别。
    #[serde(default)]
    gender: String,
    /// Exact output audio format.
    /// 精确输出音频格式。
    #[serde(rename = "format", default = "default_fun_music_format")]
    output_format: String,
    /// AIGC audio watermark switch.
    /// AIGC 音频水印开关。
    #[serde(default)]
    enable_aigc_watermark: bool,
}

/// Exact non-streaming Fun-Music response envelope.
/// 精确的非流式 Fun-Music 响应信封。
#[derive(Debug, Deserialize)]
struct AlibabaFunMusicResponse {
    /// Provider request identifier.
    /// 供应商请求标识。
    request_id: String,
    /// Terminal output.
    /// 终态输出。
    output: AlibabaFunMusicOutput,
}

/// Exact terminal Fun-Music output.
/// 精确的终态 Fun-Music 输出。
#[derive(Debug, Deserialize)]
struct AlibabaFunMusicOutput {
    /// Complete audio resource.
    /// 完整音频资源。
    audio: AlibabaFunMusicAudio,
    /// Documented terminal reason.
    /// 文档化终止原因。
    finish_reason: String,
}

/// Exact complete Fun-Music audio resource.
/// 精确的完整 Fun-Music 音频资源。
#[derive(Debug, Deserialize)]
struct AlibabaFunMusicAudio {
    /// Provider-owned audio identifier.
    /// 供应商持有音频标识。
    id: String,
    /// Temporary complete audio URL.
    /// 临时完整音频地址。
    url: String,
}

/// Executes one strict non-streaming Fun-Music request.
/// 执行一个严格的非流式 Fun-Music 请求。
///
/// # Parameters
/// 参数
/// Exact Beijing route, credential, model, and model-specific arguments are frozen.
/// 精确北京路由、凭据、模型与模型专属参数保持冻结。
///
/// # Returns
/// 返回值
/// One normalized MP3 or WAV resource.
/// 返回一个归一化 MP3 或 WAV 资源。
pub(crate) async fn execute_alibaba_fun_music_tool(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    target: &ProviderToolTarget,
    arguments: Value,
    secret: &CredentialExecutionSecret,
) -> Result<ProviderToolExecutionResult, String> {
    let arguments: AlibabaFunMusicArguments = serde_json::from_value(arguments)
        .map_err(|error| format!("Alibaba Fun-Music arguments are invalid: {error}"))?;
    validate_fun_music_arguments(target.upstream_target_id.as_str(), &arguments)?;
    let mut input = serde_json::Map::new();
    if !arguments.prompt.is_empty() {
        input.insert(String::from("prompt"), Value::String(arguments.prompt));
    }
    if !arguments.lyrics.is_empty() {
        input.insert(String::from("lyrics"), Value::String(arguments.lyrics));
    }
    if arguments.is_instrumental {
        input.insert(String::from("is_instrumental"), Value::Bool(true));
    }
    if !arguments.gender.is_empty() {
        input.insert(String::from("gender"), Value::String(arguments.gender));
    }
    input.insert(
        String::from("format"),
        Value::String(arguments.output_format.clone()),
    );
    if arguments.enable_aigc_watermark {
        input.insert(String::from("enable_aigc_watermark"), Value::Bool(true));
    }
    let response = http
        .post(crate::provider_tool_execution::provider_tool_url(
            entry, target,
        ))
        .bearer_auth(secret.expose_value())
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": target.upstream_target_id,
            "input": Value::Object(input)
        }))
        .send()
        .await
        .map_err(|error| format!("Alibaba Fun-Music request failed: {error}"))?;
    let status = response.status();
    let body = crate::provider_tool_execution::read_bounded_provider_response(
        response,
        "Alibaba Fun-Music",
    )
    .await?;
    if !status.is_success() {
        return Err(format!(
            "Alibaba Fun-Music failed: HTTP {} | {}",
            status.as_u16(),
            body
        ));
    }
    let result: AlibabaFunMusicResponse = serde_json::from_str(body.as_str())
        .map_err(|error| format!("Alibaba Fun-Music JSON is invalid: {error}"))?;
    if result.request_id.trim().is_empty()
        || result.output.finish_reason != "stop"
        || result.output.audio.id.trim().is_empty()
    {
        return Err(String::from(
            "Alibaba Fun-Music response omitted its confirmed terminal audio.",
        ));
    }
    let audio_url = crate::provider_tool_execution::validated_public_http_url(
        result.output.audio.url.as_str(),
        "Alibaba Fun-Music audio URL",
    )?;
    let mime_type = match arguments.output_format.as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => unreachable!("Fun-Music output format was validated"),
    };
    Ok(ProviderToolExecutionResult {
        upstream_response_id: Some(result.request_id),
        output: ProviderToolNormalizedOutput::Resources(vec![ProviderToolGeneratedResource {
            output_id: String::from("audio-0"),
            kind: String::from("audio"),
            mime_type: String::from(mime_type),
            bytes: None,
            download_url: Some(audio_url.to_string()),
        }]),
    })
}

/// Returns the official Fun-Music default output format.
/// 返回官方 Fun-Music 默认输出格式。
///
/// # Returns
/// 返回值
/// The documented `mp3` default.
/// 返回文档明确的 `mp3` 默认值。
fn default_fun_music_format() -> String {
    String::from("mp3")
}

/// Validates one model-specific Fun-Music request before dispatch.
/// 在分派前校验一个模型专属 Fun-Music 请求。
///
/// # Parameters
/// 参数
/// `model_id` fixes conditional fields and `arguments` contains typed input.
/// `model_id` 固定条件字段，`arguments` 包含强类型输入。
///
/// # Returns
/// 返回值
/// Success only for the documented non-streaming subset.
/// 仅对文档明确的非流式子集返回成功。
fn validate_fun_music_arguments(
    model_id: &str,
    arguments: &AlibabaFunMusicArguments,
) -> Result<(), String> {
    if !matches!(arguments.output_format.as_str(), "mp3" | "wav") {
        return Err(String::from("Alibaba Fun-Music format must be mp3 or wav."));
    }
    if arguments.prompt.chars().count() > 2_000 || arguments.lyrics.chars().count() > 2_000 {
        return Err(String::from(
            "Alibaba Fun-Music prompt or lyrics exceeds the non-streaming limit.",
        ));
    }
    match model_id {
        "fun-music-v1" => {
            if arguments.prompt.is_empty() && arguments.lyrics.is_empty() {
                return Err(String::from(
                    "Alibaba Fun-Music V1 requires prompt or lyrics.",
                ));
            }
            if !arguments.gender.is_empty()
                && !matches!(arguments.gender.as_str(), "male" | "female")
            {
                return Err(String::from(
                    "Alibaba Fun-Music V1 gender must be male or female.",
                ));
            }
        }
        "fun-music-preview" => {
            if arguments.prompt.is_empty() {
                return Err(String::from("Alibaba Fun-Music Preview requires a prompt."));
            }
            if !arguments.gender.is_empty() {
                return Err(String::from(
                    "Alibaba Fun-Music Preview does not support gender.",
                ));
            }
        }
        _ => return Err(String::from("Alibaba Fun-Music model is unsupported.")),
    }
    Ok(())
}

/// Validates one provider-returned resource URL.
/// 校验一个供应商返回的资源 URL。
///
/// # Parameters
/// 参数
/// `url` is the candidate and `operation` scopes diagnostics.
/// `url` 是候选地址，`operation` 限定诊断信息。
///
/// # Returns
/// 返回值
/// Success only for an absolute HTTPS URL without embedded credentials.
/// 仅对不含嵌入凭据的绝对 HTTPS 地址返回成功。
fn validate_resource_url(url: &str, operation: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| format!("{operation} resource URL is invalid: {error}"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(format!(
            "{operation} resource URL must be absolute HTTPS without user information."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AlibabaFunMusicArguments, AlibabaHappyHorseCommonArguments, AlibabaWanVideoArguments,
        append_chat_delta_text, omni_instruction, percent_encode_component,
        validate_fun_asr_arguments, validate_fun_music_arguments, validate_happyhorse_common,
        validate_wan_video_arguments,
    };
    use serde_json::json;

    /// Verifies task prompts remain frozen and moderation is not inferred.
    /// 验证任务提示词保持冻结且不会推断审核语义。
    #[test]
    fn validates_frozen_omni_instructions() {
        assert_eq!(
            omni_instruction("question_answer", Some("What happened?"))
                .expect("question instruction"),
            "Answer this question using only evidence from the supplied media:\nWhat happened?"
        );
        assert!(omni_instruction("moderate", None).is_err());
    }

    /// Verifies visible text arrays and strings share the strict SSE accumulator.
    /// 验证可见文本数组与字符串共享严格 SSE 累积器。
    #[test]
    fn appends_chat_delta_text_shapes() {
        let mut output = String::new();
        append_chat_delta_text(
            &mut output,
            &json!({"choices": [{"delta": {"content": "first"}}]}),
        )
        .expect("string delta");
        append_chat_delta_text(
            &mut output,
            &json!({"choices": [{"delta": {"content": [{"type": "text", "text": " second"}]}}]}),
        )
        .expect("part delta");
        assert_eq!(output, "first second");
    }

    /// Verifies URI component encoding cannot inject query separators.
    /// 验证 URI 组件编码无法注入查询分隔符。
    #[test]
    fn encodes_provider_identifiers() {
        assert_eq!(percent_encode_component("a/b?c"), "a%2Fb%3Fc");
    }

    /// Verifies HappyHorse common parameter validation is closed.
    /// 验证 HappyHorse 通用参数校验保持封闭。
    #[test]
    fn validates_happyhorse_parameter_enums() {
        let valid = AlibabaHappyHorseCommonArguments {
            negative_prompt: String::new(),
            resolution: String::from("1080P"),
            aspect_ratio: String::from("16:9"),
            duration_seconds: Some(5),
            seed: Some(0),
            prompt_extend: Some(true),
            watermark: Some(false),
        };
        assert!(validate_happyhorse_common(&valid).is_ok());
        let invalid = AlibabaHappyHorseCommonArguments {
            aspect_ratio: String::from("2:1"),
            ..valid
        };
        assert!(validate_happyhorse_common(&invalid).is_err());
    }

    /// Keeps the Fun-ASR validator referenced by unit coverage.
    /// 通过单元覆盖保持 Fun-ASR 校验器被引用。
    #[test]
    fn fun_asr_rejects_empty_sources() {
        let arguments = super::AlibabaFunAsrArguments {
            sources: Vec::new(),
            language: None,
            channel_ids: Vec::new(),
            diarization: false,
            speaker_count: None,
            vocabulary_id: None,
        };
        assert!(validate_fun_asr_arguments(&arguments).is_err());
    }

    /// Verifies each Wan 2.7 carrier rejects fields owned by the other model.
    /// 验证每个 Wan 2.7 载体都会拒绝属于另一模型的字段。
    #[test]
    fn validates_wan_video_model_boundaries() {
        let text_to_video = AlibabaWanVideoArguments {
            prompt: String::from("Neon city"),
            negative_prompt: String::new(),
            first_frame: String::new(),
            last_frame: String::new(),
            audio_source: None,
            resolution: String::from("1080P"),
            aspect_ratio: String::from("16:9"),
            duration_seconds: Some(10),
            prompt_extend: Some(true),
            watermark: Some(false),
            seed: Some(7),
        };
        assert!(validate_wan_video_arguments("wan2.7-t2v", &text_to_video).is_ok());
        let invalid_image_to_video = AlibabaWanVideoArguments {
            first_frame: String::from("data:image/png;base64,AA=="),
            aspect_ratio: String::from("16:9"),
            ..text_to_video
        };
        assert!(validate_wan_video_arguments("wan2.7-i2v", &invalid_image_to_video).is_err());
    }

    /// Verifies Fun-Music model-specific gender and prompt requirements.
    /// 验证 Fun-Music 模型专属性别与提示词要求。
    #[test]
    fn validates_fun_music_model_boundaries() {
        let v1 = AlibabaFunMusicArguments {
            prompt: String::from("Piano ballad"),
            lyrics: String::new(),
            is_instrumental: false,
            gender: String::from("female"),
            output_format: String::from("mp3"),
            enable_aigc_watermark: false,
        };
        assert!(validate_fun_music_arguments("fun-music-v1", &v1).is_ok());
        let preview = AlibabaFunMusicArguments {
            gender: String::from("female"),
            ..v1
        };
        assert!(validate_fun_music_arguments("fun-music-preview", &preview).is_err());
    }
}
