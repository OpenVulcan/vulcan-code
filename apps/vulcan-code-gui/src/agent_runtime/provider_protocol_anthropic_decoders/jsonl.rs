#![allow(dead_code)]

//! Incremental JSON Lines decoding utilities for Anthropic protocol streams.
//! 面向 Anthropic 协议流的增量 JSON Lines 解码工具。

use super::line::LineDecoder;
use futures_util::Stream;
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Error type produced by the Anthropic JSONL decoder.
/// Anthropic JSONL 解码器产生的错误类型。
#[derive(Debug)]
pub enum JsonlDecoderError {
    /// Wrapper for JSON parsing errors.
    /// JSON 解析错误包装。
    Parse(serde_json::Error),
    /// Wrapper for stream errors.
    /// 流错误包装。
    Stream(std::io::Error),
}

impl Display for JsonlDecoderError {
    /// Formats one decoder error as a human-readable message.
    /// 将一个解码器错误格式化为可读消息。
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "Failed to parse JSON: {error}"),
            Self::Stream(error) => write!(f, "Stream error: {error}"),
        }
    }
}

impl std::error::Error for JsonlDecoderError {}

/// Incrementally decodes a JSONL byte stream into typed JSON items.
/// 将 JSONL 字节流增量解码为强类型 JSON 条目。
pub struct JsonlDecoder<T, S>
where
    T: DeserializeOwned + Unpin,
    S: Stream<Item = Result<Vec<u8>, std::io::Error>> + Unpin,
{
    /// Underlying byte stream that still needs decoding.
    /// 仍需解码的底层字节流。
    stream: S,
    /// Incremental line decoder reused across chunks.
    /// 跨 chunk 复用的增量行解码器。
    line_decoder: LineDecoder,
    /// Buffered parsed JSON items ready to be yielded.
    /// 已解析完成、待输出的 JSON 条目缓冲区。
    buffer: Vec<T>,
    /// Marker that keeps the generic item type visible to the compiler.
    /// 让编译器可见泛型条目类型的标记。
    _phantom: PhantomData<T>,
}

impl<T, S> JsonlDecoder<T, S>
where
    T: DeserializeOwned + Unpin,
    S: Stream<Item = Result<Vec<u8>, std::io::Error>> + Unpin,
{
    /// Creates one JSONL decoder from a byte stream.
    /// 从一个字节流创建 JSONL 解码器。
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            line_decoder: LineDecoder::new(),
            buffer: Vec::new(),
            _phantom: PhantomData,
        }
    }

    /// Processes one chunk and returns all parsed JSON items that became available from it.
    /// 处理一个 chunk，并返回由其产生的所有可用 JSON 条目。
    fn process_chunk(&mut self, chunk: &[u8]) -> Result<Vec<T>, JsonlDecoderError> {
        let lines = self.line_decoder.decode(chunk);
        let mut results = Vec::with_capacity(lines.len());

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }

            let value: T = serde_json::from_str(&line).map_err(JsonlDecoderError::Parse)?;
            results.push(value);
        }

        Ok(results)
    }

    /// Flushes the pending line decoder state into parsed JSON items.
    /// 将待处理的行解码器状态刷新为已解析 JSON 条目。
    fn flush(&mut self) -> Result<Vec<T>, JsonlDecoderError> {
        let lines = self.line_decoder.flush();
        let mut results = Vec::with_capacity(lines.len());

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }

            let value: T = serde_json::from_str(&line).map_err(JsonlDecoderError::Parse)?;
            results.push(value);
        }

        Ok(results)
    }
}

impl<T, S> Stream for JsonlDecoder<T, S>
where
    T: DeserializeOwned + Unpin,
    S: Stream<Item = Result<Vec<u8>, std::io::Error>> + Unpin,
{
    type Item = Result<T, JsonlDecoderError>;

    /// Polls the next decoded JSON item from the underlying byte stream.
    /// 从底层字节流中轮询下一个已解码 JSON 条目。
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if !this.buffer.is_empty() {
            return Poll::Ready(Some(Ok(this.buffer.remove(0))));
        }

        match this.stream.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok(chunk))) => match this.process_chunk(&chunk) {
                Ok(mut parsed) => {
                    if !parsed.is_empty() {
                        let item = parsed.remove(0);
                        this.buffer.append(&mut parsed);
                        Poll::Ready(Some(Ok(item)))
                    } else {
                        Pin::new(this).poll_next(cx)
                    }
                }
                Err(error) => Poll::Ready(Some(Err(error))),
            },
            Poll::Ready(Some(Err(error))) => {
                Poll::Ready(Some(Err(JsonlDecoderError::Stream(error))))
            }
            Poll::Ready(None) => match this.flush() {
                Ok(mut parsed) => {
                    if !parsed.is_empty() {
                        let item = parsed.remove(0);
                        this.buffer.append(&mut parsed);
                        Poll::Ready(Some(Ok(item)))
                    } else {
                        Poll::Ready(None)
                    }
                }
                Err(error) => Poll::Ready(Some(Err(error))),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::JsonlDecoder;
    use futures_util::StreamExt;

    /// Verifies that multiple JSONL lines are decoded into multiple typed values.
    /// 验证多条 JSONL 行可被解码为多个强类型值。
    #[tokio::test]
    async fn decodes_multiple_jsonl_values() {
        let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
            b"{\"name\":\"alice\"}\n{\"name\":\"bob\"}\n".to_vec(),
        )]);
        let mut decoder = JsonlDecoder::<serde_json::Value, _>::new(stream);

        let first = decoder
            .next()
            .await
            .expect("first item should exist")
            .expect("first item should parse");
        let second = decoder
            .next()
            .await
            .expect("second item should exist")
            .expect("second item should parse");

        assert_eq!(first["name"], "alice");
        assert_eq!(second["name"], "bob");
    }
}
