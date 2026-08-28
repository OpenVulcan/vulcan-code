//! V1 service-facing event summary DTO (build-time fixture).
//! V1 服务面事件摘要 DTO(构建期 fixture)。
//!
//! `ConversationEventSummary` mirrors the main-event envelope one-to-one so
//! hydration, forward pagination, and live deltas all project through the same
//! shape. The WebUI reducer (the conversation event window store) consumes
//! exactly these summaries and never rebuilds a second message/record model.
//! `ConversationEventSummary` 与主事件 envelope 一一对应，使 hydration、向前
//! 分页与实时 delta 都通过同一形态投影。WebUI reducer(会话事件窗口 store)
//! 精确消费这些摘要，绝不再重建第二套 message/record 模型。

use serde::{Deserialize, Serialize};

use crate::v1::event::{ConversationEvent, EventBody, EventVisibility};
use crate::v1::ids::{Seq, TurnId};

/// One frontend-visible event summary (hydration and live share this DTO).
/// 一条前端可见事件摘要(hydration 与实时共用此 DTO)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEventSummary {
    /// Contiguous stream position.
    /// 连续流位置。
    pub seq: Seq,
    /// Owning logical turn, when any.
    /// 所属逻辑回合(如有)。
    pub turn_id: Option<TurnId>,
    /// Immutable creation time in Unix milliseconds.
    /// 不可变创建时间(Unix 毫秒)。
    pub created_at_ms: i64,
    /// Mutable last-update time in Unix milliseconds.
    /// 可变最近更新时间(Unix 毫秒)。
    pub updated_at_ms: i64,
    /// Monotonic semantic revision for same-seq evolution.
    /// 用于同 seq 演化的单调语义修订号。
    pub event_revision: u64,
    /// Typed projection strategy.
    /// 强类型投影策略。
    pub visibility: EventVisibility,
    /// Complete strongly typed event body; no semantic field is discarded at
    /// the service boundary.
    /// 完整强类型事件体；服务边界不丢弃任何语义字段。
    pub event: EventBody,
}

/// One ownership-bound event batch shared by hydration, pagination, live
/// updates, and atomic rebases.
/// hydration、分页、实时更新与原子 rebase 共享的一批带归属事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEventWindowBatch {
    /// Cross-system session identity owning every event in this batch.
    /// 拥有本批次全部事件的跨系统会话身份。
    pub session_id: String,
    /// Monotonic window generation used to reject delayed old responses.
    /// 用于拒绝延迟旧响应的单调窗口代次。
    pub window_generation: u64,
    /// Complete event summaries in exact seq order.
    /// 按精确 seq 顺序排列的完整事件摘要。
    pub events: Vec<ConversationEventSummary>,
}

impl ConversationEventWindowBatch {
    /// Projects one ownership-bound event slice without changing its order.
    /// 在不改变顺序的情况下投影一段带归属事件。
    pub fn from_events(
        session_id: impl Into<String>,
        window_generation: u64,
        events: &[ConversationEvent],
    ) -> Self {
        Self {
            session_id: session_id.into(),
            window_generation,
            events: events
                .iter()
                .map(ConversationEventSummary::from_event)
                .collect(),
        }
    }
}

impl ConversationEventSummary {
    /// Projects one main event into its service summary.
    /// 把一条主事件投影为服务摘要。
    pub fn from_event(event: &ConversationEvent) -> Self {
        Self {
            seq: event.seq,
            turn_id: event.turn_id,
            created_at_ms: event.created_at_ms,
            updated_at_ms: event.updated_at_ms,
            event_revision: event.event_revision,
            visibility: event.visibility,
            event: event.event.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::document::SessionDocument;

    /// The summary mirrors the envelope fields exactly.
    /// 摘要精确镜像 envelope 字段。
    #[test]
    fn summary_mirrors_envelope() {
        let mut document = SessionDocument::new("s", "p", "t", 1);
        document
            .submit_root_input_fixture("hi", Vec::new(), 1)
            .unwrap();
        let api = document
            .start_api(
                crate::v1::event::ApiPurposeKind::MainGeneration,
                crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
                0,
            )
            .unwrap();
        document
            .append_assistant_text(
                api,
                "x",
                crate::v1::event::AssistantSemanticStatus::Completed,
                2,
            )
            .unwrap();
        let event = &document.events()[2];
        let summary = ConversationEventSummary::from_event(event);
        assert_eq!(summary.seq, event.seq);
        assert_eq!(summary.turn_id, event.turn_id);
        assert_eq!(summary.created_at_ms, event.created_at_ms);
        assert_eq!(summary.updated_at_ms, event.updated_at_ms);
        assert_eq!(summary.event_revision, event.event_revision);
        assert_eq!(summary.visibility, event.visibility);
        assert!(matches!(
            &summary.event,
            EventBody::AssistantText(text) if text.api_call_id == api && text.text == "x"
        ));
        let _ = api;
    }

    /// The summary serializes with the frozen field names.
    /// 摘要使用冻结字段名序列化。
    #[test]
    fn summary_golden_json() {
        let mut document = SessionDocument::new("s", "p", "t", 1);
        document
            .submit_root_input_fixture("hi", Vec::new(), 1)
            .unwrap();
        let event = &document.events()[0];
        let summary = ConversationEventSummary::from_event(event);
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains(r#""event":{"kind":"user_input""#));
        assert!(json.contains(r#""visibility":"provider""#));
        let decoded: ConversationEventSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, summary);
    }

    /// Tool events expose the tool call id as the stable entity id.
    /// 工具事件把 tool_call_id 暴露为稳定实体 id。
    #[test]
    fn tool_event_entity_id_is_tool_call_id() {
        let mut document = SessionDocument::new("s", "p", "t", 1);
        document
            .submit_root_input_fixture("run", Vec::new(), 1)
            .unwrap();
        let api = document
            .start_api(
                crate::v1::event::ApiPurposeKind::MainGeneration,
                crate::v1::event::ApiRequestSnapshot::new("openai", "gpt-5"),
                0,
            )
            .unwrap();
        let tool = document
            .start_tool(
                api,
                "shell",
                crate::v1::event::ToolPayload::try_new(serde_json::json!({})).unwrap(),
                None,
                2,
            )
            .unwrap();
        let event = document
            .events()
            .iter()
            .find(|event| matches!(&event.event, EventBody::ToolCall(_)))
            .unwrap();
        let summary = ConversationEventSummary::from_event(event);
        assert!(matches!(
            &summary.event,
            EventBody::ToolCall(event) if event.tool_call_id == tool
        ));
    }

    /// Ownership fields and event bodies survive the shared batch contract.
    /// 归属字段与事件体经过共享批次契约后保持不变。
    #[test]
    fn window_batch_round_trip_preserves_owner_and_events() {
        let mut document = SessionDocument::new("session-golden", "p", "t", 1);
        document
            .submit_root_input_fixture("hello", Vec::new(), 1)
            .unwrap();
        let batch =
            ConversationEventWindowBatch::from_events("session-golden", 7, document.events());
        let json = serde_json::to_string(&batch).unwrap();
        assert_eq!(
            json,
            r#"{"session_id":"session-golden","window_generation":7,"events":[{"seq":1,"turn_id":2,"created_at_ms":1,"updated_at_ms":1,"event_revision":1,"visibility":"provider","event":{"kind":"user_input","client_action_id":"fixture-root-1-1-1","input_id":1,"input_source":"root","text":"hello"}}]}"#
        );
        let decoded: ConversationEventWindowBatch = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, batch);
    }
}
