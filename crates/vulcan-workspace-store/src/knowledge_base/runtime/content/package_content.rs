//! Portable current-content projection and one-change fresh-library import.
//! 可移植当前内容投影与单变更新库导入。

use super::*;

/// Fully validated candidate content, owned until its single import transaction.
/// 全部验证后的候选内容，在唯一导入事务前保持所有权。
pub(crate) struct ValidatedPackageContent {
    /// New immutable identities and canonical document metadata.
    /// 新不可变身份与规范文档元数据。
    documents: Documents,
    /// Canonical explicit folders.
    /// 规范显式目录。
    folders: Folders,
    /// Exact unique content-addressed bodies.
    /// 精确去重内容寻址正文。
    bodies: BTreeMap<String, String>,
}

impl ValidatedPackageContent {
    /// Returns exact current document count without exposing body bytes.
    /// 返回精确当前文档数，不暴露正文字节。
    pub(crate) fn document_count(&self) -> usize {
        self.documents.len()
    }
    /// Returns exact explicit directory count without audit metadata.
    /// 返回精确显式目录数，不含审计元数据。
    pub(crate) fn folder_count(&self) -> usize {
        self.folders.len()
    }
}

/// One bounded logical export snapshot, excluding history, local IDs and vector rows.
/// 一份有界逻辑导出快照，不含历史、本地身份和向量行。
pub(crate) struct PackageExportContent {
    /// Exact serialized JSONL bytes.
    /// 精确序列化 JSONL 字节。
    pub(crate) jsonl: Vec<u8>,
    /// Current document count.
    /// 当前文档数。
    pub(crate) document_count: usize,
    /// Explicit directory count.
    /// 显式目录数。
    pub(crate) folder_count: usize,
    /// Exact dictionary configuration.
    /// 精确词典配置。
    pub(crate) custom_words: Vec<KnowledgeCustomWord>,
}

/// Validates current-format records and assigns fresh IDs; returns an unpublished candidate or an explicit error.
/// 验证当前格式记录并分配全新身份；返回未发布候选或明确错误。
pub(crate) fn prepare_package_content(
    records: Vec<KnowledgePackageRecord>,
) -> Result<ValidatedPackageContent, String> {
    if records.len() > MAX_KNOWLEDGE_PACKAGE_ENTRIES {
        return Err("ERR_KNOWLEDGE_PACKAGE_ENTRY_LIMIT".into());
    }
    // Separate typed entity maps ensure one exact namespace validation authority.
    // 分离的类型化实体映射确保只有一个精确命名空间验证权威。
    let mut documents = Documents::new();
    // Explicit folders preserve their descriptions but receive new audit sequences.
    // 显式目录保留说明，但接收新审计序号。
    let mut folders = Folders::new();
    // Body ownership moves into the candidate rather than cloning all imported text.
    // 正文所有权移入候选，而非克隆全部导入文本。
    let mut bodies = BTreeMap::new();
    for record in records {
        match record {
            KnowledgePackageRecord::Folder(folder) => {
                // Portable paths are already canonical; malformed spelling is rejected rather than rewritten.
                // 可移植路径应已规范；拒绝畸形拼写而非改写。
                let (path, key) = normalize_path(&folder.path, false)?;
                if path != folder.path {
                    return Err("ERR_KNOWLEDGE_PACKAGE_PATH_NOT_CANONICAL".into());
                }
                if folders
                    .insert(
                        key.clone(),
                        KnowledgeFolder {
                            path_key: key,
                            path,
                            description: folder.description,
                            created_change_seq: 0,
                            updated_change_seq: 0,
                        },
                    )
                    .is_some()
                {
                    return Err("ERR_KNOWLEDGE_PATH_CONFLICT".into());
                }
            }
            KnowledgePackageRecord::Document(portable) => {
                // Summary state is an explicit format field, never inferred from another content source.
                // 摘要状态是显式格式字段，绝不从其他内容来源推断。
                let summary = match portable.summary_state.as_str() {
                    "pending" if portable.summary.is_empty() => None,
                    "provided" | "stale" => Some(portable.summary.as_str()),
                    _ => return Err("ERR_KNOWLEDGE_SUMMARY_STATE_INVALID".into()),
                };
                // The normal document factory is the sole path, body and provenance authority.
                // 普通文档工厂是唯一路径、正文和来源权威。
                let mut document = new_document(
                    &portable.path,
                    &portable.title,
                    summary,
                    &portable.body,
                    portable.embed_policy,
                    portable.provenance,
                )?;
                if document.path != portable.path {
                    return Err("ERR_KNOWLEDGE_PACKAGE_PATH_NOT_CANONICAL".into());
                }
                document.summary_state = portable.summary_state;
                document.keywords = portable.keywords;
                validate_document(&document, &portable.body)?;
                bodies
                    .entry(document.body_sha256.clone())
                    .or_insert(portable.body);
                documents.insert(document.id.clone(), document);
            }
        }
    }
    validate_namespace(&documents, &folders)?;
    Ok(ValidatedPackageContent {
        documents,
        folders,
        bodies,
    })
}

