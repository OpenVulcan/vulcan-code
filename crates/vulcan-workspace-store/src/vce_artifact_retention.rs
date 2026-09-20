//! Bounded artifact retention without touching published index generations or read leases.
//! 有界产物保留，不触碰已发布索引代次和读取租约。

use rusqlite::{Connection, OptionalExtension, Transaction};

/// Retain at most 512 MiB of unpinned reusable materials per physical project database.
/// 每个物理项目数据库最多保留 512 MiB 未固定的可复用材料。
/// Active or unreconciled runs remain explicit roots and may exceed this cache budget.
/// 活动或未对账运行仍为显式根引用，可以超过此缓存预算。
const CACHE_BYTES: i64 = 512 * 1024 * 1024;

/// Collect obsolete terminal owners, then evict reusable cache entries until unpinned bytes are bounded.
/// 回收过期终态归属，再淘汰可复用缓存条目，直至未固定字节有界。
/// `keep_run` protects the caller's current operation; returns only after transactional reclamation.
/// `keep_run` 保护调用者当前操作；事务回收完成后返回，失败时返回明确错误。
pub(super) fn collect(connection: &mut Connection, keep_run: &str) -> Result<(), String> {
    collect_with_budget(connection, keep_run, CACHE_BYTES)
}

/// Apply the production retention algorithm with an explicit unpinned cache budget.
/// 使用显式未固定缓存预算执行生产保留算法。
/// The connection and retained run define ownership; the byte budget allows small isolated tests.
/// 连接和保留运行定义归属；字节预算支持小型隔离测试。返回回收结果或错误。
pub(super) fn collect_with_budget(
    connection: &mut Connection,
    keep_run: &str,
    cache_bytes: i64,
) -> Result<(), String> {
    // Host receipts distinguish abandoned plans from jobs still performing cleanup.
    // 宿主回执区分已遗弃计划和仍在清理的作业。
    let jobs_exist = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='vce_host_jobs'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    // One transaction prevents a cache or owner from disappearing between mark and sweep.
    // 单个事务防止缓存或归属在标记和清扫之间消失。
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction.execute_batch("CREATE TEMP TABLE vce_ended_jobs(job_id TEXT PRIMARY KEY); CREATE TEMP TABLE vce_active_jobs(job_id TEXT PRIMARY KEY);").map_err(|error|error.to_string())?;
    if jobs_exist {
        transaction.execute_batch("INSERT INTO vce_ended_jobs SELECT job_id FROM vce_host_jobs WHERE json_extract(snapshot_json,'$.state') IN ('succeeded','failed','cancelled','interrupted'); INSERT INTO vce_active_jobs SELECT job_id FROM vce_host_jobs WHERE json_extract(snapshot_json,'$.state') IN ('queued','running','cancelling');").map_err(|error|error.to_string())?;
    }
    // Retain the two newest settled snapshots for diagnostics. Unknown uploads/submissions and publication
    // intents remain roots; deleting a manifest never authorizes deleting a published checkpoint.
    // 保留最近两份已结束快照用于诊断。未知上传、提交和发布意图仍是根引用；
    // 删除清单绝不授权删除已发布检查点。
    transaction.execute(r#"DELETE FROM vce_embedding_runs WHERE (namespace_id,run_id) IN (
      SELECT r.namespace_id,r.run_id FROM vce_embedding_runs r
      WHERE r.run_id != ?
      AND (r.state IN ('succeeded','failed','cancelled') OR
           (r.state IN ('planned','running','candidate_ready') AND EXISTS(SELECT 1 FROM vce_ended_jobs j WHERE j.job_id=r.run_id)))
      AND NOT EXISTS(SELECT 1 FROM vce_active_jobs j WHERE j.job_id=r.run_id)
      AND NOT EXISTS(SELECT 1 FROM vce_publication_intents p WHERE p.namespace_id=r.namespace_id AND p.refresh_id=r.run_id AND p.stage IN ('preparing','committed'))
      AND NOT EXISTS(SELECT 1 FROM vce_embedding_remote_batches b WHERE b.namespace_id=r.namespace_id AND b.run_id=r.run_id
          AND (json_extract(b.record_json,'$.state') NOT IN ('planned','upload_failed','submission_failed','succeeded','failed','cancelled')
               OR json_extract(b.record_json,'$.cleanup_warning') IS NOT NULL
               OR (json_extract(b.record_json,'$.state') IN ('upload_failed','submission_failed') AND json_extract(b.record_json,'$.uploaded_input') IS NOT NULL)))
      ORDER BY r.updated_at_unix_ms DESC,r.namespace_id,r.run_id LIMIT -1 OFFSET 2
    )"#, [keep_run]).map_err(|error|error.to_string())?;
    sweep(&transaction)?;
    loop {
        // Count distinct physical bytes which no surviving run pins, including descriptor bodies.
        // 统计未被存活运行固定的去重物理字节，包含描述正文。
        let unpinned: i64 = transaction.query_row(r#"WITH pinned_artifacts AS (
          SELECT artifact_hash FROM vce_embedding_run_nodes UNION SELECT artifact_hash FROM vce_embedding_result_refs
        ), pinned_payloads AS (
          SELECT artifact_hash AS payload_hash FROM pinned_artifacts UNION
          SELECT p.payload_hash FROM vce_shared_artifact_payloads p JOIN pinned_artifacts a USING(artifact_hash)
        ) SELECT coalesce(sum(byte_size),0) FROM vce_shared_payloads p WHERE NOT EXISTS(SELECT 1 FROM pinned_payloads a WHERE a.payload_hash=p.payload_hash)"#,[],|row|row.get(0)).map_err(|error|error.to_string())?;
        if unpinned <= cache_bytes {
            break;
        }
        // Evict a bounded LRU page instead of rescanning the payload graph for every cache entry.
        // 淘汰有界 LRU 页，避免每淘汰一项都重新扫描负载图。
        let removed = transaction.execute(r#"DELETE FROM vce_prepared_artifact_cache WHERE rowid IN (
          SELECT c.rowid FROM vce_prepared_artifact_cache c
          WHERE NOT EXISTS(SELECT 1 FROM vce_embedding_run_nodes n WHERE n.artifact_hash=c.artifact_hash)
          ORDER BY last_used_unix_ms,contract_hash,blob_name LIMIT 64
        )"#,[]).map_err(|error|error.to_string())?
        + transaction.execute(r#"DELETE FROM vce_embedding_result_cache WHERE rowid IN (
          SELECT c.rowid FROM vce_embedding_result_cache c
          WHERE NOT EXISTS(SELECT 1 FROM vce_embedding_result_refs r WHERE r.artifact_hash=c.artifact_hash)
          ORDER BY last_used_unix_ms,request_hash LIMIT 64
        )"#,[]).map_err(|error|error.to_string())?;
        if removed == 0 {
            return Err(String::from(
                "VCE unpinned artifacts have no reclaimable owners",
            ));
        }
        sweep(&transaction)?;
    }
    transaction
        .execute_batch("DROP TABLE vce_ended_jobs; DROP TABLE vce_active_jobs;")
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())
}

