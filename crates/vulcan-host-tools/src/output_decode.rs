//! Shared shell-output decoding helpers for one-shot and streaming command execution.
//! 供单次命令与流式命令执行共用的 shell 输出解码辅助能力。

use encoding_rs::{
    BIG5, CoderResult, Decoder, EUC_KR, Encoding, GB18030, IBM866, SHIFT_JIS, UTF_8, WINDOWS_874,
    WINDOWS_1250, WINDOWS_1251, WINDOWS_1252, WINDOWS_1253, WINDOWS_1254, WINDOWS_1255,
    WINDOWS_1256, WINDOWS_1257, WINDOWS_1258,
};
use std::char::decode_utf16;
use std::mem;
use sys_locale::get_locale;

/// Stable UTF-8 BOM prefix recognized at the start of shell output.
/// 在 shell 输出起始位置识别的稳定 UTF-8 BOM 前缀。
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Stable UTF-16 little-endian BOM prefix recognized at the start of shell output.
/// 在 shell 输出起始位置识别的稳定 UTF-16 小端 BOM 前缀。
const UTF16_LE_BOM: [u8; 2] = [0xFF, 0xFE];

/// Stable UTF-16 big-endian BOM prefix recognized at the start of shell output.
/// 在 shell 输出起始位置识别的稳定 UTF-16 大端 BOM 前缀。
const UTF16_BE_BOM: [u8; 2] = [0xFE, 0xFF];

/// Minimum probe size used before deciding that one non-UTF-8 shell stream should fall back to a legacy code page.
/// 在判定非 UTF-8 shell 流应回退到传统代码页前使用的最小探测字节数。
const UTF8_PROBE_MIN_BYTES: usize = 4;

/// Decodes one complete shell-output byte slice into display-ready text using platform-aware charset fallback.
/// 使用具备平台感知回退策略的字符集转换，将一段完整 shell 输出字节流解码为可展示文本。
pub fn decode_shell_output_bytes(bytes: &[u8]) -> String {
    decode_shell_output_bytes_with_fallback(bytes, preferred_legacy_shell_encoding())
}

/// One incremental decoder mode used while the stream is still active.
/// 流仍在进行时使用的一种增量解码模式。
enum ShellOutputStreamDecodeMode {
    /// Output has not produced enough signal yet, so detection is still deferred.
    /// 输出还未产生足够信号，因此仍延后做编码判定。
    Undecided,
    /// Output has been bound to one byte-oriented encoding decoder.
    /// 输出已经绑定到某个字节型编码解码器。
    ByteEncoding(Decoder),
    /// Output has been bound to one UTF-16 stream decoder.
    /// 输出已经绑定到某个 UTF-16 流式解码器。
    Utf16(Utf16StreamDecoder),
}

/// One shared incremental shell-output decoder reused by both stdout and stderr stream pumps.
/// 同时供 stdout 与 stderr 流读取器复用的一份共享增量 shell 输出解码器。
pub struct ShellOutputStreamDecoder {
    /// Current decoding mode selected for the active stream.
    /// 当前活跃流已选定的解码模式。
    mode: ShellOutputStreamDecodeMode,
    /// Raw bytes buffered before the runtime has enough evidence to lock one charset.
    /// 运行时在拥有足够证据锁定字符集之前暂存的原始字节。
    pending_probe_bytes: Vec<u8>,
    /// Preferred legacy fallback encoding derived from the host platform.
    /// 从宿主平台推导出的首选传统回退编码。
    fallback_encoding: &'static Encoding,
}

impl Default for ShellOutputStreamDecoder {
    /// Builds one incremental shell-output decoder with the platform-preferred legacy fallback encoding.
    /// 使用平台首选传统回退编码构建一份增量 shell 输出解码器。
    fn default() -> Self {
        Self::new()
    }
}

impl ShellOutputStreamDecoder {
    /// Builds one incremental shell-output decoder with platform-aware fallback behavior.
    /// 构建一份具备平台感知回退行为的增量 shell 输出解码器。
    pub fn new() -> Self {
        Self::new_with_fallback(preferred_legacy_shell_encoding())
    }

