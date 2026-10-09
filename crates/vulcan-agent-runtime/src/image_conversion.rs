//! Shared static-image conversion for native reads, composer attachments and image-understanding backends.
//! 原生读取、会话附件和图片理解后端共享的静态图片转换。

use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Write},
    path::Path,
};
use tokio::io::AsyncReadExt;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_provider_adapters::PreparedMediaSourceIdentity;
#[cfg(test)]
mod tests;

/// A derived attachment and its temporary file owner; original asset identity is never reused for changed bytes.
/// 派生附件及其临时文件所有者；改变字节后绝不复用原资产身份。
pub struct ConvertedAttachment {
    /// Verified PNG metadata for delivery or persistent restaging.
    /// 用于交付或持久重暂存的已校验PNG元数据。
    pub attachment: ConversationAttachment,
    /// File lifetime spans the caller's delivery; dropping it cleans the derivative only.
    /// 文件生命周期覆盖调用方交付；释放时仅清理派生文件。
    _file: tempfile::NamedTempFile,
}

/// Converts image `attachment` only when the destination rejects its MIME and admits PNG.
/// 仅当目标拒绝图片`attachment`的MIME且接受PNG时进行转换。
/// Returns a separately owned derivative, no change for accepted/nonimage media, or a concrete conversion error.
/// 返回独立持有的派生产物；已支持或非图片媒体不变；转换失败返回具体错误。
pub async fn convert_attachment_if_needed(
    attachment: &ConversationAttachment,
    accepts_original: bool,
    accepts_png: bool,
) -> Result<Option<ConvertedAttachment>, String> {
    if attachment.kind != ConversationAttachmentKind::Image || accepts_original {
        return Ok(None);
    }
    if !accepts_png {
        return Err(format!(
            "Image MIME `{}` is unsupported and automatic PNG conversion is unavailable because the target does not accept image/png.",
            attachment.mime_type
        ));
    }
    // Canonical metadata proves the original source before any derivative is created.
    // 规范元数据在创建派生文件之前证明原始来源。
    let identity = PreparedMediaSourceIdentity {
        byte_len: attachment
            .byte_len
            .ok_or("Image source has no canonical byte length.")?,
        content_sha256: attachment
            .source_sha256
            .clone()
            .ok_or("Image source has no canonical content digest.")?,
    };
    // Decoder choice is exact and never falls back to an external program or AI.
    // 解码器选择精确，绝不回退到外部程序或AI。
    let codec = codec_for_mime(&attachment.mime_type).ok_or_else(|| {
        format!(
            "No automatic PNG converter supports image MIME `{}`.",
            attachment.mime_type
        )
    })?;
    // Retain the file owner while downstream code consumes its canonical local metadata.
    // 下游代码消费规范本地元数据时保留文件所有者。
    let file = convert(
        Path::new(&attachment.local_path),
        identity.byte_len,
        Some(&identity),
        codec,
    )
    .await
    .map_err(|error| {
        format!(
            "Image conversion from {} to image/png failed: {error}",
            attachment.mime_type
        )
    })?;
    derived_attachment(attachment, file, "image/png", "png").map(Some)
}

/// Builds the derivative of `attachment` stored in `file` as `mime` with filename `extension`.
/// 以`mime`及文件扩展名`extension`构建存于`file`的`attachment`派生附件。
/// Returns the owned derivative with fresh size and digest, or the metadata/digest error.
/// 返回带新大小及摘要的自有派生附件，或元数据及摘要错误。
fn derived_attachment(
    attachment: &ConversationAttachment,
    file: tempfile::NamedTempFile,
    mime: &str,
    extension: &str,
) -> Result<ConvertedAttachment, String> {
    // Converted content is a new source, so clear immutable asset and original delivery identities.
    // 转换内容是新来源，因此清除不可变资产身份及原交付身份。
    let mut derived = attachment.clone();
    derived.file_name = Path::new(&attachment.file_name)
        .with_extension(extension)
        .to_string_lossy()
        .into_owned();
    derived.local_path = vulcan_runtime_assets::native_host_path_text_from_path(file.path());
    derived.mime_type = mime.into();
    derived.byte_len = Some(
        file.as_file()
            .metadata()
            .map_err(|error| error.to_string())?
            .len(),
    );
    derived.source_sha256 = Some(crate::attachment_assets::streaming_file_sha256(
        file.path(),
    )?);
    derived.asset_id = None;
    derived.resource_path = None;
    derived.delivery_mode = None;
    derived.preview_local_path = None;
    Ok(ConvertedAttachment {
        attachment: derived,
        _file: file,
    })
}

