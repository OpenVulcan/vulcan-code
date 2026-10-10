//! Strict multi-encoding text decoding and lossless re-encoding for native file tools.
//! 原生文件工具使用的严格多编码文本解码与无损回编码。

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::Encoding;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};

/// Stable upper bound used before encoding detection performs any allocation.
/// 编码检测进行任何分配前使用的稳定大小上限。
pub const MAX_DETECTABLE_TEXT_BYTES: usize = 16 * 1024 * 1024;

/// Minimum non-ASCII evidence required before a legacy detector result can be writable.
/// 传统编码检测结果可用于写回前所需的最少非 ASCII 证据。
const MIN_TRUSTED_LEGACY_NON_ASCII_BYTES: usize = 8;

/// Byte window used to verify that a legacy detector result converges before end of input.
/// 用于验证传统编码检测结果在输入结束前已收敛的字节窗口。
const LEGACY_DETECTION_CHECKPOINT_BYTES: usize = 16;

/// Identifies the exact text encoding used for decoding and writing a document.
/// 标识文档解码和写回使用的精确文本编码。
#[derive(Debug, Clone, Copy)]
pub enum TextEncoding {
    /// UTF-8.
    /// UTF-8。
    Utf8,
    /// Little-endian UTF-16.
    /// 小端 UTF-16。
    Utf16Le,
    /// Big-endian UTF-16.
    /// 大端 UTF-16。
    Utf16Be,
    /// Little-endian UTF-32.
    /// 小端 UTF-32。
    Utf32Le,
    /// Big-endian UTF-32.
    /// 大端 UTF-32。
    Utf32Be,
    /// One Encoding Standard legacy codec.
    /// 一种 Encoding Standard 传统编码。
    Legacy(&'static Encoding),
}

impl TextEncoding {
    /// Returns the stable model-visible encoding label.
    /// 返回稳定的模型可见编码标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Utf32Le => "utf-32le",
            Self::Utf32Be => "utf-32be",
            Self::Legacy(encoding) => encoding.name(),
        }
    }

    /// Returns whether this encoding is compatible with the supplied BOM.
    /// 返回当前编码是否与给定 BOM 兼容。
    fn accepts_bom(self, bom: TextBom) -> bool {
        matches!(
            (self, bom),
            (_, TextBom::None)
                | (Self::Utf8, TextBom::Utf8)
                | (Self::Utf16Le, TextBom::Utf16Le)
                | (Self::Utf16Be, TextBom::Utf16Be)
                | (Self::Utf32Le, TextBom::Utf32Le)
                | (Self::Utf32Be, TextBom::Utf32Be)
        )
    }
}

/// Identifies the exact byte-order mark retained for a text document.
/// 标识为文本文档保留的精确字节顺序标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBom {
    /// No BOM.
    /// 无 BOM。
    None,
    /// UTF-8 BOM.
    /// UTF-8 BOM。
    Utf8,
    /// UTF-16 little-endian BOM.
    /// UTF-16 小端 BOM。
    Utf16Le,
    /// UTF-16 big-endian BOM.
    /// UTF-16 大端 BOM。
    Utf16Be,
    /// UTF-32 little-endian BOM.
    /// UTF-32 小端 BOM。
    Utf32Le,
    /// UTF-32 big-endian BOM.
    /// UTF-32 大端 BOM。
    Utf32Be,
}

impl TextBom {
    /// Returns whether a BOM is present.
    /// 返回是否存在 BOM。
    pub fn is_present(self) -> bool {
        self != Self::None
    }

    /// Returns the exact BOM bytes.
    /// 返回精确的 BOM 字节。
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::None => &[],
            Self::Utf8 => &[0xEF, 0xBB, 0xBF],
            Self::Utf16Le => &[0xFF, 0xFE],
            Self::Utf16Be => &[0xFE, 0xFF],
            Self::Utf32Le => &[0xFF, 0xFE, 0x00, 0x00],
            Self::Utf32Be => &[0x00, 0x00, 0xFE, 0xFF],
        }
    }
}

