//! Host-injected provider and storage contracts for the VCE functional entry.
//! VCE 功能入口使用的宿主注入型模型与存储契约。
//!
//! The contracts deliberately describe operations and data ownership only. VulcanCode supplies
//! the registered AI capability, VLDB-backed storage, authorization, and lifecycle scheduling.
//! 这些契约只描述操作和数据所有权。VulcanCode 提供已注册的 AI 能力、基于 VLDB 的存储、授权和生命周期调度。

use crate::embedding::{EmbeddingBatch, EmbeddingSpace, EmbeddingVector, SegmentVector};
use crate::management::{
    VceManagementQueryRequest, VceManagementQueryResponse, VceOperationKind, VceOperationPhase,
};
use crate::model::{Chunk, SearchHit};
use crate::path_index::PathDocument;
use crate::state::VceStateSnapshot;
use crate::structure::StructureMode;
use crate::symbol::SymbolOccurrence;
use crate::workspace::{Checkpoint, FileObservation};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::pin::Pin;

/// A boxed asynchronous operation supplied by the upper host.
/// 由上层宿主提供的装箱异步操作。
pub type VceFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Identify one provider or storage operation at the VCE boundary.
/// 标识 VCE 边界上的一种模型或存储操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VcePortOperation {
    /// Create a document, path, or query embedding.
    /// 创建代码文档、路径文档或查询向量。
    Embedding,
    /// Rewrite a natural-language retrieval query.
    /// 改写自然语言检索查询。
    QueryRewrite,
    /// Rerank the fused retrieval candidates.
    /// 对融合后的检索候选进行重排。
    Rerank,
    /// Find blobs that are unknown or not indexed by storage.
    /// 查询存储中未知或尚未建立索引的 Blob。
    FindMissing,
    /// Read immutable source text through the host workspace boundary.
    /// 通过宿主工作区边界读取不可变源码文本。
    SourceRead,
    /// Stage a bounded batch of prepared index records.
    /// 暂存一批有界的已准备索引记录。
    Stage,
    /// Abort one incomplete staging transaction.
    /// 中止一次未完成的暂存事务。
    AbortStage,
    /// Atomically commit staged index data and a checkpoint.
    /// 原子提交暂存索引数据和检查点。
    Commit,
    /// Search the content vector channel.
    /// 搜索内容向量通道。
    DenseSearch,
    /// Search the auxiliary path vector channel.
    /// 搜索辅助路径向量通道。
    PathSearch,
    /// Search the exact symbol channel.
    /// 搜索精确符号通道。
    ExactSearch,
    /// Read host-owned index management data.
    /// 读取宿主持有的索引管理数据。
    ManagementSnapshot,
    /// Execute a typed read-only management query.
    /// 执行一项类型化只读管理查询。
    ManagementQuery,
}

impl VcePortOperation {
    /// Return the stable operation identifier used in diagnostics and host routing.
    /// 返回用于诊断和宿主路由的稳定操作标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::QueryRewrite => "query_rewrite",
            Self::Rerank => "rerank",
            Self::FindMissing => "find_missing",
            Self::SourceRead => "source_read",
            Self::Stage => "stage",
            Self::AbortStage => "abort_stage",
            Self::Commit => "commit",
            Self::DenseSearch => "dense_search",
            Self::PathSearch => "path_search",
            Self::ExactSearch => "exact_search",
            Self::ManagementSnapshot => "management_snapshot",
            Self::ManagementQuery => "management_query",
        }
    }
}

/// Bind one VCE index port to an immutable code-semantic namespace and vector contract.
/// 将一个 VCE 索引端口绑定到不可变的代码语义命名空间和向量契约。
/// Serialization preserves identity only; decoded bindings still require normal host/core admission checks.
/// 序列化仅保留身份；解码绑定仍须经过正常宿主／核心准入检查。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VceIndexBinding {
    /// Host-owned code-semantic namespace identity.
    /// 宿主持有的代码语义命名空间身份。
    pub namespace_id: String,
    /// Stable checkpoint chain owned by this namespace.
    /// 此命名空间持有的稳定检查点链。
    pub checkpoint_chain_id: String,
    /// Host storage Schema version used by the namespace.
    /// 此命名空间使用的宿主存储 Schema 版本。
    pub schema_version: u32,
    /// Exact vector compatibility contract persisted by the index.
    /// 索引持久化的精确向量兼容契约。
    pub embedding_space: EmbeddingSpace,
}

impl VceIndexBinding {
    /// Build a validated code-semantic index binding without opening storage.
    /// 在不打开存储的情况下构建经过校验的代码语义索引绑定。
    pub fn new(
        namespace_id: impl Into<String>,
        checkpoint_chain_id: impl Into<String>,
        schema_version: u32,
        embedding_space: EmbeddingSpace,
    ) -> Result<Self, VcePortError> {
        let binding = Self {
            namespace_id: namespace_id.into(),
            checkpoint_chain_id: checkpoint_chain_id.into(),
            schema_version,
            embedding_space,
        };
        if binding.namespace_id.trim().is_empty()
            || binding.checkpoint_chain_id.trim().is_empty()
            || binding.schema_version == 0
            || !binding.embedding_space.is_well_formed()
        {
            return Err(VcePortError::Rejected {
                operation: VcePortOperation::ManagementSnapshot,
                reason: "VCE index binding is incomplete".to_owned(),
            });
        }
        Ok(binding)
    }
}

/// Describe one VCE-owned registration that follows the common host AI contract.
/// 描述一项遵循公共宿主 AI 契约、但由 VCE 独立持有的注册。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceAiBinding {
    /// Stable registration identity allocated by the host.
    /// 由宿主分配的稳定注册标识。
    pub registration_id: String,
    /// Authorized consumer profile used for provider routing and quotas.
    /// 用于 Provider 路由和配额控制的已授权消费配置标识。
    pub consumer_profile_id: String,
    /// Logical vector-space identity shared by the indexed vectors.
    /// 与已建立索引向量共享的逻辑向量空间标识。
    pub vector_space_id: String,
}

impl VceAiBinding {
    /// Build and validate one fixed AI registration binding.
    /// 构建并校验一次固定的 AI 注册绑定。
    pub fn new(
        registration_id: impl Into<String>,
        consumer_profile_id: impl Into<String>,
        vector_space_id: impl Into<String>,
    ) -> Result<Self, VcePortError> {
        let binding = Self {
            registration_id: registration_id.into(),
            consumer_profile_id: consumer_profile_id.into(),
            vector_space_id: vector_space_id.into(),
        };
        if binding.registration_id.trim().is_empty()
            || binding.consumer_profile_id.trim().is_empty()
            || binding.vector_space_id.trim().is_empty()
        {
            return Err(VcePortError::Rejected {
                operation: VcePortOperation::Embedding,
                reason: "AI binding identifiers must not be empty".to_owned(),
            });
        }
        Ok(binding)
    }
}

