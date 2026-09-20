//! Immutable embedding results shared by exact frozen request contracts.
//! 按精确冻结请求契约共享的不可变向量结果。

use super::{shared_artifacts, shared_manifest_store};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use vulcan_context_engine::{VceEmbeddingBatchRecord, VceEmbeddingBatchResult};

/// Refuse a new overlapping run while an earlier provider task still requires reconciliation.
/// 较早供应商任务仍需对账时拒绝新的重叠运行。
/// The candidate manifest supplies exact input IDs; errors retain the original run and remote task IDs.
/// 候选清单提供精确输入 ID；错误保留原运行和远端任务 ID。
pub(super) fn require_resolved_predecessors(
    connection: &Connection,
    namespace_id: &str,
    run_id: &str,
    candidate_batches: &[VceEmbeddingBatchRecord],
) -> Result<(), String> {
    // An existing admitted run must remain recoverable under its original task identities.
    // 已准入运行必须能沿原任务身份继续恢复。
    let admitted: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM vce_embedding_runs WHERE namespace_id=? AND run_id=? AND manifest_complete=1)",params![namespace_id,run_id],|row|row.get(0)).map_err(|error|error.to_string())?;
    if admitted {
        return Ok(());
    }
    // Query only unresolved work, not settled historical diagnostics.
    // 仅查询未决工作，不查询已结束的历史诊断。
    let mut statement = connection.prepare("SELECT namespace_id,run_id,record_json FROM vce_embedding_remote_batches WHERE NOT(namespace_id=? AND run_id=?) AND json_extract(record_json,'$.state') IN ('uploading','upload_unknown','uploaded','submitting','submission_unknown','submitted','ready')").map_err(|error|error.to_string())?;
    let rows = statement
        .query_map(params![namespace_id, run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (namespace, run, json) = row.map_err(|error| error.to_string())?;
        let record: vulcan_context_engine::VceEmbeddingRemoteBatchRecord =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        record.validate()?;
        // Reconcile overlapping unknown work even after changing providers: older envelopes may
        // record the original binding rather than the actual approved producer.
        // 即使切换供应商也要先对账重叠的未知工作：旧概要可能记录原始绑定，
        // 而非实际获批准的生产方。
        // Purpose-local ranges are frozen when each provider task is admitted.
        // 每个供应商任务准入时已冻结用途内范围。
        let mut batches = connection.prepare("SELECT batch_json FROM vce_embedding_run_batches WHERE namespace_id=? AND run_id=? AND json_extract(batch_json,'$.purpose')=? ORDER BY batch_index LIMIT ? OFFSET ?").map_err(|error|error.to_string())?;
        let old_batches = batches
            .query_map(
                params![
                    namespace,
                    run,
                    record.purpose,
                    record.logical_batch_count,
                    record.first_logical_batch_index
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        let requested = candidate_batches
            .iter()
            .filter(|batch| batch.purpose == record.purpose)
            .flat_map(|batch| batch.input_ids.iter().map(String::as_str))
            .collect::<std::collections::BTreeSet<_>>();
        for batch in old_batches {
            let batch: VceEmbeddingBatchRecord =
                serde_json::from_str(&batch.map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            if batch
                .input_ids
                .iter()
                .any(|id| requested.contains(id.as_str()))
            {
                return Err(format!(
                    "VCE provider work requires reconciliation before retry: run={run}, task={}, state={:?}. Resume or reconcile that task before submitting overlapping inputs.",
                    record.remote_task_id, record.state
                ));
            }
        }
    }
    Ok(())
}

/// Result owners and reusable request identities, independent of execution names.
/// 结果归属与可复用请求身份，独立于执行名称。
pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS vce_embedding_result_refs (
 namespace_id TEXT NOT NULL, run_id TEXT NOT NULL, batch_id TEXT NOT NULL,
 artifact_hash TEXT NOT NULL, updated_at_unix_ms INTEGER NOT NULL,
 PRIMARY KEY(namespace_id,run_id,batch_id),
 FOREIGN KEY(namespace_id,run_id) REFERENCES vce_embedding_runs(namespace_id,run_id) ON DELETE CASCADE,
 FOREIGN KEY(artifact_hash) REFERENCES vce_shared_artifacts(artifact_hash)
);
CREATE INDEX IF NOT EXISTS idx_vce_result_artifact_owner ON vce_embedding_result_refs(artifact_hash);
CREATE TABLE IF NOT EXISTS vce_embedding_result_cache (
 request_hash TEXT PRIMARY KEY, artifact_hash TEXT NOT NULL, last_used_unix_ms INTEGER NOT NULL,
 FOREIGN KEY(artifact_hash) REFERENCES vce_shared_artifacts(artifact_hash)
);
CREATE INDEX IF NOT EXISTS idx_vce_result_cache_owner ON vce_embedding_result_cache(artifact_hash);
"#;

/// Derive a cache key from the frozen producer and ordered input identities.
/// 根据冻结的生产方与有序输入身份派生缓存键。
/// Missing physical producer identity disables cross-run reuse; returns the optional digest.
/// 缺少物理生产方身份时禁用跨运行复用；返回可选摘要或契约错误。
fn request_identity(
    connection: &Connection,
    namespace: &str,
    run: &str,
    batch: &VceEmbeddingBatchRecord,
) -> Result<Option<String>, String> {
    // Logical namespace names do not identify the physical embedding space.
    // 逻辑命名空间名称不代表物理向量空间。
    let contract = shared_manifest_store::run_contract(connection, namespace, run)?;
    if contract.embedding_space.physical_fingerprint.is_none() {
        return Ok(None);
    }
    // Batch IDs and attempt counts are intentionally excluded; input order and budgets remain exact.
    // 明确排除批次 ID 和尝试次数；输入顺序与预算保持精确。
    let key = serde_json::to_string(&(
        shared_manifest_store::contract_identity(&contract)?,
        &batch.purpose,
        &batch.input_ids,
        batch.total_chars,
    ))
    .map_err(|error| error.to_string())?;
    Ok(Some(shared_artifacts::content_identity(&key)))
}

/// Read one exact owner's result and restore its public batch identity.
/// 读取精确归属的一份结果并恢复其公共批次身份。
/// Returns absence or a validated result; never consults a cache during lifecycle validation.
/// 返回空值或已校验结果；生命周期校验时绝不查询缓存。
pub(super) fn load(
    connection: &Connection,
    namespace: &str,
    run: &str,
    batch: &str,
) -> Result<Option<VceEmbeddingBatchResult>, String> {
    // The owner reference proves that this run durably adopted the immutable result.
    // 归属引用证明本次运行已持久接纳不可变结果。
    let artifact: Option<String> = connection.query_row("SELECT artifact_hash FROM vce_embedding_result_refs WHERE namespace_id=? AND run_id=? AND batch_id=?", params![namespace,run,batch], |row| row.get(0)).optional().map_err(|error| error.to_string())?;
    artifact
        .map(|artifact| decode(connection, &artifact, batch))
        .transpose()
}

/// Decode a shared result and verify its normalized identity before restoring ownership.
/// 解码共享结果，恢复归属前校验规范化身份。
/// The artifact hash selects exact bytes; batch supplies only the new execution identity.
/// 产物哈希选择精确字节；批次仅提供新的执行身份。返回结果或完整性错误。
fn decode(
    connection: &Connection,
    artifact: &str,
    batch: &str,
) -> Result<VceEmbeddingBatchResult, String> {
    // Only execution identity is normalized; vector values and input IDs stay lossless.
    // 仅规范化执行身份；向量值与输入 ID 保持无损。
    let mut result: VceEmbeddingBatchResult =
        serde_json::from_value(shared_artifacts::load_artifact(connection, artifact)?)
            .map_err(|error| error.to_string())?;
    if result.batch_id != "shared" {
        return Err(String::from("Invalid shared embedding result identity"));
    }
    result.batch_id = batch.to_owned();
    result.validate()?;
    Ok(result)
}

/// Persist a validated result and immutable owner in the caller's transaction.
/// 在调用者事务中持久保存已校验结果与不可变归属。
/// Caller verifies request coverage and active-run admission; returns a conflict explicitly.
/// 调用者校验请求覆盖和活动运行准入；冲突时显式返回错误。
pub(super) fn store(
    transaction: &Transaction<'_>,
    namespace: &str,
    run: &str,
    batch: &VceEmbeddingBatchRecord,
    result: &VceEmbeddingBatchResult,
    now: i64,
) -> Result<(), String> {
    store_owned_result(transaction, namespace, run, batch, result, now, true)
}

/// Store exact ownership and optionally publish a cache key proven by the current producer contract.
/// 保存精确归属，并可选择发布由当前生产方契约证明的缓存键。
/// Legacy results remain replayable in their own runs; their historical envelopes did not prove overrides.
/// 旧结果仍可在原运行重放；其历史概要未证明授权切换后的生产方。返回持久化结果。
fn store_owned_result(
    transaction: &Transaction<'_>,
    namespace: &str,
    run: &str,
    batch: &VceEmbeddingBatchRecord,
    result: &VceEmbeddingBatchResult,
    now: i64,
    publish_cache: bool,
) -> Result<(), String> {
    result.validate()?;
    batch.validate()?;
    // Migration and direct persistence obey the same exact request coverage and dimensionality.
    // 迁移和直接持久化遵循相同的精确请求覆盖及维度约束。
    let contract = shared_manifest_store::run_contract(transaction, namespace, run)?;
    let expected = batch
        .input_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    let actual = result
        .vectors
        .iter()
        .map(|vector| vector.input_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if result.batch_id != batch.batch_id
        || actual != expected
        || result.vectors.len() != batch.input_ids.len()
        || result
            .vectors
            .iter()
            .any(|vector| vector.values.len() != contract.embedding_space.dimension)
    {
        return Err(String::from(
            "Embedding result does not match its frozen request or dimension",
        ));
    }
    if let Some(existing) = load(transaction, namespace, run, &result.batch_id)? {
        return if existing == *result {
            Ok(())
        } else {
            Err(String::from(
                "Embedding result conflicts with its immutable durable value",
            ))
        };
    }
    // Normalize only the execution ID so identical provider responses can share bytes.
    // 仅规范化执行 ID，使相同供应商响应能够共享字节。
    let mut reusable = result.clone();
    reusable.batch_id = String::from("shared");
    // Preserve provider order because exact replay equality is part of the public contract.
    // 保留供应商顺序，因为精确重放相等性属于公共契约。
    let artifact = shared_artifacts::store_artifact(
        transaction,
        serde_json::to_value(reusable).map_err(|error| error.to_string())?,
    )?;
    transaction
        .execute(
            "INSERT INTO vce_embedding_result_refs VALUES (?,?,?,?,?)",
            params![namespace, run, result.batch_id, artifact, now],
        )
        .map_err(|error| error.to_string())?;
    if let Some(key) =
        request_identity(transaction, namespace, run, batch)?.filter(|_| publish_cache)
    {
        transaction.execute("INSERT INTO vce_embedding_result_cache VALUES (?,?,?) ON CONFLICT(request_hash) DO UPDATE SET artifact_hash=excluded.artifact_hash,last_used_unix_ms=excluded.last_used_unix_ms", params![key,artifact,now]).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Atomically adopt a compatible cache result into an active run before any provider submission.
/// 在供应商提交前，将兼容缓存结果原子接纳到活动运行。
/// Returns the owned result or absence; unresolved remote tasks must be reconciled by the caller first.
/// 返回已归属结果或空值；调用者必须先对账未决远端任务。
pub(super) fn reuse(
    transaction: &Transaction<'_>,
    namespace: &str,
    run: &str,
    batch: &VceEmbeddingBatchRecord,
    now: i64,
) -> Result<Option<VceEmbeddingBatchResult>, String> {
    if let Some(result) = load(transaction, namespace, run, &batch.batch_id)? {
        return Ok(Some(result));
    }
    // No producer fingerprint means there is no proof that another run's vectors are compatible.
    // 无生产方指纹意味着无法证明其他运行的向量兼容。
    let Some(key) = request_identity(transaction, namespace, run, batch)? else {
        return Ok(None);
    };
    // Exact input identities and contract select a single immutable cache entry.
    // 精确输入身份与契约选择唯一不可变缓存条目。
    let artifact: Option<String> = transaction
        .query_row(
            "SELECT artifact_hash FROM vce_embedding_result_cache WHERE request_hash=?",
            [&key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some(artifact) = artifact else {
        return Ok(None);
    };
    // Read and validate before publishing the new owner reference.
    // 发布新归属引用前读取并校验。
    let result = decode(transaction, &artifact, &batch.batch_id)?;
    if result
        .vectors
        .iter()
        .map(|vector| vector.input_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        != batch.input_ids.iter().map(String::as_str).collect()
    {
        return Err(String::from(
            "Cached embedding result input identity mismatch",
        ));
    }
    transaction
        .execute(
            "INSERT INTO vce_embedding_result_refs VALUES (?,?,?,?,?)",
            params![namespace, run, batch.batch_id, artifact, now],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE vce_embedding_result_cache SET last_used_unix_ms=? WHERE request_hash=?",
            params![now, key],
        )
        .map_err(|error| error.to_string())?;
    Ok(Some(result))
}

/// Convert one historical result per transaction, verifying read-back before removing old bytes.
/// 每个历史结果独立事务转换，回读校验后才删除旧字节。
/// Interrupted migration resumes through remaining legacy rows; returns exact data errors.
/// 中断迁移从剩余旧记录恢复；返回明确数据错误。
pub(super) fn migrate(connection: &mut Connection) -> Result<(), String> {
    // Absence is the completed migration marker; runtime reads use only references.
    // 不存在即迁移完成标记；运行时仅通过引用读取。
    let legacy = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='vce_embedding_batch_results'",
            [],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if legacy.is_none() {
        return Ok(());
    }
    loop {
        // One result is the bounded migration unit.
        // 单个结果是有界迁移单元。
        let row = connection.query_row("SELECT namespace_id,run_id,batch_id,result_json,updated_at_unix_ms FROM vce_embedding_batch_results LIMIT 1", [], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,i64>(4)?))).optional().map_err(|error| error.to_string())?;
        let Some((namespace, run, batch_id, json, now)) = row else {
            break;
        };
        // Parent and batch metadata must already exist for every historical durable result.
        // 每份历史持久结果必须已存在父运行和批次元数据。
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        // Decode public records with their existing validation contract.
        // 按已有校验契约解码公共记录。
        let batch_json: String = transaction.query_row("SELECT batch_json FROM vce_embedding_run_batches WHERE namespace_id=? AND run_id=? AND batch_id=?", params![namespace,run,batch_id], |row| row.get(0)).map_err(|error| error.to_string())?;
        let batch: VceEmbeddingBatchRecord =
            serde_json::from_str(&batch_json).map_err(|error| error.to_string())?;
        let result: VceEmbeddingBatchResult =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        result.validate()?;
        if result.batch_id != batch_id {
            return Err(String::from("Legacy embedding result owner mismatch"));
        }
        // Old envelopes can name the original binding after an approved producer switch.
        // 旧概要在批准切换生产方后仍可能记录原始绑定。
        // Preserve exact owned replay, but never assert cross-run producer compatibility from that field.
        // 保留精确归属重放，但绝不根据该字段断言跨运行生产方兼容。
        store_owned_result(&transaction, &namespace, &run, &batch, &result, now, false)?;
        if load(&transaction, &namespace, &run, &batch_id)?.as_ref() != Some(&result) {
            return Err(String::from("Embedding result migration failed read-back"));
        }
        transaction.execute("DELETE FROM vce_embedding_batch_results WHERE namespace_id=? AND run_id=? AND batch_id=?", params![namespace,run,batch_id]).map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
    }
    connection
        .execute_batch("DROP TABLE vce_embedding_batch_results")
        .map_err(|error| error.to_string())
}
