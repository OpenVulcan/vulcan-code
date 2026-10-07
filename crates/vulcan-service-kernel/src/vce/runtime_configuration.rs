//! Read-only project configuration preparation before model registration or index allocation.
//! 在模型注册或索引分配之前进行只读项目配置准备。

use super::*;

/// Frozen engine, source and physical-index contracts; preparation grants no execution authority.
/// 冻结的引擎、源码及物理索引契约；准备本身不授予执行权限。
pub(super) struct VcePreparedRuntimeConfiguration {
    /// Core configuration used by the eventual admitted worker.
    /// 最终已准入工作任务使用的核心配置。
    pub(super) engine: vulcan_context_engine::VceConfig,
    /// Canonical source adapter with exact project filters, without a scanned manifest.
    /// 带精确项目过滤规则的规范源码适配器，不包含已扫描清单。
    pub(super) source: Arc<VceProjectSourceAdapter>,
    /// Host-owned physical allocation contract, not an opened database handle.
    /// 宿主持有的物理分配契约，并非已打开数据库句柄。
    pub(super) index: VceProjectIndexConfig,
}

impl VcePreparedRuntimeConfiguration {
    /// Bind provider execution to an existing stored logical space, or the explicitly prepared new target.
    /// 将供应商执行绑定到既有存储逻辑空间，或明确准备的新目标。
    /// `original` is authoritative persisted configuration when present. Returns exact execution identity.
    /// `original` 存在时为权威持久配置。返回精确执行身份。
    pub(super) fn execution_space(
        &self,
        original: Option<&VceProjectIndexConfig>,
    ) -> Result<vulcan_context_engine::EmbeddingSpace, String> {
        // execution keeps current producer/dimension facts; only the stored logical namespace is reused.
        // execution 保留当前生产者／维度事实；仅复用存储逻辑命名空间。
        let mut execution = self.engine.embedding_space.clone();
        if let Some(original) = original {
            if original.project_id != self.index.project_id
                || original.root_path != self.index.root_path
                || original.root_hash != self.index.root_hash
            {
                return Err(String::from(
                    "VCE stored execution space belongs to a different project root",
                ));
            }
            execution.vector_space_id = original.vector_space_id.clone();
        }
        Ok(execution)
    }
}

