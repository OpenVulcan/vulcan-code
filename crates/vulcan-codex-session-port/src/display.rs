use crate::contract::{
    CodexEventVisibility, CodexSessionEvent, CodexSessionEventKind, CodexThreadId, CodexTurnId,
    CodexVisibleRole,
};
use serde::{Deserialize, Serialize};

/// One user-facing display item projected from Codex-compatible events.
/// 从 Codex 兼容事件投影出的单条用户可见展示项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexDisplayItem {
    /// Stable display item identifier.
    /// 稳定展示项标识符。
    pub id: String,
    /// Owning thread identifier.
    /// 所属线程标识符。
    pub thread_id: CodexThreadId,
    /// Optional turn identifier.
    /// 可选轮次标识符。
    pub turn_id: Option<CodexTurnId>,
    /// User-facing role.
    /// 面向用户的角色。
    pub role: CodexVisibleRole,
    /// Display text.
    /// 展示文本。
    pub text: String,
    /// Whether this item is terminal for its lifecycle.
    /// 该条目是否已到达生命周期终态。
    pub completed: bool,
}

/// Incremental display projection state.
/// 增量展示投影状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexDisplayProjection {
    /// Projected display items in stable order.
    /// 按稳定顺序排列的展示项。
    pub items: Vec<CodexDisplayItem>,
}

impl CodexDisplayProjection {
    /// Applies one Codex-compatible event to the projection.
    /// 将一条 Codex 兼容事件应用到展示投影。
    pub fn apply_event(&mut self, event: &CodexSessionEvent) {
        if let Some(item) = project_codex_display_event(event) {
            self.upsert_item(item);
        }
    }

    /// Inserts or replaces one display item by identifier.
    /// 按标识符插入或替换一条展示项。
    pub fn upsert_item(&mut self, item: CodexDisplayItem) {
        if let Some(existing) = self
            .items
            .iter_mut()
            .find(|candidate| candidate.id == item.id)
        {
            *existing = item;
        } else {
            self.items.push(item);
        }
    }
}

/// Projects one Codex-compatible event into a user-facing item when it is visible.
/// 当事件可见时，将单条 Codex 兼容事件投影为用户展示项。
pub fn project_codex_display_event(event: &CodexSessionEvent) -> Option<CodexDisplayItem> {
    if event.visibility != CodexEventVisibility::Visible {
        return None;
    }
    let text = event
        .payload
        .get("text")
        .and_then(|value| value.as_str())
        .or_else(|| {
            event
                .payload
                .get("message")
                .and_then(|value| value.as_str())
        })
        .unwrap_or_default()
        .to_string();
    if text.trim().is_empty() {
        return None;
    }
    let role = match event.kind {
        CodexSessionEventKind::UserInput => CodexVisibleRole::User,
        CodexSessionEventKind::ItemStarted | CodexSessionEventKind::ItemCompleted => {
            CodexVisibleRole::Tool
        }
        CodexSessionEventKind::AgentMessage
        | CodexSessionEventKind::AgentCompleted
        | CodexSessionEventKind::TurnCompleted => CodexVisibleRole::Assistant,
        _ => CodexVisibleRole::Notice,
    };
    Some(CodexDisplayItem {
        id: event
            .item_id
            .clone()
            .unwrap_or_else(|| event.event_id.clone()),
        thread_id: event.thread_id,
        turn_id: event.turn_id.clone(),
        role,
        text,
        completed: matches!(
            event.kind,
            CodexSessionEventKind::ItemCompleted
                | CodexSessionEventKind::AgentCompleted
                | CodexSessionEventKind::TurnCompleted
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CodexEventScope, CodexEventVisibility};
    use serde_json::json;

    /// Verifies hidden events never enter the user-facing display projection.
    /// 验证隐藏事件不会进入用户可见展示投影。
    #[test]
    fn hidden_events_do_not_project_to_display_items() {
        let event = CodexSessionEvent::new(
            "event-1",
            CodexThreadId::new_v4(),
            1,
            CodexSessionEventKind::AgentMessage,
            CodexEventScope::Agent,
            CodexEventVisibility::Hidden,
            1000,
        )
        .with_payload(json!({ "text": "hidden result" }));

        assert_eq!(project_codex_display_event(&event), None);
    }

    /// Verifies visible user input projects into a stable display item.
    /// 验证可见用户输入会投影为稳定展示项。
    #[test]
    fn visible_user_input_projects_to_display_item() {
        let thread_id = CodexThreadId::new_v4();
        let event = CodexSessionEvent::new(
            "event-1",
            thread_id,
            1,
            CodexSessionEventKind::UserInput,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
            1000,
        )
        .with_turn_id("turn-1")
        .with_payload(json!({ "text": "hello" }));

        let item = project_codex_display_event(&event).expect("visible event should project");

        assert_eq!(item.thread_id, thread_id);
        assert_eq!(item.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(item.role, CodexVisibleRole::User);
        assert_eq!(item.text, "hello");
        assert!(!item.completed);
    }
}