/// Describes how the runtime selected a document encoding.
/// 描述运行时如何选择文档编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncodingSource {
    /// The caller supplied an explicit encoding.
    /// 调用方显式提供了编码。
    Explicit,
    /// A Unicode BOM selected the encoding.
    /// Unicode BOM 选择了编码。
    Bom,
    /// Strict UTF-8 validation selected UTF-8.
    /// 严格 UTF-8 校验选择了 UTF-8。
    Utf8Validation,
    /// A strong Unicode NUL-byte pattern selected UTF-16 or UTF-32.
    /// 强 Unicode 空字节模式选择了 UTF-16 或 UTF-32。
    UnicodePattern,
    /// chardetng selected a legacy Encoding Standard codec.
    /// chardetng 选择了 Encoding Standard 传统编码。
    Detector,
}

impl TextEncodingSource {
    /// Returns the stable serialized source label.
    /// 返回稳定的序列化来源标签。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Bom => "bom",
            Self::Utf8Validation => "utf8_validation",
            Self::UnicodePattern => "unicode_pattern",
            Self::Detector => "detector",
        }
    }
}

/// One strictly decoded text payload with write-back metadata.
/// 一份带写回元数据的严格解码文本载荷。
#[derive(Debug, Clone)]
pub struct DecodedText {
    /// Unicode text without a BOM character.
    /// 不含 BOM 字符的 Unicode 文本。
    pub text: String,
    /// Exact source encoding.
    /// 精确源编码。
    pub encoding: TextEncoding,
    /// Exact source BOM.
    /// 精确源 BOM。
    pub bom: TextBom,
    /// Encoding-selection source.
    /// 编码选择来源。
    pub source: TextEncodingSource,
    /// Whether the runtime accepted the selection as safe for write-back.
    /// 运行时是否将该选择视为可安全写回。
    pub certain: bool,
}

/// One strict text-codec failure with a stable file-tool error code.
/// 一条带稳定文件工具错误码的严格文本编解码失败。
#[derive(Debug, Clone)]
pub struct TextCodecError {
    /// Stable machine-readable code.
    /// 稳定的机器可读错误码。
    pub code: &'static str,
    /// English model-facing message.
    /// 面向模型的英文消息。
    pub message: String,
}

