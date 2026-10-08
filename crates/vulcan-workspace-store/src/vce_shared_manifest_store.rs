//! Shared preparation artifacts referenced by run-local manifests.
//! 运行范围清单引用的共享准备产物。

use super::shared_artifacts::{content_identity, load_artifact, store_artifact};
use rusqlite::{Connection, Transaction, params};
use serde::Deserialize;
use vulcan_context_engine::{VceCodeNodeSnapshot, VceEmbeddingRunContract};

/// Run references and an exact-contract cache share immutable preparation objects.
/// 运行引用与精确契约缓存共享不可变准备对象。
pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS vce_embedding_run_nodes (
    namespace_id TEXT NOT NULL, run_id TEXT NOT NULL, node_index INTEGER NOT NULL CHECK(node_index >= 0),
    blob_name TEXT NOT NULL, artifact_hash TEXT NOT NULL,
    PRIMARY KEY(namespace_id,run_id,node_index),
    FOREIGN KEY(namespace_id,run_id) REFERENCES vce_embedding_runs(namespace_id,run_id) ON DELETE CASCADE,
    FOREIGN KEY(artifact_hash) REFERENCES vce_shared_artifacts(artifact_hash)
);
CREATE INDEX IF NOT EXISTS idx_vce_node_artifact_owner ON vce_embedding_run_nodes(artifact_hash);
CREATE TABLE IF NOT EXISTS vce_prepared_artifact_cache (
    contract_hash TEXT NOT NULL, blob_name TEXT NOT NULL, artifact_hash TEXT NOT NULL,
    last_used_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(contract_hash,blob_name),
    FOREIGN KEY(artifact_hash) REFERENCES vce_shared_artifacts(artifact_hash)
);
CREATE INDEX IF NOT EXISTS idx_vce_prepared_artifact_owner ON vce_prepared_artifact_cache(artifact_hash);
"#;

/// Read only the known contract field from the existing versioned run envelope.
/// 仅读取已有版本化运行概要中的已知契约字段。
#[derive(Deserialize)]
struct ContractEnvelope {
    /// Frozen preparation and provider compatibility facts.
    /// 冻结的准备及供应商兼容事实。
    contract: VceEmbeddingRunContract,
}

/// Compute a reusable contract identity independently of an execution namespace.
/// 独立于执行命名空间计算可复用契约身份。
/// `contract` retains physical producer, dimensions and preparation rules; returns a stable digest.
/// `contract` 保留物理生产方、维度与准备规则；返回稳定摘要或校验错误。
pub(super) fn contract_identity(contract: &VceEmbeddingRunContract) -> Result<String, String> {
    contract.validate()?;
    // Logical vector-space names change during rebuilds; physical compatibility fields must not change.
    // 逻辑向量空间名称在重建时变化；物理兼容字段必须保持不变。
    let mut reusable = contract.clone();
    reusable.embedding_space.vector_space_id.clear();
    serde_json::to_string(&reusable)
        .map(|json| content_identity(&json))
        .map_err(|error| error.to_string())
}

/// Exclude run-wide purpose presence from per-file preparation identity.
/// 从单文件准备身份中排除运行级用途是否出现。
/// All preparation budgets and physical versions remain frozen; returns an exact cache key.
/// 所有准备预算与物理版本仍冻结；返回精确缓存键或契约错误。
pub(super) fn preparation_identity(contract: &VceEmbeddingRunContract) -> Result<String, String> {
    // Another file can introduce an additional purpose without changing this file's preparation.
    // 其他文件可引入额外用途，但不会改变本文件准备结果。
    let mut per_file = contract.clone();
    per_file.purposes.clear();
    contract_identity(&per_file)
}