/// Remove only objects unreachable from run owners and the bounded reuse caches.
/// 仅移除从运行归属和有界复用缓存不可达的对象。
/// The caller owns the transaction; indexed reverse references keep deletion local.
/// 调用者持有事务；带索引的反向引用保持删除局部化。返回存储结果。
fn sweep(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction.execute_batch(r#"
      DELETE FROM vce_shared_artifacts AS a
      WHERE NOT EXISTS(SELECT 1 FROM vce_embedding_run_nodes n WHERE n.artifact_hash=a.artifact_hash)
        AND NOT EXISTS(SELECT 1 FROM vce_embedding_result_refs r WHERE r.artifact_hash=a.artifact_hash)
        AND NOT EXISTS(SELECT 1 FROM vce_prepared_artifact_cache c WHERE c.artifact_hash=a.artifact_hash)
        AND NOT EXISTS(SELECT 1 FROM vce_embedding_result_cache c WHERE c.artifact_hash=a.artifact_hash);
      DELETE FROM vce_shared_payloads AS p
      WHERE NOT EXISTS(SELECT 1 FROM vce_shared_artifacts a WHERE a.artifact_hash=p.payload_hash)
        AND NOT EXISTS(SELECT 1 FROM vce_shared_artifact_payloads a WHERE a.payload_hash=p.payload_hash);
    "#).map_err(|error|error.to_string())
}
