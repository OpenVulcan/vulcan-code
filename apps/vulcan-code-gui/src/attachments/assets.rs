//! Host-managed attachment staging helpers shared by the composer, persistence, and provider replay layers.
//! 供提示词输入区、持久化与 Provider 回放层共享的宿主管理附件暂存辅助模块。

use crate::attachment_asset_index::{
    AttachmentReplayStatus, current_attachment_timestamp_parts, record_staged_attachment,
    resolve_attachment_replay_status,
};
use crate::workspace_path::{
    canonicalize_workspace_path, normalized_workspace_path_text_from_path,
};
use filetime::{FileTime, set_file_times};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

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

    let home_directory = dirs::home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    home_directory
        .join(WORKSPACE_VULCAN_DIR_NAME)
        .join(WORKSPACE_CODE_DIR_NAME)
        .join(MANAGED_ATTACHMENT_DIR_NAME)
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
        "audio/mpeg" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/mpeg",
            extension: "mp3",
        }),
        "audio/wav" | "audio/x-wav" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/wav",
            extension: "wav",
        }),
        "audio/mp4" => Some(AttachmentDescriptor {
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
        "audio/ogg" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/ogg",
            extension: "ogg",
        }),
        "video/mp4" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/mp4",
            extension: "mp4",
        }),
        "video/quicktime" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/quicktime",
            extension: "mov",
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
        "video/x-msvideo" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/x-msvideo",
            extension: "avi",
        }),
        "application/pdf" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Pdf,
            mime_type: "application/pdf",
            extension: "pdf",
        }),
        _ => None,
    }
}

/// Copies one supported source file into the managed attachment store and returns the normalized attachment payload used by future replays.
/// 将一份受支持的源文件复制到托管附件存储中，并返回未来回放使用的规范化附件载荷。
pub fn stage_attachment_file(
    workspace_path: &str,
    source_path: &Path,
) -> Result<ConversationAttachment, String> {
    let descriptor = attachment_descriptor_from_path(source_path)
        .ok_or_else(|| format!("文件 `{}` 不是受支持的附件格式。", source_path.display()))?;
    let canonical_source_path = canonicalize_workspace_path(source_path);
    let file_bytes = fs::read(&canonical_source_path).map_err(|error| {
        format!(
            "读取附件文件 `{}` 失败：{}。",
            canonical_source_path.display(),
            error
        )
    })?;
    let file_name = canonical_source_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| canonical_source_path.display().to_string());

    stage_attachment_bytes(
        workspace_path,
        &descriptor,
        &file_name,
        Some(canonical_source_path.as_path()),
        &file_bytes,
    )
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
    let timestamp = current_attachment_timestamp_parts();
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let category_directory = resolve_attachment_category_directory(&asset_root, &descriptor.kind);
    fs::create_dir_all(&category_directory).map_err(|error| {
        format!(
            "创建托管附件分类目录 `{}` 失败：{}。",
            category_directory.display(),
            error
        )
    })?;

    let asset_id = Uuid::new_v4().to_string();
    let sanitized_file_name =
        sanitize_attachment_file_name(suggested_file_name, descriptor.extension);
    let managed_file_path = category_directory.join(format!(
        "{}-{asset_id}-{}",
        timestamp.compact_prefix, sanitized_file_name
    ));
    fs::write(&managed_file_path, bytes).map_err(|error| {
        format!(
            "写入托管附件 `{}` 失败：{}。",
            managed_file_path.display(),
            error
        )
    })?;

    set_managed_attachment_file_times(&managed_file_path, timestamp.unix_millis)?;
    let attachment = ConversationAttachment {
        kind: descriptor.kind.clone(),
        file_name: sanitized_file_name,
        local_path: managed_file_path.to_string_lossy().into_owned(),
        source_path: source_path
            .map(canonicalize_workspace_path)
            .map(|path| normalized_workspace_path_text_from_path(&path)),
        asset_id: Some(asset_id),
        byte_len: Some(bytes.len() as u64),
        preview_local_path: None,
        mime_type: descriptor.mime_type.to_string(),
    };
    record_staged_attachment(workspace_path, &attachment, &timestamp)?;
    Ok(attachment)
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
            "创建导出附件目录 `{}` 失败：{}。",
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
    fs::write(&output_path, bytes)
        .map_err(|error| format!("写入导出附件 `{}` 失败：{}。", output_path.display(), error))?;
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
            "剪贴板图片像素数据长度无效：期望 {expected_len} 字节，实际 {} 字节。",
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
            .map_err(|error| format!("创建 PNG 编码头失败：{error}。"))?;
        writer
            .write_image_data(rgba)
            .map_err(|error| format!("写入 PNG 图片数据失败：{error}。"))?;
    }

    Ok(output)
}

