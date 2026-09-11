//! Durable VCE embedding-run metadata with frozen source bodies and no provider secrets.
//! 携带冻结源码正文且不包含供应商密钥的 VCE 向量运行元数据。

use crate::embedding::{EmbeddingBatch, EmbeddingSpace, SegmentVector};
use crate::indexing::PreparedIndex;
use crate::ports::VceAiPurpose;
use crate::structure::StructureMode;
use crate::symbol::SymbolOccurrence;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Maximum serialized bytes accepted for the run envelope without node or batch payloads.
/// 不含节点或批次负载的运行概要允许的最大序列化字节数。
const VCE_EMBEDDING_MANIFEST_ENVELOPE_MAX_BYTES: usize = 64 * 1024;

/// Maximum serialized bytes accepted for one durable provider batch record.
/// 单条持久化供应商批次记录允许的最大序列化字节数。
const VCE_EMBEDDING_BATCH_RECORD_MAX_BYTES: usize = 1024 * 1024;

/// Extra metadata allowance added after scaling the configured source-file limit.
/// 按配置源码文件上限放大后额外预留的元数据字节数。
const VCE_EMBEDDING_NODE_METADATA_ALLOWANCE_BYTES: u64 = 1024 * 1024;

/// Maximum expansion factor for source, Chunk, segment, symbol, and JSON representations.
/// 源码、代码块、分段、符号及 JSON 表示允许的最大放大倍数。
const VCE_EMBEDDING_NODE_SERIALIZATION_EXPANSION: u64 = 32;

/// The only persisted embedding-manifest schema currently accepted by VCE.
/// VCE 当前唯一接受的持久化向量清单结构版本。
pub const VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Durable lifecycle for one provider-independent embedding run.
/// 一个与供应商无关的向量运行的持久化生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VceEmbeddingRunState {
    /// The complete work description has been persisted before provider I/O.
    /// 已在供应商调用前持久化完整工作描述。
    Planned,
    /// At least one provider batch is admitted and execution may resume.
    /// 至少一个供应商批次已准入，执行可以恢复。
    Running,
    /// All vectors are materialized in staging and await atomic publication.
    /// 所有向量已物化到暂存区，等待原子发布。
    CandidateReady,
    /// The run was published into a committed checkpoint.
    /// 运行结果已发布到已提交检查点。
    Succeeded,
    /// The run stopped with an explicit diagnostic.
    /// 运行以明确诊断停止。
    Failed,
    /// The run was cancelled before publication.
    /// 运行在发布前被取消。
    Cancelled,
}

/// Durable state for one bounded provider request batch.
/// 一个有界供应商请求批次的持久化状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VceEmbeddingBatchState {
    /// The batch has not reached the provider.
    /// 批次尚未到达供应商。
    Pending,
    /// The batch is admitted and may be retried by its stable identity.
    /// 批次已准入，可按稳定身份重试。
    Running,
    /// The provider response was validated and retained.
    /// 供应商响应已校验并保留。
    Succeeded,
    /// The batch stopped with an explicit bounded error.
    /// 批次以明确且有界的错误停止。
    Failed,
}

/// Frozen compatibility and batching contract for one embedding run.
/// 一个向量运行固定的兼容性与批次契约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VceEmbeddingRunContract {
    /// Logical and physical vector-space identity captured at planning time.
    /// 在规划时捕获的逻辑与物理向量空间身份。
    pub embedding_space: EmbeddingSpace,
    /// Maximum input count configured by the VCE workspace.
    /// VCE 工作区配置的最大输入条数。
    pub configured_max_inputs: usize,
    /// Provider-advertised input limit, when the capability is explicit.
    /// 供应商明确声明的输入条数上限；未声明时为空。
    pub provider_max_inputs: Option<usize>,
    /// Effective limit actually used to construct every request batch.
    /// 实际用于构建所有请求批次的有效上限。
    pub effective_max_inputs: usize,
    /// Maximum Unicode scalar values per request batch.
    /// 每个请求批次允许的最大 Unicode 标量数量。
    pub max_chars: usize,
    /// Maximum Unicode scalar values per provider segment.
    /// 每个供应商分段允许的最大 Unicode 标量数量。
    pub max_segment_chars: usize,
    /// Unicode scalar overlap between adjacent segments.
    /// 相邻分段之间重叠的 Unicode 标量数量。
    pub overlap_chars: usize,
    /// Maximum source-file size eligible for semantic preparation.
    /// 允许进行语义准备的源文件最大字节数。
    pub max_file_bytes: u64,
    /// Provider-neutral purposes included in this frozen run.
    /// 本次固定运行包含的供应商无关用途。
    pub purposes: Vec<String>,
}

impl VceEmbeddingRunContract {
    /// Validate the frozen limits and purpose set without consulting runtime capabilities.
    /// 不读取运行时能力，直接校验已经冻结的限制和用途集合。
    ///
    /// Validates the receiver without host I/O; returns unit on success or a precise contract error.
    /// 不执行宿主读写地校验当前对象；成功返回空值，失败返回明确契约错误。
    pub fn validate(&self) -> Result<(), String> {
        if !self.embedding_space.is_well_formed()
            || self.configured_max_inputs == 0
            || self.effective_max_inputs == 0
            || self.max_chars == 0
            || self.max_segment_chars == 0
            || self.max_file_bytes == 0
            || self.provider_max_inputs == Some(0)
            || self.provider_max_inputs.is_some_and(|limit| {
                self.effective_max_inputs != self.configured_max_inputs.min(limit)
            })
            || (self.provider_max_inputs.is_none()
                && self.effective_max_inputs != self.configured_max_inputs)
        {
            return Err(String::from(
                "VCE embedding run contract limits are invalid",
            ));
        }
        // Unique purposes used to reject duplicated or unsupported execution channels.
        // 用于拒绝重复或不支持的执行通道的唯一用途集合。
        let purposes = self
            .purposes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if purposes.len() != self.purposes.len()
            || purposes.iter().any(|purpose| {
                ![
                    VceAiPurpose::CodeDocumentEmbedding.as_str(),
                    VceAiPurpose::CodePathEmbedding.as_str(),
                ]
                .contains(purpose)
            })
        {
            return Err(String::from(
                "VCE embedding run contract purposes are invalid",
            ));
        }
        Ok(())
    }
}

