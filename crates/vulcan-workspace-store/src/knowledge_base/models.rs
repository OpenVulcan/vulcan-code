//! Public knowledge-base domain models shared by storage, service, and AI tooling.
//! 由存储、服务与 AI 工具共享的知识库领域模型。

use serde::{Deserialize, Serialize};

/// Stable format identifier stored in every portable knowledge package.
/// 每个可移植知识库包中存储的稳定格式标识。
pub const KNOWLEDGE_PACKAGE_FORMAT: &str = "vulcan-code-knowledge-base";

/// Current portable knowledge package version.
/// 当前可移植知识库包版本。
pub const KNOWLEDGE_PACKAGE_VERSION: u32 = 2;

/// Maximum accepted portable package archive size.
/// 可接受的可移植知识库归档最大字节数。
pub const MAX_KNOWLEDGE_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Selects which durable corpus is allowed to answer a knowledge search.
/// 选择知识库检索时允许使用的持久化内容层。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRetrievalMode {
    /// Search only the authoritative source corpus with Jieba BM25.
    /// 仅使用权威原始库执行 Jieba BM25 检索。
    RawBm25,
    /// Search the curated display corpus with Jieba BM25.
    /// 仅使用整理后的显示库执行 Jieba BM25 检索。
    DisplayBm25,
    /// Fuse display-corpus BM25 with a compatible vector index.
    /// 将显示库 BM25 与兼容的向量索引融合检索。
    DisplayHybrid,
}

impl Default for KnowledgeRetrievalMode {
    /// Returns the zero-cost, backwards-compatible retrieval mode.
    /// 返回零成本且兼容旧库的检索模式。
    fn default() -> Self {
        Self::RawBm25
    }
}

/// Controls whether a missing display corpus may be used as an implicit fallback.
/// 控制显示库缺失时是否允许隐式回退。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeFallbackPolicy {
    /// Never silently switch corpus; the caller must explicitly select raw mode.
    /// 禁止静默切换内容层，调用方必须显式选择原始库模式。
    ExplicitOnly,
    /// Permit raw BM25 only when the display corpus has not been created.
    /// 仅在显示库尚未创建时允许回退到原始库 BM25。
    RawOnlyWhenDisplayMissing,
}

impl Default for KnowledgeFallbackPolicy {
    /// Returns the fail-closed policy required for curated retrieval.
    /// 返回整理检索所需的失败关闭策略。
    fn default() -> Self {
        Self::ExplicitOnly
    }
}

/// Identifies a configured inference route without storing credentials.
/// 标识一个推理路由，但不保存任何凭据。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRouteBindingKind {
    /// Selects a protocol profile route.
    /// 选择协议配置路由。
    Protocol,
    /// Selects a provider-native action route.
    /// 选择供应商原生动作路由。
    Action,
}

impl Default for KnowledgeRouteBindingKind {
    /// Preserves the legacy protocol interpretation for old settings snapshots.
    /// 为旧配置快照保留协议路由解释。
    fn default() -> Self {
        Self::Protocol
    }
}

/// Returns the stable persisted label for one route binding kind.
/// 返回一种路由绑定类型的稳定持久化标签。
impl KnowledgeRouteBindingKind {
    /// Returns the exact wire label used in fingerprints and diagnostics.
    /// 返回指纹与诊断使用的精确线协议标签。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Action => "action",
        }
    }
}

/// KnowledgeRouteReference stores one credential-free route identity.
/// KnowledgeRouteReference 保存一条不含凭据的路由身份。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRouteReference {
    /// Consumer profile that owns authorization and budgets.
    /// 负责授权与配额的消费方配置名称。
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
    pub binding_kind: KnowledgeRouteBindingKind,
    /// Normalized service-entry identifier.
    /// 规范化服务端点标识。
    pub service_entry_id: String,
    /// Model identifier exposed by the provider.
    /// Provider 暴露的模型标识。
    pub model_id: String,
    /// Usage scope authorized for the background knowledge operation.
    /// 知识库后台操作获授权的使用范围。
    #[serde(default = "default_knowledge_route_usage_scope")]
    pub usage_scope: String,
    /// Optional connection identifier used by the host configuration.
    /// 宿主配置中可选的连接标识。
    #[serde(default)]
    pub connection_id: Option<String>,
}

/// Returns the only usage scope accepted by detached knowledge processing.
/// 返回分离式知识库处理唯一接受的使用范围。
fn default_knowledge_route_usage_scope() -> String {
    String::from("internal_background")
}

/// Immutable model metadata captured at configuration or successful indexing time.
/// 在配置或索引成功时捕获的不可变模型元数据。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeModelSnapshot {
    /// Route identity used for the model request.
    /// 模型请求使用的路由身份。
    pub route: KnowledgeRouteReference,
    /// Dimension declared or observed for an embedding space.
    /// 向量空间声明或实际观测到的维度。
    #[serde(default)]
    pub dimensions: Option<u32>,
    /// Provider capability revision, when supplied by the catalog.
    /// Provider 能力配置提供的版本号（如果有）。
    #[serde(default)]
    pub capability_revision: Option<String>,
    /// Capture timestamp in Unix milliseconds.
    /// 捕获时间的 Unix 毫秒时间戳。
    pub captured_at_ms: i64,
}

/// Records the model and prompt history for curated display-corpus generation.
/// 记录整理显示库所使用的模型与提示词历史。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCurationSettings {
    /// Whether LLM curation is enabled for this library.
    /// 此资料库是否启用 LLM 整理。
    #[serde(default)]
    pub enabled: bool,
    /// Desired route for the next curation run.
    /// 下一次整理任务期望使用的路由。
    #[serde(default)]
    pub route: Option<KnowledgeRouteReference>,
    /// Versioned fixed prompt identifier.
    /// 固定提示词的版本标识。
    #[serde(default = "default_knowledge_prompt_version")]
    pub prompt_version: String,
    /// Desired model fingerprint retained for change warnings.
    /// 用于检测模型变化的期望模型指纹。
    #[serde(default)]
    pub desired_model_fingerprint: Option<String>,
    /// Selected provider-neutral reasoning effort for curation requests.
    /// 整理请求选中的供应商无关推理级别。
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Snapshot captured when the library was first configured.
    /// 资料库首次配置时捕获的模型快照。
    #[serde(default)]
    pub created_model_snapshot: Option<KnowledgeModelSnapshot>,
    /// Snapshot captured by the last fully successful curation sync.
    /// 最近一次完整成功整理同步时捕获的模型快照。
    #[serde(default)]
    pub last_successful_sync_snapshot: Option<KnowledgeModelSnapshot>,
}

/// Returns the fixed prompt version used by newly-created or migrated display settings.
/// 返回新建或迁移显示配置使用的固定提示词版本。
fn default_knowledge_prompt_version() -> String {
    String::from("knowledge-display-v1")
}

/// Describes how embedding batches are selected and which model space is valid.
/// 描述向量批量大小如何确定以及当前有效的向量空间。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeEmbeddingSettings {
    /// Whether vector generation and vector retrieval are enabled.
    /// 是否启用向量生成与向量检索。
    #[serde(default)]
    pub enabled: bool,
    /// Desired embedding route for future indexing.
    /// 后续索引期望使用的向量路由。
    #[serde(default)]
    pub route: Option<KnowledgeRouteReference>,
    /// Fingerprint of the desired embedding model.
    /// 期望向量模型的指纹。
    #[serde(default)]
    pub desired_model_fingerprint: Option<String>,
    /// User-configured expected dimension.
    /// 用户配置的期望维度。
    #[serde(default)]
    pub configured_dimension: Option<u32>,
    /// Dimension observed from a capability probe or response.
    /// 从能力探测或实际响应观测到的维度。
    #[serde(default)]
    pub observed_dimension: Option<u32>,
    /// Dimension used by the last successful vector index.
    /// 最近一次成功向量索引实际使用的维度。
    #[serde(default)]
    pub indexed_dimension: Option<u32>,
    /// Effective batch size used by the next or current build.
    /// 下一次或当前构建使用的有效批量大小。
    #[serde(default = "default_knowledge_embedding_batch_size")]
    pub embedding_batch_size: u32,
    /// Maximum input count declared by the selected model capability.
    /// 所选模型能力配置声明的最大输入数量。
    #[serde(default)]
    pub model_max_batch_size: Option<u32>,
    /// Source of the effective batch size.
    /// 有效批量大小的来源。
    #[serde(default = "default_knowledge_batch_size_source")]
    pub batch_size_source: KnowledgeBatchSizeSource,
    /// Snapshot captured when vector indexing was first configured.
    /// 首次配置向量索引时捕获的模型快照。
    #[serde(default)]
    pub created_model_snapshot: Option<KnowledgeModelSnapshot>,
    /// Snapshot captured by the last fully successful vector sync.
    /// 最近一次完整成功向量同步时捕获的模型快照。
    #[serde(default)]
    pub last_successful_sync_snapshot: Option<KnowledgeModelSnapshot>,
}

