//! Host-managed attachment staging helpers shared by the composer, persistence, and provider replay layers.
//! 供提示词输入区、持久化与 Provider 回放层共享的宿主管理附件暂存辅助模块。

use crate::attachment_asset_index::{
    AttachmentReplayStatus, current_attachment_timestamp_parts, record_or_reuse_project_attachment,
    record_staged_attachment, resolve_attachment_replay_status,
};
use crate::workspace_path::{
    canonicalize_workspace_path, normalized_workspace_path_text_from_path,
};
use filetime::{FileTime, set_file_times};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_runtime_assets::user_vulcan_code_root_for_home;

/// Stable hidden workspace directory used to retain host-managed attachment copies.
/// 用于保留宿主管理附件副本的稳定隐藏工作区目录名称。
const WORKSPACE_VULCAN_DIR_NAME: &str = ".vulcan";

/// Stable nested workspace subtree used to group code-owned project data.
/// 用于归拢代码托管项目数据的稳定工作区子树名称。
const WORKSPACE_CODE_DIR_NAME: &str = "code";

/// Stable leaf directory used below the host-managed attachment root.
/// 宿主管理附件根目录下使用的稳定叶子目录名称。
const MANAGED_ATTACHMENT_DIR_NAME: &str = "attachments";

/// Stable image category directory name used below the managed attachment root.
/// 宿主管理附件根目录下用于图片分类的稳定目录名称。
const IMAGE_ATTACHMENT_DIR_NAME: &str = "images";

/// Stable audio category directory name used below the managed attachment root.
/// 宿主管理附件根目录下用于音频分类的稳定目录名称。
const AUDIO_ATTACHMENT_DIR_NAME: &str = "audio";

/// Stable video category directory name used below the managed attachment root.
/// 宿主管理附件根目录下用于视频分类的稳定目录名称。
const VIDEO_ATTACHMENT_DIR_NAME: &str = "video";

/// Stable PDF category directory name used below the managed attachment root.
/// 宿主管理附件根目录下用于 PDF 分类的稳定目录名称。
const PDF_ATTACHMENT_DIR_NAME: &str = "pdf";
/// Stable generic-file category directory name used below the managed attachment root.
/// 宿主管理附件根目录下用于通用文件分类的稳定目录名称。
const FILE_ATTACHMENT_DIR_NAME: &str = "files";

/// Maximum JPEG preview sidecar size accepted from the browser composer.
/// 输入区从浏览器接收的 JPEG 预览旁路文件最大尺寸。
const IMAGE_PREVIEW_SIDECAR_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Structured descriptor resolved from one supported attachment file path or MIME type.
/// 从受支持附件文件路径或 MIME 类型解析得到的结构化描述信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentDescriptor {
    /// Stable attachment kind consumed by capability gates and provider mappers.
    /// 供能力门控与 Provider 映射消费的稳定附件类型。
    pub kind: ConversationAttachmentKind,
    /// Stable MIME type used by the host-managed asset copy and provider payload encoders.
    /// 供宿主管理资产副本与 Provider 载荷编码器使用的稳定 MIME 类型。
    pub mime_type: &'static str,
    /// Preferred file extension used when the host materializes one new managed asset.
    /// 宿主物化新的托管资产时使用的首选文件扩展名。
    pub extension: &'static str,
}

/// Returns the closed generic-file descriptor used when no media or document format is known.
/// 当无法识别媒体或文档格式时，返回封闭的通用文件描述。
pub fn generic_attachment_descriptor() -> AttachmentDescriptor {
    AttachmentDescriptor {
        kind: ConversationAttachmentKind::File,
        mime_type: "application/octet-stream",
        extension: "bin",
    }
}

/// Returns a generic staging descriptor whose modality follows one validated top-level MIME type.
/// 返回一份通用暂存描述，其模态遵循一个已校验的顶级 MIME 类型。
///
/// # Parameters
/// 参数
/// `mime_type` is normalized only for modality classification; its concrete value is persisted separately.
/// `mime_type` 仅用于模态分类；其具体值会另行持久化。
///
/// # Returns
/// 返回值
/// A generic image, audio, video, or file descriptor without claiming protocol-native format support.
/// 返回通用图片、音频、视频或文件描述，但不声称协议原生支持该格式。
pub fn generic_attachment_descriptor_for_mime_type(mime_type: &str) -> AttachmentDescriptor {
    let normalized = mime_type.trim().to_ascii_lowercase();
    let kind = if normalized.starts_with("image/") {
        ConversationAttachmentKind::Image
    } else if normalized.starts_with("audio/") {
        ConversationAttachmentKind::Audio
    } else if normalized.starts_with("video/") {
        ConversationAttachmentKind::Video
    } else {
        ConversationAttachmentKind::File
    };
    AttachmentDescriptor {
        kind,
        mime_type: "application/octet-stream",
        extension: "bin",
    }
}

/// Resolves the stable managed attachment root for the current workspace or falls back to the shared Vulcan home code subtree.
/// 为当前工作区解析稳定的托管附件根目录，并在缺失时回退到共享 Vulcan 家目录下的 code 子树。
pub fn resolve_managed_attachment_root(workspace_path: &str) -> PathBuf {
    let trimmed_workspace = workspace_path.trim();
    if !trimmed_workspace.is_empty() {
        let workspace_candidate = canonicalize_workspace_path(Path::new(trimmed_workspace));
        let workspace_directory = if workspace_candidate.is_dir() {
            workspace_candidate
        } else {
            workspace_candidate
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(workspace_candidate)
        };
        return workspace_directory
            .join(WORKSPACE_VULCAN_DIR_NAME)
            .join(WORKSPACE_CODE_DIR_NAME)
            .join(MANAGED_ATTACHMENT_DIR_NAME);
    }

    let home_directory = vulcan_runtime_assets::user_home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    user_vulcan_code_root_for_home(&home_directory).join(MANAGED_ATTACHMENT_DIR_NAME)
}

/// Resolves the supported attachment descriptor for one local file path when the extension is recognized by the host.
/// 当扩展名被宿主识别时，为本地文件路径解析受支持的附件描述信息。
pub fn attachment_descriptor_from_path(path: &Path) -> Option<AttachmentDescriptor> {
    let extension = path.extension()?.to_str()?.trim().to_ascii_lowercase();
    attachment_descriptor_from_extension(extension.as_str())
}

/// Resolves the supported attachment descriptor for one MIME type when the host knows how to stage it.
/// 当宿主知道如何暂存该 MIME 类型时，为其解析受支持的附件描述信息。
pub fn attachment_descriptor_from_mime_type(mime_type: &str) -> Option<AttachmentDescriptor> {
    let normalized_mime_type = mime_type.trim().to_ascii_lowercase();
    match normalized_mime_type.as_str() {
        "image/png" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/png",
            extension: "png",
        }),
        "image/jpeg" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/jpeg",
            extension: "jpg",
        }),
        "image/webp" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/webp",
            extension: "webp",
        }),
        "image/gif" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/gif",
            extension: "gif",
        }),
        "image/bmp" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/bmp",
            extension: "bmp",
        }),
        "audio/mpeg" | "audio/mp3" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/mpeg",
            extension: "mp3",
        }),
        "audio/wav" | "audio/x-wav" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/wav",
            extension: "wav",
        }),
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/mp4",
            extension: "m4a",
        }),
        "audio/aac" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/aac",
            extension: "aac",
        }),
        "audio/flac" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/flac",
            extension: "flac",
        }),
        "audio/ogg" | "audio/oga" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/ogg",
            extension: "ogg",
        }),
        "audio/pcm" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/pcm",
            extension: "pcm",
        }),
        "audio/l16" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/L16",
            extension: "pcm",
        }),
        "audio/basic" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/basic",
            extension: "ulaw",
        }),
        "audio/opus" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/opus",
            extension: "opus",
        }),
        "video/mp4" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/mp4",
            extension: "mp4",
        }),
        "video/mpeg" | "video/mpg" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/mpeg",
            extension: "mpeg",
        }),
        "video/quicktime" | "video/mov" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/quicktime",
            extension: "mov",
        }),
        "video/x-m4v" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-m4v",
            extension: "m4v",
        }),
        "video/x-matroska" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-matroska",
            extension: "mkv",
        }),
        "video/webm" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/webm",
            extension: "webm",
        }),
        "video/x-msvideo" | "video/avi" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-msvideo",
            extension: "avi",
        }),
        "video/x-flv" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-flv",
            extension: "flv",
        }),
        "video/x-ms-wmv" | "video/wmv" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-ms-wmv",
            extension: "wmv",
        }),
        "video/3gpp" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/3gpp",
            extension: "3gp",
        }),
        "video/3gpp2" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/3gpp2",
            extension: "3g2",
        }),
        "application/pdf" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Pdf,
            mime_type: "application/pdf",
            extension: "pdf",
        }),
        "application/json" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::File,
            mime_type: "application/json",
            extension: "json",
        }),
        _ => crate::image_conversion::codec_for_mime(&normalized_mime_type).map(|codec| {
            AttachmentDescriptor {
                kind: ConversationAttachmentKind::Image,
                mime_type: codec.mime,
                extension: codec.extensions[0],
            }
        }),
    }
}