/// Report the host capability state without exposing provider implementation details.
/// 报告宿主能力状态，但不暴露 Provider 的具体实现细节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceAiCapabilitySnapshot {
    /// Fixed registration identity visible to diagnostics.
    /// 诊断可见的固定注册标识。
    pub registration_id: String,
    /// Consumer profile routed by the host.
    /// 宿主实际路由使用的消费配置。
    pub consumer_profile_id: String,
    /// Logical vector-space identity served by this capability.
    /// 此能力服务的逻辑向量空间标识。
    pub vector_space_id: String,
    /// Whether the common registration is active.
    /// 共同注册是否处于活动状态。
    pub registered: bool,
    /// Whether document and query Embedding calls are available.
    /// 代码文档和查询向量调用是否可用。
    pub embedding_available: bool,
    /// Whether query rewrite calls are available.
    /// 查询改写调用是否可用。
    pub query_rewrite_available: bool,
    /// Whether Rerank calls are available.
    /// 重排调用是否可用。
    pub rerank_available: bool,
}

/// Identify the semantic purpose of one call on the VCE-owned AI registration.
/// 标识 VCE 独立持有的 AI 注册上的一次调用语义用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VceAiPurpose {
    /// Embed a source code chunk.
    /// 对源代码块进行向量化。
    CodeDocumentEmbedding,
    /// Embed the auxiliary path document.
    /// 对辅助路径文档进行向量化。
    CodePathEmbedding,
    /// Embed a natural-language retrieval query for content search.
    /// 对内容检索的自然语言查询进行向量化。
    CodeQueryEmbedding,
    /// Embed a natural-language retrieval query for path search.
    /// 对路径检索的自然语言查询进行向量化。
    CodePathQueryEmbedding,
    /// Ask the registered LLM to expand or rewrite a retrieval query.
    /// 请求已注册的 LLM 扩展或改写检索查询。
    CodeQueryRewrite,
    /// Ask the registered Rerank capability to order candidates.
    /// 请求已注册的重排能力对候选排序。
    CodeCandidateRerank,
}

impl VceAiPurpose {
    /// Return the stable purpose identifier passed to host routing and telemetry.
    /// 返回传递给宿主路由和遥测系统的稳定用途标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CodeDocumentEmbedding => "code_retrieval.document_embedding",
            Self::CodePathEmbedding => "code_retrieval.path_embedding",
            Self::CodeQueryEmbedding => "code_retrieval.query_embedding",
            Self::CodePathQueryEmbedding => "code_retrieval.path_query_embedding",
            Self::CodeQueryRewrite => "code_retrieval.query_rewrite",
            Self::CodeCandidateRerank => "code_retrieval.candidate_rerank",
        }
    }
}

/// Carry the common identity fields required for one AI call.
/// 携带一次 AI 调用所需的共同身份字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceAiRequestContext {
    /// Stable operation identity supplied by VulcanCode.
    /// 由 VulcanCode 提供的稳定操作标识。
    pub operation_id: String,
    /// Fixed host registration identity selected for all VCE AI operations.
    /// 为全部 VCE AI 操作选择的固定宿主注册身份。
    pub registration_id: String,
    /// Semantic purpose used within the VCE registration.
    /// VCE 注册内部使用的语义用途。
    pub purpose: VceAiPurpose,
    /// Consumer profile that must match the registered capability.
    /// 必须与已注册能力匹配的消费配置。
    pub consumer_profile_id: String,
    /// Vector space used by Embedding-related operations.
    /// 向量相关操作使用的向量空间。
    pub vector_space_id: String,
}

impl VceAiRequestContext {
    /// Construct a request context from the fixed host binding.
    /// 根据固定宿主绑定构造请求上下文。
    pub fn new(
        operation_id: impl Into<String>,
        purpose: VceAiPurpose,
        binding: &VceAiBinding,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            registration_id: binding.registration_id.clone(),
            purpose,
            consumer_profile_id: binding.consumer_profile_id.clone(),
            vector_space_id: binding.vector_space_id.clone(),
        }
    }
}

/// Request one provider-sized Embedding batch from the shared host capability.
/// 通过共享宿主能力请求一个 Provider 批次的向量化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceEmbeddingRequest {
    /// Common host routing and correlation context.
    /// 宿主路由和关联使用的共同上下文。
    pub context: VceAiRequestContext,
    /// Ordered segment batch whose identities must be preserved in the response.
    /// 必须在响应中保持身份对齐的有序分段批次。
    pub batch: EmbeddingBatch,
}

/// Response for one validated Embedding batch.
/// 一个已校验向量批次的响应。
#[derive(Debug, Clone, PartialEq)]
pub struct VceEmbeddingResponse {
    /// Operation identity copied from the request.
    /// 从请求复制的操作标识。
    pub operation_id: String,
    /// Vectors aligned to the requested batch segment identities.
    /// 与请求批次分段身份对齐的向量。
    pub vectors: Vec<SegmentVector>,
    /// Logical and physical compatibility metadata for the returned vectors.
    /// 返回向量的逻辑和物理兼容性元数据。
    pub space: EmbeddingSpace,
}

/// Request an optional LLM query rewrite through the same VCE registration.
/// 通过同一项 VCE 注册请求可选的 LLM 查询改写。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceQueryRewriteRequest {
    /// Common host routing and correlation context.
    /// 宿主路由和关联使用的共同上下文。
    pub context: VceAiRequestContext,
    /// Original normalized query that must remain in the final plan.
    /// 必须保留在最终计划中的原始规范化查询。
    pub query: String,
    /// Maximum number of rewrites requested from the LLM.
    /// 请求 LLM 返回的最大改写数量。
    pub max_rewrites: usize,
}

/// Response containing optional query rewrites from the host LLM.
/// 包含宿主 LLM 可选查询改写结果的响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceQueryRewriteResponse {
    /// Operation identity copied from the request.
    /// 从请求复制的操作标识。
    pub operation_id: String,
    /// Candidate rewrites; the engine filters duplicates and empty values.
    /// 候选改写文本，由引擎过滤重复项和空值。
    pub rewrites: Vec<String>,
}

