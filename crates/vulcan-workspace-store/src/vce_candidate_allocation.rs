//! Exact candidate-table allocation receipts and cleanup after the owning host has drained writes.
//! 精确候选表分配回执及所属宿主排空写入后的清理。

use super::*;
use vulcan_lancedb_engine::types::LanceDbDropTableInput;

/// One of the two vector tables whose creation must be acknowledged independently.
/// 需要独立确认创建的两张向量表之一。
#[derive(Clone, Copy)]
pub(super) enum CandidateVectorTable {
    /// Code-content vectors.
    /// 代码内容向量。
    Content,
    /// Code-path vectors.
    /// 代码路径向量。
    Path,
}

impl CandidateVectorTable {
    /// Return the fixed schema column for this table's acknowledged allocation flag.
    /// 返回此表已确认分配标志的固定结构列。
    /// No parameters; the result never contains user-controlled SQL text.
    /// 无参数；结果绝不包含用户控制的 SQL 文本。
    fn column(self) -> &'static str {
        match self {
            Self::Content => "content_created",
            Self::Path => "path_created",
        }
    }
}

/// Persisted proof of successful creation, separate from the mere intention to allocate a table.
/// 成功创建的持久证明，与仅打算分配某张表的意图分离。
#[derive(Deserialize)]
struct CandidateAllocationRow {
    /// One only after content creation was acknowledged; zero does not prove that an uncertain request did nothing.
    /// 仅内容创建得到确认后为一；零不证明结果不明的请求没有执行任何操作。
    content_created: i64,
    /// One only after path creation was acknowledged.
    /// 仅路径创建得到确认后为一。
    path_created: i64,
}

impl VceDataPlaneHandle {
    /// Record successful non-overwriting table creation under the exact reserved candidate namespace.
    /// 在精确预留候选命名空间下记录成功且未覆盖的表创建。
    /// `namespace` is the reserved owner and `table` selects a fixed flag; returns acknowledgement or explicit failure.
    /// `namespace` 为预留所有者，`table` 选择固定标志；返回确认或明确失败。
    pub(super) async fn record_candidate_table_created(
        &self,
        namespace: &str,
        table: CandidateVectorTable,
    ) -> Result<(), String> {
        // namespace/column are exact ownership and a fixed schema identifier, not a table-name guess.
        // namespace/column 是精确归属和固定结构标识，不是猜测的表名。
        let namespace = sqlite_string_literal(namespace);
        let column = table.column();
        // Even cancellation during creation must retain the acknowledged allocation for later drained cleanup.
        // 即使取消发生于创建期间，也必须保留已确认分配，以便稍后排空清理。
        let script = format!(
            "BEGIN IMMEDIATE;\nINSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid) VALUES ('{namespace}', CASE WHEN EXISTS(SELECT 1 FROM vce_rebuild_candidates c JOIN vce_project_bindings b ON b.namespace_id = c.namespace_id WHERE c.namespace_id = '{namespace}' AND c.state IN ('allocating', 'aborted') AND b.active = 0 AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints WHERE namespace_id = b.namespace_id)) THEN 1 ELSE 0 END);\nUPDATE vce_rebuild_candidates SET {column} = 1 WHERE namespace_id = '{namespace}';\nDELETE FROM vce_commit_guards WHERE namespace_id = '{namespace}';\nCOMMIT;"
        );
        self.execute(
            &script,
            Vec::new(),
            "Failed to record the VCE candidate table creation receipt",
        )
        .await
    }
}

