//! Native microphone-only OpenVulcan routes.
//! 仅供麦克风使用的原生 OpenVulcan 路由。
//!
//! Contract: openvulcan-web docs/deployment/VOICE_INPUT.md and internal/voiceinput/service.go.
//! 契约来源为 openvulcan-web 的 docs/deployment/VOICE_INPUT.md 与 internal/voiceinput/service.go。

use std::{collections::HashSet, sync::LazyLock, time::Duration};

use reqwest::{Method, StatusCode, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::CredentialExecutionSecret;

/// Duration of the fixed version-one microphone format, in seconds.
/// 第一版固定麦克风格式的录音时长上限，单位为秒。
pub const OPENVULCAN_VOICE_DURATION_SECONDS: u32 = 120;

/// Largest canonical mono 16-bit 16 kHz microphone WAV, including its header.
/// 单声道、16 位、16 kHz 标准麦克风 WAV 的最大字节数，包含文件头。
pub const OPENVULCAN_VOICE_MAX_AUDIO_BYTES: usize =
    44 + OPENVULCAN_VOICE_DURATION_SECONDS as usize * 16_000 * 2;

/// Bounded bytes for either a catalog or one transcript response.
/// 目录与单次转写响应的字节数上限。
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Transport rejects redirects and protocol retries for this potentially billed request.
/// 此传输拒绝重定向和协议层重试，以避免重复提交可能计费的请求。
static HTTP: LazyLock<Option<reqwest::Client>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .ok()
});

/// Public metadata from the authenticated microphone directory.
/// 来自认证麦克风目录的公开元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenVulcanVoiceModel {
    /// Stable publication ID sent to the microphone service.
    /// 发送给麦克风服务的稳定发布标识。
    pub id: String,
    /// Name chosen by the site administrator.
    /// 站点管理员设置的显示名称。
    pub display_name: String,
    /// Explicit category checked before presenting any model.
    /// 在展示模型前校验的明确分类。
    category: VoiceModelCategory,
    /// Exact preferred-language hints permitted for this model.
    /// 此模型允许的精确优先语言提示。
    pub supported_languages: Vec<String>,
}

/// Closed set of categories admitted on this dedicated route.
/// 此专用路由允许的封闭分类集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum VoiceModelCategory {
    /// Synchronous transcription of a single microphone recording.
    /// 同步转写单段麦克风录音。
    SpeechTranscribe,
}

/// Versioned availability and model catalog for the account's site.
/// 账号所属站点的版本化可用性和模型目录。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenVulcanVoiceCatalog {
    /// Wire version and fixed audio duration enforced by this client.
    /// 本客户端执行的线协议版本与固定录音时长。
    contract_version: u32,
    max_duration_seconds: u32,
    /// Whether the deployment publishes the free microphone channel.
    /// 当前部署是否发布免费麦克风通道。
    pub available: bool,
    /// Site default for an account without a usable saved cloud model.
    /// 账号没有可用的已选云端模型时使用的站点默认模型。
    pub default_model_id: String,
    /// Enabled models in their server-defined order.
    /// 按服务端顺序排列的已启用模型。
    pub models: Vec<OpenVulcanVoiceModel>,
}

/// Response for one transcription attempt.
/// 一次转写请求的响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transcript {
    /// Text recognized from this recording, possibly empty.
    /// 此录音识别出的文本，可能为空。
    text: String,
    /// Opaque server receipt identifying the attempt.
    /// 标识本次请求的不透明服务端回执。
    request_id: String,
}

/// Stable failure reasons that can be shown without server error text.
/// 可在不显示服务端原始错误文本的情况下展示的稳定失败原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenVulcanVoiceError {
    /// Native credential is missing or expired.
    /// 原生凭据缺失或已过期。
    SignIn,
    /// The site cannot accept this request.
    /// 站点当前无法接受此请求。
    Unavailable,
    /// The published ID is no longer active.
    /// 发布标识已失效。
    ModelUnavailable,
    /// The model no longer permits the requested language hint.
    /// 模型已不再接受请求的语言提示。
    UnsupportedLanguage,
    /// The shared service or the caller's quota requires waiting.
    /// 共享服务或调用方额度要求等待。
    RetryAfter(u64),
    /// The audio violated the fixed WAV contract.
    /// 音频违反固定 WAV 契约。
    InvalidAudio,
}