/// Longest image edge in pixels kept by composer compression; larger images are downscaled to it.
/// 输入区图片压缩保留的最长边像素；更大的图片缩小到此尺寸。
pub const COMPRESSION_MAX_EDGE_PX: u32 = 2048;

/// Native originals at or below this size that need no downscale are kept without decoding pixels.
/// 不超过此大小且无需缩小的原生原图保持不变，不解码像素。
pub const COMPRESSION_MIN_SOURCE_BYTES: u64 = 256 * 1024;

/// JPEG quality of compressed opaque images.
/// 压缩后不透明图片的JPEG质量。
const COMPRESSION_JPEG_QUALITY: u8 = 85;

/// A native original is replaced only when its same-size derivative keeps at most this percentage of its bytes.
/// 仅当同尺寸派生产物最多保留原字节的此百分比时才替换原生原图。
const COMPRESSION_MAX_KEPT_PERCENT: u64 = 90;

/// Image formats the frozen destination accepts natively, resolved by the caller before compression.
/// 冻结目标原生接受的图片格式，由调用方在压缩前解析。
#[derive(Debug, Clone, Copy)]
pub struct CompressionTargets {
    /// Whether the original MIME is accepted unchanged.
    /// 原MIME是否可原样接受。
    pub original: bool,
    /// Whether image/jpeg is accepted.
    /// 是否接受image/jpeg。
    pub jpeg: bool,
    /// Whether image/png is accepted.
    /// 是否接受image/png。
    pub png: bool,
}

/// Downscales image `attachment` to `COMPRESSION_MAX_EDGE_PX` and re-encodes it, converting unsupported formats too.
/// 将图片`attachment`缩小到`COMPRESSION_MAX_EDGE_PX`并重新编码，同时转换不受支持的格式。
/// Opaque images become JPEG and transparent ones PNG within `targets`; animated or multipage natives stay unchanged.
/// 在`targets`范围内不透明图片转为JPEG、透明图片转为PNG；动画或多页原生图片保持不变。
/// Returns a derivative only when the format must change, the image was downscaled, or bytes shrink enough;
/// otherwise none, so planning a compressed derivative again is a no-op. Errors are concrete read/decode/encode failures.
/// 仅在必须转换格式、已缩小尺寸或字节明显减少时返回派生产物，否则返回空，使再次规划压缩产物不产生变化；
/// 错误为具体的读取、解码或编码失败。
pub async fn compress_attachment(
    attachment: &ConversationAttachment,
    targets: CompressionTargets,
) -> Result<Option<ConvertedAttachment>, String> {
    if attachment.kind != ConversationAttachmentKind::Image {
        return Ok(None);
    }
    // PNG is native everywhere and therefore absent from the conversion-only codec registry.
    // PNG处处原生支持，因此不在仅转换用的编解码注册表中。
    let format = if attachment.mime_type == "image/png" {
        Some(ImageFormat::Png)
    } else {
        codec_for_mime(&attachment.mime_type).map(|codec| codec.format)
    };
    let Some(format) = format else {
        return if targets.original {
            Ok(None)
        } else {
            Err(format!(
                "No automatic image converter supports image MIME `{}`.",
                attachment.mime_type
            ))
        };
    };
    let identity = PreparedMediaSourceIdentity {
        byte_len: attachment
            .byte_len
            .ok_or("Image source has no canonical byte length.")?,
        content_sha256: attachment
            .source_sha256
            .clone()
            .ok_or("Image source has no canonical content digest.")?,
    };
    let bytes = read_source_snapshot(
        Path::new(&attachment.local_path),
        identity.byte_len,
        Some(&identity),
    )
    .await?;
    let byte_len = identity.byte_len;
    let compressed =
        tokio::task::spawn_blocking(move || compress_bytes(&bytes, format, byte_len, targets))
            .await
            .map_err(|error| format!("image compression worker failed: {error}"))?
            .map_err(|error| {
                format!(
                    "Image compression of {} failed: {error}",
                    attachment.mime_type
                )
            })?;
    match compressed {
        Some((file, mime, extension)) => {
            derived_attachment(attachment, file, mime, extension).map(Some)
        }
        None => Ok(None),
    }
}