/// Returns default native MIME types for one media kind, excluding formats supported only by image conversion.
/// 返回一种媒体类型的默认原生MIME，不包含仅通过图片转换支持的格式。
///
/// # Parameters
/// 参数
/// `kind` is the canonical attachment modality whose closed host registry is requested.
/// `kind` 是需要查询封闭宿主注册表的规范附件模态。
///
/// # Returns
/// 返回值
/// A stable native default list; independent target constraints may select additional registered image formats.
/// 稳定的原生默认列表；独立目标约束可以选择额外已注册图片格式。
/// Non-media kinds return an empty list.
/// 非媒体类型返回空列表。
pub(crate) fn registered_media_attachment_mime_types(
    kind: ConversationAttachmentKind,
) -> &'static [&'static str] {
    match kind {
        ConversationAttachmentKind::Image => &[
            "image/png",
            "image/jpeg",
            "image/webp",
            "image/gif",
            "image/bmp",
        ],
        ConversationAttachmentKind::Audio => &[
            "audio/mpeg",
            "audio/wav",
            "audio/mp4",
            "audio/aac",
            "audio/flac",
            "audio/ogg",
            "audio/opus",
            "audio/pcm",
            "audio/basic",
        ],
        ConversationAttachmentKind::Video => &[
            "video/mp4",
            "video/mpeg",
            "video/quicktime",
            "video/x-m4v",
            "video/x-matroska",
            "video/webm",
            "video/x-msvideo",
            "video/x-flv",
            "video/x-ms-wmv",
            "video/3gpp",
            "video/3gpp2",
        ],
        ConversationAttachmentKind::Pdf | ConversationAttachmentKind::File => &[],
    }
}

/// Copies one supported source file into the managed attachment store and returns the normalized attachment payload used by future replays.
/// 将一份受支持的源文件复制到托管附件存储中，并返回未来回放使用的规范化附件载荷。
pub fn stage_attachment_file(
    workspace_path: &str,
    source_path: &Path,
) -> Result<ConversationAttachment, String> {
    let descriptor =
        attachment_descriptor_from_path(source_path).unwrap_or_else(generic_attachment_descriptor);
    let canonical_source_path = canonicalize_workspace_path(source_path);
    let file_bytes = fs::read(&canonical_source_path).map_err(|error| {
        format!(
            "Failed to read attachment file `{}`: {}.",
            canonical_source_path.display(),
            error
        )
    })?;
    let file_name = canonical_source_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            vulcan_runtime_assets::native_host_path_text_from_path(&canonical_source_path)
        });

    stage_attachment_bytes(
        workspace_path,
        &descriptor,
        &file_name,
        Some(canonical_source_path.as_path()),
        &file_bytes,
    )
}

/// Copies one composer-selected file while reusing an identical asset from the same project index.
/// 复制一个输入区选择的文件，并复用同一项目索引中的相同资产。
///
/// # Parameters
/// 参数
/// `workspace_path` identifies the project scope and `source_path` identifies the readable source file.
/// `workspace_path` 标识项目范围，`source_path` 标识可读取的源文件。
///
/// # Returns
/// 返回值
/// The canonical project attachment selected by SHA-256, byte length, and MIME type.
/// 返回按 SHA-256、字节长度与 MIME 类型选定的规范项目附件。
pub fn stage_project_attachment_file(
    workspace_path: &str,
    source_path: &Path,
) -> Result<ConversationAttachment, String> {
    // descriptor is determined from the exact source name before generic fallback.
    // descriptor 先依据精确源文件名确定，无法识别时再使用通用描述。
    let descriptor =
        attachment_descriptor_from_path(source_path).unwrap_or_else(generic_attachment_descriptor);
    // canonical_source_path is the exact readable source retained as diagnostics provenance.
    // canonical_source_path 是作为诊断来源保留的精确可读源路径。
    let canonical_source_path = canonicalize_workspace_path(source_path);
    // file_bytes supplies the immutable content fingerprint and managed copy.
    // file_bytes 提供不可变内容特征与托管副本。
    let file_bytes = fs::read(&canonical_source_path).map_err(|error| {
        format!(
            "Failed to read attachment file `{}`: {}.",
            canonical_source_path.display(),
            error
        )
    })?;
    // file_name retains the exact source display name before host sanitization.
    // file_name 在宿主清理前保留精确源展示名称。
    let file_name = canonical_source_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            vulcan_runtime_assets::native_host_path_text_from_path(&canonical_source_path)
        });

    stage_project_attachment_bytes(
        workspace_path,
        &descriptor,
        &file_name,
        Some(canonical_source_path.as_path()),
        &file_bytes,
    )
}

/// Streams one already described source file into the managed attachment store without buffering its contents.
/// 将一个已有描述的源文件流式复制到托管附件存储，避免把文件内容整体缓冲到内存。
///
/// # Parameters
/// 参数
/// `workspace_path` selects the managed attachment root.
/// `workspace_path` 选择托管附件根目录。
/// `descriptor` provides the host-controlled attachment kind and MIME metadata.
/// `descriptor` 提供宿主控制的附件类型与 MIME 元数据。
/// `suggested_file_name` is normalized before the managed path is created.
/// `suggested_file_name` 会在创建托管路径前进行规范化。
/// `source_path` is the complete source file that will be copied.
/// `source_path` 是将被复制的完整源文件。
///
/// # Returns
/// 返回值
/// A persisted attachment descriptor whose managed file is fully written and indexed.
/// 返回已完整写入并建立索引的托管附件描述。
pub fn stage_attachment_file_streaming(
    workspace_path: &str,
    descriptor: &AttachmentDescriptor,
    suggested_file_name: &str,
    source_path: &Path,
) -> Result<ConversationAttachment, String> {
    let canonical_source_path = canonicalize_workspace_path(source_path);
    let source_metadata = fs::metadata(&canonical_source_path).map_err(|error| {
        format!(
            "Failed to read streaming attachment source `{}`: {error}",
            canonical_source_path.display()
        )
    })?;
    if !source_metadata.is_file() {
        return Err(format!(
            "Streaming attachment source path `{}` is not a file.",
            canonical_source_path.display()
        ));
    }
    let timestamp = current_attachment_timestamp_parts();
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let category_directory = resolve_attachment_category_directory(&asset_root, &descriptor.kind);
    fs::create_dir_all(&category_directory).map_err(|error| {
        format!(
            "Failed to create managed attachment category directory `{}`: {error}",
            category_directory.display()
        )
    })?;
    let asset_id = Uuid::new_v4().to_string();
    let sanitized_file_name = if descriptor.kind == ConversationAttachmentKind::File
        && descriptor.mime_type == "application/octet-stream"
    {
        sanitize_generic_attachment_file_name(suggested_file_name)
    } else {
        sanitize_attachment_file_name(suggested_file_name, descriptor.extension)
    };
    let managed_file_path = category_directory.join(format!(
        "{}-{asset_id}-{}",
        timestamp.compact_prefix, sanitized_file_name
    ));
    let copied_bytes = match fs::copy(&canonical_source_path, &managed_file_path) {
        Ok(copied_bytes) => copied_bytes,
        Err(error) => {
            return Err(rollback_failed_managed_attachment(
                &managed_file_path,
                format!(
                    "Failed to stream-copy attachment `{}` to `{}`: {error}",
                    canonical_source_path.display(),
                    managed_file_path.display()
                ),
            ));
        }
    };
    if copied_bytes != source_metadata.len() {
        return Err(rollback_failed_managed_attachment(
            &managed_file_path,
            format!(
                "Byte count mismatch after stream-copying attachment `{}` to `{}`: expected {}, got {}.",
                canonical_source_path.display(),
                managed_file_path.display(),
                source_metadata.len(),
                copied_bytes
            ),
        ));
    }
    if let Err(error) = set_managed_attachment_file_times(&managed_file_path, timestamp.unix_millis)
    {
        return Err(rollback_failed_managed_attachment(
            &managed_file_path,
            error,
        ));
    }
    // Every fallible value is resolved before the record exists, so a failure removes the copy.
    // 每个可能失败的值都在记录建立前解析，失败时删除副本。
    let resource_path = match managed_attachment_resource_path(workspace_path, &managed_file_path)
        .and_then(|resource_path| {
            streaming_file_sha256(&managed_file_path).map(|sha256| (resource_path, sha256))
        }) {
        Ok(resolved) => resolved,
        Err(error) => {
            return Err(rollback_failed_managed_attachment(
                &managed_file_path,
                error,
            ));
        }
    };
    let (resource_path, source_sha256) = resource_path;
    let attachment = ConversationAttachment {
        kind: descriptor.kind.clone(),
        file_name: sanitized_file_name,
        local_path: vulcan_runtime_assets::native_host_path_text_from_path(&managed_file_path),
        resource_path: Some(resource_path),
        source_path: None,
        asset_id: Some(asset_id),
        byte_len: Some(source_metadata.len()),
        source_sha256: Some(source_sha256),
        delivery_mode: None,
        preview_local_path: None,
        mime_type: descriptor.mime_type.to_string(),
    };
    if let Err(error) = record_staged_attachment(workspace_path, &attachment, &timestamp) {
        return Err(rollback_failed_managed_attachment(
            &managed_file_path,
            error,
        ));
    }
    Ok(attachment)
}

