//! Strict JSONL segment serialization and parsing.
//! 严格 JSONL 分片序列化与解析。
//!
//! Every segment is one event per line in `seq` order. The reader reports the
//! exact file, segment, and line for any corruption and never truncates,
//! reorders, or backfills.
//! 每个分片是每行一个事件的 JSONL，按 `seq` 排列。读取器对任何损坏都报告
//! 精确文件、分片与行号，绝不截断、重排或补写。

use std::path::Path;

use vulcan_session_core::v1::event::ConversationEvent;
use vulcan_session_core::v1::ids::Seq;

use super::errors::SessionV1Error;

/// Serializes one ordered event slice into one JSONL byte buffer.
/// 把一个有序事件切片序列化为一个 JSONL 字节缓冲。
///
/// Events with non-contiguous seq cause an explicit error; oversized events
/// are never truncated or externalized.
/// 不连续 seq 的事件引发显式错误；超大事件绝不截断或外置。
pub fn serialize_segment(events: &[ConversationEvent]) -> Result<Vec<u8>, SessionV1Error> {
    let mut buffer = Vec::new();
    // Seed the zip range from the first authoritative seq so the loop checks exact contiguity.
    // 从首个权威 seq 初始化 zip 范围，使循环校验精确连续性。
    let expected_start = events.first().map(|event| event.seq().value()).unwrap_or(0);
    for (expected, event) in (expected_start..).zip(events.iter()) {
        if event.seq().value() != expected {
            return Err(SessionV1Error::discontinuity(
                "",
                "segment serialization",
                expected,
                event.seq().value(),
            ));
        }
        serde_json::to_writer(&mut buffer, event).map_err(|error| {
            SessionV1Error::atomic_write(
                "",
                "serialize",
                format!("serialize event seq {}: {error}", event.seq().value()),
            )
        })?;
        buffer.push(b'\n');
    }
    Ok(buffer)
}

/// Parses one segment file strictly, checking internal seq contiguity.
/// 严格解析一个分片文件，检查内部 seq 连续性。
///
/// `expected_start_seq` is the seq the first line must carry; pass `None` when
/// the caller validates the boundary itself.
/// `expected_start_seq` 是第一行必须携带的 seq；由调用方自校验边界时传入
/// `None`。
pub fn parse_segment(
    path: &Path,
    segment_number: u64,
    expected_start_seq: Option<Seq>,
) -> Result<Vec<ConversationEvent>, SessionV1Error> {
    let bytes = std::fs::read(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SessionV1Error::segment_missing(path, segment_number),
        _ => SessionV1Error::io(path.to_path_buf(), format!("read segment: {error}")),
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        SessionV1Error::segment_corrupt(
            path,
            segment_number,
            0,
            None,
            format!("segment is not valid UTF-8: {error}"),
        )
    })?;
    let mut events = Vec::new();
    let mut next_seq = expected_start_seq.map(|seq| seq.value()).unwrap_or(1u64);
    let mut byte_offset = 0u64;
    for line in text.split_inclusive('\n') {
        let line_number = events.len() as u64 + 1;
        let line_text = line.trim_end_matches(['\n', '\r']);
        let line_start = byte_offset;
        byte_offset += line.len() as u64;
        if line_text.trim().is_empty() {
            continue;
        }
        let event: ConversationEvent = serde_json::from_str(line_text).map_err(|error| {
            SessionV1Error::segment_corrupt(
                path,
                segment_number,
                line_number,
                Some(line_start),
                format!("invalid event line: {error}"),
            )
        })?;
        if event.seq().value() != next_seq {
            return Err(SessionV1Error::discontinuity(
                path,
                format!("segment s{segment_number:07}"),
                next_seq,
                event.seq().value(),
            ));
        }
        next_seq += 1;
        events.push(event);
    }
    Ok(events)
}

/// Returns the byte length of one segment file.
/// 返回一个分片文件的字节长度。
pub fn segment_byte_len(path: &Path, segment_number: u64) -> Result<u64, SessionV1Error> {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => SessionV1Error::segment_missing(path, segment_number),
            _ => SessionV1Error::io(path.to_path_buf(), format!("stat segment: {error}")),
        })
}
