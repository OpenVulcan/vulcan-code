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

    assert!(error.contains("escapes the workspace"));
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

/// Verifies parent projection uses the shared workspace boundary check.
/// 验证父路径投影会使用共享工作区边界检查。
#[test]
fn workspace_file_parent_inside_root_uses_workspace_boundary() {
    let root = test_workspace_file_temp_dir("parent-root");
    let outside = test_workspace_file_temp_dir("parent-outside");
    let nested = root.join("nested");
    let outside_child = outside.join("child");

    assert_eq!(workspace_file_parent_inside_root(&root, &root), None);
    assert_eq!(
        workspace_file_parent_inside_root(&root, &nested),
        Some(root.clone())
    );
    assert_eq!(
        workspace_file_parent_inside_root(&root, &outside_child),
        None
    );
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

/// Verifies a drive-root workspace contains its children while sibling drives and prefixes stay outside.
/// 验证盘符根工作区包含其子路径，而其他盘符与同前缀路径仍在外部。
#[cfg(windows)]
#[test]
fn workspace_path_inside_root_accepts_drive_root_children() {
    let drive_root = Path::new(r"D:\");
    assert!(workspace_path_is_inside_root(drive_root, drive_root));
    assert!(workspace_path_is_inside_root(
        Path::new(r"D:\src\main.rs"),
        drive_root
    ));
    assert!(!workspace_path_is_inside_root(
        Path::new(r"E:\src"),
        drive_root
    ));

    let project_root = Path::new(r"D:\work\app");
    assert!(workspace_path_is_inside_root(
        Path::new(r"d:\WORK\app\src"),
        project_root
    ));
    assert!(!workspace_path_is_inside_root(
        Path::new(r"D:\work\application"),
        project_root
    ));
}

/// Verifies relative path projection keeps root and nested child semantics.
/// 验证相对路径投影会保持根目录与嵌套子路径语义。
#[test]
fn workspace_file_relative_path_text_projects_root_and_children() {
    // Root path used by the relative projection examples.
    // 相对路径投影示例使用的根路径。
    let root = test_workspace_file_temp_dir("relative-path");
    // Nested child path that exercises multi-component slash joining.
    // 用于覆盖多级组件斜杠拼接的嵌套子路径。
    let nested = root.join("DATA").join("yycfg").join("中文配置.bin");

    assert_eq!(workspace_file_relative_path_text(&root, &root), "");
    assert_eq!(
        workspace_file_relative_path_text(&root, &nested),
        String::from("DATA/yycfg/中文配置.bin")
    );
    let _ = std::fs::remove_dir_all(root);
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
        workspace_file_kind_from_path(Path::new("diagram.svg")),
        WorkspaceFileEntryKind::Image
    );
    assert_eq!(
        workspace_file_kind_from_path(Path::new("Dockerfile")),
        WorkspaceFileEntryKind::Code
    );
    assert_eq!(
        workspace_file_kind_from_path(Path::new("AGENTS.md")),
        WorkspaceFileEntryKind::Code
    );
    for document in [
        "manual.pdf",
        "notes.docx",
        "budget.xlsx",
        "legacy.xls",
        "sheet.ods",
        "legacy.doc",
        "slides.ppt",
        "deck.pptx",
    ] {
        assert_eq!(
            workspace_file_kind_from_path(Path::new(document)),
            WorkspaceFileEntryKind::Document,
            "{document}"
        );
    }
    assert_eq!(
        workspace_file_kind_from_path(Path::new("capture.raw")),
        WorkspaceFileEntryKind::Other
    );
}

/// Verifies media preview filters are mapped by media kind.
/// 验证媒体预览过滤器会按媒体类别映射。
#[test]
fn workspace_file_media_kind_allowed_maps_media_filters() {
    let image_only = WorkspaceFileFilterOptions {
        allow_images: true,
        allow_videos: false,
        ..WorkspaceFileFilterOptions::default()
    };
    let video_only = WorkspaceFileFilterOptions {
        allow_images: false,
        allow_videos: true,
        ..WorkspaceFileFilterOptions::default()
    };

    assert!(workspace_file_media_kind_allowed(
        &WorkspaceFileEntryKind::Image,
        &image_only
    ));
    assert!(!workspace_file_media_kind_allowed(
        &WorkspaceFileEntryKind::Video,
        &image_only
    ));
    assert!(workspace_file_media_kind_allowed(
        &WorkspaceFileEntryKind::Video,
        &video_only
    ));
    assert!(!workspace_file_media_kind_allowed(
        &WorkspaceFileEntryKind::Image,
        &video_only
    ));
    assert!(!workspace_file_media_kind_allowed(
        &WorkspaceFileEntryKind::Code,
        &image_only
    ));
}

/// Verifies workspace file MIME types come from the same metadata as extension classification.
/// 验证工作区文件 MIME 类型与扩展名分类来自同一份元数据。
#[test]
fn workspace_file_mime_type_uses_extension_metadata() {
    assert_eq!(
        workspace_file_mime_type(Path::new("diagram.svg"), &WorkspaceFileEntryKind::Image),
        Some(String::from("image/svg+xml"))
    );
    assert_eq!(
        workspace_file_mime_type(Path::new("Cargo.toml"), &WorkspaceFileEntryKind::Code),
        Some(String::from("text/plain"))
    );
    assert_eq!(
        workspace_file_mime_type(Path::new("Dockerfile"), &WorkspaceFileEntryKind::Code),
        Some(String::from("text/plain"))
    );
    assert_eq!(
        workspace_file_mime_type(Path::new("capture.raw"), &WorkspaceFileEntryKind::Other),
        None
    );
}

/// Verifies workspace file summaries reuse path metadata for kind, extension, and MIME.
/// 验证工作区文件摘要会复用路径元数据生成类别、扩展名和 MIME。
#[test]
fn workspace_file_entry_summary_projects_path_metadata() {
    let root = test_workspace_file_temp_dir("path-metadata-summary");
    let diagram = root.join("diagram.svg");
    std::fs::write(&diagram, "<svg />\n").expect("write svg file");

    let summary =
        workspace_file_entry_summary(&root, &diagram, &WorkspaceFileFilterOptions::default())
            .expect("svg summary should be visible");

    assert_eq!(summary.kind, WorkspaceFileEntryKind::Image);
    assert_eq!(summary.extension, Some(String::from("svg")));
    assert_eq!(summary.mime_type, Some(String::from("image/svg+xml")));
    let _ = std::fs::remove_dir_all(root);
}

/// Verifies non-preview files are visible unless the dedicated filter is enabled.
/// 验证非预览文件默认可见，仅在专用过滤器启用后隐藏。
#[test]
fn workspace_file_entry_summary_filters_non_preview_files_only_when_requested() {
    let root = test_workspace_file_temp_dir("non-preview-filter");
    let data_directory = root.join("DATA").join("yycfg");
    std::fs::create_dir_all(&data_directory).expect("create data directory");
    let binary_file = data_directory.join("中文配置.bin");
    std::fs::write(&binary_file, "binary placeholder").expect("write binary file");

    let default_file_summary =
        workspace_file_entry_summary(&root, &binary_file, &WorkspaceFileFilterOptions::default())
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
        default_file_summary.relative_path,
        String::from("DATA/yycfg/中文配置.bin")
    );
    assert_eq!(
        default_directory_summary.kind,
        WorkspaceFileEntryKind::Directory
    );
    assert!(workspace_file_entry_summary(&root, &binary_file, &filters).is_none());
    assert!(workspace_file_entry_summary(&root, &root.join("DATA"), &filters).is_none());
    let _ = std::fs::remove_dir_all(root);
}