impl VceProjectIndex {
    /// Remove only tables whose creation by this exact aborted job was durably acknowledged.
    /// 仅移除此精确已中止作业曾持久确认创建的表。
    /// `base` is the admitted owner; the host must first drain allocation, revoke publication and abort staging.
    /// `base` 为已准入所有者；宿主必须先排空分配、撤销发布并中止暂存。
    /// Returns after known tables are removed; unacknowledged or colliding tables are never guessed or deleted.
    /// 在已知表移除后返回；绝不猜测或删除未确认表及冲突表。
    pub async fn cleanup_cancelled_rebuild_candidate(
        &self,
        base: &VceRebuildCandidateBase,
    ) -> Result<(), String> {
        self.require_write_access()?;
        // rows prove the immutable physical names, exact cancelled owner, and absence of any committed checkpoint.
        // rows 证明不可变物理名称、精确取消所有者以及不存在任何已提交检查点。
        let rows: Vec<CandidateAllocationRow> = self.data_plane.query_rows(
            "SELECT c.content_created, c.path_created FROM vce_rebuild_candidates c JOIN vce_project_bindings b ON b.namespace_id = c.namespace_id WHERE c.namespace_id = ? AND c.base_namespace_id = ? AND c.base_checkpoint_version = ? AND c.job_id = ? AND c.state = 'aborted' AND b.project_id = ? AND b.active = 0 AND b.content_table = ? AND b.path_table = ? AND NOT EXISTS(SELECT 1 FROM vce_checkpoints WHERE namespace_id = b.namespace_id) AND NOT EXISTS(SELECT 1 FROM vce_generations WHERE namespace_id = b.namespace_id AND state = 'staging');",
            vec![sqlite_text(&self.binding.namespace_id), sqlite_text(&base.namespace_id), sqlite_u64_raw(base.checkpoint_version, "VCE cancelled predecessor checkpoint")?, sqlite_text(&base.job_id), sqlite_text(&self.project_id), sqlite_text(&self.content_table), sqlite_text(&self.path_table)],
        ).await?;
        let [allocation] = rows.as_slice() else {
            return Err(String::from(
                "VCE candidate table cleanup requires the exact revoked owner and drained staging",
            ));
        };
        for (table, created, name) in [
            (
                CandidateVectorTable::Content,
                allocation.content_created,
                self.content_table.as_str(),
            ),
            (
                CandidateVectorTable::Path,
                allocation.path_created,
                self.path_table.as_str(),
            ),
        ] {
            if created == 0 {
                continue;
            }
            if created != 1 {
                return Err(String::from(
                    "VCE candidate allocation receipt contains an invalid creation flag",
                ));
            }
            let engine = self.data_plane.vector_engine().await?;
            engine
                .drop_table(LanceDbDropTableInput {
                    table_name: name.to_string(),
                })
                .await
                .map_err(|error| {
                    format!("Drop acknowledged VCE candidate table `{name}`: {error}")
                })?;
            // column is fixed by the enum; its receipt is cleared only after acknowledged deletion.
            // column 由枚举固定；仅在确认删除之后清除其回执。
            let column = table.column();
            self.data_plane.execute(
                &format!("UPDATE vce_rebuild_candidates SET {column} = 0 WHERE namespace_id = ? AND job_id = ? AND state = 'aborted';"),
                vec![sqlite_text(&self.binding.namespace_id), sqlite_text(&base.job_id)],
                "Failed to confirm the VCE candidate table cleanup",
            ).await?;
        }
        Ok(())
    }
}

