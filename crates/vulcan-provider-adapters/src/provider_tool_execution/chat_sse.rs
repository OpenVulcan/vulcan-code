//! Incremental bounded decoder for the existing provider-tool Chat SSE contract.
//! 既有供应商工具 Chat SSE 契约的增量有界解码器。

use super::{
    admit_provider_response_bytes, append_generic_openai_chat_delta, parse_bounded_provider_json,
};

/// Owns framing offsets separately from the accumulated visible text.
/// 将分帧偏移与累积可见文本分开持有。
#[derive(Default)]
pub(super) struct ChatSseDecoder {
    pending: Vec<u8>,
    scan_from: usize,
    output: String,
    terminal: bool,
    received_bytes: usize,
    #[cfg(test)]
    scanned_bytes: usize,
}

impl ChatSseDecoder {
    /// Consumes one transport chunk, scanning new bytes only and compacting at most once.
    /// 消费一个传输块，仅扫描新字节，且最多整理一次缓冲区。
    ///
    /// The whole stream shares one byte ceiling, which also bounds the accumulated text.
    /// 整条流共享一个字节上限，累积文本因此同样有界。
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<(), String> {
        self.received_bytes =
            admit_provider_response_bytes(self.received_bytes, chunk.len(), "Provider Chat SSE")?;
        self.pending.extend_from_slice(chunk);
        let mut consumed = 0usize;
        while self.scan_from < self.pending.len() {
            let next_newline = self.pending[self.scan_from..]
                .iter()
                .position(|byte| *byte == b'\n');
            #[cfg(test)]
            {
                self.scanned_bytes +=
                    next_newline.map_or(self.pending.len() - self.scan_from, |offset| offset + 1);
            }
            let Some(offset) = next_newline else {
                self.scan_from = self.pending.len();
                break;
            };
            let newline = self.scan_from + offset;
            let line = &self.pending[consumed..newline];
            consume_line(line, &mut self.output, &mut self.terminal)?;
            consumed = newline + 1;
            self.scan_from = consumed;
        }
        if consumed > 0 {
            self.pending.drain(..consumed);
            self.scan_from -= consumed;
        }
        Ok(())
    }

    /// Enforces the same EOF, terminal-record and visible-text rules as the original decoder.
    /// 强制执行与原解码器一致的 EOF、终止记录及可见文本规则。
    pub(super) fn finish(self) -> Result<String, String> {
        if !self.pending.iter().all(u8::is_ascii_whitespace) {
            return Err("Provider Chat SSE ended with an incomplete record.".to_string());
        }
        if !self.terminal {
            return Err("Provider Chat SSE ended before the [DONE] terminal record.".to_string());
        }
        if self.output.trim().is_empty() {
            return Err("Provider Chat SSE returned no visible text.".to_string());
        }
        Ok(self.output)
    }
}

/// Decodes one complete data line without copying its framing bytes.
/// 解码一条完整 data 行，不复制分帧字节。
fn consume_line(mut line: &[u8], output: &mut String, terminal: &mut bool) -> Result<(), String> {
    while line.last() == Some(&b'\r') {
        line = &line[..line.len() - 1];
    }
    if line.is_empty() || line.starts_with(b":") {
        return Ok(());
    }
    let Some(data) = line.strip_prefix(b"data:") else {
        return Ok(());
    };
    let data = std::str::from_utf8(data)
        .map_err(|_| "Provider Chat SSE data is not UTF-8.".to_string())?
        .trim();
    if data == "[DONE]" {
        if *terminal {
            return Err("Provider Chat SSE emitted more than one terminal record.".to_string());
        }
        *terminal = true;
        return Ok(());
    }
    if *terminal {
        return Err("Provider Chat SSE emitted data after its terminal record.".to_string());
    }
    let value = parse_bounded_provider_json(data, "Provider Chat SSE")?;
    append_generic_openai_chat_delta(output, &value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = ": heartbeat\r\nevent: message\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"中文\"}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"🙂\"}}]}\n\ndata: [DONE]\r\n\r\n";

    /// Chunk boundaries, including inside UTF-8 and CRLF, cannot change the decoded result.
    /// 分块边界即使位于 UTF-8 或 CRLF 内部，也不得改变解码结果。
    #[test]
    fn every_split_preserves_chat_sse_text() {
        for split in 0..=VALID.len() {
            let mut decoder = ChatSseDecoder::default();
            decoder
                .push(&VALID.as_bytes()[..split])
                .expect("first chunk");
            decoder
                .push(&VALID.as_bytes()[split..])
                .expect("second chunk");
            assert_eq!(decoder.scanned_bytes, VALID.len());
            assert_eq!(decoder.finish().expect("complete stream"), "中文🙂");
        }
        let mut decoder = ChatSseDecoder::default();
        for byte in VALID.as_bytes() {
            decoder.push(std::slice::from_ref(byte)).expect("one byte");
        }
        assert_eq!(decoder.scanned_bytes, VALID.len());
        assert_eq!(decoder.finish().expect("complete stream"), "中文🙂");
    }

    /// A long fragmented line is scanned once, not once per arriving chunk.
    /// 长行被切成小块时仅扫描一次，不随每次到块重复扫描前缀。
    #[test]
    fn fragmented_long_line_has_linear_scan_count() {
        let mut decoder = ChatSseDecoder::default();
        let comment = format!(":{}\n", "x".repeat(64 * 1024));
        for chunk in comment.as_bytes().chunks(17) {
            decoder.push(chunk).expect("bounded comment");
        }
        decoder.push(VALID.as_bytes()).expect("valid payload");
        assert_eq!(decoder.scanned_bytes, comment.len() + VALID.len());
        assert_eq!(decoder.finish().expect("complete payload"), "中文🙂");
    }

    /// Terminal and malformed-input rejection remains independent from transport chunking.
    /// 终止记录及畸形输入的拒绝规则不依赖传输分块。
    #[test]
    fn malformed_and_incomplete_streams_remain_errors() {
        for bytes in [
            b"data: [DONE]\ndata: [DONE]\n".as_slice(),
            b"data: [DONE]\ndata: {}\n".as_slice(),
            b"data: \xff\n".as_slice(),
            b"data: {invalid}\n".as_slice(),
        ] {
            let mut decoder = ChatSseDecoder::default();
            assert!(decoder.push(bytes).is_err(), "must reject {bytes:?}");
        }
        let mut decoder = ChatSseDecoder::default();
        decoder.push(b"data: [DONE]").expect("not yet complete");
        assert!(
            decoder
                .finish()
                .expect_err("incomplete")
                .contains("incomplete")
        );
        let mut decoder = ChatSseDecoder::default();
        decoder.push(b": heartbeat\n").expect("comment");
        assert!(
            decoder
                .finish()
                .expect_err("missing terminal")
                .contains("[DONE]")
        );
        let mut decoder = ChatSseDecoder::default();
        decoder.push(b"data: [DONE]\n").expect("terminal");
        assert!(
            decoder
                .finish()
                .expect_err("empty text")
                .contains("no visible text")
        );
    }

    /// The stream ceiling is checked before a chunk enters the pending buffer.
    /// 数据块进入待处理缓冲区之前先检查整条流的上限。
    #[test]
    fn stream_bytes_are_bounded_before_buffering() {
        let mut decoder = ChatSseDecoder {
            received_bytes: usize::try_from(super::super::PROVIDER_TOOL_RESPONSE_BYTE_LIMIT)
                .expect("64-bit host"),
            ..ChatSseDecoder::default()
        };
        assert!(decoder.push(b"x").is_err());
        assert!(decoder.pending.is_empty());
    }
}
