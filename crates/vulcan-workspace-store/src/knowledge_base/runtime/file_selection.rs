//! Read-only file preflight and explicit persistent ignore mutations.
//! 只读文件执行前检查与明确的持久忽略变更。
use super::import::{canonical_import_root, path_key, slash_path};
use super::*;
use crate::knowledge_base::{
    KNOWLEDGE_SOURCE_BYTE_LIMIT, KnowledgeDisplayIndexStatus,
    KnowledgeFileDisposition as Disposition, KnowledgeFilePreview, KnowledgeFilePreviewEntry,
    KnowledgeIgnoredFile, KnowledgeLibrarySettings, KnowledgeLibrarySummary, sha256_hex,
};
use crate::runtime_assets;
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::{Component, Path};
use vulcan_runtime_assets::text_decode::decode_complete_user_text;

/// Returns a normalized exact root/path key; inputs are canonical roots and validated relative paths.
/// 返回规范化的精确根目录及路径键，输入为规范根目录和已验证相对路径。
pub(super) fn file_key(root: &str, relative: &str) -> String {
    sha256_hex(format!("{}\n{}", path_key(Path::new(root)), relative.to_lowercase()).as_bytes())
}

/// Reads at most the source limit plus one byte from `path`, rejecting links; returns bytes or an I/O error.
/// 从 `path` 最多读取来源上限加一字节并拒绝链接，返回字节或 I/O 错误。
pub(super) fn read_preview_bytes(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    // Recheck kind after scanning because the filesystem may change before execution.
    // 扫描后重新检查类型，因为执行前文件系统可能变化。
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(std::io::Error::other("文件类型已变化，请重新扫描。"));
    }
    // Bounding the read protects against a file growing between metadata inspection and open.
    // 限制读取量，防止文件在元数据检查与打开之间增长。
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(KNOWLEDGE_SOURCE_BYTE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Builds a read-only filesystem inventory for `root`; returns disabled rows rather than hiding oversized files.
/// 为 `root` 构建只读文件清单，返回不可选记录而不是隐藏超大文件。
fn scan_preview(root: &Path) -> Result<Vec<KnowledgeFilePreviewEntry>, String> {
    // All paths originate from non-reparse traversal below the validated root.
    // 全部路径来自已验证根目录下不跟随重解析点的遍历。
    let root_text = runtime_assets::native_host_path_text_from_path(root);
    // Pending directories and output rows are bounded to avoid unbounded traversal.
    // 待遍历目录及输出记录有界，避免无界遍历。
    let mut pending = vec![root.to_path_buf()];
    // Entries retain every file, including unsupported extensions.
    // 记录保留所有文件，包括不支持的扩展名。
    let mut entries = Vec::new();
    // Count both directories and files toward the preview safety bound.
    // 目录和文件均计入预览安全上限。
    let mut visited = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| format!("扫描目录失败：{error}"))?
        {
            // An entry is inspected without following symlinks or junctions.
            // 检查目录项但不跟随符号链接或联接点。
            let entry = entry.map_err(|error| format!("读取目录项失败：{error}"))?;
            visited += 1;
            if visited > 20_000 {
                return Err(String::from("预览目录项超过 20000，请选择更小的来源目录。"));
            }
            // File metadata supplies the exact selection size boundary.
            // 文件元数据提供精确的选择大小边界。
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|error| format!("读取文件属性失败：{error}"))?;
            if metadata.is_dir() && !metadata_is_link_or_reparse(&metadata) {
                pending.push(entry.path());
                continue;
            }
            // Relative paths come only from the scanned directory tree.
            // 相对路径只从扫描目录树中取得。
            let relative = slash_path(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|error| error.to_string())?,
            )?;
            // Start disabled for unknown file kinds, then validate Markdown content.
            // 未知文件类型初始不可选，然后验证 Markdown 内容。
            let mut row = KnowledgeFilePreviewEntry {
                key: file_key(&root_text, &relative),
                source_root: root_text.clone(),
                relative_path: relative,
                byte_size: metadata.len(),
                disposition: Disposition::Unsupported,
                detail: None,
                document_id: None,
                content_sha256: None,
                already_completed: false,
            };
            if metadata.is_file()
                && !metadata_is_link_or_reparse(&metadata)
                && entry
                    .path()
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("md"))
            {
                row.disposition = if metadata.len() > KNOWLEDGE_SOURCE_BYTE_LIMIT {
                    Disposition::Oversized
                } else {
                    Disposition::Ready
                };
                if row.disposition == Disposition::Ready {
                    match read_preview_bytes(&entry.path()) {
                        Ok(bytes) => {
                            row.byte_size = bytes.len() as u64;
                            if row.byte_size > KNOWLEDGE_SOURCE_BYTE_LIMIT {
                                row.disposition = Disposition::Oversized;
                            } else if let Err(error) = decode_complete_user_text(&bytes) {
                                row.disposition = Disposition::Unreadable;
                                row.detail = Some(error.to_string());
                            } else {
                                row.content_sha256 = Some(sha256_hex(&bytes));
                            }
                        }
                        Err(error) => {
                            row.disposition = Disposition::Unreadable;
                            row.detail = Some(error.to_string());
                        }
                    }
                }
            }
            entries.push(row);
        }
    }
    entries.sort_by(|left, right| left.key.cmp(&right.key));
    if entries.windows(2).any(|pair| pair[0].key == pair[1].key) {
        return Err(String::from("来源文件路径在大小写规范化后冲突。"));
    }
    Ok(entries)
}

