//! Closed knowledge lifecycle requests; document content is writable only through AI tools.
//! 封闭知识生命周期请求；文档内容仅能经 AI 工具写入。

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Explicit library indexing mode.
/// 显式知识库索引模式.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeIndexModeRequest {
    /// Directory and read only.
    /// 仅目录及读取。
    Browse,
    /// Jieba full-text retrieval.
    /// 结巴全文检索。
    Bm25,
    /// Vector retrieval.
    /// 向量检索。
    Vector,
    /// Full-text/vector rank fusion.
    /// 全文及向量排名融合。
    Hybrid,
}

/// Exact route binding family.
/// 精确路由绑定家族。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRouteBindingKindRequest {
    /// Protocol profile binding.
    /// 协议配置档绑定。
    Protocol,
    /// Provider-native action binding.
    /// 供应商原生动作绑定。
    Action,
}

/// Exact immutable history image side.
/// 精确不可变历史镜像侧别。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeChangeSideRequest {
    /// Before the change.
    /// 变更前。
    Before,
    /// After the change.
    /// 变更后。
    After,
}

/// Exact user-selected embedding route without host authorization fields.
/// 不包含宿主授权字段的精确人工选择嵌入路由。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRouteReferenceRequest {
    /// Provider catalog identity.
    /// 供应商目录身份。
    pub provider_id: String,
    /// Exact protocol profile or action binding.
    /// 精确协议配置档或动作绑定。
    pub protocol: String,
    /// Declared interpretation of protocol.
    /// 协议的声明解释方式。
    pub binding_kind: KnowledgeRouteBindingKindRequest,
    /// Exact service endpoint identity.
    /// 精确服务端点身份。
    pub service_entry_id: String,
    /// Exact signed-catalog model identity.
    /// 精确签名目录模型身份。
    pub model_id: String,
    /// Exact credential connection identity; no credentials are transferred.
    /// 精确凭据连接身份；不传递凭据。
    pub connection_id: String,
}

/// Explicit model dimensions and optional batching selected by the human.
/// 人工选择的显式模型维度及可选批量设置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeEmbeddingSettingsRequest {
    /// Exact selected embedding route.
    /// 精确选定嵌入路由。
    pub route: KnowledgeRouteReferenceRequest,
    /// Positive output dimension validated against current official capabilities.
    /// 根据当前官方能力验证的正输出维度。
    pub dimensions: u32,
    /// Optional positive batch count bounded by official capabilities.
    /// 受官方能力约束的可选正批量数量。
    pub batch_size: Option<u32>,
}

/// Library-owned mode and optional exact embedding route.
/// 库自身拥有的模式及可选精确嵌入路由。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeLibrarySettingsRequest {
    /// Required declared library indexing mode.
    /// 必填声明知识库索引模式。
    pub index_mode: KnowledgeIndexModeRequest,
    /// Required in vector/hybrid modes and forbidden in other modes.
    /// 向量及混合模式必填，其他模式禁止。
    pub embedding: Option<KnowledgeEmbeddingSettingsRequest>,
}

/// Creates an empty library with human lifecycle configuration.
/// 使用人工生命周期配置创建空知识库。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeLibraryCreateRequest {
    /// Unique visible library name.
    /// 唯一可见知识库名称。
    pub name: String,
    /// Human purpose guiding AI content organization.
    /// 指导 AI 内容组织的人工用途。
    pub description: String,
    /// Optional exact initial project bindings.
    /// 可选精确初始项目绑定。
    #[serde(default)]
    pub project_ids: Vec<String>,
    /// Explicit mode and embedding selection.
    /// 显式模式及嵌入选择。
    pub settings: KnowledgeLibrarySettingsRequest,
}

/// Changes only human name and purpose; content is never accepted.
/// 仅变更人工名称与用途；绝不接受内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeLibraryRenameRequest {
    /// Immutable library UUID.
    /// 不可变知识库 UUID。
    pub library_id: String,
    /// New unique visible name.
    /// 新唯一可见名称。
    pub name: String,
    /// New human purpose.
    /// 新人工用途。
    pub description: String,
}

