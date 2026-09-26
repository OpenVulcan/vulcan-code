//! Bounded incremental framing for registered provider transport dialects.
//! 已注册供应商传输方言的有界增量分帧。

use serde_json::Value;
use vulcan_llm_core::ProtocolTransportProfile;
use vulcan_llm_core::{ProtocolConversionError, ProtocolStreamFraming};

/// One parsed transport frame and its optional SSE event name.
/// 一条解析后的传输帧及其可选 SSE 事件名称。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StreamFrame {
    /// Optional event name.
    /// 可选事件名称。
    pub(super) event_name: Option<String>,
    /// Complete frame data.
    /// 完整帧数据。
    pub(super) data: String,
}

/// Incrementally frames arbitrary chunks with bounded retained memory.
/// 使用有界保留内存对任意分片进行增量分帧。
#[derive(Debug)]
pub(super) struct IncrementalFrameParser {
    /// Registered framing dialect.
    /// 已注册的分帧方言。
    framing: ProtocolStreamFraming,
    /// Maximum complete frame size.
    /// 完整帧最大尺寸。
    max_frame_bytes: usize,
    /// Maximum unconsumed buffer size.
    /// 未消费缓冲区最大尺寸。
    max_buffer_bytes: usize,
    /// Unprocessed bytes retained across transport chunks.
    /// 在传输分片间保留的未处理字节。
    buffer: Vec<u8>,
    /// First unconsumed byte.
    /// 首个未消费字节位置。
    consumed: usize,
}

impl IncrementalFrameParser {
    /// Creates a bounded parser from one immutable transport profile.
    /// 根据一个不可变传输 Profile 创建有界解析器。
    ///
    /// # Parameters
    /// 参数
    /// `profile` provides the exact framing and byte limits.
    /// `profile` 提供精确分帧方式与字节上限。
    ///
    /// # Returns
    /// 返回值
    /// A new empty parser.
    /// 返回一个新的空解析器。
    pub(super) fn new(profile: ProtocolTransportProfile) -> Self {
        Self {
            framing: profile.framing,
            max_frame_bytes: profile.max_frame_bytes,
            max_buffer_bytes: profile.max_buffer_bytes,
            buffer: Vec::new(),
            consumed: 0,
        }
    }

