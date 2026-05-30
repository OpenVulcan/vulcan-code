//! Hook-payload projection and VMM context-injection conversion.
//! Hook payload 投影与 VMM 上下文注入转换。

use crate::config::VmmEffectiveConfig;
use crate::protocol::{
    ContextItem, PostActionRequest, PostActionTimelineItem, PostActionTimelineType,
    PreCheckRequest, PreCheckResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt::{self, Display};
use vulcan_context_injection::{
    InjectionCompactionPolicy, InjectionInsert, InjectionInsertItem, InjectionKind, InjectionScope,
};

/// Maximum user content characters sent to VMM.
/// 发送给 VMM 的用户内容最大字符数。
const USER_CONTENT_LIMIT: usize = 4_000;

/// Maximum assistant content characters sent to VMM.
/// 发送给 VMM 的助手内容最大字符数。
const ASSISTANT_CONTENT_LIMIT: usize = 6_000;

/// Maximum middle timeline content characters sent to VMM.
/// 发送给 VMM 的中间时间线内容最大字符数。
const TIMELINE_CONTENT_LIMIT: usize = 1_600;

/// Error returned while projecting hook payload into VMM requests.
/// 将 hook payload 投影为 VMM 请求时返回的错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmAdapterError {
    /// Stable error kind.
    /// 稳定错误类型。
    pub kind: String,
    /// Human-readable error message.
    /// 人类可读错误信息。
    pub message: String,
}

impl VmmAdapterError {
    /// Builds one adapter error.
    /// 构建一个适配器错误。
    fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

impl Display for VmmAdapterError {
    /// Formats the adapter error for hook diagnostics.
    /// 为 hook 诊断格式化适配器错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for VmmAdapterError {}

/// Hook timeline item normalized for VMM projection decisions.
/// 为 VMM 投影决策归一化后的 hook 时间线项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmHookTimelineItem {
    /// Generic hook role such as user, assistant, or tool.
    /// 通用 hook 角色，例如 user、assistant 或 tool。
    pub role: String,
    /// Generic hook event kind such as user_followup or ask_answer.
    /// 通用 hook 事件类型，例如 user_followup 或 ask_answer。
    pub kind: String,
    /// Compact event content.
    /// 精简后的事件内容。
    pub content: String,
    /// Optional source id from hook payload.
    /// hook payload 中的可选来源标识。
    pub source_id: Option<String>,
}

/// Completed turn projection used before building PostAction.
/// 构建 PostAction 前使用的已完成回合投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmCompletedTurnProjection {
    /// Host session identifier.
    /// 宿主会话标识。
    pub session_id: String,
    /// First user prompt.
    /// 首个用户提示词。
    pub user_content: String,
    /// Final assistant answer.
    /// 最终助手回答。
    pub assistant_content: String,
    /// VMM middle timeline items.
    /// VMM 中间时间线项。
    pub timeline: Vec<PostActionTimelineItem>,
}

/// Builds a VMM PreCheck request from a precheck hook payload.
/// 从 precheck hook payload 构建 VMM PreCheck 请求。
pub fn precheck_request_from_payload(
    payload: &Value,
    config: &VmmEffectiveConfig,
) -> Result<PreCheckRequest, VmmAdapterError> {
    let session_id = required_string(payload, "session_id")?;
    let user_content = required_string(payload, "user_content")?;
    Ok(PreCheckRequest {
        session_id,
        user_id: required_config_id(config.user_id, "user_id")?,
        project_id: required_config_id(config.project_id, "project_id")?,
        user_content: compact_text(&user_content, USER_CONTENT_LIMIT),
        recall_mode: config.precheck_recall_mode(),
    })
}

/// Builds a VMM PostAction request from a completed-turn hook payload.
/// 从 completed-turn hook payload 构建 VMM PostAction 请求。
pub fn post_action_request_from_completed_payload(
    payload: &Value,
    config: &VmmEffectiveConfig,
) -> Result<PostActionRequest, VmmAdapterError> {
    let projection = completed_turn_projection_from_payload(payload)?;
    Ok(PostActionRequest {
        session_id: projection.session_id,
        user_id: required_config_id(config.user_id, "user_id")?,
        project_id: required_config_id(config.project_id, "project_id")?,
        user_content: projection.user_content,
        assistant_content: projection.assistant_content,
        timeline: projection.timeline,
    })
}