impl Default for KnowledgeEmbeddingSettings {
    /// Returns a disabled embedding configuration with the documented fallback batch.
    /// 返回关闭向量且使用文档化回退批量大小的配置。
    fn default() -> Self {
        Self {
            enabled: false,
            route: None,
            desired_model_fingerprint: None,
            configured_dimension: None,
            observed_dimension: None,
            indexed_dimension: None,
            embedding_batch_size: KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE,
            model_max_batch_size: None,
            batch_size_source: KnowledgeBatchSizeSource::FallbackDefault,
            created_model_snapshot: None,
            last_successful_sync_snapshot: None,
        }
    }
}

/// Records how the persisted embedding batch size was selected.
/// 记录持久化向量批量大小的选择来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeBatchSizeSource {
    /// Read from the selected model's official capability configuration.
    /// 直接读取所选模型的官方能力配置。
    ModelCapability,
    /// Used because the capability did not declare a maximum input count.
    /// 因能力配置未声明最大输入数量而使用回退值。
    FallbackDefault,
    /// Explicitly overridden by the user within the model capability limit.
    /// 用户在模型能力上限内显式覆盖的值。
    UserOverride,
}

/// Returns the fallback batch-size source used by legacy settings snapshots.
/// 返回旧配置快照使用的回退批量来源。
fn default_knowledge_batch_size_source() -> KnowledgeBatchSizeSource {
    KnowledgeBatchSizeSource::FallbackDefault
}

/// Fallback batch size used only when official capability data is absent.
/// 仅在官方能力数据缺失时使用的回退批量大小。
pub const KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE: u32 = 10;

/// Returns the documented fallback batch size for legacy or incomplete settings.
/// 返回旧配置或不完整配置使用的文档化回退批量大小。
fn default_knowledge_embedding_batch_size() -> u32 {
    KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE
}

/// Describes the lifecycle and compatibility state of the vector index.
/// 描述向量索引的生命周期与兼容状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeVectorIndexStatus {
    /// Vector retrieval is disabled by policy or compatibility checks.
    /// 因策略或兼容性检查而关闭向量检索。
    Disabled,
    /// A build has been requested but has not started.
    /// 已请求构建但尚未开始。
    Pending,
    /// A vector build is currently running.
    /// 向量构建正在执行。
    Building,
    /// The active index passed model and dimension checks.
    /// 当前索引通过了模型与维度校验。
    Ready,
    /// The source/display corpus changed after the last vector sync.
    /// 原始库或显示库在最近一次向量同步后发生变化。
    Stale,
    /// The last vector build failed.
    /// 最近一次向量构建失败。
    Failed,
}

/// Describes the lifecycle of the derived AI-curated display corpus.
/// 描述 AI 整理显示库的派生内容生命周期。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeDisplayIndexStatus {
    /// No display corpus has been generated yet.
    /// 尚未生成显示库。
    NotCreated,
    /// A display build is queued.
    /// 显示库构建已排队。
    Pending,
    /// A display build is running.
    /// 显示库构建正在执行。
    Building,
    /// The display corpus passed validation and indexing.
    /// 显示库已通过校验与索引。
    Ready,
    /// Source or curation configuration changed after the last build.
    /// 原始库或整理配置在上次构建后发生变化。
    Stale,
    /// The last display build failed.
    /// 最近一次显示库构建失败。
    Failed,
}

impl Default for KnowledgeDisplayIndexStatus {
    /// Starts every library without an AI-generated display corpus.
    /// 新资料库默认不包含 AI 生成的显示库。
    fn default() -> Self {
        Self::NotCreated
    }
}

/// Stores durable display-corpus generation state independently from raw content.
/// 将显示库生成状态与原始内容独立持久化。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDisplayIndexState {
    /// Current display-corpus lifecycle status.
    /// 当前显示库生命周期状态。
    #[serde(default)]
    pub status: KnowledgeDisplayIndexStatus,
    /// Active display generation used for atomic publication.
    /// 用于原子发布的当前显示库代次。
    #[serde(default)]
    pub active_generation: Option<String>,
    /// Pending generation retained so a failed build can resume without discarding completed documents.
    /// 保留待完成代次，使失败构建可以继续补齐而无需丢弃已完成文档。
    #[serde(default)]
    pub pending_generation: Option<String>,
    /// Source revision captured when the pending generation started.
    /// 待完成代次开始时记录的原始库修订号。
    #[serde(default)]
    pub pending_source_content_revision: Option<u64>,
    /// Prompt version captured when the pending generation started.
    /// 待完成代次开始时记录的提示词版本。
    #[serde(default)]
    pub pending_prompt_version: Option<String>,
    /// Curation model fingerprint captured when the pending generation started.
    /// 待完成代次开始时记录的整理模型指纹。
    #[serde(default)]
    pub pending_model_fingerprint: Option<String>,
    /// Raw content revision represented by the display corpus.
    /// 显示库所对应的原始内容修订号。
    #[serde(default)]
    pub source_content_revision: Option<u64>,
    /// Prompt version used by the active display generation.
    /// 当前显示库代次使用的提示词版本。
    #[serde(default)]
    pub indexed_prompt_version: Option<String>,
    /// Curation model fingerprint used by the active display generation.
    /// 当前显示库代次使用的整理模型指纹。
    #[serde(default)]
    pub indexed_model_fingerprint: Option<String>,
    /// Number of validated display documents in the active generation.
    /// 当前显示库代次中已校验的显示文档数量。
    #[serde(default)]
    pub generated_document_count: u64,
    /// Last explicit display-corpus error.
    /// 最近一次显示库构建错误。
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Default for KnowledgeVectorIndexStatus {
    /// Starts every newly-created raw library with vector retrieval disabled.
    /// 新建原始资料库默认关闭向量检索。
    fn default() -> Self {
        Self::Disabled
    }
}

/// Stores durable vector-index state independently from the desired model settings.
/// 将持久化向量索引状态与期望模型配置分离保存。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeVectorIndexState {
    /// Current derived-index lifecycle state.
    /// 当前派生索引生命周期状态。
    #[serde(default)]
    pub status: KnowledgeVectorIndexStatus,
    /// Active generation identifier for atomic index swaps.
    /// 用于原子切换索引的当前代次标识。
    #[serde(default)]
    pub active_generation: Option<String>,
    /// Model fingerprint used by the active vector index.
    /// 当前向量索引实际使用的模型指纹。
    #[serde(default)]
    pub indexed_model_fingerprint: Option<String>,
    /// Dimension used by the active vector index.
    /// 当前向量索引实际使用的维度。
    #[serde(default)]
    pub indexed_dimension: Option<u32>,
    /// User-confirmed compatibility override, including its risk note.
    /// 用户确认的兼容性覆盖记录及其风险说明。
    #[serde(default)]
    pub compatibility_override: Option<String>,
    /// Last explicit vector-index error.
    /// 最近一次明确的向量索引错误。
    #[serde(default)]
    pub last_error: Option<String>,
}

