//! AI-maintained knowledge and explicit human lifecycle models.
//! AI 维护知识与显式人工生命周期模型。

use serde::{Deserialize, Serialize};

/// Maximum UTF-8 bytes in one body.
/// 单篇正文的最大 UTF-8 字节数。
pub const MAX_KNOWLEDGE_BODY_BYTES: usize = 1024 * 1024;

/// Maximum files accepted by one ingest.
/// 单次搬运最大文件数。
pub const MAX_KNOWLEDGE_INGEST_FILES: usize = 200;

/// Maximum decoded bytes accepted by one ingest.
/// 单次搬运最大解码字节数。
pub const MAX_KNOWLEDGE_INGEST_BYTES: usize = 32 * MAX_KNOWLEDGE_BODY_BYTES;

/// Maximum summary Unicode scalar count.
/// 摘要 Unicode 标量数上限。
pub const MAX_KNOWLEDGE_SUMMARY_CHARS: usize = 2000;

/// Maximum keywords per document.
/// 每文档关键词数上限。
pub const MAX_KNOWLEDGE_KEYWORDS: usize = 32;

/// Maximum path component count.
/// 路径组件数量上限。
pub const MAX_KNOWLEDGE_PATH_DEPTH: usize = 8;

/// Maximum Unicode scalars per path segment.
/// 路径段 Unicode 标量数上限。
pub const MAX_KNOWLEDGE_PATH_SEGMENT_CHARS: usize = 128;

/// Maximum heading lines per ingest receipt.
/// 每篇搬运回执标题行数上限。
pub const MAX_KNOWLEDGE_OUTLINE_LINES: usize = 40;

/// Automatic attempts before human retry.
/// 人工重试前的自动尝试次数。
pub const MAX_KNOWLEDGE_EMBEDDING_ATTEMPTS: u32 = 3;

/// Maximum lines returned by one read.
/// 单次读取最大行数。
pub const MAX_KNOWLEDGE_READ_LINES: usize = 400;

/// Maximum federated query count.
/// 联合查询数量上限。
pub const MAX_KNOWLEDGE_QUERIES: usize = 8;

/// Maximum Unicode scalars per query.
/// 每查询 Unicode 标量数上限。
pub const MAX_KNOWLEDGE_QUERY_CHARS: usize = 512;

/// Maximum page rows or search hits.
/// 分页行数或检索命中上限。
pub const MAX_KNOWLEDGE_PAGE_SIZE: u32 = 200;

/// Reciprocal-rank-fusion denominator.
/// 倒数排名融合分母。
pub const KNOWLEDGE_RRF_K: f64 = 60.0;

/// Default embedding batch bounded by model capabilities.
/// 受模型能力限制的默认嵌入批量。
pub const KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE: u32 = 64;

/// Portable package family.
/// 可移植套件家族。
pub const KNOWLEDGE_PACKAGE_FORMAT: &str = "vulcan-code-knowledge-base";

/// Exact accepted package version.
/// 唯一接受的套件版本。
pub const KNOWLEDGE_PACKAGE_VERSION: u32 = 3;

/// Maximum compressed or expanded package bytes.
/// 压缩或解压后套件字节上限。
pub const MAX_KNOWLEDGE_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

/// Library-owned indexing contract; no silent mode changes.
/// 库自身拥有的索引契约；不得静默改变模式。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeIndexMode {
    /// Directory and read access only.
    /// 仅目录和读取访问。
    Browse,
    /// Jieba full-text search.
    /// 结巴全文检索。
    Bm25,
    /// Vector search only.
    /// 仅向量检索。
    Vector,
    /// Full-text and vector rank fusion.
    /// 全文与向量排名融合。
    Hybrid,
}

impl KnowledgeIndexMode {
    /// Returns whether this mode requires full-text indexing.
    /// 返回该模式是否需要全文索引。
    pub const fn has_fts(self) -> bool {
        matches!(self, Self::Bm25 | Self::Hybrid)
    }
    /// Returns whether this mode requires embeddings.
    /// 返回该模式是否需要嵌入。
    pub const fn has_vectors(self) -> bool {
        matches!(self, Self::Vector | Self::Hybrid)
    }
}

