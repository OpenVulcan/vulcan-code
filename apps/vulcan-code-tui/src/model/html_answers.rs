//! HTML presentation facts shared by the service and synthetic backends.
//! 真实服务与合成后端共享的 HTML 展示事实。

use std::collections::BTreeMap;
pub use vulcan_session_core::v1::config::HtmlAnswerMode;
pub use vulcan_session_core::v1::html_answers::{
    HtmlAnswerAvailability, HtmlAnswerEventProjection,
};

/// Exact loaded generation, durable preference and source annotations; contains no viewer secrets.
/// 精确加载代次、可靠偏好和源码标注；不包含查看秘密。
#[derive(Debug, Clone, Default)]
pub struct HtmlAnswerSession {
    /// None until the owning window has loaded; local drafts use generation zero.
    /// 归属窗口加载前为空；本地草稿使用零代次。
    pub generation: Option<u64>,
    /// None means the authority is not loaded, rather than a confirmed disabled preference.
    /// 为空表示权威尚未加载，不表示已确认关闭。
    pub mode: Option<HtmlAnswerMode>,
    /// Core annotations keyed by their exact owning event sequence.
    /// 按精确所属事件序号索引的 Core 标注。
    pub projections: BTreeMap<u64, HtmlAnswerEventProjection>,
}

impl HtmlAnswerSession {
    /// Apply only forward desired/prepared revisions inside this exact generation.
    /// 仅在当前精确代次内应用向前的期望及准备修订。
    pub fn accept_mode(&mut self, next: HtmlAnswerMode) {
        let newer = self.mode.as_ref().is_none_or(|current| {
            if next.revision != current.revision {
                return next.revision > current.revision;
            }
            next.prepared.as_ref().is_some_and(|prepared| {
                current
                    .prepared
                    .as_ref()
                    .is_none_or(|old| prepared.revision > old.revision)
            })
        });
        if newer {
            self.mode = Some(next);
        }
    }