/// Compresses bounded `bytes` of exact `format` and original size `byte_len` for `targets`.
/// 为`targets`压缩精确`format`、原大小`byte_len`的有界`bytes`。
/// Returns the derivative file with its MIME and extension, none when the original should stay, or an error.
/// 返回派生文件及其MIME和扩展名；原图应保留时返回空；否则返回错误。
fn compress_bytes(
    bytes: &[u8],
    format: ImageFormat,
    byte_len: u64,
    targets: CompressionTargets,
) -> Result<Option<(tempfile::NamedTempFile, &'static str, &'static str)>, String> {
    // Small native images within the edge bound are kept after reading only their header.
    // 尺寸在上限内的小型原生图片只读取文件头后即保留。
    if targets.original && byte_len <= COMPRESSION_MIN_SOURCE_BYTES {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        reader.limits(decoder_limits());
        let (width, height) = reader
            .into_dimensions()
            .map_err(|error| error.to_string())?;
        if width.max(height) <= COMPRESSION_MAX_EDGE_PX {
            return Ok(None);
        }
    }
    let decoded = match decode_static(bytes, format) {
        Ok(decoded) => decoded,
        // Animated or multipage natives keep every frame rather than collapsing to one still.
        // 动画或多页原生图片保留全部帧，不压成单张静态图。
        Err(_) if targets.original => return Ok(None),
        Err(error) => return Err(error),
    };
    let downscaled = decoded.width().max(decoded.height()) > COMPRESSION_MAX_EDGE_PX;
    let image = if downscaled {
        decoded.resize(
            COMPRESSION_MAX_EDGE_PX,
            COMPRESSION_MAX_EDGE_PX,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        decoded
    };
    // Transparency needs PNG; JPEG would flatten it.
    // 透明度需要PNG；JPEG会将其压平。
    let opaque =
        !image.color().has_alpha() || image.to_rgba8().pixels().all(|pixel| pixel[3] == u8::MAX);
    let jpeg = if (opaque || !targets.png) && targets.jpeg {
        true
    } else if targets.png {
        false
    } else {
        return Err("the target accepts neither image/jpeg nor image/png".into());
    };
    let mut file = tempfile::Builder::new()
        .prefix("vulcan-compressed-image-")
        .suffix(if jpeg { ".jpg" } else { ".png" })
        .tempfile()
        .map_err(|error| error.to_string())?;
    let writer = BoundedOutputWriter {
        file: file.as_file_mut(),
        written: 0,
    };
    if jpeg {
        DynamicImage::ImageRgb8(image.to_rgb8()).write_with_encoder(
            image::codecs::jpeg::JpegEncoder::new_with_quality(writer, COMPRESSION_JPEG_QUALITY),
        )
    } else {
        image.write_with_encoder(image::codecs::png::PngEncoder::new(writer))
    }
    .map_err(|error| error.to_string())?;
    let written = file
        .as_file()
        .metadata()
        .map_err(|error| error.to_string())?
        .len();
    // A native original stays unless downscaling happened or re-encoding saves enough bytes.
    // 原生原图保持不变，除非已缩小尺寸或重新编码节省足够字节。
    if targets.original
        && (written >= byte_len
            || (!downscaled
                && written.saturating_mul(100)
                    > byte_len.saturating_mul(COMPRESSION_MAX_KEPT_PERCENT)))
    {
        return Ok(None);
    }
    Ok(Some(if jpeg {
        (file, "image/jpeg", "jpg")
    } else {
        (file, "image/png", "png")
    }))
}

/// Checks native image input against exact frozen `profile`, model modalities and resolved `constraints`.
/// 根据精确冻结`profile`、模型模态及已解析`constraints`检查原生图片输入。
/// Returns capability admission only; delivery subsequently validates actual byte and count limits.
/// 仅返回能力准入；后续交付按真实字节数及数量校验上限。
pub fn native_image_mime_supported(
    profile: &str,
    model_inputs: &[vulcan_ai_contract::ResourceKind],
    constraints: &[vulcan_provider_adapters::InputResourceConstraint],
    mime: &str,
) -> bool {
    // Empty independent MIME constraints use the existing native defaults, not all decodable formats.
    // 独立MIME约束为空时使用既有原生默认值，而非所有可解码格式。
    let constraint = constraints
        .iter()
        .find(|constraint| constraint.resource_kind == vulcan_ai_contract::ResourceKind::Image);
    if constraint.is_none_or(|constraint| constraint.accepted_mime_types.is_empty())
        && !crate::attachment_assets::registered_media_attachment_mime_types(
            ConversationAttachmentKind::Image,
        )
        .contains(&mime)
    {
        return false;
    }
    // Reuse the same target facts as conversation and read_media instead of inferring provider capabilities.
    // 复用会话及read_media的目标事实，不推断供应商能力。
    let facts = vulcan_provider_adapters::MediaDeliveryTarget {
        profile_id: profile,
        model_inputs,
        constraint,
        upload: None,
    }
    .facts(
        vulcan_ai_contract::ResourceKind::Image,
        mime,
        1,
        vulcan_provider_adapters::MediaDeliveryOrigin::NewIntent,
        1,
    );
    facts.target_supported && facts.inline_supported && facts.target_max_bytes != Some(0)
}

/// Converts local `sources` against an exact image-understanding `profile`, returning temporary owners for dispatch.
/// 根据精确图片理解`profile`转换本地`sources`，返回分派期间的临时所有者。
/// The reviewed profile declares ordered alternative channels; matching any route retains the native source.
/// 已审核Profile声明有序备选通道；任一路线匹配时保留原生来源。
pub(crate) async fn convert_provider_sources(
    sources: &mut [vulcan_provider_adapters::ProviderToolMediaSource],
    profile: &vulcan_provider_adapters::ProviderToolMediaInputProfile,
) -> Result<Vec<tempfile::NamedTempFile>, String> {
    // Only image understanding is eligible; audio/video sources retain their original bytes and constraints.
    // 仅图片理解符合条件；音频和视频来源保留原字节及约束。
    let mut files = Vec::new();
    if profile.resource_kind != vulcan_ai_contract::ResourceKind::Image {
        return Ok(files);
    }
    // Channel and binding MIME intersection is already resolved by the authoritative provider runtime.
    // 权威供应商运行时已经解析通道及绑定的MIME交集。
    let accepts = |mime: &str| {
        profile.routes.iter().any(|route| {
            (route.accepted_mime_types.is_empty()
                && crate::attachment_assets::registered_media_attachment_mime_types(
                    ConversationAttachmentKind::Image,
                )
                .contains(&mime))
                || route
                    .accepted_mime_types
                    .iter()
                    .any(|accepted| accepted.eq_ignore_ascii_case(mime))
        })
    };
    for source in sources {
        if accepts(&source.mime_type) {
            continue;
        }
        if !accepts("image/png") {
            return Err(format!(
                "Image MIME `{}` is unsupported; the configured understanding target does not accept PNG conversion output.",
                source.mime_type
            ));
        }
        // The source proof came from the owned resource resolver; conversion verifies it before decoding.
        // 来源证明来自有归属的资源解析器；转换在解码前验证该证明。
        let codec = codec_for_mime(&source.mime_type).ok_or_else(|| {
            format!(
                "No automatic PNG converter supports image MIME `{}`.",
                source.mime_type
            )
        })?;
        // Keep each derivative alive until the provider has consumed it.
        // 保留每份派生产物，直到供应商消费完成。
        let file = convert(
            &source.local_path,
            source.source_identity.byte_len,
            Some(&source.source_identity),
            codec,
        )
        .await
        .map_err(|error| {
            format!(
                "Image conversion from {} to image/png failed: {error}",
                source.mime_type
            )
        })?;
        source.local_path = file.path().to_path_buf();
        source.file_name = Path::new(&source.file_name)
            .with_extension("png")
            .to_string_lossy()
            .into_owned();
        source.mime_type = "image/png".into();
        source.source_identity = PreparedMediaSourceIdentity {
            byte_len: file
                .as_file()
                .metadata()
                .map_err(|error| error.to_string())?
                .len(),
            content_sha256: crate::attachment_assets::streaming_file_sha256(file.path())?,
        };
        files.push(file);
    }
    Ok(files)
}

/// Maximum bytes per conversion source, decoded raster or PNG output; not a claim about total process memory.
/// 每份转换源文件、解码像素或PNG输出的字节上限；不表示进程总内存上限。
pub const BUFFER_LIMIT_BYTES: u64 = 64 * 1024 * 1024;

/// One explicitly compiled decoder with canonical MIME and local filename extensions.
/// 一个显式编译的解码器及其规范MIME和本地文件扩展名。
pub struct ImageCodec {
    /// MIME consumed by this decoder, independent of native model support.
    /// 此解码器消费的MIME，独立于模型原生支持情况。
    pub mime: &'static str,
    /// Filename extensions resolved only for explicit local paths.
    /// 仅为显式本地路径解析的文件扩展名。
    pub extensions: &'static [&'static str],
    /// Exact decoder selection; never probe alternative formats after failure.
    /// 精确解码器选择；失败后绝不探测其他格式。
    format: ImageFormat,
}