/// Immutable code-node facts captured before any embedding request is sent.
/// 在发送任何向量请求前捕获的不可变代码节点事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VceCodeNodeSnapshot {
    /// Stable content Blob identity for the frozen source node.
    /// 稳定的内容 Blob 身份；源码正文在需要时随快照一并保存。
    pub blob_name: String,
    /// Canonical workspace-relative source path.
    /// 规范化的工作区相对源码路径。
    pub path: String,
    /// Workspace generation that admitted this node.
    /// 接收此节点的工作区代次。
    pub generation: u64,
    /// Source byte size observed during reconciliation.
    /// 协调阶段观察到的源码字节数。
    pub source_size: u64,
    /// Optional source modification time observed during reconciliation.
    /// 协调阶段观察到的可选源码修改时间。
    pub mtime_ns: Option<u128>,
    /// Exact UTF-8 source body captured before provider I/O when this node needs preparation.
    /// 对需要准备的节点，在供应商调用前捕获的精确 UTF-8 源码正文。
    #[serde(default)]
    pub source_content: Option<String>,
    /// Complete deterministic preparation artifact used to resume without source re-reading or re-analysis.
    /// 用于在无需重新读取源码或重新分析的情况下恢复的完整确定性准备产物。
    #[serde(default)]
    pub prepared_index: Option<PreparedIndex>,
    /// Structural analyzer identity captured with the prepared artifact.
    /// 与准备产物一同捕获的结构分析器身份。
    #[serde(default)]
    pub structure_analyzer_id: Option<String>,
    /// Structural mode captured with the prepared artifact.
    /// 与准备产物一同捕获的结构模式。
    #[serde(default)]
    pub structure_mode: Option<StructureMode>,
    /// Structural fallback diagnostic captured before provider I/O.
    /// 在供应商调用前捕获的结构回退诊断。
    #[serde(default)]
    pub structure_fallback_reason: Option<String>,
    /// Exact symbol occurrences produced from the same immutable preparation.
    /// 从同一不可变准备产物生成的精确符号出现记录。
    #[serde(default)]
    pub symbols: Vec<SymbolOccurrence>,
    /// Explicit reason when this node is retained without semantic embedding.
    /// 节点保留但不建立语义向量时的明确原因。
    #[serde(default)]
    pub ready_without_embedding_reason: Option<String>,
    /// Deterministic chunk identities that make up the content plan.
    /// 构成内容计划的确定性代码块身份。
    pub chunk_ids: Vec<String>,
    /// Logical content-input identities submitted to the provider plan.
    /// 提交到供应商计划中的逻辑内容输入身份。
    pub content_input_ids: Vec<String>,
    /// Optional logical path-input identity submitted to the path plan.
    /// 提交到路径计划中的可选逻辑路径输入身份。
    pub path_input_id: Option<String>,
}

/// Persisted identity and status for one provider request batch.
/// 一个供应商请求批次的持久化身份和状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VceEmbeddingBatchRecord {
    /// Stable run-local batch identity.
    /// 运行范围内稳定的批次身份。
    pub batch_id: String,
    /// Provider-neutral operation purpose such as content or path embedding.
    /// 与供应商无关的操作用途，例如内容或路径向量。
    pub purpose: String,
    /// Provider input identities in their exact request order.
    /// 按请求精确顺序排列的供应商输入身份。
    pub input_ids: Vec<String>,
    /// Total Unicode scalar count represented by this batch.
    /// 此批次表示的 Unicode 标量总数。
    pub total_chars: usize,
    /// Last durable batch state.
    /// 最近一次持久化的批次状态。
    pub state: VceEmbeddingBatchState,
    /// Number of admitted attempts for this stable batch identity.
    /// 此稳定批次身份已准入的尝试次数。
    pub attempt: u32,
    /// Bounded failure diagnostic, when the batch did not succeed.
    /// 批次未成功时保存的有界失败诊断。
    pub error: Option<String>,
}

impl VceEmbeddingBatchRecord {
    /// Build a pending record directly from an already validated embedding batch.
    /// 根据已校验的向量批次直接构建待处理持久化记录。
    ///
    /// `batch_id` and `purpose` identify `batch`; returns its initial durable record without sending a request.
    /// `batch_id` 与 `purpose` 标识 `batch`；返回初始持久记录，不发送请求。
    pub fn pending(
        batch_id: impl Into<String>,
        purpose: impl Into<String>,
        batch: &EmbeddingBatch,
    ) -> Self {
        Self {
            batch_id: batch_id.into(),
            purpose: purpose.into(),
            input_ids: batch.request_input_ids(),
            total_chars: batch.total_chars,
            state: VceEmbeddingBatchState::Pending,
            attempt: 0,
            error: None,
        }
    }