/// Materializes one in-memory attachment payload into the managed attachment store and returns the normalized replay descriptor.
/// 将一份内存中的附件载荷物化到托管附件存储中，并返回规范化回放描述信息。
pub fn stage_attachment_bytes(
    workspace_path: &str,
    descriptor: &AttachmentDescriptor,
    suggested_file_name: &str,
    source_path: Option<&Path>,
    bytes: &[u8],
) -> Result<ConversationAttachment, String> {
    stage_attachment_bytes_with_effective_mime_type(
        workspace_path,
        descriptor,
        suggested_file_name,
        source_path,
        descriptor.mime_type,
        bytes,
        false,
    )
}

/// Materializes composer bytes while reusing an identical asset from the same project index.
/// 物化输入区字节，并复用同一项目索引中的相同资产。
///
/// # Parameters
/// 参数
/// `workspace_path` identifies the project, `descriptor` owns host MIME semantics,
/// `suggested_file_name` is display metadata, `source_path` is optional provenance, and `bytes`
/// are the immutable payload.
/// `workspace_path` 标识项目，`descriptor` 持有宿主 MIME 语义，`suggested_file_name` 是展示元数据，
/// `source_path` 是可选来源，`bytes` 是不可变载荷。
///
/// # Returns
/// 返回值
/// The existing or newly published canonical attachment for the exact project fingerprint.
/// 返回精确项目特征对应的既有或新发布规范附件。
pub fn stage_project_attachment_bytes(
    workspace_path: &str,
    descriptor: &AttachmentDescriptor,
    suggested_file_name: &str,
    source_path: Option<&Path>,
    bytes: &[u8],
) -> Result<ConversationAttachment, String> {
    stage_attachment_bytes_with_effective_mime_type(
        workspace_path,
        descriptor,
        suggested_file_name,
        source_path,
        descriptor.mime_type,
        bytes,
        true,
    )
}

/// Materializes one generic browser attachment while preserving its validated concrete MIME metadata.
/// 物化一份通用浏览器附件，同时保留其经过校验的具体 MIME 元数据。
///
/// # Parameters
/// 参数
/// `workspace_path` owns the managed store, `suggested_file_name` is display metadata,
/// `source_path` is optional diagnostics provenance, `mime_type` is browser-provided metadata,
/// and `bytes` are the immutable source bytes.
/// `workspace_path` 持有托管存储，`suggested_file_name` 是展示元数据，`source_path` 是可选诊断来源，
/// `mime_type` 是浏览器提供的元数据，`bytes` 是不可变源字节。
///
/// # Returns
/// 返回值
/// A canonical generic attachment with a validated MIME type, or one explicit validation/staging error.
/// 返回带有已校验 MIME 类型的规范通用附件，或一个明确的校验/暂存错误。
pub fn stage_generic_attachment_bytes(
    workspace_path: &str,
    suggested_file_name: &str,
    source_path: Option<&Path>,
    mime_type: &str,
    bytes: &[u8],
) -> Result<ConversationAttachment, String> {
    let effective_mime_type = normalize_generic_attachment_mime_type(mime_type)?;
    let descriptor = generic_attachment_descriptor_for_mime_type(&effective_mime_type);
    stage_attachment_bytes_with_effective_mime_type(
        workspace_path,
        &descriptor,
        suggested_file_name,
        source_path,
        effective_mime_type.as_str(),
        bytes,
        false,
    )
}

/// Materializes generic composer bytes with exact project-scoped fingerprint reuse.
/// 使用精确项目级内容特征复用来物化通用输入区字节。
///
/// # Parameters
/// 参数
/// `workspace_path` identifies the project, `suggested_file_name` is display metadata,
/// `source_path` is optional provenance, `mime_type` is validated browser metadata, and `bytes`
/// are the immutable payload.
/// `workspace_path` 标识项目，`suggested_file_name` 是展示元数据，`source_path` 是可选来源，
/// `mime_type` 是已校验的浏览器元数据，`bytes` 是不可变载荷。
///
/// # Returns
/// 返回值
/// The existing or newly published canonical generic attachment for the exact project fingerprint.
/// 返回精确项目特征对应的既有或新发布规范通用附件。
pub fn stage_project_generic_attachment_bytes(
    workspace_path: &str,
    suggested_file_name: &str,
    source_path: Option<&Path>,
    mime_type: &str,
    bytes: &[u8],
) -> Result<ConversationAttachment, String> {
    // effective_mime_type is normalized once before it participates in the deduplication key.
    // effective_mime_type 在参与排重键前统一规范化一次。
    let effective_mime_type = normalize_generic_attachment_mime_type(mime_type)?;
    // descriptor preserves the exact generic modality without claiming a registered file format.
    // descriptor 保留精确通用模态，但不声称已注册文件格式。
    let descriptor = generic_attachment_descriptor_for_mime_type(effective_mime_type.as_str());
    stage_attachment_bytes_with_effective_mime_type(
        workspace_path,
        &descriptor,
        suggested_file_name,
        source_path,
        effective_mime_type.as_str(),
        bytes,
        true,
    )
}

/// Implements immutable byte staging with one already validated effective MIME type.
/// 使用一个已经校验的有效 MIME 类型实现不可变字节暂存。
///
/// # Parameters
/// 参数
/// All fields are the exact canonical staging inputs; `effective_mime_type` is persisted in the asset index.
/// 全部字段均为精确规范暂存输入；`effective_mime_type` 会持久化到资产索引。
///
/// # Returns
/// 返回值
/// The indexed canonical attachment or one explicit staging error.
/// 返回已建立索引的规范附件，或一个明确的暂存错误。
fn stage_attachment_bytes_with_effective_mime_type(
    workspace_path: &str,
    descriptor: &AttachmentDescriptor,
    suggested_file_name: &str,
    source_path: Option<&Path>,
    effective_mime_type: &str,
    bytes: &[u8],
    reuse_project_asset: bool,
) -> Result<ConversationAttachment, String> {
    let timestamp = current_attachment_timestamp_parts();
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let category_directory = resolve_attachment_category_directory(&asset_root, &descriptor.kind);
    fs::create_dir_all(&category_directory).map_err(|error| {
        format!(
            "Failed to create managed attachment category directory `{}`: {}.",
            category_directory.display(),
            error
        )
    })?;

    let asset_id = Uuid::new_v4().to_string();
    let sanitized_file_name =
        if descriptor.mime_type == "application/octet-stream" && descriptor.extension == "bin" {
            sanitize_generic_attachment_file_name(suggested_file_name)
        } else {
            sanitize_attachment_file_name(suggested_file_name, descriptor.extension)
        };
    let managed_file_path = category_directory.join(format!(
        "{}-{asset_id}-{}",
        timestamp.compact_prefix, sanitized_file_name
    ));
    fs::write(&managed_file_path, bytes).map_err(|error| {
        format!(
            "Failed to write managed attachment `{}`: {}.",
            managed_file_path.display(),
            error
        )
    })?;

    if let Err(error) = set_managed_attachment_file_times(&managed_file_path, timestamp.unix_millis)
    {
        return Err(rollback_failed_managed_attachment(
            &managed_file_path,
            error,
        ));
    }
    // The resource path is resolved before the record exists, so a failure removes the file.
    // 资源路径在记录建立前解析，失败时删除文件。
    let resource_path = match managed_attachment_resource_path(workspace_path, &managed_file_path) {
        Ok(resource_path) => resource_path,
        Err(error) => {
            return Err(rollback_failed_managed_attachment(
                &managed_file_path,
                error,
            ));
        }
    };
    let attachment = ConversationAttachment {
        kind: descriptor.kind.clone(),
        file_name: sanitized_file_name,
        local_path: vulcan_runtime_assets::native_host_path_text_from_path(&managed_file_path),
        resource_path: Some(resource_path),
        source_path: source_path
            .map(canonicalize_workspace_path)
            .map(|path| normalized_workspace_path_text_from_path(&path)),
        asset_id: Some(asset_id),
        byte_len: Some(bytes.len() as u64),
        source_sha256: Some(hex::encode(Sha256::digest(bytes))),
        delivery_mode: None,
        preview_local_path: None,
        mime_type: effective_mime_type.to_string(),
    };
    // indexed_attachment is either this candidate or the existing project asset selected atomically.
    // indexed_attachment 是当前候选，或原子选定的既有项目资产。
    let indexed_attachment = if reuse_project_asset {
        record_or_reuse_project_attachment(workspace_path, &attachment, &timestamp)
    } else {
        record_staged_attachment(workspace_path, &attachment, &timestamp)
            .map(|()| attachment.clone())
    };
    let indexed_attachment = match indexed_attachment {
        Ok(indexed_attachment) => indexed_attachment,
        Err(error) => {
            return Err(rollback_failed_managed_attachment(
                &managed_file_path,
                error,
            ));
        }
    };
    if indexed_attachment.asset_id != attachment.asset_id {
        // The winning record already owns its managed bytes; this unindexed duplicate must disappear.
        // 获胜记录已经持有托管字节；当前未入索引的重复副本必须删除。
        fs::remove_file(&managed_file_path).map_err(|error| {
            format!(
                "Failed to delete duplicate managed attachment `{}`: {}.",
                managed_file_path.display(),
                error
            )
        })?;
    }
    Ok(indexed_attachment)
}

