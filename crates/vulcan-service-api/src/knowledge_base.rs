//! Browser-facing knowledge-base request and selection contracts.
//! 面向浏览器的知识库请求与选择契约。

use serde::{Deserialize, Serialize};

/// Selects one of the supported knowledge retrieval policies at creation time.
/// 在创建资料库时选择一种受支持的检索策略。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRetrievalModeRequest {
    /// Use only the authoritative Jieba BM25 corpus.
    /// 仅使用权威原始库 Jieba BM25。
    RawBm25,
    /// Use the curated display corpus with Jieba BM25.
    /// 使用整理后的显示库 Jieba BM25。
    DisplayBm25,
    /// Use display-corpus BM25 and a vector index.
    /// 使用显示库 BM25 与向量索引。
    DisplayHybrid,
}

impl Default for KnowledgeRetrievalModeRequest {
    /// Returns the backwards-compatible raw BM25 policy.
    /// 返回兼容旧资料库的原始 BM25 策略。
    fn default() -> Self {
        Self::RawBm25
    }
}

/// Selects whether raw-corpus fallback is implicit.
/// 选择是否允许隐式回退原始库。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeFallbackPolicyRequest {
    /// Require an explicit raw-corpus selection.
    /// 要求显式选择原始库。
    ExplicitOnly,
    /// Fall back only when the display corpus is absent.
    /// 仅在显示库不存在时回退。
    RawOnlyWhenDisplayMissing,
}

impl Default for KnowledgeFallbackPolicyRequest {
    /// Returns the fail-closed default.
    /// 返回失败关闭默认值。
    fn default() -> Self {
        Self::ExplicitOnly
    }
}

/// Identifies one inference route without carrying credentials.
/// 标识一个推理路由且不携带凭据。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRouteBindingKindRequest {
    /// Selects a protocol profile route.
    /// 选择协议配置路由。
    Protocol,
    /// Selects a provider-native action route.
    /// 选择供应商原生动作路由。
    Action,
}

impl Default for KnowledgeRouteBindingKindRequest {
    /// Preserves the legacy protocol interpretation for older callers.
    /// 为旧调用方保留协议路由解释。
    fn default() -> Self {
        Self::Protocol
    }
}

/// Returns the default usage scope accepted by knowledge background tasks.
/// 返回知识库后台任务接受的默认使用范围。
fn default_knowledge_route_usage_scope() -> String {
    String::from("internal_background")
}

/// KnowledgeRouteReferenceRequest carries a credential-free selected route from the browser.
/// KnowledgeRouteReferenceRequest 承载浏览器选择的不含凭据路由。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRouteReferenceRequest {
    /// Consumer profile used for authorization and budgets.
    /// 用于授权与配额的消费方配置。
    pub consumer_profile_id: String,
    /// Provider catalog identifier.
    /// Provider 目录标识。
    pub provider_id: String,
    /// Transport protocol or route family.
    /// 传输协议或路由类型。
    pub protocol: String,
    /// Exact binding kind used to interpret `protocol`.
    /// 用于解释 `protocol` 的精确绑定类型。
    #[serde(default)]
    pub binding_kind: KnowledgeRouteBindingKindRequest,
    /// Normalized service-entry identifier.
    /// 规范化服务端点标识。
    pub service_entry_id: String,
    /// Provider model identifier.
    /// Provider 模型标识。
    pub model_id: String,
    /// Usage scope authorized for detached knowledge processing.
    /// 分离式知识库处理获授权的使用范围。
    #[serde(default = "default_knowledge_route_usage_scope")]
    pub usage_scope: String,
    /// Optional host connection identifier.
    /// 可选的宿主连接标识。
    #[serde(default)]
    pub connection_id: Option<String>,
}

/// Creation-time curation model configuration.
/// 创建时的整理模型配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCurationSettingsRequest {
    /// Enables fixed-prompt JSON curation.
    /// 是否启用固定提示词 JSON 整理。
    #[serde(default)]
    pub enabled: bool,
    /// Route selected for curation.
    /// 整理任务选择的路由。
    #[serde(default)]
    pub route: Option<KnowledgeRouteReferenceRequest>,
    /// Versioned fixed prompt identifier.
    /// 固定提示词版本标识。
    #[serde(default = "default_knowledge_prompt_version")]
    pub prompt_version: String,
    /// Desired model fingerprint used for change warnings.
    /// 用于模型变化警告的期望模型指纹。
    #[serde(default)]
    pub desired_model_fingerprint: Option<String>,
    /// Optional provider-neutral reasoning effort for curation requests.
    /// 整理请求可选的供应商无关推理级别。
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

