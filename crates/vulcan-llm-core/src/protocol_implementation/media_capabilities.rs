//! Source-form capabilities of registered media projectors, independent of model and size limits.
//! 已注册媒体投影器的来源形态能力，独立于模型及大小限制。

use super::*;
use vulcan_ai_contract::ResourceSourceKind;

/// Checks the exact registered `profile_id`, `kind`, `mime` and prepared `source` form.
/// 检查精确已注册的 `profile_id`、`kind`、`mime` 与已准备 `source` 形态。
/// Returns encoding support only; callers must separately enforce model, MIME, size and count contracts.
/// 仅返回编码支持；调用者仍须独立执行模型、MIME、大小及数量契约。
/// Unprepared host references are not guessed to be inline or remote.
/// 不把未准备的宿主引用猜测为内联或远程来源。
pub fn protocol_profile_supports_media_source(
    profile_id: &str,
    kind: ResourceKind,
    mime: &str,
    source: ResourceSourceKind,
) -> bool {
    // Normalize MIME labels without reclassifying the caller's authoritative resource kind.
    // 规范 MIME 标签，不重新分类调用者提供的权威资源类型。
    let mime = mime.trim().to_ascii_lowercase();
    // Known media families cannot be disguised as generic files to bypass modality checks.
    // 不能把已知媒体族伪装成通用文件以绕过模态校验。
    let kind_matches = !mime.is_empty()
        && match kind {
            ResourceKind::Image => mime.starts_with("image/"),
            ResourceKind::Audio => mime.starts_with("audio/"),
            ResourceKind::Video => mime.starts_with("video/"),
            ResourceKind::File => !["image/", "audio/", "video/"]
                .iter()
                .any(|prefix| mime.starts_with(prefix)),
        };
    if !kind_matches || source == ResourceSourceKind::HostResolved {
        return false;
    }
    match profile_id {
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            matches!(kind, ResourceKind::Image | ResourceKind::File)
        }
        OPENAI_CHAT_PROFILE_ID => match kind {
            ResourceKind::Image | ResourceKind::Video => {
                source != ResourceSourceKind::ProviderFileId
            }
            ResourceKind::File => source != ResourceSourceKind::RemoteUrl,
            ResourceKind::Audio => {
                source == ResourceSourceKind::InlineData
                    && matches!(
                        mime.as_str(),
                        "audio/wav"
                            | "audio/x-wav"
                            | "audio/mpeg"
                            | "audio/mp3"
                            | "audio/aiff"
                            | "audio/x-aiff"
                            | "audio/aac"
                            | "audio/ogg"
                            | "audio/flac"
                            | "audio/mp4"
                            | "audio/m4a"
                            | "audio/x-m4a"
                    )
            }
        },
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => {
            // The standard `source.type = file` shape is registered for images only.
            // 标准 `source.type = file` 结构仅为图片注册。
            if source == ResourceSourceKind::ProviderFileId && kind != ResourceKind::Image {
                return false;
            }
            match kind {
                ResourceKind::Image => {
                    source == ResourceSourceKind::RemoteUrl
                        || anthropic::anthropic_image_media_type_supported(&mime)
                }
                ResourceKind::File => true,
                ResourceKind::Video => {
                    profile_id == ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
                        && (source == ResourceSourceKind::RemoteUrl
                            || matches!(
                                mime.as_str(),
                                "video/mp4"
                                    | "video/x-msvideo"
                                    | "video/quicktime"
                                    | "video/mov"
                                    | "video/x-matroska"
                            ))
                }
                ResourceKind::Audio => false,
            }
        }
        GEMINI_GENERATE_CONTENT_PROFILE_ID | GOOGLE_CODE_ASSIST_PROFILE_ID => {
            source != ResourceSourceKind::ProviderFileId
        }
        GEMINI_INTERACTIONS_PROFILE_ID => source == ResourceSourceKind::InlineData,
        _ => false,
    }
}