    /// Pushes arbitrary transport bytes.
    /// 推入任意传输字节。
    ///
    /// # Parameters
    /// 参数
    /// `chunk` is one arbitrary network byte chunk.
    /// `chunk` 是一个任意网络字节分片。
    ///
    /// # Returns
    /// 返回值
    /// Complete frames, or a deterministic framing limit/encoding error.
    /// 返回完整帧，或确定性的分帧上限/编码错误。
    pub(super) fn push(
        &mut self,
        mut chunk: &[u8],
    ) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        let mut frames = Vec::new();
        while !chunk.is_empty() {
            self.compact_if_worthwhile();
            let retained = self.retained_len();
            if retained >= self.max_buffer_bytes {
                return Err(self.buffer_limit_error());
            }
            let capacity = self.max_buffer_bytes - retained;
            let take = capacity.min(chunk.len());
            self.buffer.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
            frames.extend(self.drain_frames(false)?);
            if self.retained_len() >= self.max_buffer_bytes && !chunk.is_empty() {
                return Err(self.buffer_limit_error());
            }
        }
        Ok(frames)
    }

    /// Finishes framing and includes a final non-empty record.
    /// 完成分帧并包含最后一条非空记录。
    ///
    /// # Returns
    /// 返回值
    /// Final complete frames, or a deterministic truncation/encoding error.
    /// 返回最终完整帧，或确定性的截断/编码错误。
    pub(super) fn finish(&mut self) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        self.drain_frames(true)
    }

    /// Drains complete records from the internal byte buffer.
    /// 从内部字节缓冲区提取完整记录。
    ///
    /// # Parameters
    /// 参数
    /// `include_tail` controls whether the final unterminated record is accepted.
    /// `include_tail` 控制是否接受最后一条未带分隔符的记录。
    ///
    /// # Returns
    /// 返回值
    /// Complete frames in transport order.
    /// 返回按传输顺序排列的完整帧。
    fn drain_frames(
        &mut self,
        include_tail: bool,
    ) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        let mut frames = Vec::new();
        while let Some((record_end, delimiter_len)) = find_record_end(self.framing, self.retained())
        {
            self.validate_frame_size(record_end)?;
            let start = self.consumed;
            let end = start + record_end;
            let record = &self.buffer[start..end];
            if let Some(frame) = parse_stream_record(self.framing, record)? {
                frames.push(frame);
            }
            self.consumed = end + delimiter_len;
        }
        if include_tail && self.retained_len() > 0 {
            let record_len = self.retained_len();
            self.validate_frame_size(record_len)?;
            let record = self.retained();
            if let Some(frame) = parse_stream_record(self.framing, record)? {
                frames.push(frame);
            }
            self.consumed = self.buffer.len();
        }
        self.compact_if_worthwhile();
        Ok(frames)
    }

    /// Returns the unconsumed suffix.
    /// 返回未消费的后缀。
    ///
    /// # Returns
    /// 返回值
    /// Borrowed pending bytes.
    /// 返回借用的待处理字节。
    fn retained(&self) -> &[u8] {
        &self.buffer[self.consumed..]
    }

    /// Returns the number of unconsumed bytes.
    /// 返回未消费字节数。
    ///
    /// # Returns
    /// 返回值
    /// Pending byte count.
    /// 返回待处理字节数。
    fn retained_len(&self) -> usize {
        self.buffer.len().saturating_sub(self.consumed)
    }

    /// Compacts only after consumed storage is materially large.
    /// 仅在已消费存储达到显著规模后执行压缩。
    fn compact_if_worthwhile(&mut self) {
        if self.consumed == self.buffer.len() {
            self.buffer.clear();
            self.consumed = 0;
        } else if self.consumed >= 64 * 1024 && self.consumed >= self.buffer.len() / 2 {
            self.buffer.copy_within(self.consumed.., 0);
            self.buffer.truncate(self.buffer.len() - self.consumed);
            self.consumed = 0;
        }
    }

    /// Rejects one oversized complete or pending frame.
    /// 拒绝一个过大的完整帧或待处理帧。
    ///
    /// # Parameters
    /// 参数
    /// `frame_bytes` is the exact candidate frame size.
    /// `frame_bytes` 是候选帧的精确尺寸。
    ///
    /// # Returns
    /// 返回值
    /// Success within the profile limit, otherwise a controlled error.
    /// 在 Profile 上限内返回成功，否则返回受控错误。
    fn validate_frame_size(&self, frame_bytes: usize) -> Result<(), ProtocolConversionError> {
        if frame_bytes <= self.max_frame_bytes {
            return Ok(());
        }
        Err(ProtocolConversionError::new(format!(
            "Provider stream frame exceeded the registered {} byte limit.",
            self.max_frame_bytes
        )))
    }

    /// Builds one controlled pending-buffer limit error.
    /// 构建一个受控的待处理缓冲区上限错误。
    ///
    /// # Returns
    /// 返回值
    /// A deterministic protocol conversion failure.
    /// 返回确定性的协议转换失败。
    fn buffer_limit_error(&self) -> ProtocolConversionError {
        ProtocolConversionError::new(format!(
            "Provider stream buffer exceeded the registered {} byte limit.",
            self.max_buffer_bytes
        ))
    }
}

/// Finds the first complete delimiter for one registered framing dialect.
/// 查找一种已注册分帧方言的首个完整分隔符。
///
/// # Parameters
/// 参数
/// `framing` is the exact dialect and `buffer` is the unconsumed suffix.
/// `framing` 是精确方言，`buffer` 是未消费后缀。
///
/// # Returns
/// 返回值
/// Record-end offset and delimiter length, or `None`.
/// 返回记录结束偏移与分隔符长度，否则返回 `None`。
fn find_record_end(framing: ProtocolStreamFraming, buffer: &[u8]) -> Option<(usize, usize)> {
    match framing {
        ProtocolStreamFraming::Sse => {
            for index in 0..buffer.len() {
                let Some(first_len) = sse_line_ending_len(buffer, index) else {
                    continue;
                };
                let second_start = index + first_len;
                if let Some(second_len) = sse_line_ending_len(buffer, second_start) {
                    return Some((index, first_len + second_len));
                }
            }
            None
        }
        ProtocolStreamFraming::JsonLines | ProtocolStreamFraming::Ndjson => buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| (index, 1)),
        ProtocolStreamFraming::SingleJson => None,
    }
}

