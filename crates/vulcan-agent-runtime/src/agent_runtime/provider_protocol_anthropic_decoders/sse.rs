//! Incremental Server-Sent Event decoding utilities for Anthropic protocol streams.
//! 面向 Anthropic 协议流的增量 Server-Sent Event 解码工具。

use super::line::{self, LineDecoder};
use futures_util::StreamExt;
use futures_util::stream::Stream;
use std::fmt::{Display, Formatter};

/// One decoded SSE event with its event name, combined data payload, and raw source lines.
/// 一条已解码的 SSE 事件，包含事件名、合并后的数据载荷与原始源行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSentEvent {
    /// Optional SSE event name.
    /// 可选的 SSE 事件名称。
    pub event: Option<String>,
    /// Combined SSE data payload with `data:` lines joined by newlines.
    /// 由 `data:` 行按换行拼接后的 SSE 数据载荷。
    pub data: String,
    /// Raw SSE lines that formed the event.
    /// 组成该事件的原始 SSE 行。
    pub raw: Vec<String>,
}

/// Error type produced by the Anthropic SSE decoder.
/// Anthropic SSE 解码器产生的错误类型。
#[derive(Debug)]
pub enum SseDecoderError {
    /// Wrapper for IO errors from the underlying byte stream.
    /// 底层字节流产生的 IO 错误包装。
    Io(std::io::Error),
}

impl Display for SseDecoderError {
    /// Formats one decoder error as a human-readable message.
    /// 将一个解码器错误格式化为可读消息。
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "IO error: {error}"),
        }
    }
}

impl std::error::Error for SseDecoderError {}

/// Incrementally decodes one byte stream into completed SSE events.
/// 将一个字节流增量解码为完整的 SSE 事件。
pub fn iter_sse_messages<S>(
    mut stream: S,
) -> impl Stream<Item = Result<ServerSentEvent, SseDecoderError>>
where
    S: Stream<Item = Result<Vec<u8>, std::io::Error>> + Unpin,
{
    let mut sse_decoder = SseEventDecoder::new();
    let mut line_decoder = LineDecoder::new();
    let mut buffer = Vec::new();

    async_stream::stream! {
        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(error) => {
                    yield Err(SseDecoderError::Io(error));
                    continue;
                }
            };

            buffer.extend_from_slice(&chunk);

            while let Some((chunk_data, remaining)) = extract_sse_chunk(&buffer) {
                buffer = remaining;

                for line in line_decoder.decode(&chunk_data) {
                    if let Some(event) = sse_decoder.decode(&line) {
                        yield Ok(event);
                    }
                }
            }
        }

        // rawBuffer may contain the final event when a compatible endpoint closes without a trailing blank separator.
        // 当兼容端点未发送尾随空行便关闭连接时，原始缓冲区仍可能包含最后一条事件。
        if !buffer.is_empty() {
            for line in line_decoder.decode(&buffer) {
                if let Some(event) = sse_decoder.decode(&line) {
                    yield Ok(event);
                }
            }
        }

        for line in line_decoder.flush() {
            if let Some(event) = sse_decoder.decode(&line) {
                yield Ok(event);
            }
        }

        if (!sse_decoder.data.is_empty() || sse_decoder.event.is_some())
            && let Some(event) = sse_decoder.decode("")
        {
            yield Ok(event);
        }
    }
}

/// Internal stateful SSE line decoder used by the Anthropic event parser.
/// Anthropic 事件解析器使用的内部有状态 SSE 行解码器。
#[derive(Debug, Default)]
struct SseEventDecoder {
    /// Accumulated `data:` lines for the current SSE event.
    /// 当前 SSE 事件已累积的 `data:` 行。
    data: Vec<String>,
    /// Current `event:` name for the pending SSE event.
    /// 当前待处理 SSE 事件的 `event:` 名称。
    event: Option<String>,
    /// Raw source lines that belong to the pending SSE event.
    /// 属于待处理 SSE 事件的原始源行。
    chunks: Vec<String>,
}

impl SseEventDecoder {
    /// Creates one fresh line-oriented SSE event decoder.
    /// 创建一个新的按行处理的 SSE 事件解码器。
    fn new() -> Self {
        Self::default()
    }

    /// Consumes one text line and returns a completed SSE event when the event boundary is reached.
    /// 消耗一条文本行，并在事件边界到达时返回一条完整 SSE 事件。
    fn decode(&mut self, line: &str) -> Option<ServerSentEvent> {
        let mut line = line.to_string();

        if line.ends_with('\r') {
            line = line[..line.len() - 1].to_string();
        }

        if line.is_empty() {
            if self.event.is_none() && self.data.is_empty() {
                return None;
            }

            let sse = ServerSentEvent {
                event: self.event.clone(),
                data: self.data.join("\n"),
                raw: self.chunks.clone(),
            };

            self.event = None;
            self.data.clear();
            self.chunks.clear();

            return Some(sse);
        }

        self.chunks.push(line.clone());

        if line.starts_with(':') {
            return None;
        }

        let (field_name, value) = line
            .split_once(':')
            .map_or((line.as_str(), ""), |(field, value)| (field, value));
        let value = value.strip_prefix(' ').unwrap_or(value);

        match field_name {
            "event" => self.event = Some(value.to_string()),
            "data" => self.data.push(value.to_string()),
            _ => {}
        }

        None
    }
}