/// Load the exact frozen contract of one admitted run, never the current settings.
/// 读取已准入运行的精确冻结契约，绝不读取当前设置替代。
/// The database connection and run keys select the envelope; returns its validated contract.
/// 数据库连接与运行键选择概要；返回经过校验的契约或错误。
pub(super) fn run_contract(
    connection: &Connection,
    namespace: &str,
    run: &str,
) -> Result<VceEmbeddingRunContract, String> {
    // The envelope is bounded metadata and does not contain source bodies.
    // 概要是有界元数据，不包含源码正文。
    let json: String = connection
        .query_row(
            "SELECT manifest_json FROM vce_embedding_runs WHERE namespace_id=? AND run_id=?",
            params![namespace, run],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    // Unknown envelope fields remain owned by the manifest module; the contract itself is strict.
    // 其他概要字段仍由清单模块持有；契约自身采用严格解析。
    let envelope: ContractEnvelope =
        serde_json::from_str(&json).map_err(|error| error.to_string())?;
    envelope.contract.validate()?;
    Ok(envelope.contract)
}

/// Store one node and its owner atomically, normalizing only its run-local generation.
/// 原子保存一个节点及其归属，仅规范化运行范围的代次。
/// The caller supplies exact run/index keys and a validated node; returns after reference persistence.
/// 调用者提供精确运行、序号与已校验节点；引用持久化后返回或报告错误。
pub(super) fn store_node(
    transaction: &Transaction<'_>,
    namespace: &str,
    run: &str,
    index: i64,
    node: &VceCodeNodeSnapshot,
    contract: &VceEmbeddingRunContract,
    now: i64,
) -> Result<(), String> {
    // Generation is reconstructed from the owning run, so unchanged artifacts can span retries.
    // 代次从所属运行重建，使未变化的产物能够跨重试共享。
    let mut reusable = node.clone();
    reusable.generation = 0;
    if let Some(prepared) = &mut reusable.prepared_index {
        prepared.pending.generation = 0;
    }
    // Shared text extraction removes repeated source, chunk, symbol and input body copies.
    // 共享文本提取消除重复的源码、代码块、符号与输入正文副本。
    let artifact = store_artifact(
        transaction,
        serde_json::to_value(&reusable).map_err(|error| error.to_string())?,
    )?;
    transaction.execute(
        "INSERT INTO vce_embedding_run_nodes(namespace_id,run_id,node_index,blob_name,artifact_hash) VALUES (?,?,?,?,?)",
        params![namespace, run, index, node.blob_name, artifact],
    ).map_err(|error| error.to_string())?;
    // Only successful deterministic preparation is reusable; explicit structural failures must be retried.
    // 仅成功的确定性准备可复用；显式结构失败必须重新尝试。
    if node.prepared_index.is_some() && node.structure_fallback_reason.is_none() {
        transaction.execute(
            "INSERT INTO vce_prepared_artifact_cache(contract_hash,blob_name,artifact_hash,last_used_unix_ms) VALUES (?,?,?,?) ON CONFLICT(contract_hash,blob_name) DO UPDATE SET artifact_hash=excluded.artifact_hash,last_used_unix_ms=excluded.last_used_unix_ms",
            params![preparation_identity(contract)?, node.blob_name, artifact, now],
        ).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Restore a shared node into one precise run generation.
/// 将共享节点恢复到一个精确运行代次。
/// Checks the stored Blob identity and normalized generation before returning a usable snapshot.
/// 返回可用快照前校验持久 Blob 身份和规范化代次。
pub(super) fn restore_node(
    connection: &Connection,
    artifact: &str,
    blob: &str,
    generation: u64,
) -> Result<VceCodeNodeSnapshot, String> {
    // Public types retain their full lossless shape at the storage boundary.
    // 公共类型在存储边界保留完整无损形状。
    let mut node: VceCodeNodeSnapshot =
        serde_json::from_value(load_artifact(connection, artifact)?)
            .map_err(|error| error.to_string())?;
    if node.blob_name != blob || node.generation != 0 || generation == 0 {
        return Err(String::from("VCE shared node ownership is invalid"));
    }
    if let Some(prepared) = &mut node.prepared_index {
        if prepared.pending.generation != 0 || prepared.pending.blob_name != blob {
            return Err(String::from("VCE shared preparation identity is invalid"));
        }
        prepared.pending.generation = generation;
    }
    node.generation = generation;
    Ok(node)
}
