//! Host-side image-understanding compatibility tool backed by user-configured external providers.
//! 由用户配置的外部提供方驱动的宿主侧图片理解兼容工具。

use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, read_provider_attachment,
};
use crate::attachment_assets::attachment_descriptor_from_path;
use crate::provider_config::{
    PersistedAiToolConfiguration, PersistedUnderstandImageProviderEntry,
    UnderstandImageProviderKind,
};
use crate::workspace_path::canonicalize_workspace_path;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error as StdError, fs, path::PathBuf, time::Duration};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

/// Parsed invocation arguments accepted by the semantic `understand_image` tool.
/// 语义 `understand_image` 工具接受的已解析调用参数。
#[derive(Debug, Clone)]
struct UnderstandImageInvocation {
    /// Optional semantic target selector such as `auto` or `instance/target`.
    /// 可选的语义目标选择器，例如 `auto` 或 `实例/目标`。
    target: Option<String>,
    /// Attachment selector resolved against the host-visible image catalog.
    /// 针对宿主可见图片目录解析的附件选择器。
    image: String,
    /// Optional user question about the image.
    /// 面向图片的可选用户问题。
    prompt: String,
}

/// Executes configured image understanding by trying usable providers in configured order.
/// 按配置顺序尝试可用提供方来执行配置化图片理解。
pub async fn execute_configured_understand_image(
    args: Value,
    configuration: PersistedAiToolConfiguration,
    conversation_attachments: Vec<ConversationAttachment>,
) -> Result<Value, String> {
    let invocation = parse_understand_image_invocation(args)?;
    let providers = configuration
        .projected_understand_image_providers()
        .into_iter()
        .filter(PersistedUnderstandImageProviderEntry::is_usable)
        .collect::<Vec<_>>();
    if providers.is_empty() {
        return Err(String::from(
            "The `understand_image` tool is using host compatibility mode, but no usable image-understanding providers are configured.",
        ));
    }

    let image_attachments = conversation_attachments
        .into_iter()
        .filter(|attachment| attachment.kind == ConversationAttachmentKind::Image)
        .collect::<Vec<_>>();
    let image_attachment = resolve_image_attachment_or_local_path(
        &image_attachments,
        &invocation.image,
    )
    .ok_or_else(|| {
        let image_selectors = available_image_selectors(&image_attachments);
        if image_selectors.is_empty() {
            format!(
                "The image `{}` requested by `understand_image` is not a registered image resource id and is not a readable supported local image path.",
                invocation.image
            )
        } else {
            format!(
                "The image `{}` requested by `understand_image` does not exist. Available image selectors: {}.",
                invocation.image,
                image_selectors.join(", ")
            )
        }
    })?;
    let available_targets = providers
        .iter()
        .map(PersistedUnderstandImageProviderEntry::qualified_target)
        .collect::<Vec<_>>();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| {
            format!("Failed to build the HTTP client for `understand_image`: {error}")
        })?;

    if let Some(explicit_target) = invocation
        .target
        .as_deref()
        .map(str::trim)
        .filter(|target| !target.is_empty() && !target.eq_ignore_ascii_case("auto"))
    {
        let provider = providers
            .iter()
            .find(|provider| provider.matches_target(explicit_target))
            .ok_or_else(|| {
                format!(
                    "The target `{explicit_target}` requested by `understand_image` does not exist or is not available. Available targets: {}.",
                    available_targets.join(", ")
                )
            })?;

        let content = execute_single_provider_understanding(
            &client,
            provider,
            &invocation,
            &image_attachment,
        )
        .await
        .map_err(|error| {
            format!(
                "The requested `understand_image` target `{}` failed: {error}",
                provider.qualified_target()
            )
        })?;
        return Ok(Value::String(render_understand_image_result(
            provider,
            &invocation,
            &image_attachment,
            &content,
        )));
    }

    let mut failures = Vec::new();
    for provider in &providers {
        match execute_single_provider_understanding(
            &client,
            provider,
            &invocation,
            &image_attachment,
        )
        .await
        {
            Ok(content) => {
                return Ok(Value::String(render_understand_image_result(
                    provider,
                    &invocation,
                    &image_attachment,
                    &content,
                )));
            }
            Err(error) => failures.push(format!("{}: {error}", provider.name)),
        }
    }

    Err(format!(
        "All configured `understand_image` providers failed: {}.",
        failures.join("; ")
    ))
}