    /// Feeds one new byte chunk into the decoder and returns the newly decoded text fragment.
    /// 向解码器喂入一段新的字节分片，并返回新解出的文本片段。
    pub fn push_bytes(&mut self, bytes: &[u8]) -> String {
        if bytes.is_empty() {
            return String::new();
        }
        match &mut self.mode {
            ShellOutputStreamDecodeMode::Undecided => {
                self.pending_probe_bytes.extend_from_slice(bytes);
                self.drain_pending_probe_bytes(false)
            }
            ShellOutputStreamDecodeMode::ByteEncoding(decoder) => {
                decode_byte_stream(decoder, bytes, false)
            }
            ShellOutputStreamDecodeMode::Utf16(decoder) => decoder.decode_bytes(bytes, false),
        }
    }

    /// Flushes the decoder after the stream has ended and returns any remaining decoded text.
    /// 在流结束后刷新解码器，并返回剩余尚未输出的解码文本。
    pub fn finish(&mut self) -> String {
        match &mut self.mode {
            ShellOutputStreamDecodeMode::Undecided => self.drain_pending_probe_bytes(true),
            ShellOutputStreamDecodeMode::ByteEncoding(decoder) => {
                decode_byte_stream(decoder, &[], true)
            }
            ShellOutputStreamDecodeMode::Utf16(decoder) => decoder.decode_bytes(&[], true),
        }
    }

    /// Builds one test-scoped decoder pinned to the supplied legacy fallback encoding.
    /// 构建一份测试专用解码器，并将传统回退编码固定为给定值。
    #[cfg(test)]
    fn new_with_test_fallback(fallback_encoding: &'static Encoding) -> Self {
        Self::new_with_fallback(fallback_encoding)
    }

    /// Builds one decoder bound to the supplied legacy fallback encoding.
    /// 构建一份绑定到给定传统回退编码的解码器。
    fn new_with_fallback(fallback_encoding: &'static Encoding) -> Self {
        Self {
            mode: ShellOutputStreamDecodeMode::Undecided,
            pending_probe_bytes: Vec::new(),
            fallback_encoding,
        }
    }

    /// Drains the probe buffer once enough evidence exists, while keeping ASCII chunks cheap and charset-safe.
    /// 在证据足够时排空探测缓冲，同时让 ASCII 分片保持低成本且字符集安全。
    fn drain_pending_probe_bytes(&mut self, is_last: bool) -> String {
        if self.pending_probe_bytes.is_empty() {
            return String::new();
        }

        if is_partial_prefix(&self.pending_probe_bytes, &UTF16_LE_BOM) && !is_last {
            return String::new();
        }
        if self.pending_probe_bytes.starts_with(&UTF16_LE_BOM) {
            return self.promote_to_utf16_decoder(true, UTF16_LE_BOM.len(), is_last);
        }
        if is_partial_prefix(&self.pending_probe_bytes, &UTF16_BE_BOM) && !is_last {
            return String::new();
        }
        if self.pending_probe_bytes.starts_with(&UTF16_BE_BOM) {
            return self.promote_to_utf16_decoder(false, UTF16_BE_BOM.len(), is_last);
        }
        if is_partial_prefix(&self.pending_probe_bytes, &UTF8_BOM) && !is_last {
            return String::new();
        }
        if self.pending_probe_bytes.starts_with(&UTF8_BOM) {
            return self.promote_to_byte_decoder(UTF_8, UTF8_BOM.len(), is_last);
        }

        // ASCII bytes are emitted immediately because every supported fallback encoding preserves ASCII verbatim.
        // ASCII 字节会立刻输出，因为所有支持的回退编码都会逐字节保留 ASCII。
        if self.pending_probe_bytes.iter().all(u8::is_ascii) {
            let ascii_text = String::from_utf8_lossy(&self.pending_probe_bytes).into_owned();
            self.pending_probe_bytes.clear();
            return ascii_text;
        }

        match std::str::from_utf8(&self.pending_probe_bytes) {
            Ok(_) => self.promote_to_byte_decoder(UTF_8, 0, is_last),
            Err(error) if error.error_len().is_none() && error.valid_up_to() > 0 => {
                self.promote_to_byte_decoder(UTF_8, 0, is_last)
            }
            Err(error)
                if error.error_len().is_none()
                    && !is_last
                    && self.pending_probe_bytes.len() < UTF8_PROBE_MIN_BYTES =>
            {
                String::new()
            }
            Err(_) => self.promote_to_byte_decoder(self.fallback_encoding, 0, is_last),
        }
    }