impl TextCodecError {
    /// Creates one codec error.
    /// 创建一条编解码错误。
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl Display for TextCodecError {
    /// Renders the stable codec error.
    /// 渲染稳定的编解码错误。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for TextCodecError {}

/// Strictly decodes bytes using an optional explicit encoding label.
/// 使用可选的显式编码标签严格解码字节。
pub fn decode_text(
    bytes: &[u8],
    requested_encoding: Option<&str>,
) -> Result<DecodedText, TextCodecError> {
    if bytes.len() > MAX_DETECTABLE_TEXT_BYTES {
        return Err(TextCodecError::new(
            "output_limit_exceeded",
            "The text file exceeds the native file size limit.",
        ));
    }
    // BOM detection must test UTF-32 before UTF-16 because their little-endian prefixes overlap.
    // BOM 检测必须先测试 UTF-32 再测试 UTF-16，因为它们的小端前缀重叠。
    let (detected_bom, body) = split_bom(bytes);
    if let Some(label) = requested_encoding {
        let encoding = parse_encoding_label(label)?;
        if !encoding.accepts_bom(detected_bom) {
            return Err(TextCodecError::new(
                "unsupported_encoding",
                format!(
                    "The requested encoding {} conflicts with the file BOM.",
                    encoding.label()
                ),
            ));
        }
        let text = decode_with_encoding(body, encoding)?;
        validate_decoded_text(&text)?;
        return Ok(DecodedText {
            text,
            encoding,
            bom: detected_bom,
            source: TextEncodingSource::Explicit,
            certain: true,
        });
    }
    if detected_bom != TextBom::None {
        let encoding = encoding_for_bom(detected_bom);
        let text = decode_with_encoding(body, encoding)?;
        validate_decoded_text(&text)?;
        return Ok(DecodedText {
            text,
            encoding,
            bom: detected_bom,
            source: TextEncodingSource::Bom,
            certain: true,
        });
    }
    if let Some(encoding) = unicode_pattern_encoding(bytes)
        && let Ok(text) = decode_with_encoding(bytes, encoding)
        && validate_decoded_text(&text).is_ok()
    {
        return Ok(DecodedText {
            text,
            encoding,
            bom: TextBom::None,
            source: TextEncodingSource::UnicodePattern,
            certain: true,
        });
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        validate_decoded_text(text)?;
        return Ok(DecodedText {
            text: text.to_string(),
            encoding: TextEncoding::Utf8,
            bom: TextBom::None,
            source: TextEncodingSource::Utf8Validation,
            certain: true,
        });
    }
    if looks_binary(bytes) {
        return Err(TextCodecError::new(
            "binary_file",
            "The target contains binary control bytes and cannot be edited as text.",
        ));
    }
    // chardetng is used only after strict Unicode paths fail, and the guessed codec must round-trip.
    // chardetng 仅在严格 Unicode 路径失败后使用，并且检测到的编码必须能够往返一致。
    let (guessed, certain) = trusted_legacy_encoding(bytes)?;
    let encoding = TextEncoding::Legacy(guessed);
    let text = decode_with_encoding(bytes, encoding)?;
    validate_decoded_text(&text)?;
    let round_trip = encode_without_bom(&text, encoding)?;
    if round_trip != bytes {
        return Err(TextCodecError::new(
            "encoding_ambiguous",
            "The file encoding could not be detected without changing the original bytes. Provide encoding explicitly.",
        ));
    }
    Ok(DecodedText {
        text,
        encoding,
        bom: TextBom::None,
        source: TextEncodingSource::Detector,
        certain,
    })
}

/// Selects a legacy encoding only after the detector converges with enough non-ASCII evidence.
/// 仅在检测器凭借足够的非 ASCII 证据收敛后选择传统编码。
///
/// # Parameters
/// 参数
/// `bytes` is the complete BOM-less payload after strict Unicode paths failed.
/// `bytes` 是严格 Unicode 路径失败后的完整无 BOM 载荷。
///
/// # Returns
/// 返回值
/// A codec and whether independent checkpoints support write-back, or an explicit ambiguity error.
/// 返回编码及独立检查点是否支持写回，或返回明确歧义错误。
fn trusted_legacy_encoding(bytes: &[u8]) -> Result<(&'static Encoding, bool), TextCodecError> {
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    let mut non_ascii_count = 0usize;
    let mut supported_guesses = Vec::new();
    let chunk_count = bytes.len().div_ceil(LEGACY_DETECTION_CHECKPOINT_BYTES);
    for (index, chunk) in bytes.chunks(LEGACY_DETECTION_CHECKPOINT_BYTES).enumerate() {
        non_ascii_count =
            non_ascii_count.saturating_add(chunk.iter().filter(|byte| !byte.is_ascii()).count());
        let last = index.saturating_add(1) == chunk_count;
        let _ = detector.feed(chunk, last);
        if non_ascii_count >= MIN_TRUSTED_LEGACY_NON_ASCII_BYTES {
            supported_guesses.push(detector.guess(None, Utf8Detection::Deny));
        }
    }
    let candidate = detector.guess(None, Utf8Detection::Deny);
    let converged = supported_guesses.len() >= 2
        && supported_guesses
            .iter()
            .rev()
            .take(2)
            .all(|encoding| std::ptr::eq(*encoding, candidate));
    if converged {
        return Ok((candidate, true));
    }
    if short_legacy_sample_is_consistent(bytes, candidate)? {
        // Repeating the same sample cannot establish its original encoding; allow a labelled preview only.
        // 重复同一样本不能证明原始编码；只允许带标记的预览。
        return Ok((candidate, false));
    }
    Err(TextCodecError::new(
        "encoding_ambiguous",
        format!(
            "The file encoding is ambiguous; the statistical candidate was {} but lacked stable evidence. Retry file_read with an explicit encoding.",
            candidate.name()
        ),
    ))
}

/// Validates a short legacy sample by textual evidence and repeated detector consistency.
/// 通过文本证据与重复检测器一致性校验较短的传统编码样本。
///
/// # Parameters
/// 参数
/// `bytes` is the complete short payload and `candidate` is the full-sample detector result.
/// `bytes` 是完整短载荷，`candidate` 是完整样本的检测结果。
///
/// # Returns
/// 返回值
/// `true` only when strict decoded text is meaningful and amplification keeps the same codec.
/// 仅在严格解码文本具备意义且放大后仍保持相同编码时返回 `true`。
fn short_legacy_sample_is_consistent(
    bytes: &[u8],
    candidate: &'static Encoding,
) -> Result<bool, TextCodecError> {
    let text = decode_with_encoding(bytes, TextEncoding::Legacy(candidate))?;
    validate_decoded_text(&text)?;
    let meaningful_characters = text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count();
    if meaningful_characters < 2 {
        return Ok(false);
    }
    let repeat_count = 256usize.div_ceil(bytes.len()).clamp(2, 32);
    let mut amplified = Vec::with_capacity(bytes.len().saturating_mul(repeat_count));
    for _ in 0..repeat_count {
        amplified.extend_from_slice(bytes);
    }
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Allow);
    let _ = detector.feed(&amplified, true);
    Ok(std::ptr::eq(
        detector.guess(None, Utf8Detection::Deny),
        candidate,
    ))
}

/// Strictly encodes Unicode text using the requested encoding and BOM policy.
/// 使用请求的编码和 BOM 策略严格编码 Unicode 文本。
pub fn encode_text(
    text: &str,
    encoding: TextEncoding,
    bom: TextBom,
) -> Result<Vec<u8>, TextCodecError> {
    if !encoding.accepts_bom(bom) {
        return Err(TextCodecError::new(
            "unsupported_encoding",
            "The requested BOM does not match the selected encoding.",
        ));
    }
    let body = encode_without_bom(text, encoding)?;
    let mut bytes = Vec::with_capacity(bom.bytes().len().saturating_add(body.len()));
    bytes.extend_from_slice(bom.bytes());
    bytes.extend_from_slice(&body);
    Ok(bytes)
}

/// Resolves one public encoding label without guessing aliases in application code.
/// 在应用代码中不猜测别名地解析一个公开编码标签。
pub fn parse_encoding_label(label: &str) -> Result<TextEncoding, TextCodecError> {
    let normalized = label.trim().to_ascii_lowercase().replace('_', "-");
    let encoding = match normalized.as_str() {
        "utf-8" | "utf8" => TextEncoding::Utf8,
        "utf-16le" | "utf16le" => TextEncoding::Utf16Le,
        "utf-16be" | "utf16be" => TextEncoding::Utf16Be,
        "utf-32le" | "utf32le" => TextEncoding::Utf32Le,
        "utf-32be" | "utf32be" => TextEncoding::Utf32Be,
        _ => Encoding::for_label(normalized.as_bytes())
            .map(canonical_text_encoding)
            .ok_or_else(|| {
                TextCodecError::new(
                    "unsupported_encoding",
                    format!("Unsupported text encoding: {label}"),
                )
            })?,
    };
    Ok(encoding)
}

/// Converts Encoding Standard Unicode aliases to native Unicode variants and keeps legacy codecs distinct.
/// 将 Encoding Standard 的 Unicode 别名转换为原生 Unicode 变体，并保持传统编码独立。
fn canonical_text_encoding(encoding: &'static Encoding) -> TextEncoding {
    if std::ptr::eq(encoding, encoding_rs::UTF_8) {
        TextEncoding::Utf8
    } else if std::ptr::eq(encoding, encoding_rs::UTF_16LE) {
        TextEncoding::Utf16Le
    } else if std::ptr::eq(encoding, encoding_rs::UTF_16BE) {
        TextEncoding::Utf16Be
    } else {
        TextEncoding::Legacy(encoding)
    }
}

/// Splits a recognized Unicode BOM from its body.
/// 从正文中分离已识别的 Unicode BOM。
fn split_bom(bytes: &[u8]) -> (TextBom, &[u8]) {
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        (TextBom::Utf32Be, &bytes[4..])
    } else if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        (TextBom::Utf32Le, &bytes[4..])
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (TextBom::Utf8, &bytes[3..])
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (TextBom::Utf16Be, &bytes[2..])
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        (TextBom::Utf16Le, &bytes[2..])
    } else {
        (TextBom::None, bytes)
    }
}

