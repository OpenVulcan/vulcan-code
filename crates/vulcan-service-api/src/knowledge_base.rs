//! Browser-facing knowledge-base request and selection contracts.
//! 面向浏览器的资料库请求与选择契约。

use serde::{Deserialize, Serialize};

/// Creates one named knowledge library and optionally binds initial projects.
/// 创建一个命名资料库并可选绑定初始项目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibraryCreateRequest {
    /// User-visible non-empty library name.
    /// 用户可见的非空资料库名称。
    pub name: String,
    /// Optional user-visible description.
    /// 可选的用户可见说明。
    #[serde(default)]
    pub description: String,
    /// Initial project identifiers authorized for the library.
    /// 初始授权给该资料库的项目标识符。
    #[serde(default)]
    pub project_ids: Vec<String>,
}

/// Renames one library without changing its immutable identity.
/// 在不改变不可变身份的前提下重命名一个资料库。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibraryRenameRequest {
    /// Immutable library identifier.
    /// 不可变资料库标识符。
    pub library_id: String,
    /// New user-visible name.
    /// 新的用户可见名称。
    pub name: String,
    /// New user-visible description.
    /// 新的用户可见说明。
    #[serde(default)]
    pub description: String,
}

/// Updates one project authorization binding.
/// 更新一条项目授权绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectBindingRequest {
    /// Immutable library identifier.
    /// 不可变资料库标识符。
    pub library_id: String,
    /// Immutable project identifier.
    /// 不可变项目标识符。
    pub project_id: String,
    /// Whether the project remains authorized.
    /// 项目是否继续获得授权。
    pub authorized: bool,
    /// Whether inherited sessions enable the library by default.
    /// 继承型会话是否默认启用资料库。
    #[serde(default)]
    pub default_enabled: bool,
}

/// Starts one recursive Markdown directory import.
/// 启动一次递归 Markdown 目录导入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeImportRequest {
    /// Immutable target library identifier.
    /// 不可变目标资料库标识符。
    pub library_id: String,
    /// Host directory selected by the user.
    /// 用户选择的宿主目录。
    pub source_root: String,
    /// Existing durable source identity to remap, or none when adding or synchronizing by path.
    /// 要重新映射的现有持久化来源身份；按路径新增或同步时为空。
    #[serde(default)]
    pub source_id: Option<String>,
}

/// Creates or updates one library-local Jieba custom word.
/// 创建或更新一个资料库局部 Jieba 自定义词。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCustomWordRequest {
    /// Immutable target library identifier.
    /// 不可变目标资料库标识符。
    pub library_id: String,
    /// Exact custom word text.
    /// 精确的自定义词文本。
    pub word: String,
    /// Positive Jieba word weight.
    /// 正数 Jieba 词权重。
    pub weight: u32,
}

/// Requests a bounded document page.
/// 请求一个有界文档分页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentListRequest {
    /// Current project identifier used for authorization.
    /// 用于授权校验的当前项目标识符。
    pub project_id: String,
    /// Immutable library identifier.
    /// 不可变资料库标识符。
    pub library_id: String,
    /// Optional exact folder classification.
    /// 可选的精确文件夹分类。
    #[serde(default)]
    pub folder_path: Option<String>,
    /// Requested page size.
    /// 请求分页大小。
    #[serde(default = "default_document_limit")]
    pub limit: u32,
    /// Zero-based row offset.
    /// 从零开始的记录偏移。
    #[serde(default)]
    pub offset: u32,
}

/// Requests a bounded document page from the selected library management surface.
/// 从选中资料库的管理界面请求一个有界文档分页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibraryDocumentListRequest {
    /// Immutable library identifier selected in the management surface.
    /// 管理界面中选中的不可变资料库标识符。
    pub library_id: String,
    /// Optional exact folder classification.
    /// 可选的精确文件夹分类。
    #[serde(default)]
    pub folder_path: Option<String>,
    /// Requested page size.
    /// 请求分页大小。
    #[serde(default = "default_document_limit")]
    pub limit: u32,
    /// Zero-based row offset.
    /// 从零开始的记录偏移量。
    #[serde(default)]
    pub offset: u32,
}

/// Requests one complete portable knowledge-library export.
/// 请求一次完整的可移植资料库导出。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageExportRequest {
    /// Immutable source library identifier.
    /// 不可变的来源资料库标识符。
    pub library_id: String,
    /// Existing destination directory selected by the user.
    /// 用户选择的现有目标目录。
    pub destination_directory: String,
}

/// Requests read-only validation of one candidate .vkb package.
/// 请求只读校验一个候选 .vkb 包。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageInspectRequest {
    /// Native archive path selected by the user.
    /// 用户选择的原生归档路径。
    pub archive_path: String,
}

/// Explicit package-import strategy chosen after inspection.
/// 检查后选择的显式资料库包导入策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgePackageImportStrategy {
    /// Creates one independent library with a new immutable identity.
    /// 使用新的不可变标识创建一个独立资料库。
    Create,
    /// Replaces one exact existing library while retaining its identity and bindings.
    /// 替换一个精确现有资料库，同时保留其标识和绑定。
    Replace,
}