/// Complete per-library policy and model configuration persisted as JSON.
/// 按资料库持久化的完整策略与模型配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySettings {
    /// Initialization and retrieval mode selected by the user.
    /// 用户选择的初始化与检索模式。
    #[serde(default)]
    pub retrieval_mode: KnowledgeRetrievalMode,
    /// Policy controlling implicit raw-corpus fallback.
    /// 控制是否允许隐式回退原始库的策略。
    #[serde(default)]
    pub fallback_policy: KnowledgeFallbackPolicy,
    /// Display-corpus curation configuration.
    /// 显示库整理配置。
    #[serde(default)]
    pub curation: KnowledgeCurationSettings,
    /// Derived AI-curated display-corpus state.
    /// 派生 AI 整理显示库状态。
    #[serde(default)]
    pub display_index: KnowledgeDisplayIndexState,
    /// Embedding configuration and official capability snapshot.
    /// 向量配置及官方能力快照。
    #[serde(default)]
    pub embedding: KnowledgeEmbeddingSettings,
    /// Derived vector-index state.
    /// 派生向量索引状态。
    #[serde(default)]
    pub vector_index: KnowledgeVectorIndexState,
}

impl Default for KnowledgeLibrarySettings {
    /// Returns the raw-BM25 policy used by legacy libraries.
    /// 返回旧资料库兼容的原始 BM25 策略。
    fn default() -> Self {
        Self {
            retrieval_mode: KnowledgeRetrievalMode::RawBm25,
            fallback_policy: KnowledgeFallbackPolicy::ExplicitOnly,
            curation: KnowledgeCurationSettings {
                enabled: false,
                prompt_version: String::from("knowledge-display-v1"),
                ..KnowledgeCurationSettings::default()
            },
            display_index: KnowledgeDisplayIndexState::default(),
            embedding: KnowledgeEmbeddingSettings::default(),
            vector_index: KnowledgeVectorIndexState::default(),
        }
    }
}

impl KnowledgeLibrarySettings {
    /// Disables an unsafe vector index when persisted dimensions no longer agree.
    /// 当持久化维度不一致时关闭不安全的向量索引。
    pub fn normalize_derived_compatibility(&mut self) {
        if !self.embedding.enabled {
            self.vector_index = KnowledgeVectorIndexState::default();
            return;
        }
        let dimensions = [
            self.embedding.configured_dimension,
            self.embedding.observed_dimension,
            self.embedding.indexed_dimension,
            self.vector_index.indexed_dimension,
        ];
        let known_dimensions = dimensions.iter().flatten().copied().collect::<Vec<_>>();
        let dimension_mismatch = known_dimensions
            .first()
            .is_some_and(|first| known_dimensions.iter().any(|dimension| dimension != first))
            || matches!(self.vector_index.status, KnowledgeVectorIndexStatus::Ready)
                && self.vector_index.indexed_dimension.is_none();
        if dimension_mismatch {
            self.vector_index.status = KnowledgeVectorIndexStatus::Disabled;
            self.vector_index.compatibility_override = None;
            self.vector_index.last_error = Some(String::from(
                "向量配置、实际观测与索引维度不一致，向量检索已自动关闭",
            ));
        } else if matches!(self.vector_index.status, KnowledgeVectorIndexStatus::Ready)
            && (self.vector_index.active_generation.is_none()
                || self.vector_index.indexed_model_fingerprint.is_none()
                || self.vector_index.indexed_dimension.is_none())
        {
            self.vector_index.status = KnowledgeVectorIndexStatus::Disabled;
            self.vector_index.compatibility_override = None;
            self.vector_index.last_error = Some(String::from(
                "向量索引就绪状态缺少活动代次、模型指纹或维度，向量检索已自动关闭",
            ));
        } else if matches!(self.vector_index.status, KnowledgeVectorIndexStatus::Ready)
            && self.vector_index.compatibility_override.is_none()
            && self.embedding.desired_model_fingerprint.is_some()
            && self.embedding.desired_model_fingerprint
                != self.vector_index.indexed_model_fingerprint
        {
            self.vector_index.status = KnowledgeVectorIndexStatus::Stale;
            self.vector_index.last_error = Some(String::from(
                "向量模型指纹发生变化，旧索引仍保留，必须重建或明确确认同维度兼容",
            ));
        }
    }

    /// Validates mode-dependent settings before they are persisted.
    /// 在持久化前校验依赖检索模式的配置约束。
    pub fn validate(&self) -> Result<(), String> {
        match self.retrieval_mode {
            KnowledgeRetrievalMode::RawBm25 => {
                if self.curation.enabled || self.embedding.enabled {
                    return Err(String::from("原始 BM25 模式不能启用显示库整理或向量检索"));
                }
            }
            KnowledgeRetrievalMode::DisplayBm25 => {
                if !self.curation.enabled {
                    return Err(String::from("显示库 BM25 模式必须启用整理模型"));
                }
                if self.embedding.enabled {
                    return Err(String::from("显示库 BM25 模式不能启用向量检索"));
                }
            }
            KnowledgeRetrievalMode::DisplayHybrid => {
                if !self.curation.enabled {
                    return Err(String::from("显示库混合模式必须启用整理模型"));
                }
                if !self.embedding.enabled {
                    return Err(String::from("显示库混合模式必须启用向量检索配置"));
                }
            }
        }
        if self.curation.enabled && self.curation.route.is_none() {
            return Err(String::from("启用显示库整理时必须选择整理模型路由"));
        }
        if let Some(route) = self.curation.route.as_ref() {
            validate_knowledge_route_reference(route, "整理")?;
        }
        if self.curation.enabled && self.curation.prompt_version.trim().is_empty() {
            return Err(String::from("显示库整理提示词版本不能为空"));
        }
        if self.embedding.enabled && self.embedding.route.is_none() {
            return Err(String::from("启用向量检索时必须选择向量模型路由"));
        }
        if let Some(route) = self.embedding.route.as_ref() {
            validate_knowledge_route_reference(route, "向量")?;
        }
        if self.embedding.enabled && self.embedding.configured_dimension.is_none() {
            return Err(String::from("启用向量检索时必须配置向量维度"));
        }
        if self.embedding.embedding_batch_size == 0 {
            return Err(String::from("向量批量大小必须大于 0"));
        }
        if let Some(maximum) = self.embedding.model_max_batch_size {
            if maximum == 0 {
                return Err(String::from("模型能力配置的最大向量批量不能为 0"));
            }
            if self.embedding.embedding_batch_size > maximum {
                return Err(format!(
                    "向量批量大小 {} 超过模型能力上限 {}",
                    self.embedding.embedding_batch_size, maximum
                ));
            }
        }
        if self.embedding.enabled && self.embedding.configured_dimension == Some(0) {
            return Err(String::from("向量维度必须大于 0"));
        }
        if let Some(note) = self.vector_index.compatibility_override.as_deref() {
            if !self.embedding.enabled {
                return Err(String::from("关闭向量检索时不能保留兼容性覆盖记录"));
            }
            if note.trim().is_empty() {
                return Err(String::from("向量兼容性覆盖说明不能为空"));
            }
        }
        Ok(())
    }
}

/// Validates one persisted route identity before it can authorize background inference.
/// 在授权后台推理前校验一条持久化路由身份。
fn validate_knowledge_route_reference(
    route: &KnowledgeRouteReference,
    label: &str,
) -> Result<(), String> {
    if route.consumer_profile_id.trim().is_empty()
        || route.provider_id.trim().is_empty()
        || route.protocol.trim().is_empty()
        || route.service_entry_id.trim().is_empty()
        || route.model_id.trim().is_empty()
    {
        return Err(format!("{label}模型路由身份不能为空"));
    }
    if route.usage_scope != "internal_background" {
        return Err(format!(
            "{label}模型路由必须使用 internal_background 使用范围"
        ));
    }
    Ok(())
}

/// Resolves a user override against the official embedding capability limit.
/// 根据官方向量能力上限解析用户覆盖值。
pub fn resolve_embedding_batch_size(
    user_override: Option<u32>,
    model_maximum_inputs: Option<u32>,
) -> Result<(u32, KnowledgeBatchSizeSource), String> {
    if model_maximum_inputs == Some(0) {
        return Err(String::from("模型能力配置的最大向量批量不能为 0"));
    }
    let batch_size = user_override
        .unwrap_or_else(|| model_maximum_inputs.unwrap_or(KNOWLEDGE_DEFAULT_EMBEDDING_BATCH_SIZE));
    if batch_size == 0 {
        return Err(String::from("向量批量大小必须大于 0"));
    }
    if let Some(maximum) = model_maximum_inputs.filter(|maximum| batch_size > *maximum) {
        return Err(format!(
            "向量批量大小 {} 超过模型能力上限 {}",
            batch_size, maximum
        ));
    }
    let source = if user_override.is_some() {
        KnowledgeBatchSizeSource::UserOverride
    } else if model_maximum_inputs.is_some() {
        KnowledgeBatchSizeSource::ModelCapability
    } else {
        KnowledgeBatchSizeSource::FallbackDefault
    };
    Ok((batch_size, source))
}

