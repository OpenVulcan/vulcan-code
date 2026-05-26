//! Session projection snapshot state extracted from the main workbench module.
//! 从主工作台模块中抽取的会话投影快照状态。

use crate::model::SessionId;

/// Session-scoped agent-view projection snapshot materialized off the UI thread before being applied back into the live workbench caches.
/// 在 UI 线程之外先行物化、随后再回写到工作台实时缓存中的按会话归属 agent-view 投影视图快照。
#[derive(Debug, Clone)]
pub(super) struct SessionAgentViewProjectionSnapshot {
    /// Stable session identifier owning the projected transcript payloads.
    /// 拥有当前投影转录载荷的稳定会话标识。
    pub(super) session_id: SessionId,
    /// Structured transcript document rendered by the unified agent-view surface.
    /// 由统一 agent-view 表面渲染的结构化转录文档。
    pub(super) document: vulcan_agent_view::AgentViewDocument,
    /// Selectable projection aligned with the structured document and reused by selection-aware transcript surfaces.
    /// 与结构化文档对齐、并供支持文本选择的转录表面复用的可选投影对象。
    pub(super) selectable_document: vulcan_agent_view::AgentViewSelectableDocument,
    /// Plain selectable transcript text stored into the read-only text-editor buffer cache.
    /// 保存到只读文本编辑缓冲区缓存中的可选转录纯文本。
    pub(super) selectable_text: String,
    /// Raw-source projection stored into the dedicated source-view buffer cache.
    /// 保存到专用源码视图缓冲区缓存中的原始源码投影文本。
    pub(super) raw_source_text: String,
}