/// Request optional candidate Rerank through the same VCE registration.
/// 通过同一项 VCE 注册请求可选的候选结果重排。
#[derive(Debug, Clone, PartialEq)]
pub struct VceRerankRequest {
    /// Common host routing and correlation context.
    /// 宿主路由和关联使用的共同上下文。
    pub context: VceAiRequestContext,
    /// Original query supplied to the Rerank operation.
    /// 提交给重排操作的原始查询。
    pub query: String,
    /// Fused candidates in the exact index order used by the response.
    /// 按响应索引顺序提供的融合候选结果。
    pub candidates: Vec<SearchHit>,
    /// Maximum number of candidates the host should prioritize.
    /// 宿主应优先排序的最大候选数量。
    pub top_n: usize,
}

/// Response containing a partial or complete candidate index order.
/// 包含部分或完整候选索引顺序的响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceRerankResponse {
    /// Operation identity copied from the request.
    /// 从请求复制的操作标识。
    pub operation_id: String,
    /// Candidate indexes in descending reranker preference.
    /// 按重排偏好从高到低排列的候选下标。
    pub ordered_candidate_indexes: Vec<usize>,
}

impl VceRerankResponse {
    /// Validate that every returned candidate index is unique and in range.
    /// 校验返回的每个候选下标都唯一且处于有效范围内。
    pub fn validate_for(
        &self,
        operation_id: &str,
        candidate_count: usize,
    ) -> Result<(), VcePortError> {
        if self.operation_id != operation_id {
            return Err(VcePortError::InvalidResponse {
                operation: VcePortOperation::Rerank,
                reason: format!(
                    "response operation id `{}` does not match `{operation_id}`",
                    self.operation_id
                ),
            });
        }
        let mut seen = BTreeSet::new();
        for index in &self.ordered_candidate_indexes {
            if *index >= candidate_count {
                return Err(VcePortError::InvalidResponse {
                    operation: VcePortOperation::Rerank,
                    reason: format!("candidate index {index} is outside {candidate_count} items"),
                });
            }
            if !seen.insert(*index) {
                return Err(VcePortError::InvalidResponse {
                    operation: VcePortOperation::Rerank,
                    reason: format!("candidate index {index} is duplicated"),
                });
            }
        }
        Ok(())
    }
}

/// Describe one immutable source document expected by VCE during index preparation.
/// 描述 VCE 在索引准备期间期望读取的一份不可变源码文档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceSourceExpectation {
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Blob identity already calculated for the expected source version.
    /// 已为预期源码版本计算出的 Blob 身份。
    pub blob_name: String,
    /// Expected source size in bytes.
    /// 预期源码字节大小。
    pub size: u64,
    /// Expected source modification timestamp when available.
    /// 可用时的预期源码修改时间。
    pub mtime_ns: Option<u128>,
}

/// Request immutable source text without granting VCE filesystem ownership.
/// 请求不可变源码文本，但不向 VCE 授予文件系统所有权。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceSourceReadRequest {
    /// Stable operation identity for this bounded read batch.
    /// 此有界读取批次使用的稳定操作标识。
    pub operation_id: String,
    /// Exact source identities that the host must return.
    /// 宿主必须返回的精确源码身份集合。
    pub expectations: Vec<VceSourceExpectation>,
}

/// One host-owned immutable source snapshot returned to VCE.
/// 宿主返回给 VCE 的一份不可变源码快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceSourceDocument {
    /// Canonical workspace-relative path.
    /// 规范化的工作区相对路径。
    pub path: String,
    /// Verbatim UTF-8 source text.
    /// 原始 UTF-8 源码文本。
    pub content: String,
    /// Source size observed by the host.
    /// 宿主观察到的源码字节大小。
    pub size: u64,
    /// Source modification timestamp observed by the host.
    /// 宿主观察到的源码修改时间。
    pub mtime_ns: Option<u128>,
}

/// Response containing an exact bounded source snapshot batch.
/// 包含精确有界源码快照批次的响应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceSourceReadResponse {
    /// Operation identity copied from the request.
    /// 从请求复制的操作标识。
    pub operation_id: String,
    /// Documents corresponding exactly to the requested expectations.
    /// 与请求预期项精确对应的文档。
    pub documents: Vec<VceSourceDocument>,
}

/// Request missing-state classification for a bounded set of Blob identities.
/// 请求对一组有界 Blob 身份进行缺失状态分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindMissingRequest {
    /// Stable refresh operation identity.
    /// 稳定的刷新操作标识。
    pub request_id: String,
    /// Exact Blob set that the host is allowed to inspect.
    /// 宿主被允许检查的精确 Blob 集合。
    pub blob_names: BTreeSet<String>,
}

/// Storage classification of requested Blobs.
/// 存储对所请求 Blob 的分类结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindMissingResponse {
    /// Operation identity copied from the request.
    /// 从请求复制的操作标识。
    pub operation_id: String,
    /// Blobs unknown to the storage layer.
    /// 存储层完全未知的 Blob。
    pub unknown: BTreeSet<String>,
    /// Blobs known to storage but not semantically indexed.
    /// 存储已知但尚未建立语义索引的 Blob。
    pub nonindexed: BTreeSet<String>,
}

/// Stage a bounded set of complete Blob replacements before checkpoint publication.
/// 在发布检查点前暂存一组有界的完整 Blob 替换记录。
#[derive(Debug, Clone, PartialEq)]
pub struct IndexStageBatch {
    /// Stable operation identity for this physical staging batch.
    /// 此物理暂存批次使用的稳定操作标识。
    pub operation_id: String,
    /// Stable logical refresh identity shared by every batch and publication.
    /// 所有批次和发布操作共享的稳定逻辑刷新身份。
    pub refresh_id: String,
    /// Workspace generation represented by the staged records.
    /// 暂存记录所代表的工作区代数。
    pub generation: u64,
    /// Complete Blob replacements contained in this bounded batch.
    /// 此有界批次中包含的完整 Blob 替换记录。
    pub blobs: Vec<IndexedBlob>,
}

/// Confirm one idempotent staging batch before VCE releases its memory.
/// 在 VCE 释放内存前确认一次幂等暂存批次。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStageReceipt {
    /// Operation identity copied from the batch request.
    /// 从批次请求复制的操作标识。
    pub operation_id: String,
    /// Logical refresh identity copied from the batch request.
    /// 从批次请求复制的逻辑刷新身份。
    pub refresh_id: String,
    /// Workspace generation accepted by storage.
    /// 存储接受的工作区代数。
    pub generation: u64,
    /// Exact Blob identities accepted by this batch.
    /// 此批次接受的精确 Blob 身份。
    pub staged_blob_names: Vec<String>,
}