/// Explicit index mode change returning a durable operation.
/// 返回持久操作的显式索引模式变更。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeIndexModeRequestPayload {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// New declared mode and exact embedding configuration.
    /// 新声明模式及精确嵌入配置。
    pub settings: KnowledgeLibrarySettingsRequest,
}

/// Authorizes or revokes one exact project binding.
/// 授权或撤销单个精确项目绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeProjectBindingRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Exact project identity.
    /// 精确项目身份。
    pub project_id: String,
    /// Whether the binding must exist.
    /// 绑定是否必须存在。
    pub authorized: bool,
    /// Whether inherited session selection includes this authorized library.
    /// 继承会话选择是否包含此已授权库。
    #[serde(default)]
    pub default_enabled: bool,
}

/// Creates or updates one library-local Jieba term.
/// 创建或更新单个库内结巴词语。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeCustomWordRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Exact term text.
    /// 精确词语文本。
    pub word: String,
    /// Positive dictionary weight.
    /// 正词典权重。
    pub weight: u32,
}

/// Reads a head-bound logical directory page for management.
/// 读取用于管理的绑定变更头逻辑目录页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeLibraryDocumentListRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Logical folder; empty means root.
    /// 逻辑目录；空字符串表示根。
    #[serde(default)]
    pub path: String,
    /// Descendant levels, default one.
    /// 后代层数，默认一层。
    #[serde(default = "default_depth")]
    pub depth: usize,
    /// Opaque continuation for the same library/path/depth.
    /// 相同库、路径及深度的不透明续读标识。
    pub cursor: Option<String>,
    /// Requested entries bounded by the storage authority.
    /// 受存储权威约束的请求条目数。
    #[serde(default = "default_page_limit")]
    pub limit: u32,
}

/// Reads a project-authorized logical directory page.
/// 读取经项目授权的逻辑目录页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeDocumentListRequest {
    /// Exact owning project identity.
    /// 精确所属项目身份。
    pub project_id: String,
    /// Exact authorized library UUID.
    /// 精确已授权库 UUID。
    pub library_id: String,
    /// Logical directory, empty for root.
    /// 逻辑目录；根目录为空。
    #[serde(default)]
    pub path: String,
    /// Descendant levels, default one.
    /// 后代层数，默认一层。
    #[serde(default = "default_depth")]
    pub depth: usize,
    /// Opaque continuation.
    /// 不透明续读标识。
    pub cursor: Option<String>,
    /// Requested bounded page length.
    /// 请求的有界分页长度。
    #[serde(default = "default_page_limit")]
    pub limit: u32,
}

/// Reads exactly one document selector and a bounded inclusive line range.
/// 读取精确单个文档选择器及有界包含边界行范围。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeDocumentReadRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Stable document ID, mutually exclusive with path.
    /// 稳定文档身份，与路径互斥。
    pub document_id: Option<String>,
    /// Exact logical document path, mutually exclusive with ID.
    /// 精确逻辑文档路径，与身份互斥。
    pub path: Option<String>,
    /// First one-based inclusive line.
    /// 从一开始且包含边界的首行。
    pub start_line: Option<usize>,
    /// Last one-based inclusive line.
    /// 从一开始且包含边界的末行。
    pub end_line: Option<usize>,
}

/// Reads one document under an explicit project authorization.
/// 在显式项目授权下读取单篇文档。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeProjectDocumentReadRequest {
    /// Exact project identity.
    /// 精确项目身份。
    pub project_id: String,
    /// Exact library, selector and line window.
    /// 精确知识库、选择器及行窗口。
    pub document: KnowledgeDocumentReadRequest,
}