/// Extracts one SSE chunk up to the next double-newline boundary.
/// 提取直到下一个双换行边界之前的一段 SSE 块。
fn extract_sse_chunk(buffer: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let pattern_index = line::find_double_newline_index(buffer);

    if pattern_index <= 0 {
        return None;
    }

    let pattern_index = pattern_index as usize;
    let chunk = buffer.get(..pattern_index)?.to_vec();
    let remaining = buffer.get(pattern_index..)?.to_vec();

    Some((chunk, remaining))
}

#[cfg(test)]
mod tests {
    use super::iter_sse_messages;
    use futures_util::StreamExt;

    /// Verifies that one single SSE event is decoded from a basic `data:` payload.
    /// 验证基础 `data:` 载荷能够解码出一条 SSE 事件。
    #[tokio::test]
    async fn decodes_basic_event() {
        let stream =
            futures_util::stream::iter(vec![Ok::<_, std::io::Error>(b"data: hello\n\n".to_vec())]);
        let decoded = iter_sse_messages(stream);
        futures_util::pin_mut!(decoded);

        let event = decoded
            .next()
            .await
            .expect("event should exist")
            .expect("event should decode");

        assert_eq!(event.event, None);
        assert_eq!(event.data, "hello");
        assert_eq!(event.raw, vec![String::from("data: hello")]);
    }

    /// Verifies that a final SSE event is retained when the transport closes without a blank separator.
    /// 验证传输未发送空行分隔符便关闭时，最后一条 SSE 事件仍会被保留。
    #[tokio::test]
    async fn decodes_final_event_without_blank_separator() {
        // stream closes immediately after the final data line without a separator.
        // stream 在最后一条数据行后立即关闭，不携带分隔符。
        let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
            b"event: message_stop\ndata: {\"type\":\"message_stop\"}".to_vec(),
        )]);
        // decoded applies the production incremental SSE path.
        // decoded 使用生产环境的增量 SSE 路径。
        let decoded = iter_sse_messages(stream);
        futures_util::pin_mut!(decoded);

        let event = decoded
            .next()
            .await
            .expect("final event should exist")
            .expect("final event should decode");

        assert_eq!(event.event.as_deref(), Some("message_stop"));
        assert_eq!(event.data, r#"{"type":"message_stop"}"#);
    }

    /// Verifies that long UTF-8 tool-input events survive arbitrary one-byte transport chunks.
    /// 验证长 UTF-8 工具输入事件经过任意单字节传输分片后仍保持完整。
    #[tokio::test]
    async fn decodes_long_tool_input_from_single_byte_chunks() {
        // argumentText exceeds the reported failure position and includes multi-byte UTF-8 text.
        // argumentText 超过已报告的失败位置，并包含多字节 UTF-8 文本。
        let argument_text = "火山".repeat(2_500);
        // payload mirrors one real Anthropic input-json delta event.
        // payload 模拟一条真实的 Anthropic 输入 JSON 增量事件。
        let payload = serde_json::json!({
            "type": "content_block_delta",
            "index": 1,
            "delta": {
                "type": "input_json_delta",
                "partial_json": serde_json::json!({ "content": argument_text }).to_string(),
            },
        })
        .to_string();
        // wire contains the complete SSE event framing.
        // wire 包含完整的 SSE 事件帧。
        let wire = format!("event: content_block_delta\ndata: {payload}\n\n");
        // chunks force every UTF-8 byte and event delimiter across transport boundaries.
        // chunks 强制每个 UTF-8 字节及事件分隔符跨越传输边界。
        let chunks = wire
            .into_bytes()
            .into_iter()
            .map(|byte| Ok::<_, std::io::Error>(vec![byte]))
            .collect::<Vec<_>>();
        // decoded applies the production incremental SSE path to all byte fragments.
        // decoded 对全部字节碎片使用生产环境的增量 SSE 路径。
        let decoded = iter_sse_messages(futures_util::stream::iter(chunks));
        futures_util::pin_mut!(decoded);

        let event = decoded
            .next()
            .await
            .expect("tool-input event should exist")
            .expect("tool-input event should decode");

        assert_eq!(event.event.as_deref(), Some("content_block_delta"));
        assert_eq!(event.data, payload);
    }
}