    /// Validate immutable identity, bounded metadata, and mutable lifecycle consistency.
    /// 校验不可变身份、有界元数据以及可变生命周期一致性。
    ///
    /// Validates the receiver without host I/O; returns unit on success or a precise contract error.
    /// 不执行宿主读写地校验当前对象；成功返回空值，失败返回明确契约错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.batch_id.trim().is_empty()
            || self.purpose.trim().is_empty()
            || self.batch_id.len() > 8192
            || ![
                VceAiPurpose::CodeDocumentEmbedding.as_str(),
                VceAiPurpose::CodePathEmbedding.as_str(),
            ]
            .contains(&self.purpose.as_str())
            || self.input_ids.is_empty()
            || self.total_chars == 0
            || self.attempt > i32::MAX as u32
            || (matches!(
                self.state,
                VceEmbeddingBatchState::Running | VceEmbeddingBatchState::Succeeded
            ) && self.attempt == 0)
            || self.error.as_ref().is_some_and(|error| error.len() > 8192)
        {
            return Err(String::from(
                "VCE embedding manifest batch identity is invalid",
            ));
        }
        // Seen segment identities used to reject duplicate provider inputs.
        // 用于拒绝重复供应商输入的已见分段身份集合。
        let mut input_ids = BTreeSet::new();
        if self.input_ids.iter().any(|input_id| {
            input_id.trim().is_empty() || input_id.len() > 8192 || !input_ids.insert(input_id)
        }) {
            return Err(String::from(
                "VCE embedding manifest batch inputs are invalid",
            ));
        }
        // Whether the mutable state disagrees with the optional failure diagnostic.
        // 可变状态是否与可选失败诊断不一致。
        let state_error_is_invalid = match self.state {
            VceEmbeddingBatchState::Failed => self
                .error
                .as_ref()
                .is_none_or(|error| error.trim().is_empty()),
            VceEmbeddingBatchState::Pending
            | VceEmbeddingBatchState::Running
            | VceEmbeddingBatchState::Succeeded => self.error.is_some(),
        };
        if state_error_is_invalid {
            return Err(String::from(
                "VCE embedding manifest batch state and error disagree",
            ));
        }
        // Serialized representation used for per-record bounds or round-trip verification.
        // 用于单条记录边界或往返校验的序列化表示。
        let encoded = serde_json::to_vec(self)
            .map_err(|error| format!("Encode VCE embedding batch record: {error}"))?;
        if encoded.len() > VCE_EMBEDDING_BATCH_RECORD_MAX_BYTES {
            return Err(String::from(
                "VCE embedding batch record exceeds its metadata bound",
            ));
        }
        Ok(())
    }

    /// Hash only the immutable request identity so lifecycle updates cannot change plan ownership.
    /// 仅哈希不可变请求身份，使生命周期更新不能改变计划归属。
    ///
    /// Returns the receiver's stable SHA-256 request digest, or an error for invalid metadata.
    /// 返回当前对象稳定的 SHA-256 请求摘要；元数据无效时返回错误。
    pub fn identity_fingerprint(&self) -> Result<String, String> {
        self.validate()?;
        sha256_json(
            &VceEmbeddingBatchIdentity {
                batch_id: &self.batch_id,
                purpose: &self.purpose,
                input_ids: &self.input_ids,
                total_chars: self.total_chars,
            },
            "VCE embedding batch identity",
        )
    }

    /// Mark the batch as admitted and increment its durable attempt counter.
    /// 将批次标记为已准入并递增其持久化尝试计数。
    ///
    /// Advances the attempt counter without I/O; returns an error for success replay, missing identity, or counter overflow.
    /// 不执行读写地推进尝试计数；成功批次重放、身份缺失或计数溢出时返回错误。
    pub fn mark_running(&mut self) -> Result<(), String> {
        if self.state == VceEmbeddingBatchState::Succeeded {
            return Err(String::from(
                "VCE embedding batch cannot run again after success",
            ));
        }
        if self.input_ids.is_empty() || self.batch_id.trim().is_empty() {
            return Err(String::from("VCE embedding batch identity is empty"));
        }
        self.attempt = self
            .attempt
            .checked_add(1)
            .ok_or_else(|| String::from("VCE embedding batch attempt overflowed"))?;
        self.state = VceEmbeddingBatchState::Running;
        self.error = None;
        Ok(())
    }

    /// Mark a validated provider response as durable success.
    /// 将已校验的供应商响应标记为持久化成功。
    ///
    /// Clears the previous diagnostic and records success in memory; returns unit without persisting a result.
    /// 清除先前诊断并在内存中记录成功；返回空值，不执行结果持久化。
    pub fn mark_succeeded(&mut self) {
        self.state = VceEmbeddingBatchState::Succeeded;
        self.error = None;
    }

    /// Mark a batch failure while preserving its stable retry identity.
    /// 标记批次失败并保留其稳定重试身份。
    ///
    /// `error` supplies the diagnostic; stores a UTF-8-safe bounded failure while preserving request identity.
    /// `error` 提供诊断；保留请求身份，保存 UTF-8 安全且有界的失败状态。
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.state = VceEmbeddingBatchState::Failed;
        // The storage contract bounds UTF-8 bytes, so truncate only at a complete scalar boundary.
        // 存储契约限制 UTF-8 字节数，因此只在完整字符边界截断。
        let mut diagnostic = error.into();
        // Last complete UTF-8 byte boundary inside the diagnostic byte budget.
        // 诊断字节预算内最后一个完整 UTF-8 字节边界。
        let mut end = diagnostic.len().min(8_192);
        while !diagnostic.is_char_boundary(end) {
            end -= 1;
        }
        diagnostic.truncate(end);
        self.error = Some(diagnostic);
    }
}

/// Durable provider result for one completed embedding batch.
/// 一个已完成向量批次的持久化供应商结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VceEmbeddingBatchResult {
    /// Stable run-local batch identity matching the manifest record.
    /// 与清单记录匹配的运行范围内稳定批次身份。
    pub batch_id: String,
    /// Validated provider vectors retained for recovery before staging.
    /// 在暂存前为恢复保留的已校验供应商向量。
    pub vectors: Vec<VceEmbeddingVectorResult>,
}

/// One serializable segment vector retained outside the source snapshot.
/// 保存在源码快照之外的一条可序列化分段向量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VceEmbeddingVectorResult {
    /// Stable segment request identity.
    /// 稳定的分段请求身份。
    pub input_id: String,
    /// Provider vector components in their returned order.
    /// 按供应商返回顺序保存的向量分量。
    pub values: Vec<f32>,
}

impl VceEmbeddingBatchResult {
    /// Convert validated in-memory vectors into a bounded durable batch result.
    /// 将已校验的内存向量转换为有界持久化批次结果。
    ///
    /// Copies validated `vectors` under `batch_id`; returns the durable wire representation.
    /// 在 `batch_id` 下复制已校验 `vectors`；返回持久化线协议表示。
    pub fn from_vectors(batch_id: impl Into<String>, vectors: &[SegmentVector]) -> Self {
        Self {
            batch_id: batch_id.into(),
            vectors: vectors
                .iter()
                .map(|vector| VceEmbeddingVectorResult {
                    input_id: vector.input_id.clone(),
                    values: vector.values.clone(),
                })
                .collect(),
        }
    }

    /// Validate identity, dimensions, finite values, and the serialized size bound.
    /// 校验身份、维度、有限数值以及序列化大小上限。
    ///
    /// Validates the receiver without host I/O; returns unit on success or a precise contract error.
    /// 不执行宿主读写地校验当前对象；成功返回空值，失败返回明确契约错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.batch_id.trim().is_empty() || self.batch_id.len() > 8192 || self.vectors.is_empty()
        {
            return Err(String::from(
                "VCE embedding batch result identity is invalid",
            ));
        }
        // Seen segment identities used to reject duplicate provider inputs.
        // 用于拒绝重复供应商输入的已见分段身份集合。
        let mut input_ids = BTreeSet::new();
        // Expected vector dimension established by the first returned vector.
        // 由第一条返回向量确定的预期向量维度。
        let mut dimension = None;
        for vector in &self.vectors {
            if vector.input_id.trim().is_empty()
                || vector.input_id.len() > 8192
                || vector.values.is_empty()
                || !input_ids.insert(vector.input_id.clone())
                || vector.values.iter().any(|value| !value.is_finite())
            {
                return Err(String::from("VCE embedding batch result vector is invalid"));
            }
            if let Some(expected) = dimension {
                if expected != vector.values.len() {
                    return Err(String::from(
                        "VCE embedding batch result dimensions are inconsistent",
                    ));
                }
            } else {
                dimension = Some(vector.values.len());
            }
        }
        // Serialized representation used for per-record bounds or round-trip verification.
        // 用于单条记录边界或往返校验的序列化表示。
        let encoded = serde_json::to_vec(self)
            .map_err(|error| format!("Encode VCE embedding batch result: {error}"))?;
        if encoded.len() > 16 * 1024 * 1024 {
            return Err(String::from(
                "VCE embedding batch result exceeds its metadata bound",
            ));
        }
        Ok(())
    }

    /// Convert a validated durable result back to the core vector type.
    /// 将已校验的持久化结果转换回核心向量类型。
    ///
    /// Consumes and validates this durable result; returns core segment vectors or a metadata error.
    /// 消费并校验当前持久结果；返回核心分段向量或元数据错误。
    pub fn into_vectors(self) -> Result<Vec<SegmentVector>, String> {
        self.validate()?;
        Ok(self
            .vectors
            .into_iter()
            .map(|vector| SegmentVector {
                input_id: vector.input_id,
                values: vector.values,
            })
            .collect())
    }
}