#[cfg(test)]
mod settings_tests {
    use super::{
        KnowledgeBatchSizeSource, KnowledgeCurationSettings, KnowledgeEmbeddingSettings,
        KnowledgeLibrarySettings, KnowledgeRetrievalMode, KnowledgeRouteReference,
        KnowledgeVectorIndexState, KnowledgeVectorIndexStatus, resolve_embedding_batch_size,
    };

    /// Verifies the official capability maximum is used without a provider-specific branch.
    /// 验证无供应商特判时直接使用官方能力上限。
    #[test]
    fn capability_maximum_is_the_default_batch() {
        let (batch, source) = resolve_embedding_batch_size(None, Some(20)).expect("valid batch");
        assert_eq!(batch, 20);
        assert_eq!(source, KnowledgeBatchSizeSource::ModelCapability);
    }

    /// Verifies missing capability data uses the documented fallback and records its source.
    /// 验证缺失能力数据时使用文档化回退值并记录来源。
    #[test]
    fn missing_capability_uses_fallback_batch() {
        let (batch, source) = resolve_embedding_batch_size(None, None).expect("valid fallback");
        assert_eq!(batch, 10);
        assert_eq!(source, KnowledgeBatchSizeSource::FallbackDefault);
    }

    /// Verifies a user override cannot exceed the official capability limit.
    /// 验证用户覆盖值不能超过官方能力上限。
    #[test]
    fn user_override_is_bounded_by_capability() {
        let error = resolve_embedding_batch_size(Some(21), Some(20)).expect_err("must reject");
        assert!(error.contains("超过模型能力上限"));
    }

    /// Verifies the raw-BM25 default remains valid for legacy libraries.
    /// 验证旧资料库的原始 BM25 默认配置仍然有效。
    #[test]
    fn raw_settings_are_backward_compatible() {
        let settings = KnowledgeLibrarySettings::default();
        assert_eq!(settings.retrieval_mode, KnowledgeRetrievalMode::RawBm25);
        settings.validate().expect("raw defaults are valid");
    }

    /// Verifies incomplete persisted settings are upgraded through serde defaults.
    /// 验证字段不完整的持久化配置可以通过 serde 默认值升级。
    #[test]
    fn incomplete_settings_are_backward_compatible() {
        let settings: KnowledgeLibrarySettings =
            serde_json::from_str(r#"{"retrievalMode":"raw_bm25"}"#)
                .expect("legacy settings should deserialize");
        assert_eq!(settings.retrieval_mode, KnowledgeRetrievalMode::RawBm25);
        assert_eq!(settings.embedding.embedding_batch_size, 10);
        settings.validate().expect("legacy raw settings are valid");
    }

    /// Verifies a persisted dimension mismatch closes vector retrieval before exposure.
    /// 验证持久化维度不一致会在对外暴露前关闭向量检索。
    #[test]
    fn dimension_mismatch_disables_vector_index() {
        let mut settings = KnowledgeLibrarySettings {
            retrieval_mode: KnowledgeRetrievalMode::DisplayHybrid,
            curation: KnowledgeCurationSettings {
                enabled: true,
                ..KnowledgeCurationSettings::default()
            },
            embedding: KnowledgeEmbeddingSettings {
                enabled: true,
                configured_dimension: Some(1024),
                observed_dimension: Some(1536),
                indexed_dimension: Some(1024),
                ..KnowledgeEmbeddingSettings::default()
            },
            vector_index: KnowledgeVectorIndexState {
                status: KnowledgeVectorIndexStatus::Ready,
                indexed_dimension: Some(1024),
                ..KnowledgeVectorIndexState::default()
            },
            ..KnowledgeLibrarySettings::default()
        };
        settings.normalize_derived_compatibility();
        assert_eq!(
            settings.vector_index.status,
            KnowledgeVectorIndexStatus::Disabled
        );
        assert!(settings.vector_index.last_error.is_some());
    }

    /// Verifies an explicit compatibility note is valid only for enabled vector settings.
    /// 验证显式兼容性说明只能存在于启用向量的配置中。
    #[test]
    fn compatibility_override_requires_enabled_vectors() {
        let disabled_settings = KnowledgeLibrarySettings {
            vector_index: KnowledgeVectorIndexState {
                compatibility_override: Some(String::from("same space")),
                ..KnowledgeVectorIndexState::default()
            },
            ..KnowledgeLibrarySettings::default()
        };
        assert!(disabled_settings.validate().is_err());
        let curation_route = KnowledgeRouteReference {
            consumer_profile_id: String::from("knowledge_base"),
            provider_id: String::from("provider"),
            protocol: String::from("protocol"),
            service_entry_id: String::from("curation"),
            model_id: String::from("model"),
            usage_scope: String::from("internal_background"),
            ..KnowledgeRouteReference::default()
        };
        let embedding_route = KnowledgeRouteReference {
            consumer_profile_id: String::from("knowledge_base"),
            provider_id: String::from("provider"),
            protocol: String::from("protocol"),
            service_entry_id: String::from("embedding"),
            model_id: String::from("model"),
            usage_scope: String::from("internal_background"),
            ..KnowledgeRouteReference::default()
        };
        let settings = KnowledgeLibrarySettings {
            retrieval_mode: KnowledgeRetrievalMode::DisplayHybrid,
            curation: KnowledgeCurationSettings {
                enabled: true,
                route: Some(curation_route),
                prompt_version: String::from("knowledge-display-v1"),
                ..KnowledgeCurationSettings::default()
            },
            embedding: KnowledgeEmbeddingSettings {
                enabled: true,
                configured_dimension: Some(1024),
                route: Some(embedding_route),
                ..KnowledgeEmbeddingSettings::default()
            },
            vector_index: KnowledgeVectorIndexState {
                compatibility_override: Some(String::from("same space")),
                ..KnowledgeVectorIndexState::default()
            },
            ..KnowledgeLibrarySettings::default()
        };
        assert!(settings.validate().is_ok());
    }

    /// Verifies a ready index missing immutable identity is closed before it can be queried.
    /// 验证缺失不可变身份的 ready 索引会在可检索前被关闭。
    #[test]
    fn incomplete_ready_vector_index_is_disabled() {
        let mut settings = KnowledgeLibrarySettings {
            retrieval_mode: KnowledgeRetrievalMode::DisplayHybrid,
            curation: KnowledgeCurationSettings {
                enabled: true,
                ..KnowledgeCurationSettings::default()
            },
            embedding: KnowledgeEmbeddingSettings {
                enabled: true,
                configured_dimension: Some(1024),
                ..KnowledgeEmbeddingSettings::default()
            },
            vector_index: KnowledgeVectorIndexState {
                status: KnowledgeVectorIndexStatus::Ready,
                ..KnowledgeVectorIndexState::default()
            },
            ..KnowledgeLibrarySettings::default()
        };
        settings.normalize_derived_compatibility();
        assert_eq!(
            settings.vector_index.status,
            KnowledgeVectorIndexStatus::Disabled
        );
    }
}

/// Portable metadata stored as package.json inside one .vkb archive.
/// 单个 .vkb 归档内以 package.json 保存的可移植元数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageManifest {
    /// Stable package format identifier.
    /// 稳定包格式标识。
    pub format: String,
    /// Portable package protocol version.
    /// 可移植包协议版本。
    pub package_version: u32,
    /// Source library identity retained only for provenance and validation.
    /// 仅用于来源追踪和校验的原知识库标识。
    pub source_library_id: String,
    /// Original user-visible library name.
    /// 原始用户可见知识库名称。
    pub name: String,
    /// Original user-visible library description.
    /// 原始用户可见知识库说明。
    pub description: String,
    /// Optional per-library settings snapshot retained for portable migration.
    /// 可选的按资料库配置快照，用于可移植迁移。
    #[serde(default)]
    pub settings: Option<KnowledgeLibrarySettings>,
    /// Authoritative per-library SQLite schema version.
    /// 权威单库 SQLite 结构版本。
    pub schema_version: i64,
    /// Authoritative document count captured at export.
    /// 导出时捕获的权威文档数量。
    pub document_count: u64,
    /// Authoritative chunk count captured at export.
    /// 导出时捕获的权威切片数量。
    pub chunk_count: u64,
    /// Exported derived-index state.
    /// 导出的派生索引状态。
    pub index_status: String,
    /// Unix timestamp when the package was created.
    /// 创建知识库包时的 Unix 毫秒时间戳。
    pub exported_at_ms: i64,
    /// Lowercase SHA-256 digest of library/content.sqlite3.
    /// library/content.sqlite3 的小写 SHA-256 摘要。
    pub content_sha256: String,
    /// Exact uncompressed SQLite byte size.
    /// SQLite 文件的精确未压缩字节数。
    pub content_bytes: u64,
    /// Checksummed derived files retained so imports do not need to rebuild them.
    /// 保存派生文件校验摘要，使导入无需重建派生索引。
    #[serde(default)]
    pub derived_artifacts: Vec<KnowledgePackageArtifact>,
}

