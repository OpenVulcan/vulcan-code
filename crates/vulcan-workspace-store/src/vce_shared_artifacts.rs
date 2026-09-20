//! Content-addressed, lossless JSON artifacts with shared text leaves.
//! 基于内容身份的无损 JSON 产物与共享文本叶节点。

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use vulcan_context_engine::identity::sha256_hex_parts;

/// Keep each physical text row bounded while preserving exact UTF-8 bytes.
/// 保持每条物理文本记录有界，同时保留精确 UTF-8 字节。
const PAYLOAD_CHUNK_BYTES: usize = 32 * 1024;
/// Externalize substantive text while leaving small metadata inline.
/// 将较大文本独立保存，短小元数据仍内联保存。
const SHARED_TEXT_MIN_BYTES: usize = 256;

/// Relational ownership for immutable payloads and their verified artifact descriptors.
/// 不可变负载及其可验证产物描述的关系归属。
pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS vce_shared_payloads (
    payload_hash TEXT PRIMARY KEY, byte_size INTEGER NOT NULL CHECK(byte_size >= 0)
);
CREATE TABLE IF NOT EXISTS vce_shared_payload_chunks (
    payload_hash TEXT NOT NULL, chunk_index INTEGER NOT NULL, text_chunk TEXT NOT NULL,
    PRIMARY KEY(payload_hash, chunk_index),
    FOREIGN KEY(payload_hash) REFERENCES vce_shared_payloads(payload_hash) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS vce_shared_artifacts (
    artifact_hash TEXT PRIMARY KEY,
    FOREIGN KEY(artifact_hash) REFERENCES vce_shared_payloads(payload_hash)
);
CREATE TABLE IF NOT EXISTS vce_shared_artifact_payloads (
    artifact_hash TEXT NOT NULL, payload_hash TEXT NOT NULL,
    PRIMARY KEY(artifact_hash, payload_hash),
    FOREIGN KEY(artifact_hash) REFERENCES vce_shared_artifacts(artifact_hash) ON DELETE CASCADE,
    FOREIGN KEY(payload_hash) REFERENCES vce_shared_payloads(payload_hash)
);
CREATE INDEX IF NOT EXISTS idx_vce_shared_payload_owner
ON vce_shared_artifact_payloads(payload_hash, artifact_hash);
"#;

/// Exact JSON shape with text slots replaced by null and addressed by unambiguous JSON pointers.
/// 精确 JSON 形状，文本位置替换为空值，并通过无歧义 JSON 指针寻址。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDescriptor {
    /// Version the codec independently from the public VCE wire types.
    /// 编码版本独立于 VCE 公共线类型。
    version: u32,
    /// Original JSON with only explicitly listed text slots removed.
    /// 原始 JSON，仅移除明确列出的文本位置。
    value: Value,
    /// Exact pointer-to-content references, including the empty pointer for a root string.
    /// 精确指针到内容的引用，包含根字符串使用的空指针。
    texts: BTreeMap<String, String>,
}

/// Hash exact UTF-8 bytes; callers must never substitute a model name or run identity.
/// 对精确 UTF-8 字节求哈希；调用者不得替换为模型名称或运行身份。
/// `text` is the immutable payload; returns its SHA-256 content identity.
/// `text` 是不可变负载；返回其 SHA-256 内容身份。
pub(super) fn content_identity(text: &str) -> String {
    sha256_hex_parts([text.as_bytes()])
}