/// Single registry for conversion advertising, local recognition and decoder selection.
/// 转换能力说明、本地识别和解码器选择共用的唯一注册表。
pub const CODECS: &[ImageCodec] = &[
    ImageCodec {
        mime: "image/jpeg",
        extensions: &["jpg", "jpeg"],
        format: ImageFormat::Jpeg,
    },
    ImageCodec {
        mime: "image/webp",
        extensions: &["webp"],
        format: ImageFormat::WebP,
    },
    ImageCodec {
        mime: "image/gif",
        extensions: &["gif"],
        format: ImageFormat::Gif,
    },
    ImageCodec {
        mime: "image/bmp",
        extensions: &["bmp"],
        format: ImageFormat::Bmp,
    },
    ImageCodec {
        mime: "image/tiff",
        extensions: &["tif", "tiff"],
        format: ImageFormat::Tiff,
    },
    ImageCodec {
        mime: "image/x-icon",
        extensions: &["ico"],
        format: ImageFormat::Ico,
    },
    ImageCodec {
        mime: "image/x-portable-bitmap",
        extensions: &["pbm"],
        format: ImageFormat::Pnm,
    },
    ImageCodec {
        mime: "image/x-portable-graymap",
        extensions: &["pgm"],
        format: ImageFormat::Pnm,
    },
    ImageCodec {
        mime: "image/x-portable-pixmap",
        extensions: &["ppm"],
        format: ImageFormat::Pnm,
    },
    ImageCodec {
        mime: "image/x-portable-anymap",
        extensions: &["pnm", "pam"],
        format: ImageFormat::Pnm,
    },
    ImageCodec {
        mime: "image/x-tga",
        extensions: &["tga"],
        format: ImageFormat::Tga,
    },
    ImageCodec {
        mime: "image/qoi",
        extensions: &["qoi"],
        format: ImageFormat::Qoi,
    },
];