/// Exact route binding family.
/// 精确路由绑定家族。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRouteBindingKind {
    /// Declared protocol profile route.
    /// 声明的协议配置路由。
    Protocol,
    /// Declared provider-native action route.
    /// 声明的供应商原生动作路由。
    Action,
}

impl KnowledgeRouteBindingKind {
    /// Returns the stable binding identifier.
    /// 返回稳定绑定标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Action => "action",
        }
    }
}

/// Exact frozen provider route, without alternate probing.
/// 精确冻结的供应商路由，不探测替代路由。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRouteReference {
    /// Authorization and budget owner.
    /// 授权和预算归属。
    pub consumer_profile_id: String,
    /// Provider identity.
    /// 供应商身份。
    pub provider_id: String,
    /// Declared protocol.
    /// 声明协议。
    pub protocol: String,
    /// Declared binding family.
    /// 声明绑定家族。
    pub binding_kind: KnowledgeRouteBindingKind,
    /// Exact service entry.
    /// 精确服务入口。
    pub service_entry_id: String,
    /// Exact model identity.
    /// 精确模型身份。
    pub model_id: String,
    /// Authorized usage scope.
    /// 已授权使用范围。
    pub usage_scope: String,
    /// Optional credential connection.
    /// 可选凭据连接。
    pub connection_id: Option<String>,
}

/// Frozen vector-space request settings.
/// 冻结的向量空间请求设置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeEmbeddingSettings {
    /// Route shared by document and query embeddings.
    /// 文档与查询嵌入共用路由。
    pub route: KnowledgeRouteReference,
    /// Validated model and capability fingerprint.
    /// 已验证模型与能力指纹。
    pub model_fingerprint: String,
    /// Frozen positive output dimensions.
    /// 冻结的正输出维度。
    pub dimensions: u32,
    /// Batch bounded by model capability.
    /// 受模型能力限制的批量。
    pub batch_size: u32,
}

/// Durable policy; cannot mutate content.
/// 持久策略，不能变更内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeLibrarySettings {
    /// Explicit index mode.
    /// 显式索引模式。
    pub index_mode: KnowledgeIndexMode,
    /// Required only in vector modes.
    /// 仅向量模式必填。
    pub embedding: Option<KnowledgeEmbeddingSettings>,
}

impl KnowledgeLibrarySettings {
    /// Validates explicit settings; returns an English diagnostic for invalid mode or route data.
    /// 验证显式设置；模式或路由非法时返回英文诊断。

    pub fn validate(&self) -> Result<(), String> {
        if self.index_mode.has_vectors() != self.embedding.is_some() {
            return Err("ERR_KNOWLEDGE_EMBEDDING_REQUIRED: vector modes require an embedding route; other modes must omit it".into());
        }
        if let Some(embedding) = &self.embedding {
            if embedding.dimensions == 0
                || embedding.batch_size == 0
                || embedding.model_fingerprint.trim().is_empty()
            {
                return Err("ERR_KNOWLEDGE_EMBEDDING_INVALID: dimension, batch and fingerprint must be specified".into());
            }
            for value in [
                &embedding.route.consumer_profile_id,
                &embedding.route.provider_id,
                &embedding.route.protocol,
                &embedding.route.service_entry_id,
                &embedding.route.model_id,
                &embedding.route.usage_scope,
            ] {
                if value.trim().is_empty() {
                    return Err(
                        "ERR_KNOWLEDGE_ROUTE_INVALID: route identifiers must be nonempty".into(),
                    );
                }
            }
        }
        Ok(())
    }
}

/// Registry projection; invalid libraries expose their reason and deletion only.
/// 注册投影；非法知识库只暴露原因与删除。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySummary {
    /// Immutable library UUID.
    /// 不可变知识库 UUID。
    pub id: String,
    /// Human-maintained name.
    /// 人工维护名称。
    pub name: String,
    /// Purpose guiding AI decisions.
    /// 指导 AI 决策的用途。
    pub description: String,
    /// Validated policy; absent on invalid formats.
    /// 已验证策略；非法格式为空。
    pub settings: Option<KnowledgeLibrarySettings>,
    /// Active, deleting or error.
    /// 活动、删除中或错误。
    pub lifecycle_state: String,
    /// Explicit format or integrity reason.
    /// 明确的格式或完整性原因。
    pub error_message: Option<String>,
    /// Current document count.
    /// 当前文档数。
    pub document_count: u64,
    /// Current chunk count.
    /// 当前切块数。
    pub chunk_count: u64,
    /// Documents requiring summary work.
    /// 需要处理摘要的文档数。
    pub pending_summary_count: u64,
    /// Derived full-text status.
    /// 派生全文状态。
    pub index_status: String,
    /// Last content change.
    /// 最后内容变更。
    pub head_change_seq: u64,
    /// Last content commit time.
    /// 最后内容提交时间。
    pub last_change_at_ms: Option<i64>,
    /// Registry creation time.
    /// 注册创建时间。
    pub created_at_ms: i64,
    /// Registry update time.
    /// 注册更新时间。
    pub updated_at_ms: i64,
}