/// Abort all unpublished staging records for one logical refresh.
/// 中止一次逻辑刷新中全部尚未发布的暂存记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexAbortRequest {
    /// Stable abort operation identity.
    /// 稳定的中止操作标识。
    pub operation_id: String,
    /// Logical refresh whose unpublished staging records must be removed.
    /// 必须删除未发布暂存记录的逻辑刷新身份。
    pub refresh_id: String,
    /// Workspace generation associated with the staging transaction.
    /// 与暂存事务关联的工作区代数。
    pub generation: u64,
}

/// Confirm that one staging transaction is no longer publishable.
/// 确认一次暂存事务已无法再发布。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexAbortReceipt {
    /// Operation identity copied from the abort request.
    /// 从中止请求复制的操作标识。
    pub operation_id: String,
    /// Logical refresh identity copied from the abort request.
    /// 从中止请求复制的逻辑刷新身份。
    pub refresh_id: String,
    /// Workspace generation copied from the abort request.
    /// 从中止请求复制的工作区代数。
    pub generation: u64,
    /// Number of staged Blob records removed by the host.
    /// 宿主删除的暂存 Blob 记录数量。
    pub removed_blob_count: usize,
}

/// Publish one fully staged atomic index commit through the host storage adapter.
/// 通过宿主存储适配器发布一份已完整暂存的原子索引提交。
#[derive(Debug, Clone, PartialEq)]
pub struct IndexCommit {
    /// Stable refresh operation identity.
    /// 稳定的刷新操作标识。
    pub request_id: String,
    /// Workspace generation that produced this commit.
    /// 生成此提交的工作区代数。
    pub generation: u64,
    /// Stable logical refresh identity that owns all referenced staging records.
    /// 持有全部引用暂存记录的稳定逻辑刷新身份。
    pub refresh_id: String,
    /// Optional source revision observed by the host.
    /// 宿主观察到的可选源版本。
    pub source_revision: Option<String>,
    /// Blob identities that must be removed before checkpoint publication.
    /// 在发布检查点前必须移除的 Blob 身份。
    pub deleted_blobs: BTreeSet<String>,
    /// Exact staged Blob identities that must become visible.
    /// 必须变为可见的精确暂存 Blob 身份。
    pub staged_blob_names: BTreeSet<String>,
    /// Previously published checkpoint version required by compare-and-swap.
    /// compare-and-swap 要求的先前已发布检查点版本。
    pub expected_checkpoint_version: Option<u64>,
    /// Checkpoint to publish atomically with the data writes.
    /// 与数据写入一起原子发布的检查点。
    pub checkpoint: Checkpoint,
    /// Successful engine state that must become durable in the same transaction as the checkpoint.
    /// 必须与检查点在同一事务中持久化的成功引擎状态。
    pub state_snapshot: VceStateSnapshot,
}

/// Receipt returned after the host atomically publishes an index commit.
/// 宿主原子发布索引提交后返回的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    /// Stable commit operation identity copied from the request.
    /// 从请求复制的稳定提交操作标识。
    pub operation_id: String,
    /// Stable logical refresh identity copied from the commit.
    /// 从提交请求复制的稳定逻辑刷新身份。
    pub refresh_id: String,
    /// Checkpoint chain identity published by the commit.
    /// 此次提交发布的检查点链身份。
    pub checkpoint_chain_id: String,
    /// Workspace generation published by the commit.
    /// 此次提交发布的工作区代数。
    pub generation: u64,
    /// Checkpoint version published by the commit.
    /// 此次提交发布的检查点版本。
    pub checkpoint_version: u64,
    /// Number of Blob writes accepted by the host.
    /// 宿主接受的 Blob 写入数量。
    pub indexed_count: usize,
    /// Number of Blob deletions accepted by the host.
    /// 宿主接受的 Blob 删除数量。
    pub deleted_count: usize,
    /// Stable Blob identities accepted by the host.
    /// 宿主接受的稳定 Blob 身份列表。
    pub indexed_blob_names: Vec<String>,
    /// Stable Blob identities deleted by the host.
    /// 宿主删除的稳定 Blob 身份列表。
    pub deleted_blob_names: Vec<String>,
}

/// Describe the semantic state of one Blob written to host storage.
/// 描述写入宿主存储的一个 Blob 的语义状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexedBlobStatus {
    /// Chunks and vectors are ready for semantic retrieval.
    /// 代码块和向量已准备好用于语义检索。
    Ready,
    /// The Blob is retained as metadata but intentionally has no semantic vector.
    /// 保留 Blob 元数据，但按规则不建立语义向量。
    ReadyWithoutEmbedding {
        /// Explicit admission reason for management and diagnostics.
        /// 用于管理和诊断的明确准入原因。
        reason: String,
    },
}

/// Describe one auxiliary index channel independently from primary content readiness.
/// 独立于主内容就绪状态描述一个辅助索引通道。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuxiliaryIndexStatus {
    /// The auxiliary channel was produced successfully.
    /// 辅助通道已成功生成。
    Ready,
    /// The source did not produce data for this auxiliary channel.
    /// 源码没有为此辅助通道生成数据。
    NotApplicable,
    /// The auxiliary channel failed without invalidating primary content indexing.
    /// 辅助通道失败，但不会使主内容索引失效。
    Failed {
        /// Stable diagnostic retained for management and retry planning.
        /// 为管理和重试计划保留的稳定诊断信息。
        reason: String,
    },
}

