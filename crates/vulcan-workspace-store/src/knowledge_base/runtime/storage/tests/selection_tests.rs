//! Real isolated-storage tests for confirmation and reversible ignores.
//! 确认及可撤销忽略的真实隔离存储测试。
use super::*;
use crate::knowledge_base::{
    KnowledgeFileConfirmation, KnowledgeFileDisposition as Disposition, KnowledgeFilePreview,
    KnowledgeIgnoredFile,
};

/// Measures real preview latency over 1762 imported files; no model or user database is involved.
/// 测量 1762 份已导入文件的真实预览延迟，不使用模型或用户数据库。
#[tokio::test]
async fn display_preview_large_corpus_remains_bounded() {
    // The temporary runtime and source tree are owned exclusively by this test.
    // 临时运行时和来源目录仅归当前测试所有。
    let (temporary, runtime, library) = in_process_runtime("大清单预览").await;
    // One independent source directory contains a production-sized file count.
    // 一个独立来源目录包含接近实际使用规模的文件数量。
    let root = temporary.path().join("source");
    fs::create_dir(&root).unwrap();
    for index in 0..1762 {
        fs::write(
            root.join(format!("{index:04}.md")),
            format!(
                "# 文档 {index}\r\n\r\n## 第一节\r\n普通内容\r\n\r\n## 第二节\r\n可检索内容\r\n"
            ),
        )
        .unwrap();
    }
    runtime
        .import_markdown_directory(&library, &root)
        .await
        .unwrap();
    // Timing covers the complete read-only endpoint rather than only its SQL query.
    // 计时覆盖完整只读入口，而不只是其中的 SQL 查询。
    let started = std::time::Instant::now();
    // Real stored documents exercise pagination, binding validation and normalized content hashes.
    // 真实存储文档覆盖分页、绑定校验和规范化内容哈希。
    let preview = runtime.preview_knowledge_display(&library).await.unwrap();
    // A bounded latency protects the confirmation modal from an N+1-query regression.
    // 有界延迟避免确认弹窗再次出现 N+1 查询退化。
    let elapsed = started.elapsed();
    eprintln!(
        "display preview: files={}, elapsed_ms={}",
        preview.entries.len(),
        elapsed.as_millis()
    );
    assert_eq!(preview.entries.len(), 1762);
    assert!(
        preview
            .entries
            .iter()
            .all(|row| row.disposition == Disposition::Ready)
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "preview exceeded five seconds: {elapsed:?}"
    );
    // Compare against the authoritative single-document reader to prevent raw-byte hash substitution.
    // 与权威单文档读取器比较，防止误用文件原始字节哈希。
    let first = &preview.entries[0];
    // The source content has Windows line endings, which import normalizes.
    // 来源内容使用 Windows 换行，导入时会规范化。
    let raw = runtime
        .read_document(&library, first.document_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(
        first.content_sha256,
        Some(crate::knowledge_base::sha256_hex(raw.content.as_bytes()))
    );
    assert_eq!(
        preview.fingerprint,
        runtime
            .preview_knowledge_display(&library)
            .await
            .unwrap()
            .fingerprint
    );
}

/// Selects ready rows matching `paths` from `preview`; returns an exact user confirmation.
/// 从 `preview` 中选择匹配 `paths` 的可用记录，返回精确用户确认。
fn confirm(preview: &KnowledgeFilePreview, paths: &[&str]) -> KnowledgeFileConfirmation {
    KnowledgeFileConfirmation {
        fingerprint: preview.fingerprint.clone(),
        selected_keys: preview
            .entries
            .iter()
            .filter(|row| {
                row.disposition == Disposition::Ready && paths.contains(&row.relative_path.as_str())
            })
            .map(|row| row.key.clone())
            .collect(),
    }
}

/// Waits for the exact fixture job to finish; returns after asserting successful completion.
/// 等待精确夹具任务结束，断言成功完成后返回。
async fn wait_import(runtime: &KnowledgeBaseRuntime, library: &str, id: &str) {
    for _ in 0..500 {
        // Only this isolated library and exact job are observed.
        // 仅观察当前隔离知识库及精确任务。
        let job = runtime
            .latest_import_job(library)
            .await
            .expect("read job")
            .expect("job exists");
        if job.id == id && !matches!(job.state.as_str(), "queued" | "running") {
            assert_eq!(job.state, "completed", "{:?}", job.error_message);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("isolated confirmed import did not finish");
}

/// Verifies preview-only behavior, blocked rows, scoped persistent rules and nondestructive selection.
/// 验证仅预览行为、不可选项、范围限定的持久规则和非破坏选择。
#[tokio::test]
async fn confirmed_import_and_ignores_are_scoped_reversible_and_nondestructive() {
    // The fixture owns all filesystem and database changes.
    // 夹具持有全部文件系统及数据库变更。
    let (temporary, runtime, library) = in_process_runtime("文件确认").await;
    // Two roots prove identical filenames do not share rules.
    // 两个根目录验证同名文件不共享规则。
    let root = temporary.path().join("one");
    // The second root contains an identically named Markdown source.
    // 第二个根目录包含同名 Markdown 来源。
    let other_root = temporary.path().join("two");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&other_root).unwrap();
    fs::write(root.join("a.md"), "# A\nfirst").unwrap();
    fs::write(root.join("b.md"), "# B\nsecond").unwrap();
    fs::write(root.join("big.md"), "x".repeat(65_537)).unwrap();
    fs::write(root.join("notes.txt"), "not Markdown").unwrap();
    fs::write(other_root.join("b.md"), "# Another B").unwrap();
    // Preview scans without creating any durable job or importing raw content.
    // 预览扫描不创建持久任务，也不导入原始内容。
    let preview = runtime
        .preview_knowledge_import(&library, &root, None)
        .await
        .unwrap();
    assert_eq!(preview.entries.len(), 4);
    assert!(runtime.latest_import_job(&library).await.unwrap().is_none());
    assert_eq!(
        runtime.get_library(&library).await.unwrap().document_count,
        0
    );
    assert_eq!(
        preview
            .entries
            .iter()
            .find(|row| row.relative_path == "big.md")
            .unwrap()
            .disposition,
        Disposition::Oversized
    );
    assert_eq!(
        preview
            .entries
            .iter()
            .find(|row| row.relative_path == "notes.txt")
            .unwrap()
            .disposition,
        Disposition::Unsupported
    );
    // A forged selection cannot opt a disabled oversized row back in.
    // 伪造选择不能重新纳入禁用的超大记录。
    let forged = KnowledgeFileConfirmation {
        fingerprint: preview.fingerprint.clone(),
        selected_keys: vec![
            preview
                .entries
                .iter()
                .find(|row| row.relative_path == "big.md")
                .unwrap()
                .key
                .clone(),
        ],
    };
    assert!(
        runtime
            .start_confirmed_knowledge_import(&library, None, &root, forged)
            .await
            .is_err()
    );
    assert!(runtime.latest_import_job(&library).await.unwrap().is_none());
    // Only A is explicitly selected; B remains unimported rather than being implicitly included.
    // 仅明确选择 A，B 保持未导入，不会被隐式纳入。
    let job = runtime
        .start_confirmed_knowledge_import(&library, None, &root, confirm(&preview, &["a.md"]))
        .await
        .unwrap();
    wait_import(&runtime, &library, &job.id).await;
    assert_eq!(
        runtime.get_library(&library).await.unwrap().document_count,
        1
    );
    // A matching imported file becomes completed and unavailable, while B remains selectable.
    // 匹配的已导入文件成为已完成且不可选，B 仍可选。
    let preview = runtime
        .preview_knowledge_import(&library, &root, None)
        .await
        .unwrap();
    assert_eq!(
        preview
            .entries
            .iter()
            .find(|row| row.relative_path == "a.md")
            .unwrap()
            .disposition,
        Disposition::Completed
    );
    // The rule uses the canonical root returned by the backend, not a guessed path representation.
    // 规则使用后端返回的规范根目录，而不是猜测路径表示。
    let b = preview
        .entries
        .iter()
        .find(|row| row.relative_path == "b.md")
        .unwrap();
    // User ignore remains distinct from this-run selection.
    // 用户忽略与仅本次选择保持区分。
    let rule = KnowledgeIgnoredFile {
        source_root: b.source_root.clone(),
        relative_path: b.relative_path.clone(),
    };
    runtime
        .update_knowledge_ignored_files(&library, vec![rule.clone()], true)
        .await
        .unwrap();
    assert!(
        runtime
            .start_confirmed_knowledge_import(&library, None, &root, confirm(&preview, &["b.md"]))
            .await
            .is_err()
    );
    fs::write(root.join("b.md"), "# B\nmodified while ignored").unwrap();
    // Content edits never implicitly remove a persistent ignore.
    // 内容修改绝不隐式解除持久忽略。
    let ignored = runtime
        .preview_knowledge_import(&library, &root, None)
        .await
        .unwrap();
    assert_eq!(
        ignored
            .entries
            .iter()
            .find(|row| row.relative_path == "b.md")
            .unwrap()
            .disposition,
        Disposition::Ignored
    );
    assert_eq!(
        runtime
            .preview_knowledge_import(&library, &other_root, None)
            .await
            .unwrap()
            .entries[0]
            .disposition,
        Disposition::Ready
    );
    // Saving unrelated settings must retain explicit ignore rules.
    // 保存无关配置必须保留明确忽略规则。
    runtime
        .set_library_settings(
            &library,
            &crate::knowledge_base::KnowledgeLibrarySettings::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        runtime
            .get_library(&library)
            .await
            .unwrap()
            .settings
            .ignored_files,
        vec![rule.clone()]
    );
    runtime
        .update_knowledge_ignored_files(&library, vec![rule.clone()], false)
        .await
        .unwrap();
    // Clearing an ignore restores eligibility only after the current file passes inspection.
    // 取消忽略后，仅当前文件通过检查才恢复可选。
    let restored = runtime
        .preview_knowledge_import(&library, &root, None)
        .await
        .unwrap();
    assert_eq!(
        restored
            .entries
            .iter()
            .find(|row| row.relative_path == "b.md")
            .unwrap()
            .disposition,
        Disposition::Ready
    );
    // Hash validation catches same-size file changes, not only metadata growth.
    // 哈希验证捕获等长文件变化，而不只检查元数据增长。
    let old = fs::read(root.join("b.md")).unwrap();
    fs::write(root.join("b.md"), vec![b'z'; old.len()]).unwrap();
    assert!(
        runtime
            .start_confirmed_knowledge_import(&library, None, &root, confirm(&restored, &["b.md"]))
            .await
            .is_err()
    );
    // Missing A is not deleted when B alone is confirmed for supplementation.
    // 单独确认补充 B 时，不删除已缺失的 A。
    fs::remove_file(root.join("a.md")).unwrap();
    // A fresh scan is required after both filesystem changes.
    // 两次文件系统变化后必须重新扫描。
    let current = runtime
        .preview_knowledge_import(&library, &root, None)
        .await
        .unwrap();
    // The new job processes B while preserving the prior A row and chunks.
    // 新任务处理 B，同时保留此前 A 的记录和切片。
    let job = runtime
        .start_confirmed_knowledge_import(&library, None, &root, confirm(&current, &["b.md"]))
        .await
        .unwrap();
    wait_import(&runtime, &library, &job.id).await;
    assert_eq!(
        runtime.get_library(&library).await.unwrap().document_count,
        2
    );
    assert!(
        runtime
            .list_documents(&library, None, 10, 0)
            .await
            .unwrap()
            .iter()
            .any(|document| document.relative_path == "a.md")
    );
    // Other libraries never inherit a root/path rule from this library.
    // 其他知识库绝不继承当前知识库的根目录及路径规则。
    let other = runtime.create_library("独立规则", "", &[]).await.unwrap();
    runtime
        .update_knowledge_ignored_files(&library, vec![rule.clone()], true)
        .await
        .unwrap();
    assert_eq!(
        runtime
            .preview_knowledge_import(&other.id, &root, None)
            .await
            .unwrap()
            .entries
            .iter()
            .find(|row| row.relative_path == "b.md")
            .unwrap()
            .disposition,
        Disposition::Ready
    );
    // Removal remains possible when the original source directory no longer exists.
    // 原始来源目录不再存在时仍允许取消忽略。
    fs::rename(&root, temporary.path().join("moved")).unwrap();
    runtime
        .update_knowledge_ignored_files(&library, vec![rule], false)
        .await
        .unwrap();
    assert!(
        runtime
            .get_library(&library)
            .await
            .unwrap()
            .settings
            .ignored_files
            .is_empty()
    );
}

/// Verifies removing a manual ignore never bypasses the source-size safety boundary.
/// 验证取消手动忽略绝不绕过来源大小安全边界。
#[tokio::test]
async fn clearing_ignore_rechecks_size_and_default_selection_is_explicit() {
    // Isolated files exercise the inclusive limit and one byte above it.
    // 隔离文件覆盖包含上限的边界及超过一字节。
    let (temporary, runtime, library) = in_process_runtime("忽略边界").await;
    // The source root contains no user data.
    // 来源根目录不包含用户数据。
    let root = temporary.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("boundary.md"), "x".repeat(65_536)).unwrap();
    fs::write(root.join("large.md"), "x".repeat(65_537)).unwrap();
    // The preview preserves the oversized row with an explicit disabled reason.
    // 预览保留超大记录及明确禁用原因。
    let preview = runtime
        .preview_knowledge_import(&library, &root, None)
        .await
        .unwrap();
    assert_eq!(
        preview
            .entries
            .iter()
            .find(|row| row.relative_path == "boundary.md")
            .unwrap()
            .disposition,
        Disposition::Ready
    );
    // A disabled row can be persistently ignored but cannot become selectable merely by unignoring.
    // 禁用记录可以持久忽略，但不会因取消忽略直接变为可选。
    let row = preview
        .entries
        .iter()
        .find(|row| row.relative_path == "large.md")
        .unwrap();
    // Exact rule identity is the only persisted user intent.
    // 精确规则身份是唯一持久化用户意图。
    let rule = KnowledgeIgnoredFile {
        source_root: row.source_root.clone(),
        relative_path: row.relative_path.clone(),
    };
    runtime
        .update_knowledge_ignored_files(&library, vec![rule.clone()], true)
        .await
        .unwrap();
    runtime
        .update_knowledge_ignored_files(&library, vec![rule], false)
        .await
        .unwrap();
    // Reinspection still returns oversized until the file is actually changed.
    // 文件实际改变之前，重新检查仍返回超大。
    let preview = runtime
        .preview_knowledge_import(&library, &root, None)
        .await
        .unwrap();
    assert_eq!(
        preview
            .entries
            .iter()
            .find(|row| row.relative_path == "large.md")
            .unwrap()
            .disposition,
        Disposition::Oversized
    );
    // An empty confirmed selection processes no files; absence of confirmation never means select-all.
    // 明确空选择不处理文件，缺少确认绝不等于全选。
    let job = runtime
        .start_confirmed_knowledge_import(&library, None, &root, confirm(&preview, &[]))
        .await
        .unwrap();
    wait_import(&runtime, &library, &job.id).await;
    assert_eq!(
        runtime.get_library(&library).await.unwrap().document_count,
        0
    );
}
