//! GUI-side adapters that map session transcripts into unified vulcan-agent-view documents.
//! 将会话转录映射为统一 vulcan-agent-view 文档的图形界面适配器。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::model::{SessionId, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use vulcan_agent_view::{
    AgentViewAttributes, AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind,
    AgentViewDocument, parse_agent_view_document, render_directive_source,
};

/// Builds one unified agent-view document from the visible session transcript and runtime events.
/// 根据可见会话转录与运行时事件构建一份统一 agent-view 文档。
pub fn build_agent_view_document(
    messages: &[SessionMessageEntry],
    runtime_events: &[RuntimeEvent],
) -> AgentViewDocument {
    let mut document = AgentViewDocument::new();

    for message in messages
        .iter()
        .filter(|message| message.is_visible_to_user())
    {
        append_session_message_blocks(&mut document, message);
    }

    for runtime_event in runtime_events
        .iter()
        .filter(|runtime_event| should_render_runtime_event_in_agent_view(runtime_event))
    {
        append_runtime_event_block(&mut document, runtime_event);
    }

    document
}

/// Builds the copy-original raw-source projection of one unified agent-view document.
/// 构建一份统一 agent-view 文档面向复制原文场景的源码投影。
pub fn agent_view_document_raw_source(document: &AgentViewDocument) -> String {
    document
        .blocks
        .iter()
        .map(|block| block.raw_source().trim_end().to_string())
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Builds one stable GUI-side block identifier for a unified agent-view document block.
/// 为统一 agent-view 文档区块构建一个稳定的图形界面侧标识。
pub fn agent_view_block_id(
    session_id: SessionId,
    block_index: usize,
    block: &AgentViewBlock,
) -> String {
    let mut hasher = DefaultHasher::new();
    block.raw_source().hash(&mut hasher);

    format!(
        "session-{session_id}-agent-view-block-{block_index}-{:016x}",
        hasher.finish()
    )
}

/// Returns whether the GUI should treat the block as collapsed before the user explicitly expands it.
/// 返回图形界面是否应在用户显式展开前将该区块视为折叠状态。
pub fn agent_view_block_collapsed_by_default(block: &AgentViewBlock) -> bool {
    match block {
        AgentViewBlock::Markdown(_) => false,
        AgentViewBlock::Directive(directive) => directive.collapsed_by_default,
    }
}

/// Appends the block sequence contributed by one visible session message into the target document.
/// 将单条可见会话消息贡献出的区块序列追加到目标文档中。
fn append_session_message_blocks(document: &mut AgentViewDocument, message: &SessionMessageEntry) {
    match (message.kind, message.record_kind) {
        (SessionMessageKind::User, _) => {
            document.push_directive(build_user_prompt_block(message));
        }
        (_, SessionMessageRecordKind::ThinkingOutput) => {
            document.push_directive(build_thinking_block(message));
        }
        (_, SessionMessageRecordKind::ToolCallOutput) => {
            document.push_directive(build_tool_record_block(
                AgentViewDirectiveKind::ToolCall,
                message,
            ));
        }
        (_, SessionMessageRecordKind::ToolResultOutput) => {
            let tool_kind = if message
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("tool_display_state"))
                .and_then(|value| value.as_str())
                .is_some()
            {
                AgentViewDirectiveKind::ToolCall
            } else {
                AgentViewDirectiveKind::ToolResult
            };
            document.push_directive(build_tool_record_block(tool_kind, message));
        }
        (SessionMessageKind::Error, _) => {
            document.push_directive(build_error_notice_block(message));
        }
        (_, SessionMessageRecordKind::AssistantOutput | SessionMessageRecordKind::Transcript) => {
            append_parsed_markdown_document(document, &message.body);
        }
        _ => {
            append_parsed_markdown_document(document, &message.body);
        }
    }
}

/// Appends the parsed Markdown/extension block sequence from one source string into the target document.
/// 将某段源码中解析得到的 Markdown 或扩展区块序列追加到目标文档中。
fn append_parsed_markdown_document(document: &mut AgentViewDocument, source: &str) {
    let parsed_document = parse_agent_view_document(source);
    document.blocks.extend(parsed_document.blocks);
}

/// Builds one user-prompt directive block from the visible session message.
/// 根据可见会话消息构建一条用户提示词指令区块。
fn build_user_prompt_block(message: &SessionMessageEntry) -> AgentViewDirectiveBlock {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(String::from("message-id"), message.id.clone());
    if !message.created_at.trim().is_empty() {
        attributes.insert(String::from("created-at"), message.created_at.clone());
    }
    if !message.title.trim().is_empty() {
        attributes.insert(String::from("title"), message.title.clone());
    }
    let raw_source = render_directive_source(
        &AgentViewDirectiveKind::UserPrompt,
        &attributes,
        &message.body,
    );

    AgentViewDirectiveBlock::new(
        AgentViewDirectiveKind::UserPrompt,
        attributes,
        message.body.clone(),
        raw_source,
        false,
    )
}