/// Searches one library according to its own declared mode.
/// 按自身声明模式检索单个知识库。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeLibrarySearchRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Nonempty natural-language query.
    /// 非空自然语言查询。
    pub query: String,
    /// Maximum current-chunk hits.
    /// 当前切块最大命中数。
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// Searches exact project-authorized libraries using their own modes.
/// 使用各自模式检索精确项目授权知识库。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeSearchRequest {
    /// Exact project identity.
    /// 精确项目身份。
    pub project_id: String,
    /// Exact authorized library subset.
    /// 精确已授权库子集。
    pub library_ids: Vec<String>,
    /// Nonempty natural-language query.
    /// 非空自然语言查询。
    pub query: String,
    /// Maximum fused current-chunk hits.
    /// 融合后当前切块最大命中数。
    #[serde(default = "default_search_limit")]
    pub limit: u32,
}

/// Queries an exact operation, or explicitly discovers the latest library operation.
/// 查询精确操作，或显式发现库最新操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeOperationRequest {
    /// Exact owning library UUID.
    /// 精确所属知识库 UUID。
    pub library_id: String,
    /// Stable operation ID; omission requests latest discovery.
    /// 稳定操作身份；省略时请求发现最新操作。
    pub operation_id: Option<String>,
}

/// Cancels only the exact currently active operation.
/// 仅取消精确当前活动操作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeOperationCancelRequest {
    /// Exact owning library UUID.
    /// 精确所属知识库 UUID。
    pub library_id: String,
    /// Exact stable operation UUID.
    /// 精确稳定操作 UUID。
    pub operation_id: String,
}

/// Reads a bounded immutable history summary page.
/// 读取有界不可变历史摘要页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeChangeListRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Exclusive upper sequence for older history.
    /// 较早历史的排他上界序号。
    pub before_seq: Option<u64>,
    /// Requested bounded summary count.
    /// 请求的有界摘要数。
    #[serde(default = "default_page_limit")]
    pub limit: u32,
}

/// Reads bounded items of one exact immutable change.
/// 读取单个精确不可变变更的有界条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeChangeGetRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Exact immutable change sequence.
    /// 精确不可变变更序号。
    pub seq: u64,
    /// Zero-based image offset.
    /// 从零开始的镜像偏移。
    #[serde(default)]
    pub offset: u64,
    /// Maximum image count, also subject to the shared byte budget.
    /// 最大镜像数，同时受共享字节预算约束。
    #[serde(default = "default_page_limit")]
    pub limit: u32,
}

/// Reads exact historical document text for a before/after comparison.
/// 读取用于前后比较的精确历史文档文本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeChangeDocumentReadRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Exact immutable change sequence.
    /// 精确不可变变更序号。
    pub seq: u64,
    /// Exact changed document UUID.
    /// 精确变更文档 UUID。
    pub document_id: String,
    /// Explicit before or after image.
    /// 显式变更前或变更后镜像。
    pub side: KnowledgeChangeSideRequest,
    /// First one-based inclusive line.
    /// 从一开始且包含边界的首行。
    pub start_line: Option<usize>,
    /// Last one-based inclusive line.
    /// 从一开始且包含边界的末行。
    pub end_line: Option<usize>,
}

/// Human-confirmed restoration that appends a new reversible change.
/// 追加新可还原变更的人工确认还原。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRestoreRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Target history point; zero means initially empty.
    /// 目标历史点；零表示初始空库。
    pub target_seq: u64,
    /// Optional human audit intent.
    /// 可选人工审计意图。
    #[serde(default)]
    pub note: String,
}

/// Reads current vector backlog and a bounded failure page.
/// 读取当前向量积压及有界失败页。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeEmbeddingStatusRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Zero-based failure detail offset.
    /// 从零开始的失败详情偏移。
    #[serde(default)]
    pub offset: u64,
    /// Maximum failure rows.
    /// 最大失败行数。
    #[serde(default = "default_page_limit")]
    pub limit: u32,
}

/// Retries all exhausted hashes or an exact selected document subset.
/// 重试全部耗尽哈希或精确选中文档子集。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeEmbeddingRetryRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Empty means all failed current hashes; no pending or ready work is reset.
    /// 空集合表示全部当前失败哈希；不重置待处理或就绪工作。
    #[serde(default)]
    pub document_ids: Vec<String>,
}