/// Extracts only the stable code from a standard server error envelope.
/// 只从标准服务端错误封套提取稳定错误码。
#[derive(Deserialize)]
struct ErrorEnvelope {
    /// Typed error from the current route.
    /// 当前路由返回的类型化错误。
    error: ErrorCode,
}

/// Holds the stable error code without retaining server-provided text.
/// 保存稳定错误码，不保留服务端文案。
#[derive(Deserialize)]
struct ErrorCode {
    /// Code documented by the native microphone route.
    /// 原生麦克风路由契约声明的错误码。
    code: String,
}

/// Reads a complete response within the protocol bound.
/// 在协议上限内读取完整响应。
async fn read_bounded(mut response: reqwest::Response) -> Result<Vec<u8>, OpenVulcanVoiceError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| OpenVulcanVoiceError::Unavailable)?
    {
        if chunk.len() > MAX_RESPONSE_BYTES - bytes.len() {
            return Err(OpenVulcanVoiceError::Unavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Classifies known status/code pairs and honors the server's bounded retry interval.
/// 分类已知的状态码与错误码组合，并遵守服务端给出的有界重试等待时间。
fn rejection(status: StatusCode, bytes: &[u8], retry_seconds: Option<u64>) -> OpenVulcanVoiceError {
    use OpenVulcanVoiceError as Error;
    if status == StatusCode::UNAUTHORIZED {
        return Error::SignIn;
    }
    let error = serde_json::from_slice::<ErrorEnvelope>(bytes).ok();
    match (
        status.as_u16(),
        error.as_ref().map(|body| body.error.code.as_str()),
    ) {
        (404, Some("voice_model_unavailable")) => Error::ModelUnavailable,
        (400, Some("unsupported_voice_language")) => Error::UnsupportedLanguage,
        (400, Some("invalid_voice_audio"))
        | (413, Some("voice_audio_too_large"))
        | (415, Some("unsupported_voice_audio")) => Error::InvalidAudio,
        (429, Some("voice_rate_limited")) | (503, Some("voice_busy")) => retry_seconds
            .filter(|seconds| (1..=86_400).contains(seconds))
            .map_or(Error::Unavailable, Error::RetryAfter),
        _ => Error::Unavailable,
    }
}

/// Sends one request to the credential's regional Auth origin and decodes its exact success shape.
/// 发送一次请求至凭据所属区域的 Auth 来源，并解码其精确成功响应结构。
async fn send<T: DeserializeOwned>(
    secret: &CredentialExecutionSecret,
    method: Method,
    route: &str,
    timeout: Duration,
    audio: Option<(&str, Option<&str>, Vec<u8>)>,
) -> Result<T, OpenVulcanVoiceError> {
    let region = secret
        .openvulcan_region()
        .ok_or(OpenVulcanVoiceError::SignIn)?;
    let url = format!(
        "{}/api/v1/vulcancode/voice-input/{route}",
        region.auth_origin()
    );
    let authorization = crate::openvulcan::authorization_header(secret, &url)
        .map_err(|_| OpenVulcanVoiceError::SignIn)?;
    let http = HTTP.as_ref().ok_or(OpenVulcanVoiceError::Unavailable)?;
    let mut request = http
        .request(method, url)
        .timeout(timeout)
        .header(header::AUTHORIZATION, authorization)
        .header(header::ACCEPT, "application/json");
    if let Some((model, language, bytes)) = audio {
        request = request
            .header(header::CONTENT_TYPE, "audio/wav")
            .header("X-Vulcan-Voice-Model", model)
            .body(bytes);
        if let Some(language) = language {
            request = request.header("X-Vulcan-ASR-Language", language);
        }
    }
    let response = request
        .send()
        .await
        .map_err(|_| OpenVulcanVoiceError::Unavailable)?;
    let status = response.status();
    let retry_seconds = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let bytes = read_bounded(response).await?;
    if status != StatusCode::OK {
        return Err(rejection(status, &bytes, retry_seconds));
    }
    serde_json::from_slice(&bytes).map_err(|_| OpenVulcanVoiceError::Unavailable)
}

/// Checks the fixed wire version, directory identity, and header-safe model and language fields.
/// 校验固定线协议版本、目录标识以及可安全写入请求头的模型和语言字段。
fn validate_catalog(catalog: &OpenVulcanVoiceCatalog) -> bool {
    if catalog.contract_version != 1
        || catalog.max_duration_seconds != OPENVULCAN_VOICE_DURATION_SECONDS
    {
        return false;
    }
    if !catalog.available {
        return catalog.models.is_empty() && catalog.default_model_id.is_empty();
    }
    let mut models = HashSet::new();
    catalog.models.iter().all(|model| {
        !model.id.is_empty()
            && model.id.len() <= 63
            && model.id.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || (index != 0 && byte == b'-')
            })
            && !model.display_name.trim().is_empty()
            && !model.display_name.chars().any(char::is_control)
            && models.insert(model.id.as_str())
            && model.supported_languages.iter().all(|language| {
                let valid_letters = |value: &str, lengths: &[usize]| {
                    lengths.contains(&value.len())
                        && value.bytes().all(|byte| byte.is_ascii_lowercase())
                };
                match language.split_once('-') {
                    Some((base, suffix)) => {
                        valid_letters(base, &[2, 3]) && valid_letters(suffix, &[2])
                    }
                    None => valid_letters(language, &[2, 3]),
                }
            })
            && model
                .supported_languages
                .iter()
                .collect::<HashSet<_>>()
                .len()
                == model.supported_languages.len()
    }) && models.contains(catalog.default_model_id.as_str())
}

/// Loads the authenticated microphone model directory for the given native credential.
/// 使用所给原生凭据加载已认证麦克风模型目录。
pub async fn request_openvulcan_voice_catalog(
    secret: &CredentialExecutionSecret,
) -> Result<OpenVulcanVoiceCatalog, OpenVulcanVoiceError> {
    let catalog = send(
        secret,
        Method::GET,
        "capabilities",
        Duration::from_secs(5),
        None,
    )
    .await?;
    if !validate_catalog(&catalog) {
        return Err(OpenVulcanVoiceError::Unavailable);
    }
    Ok(catalog)
}

/// Sends one in-memory microphone WAV with the user's exact model and optional language.
/// 使用用户选择的精确模型及可选语言发送一段内存中的麦克风 WAV。
pub async fn request_openvulcan_voice_transcription(
    secret: &CredentialExecutionSecret,
    model: &str,
    language: Option<&str>,
    bytes: Vec<u8>,
) -> Result<String, OpenVulcanVoiceError> {
    if bytes.len() <= 44 || bytes.len() > OPENVULCAN_VOICE_MAX_AUDIO_BYTES {
        return Err(OpenVulcanVoiceError::InvalidAudio);
    }
    let transcript: Transcript = send(
        secret,
        Method::POST,
        "transcriptions",
        Duration::from_secs(65),
        Some((model, language, bytes)),
    )
    .await?;
    if transcript.request_id.is_empty() {
        return Err(OpenVulcanVoiceError::Unavailable);
    }
    Ok(transcript.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks that the directory accepts a dedicated audio model and rejects a duplicate route.
    /// 检查目录接受专用语音模型，并拒绝重复路由。
    #[test]
    fn voice_input_catalog_validates_publication_identity() {
        let mut catalog: OpenVulcanVoiceCatalog = serde_json::from_value(serde_json::json!({
            "contract_version": 1, "available": true, "max_duration_seconds": OPENVULCAN_VOICE_DURATION_SECONDS,
            "default_model_id": "qwen-asr", "models": [
                {"id":"qwen-asr", "display_name":"Qwen ASR", "category":"speech_transcribe", "supported_languages":["zh","it"]}
            ],
        })).unwrap();
        assert!(validate_catalog(&catalog));
        catalog.models.push(catalog.models[0].clone());
        assert!(!validate_catalog(&catalog));
    }

    /// Checks that the expected quota code and status preserve the server's interval.
    /// 检查预期的额度错误码与状态码保留服务端等待时间。
    #[test]
    fn voice_input_quota_preserves_retry_interval() {
        assert_eq!(
            rejection(
                StatusCode::TOO_MANY_REQUESTS,
                br#"{"error":{"code":"voice_rate_limited","message":"ignored"}}"#,
                Some(20)
            ),
            OpenVulcanVoiceError::RetryAfter(20)
        );
    }
}