/// Insert a payload once inside the caller's ownership transaction.
/// 在调用者的归属事务中仅插入一次负载。
/// `transaction` owns atomicity and `text` supplies exact bytes; returns the content identity.
/// `transaction` 负责原子性，`text` 提供精确字节；返回内容身份或存储错误。
fn store_payload(transaction: &Transaction<'_>, text: &str) -> Result<String, String> {
    // Identical bodies are shared across fields, nodes, runs and namespaces in this project database.
    // 相同正文在本项目数据库的字段、节点、运行和命名空间之间共享。
    let identity = content_identity(text);
    // Only a newly inserted immutable header admits fragment writes.
    // 仅新插入的不可变头部允许写入分片。
    let inserted = transaction.execute(
        "INSERT INTO vce_shared_payloads(payload_hash,byte_size) VALUES (?,?) ON CONFLICT DO NOTHING",
        params![identity, text.len() as i64],
    ).map_err(|error| error.to_string())?;
    if inserted == 0 {
        // A mismatch indicates damaged metadata, not permission to overwrite a shared object.
        // 元数据不匹配表示损坏，不代表可以覆盖共享对象。
        let size: i64 = transaction
            .query_row(
                "SELECT byte_size FROM vce_shared_payloads WHERE payload_hash=?",
                [&identity],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if usize::try_from(size).ok() != Some(text.len()) {
            return Err(String::from(
                "VCE shared payload size conflicts with its identity",
            ));
        }
        return Ok(identity);
    }
    // The prepared statement is reused for every bounded fragment.
    // 每个有界分片复用同一个预编译语句。
    let mut insert = transaction.prepare_cached(
        "INSERT INTO vce_shared_payload_chunks(payload_hash,chunk_index,text_chunk) VALUES (?,?,?)",
    ).map_err(|error| error.to_string())?;
    // Byte offset advances only across valid UTF-8 boundaries.
    // 字节偏移只在合法 UTF-8 边界处推进。
    let mut offset = 0;
    // Contiguous ordinals make truncated or missing fragments detectable on read.
    // 连续序号使读取时能够发现分片截断或缺失。
    let mut ordinal = 0_i64;
    while offset < text.len() {
        // Avoid splitting a multibyte scalar at a page boundary.
        // 避免在分页边界切断多字节字符。
        let mut end = (offset + PAYLOAD_CHUNK_BYTES).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        insert
            .execute(params![identity, ordinal, &text[offset..end]])
            .map_err(|error| error.to_string())?;
        ordinal += 1;
        offset = end;
    }
    Ok(identity)
}

/// Read and verify an immutable payload without opening a connection per fragment.
/// 读取并校验不可变负载，不为每个分片新建连接。
/// `connection` holds the database gate and `identity` selects exact bytes; returns verified text.
/// `connection` 持有数据库闸门，`identity` 选择精确字节；返回已验证文本或错误。
fn load_payload(connection: &Connection, identity: &str) -> Result<String, String> {
    // The size header is mandatory even for an empty payload.
    // 即使负载为空也必须存在大小头部。
    let size: i64 = connection
        .query_row(
            "SELECT byte_size FROM vce_shared_payloads WHERE payload_hash=?",
            [identity],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    // Query the full key so successive payload reads do not scan unrelated fragments.
    // 使用完整键查询，使连续负载读取不扫描无关分片。
    let mut statement = connection.prepare_cached(
        "SELECT chunk_index,text_chunk FROM vce_shared_payload_chunks WHERE payload_hash=? ORDER BY chunk_index",
    ).map_err(|error| error.to_string())?;
    // Rows are streamed into one verified body instead of projected through another JSON copy.
    // 将记录流式拼入一个待验证正文，不通过另一份 JSON 副本投影。
    let rows = statement
        .query_map([identity], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    // Do not allocate from an unverified stored size.
    // 不根据未验证的持久大小分配内存。
    let mut text = String::new();
    for (ordinal, row) in rows.enumerate() {
        // Each physical fragment must occupy its declared contiguous position.
        // 每个物理分片必须位于声明的连续位置。
        let (stored_ordinal, fragment) = row.map_err(|error| error.to_string())?;
        if stored_ordinal != ordinal as i64
            || fragment.is_empty()
            || fragment.len() > PAYLOAD_CHUNK_BYTES
        {
            return Err(String::from(
                "VCE shared payload fragments are incomplete or invalid",
            ));
        }
        text.push_str(&fragment);
    }
    if usize::try_from(size).ok() != Some(text.len()) || content_identity(&text) != identity {
        return Err(String::from(
            "VCE shared payload failed content verification",
        ));
    }
    Ok(text)
}

/// Move large JSON string values into shared payloads, recording exact locations.
/// 将较大的 JSON 字符串值移入共享负载，并记录精确位置。
/// The transaction owns writes; `value` is transformed in place and `texts` receives references.
/// 事务负责写入；原地转换 `value`，将引用写入 `texts`；返回成功或错误。
fn externalize(
    transaction: &Transaction<'_>,
    value: &mut Value,
    pointer: &str,
    texts: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    match value {
        Value::String(text) if text.len() >= SHARED_TEXT_MIN_BYTES => {
            texts.insert(pointer.to_owned(), store_payload(transaction, text)?);
            *value = Value::Null;
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                externalize(transaction, item, &format!("{pointer}/{index}"), texts)?;
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields.iter_mut() {
                // RFC 6901 escaping prevents field names from aliasing array or nested paths.
                // JSON 指针转义防止字段名称与数组或嵌套路径混淆。
                let escaped = key.replace('~', "~0").replace('/', "~1");
                externalize(transaction, item, &format!("{pointer}/{escaped}"), texts)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Store a lossless JSON artifact; its reference must be inserted in this same transaction.
/// 保存无损 JSON 产物；必须在同一事务中插入其归属引用。
/// Returns the immutable descriptor hash for `value`, or an explicit storage/encoding error.
/// 返回 `value` 的不可变描述哈希，或明确的存储及编码错误。
pub(super) fn store_artifact(
    transaction: &Transaction<'_>,
    value: Value,
) -> Result<String, String> {
    // Explicit codec metadata makes references distinguishable from arbitrary user JSON objects.
    // 显式编码元数据使引用与任意用户 JSON 对象保持可区分。
    let mut descriptor = ArtifactDescriptor {
        version: 1,
        value,
        texts: BTreeMap::new(),
    };
    externalize(
        transaction,
        &mut descriptor.value,
        "",
        &mut descriptor.texts,
    )?;
    // Canonical descriptor bytes provide cross-run artifact identity.
    // 规范描述字节提供跨运行产物身份。
    let encoded = serde_json::to_string(&descriptor).map_err(|error| error.to_string())?;
    // The descriptor is itself a bounded shared payload.
    // 描述自身也是分片有界的共享负载。
    let identity = store_payload(transaction, &encoded)?;
    transaction
        .execute(
            "INSERT INTO vce_shared_artifacts(artifact_hash) VALUES (?) ON CONFLICT DO NOTHING",
            [&identity],
        )
        .map_err(|error| error.to_string())?;
    // Dependency edges allow reclamation to retain every referenced text even when other artifacts retire.
    // 依赖边使其他产物退役后仍可保留全部被引用文本。
    let mut reference = transaction.prepare_cached(
        "INSERT INTO vce_shared_artifact_payloads(artifact_hash,payload_hash) VALUES (?,?) ON CONFLICT DO NOTHING",
    ).map_err(|error| error.to_string())?;
    for payload in descriptor.texts.values() {
        reference
            .execute(params![identity, payload])
            .map_err(|error| error.to_string())?;
    }
    Ok(identity)
}

/// Restore a verified artifact without mutating its storage or fabricating missing leaves.
/// 恢复已验证产物，不修改存储，也不补造缺失叶节点。
/// `identity` selects one artifact on `connection`; returns its exact original JSON value.
/// `identity` 选择 `connection` 上的一个产物；返回精确原始 JSON 值或明确错误。
pub(super) fn load_artifact(connection: &Connection, identity: &str) -> Result<Value, String> {
    // Distinguish a declared artifact from an arbitrary text payload.
    // 区分已声明产物与任意文本负载。
    let exists = connection
        .query_row(
            "SELECT 1 FROM vce_shared_artifacts WHERE artifact_hash=?",
            [identity],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Err(String::from("VCE shared artifact is missing"));
    }
    // Parse only the supported explicit codec contract.
    // 仅解析受支持的显式编码契约。
    let mut descriptor: ArtifactDescriptor =
        serde_json::from_str(&load_payload(connection, identity)?)
            .map_err(|error| error.to_string())?;
    if descriptor.version != 1 {
        return Err(String::from("Unsupported VCE shared artifact codec"));
    }
    // Repeated symbols or plan inputs reuse a single body read within this artifact.
    // 同一产物内重复符号或计划输入复用一次正文读取。
    let mut bodies = BTreeMap::<String, String>::new();
    for (pointer, payload) in descriptor.texts {
        if !bodies.contains_key(&payload) {
            // Validate dependency ownership as well as the text's content hash.
            // 同时校验依赖归属和文本内容哈希。
            let linked = connection.query_row(
            "SELECT 1 FROM vce_shared_artifact_payloads WHERE artifact_hash=? AND payload_hash=?",
            params![identity, payload], |_| Ok(()),
        ).optional().map_err(|error| error.to_string())?;
            if linked.is_none() {
                return Err(String::from(
                    "VCE shared artifact text reference is missing",
                ));
            }
            bodies.insert(payload.clone(), load_payload(connection, &payload)?);
        }
        // A listed slot must still be null; conflicting or missing slots indicate corruption.
        // 已列出位置必须仍为空值；冲突或缺失位置表示损坏。
        let slot = descriptor
            .value
            .pointer_mut(&pointer)
            .ok_or_else(|| String::from("VCE shared artifact text slot is missing"))?;
        if !slot.is_null() {
            return Err(String::from("VCE shared artifact text slot is not empty"));
        }
        *slot = Value::String(bodies[&payload].clone());
    }
    Ok(descriptor.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify exact Unicode/path round trips, shared leaf reuse and transaction rollback.
    /// 验证精确 Unicode 及路径往返、共享叶节点复用和事务回滚。
    #[test]
    fn shared_artifacts_preserve_values_and_reuse_bodies() {
        // The fixture never opens a user database.
        // 夹具不打开用户数据库。
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        // A multibyte body spans physical fragments and appears in multiple roles.
        // 多字节正文跨越物理分片，并出现在多种角色中。
        let body = "代码正文\n".repeat(20_000);
        // Escaped field names and genuine null values must remain distinguishable.
        // 转义字段名称和真实空值必须保持可区分。
        let first =
            serde_json::json!({"source":body,"symbols":[body,body],"a/b~c":body,"empty":null});
        // Publication is atomic with all immutable payload dependencies.
        // 发布与全部不可变负载依赖保持原子性。
        let transaction = connection.transaction().unwrap();
        // Identical artifacts share their descriptor as well as their text leaves.
        // 相同产物同时共享描述和文本叶节点。
        let identity = store_artifact(&transaction, first.clone()).unwrap();
        assert_eq!(
            store_artifact(&transaction, first.clone()).unwrap(),
            identity
        );
        transaction.commit().unwrap();
        assert_eq!(load_artifact(&connection, &identity).unwrap(), first);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM vce_shared_payloads", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        {
            // Unpublished failed work must not leak a descriptor or text into the database.
            // 未发布的失败操作不得向数据库泄漏描述或文本。
            let transaction = connection.transaction().unwrap();
            store_artifact(
                &transaction,
                serde_json::json!({"body":"different".repeat(1000)}),
            )
            .unwrap();
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM vce_shared_payloads", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        connection.execute("UPDATE vce_shared_payload_chunks SET text_chunk='corrupt' WHERE payload_hash=? AND chunk_index=0", [content_identity(&body)]).unwrap();
        assert!(
            load_artifact(&connection, &identity)
                .unwrap_err()
                .contains("verification")
        );
    }
}
