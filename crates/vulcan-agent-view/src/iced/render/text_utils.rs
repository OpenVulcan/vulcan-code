//! Text-offset normalization helpers shared by transcript hit-testing and selection rendering.
//! 转录命中测试与选区渲染共享的文本偏移归一化辅助逻辑。

/// Converts one byte offset reported by the text backend into a safe character offset.
/// 将文本后端返回的字节偏移转换为安全的字符偏移。
pub(super) fn byte_offset_to_char_offset(text: &str, byte_offset: usize) -> usize {
    let clamped_byte_offset = byte_offset.min(text.len());
    let safe_byte_offset = if text.is_char_boundary(clamped_byte_offset) {
        clamped_byte_offset
    } else {
        let mut safe_byte_offset = clamped_byte_offset;
        while safe_byte_offset > 0 && !text.is_char_boundary(safe_byte_offset) {
            safe_byte_offset -= 1;
        }
        safe_byte_offset
    };

    text[..safe_byte_offset].chars().count()
}