/// Normalizes one concrete generic-file MIME type without accepting parameters or invalid tokens.
/// 规范化一个具体通用文件 MIME 类型，且不接受参数或无效 Token。
///
/// # Parameters
/// 参数
/// `mime_type` is browser-provided file metadata; an empty value means unknown content.
/// `mime_type` 是浏览器提供的文件元数据；空值表示内容类型未知。
///
/// # Returns
/// 返回值
/// A lowercase concrete MIME type, the octet-stream fallback for empty input, or an explicit error.
/// 返回小写具体 MIME 类型、空输入对应的八位流回退，或一个明确错误。
/// Normalizes browser-supplied `mime_type` for staging or metadata-only admission.
/// 为暂存或仅元数据准入规范化浏览器提供的 `mime_type`。
/// Returns the canonical MIME string or an error for invalid metadata.
/// 返回规范 MIME 字符串，元数据无效时返回错误。
pub fn normalize_generic_attachment_mime_type(mime_type: &str) -> Result<String, String> {
    let normalized = mime_type.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(String::from("application/octet-stream"));
    }
    let Some((type_name, subtype_name)) = normalized.split_once('/') else {
        return Err(String::from("Generic attachment MIME type is invalid."));
    };
    if type_name.is_empty()
        || subtype_name.is_empty()
        || subtype_name.contains('/')
        || !type_name.chars().all(is_mime_token_character)
        || !subtype_name.chars().all(is_mime_token_character)
    {
        return Err(String::from("Generic attachment MIME type is invalid."));
    }
    Ok(normalized)
}

/// Returns whether one ASCII character is valid inside a concrete MIME token.
/// 返回一个 ASCII 字符是否可用于具体 MIME Token。
///
/// # Parameters
/// 参数
/// `character` is one character from the type or subtype component.
/// `character` 是类型或子类型分量中的一个字符。
///
/// # Returns
/// 返回值
/// `true` for RFC token characters accepted by this closed metadata boundary.
/// 对该封闭元数据边界接受的 RFC Token 字符返回 `true`。
fn is_mime_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(
            character,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

/// Builds the project-relative resource path used by remote models and resource-aware host tools.
/// 构建供远程模型与资源感知宿主工具使用的项目相对资源路径。
pub(crate) fn managed_attachment_resource_path(
    workspace_path: &str,
    managed_file_path: &Path,
) -> Result<String, String> {
    // Both paths are canonicalized before containment checks so Windows extended-path prefixes cannot diverge.
    // 两条路径在边界检查前都完成规范化，避免 Windows 扩展路径前缀产生差异。
    let attachment_root =
        canonicalize_workspace_path(&resolve_managed_attachment_root(workspace_path));
    let managed_file_path = canonicalize_workspace_path(managed_file_path);
    let relative_path = managed_file_path
        .strip_prefix(&attachment_root)
        .map_err(|_| {
            format!(
                "Managed attachment `{}` is outside its authoritative root `{}`.",
                managed_file_path.display(),
                attachment_root.display()
            )
        })?;
    let relative_text = normalized_workspace_path_text_from_path(relative_path).replace('\\', "/");
    Ok(format!(".vulcan/code/attachments/{relative_text}"))
}

/// Computes the SHA-256 digest of one local attachment source without retaining it in memory.
/// 在不把完整内容保留在内存中的情况下计算一个本地附件来源的 SHA-256 摘要。
pub(crate) fn streaming_file_sha256(path: &Path) -> Result<String, String> {
    use std::io::Read;

    let mut file = fs::File::open(path).map_err(|error| {
        format!(
            "Failed to open local attachment source `{}`: {error}",
            path.display()
        )
    })?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            format!(
                "Failed to read local attachment source `{}`: {error}",
                path.display()
            )
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

/// Sanitizes an unknown file name while preserving its non-executable extension as metadata.
/// 清洗未知文件名，同时保留其作为元数据使用的非执行扩展名。
fn sanitize_generic_attachment_file_name(file_name: &str) -> String {
    let trimmed = file_name.trim();
    let raw_name = Path::new(trimmed)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("attachment.bin");
    let normalized = raw_name
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    if normalized.trim_matches(['.', ' ']).is_empty() {
        String::from("attachment.bin")
    } else {
        normalized
    }
}

/// Removes one managed attachment file after post-write metadata or index publication fails.
/// 在写入后的元数据或索引发布失败时删除一个托管附件文件。
///
/// # Parameters
/// 参数
/// `managed_file_path` is the newly written file that must not remain unindexed.
/// `managed_file_path` 是不得以未索引状态遗留的新写入文件。
/// `primary_error` is the authoritative staging failure returned by the failed operation.
/// `primary_error` 是失败操作返回的权威暂存错误。
///
/// # Returns
/// 返回值
/// The primary error, enriched with rollback failure details when cleanup also fails.
/// 返回首要错误；若清理也失败则附加回滚失败详情。
fn rollback_failed_managed_attachment(managed_file_path: &Path, primary_error: String) -> String {
    match fs::remove_file(managed_file_path) {
        Ok(()) => primary_error,
        Err(rollback_error) => format!(
            "{primary_error} Rolling back unindexed managed attachment `{}` also failed: {}.",
            managed_file_path.display(),
            rollback_error
        ),
    }
}

/// Materializes one in-memory attachment payload into one caller-selected directory while keeping host-controlled extension normalization and collision avoidance.
/// 将一份内存中的附件载荷物化到调用方选定的目录，同时保持宿主控制的扩展名规范化与重名避让。
pub fn write_attachment_bytes_to_directory(
    descriptor: &AttachmentDescriptor,
    target_directory: &Path,
    suggested_file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    // Canonical target directory used for filesystem writes when the directory already exists.
    // 当目标目录已经存在时，用于文件系统写入的规范目标目录。
    let canonical_target_directory = canonicalize_workspace_path(target_directory);
    fs::create_dir_all(&canonical_target_directory).map_err(|error| {
        format!(
            "Failed to create export attachment directory `{}`: {}.",
            canonical_target_directory.display(),
            error
        )
    })?;

    // Host-normalized file name whose extension is derived from the attachment descriptor instead of caller text.
    // 扩展名由附件描述符而非调用方文本决定的宿主规范化文件名。
    let normalized_file_name = normalize_attachment_file_name(suggested_file_name, descriptor);
    // Collision-safe output path chosen inside the caller-selected directory.
    // 在调用方选定目录内部挑选出的安全避重输出路径。
    let output_path =
        resolve_unique_attachment_output_path(&canonical_target_directory, &normalized_file_name);
    fs::write(&output_path, bytes).map_err(|error| {
        format!(
            "Failed to write exported attachment `{}`: {}.",
            output_path.display(),
            error
        )
    })?;
    Ok(output_path)
}

/// Encodes one RGBA image payload into PNG bytes so clipboard images can enter the managed attachment store through the same staging pipeline.
/// 将一份 RGBA 图片载荷编码为 PNG 字节，从而让剪贴板图片也能通过同一条暂存管线进入托管附件存储。
pub fn encode_rgba_image_to_png_bytes(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<Vec<u8>, String> {
    let expected_len = width as usize * height as usize * 4usize;
    if rgba.len() != expected_len {
        return Err(format!(
            "Invalid clipboard image pixel data length: expected {expected_len} bytes, got {} bytes.",
            rgba.len()
        ));
    }

    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("Failed to create the PNG header: {error}."))?;
        writer
            .write_image_data(rgba)
            .map_err(|error| format!("Failed to write PNG image data: {error}."))?;
    }

    Ok(output)
}

/// Returns the first readable replay path for one attachment by preferring the managed copy and then falling back to the original source path.
/// 为单条附件返回第一条可读回放路径，优先使用托管副本，再回退到原始来源路径。
pub fn resolve_attachment_replay_path(attachment: &ConversationAttachment) -> Option<PathBuf> {
    match resolve_attachment_replay_status(attachment) {
        AttachmentReplayStatus::Available => Some(PathBuf::from(attachment.local_path.trim())),
        AttachmentReplayStatus::FallbackSourceOnly => attachment
            .source_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        AttachmentReplayStatus::Missing | AttachmentReplayStatus::Purged => None,
    }
}

/// Writes one bounded JPEG preview beside an integrity-verified managed image without creating a second indexed attachment.
/// 在通过完整性校验的托管图片旁写入一份受限 JPEG 预览，且不创建第二条索引附件。
///
/// # Parameters
/// 参数
/// `attachment` is the authoritative managed image and `bytes` is the browser-derived full-frame JPEG payload.
/// `attachment` 是权威托管图片，`bytes` 是浏览器派生的完整画面 JPEG 载荷。
///
/// # Returns
/// 返回值
/// The native host path of the deterministic sidecar or one explicit validation/write error.
/// 返回确定性旁路文件的宿主原生路径，或一个明确的校验/写入错误。
pub fn write_attachment_image_preview_sidecar(
    attachment: &ConversationAttachment,
    bytes: &[u8],
) -> Result<String, String> {
    if attachment.kind != ConversationAttachmentKind::Image {
        return Err(String::from(
            "Attachment preview sidecars can only be written for managed images.",
        ));
    }
    if bytes.is_empty() || bytes.len() > IMAGE_PREVIEW_SIDECAR_MAX_BYTES {
        return Err(format!(
            "Attachment image preview must contain 1..={IMAGE_PREVIEW_SIDECAR_MAX_BYTES} bytes."
        ));
    }
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err(String::from(
            "Attachment image preview must use the frozen JPEG format.",
        ));
    }
    let managed_path = Path::new(attachment.local_path.trim());
    if !managed_path.is_file() {
        return Err(String::from(
            "Managed attachment image is unavailable for preview binding.",
        ));
    }
    let preview_path = attachment_preview_sidecar_path(attachment, "jpg")?;
    fs::write(&preview_path, bytes).map_err(|error| {
        format!(
            "Failed to write attachment image preview `{}`: {error}",
            preview_path.display()
        )
    })?;
    Ok(vulcan_runtime_assets::native_host_path_text_from_path(
        &preview_path,
    ))
}