/// Returns the encoding implied by one non-empty BOM.
/// 返回一个非空 BOM 所指示的编码。
fn encoding_for_bom(bom: TextBom) -> TextEncoding {
    match bom {
        TextBom::Utf8 => TextEncoding::Utf8,
        TextBom::Utf16Le => TextEncoding::Utf16Le,
        TextBom::Utf16Be => TextEncoding::Utf16Be,
        TextBom::Utf32Le => TextEncoding::Utf32Le,
        TextBom::Utf32Be => TextEncoding::Utf32Be,
        TextBom::None => TextEncoding::Utf8,
    }
}

/// Detects strong BOM-less UTF-16 and UTF-32 byte patterns.
/// 检测强无 BOM UTF-16 与 UTF-32 字节模式。
fn unicode_pattern_encoding(bytes: &[u8]) -> Option<TextEncoding> {
    if bytes.len() >= 8 && bytes.len().is_multiple_of(4) {
        let units = bytes.len() / 4;
        let le_hits = bytes
            .chunks_exact(4)
            .filter(|chunk| chunk[1] == 0 && chunk[2] == 0 && chunk[3] == 0)
            .count();
        let be_hits = bytes
            .chunks_exact(4)
            .filter(|chunk| chunk[0] == 0 && chunk[1] == 0 && chunk[2] == 0)
            .count();
        if le_hits.saturating_mul(4) >= units.saturating_mul(3) {
            return Some(TextEncoding::Utf32Le);
        }
        if be_hits.saturating_mul(4) >= units.saturating_mul(3) {
            return Some(TextEncoding::Utf32Be);
        }
    }
    if bytes.len() >= 4 && bytes.len().is_multiple_of(2) {
        let units = bytes.len() / 2;
        let le_hits = bytes.chunks_exact(2).filter(|chunk| chunk[1] == 0).count();
        let be_hits = bytes.chunks_exact(2).filter(|chunk| chunk[0] == 0).count();
        if le_hits.saturating_mul(4) >= units.saturating_mul(3) {
            return Some(TextEncoding::Utf16Le);
        }
        if be_hits.saturating_mul(4) >= units.saturating_mul(3) {
            return Some(TextEncoding::Utf16Be);
        }
    }
    None
}