/// A host-persistable Blob record produced by the VCE indexing logic.
/// 由 VCE 索引逻辑生成、可交给宿主持久化的 Blob 记录。
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedBlob {
    /// Stable path-plus-content Blob identity.
    /// 稳定的路径加内容 Blob 身份。
    pub blob_name: String,
    /// Canonical workspace-relative source path.
    /// 规范化的工作区相对源代码路径。
    pub path: String,
    /// Snapshot generation that produced this record.
    /// 生成此记录的快照代数。
    pub generation: u64,
    /// Structural analyzer identity that produced the code windows.
    /// 生成代码窗口的结构分析器身份。
    pub structure_analyzer_id: Option<String>,
    /// Structural mode used for this Blob when semantic indexing was applicable.
    /// 语义索引适用时此 Blob 使用的结构模式。
    pub structure_mode: Option<StructureMode>,
    /// Primary structural diagnostic when recursive fallback was required.
    /// 需要递归回退时的主结构分析诊断。
    pub structure_fallback_reason: Option<String>,
    /// Source-preserving code chunks.
    /// 保留源代码的代码块。
    pub chunks: Vec<Chunk>,
    /// One normalized content vector per logical code chunk.
    /// 每个逻辑代码块对应一个归一化内容向量。
    pub embedding_vectors: Vec<EmbeddingVector>,
    /// Auxiliary path document retained separately from content chunks.
    /// 独立于内容代码块保留的辅助路径文档。
    pub path_document: Option<PathDocument>,
    /// Normalized vector for the auxiliary path document.
    /// 辅助路径文档的归一化向量。
    pub path_embedding: Option<EmbeddingVector>,
    /// Exact symbol occurrences produced by VCE for the Blob.
    /// 由 VCE 为此 Blob 生成的精确符号出现记录。
    pub symbols: Vec<SymbolOccurrence>,
    /// Independent readiness of the path-vector channel.
    /// 路径向量通道的独立就绪状态。
    pub path_status: AuxiliaryIndexStatus,
    /// Independent readiness of the exact-symbol channel.
    /// 精确符号通道的独立就绪状态。
    pub symbol_status: AuxiliaryIndexStatus,
    /// Terminal admission/indexing state.
    /// 最终准入或索引状态。
    pub status: IndexedBlobStatus,
}

/// Search request for a dense content vector channel.
/// 内容向量通道的搜索请求。
#[derive(Debug, Clone, PartialEq)]
pub struct DenseSearchRequest {
    /// Stable retrieval operation identity for host correlation.
    /// 用于宿主关联的稳定检索操作标识。
    pub request_id: String,
    /// Query variant index within the planned query list.
    /// 计划查询列表中的查询变体下标。
    pub query_index: usize,
    /// Normalized query vector produced by the shared AI capability.
    /// 由共享 AI 能力生成的归一化查询向量。
    pub query_vector: EmbeddingVector,
    /// Exact Blob scope authorized for this search.
    /// 此次搜索被授权使用的精确 Blob 范围。
    pub allowed_blobs: BTreeSet<String>,
    /// Maximum number of host results requested.
    /// 请求宿主返回的最大结果数量。
    pub limit: usize,
}

/// Search request for the auxiliary path vector channel.
/// 辅助路径向量通道的搜索请求。
#[derive(Debug, Clone, PartialEq)]
pub struct PathSearchRequest {
    /// Stable retrieval operation identity for host correlation.
    /// 用于宿主关联的稳定检索操作标识。
    pub request_id: String,
    /// Query variant index within the planned query list.
    /// 计划查询列表中的查询变体下标。
    pub query_index: usize,
    /// Normalized query vector for path-document search.
    /// 用于路径文档搜索的归一化查询向量。
    pub query_vector: EmbeddingVector,
    /// Exact Blob scope authorized for this search.
    /// 此次搜索被授权使用的精确 Blob 范围。
    pub allowed_blobs: BTreeSet<String>,
    /// Maximum number of path results requested.
    /// 请求返回的最大路径结果数量。
    pub limit: usize,
}

/// Search request for the exact symbol channel.
/// 精确符号通道的搜索请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactSearchRequest {
    /// Stable retrieval operation identity for host correlation.
    /// 用于宿主关联的稳定检索操作标识。
    pub request_id: String,
    /// Identifier texts extracted by the VCE query planner.
    /// VCE 查询规划器提取的标识符文本。
    pub identifiers: Vec<String>,
    /// Exact Blob scope authorized for this search.
    /// 此次搜索被授权使用的精确 Blob 范围。
    pub allowed_blobs: BTreeSet<String>,
    /// Maximum number of exact results requested.
    /// 请求返回的最大精确结果数量。
    pub limit: usize,
}

/// A response from one dense search invocation.
/// 一次内容向量搜索调用的响应。
#[derive(Debug, Clone, PartialEq)]
pub struct VceSearchResponse {
    /// Stable host operation identity copied from the request.
    /// 从请求复制的稳定宿主操作标识。
    pub operation_id: String,
    /// Ranked results in descending host similarity order.
    /// 按宿主相似度从高到低排列的结果。
    pub hits: Vec<SearchHit>,
}

/// A response from one path search invocation.
/// 一次路径搜索调用的响应。
#[derive(Debug, Clone, PartialEq)]
pub struct VcePathSearchResponse {
    /// Stable host operation identity copied from the request.
    /// 从请求复制的稳定宿主操作标识。
    pub operation_id: String,
    /// Path-document hits in descending host similarity order.
    /// 按宿主相似度从高到低排列的路径文档命中。
    pub hits: Vec<SearchHit>,
    /// Optional first-content-chunk fallbacks for uncovered path Blobs.
    /// 用于补齐未覆盖路径 Blob 的可选首个内容代码块。
    pub fallback_hits: Vec<SearchHit>,
}

/// Identify the host-owned index lifecycle state shown by management surfaces.
/// 标识管理页面展示的宿主持有索引生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VceIndexStoreStatus {
    /// No host status has been reported yet.
    /// 宿主尚未报告状态。
    Unknown,
    /// Storage is not currently available.
    /// 存储当前不可用。
    Unavailable,
    /// Storage is accepting or processing refresh work.
    /// 存储正在接受或处理刷新工作。
    Refreshing,
    /// Storage has a searchable committed snapshot.
    /// 存储拥有可搜索的已提交快照。
    Ready,
    /// The last storage operation ended in an observable error.
    /// 最近一次存储操作以可观测错误结束。
    Error,
}

/// Aggregate counts reported by the host index store.
/// 宿主索引存储报告的聚合统计数量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VceIndexCounts {
    /// Number of retained file records.
    /// 保留的文件记录数量。
    pub files: u64,
    /// Number of files eligible for semantic retrieval.
    /// 可参与语义检索的文件数量。
    pub searchable_files: u64,
    /// Number of intentionally skipped files.
    /// 按准入规则主动跳过的文件数量。
    pub skipped_files: u64,
    /// Number of deleted file records retained for reconciliation.
    /// 为协调保留的已删除文件记录数量。
    pub deleted_files: u64,
    /// Number of committed Blob records.
    /// 已提交 Blob 记录数量。
    pub blobs: u64,
    /// Number of persisted content chunks.
    /// 已持久化内容代码块数量。
    pub chunks: u64,
    /// Number of persisted content and path vectors.
    /// 已持久化内容和路径向量数量。
    pub vectors: u64,
    /// Number of persisted path documents.
    /// 已持久化路径文档数量。
    pub path_documents: u64,
    /// Number of exact symbol occurrences.
    /// 精确符号出现记录数量。
    pub symbols: u64,
}