/// Resolves one valid deterministic JPEG preview sidecar for an authoritative managed image.
/// 为一条权威托管图片解析一份有效的确定性 JPEG 预览旁路文件。
///
/// # Parameters
/// 参数
/// `attachment` is the already integrity-verified canonical image attachment.
/// `attachment` 是已经通过完整性校验的规范图片附件。
///
/// # Returns
/// 返回值
/// The native host path when the bounded JPEG sidecar exists and is valid; otherwise `None`.
/// 当受限 JPEG 旁路文件存在且有效时返回宿主原生路径；否则返回 `None`。
pub fn resolve_attachment_image_preview_sidecar(
    attachment: &ConversationAttachment,
) -> Option<String> {
    use std::io::Read;

    if attachment.kind != ConversationAttachmentKind::Image {
        return None;
    }
    let preview_path = attachment_preview_sidecar_path(attachment, "jpg").ok()?;
    let metadata = fs::metadata(&preview_path).ok()?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > IMAGE_PREVIEW_SIDECAR_MAX_BYTES as u64
    {
        return None;
    }
    let mut file = fs::File::open(&preview_path).ok()?;
    let mut header = [0_u8; 3];
    file.read_exact(&mut header).ok()?;
    if header != [0xff, 0xd8, 0xff] {
        return None;
    }
    Some(vulcan_runtime_assets::native_host_path_text_from_path(
        &preview_path,
    ))
}

/// Removes any deterministic managed preview sidecar associated with the provided managed attachment path.
/// 移除与给定托管附件路径关联的任意确定性托管预览旁路文件。
pub fn remove_attachment_preview_sidecar(attachment: &ConversationAttachment) {
    let extension = match attachment.kind {
        ConversationAttachmentKind::Image => "jpg",
        ConversationAttachmentKind::Video => "png",
        _ => return,
    };
    if let Ok(preview_path) = attachment_preview_sidecar_path(attachment, extension)
        && preview_path.is_file()
    {
        let _ = fs::remove_file(preview_path);
    }
}

/// Derives the deterministic preview sidecar path from one managed attachment path.
/// 根据一条托管附件路径推导确定性的预览旁路文件路径。
///
/// # Parameters
/// 参数
/// `attachment` owns the managed path and `extension` is the frozen derived-media extension.
/// `attachment` 持有托管路径，`extension` 是冻结的派生媒体扩展名。
///
/// # Returns
/// 返回值
/// The adjacent sidecar path or one explicit invalid-path error.
/// 返回相邻旁路文件路径，或一个明确的无效路径错误。
fn attachment_preview_sidecar_path(
    attachment: &ConversationAttachment,
    extension: &str,
) -> Result<PathBuf, String> {
    let managed_path = Path::new(attachment.local_path.trim());
    let parent = managed_path.parent().ok_or_else(|| {
        String::from("Managed attachment path has no parent for a preview sidecar.")
    })?;
    let stem = managed_path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Managed attachment path has no valid file stem."))?;
    Ok(parent.join(format!("{stem}.preview.{extension}")))
}

/// Returns the stable category directory path used by the provided attachment kind below the managed attachment root.
/// 返回在托管附件根目录下由给定附件类型使用的稳定分类目录路径。
fn resolve_attachment_category_directory(
    asset_root: &Path,
    kind: &ConversationAttachmentKind,
) -> PathBuf {
    asset_root.join(attachment_kind_directory_name(kind))
}

/// Returns the stable directory name assigned to the provided attachment kind.
/// 返回分配给给定附件类型的稳定目录名称。
fn attachment_kind_directory_name(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => IMAGE_ATTACHMENT_DIR_NAME,
        ConversationAttachmentKind::Audio => AUDIO_ATTACHMENT_DIR_NAME,
        ConversationAttachmentKind::Video => VIDEO_ATTACHMENT_DIR_NAME,
        ConversationAttachmentKind::Pdf => PDF_ATTACHMENT_DIR_NAME,
        ConversationAttachmentKind::File => FILE_ATTACHMENT_DIR_NAME,
    }
}

/// Applies the current staging timestamp to the managed file-system entry so later cleanup and manual inspection both see consistent times.
/// 将当前暂存时间应用到托管文件系统条目上，从而让后续清理与人工排查都看到一致的时间。
fn set_managed_attachment_file_times(
    managed_file_path: &Path,
    unix_millis: i64,
) -> Result<(), String> {
    let seconds = unix_millis.div_euclid(1_000);
    let nanoseconds = (unix_millis.rem_euclid(1_000) * 1_000_000) as u32;
    let timestamp = FileTime::from_unix_time(seconds, nanoseconds);
    set_file_times(managed_file_path, timestamp, timestamp).map_err(|error| {
        format!(
            "Failed to update access/modification times of managed attachment `{}`: {}.",
            managed_file_path.display(),
            error
        )
    })?;
    set_managed_attachment_creation_time(managed_file_path, unix_millis)?;
    Ok(())
}

/// Applies the current staging timestamp to the managed file creation time when the host platform supports it.
/// 当宿主平台支持时，将当前暂存时间应用到托管文件创建时间。
#[cfg(windows)]
fn set_managed_attachment_creation_time(
    managed_file_path: &Path,
    unix_millis: i64,
) -> Result<(), String> {
    use std::fs::FileTimes;
    use std::os::windows::fs::FileTimesExt;

    let file = fs::OpenOptions::new()
        .write(true)
        .open(managed_file_path)
        .map_err(|error| {
            format!(
                "Failed to open managed attachment `{}` to update its creation time: {}.",
                managed_file_path.display(),
                error
            )
        })?;
    let file_times = FileTimes::new().set_created(unix_millis_to_system_time(unix_millis));
    file.set_times(file_times).map_err(|error| {
        format!(
            "Failed to update the creation time of managed attachment `{}`: {}.",
            managed_file_path.display(),
            error
        )
    })
}

/// Converts a Unix millisecond timestamp into a `SystemTime` value accepted by the safe filesystem timestamp API.
/// 将 Unix 毫秒时间戳转换为安全文件系统时间戳 API 可接受的 `SystemTime` 值。
#[cfg(windows)]
fn unix_millis_to_system_time(unix_millis: i64) -> std::time::SystemTime {
    let millis = std::time::Duration::from_millis(unix_millis.unsigned_abs());
    if unix_millis >= 0 {
        std::time::UNIX_EPOCH + millis
    } else {
        std::time::UNIX_EPOCH - millis
    }
}

/// Skips explicit creation-time updates on non-Windows hosts because only access/modify times are portable there.
/// 在非 Windows 宿主上跳过显式创建时间更新，因为只有访问/修改时间具备可移植性。
#[cfg(not(windows))]
fn set_managed_attachment_creation_time(
    _managed_file_path: &Path,
    _unix_millis: i64,
) -> Result<(), String> {
    Ok(())
}

