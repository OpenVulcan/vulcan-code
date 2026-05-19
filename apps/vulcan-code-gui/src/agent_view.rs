//! GUI-side adapters that map session transcripts into unified vulcan-agent-view documents.
//! 将会话转录映射为统一 vulcan-agent-view 文档的图形界面适配器。

mod tool_generated_audio;
mod tool_generated_document;
mod tool_generated_image;
mod tool_generated_video;
mod tool_generic;
mod tool_shared;
mod tool_task_session;

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::model::{SessionId, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use vulcan_markdown::transcript::split_streaming_markdown_source;
use vulcan_agent_view::{
    AgentViewAttributes, AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind,
    AgentViewDocument, AgentViewMarkdownBlock, AgentViewToolPresentationKind,
    parse_agent_view_document, render_directive_source,
};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

/// Stable attribute key used to tag Markdown blocks produced from sealed streaming segments.
/// 用于标记由已封口流式片段生成 Markdown 区块的稳定属性键。
const STREAMING_SEGMENT_ATTRIBUTE_KEY: &str = "streaming-segment";

/// Stable attribute value written onto sealed immutable streaming Markdown segments.
/// 写入已封口不可变流式 Markdown 片段的稳定属性值。
const STREAMING_SEGMENT_SEALED_VALUE: &str = "sealed";

/// Stable attribute value written onto the currently active streaming Markdown tail segment.
/// 写入当前活动流式 Markdown 尾片段的稳定属性值。
const STREAMING_SEGMENT_ACTIVE_TAIL_VALUE: &str = "active-tail";

/// Build options that allow the GUI to steer transcript-specific projection behaviors without leaking app state into the renderer.
/// 允许图形界面在不泄露应用状态给渲染器的前提下控制转录专用投影行为的构建选项。
#[derive(Debug, Clone, Default)]
pub struct AgentViewBuildOptions {
    /// Visible assistant-output message id that is still actively receiving streaming deltas and should therefore keep only its last Markdown block mutable.
    /// 当前仍在接收流式增量、因此只应让其最后一个 Markdown 区块保持可变的可见助手输出消息标识。
    pub active_streaming_output_message_id: Option<String>,
}

/// Builds one unified agent-view document from the visible session transcript and runtime events.
/// 根据可见会话转录与运行时事件构建一份统一 agent-view 文档。
pub fn build_agent_view_document(
    messages: &[SessionMessageEntry],
    runtime_events: &[RuntimeEvent],
) -> AgentViewDocument {
    build_agent_view_document_with_options(messages, runtime_events, &AgentViewBuildOptions::default())
}

/// Builds one unified agent-view document from the visible session transcript and runtime events using explicit GUI-side projection options.
/// 使用显式图形界面投影选项，根据可见会话转录与运行时事件构建统一 agent-view 文档。
pub fn build_agent_view_document_with_options(
    messages: &[SessionMessageEntry],
    runtime_events: &[RuntimeEvent],
    options: &AgentViewBuildOptions,
) -> AgentViewDocument {
    let mut document = AgentViewDocument::new();

    for message in messages
        .iter()
        .filter(|message| message.is_visible_to_user())
    {
        append_session_message_blocks(&mut document, message, options);
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
    if let AgentViewBlock::Directive(directive) = block
        && matches!(
            directive.kind,
            AgentViewDirectiveKind::ToolCall | AgentViewDirectiveKind::ToolResult
        )
        && let Some(message_id) = directive
            .attributes
            .get("message-id")
            .filter(|value| !value.trim().is_empty())
    {
        return format!("session-{session_id}-agent-view-tool-message-{message_id}");
    }

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
fn append_session_message_blocks(
    document: &mut AgentViewDocument,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) {
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
            append_assistant_output_blocks(document, message, options);
        }
        _ => {
            append_parsed_markdown_document(document, &message.body, Some(&message.id));
        }
    }
}

/// Appends assistant-output blocks while optionally splitting one actively streaming Markdown reply into sealed immutable segments plus one active tail segment.
/// 追加助手输出区块，并在需要时将一条活跃流式 Markdown 回复拆分成已封口不可变片段和一个活动尾片段。
fn append_assistant_output_blocks(
    document: &mut AgentViewDocument,
    message: &SessionMessageEntry,
    options: &AgentViewBuildOptions,
) {
    let is_active_streaming_output = options
        .active_streaming_output_message_id
        .as_deref()
        .is_some_and(|active_message_id| active_message_id == message.id);

    if is_active_streaming_output {
        append_streaming_markdown_document(document, &message.body, Some(&message.id));
    } else {
        append_parsed_markdown_document(document, &message.body, Some(&message.id));
    }
}

/// Appends the parsed Markdown/extension block sequence from one source string into the target document.
/// 将某段源码中解析得到的 Markdown 或扩展区块序列追加到目标文档中。
fn append_parsed_markdown_document(
    document: &mut AgentViewDocument,
    source: &str,
    message_id: Option<&str>,
) {
    let mut parsed_document = parse_agent_view_document(source);

    if let Some(message_id) = message_id.filter(|value| !value.trim().is_empty()) {
        for block in &mut parsed_document.blocks {
            block
                .attributes_mut()
                .entry(String::from("message-id"))
                .or_insert_with(|| message_id.to_string());
        }
    }

    document.blocks.extend(parsed_document.blocks);
}

/// Appends one actively streaming Markdown reply as sealed immutable segments plus one active tail segment so old blocks stop changing during token streaming.
/// 将一条活跃流式 Markdown 回复追加为已封口不可变片段和一个活动尾片段，从而让旧区块在 token 流式过程中停止变化。
fn append_streaming_markdown_document(
    document: &mut AgentViewDocument,
    source: &str,
    message_id: Option<&str>,
) {
    let partition = split_streaming_markdown_source(source);

    if partition.sealed_segments.is_empty() && partition.active_tail.is_none() {
        return;
    }

    for sealed_segment in partition.sealed_segments {
        push_streaming_markdown_segment(
            document,
            sealed_segment,
            message_id,
            STREAMING_SEGMENT_SEALED_VALUE,
        );
    }

    if let Some(active_tail) = partition.active_tail.filter(|segment| !segment.trim().is_empty()) {
        push_streaming_markdown_segment(
            document,
            active_tail,
            message_id,
            STREAMING_SEGMENT_ACTIVE_TAIL_VALUE,
        );
    }
}

/// Pushes one streaming Markdown segment into the document while attaching stable message ownership and segment-kind metadata.
/// 将单个流式 Markdown 片段推入文档，同时附着稳定的消息归属与片段类型元数据。
fn push_streaming_markdown_segment(
    document: &mut AgentViewDocument,
    raw_source: String,
    message_id: Option<&str>,
    segment_kind: &str,
) {
    let mut attributes = AgentViewAttributes::new();
    attributes.insert(
        STREAMING_SEGMENT_ATTRIBUTE_KEY.to_string(),
        segment_kind.to_string(),
    );
    if let Some(message_id) = message_id.filter(|value| !value.trim().is_empty()) {
        attributes.insert(String::from("message-id"), message_id.to_string());
    }

    document
        .blocks
        .push(AgentViewBlock::Markdown(AgentViewMarkdownBlock::new_with_attributes(
            raw_source,
            attributes,
        )));
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
    if let Some(metadata) = message.metadata.as_ref() {
        attach_user_prompt_image_attributes(&mut attributes, metadata);
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

/// Attaches user-image descriptors to one user-prompt block so the transcript renderer can show compact image tiles above the prompt bubble.
/// 向一条用户提示词区块附着用户图片描述符，使转录渲染器可以在提示词气泡上方显示紧凑图片方格。
fn attach_user_prompt_image_attributes(attributes: &mut AgentViewAttributes, metadata: &Value) {
    let Some(attachments) = metadata
        .get("transcript_attachments")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ConversationAttachment>>(value).ok())
    else {
        return;
    };

    let image_attachments = attachments
        .into_iter()
        .filter(|attachment| attachment.kind == ConversationAttachmentKind::Image)
        .filter(|attachment| {
            !attachment.file_name.trim().is_empty() && !attachment.local_path.trim().is_empty()
        })
        .collect::<Vec<_>>();
    if image_attachments.is_empty() {
        return;
    }

    attributes.insert(
        String::from("user-image-count"),
        image_attachments.len().to_string(),
    );
    for (index, attachment) in image_attachments.into_iter().enumerate() {
        attributes.insert(
            format!("user-image-{index}-file-name"),
            attachment.file_name,
        );
        attributes.insert(
            format!("user-image-{index}-local-path"),
            attachment.local_path,
        );
    }
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
    match tool_shared::resolve_tool_presentation_kind(message) {
        AgentViewToolPresentationKind::Generic => {
            tool_generic::build_generic_tool_block(kind, message)
        }
        AgentViewToolPresentationKind::TaskSession => {
            tool_task_session::build_task_session_tool_block(kind, message)
        }
        AgentViewToolPresentationKind::GeneratedImage => {
            tool_generated_image::build_generated_image_tool_block(kind, message)
        }
        AgentViewToolPresentationKind::GeneratedVideo => {
            tool_generated_video::build_generated_video_tool_block(kind, message)
        }
        AgentViewToolPresentationKind::GeneratedAudio => {
            tool_generated_audio::build_generated_audio_tool_block(kind, message)
        }
        AgentViewToolPresentationKind::GeneratedDocument => {
            tool_generated_document::build_generated_document_tool_block(kind, message)
        }
    }
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
    AgentViewToolPresentationKind::Generic.write_to_attributes(&mut attributes);
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

    use super::{build_agent_view_document, build_agent_view_document_with_options, AgentViewBuildOptions};
    use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
    use crate::model::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
    use serde_json::json;
    use vulcan_agent_view::{
        AgentViewBlock, AgentViewDirectiveKind, AgentViewToolPresentationKind,
    };

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
        assert!(matches!(
            &document.blocks[2],
            AgentViewBlock::Markdown(block)
                if block
                    .attributes
                    .get("message-id")
                    .is_some_and(|value| value == "assistant-1")
        ));
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
                    && block.body.is_empty()
                    && !block.raw_source.contains("成功读取文件")
                    && block
                        .attributes
                        .get("tool-display-state")
                        .is_some_and(|value| value == "completed")
        ));
    }

    /// Verifies user transcript rows expose compact image-strip attributes so downgraded image-understanding inputs remain visible after projection.
    /// 验证用户转录行会暴露紧凑图片条属性，使降级为图片理解输入的图片在投影后仍然可见。
    #[test]
    fn user_prompt_block_exposes_image_strip_attributes_from_metadata() {
        let messages = vec![SessionMessageEntry {
            id: String::from("user-attachment-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: String::from("请先看图再回答。"),
            metadata: Some(json!({
                "canonical_user_text": "请先看图再回答。",
                "transcript_attachments": [
                    {
                        "kind": "image",
                        "file_name": "diagram.png",
                        "local_path": "D:/diagram.png",
                        "mime_type": "image/png",
                    },
                    {
                        "kind": "audio",
                        "file_name": "demo.mp3",
                        "local_path": "D:/demo.mp3",
                        "mime_type": "audio/mpeg",
                    }
                ],
            })),
            created_at: String::from("2"),
        }];
        let document = build_agent_view_document(&messages, &[]);

        assert!(matches!(
            &document.blocks[0],
            AgentViewBlock::Directive(block)
                if block.kind == AgentViewDirectiveKind::UserPrompt
                    && block
                        .attributes
                        .get("user-image-count")
                        .is_some_and(|value| value == "1")
                    && block
                        .attributes
                        .get("user-image-0-file-name")
                        .is_some_and(|value| value == "diagram.png")
                    && block
                        .attributes
                        .get("user-image-0-local-path")
                        .is_some_and(|value| value == "D:/diagram.png")
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
                        .get(AgentViewToolPresentationKind::ATTRIBUTE_KEY)
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

    /// Verifies generated-media tool cards can be built from flattened tool-result metadata emitted by the runtime stream executor.
    /// 验证可以基于流式运行时执行器发出的扁平化工具结果元数据构建生成媒体工具卡片。
    #[test]
    fn generated_media_tool_card_exposes_visual_attributes_from_flat_metadata() {
        let messages = vec![SessionMessageEntry {
            id: String::from("tool-result-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("已生成图片"),
            body: String::from("## 工具返回\n\n成功生成图片。"),
            metadata: Some(json!({
                "tool_call_id": "call-media-1",
                "tool_display_state": "completed",
                "kind": "generated_attachments",
                "payload": {
                    "outputs": [{
                        "kind": "image",
                        "file_name": "demo.png",
                        "local_path": "D:/demo.png",
                        "mime_type": "image/png",
                    }],
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
                        .get(AgentViewToolPresentationKind::ATTRIBUTE_KEY)
                        .is_some_and(|value| value == "generated-image")
                    && block
                        .attributes
                        .get("generated-media-local-path")
                        .is_some_and(|value| value == "D:/demo.png")
        ));
    }

    /// Verifies one actively streaming assistant reply is split into sealed immutable Markdown segments plus one active tail segment.
    /// 验证单条活跃流式助手回复会被拆分为已封口不可变 Markdown 片段和一个活动尾片段。
    #[test]
    fn active_streaming_output_is_partitioned_into_sealed_segments_and_one_tail() {
        let messages = vec![SessionMessageEntry {
            id: String::from("assistant-stream-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::AssistantOutput,
            visible: true,
            title: String::from("输出"),
            body: String::from("# 标题\n\n第一段\n\n第二段"),
            metadata: None,
            created_at: String::from("3"),
        }];
        let options = AgentViewBuildOptions {
            active_streaming_output_message_id: Some(String::from("assistant-stream-1")),
        };
        let document = build_agent_view_document_with_options(&messages, &[], &options);

        assert_eq!(document.blocks.len(), 3);
        assert!(matches!(
            &document.blocks[0],
            AgentViewBlock::Markdown(block)
                if block.raw_source == "# 标题\n"
                    && block
                        .attributes
                        .get("streaming-segment")
                        .is_some_and(|value| value == "sealed")
        ));
        assert!(matches!(
            &document.blocks[1],
            AgentViewBlock::Markdown(block)
                if block.raw_source == "第一段\n"
                    && block
                        .attributes
                        .get("streaming-segment")
                        .is_some_and(|value| value == "sealed")
        ));
        assert!(matches!(
            &document.blocks[2],
            AgentViewBlock::Markdown(block)
                if block.raw_source == "第二段"
                    && block
                        .attributes
                        .get("streaming-segment")
                        .is_some_and(|value| value == "active-tail")
        ));
    }
}