/// Integrity counters used by a management page or diagnostics panel.
/// 管理页面或诊断面板使用的完整性统计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VceIndexIntegrity {
    /// Whether the host index namespace exists.
    /// 宿主索引命名空间是否存在。
    pub index_exists: bool,
    /// Whether the committed namespace is searchable.
    /// 已提交命名空间是否可搜索。
    pub searchable: bool,
    /// Number of checkpoint members expected by the host.
    /// 宿主预期的检查点成员数量。
    pub checkpoint_blob_count: u64,
    /// Number of Blob records found in the host index.
    /// 宿主索引中实际找到的 Blob 记录数量。
    pub indexed_blob_count: u64,
    /// Number of checkpoint members missing from the index.
    /// 索引中缺少的检查点成员数量。
    pub missing_blob_count: u64,
    /// Number of indexed Blobs not present in the checkpoint.
    /// 不在检查点中的多余索引 Blob 数量。
    pub orphan_blob_count: u64,
    /// Optional host-provided integrity detail.
    /// 宿主提供的可选完整性详情。
    pub detail: Option<String>,
}

/// One host-reported indexing job summary for status pages.
/// 状态页面使用的一条宿主索引作业摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceIndexJobSummary {
    /// Stable host runtime job identity.
    /// 宿主运行时作业稳定标识。
    pub job_id: String,
    /// Typed operation kind represented by this job.
    /// 此作业代表的类型化操作种类。
    pub kind: VceOperationKind,
    /// Workspace generation being processed.
    /// 正在处理的工作区代数。
    pub generation: u64,
    /// Refresh mode when this job is an initial build or incremental refresh.
    /// 此作业为初次构建或增量刷新时使用的刷新模式。
    pub mode: Option<RefreshMode>,
    /// Refresh reason when this job is an initial build or incremental refresh.
    /// 此作业为初次构建或增量刷新时使用的刷新原因。
    pub reason: Option<RefreshReason>,
    /// Typed processing phase shared by management surfaces.
    /// 管理界面共享的类型化处理阶段。
    pub phase: VceOperationPhase,
    /// One-based execution attempt for retry diagnostics.
    /// 用于重试诊断的一基执行尝试次数。
    pub attempt: u32,
    /// Completed units reported by the host.
    /// 宿主报告的已完成单元数。
    pub completed_units: u64,
    /// Total units reported by the host.
    /// 宿主报告的总单元数。
    pub total_units: u64,
    /// Optional current unit description.
    /// 可选的当前处理单元说明。
    pub unit: Option<String>,
    /// Optional host timestamp for job start.
    /// 可选的宿主作业开始时间戳。
    pub started_at_ns: Option<u128>,
    /// Optional host timestamp for job completion.
    /// 可选的宿主作业完成时间戳。
    pub finished_at_ns: Option<u128>,
    /// Whether the host coalesced another refresh into this job.
    /// 宿主是否将其他刷新合并进此作业。
    pub coalesced: bool,
    /// Optional terminal error exposed by the host.
    /// 宿主暴露的可选终止错误。
    pub error: Option<String>,
}

/// Host-owned index data required by VCE management and page projections.
/// VCE 管理和页面投影所需的宿主索引数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceIndexManagementSnapshot {
    /// Operation identity copied from the management request.
    /// 从管理请求复制的操作标识。
    pub operation_id: String,
    /// Current host index lifecycle state.
    /// 当前宿主索引生命周期状态。
    pub status: VceIndexStoreStatus,
    /// Generation represented by the host index.
    /// 宿主索引代表的工作区代数。
    pub generation: u64,
    /// Published checkpoint version, when available.
    /// 已发布检查点版本（如果可用）。
    pub checkpoint_version: Option<u64>,
    /// Aggregate index counts.
    /// 索引聚合统计数量。
    pub counts: VceIndexCounts,
    /// Integrity state for the current namespace.
    /// 当前命名空间的完整性状态。
    pub integrity: VceIndexIntegrity,
    /// Active or last host indexing job.
    /// 当前或最近一次宿主索引作业。
    pub active_job: Option<VceIndexJobSummary>,
    /// Last host storage error, if any.
    /// 最近一次宿主存储错误（如果有）。
    pub last_error: Option<String>,
}

/// Request a read-only management snapshot from host storage.
/// 请求宿主存储返回只读管理快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VceManagementRequest {
    /// Stable operation identity for host diagnostics.
    /// 用于宿主诊断的稳定操作标识。
    pub request_id: String,
}

/// Errors returned by an injected AI or storage port.
/// 注入式 AI 或存储端口返回的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VcePortError {
    /// The requested operation is not currently available.
    /// 请求的操作当前不可用。
    Unavailable {
        /// Host operation that is not currently registered or available.
        /// 当前未注册或不可用的宿主操作。
        operation: VcePortOperation,
    },
    /// The host rejected the request before execution.
    /// 宿主在执行前拒绝了请求。
    Rejected {
        /// Operation rejected by the host.
        /// 被宿主拒绝的操作。
        operation: VcePortOperation,
        /// Explicit rejection reason.
        /// 明确的拒绝原因。
        reason: String,
    },
    /// The host operation failed after it was accepted.
    /// 宿主接受请求后操作失败。
    Failed {
        /// Failed host operation.
        /// 失败的宿主操作。
        operation: VcePortOperation,
        /// Explicit host failure message.
        /// 宿主提供的明确失败信息。
        message: String,
    },
    /// The host operation failed transiently and may be retried after the supplied delay.
    /// 宿主操作发生瞬时失败，可在给定延迟后重试。
    Retryable {
        /// Failed host operation.
        /// 失败的宿主操作。
        operation: VcePortOperation,
        /// Explicit host failure message.
        /// 宿主提供的明确失败信息。
        message: String,
        /// Optional provider- or host-supplied delay in milliseconds.
        /// 可选的供应商或宿主指定毫秒延迟。
        retry_after_ms: Option<u64>,
    },
    /// The host returned data that violates the VCE contract.
    /// 宿主返回的数据违反 VCE 契约。
    InvalidResponse {
        /// Operation that produced the invalid response.
        /// 产生无效响应的操作。
        operation: VcePortOperation,
        /// Explicit contract violation detail.
        /// 明确的契约违反详情。
        reason: String,
    },
}