/// Raw digest row for read-only completed-file classification.
/// 用于只读已完成文件分类的原始摘要记录。
#[derive(Deserialize)]
struct ImportedHash {
    /// Stable source-relative identity.
    /// 稳定的来源相对身份。
    relative_path_key: String,
    /// Stored raw document identity.
    /// 已保存原始文档身份。
    id: String,
    /// Stored file-byte digest.
    /// 已保存文件字节摘要。
    source_sha256: String,
}

/// Minimal stored document projection for batched display preflight, without chunk joins.
/// 显示执行前批量检查使用的最小已存文档投影，不联接切片表。
#[derive(Deserialize)]
struct DisplayPreviewRow {
    /// Exact imported document identity.
    /// 精确的已导入文档身份。
    id: String,
    /// Authoritative source owner.
    /// 权威来源归属。
    source_id: String,
    /// Stored source-relative path.
    /// 已存来源相对路径。
    relative_path: String,
    /// Measured original file byte size.
    /// 实测原始文件字节数。
    byte_size: i64,
    /// Normalized imported text, not the original file-byte hash.
    /// 规范化导入文本，而非原始文件字节哈希。
    content: String,
}

/// One authoritative raw-content revision; preview never requires a full physical FTS audit.
/// 单个权威原始内容修订，预览不需要完整物理 FTS 审计。
#[derive(Deserialize)]
struct PreviewRevisionRow {
    /// Decimal revision value stored in knowledge_metadata.
    /// knowledge_metadata 中保存的十进制修订值。
    value: String,
}