/// Explicit project authorization.
/// 显式项目授权。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeProjectBinding {
    /// Authorized library.
    /// 已授权知识库。
    pub library_id: String,
    /// Owning project.
    /// 所属项目。
    pub project_id: String,
    /// Inherited enablement.
    /// 继承启用状态。
    pub default_enabled: bool,
}

/// Per-document vector inclusion policy.
/// 每文档向量收录策略。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeEmbedPolicy {
    /// Summary only.
    /// 仅摘要。
    #[default]
    Summary,
    /// Summary and body.
    /// 摘要和正文。
    Full,
    /// Exclude all chunks.
    /// 排除全部切块。
    None,
}

/// Observed source evidence.
/// 已观测来源证据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeProvenance {
    /// File, URL or session discriminator.
    /// 文件、网址或会话判别标识。
    pub kind: String,
    /// Exact source identity.
    /// 精确来源身份。
    #[serde(rename = "ref")]
    pub reference: String,
    /// Observed original-byte digest.
    /// 已观测原字节摘要。
    pub sha256: Option<String>,
    /// Observation time.
    /// 观测时间。
    pub observed_at_ms: i64,
}

/// Complete current document and history image; body is content addressed.
/// 完整当前文档与历史镜像；正文采用内容寻址。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeDocumentSummary {
    /// Stable document UUID across moves.
    /// 移动后稳定的文档 UUID。
    pub id: String,
    /// NFC relative path.
    /// NFC 相对路径。
    pub path: String,
    /// Case-insensitive comparison key.
    /// 忽略大小写的比较键。
    pub path_key: String,
    /// Parent directory.
    /// 父目录。
    pub folder_path: String,
    /// AI-maintained title.
    /// AI 维护标题。
    pub title: String,
    /// AI-maintained summary.
    /// AI 维护摘要。
    pub summary: String,
    /// Provided, pending or stale.
    /// 已提供、待补充或待复核。
    pub summary_state: String,
    /// AI-maintained keywords.
    /// AI 维护关键词。
    pub keywords: Vec<String>,
    /// Immutable body digest shared by history.
    /// 与历史共享的不可变正文摘要。
    pub body_sha256: String,
    /// Logical body line count.
    /// 正文逻辑行数。
    pub line_count: usize,
    /// Vector inclusion policy.
    /// 向量收录策略。
    pub embed_policy: KnowledgeEmbedPolicy,
    /// Source observations.
    /// 来源观测。
    pub provenance: Vec<KnowledgeProvenance>,
    /// Monotonic optimistic token, including restores.
    /// 包括还原在内的单调乐观令牌。
    pub revision: u64,
    /// Creation change.
    /// 创建变更。
    pub created_change_seq: u64,
    /// Latest update.
    /// 最新更新。
    pub updated_change_seq: u64,
}

/// Read-only exact body window with authoritative metadata.
/// 只读精确正文窗口与权威元数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeDocumentContent {
    /// Current document.
    /// 当前文档。
    pub document: KnowledgeDocumentSummary,
    /// Exact requested text window.
    /// 请求的精确文本窗口。
    pub content: String,
    /// First inclusive line.
    /// 包含边界首行。
    pub start_line: usize,
    /// Last inclusive line.
    /// 包含边界末行。
    pub end_line: usize,
    /// Continuation if truncated.
    /// 截断时续读行。
    pub next_line: Option<usize>,
}