/// Prepare deterministic project contracts without registering models, scanning files or opening storage.
/// 准备确定性项目契约，不注册模型、不扫描文件、不打开存储。
/// Project/root/settings and the admitted embedding facts are inputs; `replacement_job` only scopes a candidate.
/// 项目／根目录／设置及已准入向量事实为输入；`replacement_job` 仅限定候选作用域。
/// Returns frozen configuration; the caller must separately prove durable job ownership before execution.
/// 返回冻结配置；调用方在执行前必须另行证明持久作业归属。
pub(super) fn prepare_runtime_configuration(
    project_id: &str,
    project_root: &Path,
    settings: &ProjectVceSettings,
    dimension: u32,
    fingerprint: &str,
    replacement_job: Option<&str>,
) -> Result<VcePreparedRuntimeConfiguration, String> {
    validate_project_identity(project_id, settings.revision)?;
    settings.validate()?;
    if replacement_job.is_some_and(|job| job.trim().is_empty() || job.len() > 512) {
        return Err(String::from(
            "VCE replacement configuration requires an exact bounded job identity",
        ));
    }
    // preliminary_limits preserve the existing root/filter validation before core limits are available.
    // preliminary_limits 在核心限制可用之前保留既有根目录／过滤验证。
    let preliminary_limits = VceSourceLimits::new(settings.max_file_bytes, 64)?;
    // preliminary validates only root metadata and ignore syntax; activation does not enumerate project files.
    // preliminary 仅验证根目录元数据及忽略语法；激活不枚举项目文件。
    let preliminary = VceProjectSourceAdapter::activate(
        project_root,
        preliminary_limits,
        &settings.extra_ignore_patterns,
    )?;
    // root_path and root_hash preserve the host's single canonical ownership identity.
    // root_path 与 root_hash 保留宿主唯一规范归属身份。
    let root_path = crate::normalized_workspace_path_text_from_path(preliminary.project_root());
    let root_hash = hex::encode(Sha256::digest(root_path.as_bytes()));
    // suffix isolates a confirmed replacement attempt while leaving ordinary project identities unchanged.
    // suffix 隔离已确认替换尝试，同时保持普通项目身份不变。
    let suffix = replacement_job
        .map(|job| format!(":rebuild:{job}"))
        .unwrap_or_default();
    // chain and vector_space_id are stable for replay of the same job and distinct for another attempt.
    // chain 与 vector_space_id 对同一作业重放稳定，对另一尝试保持独立。
    let chain = format!("project:{project_id}:root:{root_hash}{suffix}");
    let vector_space_id = format!("code-semantic:{project_id}:{root_hash}{suffix}");
    // space carries explicit readiness facts; actual HostAI registration must match it before execution.
    // space 携带明确就绪事实；执行前实际宿主模型注册必须与其匹配。
    let space = vulcan_context_engine::EmbeddingSpace {
        vector_space_id: vector_space_id.clone(),
        physical_fingerprint: Some(fingerprint.to_owned()),
        dimension: usize::try_from(dimension)
            .map_err(|_| String::from("VCE dimension exceeds host range"))?,
        preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
        chunker_version: String::from(VCE_CHUNKER_VERSION),
    };
    // engine applies exactly the existing project resource and retrieval settings, without selecting providers.
    // engine 精确应用既有项目资源及检索设置，不选择供应商。
    let mut engine = vulcan_context_engine::VceConfig::new(chain.clone(), space.clone())
        .map_err(|error| error.to_string())?;
    engine.indexing.max_file_bytes = settings.max_file_bytes;
    engine.embedding_max_concurrency = settings.embedding_max_concurrency;
    engine.query_max_concurrency = settings.query_max_concurrency;
    engine.retrieval.coverage.max_results = settings.max_results;
    engine.retrieval.coverage.max_chars = settings.max_context_chars;
    engine.validate().map_err(|error| error.to_string())?;
    // source adopts validated core batch limits but still performs no scan or model call.
    // source 采用已验证的核心批次限制，但仍不扫描或调用模型。
    let source = Arc::new(VceProjectSourceAdapter::activate(
        preliminary.project_root(),
        VceSourceLimits::new(
            engine.indexing.max_file_bytes,
            engine.index_stage_blob_limit,
        )?,
        &settings.extra_ignore_patterns,
    )?);
    // index describes the exact physical target; constructing it neither creates nor activates that target.
    // index 描述精确物理目标；构造它不会创建或激活该目标。
    let index = VceProjectIndexConfig {
        project_id: project_id.to_owned(),
        root_path,
        root_hash,
        binding: VceIndexBinding::new(
            engine.index_namespace_id.clone(),
            chain,
            engine.index_schema_version,
            space,
        )
        .map_err(|error| error.to_string())?,
        vector_space_id,
        physical_fingerprint: fingerprint.to_owned(),
        preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
        chunker_version: String::from(VCE_CHUNKER_VERSION),
        dimension,
        config_revision: settings.revision,
    };
    index.validate()?;
    Ok(VcePreparedRuntimeConfiguration {
        engine,
        source,
        index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check normal identity stability and job-specific replacement isolation without any external services.
    /// 无外部服务地检查普通身份稳定性与作业专属替换隔离。
    /// Creates only a temporary empty project; assertions return no value and perform no indexing.
    /// 仅创建临时空项目；断言无返回值，且不执行索引。
    #[test]
    fn preparation_preserves_normal_identity_and_isolates_replacement_jobs() {
        // root is an empty existing directory; preparation must leave it empty.
        // root 是已存在的空目录；准备必须保持其为空。
        let root = tempfile::tempdir().expect("create configuration fixture");
        // settings and fingerprint are explicit host facts, not inferred provider defaults.
        // settings 与 fingerprint 为明确宿主事实，并非推断供应商默认值。
        let settings = ProjectVceSettings {
            enabled: true,
            ..ProjectVceSettings::default()
        };
        let fingerprint = format!("sha256:{}", "a".repeat(64));
        // normal retains the preexisting project/root identity.
        // normal 保留既有项目／根目录身份。
        let normal = prepare_runtime_configuration(
            "fixture-project",
            root.path(),
            &settings,
            1024,
            &fingerprint,
            None,
        )
        .expect("prepare normal configuration");
        assert_eq!(
            normal.index.vector_space_id,
            format!("code-semantic:fixture-project:{}", normal.index.root_hash)
        );
        assert_eq!(
            normal.index.binding.checkpoint_chain_id,
            format!("project:fixture-project:root:{}", normal.index.root_hash)
        );
        assert_eq!(
            normal.engine.embedding_space,
            normal.index.binding.embedding_space
        );
        // first and replay use the same durable identity; second must never reuse either target namespace or table space.
        // first 与 replay 使用同一持久身份；second 绝不能复用目标命名空间或表空间。
        let first = prepare_runtime_configuration(
            "fixture-project",
            root.path(),
            &settings,
            1024,
            &fingerprint,
            Some("job-one"),
        )
        .expect("prepare replacement");
        let replay = prepare_runtime_configuration(
            "fixture-project",
            root.path(),
            &settings,
            1024,
            &fingerprint,
            Some("job-one"),
        )
        .expect("prepare replacement replay");
        let second = prepare_runtime_configuration(
            "fixture-project",
            root.path(),
            &settings,
            2048,
            &fingerprint,
            Some("job-two"),
        )
        .expect("prepare distinct replacement");
        assert_eq!(first.index, replay.index);
        assert_ne!(
            normal.index.binding.namespace_id,
            first.index.binding.namespace_id
        );
        assert_ne!(
            first.index.binding.namespace_id,
            second.index.binding.namespace_id
        );
        assert_ne!(first.index.vector_space_id, second.index.vector_space_id);
        assert_eq!(normal.index.root_path, second.index.root_path);
        assert_eq!(normal.index.root_hash, second.index.root_hash);
        // Restoring a published replacement must reuse its stored logical ID, not regenerate the ordinary one.
        // 恢复已发布替换库必须复用其存储逻辑编号，不能重新生成普通编号。
        let restored_execution = normal
            .execution_space(Some(&first.index))
            .expect("restore exact stored execution scope");
        assert_eq!(
            restored_execution.vector_space_id,
            first.index.vector_space_id
        );
        vulcan_context_engine::VceEmbeddingExecutionContract::new(
            first.index.binding,
            restored_execution,
            "fixture-approved-origin",
        )
        .expect("restored candidate scope remains compatible");
        assert_eq!(second.engine.embedding_space.dimension, 2048);
        assert_eq!(
            std::fs::read_dir(root.path())
                .expect("inspect untouched root")
                .count(),
            0
        );
    }

    /// Reject invalid replacement identity and missing roots without allocating directories or guessing defaults.
    /// 拒绝无效替换身份及缺失根目录，不分配目录或猜测默认值。
    /// Temporary fixture inputs produce explicit errors; returns no value.
    /// 临时夹具输入产生明确错误；无返回值。
    #[test]
    fn preparation_rejects_invalid_or_missing_admission_facts() {
        // root contains no source or index state.
        // root 不包含源码或索引状态。
        let root = tempfile::tempdir().expect("create rejection fixture");
        let settings = ProjectVceSettings::default();
        let fingerprint = format!("sha256:{}", "a".repeat(64));
        assert!(
            prepare_runtime_configuration(
                "fixture-project",
                root.path(),
                &settings,
                1024,
                &fingerprint,
                Some(" ")
            )
            .is_err()
        );
        assert!(
            prepare_runtime_configuration(
                "fixture-project",
                root.path(),
                &settings,
                0,
                &fingerprint,
                None
            )
            .is_err()
        );
        // missing remains absent after failed preparation.
        // missing 在准备失败后保持缺失。
        let missing = root.path().join("missing");
        assert!(
            prepare_runtime_configuration(
                "fixture-project",
                &missing,
                &settings,
                1024,
                &fingerprint,
                None
            )
            .is_err()
        );
        assert!(!missing.exists());
    }

    /// Preserve current producer facts during restore and reject cross-root or different-dimensional execution.
    /// 恢复时保留当前生产者事实，拒绝跨根目录或不同维度的执行。
    /// Uses isolated metadata-only configurations; assertions return no value and call no model or database.
    /// 使用隔离的纯元数据配置；断言无返回值，不调用模型或数据库。
    #[test]
    fn restored_scope_does_not_replace_producer_facts_or_bypass_compatibility() {
        // root/settings identify one explicitly configured project without source or index contents.
        // root/settings 标识一个明确配置的项目，不含源码或索引内容。
        let root = tempfile::tempdir().expect("create restore contract fixture");
        let settings = ProjectVceSettings::default();
        // original and current fingerprints intentionally represent distinct providers of a compatible space.
        // original 与 current 指纹有意表示兼容空间的不同供应商。
        let original_fingerprint = format!("sha256:{}", "a".repeat(64));
        let current_fingerprint = format!("sha256:{}", "b".repeat(64));
        // original is an already published job-scoped target, not the ordinary project namespace.
        // original 为已发布的作业专属目标，并非普通项目命名空间。
        let original = prepare_runtime_configuration(
            "fixture-project",
            root.path(),
            &settings,
            1024,
            &original_fingerprint,
            Some("published-job"),
        )
        .expect("prepare original target");
        // current retains its selected producer; only an explicit grant may connect it to stored vectors.
        // current 保留所选生产者；只有明确授权才可将其连接到存储向量。
        let current = prepare_runtime_configuration(
            "fixture-project",
            root.path(),
            &settings,
            1024,
            &current_fingerprint,
            None,
        )
        .expect("prepare current producer");
        let execution = current
            .execution_space(Some(&original.index))
            .expect("resolve stored scope");
        assert_eq!(
            execution.physical_fingerprint.as_deref(),
            Some(current_fingerprint.as_str())
        );
        assert_eq!(original.index.physical_fingerprint, original_fingerprint);
        assert_eq!(execution.vector_space_id, original.index.vector_space_id);
        assert!(
            vulcan_context_engine::VceEmbeddingExecutionContract::new(
                original.index.binding.clone(),
                execution.clone(),
                "",
            )
            .is_err(),
            "restored scope does not supply its own compatibility authorization"
        );
        assert!(
            vulcan_context_engine::VceEmbeddingExecutionContract::new(
                original.index.binding.clone(),
                execution,
                "fixture-explicit-compatible-grant",
            )
            .is_ok()
        );
        // different_dimension must remain different: reusing an ID must never silently borrow the old dimension.
        // different_dimension 必须保持不同：复用编号绝不能静默借用旧维度。
        let different_dimension = prepare_runtime_configuration(
            "fixture-project",
            root.path(),
            &settings,
            2048,
            &current_fingerprint,
            None,
        )
        .expect("prepare explicitly changed dimension");
        let execution = different_dimension
            .execution_space(Some(&original.index))
            .expect("resolve identity only");
        assert_eq!(execution.dimension, 2048);
        assert!(
            vulcan_context_engine::VceEmbeddingExecutionContract::new(
                original.index.binding.clone(),
                execution,
                "fixture-cannot-authorize-different-dimensions",
            )
            .is_err()
        );
        // another_root is a separately canonicalized root, even when the stable project ID is unchanged.
        // another_root 是单独规范化的根目录，即使稳定项目编号没有改变。
        let another_root = tempfile::tempdir().expect("create unrelated root");
        let relocated = prepare_runtime_configuration(
            "fixture-project",
            another_root.path(),
            &settings,
            1024,
            &current_fingerprint,
            None,
        )
        .expect("prepare relocated project");
        assert!(relocated.execution_space(Some(&original.index)).is_err());
        // another_project cannot inherit the same physical root's logical space without matching ownership.
        // another_project 在归属不匹配时不能继承同一物理根目录的逻辑空间。
        let another_project = prepare_runtime_configuration(
            "different-project",
            root.path(),
            &settings,
            1024,
            &current_fingerprint,
            None,
        )
        .expect("prepare unrelated project identity");
        assert!(
            another_project
                .execution_space(Some(&original.index))
                .is_err()
        );
    }
}