/// Returns the byte length of one SSE line ending at an exact offset.
/// 返回精确偏移处一项 SSE 行结束符的字节长度。
///
/// # Parameters
/// 参数
/// `buffer` is the pending transport data and `index` is the candidate line-ending offset.
/// `buffer` 是待处理传输数据，`index` 是候选行结束符偏移。
///
/// # Returns
/// 返回值
/// Two bytes for CRLF, one byte for CR or LF, otherwise `None`.
/// CRLF 返回两个字节，CR 或 LF 返回一个字节，否则返回 `None`。
fn sse_line_ending_len(buffer: &[u8], index: usize) -> Option<usize> {
    match buffer.get(index) {
        Some(b'\r') if buffer.get(index + 1) == Some(&b'\n') => Some(2),
        Some(b'\r' | b'\n') => Some(1),
        _ => None,
    }
}

/// Parses one complete record according to its registered framing.
/// 根据已注册分帧方式解析一条完整记录。
///
/// # Parameters
/// 参数
/// `framing` is the exact dialect and `record` contains one complete frame.
/// `framing` 是精确方言，`record` 包含一个完整帧。
///
/// # Returns
/// 返回值
/// A decoded frame, no frame for heartbeat/blank records, or a controlled error.
/// 返回解码帧；心跳/空记录返回空；编码错误返回受控错误。
fn parse_stream_record(
    framing: ProtocolStreamFraming,
    record: &[u8],
) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    match framing {
        ProtocolStreamFraming::Sse => parse_sse_record(record),
        ProtocolStreamFraming::JsonLines | ProtocolStreamFraming::Ndjson => {
            parse_json_line_record(record)
        }
        ProtocolStreamFraming::SingleJson => parse_single_json_record(record),
    }
}

/// Parses one complete SSE record.
/// 解析一条完整 SSE 记录。
///
/// # Parameters
/// 参数
/// `record` contains one complete SSE record without its blank-line delimiter.
/// `record` 包含一条不带空行分隔符的完整 SSE 记录。
///
/// # Returns
/// 返回值
/// A semantic data frame, no frame for comments, or a controlled UTF-8 error.
/// 返回语义数据帧；注释返回空；UTF-8 错误返回受控错误。
fn parse_sse_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!("Provider SSE record is not valid UTF-8: {error}"))
    })?;
    let mut event_name = None;
    let mut data_lines = Vec::new();
    // SSE permits CR, LF, and CRLF independently, so normalize all three before field parsing.
    // SSE 独立允许 CR、LF 与 CRLF，因此在字段解析前统一三种换行。
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for line in normalized.lines() {
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(StreamFrame {
        event_name,
        data: data_lines.join("\n"),
    }))
}

/// Parses one JSONL or NDJSON record.
/// 解析一条 JSONL 或 NDJSON 记录。
///
/// # Parameters
/// 参数
/// `record` is one line without its LF delimiter.
/// `record` 是不带 LF 分隔符的一行。
///
/// # Returns
/// 返回值
/// A trimmed JSON frame, no frame for a blank line, or a controlled UTF-8 error.
/// 返回去除首尾空白的 JSON 帧；空行返回空；UTF-8 错误返回受控错误。
fn parse_json_line_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON line is not valid UTF-8: {error}"))
    })?;
    let data = text.trim_end_matches('\r').trim();
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(data).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON line is invalid: {error}"))
    })?;
    Ok(Some(StreamFrame {
        event_name: None,
        data: data.to_string(),
    }))
}

/// Parses one complete single-JSON response.
/// 解析一份完整的单 JSON 响应。
///
/// # Parameters
/// 参数
/// `record` is the complete response body.
/// `record` 是完整响应体。
///
/// # Returns
/// 返回值
/// A validated JSON frame, no frame for an empty body, or a controlled error.
/// 返回已校验 JSON 帧；空响应体返回空；错误返回受控结果。
fn parse_single_json_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Provider JSON response is not valid UTF-8: {error}"
        ))
    })?;
    let data = text.trim();
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(data).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON response is invalid: {error}"))
    })?;
    Ok(Some(StreamFrame {
        event_name: None,
        data: data.to_string(),
    }))
}

#[cfg(test)]
mod tests;