/// Builds the JSON schema used by the configured image-understanding fallback tool.
/// 构建配置化图片理解回退工具使用的 JSON Schema。
pub fn configured_understand_image_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
    _conversation_attachments: &[ConversationAttachment],
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(
            configuration
                .usable_targets_for_kind(crate::provider_config::AiToolKind::UnderstandImage),
        )
        .collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "enum": target_enum,
                "default": "auto",
                "description": "Image-understanding target selector. Use `auto` to follow the configured default provider order, or choose one explicit instance/target pair."
            },
            "image": {
                "type": "string",
                "description": "Image selector. Use the resource id provided for pasted or uploaded image attachments, or use a full local path when the image was provided as a path or returned by another host tool."
            },
            "prompt": {
                "type": "string",
                "description": "Question or instruction about the image. Defaults to `Describe the image.` when omitted."
            }
        },
        "required": ["image"],
        "additionalProperties": false
    })
}

/// Parses model-provided JSON arguments into a stable image-understanding invocation.
/// 将模型提供的 JSON 参数解析为稳定的图片理解调用。
fn parse_understand_image_invocation(args: Value) -> Result<UnderstandImageInvocation, String> {
    let object = args.as_object().ok_or_else(|| {
        String::from("The arguments for `understand_image` must be a JSON object.")
    })?;
    let target = object
        .get("target")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let image = object
        .get("image")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            String::from("The `understand_image` tool is missing the required `image` argument.")
        })?
        .to_string();
    let prompt = object
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Describe the image.")
        .to_string();

    Ok(UnderstandImageInvocation {
        target,
        image,
        prompt,
    })
}

/// Executes one provider-specific image-understanding request.
/// 执行单个提供方专属图片理解请求。
async fn execute_single_provider_understanding(
    client: &reqwest::Client,
    provider: &PersistedUnderstandImageProviderEntry,
    invocation: &UnderstandImageInvocation,
    attachment: &ConversationAttachment,
) -> Result<String, String> {
    match provider.kind {
        UnderstandImageProviderKind::MiniMax => {
            invoke_minimax_image_understanding(client, provider, invocation, attachment).await
        }
    }
}

/// Calls the MiniMax VLM endpoint using the current provider configuration and image attachment.
/// 使用当前提供方配置与图片附件调用 MiniMax VLM 端点。
async fn invoke_minimax_image_understanding(
    client: &reqwest::Client,
    provider: &PersistedUnderstandImageProviderEntry,
    invocation: &UnderstandImageInvocation,
    attachment: &ConversationAttachment,
) -> Result<String, String> {
    let Some(api_key) = provider.resolved_api_key()? else {
        return Err(String::from("MiniMax Vision is missing a valid API key."));
    };
    let endpoint = resolve_minimax_vlm_endpoint(provider)?;
    let payload = match read_provider_attachment(attachment).map_err(|error| error.to_string())? {
        Some(payload) => payload,
        None => return Ok(build_missing_attachment_replay_text(attachment)),
    };
    let response = client
        .post(endpoint)
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({
            "prompt": invocation.prompt,
            "image_url": payload.data_url,
        }))
        .send()
        .await
        .map_err(format_minimax_vision_request_error)?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|error| format!("Failed to read the MiniMax Vision response body: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "MiniMax Vision returned HTTP {}: {}",
            status,
            truncate_error_text(&body_text, 400)
        ));
    }

    let payload: Value = serde_json::from_str(&body_text)
        .map_err(|error| format!("Failed to parse MiniMax Vision JSON response: {error}"))?;
    extract_minimax_vlm_content(&payload).ok_or_else(|| {
        format!(
            "MiniMax Vision returned no readable content: {}",
            truncate_error_text(&body_text, 240)
        )
    })
}

/// Resolves the MiniMax VLM endpoint by combining base URL overrides with region defaults while rejecting obvious regular text-API endpoints.
/// 在拒绝明显普通文本接口地址的前提下，结合基址覆盖与区域默认值解析 MiniMax VLM 端点。
fn resolve_minimax_vlm_endpoint(
    provider: &PersistedUnderstandImageProviderEntry,
) -> Result<String, String> {
    if provider.base_url.trim().is_empty() {
        return if provider.resolved_region().eq_ignore_ascii_case("cn") {
            Ok(String::from("https://api.minimaxi.com/v1/coding_plan/vlm"))
        } else {
            Ok(provider.resolved_base_url())
        };
    }

    let mut url = reqwest::Url::parse(&provider.resolved_base_url())
        .map_err(|error| format!("MiniMax Vision base URL is invalid: {error}"))?;
    let normalized_path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if minimax_path_looks_like_regular_text_api(&normalized_path) {
        return Err(String::from(
            "MiniMax image understanding uses the MiniMax VLM endpoint and does not support regular text-model API URLs. Provide an image-understanding endpoint (the current official path is `/v1/coding_plan/vlm`) or leave the field empty to use the default.",
        ));
    }
    if normalized_path.is_empty() || normalized_path == "/" || normalized_path == "/v1" {
        url.set_path("/v1/coding_plan/vlm");
    }
    Ok(url.to_string())
}