/// Durable manifest for a complete source snapshot and its provider batches.
/// 一个完整源码快照及其供应商批次的持久化清单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VceEmbeddingRunManifest {
    /// Manifest format version independent from the index schema.
    /// 独立于索引结构版本的清单格式版本。
    pub schema_version: u32,
    /// Stable refresh/job identity used for idempotent recovery.
    /// 用于幂等恢复的稳定刷新／作业身份。
    pub run_id: String,
    /// Exact index namespace that owns this manifest.
    /// 拥有此清单的精确索引命名空间。
    pub namespace_id: String,
    /// Workspace generation represented by the manifest.
    /// 此清单表示的工作区代次。
    pub generation: u64,
    /// Optional source revision admitted with the refresh request.
    /// 刷新请求准入的可选源码修订号。
    pub source_revision: Option<String>,
    /// Durable run lifecycle.
    /// 持久化运行生命周期。
    pub state: VceEmbeddingRunState,
    /// Frozen model, vector-space, preprocessing, and effective batching contract.
    /// 固定的模型、向量空间、预处理和有效批次契约。
    pub contract: VceEmbeddingRunContract,
    /// Published checkpoint used as the comparison base for this run.
    /// 本次运行用于对比的已发布检查点版本。
    #[serde(default)]
    pub base_checkpoint_version: Option<u64>,
    /// Blob identities retained from the comparison base without new provider requests.
    /// 从对比基线直接复用、无需新增供应商请求的 Blob 身份。
    #[serde(default)]
    pub reused_blob_names: Vec<String>,
    /// Code-node comparison template captured before provider I/O.
    /// 供应商调用前捕获的代码节点对比模板。
    pub nodes: Vec<VceCodeNodeSnapshot>,
    /// Content and path request batches in stable execution order.
    /// 按稳定执行顺序排列的内容和路径请求批次。
    pub batches: Vec<VceEmbeddingBatchRecord>,
    /// Creation timestamp for diagnostics and recovery ordering.
    /// 用于诊断和恢复排序的创建时间。
    pub created_at_unix_ms: u64,
    /// Last durable mutation timestamp.
    /// 最近一次持久化变更时间。
    pub updated_at_unix_ms: u64,
}

