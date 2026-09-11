//! Exact committed embedding origins used by host-owned compatibility confirmation.
//! 宿主兼容确认使用的精确已提交向量来源。

use super::*;

/// Original index identity and all actual vector producers in the current checkpoint.
/// 原始索引身份及当前检查点中的全部实际向量生产来源。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VceEmbeddingProvenance {
    /// Original physical identity, which a compatibility approval must never overwrite.
    /// 原始物理身份，兼容批准绝不能覆盖它。
    pub original: VceProjectIndexConfig,
    /// Exact committed checkpoint, absent only before first publication.
    /// 精确已提交检查点，仅首次发布前缺失。
    pub checkpoint_version: Option<u64>,
    /// Original identity plus the currently represented vector producer fingerprints.
    /// 原始身份及当前仍然存在的向量生产来源指纹。
    pub source_fingerprints: BTreeSet<String>,
}

/// One atomic metadata read of checkpoint and actual producer integrity.
/// 一次检查点与实际生产来源完整性的原子元数据读取。
#[derive(Deserialize)]
struct ProvenanceRow {
    /// Exact checkpoint observed by the same SQLite statement as its origins.
    /// 与来源在同一 SQLite 语句中观察到的精确检查点。
    checkpoint_version: Option<u64>,
    /// Distinct current sources encoded within that read snapshot.
    /// 在该读取快照中编码的当前去重来源。
    sources_json: String,
    /// Vector-bearing members whose exact production attempt lacks matching provenance.
    /// 精确生产尝试缺少匹配来源的有向量成员数量。
    invalid_origin_count: u64,
}

impl VceDataPlaneHandle {
    /// Read exact project provenance without scanning sources, creating tables or registering AI.
    /// 读取精确项目来源，不扫描源码、不创建表、不注册 AI。
    ///
    /// `project_id` selects host ownership. Returns validated provenance or confirmed absence.
    /// `project_id` 选择宿主归属。返回已验证来源或已确认缺失。
    pub async fn embedding_provenance(
        &self,
        project_id: &str,
    ) -> Result<Option<VceEmbeddingProvenance>, String> {
        // original is loaded from the physical binding, never the newly selected UI route.
        // original 从物理绑定加载，绝不来自界面新选择的路由。
        let Some(original) = self.load_active_project_config(project_id).await? else {
            return Ok(None);
        };
        // sql joins the single immutable producer source through current checkpoint membership.
        // sql 通过当前检查点成员关系联接唯一不可变生产来源。
        let sql = r#"SELECT c.checkpoint_version,
            (SELECT COALESCE(json_group_array(physical_fingerprint), '[]')
             FROM (SELECT DISTINCT physical_fingerprint FROM vce_blob_embedding_origins
                   WHERE namespace_id = b.namespace_id ORDER BY physical_fingerprint)) AS sources_json,
            (SELECT COUNT(*) FROM vce_blobs live
             WHERE live.namespace_id = b.namespace_id AND (live.vector_count > 0 OR live.path_status = 'ready')
               AND (live.embedding_producer_fingerprint IS NULL OR NOT EXISTS(
                   SELECT 1 FROM vce_embedding_generation_origins origin
                   WHERE origin.namespace_id = live.namespace_id AND origin.refresh_id = live.refresh_id
                     AND origin.generation = live.generation
                     AND origin.physical_fingerprint = live.embedding_producer_fingerprint))) AS invalid_origin_count
            FROM vce_project_bindings b LEFT JOIN vce_checkpoints c ON c.namespace_id = b.namespace_id
            WHERE b.project_id = ? AND b.active = 1 AND b.namespace_id = ? AND b.physical_fingerprint = ?
              AND b.root_hash = ? AND b.dimension = ? AND b.preprocessing_version = ?
              AND b.chunker_version = ? AND b.schema_version = ?;"#;
        // rows must still match the binding read above; replacement during inspection is explicit.
        // rows 必须仍匹配上方读取的绑定；检查期间发生替换时明确失败。
        let rows: Vec<ProvenanceRow> = self
            .query_rows(
                sql,
                vec![
                    sqlite_text(project_id),
                    sqlite_text(&original.binding.namespace_id),
                    sqlite_text(&original.physical_fingerprint),
                    sqlite_text(&original.root_hash),
                    SqliteValue::Integer(i64::from(original.dimension)),
                    sqlite_text(&original.preprocessing_version),
                    sqlite_text(&original.chunker_version),
                    SqliteValue::Integer(i64::from(original.binding.schema_version)),
                ],
            )
            .await?;
        let [row] = rows.as_slice() else {
            return Err(String::from(
                "VCE embedding provenance changed during inspection",
            ));
        };
        if row.checkpoint_version == Some(0) || row.invalid_origin_count != 0 {
            return Err(String::from(
                "VCE live vectors have missing or inconsistent embedding origins",
            ));
        }
        // sources preserve the original identity even for a legitimately empty initialized index.
        // sources 即使对合法的已初始化空索引也保留原始身份。
        let mut sources: BTreeSet<String> = serde_json::from_str(&row.sources_json)
            .map_err(|error| format!("Decode VCE embedding provenance: {error}"))?;
        if sources
            .iter()
            .any(|source| source.trim().is_empty() || source.len() > 4096)
        {
            return Err(String::from(
                "VCE embedding provenance contains an invalid source",
            ));
        }
        sources.insert(original.physical_fingerprint.clone());
        Ok(Some(VceEmbeddingProvenance {
            original,
            checkpoint_version: row.checkpoint_version,
            source_fingerprints: sources,
        }))
    }
}