/// Executes one previously inspected package import.
/// 执行一次已经检查的资料库包导入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageImportRequest {
    /// Native archive path selected by the user.
    /// 用户选择的原生归档路径。
    pub archive_path: String,
    /// Final user-visible library name.
    /// 最终用户可见资料库名称。
    pub name: String,
    /// Explicit create-or-replace strategy.
    /// 显式的新建或替换策略。
    pub strategy: KnowledgePackageImportStrategy,
    /// Exact replacement target required only for Replace.
    /// 仅 Replace 策略需要的精确替换目标。
    #[serde(default)]
    pub target_library_id: Option<String>,
}

/// One selectable .vkb file shown by the service-backed package picker.
/// 服务支持的资料库包选择器中显示的一个可选 .vkb 文件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageFileSummary {
    /// File display name.
    /// 文件显示名称。
    pub name: String,
    /// Absolute native file path.
    /// 原生绝对文件路径。
    pub path: String,
    /// Compressed archive byte size.
    /// 压缩归档字节数。
    pub byte_size: u64,
}

/// One directory listing augmented with selectable .vkb files.
/// 一份增加了可选 .vkb 文件的目录列表。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageFileListing {
    /// Shared directory navigation payload.
    /// 共享目录导航载荷。
    pub directory: crate::ProjectDirectoryListing,
    /// Ordinary .vkb files in the current directory.
    /// 当前目录中的普通 .vkb 文件。
    pub packages: Vec<KnowledgePackageFileSummary>,
}

/// Performs one bounded BM25 search across explicitly authorized libraries.
/// 在显式授权资料库中执行一次有界 BM25 检索。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchRequest {
    /// Current project identifier used for authorization.
    /// 用于授权校验的当前项目标识符。
    pub project_id: String,
    /// Library identifiers selected by the caller.
    /// 调用方选择的资料库标识符。
    pub library_ids: Vec<String>,
    /// Non-empty search query.
    /// 非空搜索词。
    pub query: String,
    /// Maximum result count.
    /// 最大结果数量。
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// Performs a bounded BM25 search inside one selected management library.
/// 在管理界面选中的单个资料库内执行有界 BM25 检索。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySearchRequest {
    /// Immutable library identifier selected in the management surface.
    /// 管理界面中选中的不可变资料库标识符。
    pub library_id: String,
    /// Non-empty search query.
    /// 非空搜索词。
    pub query: String,
    /// Maximum result count.
    /// 最大结果数量。
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// Persists one session's knowledge-library selection.
/// 持久化一个会话的资料库选择。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionKnowledgeLibrarySelectionRequest {
    /// Owning project identifier used for authorization validation.
    /// 用于授权校验的所属项目标识符。
    pub project_id: String,
    /// Existing conversation identifier.
    /// 已存在的会话标识符。
    pub conversation_id: String,
    /// Selection mode, either inherit or explicit.
    /// 选择模式，只能是 inherit 或 explicit。
    pub mode: String,
    /// Explicit library identifiers when mode is explicit.
    /// explicit 模式下的显式资料库标识符。
    #[serde(default)]
    pub library_ids: Vec<String>,
}

/// Current session selection together with its effective authorized library identifiers.
/// 当前会话选择及其生效的已授权资料库标识符。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionKnowledgeLibrarySelectionSummary {
    /// Selection mode, either inherit or explicit.
    /// 选择模式，只能是 inherit 或 explicit。
    pub mode: String,
    /// Explicit identifiers persisted for the session.
    /// 为会话持久化的显式标识符。
    pub library_ids: Vec<String>,
    /// Effective identifiers after project authorization and default selection.
    /// 经过项目授权与默认选择后的生效标识符。
    pub effective_library_ids: Vec<String>,
}

/// Returns the default document page size.
/// 返回默认文档分页大小。
fn default_document_limit() -> u32 {
    100
}

/// Returns the default BM25 result limit.
/// 返回默认 BM25 结果数量。
fn default_search_limit() -> u32 {
    20
}

#[cfg(test)]
mod tests {
    use super::KnowledgeImportRequest;

    /// Verifies new callers can target a durable source while old add-directory payloads remain valid.
    /// 验证新调用方可以指定持久化来源，同时旧的新增目录载荷仍然有效。
    #[test]
    fn knowledge_import_request_accepts_optional_source_identity() {
        // addRequest represents the backward-compatible add-or-sync-by-path payload.
        // addRequest 表示向后兼容的按路径新增或同步载荷。
        let add_request: KnowledgeImportRequest = serde_json::from_value(serde_json::json!({
            "libraryId": "library-1",
            "sourceRoot": "D:\\docs"
        }))
        .expect("deserialize add-directory request");
        assert_eq!(add_request.source_id, None);

        // remapRequest represents an explicit physical-path change for one durable source.
        // remapRequest 表示一个持久化来源的显式物理路径修改。
        let remap_request: KnowledgeImportRequest = serde_json::from_value(serde_json::json!({
            "libraryId": "library-1",
            "sourceRoot": "E:\\docs",
            "sourceId": "source-1"
        }))
        .expect("deserialize source-remap request");
        assert_eq!(remap_request.source_id.as_deref(), Some("source-1"));
    }
}