impl VceEmbeddingRunManifest {
    /// Validate ownership, bounded metadata, and unique node/batch identities.
    /// 校验归属、有界元数据以及节点／批次身份唯一性。
    ///
    /// Validates the receiver without host I/O; returns unit on success or a precise contract error.
    /// 不执行宿主读写地校验当前对象；成功返回空值，失败返回明确契约错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION
            || self.run_id.trim().is_empty()
            || self.namespace_id.trim().is_empty()
            || self.generation == 0
            || self.created_at_unix_ms == 0
            || self.updated_at_unix_ms < self.created_at_unix_ms
        {
            return Err(String::from("VCE embedding manifest envelope is invalid"));
        }
        if self.run_id.len() > 512 || self.namespace_id.len() > 512 {
            return Err(String::from(
                "VCE embedding manifest identity exceeds its bound",
            ));
        }
        if self
            .source_revision
            .as_ref()
            .is_some_and(|revision| revision.trim().is_empty() || revision.len() > 512)
        {
            return Err(String::from(
                "VCE embedding manifest source revision is invalid",
            ));
        }
        i64::try_from(self.created_at_unix_ms).map_err(|_| {
            String::from("VCE embedding manifest creation time exceeds SQLite range")
        })?;
        i64::try_from(self.updated_at_unix_ms)
            .map_err(|_| String::from("VCE embedding manifest update time exceeds SQLite range"))?;
        self.contract.validate()?;
        // Project-independent metadata projection that excludes source-node and batch payloads.
        // 排除源码节点和批次负载的项目规模无关元数据投影。
        let envelope = VceEmbeddingManifestEnvelope {
            schema_version: self.schema_version,
            run_id: &self.run_id,
            namespace_id: &self.namespace_id,
            generation: self.generation,
            source_revision: self.source_revision.as_deref(),
            state: self.state,
            contract: &self.contract,
            base_checkpoint_version: self.base_checkpoint_version,
            node_count: self.nodes.len(),
            reused_blob_count: self.reused_blob_names.len(),
            batch_count: self.batches.len(),
            created_at_unix_ms: self.created_at_unix_ms,
            updated_at_unix_ms: self.updated_at_unix_ms,
        };
        // Serialized envelope whose size cannot grow with the number of source nodes.
        // 序列化概要，其大小不会随源码节点数量增长。
        let encoded_envelope = serde_json::to_vec(&envelope)
            .map_err(|error| format!("Encode VCE embedding manifest envelope: {error}"))?;
        if encoded_envelope.len() > VCE_EMBEDDING_MANIFEST_ENVELOPE_MAX_BYTES {
            return Err(String::from(
                "VCE embedding manifest envelope exceeds its metadata bound",
            ));
        }
        // Frozen source-file budget from the mandatory run contract.
        // 来自必需运行契约的冻结源码文件预算。
        let configured_source_bound = self.contract.max_file_bytes;
        // Checked per-node allowance covering source and prepared artifact representations.
        // 经过校验的单节点配额，覆盖源码和准备产物表示。
        let node_metadata_bound = configured_source_bound
            .checked_mul(VCE_EMBEDDING_NODE_SERIALIZATION_EXPANSION)
            .and_then(|bytes| bytes.checked_add(VCE_EMBEDDING_NODE_METADATA_ALLOWANCE_BYTES))
            .ok_or_else(|| String::from("VCE embedding node metadata bound overflowed"))?;
        // Source-node identities or snapshots retained for uniqueness and complete-plan checks.
        // 为唯一性和完整计划检查保留的源码节点身份或快照。
        let mut nodes = BTreeSet::new();
        for node in &self.nodes {
            // Optional exact source text retained inside the deterministic preparation artifact.
            // 保存在确定性准备产物内的可选精确源码正文。
            let prepared_content = node
                .prepared_index
                .as_ref()
                .map(|prepared| &prepared.pending.content);
            if node.blob_name.trim().is_empty()
                || node.path.trim().is_empty()
                || node.blob_name.len() > 8192
                || node.path.len() > 8192
                || node.generation != self.generation
                || node
                    .source_content
                    .as_ref()
                    .is_some_and(|content| content.len() as u64 != node.source_size)
                || prepared_content.is_some_and(|content| content.len() as u64 != node.source_size)
                || node
                    .source_content
                    .as_ref()
                    .zip(prepared_content)
                    .is_some_and(|(source_content, prepared_content)| {
                        source_content != prepared_content
                    })
                || node.prepared_index.as_ref().is_some_and(|prepared| {
                    prepared.pending.blob_name != node.blob_name
                        || prepared.pending.path != node.path
                        || prepared.pending.generation != node.generation
                        || prepared
                            .chunks
                            .iter()
                            .map(|chunk| chunk.chunk_id.clone())
                            .collect::<Vec<_>>()
                            != node.chunk_ids
                        || prepared
                            .embedding_plan
                            .as_ref()
                            .map_or_else(Vec::new, |plan| {
                                plan.inputs
                                    .iter()
                                    .map(|input| input.input_id.clone())
                                    .collect::<Vec<_>>()
                            })
                            != node.content_input_ids
                        || prepared
                            .path_document
                            .as_ref()
                            .map(|document| document.path_id.clone())
                            != node.path_input_id
                })
                || node
                    .ready_without_embedding_reason
                    .as_ref()
                    .is_some_and(|reason| reason.trim().is_empty() || reason.len() > 8_192)
                || node.prepared_index.is_some()
                    && (node.structure_analyzer_id.is_none() || node.structure_mode.is_none())
                || node.ready_without_embedding_reason.is_some()
                    && (node.prepared_index.is_some()
                        || !node.chunk_ids.is_empty()
                        || !node.content_input_ids.is_empty()
                        || node.path_input_id.is_some()
                        || node.structure_analyzer_id.is_some()
                        || node.structure_mode.is_some()
                        || node.structure_fallback_reason.is_some()
                        || !node.symbols.is_empty())
                || !nodes.insert(node.blob_name.clone())
            {
                return Err(String::from(
                    "VCE embedding manifest node identity is invalid",
                ));
            }
            // Only this node's bytes are materialized for its independent size check.
            // 仅物化当前节点的字节，执行其独立大小检查。
            let encoded_node = serde_json::to_vec(node)
                .map_err(|error| format!("Encode VCE embedding manifest node: {error}"))?;
            if u64::try_from(encoded_node.len()).map_or(true, |bytes| bytes > node_metadata_bound) {
                return Err(String::from(
                    "VCE embedding manifest node exceeds its metadata bound",
                ));
            }
        }
        // Seen comparison-base Blob identities used to reject duplicates.
        // 用于拒绝重复项的已见对比基线 Blob 身份。
        let mut reused = BTreeSet::new();
        if self.reused_blob_names.iter().any(|blob_name| {
            blob_name.trim().is_empty() || blob_name.len() > 8192 || !reused.insert(blob_name)
        }) || self
            .reused_blob_names
            .iter()
            .any(|blob_name| nodes.contains(blob_name))
            || self
                .base_checkpoint_version
                .is_some_and(|version| version == 0)
        {
            return Err(String::from(
                "VCE embedding manifest comparison base is invalid",
            ));
        }
        // Seen stable batch identities used to reject duplicate execution records.
        // 用于拒绝重复执行记录的已见稳定批次身份。
        let mut batches = BTreeSet::new();
        for batch in &self.batches {
            batch.validate()?;
            if !batches.insert(batch.batch_id.clone()) {
                return Err(String::from(
                    "VCE embedding manifest batch identity is invalid",
                ));
            }
        }
        // Run-level publication states are trustworthy only when every required child batch agrees.
        // 仅当全部必需子批次状态一致时，运行级发布状态才可信。
        let has_unfinished_batch = self.batches.iter().any(|batch| {
            matches!(
                batch.state,
                VceEmbeddingBatchState::Pending | VceEmbeddingBatchState::Running
            )
        });
        // Content vectors are mandatory, while path vectors are an explicitly degradable auxiliary channel.
        // 内容向量是必需结果，路径向量则是允许显式降级的辅助通道。
        let has_incomplete_content_batch = self.batches.iter().any(|batch| {
            batch.purpose == VceAiPurpose::CodeDocumentEmbedding.as_str()
                && batch.state != VceEmbeddingBatchState::Succeeded
        });
        if matches!(
            self.state,
            VceEmbeddingRunState::CandidateReady | VceEmbeddingRunState::Succeeded
        ) && (has_unfinished_batch || has_incomplete_content_batch)
        {
            return Err(String::from(
                "VCE embedding manifest run state disagrees with its batches",
            ));
        }
        Ok(())
    }

    /// Compute the immutable full-plan fingerprint without serializing the project as one buffer.
    /// 在不把整个项目序列化为单一缓冲区的前提下计算不可变完整计划指纹。
    ///
    /// Returns a digest of all ordered immutable fields; lifecycle timestamps and mutable batch states are excluded.
    /// 返回全部有序不可变字段的摘要；排除生命周期时间戳和可变批次状态。
    pub fn plan_fingerprint(&self) -> Result<String, String> {
        self.validate()?;
        // Incremental SHA-256 state updated once per length-prefixed plan component.
        // 按带长度前缀的计划组成项逐次更新的 SHA-256 状态。
        let mut hasher = Sha256::new();
        update_sha256_json(
            &mut hasher,
            &VceEmbeddingPlanEnvelope {
                schema_version: self.schema_version,
                run_id: &self.run_id,
                namespace_id: &self.namespace_id,
                generation: self.generation,
                source_revision: self.source_revision.as_deref(),
                contract: &self.contract,
                base_checkpoint_version: self.base_checkpoint_version,
                reused_blob_count: self.reused_blob_names.len(),
                node_count: self.nodes.len(),
                batch_count: self.batches.len(),
            },
            "VCE embedding plan envelope",
        )?;
        for blob_name in &self.reused_blob_names {
            update_sha256_json(&mut hasher, blob_name, "VCE reused Blob identity")?;
        }
        for node in &self.nodes {
            update_sha256_json(&mut hasher, node, "VCE embedding plan node")?;
        }
        for batch in &self.batches {
            update_sha256_json(
                &mut hasher,
                &VceEmbeddingBatchIdentity {
                    batch_id: &batch.batch_id,
                    purpose: &batch.purpose,
                    input_ids: &batch.input_ids,
                    total_chars: batch.total_chars,
                },
                "VCE embedding plan batch",
            )?;
        }
        Ok(finalize_sha256_hex(hasher))
    }

    /// Move the run to the durable running state after its complete plan is stored.
    /// 在完整计划持久化后将运行转为持久化执行状态。
    ///
    /// Uses `now_unix_ms` as the transition time; returns an error for an invalid manifest or a time before creation.
    /// 使用 `now_unix_ms` 作为转换时间；清单无效或时间早于创建时间时返回错误。
    pub fn mark_running(&mut self, now_unix_ms: u64) -> Result<(), String> {
        self.validate()?;
        if self.state == VceEmbeddingRunState::Succeeded {
            return Err(String::from(
                "VCE embedding manifest cannot run again after publication",
            ));
        }
        if now_unix_ms < self.created_at_unix_ms {
            return Err(String::from(
                "VCE embedding manifest timestamp moved backwards",
            ));
        }
        self.state = VceEmbeddingRunState::Running;
        self.updated_at_unix_ms = now_unix_ms;
        Ok(())
    }

    /// Mark all batches as candidate-ready after staged vectors are materialized.
    /// 在暂存向量全部物化后将所有批次标记为候选就绪。
    ///
    /// `now_unix_ms` supplies the transition time; returns success only after all required content batches finish.
    /// `now_unix_ms` 提供转换时间；仅在全部必需内容批次结束后返回成功。
    pub fn mark_candidate_ready(&mut self, now_unix_ms: u64) -> Result<(), String> {
        if self.state != VceEmbeddingRunState::Running {
            return Err(String::from(
                "VCE embedding manifest is not running and cannot become candidate-ready",
            ));
        }
        if self.batches.iter().any(|batch| {
            (batch.purpose == VceAiPurpose::CodeDocumentEmbedding.as_str()
                && batch.state != VceEmbeddingBatchState::Succeeded)
                || matches!(
                    batch.state,
                    VceEmbeddingBatchState::Pending | VceEmbeddingBatchState::Running
                )
        }) {
            return Err(String::from(
                "VCE embedding manifest has unfinished batches",
            ));
        }
        self.state = VceEmbeddingRunState::CandidateReady;
        self.updated_at_unix_ms = now_unix_ms;
        self.validate()
    }

    /// Mark the run as durably published after the checkpoint receipt is validated.
    /// 在检查点回执校验成功后将运行标记为持久化发布完成。
    ///
    /// Uses `now_unix_ms` as publication time; returns success only for a candidate-ready manifest with valid metadata.
    /// 使用 `now_unix_ms` 作为发布时间；仅对元数据有效的候选就绪清单返回成功。
    pub fn mark_succeeded(&mut self, now_unix_ms: u64) -> Result<(), String> {
        if self.state != VceEmbeddingRunState::CandidateReady {
            return Err(String::from(
                "VCE embedding manifest is not ready for publication",
            ));
        }
        self.state = VceEmbeddingRunState::Succeeded;
        self.updated_at_unix_ms = now_unix_ms;
        self.validate()
    }
}

