//! Host publication checks serialized with the existing public AI configuration transaction.
//! 与现有公共 AI 配置事务串行化的宿主发布检查。

use super::*;
use vulcan_context_engine::*;

/// Exact frozen facts that can invalidate an index publication, excluding query and scheduling policy.
/// 能使索引发布失效的精确冻结事实，不包含查询及调度策略。
#[derive(Clone, Debug)]
pub(crate) struct VcePublicationContract {
    /// Host-owned project identity.
    /// 宿主持有项目身份。
    pub(crate) project_id: String,
    /// Canonical source root used by the admitted scanner.
    /// 已准入扫描器使用的规范源码根。
    pub(crate) canonical_root: String,
    /// Actual embedding producer fingerprint, not a rewritten stored-space identity.
    /// 实际向量生产来源指纹，不是改写后的存储空间身份。
    pub(crate) embedding_fingerprint: String,
    /// Exact admitted vector dimension.
    /// 精确已准入向量维度。
    pub(crate) dimension: u32,
    /// Source size filter affecting checkpoint membership.
    /// 影响检查点成员的源码大小过滤规则。
    pub(crate) max_file_bytes: u64,
    /// Exact project ignore rules used by the scanner.
    /// 扫描器使用的精确项目忽略规则。
    pub(crate) extra_ignore_patterns: Vec<String>,
    /// Exact original index and all current origins; absence is valid only before a binding exists.
    /// 精确原始索引及全部当前来源；仅绑定建立前允许缺失。
    pub(crate) provenance: Option<vulcan_workspace_store::vce_data_plane::VceEmbeddingProvenance>,
    /// Exact durable replacement admission; ordinary readers and writers always leave this absent.
    /// 精确持久替换准入；普通读取方及写入方始终不携带此字段。
    /// This permits building a distinct target, never querying predecessor vectors with a new model.
    /// 此字段允许构建独立目标，绝不允许新模型查询前驱向量。
    pub(crate) replacement_admission:
        Option<Box<vulcan_workspace_store::vce_data_plane::VceStoredJob>>,
}

/// Synchronous host-owned check performed after the public configuration transaction is acquired.
/// 获取公共配置事务后执行的同步宿主自有检查。
/// Input is the frozen contract; returns an explicit revocation/drift diagnostic or admission.
/// 输入为冻结契约；返回明确撤销、漂移诊断或准入。
pub(crate) type VcePublicationAuthority =
    Arc<dyn Fn(&VcePublicationContract) -> Result<(), VcePublicationError> + Send + Sync>;

/// Typed denial keeps a deliberate rebuild requirement distinct from an operational failure.
/// 类型化拒绝使明确重建要求与运行失败保持区分。
#[derive(Debug)]
pub(crate) enum VcePublicationError {
    /// Configuration drift, revoked authority or an operational diagnostic.
    /// 配置漂移、权威撤销或操作诊断。
    Rejected(String),
    /// The retained index needs explicit compatibility approval or a human rebuild.
    /// 保留索引需要明确兼容批准或人工重建。
    RebuildRequired(String),
}

impl From<String> for VcePublicationError {
    /// Preserve an ordinary authority error without guessing a lifecycle from its text.
    /// 保留普通权威错误，不根据文字猜测生命周期。
    /// Input is the exact diagnostic; returns an operational denial.
    /// 输入为精确诊断；返回操作拒绝。
    fn from(error: String) -> Self {
        Self::Rejected(error)
    }
}

impl From<VcePublicationError> for String {
    /// Project a typed denial into existing job and core error boundaries without losing its message.
    /// 将类型化拒绝投影到既有作业及核心错误边界，不丢失消息。
    /// Input is the classified denial; returns its exact diagnostic.
    /// 输入为分类拒绝；返回精确诊断。
    fn from(error: VcePublicationError) -> Self {
        match error {
            VcePublicationError::Rejected(message)
            | VcePublicationError::RebuildRequired(message) => message,
        }
    }
}

/// Index-port view preserving storage ownership while checking authority only at publication.
/// 保留存储归属、仅在发布时检查权威的索引端口视图。
pub(super) struct VcePublicationIndex {
    /// Exact runtime retained until the core finishes commit or rollback.
    /// 保留到核心完成提交或回滚的精确运行时。
    pub(super) runtime: Arc<VceProjectRuntime>,
    /// Host resolver shared with model-setting confirmation.
    /// 与模型设置确认共享的宿主解析器。
    pub(super) authority: VcePublicationAuthority,
    /// Existing manager run identity used for the commit's durable read lease.
    /// 提交持久读取租约使用的既有管理器运行身份。
    owner_run_id: String,
    /// Exact committed read view handed to the completed private engine without storage I/O.
    /// 无需存储输入输出便可移交给已完成私有引擎的精确已提交读取视图。
    committed_read_index: Arc<Mutex<Option<VceProjectIndex>>>,
}