/// One checksummed derived artifact stored inside a portable knowledge package.
/// 可移植知识库包内保存的一项带校验摘要的派生产物。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageArtifact {
    /// Exact safe ZIP path and corresponding relative library path.
    /// 精确的安全 ZIP 路径及其对应的资料库相对路径。
    pub archive_path: String,
    /// Lowercase SHA-256 digest of the uncompressed artifact bytes.
    /// 派生产物未压缩字节的小写 SHA-256 摘要。
    pub sha256: String,
    /// Exact uncompressed artifact byte size.
    /// 派生产物的精确未压缩字节数。
    pub bytes: u64,
}

/// Result returned after one package is exported.
/// 一个知识库包导出后返回的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageExportResult {
    /// Final native archive path selected by the host.
    /// 宿主选择的最终原生归档路径。
    pub archive_path: String,
    /// Portable package metadata written into the archive.
    /// 写入归档的可移植包元数据。
    pub package: KnowledgePackageManifest,
}

/// Read-only inspection result for one candidate package.
/// 一个候选知识库包的只读检查结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageInspection {
    /// Canonical native archive path accepted by the host.
    /// 宿主接受的规范原生归档路径。
    pub archive_path: String,
    /// Validated portable package metadata.
    /// 已校验的可移植包元数据。
    pub package: KnowledgePackageManifest,
    /// Existing active library with the same normalized name, when present.
    /// 存在时返回同规范名称的现有活动知识库。
    pub conflicting_library: Option<KnowledgeLibrarySummary>,
}

/// Result returned after one package import or replacement.
/// 一个知识库包导入或替换后返回的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePackageImportResult {
    /// Imported or replaced authoritative library summary.
    /// 已导入或替换的权威知识库摘要。
    pub library: KnowledgeLibrarySummary,
    /// Whether an existing physical library was explicitly replaced.
    /// 是否显式替换了现有物理知识库。
    pub replaced_existing: bool,
    /// Non-fatal cleanup or post-commit warnings retained after a successful import.
    /// 成功导入后保留的非致命清理或提交后告警。
    pub warnings: Vec<String>,
}

/// One custom Jieba word stored inside a physical library database.
/// 存储在物理知识库数据库中的一个 Jieba 自定义词。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCustomWord {
    /// Exact custom word text.
    /// 精确的自定义词文本。
    pub word: String,
    /// Jieba dictionary weight.
    /// Jieba 词典权重。
    pub weight: u32,
}

/// One project-scoped knowledge library shown to users and runtime policy code.
/// 展示给用户和运行时策略代码的一个项目级知识库。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeLibrarySummary {
    /// Immutable UUID string identifying the library.
    /// 标识知识库的不可变 UUID 字符串。
    pub id: String,
    /// User-visible library name.
    /// 用户可见的知识库名称。
    pub name: String,
    /// Optional user-visible library description.
    /// 可选的用户可见知识库说明。
    pub description: String,
    /// Durable per-library retrieval and model settings.
    /// 持久化的按资料库检索与模型配置。
    #[serde(default)]
    pub settings: KnowledgeLibrarySettings,
    /// Durable lifecycle state such as active, deleting, or error.
    /// 持久化生命周期状态，例如 active、deleting 或 error。
    pub lifecycle_state: String,
    /// Number of authoritative Markdown documents.
    /// 权威 Markdown 文档数量。
    pub document_count: u64,
    /// Number of authoritative searchable chunks.
    /// 权威可检索切片数量。
    pub chunk_count: u64,
    /// Derived FTS index state.
    /// 派生 FTS 索引状态。
    pub index_status: String,
    /// Unix timestamp of the most recent completed import.
    /// 最近一次完成导入的 Unix 毫秒时间戳。
    pub last_completed_import_at_ms: Option<i64>,
    /// Unix timestamp when the library was created.
    /// 知识库创建时的 Unix 毫秒时间戳。
    pub created_at_ms: i64,
    /// Unix timestamp when the library metadata last changed.
    /// 知识库元数据最近变更时的 Unix 毫秒时间戳。
    pub updated_at_ms: i64,
}

/// One durable Markdown source directory attached to a knowledge library.
/// 关联到一个知识库的持久化 Markdown 来源目录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceSummary {
    /// Immutable source identifier retained when the physical directory is remapped.
    /// 物理目录重新映射时保持不变的来源标识符。
    pub id: String,
    /// Canonical native directory path currently assigned to the source.
    /// 当前分配给该来源的规范原生目录路径。
    pub canonical_root: String,
    /// User-visible directory name derived from the canonical root.
    /// 从规范根目录派生的用户可见目录名称。
    pub display_name: String,
    /// Number of authoritative Markdown documents currently owned by the source.
    /// 当前由该来源持有的权威 Markdown 文档数量。
    pub document_count: u64,
    /// Number of authoritative searchable chunks currently owned by the source.
    /// 当前由该来源持有的权威可检索切片数量。
    pub chunk_count: u64,
    /// Unix timestamp when the durable source identity was created.
    /// 持久化来源身份创建时的 Unix 毫秒时间戳。
    pub created_at_ms: i64,
    /// Unix timestamp when the source directory was last scanned.
    /// 来源目录最近一次扫描时的 Unix 毫秒时间戳。
    pub last_scanned_at_ms: i64,
}

/// Stable kind for one durable knowledge import, derivation, or index-maintenance operation.
/// 一次持久化资料导入、派生构建或索引维护操作的稳定类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeOperationKind {
    /// Recursive Markdown directory import and incremental index synchronization.
    /// 递归 Markdown 目录导入与增量索引同步。
    Import,
    /// Non-destructive completion of missing, mismatched, or orphaned derived FTS rows.
    /// 非破坏性补全缺失、错配或孤立的派生 FTS 记录。
    Repair,
    /// Destructive FTS-only rebuild from all authoritative chunks.
    /// 从全部权威切片执行仅针对 FTS 的破坏性重建。
    Rebuild,
    /// AI curation build for the derived display corpus.
    /// 面向派生显示库的 AI 整理构建。
    DisplayBuild,
    /// Embedding build for the active derived display generation.
    /// 面向当前显示库活动代次的向量构建。
    VectorBuild,
}

impl KnowledgeOperationKind {
    /// Returns the stable database and wire representation for this operation kind.
    /// 返回该操作类型稳定的数据库与线协议表示。
    ///
    /// Returns: one of the stable persisted operation names.
    /// 返回值：稳定的持久化操作名称之一。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Import => "import",
            Self::Repair => "repair",
            Self::Rebuild => "rebuild",
            Self::DisplayBuild => "display_build",
            Self::VectorBuild => "vector_build",
        }
    }

    /// Parses one exact persisted operation kind without accepting aliases.
    /// 解析一个精确的持久化操作类型，不接受别名。
    ///
    /// Parameters: `value` is the database text to validate.
    /// Returns: the typed operation kind or an explicit integrity error.
    /// 参数：`value` 是待校验的数据库文本。返回值：强类型操作类型或明确的完整性错误。
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "import" => Ok(Self::Import),
            "repair" => Ok(Self::Repair),
            "rebuild" => Ok(Self::Rebuild),
            "display_build" => Ok(Self::DisplayBuild),
            "vector_build" => Ok(Self::VectorBuild),
            _ => Err(format!("未知的知识库操作类型：{value}")),
        }
    }
}

