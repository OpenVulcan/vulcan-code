//! Markdown long-term memory stored in the user directory.
//! 存放在用户目录中的 Markdown 长期记忆。
//!
//! Memory lives in three scopes (user, space, project), each one Markdown file whose H1–H4
//! headings are nodes. Reads never rewrite a file; writes validate revisions, keep untouched
//! bytes and replace each file atomically. Every boundary value of the feature is defined here.
//! 记忆分为用户、空间、项目三层，每层一个 Markdown 文件，H1–H4 标题即节点。读取绝不改写文件；
//! 写入校验修订、保留未触及的字节并原子替换文件。本功能的全部边界值都在此定义。

mod document;
mod error;
mod operations;
mod registry;
mod scope;
mod search;
mod settings;
mod snapshot;
mod store;
mod timestamp;

pub use document::{MemoryDocument, MemoryNode, NodeMeta, NodeSource, NodeView};
pub use error::MemoryError;
pub use operations::{
    DuplicateHint, NodePlacement, WriteOperation, WriteOperationReceipt, WriteReceipt,
};
pub use registry::{MemoryProject, MemoryRegistry, OrphanCandidate};
pub use scope::{MemoryScope, preset_sections};
pub use search::{
    OutlineRequest, SearchHit, SearchPage, SearchRequest, render_outline, render_search_page,
    search_nodes,
};
pub use settings::{InjectionBudget, MarkdownMemorySettings, MemoryMode};
pub use snapshot::{BudgetPart, BudgetUsage, InjectionSnapshot, build_injection_snapshot};
pub use store::{MarkdownMemoryStore, ProjectContext, ScopeDocuments};

/// Directory under the user Vulcan root that owns every memory file.
/// 用户 Vulcan 根目录下持有全部记忆文件的目录。
pub const MEMORY_DIRECTORY: &str = "memory";

/// Directory under [`MEMORY_DIRECTORY`] that owns the Markdown memory files.
/// [`MEMORY_DIRECTORY`] 下持有 Markdown 记忆文件的目录。
pub const MARKDOWN_DIRECTORY: &str = "markdown";

/// Deepest heading level that forms a memory node; deeper headings stay body text.
/// 构成记忆节点的最深标题级别；更深的标题保持为正文。
pub const MAX_NODE_LEVEL: u8 = 4;

/// Longest accepted node title in characters.
/// 可接受的最长节点标题字符数。
pub const MAX_TITLE_CHARS: usize = 120;

/// Largest accepted body for one node in UTF-8 bytes.
/// 单个节点可接受的最大正文 UTF-8 字节数。
pub const MAX_NODE_BODY_BYTES: usize = 16 * 1024;

/// Largest memory file a write may produce, in UTF-8 bytes.
/// 一次写入可以产生的最大记忆文件 UTF-8 字节数。
pub const MAX_FILE_BYTES: usize = 512 * 1024;

/// Most operations accepted by one `memory_write` call.
/// 一次 `memory_write` 调用接受的最多操作数。
pub const MAX_OPERATIONS_PER_WRITE: usize = 16;

/// Most search patterns accepted by one query.
/// 一次查询接受的最多模式数。
pub const MAX_SEARCH_PATTERNS: usize = 12;

/// Default number of nodes returned by one search page.
/// 单页搜索默认返回的节点数。
pub const DEFAULT_SEARCH_LIMIT: usize = 6;

/// Largest number of nodes returned by one search page.
/// 单页搜索最多返回的节点数。
pub const MAX_SEARCH_LIMIT: usize = 20;

/// Body bytes shown per page of one long node; pages break on line boundaries.
/// 长节点每页显示的正文字节数；分页落在行边界。
pub const NODE_PAGE_BYTES: usize = 6 * 1024;

/// Usage ratio, in percent, at which a budget part is reported as nearly full.
/// 预算分项被报告为接近用满的占用百分比。
pub const BUDGET_WARNING_PERCENT: usize = 80;

/// Format version of `registry.json`.
/// `registry.json` 的格式版本。
pub const REGISTRY_FORMAT_VERSION: u32 = 1;

/// Format version of `settings.json`.
/// `settings.json` 的格式版本。
pub const SETTINGS_FORMAT_VERSION: u32 = 1;

/// Format version of an export package manifest.
/// 导出包清单的格式版本。
pub const EXPORT_FORMAT_VERSION: u32 = 1;

/// Estimates the prompt tokens of memory text with the shared host estimator.
/// 使用宿主共享估算器估算记忆文本的提示词 Token。
pub fn estimate_tokens(text: &str) -> usize {
    vulcan_context::estimate_mixed_language_text_tokens(text)
}