/// Resolves exact canonical `mime` to a compiled image decoder, returning none for other media.
/// 将精确规范`mime`解析为已编译图片解码器；其他媒体返回空。
pub fn codec_for_mime(mime: &str) -> Option<&'static ImageCodec> {
    CODECS.iter().find(|codec| codec.mime == mime)
}

/// Lists additional PNG-convertible formats for native `mimes`; returns none unless PNG itself is admitted.
/// 为原生`mimes`列出额外可转PNG的格式；PNG本身未准入时返回空。
pub fn convertible_codecs(mimes: &[&str]) -> Vec<&'static ImageCodec> {
    if !mimes.contains(&"image/png") {
        return Vec::new();
    }
    CODECS
        .iter()
        .filter(|codec| !mimes.contains(&codec.mime))
        .collect()
}

/// Describes additional conversion inputs for exact native `mimes`, including limits and failure semantics.
/// 为精确原生`mimes`说明额外转换输入，包括限制及失败语义。
pub fn guidance(mimes: &[&str]) -> String {
    // Derive advertised formats from the same registry used to choose decoders.
    // 从选择解码器使用的同一注册表派生声明格式。
    let codecs = convertible_codecs(mimes);
    if !mimes.contains(&"image/png") {
        return "Automatic image conversion: unavailable because the target does not accept image/png. Audio and video are never transcoded.".into();
    }
    if codecs.is_empty() {
        return "All registered convertible image formats are already native; no additional image formats are accepted. Audio and video are never transcoded.".into();
    }
    // Explicit extensions help the model choose paths without treating every image MIME as supported.
    // 明确扩展名帮助模型选择路径，避免认为任意图片MIME均受支持。
    let formats = codecs
        .iter()
        .map(|codec| format!("{} ({})", codec.mime, codec.extensions.join("/")))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Additional images automatically converted to PNG: {formats}. Native formats are sent unchanged. Only static images are converted; animated GIF/WebP and multipage TIFF fail rather than lose frames/pages; ICO selects the decoder's best icon variant. Source, decoded raster and converted PNG each must fit {BUFFER_LIMIT_BYTES} bytes, followed by the target's delivery limits. Conversion failure returns a call error with its reason and attaches no content. Unlisted image formats are unsupported. Audio and video are never transcoded."
    )
}