impl Display for VcePortError {
    /// Format an explicit host-boundary error for diagnostics.
    /// 将明确的宿主边界错误格式化为诊断文本。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable { operation } => {
                write!(
                    formatter,
                    "VCE port operation `{}` is unavailable",
                    operation.as_str()
                )
            }
            Self::Rejected { operation, reason } => write!(
                formatter,
                "VCE port operation `{}` was rejected: {reason}",
                operation.as_str()
            ),
            Self::Failed { operation, message } => write!(
                formatter,
                "VCE port operation `{}` failed: {message}",
                operation.as_str()
            ),
            Self::Retryable {
                operation,
                message,
                retry_after_ms,
            } => match retry_after_ms {
                Some(delay) => write!(
                    formatter,
                    "VCE port operation `{}` failed transiently (retry after {delay} ms): {message}",
                    operation.as_str()
                ),
                None => write!(
                    formatter,
                    "VCE port operation `{}` failed transiently: {message}",
                    operation.as_str()
                ),
            },
            Self::InvalidResponse { operation, reason } => write!(
                formatter,
                "VCE port operation `{}` returned an invalid response: {reason}",
                operation.as_str()
            ),
        }
    }
}

impl Error for VcePortError {}

/// Refresh mode selected by the host watcher or manual manager action.
/// 由宿主监听器或手动管理操作选择的刷新模式。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum RefreshMode {
    /// The observations are the authoritative complete workspace listing.
    /// 观察结果是权威的完整工作区文件列表。
    Full,
    /// Only the supplied observations and removed paths changed.
    /// 只有提供的观察路径和删除路径发生了变化。
    Paths,
}

/// Reason recorded for one workspace refresh.
/// 记录一次工作区刷新的原因。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum RefreshReason {
    /// Initial host synchronization.
    /// 宿主首次同步。
    Initial,
    /// A file watcher reported local filesystem changes.
    /// 文件监听器报告了本地文件系统变化。
    FileWatcher,
    /// A Git watcher reported repository changes after the host rescanned paths.
    /// Git 监听器报告仓库变化且宿主已重新扫描路径。
    GitWatcher,
    /// An explicit user or manager refresh.
    /// 用户或管理器显式触发刷新。
    Manual,
    /// Recovery after an earlier failed or interrupted job.
    /// 之前作业失败或中断后的恢复刷新。
    Recovery,
}

/// A functional refresh request, independent of any watcher transport.
/// 与监听器传输无关的功能刷新请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshRequest {
    /// Stable refresh operation identity supplied by the host.
    /// 由宿主提供的稳定刷新操作标识。
    pub request_id: String,
    /// Whether the observations are complete or incremental.
    /// 观察结果是完整列表还是增量列表。
    pub mode: RefreshMode,
    /// Why the host requested the refresh.
    /// 宿主请求刷新的原因。
    pub reason: RefreshReason,
    /// Whether every present Blob must be rebuilt even when its identity is already indexed.
    /// 是否必须重建每个现存 Blob，即使其身份已经建立索引。
    pub force_reindex: bool,
    /// File observations read by the host and supplied to VCE.
    /// 宿主读取后提供给 VCE 的文件观察结果。
    pub observations: Vec<FileObservation>,
    /// Paths the host has confirmed as removed in an incremental refresh.
    /// 增量刷新中由宿主确认已删除的路径。
    pub removed_paths: Vec<String>,
    /// Optional repository or workspace source revision.
    /// 可选的仓库或工作区源版本。
    pub source_revision: Option<String>,
}

impl RefreshRequest {
    /// Construct a complete authoritative workspace refresh request.
    /// 构造一次完整且权威的工作区刷新请求。
    pub fn full(
        request_id: impl Into<String>,
        reason: RefreshReason,
        observations: Vec<FileObservation>,
        source_revision: Option<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            mode: RefreshMode::Full,
            reason,
            force_reindex: false,
            observations,
            removed_paths: Vec::new(),
            source_revision,
        }
    }

    /// Construct a complete refresh that rebuilds every present Blob behind one atomic checkpoint.
    /// 构造一次在单个原子检查点之后重建全部现存 Blob 的完整刷新。
    ///
    /// # Parameters
    /// 参数
    /// `request_id` is the stable host-owned operation identity.
    /// `request_id` 是稳定的宿主持有操作身份。
    /// `reason` records why the host requested the rebuild.
    /// `reason` 记录宿主请求重建的原因。
    /// `observations` is the complete authoritative workspace manifest.
    /// `observations` 是完整权威工作区清单。
    /// `source_revision` is the optional host source revision.
    /// `source_revision` 是可选宿主源码修订。
    ///
    /// # Returns
    /// 返回值
    /// A validated full-scope request with forced reindexing enabled.
    /// 返回启用强制重建的完整作用域请求。
    pub fn full_rebuild(
        request_id: impl Into<String>,
        reason: RefreshReason,
        observations: Vec<FileObservation>,
        source_revision: Option<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            mode: RefreshMode::Full,
            reason,
            force_reindex: true,
            observations,
            removed_paths: Vec::new(),
            source_revision,
        }
    }

    /// Construct an incremental refresh request for changed and removed paths.
    /// 构造一次包含变化路径和删除路径的增量刷新请求。
    pub fn paths(
        request_id: impl Into<String>,
        reason: RefreshReason,
        observations: Vec<FileObservation>,
        removed_paths: Vec<String>,
        source_revision: Option<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            mode: RefreshMode::Paths,
            reason,
            force_reindex: false,
            observations,
            removed_paths,
            source_revision,
        }
    }
}

/// Identify which workspace scope a retrieval request may use.
/// 标识一次检索请求允许使用的工作区范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalScope {
    /// Use exactly the last committed VCE checkpoint.
    /// 仅使用 VCE 最近一次已提交检查点。
    CurrentCheckpoint,
    /// Use exactly the supplied Blob identities.
    /// 仅使用调用方提供的精确 Blob 身份集合。
    Blobs(BTreeSet<String>),
}

/// Functional retrieval request accepted by the VCE engine.
/// VCE 引擎接受的功能检索请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveRequest {
    /// Stable retrieval operation identity supplied by the host.
    /// 由宿主提供的稳定检索操作标识。
    pub request_id: String,
    /// Natural-language description of the requested code context.
    /// 所需代码上下文的自然语言描述。
    pub information_request: String,
    /// Explicitly authorized retrieval scope.
    /// 明确授权的检索范围。
    pub scope: RetrievalScope,
    /// Whether the registered LLM may rewrite the query.
    /// 是否允许已注册的 LLM 改写查询。
    pub allow_query_rewrite: bool,
    /// Whether the registered Rerank capability may reorder candidates.
    /// 是否允许已注册的重排能力重新排列候选。
    pub allow_rerank: bool,
}