/// Exact metadata and physical-row integrity snapshot for one derived Jieba FTS index.
/// 一个派生 Jieba FTS 索引的精确元数据与物理记录完整性快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeIndexIntegritySummary {
    /// Immutable library identifier inspected by the runtime.
    /// 运行时检查的不可变知识库标识符。
    pub library_id: String,
    /// Whether the physical FTS virtual table currently exists.
    /// 物理 FTS 虚拟表当前是否存在。
    pub index_exists: bool,
    /// Whether every fail-closed metadata and physical integrity condition currently passes.
    /// 当前是否通过全部失败关闭元数据与物理完整性条件。
    pub searchable: bool,
    /// Persisted index lifecycle state.
    /// 持久化索引生命周期状态。
    pub index_status: String,
    /// Configured tokenizer identifier.
    /// 配置的分词器标识符。
    pub tokenizer: String,
    /// Current authoritative content revision.
    /// 当前权威内容版本。
    pub content_revision: u64,
    /// Content revision represented by the FTS index.
    /// FTS 索引表示的内容版本。
    pub indexed_content_revision: u64,
    /// Current Jieba dictionary revision.
    /// 当前 Jieba 词典版本。
    pub dictionary_revision: u64,
    /// Dictionary revision represented by the FTS index.
    /// FTS 索引表示的词典版本。
    pub indexed_dictionary_revision: u64,
    /// Number of authoritative Markdown chunks.
    /// 权威 Markdown 切片数量。
    pub authoritative_chunk_count: u64,
    /// Number of physical FTS rows.
    /// 物理 FTS 记录数量。
    pub indexed_chunk_count: u64,
    /// Authoritative chunks without one exact matching FTS source row.
    /// 没有精确匹配 FTS 源记录的权威切片数量。
    pub missing_or_mismatched_count: u64,
    /// Excess duplicate, orphaned, or source-mismatched physical FTS rows.
    /// 冗余重复、孤立或源字段错配的物理 FTS 记录数量。
    pub orphan_or_mismatched_count: u64,
}

/// One explicit authorization binding between a library and a project.
/// 知识库与项目之间的一条显式授权绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeProjectBinding {
    /// Immutable library identifier.
    /// 不可变知识库标识符。
    pub library_id: String,
    /// Immutable project identifier.
    /// 不可变项目标识符。
    pub project_id: String,
    /// Whether inherited session selection enables this library by default.
    /// 继承型会话选择是否默认启用此知识库。
    pub default_enabled: bool,
}

/// Persisted summary for one imported Markdown document.
/// 一份已导入 Markdown 文档的持久化摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentSummary {
    /// Immutable document identifier.
    /// 不可变文档标识符。
    pub id: String,
    /// Owning source identifier.
    /// 所属来源标识符。
    pub source_id: String,
    /// Slash-normalized path relative to the imported source root.
    /// 相对于导入来源根目录、以斜杠规范化的路径。
    pub relative_path: String,
    /// Slash-normalized folder classification derived from the source tree.
    /// 从来源目录树派生的斜杠规范化文件夹分类。
    pub folder_path: String,
    /// Deterministic document title.
    /// 确定性的文档标题。
    pub title: String,
    /// Source byte size.
    /// 来源字节大小。
    pub byte_size: u64,
    /// Source line count after newline normalization.
    /// 换行规范化后的来源行数。
    pub line_count: u64,
    /// Number of searchable chunks owned by the document.
    /// 文档拥有的可检索切片数量。
    pub chunk_count: u64,
    /// Unix timestamp when the authoritative row last changed.
    /// 权威记录最近变更时的 Unix 毫秒时间戳。
    pub updated_at_ms: i64,
}

/// Full authoritative content returned for one knowledge document.
/// 返回给调用方的一份资料文档完整权威内容。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentContent {
    /// Document metadata.
    /// 文档元数据。
    pub document: KnowledgeDocumentSummary,
    /// Newline-normalized UTF-8 Markdown content.
    /// 换行规范化后的 UTF-8 Markdown 内容。
    pub content: String,
}

/// One AI-curated display document retained as a derived projection of an authoritative document.
/// 一份由 AI 整理、作为权威文档派生投影保存的显示库文档。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDisplayDocument {
    /// Stable display-document identifier.
    /// 稳定的显示文档标识符。
    pub id: String,
    /// Immutable authoritative document identifier.
    /// 不可变的权威原始文档标识符。
    pub source_document_id: String,
    /// Source file hash used for incremental rebuild decisions.
    /// 用于增量重建判断的源文件哈希。
    pub source_sha256: String,
    /// Slash-normalized source path.
    /// 斜杠规范化后的来源路径。
    pub relative_path: String,
    /// Slash-normalized source folder.
    /// 斜杠规范化后的来源文件夹。
    pub folder_path: String,
    /// LLM-normalized display title.
    /// LLM 规范化后的显示标题。
    pub title: String,
    /// LLM-generated concise summary.
    /// LLM 生成的精炼摘要。
    pub summary: String,
    /// Keywords and aliases used for exact display search.
    /// 用于显示库精确搜索的关键词与别名。
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Functions, commands, or symbols extracted by the curation model.
    /// 整理模型提取的函数、命令或符号。
    #[serde(default)]
    pub symbols: Vec<String>,
    /// Stable semantic text sent to the embedding model.
    /// 发送给向量模型的稳定语义文本。
    pub embedding_text: String,
    /// Validated normalized Markdown shown to users.
    /// 展示给用户的已校验规范 Markdown。
    pub markdown: String,
    /// Generation that produced this document.
    /// 生成当前文档的显示库代次。
    pub generation: String,
    /// Unix timestamp of successful curation.
    /// 整理成功时的 Unix 毫秒时间戳。
    pub generated_at_ms: i64,
}

/// One searchable display chunk with its source mapping.
/// 一条带来源映射的可检索显示库切片。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDisplayChunk {
    /// Stable display chunk identifier.
    /// 稳定的显示库切片标识符。
    pub id: String,
    /// Owning display-document identifier.
    /// 所属显示文档标识符。
    pub display_document_id: String,
    /// Zero-based chunk ordinal.
    /// 从零开始的切片序号。
    pub ordinal: u64,
    /// Display Markdown heading ancestry.
    /// 显示 Markdown 标题层级。
    pub heading_path: String,
    /// One-based inclusive display line range.
    /// 从一开始且包含边界的显示行范围。
    pub start_line: u64,
    /// One-based inclusive display line range end.
    /// 从一开始且包含边界的显示行范围结束。
    pub end_line: u64,
    /// Normalized display Markdown chunk content.
    /// 规范化后的显示 Markdown 切片内容。
    pub content: String,
    /// Semantic text used to generate the vector for this chunk.
    /// 用于为当前切片生成向量的语义文本。
    pub embedding_text: String,
    /// Generation that produced this chunk.
    /// 生成当前切片的显示库代次。
    pub generation: String,
}

/// One authoritative Markdown chunk and its source citation.
/// 一个权威 Markdown 切片及其来源引用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunk {
    /// Immutable chunk identifier used by FTS.
    /// FTS 使用的不可变切片标识符。
    pub id: String,
    /// Owning document identifier.
    /// 所属文档标识符。
    pub document_id: String,
    /// Zero-based deterministic ordinal inside the document.
    /// 文档内从零开始的确定性序号。
    pub ordinal: u64,
    /// Slash-separated Markdown heading ancestry.
    /// 以斜杠分隔的 Markdown 标题层级。
    pub heading_path: String,
    /// One-based inclusive first source line.
    /// 从一开始且包含边界的来源起始行。
    pub start_line: u64,
    /// One-based inclusive last source line.
    /// 从一开始且包含边界的来源结束行。
    pub end_line: u64,
    /// Original Markdown slice.
    /// 原始 Markdown 切片。
    pub content: String,
    /// Mixed-language estimated token count.
    /// 中英文混合估算 token 数量。
    pub estimated_tokens: u64,
}