/// Appends one strict entity record; returns an explicit bound error before exceeding the export buffer.
/// 追加一条严格实体记录；超过导出缓冲上限前返回明确边界错误。
fn append_record(jsonl: &mut Vec<u8>, record: &KnowledgePackageRecord) -> Result<(), String> {
    // The temporary allocation is limited to the current already-validated record.
    // 临时分配只限于当前已验证记录。
    let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
    if jsonl
        .len()
        .checked_add(bytes.len())
        .and_then(|size| size.checked_add(1))
        .ok_or("ERR_KNOWLEDGE_PACKAGE_TOO_LARGE")? as u64
        > MAX_KNOWLEDGE_PACKAGE_BYTES
    {
        return Err("ERR_KNOWLEDGE_PACKAGE_TOO_LARGE".into());
    }
    jsonl.extend(bytes);
    jsonl.push(b'\n');
    Ok(())
}

/// Serializes one current read snapshot; checks source size before loading metadata or body strings.
/// 序列化一个当前读取快照；加载元数据或正文前检查来源大小。
pub(crate) fn serialize_package_content(
    connection: &Connection,
) -> Result<PackageExportContent, String> {
    // SQL computes a lower bound on serialized bytes without materializing the corpus in Rust.
    // SQL 无需在 Rust 中物化语料即可计算序列化字节下界。
    let (count, bytes): (usize, u64) = connection.query_row("SELECT
        (SELECT COUNT(*) FROM kb_documents)+(SELECT COUNT(*) FROM kb_folders),
        COALESCE((SELECT SUM(length(CAST(d.path AS BLOB))+length(CAST(d.title AS BLOB))+length(CAST(d.summary AS BLOB))+length(CAST(d.keywords_json AS BLOB))+length(CAST(d.provenance_json AS BLOB))+length(CAST(b.content AS BLOB))) FROM kb_documents d JOIN kb_blobs b ON b.sha256=d.body_sha256),0)
        +COALESCE((SELECT SUM(length(CAST(path AS BLOB))+length(CAST(description AS BLOB))) FROM kb_folders),0);",
        [], |row| Ok((unsigned_column(row, 0)?, unsigned_column(row, 1)?))).map_err(|error| error.to_string())?;
    if count > MAX_KNOWLEDGE_PACKAGE_ENTRIES {
        return Err("ERR_KNOWLEDGE_PACKAGE_ENTRY_LIMIT".into());
    }
    if bytes > MAX_KNOWLEDGE_PACKAGE_BYTES {
        return Err("ERR_KNOWLEDGE_PACKAGE_TOO_LARGE".into());
    }
    // Existing typed row readers remain the sole metadata decoder.
    // 既有类型化行读取器仍是唯一元数据解码器。
    let documents = documents(connection)?;
    // Explicit folders are exported once; implicit ancestors reconstruct from document paths.
    // 显式目录仅导出一次；隐式祖先从文档路径重建。
    let folders = folders(connection)?;
    validate_namespace(&documents, &folders)?;
    // Capture exact counts before consuming owned rows.
    // 消费自有记录前捕获精确数量。
    let document_count = documents.len();
    // The folder count describes explicit metadata only.
    // 目录数量仅描述显式元数据。
    let folder_count = folders.len();
    // The buffer never grows beyond the shared package bound.
    // 缓冲绝不增长超过共享套件边界。
    let mut jsonl = Vec::new();
    for folder in folders.into_values() {
        append_record(
            &mut jsonl,
            &KnowledgePackageRecord::Folder(KnowledgePackageFolder {
                path: folder.path,
                description: folder.description,
            }),
        )?;
    }
    // Sort by path for deterministic ordering independent of newly assigned import UUIDs.
    // 按路径排序，确保顺序不受重新分配的导入 UUID 影响。
    let mut documents: Vec<_> = documents.into_values().collect();
    documents.sort_by(|first, second| first.path_key.cmp(&second.path_key));
    for document in documents {
        // Each exact body is checked before entering the portable representation.
        // 每份精确正文在进入可移植表示前都经过检查。
        let body = body(connection, &document.body_sha256)?;
        validate_document(&document, &body)?;
        append_record(
            &mut jsonl,
            &KnowledgePackageRecord::Document(KnowledgePackageDocument {
                path: document.path,
                title: document.title,
                summary: document.summary,
                summary_state: document.summary_state,
                keywords: document.keywords,
                body,
                embed_policy: document.embed_policy,
                provenance: document.provenance,
            }),
        )?;
    }
    // Dictionary rows are configuration and are captured in the same snapshot as content.
    // 词典行属于配置，与内容在同一快照捕获。
    let custom_words = vulcan_sqlite_engine::tokenizer::list_custom_words(connection)
        .map_err(|error| error.to_string())?
        .words
        .into_iter()
        .map(|entry| {
            Ok(KnowledgeCustomWord {
                word: entry.word,
                weight: u32::try_from(entry.weight).map_err(|error| error.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PackageExportContent {
        jsonl,
        document_count,
        folder_count,
        custom_words,
    })
}

impl KnowledgeBaseRuntime {
    /// Imports validated content only into an untouched fresh library; returns its sole initial change.
    /// 仅向未经修改的新库导入已验证内容；返回唯一初始变更。
    pub(in crate::knowledge_base::runtime) async fn import_package_content(
        &self,
        library_id: &str,
        content: ValidatedPackageContent,
    ) -> Result<KnowledgeWriteResult, String> {
        // Fresh history admission shares the same lock as AI writes, restore and deletion.
        // 新历史准入与 AI 写入、还原和删除共享同一锁。
        let _guard = self.library_mutation_guard(library_id).await?;
        // Human-owned mode remains the exact policy selected by the validated manifest.
        // 人工拥有的模式保持为已验证清单选择的精确策略。
        let settings = self.content_settings(library_id, None).await?;
        // The existing database is opened only after exact format validation.
        // 已有数据库仅在精确格式验证后打开。
        let (_, binding) = self.ensure_library_binding(library_id).await?;
        // Import actor identity never comes from portable user-supplied fields.
        // 导入执行者身份绝不来自可移植用户字段。
        let actor = KnowledgeChangeActor {
            kind: "user".into(),
            session_id: None,
            turn_id: None,
            tool_call_id: None,
        };
        // One connection owns the empty-state check and the one-change transaction.
        // 一个连接拥有空状态检查与单变更事务。
        let receipt = local_sql::with_connection_async(&self.host_runtime.local, self.resolve_library_binding(binding)?, move |connection| {
            let empty = connection.query_row("SELECT NOT EXISTS(SELECT 1 FROM kb_changes) AND NOT EXISTS(SELECT 1 FROM kb_documents) AND NOT EXISTS(SELECT 1 FROM kb_folders);", [], |row| row.get::<_, bool>(0)).map_err(|error| error.to_string())?;
            if !empty { return Err("ERR_KNOWLEDGE_IMPORT_REQUIRES_FRESH_LIBRARY".into()); }
            commit_snapshot(connection, SnapshotChange { before: &Documents::new(), before_folders: &Folders::new(),
                after: content.documents, after_folders: content.folders, bodies: content.bodies, actor: &actor,
                operation: "import", note: "Imported current content from a verified portable package", restore_target_seq: None, settings: &settings })
        }).await?;
        self.wake_knowledge_worker(library_id).await;
        Ok(receipt)
    }
}
