//! Readable projection of the validated media-understanding result.
//! 已校验媒体理解结果的可读投影。

use serde::Deserialize;
use serde_json::Value;

/// Exact fields consumed from the already validated canonical envelope.
/// 从已校验规范信封消费的准确字段。
#[derive(Deserialize)]
struct Understanding {
    /// Media kind selected by the capability.
    /// 能力所选的媒体类型。
    media_kind: String,
    /// Sole answer body.
    /// 唯一回答正文。
    answer: String,
    /// Optional observed timeline entries.
    /// 可选的已观察时间线条目。
    segments: Vec<Segment>,
    /// Nonfatal warnings returned by the producer.
    /// 生产者返回的非致命警告。
    warnings: Vec<String>,
}

/// One canonical segment, including explicitly unknown endpoints.
/// 一条规范片段，包含明确未知的端点。
#[derive(Deserialize)]
struct Segment {
    /// Observed start offset in milliseconds, or unknown.
    /// 已观察的起始毫秒偏移，或未知。
    start_ms: Option<u64>,
    /// Observed end offset in milliseconds, or unknown.
    /// 已观察的结束毫秒偏移，或未知。
    end_ms: Option<u64>,
    /// Independent segment content.
    /// 独立片段正文。
    content: String,
}

/// Converts a validated payload to Markdown while preserving timeline and warnings.
/// 将已校验载荷转为 Markdown，同时保留时间线与警告。
/// Takes the canonical value; returns one body, or an explicit schema error.
/// 接收规范值，返回一份正文或明确的结构错误。
pub(super) fn render(payload: Value) -> Result<String, String> {
    // Deserialization follows validation; missing fields must fail rather than be guessed.
    // 反序列化在校验之后进行，缺失字段必须失败，不能猜测。
    let result: Understanding = serde_json::from_value(payload)
        .map_err(|error| format!("Invalid media-understanding projection: {error}"))?;
    // A heading keeps a literal JSON answer as source text through frontend envelope decoding.
    // 标题使 JSON 形式的原始回答在前端信封解码后仍保持正文语义。
    let mut output = format!(
        "# {} understanding\n\n{}\n",
        result.media_kind, result.answer
    );
    if !result.segments.is_empty() {
        output.push_str("\n## Timeline\n");
        for segment in result.segments {
            output.push_str(&format!(
                "\n### {}–{} ms\n\n{}\n",
                endpoint(segment.start_ms),
                endpoint(segment.end_ms),
                segment.content
            ));
        }
    }
    if !result.warnings.is_empty() {
        output.push_str("\n## Warnings\n");
        for warning in result.warnings {
            output.push_str(&format!("\n{warning}\n"));
        }
    }
    Ok(output)
}

/// Formats a nullable timeline endpoint without substituting a false zero.
/// 格式化可空时间线端点，不替换为虚假的零。
/// Takes an optional millisecond offset; returns its number or the unknown marker.
/// 接收可选毫秒偏移，返回数值或未知标记。
fn endpoint(value: Option<u64>) -> String {
    value.map_or_else(|| String::from("unknown"), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    /// Ensures the real invocation envelope stores the answer only once and retains warnings.
    /// 确保实际调用信封只保存一份回答，并保留警告。
    #[test]
    fn shares_one_body_and_preserves_unknown_timestamps() {
        // The fixture follows the validator's exact segment shape.
        // 夹具遵循校验器准确的片段结构。
        let result = super::super::media_understanding_invocation_result(serde_json::json!({
            "schema_version": "vulcan.media-understanding.v1", "media_kind": "video",
            "answer": "Unique answer", "segments": [
                { "start_ms": null, "end_ms": 2000, "content": "Unique segment" }
            ], "warnings": ["Partial visibility"]
        }))
        .expect("projection");
        assert!(result.host_result.is_none());
        // A missing endpoint stays visibly unknown, and no JSON envelope duplicates source text.
        // 缺失端点保持可见未知状态，JSON 信封不会复制正文。
        let body = result.model_output.as_str().expect("Markdown");
        assert_eq!(body.matches("Unique answer").count(), 1);
        assert!(body.contains("unknown–2000 ms"));
        assert!(body.contains("Partial visibility"));
    }
}
