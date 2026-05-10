//! Agent transcript document primitives and parsing helpers for Vulkan Code surfaces.
//! Vulcan Code 输出界面使用的代理转录文档基础类型与解析辅助逻辑。

pub mod document;
pub mod editor_core;
pub mod iced;
pub mod iced_style;
pub mod iced_surface;
pub mod iced_theme;
pub mod parser;
pub mod selectable_document;

pub use document::{
    AgentViewAttributes, AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind,
    AgentViewDocument, AgentViewMarkdownBlock, markdown_source_to_plain_text,
    render_directive_source,
};
pub use editor_core::{Cursor, Selection, TextBuffer, ViewState};
pub use iced::{
    SelectableTranscriptDragAutoScroll, SelectableTranscriptDragAutoScrollDirection,
    SelectableTranscriptInteraction, view_selectable_rendered_transcript,
};
pub use iced_style::{thinking_modal_backdrop_style, thinking_modal_card_style};
pub use iced_surface::{
    THINKING_MODAL_ANIMATION_DURATION, THINKING_MODAL_TICK_INTERVAL, ThinkingModalPhase,
    build_modal_thinking_surface_document, build_modal_tool_surface_document,
    build_overlay_thinking_surface_document, build_thinking_title, estimate_thinking_token_count,
    format_thinking_duration_label, thinking_modal_progress, thinking_overlay_scrollable_id,
    view_thinking_modal_surface, view_thinking_overlay_surface,
};
pub use parser::parse_agent_view_document;
pub use selectable_document::{
    AgentViewSelectableBlock, AgentViewSelectableBlockKind, AgentViewSelectableDocument,
};