    /// Locks the stream onto one byte-oriented decoder and drains the current probe bytes through it.
    /// 将当前流锁定到某个字节型解码器，并通过它排空当前探测字节。
    fn promote_to_byte_decoder(
        &mut self,
        encoding: &'static Encoding,
        skip_prefix_bytes: usize,
        is_last: bool,
    ) -> String {
        let pending_bytes = mem::take(&mut self.pending_probe_bytes);
        let mut decoder = encoding.new_decoder();
        let decoded_text =
            decode_byte_stream(&mut decoder, &pending_bytes[skip_prefix_bytes..], is_last);
        self.mode = ShellOutputStreamDecodeMode::ByteEncoding(decoder);
        decoded_text
    }

    /// Locks the stream onto one UTF-16 decoder and drains the current probe bytes through it.
    /// 将当前流锁定到某个 UTF-16 解码器，并通过它排空当前探测字节。
    fn promote_to_utf16_decoder(
        &mut self,
        little_endian: bool,
        skip_prefix_bytes: usize,
        is_last: bool,
    ) -> String {
        let pending_bytes = mem::take(&mut self.pending_probe_bytes);
        let mut decoder = Utf16StreamDecoder::new(little_endian);
        let decoded_text = decoder.decode_bytes(&pending_bytes[skip_prefix_bytes..], is_last);
        self.mode = ShellOutputStreamDecodeMode::Utf16(decoder);
        decoded_text
    }
}

/// One UTF-16 stream decoder that preserves incomplete trailing bytes across incremental reads.
/// 一份会在增量读取之间保留不完整尾字节的 UTF-16 流式解码器。
struct Utf16StreamDecoder {
    /// Whether the decoder should interpret incoming code units as little-endian.
    /// 解码器是否应按小端方式解释输入码元。
    little_endian: bool,
    /// Optional dangling byte retained from the prior chunk until the next chunk arrives.
    /// 从上一分片保留到下一分片的可选悬空字节。
    pending_byte: Option<u8>,
}

impl Utf16StreamDecoder {
    /// Builds one UTF-16 stream decoder with the requested byte order.
    /// 根据所需字节序构建一份 UTF-16 流式解码器。
    fn new(little_endian: bool) -> Self {
        Self {
            little_endian,
            pending_byte: None,
        }
    }

    /// Decodes one UTF-16 byte chunk and flushes the final dangling byte as a replacement character when needed.
    /// 解码一段 UTF-16 字节分片，并在需要时把最终悬空字节刷新为替代字符。
    fn decode_bytes(&mut self, bytes: &[u8], is_last: bool) -> String {
        let mut code_units =
            Vec::with_capacity((bytes.len() + usize::from(self.pending_byte.is_some())) / 2);
        for byte in bytes.iter().copied() {
            match self.pending_byte.take() {
                Some(first_byte) => {
                    let code_unit = if self.little_endian {
                        u16::from_le_bytes([first_byte, byte])
                    } else {
                        u16::from_be_bytes([first_byte, byte])
                    };
                    code_units.push(code_unit);
                }
                None => {
                    self.pending_byte = Some(byte);
                }
            }
        }

        let mut decoded_text = decode_utf16(code_units)
            .map(|result| result.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect::<String>();
        if is_last && self.pending_byte.take().is_some() {
            decoded_text.push(char::REPLACEMENT_CHARACTER);
        }
        decoded_text
    }
}

/// Decodes one byte chunk through an `encoding_rs` decoder while preserving incremental decoder state.
/// 通过 `encoding_rs` 解码器解码一段字节分片，并保留增量解码状态。
fn decode_byte_stream(decoder: &mut Decoder, bytes: &[u8], is_last: bool) -> String {
    let mut remaining_bytes = bytes;
    let mut decoded_text = String::new();
    loop {
        let reserve_bytes = decoder
            .max_utf8_buffer_length(remaining_bytes.len())
            .unwrap_or_else(|| remaining_bytes.len().saturating_mul(4).saturating_add(4))
            .max(4);
        decoded_text.reserve(reserve_bytes);
        let (result, read_bytes, _) =
            decoder.decode_to_string(remaining_bytes, &mut decoded_text, is_last);
        remaining_bytes = &remaining_bytes[read_bytes..];
        if matches!(result, CoderResult::InputEmpty) {
            break;
        }
    }
    decoded_text
}

/// Returns whether the provided bytes still form one partial prefix of the target BOM marker.
/// 返回给定字节是否仍构成目标 BOM 标记的部分前缀。
fn is_partial_prefix(bytes: &[u8], marker: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() < marker.len()
        && marker
            .iter()
            .zip(bytes.iter())
            .all(|(expected_byte, actual_byte)| expected_byte == actual_byte)
}

/// Decodes one complete shell-output byte slice with the supplied legacy fallback encoding.
/// 使用给定传统回退编码解码一段完整 shell 输出字节流。
fn decode_shell_output_bytes_with_fallback(
    bytes: &[u8],
    fallback_encoding: &'static Encoding,
) -> String {
    let mut decoder = ShellOutputStreamDecoder::new_with_fallback(fallback_encoding);
    let mut decoded_text = decoder.push_bytes(bytes);
    decoded_text.push_str(&decoder.finish());
    decoded_text
}

/// Returns the host-preferred legacy shell-output encoding used when UTF-8 decoding is not viable.
/// 在 UTF-8 解码不可行时，返回宿主首选的传统 shell 输出编码。
fn preferred_legacy_shell_encoding() -> &'static Encoding {
    preferred_legacy_shell_encoding_from_locale(get_locale().as_deref())
}

