//! Derived full-text and dictionary lifecycle, with transactional reconstruction.
//! 派生全文与词典生命周期，采用事务重建。

use super::*;
use rusqlite::Connection;
use vulcan_sqlite_engine::{
    fts::ensure_fts_index,
    tokenizer::{self, TokenizerMode},
};

/// Rebuilds or removes FTS in the caller's transaction; returns exact storage errors.
/// 在调用方事务内重建或删除全文索引；返回精确存储错误。
pub(super) fn rebuild_fts(connection: &Connection, enabled: bool) -> Result<(), String> {
    if enabled {
        ensure_fts_index(connection, KNOWLEDGE_FTS_INDEX, TokenizerMode::Jieba)
            .map_err(|e| e.to_string())?;
        connection.execute_batch(&format!("DELETE FROM {KNOWLEDGE_FTS_INDEX};
            INSERT INTO {KNOWLEDGE_FTS_INDEX}(id,file_path,title,content)
            SELECT c.id,d.path,d.title,c.text FROM kb_chunks c JOIN kb_documents d ON d.id=c.document_id;
            UPDATE knowledge_metadata SET value=(SELECT value FROM knowledge_metadata WHERE key='head_change_seq') WHERE key='indexed_change_seq';
            UPDATE knowledge_metadata SET value=(SELECT value FROM knowledge_metadata WHERE key='dictionary_revision') WHERE key='indexed_dictionary_revision';
            UPDATE knowledge_metadata SET value='ready' WHERE key='index_status';")).map_err(|e|e.to_string())?;
    } else {
        connection
            .execute_batch(&format!(
                "DROP TABLE IF EXISTS {KNOWLEDGE_FTS_INDEX};
            UPDATE knowledge_metadata SET value='disabled' WHERE key='index_status';"
            ))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reads one exact nonnegative metadata integer; returns corruption rather than a guessed default.
/// 读取一个精确非负元数据整数；返回损坏错误而非猜测默认值。
fn metadata_integer(connection: &Connection, key: &str) -> Result<u64, String> {
    connection
        .query_row(
            "SELECT value FROM knowledge_metadata WHERE key=?;",
            [key],
            |row| row.get::<_, String>(0),
        )
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|e| format!("ERR_KNOWLEDGE_METADATA_INTEGER: {e}"))
}

impl KnowledgeBaseRuntime {
    /// Rebuilds only current derived FTS data; returns integrity after an atomic publication.
    /// 仅重建当前派生全文数据；原子发布后返回完整性。
    pub async fn rebuild_index(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeIndexIntegritySummary, String> {
        // Rebuilds share the content lock, preventing lost writes between scan and publication.
        // 重建共享内容锁，防止扫描和发布之间丢失写入。
        let guard = self.library_mutation_guard(library_id).await?;
        // The target mode is library-owned, never inferred from old tables.
        // 目标模式由库拥有，绝不从旧表推断。
        let library = self.require_active_library(library_id).await?;
        // Invalid settings cannot be replaced with defaults during maintenance.
        // 维护期间非法设置不能以默认值替换。
        let settings = library.settings.ok_or("ERR_KNOWLEDGE_SETTINGS_MISSING")?;
        // Resolve the established host binding before entering the transaction.
        // 进入事务前解析既有宿主绑定。
        let (_, binding_id) = self.ensure_library_binding(library_id).await?;
        local_sql::with_connection_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            move |connection| {
                let transaction = connection
                    .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                    .map_err(|e| e.to_string())?;
                rebuild_fts(&transaction, settings.index_mode.has_fts())?;
                transaction.commit().map_err(|e| e.to_string())
            },
        )
        .await?;
        drop(guard);
        self.index_integrity(library_id).await
    }

    /// Computes exact index consistency for current chunks and dictionaries; returns typed diagnostics.
    /// 为当前切块和词典计算精确索引一致性；返回类型化诊断。
    pub async fn index_integrity(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeIndexIntegritySummary, String> {
        // Registry and metadata errors remain explicit before inspecting derived data.
        // 检查派生数据前保持注册和元数据错误明确。
        let settings = self
            .require_active_library(library_id)
            .await?
            .settings
            .ok_or("ERR_KNOWLEDGE_SETTINGS_MISSING")?;
        // The consistent snapshot covers all authoritative and derived comparisons.
        // 一致快照覆盖全部权威与派生比较。
        let (_, binding_id) = self.ensure_library_binding(library_id).await?;
        local_sql::with_read_snapshot_async(&self.host_runtime.local,self.resolve_library_binding(&binding_id)?,move |connection|{
            tokenizer::ensure_jieba_tokenizer_registered(connection).map_err(|e|e.to_string())?;
            let present=connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?);",[KNOWLEDGE_FTS_INDEX],|row|row.get::<_,bool>(0)).map_err(|e|e.to_string())?;
            let document_count=connection.query_row("SELECT COUNT(*) FROM kb_documents;",[],|row|unsigned_column(row,0)).map_err(|e|e.to_string())?;
            let chunk_count=connection.query_row("SELECT COUNT(*) FROM kb_chunks;",[],|row|unsigned_column(row,0)).map_err(|e|e.to_string())?;
            let (indexed_chunk_count,mismatched_rows)=if present {
                let count=connection.query_row(&format!("SELECT COUNT(*) FROM {KNOWLEDGE_FTS_INDEX};"),[],|row|unsigned_column(row,0)).map_err(|e|e.to_string())?;
                let mismatched=connection.query_row(&format!("SELECT
                    (SELECT COUNT(*) FROM kb_chunks c JOIN kb_documents d ON d.id=c.document_id WHERE
                     (SELECT COUNT(*) FROM {KNOWLEDGE_FTS_INDEX} f WHERE f.id=c.id AND f.file_path=d.path AND f.title=d.title AND f.content=c.text)!=1)
                    +(SELECT COUNT(*) FROM {KNOWLEDGE_FTS_INDEX} f WHERE NOT EXISTS(SELECT 1 FROM kb_chunks c WHERE c.id=f.id));"),[],|row|unsigned_column(row,0)).map_err(|e|e.to_string())?;
                (count,mismatched)
            }else{(0,chunk_count)};
            let head_change_seq=metadata_integer(connection,"head_change_seq")?;
            let indexed_change_seq=metadata_integer(connection,"indexed_change_seq")?;
            let dictionary_revision=metadata_integer(connection,"dictionary_revision")?;
            let indexed_dictionary_revision=metadata_integer(connection,"indexed_dictionary_revision")?;
            let healthy=if settings.index_mode.has_fts(){present && chunk_count==indexed_chunk_count && mismatched_rows==0 && head_change_seq==indexed_change_seq && dictionary_revision==indexed_dictionary_revision}else{!present};
            Ok(KnowledgeIndexIntegritySummary{index_mode:settings.index_mode,document_count,chunk_count,fts_present:present,indexed_chunk_count,mismatched_rows,head_change_seq,indexed_change_seq,dictionary_revision,indexed_dictionary_revision,healthy})
        }).await
    }

    /// Lists this library's exact custom dictionary; returns validated positive weights.
    /// 列出该库精确自定义词典；返回已验证正权重。
    pub async fn list_custom_words(
        &self,
        library_id: &str,
    ) -> Result<Vec<KnowledgeCustomWord>, String> {
        self.require_active_library(library_id).await?;
        // The dictionary table is created by the fresh schema, never as a legacy upgrade.
        // 词典表由全新结构创建，绝非旧格式升级。
        let (_, binding_id) = self.ensure_library_binding(library_id).await?;
        local_sql::with_read_snapshot_async(
            &self.host_runtime.local,
            self.resolve_library_binding(&binding_id)?,
            |connection| {
                tokenizer::list_custom_words(connection)
                    .map_err(|e| e.to_string())?
                    .words
                    .into_iter()
                    .map(|entry| {
                        Ok(KnowledgeCustomWord {
                            word: entry.word,
                            weight: u32::try_from(entry.weight).map_err(|e| e.to_string())?,
                        })
                    })
                    .collect()
            },
        )
        .await
    }

    /// Replaces only human dictionary settings and rebuilds FTS atomically; returns refreshed terms.
    /// 仅替换人工词典设置并原子重建全文索引；返回刷新词语。
    pub async fn set_custom_words(
        &self,
        library_id: &str,
        words: Vec<KnowledgeCustomWord>,
    ) -> Result<Vec<KnowledgeCustomWord>, String> {
        // Reject duplicate terms and invalid weights before any dictionary mutation.
        // 词典变更前拒绝重复词语和非法权重。
        let mut unique = BTreeSet::new();
        for entry in &words {
            if entry.word.trim() != entry.word
                || entry.word.is_empty()
                || entry.word.chars().any(char::is_control)
                || entry.weight == 0
                || !unique.insert(entry.word.clone())
            {
                return Err("ERR_KNOWLEDGE_CUSTOM_WORD_INVALID".into());
            }
        }
        // Dictionary replacement participates in the same library mutation domain.
        // 词典替换参与同一知识库变更域。
        let guard = self.library_mutation_guard(library_id).await?;
        // Current mode determines whether dictionary-dependent FTS exists.
        // 当前模式决定是否存在依赖词典的全文索引。
        let settings = self
            .require_active_library(library_id)
            .await?
            .settings
            .ok_or("ERR_KNOWLEDGE_SETTINGS_MISSING")?;
        // Keep dictionary rows, revision and rebuilt FTS in one SQLite transaction.
        // 将词典行、修订与重建全文索引保持在同一 SQLite 事务。
        let (_, binding_id) = self.ensure_library_binding(library_id).await?;
        local_sql::with_connection_async(&self.host_runtime.local,self.resolve_library_binding(&binding_id)?,move |connection|{
            let transaction=connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            for entry in tokenizer::list_custom_words(&transaction).map_err(|e|e.to_string())?.words {
                tokenizer::remove_custom_word(&transaction,&entry.word).map_err(|e|e.to_string())?;
            }
            for entry in words {tokenizer::upsert_custom_word(&transaction,&entry.word,entry.weight as usize).map_err(|e|e.to_string())?;}
            transaction.execute("UPDATE knowledge_metadata SET value=CAST(value AS INTEGER)+1 WHERE key='dictionary_revision';",[]).map_err(|e|e.to_string())?;
            rebuild_fts(&transaction,settings.index_mode.has_fts())?;
            transaction.commit().map_err(|e|e.to_string())
        }).await?;
        drop(guard);
        self.list_custom_words(library_id).await
    }
}