/// Resolves one supported attachment descriptor from a normalized extension string.
/// 根据规范化扩展名解析一份受支持的附件描述信息。
fn attachment_descriptor_from_extension(extension: &str) -> Option<AttachmentDescriptor> {
    match extension {
        "png" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/png",
            extension: "png",
        }),
        "jpg" | "jpeg" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/jpeg",
            extension: "jpg",
        }),
        "webp" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/webp",
            extension: "webp",
        }),
        "gif" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/gif",
            extension: "gif",
        }),
        "bmp" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/bmp",
            extension: "bmp",
        }),
        "mp3" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/mpeg",
            extension: "mp3",
        }),
        "wav" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/wav",
            extension: "wav",
        }),
        "m4a" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/mp4",
            extension: "m4a",
        }),
        "aac" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/aac",
            extension: "aac",
        }),
        "flac" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/flac",
            extension: "flac",
        }),
        "ogg" | "oga" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/ogg",
            extension: "ogg",
        }),
        "opus" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/opus",
            extension: "opus",
        }),
        "pcm" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/pcm",
            extension: "pcm",
        }),
        "ulaw" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/basic",
            extension: "ulaw",
        }),
        "mp4" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/mp4",
            extension: "mp4",
        }),
        "mpeg" | "mpg" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/mpeg",
            extension: "mpeg",
        }),
        "mov" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/quicktime",
            extension: "mov",
        }),
        "m4v" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-m4v",
            extension: "m4v",
        }),
        "mkv" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-matroska",
            extension: "mkv",
        }),
        "webm" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/webm",
            extension: "webm",
        }),
        "avi" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-msvideo",
            extension: "avi",
        }),
        "flv" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-flv",
            extension: "flv",
        }),
        "wmv" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-ms-wmv",
            extension: "wmv",
        }),
        "3gp" | "3gpp" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/3gpp",
            extension: "3gp",
        }),
        "3g2" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/3gpp2",
            extension: "3g2",
        }),
        "pdf" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Pdf,
            mime_type: "application/pdf",
            extension: "pdf",
        }),
        _ => crate::image_conversion::CODECS
            .iter()
            .find(|codec| codec.extensions.contains(&extension))
            .map(|codec| AttachmentDescriptor {
                kind: ConversationAttachmentKind::Image,
                mime_type: codec.mime,
                extension: codec.extensions[0],
            }),
    }
}

/// Normalizes one host-visible attachment file name while forcing the final extension chosen by the host descriptor.
/// 规范化一条宿主可见附件文件名，并强制使用宿主描述符选定的最终扩展名。
pub fn normalize_attachment_file_name(
    file_name: &str,
    descriptor: &AttachmentDescriptor,
) -> String {
    sanitize_attachment_file_name(file_name, descriptor.extension)
}