/// Builds a VMM ChatCompact request from a compaction hook payload.
/// 从压缩 hook payload 构建 VMM ChatCompact 请求。
pub fn chat_compact_request_from_payload(
    payload: &Value,
    config: &VmmEffectiveConfig,
) -> Result<crate::protocol::ChatCompactRequest, VmmAdapterError> {
    Ok(crate::protocol::ChatCompactRequest {
        session_id: required_string(payload, "session_id")?,
        user_id: required_config_id(config.user_id, "user_id")?,
        project_id: required_config_id(config.project_id, "project_id")?,
    })
}

/// Converts a VMM PreCheck response into a transient context injection.
/// 将 VMM PreCheck 响应转换成临时上下文注入。
pub fn injection_from_precheck_response(
    response: &PreCheckResponse,
    config: &VmmEffectiveConfig,
) -> Option<InjectionInsert> {
    if !response.should_inject {
        return None;
    }
    let items = response
        .context_items
        .iter()
        .filter_map(format_context_item)
        .map(|content| InjectionInsertItem {
            content,
            ttl_turns: Some(config.implicit_memory_turns),
            compaction_policy: Some(InjectionCompactionPolicy::Keep),
            source_display: Some(String::from("VMM")),
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        return None;
    }
    Some(InjectionInsert {
        kind: InjectionKind::Transient,
        tag: config.injection_tag.clone(),
        description: config.injection_description.clone(),
        items,
        ttl_turns: Some(config.implicit_memory_turns),
        compaction_policy: InjectionCompactionPolicy::Keep,
        scope: InjectionScope::Session,
        priority: -50,
        source_display: Some(String::from("VMM")),
    })
}

/// Builds a completed-turn projection from generic hook JSON.
/// 从通用 hook JSON 构建已完成回合投影。
fn completed_turn_projection_from_payload(
    payload: &Value,
) -> Result<VmmCompletedTurnProjection, VmmAdapterError> {
    let session_id = required_string(payload, "session_id")?;
    let user_content = compact_text(
        &required_string(payload, "user_content")?,
        USER_CONTENT_LIMIT,
    );
    let assistant_content = compact_text(
        &required_string(payload, "assistant_content")?,
        ASSISTANT_CONTENT_LIMIT,
    );
    let timeline = parse_hook_timeline(payload.get("timeline"))
        .into_iter()
        .filter_map(project_hook_timeline_item)
        .collect::<Vec<_>>();
    if user_content.is_empty() {
        return Err(VmmAdapterError::new(
            "missing_user_content",
            "completed-turn payload does not contain user_content",
        ));
    }
    if assistant_content.is_empty() {
        return Err(VmmAdapterError::new(
            "missing_assistant_content",
            "completed-turn payload does not contain assistant_content",
        ));
    }
    Ok(VmmCompletedTurnProjection {
        session_id,
        user_content,
        assistant_content,
        timeline: dedupe_adjacent_timeline(timeline),
    })
}

/// Parses hook timeline values into normalized items.
/// 将 hook timeline 值解析为归一化条目。
fn parse_hook_timeline(value: Option<&Value>) -> Vec<VmmHookTimelineItem> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_hook_timeline_item).collect())
        .unwrap_or_default()
}

/// Parses one hook timeline item.
/// 解析一条 hook 时间线项。
fn parse_hook_timeline_item(value: &Value) -> Option<VmmHookTimelineItem> {
    let role = optional_string(value, "role")?;
    let kind = optional_string(value, "kind")?;
    let content = compact_text(&optional_string(value, "content")?, TIMELINE_CONTENT_LIMIT);
    if content.is_empty() {
        return None;
    }
    Some(VmmHookTimelineItem {
        role,
        kind,
        content,
        source_id: optional_string(value, "source_id"),
    })
}

