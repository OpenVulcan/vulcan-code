//! Human package lifecycle: current snapshot export and fresh-library import only.
//! 人工套件生命周期：仅当前快照导出与新库导入。

use super::package_archive::{prepare_knowledge_package, write_package};
use super::*;
use std::path::Path;

impl KnowledgeBaseRuntime {
    /// Exports one exact library to an existing absolute directory; returns the immutable archive identity.
    /// 将一个精确知识库导出到已有绝对目录；返回不可变归档身份。
    /// The mutation guard freezes content, purpose, mode and dictionary across the logical snapshot.
    /// 变更守卫在逻辑快照期间冻结内容、用途、模式和词典。
    pub async fn export_package(
        &self,
        library_id: &str,
        destination_directory: &Path,
    ) -> Result<KnowledgePackageExportResult, String> {
        // No source SQLite or vector file is copied or checkpointed for this format.
        // 此格式不复制或检查点任何来源 SQLite 或向量文件。
        let _guard = self.library_mutation_guard(library_id).await?;
        // Unsupported libraries retain deletion only and cannot be exported as current data.
        // 不支持的库仅保留删除，不能作为当前数据导出。
        let library = self.require_active_library(library_id).await?;
        // Logical reads share one consistent SQLite snapshot.
        // 逻辑读取共享一个一致 SQLite 快照。
        let (_, binding) = self.ensure_library_binding(library_id).await?;
        // Current rows and dictionary config are captured by their existing typed authorities.
        // 当前行与词典配置由既有类型化权威捕获。
        let snapshot = local_sql::with_read_snapshot_async(
            &self.host_runtime.local,
            self.resolve_library_binding(binding)?,
            content::serialize_package_content,
        )
        .await?;
        // Own the explicitly selected directory before entering blocking ZIP I/O.
        // 进入阻塞 ZIP 输入输出前持有显式选择目录。
        let destination = destination_directory.to_path_buf();
        tokio::task::spawn_blocking(move || write_package(library, snapshot, &destination))
            .await
            .map_err(|error| format!("Failed to join knowledge export: {error}"))?
    }

    /// Inspects a bounded current-format archive without creating a library or modifying any existing content.
    /// 检查有界当前格式归档，不创建知识库或修改任何已有内容。
    pub async fn inspect_package(
        &self,
        archive_path: &Path,
    ) -> Result<KnowledgePackageInspection, String> {
        // Archive validation owns the only file handle used for digest and decoding.
        // 归档验证持有摘要和解码使用的唯一文件句柄。
        let path = archive_path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            prepare_knowledge_package(&path, None).map(|prepared| prepared.inspection)
        })
        .await
        .map_err(|error| format!("Failed to join knowledge package inspection: {error}"))?
    }

    /// Imports a previously inspected archive under an explicit new name; requires the exact approved archive digest.
    /// 以显式新名称导入已检查归档；要求精确已批准归档摘要。
    /// Creates one unbound independent library whose history begins with a single human import change.
    /// 创建一个未绑定独立知识库，其历史从唯一人工导入变更开始。
    pub async fn import_package_as_new(
        &self,
        archive_path: &Path,
        name: &str,
        expected_sha256: &str,
    ) -> Result<KnowledgePackageImportResult, String> {
        // Preview identity is checked again before any application registry write.
        // 任何应用注册写入前再次检查预检身份。
        let path = archive_path.to_path_buf();
        // Own the approved checksum for blocking validation.
        // 为阻塞验证持有已批准校验和。
        let expected = expected_sha256.to_string();
        // The whole package is validated before allocating the new library.
        // 分配新库前验证整个套件。
        let prepared =
            tokio::task::spawn_blocking(move || prepare_knowledge_package(&path, Some(&expected)))
                .await
                .map_err(|error| {
                    format!("Failed to join knowledge package validation: {error}")
                })??;
        // Newly imported libraries have no project authorization until the human explicitly binds them.
        // 新导入库在人工显式绑定前没有项目授权。
        let library = self
            .create_library(
                name,
                &prepared.inspection.manifest.description,
                &[],
                &prepared.inspection.manifest.settings,
            )
            .await?;
        // Dictionary configuration is restored without importing any derived index rows.
        // 恢复词典配置，不导入任何派生索引行。
        let installed = async {
            self.set_custom_words(&library.id, prepared.inspection.manifest.custom_words)
                .await?;
            self.import_package_content(&library.id, prepared.content)
                .await
        }
        .await;
        // Failed fresh-library installation is removed; existing libraries and histories are never replaced.
        // 移除失败的新库安装；绝不替换已有知识库和历史。
        let receipt = match installed {
            Ok(receipt) => receipt,
            Err(error) => {
                // A concurrent content change must be preserved rather than removed by failed-import cleanup.
                // 并发内容变更必须保留，不能由失败导入清理移除。
                self.discard_empty_library(&library.id)
                    .await
                    .map_err(|cleanup| {
                        format!(
                            "{error}; failed to remove new library {}: {cleanup}",
                            library.id
                        )
                    })?;
                return Err(error);
            }
        };
        // Post-commit status failure never triggers rollback of a successful import.
        // 提交后状态读取失败绝不触发成功导入回滚。
        let current = self.get_library(&library.id).await.map_err(|error| {
            format!(
                "Import committed for library {}; status refresh failed: {error}",
                library.id
            )
        })?;
        Ok(KnowledgePackageImportResult {
            library: current,
            change_seq: receipt
                .change_seq
                .ok_or("ERR_KNOWLEDGE_IMPORT_CHANGE_MISSING")?,
        })
    }
}
