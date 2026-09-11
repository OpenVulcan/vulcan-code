//! Immutable code versions and exact checkpoint read leases in the VCE-owned database.
//! VCE 自有数据库中的不可变代码版本与精确检查点读取租约。

use super::{
    Checkpoint, CountRow, VcePortError, VcePortOperation, VceProjectIndex,
    current_unix_epoch_millis_i64, lance_string_literal, lance_string_set, port_rejected,
    sqlite_text, sqlite_u64_raw,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Exact immutable producer identity for one member of a committed checkpoint.
/// 已提交检查点中一个成员的精确不可变生产身份。
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct ReadMemberRow {
    /// Stable content-and-path identity supplied by the semantic core.
    /// 语义核心提供的稳定内容与路径身份。
    pub(super) blob_name: String,
    /// Exact refresh attempt; generation numbers alone may be reused after a failed attempt.
    /// 精确刷新尝试；失败尝试之后可能复用代次数值。
    pub(super) refresh_id: String,
    /// Workspace generation associated with the immutable producer attempt.
    /// 与不可变生产尝试关联的工作区代次。
    pub(super) generation: u64,
}

/// Predecessor member together with its exact vector provenance, read before publication.
/// 发布前读取的前驱成员及其精确向量来源。
#[derive(Deserialize)]
struct ReadMemberProvenanceRow {
    /// Immutable producer identity of the retained member.
    /// 保留成员的不可变生产身份。
    #[serde(flatten)]
    member: ReadMemberRow,
    /// Actual vector producer; absent for members without content or path vectors.
    /// 实际向量生产者；没有内容或路径向量的成员缺失此值。
    physical_fingerprint: Option<String>,
    /// Nonzero when a vector-bearing member lacks exact generation provenance.
    /// 有向量成员缺少精确代次来源时非零。
    invalid_origin: u64,
}

/// Shared publication lease whose final reference releases durable storage protection.
/// 在最后一个引用释放时解除持久存储保护的共享发布租约。
pub(super) struct VceReadLease {
    /// Unpinned index retains the exact physical binding and performs deferred cleanup.
    /// 未固定索引保留精确物理绑定并执行延迟清理。
    index: VceProjectIndex,
    /// Unique durable lease identity; never inferred from a project name or timestamp.
    /// 唯一持久租约身份；绝不从项目名或时间戳推断。
    lease_id: String,
    /// Exact host run that admitted this publication.
    /// 准入本次发布的精确宿主运行身份。
    owner_run_id: String,
    /// Core checkpoint fixed for all channels of the same query.
    /// 为同一查询的全部通道固定的核心检查点。
    pub(super) checkpoint: Checkpoint,
    /// Each member references its own production attempt, including unchanged older members.
    /// 每个成员引用自己的生产尝试，包含未变化的较早成员。
    pub(super) members: BTreeMap<String, ReadMemberRow>,
    /// Exact source-space facts captured with this publication, independent from future commits.
    /// 与本次发布共同捕获的精确来源空间事实，独立于未来提交。
    pub(super) provenance: super::VceEmbeddingProvenance,
}

impl VceReadLease {
    /// Generate the durable lease insertion inside the checkpoint's existing commit transaction.
    /// 在检查点既有提交事务内部生成持久租约插入语句。
    /// `created_at` is the transaction timestamp; returns escaped SQL without executing I/O.
    /// `created_at` 是事务时间戳；返回已转义的 SQL，不执行输入输出。
    pub(super) fn commit_clause(&self, created_at: i64) -> String {
        format!(
            "INSERT OR ROLLBACK INTO vce_read_leases(namespace_id, lease_id, owner_run_id, checkpoint_version, created_at_unix_ms) VALUES ('{}', '{}', '{}', {}, {created_at});\n",
            super::sqlite_string_literal(&self.index.binding.namespace_id),
            super::sqlite_string_literal(&self.lease_id),
            super::sqlite_string_literal(&self.owner_run_id),
            self.checkpoint.version,
        )
    }

    /// Convert a transaction-acknowledged lease into its immutable query handle without further I/O.
    /// 将事务已确认的租约转换为不可变查询句柄，不再执行输入输出。
    /// Consumes the exact lease and returns its physically retained read-only index.
    /// 消费精确租约，返回保留物理绑定的只读索引。
    pub(super) fn into_read_index(self) -> VceProjectIndex {
        // read_index shares this lease with all queries admitted from one publication.
        // read_index 与从同次发布准入的全部查询共享此租约。
        let mut read_index = self.index.clone();
        read_index.read_view = Some(Arc::new(self));
        read_index
    }

    /// Validate allowed members and build a producer-exact LanceDB scope predicate.
    /// 验证允许成员并构建精确到生产尝试的 LanceDB 范围谓词。
    ///
    /// `allowed` is the core query subset. Returns a grouped predicate, or an ownership error;
    /// an empty scope explicitly matches no rows rather than the entire namespace.
    /// `allowed` 为核心查询子集。返回分组谓词或归属错误；
    /// 空范围明确不匹配任何行，而不是匹配整个命名空间。
    pub(super) fn vector_scope(&self, allowed: &BTreeSet<String>) -> Result<String, String> {
        // groups reduce repeated batch identities without discarding exact Blob ownership.
        // groups 减少重复批次身份，不丢弃精确块对象归属。
        let mut groups: BTreeMap<(&str, u64), BTreeSet<String>> = BTreeMap::new();
        for blob in allowed {
            // member is resolved only from the pinned map, never the latest checkpoint.
            // member 仅从固定映射解析，绝不读取最新检查点。
            let member = self
                .members
                .get(blob)
                .ok_or_else(|| String::from("VCE query scope is outside its pinned checkpoint"))?;
            groups
                .entry((&member.refresh_id, member.generation))
                .or_default()
                .insert(blob.clone());
        }
        if groups.is_empty() {
            return Ok(String::from("1 = 0"));
        }
        Ok(groups
            .into_iter()
            .map(|((refresh, generation), blobs)| {
                format!(
                    "(refresh_id = '{}' AND generation = {generation} AND blob_name IN ({}))",
                    lance_string_literal(refresh),
                    lance_string_set(&blobs),
                )
            })
            .collect::<Vec<_>>()
            .join(" OR "))
    }
}

/// Exact release identity recorded only after the final in-process read reference ends.
/// 仅在最后一个进程内读取引用结束后记录的精确释放身份。
#[derive(Clone, PartialEq, Eq)]
pub(super) struct VceReleasedReadLease {
    /// Immutable namespace whose persistent lease is being released.
    /// 正在释放持久租约的不可变命名空间。
    namespace_id: String,
    /// Exact durable lease key, never a generation-wide deletion scope.
    /// 精确持久租约键，绝不是代次范围删除条件。
    lease_id: String,
    /// Host run owning this completed lease.
    /// 持有本已完成租约的宿主运行身份。
    owner_run_id: String,
}

impl Drop for VceReadLease {
    /// Queue exact lease release without spawning detached work or retaining an extra binding.
    /// 排入精确租约释放，不启动脱离管理的工作或保留额外绑定。
    ///
    /// Takes the final guard reference and returns nothing. A poisoned queue retains the durable
    /// lease and logs an explicit failure; cleanup never guesses that protection has expired.
    /// 接收最后一个守卫引用，无返回值。队列中毒时保留持久租约并明确记录错误；
    /// 清理绝不猜测保护已过期。
    fn drop(&mut self) {
        match self.index.data_plane.read_releases.lock() {
            Ok(mut releases) => releases.push(VceReleasedReadLease {
                namespace_id: self.index.binding.namespace_id.clone(),
                lease_id: self.lease_id.clone(),
                owner_run_id: self.owner_run_id.clone(),
            }),
            Err(error) => eprintln!("[vce] exact read lease remains protected: {error}"),
        }
    }
}

impl super::VceDataPlaneHandle {
    /// Flush released leases before another read admission, cleanup or binding teardown.
    /// 在另一读取准入、清理或绑定拆卸之前提交已释放租约。
    ///
    /// No parameters. Returns exact release errors; unacknowledged batches survive cancellation.
    /// 无参数。返回精确释放错误；尚未确认的批次在取消后仍然保留。
    pub(super) async fn flush_released_read_leases(&self) -> Result<(), String> {
        // Keep identities queued until acknowledgement so dropping this future cannot lose a release.
        // 身份保留在队列中直到确认，避免丢弃此异步过程时遗失释放记录。
        let releases = {
            let queue = self
                .read_releases
                .lock()
                .map_err(|_| String::from("VCE read release queue is poisoned"))?;
            queue.clone()
        };
        if releases.is_empty() {
            return Ok(());
        }
        // items bind every namespace, lease and owner as data to prevent broad deletion.
        // items 将每个命名空间、租约和所有者作为数据绑定，防止宽泛删除。
        let items = releases
            .iter()
            .map(|lease| {
                vec![
                    sqlite_text(&lease.namespace_id),
                    sqlite_text(&lease.lease_id),
                    sqlite_text(&lease.owner_run_id),
                ]
            })
            .collect();
        self.execute_batch(
            "DELETE FROM vce_read_leases WHERE namespace_id = ? AND lease_id = ? AND owner_run_id = ?;",
            items, "释放 VCE 检查点读取租约失败",
        ).await?;
        // Concurrent drains are idempotent; newly queued identities must not be acknowledged here.
        // 并发提交具有幂等性；本次不能确认新入队的其他身份。
        self.read_releases
            .lock()
            .map_err(|_| String::from("VCE read release queue is poisoned"))?
            .retain(|lease| !releases.contains(lease));
        Ok(())
    }
}

impl VceProjectIndex {
    /// Clone a management read excluding only staging owned by the host's currently observed refresh.
    /// 克隆管理读取，仅排除宿主当前已观察刷新所持有的暂存。
    /// `refresh_id` is an exact live host identity, not inferred from persisted phase or age.
    /// `refresh_id` 为精确存活宿主身份，不从持久阶段或时间推断。
    /// Returns a read-local projection; the caller must reject it if the active identity changes during I/O.
    /// 返回读取本地投影；若输入输出期间活动身份变化，调用方必须拒绝本次结果。
    pub fn management_read_for_refresh(&self, refresh_id: Option<&str>) -> Self {
        // The original writer and publication lease keep their diagnostics and authority unchanged.
        // 原写入句柄及发布租约的诊断与权限保持不变。
        let mut read = self.clone();
        read.management_active_refresh = refresh_id.map(str::to_owned);
        read
    }

    /// Return the exact namespace/checkpoint identity held by this immutable read lease.
    /// 返回此不可变读取租约持有的精确命名空间及检查点身份。
    /// No parameters or I/O; unpinned writer handles return absence instead of a guessed version.
    /// 无参数和输入输出；未固定写入句柄返回空值，不猜测版本。
    pub fn management_read_identity(&self) -> Option<String> {
        // The physical namespace distinguishes replacements whose numeric checkpoint versions coincide.
        // 物理命名空间区分数字检查点版本相同的替换空间。
        let view = self.read_view.as_ref()?;
        Some(format!(
            "{}:{}",
            self.binding.namespace_id, view.checkpoint.version
        ))
    }

    /// Read management rows from the handle's immutable checkpoint or the writer's active views.
    /// 从句柄的不可变检查点或写入句柄的活动视图读取管理记录。
    /// `sql` and `params` define an internal read; returns decoded rows or the storage error.
    /// `sql` 与 `params` 定义内部读取；返回解码记录或存储错误。
    pub(super) async fn query_management_rows<T: for<'de> Deserialize<'de>>(
        &self,
        sql: &str,
        params: Vec<SqliteValue>,
    ) -> Result<Vec<T>, String> {
        // A lease retains all referenced versions; shadow only content views, never live job tables.
        // 租约保留全部引用版本；仅覆盖内容视图，不覆盖实时作业表。
        let scoped_sql = match &self.read_view {
            Some(view) => format!(
                "WITH vce_checkpoints AS (SELECT * FROM vce_read_checkpoints WHERE checkpoint_version = {}),
                 vce_blobs AS (
                   SELECT b.* FROM vce_blob_versions b JOIN vce_read_members m
                   ON m.namespace_id = b.namespace_id AND m.refresh_id = b.refresh_id
                   AND m.generation = b.generation AND m.blob_name = b.blob_name
                   WHERE m.checkpoint_version = {}),
                 vce_chunks AS (
                   SELECT c.* FROM vce_chunk_versions c JOIN vce_blobs b
                   ON b.namespace_id = c.namespace_id AND b.refresh_id = c.refresh_id
                   AND b.generation = c.generation AND b.blob_name = c.blob_name)
                 {sql}",
                view.checkpoint.version, view.checkpoint.version,
            ),
            None => sql.to_owned(),
        };
        self.data_plane.query_rows(&scoped_sql, params).await
    }

    /// Prepare exact membership before committing a new checkpoint and its publication lease together.
    /// 在一同提交新检查点及其发布租约前准备精确成员映射。
    /// `request` is validated by commit; `owner_run_id` identifies the host. Returns an unpersisted guard.
    /// `request` 已通过提交校验；`owner_run_id` 标识宿主。返回尚未持久化的守卫。
    pub(super) async fn prepare_publication_lease(
        &self,
        request: &super::IndexCommit,
        owner_run_id: &str,
    ) -> Result<VceReadLease, String> {
        // provenance supplies immutable binding facts; its current source set is replaced below.
        // provenance 提供不可变绑定事实；其当前来源集合在下方重新计算。
        let mut provenance = self.embedding_provenance().await?;
        if provenance.checkpoint_version != request.expected_checkpoint_version {
            return Err(String::from(
                "VCE predecessor changed before publication preparation",
            ));
        }
        provenance.checkpoint_version = Some(request.checkpoint.version);
        provenance.source_fingerprints =
            BTreeSet::from([provenance.original.physical_fingerprint.clone()]);
        // members starts with immutable predecessor producers, never a generation-wide guess.
        // members 从不可变前驱生产来源开始，绝不按整个代次猜测。
        let mut members = BTreeMap::new();
        if let Some(version) = request.expected_checkpoint_version {
            // rows bind unchanged members to the predecessor checked by the same commit CAS.
            // rows 将未变化成员绑定到同次提交比较交换所校验的前驱。
            let rows: Vec<ReadMemberProvenanceRow> = self.data_plane.query_rows(
                "SELECT m.blob_name, m.refresh_id, m.generation, CASE WHEN b.vector_count > 0 OR b.path_status = 'ready' THEN b.embedding_producer_fingerprint ELSE NULL END AS physical_fingerprint, CASE WHEN (b.vector_count > 0 OR b.path_status = 'ready') AND (b.embedding_producer_fingerprint IS NULL OR NOT EXISTS(SELECT 1 FROM vce_embedding_generation_origins o WHERE o.namespace_id = b.namespace_id AND o.refresh_id = b.refresh_id AND o.generation = b.generation AND o.physical_fingerprint = b.embedding_producer_fingerprint)) THEN 1 ELSE 0 END AS invalid_origin FROM vce_read_members m JOIN vce_blob_versions b ON b.namespace_id = m.namespace_id AND b.refresh_id = m.refresh_id AND b.generation = m.generation AND b.blob_name = m.blob_name WHERE m.namespace_id = ? AND m.checkpoint_version = ?;",
                vec![sqlite_text(&self.binding.namespace_id), sqlite_u64_raw(version, "VCE predecessor checkpoint")?],
            ).await?;
            for row in rows {
                if !request.checkpoint.members.contains(&row.member.blob_name)
                    || request.staged_blob_names.contains(&row.member.blob_name)
                {
                    continue;
                }
                if row.invalid_origin != 0
                    || row.member.generation == 0
                    || row.member.refresh_id.trim().is_empty()
                    || members
                        .insert(row.member.blob_name.clone(), row.member)
                        .is_some()
                {
                    return Err(String::from(
                        "VCE predecessor has invalid producer membership",
                    ));
                }
                if let Some(fingerprint) = row.physical_fingerprint {
                    if fingerprint.trim().is_empty() || fingerprint.len() > 4096 {
                        return Err(String::from(
                            "VCE predecessor has invalid embedding provenance",
                        ));
                    }
                    provenance.source_fingerprints.insert(fingerprint);
                }
            }
        }
        for blob_name in &request.staged_blob_names {
            members.insert(
                blob_name.clone(),
                ReadMemberRow {
                    blob_name: blob_name.clone(),
                    refresh_id: request.refresh_id.clone(),
                    generation: request.generation,
                },
            );
        }
        if members.keys().cloned().collect::<BTreeSet<_>>() != request.checkpoint.members {
            return Err(String::from(
                "VCE publication producer membership is incomplete",
            ));
        }
        // staged_vectors determines whether this exact attempt actually contributes a new source.
        // staged_vectors 判断本次精确尝试是否实际贡献新的来源。
        let staged_vectors: Vec<CountRow> = self.data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM vce_staged_blobs WHERE namespace_id = ? AND refresh_id = ? AND generation = ? AND (vector_count > 0 OR path_status = 'ready');",
            vec![sqlite_text(&self.binding.namespace_id), sqlite_text(&request.refresh_id), sqlite_u64_raw(request.generation, "VCE publication generation")?],
        ).await?;
        let [staged_vectors] = staged_vectors.as_slice() else {
            return Err(String::from(
                "VCE publication staging provenance is missing",
            ));
        };
        if staged_vectors.count > 0 {
            provenance
                .source_fingerprints
                .insert(self.embedding_producer_fingerprint().to_owned());
        }
        Ok(VceReadLease {
            index: self.clone(),
            lease_id: uuid::Uuid::new_v4().to_string(),
            owner_run_id: owner_run_id.to_owned(),
            checkpoint: request.checkpoint.clone(),
            members,
            provenance,
        })
    }

    /// Pin the exact currently published checkpoint before admitting a shared query view.
    /// 在准入共享查询视图之前固定精确的当前已发布检查点。
    ///
    /// `checkpoint` is the matching engine snapshot; `owner_run_id` is the host's admitted run.
    /// Returns a read-only index clone holding durable and physical leases, or a race/error.
    /// `checkpoint` 为匹配的引擎快照；`owner_run_id` 为宿主已准入运行身份。
    /// 返回持有持久和物理租约的只读索引克隆，或竞态／错误。
    pub async fn pin_checkpoint(
        &self,
        checkpoint: Checkpoint,
        owner_run_id: &str,
    ) -> Result<Self, String> {
        if self.read_view.is_some()
            || owner_run_id.trim().is_empty()
            || checkpoint.chain_id != self.binding.checkpoint_chain_id
        {
            return Err(String::from(
                "VCE checkpoint lease requires an unpinned matching index and exact host run",
            ));
        }
        self.data_plane.flush_released_read_leases().await?;
        // lease_id is collision-resistant across concurrent hosts and publication lifetimes.
        // lease_id 在并发宿主和发布生命周期之间抗碰撞。
        let lease_id = uuid::Uuid::new_v4().to_string();
        // version is checked against SQLite's actual integer domain before any write.
        // version 在任何写入之前按 SQLite 实际整数范围检查。
        let version = sqlite_u64_raw(checkpoint.version, "VCE read checkpoint")?;
        // snapshot equality binds the member set as well as its version and chain identity.
        // snapshot 相等性同时绑定成员集合、版本及链身份。
        let snapshot = serde_json::to_string(&checkpoint).map_err(|error| error.to_string())?;
        // Capture provenance before insertion; the subsequent exact checkpoint predicate rejects drift.
        // 在插入前捕获来源；随后的精确检查点谓词拒绝漂移。
        let provenance = self.embedding_provenance().await?;
        if provenance.checkpoint_version != Some(checkpoint.version) {
            return Err(String::from(
                "VCE checkpoint changed before provenance admission",
            ));
        }
        // Install the guard before I/O so an ambiguous response still records the exact release.
        // 在输入输出之前安装守卫，确保响应不明确时仍登记精确释放身份。
        let mut lease = VceReadLease {
            index: self.clone(),
            lease_id,
            owner_run_id: owner_run_id.to_owned(),
            checkpoint,
            members: BTreeMap::new(),
            provenance,
        };
        self.data_plane.execute(
            "INSERT INTO vce_read_leases(namespace_id, lease_id, owner_run_id, checkpoint_version, created_at_unix_ms) SELECT namespace_id, ?, ?, checkpoint_version, ? FROM vce_checkpoints WHERE namespace_id = ? AND checkpoint_version = ? AND checkpoint_json = ?;",
            vec![sqlite_text(&lease.lease_id), sqlite_text(owner_run_id), SqliteValue::Integer(current_unix_epoch_millis_i64()?),
                sqlite_text(&self.binding.namespace_id), version, sqlite_text(snapshot)],
            "固定 VCE 已发布检查点失败",
        ).await?;
        // admitted distinguishes a successful SQL statement from an insert that matched no checkpoint.
        // admitted 区分 SQL 语句执行成功与未匹配检查点而未插入的情况。
        let admitted: Vec<CountRow> = self.data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM vce_read_leases WHERE namespace_id = ? AND lease_id = ? AND owner_run_id = ?;",
            vec![sqlite_text(&self.binding.namespace_id), sqlite_text(&lease.lease_id), sqlite_text(owner_run_id)],
        ).await?;
        if admitted.first().map(|row| row.count) != Some(1) {
            return Err(String::from("VCE checkpoint changed before read admission"));
        }
        // rows come from immutable checkpoint membership, not mutable current management views.
        // rows 来自不可变检查点成员关系，而不是可变的当前管理视图。
        let rows: Vec<ReadMemberRow> = self.data_plane.query_rows(
            "SELECT blob_name, refresh_id, generation FROM vce_read_members WHERE namespace_id = ? AND checkpoint_version = ?;",
            vec![sqlite_text(&self.binding.namespace_id), sqlite_u64_raw(lease.checkpoint.version, "VCE read checkpoint")?],
        ).await?;
        for row in rows {
            if row.generation == 0
                || row.refresh_id.trim().is_empty()
                || !lease.checkpoint.members.contains(&row.blob_name)
                || lease.members.insert(row.blob_name.clone(), row).is_some()
            {
                return Err(String::from(
                    "VCE pinned checkpoint has invalid or duplicate producer membership",
                ));
            }
        }
        if lease.members.keys().cloned().collect::<BTreeSet<_>>() != lease.checkpoint.members {
            return Err(String::from(
                "VCE pinned checkpoint producer membership is incomplete",
            ));
        }
        Ok(lease.into_read_index())
    }

    /// Require an admitted immutable view for a retrieval operation.
    /// 为检索操作要求已准入不可变视图。
    ///
    /// `operation` attributes a rejected unpinned call. Returns the exact held checkpoint lease.
    /// `operation` 为拒绝的未固定调用标记归属。返回精确持有的检查点租约。
    pub(super) fn require_read_view(
        &self,
        operation: VcePortOperation,
    ) -> Result<&VceReadLease, VcePortError> {
        self.read_view.as_deref().ok_or_else(|| {
            port_rejected(
                operation,
                "VCE retrieval requires a pinned checkpoint read view",
            )
        })
    }

    /// Retire only checkpoints that are neither current nor retained by any durable read lease.
    /// 仅退役既非当前、又未被任何持久读取租约保留的检查点。
    ///
    /// No parameters. Returns an explicit storage error; member cascades expose safely reclaimable versions.
    /// 无参数。返回明确存储错误；成员级联释放使可安全回收的版本暴露出来。
    pub(super) async fn retire_unleased_checkpoints(&self) -> Result<(), String> {
        self.data_plane.execute(
            "DELETE FROM vce_read_checkpoints WHERE namespace_id = ? AND NOT EXISTS(SELECT 1 FROM vce_active_checkpoints a WHERE a.namespace_id = vce_read_checkpoints.namespace_id AND a.checkpoint_version = vce_read_checkpoints.checkpoint_version) AND NOT EXISTS(SELECT 1 FROM vce_read_leases l WHERE l.namespace_id = vce_read_checkpoints.namespace_id AND l.checkpoint_version = vce_read_checkpoints.checkpoint_version);",
            vec![sqlite_text(&self.binding.namespace_id)], "退役无读取引用的 VCE 检查点失败",
        ).await
    }

    /// Capture exact old producer identities replaced or deleted by this validated commit.
    /// 捕获被本次已验证提交替换或删除的精确旧生产身份。
    ///
    /// `request` supplies the validated membership transition. Returns only currently committed rows.
    /// `request` 提供已验证的成员转换。仅返回当前已提交记录。
    pub(super) async fn retiring_members(
        &self,
        request: &super::IndexCommit,
    ) -> Result<Vec<ReadMemberRow>, VcePortError> {
        // replaced is a set of content identities, not a path wildcard or generation cutoff.
        // replaced 是内容身份集合，不是路径通配符或代次截止值。
        let replaced = request
            .deleted_blobs
            .union(&request.staged_blob_names)
            .cloned()
            .collect::<BTreeSet<_>>();
        if replaced.is_empty() {
            return Ok(Vec::new());
        }
        // sql reads exact producer rows under the caller's checked checkpoint/CAS boundary.
        // sql 在调用方已检查的检查点／比较交换边界内读取精确生产记录。
        let sql = format!(
            "SELECT blob_name, refresh_id, generation FROM vce_blobs WHERE namespace_id = ? AND blob_name IN ({});",
            super::placeholders(replaced.len())
        );
        // parameters bind member identities without interpolating untrusted text.
        // parameters 绑定成员身份，不插值不可信文本。
        let mut parameters = vec![sqlite_text(&self.binding.namespace_id)];
        parameters.extend(replaced.iter().map(sqlite_text));
        self.data_plane
            .query_rows(&sql, parameters)
            .await
            .map_err(|message| super::port_failed(VcePortOperation::Commit, message))
    }
}