/// Returns whether one MiniMax path obviously points at a regular text-generation endpoint instead of the dedicated MiniMax VLM endpoint.
/// 返回单个 MiniMax 路径是否明显指向普通文本生成接口而非专用的 MiniMax VLM 端点。
fn minimax_path_looks_like_regular_text_api(path: &str) -> bool {
    let normalized = path.trim().to_ascii_lowercase();
    normalized.contains("chatcompletion")
        || normalized.contains("/text/")
        || normalized.contains("/anthropic")
        || normalized.contains("/openai")
        || normalized.contains("/responses")
        || normalized.contains("/messages")
}

/// Extracts the final human-readable content from one MiniMax VLM response payload.
/// 从单条 MiniMax VLM 响应载荷中提取最终的人类可读内容。
fn extract_minimax_vlm_content(payload: &Value) -> Option<String> {
    let direct = payload
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if direct.is_some() {
        return direct;
    }

    payload
        .pointer("/data/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Resolves one user-facing image selector against the latest host-visible image attachment catalog.
/// 针对最新宿主可见图片附件目录解析单个面向用户的图片选择器。
fn resolve_image_attachment_or_local_path(
    attachments: &[ConversationAttachment],
    image_selector: &str,
) -> Option<ConversationAttachment> {
    if let Some(attachment) = resolve_registered_image_attachment(attachments, image_selector) {
        return Some(attachment.clone());
    }

    resolve_image_attachment_from_local_path(image_selector)
}

/// Resolves one user-facing image selector against the latest host-visible image attachment catalog.
/// 针对最新宿主可见图片附件目录解析单个面向用户的图片选择器。
fn resolve_registered_image_attachment<'a>(
    attachments: &'a [ConversationAttachment],
    image_selector: &str,
) -> Option<&'a ConversationAttachment> {
    let selector = image_selector.trim();
    if selector.is_empty() {
        return None;
    }
    let normalized_selector = selector.to_ascii_lowercase();

    attachments.iter().find(|attachment| {
        attachment.local_path.trim().eq_ignore_ascii_case(selector)
            || attachment
                .source_path
                .as_deref()
                .is_some_and(|source_path| source_path.trim().eq_ignore_ascii_case(selector))
            || attachment
                .preview_local_path
                .as_deref()
                .is_some_and(|preview_path| preview_path.trim().eq_ignore_ascii_case(selector))
            || attachment
                .file_name
                .trim()
                .eq_ignore_ascii_case(&normalized_selector)
            || attachment
                .asset_id
                .as_deref()
                .is_some_and(|asset_id| asset_id.trim().eq_ignore_ascii_case(&normalized_selector))
            || std::path::Path::new(&attachment.local_path)
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|file_name| {
                    file_name.trim().eq_ignore_ascii_case(&normalized_selector)
                })
    })
}

/// Resolves one direct local image path into a synthetic image attachment descriptor.
/// 将一条直接本地图片路径解析为合成图片附件描述。
fn resolve_image_attachment_from_local_path(
    image_selector: &str,
) -> Option<ConversationAttachment> {
    // Candidate path supplied directly by the model or the user.
    // 模型或用户直接提供的候选路径。
    let selector = image_selector.trim();
    if selector.is_empty() {
        return None;
    }

    // Canonical filesystem path used for stable replay and provider payload reads.
    // 用于稳定回放与 Provider 载荷读取的规范文件系统路径。
    let candidate_path = canonicalize_workspace_path(&PathBuf::from(selector));
    if !candidate_path.is_file() {
        return None;
    }

    // Host attachment descriptor inferred from the local file extension.
    // 根据本地文件扩展名推断出的宿主附件描述信息。
    let descriptor = attachment_descriptor_from_path(&candidate_path)?;
    if descriptor.kind != ConversationAttachmentKind::Image {
        return None;
    }

    let metadata = fs::metadata(&candidate_path).ok();
    Some(ConversationAttachment {
        kind: ConversationAttachmentKind::Image,
        file_name: candidate_path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                vulcan_runtime_assets::native_host_path_text_from_path(&candidate_path)
            }),
        local_path: vulcan_runtime_assets::native_host_path_text_from_path(&candidate_path),
        source_path: None,
        asset_id: None,
        byte_len: metadata.map(|value| value.len()),
        preview_local_path: None,
        mime_type: String::from(descriptor.mime_type),
    })
}