/// Serializable bounded view of mutable run metadata without project-sized payload collections.
/// 不含项目规模负载集合的可序列化有界运行元数据视图。
#[derive(Serialize)]
struct VceEmbeddingManifestEnvelope<'a> {
    /// Manifest schema version.
    /// 清单结构版本。
    schema_version: u32,
    /// Stable run identity.
    /// 稳定运行身份。
    run_id: &'a str,
    /// Owning index namespace.
    /// 所属索引命名空间。
    namespace_id: &'a str,
    /// Frozen workspace generation.
    /// 冻结工作区代次。
    generation: u64,
    /// Optional admitted source revision.
    /// 可选已准入源码修订。
    source_revision: Option<&'a str>,
    /// Current durable run state.
    /// 当前持久化运行状态。
    state: VceEmbeddingRunState,
    /// Frozen execution contract.
    /// 冻结执行契约。
    contract: &'a VceEmbeddingRunContract,
    /// Optional comparison checkpoint.
    /// 可选对比检查点。
    base_checkpoint_version: Option<u64>,
    /// Number of frozen source nodes.
    /// 冻结源码节点数量。
    node_count: usize,
    /// Number of reused Blob identities.
    /// 复用 Blob 身份数量。
    reused_blob_count: usize,
    /// Number of provider batches.
    /// 供应商批次数量。
    batch_count: usize,
    /// Creation timestamp.
    /// 创建时间戳。
    created_at_unix_ms: u64,
    /// Last mutation timestamp.
    /// 最近变更时间戳。
    updated_at_unix_ms: u64,
}

/// Immutable envelope fields included in a complete embedding-plan fingerprint.
/// 完整向量计划指纹包含的不可变概要字段。
#[derive(Serialize)]
struct VceEmbeddingPlanEnvelope<'a> {
    /// Manifest schema version.
    /// 清单结构版本。
    schema_version: u32,
    /// Stable run identity.
    /// 稳定运行身份。
    run_id: &'a str,
    /// Owning namespace.
    /// 所属命名空间。
    namespace_id: &'a str,
    /// Frozen generation.
    /// 冻结代次。
    generation: u64,
    /// Optional source revision.
    /// 可选源码修订。
    source_revision: Option<&'a str>,
    /// Frozen execution contract.
    /// 冻结执行契约。
    contract: &'a VceEmbeddingRunContract,
    /// Optional comparison checkpoint.
    /// 可选对比检查点。
    base_checkpoint_version: Option<u64>,
    /// Number of reused Blob identities.
    /// 复用 Blob 身份数量。
    reused_blob_count: usize,
    /// Number of frozen source nodes.
    /// 冻结源码节点数量。
    node_count: usize,
    /// Number of provider batches.
    /// 供应商批次数量。
    batch_count: usize,
}

/// Immutable provider-request fields included in plan and batch fingerprints.
/// 计划及批次指纹包含的不可变供应商请求字段。
#[derive(Serialize)]
struct VceEmbeddingBatchIdentity<'a> {
    /// Stable batch identity.
    /// 稳定批次身份。
    batch_id: &'a str,
    /// Provider-neutral purpose.
    /// 与供应商无关的用途。
    purpose: &'a str,
    /// Ordered provider input identities.
    /// 有序供应商输入身份。
    input_ids: &'a [String],
    /// Total Unicode scalar count.
    /// Unicode 标量总数。
    total_chars: usize,
}

/// Hash one JSON value with a length prefix so adjacent values cannot collide structurally.
/// 使用长度前缀哈希一个 JSON 值，避免相邻值发生结构性碰撞。
///
/// Adds serialized `value` to `hasher` with its byte length; `context` names serialization errors.
/// 将序列化 `value` 连同字节长度加入 `hasher`；`context` 标识序列化错误。
fn update_sha256_json<T: Serialize>(
    hasher: &mut Sha256,
    value: &T,
    context: &str,
) -> Result<(), String> {
    // Serialized representation used for per-record bounds or round-trip verification.
    // 用于单条记录边界或往返校验的序列化表示。
    let encoded =
        serde_json::to_vec(value).map_err(|error| format!("Encode {context}: {error}"))?;
    hasher.update((encoded.len() as u64).to_le_bytes());
    hasher.update(encoded);
    Ok(())
}

