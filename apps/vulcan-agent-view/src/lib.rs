//! Agent transcript document primitives and parsing helpers for Vulkan Code surfaces.
//! Vulcan Code 输出界面使用的代理转录文档基础类型与解析辅助逻辑。

pub mod document;
pub mod editor_core;
pub mod parser;
pub mod selectable_document;

pub use document::{
    AgentViewAttributes, AgentViewBlock, AgentViewDirectiveBlock, AgentViewDirectiveKind,
    AgentViewDocument, AgentViewMarkdownBlock, markdown_source_to_plain_text,
    render_directive_source,
};
pub use editor_core::{Cursor, Selection, TextBuffer, ViewState};
pub use parser::parse_agent_view_document;
pub use selectable_document::{
    AgentViewSelectableBlock, AgentViewSelectableBlockKind, AgentViewSelectableDocument,
};