/// Creation-time embedding model and official capability configuration.
/// 创建时的向量模型与官方能力配置。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingSettingsRequest {
    /// Enables vector generation and retrieval.
    /// 是否启用向量生成与检索。
    #[serde(default)]
    pub enabled: bool,
    /// Route selected for embeddings.
    /// 向量任务选择的路由。
    #[serde(default)]
    pub route: Option<KnowledgeRouteReferenceRequest>,
    /// Desired embedding model fingerprint.
    /// 期望向量模型指纹。
    #[serde(default)]
    pub desired_model_fingerprint: Option<String>,
    /// User-configured expected dimension.
    /// 用户配置的期望维度。
    #[serde(default)]
    pub configured_dimension: Option<u32>,
    /// Maximum input count read from the official model capability entry.
    /// 从官方模型能力条目读取的最大输入数量。
    #[serde(default)]
    pub model_max_batch_size: Option<u32>,
    /// Optional user override; the service validates it against the capability.
    /// 可选用户覆盖值，服务端会根据能力上限校验。
    #[serde(default)]
    pub requested_batch_size: Option<u32>,
    /// User-confirmed same-dimension compatibility note for an existing index.
    /// 用户确认同维度兼容时填写的现有索引风险说明。
    #[serde(default)]
    pub compatibility_override: Option<String>,
}

/// Complete creation-time per-library settings request.
/// 完整的创建时按资料库配置请求。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySettingsRequest {
    /// Retrieval policy selected by the user.
    /// 用户选择的检索策略。
    #[serde(default)]
    pub retrieval_mode: KnowledgeRetrievalModeRequest,
    /// Fallback policy selected by the user.
    /// 用户选择的回退策略。
    #[serde(default)]
    pub fallback_policy: KnowledgeFallbackPolicyRequest,
    /// Curated display-corpus configuration.
    /// 整理显示库配置。
    #[serde(default)]
    pub curation: KnowledgeCurationSettingsRequest,
    /// Embedding configuration sourced from official capabilities.
    /// 来源于官方能力配置的向量设置。
    #[serde(default)]
    pub embedding: KnowledgeEmbeddingSettingsRequest,
}

/// Returns the version of the fixed display-corpus prompt.
/// 返回显示库固定提示词版本。
fn default_knowledge_prompt_version() -> String {
    String::from("knowledge-display-v1")
}

/// Creates one named knowledge library and optionally binds initial projects.
/// 创建一个命名知识库并可选绑定初始项目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibraryCreateRequest {
    /// User-visible non-empty library name.
    /// 用户可见的非空知识库名称。
    pub name: String,
    /// Optional user-visible description.
    /// 可选的用户可见说明。
    #[serde(default)]
    pub description: String,
    /// Initial project identifiers authorized for the library.
    /// 初始授权给该知识库的项目标识符。
    #[serde(default)]
    pub project_ids: Vec<String>,
    /// Per-library retrieval and model policy selected during creation.
    /// 创建时选择的按资料库检索与模型策略。
    #[serde(default)]
    pub settings: KnowledgeLibrarySettingsRequest,
}

/// Updates one existing library's retrieval and model settings.
/// 更新一个已有资料库的检索与模型配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySettingsUpdateRequest {
    /// Immutable target library identifier.
    /// 不可变目标资料库标识。
    pub library_id: String,
    /// New validated per-library configuration.
    /// 新的、经过校验的按资料库配置。
    pub settings: KnowledgeLibrarySettingsRequest,
}

/// Renames one library without changing its immutable identity.
/// 在不改变不可变身份的前提下重命名一个知识库。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibraryRenameRequest {
    /// Immutable library identifier.
    /// 不可变知识库标识符。
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
    /// 不可变知识库标识符。
    pub library_id: String,
    /// Immutable project identifier.
    /// 不可变项目标识符。
    pub project_id: String,
    /// Whether the project remains authorized.
    /// 项目是否继续获得授权。
    pub authorized: bool,
    /// Whether inherited sessions enable the library by default.
    /// 继承型会话是否默认启用知识库。
    #[serde(default)]
    pub default_enabled: bool,
}