/// Builds the ordered unique image selector list used only for runtime diagnostics.
/// 构建仅用于运行时诊断的有序唯一图片选择器列表。
fn available_image_selectors(attachments: &[ConversationAttachment]) -> Vec<String> {
    let mut seen_selectors = BTreeSet::new();
    let mut selectors = Vec::new();

    for attachment in attachments {
        if attachment.kind != ConversationAttachmentKind::Image {
            continue;
        }
        for selector in [
            attachment.asset_id.as_deref(),
            Some(attachment.local_path.as_str()),
            attachment.source_path.as_deref(),
            attachment.preview_local_path.as_deref(),
            Some(attachment.file_name.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            let selector = selector.trim();
            let normalized = selector.to_ascii_lowercase();
            if selector.is_empty() || !seen_selectors.insert(normalized) {
                continue;
            }
            selectors.push(selector.to_string());
        }
    }

    selectors
}

/// Renders one final host-tool result for the model after image understanding completes.
/// 在图片理解完成后为模型渲染最终宿主工具结果文本。
fn render_understand_image_result(
    provider: &PersistedUnderstandImageProviderEntry,
    invocation: &UnderstandImageInvocation,
    attachment: &ConversationAttachment,
    content: &str,
) -> String {
    format!(
        "Image understanding result\n\nProvider: {} ({})\nAttachment: {}\nQuestion: {}\n\n{}",
        provider.name,
        provider.qualified_target(),
        attachment.file_name,
        invocation.prompt,
        content.trim()
    )
}

/// Formats one MiniMax VLM transport failure with the reqwest classification flags and source chain.
/// 使用 reqwest 分类标记与 source 链格式化单次 MiniMax VLM 传输失败。
fn format_minimax_vision_request_error(error: reqwest::Error) -> String {
    let mut parts = vec![format!("MiniMax Vision request failed: {error}")];
    let flags = [
        ("timeout", error.is_timeout()),
        ("connect", error.is_connect()),
        ("request", error.is_request()),
        ("body", error.is_body()),
    ]
    .into_iter()
    .filter_map(|(label, active)| active.then_some(label))
    .collect::<Vec<_>>();
    if !flags.is_empty() {
        parts.push(format!("kind={}", flags.join(",")));
    }

    let mut sources = Vec::new();
    let mut current = StdError::source(&error);
    while let Some(source) = current {
        sources.push(source.to_string());
        current = source.source();
    }
    if !sources.is_empty() {
        parts.push(format!(
            "sources={}",
            truncate_error_text(&sources.join(" -> "), 600)
        ));
    }

    parts.join("; ")
}

/// Truncates provider error bodies before they are surfaced to the model or UI.
/// 在向模型或界面展示提供方错误正文前进行截断。
fn truncate_error_text(text: &str, max_chars: usize) -> String {
    let mut truncated = text.trim().chars().take(max_chars).collect::<String>();
    if text.trim().chars().count() > max_chars {
        truncated.push_str("...");
    }
    truncated
}

#[cfg(test)]
mod tests {
    //! Host-side image-understanding compatibility tests.
    //! 宿主侧图片理解兼容测试。

    use super::{
        configured_understand_image_parameters_schema, parse_understand_image_invocation,
        resolve_image_attachment_or_local_path, resolve_minimax_vlm_endpoint,
    };
    use crate::provider_config::{
        PersistedAiToolConfiguration, PersistedUnderstandImageConfiguration,
        PersistedUnderstandImageProviderEntry, UnderstandImageProviderKind,
    };
    use serde_json::json;
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one minimal image attachment used by schema tests.
    /// 构建供 Schema 测试使用的一条最小图片附件。
    fn test_image_attachment(file_name: &str) -> ConversationAttachment {
        ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: file_name.to_string(),
            local_path: format!("C:\\temp\\{file_name}"),
            source_path: None,
            asset_id: Some(format!("asset-{file_name}")),
            byte_len: Some(4),
            preview_local_path: None,
            mime_type: String::from("image/png"),
        }
    }

    /// Parses the schema-declared image-understanding argument names.
    /// 解析 Schema 声明的图片理解参数名称。
    #[test]
    fn parse_understand_image_invocation_accepts_schema_argument_names() {
        let invocation = parse_understand_image_invocation(json!({
            "target": "minimax/minimax",
            "image": "screen.png",
            "prompt": "What text appears in the image?"
        }))
        .expect("arguments should parse");

        assert_eq!(invocation.target.as_deref(), Some("minimax/minimax"));
        assert_eq!(invocation.image, "screen.png");
        assert_eq!(invocation.prompt, "What text appears in the image?");
    }

    /// Rejects alias-only image-understanding arguments that are absent from the schema.
    /// 拒绝未出现在 Schema 中的图片理解别名参数。
    #[test]
    fn parse_understand_image_invocation_rejects_alias_only_image() {
        let error = parse_understand_image_invocation(json!({
            "attachment": "screen.png",
            "question": "What text appears in the image?"
        }))
        .expect_err("alias-only image should be rejected");

        assert!(error.contains("image"));
    }

    /// Resolves resource ids and full local image paths while retaining legacy selector compatibility.
    /// 解析资源标识与完整本地图片路径，同时保留旧选择器兼容性。
    #[test]
    fn resolve_image_attachment_accepts_resource_id_and_full_local_path() {
        let attachment = test_image_attachment("screen.png");
        let attachments = vec![attachment.clone()];
        let selected = resolve_image_attachment_or_local_path(&attachments, "asset-screen.png")
            .expect("resource id should resolve");

        assert_eq!(selected.file_name, "screen.png");
        let selected = resolve_image_attachment_or_local_path(&attachments, r"C:\temp\screen.png")
            .expect("full local path should resolve");

        assert_eq!(selected.file_name, "screen.png");
        assert_eq!(
            resolve_image_attachment_or_local_path(&[attachment], "screen.png")
                .expect("legacy file name should still resolve")
                .file_name,
            "screen.png"
        );
    }

    /// Resolves one readable local image path even when it is not registered as a conversation attachment.
    /// 即使本地图片路径没有注册为会话附件，也能解析可读取的本地图片路径。
    #[test]
    fn resolve_image_attachment_accepts_unregistered_local_image_path() {
        let image_path = std::env::temp_dir().join(format!(
            "vulcan-understand-image-test-{}.png",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&image_path, [0_u8, 1, 2, 3]).expect("temporary image should write");

        let selected =
            resolve_image_attachment_or_local_path(&[], image_path.to_string_lossy().as_ref())
                .expect("unregistered local image path should resolve");

        assert_eq!(selected.kind, ConversationAttachmentKind::Image);
        assert_eq!(selected.mime_type, "image/png");
        assert!(std::path::Path::new(&selected.local_path).is_file());

        let _ = std::fs::remove_file(image_path);
    }

    /// Exposes a strict schema with stable target routing and selector-based image selection.
    /// 暴露一份包含稳定 target 路由与基于选择器图片选择的严格 Schema。
    #[test]
    fn configured_understand_image_schema_requires_image() {
        let configuration = PersistedAiToolConfiguration {
            vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
                id: String::from("minimax"),
                vendor_id: String::from("minimax"),
                name: String::from("MiniMax"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        crate::provider_config::AiToolKind::UnderstandImage,
                        "minimax",
                        "minimax",
                    ),
                ],
            }],
            understand_image: PersistedUnderstandImageConfiguration {
                target_order: vec![String::from("minimax/minimax")],
                ..PersistedUnderstandImageConfiguration::default()
            },
            ..PersistedAiToolConfiguration::default()
        };
        let schema = configured_understand_image_parameters_schema(
            &configuration,
            &[test_image_attachment("screen.png")],
        );

        assert_eq!(schema["required"][0], "image");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["target"]["enum"][0], "auto");
        assert!(
            schema["properties"]["image"].get("enum").is_none(),
            "image selector should not use attachment-derived enums"
        );
        assert!(
            schema["properties"]["image"]["description"]
                .as_str()
                .expect("image description should be text")
                .contains("resource id")
        );
        assert!(
            schema["properties"]["image"]["description"]
                .as_str()
                .expect("image description should be text")
                .contains("full local path")
        );
    }

    /// Completes the VLM path when the provider uses only a bare MiniMax base URL.
    /// 当提供方仅使用裸 MiniMax 基址时自动补全 VLM 路径。
    #[test]
    fn resolve_minimax_vlm_endpoint_completes_vlm_path_for_bare_base_url() {
        let mut provider = PersistedUnderstandImageProviderEntry::new(
            "minimax",
            UnderstandImageProviderKind::MiniMax,
        );
        provider.base_url = String::from("https://api.minimax.io/v1");

        let endpoint = resolve_minimax_vlm_endpoint(&provider).expect("endpoint should resolve");

        assert_eq!(endpoint, "https://api.minimax.io/v1/coding_plan/vlm");
    }
}
