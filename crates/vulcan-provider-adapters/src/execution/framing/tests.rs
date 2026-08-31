//! Regression coverage for provider byte framing and retention limits.
//! 供应商字节分帧与保留上限的回归覆盖。

use super::*;

/// Verifies SSE framing survives arbitrary UTF-8 and record chunk boundaries.
/// 验证 SSE 分帧可以跨越任意 UTF-8 与记录分片边界。
#[test]
fn sse_parser_handles_arbitrary_chunks() {
    let bytes = "event: delta\r\ndata: {\"text\":\"你好\"}\r\n\r\n".as_bytes();
    let split = bytes
        .windows(3)
        .position(|window| window == "你".as_bytes())
        .expect("UTF-8 text must exist")
        + 1;
    let mut parser = IncrementalFrameParser::new(ProtocolTransportProfile::bounded_sse());
    assert!(
        parser
            .push(&bytes[..split])
            .expect("first chunk should buffer")
            .is_empty()
    );
    let frames = parser
        .push(&bytes[split..])
        .expect("second chunk should complete");
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].event_name.as_deref(), Some("delta"));
    assert!(frames[0].data.contains("你好"));
}

/// Verifies SSE framing accepts CR-only and mixed line endings permitted by the event-stream grammar.
/// 验证 SSE 分帧接受事件流语法允许的单 CR 与混合行结束符。
#[test]
fn sse_parser_accepts_cr_only_and_mixed_line_endings() {
    for bytes in [
        b"event: delta\rdata: {\"one\":1}\r\r".as_slice(),
        b"event: delta\r\ndata: {\"two\":2}\r\n\n".as_slice(),
        b"event: delta\ndata: {\"three\":3}\n\r\n".as_slice(),
    ] {
        let mut parser = IncrementalFrameParser::new(ProtocolTransportProfile::bounded_sse());
        let split = bytes.len() - 1;
        let mut frames = parser
            .push(&bytes[..split])
            .expect("first mixed-ending chunk should parse or buffer");
        frames.extend(
            parser
                .push(&bytes[split..])
                .expect("second mixed-ending chunk should parse"),
        );
        frames.extend(parser.finish().expect("mixed-ending stream should finish"));
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event_name.as_deref(), Some("delta"));
        assert!(frames[0].data.starts_with('{'));
    }
}

/// Verifies every registered non-SSE framing handles arbitrary chunk boundaries.
/// 验证每种已注册非 SSE 分帧方式都能处理任意分片边界。
#[test]
fn bounded_parser_supports_json_lines_ndjson_and_single_json() {
    for framing in [
        ProtocolStreamFraming::JsonLines,
        ProtocolStreamFraming::Ndjson,
    ] {
        let mut profile = ProtocolTransportProfile::bounded_sse();
        profile.framing = framing;
        let mut parser = IncrementalFrameParser::new(profile);
        let mut frames = parser
            .push(b"{\"one\":1}\n\n{\"two\"")
            .expect("first lines should parse");
        frames.extend(parser.push(b":2}\r\n").expect("split line should complete"));
        frames.extend(parser.finish().expect("line parser should finish"));
        assert_eq!(
            frames
                .iter()
                .map(|frame| frame.data.as_str())
                .collect::<Vec<_>>(),
            vec![r#"{"one":1}"#, r#"{"two":2}"#]
        );
    }

    let mut profile = ProtocolTransportProfile::bounded_sse();
    profile.framing = ProtocolStreamFraming::SingleJson;
    let mut parser = IncrementalFrameParser::new(profile);
    assert!(
        parser
            .push(b"{\"complete\"")
            .expect("single JSON should buffer")
            .is_empty()
    );
    assert!(
        parser
            .push(b":true}")
            .expect("single JSON should remain buffered")
            .is_empty()
    );
    assert_eq!(
        parser.finish().expect("single JSON should finish")[0].data,
        r#"{"complete":true}"#
    );
}

/// Verifies frame and pending-buffer limits fail deterministically.
/// 验证帧与待处理缓冲区上限会确定性失败。
#[test]
fn bounded_parser_rejects_oversized_frames_and_buffers() {
    let mut frame_profile = ProtocolTransportProfile::bounded_sse();
    frame_profile.max_frame_bytes = 4;
    frame_profile.max_buffer_bytes = 32;
    let mut frame_parser = IncrementalFrameParser::new(frame_profile);
    let frame_error = frame_parser
        .push(b"data: {}\n\n")
        .expect_err("oversized frame should fail");
    assert!(frame_error.message.contains("4 byte limit"));

    let mut buffer_profile = ProtocolTransportProfile::bounded_sse();
    buffer_profile.max_frame_bytes = 32;
    buffer_profile.max_buffer_bytes = 8;
    let mut buffer_parser = IncrementalFrameParser::new(buffer_profile);
    let buffer_error = buffer_parser
        .push(b"123456789")
        .expect_err("unterminated buffer should fail");
    assert!(buffer_error.message.contains("8 byte limit"));
}