/// Exports current content and configuration only.
/// 仅导出当前内容及配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgePackageExportRequest {
    /// Exact library UUID.
    /// 精确知识库 UUID。
    pub library_id: String,
    /// Existing native destination directory.
    /// 已有原生目标目录。
    pub destination_directory: String,
}

/// Validates a package without side effects.
/// 无副作用校验套件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgePackageInspectRequest {
    /// Exact native package path.
    /// 精确原生套件路径。
    pub archive_path: String,
}

/// Imports the inspected bytes as a new independent library and fresh history.
/// 将已预检字节导入为新的独立库及全新历史。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgePackageImportRequest {
    /// Exact native package path.
    /// 精确原生套件路径。
    pub archive_path: String,
    /// Unique final library name.
    /// 唯一最终知识库名称。
    pub name: String,
    /// Whole-file SHA-256 returned by inspection.
    /// 预检返回的完整文件 SHA-256。
    pub expected_sha256: String,
}

/// A selectable knowledge package file.
/// 可选的知识库套件文件。
pub type KnowledgePackageFileSummary = PackageFileSummary;

/// Directory navigation with selectable `.vkb` knowledge packages.
/// 包含可选 `.vkb` 知识库套件的目录导航。
pub type KnowledgePackageFileListing = PackageFileListing;

/// One selectable package file of a native directory.
/// 原生目录中单个可选的套件文件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageFileSummary {
    /// Visible file name.
    /// 可见文件名。
    pub name: String,
    /// Absolute native path.
    /// 原生绝对路径。
    pub path: String,
    /// Compressed package size.
    /// 压缩套件大小。
    pub byte_size: u64,
}

/// Directory navigation with the selectable package files of one kind.
/// 包含某一类可选套件文件的目录导航。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageFileListing {
    /// Shared native directory navigation.
    /// 共享原生目录导航。
    pub directory: crate::ProjectDirectoryListing,
    /// Ordinary files of the package kind in the directory.
    /// 目录中该套件类型的普通文件。
    pub packages: Vec<PackageFileSummary>,
}

/// Persists only a session's library selection, never a retrieval policy.
/// 仅持久化会话库选择，绝不持久化检索策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionKnowledgeLibrarySelectionRequest {
    /// Exact owning project identity.
    /// 精确所属项目身份。
    pub project_id: String,
    /// Exact existing conversation UUID.
    /// 精确已有会话 UUID。
    pub conversation_id: String,
    /// Explicit inherit or explicit selection mode.
    /// 显式 inherit 或 explicit 选择模式。
    pub mode: String,
    /// Exact selected authorized libraries.
    /// 精确选中已授权知识库。
    #[serde(default)]
    pub library_ids: Vec<String>,
}

/// Persisted selection and currently effective authorization.
/// 持久选择及当前有效授权。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionKnowledgeLibrarySelectionSummary {
    /// Persisted inherit or explicit selection mode.
    /// 持久化 inherit 或 explicit 选择模式。
    pub mode: String,
    /// Exact persisted selected libraries.
    /// 精确持久化选中库。
    pub library_ids: Vec<String>,
    /// Current project-authorized enabled libraries.
    /// 当前项目授权启用库。
    pub effective_library_ids: Vec<String>,
}

/// Human metadata updates use the same closed name/purpose contract as rename.
/// 人工元数据更新使用与重命名相同的封闭名称及用途契约。
pub type KnowledgeLibrarySettingsUpdateRequest = KnowledgeLibraryRenameRequest;

/// Returns the declared default directory depth; no inputs and no side effects.
/// 返回已声明默认目录深度；无输入且无副作用。
fn default_depth() -> usize {
    1
}

/// Returns the requested default page length; storage remains the upper-bound authority.
/// 返回请求默认分页长度；上限权威仍归存储所有。
fn default_page_limit() -> u32 {
    50
}

/// Returns the requested default search limit; storage validates the final bound.
/// 返回请求默认检索上限；存储验证最终边界。
fn default_search_limit() -> u32 {
    20
}