/// Builds one thinking directive block from the visible session message.
/// 根据可见会话消息构建一条思考指令区块。
fn build_thinking_block(message: &SessionMessageEntry) -> AgentViewDirectiveBlock {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(String::from("message-id"), message.id.clone());
    attributes.insert(String::from("collapsed"), String::from("true"));
    if !message.created_at.trim().is_empty() {
        attributes.insert(String::from("created-at"), message.created_at.clone());
    }
    if !message.title.trim().is_empty() {
        attributes.insert(String::from("title"), message.title.clone());
    }
    let raw_source = render_directive_source(
        &AgentViewDirectiveKind::Thinking,
        &attributes,
        &message.body,
    );

    AgentViewDirectiveBlock::new(
        AgentViewDirectiveKind::Thinking,
        attributes,
        message.body.clone(),
        raw_source,
        true,
    )
}

/// Builds one tool-call or tool-result directive block from the visible session message.
/// 根据可见会话消息构建一条工具调用或工具返回指令区块。
fn build_tool_record_block(
    kind: AgentViewDirectiveKind,
    message: &SessionMessageEntry,
) -> AgentViewDirectiveBlock {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(String::from("message-id"), message.id.clone());
    if !message.created_at.trim().is_empty() {
        attributes.insert(String::from("created-at"), message.created_at.clone());
    }
    if !message.title.trim().is_empty() {
        attributes.insert(String::from("title"), message.title.clone());
    }
    if let Some(tool_display_state) = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_display_state"))
        .and_then(|value| value.as_str())
        .filter(|state| !state.trim().is_empty())
    {
        attributes.insert(
            String::from("tool-display-state"),
            tool_display_state.to_string(),
        );
    }
    if message.metadata.as_ref().is_some_and(|metadata| {
        metadata.get("tool_name").and_then(Value::as_str) == Some("task")
            || metadata.get("kind").and_then(Value::as_str) == Some("task_session")
    }) {
        attributes.insert(
            String::from("tool-visual-kind"),
            String::from("task-session"),
        );
    }
    if let Some(task_session_id) = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("payload"))
        .and_then(|payload| {
            payload
                .get("task_session_id")
                .or_else(|| payload.get("task_id"))
        })
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        attributes.insert(String::from("task-session-id"), task_session_id.to_string());
    }
    if let Some(task_execution_status) = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("payload"))
        .and_then(|payload| payload.get("execution_status"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        attributes.insert(
            String::from("task-session-execution-status"),
            task_execution_status.to_string(),
        );
    }
    let raw_source = render_directive_source(&kind, &attributes, &message.body);

    AgentViewDirectiveBlock::new(kind, attributes, message.body.clone(), raw_source, false)
}

/// Builds one error notice directive block from the visible session message.
/// 根据可见会话消息构建一条错误提示指令区块。
fn build_error_notice_block(message: &SessionMessageEntry) -> AgentViewDirectiveBlock {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(String::from("severity"), String::from("error"));
    attributes.insert(String::from("message-id"), message.id.clone());
    if !message.title.trim().is_empty() {
        attributes.insert(String::from("title"), message.title.clone());
    }
    let raw_source =
        render_directive_source(&AgentViewDirectiveKind::Notice, &attributes, &message.body);

    AgentViewDirectiveBlock::new(
        AgentViewDirectiveKind::Notice,
        attributes,
        message.body.clone(),
        raw_source,
        false,
    )
}

/// Appends the block contributed by one runtime event into the target document.
/// 将单条运行时事件贡献出的区块追加到目标文档中。
fn append_runtime_event_block(document: &mut AgentViewDocument, runtime_event: &RuntimeEvent) {
    match runtime_event.kind {
        RuntimeEventKind::ToolStarted => {
            document.push_directive(build_runtime_tool_block(
                AgentViewDirectiveKind::ToolCall,
                runtime_event,
                true,
            ));
        }
        RuntimeEventKind::ToolFinished => {
            document.push_directive(build_runtime_tool_block(
                AgentViewDirectiveKind::ToolResult,
                runtime_event,
                true,
            ));
        }
        RuntimeEventKind::Failed => {
            document.push_directive(build_runtime_notice_block(runtime_event, Some("error")));
        }
        RuntimeEventKind::DelegateStarted | RuntimeEventKind::DelegateFinished => {
            document.push_directive(build_runtime_notice_block(runtime_event, Some("delegate")));
        }
        RuntimeEventKind::TokenDelta => {
            document.push_directive(build_runtime_notice_block(runtime_event, Some("status")));
        }
    }
}