/// Projects one generic hook timeline item into the VMM middle timeline contract.
/// 将一条通用 hook 时间线项投影到 VMM 中间时间线契约。
fn project_hook_timeline_item(item: VmmHookTimelineItem) -> Option<PostActionTimelineItem> {
    let kind = match (item.role.as_str(), item.kind.as_str()) {
        ("user", "user_followup" | "user_interrupt") => PostActionTimelineType::User,
        ("assistant", "ask") => PostActionTimelineType::Assistant,
        ("user", "ask_answer" | "ask_rejected") => PostActionTimelineType::User,
        _ => return None,
    };
    Some(PostActionTimelineItem {
        kind,
        content: item.content,
    })
}

/// Removes only adjacent duplicate VMM timeline items.
/// 只移除相邻重复的 VMM 时间线项。
fn dedupe_adjacent_timeline(timeline: Vec<PostActionTimelineItem>) -> Vec<PostActionTimelineItem> {
    let mut deduped = Vec::new();
    let mut seen_adjacent = BTreeSet::new();
    for item in timeline {
        let key = format!("{}:{}", item.kind.as_wire_type(), item.content);
        if seen_adjacent.contains(&key) {
            continue;
        }
        seen_adjacent.clear();
        seen_adjacent.insert(key);
        deduped.push(item);
    }
    deduped
}

/// Formats one PreCheck context item into provider-visible recall content.
/// 将一条 PreCheck 上下文项格式化为 Provider 可见召回内容。
fn format_context_item(item: &ContextItem) -> Option<String> {
    let text = collapse_context_item_text(&item.text);
    if text.is_empty() {
        return None;
    }
    let turn_label = format_context_turn_label(item);
    let source_score = format_context_source_score(item.score);
    let created_time = format_context_created_time(&item.created_datetime)
        .map(|time| format!("|TIME:{time}"))
        .unwrap_or_default();
    Some(format!(
        "[{turn_label}|SOURCE:{source_score}{created_time}]{text}"
    ))
}

/// Builds the dialogue lookup marker for one recalled context item.
/// 为单条召回上下文构建对话回查标记。
fn format_context_turn_label(item: &ContextItem) -> String {
    if item.has_dialogue && item.turn_id > 0 {
        format!("TURN_ID:{}", item.turn_id)
    } else {
        String::from("NOTURN")
    }
}

/// Formats one VMM relevance score for provider-visible context.
/// 为模型可见上下文格式化一个 VMM 相关性分数。
fn format_context_source_score(score: f64) -> String {
    if score.is_finite() {
        format!("{score:.3}")
    } else {
        String::from("0.000")
    }
}

