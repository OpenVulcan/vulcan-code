//! Workspace file service tests.
//! 工作区文件服务测试。

use super::*;
use std::path::Path;

/// Verifies workspace file path resolution rejects paths outside the root.
/// 验证工作区文件路径解析会拒绝根目录外路径。
#[test]
fn workspace_file_child_path_rejects_outside_root() {
    let root = test_workspace_file_temp_dir("root");
    let outside = test_workspace_file_temp_dir("outside");
    let error = workspace_file_canonical_child_path(&root, outside.to_str(), "文件路径")
        .expect_err("outside path should be rejected");

    assert!(error.contains("越出工作区"));
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

/// Verifies dot-name filtering can hide and reveal dot-prefixed code files.
/// 验证点名称过滤可以隐藏和显示点开头代码文件。
#[test]
fn workspace_file_entry_summary_filters_dot_names() {
    let root = test_workspace_file_temp_dir("dot-filter");
    let hidden_file = root.join(".hidden.rs");
    std::fs::write(&hidden_file, "fn main() {}\n").expect("write hidden file");

    assert!(
        workspace_file_entry_summary(&root, &hidden_file, &WorkspaceFileFilterOptions::default())
            .is_none()
    );
    let filters = WorkspaceFileFilterOptions {
        ignore_dot_names: false,
        ..WorkspaceFileFilterOptions::default()
    };
    let summary = workspace_file_entry_summary(&root, &hidden_file, &filters)
        .expect("dot file should be visible after disabling dot filter");

    assert_eq!(summary.kind, WorkspaceFileEntryKind::Code);
    assert!(summary.hidden);
    let _ = std::fs::remove_dir_all(root);
}

/// Verifies workspace file kind classification marks raw files as other files.
/// 验证工作区文件类别识别会将 raw 文件标记为其他文件。
#[test]
fn workspace_file_kind_classifies_code_media_and_other_files() {
    assert_eq!(
        workspace_file_kind_from_path(Path::new("src/lib.rs")),
        WorkspaceFileEntryKind::Code
    );
    assert_eq!(
        workspace_file_kind_from_path(Path::new("preview.png")),
        WorkspaceFileEntryKind::Image
    );
    assert_eq!(
        workspace_file_kind_from_path(Path::new("clip.mp4")),
        WorkspaceFileEntryKind::Video
    );
    assert_eq!(
        workspace_file_kind_from_path(Path::new("capture.raw")),
        WorkspaceFileEntryKind::Other
    );
}

/// Verifies non-preview files are visible unless the dedicated filter is enabled.
/// 验证非预览文件默认可见，仅在专用过滤器启用后隐藏。
#[test]
fn workspace_file_entry_summary_filters_non_preview_files_only_when_requested() {
    let root = test_workspace_file_temp_dir("non-preview-filter");
    let data_directory = root.join("DATA").join("yycfg");
    std::fs::create_dir_all(&data_directory).expect("create data directory");
    let spreadsheet = data_directory.join("中文配置.xls");
    std::fs::write(&spreadsheet, "spreadsheet placeholder").expect("write spreadsheet");

    let default_file_summary =
        workspace_file_entry_summary(&root, &spreadsheet, &WorkspaceFileFilterOptions::default())
            .expect("non-preview file should be visible by default");
    let default_directory_summary = workspace_file_entry_summary(
        &root,
        &root.join("DATA"),
        &WorkspaceFileFilterOptions::default(),
    )
    .expect("directory containing non-preview files should be visible by default");
    let filters = WorkspaceFileFilterOptions {
        filter_non_preview_files: true,
        ..WorkspaceFileFilterOptions::default()
    };

    assert_eq!(default_file_summary.kind, WorkspaceFileEntryKind::Other);
    assert_eq!(
        default_directory_summary.kind,
        WorkspaceFileEntryKind::Directory
    );
    assert!(workspace_file_entry_summary(&root, &spreadsheet, &filters).is_none());
    assert!(workspace_file_entry_summary(&root, &root.join("DATA"), &filters).is_none());
    let _ = std::fs::remove_dir_all(root);
}

/// Verifies large code previews are truncated by the backend read limit.
/// 验证大代码预览会被后端读取上限截断。
#[test]
fn workspace_file_read_code_response_truncates_large_files() {
    let root = test_workspace_file_temp_dir("read-limit");
    let file = root.join("large.rs");
    let content = "a".repeat(WORKSPACE_FILE_READ_LIMIT_BYTES + 128);
    std::fs::write(&file, content).expect("write large file");
    let metadata = std::fs::metadata(&file).expect("large file metadata");

    let response =
        workspace_file_read_code_response(&root, &file, &metadata, WorkspaceFileEntryKind::Code)
            .expect("large code preview should read");

    assert!(response.truncated);
    assert_eq!(
        response.content.as_deref().map(str::len),
        Some(WORKSPACE_FILE_READ_LIMIT_BYTES)
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Verifies non-UTF-8 text files can be previewed through the GB18030 fallback.
/// 验证非 UTF-8 文本文件可以通过 GB18030 回退编码预览。
#[test]
fn workspace_file_read_code_response_decodes_gb18030_text() {
    let root = test_workspace_file_temp_dir("gb18030-read");
    let file = root.join("Nations.ini");
    let source = "[国家]\r\n名称=盟重\r\n";
    let (encoded, _, had_errors) = GB18030.encode(source);
    assert!(!had_errors);
    std::fs::write(&file, encoded.as_ref()).expect("write gb18030 ini");
    let metadata = std::fs::metadata(&file).expect("gb18030 file metadata");

    let response =
        workspace_file_read_code_response(&root, &file, &metadata, WorkspaceFileEntryKind::Code)
            .expect("gb18030 code preview should decode");

    assert_eq!(response.encoding.as_deref(), Some("gb18030"));
    assert_eq!(response.content.as_deref(), Some(source));
    let _ = std::fs::remove_dir_all(root);
}

/// Verifies truncated UTF-8 previews keep valid text instead of falling back to GB18030.
/// 验证被截断的 UTF-8 预览会保留有效文本，而不是回退到 GB18030。
#[test]
fn workspace_file_decode_text_preview_keeps_truncated_utf8_prefix() {
    let mut bytes = "配置😀".as_bytes().to_vec();
    bytes.pop();

    let decoded = workspace_file_decode_text_preview(&bytes);

    assert_eq!(decoded.encoding, "utf-8");
    assert_eq!(decoded.content, "配置");
}

/// Creates one unique canonical temporary directory for workspace file tests.
/// 为工作区文件测试创建单个唯一规范化临时目录。
fn test_workspace_file_temp_dir(label: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "vulcan-workspace-files-{label}-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).expect("create workspace file temp dir");
    std::fs::canonicalize(directory).expect("canonical workspace file temp dir")
}