/// Returns whether one runtime event should surface inside the unified agent transcript instead of staying only in diagnostics.
/// 返回某条运行时事件是否应进入统一代理转录，而不是仅保留在诊断视图中。
fn should_render_runtime_event_in_agent_view(runtime_event: &RuntimeEvent) -> bool {
    let _ = runtime_event;

    false
}

/// Builds one runtime-owned tool-call or tool-result directive block from the runtime event.
/// 根据运行时事件构建一条运行时持有的工具调用或工具结果指令区块。
fn build_runtime_tool_block(
    kind: AgentViewDirectiveKind,
    runtime_event: &RuntimeEvent,
    collapsed_by_default: bool,
) -> AgentViewDirectiveBlock {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(String::from("title"), runtime_event.title.clone());
    attributes.insert(
        String::from("runtime-kind"),
        runtime_event_kind_label(runtime_event.kind).to_string(),
    );
    if collapsed_by_default {
        attributes.insert(String::from("collapsed"), String::from("true"));
    }
    let raw_source = render_directive_source(&kind, &attributes, &runtime_event.detail);

    AgentViewDirectiveBlock::new(
        kind,
        attributes,
        runtime_event.detail.clone(),
        raw_source,
        collapsed_by_default,
    )
}

/// Builds one runtime-owned notice directive block from the runtime event.
/// 根据运行时事件构建一条运行时持有的提示指令区块。
fn build_runtime_notice_block(
    runtime_event: &RuntimeEvent,
    severity: Option<&str>,
) -> AgentViewDirectiveBlock {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(String::from("title"), runtime_event.title.clone());
    attributes.insert(
        String::from("runtime-kind"),
        runtime_event_kind_label(runtime_event.kind).to_string(),
    );
    if let Some(severity) = severity {
        attributes.insert(String::from("severity"), severity.to_string());
    }
    let raw_source = render_directive_source(
        &AgentViewDirectiveKind::Notice,
        &attributes,
        &runtime_event.detail,
    );

    AgentViewDirectiveBlock::new(
        AgentViewDirectiveKind::Notice,
        attributes,
        runtime_event.detail.clone(),
        raw_source,
        false,
    )
}

