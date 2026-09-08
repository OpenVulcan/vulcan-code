//! Canonical native turn endpoints and external-compatible business payload projection.
//! 规范原生回合端点与外置兼容业务载荷投影。

use serde::Serialize;
use vulcan_memorymesh_core::{CoreError, ErrorCode, ErrorStage, Role, Turn};
use vulcan_vmm::{PostActionTimelineItem, PostActionTimelineType};

/// Carries only business dialogue fields; storage ownership, lifecycle, and neighbor metadata stay private.
/// 仅携带业务对话字段；存储归属、生命周期及相邻回合元数据保持内部私有。
#[derive(Serialize)]
pub(super) struct NativeTurnDialogue {
    /// Carries the canonical initial user message, including an empty sanitized message.
    /// 携带规范初始用户消息，包括清洗后为空的消息。
    pub user: String,
    /// Carries middle interactions without promoting an ask message to the final answer.
    /// 携带中间交互，不将 ask 消息提升为最终回答。
    pub timeline: Vec<PostActionTimelineItem>,
    /// Carries the canonical final answer; empty means absent or cleaned away.
    /// 携带规范最终回答；为空表示缺失或被清洗移除。
    pub assistant: String,
}

/// Projects one native turn using the exact timestamps assigned by post_action_timeline and emit_intake_turn.
/// 根据 post_action_timeline 与 emit_intake_turn 写入的准确时间戳投影一个原生回合。
/// Returns a typed error for malformed native timelines; never guesses another role or field source.
/// 原生时间线格式错误时返回强类型错误；绝不猜测另一角色或字段来源。
pub(super) fn native_turn_dialogue(turn: &Turn) -> Result<NativeTurnDialogue, CoreError> {
    // Native intake always starts with the main user message before the completion timestamp.
    // 原生准入始终以完成时间戳之前的主用户消息开头。
    let (user, rest) = turn.timeline.split_first().ok_or_else(invalid_timeline)?;
    if user.role != Role::User || user.occurred_at_ns >= turn.created_at_ns {
        return Err(invalid_timeline());
    }
    // Only the final assistant is stamped at completed_at_ns, which Core stores as created_at_ns.
    // 只有最终助手消息标记为 completed_at_ns，Core 将该值保存为 created_at_ns。
    // Middle asks are strictly earlier, even if no final assistant answer was supplied.
    // 中间 ask 严格早于该时间，即使没有提交最终助手回答也如此。
    let (middle, assistant) = match rest.split_last() {
        Some((last, middle))
            if last.role == Role::Assistant && last.occurred_at_ns == turn.created_at_ns =>
        {
            (middle, last.content.trim().to_owned())
        }
        _ => (rest, String::new()),
    };
    // Preserve the host's monotonic intermediate order and reject non-business role pollution.
    // 保留宿主单调递增的中间顺序，并拒绝非业务角色污染。
    let mut previous_at = user.occurred_at_ns;
    // Allocate exactly the known middle segment rather than reconstructing speaker buckets.
    // 仅为已确认中间片段分配空间，不重构按发言者分组的内容。
    let mut timeline = Vec::with_capacity(middle.len());
    for item in middle {
        if item.occurred_at_ns <= previous_at || item.occurred_at_ns >= turn.created_at_ns {
            return Err(invalid_timeline());
        }
        previous_at = item.occurred_at_ns;
        // These are the only two roles accepted by the host PostAction contract.
        // 这两种角色是宿主 PostAction 契约仅允许的角色。
        let kind = match item.role {
            Role::User => PostActionTimelineType::User,
            Role::Assistant => PostActionTimelineType::Assistant,
            Role::System | Role::Tool => return Err(invalid_timeline()),
        };
        timeline.push(PostActionTimelineItem {
            kind,
            content: item.content.trim().to_owned(),
        });
    }
    Ok(NativeTurnDialogue {
        user: user.content.trim().to_owned(),
        timeline,
        assistant,
    })
}

/// Applies Go encoding/json's five additional string escapes to already valid serialized JSON.
/// 对已合法序列化的 JSON 应用 Go encoding/json 的五种额外字符串转义。
pub(super) fn escape_business_json(serialized: String) -> String {
    // These characters cannot form JSON structural syntax; literal backslash escapes remain untouched.
    // 这些字符不构成 JSON 结构语法；字面反斜杠转义保持不变。
    let mut escaped = String::with_capacity(serialized.len());
    for character in serialized.chars() {
        match character {
            '<' => escaped.push_str("\\u003c"),
            '>' => escaped.push_str("\\u003e"),
            '&' => escaped.push_str("\\u0026"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Creates a stable model-facing diagnostic without echoing private conversation text.
/// 构造稳定的面向模型诊断，不回显私有对话文本。
fn invalid_timeline() -> CoreError {
    CoreError::new(
        ErrorCode::InvalidArgument,
        ErrorStage::Effect,
        "native VMM turn does not match canonical user/middle/final-assistant boundaries",
    )
}