/// Return the SHA-256 JSON fingerprint for one bounded value.
/// 返回单个有界值的 SHA-256 JSON 指纹。
///
/// Hashes serialized `value`; returns lowercase hexadecimal or an error labeled by `context`.
/// 哈希序列化 `value`；返回小写十六进制文本或由 `context` 标识的错误。
fn sha256_json<T: Serialize>(value: &T, context: &str) -> Result<String, String> {
    // Incremental SHA-256 state updated once per length-prefixed plan component.
    // 按带长度前缀的计划组成项逐次更新的 SHA-256 状态。
    let mut hasher = Sha256::new();
    update_sha256_json(&mut hasher, value, context)?;
    Ok(finalize_sha256_hex(hasher))
}

/// Encode a finalized SHA-256 digest as stable lowercase hexadecimal text.
/// 将最终 SHA-256 摘要编码为稳定的小写十六进制文本。
///
/// Consumes `hasher` and returns the lowercase hexadecimal digest.
/// 消费 `hasher` 并返回小写十六进制摘要。
fn finalize_sha256_hex(hasher: Sha256) -> String {
    // ASCII alphabet used for stable lowercase SHA-256 encoding.
    // 用于稳定小写 SHA-256 编码的 ASCII 字母表。
    const HEX: &[u8; 16] = b"0123456789abcdef";
    // Final digest bytes converted to stable lowercase hexadecimal text.
    // 将转换为稳定小写十六进制文本的最终摘要字节。
    let digest = hasher.finalize();
    // Serialized representation used for per-record bounds or round-trip verification.
    // 用于单条记录边界或往返校验的序列化表示。
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::{EmbeddingInput, EmbeddingPlan};

    /// Build the mandatory frozen execution contract used by manifest fixtures.
    /// 构建清单夹具使用的必需冻结执行契约。
    fn test_embedding_contract() -> VceEmbeddingRunContract {
        VceEmbeddingRunContract {
            embedding_space: EmbeddingSpace {
                vector_space_id: String::from("test-space"),
                physical_fingerprint: Some(String::from("test-provider:test-model:3")),
                dimension: 3,
                preprocessing_version: String::from("vce-preprocess-v1"),
                chunker_version: String::from("vce-chunker-v1"),
            },
            configured_max_inputs: 20,
            provider_max_inputs: Some(20),
            effective_max_inputs: 20,
            max_chars: 8_192,
            max_segment_chars: 4_096,
            overlap_chars: 128,
            max_file_bytes: 16 * 1024 * 1024,
            purposes: vec![
                VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
                VceAiPurpose::CodePathEmbedding.as_str().to_owned(),
            ],
        }
    }

    /// Verify a validated provider result survives serialization and restores exact segment identities.
    /// 验证已校验供应商结果经过序列化后仍能恢复精确分段身份。
    #[test]
    fn batch_result_round_trip_preserves_vectors() {
        // Deterministic input plan used by the manifest lifecycle fixture.
        // 清单生命周期夹具使用的确定性输入计划。
        let plan = EmbeddingPlan::new(
            vec![
                EmbeddingInput::new("blob-a", "alpha").expect("first input"),
                EmbeddingInput::new("blob-b", "beta").expect("second input"),
            ],
            2,
            64,
            64,
            0,
        )
        .expect("build plan");
        // Deterministic vectors covering every requested segment identity.
        // 覆盖每个请求分段身份的确定性向量。
        let vectors = plan
            .batches
            .iter()
            .flat_map(|batch| batch.request_input_ids())
            .map(|input_id| SegmentVector {
                input_id,
                values: vec![0.25, 0.75],
            })
            .collect::<Vec<_>>();
        // Durable provider-result fixture used for exact round-trip verification.
        // 用于精确往返校验的持久化供应商结果夹具。
        let result = VceEmbeddingBatchResult::from_vectors("run:content:0", &vectors);
        result.validate().expect("validate result");
        // Serialized representation used for per-record bounds or round-trip verification.
        // 用于单条记录边界或往返校验的序列化表示。
        let encoded = serde_json::to_vec(&result).expect("encode result");
        // Decoded durable result compared with the original fixture.
        // 与原始夹具比较的已解码持久结果。
        let decoded: VceEmbeddingBatchResult =
            serde_json::from_slice(&encoded).expect("decode result");
        assert_eq!(decoded, result);
        assert_eq!(decoded.into_vectors().expect("restore vectors"), vectors);
    }

    /// Verify UTF-8 diagnostics respect byte limits and oversized individual batches remain rejected.
    /// 验证 UTF-8 诊断遵守字节上限，并持续拒绝单个超大批次。
    #[test]
    fn batch_metadata_bounds_preserve_multibyte_diagnostics() {
        // This fixture exercises the public transition that is used after provider failures.
        // 该夹具覆盖供应商失败后使用的公开状态转换。
        let mut batch = VceEmbeddingBatchRecord {
            batch_id: String::from("bounded-batch"),
            purpose: VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
            input_ids: vec![String::from("bounded-input")],
            total_chars: 1,
            state: VceEmbeddingBatchState::Pending,
            attempt: 0,
            error: None,
        };
        batch.mark_failed("错误🙂".repeat(4_000));
        batch
            .validate()
            .expect("bounded UTF-8 failure is persistable");
        assert!(batch.error.as_ref().expect("failure diagnostic").len() <= 8_192);
        batch.input_ids = (0..129)
            .map(|index| format!("{index:04}{}", "x".repeat(8_188)))
            .collect();
        assert_eq!(
            batch.validate().expect_err("reject oversized single batch"),
            "VCE embedding batch record exceeds its metadata bound"
        );
    }

    /// Verify the manifest lifecycle cannot publish before every content batch succeeds.
    /// 验证所有内容批次成功前清单生命周期不能发布。
    #[test]
    fn manifest_lifecycle_requires_content_batches_before_publication() {
        // Deterministic input plan used by the manifest lifecycle fixture.
        // 清单生命周期夹具使用的确定性输入计划。
        let plan = EmbeddingPlan::new(
            vec![EmbeddingInput::new("blob-a", "alpha").expect("input")],
            1,
            64,
            64,
            0,
        )
        .expect("build plan");
        // Complete source and batch plan used to verify lifecycle or project-scale behavior.
        // 用于验证生命周期或项目规模行为的完整源码及批次计划。
        let mut manifest = VceEmbeddingRunManifest {
            schema_version: VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
            run_id: String::from("run-a"),
            namespace_id: String::from("namespace-a"),
            generation: 1,
            source_revision: None,
            state: VceEmbeddingRunState::Planned,
            contract: test_embedding_contract(),
            base_checkpoint_version: None,
            reused_blob_names: Vec::new(),
            nodes: vec![VceCodeNodeSnapshot {
                blob_name: String::from("blob-a"),
                path: String::from("src/a.rs"),
                generation: 1,
                source_size: 5,
                mtime_ns: None,
                source_content: Some(String::from("alpha")),
                prepared_index: None,
                structure_analyzer_id: None,
                structure_mode: None,
                structure_fallback_reason: None,
                symbols: Vec::new(),
                ready_without_embedding_reason: None,
                chunk_ids: vec![String::from("chunk-a")],
                content_input_ids: plan
                    .inputs
                    .iter()
                    .map(|input| input.input_id.clone())
                    .collect(),
                path_input_id: None,
            }],
            batches: vec![VceEmbeddingBatchRecord::pending(
                "run-a:content:0",
                VceAiPurpose::CodeDocumentEmbedding.as_str(),
                &plan.batches[0],
            )],
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        };
        manifest.validate().expect("validate planned manifest");
        // Immutable plan digest captured before any lifecycle transition.
        // 在任何生命周期转换前捕获的不可变计划摘要。
        let planned_fingerprint = manifest.plan_fingerprint().expect("fingerprint plan");
        assert!(manifest.mark_candidate_ready(2).is_err());
        manifest.mark_running(2).expect("mark running");
        assert!(manifest.mark_candidate_ready(3).is_err());
        manifest.batches[0]
            .mark_running()
            .expect("mark batch running");
        manifest.batches[0].mark_succeeded();
        manifest
            .mark_candidate_ready(3)
            .expect("mark candidate ready");
        manifest.mark_succeeded(4).expect("mark succeeded");
        assert_eq!(manifest.state, VceEmbeddingRunState::Succeeded);
        assert_eq!(
            manifest.plan_fingerprint().expect("fingerprint result"),
            planned_fingerprint
        );
        assert!(manifest.mark_running(5).is_err());
    }

    /// Verify restored publication states cannot contradict their durable child batches.
    /// 验证恢复出的发布状态不能与其持久化子批次相矛盾。
    #[test]
    fn manifest_validation_rejects_published_state_with_incomplete_batches() {
        // Deterministic input plan used to create one mandatory content batch.
        // 用于创建一个必需内容批次的确定性输入计划。
        let plan = EmbeddingPlan::new(
            vec![EmbeddingInput::new("blob-a", "alpha").expect("input")],
            1,
            64,
            64,
            0,
        )
        .expect("build plan");
        // Minimal valid run whose batch lifecycle is mutated below as a corruption fixture.
        // 最小有效运行；下方将修改其批次生命周期以模拟损坏。
        let mut manifest = VceEmbeddingRunManifest {
            schema_version: VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
            run_id: String::from("run-state-consistency"),
            namespace_id: String::from("namespace-a"),
            generation: 1,
            source_revision: None,
            state: VceEmbeddingRunState::Running,
            contract: test_embedding_contract(),
            base_checkpoint_version: None,
            reused_blob_names: Vec::new(),
            nodes: Vec::new(),
            batches: vec![VceEmbeddingBatchRecord::pending(
                "run-state-consistency:content:0",
                VceAiPurpose::CodeDocumentEmbedding.as_str(),
                &plan.batches[0],
            )],
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        };
        manifest
            .validate()
            .expect("running batch plan is recoverable");
        manifest.state = VceEmbeddingRunState::CandidateReady;
        assert_eq!(
            manifest
                .validate()
                .expect_err("candidate cannot retain pending content"),
            "VCE embedding manifest run state disagrees with its batches"
        );
        manifest.batches[0]
            .mark_running()
            .expect("admit content batch");
        manifest.batches[0].mark_failed("provider failed");
        assert!(manifest.validate().is_err());
        manifest.batches[0].mark_succeeded();
        manifest.validate().expect("completed content may publish");

        // A failed path batch remains a valid explicit degradation after mandatory content succeeds.
        // 必需内容成功后，失败的路径批次仍是合法的显式降级结果。
        let mut path_batch = VceEmbeddingBatchRecord::pending(
            "run-state-consistency:path:0",
            VceAiPurpose::CodePathEmbedding.as_str(),
            &plan.batches[0],
        );
        path_batch.mark_failed("path provider failed");
        manifest.batches.push(path_batch);
        manifest
            .validate()
            .expect("failed auxiliary path batch may publish");
    }

    /// Verify project-wide manifests may exceed the retired single-row eight-MiB limit.
    /// 验证项目级清单可以超过已经退役的单行八 MiB 上限。
    #[test]
    fn manifest_total_size_is_not_a_single_metadata_bound() {
        // Repeated source text making the aggregate fixture exceed the old eight-MiB bound.
        // 使聚合夹具超过旧八 MiB 上限的重复源码正文。
        let source = "x".repeat(64 * 1024);
        // Source-node identities or snapshots retained for uniqueness and complete-plan checks.
        // 为唯一性和完整计划检查保留的源码节点身份或快照。
        let nodes = (0..140)
            .map(|index| VceCodeNodeSnapshot {
                blob_name: format!("blob-{index:04}"),
                path: format!("src/generated-{index:04}.rs"),
                generation: 1,
                source_size: source.len() as u64,
                mtime_ns: None,
                source_content: Some(source.clone()),
                prepared_index: None,
                structure_analyzer_id: None,
                structure_mode: None,
                structure_fallback_reason: None,
                symbols: Vec::new(),
                ready_without_embedding_reason: Some(String::from("fixture skip")),
                chunk_ids: Vec::new(),
                content_input_ids: Vec::new(),
                path_input_id: None,
            })
            .collect::<Vec<_>>();
        // Complete source and batch plan used to verify lifecycle or project-scale behavior.
        // 用于验证生命周期或项目规模行为的完整源码及批次计划。
        let manifest = VceEmbeddingRunManifest {
            schema_version: VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
            run_id: String::from("large-run"),
            namespace_id: String::from("large-namespace"),
            generation: 1,
            source_revision: None,
            state: VceEmbeddingRunState::Planned,
            contract: test_embedding_contract(),
            base_checkpoint_version: None,
            reused_blob_names: Vec::new(),
            nodes,
            batches: Vec::new(),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 1,
        };
        // Serialized representation used for per-record bounds or round-trip verification.
        // 用于单条记录边界或往返校验的序列化表示。
        let encoded = serde_json::to_vec(&manifest).expect("encode large manifest fixture");
        assert!(encoded.len() > 8 * 1024 * 1024);
        manifest.validate().expect("validate large manifest");
        assert_eq!(
            manifest
                .plan_fingerprint()
                .expect("fingerprint large plan")
                .len(),
            64
        );
    }
}