/// Returns the locale-derived legacy shell-output encoding used after UTF-8 probing fails.
/// 在 UTF-8 探测失败后，返回从 locale 推导出的传统 shell 输出编码。
fn preferred_legacy_shell_encoding_from_locale(locale_tag: Option<&str>) -> &'static Encoding {
    let normalized_locale = locale_tag
        .map(str::trim)
        .filter(|locale| !locale.is_empty())
        .unwrap_or("en-US")
        .replace('_', "-")
        .to_ascii_lowercase();
    if normalized_locale.starts_with("zh-hant")
        || normalized_locale.starts_with("zh-tw")
        || normalized_locale.starts_with("zh-hk")
        || normalized_locale.starts_with("zh-mo")
    {
        return BIG5;
    }
    if normalized_locale.starts_with("zh") {
        return GB18030;
    }
    if normalized_locale.starts_with("ja") {
        return SHIFT_JIS;
    }
    if normalized_locale.starts_with("ko") {
        return EUC_KR;
    }
    if normalized_locale.starts_with("th") {
        return WINDOWS_874;
    }
    if normalized_locale.starts_with("ru")
        || normalized_locale.starts_with("uk")
        || normalized_locale.starts_with("bg")
        || normalized_locale.starts_with("mk")
        || normalized_locale.starts_with("sr")
    {
        return WINDOWS_1251;
    }
    if normalized_locale.starts_with("pl")
        || normalized_locale.starts_with("cs")
        || normalized_locale.starts_with("sk")
        || normalized_locale.starts_with("sl")
        || normalized_locale.starts_with("hr")
        || normalized_locale.starts_with("hu")
        || normalized_locale.starts_with("ro")
        || normalized_locale.starts_with("sq")
    {
        return WINDOWS_1250;
    }
    if normalized_locale.starts_with("el") {
        return WINDOWS_1253;
    }
    if normalized_locale.starts_with("tr") {
        return WINDOWS_1254;
    }
    if normalized_locale.starts_with("he") {
        return WINDOWS_1255;
    }
    if normalized_locale.starts_with("ar") {
        return WINDOWS_1256;
    }
    if normalized_locale.starts_with("et")
        || normalized_locale.starts_with("lt")
        || normalized_locale.starts_with("lv")
    {
        return WINDOWS_1257;
    }
    if normalized_locale.starts_with("vi") {
        return WINDOWS_1258;
    }
    if normalized_locale.starts_with("be") {
        return IBM866;
    }
    if normalized_locale.starts_with("en")
        || normalized_locale.starts_with("de")
        || normalized_locale.starts_with("fr")
        || normalized_locale.starts_with("es")
        || normalized_locale.starts_with("it")
        || normalized_locale.starts_with("nl")
        || normalized_locale.starts_with("pt")
        || normalized_locale.starts_with("da")
        || normalized_locale.starts_with("fi")
        || normalized_locale.starts_with("nb")
        || normalized_locale.starts_with("sv")
    {
        return WINDOWS_1252;
    }
    UTF_8
}