/// One fully materialized BM25 search hit.
/// 一条完成权威数据物化的 BM25 检索命中。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchHit {
    /// Library containing the hit.
    /// 命中所属知识库标识符。
    pub library_id: String,
    /// User-visible library name.
    /// 用户可见知识库名称。
    pub library_name: String,
    /// Document containing the hit.
    /// 命中所属文档标识符。
    pub document_id: String,
    /// Chunk identifier used by FTS.
    /// FTS 使用的切片标识符。
    pub chunk_id: String,
    /// Document title.
    /// 文档标题。
    pub title: String,
    /// Relative source path.
    /// 相对来源路径。
    pub relative_path: String,
    /// Markdown heading ancestry.
    /// Markdown 标题层级。
    pub heading_path: String,
    /// One-based inclusive first source line.
    /// 从一开始且包含边界的来源起始行。
    pub start_line: u64,
    /// One-based inclusive last source line.
    /// 从一开始且包含边界的来源结束行。
    pub end_line: u64,
    /// Authoritative Markdown chunk content.
    /// 权威 Markdown 切片内容。
    pub content: String,
    /// Stable lexical match strategy, either `bm25` or `substring`.
    /// 稳定的词法匹配策略，只能是 `bm25` 或 `substring`。
    pub match_kind: String,
    /// Per-library BM25 score where larger is better.
    /// 单知识库内越大越优的 BM25 分数；子串补充命中固定为零。
    pub bm25_score: f64,
    /// Optional reciprocal-rank-fusion score used only across libraries.
    /// 仅跨知识库融合时使用的可选倒数排名融合分数。
    pub rrf_score: Option<f64>,
}

/// One library-level search failure retained in a federated response.
/// 联合检索响应中保留的一条知识库级失败。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchFailure {
    /// Library that failed.
    /// 失败的知识库标识符。
    pub library_id: String,
    /// Explicit failure detail.
    /// 显式失败详情。
    pub error: String,
}

/// One exact FTS MATCH expression generated by vldb for a physical library.
/// vldb 为一个物理知识库生成的一条精确 FTS MATCH 表达式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeFtsQuery {
    /// Library whose dictionary produced the expression.
    /// 其词典生成该表达式的知识库。
    pub library_id: String,
    /// Safe Jieba-tokenized expression sent to SQLite FTS5.
    /// 发送给 SQLite FTS5 的安全 Jieba 分词表达式。
    pub expression: String,
}

/// Federated BM25 response with explicit partial failures.
/// 带显式部分失败信息的联合 BM25 响应。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchResult {
    /// Original non-empty query.
    /// 原始非空查询。
    pub query: String,
    /// Ranked, authoritative hits.
    /// 完成排序的权威命中。
    pub hits: Vec<KnowledgeSearchHit>,
    /// Per-library FTS expressions generated by vldb.
    /// vldb 生成的逐知识库 FTS 表达式。
    pub fts_queries: Vec<KnowledgeFtsQuery>,
    /// Library-level failures not hidden from the caller.
    /// 不向调用方隐藏的知识库级失败。
    pub failures: Vec<KnowledgeSearchFailure>,
    /// Explicit compatibility and retrieval warnings for the caller.
    /// 返回给调用方的兼容性与检索风险提示。
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Final report for one synchronous Markdown directory import.
/// 一次同步 Markdown 目录导入的最终报告。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeImportReport {
    /// Durable import job identifier.
    /// 持久化导入任务标识符。
    pub job_id: String,
    /// Target library identifier.
    /// 目标知识库标识符。
    pub library_id: String,
    /// Number of Markdown files discovered.
    /// 扫描到的 Markdown 文件数量。
    pub scanned_files: u64,
    /// Number of new or changed documents committed.
    /// 已提交的新增或变更文档数量。
    pub changed_files: u64,
    /// Number of unchanged documents retained.
    /// 保留的未变化文档数量。
    pub unchanged_files: u64,
    /// Number of documents deleted after a confirmed rescan.
    /// 在确认重新扫描后删除的文档数量。
    pub deleted_files: u64,
    /// Number of files skipped because they could not be read safely.
    /// 因无法安全读取而跳过的文件数量。
    pub skipped_files: u64,
    /// Total source bytes read during the import.
    /// 导入期间读取的来源总字节数。
    pub processed_bytes: u64,
    /// Number of authoritative chunks after the import.
    /// 导入后的权威切片数量。
    pub chunk_count: u64,
    /// Bounded user-visible warnings.
    /// 有界的用户可见告警。
    pub warnings: Vec<String>,
}

/// Describes the current request being surfaced by one durable knowledge operation.
/// 描述一个持久化资料库操作当前正在展示的请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeOperationCurrentRequest {
    /// Stable request identity assigned by the knowledge worker.
    /// 知识库工作器分配的稳定请求标识。
    pub request_id: String,
    /// Human-readable batch or file label.
    /// 面向用户的批次或文件标签。
    pub label: String,
    /// Current request phase.
    /// 当前请求阶段。
    pub phase: String,
    /// Exact provider identifier when resolved.
    /// 已解析时的精确供应商标识。
    pub provider: String,
    /// Exact model identifier when resolved.
    /// 已解析时的精确模型标识。
    pub model: String,
    /// Exact protocol or action binding label.
    /// 精确协议或动作绑定标签。
    pub protocol: String,
    /// Current request lifecycle status.
    /// 当前请求生命周期状态。
    pub status: String,
    /// Current Markdown-formatted stream preview.
    /// 当前 Markdown 格式的流式预览内容。
    pub output_markdown: String,
    /// Request start timestamp in Unix milliseconds.
    /// 请求开始时间的 Unix 毫秒时间戳。
    pub started_at_ms: u64,
    /// Latest known input token count for the request.
    /// 当前请求最新已知输入 Token 数量。
    pub input_tokens: Option<u64>,
    /// Latest known output token count for the request.
    /// 当前请求最新已知输出 Token 数量。
    pub output_tokens: Option<u64>,
    /// Latest known cache-read token count for the request.
    /// 当前请求最新已知缓存读取 Token 数量。
    pub cache_read_tokens: Option<u64>,
    /// Latest known cache-write token count for the request.
    /// 当前请求最新已知缓存写入 Token 数量。
    pub cache_write_tokens: Option<u64>,
}

/// Summarizes one completed knowledge operation request for the compact history panel.
/// 概括资料库操作最近请求面板中的一条已完成请求。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeOperationRequestSummary {
    /// Stable request identity.
    /// 稳定请求标识。
    pub request_id: String,
    /// Batch or file label shown to the user.
    /// 面向用户显示的批次或文件标签。
    pub label: String,
    /// Operation phase that produced this request.
    /// 产生该请求的操作阶段。
    pub phase: String,
    /// Provider identifier.
    /// 供应商标识。
    pub provider: String,
    /// Model identifier.
    /// 模型标识。
    pub model: String,
    /// Request protocol or action binding.
    /// 请求协议或动作绑定。
    pub protocol: String,
    /// Terminal request status.
    /// 请求终态状态。
    pub status: String,
    /// Request duration in milliseconds.
    /// 请求耗时毫秒数。
    pub duration_ms: u64,
    /// Final known input token count.
    /// 最终已知输入 Token 数量。
    pub input_tokens: Option<u64>,
    /// Final known output token count.
    /// 最终已知输出 Token 数量。
    pub output_tokens: Option<u64>,
    /// Final known cache-read token count.
    /// 最终已知缓存读取 Token 数量。
    pub cache_read_tokens: Option<u64>,
    /// Final known cache-write token count.
    /// 最终已知缓存写入 Token 数量。
    pub cache_write_tokens: Option<u64>,
}

