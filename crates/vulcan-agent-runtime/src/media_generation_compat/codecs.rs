//! Encoding, MIME type, and compact error-text helpers for media generation.
//! 媒体生成使用的编码、MIME 类型与紧凑错误文本辅助能力。

/// Decodes one hex string into raw bytes with a provider-specific error label.
/// 使用带有提供方标签的错误消息把十六进制字符串解码为原始字节。
pub(super) fn decode_hex_bytes(hex: &str, provider_label: &str) -> Result<Vec<u8>, String> {
    let trimmed = hex.trim();
    if trimmed.is_empty() {
        return Err(format!("{provider_label} returned empty audio data."));
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!(
            "{provider_label} returned audio data that is not valid hexadecimal."
        ));
    }
    if !trimmed.len().is_multiple_of(2) {
        return Err(format!(
            "{provider_label} returned hexadecimal audio data with an odd length."
        ));
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let raw_bytes = trimmed.as_bytes();
    let mut index = 0usize;
    while index < raw_bytes.len() {
        let chunk = std::str::from_utf8(&raw_bytes[index..index + 2]).map_err(|error| {
            format!("Failed to decode hexadecimal data from {provider_label}: {error}")
        })?;
        let byte = u8::from_str_radix(chunk, 16).map_err(|error| {
            format!("Failed to decode hexadecimal data from {provider_label}: {error}")
        })?;
        bytes.push(byte);
        index += 2;
    }
    Ok(bytes)
}

/// Maps one requested audio format to the MIME type expected by the managed attachment store.
/// 将单个请求的音频格式映射到托管附件存储期望的 MIME 类型。
pub(super) fn audio_format_to_mime_type(format: &str) -> Result<&'static str, String> {
    match format.trim().to_ascii_lowercase().as_str() {
        "mp3" => Ok("audio/mpeg"),
        "wav" => Ok("audio/wav"),
        "m4a" => Ok("audio/mp4"),
        "aac" => Ok("audio/aac"),
        "flac" => Ok("audio/flac"),
        other => Err(format!(
            "Audio format `{other}` is not currently supported."
        )),
    }
}

/// Truncates one error body so host-visible messages stay compact and readable.
/// 截断单条错误正文，使宿主可见消息保持紧凑且可读。
pub(super) fn truncate_error_text(value: &str, limit: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let shortened = trimmed.chars().take(limit).collect::<String>();
    format!("{shortened}…")
}