/// Verify partial allocation ownership and protect a pre-existing table whose name collides with the target.
/// 验证部分分配归属，并保护与目标名称冲突的已有表。
/// Inputs belong to the real temporary fixture; returns after exact cleanup and predecessor checks.
/// 输入属于真实临时夹具；精确清理及前驱检查后返回。
#[cfg(test)]
pub(super) async fn verify_partial_candidate_allocation(
    factory: VceDataPlaneFactory,
    data_plane: VceDataPlaneHandle,
    original: VceProjectIndex,
    mut config: VceProjectIndexConfig,
    mut base: VceRebuildCandidateBase,
) {
    // Isolate the intentional collision from every other fixture publication.
    // 将有意碰撞与其他夹具发布隔离。
    config.binding.namespace_id.push_str(":partial");
    config.binding.checkpoint_chain_id.push_str(":partial");
    config.vector_space_id.push_str(":partial");
    config.binding.embedding_space.vector_space_id = config.vector_space_id.clone();
    base.job_id.push_str(":partial");
    let store = original.job_store();
    let queued = jobs::replacement_test_receipt(config.clone(), base.clone());
    store
        .register(&queued)
        .await
        .expect("register partial allocation owner");
    let mut running = queued.clone();
    running.sequence += 1;
    running.state = VceJobState::Running;
    store
        .update(&queued, &running)
        .await
        .expect("start partial allocation owner");
    // path is created by the fixture, not by the candidate allocator.
    // path 由夹具创建，而非候选分配器创建。
    let path = vector_table_name(
        "path",
        &config.vector_space_id,
        &config.physical_fingerprint,
        &config.preprocessing_version,
        &config.chunker_version,
        config.dimension,
    );
    let content = vector_table_name(
        "content",
        &config.vector_space_id,
        &config.physical_fingerprint,
        &config.preprocessing_version,
        &config.chunker_version,
        config.dimension,
    );
    data_plane
        .ensure_vector_table(&path, config.dimension)
        .await
        .expect("create fixture-owned conflicting table");
    assert!(
        factory
            .find_rebuild_candidate(&config, &base)
            .await
            .expect("lookup before allocation")
            .is_none()
    );
    assert!(
        factory
            .prepare_rebuild_candidate(config.clone(), base.clone())
            .await
            .is_err()
    );
    // Content creation remains acknowledged even though creating the second table failed.
    // 即使第二张表创建失败，内容表创建仍保留确认。
    let allocations: Vec<CountRow> = data_plane.query_rows(
        "SELECT COUNT(*) AS count FROM vce_rebuild_candidates WHERE namespace_id = ? AND job_id = ? AND state = 'allocating' AND content_created = 1 AND path_created = 0;",
        vec![sqlite_text(&config.binding.namespace_id), sqlite_text(&base.job_id)],
    ).await.expect("read partial creation receipt");
    assert!(matches!(allocations.as_slice(), [row] if row.count == 1));
    let candidate = factory
        .find_rebuild_candidate(&config, &base)
        .await
        .expect("find interrupted allocation")
        .expect("recoverable allocation");
    assert!(
        candidate
            .admit_rebuild_candidate_writer(base.clone())
            .await
            .is_err()
    );
    assert!(
        candidate
            .cleanup_cancelled_rebuild_candidate(&base)
            .await
            .is_err()
    );
    let mut foreign = base.clone();
    foreign.job_id.push_str(":foreign");
    assert!(candidate.cancel_rebuild_candidate(&foreign).await.is_err());
    candidate
        .cancel_rebuild_candidate(&base)
        .await
        .expect("revoke interrupted allocation");
    candidate
        .abort_refresh_staging(&base.job_id)
        .await
        .expect("drain empty attempt");
    assert!(
        candidate
            .cleanup_cancelled_rebuild_candidate(&foreign)
            .await
            .is_err()
    );
    candidate
        .cleanup_cancelled_rebuild_candidate(&base)
        .await
        .expect("remove only acknowledged content table");
    assert!(
        data_plane
            .ensure_vector_table(&path, config.dimension)
            .await
            .is_err(),
        "preserve the fixture's conflicting path table"
    );
    // Recreating content proves removal; this new table now belongs to the fixture rather than the old job.
    // 重新创建内容表证明已删除；此新表现在属于夹具，而非旧作业。
    data_plane
        .ensure_vector_table(&content, config.dimension)
        .await
        .expect("candidate content was removed");
    candidate
        .cleanup_cancelled_rebuild_candidate(&base)
        .await
        .expect("repeat exact cleanup");
    assert!(
        data_plane
            .ensure_vector_table(&content, config.dimension)
            .await
            .is_err(),
        "cleared flags must preserve another creator's table"
    );
    let cleaned: Vec<CountRow> = data_plane.query_rows(
        "SELECT COUNT(*) AS count FROM vce_rebuild_candidates WHERE namespace_id = ? AND state = 'aborted' AND content_created = 0 AND path_created = 0;",
        vec![sqlite_text(&config.binding.namespace_id)],
    ).await.expect("read acknowledged cleanup");
    assert!(matches!(cleaned.as_slice(), [row] if row.count == 1));
    assert_eq!(
        data_plane
            .load_active_project_config(&original.project_id)
            .await
            .expect("read predecessor")
            .expect("active predecessor")
            .binding
            .namespace_id,
        base.namespace_id
    );
    let mut failed = running.clone();
    failed.sequence += 1;
    failed.state = VceJobState::Failed;
    failed.error = Some(String::from("intentional fixture path collision"));
    store
        .update(&running, &failed)
        .await
        .expect("finish fixture owner");
    // Only the fixture owns these tables now; explicit deletion is confined to its temporary physical binding.
    // 这两张表现在仅属于夹具；明确删除限定在其临时物理绑定内。
    for table in [&content, &path] {
        data_plane
            .vector_engine()
            .await
            .expect("open fixture vector engine")
            .drop_table(LanceDbDropTableInput {
                table_name: table.to_string(),
            })
            .await
            .expect("release fixture-owned table");
    }
}