/// Snapshots `path` at `byte_len`, verifies optional managed `identity`, then decodes exact `codec` to a temporary PNG.
/// 按`byte_len`快照读取`path`、校验可选托管`identity`，再用精确`codec`解码到临时PNG。
/// Returns a file owner retained through delivery, or the concrete read, identity, decode or encode error.
/// 返回需保留到交付完成的文件所有者，或具体的读取、身份、解码或编码错误。
pub async fn convert(
    path: &Path,
    byte_len: u64,
    identity: Option<&PreparedMediaSourceIdentity>,
    codec: &'static ImageCodec,
) -> Result<tempfile::NamedTempFile, String> {
    let bytes = read_source_snapshot(path, byte_len, identity).await?;
    tokio::task::spawn_blocking(move || encode_png(&bytes, codec.format))
        .await
        .map_err(|error| format!("image conversion worker failed: {error}"))?
}

/// Reads `path` as an immutable snapshot of exactly `byte_len` bytes and verifies optional managed `identity`.
/// 以恰好`byte_len`字节的不可变快照读取`path`，并校验可选托管`identity`。
/// Returns the bytes, or a size, read or identity error.
/// 返回字节，或大小、读取及身份错误。
async fn read_source_snapshot(
    path: &Path,
    byte_len: u64,
    identity: Option<&PreparedMediaSourceIdentity>,
) -> Result<Vec<u8>, String> {
    if byte_len == 0 || byte_len > BUFFER_LIMIT_BYTES {
        return Err(format!(
            "source size {byte_len} is outside 1..={BUFFER_LIMIT_BYTES} bytes"
        ));
    }
    // Read at most one byte beyond the admitted size to detect source growth without unbounded allocation.
    // 最多读取准入大小再加一字节，检测来源增长且避免无界分配。
    let source = tokio::fs::File::open(path)
        .await
        .map_err(|error| error.to_string())?;
    // The immutable snapshot is the only input used by the blocking decoder.
    // 不可变快照是阻塞解码器使用的唯一输入。
    let mut bytes = Vec::new();
    source
        .take(byte_len + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != byte_len {
        return Err("source size changed during image conversion".into());
    }
    if let Some(expected) = identity
        && (expected.byte_len != byte_len
            || expected.content_sha256 != hex::encode(Sha256::digest(&bytes)))
    {
        return Err("media source changed from its registered identity".into());
    }
    Ok(bytes)
}

/// Returns decoder allocation limits derived from the single conversion buffer bound.
/// 返回从唯一转换缓冲区上限派生的解码器分配限制。
fn decoder_limits() -> image::Limits {
    // Decoder limits complement the explicit raster-size check; neither permits unconstrained output.
    // 解码器限制补充显式像素大小检查；二者均不允许无界输出。
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(BUFFER_LIMIT_BYTES);
    limits
}

/// Checks `decoder` dimensions and output bytes before decoding; returns an explicit size error.
/// 在解码前检查`decoder`尺寸和输出字节数；越界返回明确大小错误。
fn check_raster(decoder: &impl ImageDecoder) -> Result<(), String> {
    // PNG normalization may require RGBA8 even when the source uses fewer bytes per pixel.
    // 即使来源每像素字节更少，PNG规范化仍可能需要RGBA8。
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || decoder.total_bytes() > BUFFER_LIMIT_BYTES
        || u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(4)
            > BUFFER_LIMIT_BYTES
    {
        return Err(format!(
            "decoded image {width}x{height} exceeds the {BUFFER_LIMIT_BYTES}-byte raster limit or has zero dimensions"
        ));
    }
    Ok(())
}

/// Decodes exact `format` from bounded `bytes`, rejecting animation and multiple pages; returns oriented pixels.
/// 从有界`bytes`按精确`format`解码，拒绝动画和多页内容；返回已应用方向的像素。
fn decode_static(bytes: &[u8], format: ImageFormat) -> Result<DynamicImage, String> {
    match format {
        ImageFormat::Gif => {
            // Inspect two frames only, so animation cannot silently collapse to a still image.
            // 仅检查两帧，避免动画被静默压成静态图片。
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
                .map_err(|error| error.to_string())?;
            decoder
                .set_limits(decoder_limits())
                .map_err(|error| error.to_string())?;
            check_raster(&decoder)?;
            // Keep the single decoded frame and reject any additional frame or decoding error.
            // 保留唯一解码帧，拒绝任何额外帧或解码错误。
            let mut frames = decoder.into_frames();
            // A missing frame is an invalid image, never a successful empty conversion.
            // 缺少帧表示无效图片，绝不视为成功的空转换。
            let frame = frames
                .next()
                .ok_or("GIF contains no image frame")?
                .map_err(|error| error.to_string())?;
            if frames
                .next()
                .transpose()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                return Err(
                    "animated GIF cannot be converted to one PNG without losing frames".into(),
                );
            }
            return Ok(DynamicImage::ImageRgba8(frame.into_buffer()));
        }
        ImageFormat::WebP => {
            // The codec exposes the container animation flag before pixel decoding.
            // 编解码器在像素解码前暴露容器动画标记。
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))
                .map_err(|error| error.to_string())?;
            if decoder.has_animation() {
                return Err(
                    "animated WebP cannot be converted to one PNG without losing frames".into(),
                );
            }
        }
        ImageFormat::Tiff => {
            // Reuse TIFF's own directory parser instead of inventing a second binary parser.
            // 复用TIFF自身目录解析器，不另造二进制解析器。
            let decoder = tiff::decoder::Decoder::new(Cursor::new(bytes))
                .map_err(|error| error.to_string())?;
            if decoder.more_images() {
                return Err(
                    "multipage TIFF cannot be converted to one PNG without losing pages".into(),
                );
            }
        }
        _ => {}
    }
    // Fixed decoder selection prevents MIME spoofing from selecting an unrelated parser.
    // 固定解码器选择，防止MIME伪装选择无关解析器。
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decoder_limits());
    // Inspect actual output size before DynamicImage allocates its pixel buffer.
    // 在DynamicImage分配像素缓冲区之前检查实际输出大小。
    let mut decoder = reader.into_decoder().map_err(|error| error.to_string())?;
    check_raster(&decoder)?;
    // Apply EXIF orientation while the source metadata is still available.
    // 在来源元数据仍可用时读取EXIF方向。
    let orientation = decoder.orientation().map_err(|error| error.to_string())?;
    // The decoded image owns its pixels independently of the original file.
    // 解码图片持有独立于原文件的像素。
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(|error| error.to_string())?;
    decoded.apply_orientation(orientation);
    Ok(decoded)
}

