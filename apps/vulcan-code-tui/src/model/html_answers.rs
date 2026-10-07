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

/// Substitute exact Core fence intervals with short inert summaries; ordinary text stays borrowed.
/// 将精确 Core 围栏区间替换为简短不可执行摘要；普通正文保持借用。
/// Invalid byte intervals fail closed to the original display source without minting authority.
/// 无效字节区间仅保留原始展示源码，绝不签发权威。
pub fn compact_source<'a>(
    text: &'a str,
    projection: Option<&HtmlAnswerEventProjection>,
) -> std::borrow::Cow<'a, str> {
    let Some(projection) = projection.filter(|value| !value.answers.is_empty()) else {
        return text.into();
    };
    let mut compact = String::new();
    let mut cursor = 0;
    for answer in &projection.answers {
        let Some(before) = text.get(cursor..answer.fence.byte_start) else {
            return text.into();
        };
        if text
            .get(answer.fence.byte_start..answer.fence.byte_end)
            .is_none()
        {
            return text.into();
        }
        compact.push_str(before);
        compact.push_str(&crate::tr!("htmlAnswers.label"));
        compact.push_str(" · ");
        compact.push_str(answer.title.as_deref().unwrap_or("HTML"));
        compact.push_str(" · i / /html-results\n");
        cursor = answer.fence.byte_end;
    }
    compact.push_str(&text[cursor..]);
    compact.into()
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