    /// Return whether the saved preference has not reached the next prepared business request.
    /// 返回已保存偏好是否尚未到达下一次业务请求准备阶段。
    pub fn awaiting_request(&self) -> bool {
        self.mode.as_ref().is_some_and(|mode| {
            mode.prepared
                .as_ref()
                .map_or(mode.enabled, |prepared| prepared.revision != mode.revision)
        })
    }
    /// Return the ready immutable source selected by `seq` and `ordinal` for this root session.
    /// 返回此根会话内由 `seq` 与 `ordinal` 选中的就绪不可变源码。
    /// Missing authority or noncanonical session IDs have no executable identity.
    /// 缺少权威或会话标识不规范时不存在可执行身份。
    pub fn source(
        &self,
        session_id: &str,
        seq: u64,
        ordinal: usize,
    ) -> Option<vulcan_service_api::HtmlAnswerSourceIdentity> {
        let projection = self.projections.get(&seq)?;
        let answer = projection
            .answers
            .iter()
            .find(|answer| answer.ordinal == ordinal)?;
        let HtmlAnswerAvailability::Ready { artifact_id, .. } = &answer.availability else {
            return None;
        };
        Some(vulcan_service_api::HtmlAnswerSourceIdentity {
            session_id: session_id.parse().ok()?,
            parent_session_id: None,
            window_generation: self.generation?,
            event_seq: seq,
            event_revision: projection.event_revision,
            artifact_id: artifact_id.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A late save acknowledgement cannot erase the request's already-confirmed preparation.
    /// 迟到的保存确认不能抹除请求已确认的准备状态。
    #[test]
    fn html_mode_preparation_never_regresses() {
        let mut state = HtmlAnswerSession::default();
        let desired: HtmlAnswerMode = serde_json::from_value(
            serde_json::json!({"enabled":true,"revision":2,"prepared":null}),
        )
        .unwrap();
        state.accept_mode(desired.clone());
        assert!(state.awaiting_request());
        let prepared: HtmlAnswerMode = serde_json::from_value(serde_json::json!({"enabled":true,"revision":2,"prepared":{"enabled":true,"revision":2}})).unwrap();
        state.accept_mode(prepared.clone());
        state.accept_mode(desired);
        assert_eq!(state.mode, Some(prepared));
        assert!(!state.awaiting_request());
    }

    /// Cards retain original order and invalid Core ranges cannot become clickable display segments.
    /// 卡片保留原始顺序，非法 Core 范围不能成为可点击展示片段。
    #[test]
    fn html_answer_parts_preserve_source_order_and_reject_invalid_ranges() {
        let (_, event, mut projection) = fixture();
        let vulcan_session_core::v1::EventBody::AssistantText(body) = event.event else {
            unreachable!()
        };
        let parts = source_parts(&body.text, Some(&projection));
        assert!(
            matches!(parts.as_slice(), [HtmlAnswerDisplayPart::Text(before), HtmlAnswerDisplayPart::Answer(_), HtmlAnswerDisplayPart::Text(after)] if before.contains("中文") && after.contains("after"))
        );
        projection.answers.first_mut().unwrap().fence.byte_end = body.text.len() + 1;
        assert!(
            matches!(source_parts(&body.text, Some(&projection)).as_slice(), [HtmlAnswerDisplayPart::Text(text)] if *text == body.text)
        );
    }
}

/// One authoritative display segment; action identity is never encoded in model-authored Markdown.
/// 一段权威展示内容；动作身份绝不编码进模型生成的 Markdown。
pub enum HtmlAnswerDisplayPart<'a> {
    /// Original ordinary text between result fences.
    /// 结果围栏之间的原始普通文字。
    Text(&'a str),
    /// Exact Core descriptor at its original position.
    /// 原始位置上的精确 Core 描述。
    Answer(&'a vulcan_session_core::v1::html_answers::HtmlAnswerDescriptor),
}

/// Split exact Core fence intervals into typed result cards, preserving ordinary source order.
/// 按精确 Core 围栏区间拆分强类型结果卡，保留普通源码顺序。
/// Invalid ranges return only original text; no action is minted from unverified offsets.
/// 非法范围仅返回原始文字；不从未经验证的偏移生成动作。
pub fn source_parts<'a>(
    text: &'a str,
    projection: Option<&'a HtmlAnswerEventProjection>,
) -> Vec<HtmlAnswerDisplayPart<'a>> {
    let Some(projection) = projection.filter(|value| !value.answers.is_empty()) else {
        return vec![HtmlAnswerDisplayPart::Text(text)];
    };
    let mut parts = Vec::new();
    let mut cursor = 0;
    for answer in &projection.answers {
        let Some(before) = text.get(cursor..answer.fence.byte_start) else {
            return vec![HtmlAnswerDisplayPart::Text(text)];
        };
        if text
            .get(answer.fence.byte_start..answer.fence.byte_end)
            .is_none()
        {
            return vec![HtmlAnswerDisplayPart::Text(text)];
        }
        parts.push(HtmlAnswerDisplayPart::Text(before));
        parts.push(HtmlAnswerDisplayPart::Answer(answer));
        cursor = answer.fence.byte_end;
    }
    parts.push(HtmlAnswerDisplayPart::Text(&text[cursor..]));
    parts
}

/// Ephemeral viewer receipt; diagnostics deliberately omit its URL and one-use ticket.
/// 瞬态查看回执；诊断刻意省略地址及单次票据。
#[derive(Clone)]
pub struct HtmlAnswerLink(pub vulcan_service_api::HtmlAnswerViewLinkResponse);

impl std::fmt::Debug for HtmlAnswerLink {
    /// Redact the capability while preserving the type in diagnostic output.
    /// 在诊断输出保留类型并遮蔽能力凭据。
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str("HtmlAnswerLink(<redacted>)")
    }
}

/// Explicit delivery selected by the user, independent from acquiring authority.
/// 用户明确选择的交付方式，独立于授权获取。
#[derive(Debug, Clone, Copy)]
pub enum HtmlAnswerDelivery {
    Open,
    Copy,
}

/// Return a synthetic root and canonical CRLF/Unicode source with Core-derived descriptors.
/// 返回合成根会话及带 Core 派生描述的 CRLF/Unicode 权威源码。
#[cfg(test)]
pub(crate) fn fixture() -> (
    String,
    vulcan_session_core::v1::ConversationEvent,
    HtmlAnswerEventProjection,
) {
    use vulcan_session_core::v1::{
        ApiCallId, AssistantSemanticStatus, AssistantTextEvent, ConversationEvent, EventBody,
        EventVisibility, Seq,
    };
    let id = uuid::Uuid::new_v4().to_string();
    let event = ConversationEvent { seq: Seq::try_new(7).unwrap(), turn_id: None, created_at_ms: 1, updated_at_ms: 2,
        event_revision: 4, visibility: EventVisibility::Provider, event: EventBody::AssistantText(AssistantTextEvent {
            api_call_id: ApiCallId::try_new(1).unwrap(), provider_source: None, status: AssistantSemanticStatus::Completed,
            text: "中文 🌏\r\n\r\n```vulcan-html {\"version\":1,\"title\":\"Diagram\"}\r\n<!doctype html><button>synthetic-code-marker</button>\r\n```\r\n\r\nafter".into(),
        }) };
    let projection = HtmlAnswerEventProjection::from_event(&id, &event, true).unwrap();
    (id, event, projection)
}