/// Sanitizes one host-visible attachment file name while forcing the final extension chosen by the host.
/// 对一条宿主可见附件文件名做清洗，并强制使用宿主选定的最终扩展名。
fn sanitize_attachment_file_name(file_name: &str, fallback_extension: &str) -> String {
    // Trimmed caller-supplied file name or stem candidate.
    // 调用方提供的裁剪后文件名或名称主体候选值。
    let trimmed_file_name = file_name.trim();
    // Raw stem text extracted from the caller input before host-side sanitization.
    // 在宿主侧清洗前从调用方输入中提取出的原始名称主体文本。
    let raw_stem = Path::new(trimmed_file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            (!trimmed_file_name.is_empty())
                .then_some(trimmed_file_name.trim_matches(['.', ' ']).trim())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("attachment");
    // Sanitized file-stem text with invalid path characters rewritten to underscores.
    // 将非法路径字符改写为下划线后的清洗名称主体文本。
    let normalized_stem = raw_stem
        .chars()
        .map(|character| {
            if matches!(
                character,
                '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    // Windows-safe file-stem text with empty and reserved-name cases normalized away.
    // 已对空值与 Windows 保留名情况完成规范化处理的安全名称主体文本。
    let safe_stem = finalize_attachment_file_stem(&normalized_stem);
    format!("{safe_stem}.{fallback_extension}")
}

/// Finalizes one sanitized attachment file stem so empty or Windows-reserved names stop leaking into real files.
/// 对清洗后的附件名称主体做最终规范化，避免空名称或 Windows 保留名泄露到真实文件。
fn finalize_attachment_file_stem(stem: &str) -> String {
    // Trimmed stem text without leading or trailing dots that Windows would reject.
    // 移除 Windows 不接受的首尾点号后的裁剪名称主体文本。
    let trimmed_stem = stem.trim().trim_matches('.');
    if trimmed_stem.is_empty() {
        return String::from("attachment");
    }
    if is_windows_reserved_file_stem(trimmed_stem) {
        return format!("{trimmed_stem}_");
    }
    trimmed_stem.to_string()
}

/// Returns whether one file stem matches the Windows reserved device-name set and therefore cannot be used directly.
/// 返回一个文件名主体是否命中 Windows 保留设备名集合，因此不能被直接使用。
fn is_windows_reserved_file_stem(stem: &str) -> bool {
    matches!(
        stem.trim().to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Resolves one collision-safe output path below the caller-selected directory by appending numbered suffixes when needed.
/// 在调用方选定目录下解析一条安全避重输出路径，必要时追加编号后缀。
fn resolve_unique_attachment_output_path(directory: &Path, file_name: &str) -> PathBuf {
    // First-choice output path using the host-normalized file name without any collision suffix.
    // 使用宿主规范化文件名且不带避重后缀的首选输出路径。
    let preferred_path = directory.join(file_name);
    if !preferred_path.exists() {
        return preferred_path;
    }

    // Base stem reused while probing numbered collision suffixes.
    // 在探测编号避重后缀期间复用的基础名称主体。
    let base_stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment");
    // Stable extension reused while probing numbered collision suffixes.
    // 在探测编号避重后缀期间复用的稳定扩展名。
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty());

    for suffix_index in 1usize.. {
        // Candidate collision-safe file name carrying one numbered suffix.
        // 带有编号后缀的一条候选避重文件名。
        let suffixed_file_name = if let Some(extension) = extension {
            format!("{base_stem}-{suffix_index:03}.{extension}")
        } else {
            format!("{base_stem}-{suffix_index:03}")
        };
        // Candidate collision-safe output path carrying one numbered suffix.
        // 带有编号后缀的一条候选避重输出路径。
        let candidate_path = directory.join(suffixed_file_name);
        if !candidate_path.exists() {
            return candidate_path;
        }
    }

    unreachable!("attachment output path probing should always find a free numbered suffix")
}

#[cfg(test)]
mod tests {
    //! Attachment-asset regression tests covering staging, replay fallback, and MIME resolution.
    //! 覆盖暂存、回放回退与 MIME 解析的附件资产回归测试。

    use super::{
        IMAGE_PREVIEW_SIDECAR_MAX_BYTES, attachment_descriptor_from_mime_type,
        attachment_descriptor_from_path, encode_rgba_image_to_png_bytes,
        generic_attachment_descriptor, normalize_attachment_file_name,
        remove_attachment_preview_sidecar, resolve_attachment_replay_path,
        resolve_managed_attachment_root, stage_attachment_bytes, stage_attachment_file,
        stage_generic_attachment_bytes, stage_project_attachment_bytes,
        stage_project_generic_attachment_bytes, write_attachment_bytes_to_directory,
        write_attachment_image_preview_sidecar,
    };
    use crate::workspace_path::{
        canonicalize_workspace_path, normalized_workspace_path_text_from_path,
    };
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::ConversationAttachmentKind;

    /// Builds one unique temporary source file path inside the system temp directory.
    /// 在系统临时目录中构建一条唯一的临时源文件路径。
    fn unique_temp_file_path(extension: &str) -> std::path::PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("vulcan-attachment-assets-{unique_id}.{extension}"))
    }

    /// Builds one unique temporary workspace directory path so staging tests do not touch the shared home attachment cache.
    /// 构建一条唯一的临时工作区目录路径，使暂存测试不会触碰共享家目录附件缓存。
    fn unique_workspace_directory() -> std::path::PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("vulcan-attachment-assets-workspace-{unique_id}"));
        std::fs::create_dir_all(&directory).expect("temporary workspace directory should create");
        directory
    }

    /// Removes one temporary workspace directory after a staging test has finished.
    /// 在暂存测试结束后移除一条临时工作区目录。
    fn cleanup_workspace_directory(directory: &std::path::Path) {
        let _ = std::fs::remove_dir_all(directory);
    }

    /// Verifies staging one source file copies it into the managed attachment directory while retaining the original source path.
    /// 验证暂存源文件时会把它复制到托管附件目录，并保留原始来源路径。
    #[test]
    fn stage_attachment_file_materializes_managed_copy() {
        let workspace_directory = unique_workspace_directory();
        let source_path = unique_temp_file_path("png");
        std::fs::write(&source_path, [0_u8, 1, 2, 3]).expect("temporary source file should write");

        let attachment =
            stage_attachment_file(&workspace_directory.to_string_lossy(), &source_path)
                .expect("attachment should stage successfully");
        let expected_source_path =
            normalized_workspace_path_text_from_path(&canonicalize_workspace_path(&source_path));

        assert_eq!(attachment.kind, ConversationAttachmentKind::Image);
        assert!(std::path::Path::new(&attachment.local_path).is_file());
        assert_eq!(
            attachment.source_path.as_deref(),
            Some(expected_source_path.as_str())
        );
        assert_eq!(attachment.byte_len, Some(4));

        let _ = std::fs::remove_file(&source_path);
        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies in-memory staging creates a managed copy even when no original source path exists.
    /// 验证内存载荷暂存时即使没有原始来源路径，也会创建托管副本。
    #[test]
    fn stage_attachment_bytes_without_source_path_creates_managed_copy() {
        let workspace_directory = unique_workspace_directory();
        let descriptor = attachment_descriptor_from_mime_type("image/png")
            .expect("png mime type should stay supported");
        let attachment = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            "clipboard-image.png",
            None,
            &[0_u8, 1, 2, 3],
        )
        .expect("attachment bytes should stage successfully");

        assert_eq!(attachment.kind, ConversationAttachmentKind::Image);
        assert!(attachment.source_path.is_none());
        assert!(std::path::Path::new(&attachment.local_path).is_file());

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies repeated composer staging reuses one exact project asset and locator.
    /// 验证重复输入区暂存会复用同一个精确项目资产与定位符。
    #[test]
    fn project_attachment_staging_reuses_identical_content() {
        // workspace_directory is the project boundary for fingerprint reuse.
        // workspace_directory 是内容特征复用的项目边界。
        let workspace_directory = unique_workspace_directory();
        // descriptor supplies the exact MIME dimension of the deduplication key.
        // descriptor 提供排重键中的精确 MIME 维度。
        let descriptor = attachment_descriptor_from_mime_type("image/png")
            .expect("png mime type should stay supported");
        // first creates the canonical managed asset.
        // first 创建规范托管资产。
        let first = stage_project_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            "first-name.png",
            None,
            &[0_u8, 1, 2, 3],
        )
        .expect("first project attachment should stage");
        // second carries different display metadata but the same required fingerprint.
        // second 携带不同展示元数据，但具有相同的必需内容特征。
        let second = stage_project_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            "second-name.png",
            None,
            &[0_u8, 1, 2, 3],
        )
        .expect("identical project attachment should reuse");

        assert_eq!(second.asset_id, first.asset_id);
        assert_eq!(second.resource_path, first.resource_path);
        assert_eq!(second.local_path, first.local_path);
        assert_eq!(second.file_name, first.file_name);
        // managed_files proves the duplicate candidate copy was removed after atomic index reuse.
        // managed_files 证明原子索引复用后重复候选副本已被删除。
        let managed_files = std::fs::read_dir(
            resolve_managed_attachment_root(&workspace_directory.to_string_lossy()).join("images"),
        )
        .expect("managed image directory should read")
        .count();
        assert_eq!(managed_files, 1);

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies project identity and MIME remain hard dimensions of composer attachment reuse.
    /// 验证项目身份与 MIME 始终是输入区附件复用的硬维度。
    #[test]
    fn project_attachment_staging_keeps_scope_and_mime_isolated() {
        // first_workspace and second_workspace model two independent project attachment indexes.
        // first_workspace 与 second_workspace 模拟两个独立项目附件索引。
        let first_workspace = unique_workspace_directory();
        let second_workspace = unique_workspace_directory();
        // source_bytes are intentionally identical across every staging attempt.
        // source_bytes 在每次暂存尝试中刻意保持完全相同。
        let source_bytes = b"same immutable bytes";
        // first_mime and second_mime prove MIME participates even inside one project.
        // first_mime 与 second_mime 证明即使在同一项目内 MIME 也参与排重。
        let first_mime = stage_project_generic_attachment_bytes(
            &first_workspace.to_string_lossy(),
            "payload.one",
            None,
            "application/x-vulcan-one",
            source_bytes,
        )
        .expect("first generic project attachment should stage");
        let second_mime = stage_project_generic_attachment_bytes(
            &first_workspace.to_string_lossy(),
            "payload.two",
            None,
            "application/x-vulcan-two",
            source_bytes,
        )
        .expect("different MIME should stage independently");
        // other_project proves an identical fingerprint cannot cross the workspace-owned index.
        // other_project 证明完全相同的内容特征不能跨越工作区所有的索引。
        let other_project = stage_project_generic_attachment_bytes(
            &second_workspace.to_string_lossy(),
            "payload.one",
            None,
            "application/x-vulcan-one",
            source_bytes,
        )
        .expect("another project should stage independently");

        assert_ne!(first_mime.asset_id, second_mime.asset_id);
        assert_ne!(first_mime.asset_id, other_project.asset_id);
        assert_ne!(first_mime.local_path, other_project.local_path);

        cleanup_workspace_directory(&first_workspace);
        cleanup_workspace_directory(&second_workspace);
    }

    /// Verifies an oversized-image derivative is stored as an adjacent sidecar and removed with its canonical asset.
    /// 验证超大图片派生内容会作为相邻旁路文件保存，并随规范资产一起移除。
    #[test]
    fn image_preview_sidecar_is_bound_without_a_second_asset() {
        let workspace_directory = unique_workspace_directory();
        let descriptor = attachment_descriptor_from_mime_type("image/png")
            .expect("png mime type should stay supported");
        let attachment = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            "oversized-image.png",
            None,
            &[0_u8, 1, 2, 3],
        )
        .expect("canonical image should stage successfully");
        let preview_bytes = [0xff_u8, 0xd8, 0xff, 0xd9];

        let preview_path = write_attachment_image_preview_sidecar(&attachment, &preview_bytes)
            .expect("jpeg preview sidecar should write");
        assert_eq!(
            std::fs::read(&preview_path).expect("preview sidecar should read"),
            preview_bytes
        );

        remove_attachment_preview_sidecar(&attachment);
        assert!(!Path::new(&preview_path).exists());
        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies preview sidecars reject non-JPEG bytes, oversize payloads, and non-image attachments.
    /// 验证预览旁路文件会拒绝非 JPEG 字节、超限载荷与非图片附件。
    #[test]
    fn image_preview_sidecar_rejects_invalid_payload_or_modality() {
        let workspace_directory = unique_workspace_directory();
        let image = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &attachment_descriptor_from_mime_type("image/png")
                .expect("png mime type should stay supported"),
            "image.png",
            None,
            &[0_u8, 1, 2, 3],
        )
        .expect("canonical image should stage successfully");
        let generic = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &generic_attachment_descriptor(),
            "archive.bin",
            None,
            b"generic bytes",
        )
        .expect("generic attachment should stage successfully");
        let mut oversized = vec![0_u8; IMAGE_PREVIEW_SIDECAR_MAX_BYTES + 1];
        oversized[..3].copy_from_slice(&[0xff, 0xd8, 0xff]);

        assert!(write_attachment_image_preview_sidecar(&image, b"not-jpeg").is_err());
        assert!(write_attachment_image_preview_sidecar(&image, &oversized).is_err());
        assert!(
            write_attachment_image_preview_sidecar(&generic, &[0xff, 0xd8, 0xff, 0xd9]).is_err()
        );
        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies unknown files retain a safe user-visible name and canonical forward-slash resource locator.
    /// 验证未知文件会保留安全的用户可见名称与使用正斜杠的规范资源定位符。
    #[test]
    fn stage_generic_attachment_preserves_safe_name_and_resource_locator() {
        let workspace_directory = unique_workspace_directory();
        let attachment = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &generic_attachment_descriptor(),
            "custom.package",
            None,
            b"generic bytes",
        )
        .expect("generic attachment should stage successfully");

        assert_eq!(attachment.kind, ConversationAttachmentKind::File);
        assert_eq!(attachment.file_name, "custom.package");
        let resource_path = attachment
            .resource_path
            .as_deref()
            .expect("generic attachment should have a safe resource locator");
        assert!(resource_path.starts_with(".vulcan/code/attachments/files/"));
        assert!(!resource_path.contains('\\'));

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies generic browser files retain valid concrete MIME metadata and reject parameter injection.
    /// 验证通用浏览器文件会保留有效具体 MIME 元数据，并拒绝参数注入。
    #[test]
    fn stage_generic_attachment_preserves_valid_concrete_mime_type() {
        let workspace_directory = unique_workspace_directory();
        let attachment = stage_generic_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            "workbook.xlsx",
            None,
            "Application/Vnd.Openxmlformats-Officedocument.Spreadsheetml.Sheet",
            b"spreadsheet bytes",
        )
        .expect("generic attachment should retain its validated MIME type");

        assert_eq!(
            attachment.mime_type,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        let image = stage_generic_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            "diagram.svg",
            None,
            "image/svg+xml",
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .expect("unknown image formats should retain their media modality");
        assert_eq!(image.kind, ConversationAttachmentKind::Image);
        assert_eq!(image.mime_type, "image/svg+xml");
        assert!(
            image
                .resource_path
                .as_deref()
                .is_some_and(|path| path.starts_with(".vulcan/code/attachments/images/"))
        );
        assert!(
            stage_generic_attachment_bytes(
                &workspace_directory.to_string_lossy(),
                "unsafe.bin",
                None,
                "application/octet-stream; charset=binary",
                b"unsafe metadata bytes",
            )
            .is_err()
        );

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies an index publication failure rolls back the managed file written earlier in staging.
    /// 验证索引发布失败时会回滚暂存流程中较早写入的托管文件。
    #[test]
    fn stage_attachment_bytes_rolls_back_file_when_index_publication_fails() {
        // workspace_directory isolates the deliberately blocked index directory.
        // workspace_directory 隔离刻意阻塞的索引目录。
        let workspace_directory = unique_workspace_directory();
        // asset_root is resolved through the production workspace mapping.
        // asset_root 通过生产工作区映射解析。
        let asset_root = resolve_managed_attachment_root(&workspace_directory.to_string_lossy());
        std::fs::create_dir_all(&asset_root).expect("managed attachment root should be created");
        // blocking_index_path is a regular file where the index directory must be created.
        // blocking_index_path 是位于索引目录预期位置的普通文件。
        let blocking_index_path = asset_root.join("index");
        std::fs::write(&blocking_index_path, b"blocks index directory")
            .expect("blocking index file should be written");
        // descriptor drives the normal image-category staging path before index publication fails.
        // descriptor 在索引发布失败前驱动正常的图片分类暂存路径。
        let descriptor = attachment_descriptor_from_mime_type("image/png")
            .expect("png mime type should stay supported");

        // result must report the index failure after rolling back the managed file.
        // result 必须在回滚托管文件后报告索引失败。
        let result = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            "rollback-image.png",
            None,
            &[0_u8, 1, 2, 3],
        );
        // managed_file_count counts files inside category directories while excluding the deliberate index blocker.
        // managed_file_count 统计分类目录内文件，并排除刻意设置的索引阻塞文件。
        let mut managed_file_count = 0usize;
        for entry in std::fs::read_dir(&asset_root).expect("managed attachment root should read") {
            // entry is one direct child of the managed attachment root.
            // entry 是托管附件根目录的一个直接子项。
            let entry = entry.expect("managed attachment root entry should read");
            if entry
                .file_type()
                .expect("managed attachment root entry type should read")
                .is_dir()
            {
                managed_file_count += std::fs::read_dir(entry.path())
                    .expect("managed attachment category should read")
                    .count();
            }
        }
        cleanup_workspace_directory(&workspace_directory);

        assert!(result.is_err());
        assert_eq!(managed_file_count, 0);
    }

    /// Verifies caller-supplied mismatched extensions are discarded so the managed file name always reflects the host-selected format.
    /// 验证会丢弃调用方传入的错误扩展名，从而使托管文件名始终反映宿主选定的真实格式。
    #[test]
    fn normalize_attachment_file_name_forces_host_selected_extension() {
        let descriptor = attachment_descriptor_from_mime_type("image/jpeg")
            .expect("jpeg descriptor should resolve");
        assert_eq!(
            normalize_attachment_file_name("poster.png", &descriptor),
            "poster.jpg"
        );
        assert_eq!(
            normalize_attachment_file_name("CON.gif", &descriptor),
            "CON_.jpg"
        );
    }

    /// Verifies exporting one attachment payload to a caller-selected directory avoids overwriting an existing file by appending a numbered suffix.
    /// 验证将一份附件载荷导出到调用方选定目录时，会通过追加编号后缀来避免覆盖已存在文件。
    #[test]
    fn write_attachment_bytes_to_directory_avoids_existing_file_collisions() {
        let workspace_directory = unique_workspace_directory();
        let export_directory = workspace_directory.join("exports");
        std::fs::create_dir_all(&export_directory).expect("export directory should create");
        let descriptor = attachment_descriptor_from_mime_type("image/png")
            .expect("png descriptor should resolve");
        let existing_file = export_directory.join("cover.png");
        std::fs::write(&existing_file, [0_u8, 1, 2, 3]).expect("existing file should write");

        let output_path = write_attachment_bytes_to_directory(
            &descriptor,
            &export_directory,
            "cover.jpg",
            &[4_u8],
        )
        .expect("exported file should materialize");

        assert_eq!(
            output_path.file_name().and_then(|value| value.to_str()),
            Some("cover-001.png")
        );
        assert!(output_path.is_file());

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies replay-path resolution falls back to the original source file when the managed copy no longer exists.
    /// 验证当托管副本不存在时，回放路径解析会回退到原始源文件。
    #[test]
    fn resolve_attachment_replay_path_falls_back_to_original_source() {
        let workspace_directory = unique_workspace_directory();
        let source_path = unique_temp_file_path("pdf");
        std::fs::write(&source_path, [0_u8, 1, 2, 3]).expect("temporary source file should write");

        let attachment =
            stage_attachment_file(&workspace_directory.to_string_lossy(), &source_path)
                .expect("attachment should stage successfully");
        std::fs::remove_file(&attachment.local_path).expect("managed copy should remove");

        let resolved_replay_path =
            resolve_attachment_replay_path(&attachment).expect("source fallback should resolve");
        assert_eq!(
            canonicalize_workspace_path(&resolved_replay_path),
            canonicalize_workspace_path(&source_path)
        );

        let _ = std::fs::remove_file(&source_path);
        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies supported descriptors can be resolved from both file paths and MIME types.
    /// 验证受支持的描述信息既可从文件路径也可从 MIME 类型解析出来。
    #[test]
    fn attachment_descriptor_resolution_matches_expected_kind() {
        let source_path = unique_temp_file_path("mp4");
        assert_eq!(
            attachment_descriptor_from_path(&source_path)
                .expect("mp4 path should stay supported")
                .kind,
            ConversationAttachmentKind::Video
        );
        assert_eq!(
            attachment_descriptor_from_mime_type("application/pdf")
                .expect("pdf mime type should stay supported")
                .kind,
            ConversationAttachmentKind::Pdf
        );
        assert_eq!(
            attachment_descriptor_from_mime_type("video/x-flv")
                .expect("Kimi FLV MIME type should be supported")
                .kind,
            ConversationAttachmentKind::Video
        );
        assert_eq!(
            attachment_descriptor_from_path(&unique_temp_file_path("3gp"))
                .expect("Kimi 3GP path should be supported")
                .mime_type,
            "video/3gpp"
        );
        assert_eq!(
            attachment_descriptor_from_path(&unique_temp_file_path("m4v"))
                .expect("Kimi M4V path should be supported")
                .mime_type,
            "video/x-m4v"
        );
        assert_eq!(
            attachment_descriptor_from_mime_type("video/3gpp2")
                .expect("Kimi 3G2 MIME type should be supported")
                .extension,
            "3g2"
        );
        for extension in ["oga", "opus", "pcm", "ulaw"] {
            assert_eq!(
                attachment_descriptor_from_path(&unique_temp_file_path(extension))
                    .expect("registered audio path should be supported")
                    .kind,
                ConversationAttachmentKind::Audio
            );
        }
    }

    /// Verifies the managed attachment root stays inside the shared Vulcan home directory when no workspace path is provided.
    /// 验证当未提供工作区路径时，托管附件根目录会稳定落在共享 Vulcan 家目录下。
    #[test]
    fn resolve_managed_attachment_root_uses_shared_home_fallback() {
        let managed_root = resolve_managed_attachment_root("");

        assert!(
            managed_root.ends_with(
                Path::new(vulcan_runtime_assets::USER_VULCAN_ROOT_DIRECTORY_NAME)
                    .join("code")
                    .join("attachments")
            )
        );
    }

    /// Verifies the managed attachment root stays inside the workspace-local `.vulcan/code/attachments` subtree.
    /// 验证托管附件根目录稳定落在工作区本地的 `.vulcan/code/attachments` 子树下。
    #[test]
    fn resolve_managed_attachment_root_uses_workspace_vulcan_code_subtree() {
        let workspace_directory = unique_workspace_directory();

        assert!(
            resolve_managed_attachment_root(&workspace_directory.to_string_lossy())
                .ends_with(Path::new(".vulcan/code/attachments"))
        );

        cleanup_workspace_directory(&workspace_directory);
    }

    /// Verifies clipboard-style RGBA image bytes can be encoded into a PNG payload for managed staging.
    /// 验证剪贴板风格的 RGBA 图片字节可被编码成用于托管暂存的 PNG 载荷。
    #[test]
    fn encode_rgba_image_to_png_bytes_builds_png_payload() {
        let png_bytes = encode_rgba_image_to_png_bytes(1, 1, &[255_u8, 0, 0, 255])
            .expect("rgba clipboard image should encode");

        assert!(png_bytes.starts_with(&[137, 80, 78, 71]));
    }
}