/// Explicit directory metadata; implicit folders derive from paths.
/// 显式目录元数据；隐式目录从路径派生。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeFolder {
    /// Comparison identity.
    /// 比较身份。
    pub path_key: String,
    /// NFC display path.
    /// NFC 显示路径。
    pub path: String,
    /// AI-maintained purpose.
    /// AI 维护用途。
    pub description: String,
    /// Creation change.
    /// 创建变更。
    pub created_change_seq: u64,
    /// Last update.
    /// 最近更新。
    pub updated_change_seq: u64,
}

/// Directory or document entry with stable sorting identity.
/// 具有稳定排序身份的目录或文档条目。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeListEntry {
    /// Folder or document discriminator.
    /// 目录或文档判别标识。
    pub kind: String,
    /// Canonical display path.
    /// 规范显示路径。
    pub path: String,
    /// Directory purpose.
    /// 目录用途。
    pub description: String,
    /// Descendant documents.
    /// 后代文档数。
    pub document_count: usize,
    /// Document metadata for document entries.
    /// 文档条目的文档元数据。
    pub document: Option<KnowledgeDocumentSummary>,
}

/// Directory page tied to a content head; stale cursors are rejected.
/// 绑定内容变更头的目录页；拒绝过期游标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeListPage {
    /// Returned entries.
    /// 返回条目。
    pub entries: Vec<KnowledgeListEntry>,
    /// Opaque continuation.
    /// 不透明续读标识。
    pub next_cursor: Option<String>,
    /// Observed content head.
    /// 观测到的内容头。
    pub head_change_seq: u64,
}

/// Actual authenticated invocation identity.
/// 实际已认证调用身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeChangeActor {
    /// AI, user or system.
    /// AI、用户或系统。
    pub kind: String,
    /// Executing root or child session.
    /// 执行的根会话或子会话。
    pub session_id: Option<String>,
    /// Actual turn identity.
    /// 实际回合身份。
    pub turn_id: Option<String>,
    /// Actual tool call identity.
    /// 实际工具调用身份。
    pub tool_call_id: Option<String>,
}

/// Immutable history summary.
/// 不可变历史摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeChange {
    /// Monotonic sequence.
    /// 单调序号。
    pub seq: u64,
    /// Commit time.
    /// 提交时间。
    pub at_ms: i64,
    /// Authenticated actor.
    /// 已认证执行者。
    pub actor: KnowledgeChangeActor,
    /// Explicit operation.
    /// 显式操作。
    pub operation: String,
    /// Auditable intent.
    /// 可审计意图。
    pub note: String,
    /// Target of a restore.
    /// 还原目标。
    pub restore_target_seq: Option<u64>,
    /// Changed entity count.
    /// 变更实体数。
    pub item_count: usize,
}

/// Exact row images without repeated body bytes.
/// 不重复正文字节的精确行镜像。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeChangeItem {
    /// Document or folder.
    /// 文档或目录。
    pub entity: String,
    /// Stable document ID or folder key.
    /// 稳定文档身份或目录键。
    pub entity_key: String,
    /// Complete prior row.
    /// 完整前一记录。
    pub before: Option<serde_json::Value>,
    /// Complete next row.
    /// 完整后一记录。
    pub after: Option<serde_json::Value>,
}

/// History detail and ordered atomic images.
/// 历史详情与有序原子镜像。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeChangeDetail {
    /// Summary.
    /// 摘要。
    pub change: KnowledgeChange,
    /// Ordered images.
    /// 有序镜像。
    pub items: Vec<KnowledgeChangeItem>,
}

/// Exact literal edit checked before committing.
/// 提交前校验的精确字面编辑。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeTextEdit {
    /// Nonempty literal match.
    /// 非空字面匹配。
    pub old_text: String,
    /// Replacement text.
    /// 替换文本。
    pub new_text: String,
    /// Explicitly permit multiple matches.
    /// 显式允许多个匹配。
    #[serde(default)]
    pub replace_all: bool,
}