/// Build the sole supported immutable read layout and current-state management views.
/// 构建唯一支持的不可变读取布局及当前状态管理视图。
///
/// No parameters. Returns DDL for a newly authorized VCE project; this is not a historical migration.
/// 无参数。返回新授权 VCE 项目的结构定义；这不是历史格式迁移。
pub(super) fn read_generation_schema_script() -> &'static str {
    r#"
CREATE TABLE IF NOT EXISTS vce_read_checkpoints (
    namespace_id TEXT NOT NULL,
    checkpoint_version INTEGER NOT NULL CHECK(checkpoint_version > 0),
    generation INTEGER NOT NULL CHECK(generation > 0),
    refresh_id TEXT NOT NULL CHECK(length(trim(refresh_id)) > 0),
    checkpoint_json TEXT NOT NULL,
    source_revision TEXT,
    committed_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, checkpoint_version),
    FOREIGN KEY(namespace_id) REFERENCES vce_project_bindings(namespace_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS vce_active_checkpoints (
    namespace_id TEXT PRIMARY KEY,
    checkpoint_version INTEGER NOT NULL,
    FOREIGN KEY(namespace_id, checkpoint_version) REFERENCES vce_read_checkpoints(namespace_id, checkpoint_version)
);
CREATE VIEW IF NOT EXISTS vce_checkpoints AS
SELECT r.* FROM vce_read_checkpoints r JOIN vce_active_checkpoints a
ON a.namespace_id = r.namespace_id AND a.checkpoint_version = r.checkpoint_version;
CREATE TABLE IF NOT EXISTS vce_blob_versions (
    namespace_id TEXT NOT NULL, refresh_id TEXT NOT NULL, generation INTEGER NOT NULL,
    blob_name TEXT NOT NULL, path TEXT NOT NULL,
    structure_analyzer_id TEXT, structure_mode TEXT, structure_fallback_reason TEXT,
    path_document TEXT, path_status TEXT NOT NULL, path_error TEXT,
    symbol_status TEXT NOT NULL, symbol_error TEXT, status TEXT NOT NULL, status_reason TEXT,
    chunk_count INTEGER NOT NULL, vector_count INTEGER NOT NULL, symbol_count INTEGER NOT NULL,
    embedding_producer_fingerprint TEXT,
    PRIMARY KEY(namespace_id, refresh_id, generation, blob_name),
    UNIQUE(namespace_id, refresh_id, generation, path),
    FOREIGN KEY(namespace_id, refresh_id, generation) REFERENCES vce_generations(namespace_id, refresh_id, generation)
);
CREATE TABLE IF NOT EXISTS vce_chunk_versions (
    namespace_id TEXT NOT NULL, refresh_id TEXT NOT NULL, generation INTEGER NOT NULL,
    blob_name TEXT NOT NULL, chunk_id TEXT NOT NULL, content_hash TEXT NOT NULL,
    path TEXT NOT NULL, content TEXT NOT NULL, start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL, start_char_offset INTEGER NOT NULL, kind TEXT NOT NULL,
    vector_ready INTEGER NOT NULL CHECK(vector_ready IN (0, 1)),
    PRIMARY KEY(namespace_id, refresh_id, generation, chunk_id),
    FOREIGN KEY(namespace_id, refresh_id, generation, blob_name)
        REFERENCES vce_blob_versions(namespace_id, refresh_id, generation, blob_name) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS vce_symbol_versions (
    namespace_id TEXT NOT NULL, refresh_id TEXT NOT NULL, generation INTEGER NOT NULL,
    blob_name TEXT NOT NULL, occurrence_id TEXT NOT NULL, identifier TEXT NOT NULL,
    path TEXT NOT NULL, start_line INTEGER NOT NULL, end_line INTEGER NOT NULL,
    start_char_offset INTEGER NOT NULL, chunk_id TEXT NOT NULL, content TEXT NOT NULL, kind TEXT NOT NULL,
    PRIMARY KEY(namespace_id, refresh_id, generation, occurrence_id),
    FOREIGN KEY(namespace_id, refresh_id, generation, blob_name)
        REFERENCES vce_blob_versions(namespace_id, refresh_id, generation, blob_name) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_vce_version_symbol_lookup
ON vce_symbol_versions(namespace_id, identifier, refresh_id, generation, blob_name);
CREATE TABLE IF NOT EXISTS vce_read_members (
    namespace_id TEXT NOT NULL, checkpoint_version INTEGER NOT NULL,
    blob_name TEXT NOT NULL, refresh_id TEXT NOT NULL, generation INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, checkpoint_version, blob_name),
    FOREIGN KEY(namespace_id, checkpoint_version) REFERENCES vce_read_checkpoints(namespace_id, checkpoint_version) ON DELETE CASCADE,
    FOREIGN KEY(namespace_id, refresh_id, generation, blob_name) REFERENCES vce_blob_versions(namespace_id, refresh_id, generation, blob_name)
);
CREATE INDEX IF NOT EXISTS idx_vce_read_member_version
ON vce_read_members(namespace_id, refresh_id, generation, blob_name);
CREATE TABLE IF NOT EXISTS vce_read_leases (
    namespace_id TEXT NOT NULL, lease_id TEXT NOT NULL, owner_run_id TEXT NOT NULL,
    checkpoint_version INTEGER NOT NULL, created_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(namespace_id, lease_id),
    FOREIGN KEY(namespace_id, checkpoint_version) REFERENCES vce_read_checkpoints(namespace_id, checkpoint_version)
);
CREATE INDEX IF NOT EXISTS idx_vce_read_lease_checkpoint
ON vce_read_leases(namespace_id, checkpoint_version);
CREATE VIEW IF NOT EXISTS vce_blobs AS
SELECT b.* FROM vce_blob_versions b JOIN vce_read_members m
ON m.namespace_id = b.namespace_id AND m.refresh_id = b.refresh_id AND m.generation = b.generation AND m.blob_name = b.blob_name
JOIN vce_active_checkpoints a ON a.namespace_id = m.namespace_id AND a.checkpoint_version = m.checkpoint_version;
CREATE VIEW IF NOT EXISTS vce_chunks AS
SELECT c.* FROM vce_chunk_versions c JOIN vce_blobs b
ON b.namespace_id = c.namespace_id AND b.refresh_id = c.refresh_id AND b.generation = c.generation AND b.blob_name = c.blob_name;
CREATE VIEW IF NOT EXISTS vce_symbols AS
SELECT s.* FROM vce_symbol_versions s JOIN vce_blobs b
ON b.namespace_id = s.namespace_id AND b.refresh_id = s.refresh_id AND b.generation = s.generation AND b.blob_name = s.blob_name;
CREATE VIEW IF NOT EXISTS vce_blob_embedding_origins AS
SELECT namespace_id, blob_name, generation, embedding_producer_fingerprint AS physical_fingerprint
FROM vce_blobs WHERE embedding_producer_fingerprint IS NOT NULL;
"#
}