impl KnowledgeBaseRuntime {
    /// Reads only content_revision from the validated `space`/`binding`; returns its checked numeric value.
    /// 仅从已验证的 `space`/`binding` 读取 content_revision，返回经过校验的数值。
    async fn read_preview_revision(&self, space: &str, binding: &str) -> Result<u64, String> {
        // Reading this scalar avoids comparing every stored chunk against the FTS index twice per preview.
        // 读取该标量，避免每次预览两次将全部存储切片与 FTS 索引比较。
        let rows: Vec<PreviewRevisionRow> = self
            .query_library_rows(
                space,
                binding,
                "SELECT value FROM knowledge_metadata WHERE key = 'content_revision';",
                Vec::new(),
            )
            .await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| String::from("知识库内容修订缺失。"))?
            .value
            .parse::<u64>()
            .map_err(|error| format!("知识库内容修订无效：{error}"))
    }

    /// Previews `root` for import into `library_id`, optionally remapping `source_id`; never creates an import job.
    /// 预览向 `library_id` 导入的 `root`，可选重新映射 `source_id`，绝不创建导入任务。
    pub async fn preview_knowledge_import(
        &self,
        library_id: &str,
        root: &Path,
        source_id: Option<&str>,
    ) -> Result<KnowledgeFilePreview, String> {
        // Canonicalization is shared with the authoritative importer.
        // 规范化与权威导入器共用。
        let canonical = canonical_import_root(root)?;
        // The persisted library owns ignore rules and source associations.
        // 持久化知识库持有忽略规则及来源关联。
        let library = self.get_library(library_id).await?;
        // Resolve once and retain the starting revision to reject concurrent corpus changes.
        // 解析一次绑定并保留起始修订，用于拒绝并发内容变化。
        let (space, binding) = self.ensure_library_binding(library_id).await?;
        // The scalar version check does not scan the search index.
        // 标量版本检查不扫描搜索索引。
        let revision = self.read_preview_revision(&space, &binding).await?;
        // Offload filesystem traversal without creating any write-side operation.
        // 将文件系统遍历移到阻塞工作线程，不创建任何写入操作。
        let mut entries = tokio::task::spawn_blocking(move || scan_preview(&canonical))
            .await
            .map_err(|error| error.to_string())??;
        // Sources are authoritative for identity remapping and collision detection.
        // 来源记录是身份重新映射及冲突检测的权威。
        let sources = self.list_sources(library_id).await?;
        // Root identity uses the same path normalization as importer source lookup.
        // 根目录身份使用与导入器来源查找相同的路径规范化。
        let root_key = path_key(&canonical_import_root(root)?);
        // Explicit source IDs must belong to this exact library.
        // 明确来源标识必须属于当前精确知识库。
        let source = match source_id {
            Some(id) => Some(
                sources
                    .iter()
                    .find(|source| source.id == id)
                    .ok_or_else(|| String::from("来源目录已不存在，请刷新。"))?,
            ),
            None => sources
                .iter()
                .find(|source| path_key(Path::new(&source.canonical_root)) == root_key),
        };
        if sources.iter().any(|candidate| {
            path_key(Path::new(&candidate.canonical_root)) == root_key
                && source.is_some_and(|source| candidate.id != source.id)
        }) {
            return Err(String::from("所选目录已属于另一个知识库来源。"));
        }
        if let Some(source) = source {
            // Existing raw hashes avoid reimporting content that already matches byte-for-byte.
            // 已有原始哈希避免重新导入字节完全一致的内容。
            let hashes: Vec<ImportedHash> = self.query_library_rows(&space, &binding,
                "SELECT id, relative_path_key, source_sha256 FROM knowledge_documents WHERE source_id = ?;", vec![text(&source.id)]).await?;
            for row in &mut entries {
                if let Some(existing) = hashes
                    .iter()
                    .find(|existing| existing.relative_path_key == row.relative_path.to_lowercase())
                {
                    row.document_id = Some(existing.id.clone());
                    row.already_completed =
                        row.content_sha256.as_ref() == Some(&existing.source_sha256);
                    if row.already_completed {
                        row.disposition = Disposition::Completed;
                    }
                }
            }
        }
        self.finish_file_preview(&library, "import", &root_key, source_id, entries, revision)
            .await
    }

    /// Finds the reusable pending or active display generation for `settings` and current `revision`.
    /// 根据 `settings` 和当前 `revision` 查找可复用的待完成或活动显示代次。
    pub fn reusable_knowledge_display_generation(
        settings: &KnowledgeLibrarySettings,
        revision: u64,
    ) -> Option<String> {
        if settings.display_index.pending_source_content_revision == Some(revision)
            && settings.display_index.pending_prompt_version.as_deref()
                == Some(settings.curation.prompt_version.as_str())
            && settings.display_index.pending_model_fingerprint
                == settings.curation.desired_model_fingerprint
            && let Some(generation) = &settings.display_index.pending_generation
        {
            return Some(generation.clone());
        }
        if settings.display_index.indexed_prompt_version.as_deref()
            == Some(settings.curation.prompt_version.as_str())
            && settings.display_index.indexed_model_fingerprint
                == settings.curation.desired_model_fingerprint
        {
            return settings.display_index.active_generation.clone();
        }
        None
    }

    /// Previews display generation from the current imported corpus, preserving completed source hashes.
    /// 从当前已导入内容预览显示生成，保留已完成来源的哈希。
    pub async fn preview_knowledge_display(
        &self,
        library_id: &str,
    ) -> Result<KnowledgeFilePreview, String> {
        // Settings and content revision define the sole provenance boundary.
        // 配置和内容修订定义唯一来源边界。
        let library = self.get_library(library_id).await?;
        // Source roots make identically named files in different directories independent.
        // 来源根目录使不同目录下的同名文件保持独立。
        let sources = self.list_sources(library_id).await?;
        // Resolve the database binding once instead of revalidating it for every document.
        // 只解析一次数据库绑定，不为每份文档重复验证。
        let (space, binding) = self.ensure_library_binding(library_id).await?;
        // The current raw revision controls pending-generation reuse.
        // 当前原始修订控制待完成代次复用。
        let revision = self.read_preview_revision(&space, &binding).await?;
        // Successful hashes may come from a matching pending generation or the last published one.
        // 成功哈希可来自匹配的待完成代次或上次发布代次。
        let hashes = if let Some(generation) =
            Self::reusable_knowledge_display_generation(&library.settings, revision)
        {
            self.list_display_generation_source_hashes(
                library_id,
                &generation,
                &super::display::display_fts_index_name(&generation)?,
            )
            .await?
        } else {
            BTreeMap::new()
        };
        // Pagination keeps raw-content reads bounded per query.
        // 分页使每次原始内容查询保持有界。
        let mut offset = 0_i64;
        // Rows represent all source documents, not only selectable ones.
        // 记录代表所有来源文档，而不只是可选项。
        let mut entries = Vec::new();
        loop {
            // Batch exactly the preview fields, without per-document I/O or chunk-count joins.
            // 批量读取精确预览字段，不执行逐文档 I/O 或切片计数联接。
            let page: Vec<DisplayPreviewRow> = self.query_library_rows(&space, &binding,
                "SELECT id, source_id, relative_path, byte_size, content FROM knowledge_documents ORDER BY id LIMIT 128 OFFSET ?;",
                vec![int(offset)]).await?;
            if page.is_empty() {
                break;
            }
            // A partial page terminates enumeration without another binding or count query.
            // 不满一页时结束枚举，不再查询绑定或计数。
            let last_page = page.len() < 128;
            offset += page.len() as i64;
            for document in page {
                // Every imported document must have one authoritative source root.
                // 每份导入文档必须对应一个权威来源根目录。
                let source = sources
                    .iter()
                    .find(|source| source.id == document.source_id)
                    .ok_or_else(|| String::from("文档来源目录缺失，请修复知识库。"))?;
                // Content hashes use the same normalized imported text as generated documents.
                // 内容哈希使用与生成文档相同的规范化导入文本。
                let hash = sha256_hex(document.content.as_bytes());
                // Validate persisted sizes before converting the database projection to public metadata.
                // 将数据库投影转为公共元数据前校验持久化大小。
                let byte_size = nonnegative_u64(document.byte_size, "byte_size")?;
                // Completed sources remain retained even if they now exceed the default limit.
                // 已完成来源即使超过默认上限也继续保留。
                let completed = hashes.get(&document.id) == Some(&hash);
                entries.push(KnowledgeFilePreviewEntry {
                    key: file_key(&source.canonical_root, &document.relative_path),
                    source_root: source.canonical_root.clone(),
                    relative_path: document.relative_path,
                    byte_size,
                    disposition: if completed {
                        Disposition::Completed
                    } else if byte_size > KNOWLEDGE_SOURCE_BYTE_LIMIT {
                        Disposition::Oversized
                    } else {
                        Disposition::Ready
                    },
                    document_id: Some(document.id),
                    content_sha256: Some(hash),
                    already_completed: completed,
                    detail: None,
                });
            }
            if last_page {
                break;
            }
        }
        self.finish_file_preview(&library, "display", "", None, entries, revision)
            .await
    }

    /// Applies the captured `library` rules and fingerprints `entries` only if `expected_revision` is still current.
    /// 应用捕获的 `library` 规则，仅在 `expected_revision` 仍有效时为 `entries` 计算指纹。
    async fn finish_file_preview(
        &self,
        library: &KnowledgeLibrarySummary,
        operation: &str,
        root: &str,
        source_id: Option<&str>,
        mut entries: Vec<KnowledgeFilePreviewEntry>,
        // Starting revision must still match after all rows have been read.
        // 读取全部记录后，起始修订仍必须匹配。
        expected_revision: u64,
    ) -> Result<KnowledgeFilePreview, String> {
        // Exact keys prevent ignore rules from leaking across roots or libraries.
        // 精确键避免忽略规则跨越来源目录或知识库。
        let ignored: BTreeSet<_> = library
            .settings
            .ignored_files
            .iter()
            .map(|file| file_key(&file.source_root, &file.relative_path))
            .collect();
        for row in &mut entries {
            if ignored.contains(&row.key) {
                row.disposition = Disposition::Ignored;
            }
        }
        entries.sort_by(|left, right| left.key.cmp(&right.key));
        // Source revisions reject confirmations made before a concurrent import.
        // 来源修订拒绝并发导入之前作出的确认。
        let (space, binding) = self.ensure_library_binding(&library.id).await?;
        // A changed corpus invalidates the entire preview instead of mixing pages from two revisions.
        // 内容库变化使整个预览失效，避免混合两次修订的分页。
        let revision = self.read_preview_revision(&space, &binding).await?;
        if revision != expected_revision {
            return Err(String::from("知识库内容在扫描期间变化，请重新扫描。"));
        }
        // Only provenance and policy are included; the worker's queued/building status is not content.
        // 只纳入来源和策略，工作器排队或构建状态不属于内容。
        let encoded = serde_json::to_vec(&serde_json::json!([
            library.id,
            operation,
            root,
            source_id,
            revision,
            library.settings.curation,
            library.settings.ignored_files,
            entries
        ]))
        .map_err(|error| error.to_string())?;
        Ok(KnowledgeFilePreview {
            fingerprint: sha256_hex(&encoded),
            entries,
        })
    }

    /// Adds/removes exact `files` for one library; returns updated rules without deleting any corpus data.
    /// 为单个知识库增删精确 `files`，返回更新后的规则，不删除任何内容库数据。
    pub async fn update_knowledge_ignored_files(
        &self,
        library_id: &str,
        files: Vec<KnowledgeIgnoredFile>,
        ignored: bool,
    ) -> Result<Vec<KnowledgeIgnoredFile>, String> {
        // Serialize with import and publication, then reject active worker mutations.
        // 与导入和发布串行化，并拒绝活动工作器期间的变更。
        let _guard = self.library_mutation_guard(library_id).await?;
        if self.active_operations.lock().await.contains_key(library_id) {
            return Err(String::from("知识库操作进行中，请结束后修改忽略规则。"));
        }
        // Current settings retain every unrelated user preference.
        // 当前配置保留全部无关用户偏好。
        let mut settings = self.get_library(library_id).await?.settings;
        // Track semantic changes so no-op requests do not invalidate completed generations.
        // 记录语义变更，避免空操作使已完成代次失效。
        let before = settings.ignored_files.clone();
        for mut file in files {
            if file.relative_path.is_empty()
                || Path::new(&file.relative_path)
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
                || file.relative_path.contains('\\')
            {
                return Err(String::from("忽略项必须是来源目录内的规范相对文件路径。"));
            }
            if ignored {
                file.source_root = runtime_assets::native_host_path_text_from_path(
                    &canonical_import_root(Path::new(&file.source_root))?,
                );
            }
            // Removal intentionally works even after the directory has been moved or deleted.
            // 目录移动或删除后仍允许取消忽略。
            let key = file_key(&file.source_root, &file.relative_path);
            settings
                .ignored_files
                .retain(|existing| file_key(&existing.source_root, &existing.relative_path) != key);
            if ignored {
                settings.ignored_files.push(file);
            }
        }
        settings
            .ignored_files
            .sort_by_key(|file| file_key(&file.source_root, &file.relative_path));
        if settings.ignored_files == before {
            return Ok(before);
        }
        if settings.curation.enabled && settings.display_index.active_generation.is_some() {
            settings.display_index.status = KnowledgeDisplayIndexStatus::Stale;
        }
        // Persist only in existing settings JSON; no database schema migration or implicit file deletion.
        // 仅写入现有配置 JSON，不迁移数据库结构，也不隐式删除文件。
        let serialized = serde_json::to_string(&settings).map_err(|error| error.to_string())?;
        local_sql::execute_script_async(&self.host_runtime.local, LogicalDatabase::AppState,
            "UPDATE knowledge_libraries SET settings_json = ?, updated_at_ms = ? WHERE id = ? AND lifecycle_state = 'active';",
            vec![text(serialized), int(unix_epoch_millis()?), text(library_id)]).await?;
        Ok(settings.ignored_files)
    }
}
