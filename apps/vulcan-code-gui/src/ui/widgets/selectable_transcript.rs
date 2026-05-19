//! Compatibility re-export for the agent-view transcript renderer.
//! agent-view 转录渲染器的兼容性转发导出。

pub use vulcan_agent_view::{
    SelectableTranscriptCommand, SelectableTranscriptContextMenuMediaKind,
    SelectableTranscriptContextMenuMediaTarget, SelectableTranscriptContextMenuRequest,
    SelectableTranscriptDragAutoScroll, SelectableTranscriptDragAutoScrollDirection,
    SelectableTranscriptInteraction, SelectableTranscriptSurfaceId,
    view_selectable_rendered_transcript_owned_with_context_menu,
    view_selectable_rendered_transcript_shared_with_context_menu,
    view_selectable_rendered_transcript_with_context_menu,
};