/// Aggregates observable usage and request state for one knowledge operation.
/// 聚合一个资料库操作的可观察用量与请求状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct KnowledgeOperationObservability {
    /// Number of LLM API requests dispatched by the worker.
    /// 工作器实际发起的 LLM API 请求数量。
    pub llm_request_count: u64,
    /// Number of explicit schema-repair or provider retry requests.
    /// 明确的 Schema 修复或供应商重试请求数量。
    pub llm_retry_count: u64,
    /// Number of embedding API batches dispatched by the worker.
    /// 工作器实际发起的向量 API 批次数量。
    pub embedding_request_count: u64,
    /// Number of successfully returned embedding vectors.
    /// 成功返回的向量数量。
    pub generated_vector_count: u64,
    /// Cumulative input token count, or null when a request did not report it.
    /// 累计输入 Token 数量；任一请求未报告时为 null。
    pub input_tokens: Option<u64>,
    /// Cumulative output token count, or null when a request did not report it.
    /// 累计输出 Token 数量；任一请求未报告时为 null。
    pub output_tokens: Option<u64>,
    /// Cumulative cache-read token count, or null when a request did not report it.
    /// 累计缓存读取 Token 数量；任一请求未报告时为 null。
    pub cache_read_tokens: Option<u64>,
    /// Cumulative cache-write token count, or null when a request did not report it.
    /// 累计缓存写入 Token 数量；任一请求未报告时为 null。
    pub cache_write_tokens: Option<u64>,
    /// Current active request, if one is in flight.
    /// 当前正在执行的请求；没有活动请求时为空。
    pub current_request: Option<KnowledgeOperationCurrentRequest>,
    /// Bounded newest-first request history; the host retains at most three items.
    /// 有界的最新请求优先历史；宿主最多保留三条记录。
    pub recent_requests: Vec<KnowledgeOperationRequestSummary>,
}

impl Default for KnowledgeOperationObservability {
    /// Creates a zeroed observable snapshot whose cumulative usage is known to be zero.
    /// 创建一份累计用量明确为零的初始可观测快照。
    fn default() -> Self {
        Self {
            llm_request_count: 0,
            llm_retry_count: 0,
            embedding_request_count: 0,
            generated_vector_count: 0,
            input_tokens: Some(0),
            output_tokens: Some(0),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
            current_request: None,
            recent_requests: Vec::new(),
        }
    }
}

/// Durable import or index-maintenance job progress and terminal state.
/// 持久化导入或索引维护任务的进度与终态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeImportJobSummary {
    /// Durable job identifier.
    /// 持久化任务标识符。
    pub id: String,
    /// Target library identifier.
    /// 目标知识库标识符。
    pub library_id: String,
    /// Stable import, repair, or rebuild operation kind.
    /// 稳定的导入、补全或重建操作类型。
    pub operation_kind: KnowledgeOperationKind,
    /// Durable lifecycle state.
    /// 持久化生命周期状态。
    pub state: String,
    /// Current processing phase.
    /// 当前处理阶段。
    pub phase: String,
    /// Discovered Markdown count.
    /// 已发现 Markdown 数量。
    pub scanned_files: u64,
    /// Total Markdown count after discovery completes, or zero while still scanning.
    /// 发现完成后的 Markdown 总数，扫描中为零。
    pub total_files: u64,
    /// Number of Markdown files whose parsing work has completed.
    /// 已完成解析工作的 Markdown 文件数。
    pub processed_files: u64,
    /// Total index items in the active indexing or recovery phase.
    /// 当前索引或恢复阶段的索引项总数。
    pub total_index_items: u64,
    /// Index items completed in the active indexing or recovery phase.
    /// 当前索引或恢复阶段已完成的索引项数。
    pub processed_index_items: u64,
    /// Current relative path being scanned or processed.
    /// 当前正在扫描或处理的相对路径。
    pub current_file: Option<String>,
    /// Source bytes read so far.
    /// 当前已读取来源字节数。
    pub processed_bytes: u64,
    /// Bounded warnings.
    /// 有界告警。
    pub warnings: Vec<String>,
    /// Optional terminal error.
    /// 可选终态错误。
    pub error_message: Option<String>,
    /// Live request and usage observability snapshot.
    /// 实时请求与用量可观测快照。
    #[serde(default)]
    pub observability: KnowledgeOperationObservability,
}

#[cfg(test)]
mod tests {
    use super::{KnowledgeImportJobSummary, KnowledgeOperationKind, KnowledgeSourceSummary};

    /// Verifies the browser progress contract uses explicit camelCase field names.
    /// 验证浏览器进度契约使用显式 camelCase 字段名。
    #[test]
    fn import_job_serializes_with_browser_field_names() {
        // Job is a complete representative snapshot for every compound progress field.
        // Job 是覆盖全部复合进度字段的完整代表性快照。
        let job = KnowledgeImportJobSummary {
            id: String::from("job-1"),
            library_id: String::from("library-1"),
            operation_kind: KnowledgeOperationKind::Import,
            state: String::from("running"),
            phase: String::from("parsing"),
            scanned_files: 8,
            total_files: 8,
            processed_files: 3,
            total_index_items: 21,
            processed_index_items: 13,
            current_file: Some(String::from("guide/intro.md")),
            processed_bytes: 1024,
            warnings: Vec::new(),
            error_message: None,
            observability: Default::default(),
        };
        // Value exposes the exact JSON shape consumed by KnowledgeBaseArea.
        // Value 暴露 KnowledgeBaseArea 使用的准确 JSON 结构。
        let value = serde_json::to_value(job).expect("serialize import progress");
        assert_eq!(value["libraryId"], "library-1");
        assert_eq!(value["operationKind"], "import");
        assert_eq!(value["scannedFiles"], 8);
        assert_eq!(value["totalFiles"], 8);
        assert_eq!(value["processedFiles"], 3);
        assert_eq!(value["totalIndexItems"], 21);
        assert_eq!(value["processedIndexItems"], 13);
        assert_eq!(value["currentFile"], "guide/intro.md");
        assert_eq!(value["processedBytes"], 1024);
        assert!(value.get("total_files").is_none());
    }

    /// Verifies legacy empty observability payloads migrate through serde defaults.
    /// 验证旧版本空的可观测载荷可以通过 serde 默认值完成迁移。
    #[test]
    fn empty_observability_payload_uses_safe_defaults() {
        let snapshot: super::KnowledgeOperationObservability =
            serde_json::from_str("{}").expect("empty observability payload should parse");
        assert_eq!(snapshot.llm_request_count, 0);
        assert!(snapshot.current_request.is_none());
        assert!(snapshot.recent_requests.is_empty());
    }

    /// Verifies durable source management uses the browser's camelCase contract.
    /// 验证持久化来源管理使用浏览器侧 camelCase 契约。
    #[test]
    fn source_summary_serializes_with_browser_field_names() {
        // source is a complete representative durable Markdown root.
        // source 是一条完整且具有代表性的持久化 Markdown 根目录。
        let source = KnowledgeSourceSummary {
            id: String::from("source-1"),
            canonical_root: String::from("D:\\docs"),
            display_name: String::from("docs"),
            document_count: 42,
            chunk_count: 128,
            created_at_ms: 100,
            last_scanned_at_ms: 200,
        };
        // value exposes the exact JSON shape consumed by the source management list.
        // value 暴露来源管理列表使用的准确 JSON 结构。
        let value = serde_json::to_value(source).expect("serialize knowledge source summary");
        assert_eq!(value["canonicalRoot"], "D:\\docs");
        assert_eq!(value["displayName"], "docs");
        assert_eq!(value["documentCount"], 42);
        assert_eq!(value["chunkCount"], 128);
        assert_eq!(value["lastScannedAtMs"], 200);
        assert!(value.get("canonical_root").is_none());
    }

    /// Verifies every durable operation kind has one stable wire representation.
    /// 验证每个持久化操作类型都拥有唯一稳定的线协议表示。
    #[test]
    fn operation_kinds_round_trip_without_aliases() {
        let kinds = [
            KnowledgeOperationKind::Import,
            KnowledgeOperationKind::Repair,
            KnowledgeOperationKind::Rebuild,
            KnowledgeOperationKind::DisplayBuild,
            KnowledgeOperationKind::VectorBuild,
        ];
        for kind in kinds {
            assert_eq!(KnowledgeOperationKind::parse(kind.as_str()), Ok(kind));
        }
        assert!(KnowledgeOperationKind::parse("display-build").is_err());
        assert!(KnowledgeOperationKind::parse("vector-build").is_err());
    }
}