impl VcePublicationIndex {
    /// Create an owned publication port using the existing runtime, authority and host run.
    /// 使用既有运行时、权威校验器和宿主运行身份创建有归属的发布端口。
    /// `runtime` owns storage; `authority` checks policy; `owner_run_id` names the host lifetime.
    /// `runtime` 持有存储；`authority` 校验策略；`owner_run_id` 标识宿主生命周期。
    /// Returns a fresh per-refresh handoff slot without starting storage or provider operations.
    /// 返回每次刷新独立的移交槽位，不启动存储或供应商操作。
    pub(super) fn new(
        runtime: Arc<VceProjectRuntime>,
        authority: VcePublicationAuthority,
        owner_run_id: &str,
    ) -> Self {
        Self {
            runtime,
            authority,
            owner_run_id: owner_run_id.to_owned(),
            committed_read_index: Arc::new(Mutex::new(None)),
        }
    }

    /// Transfer the transaction's retained read view to the engine publication.
    /// 将事务保留的读取视图转移给引擎发布对象。
    /// No parameters. Returns an internal contract error if no committed lease was produced.
    /// 无参数。若没有产生已提交租约则返回内部契约错误。
    pub(super) async fn take_committed_read_index(&self) -> Result<VceProjectIndex, String> {
        self.committed_read_index
            .lock()
            .await
            .take()
            .ok_or_else(|| {
                String::from("ContextEngine committed refresh omitted its retained read view")
            })
    }

    /// Validate authorization and the immutable write-relevant contract under the caller's transaction.
    /// 在调用方事务内校验授权及不可变写入相关契约。
    /// No parameters. Returns revocation before any publication writes, while reads remain independent.
    /// 无参数。在任何发布写入前返回撤销结果，读取保持独立。
    pub(super) async fn validate(&self) -> Result<(), String> {
        if !self
            .runtime
            .operation_admission
            .accepting
            .load(Ordering::Acquire)
        {
            return Err(String::from(
                "ContextEngine project stopped before index publication",
            ));
        }
        let provenance = self.runtime.index.embedding_provenance().await?;
        let contract = VcePublicationContract::for_runtime(&self.runtime, provenance)?;
        (self.authority)(&contract).map_err(String::from)
    }
}

impl VcePublicationContract {
    /// Validate the physical scope of an explicit replacement, keeping it separate from compatibility grants.
    /// 校验明确替换的物理作用域，使其与兼容授权分离。
    /// Returns true only for a queued/running receipt matching this target and the exact initialized predecessor.
    /// 仅当排队／运行回执匹配此目标及精确已初始化前驱时返回真。
    /// No parameters or side effects; the host must still validate live models, project intent and stored ownership.
    /// 无参数或副作用；宿主仍须验证实时模型、项目意图及存储归属。
    pub(in crate::vce) fn validates_replacement_admission(&self) -> Result<bool, String> {
        use vulcan_workspace_store::vce_data_plane::VceJobState;
        let Some(receipt) = &self.replacement_admission else {
            return Ok(false);
        };
        receipt.validate()?;
        // replacement is mandatory in this explicit branch; absent metadata is not a compatibility fallback.
        // 此明确分支必须携带 replacement；元数据缺失不是兼容回退。
        let replacement = receipt.replacement.as_ref().ok_or_else(|| {
            String::from("VCE replacement admission omitted its durable physical target")
        })?;
        let provenance = self.provenance.as_ref().ok_or_else(|| {
            String::from("VCE replacement admission requires an initialized predecessor")
        })?;
        let target = &replacement.target;
        let original = &provenance.original;
        if !matches!(receipt.state, VceJobState::Queued | VceJobState::Running)
            || receipt.project_id != self.project_id
            || original.project_id != self.project_id
            || replacement.predecessor.namespace_id != original.binding.namespace_id
            || provenance.checkpoint_version != Some(replacement.predecessor.checkpoint_version)
            || target.root_path != self.canonical_root
            || original.root_path != self.canonical_root
            || target.root_hash != original.root_hash
            || target.vector_space_id == original.vector_space_id
            || target.binding.checkpoint_chain_id == original.binding.checkpoint_chain_id
            || target.dimension != self.dimension
            || target.physical_fingerprint != self.embedding_fingerprint
            || target.preprocessing_version != VCE_PREPROCESSING_VERSION
            || target.chunker_version != VCE_CHUNKER_VERSION
        {
            return Err(String::from(
                "VCE replacement admission changed its predecessor or independent target scope",
            ));
        }
        Ok(true)
    }

    /// Capture one runtime contract and a separately read committed provenance snapshot.
    /// 捕获一份运行时契约及单独读取的已提交来源快照。
    /// `runtime` owns frozen execution facts; `provenance` owns stored facts. Returns bounded identity metadata.
    /// `runtime` 持有冻结执行事实；`provenance` 持有存储事实。返回有界身份元数据。
    pub(super) fn for_runtime(
        runtime: &VceProjectRuntime,
        provenance: vulcan_workspace_store::vce_data_plane::VceEmbeddingProvenance,
    ) -> Result<Self, String> {
        Ok(Self {
            project_id: runtime.project_id.clone(),
            canonical_root: crate::normalized_workspace_path_text_from_path(
                runtime.source.project_root(),
            ),
            embedding_fingerprint: runtime
                .ai
                .embedding_space()
                .physical_fingerprint
                .clone()
                .ok_or_else(|| {
                    String::from("ContextEngine publisher lacks an actual embedding origin")
                })?,
            dimension: u32::try_from(runtime.ai.embedding_space().dimension).map_err(|_| {
                String::from("ContextEngine publisher dimension exceeds the host contract")
            })?,
            max_file_bytes: runtime.settings.max_file_bytes,
            extra_ignore_patterns: runtime.settings.extra_ignore_patterns.clone(),
            provenance: Some(provenance),
            replacement_admission: None,
        })
    }
}

