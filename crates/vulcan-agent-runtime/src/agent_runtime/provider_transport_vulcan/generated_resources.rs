//! Managed materialization of Provider-generated image resources.
//! Provider 生成图片资源的托管落盘。

use super::provider_failure;
use crate::attachment_assets::{AttachmentDescriptor, stage_attachment_bytes};
use base64::Engine;
use vulcan_ai_contract::{AiStreamEvent, ContentItem, ResourceKind};
use vulcan_core::types::{ConversationAttachmentKind, CoreError as VulcanCoreError};

/// Materializes one provider-generated Data URL into a host-owned workspace file.
/// 将供应商生成的 Data URL 落盘为宿主自有的工作区文件。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the failing route in controlled diagnostics.
/// `provider_id` 在受控诊断中标识失败路由。
/// `workspace_path` is the canonical workspace that owns generated outputs.
/// `workspace_path` 是拥有生成输出的规范工作区。
/// `event` is one normalized provider event.
/// `event` 是一项规范化供应商事件。
///
/// # Returns
/// 返回值
/// The unchanged event when no generated resource is present, or the same resource event with
/// a local path after managed attachment staging and index publication.
/// 当不存在生成资源时原样返回事件；否则在托管附件暂存与索引发布后返回带本地路径的同一资源事件。
pub(super) async fn materialize_provider_resource_event(
    provider_id: &str,
    workspace_path: &str,
    event: AiStreamEvent,
) -> Result<AiStreamEvent, VulcanCoreError> {
    let AiStreamEvent::ResponseItemStarted {
        item_id,
        item: ContentItem::Resource { mut resource },
    } = event
    else {
        return Ok(event);
    };
    let Some(data_url) = resource.url.as_deref() else {
        return Ok(AiStreamEvent::ResponseItemStarted {
            item_id,
            item: ContentItem::Resource { resource },
        });
    };
    if !data_url.starts_with("data:") {
        return Ok(AiStreamEvent::ResponseItemStarted {
            item_id,
            item: ContentItem::Resource { resource },
        });
    }
    let (media_type, encoded) = parse_generated_data_url(provider_id, data_url)?;
    if resource.kind != ResourceKind::Image {
        return Err(provider_failure(
            provider_id,
            "Only provider-generated image Data URLs are currently registered.",
        ));
    }
    let descriptor = generated_image_descriptor(provider_id, media_type)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            provider_failure(
                provider_id,
                format!("Provider-generated image Base64 is invalid: {error}").as_str(),
            )
        })?;
    const MAX_GENERATED_IMAGE_BYTES: usize = 50 * 1024 * 1024;
    if bytes.len() > MAX_GENERATED_IMAGE_BYTES {
        return Err(provider_failure(
            provider_id,
            "Provider-generated image exceeds the 50 MiB host limit.",
        ));
    }
    if workspace_path.trim().is_empty() {
        return Err(provider_failure(
            provider_id,
            "Generated output requires a workspace path.",
        ));
    }
    let workspace_path = workspace_path.to_string();
    let suggested_file_name = format!(
        "{}.{}",
        safe_generated_resource_stem(resource.id.as_str()),
        descriptor.extension
    );
    let attachment = tokio::task::spawn_blocking(move || {
        stage_attachment_bytes(
            workspace_path.as_str(),
            &descriptor,
            suggested_file_name.as_str(),
            None,
            bytes.as_slice(),
        )
    })
    .await
    .map_err(|error| {
        provider_failure(
            provider_id,
            format!("Generated output staging task failed: {error}").as_str(),
        )
    })?
    .map_err(|error| {
        provider_failure(
            provider_id,
            format!("Generated output could not enter managed attachment storage: {error}")
                .as_str(),
        )
    })?;
    resource.media_type = Some(media_type.to_string());
    resource.local_path = Some(attachment.local_path);
    resource.url = None;
    Ok(AiStreamEvent::ResponseItemStarted {
        item_id,
        item: ContentItem::Resource { resource },
    })
}

/// Parses one exact Base64 Data URL returned by a provider resource event.
/// 解析供应商资源事件返回的一项精确 Base64 Data URL。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled errors and `data_url` is the exact returned URL.
/// `provider_id` 标识受控错误，`data_url` 是精确返回的 URL。
///
/// # Returns
/// 返回值
/// The declared MIME type and encoded payload.
/// 返回声明的 MIME 类型与编码载荷。
fn parse_generated_data_url<'a>(
    provider_id: &str,
    data_url: &'a str,
) -> Result<(&'a str, &'a str), VulcanCoreError> {
    let body = data_url.strip_prefix("data:").ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated resource URL is not a Data URL.",
        )
    })?;
    let (declaration, encoded) = body.split_once(',').ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated Data URL is missing its payload separator.",
        )
    })?;
    let media_type = declaration.strip_suffix(";base64").ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated Data URL is not Base64 encoded.",
        )
    })?;
    if encoded.is_empty() {
        return Err(provider_failure(
            provider_id,
            "Provider-generated Data URL payload is empty.",
        ));
    }
    Ok((media_type, encoded))
}

/// Resolves a registered generated-image MIME type to its managed attachment descriptor.
/// 将已注册的生成图片 MIME 类型解析为托管附件描述。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled errors and `media_type` is the exact declared MIME type.
/// `provider_id` 标识受控错误，`media_type` 是精确声明的 MIME 类型。
///
/// # Returns
/// 返回值
/// A stable managed attachment descriptor or a controlled error for an unregistered type.
/// 返回稳定托管附件描述；未注册类型则返回受控错误。
fn generated_image_descriptor(
    provider_id: &str,
    media_type: &str,
) -> Result<AttachmentDescriptor, VulcanCoreError> {
    match media_type {
        "image/png" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/png",
            extension: "png",
        }),
        "image/jpeg" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/jpeg",
            extension: "jpg",
        }),
        "image/webp" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/webp",
            extension: "webp",
        }),
        other => Err(provider_failure(
            provider_id,
            format!("Generated image MIME type `{other}` is not registered.").as_str(),
        )),
    }
}

/// Produces a filesystem-safe stem from one provider-neutral resource identifier.
/// 从一个供应商无关资源标识生成文件系统安全的主文件名。
///
/// # Parameters
/// 参数
/// `resource_id` is the stable normalized resource identifier.
/// `resource_id` 是稳定的规范化资源标识。
///
/// # Returns
/// 返回值
/// A non-empty ASCII filename stem.
/// 返回非空 ASCII 文件主名。
fn safe_generated_resource_stem(resource_id: &str) -> String {
    let stem = resource_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(80)
        .collect::<String>();
    if stem.is_empty() {
        String::from("generated-image")
    } else {
        stem
    }
}

#[cfg(test)]
mod tests;