/// Returns the first readable replay path for one attachment by preferring the managed copy and then falling back to the original source path.
/// 为单条附件返回第一条可读回放路径，优先使用托管副本，再回退到原始来源路径。
pub fn resolve_attachment_replay_path(attachment: &ConversationAttachment) -> Option<PathBuf> {
    if matches!(
        resolve_attachment_replay_status(attachment),
        AttachmentReplayStatus::Purged
    ) {
        return None;
    }

    let managed_path = attachment.local_path.trim();
    if !managed_path.is_empty() {
        let managed_path = PathBuf::from(managed_path);
        if managed_path.is_file() {
            return Some(managed_path);
        }
    }

    attachment
        .source_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

/// Removes any deterministic managed preview sidecar associated with the provided managed attachment path.
/// 移除与给定托管附件路径关联的任意确定性托管预览旁路文件。
pub fn remove_attachment_preview_sidecar(attachment: &ConversationAttachment) {
    if matches!(attachment.kind, ConversationAttachmentKind::Video) {
        let managed_path = Path::new(&attachment.local_path);
        let parent = managed_path.parent().unwrap_or_else(|| Path::new("."));
        let stem = managed_path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("video");
        let legacy_preview_path = parent.join(format!("{stem}.preview.png"));
        if legacy_preview_path.is_file() {
            let _ = fs::remove_file(legacy_preview_path);
        }
    }
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
            "更新托管附件 `{}` 的访问/修改时间失败：{}。",
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
                "打开托管附件 `{}` 以更新创建时间失败：{}。",
                managed_file_path.display(),
                error
            )
        })?;
    let file_times = FileTimes::new().set_created(unix_millis_to_system_time(unix_millis));
    file.set_times(file_times).map_err(|error| {
        format!(
            "更新托管附件 `{}` 的创建时间失败：{}。",
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
        "ogg" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Audio,
            mime_type: "audio/ogg",
            extension: "ogg",
        }),
        "mp4" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/mp4",
            extension: "mp4",
        }),
        "mov" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Video,
            mime_type: "video/quicktime",
            extension: "mov",
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
        "pdf" => Some(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Pdf,
            mime_type: "application/pdf",
            extension: "pdf",
        }),
        _ => None,
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
        attachment_descriptor_from_mime_type, attachment_descriptor_from_path,
        encode_rgba_image_to_png_bytes, normalize_attachment_file_name,
        resolve_attachment_replay_path, resolve_managed_attachment_root, stage_attachment_bytes,
        stage_attachment_file, write_attachment_bytes_to_directory,
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

        assert_eq!(
            resolve_attachment_replay_path(&attachment)
                .as_deref()
                .map(std::path::Path::new),
            Some(canonicalize_workspace_path(&source_path).as_path())
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
    }

    /// Verifies the managed attachment root stays inside the shared Vulcan home directory when no workspace path is provided.
    /// 验证当未提供工作区路径时，托管附件根目录会稳定落在共享 Vulcan 家目录下。
    #[test]
    fn resolve_managed_attachment_root_uses_shared_home_fallback() {
        let managed_root = resolve_managed_attachment_root("");

        assert!(managed_root.ends_with(Path::new(".vulcan/code/attachments")));
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