impl VceProjectIndex {
    /// Freeze the actual producer before writing vectors for the exact refresh/generation pair.
    /// 在为精确刷新／代次组合写入向量前冻结实际生产来源。
    ///
    /// `refresh_id` and `generation` identify existing staging. Returns an exact binding or an error.
    /// `refresh_id` 与 `generation` 标识既有暂存。返回精确绑定或错误。
    pub(super) async fn bind_embedding_stage_origin(
        &self,
        refresh_id: &str,
        generation: u64,
    ) -> Result<(), String> {
        self.require_write_access()?;
        self.data_plane.execute(
            "INSERT INTO vce_embedding_stage_origins(namespace_id, refresh_id, generation, physical_fingerprint) VALUES (?, ?, ?, ?) ON CONFLICT(namespace_id, refresh_id, generation) DO NOTHING;",
            vec![sqlite_text(&self.binding.namespace_id), sqlite_text(refresh_id),
                sqlite_u64(generation, "VCE staging origin generation").map_err(|error| error.to_string())?,
                sqlite_text(self.embedding_producer_fingerprint())],
            "绑定 VCE 暂存实际来源失败",
        ).await?;
        self.validate_embedding_stage_origin(refresh_id, generation)
            .await
    }

    /// Require exact staged ownership before another batch or final commit uses its vectors.
    /// 在另一批次或最终提交使用向量前要求精确暂存归属。
    ///
    /// `refresh_id` and `generation` select one attempt. Missing metadata returns an error, never an inferred source.
    /// `refresh_id` 与 `generation` 选择一次尝试。元数据缺失返回错误，绝不推断来源。
    pub(super) async fn validate_embedding_stage_origin(
        &self,
        refresh_id: &str,
        generation: u64,
    ) -> Result<(), String> {
        // rows are read from the sole supported schema, with no pre-ledger compatibility branch.
        // rows 从唯一支持的结构读取，不提供账本前版本兼容分支。
        let rows: Vec<StageOriginRow> = self.data_plane.query_rows(
            "SELECT physical_fingerprint FROM vce_embedding_stage_origins WHERE namespace_id = ? AND refresh_id = ? AND generation = ?;",
            vec![sqlite_text(&self.binding.namespace_id), sqlite_text(refresh_id),
                sqlite_u64(generation, "VCE staging origin generation").map_err(|error| error.to_string())?],
        ).await?;
        match rows.as_slice() {
            [row] if row.physical_fingerprint == self.embedding_producer_fingerprint() => Ok(()),
            _ => Err(String::from(
                "VCE staged vectors belong to a different or missing embedding producer",
            )),
        }
    }
}

/// Exact producer of an unpublished refresh attempt.
/// 未发布刷新尝试的精确生产来源。
#[derive(Deserialize)]
struct StageOriginRow {
    /// Actual provider fingerprint frozen before the first vector write.
    /// 首次向量写入前冻结的实际供应商指纹。
    physical_fingerprint: String,
}

/// Define provenance tables as part of the sole supported project schema, not a lazy upgrade.
/// 将来源表定义为唯一支持的项目结构组成部分，而不是惰性升级。
///
/// No parameters. Returns DDL to run in the authorized project creation transaction.
/// 无参数。返回在获授权项目创建事务中运行的结构定义。
pub(super) fn embedding_origin_schema_script() -> &'static str {
    r#"
CREATE TABLE IF NOT EXISTS vce_embedding_stage_origins (
    namespace_id TEXT NOT NULL, refresh_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation > 0),
    physical_fingerprint TEXT NOT NULL CHECK(length(trim(physical_fingerprint)) > 0),
    PRIMARY KEY(namespace_id, refresh_id, generation),
    FOREIGN KEY(namespace_id, refresh_id, generation) REFERENCES vce_generations(namespace_id, refresh_id, generation) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS vce_embedding_generation_origins (
    namespace_id TEXT NOT NULL, refresh_id TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation > 0),
    physical_fingerprint TEXT NOT NULL CHECK(length(trim(physical_fingerprint)) > 0),
    checkpoint_version INTEGER NOT NULL CHECK(checkpoint_version > 0),
    committed_at_unix_ms INTEGER NOT NULL CHECK(committed_at_unix_ms >= 0),
    PRIMARY KEY(namespace_id, refresh_id, generation),
    FOREIGN KEY(namespace_id, refresh_id, generation) REFERENCES vce_generations(namespace_id, refresh_id, generation)
);
"#
}