#[cfg(test)]
mod tests {
    use super::{
        ShellOutputStreamDecoder, decode_shell_output_bytes_with_fallback,
        preferred_legacy_shell_encoding_from_locale,
    };
    use encoding_rs::{GBK, UTF_8};

    /// Encodes one test string through the supplied encoding and asserts that no replacement was needed.
    /// 通过给定编码对测试字符串做一次编码，并断言过程中不需要替代字符。
    fn encode_test_text(encoding: &'static encoding_rs::Encoding, text: &str) -> Vec<u8> {
        let (encoded_bytes, _, had_errors) = encoding.encode(text);
        assert!(
            !had_errors,
            "test fixture encoding should not require replacement"
        );
        encoded_bytes.into_owned()
    }

    /// Verifies that complete valid UTF-8 output stays on the UTF-8 path.
    /// 验证完整且合法的 UTF-8 输出会继续走 UTF-8 路径。
    #[test]
    fn decode_shell_output_bytes_prefers_utf8_when_input_is_valid_utf8() {
        let encoded_bytes = encode_test_text(UTF_8, "PowerShell 输出正常");
        let decoded_text = decode_shell_output_bytes_with_fallback(&encoded_bytes, GBK);
        assert_eq!(decoded_text, "PowerShell 输出正常");
    }

    /// Verifies that complete GBK output can be recovered through the legacy fallback path.
    /// 验证完整 GBK 输出可以通过传统回退路径正确恢复。
    #[test]
    fn decode_shell_output_bytes_uses_legacy_fallback_for_gbk_bytes() {
        let encoded_bytes = encode_test_text(GBK, "错误位置 字符 33");
        let decoded_text = decode_shell_output_bytes_with_fallback(&encoded_bytes, GBK);
        assert_eq!(decoded_text, "错误位置 字符 33");
    }

    /// Verifies that UTF-8 multibyte characters split across reads are reassembled before emission.
    /// 验证跨多次读取拆开的 UTF-8 多字节字符会在输出前重新组装。
    #[test]
    fn shell_output_stream_decoder_reassembles_split_utf8_sequences() {
        let encoded_bytes = encode_test_text(UTF_8, "命令输出");
        let mut decoder = ShellOutputStreamDecoder::new_with_test_fallback(GBK);
        let first_fragment = decoder.push_bytes(&encoded_bytes[..2]);
        let second_fragment = decoder.push_bytes(&encoded_bytes[2..]);
        let final_fragment = decoder.finish();
        assert_eq!(first_fragment, "");
        assert_eq!(format!("{second_fragment}{final_fragment}"), "命令输出");
    }

    /// Verifies that GBK multibyte characters split across reads are reassembled before emission.
    /// 验证跨多次读取拆开的 GBK 多字节字符会在输出前重新组装。
    #[test]
    fn shell_output_stream_decoder_reassembles_split_gbk_sequences() {
        let encoded_bytes = encode_test_text(GBK, "错误输出");
        let mut decoder = ShellOutputStreamDecoder::new_with_test_fallback(GBK);
        let first_fragment = decoder.push_bytes(&encoded_bytes[..1]);
        let second_fragment = decoder.push_bytes(&encoded_bytes[1..3]);
        let third_fragment = decoder.push_bytes(&encoded_bytes[3..]);
        let final_fragment = decoder.finish();
        assert_eq!(first_fragment, "");
        assert_eq!(
            format!("{second_fragment}{third_fragment}{final_fragment}"),
            "错误输出"
        );
    }

    /// Verifies that Simplified Chinese locales resolve to the Chinese legacy fallback family.
    /// 验证简体中文 locale 会解析到中文传统回退编码族。
    #[test]
    fn preferred_legacy_shell_encoding_from_locale_maps_simplified_chinese_to_gb18030() {
        let encoding = preferred_legacy_shell_encoding_from_locale(Some("zh-CN"));
        assert_eq!(encoding.name(), "gb18030");
    }
}