/// Verifies hidden descendants do not make preview-only directories visible.
/// 验证隐藏后代不会让仅预览过滤下的目录变为可见。
#[test]
fn workspace_file_entry_summary_ignores_hidden_preview_descendants() {
    let root = test_workspace_file_temp_dir("hidden-descendant-filter");
    let directory = root.join("visible-parent");
    std::fs::create_dir_all(&directory).expect("create visible parent directory");
    let hidden_file = directory.join(".hidden.rs");
    std::fs::write(&hidden_file, "fn hidden() {}\n").expect("write hidden descendant");
    let filters = WorkspaceFileFilterOptions {
        filter_non_preview_files: true,
        ..WorkspaceFileFilterOptions::default()
    };

    assert!(workspace_file_entry_summary(&root, &directory, &filters).is_none());
    let reveal_dot_filters = WorkspaceFileFilterOptions {
        ignore_dot_names: false,
        ..filters
    };
    assert!(workspace_file_entry_summary(&root, &directory, &reveal_dot_filters).is_some());
    let _ = std::fs::remove_dir_all(root);
}

/// Verifies the shared walker applies ignore files and optional depth consistently.
/// 验证共享遍历器会一致应用忽略文件与可选深度限制。
#[test]
fn workspace_file_walk_builder_applies_ignore_and_depth() {
    let root = test_workspace_file_temp_dir("walk-builder");
    std::fs::write(root.join(".ignore"), "ignored.txt\n").expect("write ignore file");
    std::fs::write(root.join("visible.rs"), "fn visible() {}\n").expect("write visible file");
    std::fs::write(root.join("ignored.txt"), "ignored\n").expect("write ignored file");
    let nested_directory = root.join("nested");
    std::fs::create_dir_all(&nested_directory).expect("create nested directory");
    std::fs::write(nested_directory.join("inner.rs"), "fn inner() {}\n").expect("write inner file");

    let walked_names =
        workspace_file_walk_builder(&root, &WorkspaceFileFilterOptions::default(), Some(1))
            .build()
            .filter_map(Result::ok)
            .filter(|entry| entry.depth() > 0)
            .filter_map(|entry| {
                entry
                    .path()
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();

    assert!(walked_names.contains(&String::from("visible.rs")));
    assert!(walked_names.contains(&String::from("nested")));
    assert!(!walked_names.contains(&String::from("ignored.txt")));
    assert!(!walked_names.contains(&String::from("inner.rs")));
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

/// Verifies media responses reuse shared file metadata without reading content.
/// 验证媒体响应复用共享文件元数据且不读取内容。
#[test]
fn workspace_file_media_response_uses_metadata_shell() {
    let root = test_workspace_file_temp_dir("media-read");
    let file = root.join("preview.png");
    std::fs::write(&file, [137, 80, 78, 71]).expect("write png header");
    let metadata = std::fs::metadata(&file).expect("png metadata");

    let response =
        workspace_file_media_response(&root, &file, &metadata, WorkspaceFileEntryKind::Image);

    assert_eq!(response.kind, WorkspaceFileEntryKind::Image);
    assert_eq!(response.mime_type.as_deref(), Some("image/png"));
    assert_eq!(response.size_bytes, 4);
    assert_eq!(response.content, None);
    assert_eq!(response.encoding, None);
    assert_eq!(response.line_count, None);
    assert!(!response.truncated);
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