/// Normalizes one optional VMM creation datetime for inline metadata.
/// 为行内元数据规范化一个可选的 VMM 创建时间。
fn format_context_created_time(value: &str) -> Option<String> {
    let normalized = collapse_context_item_text(value);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// Collapses a recalled context field into one compact inline string.
/// 将召回上下文字段压缩为一条紧凑的行内字符串。
fn collapse_context_item_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a required string field from JSON.
/// 从 JSON 中读取必填字符串字段。
fn required_string(payload: &Value, key: &str) -> Result<String, VmmAdapterError> {
    optional_string(payload, key).ok_or_else(|| {
        VmmAdapterError::new(
            format!("missing_{key}"),
            format!("payload does not contain a non-empty {key} string"),
        )
    })
}

/// Reads an optional string field from JSON.
/// 从 JSON 中读取可选字符串字段。
fn optional_string(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(normalize_text)
        .filter(|value| !value.is_empty())
}

/// Requires one resolved numeric config id.
/// 要求一个已解析的数字配置标识。
fn required_config_id(value: Option<u64>, key: &str) -> Result<u64, VmmAdapterError> {
    value.filter(|id| *id > 0).ok_or_else(|| {
        VmmAdapterError::new(
            format!("missing_{key}"),
            format!("effective VMM config does not contain a valid {key}"),
        )
    })
}

/// Compacts text to one character limit after normalization.
/// 归一化后按字符上限压缩文本。
fn compact_text(value: &str, limit: usize) -> String {
    let normalized = normalize_text(value);
    if normalized.chars().count() <= limit {
        return normalized;
    }
    let mut compacted = normalized
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    compacted.push('…');
    compacted
}

/// Normalizes text whitespace while preserving paragraph boundaries.
/// 规范化文本空白，同时保留段落边界。
fn normalize_text(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    //! Tests for VMM hook adapter projection.
    //! VMM hook 适配投影测试。

    use super::*;
    use crate::config::VmmConfigLayer;
    use serde_json::json;

    /// Builds one ready effective config for adapter tests.
    /// 为适配器测试构建一个可用最终配置。
    fn ready_config() -> VmmEffectiveConfig {
        VmmEffectiveConfig::resolve(
            &VmmConfigLayer {
                grpc_target: Some(String::from("127.0.0.1:50051")),
                user_id: Some(1),
                project_id: Some(2),
                implicit_memory_turns: Some(5),
                ..VmmConfigLayer::default()
            },
            None,
            None,
        )
    }

    #[test]
    fn precheck_payload_builds_request_with_compact_recall_mode() {
        let mut config = ready_config();
        config.session_compact_recall = true;
        let payload = json!({
            "session_id": "session-a",
            "user_content": " hello "
        });

        let request = precheck_request_from_payload(&payload, &config).expect("request");

        assert_eq!(request.session_id, "session-a");
        assert_eq!(request.user_id, 1);
        assert_eq!(request.project_id, 2);
        assert_eq!(
            request.recall_mode,
            crate::protocol::PreCheckRecallMode::PreCheckRecallModeSessionCompact
        );
    }

    #[test]
    fn completed_payload_projects_only_vmm_middle_timeline() {
        let payload = json!({
            "session_id": "session-a",
            "user_content": "first user",
            "assistant_content": "final answer",
            "timeline": [
                { "role": "tool", "kind": "tool_started", "content": "shell" },
                { "role": "user", "kind": "user_interrupt", "content": "extra user" },
                { "role": "assistant", "kind": "ask", "content": "need choice?" },
                { "role": "user", "kind": "ask_answer", "content": "choice A" },
                { "role": "tool", "kind": "tool_finished", "content": "done" }
            ]
        });

        let request =
            post_action_request_from_completed_payload(&payload, &ready_config()).expect("request");

        assert_eq!(request.timeline.len(), 3);
        assert_eq!(request.timeline[0].kind, PostActionTimelineType::User);
        assert_eq!(request.timeline[0].content, "extra user");
        assert_eq!(request.timeline[1].kind, PostActionTimelineType::Assistant);
        assert_eq!(request.timeline[1].content, "need choice?");
        assert_eq!(request.timeline[2].kind, PostActionTimelineType::User);
        assert_eq!(request.timeline[2].content, "choice A");
    }

    #[test]
    fn precheck_response_converts_context_items_to_transient_injection() {
        let response = PreCheckResponse {
            should_inject: true,
            context_items: vec![
                ContextItem {
                    text: String::from("memory one\nsecond line"),
                    score: 0.9986,
                    turn_id: 11,
                    has_dialogue: true,
                    created_datetime: String::from("2026-05-02 14:20:00"),
                    ..ContextItem::default()
                },
                ContextItem {
                    text: String::from("detached memory"),
                    score: 0.25,
                    turn_id: 22,
                    has_dialogue: false,
                    ..ContextItem::default()
                },
                ContextItem {
                    text: String::from(""),
                    ..ContextItem::default()
                },
            ],
            ..PreCheckResponse::default()
        };

        let insert = injection_from_precheck_response(&response, &ready_config()).expect("insert");

        assert_eq!(insert.tag, "vmm_inject");
        assert_eq!(insert.ttl_turns, Some(5));
        assert_eq!(insert.items.len(), 2);
        assert_eq!(
            insert.items[0].content,
            "[TURN_ID:11|SOURCE:0.999|TIME:2026-05-02 14:20:00]memory one second line"
        );
        assert_eq!(
            insert.items[1].content,
            "[NOTURN|SOURCE:0.250]detached memory"
        );
        assert_eq!(insert.items[0].source_display.as_deref(), Some("VMM"));
    }
}
