//! Import path, warning bounds, and cooperative scan cancellation regressions.
//! 导入路径、告警边界与协作扫描取消回归。

use super::*;
use tempfile::tempdir;

/// Verifies slash conversion rejects parent-directory components.
/// 验证斜杠转换会拒绝父目录组件。
#[test]
fn slash_path_rejects_parent_components() {
    assert!(slash_path(Path::new("docs/guide.md")).is_ok());
    assert!(slash_path(Path::new("../guide.md")).is_err());
}

/// Verifies warnings remain bounded by count and character length.
/// 验证告警数量与字符长度保持有界。
#[test]
fn warnings_are_bounded() {
    let mut warnings = Vec::new();
    for _ in 0..(MAX_IMPORT_WARNINGS + 5) {
        push_warning(&mut warnings, "界".repeat(MAX_WARNING_CHARS + 10));
    }
    assert_eq!(warnings.len(), MAX_IMPORT_WARNINGS);
    assert!(
        warnings
            .iter()
            .all(|warning| warning.chars().count() == MAX_WARNING_CHARS)
    );
}

/// Verifies cancellation stops directory scanning before any candidate is accepted.
/// 验证取消会在接受任何候选文件前停止目录扫描。
#[test]
fn markdown_scan_honors_preexisting_cancellation() {
    let temporary = tempdir().expect("create cancelled scan root");
    fs::write(temporary.path().join("document.md"), "# Cancelled")
        .expect("write cancelled scan fixture");
    let cancellation = AtomicBool::new(true);

    let error = scan_markdown_files(temporary.path(), &cancellation, None)
        .expect_err("cancelled scan must stop");

    assert_eq!(error, KNOWLEDGE_OPERATION_CANCELLED_ERROR);
}