/// Strictly decodes bytes with one already-resolved encoding.
/// 使用一个已解析编码严格解码字节。
fn decode_with_encoding(bytes: &[u8], encoding: TextEncoding) -> Result<String, TextCodecError> {
    match encoding {
        TextEncoding::Utf8 => std::str::from_utf8(bytes).map(str::to_string).map_err(|_| {
            TextCodecError::new("unsupported_encoding", "The file is not valid UTF-8.")
        }),
        TextEncoding::Utf16Le => decode_utf16(bytes, true),
        TextEncoding::Utf16Be => decode_utf16(bytes, false),
        TextEncoding::Utf32Le => decode_utf32(bytes, true),
        TextEncoding::Utf32Be => decode_utf32(bytes, false),
        TextEncoding::Legacy(legacy) => legacy
            .decode_without_bom_handling_and_without_replacement(bytes)
            .map(Cow::into_owned)
            .ok_or_else(|| {
                TextCodecError::new(
                    "unsupported_encoding",
                    format!(
                        "The file contains invalid {} byte sequences.",
                        legacy.name()
                    ),
                )
            }),
    }
}

/// Strictly decodes one endian-specific UTF-16 payload.
/// 严格解码一个指定端序的 UTF-16 载荷。
fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, TextCodecError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(TextCodecError::new(
            "unsupported_encoding",
            "The UTF-16 payload has an odd byte length.",
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| {
        TextCodecError::new(
            "unsupported_encoding",
            "The UTF-16 payload contains an unpaired surrogate.",
        )
    })
}

