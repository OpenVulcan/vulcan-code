//! Agent transcript document primitives and parsing helpers for Vulkan Code surfaces.
//! Vulcan Code 输出界面使用的代理转录文档基础类型与解析辅助逻辑。

pub mod document;
pub mod editor_core;
pub mod iced;
#[path = "surface/iced_style.rs"]
pub mod iced_style;
#[path = "surface/iced_surface.rs"]
pub mod iced_surface;
#[path = "surface/iced_theme.rs"]
pub mod iced_theme;
pub mod parser;
pub mod selectable_document;
mod video_thumbnail;

pub use document::{
    AgentViewAttributes, AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind,
    AgentViewDocument, AgentViewMarkdownBlock, AgentViewToolPresentationKind,
    markdown_source_to_plain_text, render_directive_source,
};
pub use editor_core::{Cursor, Selection, TextBuffer, ViewState};
pub use iced::{
    clear_selectable_transcript_render_cache_stash,
    SelectableTranscriptCommand, SelectableTranscriptContextMenuMediaKind,
    SelectableTranscriptContextMenuMediaTarget, SelectableTranscriptContextMenuRequest,
    SelectableTranscriptDragAutoScroll, SelectableTranscriptDragAutoScrollDirection,
    SelectableTranscriptInteraction, SelectableTranscriptSurfaceId,
    view_selectable_rendered_transcript,
    view_selectable_rendered_transcript_owned_with_context_menu,
    view_selectable_rendered_transcript_shared_with_context_menu,
    view_selectable_rendered_transcript_with_context_menu,
};
pub use iced_style::{thinking_modal_backdrop_style, thinking_modal_card_style};
pub use iced_surface::{
    THINKING_MODAL_ANIMATION_DURATION, THINKING_MODAL_TICK_INTERVAL, ThinkingModalPhase,
    build_modal_thinking_surface_document, build_modal_tool_surface_document,
    build_overlay_thinking_surface_document, build_thinking_title, estimate_thinking_token_count,
    format_thinking_duration_label, thinking_modal_progress, thinking_overlay_scrollable_id,
    view_thinking_modal_surface, view_thinking_modal_surface_with_context_menu,
    view_thinking_overlay_surface, view_thinking_overlay_surface_with_context_menu,
};
pub use parser::parse_agent_view_document;
pub use selectable_document::{
    AgentViewSelectableBlock, AgentViewSelectableBlockKind, AgentViewSelectableDocument,
};
pub use video_thumbnail::{VideoThumbnailPreview, resolve_video_thumbnail_preview};