impl RetrieveRequest {
    /// Construct a retrieval request against the last committed checkpoint.
    /// 构造一次针对最近已提交检查点的检索请求。
    ///
    /// Query rewrite starts disabled to avoid an unconditional LLM call; callers may opt in.
    /// 查询改写默认关闭以避免无条件 LLM 调用；调用方可以显式启用。
    pub fn current_checkpoint(
        request_id: impl Into<String>,
        information_request: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            information_request: information_request.into(),
            scope: RetrievalScope::CurrentCheckpoint,
            allow_query_rewrite: false,
            allow_rerank: true,
        }
    }

    /// Construct a retrieval request against an explicit Blob scope.
    /// 构造一次针对明确 Blob 范围的检索请求。
    ///
    /// Query rewrite starts disabled to avoid an unconditional LLM call; callers may opt in.
    /// 查询改写默认关闭以避免无条件 LLM 调用；调用方可以显式启用。
    pub fn in_blobs(
        request_id: impl Into<String>,
        information_request: impl Into<String>,
        blob_names: BTreeSet<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            information_request: information_request.into(),
            scope: RetrievalScope::Blobs(blob_names),
            allow_query_rewrite: false,
            allow_rerank: true,
        }
    }
}

/// Supply one fixed AI registration and all AI operations used by VCE.
/// 提供一次固定 AI 注册以及 VCE 使用的全部 AI 操作。
pub trait VceAiPort: Send + Sync {
    /// Return the fixed host registration used for every VCE AI call.
    /// 返回所有 VCE AI 调用共同使用的固定宿主注册。
    fn binding(&self) -> &VceAiBinding;

    /// Return a management-safe capability snapshot.
    /// 返回可安全用于管理页面的能力快照。
    fn capability_snapshot(&self) -> VceAiCapabilitySnapshot;

    /// Return the provider-declared maximum number of inputs per Embedding request when known.
    /// 返回已知时供应商声明的单次向量请求最大输入数。
    fn embedding_max_inputs(&self) -> Option<usize> {
        None
    }

    /// Execute one Embedding batch through the host-registered capability.
    /// 通过宿主注册能力执行一个向量化批次。
    fn embed(
        &self,
        request: VceEmbeddingRequest,
    ) -> VceFuture<Result<VceEmbeddingResponse, VcePortError>>;

    /// Execute one optional LLM query rewrite through the same VCE host registration.
    /// 通过同一项 VCE 宿主注册执行一次可选 LLM 查询改写。
    fn rewrite_query(
        &self,
        request: VceQueryRewriteRequest,
    ) -> VceFuture<Result<VceQueryRewriteResponse, VcePortError>>;

    /// Execute one optional candidate Rerank through the same VCE host registration.
    /// 通过同一项 VCE 宿主注册执行一次可选候选重排。
    fn rerank(
        &self,
        request: VceRerankRequest,
    ) -> VceFuture<Result<VceRerankResponse, VcePortError>>;
}

/// Supply immutable source snapshots without giving VCE filesystem ownership.
/// 提供不可变源码快照，但不把文件系统所有权交给 VCE。
pub trait VceSourcePort: Send + Sync {
    /// Read exactly the requested source identities or return an explicit stale-source error.
    /// 精确读取请求的源码身份，或返回明确的源码过期错误。
    fn read_sources(
        &self,
        request: VceSourceReadRequest,
    ) -> VceFuture<Result<VceSourceReadResponse, VcePortError>>;
}

/// Supply host-owned Blob persistence, vector search, exact search, and management data.
/// 提供宿主持有的 Blob 持久化、向量搜索、精确搜索和管理数据。
pub trait VceIndexPort: Send + Sync {
    /// Return the immutable namespace and vector contract bound to this port.
    /// 返回绑定到此端口的不可变命名空间和向量契约。
    fn binding(&self) -> &VceIndexBinding;

    /// Classify the exact requested Blobs without widening the request scope.
    /// 在不扩大请求范围的前提下分类精确请求的 Blob。
    fn find_missing(
        &self,
        request: FindMissingRequest,
    ) -> VceFuture<Result<FindMissingResponse, VcePortError>>;

    /// Idempotently stage one bounded batch before atomic publication.
    /// 在原子发布前幂等暂存一个有界批次。
    fn stage(&self, request: IndexStageBatch)
    -> VceFuture<Result<IndexStageReceipt, VcePortError>>;

    /// Abort unpublished staging data after a refresh failure.
    /// 在刷新失败后中止尚未发布的暂存数据。
    fn abort_stage(
        &self,
        request: IndexAbortRequest,
    ) -> VceFuture<Result<IndexAbortReceipt, VcePortError>>;

    /// Atomically publish staged writes, deletions, and the new checkpoint.
    /// 原子发布暂存写入、删除和新的检查点。
    fn commit(&self, request: IndexCommit) -> VceFuture<Result<CommitReceipt, VcePortError>>;

    /// Search the content vector namespace inside the supplied Blob scope.
    /// 在提供的 Blob 范围内搜索内容向量命名空间。
    fn search_dense(
        &self,
        request: DenseSearchRequest,
    ) -> VceFuture<Result<VceSearchResponse, VcePortError>>;

    /// Search the path vector namespace inside the supplied Blob scope.
    /// 在提供的 Blob 范围内搜索路径向量命名空间。
    fn search_path(
        &self,
        request: PathSearchRequest,
    ) -> VceFuture<Result<VcePathSearchResponse, VcePortError>>;

    /// Search exact symbols inside the supplied Blob scope.
    /// 在提供的 Blob 范围内搜索精确符号。
    fn search_exact(
        &self,
        request: ExactSearchRequest,
    ) -> VceFuture<Result<VceSearchResponse, VcePortError>>;

    /// Return the host-owned index status, counters, and integrity data.
    /// 返回宿主持有的索引状态、统计数量和完整性数据。
    fn management_snapshot(
        &self,
        request: VceManagementRequest,
    ) -> VceFuture<Result<VceIndexManagementSnapshot, VcePortError>>;

    /// Execute one typed read-only data, history, integrity, or maintenance-plan query.
    /// 执行一项类型化只读数据、历史、完整性或维护计划查询。
    fn management_query(
        &self,
        request: VceManagementQueryRequest,
    ) -> VceFuture<Result<VceManagementQueryResponse, VcePortError>>;
}