/// Strictly decodes one endian-specific UTF-32 payload.
/// 严格解码一个指定端序的 UTF-32 载荷。
fn decode_utf32(bytes: &[u8], little_endian: bool) -> Result<String, TextCodecError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(TextCodecError::new(
            "unsupported_encoding",
            "The UTF-32 payload length is not divisible by four.",
        ));
    }
    let mut text = String::new();
    for chunk in bytes.chunks_exact(4) {
        let scalar = if little_endian {
            u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        } else {
            u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
        };
        let character = char::from_u32(scalar).ok_or_else(|| {
            TextCodecError::new(
                "unsupported_encoding",
                "The UTF-32 payload contains an invalid Unicode scalar.",
            )
        })?;
        text.push(character);
    }
    Ok(text)
}

/// Strictly encodes text without a BOM.
/// 严格编码不含 BOM 的文本。
fn encode_without_bom(text: &str, encoding: TextEncoding) -> Result<Vec<u8>, TextCodecError> {
    match encoding {
        TextEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
        TextEncoding::Utf16Le => Ok(text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()),
        TextEncoding::Utf16Be => Ok(text
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>()),
        TextEncoding::Utf32Le => Ok(text
            .chars()
            .flat_map(|character| u32::from(character).to_le_bytes())
            .collect::<Vec<_>>()),
        TextEncoding::Utf32Be => Ok(text
            .chars()
            .flat_map(|character| u32::from(character).to_be_bytes())
            .collect::<Vec<_>>()),
        TextEncoding::Legacy(legacy) => {
            let (encoded, _actual, had_errors) = legacy.encode(text);
            if had_errors {
                return Err(TextCodecError::new(
                    "encoding_unrepresentable",
                    format!(
                        "The edited text cannot be represented by {} without replacement characters.",
                        legacy.name()
                    ),
                ));
            }
            Ok(encoded.into_owned())
        }
    }
}

/// Rejects control-heavy byte payloads after Unicode candidates have been exhausted.
/// 在 Unicode 候选耗尽后拒绝控制字节过多的载荷。
fn looks_binary(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if bytes.contains(&0) {
        return true;
    }
    let controls = bytes
        .iter()
        .filter(|byte| matches!(byte, 0x00..=0x08 | 0x0B | 0x0E..=0x1F | 0x7F))
        .count();
    controls.saturating_mul(100) > bytes.len().saturating_mul(10)
}

