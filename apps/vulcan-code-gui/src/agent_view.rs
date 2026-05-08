//! GUI-side adapters that map session transcripts into unified vulcan-agent-view documents.
//! 将会话转录映射为统一 vulcan-agent-view 文档的图形界面适配器。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::model::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
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
    session_id: usize,
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

/// Returns the short Chinese label used by the GUI block header for one unified block.
/// 返回图形界面为统一区块头部使用的中文短标签。
pub fn agent_view_block_label(block: &AgentViewBlock) -> &'static str {
    match block {
        AgentViewBlock::Markdown(_) => "Markdown",
        AgentViewBlock::Directive(directive) => match directive.kind {
            AgentViewDirectiveKind::UserPrompt => "用户指令",
            AgentViewDirectiveKind::Thinking => "思考",
            AgentViewDirectiveKind::ToolCall => "工具调用",
            AgentViewDirectiveKind::ToolResult => "工具结果",
            AgentViewDirectiveKind::Diff => "文件差异",
            AgentViewDirectiveKind::Notice => "提示",
            AgentViewDirectiveKind::Custom(_) => "扩展块",
        },
    }
}

/// Returns the readable GUI header title for one unified block.
/// 返回统一区块在图形界面头部展示时使用的可读标题。
pub fn agent_view_block_title(block: &AgentViewBlock) -> String {
    match block {
        AgentViewBlock::Markdown(markdown) => markdown
            .plain_text()
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_string())
            .unwrap_or_else(|| String::from("Markdown 输出")),
        AgentViewBlock::Directive(directive) => directive
            .attributes
            .get("title")
            .map(|title| title.trim())
            .filter(|title| !title.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| agent_view_block_label(block).to_string()),
    }
}

/// Returns the Markdown-compatible body source that should feed the GUI renderer for one unified block.
/// 返回某个统一区块供图形界面渲染器消费的 Markdown 兼容正文源码。
pub fn agent_view_block_render_source(block: &AgentViewBlock) -> &str {
    match block {
        AgentViewBlock::Markdown(markdown) => &markdown.raw_source,
        AgentViewBlock::Directive(directive) => &directive.body,
    }
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
    let title = runtime_event.title.trim();

    !matches!(title, "执行上下文" | "模型输出完成" | "MCP 工具发现")
        && !title.starts_with("MCP 工具发现 · ")
        && !title.starts_with("MCP 工具发现失败 · ")
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

    use super::{agent_view_document_raw_source, build_agent_view_document};
    use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
    use crate::model::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
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
                created_at: String::from("1"),
            },
            SessionMessageEntry {
                id: String::from("thinking-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ThinkingOutput,
                visible: true,
                title: String::from("思考"),
                body: String::from("先统一结构，再处理选择。"),
                created_at: String::from("2"),
            },
            SessionMessageEntry {
                id: String::from("assistant-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::AssistantOutput,
                visible: true,
                title: String::from("输出"),
                body: String::from("## 结果\n\n支持 **Markdown**。"),
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

    /// Verifies that runtime tool events become directive blocks and the raw-source projection keeps their fences.
    /// 验证运行时工具事件会转换为指令区块，且源码投影会保留它们的围栏表示。
    #[test]
    fn builds_runtime_tool_blocks_and_raw_projection() {
        let runtime_events = vec![RuntimeEvent::new(
            RuntimeEventKind::ToolStarted,
            "工具调用 · cargo check",
            "cargo check -p vulcan-code-gui",
        )];
        let document = build_agent_view_document(&[], &runtime_events);
        let raw_source = agent_view_document_raw_source(&document);

        assert_eq!(document.blocks.len(), 1);
        assert!(matches!(
            &document.blocks[0],
            AgentViewBlock::Directive(block)
                if block.kind == AgentViewDirectiveKind::ToolCall
        ));
        assert!(raw_source.contains(":::tool-call"));
        assert!(raw_source.contains("cargo check -p vulcan-code-gui"));
    }

    /// Verifies that runtime-only diagnostic discovery and completion events stay out of the unified transcript document.
    /// 验证仅面向运行诊断的发现类与完成类事件不会进入统一转录文档。
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
        ];
        let document = build_agent_view_document(&[], &runtime_events);

        assert_eq!(document.blocks.len(), 1);
        assert!(matches!(
            &document.blocks[0],
            AgentViewBlock::Directive(block)
                if block.kind == AgentViewDirectiveKind::ToolCall
        ));
    }
}