/// Image output writer enforcing the shared conversion bound during encoding, before disk growth.
/// 图片输出写入器，在编码及磁盘增长前执行共享转换上限。
struct BoundedOutputWriter<'a> {
    /// Owned temporary file borrowed only for encoding.
    /// 仅编码期间借用的自有临时文件。
    file: &'a mut std::fs::File,
    /// Bytes already accepted by this output writer.
    /// 此输出写入器已经接受的字节数。
    written: u64,
}

impl Write for BoundedOutputWriter<'_> {
    /// Writes `bytes` within the remaining bound, returning actual count or an explicit output-size error.
    /// 在剩余上限内写入`bytes`，返回实际数量或明确输出大小错误。
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > BUFFER_LIMIT_BYTES - self.written {
            return Err(std::io::Error::other(format!(
                "converted image exceeds {BUFFER_LIMIT_BYTES} bytes"
            )));
        }
        // Account for the actual write, including short writes from the underlying file.
        // 按实际写入量计数，包括底层文件的短写入。
        let written = self.file.write(bytes)?;
        self.written += written as u64;
        Ok(written)
    }

    /// Flushes the temporary file, forwarding the underlying I/O result.
    /// 刷新临时文件，透传底层输入输出结果。
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// Encodes bounded static `bytes` of exact `format` to a self-cleaning PNG file, returning the owner or concrete error.
/// 将精确`format`的有界静态`bytes`编码到自动清理的PNG文件，返回所有者或具体错误。
fn encode_png(bytes: &[u8], format: ImageFormat) -> Result<tempfile::NamedTempFile, String> {
    // Decode first so malformed inputs never produce a successful temporary artifact.
    // 先解码，避免畸形输入产出成功的临时产物。
    let decoded = decode_static(bytes, format)?;
    // Temporary ownership spans encoding and delivery; dropping it also removes failed outputs.
    // 临时所有权覆盖编码及交付；释放时同时删除失败输出。
    let mut file = tempfile::Builder::new()
        .prefix("vulcan-read-media-")
        .suffix(".png")
        .tempfile()
        .map_err(|error| error.to_string())?;
    decoded
        .write_with_encoder(image::codecs::png::PngEncoder::new(BoundedOutputWriter {
            file: file.as_file_mut(),
            written: 0,
        }))
        .map_err(|error| error.to_string())?;
    Ok(file)
}