/// Rejects decoded control-heavy payloads without confusing Unicode storage NULs with content.
/// 拒绝解码后控制字符过多的载荷，同时避免把 Unicode 存储空字节误当正文。
fn validate_decoded_text(text: &str) -> Result<(), TextCodecError> {
    if text.chars().any(|character| character == '\0') {
        return Err(TextCodecError::new(
            "binary_file",
            "The decoded target contains NUL characters and cannot be edited as text.",
        ));
    }
    let character_count = text.chars().count();
    if character_count == 0 {
        return Ok(());
    }
    let control_count = text
        .chars()
        .filter(|character| character.is_control() && !matches!(*character, '\t' | '\n' | '\r'))
        .count();
    if control_count.saturating_mul(100) > character_count.saturating_mul(10) {
        return Err(TextCodecError::new(
            "binary_file",
            "The decoded target contains too many control characters to edit safely as text.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        TextBom, TextEncoding, TextEncodingSource, decode_text, encode_text, parse_encoding_label,
    };

    /// Verifies strict Unicode BOM detection and byte-perfect write-back.
    /// 验证严格 Unicode BOM 检测与字节完全一致的写回。
    #[test]
    fn unicode_bom_round_trip_is_lossless() {
        let cases = [
            vec![0xEF, 0xBB, 0xBF, b'a', b'\n'],
            vec![0xFF, 0xFE, b'a', 0, 0x0A, 0],
            vec![0xFE, 0xFF, 0, b'a', 0, 0x0A],
            vec![0xFF, 0xFE, 0, 0, b'a', 0, 0, 0, 0x0A, 0, 0, 0],
            vec![0, 0, 0xFE, 0xFF, 0, 0, 0, b'a', 0, 0, 0, 0x0A],
        ];
        for original in cases {
            let decoded = decode_text(&original, None).expect("Unicode BOM should decode");
            assert_eq!(decoded.text, "a\n");
            assert_eq!(decoded.source, TextEncodingSource::Bom);
            let encoded = encode_text(&decoded.text, decoded.encoding, decoded.bom)
                .expect("Unicode BOM should encode");
            assert_eq!(encoded, original);
        }
    }

    /// Verifies strong BOM-less UTF-16 byte patterns are detected in both byte orders.
    /// 验证强无 BOM UTF-16 字节模式可按两种字节序检测。
    #[test]
    fn bomless_utf16_patterns_detect_both_byte_orders() {
        let little_endian = b"a\0b\0c\0d\0";
        let big_endian = b"\0a\0b\0c\0d";
        let little = decode_text(little_endian, None).expect("UTF-16LE should decode");
        let big = decode_text(big_endian, None).expect("UTF-16BE should decode");
        assert_eq!(little.text, "abcd");
        assert_eq!(big.text, "abcd");
        assert_eq!(little.encoding.label(), "utf-16le");
        assert_eq!(big.encoding.label(), "utf-16be");
        assert_eq!(little.source, TextEncodingSource::UnicodePattern);
        assert_eq!(big.source, TextEncodingSource::UnicodePattern);
    }

    /// Verifies BOM-less UTF-32 detection runs before binary rejection.
    /// 验证无 BOM UTF-32 检测先于二进制拒绝执行。
    #[test]
    fn bomless_utf32_pattern_is_not_binary() {
        let original = b"a\0\0\0b\0\0\0";
        let decoded = decode_text(original, None).expect("UTF-32LE should decode");
        assert_eq!(decoded.text, "ab");
        assert_eq!(decoded.encoding.label(), "utf-32le");
    }

    /// Verifies legacy codecs reject characters that cannot be represented.
    /// 验证传统编码拒绝无法表示的字符。
    #[test]
    fn legacy_encoding_rejects_unrepresentable_text() {
        let encoding = encoding_rs::WINDOWS_1252;
        let error = encode_text("snowman ☃", TextEncoding::Legacy(encoding), TextBom::None)
            .expect_err("windows-1252 must reject the snowman");
        assert_eq!(error.code, "encoding_unrepresentable");
    }

    /// Verifies the supported legacy encoding matrix round-trips through explicit labels.
    /// 验证受支持的传统编码矩阵可通过显式标签严格往返。
    #[test]
    fn explicit_legacy_encoding_matrix_round_trips() {
        let cases = [
            (encoding_rs::GBK, "简体中文"),
            (encoding_rs::BIG5, "繁體中文"),
            (encoding_rs::SHIFT_JIS, "日本語"),
            (encoding_rs::EUC_JP, "日本語"),
            (encoding_rs::EUC_KR, "한국어"),
            (encoding_rs::WINDOWS_1251, "Русский"),
            (encoding_rs::WINDOWS_1252, "café"),
        ];
        for (encoding, text) in cases {
            let encoded = encode_text(text, TextEncoding::Legacy(encoding), TextBom::None)
                .expect("legacy sample should encode");
            let decoded = decode_text(&encoded, Some(encoding.name()))
                .expect("legacy sample should decode explicitly");
            assert_eq!(decoded.text, text);
            assert_eq!(decoded.encoding.label(), encoding.name());
            assert_eq!(
                encode_text(&decoded.text, decoded.encoding, decoded.bom)
                    .expect("legacy sample should re-encode"),
                encoded
            );
        }
    }

    /// Verifies Encoding Standard Unicode aliases retain native BOM and strict codec semantics.
    /// 验证 Encoding Standard 的 Unicode 别名保留原生 BOM 与严格编解码语义。
    #[test]
    fn unicode_aliases_resolve_to_native_codecs() {
        let utf8 = parse_encoding_label("unicode-1-1-utf-8")
            .expect("registered UTF-8 alias should resolve");
        let utf16 = parse_encoding_label("utf-16").expect("registered UTF-16 alias should resolve");

        assert_eq!(utf8.label(), "utf-8");
        assert_eq!(utf16.label(), "utf-16le");
        assert_eq!(
            encode_text("text", utf16, TextBom::Utf16Le)
                .expect("native UTF-16 alias should accept its BOM"),
            [0xFF, 0xFE, b't', 0, b'e', 0, b'x', 0, b't', 0]
        );
    }

    /// Verifies sparse legacy bytes require an explicit encoding instead of accepting a default guess.
    /// 验证稀疏传统编码字节必须显式指定编码，而不是接受默认猜测。
    #[test]
    fn sparse_legacy_detection_is_reported_as_ambiguous() {
        let error = decode_text(&[0x80], None)
            .expect_err("one legacy byte cannot provide trustworthy statistical evidence");
        assert_eq!(error.code, "encoding_ambiguous");
        assert!(error.message.contains("explicit encoding"));
    }

    /// Verifies a stable, sufficiently long legacy sample remains automatically writable.
    /// 验证稳定且足够长的传统编码样本仍可自动识别并安全写回。
    #[test]
    fn stable_legacy_detection_round_trips_without_an_override() {
        let source =
            "这是一个用于验证传统字符集自动检测稳定性的较长中文文本。它包含足够多的统计信息。";
        let bytes = encode_text(
            source,
            TextEncoding::Legacy(encoding_rs::GBK),
            TextBom::None,
        )
        .expect("GBK sample should encode");
        let decoded = decode_text(&bytes, None).expect("stable GBK sample should auto-detect");
        assert_eq!(decoded.text, source);
        assert_eq!(decoded.encoding.label(), encoding_rs::GBK.name());
        assert_eq!(decoded.source, TextEncodingSource::Detector);
        assert!(
            decoded.certain,
            "independent checkpoints must support this long fixture"
        );
        assert_eq!(
            encode_text(&decoded.text, decoded.encoding, decoded.bom)
                .expect("detected sample should re-encode"),
            bytes
        );
    }

    /// Verifies the gb18030 label follows the Encoding Standard GBK-compatible codec path.
    /// 验证 gb18030 标签使用 Encoding Standard 的 GBK 兼容编解码路径。
    #[test]
    fn gb18030_label_uses_registered_compatible_codec() {
        let encoding = parse_encoding_label("gb18030").expect("gb18030 should be registered");
        let encoded = encode_text("中文", encoding, TextBom::None)
            .expect("GB18030-compatible sample should encode");
        let decoded = decode_text(&encoded, Some("gb18030"))
            .expect("GB18030-compatible sample should decode");
        assert_eq!(decoded.text, "中文");
    }

    /// Verifies valid UTF-8 containing NUL bytes is still rejected as binary content.
    /// 验证包含 NUL 字节的合法 UTF-8 仍会被拒绝为二进制正文。
    #[test]
    fn utf8_nul_content_is_rejected_as_binary() {
        let error = decode_text(b"text\0payload", None)
            .expect_err("NUL-bearing UTF-8 should not become an editable text snapshot");
        assert_eq!(error.code, "binary_file");
    }
}