/// Returns the stable short label used for runtime-event semantic mapping attributes.
/// 返回运行时事件语义映射属性使用的稳定短标签。
fn runtime_event_kind_label(kind: RuntimeEventKind) -> &'static str {
    match kind {
        RuntimeEventKind::TokenDelta => "token-delta",
        RuntimeEventKind::ToolStarted => "tool-started",
        RuntimeEventKind::ToolFinished => "tool-finished",
        RuntimeEventKind::DelegateStarted => "delegate-started",
        RuntimeEventKind::DelegateFinished => "delegate-finished",
        RuntimeEventKind::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    //! Targeted adapter tests that validate the GUI transcript-to-document mapping rules.
    //! 验证 GUI 转录到文档映射规则的定向适配器测试。

    use super::build_agent_view_document;
    use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
    use crate::model::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
    use serde_json::json;
    use vulcan_agent_view::{AgentViewBlock, AgentViewDirectiveKind};

    /// Verifies that user prompts and thinking messages become directive blocks while assistant output keeps Markdown parsing.
    /// 验证用户提示与思考消息会变成指令区块，而助手输出仍保持 Markdown 解析能力。
    #[test]
    fn builds_mixed_document_from_visible_messages() {
        let messages = vec![
            SessionMessageEntry {
                id: String::from("user-1"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户输入"),
                body: String::from("实现输出面板。"),
                metadata: None,
                created_at: String::from("1"),
            },
            SessionMessageEntry {
                id: String::from("thinking-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ThinkingOutput,
                visible: true,
                title: String::from("思考"),
                body: String::from("先统一结构，再处理选择。"),
                metadata: None,
                created_at: String::from("2"),
            },
            SessionMessageEntry {
                id: String::from("assistant-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("## 结果\n\n支持 **Markdown**。"),
                metadata: None,
                created_at: String::from("3"),
            },
        ];
        let document = build_agent_view_document(&messages, &[]);

        assert_eq!(document.blocks.len(), 3);
        assert!(matches!(
            &document.blocks[0],
            AgentViewBlock::Directive(block)
                if block.kind == AgentViewDirectiveKind::UserPrompt
        ));
        assert!(matches!(
            &document.blocks[1],
            AgentViewBlock::Directive(block)
                if block.kind == AgentViewDirectiveKind::Thinking
        ));
        assert!(matches!(&document.blocks[2], AgentViewBlock::Markdown(_)));
    }

    /// Verifies that runtime events no longer sink to the bottom of the unified transcript, even when tool-like payloads are passed directly into the adapter.
    /// 验证运行时事件即使直接传入适配器，也不会再作为沉底内容进入统一转录。
    #[test]
    fn filters_tool_runtime_events_from_unified_transcript() {
        let runtime_events = vec![RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · cargo check",
            "cargo check -p vulcan-code-gui",
        )];
        let document = build_agent_view_document(&[], &runtime_events);

        assert!(document.blocks.is_empty());
    }

    /// Verifies that all runtime-only diagnostics and delegate status events stay out of the unified transcript document so no bottom-only rows remain.
    /// 验证全部仅运行时可见的诊断与委派状态事件都会留在诊断层，从而不再产生沉底行。
    #[test]
    fn filters_runtime_diagnostic_scaffolding_events() {
        let runtime_events = vec![
            RuntimeEvent::new(RuntimeEventKind::ToolStarted, "执行上下文", "一些上下文"),
            RuntimeEvent::new(
                RuntimeEventKind::ToolFinished,
                "MCP 工具发现",
                "发现到若干工具",
            ),
            RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                "MCP 工具发现 · browser",
                "正在扫描 browser",
            ),
            RuntimeEvent::new(
                RuntimeEventKind::Failed,
                "MCP 工具发现失败 · browser",
                "扫描失败",
            ),
            RuntimeEvent::new(RuntimeEventKind::TokenDelta, "模型输出完成", "done"),
            RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                "工具调用 · cargo check",
                "cargo check -p vulcan-code-gui",
            ),
            RuntimeEvent::new(
                RuntimeEventKind::DelegateStarted,
                "委派开始 · lua-specialist",
                "开始委派给 lua-specialist",
            ),
            RuntimeEvent::new(
                RuntimeEventKind::DelegateFinished,
                "委派完成 · lua-specialist",
                "lua-specialist 已返回",
            ),
        ];
        let document = build_agent_view_document(&[], &runtime_events);

        assert!(document.blocks.is_empty());
    }

    /// Verifies that one completed tool display row still renders as a tool-call directive so the icon stays stable while the display state drives only styling.
    /// 验证一条已完成工具显示行仍会渲染为工具调用指令，从而保持图标稳定并仅由显示状态驱动样式变化。
    #[test]
    fn completed_tool_display_row_keeps_tool_call_directive_kind() {
        let messages = vec![SessionMessageEntry {
            id: String::from("tool-result-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 read_file 工具"),
            body: String::from("## 工具返回\n\n成功读取文件。"),
            metadata: Some(json!({
                "tool_call_id": "call-1",
                "tool_display_state": "completed",
            })),
            created_at: String::from("2"),
        }];
        let document = build_agent_view_document(&messages, &[]);

        assert!(matches!(
            &document.blocks[0],
            AgentViewBlock::Directive(block)
                if block.kind == AgentViewDirectiveKind::ToolCall
                    && block
                        .attributes
                        .get("tool-display-state")
                        .is_some_and(|value| value == "completed")
        ));
    }

    /// Verifies that one task-session tool card carries the dedicated visual and routing attributes needed by the GUI fallback behavior.
    /// 验证一张 task-session 工具卡会携带 GUI 回退行为所需的专属视觉与路由属性。
    #[test]
    fn task_session_tool_card_exposes_visual_and_task_attributes() {
        let messages = vec![SessionMessageEntry {
            id: String::from("task-result-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("🤖 调度 审查仓库（explorer） ✅ 完成"),
            body: String::from("## 工具返回\n\n子代理已完成。"),
            metadata: Some(json!({
                "tool_call_id": "call-task-1",
                "tool_name": "task",
                "tool_display_state": "completed",
                "kind": "task_session",
                "payload": {
                    "task_id": "00000000-0000-0000-0000-000000000001",
                    "execution_status": "completed",
                },
            })),
            created_at: String::from("2"),
        }];
        let document = build_agent_view_document(&messages, &[]);

        assert!(matches!(
            &document.blocks[0],
            AgentViewBlock::Directive(block)
                if block.kind == AgentViewDirectiveKind::ToolCall
                    && block
                        .attributes
                        .get("tool-visual-kind")
                        .is_some_and(|value| value == "task-session")
                    && block
                        .attributes
                        .get("task-session-id")
                        .is_some_and(|value| value == "00000000-0000-0000-0000-000000000001")
                    && block
                        .attributes
                        .get("task-session-execution-status")
                        .is_some_and(|value| value == "completed")
        ));
    }
}