/// Starts one recursive Markdown directory import.
/// 启动一次递归 Markdown 目录导入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeImportRequest {
    /// Immutable target library identifier.
    /// 不可变目标知识库标识符。
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
/// 创建或更新一个知识库局部 Jieba 自定义词。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCustomWordRequest {
    /// Immutable target library identifier.
    /// 不可变目标知识库标识符。
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
    /// 不可变知识库标识符。
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
/// 从选中知识库的管理界面请求一个有界文档分页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibraryDocumentListRequest {
    /// Immutable library identifier selected in the management surface.
    /// 管理界面中选中的不可变知识库标识符。
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
/// 请求一次完整的可移植知识库导出。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageExportRequest {
    /// Immutable source library identifier.
    /// 不可变的来源知识库标识符。
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
/// 检查后选择的显式知识库包导入策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgePackageImportStrategy {
    /// Creates one independent library with a new immutable identity.
    /// 使用新的不可变标识创建一个独立知识库。
    Create,
    /// Replaces one exact existing library while retaining its identity and bindings.
    /// 替换一个精确现有知识库，同时保留其标识和绑定。
    Replace,
}

/// Executes one previously inspected package import.
/// 执行一次已经检查的知识库包导入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageImportRequest {
    /// Native archive path selected by the user.
    /// 用户选择的原生归档路径。
    pub archive_path: String,
    /// Final user-visible library name.
    /// 最终用户可见知识库名称。
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
/// 服务支持的知识库包选择器中显示的一个可选 .vkb 文件。
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
/// 在显式授权知识库中执行一次有界 BM25 检索。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchRequest {
    /// Current project identifier used for authorization.
    /// 用于授权校验的当前项目标识符。
    pub project_id: String,
    /// Library identifiers selected by the caller.
    /// 调用方选择的知识库标识符。
    pub library_ids: Vec<String>,
    /// Non-empty search query.
    /// 非空搜索词。
    pub query: String,
    /// Optional explicit corpus policy for this search; omitted means the library policy.
    /// 本次检索可选的显式内容层策略；缺省时使用资料库自身策略。
    #[serde(default)]
    pub retrieval_mode: Option<KnowledgeRetrievalModeRequest>,
    /// Maximum result count.
    /// 最大结果数量。
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// Performs a bounded BM25 search inside one selected management library.
/// 在管理界面选中的单个知识库内执行有界 BM25 检索。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySearchRequest {
    /// Immutable library identifier selected in the management surface.
    /// 管理界面中选中的不可变知识库标识符。
    pub library_id: String,
    /// Non-empty search query.
    /// 非空搜索词。
    pub query: String,
    /// Optional explicit corpus policy for this management search.
    /// 管理界面本次检索可选的显式内容层策略。
    #[serde(default)]
    pub retrieval_mode: Option<KnowledgeRetrievalModeRequest>,
    /// Maximum result count.
    /// 最大结果数量。
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// Requests one complete AI-curated display-corpus rebuild.
/// 请求一次完整的 AI 整理显示库重建。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDisplayBuildRequest {
    /// Immutable target library identifier.
    /// 不可变目标资料库标识符。
    pub library_id: String,
}

/// Persists one session's knowledge-library selection.
/// 持久化一个会话的知识库选择。
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
    /// explicit 模式下的显式知识库标识符。
    #[serde(default)]
    pub library_ids: Vec<String>,
    /// Optional corpus policy applied by the session's knowledge tools.
    /// 会话知识库工具使用的可选内容层策略。
    #[serde(default)]
    pub retrieval_mode: Option<KnowledgeRetrievalModeRequest>,
}

/// Current session selection together with its effective authorized library identifiers.
/// 当前会话选择及其生效的已授权知识库标识符。
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
    /// Optional corpus policy persisted for this session.
    /// 为该会话持久化的可选内容层策略。
    #[serde(default)]
    pub retrieval_mode: Option<KnowledgeRetrievalModeRequest>,
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
