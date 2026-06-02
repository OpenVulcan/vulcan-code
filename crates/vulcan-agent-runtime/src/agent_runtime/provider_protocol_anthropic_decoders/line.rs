//! Incremental line decoding utilities for Anthropic protocol streams.
//! 面向 Anthropic 协议流的增量行解码工具。

use std::str;

/// Incrementally decodes byte chunks into text lines while preserving incomplete boundaries.
/// 以增量方式把字节块解码为文本行，并保留未闭合边界。
#[derive(Debug, Default, Clone)]
pub struct LineDecoder {
    /// Buffered bytes that have not been emitted as complete lines yet.
    /// 尚未作为完整行输出的缓冲字节。
    buffer: Vec<u8>,
    /// Index of a buffered carriage-return boundary when the next newline is still pending.
    /// 当后续换行尚未到来时，缓冲的回车边界索引。
    carriage_return_index: Option<usize>,
}

impl LineDecoder {
    /// Creates one fresh incremental line decoder.
    /// 创建一个新的增量行解码器。
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes one byte chunk into zero or more completed text lines.
    /// 将一个字节块解码为零条或多条完整文本行。
    pub fn decode(&mut self, chunk: &[u8]) -> Vec<String> {
        if chunk.is_empty() {
            return Vec::new();
        }

        self.buffer.extend_from_slice(chunk);
        let mut lines = Vec::new();

        while let Some(pattern_index) = find_newline_index(&self.buffer, self.carriage_return_index)
        {
            if pattern_index.carriage && self.carriage_return_index.is_none() {
                self.carriage_return_index = Some(pattern_index.index);
                continue;
            }

            if let Some(cr_index) = self.carriage_return_index
                && (pattern_index.index != cr_index + 1 || pattern_index.carriage)
            {
                if cr_index > 0 {
                    let line =
                        decode_text(self.buffer.get(..cr_index.saturating_sub(1)).unwrap_or(&[]));
                    lines.push(line);
                } else {
                    lines.push(String::new());
                }

                if cr_index < self.buffer.len() {
                    self.buffer = self.buffer.get(cr_index..).unwrap_or(&[]).to_vec();
                } else {
                    self.buffer.clear();
                }
                self.carriage_return_index = None;
                continue;
            }

            let end_index = if self.carriage_return_index.is_some() {
                pattern_index.preceding - 1
            } else {
                pattern_index.preceding
            };

            if end_index > 0 {
                let line = decode_text(self.buffer.get(..end_index).unwrap_or(&[]));
                lines.push(line);
            } else {
                lines.push(String::new());
            }

            if pattern_index.index < self.buffer.len() {
                self.buffer = self
                    .buffer
                    .get(pattern_index.index..)
                    .unwrap_or(&[])
                    .to_vec();
            } else {
                self.buffer.clear();
            }
            self.carriage_return_index = None;
        }

        lines
    }

    /// Flushes any remaining buffered bytes as the final lines for the current stream.
    /// 将当前流中剩余的缓冲字节刷新为最终行。
    pub fn flush(&mut self) -> Vec<String> {
        if self.buffer.is_empty() {
            return Vec::new();
        }

        self.decode("\n".as_bytes())
    }
}

/// One newline discovery result used by the incremental decoder.
/// 增量解码器使用的一次换行发现结果。
#[derive(Debug, Clone, Copy)]
struct NewlineIndex {
    /// Index of the byte immediately before the line ending.
    /// 行结束符之前那个字节的索引。
    preceding: usize,
    /// Index of the first byte after the line ending.
    /// 行结束符之后首字节的索引。
    index: usize,
    /// Whether the match was a carriage return instead of a line feed.
    /// 此次匹配是否为回车而非换行。
    carriage: bool,
}

/// Finds the next newline-like boundary in one buffered byte slice.
/// 在单个缓冲字节切片中查找下一个换行类边界。
fn find_newline_index(buffer: &[u8], start_index: Option<usize>) -> Option<NewlineIndex> {
    const NEWLINE: u8 = 0x0a;
    const CARRIAGE: u8 = 0x0d;

    let start = start_index.unwrap_or(0);

    for (i, &byte) in buffer.iter().enumerate().skip(start) {
        if byte == NEWLINE {
            return Some(NewlineIndex {
                preceding: i,
                index: i + 1,
                carriage: false,
            });
        }

        if byte == CARRIAGE {
            return Some(NewlineIndex {
                preceding: i,
                index: i + 1,
                carriage: true,
            });
        }
    }

    None
}

/// Finds the first double-newline boundary in a buffered byte slice.
/// 在缓冲字节切片中查找首个双换行边界。
pub fn find_double_newline_index(buffer: &[u8]) -> isize {
    const NEWLINE: u8 = 0x0a;
    const CARRIAGE: u8 = 0x0d;

    for (i, window) in buffer.windows(2).enumerate() {
        if window == [NEWLINE, NEWLINE] || window == [CARRIAGE, CARRIAGE] {
            return (i + 2) as isize;
        }
    }

    for (i, window) in buffer.windows(4).enumerate() {
        if window == [CARRIAGE, NEWLINE, CARRIAGE, NEWLINE] {
            return (i + 4) as isize;
        }
    }

    -1
}

/// Decodes one byte slice into a UTF-8 string while preserving invalid sequences with replacement.
/// 将一个字节切片解码为 UTF-8 字符串，并以替换形式保留无效序列。
fn decode_text(bytes: &[u8]) -> String {
    match str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).to_string(),
    }
}

/// Decodes multiple chunks with one optional final flush.
/// 解码多段输入块，并可选择执行一次最终刷新。
#[cfg(test)]
pub fn decode_chunks(chunks: &[&[u8]], flush: bool) -> Vec<String> {
    let mut decoder = LineDecoder::new();
    let mut lines = Vec::new();

    for chunk in chunks {
        lines.extend(decoder.decode(chunk));
    }

    if flush {
        lines.extend(decoder.flush());
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::decode_chunks;

    /// Verifies that one partial line remains buffered until the newline arrives.
    /// 验证一条未完成的行会被缓存在内部，直到换行到来。
    #[test]
    fn basic_line_decoding() {
        assert_eq!(
            decode_chunks(&[&b"foo"[..], &b" bar\nbaz"[..]], false),
            vec![String::from("foo bar")]
        );
    }

    /// Verifies that CRLF input is normalized into one completed line and the tail can still be flushed.
    /// 验证 CRLF 输入会被规范化为一条完整行，并且尾部仍可继续刷新。
    #[test]
    fn basic_line_decoding_with_crlf() {
        assert_eq!(
            decode_chunks(&[&b"foo"[..], &b" bar\r\nbaz"[..]], false),
            vec![String::from("foo bar")]
        );
        assert_eq!(
            decode_chunks(&[&b"foo"[..], &b" bar\r\nbaz"[..]], true),
            vec![String::from("foo bar"), String::from("baz")]
        );
    }
}
