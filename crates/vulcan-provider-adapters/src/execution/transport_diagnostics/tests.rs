//! Regression coverage for bounded provider-controlled trace labels.
//! 供应商可控追踪标签有界性的回归覆盖。

use super::bounded_trace_label;

/// Verifies provider-controlled shape labels are bounded and cannot inject trace syntax.
/// 验证供应商控制的形状标签有界且无法注入追踪语法。
#[test]
fn compact_shape_trace_labels_are_bounded_and_sanitized() {
    // normalized removes whitespace and punctuation while preserving safe structural characters.
    // normalized 移除空白与标点，同时保留安全结构字符。
    let normalized = bounded_trace_label("compaction\r\nsecret:value/field");
    assert_eq!(normalized, "compaction__secret_value_field");

    // bounded caps provider-controlled labels at the documented maximum length.
    // bounded 把供应商控制的标签限制在文档规定的最大长度内。
    let bounded = bounded_trace_label("a".repeat(80).as_str());
    assert_eq!(bounded.len(), 64);
}
