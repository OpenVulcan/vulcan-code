//! Explicit native full-text reconstruction from authoritative memory state.
//! 从权威记忆状态显式重建原生全文索引。

use super::*;
use vulcan_service_api::{VmmNativeLexicalIndexStatus, VmmNativeLexicalRebuildResult};
use vulcan_workspace_store::vmm_data_plane::VmmLexicalDocument;

impl NativeVmmMaintenance {
    /// Reads the current source version and physical index counts without initializing or repairing data.
    /// 读取当前源版本与物理索引计数，不初始化或修复数据。
    /// Returns observational counts; equal counts do not establish semantic index integrity.
    /// 返回观察计数，计数相等不代表索引语义完整。
    pub(crate) async fn lexical_index_status(&self) -> Result<VmmNativeLexicalIndexStatus, String> {
        // Temporarily exclude publication while reading both counters; the read itself changes no data.
        // 读取两个计数期间暂时排除发布，读取本身不改变数据。
        let _operation_guard = self.operation_gate.write().await;
        let snapshot = self.runtime.storage_snapshot()?;
        let counts = self.data_plane.lexical_index_counts().await?;
        Ok(VmmNativeLexicalIndexStatus {
            storage_version: snapshot.storage_version,
            active_memory_count: usize_to_u64(
                snapshot
                    .dataset
                    .memories
                    .iter()
                    .filter(|memory| memory.state == MemoryState::Active)
                    .count(),
            ),
            document_count: counts.document_count,
            indexed_row_count: counts.indexed_row_count,
        })
    }

    /// Reconstructs all active full-text documents after explicit versioned confirmation.
    /// 在明确的带版本确认后重建全部活跃全文文档。
    /// Takes the displayed storage version and confirmation; returns the upstream acknowledged count.
    /// 接收显示的存储版本与确认，返回上游确认的数量。
    /// Leaves domain records, embeddings, bindings and recycle contents unchanged; no AI admission occurs.
    /// 保持领域记录、向量、绑定及回收内容不变，不发生 AI 准入。
    pub(crate) async fn rebuild_lexical_index(
        &self,
        expected_storage_version: u64,
        confirmed: bool,
    ) -> Result<VmmNativeLexicalRebuildResult, String> {
        if !confirmed {
            return Err(String::from(
                "Confirm rebuilding the full-text index from the current memory content first.",
            ));
        }
        // Match the existing maintenance lock order; rebuild cannot race content publication.
        // 匹配既有维护锁顺序，重建不能与内容发布竞争。
        let _project_guard = self.project_mutation_gate.lock().await;
        let _operation_guard = self.operation_gate.write().await;
        if !self.native_backend_enabled.load(Ordering::Acquire) {
            return Err(String::from(
                "Full-text index rebuild is only available when internal native memory is enabled.",
            ));
        }
        self.runtime.recover_storage_mutations().await?;
        let snapshot = self.runtime.storage_snapshot()?;
        if snapshot.storage_version != expected_storage_version {
            return Err(String::from(
                "The memory content changed; refresh the full-text index status and confirm again.",
            ));
        }
        // Only the same Active lifecycle set used by persist_lexical_transition enters this derived table.
        // 仅将 persist_lexical_transition 使用的同一活跃生命周期集合放入派生表。
        let documents: Vec<_> = snapshot
            .dataset
            .memories
            .iter()
            .filter(|memory| memory.state == MemoryState::Active)
            .map(|memory| VmmLexicalDocument {
                memory_id: memory.id.to_string(),
                summary: memory.summary.clone(),
                details: memory.content.clone(),
            })
            .collect();
        let expected_count = usize_to_u64(documents.len());
        self.data_plane
            .replace_lexical_documents(&documents)
            .await?;
        let reindexed_memory_count = self.data_plane.rebuild_lexical_index().await?;
        if reindexed_memory_count != expected_count {
            return Err(format!(
                "The full-text index rebuild receipt disagrees with the authoritative active memory count: expected {expected_count}, actual {reindexed_memory_count}."
            ));
        }
        Ok(VmmNativeLexicalRebuildResult {
            storage_version: snapshot.storage_version,
            reindexed_memory_count,
        })
    }
}