impl VceIndexPort for VcePublicationIndex {
    /// Return the unchanged stored binding; no execution fingerprint masquerades as its original identity.
    /// 返回不变的存储绑定；执行指纹不会伪装成原始身份。
    fn binding(&self) -> &VceIndexBinding {
        self.runtime.index.binding()
    }

    /// Forward exact missing-blob inspection; input and output keep the underlying storage contract.
    /// 转发精确缺失块检查；输入输出保持底层存储契约。
    fn find_missing(
        &self,
        request: FindMissingRequest,
    ) -> VceFuture<Result<FindMissingResponse, VcePortError>> {
        self.runtime.index.find_missing(request)
    }

    /// Forward unpublished staging; input and output preserve the admitted generation identity.
    /// 转发未发布暂存；输入输出保留已准入代次身份。
    fn stage(
        &self,
        request: IndexStageBatch,
    ) -> VceFuture<Result<IndexStageReceipt, VcePortError>> {
        self.runtime.index.stage(request)
    }

    /// Forward rollback even after authorization is revoked; returns confirmed staging cleanup.
    /// 授权撤销后仍转发回滚；返回已确认暂存清理。
    fn abort_stage(
        &self,
        request: IndexAbortRequest,
    ) -> VceFuture<Result<IndexAbortReceipt, VcePortError>> {
        self.runtime.index.abort_stage(request)
    }

    /// Serialize final publication with configuration saves, then revalidate before committing.
    /// 将最终发布与配置保存串行化，重新校验后才提交。
    /// `request` is the prepared checkpoint; returns its retained durable receipt or a boundary error.
    /// `request` 是已准备检查点；返回已保留的持久回执或边界错误。
    fn commit(&self, request: IndexCommit) -> VceFuture<Result<CommitReceipt, VcePortError>> {
        // The view owns all state until a started commit settles; cancellation must not detach it.
        // 视图持有全部状态直到已开始提交确定；取消不得使其脱离归属。
        let view = Self {
            runtime: Arc::clone(&self.runtime),
            authority: Arc::clone(&self.authority),
            owner_run_id: self.owner_run_id.clone(),
            committed_read_index: Arc::clone(&self.committed_read_index),
        };
        Box::pin(async move {
            let _transaction = crate::begin_public_ai_configuration_transaction().await;
            view.validate()
                .await
                .map_err(|reason| VcePortError::Rejected {
                    operation: VcePortOperation::Commit,
                    reason,
                })?;
            // Retain the lease before acknowledging the core commit; no post-commit pin can fail.
            // 确认核心提交前保留租约，使提交后不再存在可失败的固定读取步骤。
            let (receipt, read_index) = view
                .runtime
                .index
                .commit_pinned(request, &view.owner_run_id)
                .await?;
            *view.committed_read_index.lock().await = Some(read_index);
            Ok(receipt)
        })
    }

    /// Forward dense reads without taking a model-settings publication lock.
    /// 转发稠密读取，不获取模型设置发布锁。
    fn search_dense(
        &self,
        request: DenseSearchRequest,
    ) -> VceFuture<Result<VceSearchResponse, VcePortError>> {
        self.runtime.index.search_dense(request)
    }

    /// Forward path reads under their original scope and return the unchanged response.
    /// 按原始作用域转发路径读取并返回不变响应。
    fn search_path(
        &self,
        request: PathSearchRequest,
    ) -> VceFuture<Result<VcePathSearchResponse, VcePortError>> {
        self.runtime.index.search_path(request)
    }

    /// Forward exact-symbol reads without changing query or project ownership.
    /// 转发精确符号读取，不改变查询或项目归属。
    fn search_exact(
        &self,
        request: ExactSearchRequest,
    ) -> VceFuture<Result<VceSearchResponse, VcePortError>> {
        self.runtime.index.search_exact(request)
    }

    /// Forward read-only management summary and preserve its storage diagnostics.
    /// 转发只读管理摘要并保留存储诊断。
    fn management_snapshot(
        &self,
        request: VceManagementRequest,
    ) -> VceFuture<Result<VceIndexManagementSnapshot, VcePortError>> {
        self.runtime.index.management_snapshot(request)
    }

    /// Forward one exact management query and return its original result contract.
    /// 转发一个精确管理查询并返回原始结果契约。
    fn management_query(
        &self,
        request: VceManagementQueryRequest,
    ) -> VceFuture<Result<VceManagementQueryResponse, VcePortError>> {
        self.runtime.index.management_query(request)
    }
}