/// Flat AI request; action validation rejects irrelevant fields.
/// 平铺 AI 请求；动作验证拒绝无关字段。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeWriteRequest {
    /// Create, update, edit, move, delete, folder or delete_folder.
    /// 创建、更新、编辑、移动、删除、目录或删除目录。
    pub action: String,
    /// Exact document selector.
    /// 精确文档选择器。
    pub document_id: Option<String>,
    /// Exact path selector or destination for creation.
    /// 精确路径选择器或创建目标。
    pub path: Option<String>,
    /// Required token for existing documents.
    /// 已有文档必填令牌。
    pub expected_revision: Option<u64>,
    /// Replacement title.
    /// 替换标题。
    pub title: Option<String>,
    /// Replacement summary.
    /// 替换摘要。
    pub summary: Option<String>,
    /// Replacement body.
    /// 替换正文。
    pub body: Option<String>,
    /// Replacement keywords.
    /// 替换关键词。
    pub keywords: Option<Vec<String>>,
    /// Replacement vector policy.
    /// 替换向量策略。
    pub embed: Option<KnowledgeEmbedPolicy>,
    /// Source observations.
    /// 来源观测。
    pub sources: Option<Vec<KnowledgeProvenance>>,
    /// Exact literal edits.
    /// 精确字面编辑。
    pub edits: Option<Vec<KnowledgeTextEdit>>,
    /// Move destination.
    /// 移动目标。
    pub new_path: Option<String>,
    /// Directory description.
    /// 目录说明。
    pub description: Option<String>,
    /// Auditable change intent.
    /// 可审计变更意图。
    #[serde(default)]
    pub note: String,
}

/// One decoded file accepted for host ingest without model round trips.
/// 宿主搬运接受的一份解码文件，不往返模型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeIngestDocument {
    /// Validated target path.
    /// 已验证目标路径。
    pub path: String,
    /// Host-derived title.
    /// 宿主派生标题。
    pub title: String,
    /// Exact decoded body.
    /// 精确解码正文。
    pub body: String,
    /// Optional AI summary.
    /// 可选 AI 摘要。
    pub summary: Option<String>,
    /// Original file observation and digest.
    /// 原文件观测与摘要。
    pub provenance: KnowledgeProvenance,
    /// Vector inclusion.
    /// 向量收录。
    pub embed: KnowledgeEmbedPolicy,
}

/// Atomic mutation receipt.
/// 原子变更回执。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeWriteResult {
    /// New change; absent only for unchanged ingest.
    /// 新变更；仅无变化搬运为空。
    pub change_seq: Option<u64>,
    /// Affected surviving documents.
    /// 受影响且仍存在的文档。
    pub documents: Vec<KnowledgeDocumentSummary>,
    /// Unchanged ingest paths.
    /// 无变化搬运路径。
    pub unchanged: Vec<String>,
    /// Pending vector count.
    /// 待生成向量数。
    pub pending_embeddings: u64,
}

/// Human-maintained custom Jieba term; no document content is editable here.
/// 人工维护的自定义结巴词；此处不能编辑文档内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeCustomWord {
    /// Exact nonempty term.
    /// 精确非空词语。
    pub word: String,
    /// Positive dictionary weight.
    /// 正词典权重。
    pub weight: u32,
}

/// Current derived-index consistency; computed from exact current rows rather than registry counters.
/// 当前派生索引一致性；从精确当前行而非注册计数计算。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeIndexIntegritySummary {
    /// Exact library index mode.
    /// 精确知识库索引模式。
    pub index_mode: KnowledgeIndexMode,
    /// Current authoritative documents.
    /// 当前权威文档数。
    pub document_count: u64,
    /// Current authoritative chunks.
    /// 当前权威切块数。
    pub chunk_count: u64,
    /// Whether the FTS table exists.
    /// 全文表是否存在。
    pub fts_present: bool,
    /// Actual FTS rows.
    /// 实际全文行数。
    pub indexed_chunk_count: u64,
    /// Missing, stale, duplicate or orphan FTS rows.
    /// 缺失、过期、重复或孤立全文行数。
    pub mismatched_rows: u64,
    /// Current content head.
    /// 当前内容头。
    pub head_change_seq: u64,
    /// Indexed content head.
    /// 已索引内容头。
    pub indexed_change_seq: u64,
    /// Current custom dictionary revision.
    /// 当前自定义词典修订。
    pub dictionary_revision: u64,
    /// Indexed custom dictionary revision.
    /// 已索引自定义词典修订。
    pub indexed_dictionary_revision: u64,
    /// Whether all required derived state agrees with current authority.
    /// 全部必需派生状态是否与当前权威一致。
    pub healthy: bool,
}
