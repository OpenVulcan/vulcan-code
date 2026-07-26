//! Strict decoding helpers for complete user-authored text resources.
//! 完整用户文本资源使用的严格解码辅助能力。

use encoding_rs::GB18030;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Stable UTF-8 BOM prefix accepted at the start of one complete user text file.
/// 一个完整用户文本文件起始位置允许出现的稳定 UTF-8 BOM 前缀。
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Owns complete user-text content together with the exact decoder selected at the external byte boundary.
/// 保存完整用户文本内容，以及在外部字节边界选中的精确解码器标签。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedUserText {
    /// Complete Unicode text ready for internal UTF-8 processing.
    /// 可供内部 UTF-8 流程处理的完整 Unicode 文本。
    pub content: String,
    /// Stable lowercase encoding label reported to callers and the WebUI.
    /// 返回给调用方及 WebUI 的稳定小写编码标签。
    pub encoding: &'static str,
}

/// Reports that one complete user-text byte sequence is neither valid UTF-8 nor valid GB18030.
/// 表示一段完整用户文本字节既不是有效 UTF-8，也不是有效 GB18030。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserTextDecodeError;

impl Display for UserTextDecodeError {
    /// Formats the deterministic decoding failure without exposing replacement characters as valid data.
    /// 格式化确定性的解码失败，避免将替代字符作为有效数据暴露。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("input is neither valid UTF-8 nor valid GB18030")
    }
}

impl Error for UserTextDecodeError {}

/// Decodes one complete user-authored text file as strict UTF-8 or strict GB18030, including GBK.
/// 将一个完整的用户文本文件严格解码为 UTF-8 或 GB18030（包含 GBK）。
///
/// Parameters: `bytes` must contain the complete file payload rather than a truncated preview.
/// Returns: decoded Unicode text and its stable encoding label, or an explicit error when both
/// supported decoders reject the byte sequence.
/// 参数：`bytes` 必须包含完整文件载荷，而不是被截断的预览。
/// 返回值：解码后的 Unicode 文本及稳定编码标签；当两种受支持解码器均拒绝该字节序列时返回显式错误。
pub fn decode_complete_user_text(bytes: &[u8]) -> Result<DecodedUserText, UserTextDecodeError> {
    let utf8_bytes = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
    if let Ok(content) = std::str::from_utf8(utf8_bytes) {
        return Ok(DecodedUserText {
            content: content.to_owned(),
            encoding: "utf-8",
        });
    }
    let content = GB18030
        .decode_without_bom_handling_and_without_replacement(bytes)
        .ok_or(UserTextDecodeError)?;
    Ok(DecodedUserText {
        content: content.into_owned(),
        encoding: "gb18030",
    })
}

#[cfg(test)]
mod tests {
    use super::{UserTextDecodeError, decode_complete_user_text};
    use encoding_rs::GBK;

    /// Encodes one test string as GBK and verifies the fixture requires no replacement.
    /// 将测试字符串编码为 GBK，并验证夹具不需要替代字符。
    fn encode_gbk_fixture(text: &str) -> Vec<u8> {
        let (encoded, _, had_errors) = GBK.encode(text);
        assert!(!had_errors);
        encoded.into_owned()
    }

    /// Verifies complete user text keeps valid UTF-8 and removes its optional BOM.
    /// 验证完整用户文本会保留有效 UTF-8，并移除可选 BOM。
    #[test]
    fn decode_complete_user_text_accepts_utf8_with_optional_bom() {
        let plain = decode_complete_user_text("知识库内容".as_bytes()).expect("decode UTF-8");
        let with_bom =
            decode_complete_user_text("\u{FEFF}知识库".as_bytes()).expect("decode UTF-8 BOM");

        assert_eq!(plain.content, "知识库内容");
        assert_eq!(plain.encoding, "utf-8");
        assert_eq!(with_bom.content, "知识库");
        assert_eq!(with_bom.encoding, "utf-8");
    }

    /// Verifies complete GBK user text is recovered through the strict GB18030 superset.
    /// 验证完整 GBK 用户文本会通过严格的 GB18030 超集正确恢复。
    #[test]
    fn decode_complete_user_text_accepts_gbk_through_gb18030() {
        let encoded = encode_gbk_fixture("服务端用户等级");
        let decoded = decode_complete_user_text(&encoded).expect("decode GBK");

        assert_eq!(decoded.content, "服务端用户等级");
        assert_eq!(decoded.encoding, "gb18030");
    }

    /// Verifies malformed trailing legacy bytes fail instead of silently inserting replacement text.
    /// 验证传统编码末尾字节残缺时会失败，而不是静默插入替代文本。
    #[test]
    fn decode_complete_user_text_rejects_incomplete_legacy_sequence() {
        assert_eq!(decode_complete_user_text(&[0x81]), Err(UserTextDecodeError));
    }
}
