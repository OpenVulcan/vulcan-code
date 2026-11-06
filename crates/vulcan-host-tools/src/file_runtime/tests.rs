use super::{
    LIFECYCLE_EXCERPT_CHARS, MAX_EDIT_LINES_PER_ARRAY, MAX_INLINE_LIFECYCLE_BYTES, MAX_READ_LINES,
    MAX_READ_RESPONSE_BYTES, PreparedChange, ReplacementPlan, apply_patch_write_paths,
    build_patch_delete_change, commit_changes, commit_validated_changes, display_path,
    execute_apply_patch, execute_file, execute_file_edit, path_key, path_resolves_inside_workspace,
    persistent_change_set_from_host_result, read_edit_snapshot, read_raw_snapshot,
    render_snapshot_replacements, resolve_path, verify_preconditions, whole_file_modify_payload,
};
use crate::text_document::parse_document_lines;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use vulcan_change_set::{ChangeSetFileChangeKind, PersistentLifecycleContent};

/// Verifies native path resolution expands the established environment placeholder syntax.
/// 验证原生路径解析会展开既有的环境变量占位符语法。
#[test]
fn environment_path_placeholders_are_expanded_once() {
    let expected_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resolved = resolve_path(expected_root, "${env:CARGO_MANIFEST_DIR}/src")
        .expect("environment placeholder should resolve");

    assert_eq!(resolved, expected_root.join("src"));
}

/// Verifies absolute and parent-relative paths outside the workspace remain accessible.
/// 验证工作区之外的绝对路径与父级相对路径仍可访问。
#[test]
fn paths_outside_workspace_are_not_treated_as_workspace_paths() {
    let parent = tempdir().expect("create shared parent directory");
    let workspace = parent.path().join("workspace");
    let outside = parent.path().join("outside");
    fs::create_dir(&workspace).expect("create workspace directory");
    fs::create_dir(&outside).expect("create outside directory");

    let absolute = resolve_path(&workspace, outside.to_string_lossy().as_ref())
        .expect("outside absolute path should resolve");
    let parent_relative = resolve_path(&workspace, "../outside/example.txt")
        .expect("outside parent-relative path should resolve");

    assert_eq!(absolute, outside);
    assert_eq!(parent_relative, outside.join("example.txt"));
}

/// Verifies every native mutation path can operate outside the workspace through the shared resolver.
/// 验证每条原生变更路径都能通过共享解析器操作工作区外文件。
#[tokio::test]
async fn native_file_tools_operate_on_external_paths() {
    let parent = tempdir().expect("create shared parent directory");
    let workspace = parent.path().join("workspace");
    let outside = parent.path().join("outside");
    let outside_path = outside.join("sample.txt");
    fs::create_dir(&workspace).expect("create workspace directory");
    fs::create_dir(&outside).expect("create outside directory");

    execute_file(
        workspace.clone(),
        json!({
            "action": "write",
            "path": display_path(&outside_path),
            "content": "before\n"
        }),
    )
    .await
    .expect("external absolute write should succeed");

    let read = execute_file(
        workspace.clone(),
        json!({"action": "read", "path": "../outside/sample.txt"}),
    )
    .await
    .expect("external parent-relative read should succeed");
    assert!(
        read.model_output
            .as_str()
            .expect("Markdown read")
            .contains("\nL1: before\n")
    );

    execute_file_edit(
        workspace.clone(),
        json!({
            "path": display_path(&outside_path),
            "old_string": "before",
            "new_string": "after edit"
        }),
    )
    .await
    .expect("external exact edit should succeed");
    assert_eq!(
        fs::read_to_string(&outside_path).expect("read externally edited file"),
        "after edit\n"
    );

    execute_apply_patch(
            workspace.clone(),
            String::from(
                "*** Begin Patch\n*** Update File: ../outside/sample.txt\n-after edit\n+after patch\n*** End Patch\n",
            ),
        )
        .await
        .expect("external Apply Patch update should succeed");
    assert_eq!(
        fs::read_to_string(&outside_path).expect("read externally patched file"),
        "after patch\n"
    );

    execute_file(
        workspace,
        json!({"action": "delete", "path": display_path(&outside_path)}),
    )
    .await
    .expect("external absolute delete should succeed");
    assert!(!outside_path.exists());
}

/// Verifies a symbolic-link parent cannot redirect a request outside the workspace.
/// 验证符号链接父目录不能把请求重定向到工作区之外。
#[test]
fn symbolic_link_escape_is_rejected() {
    let workspace = tempdir().expect("create workspace directory");
    let outside = tempdir().expect("create outside directory");
    let link = workspace.path().join("outside-link");

    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &link).expect("create directory symlink");
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir(outside.path(), &link) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("create directory symlink: {error}");
    }

    let error = resolve_path(workspace.path(), "outside-link/new.txt")
        .expect_err("symbolic-link escape should fail");
    assert_eq!(error.code, "path_outside_workspace");
}

/// Verifies an in-workspace directory symlink shares the target resource identity.
/// 验证工作区内目录符号链接与目标目录共享同一资源身份。
#[test]
fn symbolic_link_aliases_share_one_resource_key() {
    let workspace = tempdir().expect("create workspace directory");
    let target_directory = workspace.path().join("target");
    let alias_directory = workspace.path().join("alias");
    fs::create_dir(&target_directory).expect("create target directory");
    fs::write(target_directory.join("sample.txt"), "content\n").expect("write target file");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_directory, &alias_directory)
        .expect("create directory symlink");
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir(&target_directory, &alias_directory) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("create directory symlink: {error}");
    }

    let direct_key = path_key(&target_directory.join("sample.txt"))
        .expect("direct target should have an identity");
    let alias_key = path_key(&alias_directory.join("sample.txt"))
        .expect("alias target should have an identity");
    assert_eq!(alias_key, direct_key);
}

/// Verifies malformed environment references fail with an explicit diagnostic.
/// 验证格式错误的环境变量引用会返回明确诊断。
#[test]
fn malformed_environment_path_placeholder_is_rejected() {
    let error = resolve_path(Path::new("D:/workspace"), "${env:CARGO_MANIFEST_DIR/src")
        .expect_err("malformed environment placeholder should fail");

    assert_eq!(error.code, "invalid_environment_variable_reference");
}

/// Verifies read, search, and stat expose bounded text metadata without writing.
/// 验证读取、搜索和统计会暴露有界文本元数据且不会写入文件。
#[tokio::test]
async fn read_search_and_stat_are_read_only() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "alpha\nbeta\n").expect("write fixture");
    let read = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "read",
            "targets": [{"path": "sample.txt"}]
        }),
    )
    .await
    .expect("read should succeed");
    assert_eq!(read.model_output["results"][0]["line_count"], json!(2));
    let search = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "search",
            "targets": [{"path": "."}],
            "query": "beta"
        }),
    )
    .await
    .expect("search should succeed");
    assert_eq!(
        search.model_output["results"][0]["matches"][0]["line"],
        json!(2)
    );
    let bounded_path = directory.path().join("bounded.txt");
    fs::write(&bounded_path, "beta\n").expect("write bounded fixture");
    let exact_limit = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "search",
            "targets": [{"path": "bounded.txt"}],
            "query": "beta",
            "limit": 1
        }),
    )
    .await
    .expect("bounded search should succeed");
    assert_eq!(
        exact_limit.model_output["results"][0]["truncated"],
        json!(false)
    );
    fs::write(&bounded_path, "beta\nbeta\n").expect("extend bounded fixture");
    let over_limit = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "search",
            "targets": [{"path": "bounded.txt"}],
            "query": "beta",
            "limit": 1
        }),
    )
    .await
    .expect("over-limit search should succeed");
    assert_eq!(
        over_limit.model_output["results"][0]["truncated"],
        json!(true)
    );
    assert_eq!(
        over_limit.model_output["results"][0]["matches"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let stat = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "stat",
            "targets": [{"path": "sample.txt"}]
        }),
    )
    .await
    .expect("stat should succeed");
    assert!(stat.model_output["results"][0]["version"].is_string());
    assert_eq!(
        fs::read_to_string(path).expect("read fixture after read-only calls"),
        "alpha\nbeta\n"
    );
}

/// Verifies one dry-run and one applied request can contain multiple non-overlapping replacements.
/// 验证一次 dry-run 和一次应用请求都可以包含多个不重叠替换。
#[tokio::test]
async fn edit_supports_arrays_anchors_and_atomic_change_set() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(
        &path,
        "fn a() {\n  return 1;\n}\nfn c() {\n  return 3;\n}\n",
    )
    .expect("write fixture");
    let request = json!({
        "operations": [
            {
                "id": "change-a",
                "operation": "replace",
                "path": "sample.txt",
                "anchors": ["fn a() {"],
                "old_lines": ["  return 1;"],
                "new_lines": ["  return 2;", "  return 20;"]
            },
            {
                "id": "change-c",
                "operation": "replace",
                "path": "sample.txt",
                "anchors": ["fn c() {"],
                "old_lines": ["  return 3;"],
                "new_lines": ["  return 4;"]
            }
        ]
    });
    let preview = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"dry_run": true, "operations": request["operations"]}),
    )
    .await
    .expect("dry-run should succeed");
    assert_eq!(
        preview.change_set.as_ref().expect("change set")["mode"],
        json!("preview")
    );
    assert_eq!(
        fs::read_to_string(&path).expect("read after dry-run"),
        "fn a() {\n  return 1;\n}\nfn c() {\n  return 3;\n}\n"
    );
    let applied = execute_file_edit(directory.path().to_path_buf(), request)
        .await
        .expect("apply should succeed");
    assert_eq!(
        applied.change_set.as_ref().expect("change set")["mode"],
        json!("applied")
    );
    assert_eq!(
        applied.change_set.as_ref().expect("change set")["files"][0]["resource_id"],
        json!("resource-1")
    );
    assert_eq!(
        fs::read_to_string(path).expect("read after apply"),
        "fn a() {\n  return 2;\n  return 20;\n}\nfn c() {\n  return 4;\n}\n"
    );
    assert_eq!(
        applied.change_set.as_ref().expect("change set")["files"][0]["hunks"][1]["insert"][0]["line"],
        json!(6)
    );
}

/// Verifies create preflight rejects existing targets, creates missing parents only on apply, and hunks show real changed lines.
/// 验证创建预检会拒绝已有目标或缺失父目录，并且 hunk 只展示真实变更行。
#[tokio::test]
async fn edit_preflight_and_hunk_diff_are_explicit() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "line one\nline two\nline three\n").expect("write fixture");

    let existing_create = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "dry_run": true,
            "operations": [{
                "id": "existing",
                "operation": "create",
                "path": "sample.txt",
                "new_lines": ["replacement"]
            }]
        }),
    )
    .await
    .expect_err("creating an existing target must fail during preview");
    assert_eq!(existing_create.code, "path_already_exists");

    let missing_parent_request = json!({
        "operations": [{
            "id": "missing-parent",
            "operation": "create",
            "path": "missing/child.txt",
            "new_lines": ["content"]
        }]
    });
    let missing_parent_preview = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"dry_run": true, "operations": missing_parent_request["operations"]}),
    )
    .await
    .expect("missing parents should be previewable without writes");
    assert_eq!(
        missing_parent_preview
            .change_set
            .expect("preview change set")["mode"],
        json!("preview")
    );
    assert!(!directory.path().join("missing").exists());

    execute_file_edit(directory.path().to_path_buf(), missing_parent_request)
        .await
        .expect("apply should create missing parents");
    assert_eq!(
        fs::read_to_string(directory.path().join("missing/child.txt")).expect("read created file"),
        "content\n"
    );

    let result = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "middle-line",
                "operation": "replace",
                "path": "sample.txt",
                "old_lines": ["line one", "line two", "line three"],
                "new_lines": ["line one", "changed", "line three"]
            }]
        }),
    )
    .await
    .expect("replace should succeed");
    let hunk = &result.change_set.expect("change set")["files"][0]["hunks"][0];
    assert_eq!(hunk["before"], json!("line one"));
    assert_eq!(hunk["delete"].as_array().map(Vec::len), Some(1));
    assert_eq!(hunk["delete"][0]["content"], json!("line two"));
    assert_eq!(hunk["insert"].as_array().map(Vec::len), Some(1));
    assert_eq!(hunk["insert"][0]["content"], json!("changed"));
    assert_eq!(hunk["after"], json!("line three"));
}

/// Verifies adding logical lines to a file without an existing newline defaults to LF.
/// 验证向原本没有换行符的文件追加逻辑行时默认使用 LF。
#[tokio::test]
async fn edit_without_original_line_ending_defaults_to_lf() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "one").expect("write fixture");

    execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "append-line",
                "operation": "replace",
                "path": "sample.txt",
                "old_lines": ["one"],
                "new_lines": ["one", "two"]
            }]
        }),
    )
    .await
    .expect("replace should succeed");

    assert_eq!(fs::read(&path).expect("read result"), b"one\ntwo");
}

/// Verifies a pure move is accepted even when file bytes remain unchanged.
/// 验证纯移动即使文件字节不变也会被正确接受。
#[tokio::test]
async fn move_file_produces_canonical_rename() {
    let directory = tempdir().expect("create temporary directory");
    let source = directory.path().join("source.txt");
    let destination = directory.path().join("destination.txt");
    fs::write(&source, "unchanged\n").expect("write fixture");
    let stat = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "stat", "targets": [{"path": "source.txt"}]}),
    )
    .await
    .expect("stat should succeed");
    let version = stat.model_output["results"][0]["version"]
        .as_str()
        .expect("version");

    let result = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "move-source",
                "operation": "move_file",
                "path": "source.txt",
                "destination": "destination.txt",
                "expected_version": version
            }]
        }),
    )
    .await
    .expect("move should succeed");
    assert!(!source.exists());
    assert_eq!(
        fs::read_to_string(&destination).expect("read destination"),
        "unchanged\n"
    );
    let file = &result.change_set.expect("change set")["files"][0];
    assert_eq!(file["change"], json!("rename"));
    assert!(
        file["old_path"]
            .as_str()
            .unwrap_or_default()
            .ends_with("/source.txt")
    );
    assert!(
        file["new_path"]
            .as_str()
            .unwrap_or_default()
            .ends_with("/destination.txt")
    );
}

/// Verifies a move cannot silently reorder later replacements for the same resource.
/// 验证移动操作不能静默重排同一资源后续的替换操作。
#[tokio::test]
async fn move_must_be_last_for_one_source_resource() {
    let directory = tempdir().expect("create temporary directory");
    let source = directory.path().join("source.txt");
    fs::write(&source, "before\n").expect("write fixture");
    let stat = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "stat", "targets": [{"path": "source.txt"}]}),
    )
    .await
    .expect("stat should succeed");
    let version = stat.model_output["results"][0]["version"]
        .as_str()
        .expect("version");

    let error = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [
                {
                    "id": "move-first",
                    "operation": "move_file",
                    "path": "source.txt",
                    "destination": "destination.txt",
                    "expected_version": version
                },
                {
                    "id": "replace-after-move",
                    "operation": "replace",
                    "path": "source.txt",
                    "old_lines": ["before"],
                    "new_lines": ["after"]
                }
            ]
        }),
    )
    .await
    .expect_err("move-before-replace must be rejected");

    assert_eq!(error.code, "invalid_arguments");
    assert!(source.exists());
    assert!(!directory.path().join("destination.txt").exists());
}

/// Verifies ambiguous old blocks and stale version tokens fail before any write.
/// 验证重复旧块和陈旧版本令牌会在写入前失败。
#[tokio::test]
async fn edit_rejects_ambiguous_and_stale_requests() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "same\nsame\n").expect("write fixture");
    let ambiguous = execute_file_edit(
            directory.path().to_path_buf(),
            json!({"operations": [{"id": "ambiguous", "operation": "replace", "path": "sample.txt", "old_lines": ["same"], "new_lines": ["new"]}]}),
        )
        .await;
    let ambiguous_error = ambiguous.expect_err("ambiguous target should fail");
    assert_eq!(ambiguous_error.code, "target_not_unique");
    assert_eq!(ambiguous_error.details["operation_id"], json!("ambiguous"));
    assert_eq!(ambiguous_error.details["operation_index"], json!(0));
    assert!(
        ambiguous_error.details["path"]
            .as_str()
            .unwrap_or_default()
            .ends_with("sample.txt")
    );
    assert_eq!(
        ambiguous_error.details["candidate_ranges"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(ambiguous_error.details["candidate_count"], json!(2));
    assert_eq!(ambiguous_error.details["commit_scope"], json!("none"));
    assert_eq!(ambiguous_error.details["requires_reread"], json!(true));
    assert!(ambiguous_error.details["recovery_hint"].is_string());
    let oversized_old_block = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "too-long",
                "operation": "replace",
                "path": "sample.txt",
                "old_lines": ["same", "same", "missing"],
                "new_lines": ["new"]
            }]
        }),
    )
    .await
    .expect_err("an old block longer than the file should fail safely");
    assert_eq!(oversized_old_block.code, "expected_lines_not_found");
    let anchor_mismatch = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "anchor-mismatch",
                "operation": "replace",
                "path": "sample.txt",
                "anchors": ["missing anchor"],
                "old_lines": ["same"],
                "new_lines": ["new"]
            }]
        }),
    )
    .await
    .expect_err("missing anchor chain should fail");
    assert_eq!(anchor_mismatch.code, "anchor_chain_not_found");
    let read = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "stat", "targets": [{"path": "sample.txt"}]}),
    )
    .await
    .expect("stat should succeed");
    let version = read.model_output["results"][0]["version"]
        .as_str()
        .expect("version");
    fs::write(&path, "changed\n").expect("mutate fixture");
    let stale = execute_file_edit(
            directory.path().to_path_buf(),
            json!({"operations": [{"id": "delete", "operation": "delete_file", "path": "sample.txt", "expected_version": version}]}),
        )
        .await;
    assert!(stale.is_err());
    assert!(path.exists());
}

/// Verifies empty-file insertion rejects anchors that cannot provide a position.
/// 验证空文件插入会拒绝无法提供定位的 anchors。
#[tokio::test]
async fn empty_file_insertion_rejects_anchors() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("empty.txt");
    fs::write(&path, "").expect("write empty fixture");

    let error = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "invalid-anchor",
                "operation": "replace",
                "path": "empty.txt",
                "anchors": ["unused"],
                "old_lines": [],
                "new_lines": ["content"]
            }]
        }),
    )
    .await
    .expect_err("empty-file insertion should not silently ignore anchors");

    assert_eq!(error.code, "invalid_arguments");
}

/// Verifies strict action fields, duplicate ids, and relaxed match reporting.
/// 验证动作字段严格校验、重复 ID 拒绝以及宽松匹配结果的准确报告。
#[tokio::test]
async fn edit_and_read_reject_silent_contract_drift() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "value  \n").expect("write fixture");

    let invalid_read = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "read",
            "targets": [{"path": "sample.txt"}],
            "query": "value"
        }),
    )
    .await;
    assert!(invalid_read.is_err());

    let duplicate_ids = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "operations": [
                    {"id": "same", "operation": "replace", "path": "sample.txt", "old_lines": ["value  "], "new_lines": ["one"]},
                    {"id": "same", "operation": "replace", "path": "sample.txt", "old_lines": ["value  "], "new_lines": ["two"]}
                ]
            }),
        )
        .await;
    assert!(duplicate_ids.is_err());
    assert_eq!(
        fs::read_to_string(&path).expect("read fixture"),
        "value  \n"
    );

    let relaxed = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "relaxed",
                "operation": "replace",
                "path": "sample.txt",
                "old_lines": ["value"],
                "new_lines": ["changed"],
                "match_mode": "ignore_trailing_whitespace"
            }]
        }),
    )
    .await
    .expect("relaxed edit should succeed");
    assert_eq!(
        relaxed.change_set.as_ref().expect("change set")["extensions"]["vulcan.file_edit.v1"]["operation_results"]
            [0]["match_mode"],
        json!("ignore_trailing_whitespace")
    );
}

/// Verifies malformed optional types and hard output limits become explicit diagnostics.
/// 验证错误的可选字段类型和硬输出上限都会变成明确的诊断结果。
#[tokio::test]
async fn read_and_edit_limits_are_explicit() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    let large_text = (0..=MAX_READ_LINES)
        .map(|index| format!("line-{index}\n"))
        .collect::<String>();
    fs::write(&path, large_text).expect("write large fixture");

    let invalid_type = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "read",
            "targets": [{"path": "sample.txt", "numbered": "yes"}]
        }),
    )
    .await
    .expect("per-target validation should be returned as a result row");
    assert_eq!(
        invalid_type.model_output["results"][0]["error"]["code"],
        json!("invalid_arguments")
    );

    let oversized_read = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "read", "targets": [{"path": "sample.txt"}]}),
    )
    .await
    .expect("per-target output should be returned as a bounded result row");
    assert_eq!(
        oversized_read.model_output["results"][0]["truncated"],
        json!(true)
    );
    assert_eq!(
        oversized_read.model_output["results"][0]["lines"]
            .as_array()
            .map(Vec::len),
        Some(MAX_READ_LINES)
    );

    let too_many_lines = (0..=MAX_EDIT_LINES_PER_ARRAY)
        .map(|index| Value::String(format!("line-{index}")))
        .collect::<Vec<_>>();
    let oversized_edit = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "operations": [{
                "id": "too-many-lines",
                "operation": "create",
                "path": "new.txt",
                "new_lines": too_many_lines
            }]
        }),
    )
    .await
    .expect_err("line-array limit should be rejected");
    assert_eq!(oversized_edit.code, "output_limit_exceeded");
}

/// Verifies numbered Markdown retains the actual range and a single copy of source content.
/// 验证带行号的 Markdown 保留真实范围与唯一一份源文件正文。
#[tokio::test]
async fn native_read_returns_code_content_and_real_range() {
    // Cover normalized separators, blank lines, fences, Unicode, and an unterminated final line.
    // 覆盖归一化分隔符、空行、围栏、Unicode 与无末尾换行的最后一行。
    let directory = tempdir().expect("create temporary directory");
    fs::write(
        directory.path().join("sample.rs"),
        "one\r\ntwo\r\n\r\n```\r\n你好",
    )
    .expect("write fixture");
    let response = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "read", "path": "sample.rs", "start_line": 2, "end_line": 5
        }),
    )
    .await
    .expect("native read should succeed");
    // Locate the source using metadata, never an assumed Markdown header length.
    // 使用元数据定位正文，绝不假设 Markdown 标题长度。
    let markdown = response.model_output.as_str().expect("Markdown result");
    let metadata = response.read_metadata.as_ref().expect("read coordinates");
    let offset = metadata["render"]["output_start_line"]
        .as_u64()
        .expect("body line") as usize
        - 1;
    let count = metadata["range"]["line_count"]
        .as_u64()
        .expect("line count") as usize;
    assert_eq!(metadata["range"]["source_start_line"], json!(2));
    assert_eq!(metadata["line_ending"], json!("crlf"));
    assert_eq!(metadata["final_newline"], json!(false));
    assert_eq!(metadata["language"], json!("rust"));
    assert_eq!(
        markdown
            .lines()
            .skip(offset)
            .take(count)
            .collect::<Vec<_>>(),
        vec!["L2: two", "L3: ", "L4: ```", "L5: 你好"]
    );
    assert!(markdown.contains("\n````rust\n"));
    assert!(markdown.ends_with("\n````\n"));
    assert_eq!(metadata["schema"], json!("vulcan.file_read.v3"));
    assert!(metadata["range"].get("output_start_line").is_none());
    assert_eq!(markdown.matches("你好").count(), 1);
    assert!(!metadata.to_string().contains("你好"));
    for removed in ["content", "lines", "segments"] {
        assert!(
            metadata.get(removed).is_none(),
            "obsolete body field: {removed}"
        );
    }

    // Reads past EOF have an explicit zero-length mapping and never invent a source line.
    // 超出 EOF 的读取有明确零长度映射，绝不伪造源文件行。
    let beyond_end = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "read", "path": "sample.rs", "start_line": 100
        }),
    )
    .await
    .expect("empty range");
    let empty_metadata = beyond_end.read_metadata.expect("empty read coordinates");
    assert_eq!(empty_metadata["range"]["source_start_line"], json!(100));
    assert_eq!(empty_metadata["requested_range"]["start_line"], json!(100));
    assert_eq!(empty_metadata["read_status"], json!("range_beyond_eof"));
    assert_eq!(empty_metadata["range"]["line_count"], json!(0));
    assert_eq!(empty_metadata["next_start_line"], Value::Null);
    assert!(
        beyond_end
            .model_output
            .as_str()
            .unwrap()
            .contains("[range_beyond_eof]")
    );
    assert!(
        !beyond_end
            .model_output
            .as_str()
            .unwrap()
            .contains("[empty_file]")
    );
}

/// Verifies empty and mixed-ending documents project exactly their logical source lines.
/// 验证空文件与混合换行文件精确投影其逻辑源文件行。
#[tokio::test]
async fn native_read_maps_empty_and_mixed_ending_documents() {
    // Mixed physical endings must not create extra presentation lines or discard blank lines.
    // 混合物理换行不得产生额外展示行或丢弃空行。
    let directory = tempdir().expect("temporary workspace");
    for (source, expected) in [
        ("", vec![]),
        ("\n", vec![""]),
        ("a\n\rb\rc\nd\r\n", vec!["a", "b", "c", "d"]),
    ] {
        fs::write(directory.path().join("sample.txt"), source).expect("write fixture");
        let response = execute_file(
            directory.path().to_path_buf(),
            json!({"action": "read", "path": "sample.txt"}),
        )
        .await
        .expect("read fixture");
        let metadata = response.read_metadata.expect("mapping metadata");
        let start = metadata["render"]["output_start_line"]
            .as_u64()
            .expect("start") as usize;
        let count = metadata["range"]["line_count"].as_u64().expect("count") as usize;
        let body = response.model_output.as_str().expect("Markdown body");
        assert_eq!(
            body.split('\n')
                .skip(start - 1)
                .take(count)
                .collect::<Vec<_>>(),
            expected
                .iter()
                .enumerate()
                .map(|(index, line)| format!("L{}: {line}", index + 1))
                .collect::<Vec<_>>()
        );
        assert_eq!(count, expected.len());
        assert_eq!(metadata["truncated"], json!(false));
        assert_eq!(body.contains("[empty_file]"), source.is_empty());
    }
}

/// Verifies the existing line cap still provides a usable continuation in Markdown.
/// 验证既有行数上限仍会在 Markdown 中提供可用续读位置。
#[tokio::test]
async fn native_read_preserves_line_limit_and_continuation() {
    // Derive the fixture size from the production limit so future limit changes remain valid.
    // 从生产上限派生样本大小，确保未来调整上限后测试仍然有效。
    let directory = tempdir().expect("temporary workspace");
    fs::write(
        directory.path().join("many.txt"),
        "x\n".repeat(MAX_READ_LINES + 1),
    )
    .expect("write fixture");
    let response = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "read", "path": "many.txt"}),
    )
    .await
    .expect("bounded read");
    let metadata = response.read_metadata.expect("mapping metadata");
    assert_eq!(
        metadata["range"]["line_count"],
        json!(super::DEFAULT_READ_LINES)
    );
    assert_eq!(
        metadata["next_start_line"],
        json!(super::DEFAULT_READ_LINES + 1)
    );
    assert!(
        response
            .model_output
            .as_str()
            .expect("Markdown body")
            .contains(&format!("start_line={}", super::DEFAULT_READ_LINES + 1))
    );
}

/// Verifies non-recursive directory listing returns child directories as discoverable entries.
/// 验证非递归目录列举会把子目录作为可发现条目返回。
#[tokio::test]
async fn native_list_returns_child_directories() {
    let directory = tempdir().expect("create temporary directory");
    fs::create_dir(directory.path().join("src")).expect("create child directory");
    fs::write(directory.path().join("README.md"), "read me\n").expect("write child file");

    let response = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "list", "path": ".", "recursive": false}),
    )
    .await
    .expect("native list should succeed");
    let entries = response.model_output["entries"]
        .as_array()
        .expect("list entries should be an array");

    assert!(entries.iter().any(|entry| {
        entry["kind"] == "directory"
            && entry["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("/src"))
    }));
    assert!(entries.iter().any(|entry| entry["kind"] == "file"));
}

/// Verifies the single body and its metadata together remain inside the serialized response budget.
/// 验证唯一正文与元数据合计保持在序列化响应预算以内。
#[tokio::test]
async fn native_read_truncates_by_complete_serialized_payload_size() {
    // Escaped characters and Unicode must count toward persisted size, not only source bytes.
    // 转义字符与 Unicode 必须计入持久化大小，不能只统计源文件字节。
    let directory = tempdir().expect("create temporary directory");
    let line = format!("{}\n", "中\\\"".repeat(256));
    fs::write(directory.path().join("large.txt"), line.repeat(3_000)).expect("write fixture");
    let response = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "read", "path": "large.txt", "end_line": 3000}),
    )
    .await
    .expect("bounded native read");
    let encoded = serde_json::to_vec(&response).expect("serialize result");
    let metadata = response.read_metadata.as_ref().expect("read coordinates");
    let count = metadata["range"]["line_count"]
        .as_u64()
        .expect("returned lines");
    assert!(encoded.len() <= MAX_READ_RESPONSE_BYTES);
    assert_eq!(metadata["truncated"], json!(true));
    assert!(count > 0 && count < 3_000);
    assert_eq!(metadata["next_start_line"], json!(count + 1));
    assert!(
        response
            .model_output
            .as_str()
            .expect("Markdown")
            .contains(&format!("Continue with start_line={}", count + 1))
    );
    // One more complete line must exceed the budget; otherwise truncation wastes capacity.
    // 再增加一个完整行必须超出预算，否则截断浪费了容量。
    assert!(
        encoded.len() + serde_json::to_string(&line).expect("encoded line").len() - 2
            > MAX_READ_RESPONSE_BYTES
    );
}

/// Verifies one oversized logical line fails explicitly instead of returning an empty self-looping range.
/// 验证单条超大逻辑行会显式失败，而不是返回导致循环读取的空范围。
#[tokio::test]
async fn native_read_rejects_one_line_larger_than_response_limit() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(
        directory.path().join("one-line.txt"),
        "x".repeat(MAX_READ_RESPONSE_BYTES),
    )
    .expect("write oversized one-line fixture");

    let error = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "read", "path": "one-line.txt"}),
    )
    .await
    .expect_err("one oversized logical line should fail explicitly");

    assert_eq!(error.code, "output_limit_exceeded");
    assert!(error.message.contains("Logical line 1"));
}

/// Verifies exact replacement preserves an explicitly confirmed GBK encoding and untouched LFCR endings.
/// 验证精确替换保留显式确认的 GBK 编码与未触及的 LFCR 换行。
#[tokio::test]
async fn exact_edit_preserves_legacy_encoding_and_lfcr() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("legacy.txt");
    let source = "标题\n\r旧值\n\r尾部";
    let (encoded, _, had_errors) = encoding_rs::GBK.encode(source);
    assert!(!had_errors);
    fs::write(&path, encoded.as_ref()).expect("write GBK fixture");
    let response = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "path": "legacy.txt",
            "old_string": "旧值",
            "new_string": "新值",
            "encoding": "gbk"
        }),
    )
    .await
    .expect("exact edit should succeed");
    assert_eq!(response.model_output["encoding"], json!("GBK"));
    let bytes = fs::read(&path).expect("read edited GBK fixture");
    let decoded = encoding_rs::GBK
        .decode_without_bom_handling_and_without_replacement(&bytes)
        .expect("edited GBK should decode");
    assert_eq!(decoded, "标题\n\r新值\n\r尾部");
}

/// Verifies exact edit rejects zero and ambiguous matches and supports explicit replace-all.
/// 验证精确编辑拒绝零命中与歧义命中，并支持显式全部替换。
#[tokio::test]
async fn exact_edit_enforces_match_cardinality_and_replace_all() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "值🙂\n值🙂\n").expect("write Unicode fixture");

    let missing = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path": "sample.txt", "old_string": "不存在", "new_string": "新"}),
    )
    .await
    .expect_err("zero match should fail");
    assert_eq!(missing.code, "match_not_found");

    let ambiguous = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path": "sample.txt", "old_string": "值🙂", "new_string": "新"}),
    )
    .await
    .expect_err("multiple matches should fail by default");
    assert_eq!(ambiguous.code, "ambiguous_match");
    assert_eq!(ambiguous.details["match_count"], json!(2));

    let replaced = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "path": "sample.txt",
            "old_string": "值🙂",
            "new_string": "新值",
            "replace_all": true
        }),
    )
    .await
    .expect("replace_all should succeed");
    assert_eq!(replaced.model_output["match_count"], json!(2));
    assert_eq!(
        fs::read_to_string(path).expect("read replaced fixture"),
        "新值\n新值\n"
    );
}

/// Verifies exact-edit hunks contain actual complete lines rather than raw substring arguments.
/// 验证精确编辑 hunk 包含实际完整行，而不是原始子串参数。
#[tokio::test]
async fn exact_edit_change_set_uses_actual_complete_lines() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.rs");
    fs::write(&path, "let value = old();\nnext();\n").expect("write inline edit fixture");

    let response = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path": "sample.rs", "old_string": "old", "new_string": "new"}),
    )
    .await
    .expect("inline exact edit should succeed");
    let change_set = response.change_set.expect("inline edit change set");
    let hunk = &change_set["files"][0]["hunks"][0];
    assert_eq!(
        hunk["delete"],
        json!([{"line": 1, "content": "let value = old();"}])
    );
    assert_eq!(
        hunk["insert"],
        json!([{"line": 1, "content": "let value = new();"}])
    );
    assert_eq!(hunk["after"], json!("next();"));
}

/// Verifies replace-all newline edits merge overlapping line projections into one true hunk.
/// 验证全部替换换行时会把重叠行投影合并为一个真实 hunk。
#[tokio::test]
async fn exact_edit_change_set_merges_overlapping_newline_matches() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "a\nb\nc").expect("write newline edit fixture");

    let response = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "path": "sample.txt",
            "old_string": "\n",
            "new_string": "",
            "replace_all": true
        }),
    )
    .await
    .expect("newline replace-all should succeed");
    let change_set = response.change_set.expect("newline edit change set");
    let hunks = change_set["files"][0]["hunks"]
        .as_array()
        .expect("hunks should be an array");
    assert_eq!(hunks.len(), 1);
    assert_eq!(
        hunks[0]["delete"],
        json!([
            {"line": 1, "content": "a"},
            {"line": 2, "content": "b"},
            {"line": 3, "content": "c"}
        ])
    );
    assert_eq!(hunks[0]["insert"], json!([{"line": 1, "content": "abc"}]));
    assert_eq!(
        fs::read_to_string(path).expect("read newline edit result"),
        "abc"
    );
}

/// Removing text inside lines does not shift the reported coordinates of later matches.
/// 删除行内文本不会使后续匹配的报告坐标发生偏移。
#[tokio::test]
async fn exact_edit_in_line_removals_keep_their_line_numbers() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(
        directory.path().join("sample.txt"),
        "x1\nkeep\nx2\nkeep\nx3\n",
    )
    .expect("write in-line edit fixture");
    let response = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path": "sample.txt", "old_string": "x", "new_string": "", "replace_all": true}),
    )
    .await
    .expect("in-line replace-all should succeed");
    let change_set = response.change_set.expect("in-line edit change set");
    let hunks = change_set["files"][0]["hunks"]
        .as_array()
        .expect("hunks should be an array");
    let lines = |side: &str| {
        hunks
            .iter()
            .flat_map(|hunk| hunk[side].as_array().expect("line array").clone())
            .map(|line| (line["line"].as_u64().unwrap(), line["content"].clone()))
            .collect::<Vec<_>>()
    };
    // Each reported line must pair with the same line number on the other side.
    // 每条报告行都必须与另一侧相同行号的行配对。
    let expected = lines("delete")
        .into_iter()
        .map(|(line, content)| {
            let text = content.as_str().unwrap().replace('x', "");
            (line, json!(text))
        })
        .collect::<Vec<_>>();
    assert!(expected.iter().any(|(line, _)| *line == 5));
    assert_eq!(lines("insert"), expected);
}

/// Verifies an unrepresentable legacy edit leaves the original bytes untouched.
/// 验证传统编码无法表示新字符时原始字节保持不变。
#[tokio::test]
async fn exact_edit_unrepresentable_legacy_text_writes_nothing() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("legacy.txt");
    let (encoded, _, had_errors) = encoding_rs::GBK.encode("旧值\n");
    assert!(!had_errors);
    let original = encoded.into_owned();
    fs::write(&path, &original).expect("write GBK fixture");

    let error = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path": "legacy.txt", "old_string": "旧值", "new_string": "🙂", "encoding": "gbk"}),
    )
    .await
    .expect_err("GBK cannot represent emoji");
    assert_eq!(error.code, "encoding_unrepresentable");
    assert_eq!(fs::read(path).expect("read unchanged GBK bytes"), original);
}

/// Returns a fully persistable mixed patch with consistent file, resource, and operation identifiers.
/// 返回可完整持久化的混合补丁，文件、资源与操作标识保持一致。
#[tokio::test]
async fn apply_patch_mixed_lifecycles_have_unique_persistent_resources() {
    // Keep every real mutation within an isolated temporary workspace.
    // 将所有真实变更限制在隔离的临时工作区内。
    let directory = tempdir().expect("create temporary directory");
    for name in ["update.txt", "delete.txt", "move.txt", "move-edit.txt"] {
        fs::write(directory.path().join(name), "old\n").expect("write fixture");
    }
    // Include both raw lifecycle paths after a normal update and creation.
    // 在常规修改和创建之后覆盖两条原始生命周期路径。
    let response = execute_apply_patch(
            directory.path().to_path_buf(),
            String::from("*** Begin Patch\n*** Update File: update.txt\n@@\n-old\n+new\n*** Add File: add.txt\n+created\n*** Delete File: delete.txt\n*** Update File: move.txt\n*** Move to: moved.txt\n*** Update File: move-edit.txt\n*** Move to: moved-edited.txt\n@@\n-old\n+edited\n*** End Patch\n"),
        ).await.expect("mixed patch should succeed");
    // Validate the complete public envelope, including format resources, as persistence does.
    // 像持久化链路一样校验完整公共信封，包括格式资源表。
    let payload = response.change_set.expect("change set");
    assert!(
        persistent_change_set_from_host_result(&json!({
            "kind": "change_set", "payload": payload,
        }))
        .expect("mixed patch must pass durability validation")
        .is_some()
    );
    // Resolve each resource by identity rather than by an evolving array position.
    // 按标识定位每个资源，不依赖会演进的数组位置。
    let extension = &payload["extensions"]["vulcan.file_tools.v2"];
    for file in payload["files"].as_array().expect("files") {
        // Match the same identity in the extension and all related operations.
        // 在扩展及所有相关操作中匹配同一个标识。
        let resource = extension["resources"]
            .as_array()
            .expect("resources")
            .iter()
            .find(|resource| resource["resource_id"] == file["resource_id"])
            .expect("file resource must exist");
        assert_eq!(resource["resolved_path"], file["path"]);
        assert!(
            extension["operation_results"]
                .as_array()
                .expect("operations")
                .iter()
                .any(|operation| operation["resource_id"] == file["resource_id"])
        );
    }
    assert!(!directory.path().join("delete.txt").exists());
    assert!(!directory.path().join("move.txt").exists());
    assert!(!directory.path().join("move-edit.txt").exists());
    for (name, expected) in [
        ("update.txt", "new\n"),
        ("add.txt", "created\n"),
        ("moved.txt", "old\n"),
        ("moved-edited.txt", "edited\n"),
    ] {
        assert_eq!(
            fs::read_to_string(directory.path().join(name)).expect("read result"),
            expected
        );
    }
}

/// Verifies Apply Patch supports anchored update plus add in one atomic change set.
/// 验证 Apply Patch 在一个原子变更集中支持锚定更新与新增。
#[tokio::test]
async fn apply_patch_updates_and_adds_atomically() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(
        directory.path().join("sample.rs"),
        "fn greet() {\r\n    println!(\"old\");\r\n}\r\n",
    )
    .expect("write source fixture");
    let response = execute_apply_patch(
            directory.path().to_path_buf(),
            String::from(
                "*** Begin Patch\n*** Update File: sample.rs\n@@ fn greet() {\n-    println!(\"old\");\n+    println!(\"new\");\n*** Add File: added.txt\n+created\n*** End Patch\n",
            ),
        )
        .await
        .expect("patch should succeed");
    assert_eq!(response.model_output["file_count"], json!(2));
    assert_eq!(
        fs::read_to_string(directory.path().join("sample.rs")).expect("read update"),
        "fn greet() {\r\n    println!(\"new\");\r\n}\r\n"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("added.txt")).expect("read add"),
        "created\n"
    );
}

/// Verifies Apply Patch preserves mixed endings on shifted unchanged context lines.
/// 验证 Apply Patch 会在上下文行发生位移时保留各自的混合行尾。
#[tokio::test]
async fn apply_patch_preserves_mixed_context_endings_after_insertion() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("mixed.txt");
    fs::write(&path, "a\r\nb\n\rc\rd").expect("write mixed-ending fixture");

    execute_apply_patch(
            directory.path().to_path_buf(),
            String::from(
                "*** Begin Patch\n*** Update File: mixed.txt\n@@\n+inserted\n a\n b\n c\n*** End Patch\n",
            ),
        )
        .await
        .expect("mixed-ending patch should succeed");

    assert_eq!(
        fs::read_to_string(path).expect("read patched mixed-ending file"),
        "inserted\r\na\r\nb\n\rc\rd"
    );
}

/// Verifies context-only hunks are located but never emitted as empty ChangeSet hunks.
/// 验证仅含上下文的 hunk 会被定位校验，但绝不会作为空 ChangeSet hunk 输出。
#[tokio::test]
async fn apply_patch_context_only_hunks_do_not_break_the_change_set() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "alpha\nbeta\ngamma\n").expect("write fixture");

    // A context-only hunk alongside a real hunk must apply the real change only.
    // 仅含上下文的 hunk 与真实 hunk 并存时，只应用真实修改。
    let response = execute_apply_patch(
        directory.path().to_path_buf(),
        String::from(
            "*** Begin Patch\n*** Update File: sample.txt\n@@\n alpha\n@@\n-gamma\n+delta\n*** End Patch\n",
        ),
    )
    .await
    .expect("context-only hunk must not invalidate the ChangeSet");
    assert_eq!(
        fs::read_to_string(&path).expect("read patched file"),
        "alpha\nbeta\ndelta\n"
    );
    let hunks = response
        .change_set
        .as_ref()
        .and_then(|change_set| change_set.pointer("/files/0/hunks"))
        .and_then(Value::as_array)
        .expect("modify record carries hunks");
    assert_eq!(hunks.len(), 1);

    // A patch made only of context lines is a reported no-op, not an internal defect.
    // 只由上下文行组成的 patch 是明确的无变更，而不是内部缺陷。
    let error = execute_apply_patch(
        directory.path().to_path_buf(),
        String::from("*** Begin Patch\n*** Update File: sample.txt\n@@\n beta\n*** End Patch\n"),
    )
    .await
    .expect_err("context-only patch changes nothing");
    assert_eq!(error.code, "no_change");

    // A move whose only hunk is context is a pure rename with its body as lifecycle content.
    // 唯一 hunk 只含上下文的移动是纯重命名，以生命周期正文携带内容。
    let moved = execute_apply_patch(
        directory.path().to_path_buf(),
        String::from(
            "*** Begin Patch\n*** Update File: sample.txt\n*** Move to: moved.txt\n@@\n beta\n*** End Patch\n",
        ),
    )
    .await
    .expect("a context-only move is a valid rename");
    let persistent = persistent_change_set_from_host_result(&json!({
        "kind": "change_set",
        "payload": moved.change_set.expect("move returns a ChangeSet"),
    }))
    .expect("context-only move must pass durability validation")
    .expect("move must return a ChangeSet");
    assert!(persistent.files[0].hunks.is_empty());
    assert!(persistent.files[0].lifecycle_content.is_some());

    // Context longer than the whole file is reported as missing context, not an internal failure.
    // 比整个文件更长的上下文报告为找不到上下文，而不是内部失败。
    let error = execute_apply_patch(
        directory.path().to_path_buf(),
        String::from(
            "*** Begin Patch\n*** Update File: moved.txt\n@@\n alpha\n beta\n delta\n-omega\n+psi\n*** End Patch\n",
        ),
    )
    .await
    .expect_err("context longer than the file cannot match");
    assert_eq!(error.code, "patch_context_not_found");
}

/// Verifies failed patch preflight leaves every file operation unapplied.
/// 验证 patch 预校验失败会使每项文件操作都保持未应用。
#[tokio::test]
async fn apply_patch_preflight_failure_writes_nothing() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(directory.path().join("sample.txt"), "current\n").expect("write source fixture");
    let error = execute_apply_patch(
            directory.path().to_path_buf(),
            String::from(
                "*** Begin Patch\n*** Add File: added.txt\n+created\n*** Update File: sample.txt\n-missing\n+changed\n*** End Patch\n",
            ),
        )
        .await
        .expect_err("missing context should fail preflight");
    assert_eq!(error.code, "patch_context_not_found");
    assert!(!directory.path().join("added.txt").exists());
    assert_eq!(
        fs::read_to_string(directory.path().join("sample.txt")).expect("read source"),
        "current\n"
    );
}

/// Verifies lifecycle move preserves binary bytes without requiring text decoding.
/// 验证生命周期移动无需文本解码即可保持二进制字节。
#[tokio::test]
async fn native_move_preserves_binary_file_bytes() {
    let directory = tempdir().expect("create temporary directory");
    let bytes = vec![0_u8, 0xFF, 0x10, 0x80, 0x7F];
    fs::write(directory.path().join("source.bin"), &bytes).expect("write binary fixture");
    let response = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "move",
            "path": "source.bin",
            "destination": "moved.bin"
        }),
    )
    .await
    .expect("binary move should succeed");

    assert!(!directory.path().join("source.bin").exists());
    assert_eq!(
        fs::read(directory.path().join("moved.bin")).expect("read moved binary"),
        bytes
    );
    assert_eq!(
        response
            .change_set
            .as_ref()
            .and_then(|change_set| change_set.pointer("/files/0/binary")),
        Some(&Value::Bool(true))
    );
}

/// Verifies Apply Patch can delete an opaque binary file transactionally.
/// 验证 Apply Patch 可以通过事务删除不透明二进制文件。
#[tokio::test]
async fn apply_patch_deletes_binary_file_without_lossy_preview() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(
        directory.path().join("obsolete.bin"),
        [0_u8, 0xFF, 0x01, 0x80],
    )
    .expect("write binary fixture");
    let response = execute_apply_patch(
        directory.path().to_path_buf(),
        String::from("*** Begin Patch\n*** Delete File: obsolete.bin\n*** End Patch\n"),
    )
    .await
    .expect("binary delete should succeed");

    assert!(!directory.path().join("obsolete.bin").exists());
    assert_eq!(response.model_output["operation_count"], json!(1));
    assert_eq!(
        response
            .change_set
            .as_ref()
            .and_then(|change_set| change_set.pointer("/files/0/content_mode")),
        Some(&json!("omitted"))
    );
}

/// Verifies the End of File marker rejects a matching block before the real file end.
/// 验证 End of File 标记会拒绝真实文件末尾之前的匹配块。
#[tokio::test]
async fn apply_patch_end_of_file_marker_is_enforced() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(directory.path().join("sample.txt"), "first\nlast\n").expect("write text fixture");
    let error = execute_apply_patch(
            directory.path().to_path_buf(),
            String::from(
                "*** Begin Patch\n*** Update File: sample.txt\n-first\n+changed\n*** End of File\n*** End Patch\n",
            ),
        )
        .await
        .expect_err("non-terminal block should fail the EOF constraint");

    assert_eq!(error.code, "patch_context_not_found");
    assert_eq!(
        fs::read_to_string(directory.path().join("sample.txt")).expect("read unchanged file"),
        "first\nlast\n"
    );
}

/// Adding a BOM to an empty file is a format-only modify that satisfies the persistent contract.
/// 为空文件添加 BOM 属于纯格式修改，并满足持久契约。
#[tokio::test]
async fn adding_a_bom_to_an_empty_file_is_a_valid_change() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("empty.txt");
    fs::write(&path, b"").expect("write empty fixture");
    let written = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "write", "path": "empty.txt", "content": "", "bom": "present"}),
    )
    .await
    .expect("adding a BOM should succeed");
    let persistent = persistent_change_set_from_host_result(&json!({
        "kind": "change_set",
        "payload": written.change_set.expect("format change returns a ChangeSet"),
    }))
    .expect("format-only modify must satisfy the V1 contract")
    .expect("native write must return a ChangeSet");
    assert!(persistent.files[0].hunks.is_empty());
    assert_ne!(
        persistent.files[0].before_text_format,
        persistent.files[0].after_text_format
    );
    assert_eq!(
        fs::read(&path).expect("read BOM output"),
        [0xEF, 0xBB, 0xBF]
    );
}

/// Verifies native write converts from the detected source encoding and gives new files LF.
/// 验证原生整文件写入会从已检测源编码转换，并为新文件使用 LF。
#[tokio::test]
async fn native_write_converts_encoding_and_applies_new_file_defaults() {
    let directory = tempdir().expect("create temporary directory");
    let converted_path = directory.path().join("converted.txt");
    fs::write(
        &converted_path,
        [0xEF, 0xBB, 0xBF, b'o', b'l', b'd', b'\r', b'\n'],
    )
    .expect("write UTF-8 BOM fixture");

    execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "write",
            "path": "converted.txt",
            "content": "first\nsecond\n",
            "encoding": "utf-16le"
        }),
    )
    .await
    .expect("encoding conversion should succeed");
    let converted = fs::read(&converted_path).expect("read converted fixture");
    assert!(converted.starts_with(&[0xFF, 0xFE]));
    let decoded = crate::text_encoding::decode_text(&converted, None)
        .expect("converted UTF-16LE should decode");
    assert_eq!(decoded.text, "first\r\nsecond\r\n");

    let ambiguous_path = directory.path().join("ambiguous.txt");
    fs::write(&ambiguous_path, [0xE9]).expect("write sparse windows-1252 fixture");
    execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "write",
            "path": "ambiguous.txt",
            "content": "café",
            "encoding": "windows-1252",
            "source_encoding": "windows-1252"
        }),
    )
    .await
    .expect("explicit encoding should confirm an ambiguous source");
    assert_eq!(
        fs::read(&ambiguous_path).expect("read explicit legacy output"),
        b"caf\xE9"
    );

    // created retains the real host response so its public ChangeSet can be parsed unchanged.
    // created 保留真实宿主响应，以便原样解析其中的公共 ChangeSet。
    let created = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "write",
            "path": "new.txt",
            "content": "a\r\nb\n\rc\rd"
        }),
    )
    .await
    .expect("new file should succeed");
    // change_set is the exact payload returned after the new file was committed.
    // change_set 是新文件提交后返回的精确载荷。
    let change_set = created.change_set.expect("created file ChangeSet");
    // persistent is the canonical durable projection of that real host payload.
    // persistent 是该真实宿主载荷的规范可靠投影。
    let persistent = persistent_change_set_from_host_result(&json!({
        "kind": "change_set",
        "payload": change_set,
    }))
    .expect("created file ChangeSet must satisfy the V1 contract")
    .expect("native write must return a ChangeSet");
    assert_eq!(persistent.files.len(), 1);
    assert!(persistent.files[0].hunks.is_empty());
    assert!(persistent.files[0].lifecycle_content.is_some());
    assert_eq!(
        fs::read(directory.path().join("new.txt")).expect("read new file"),
        b"a\nb\nc\nd"
    );
}

/// Verifies terminator-only rewrites produce durable nonempty hunks in both public write paths.
/// 验证两种公开写入路径的纯终止符变更都生成可持久化的非空 hunk。
/// No parameters or return value; asserts exact bytes, final-newline formats and unchanged-file rejection.
/// 无参数或返回值；断言精确字节、末尾换行格式及无变化文件拒绝语义。
#[tokio::test]
async fn native_writes_preserve_terminator_only_changes() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("newline.txt");
    for exact_edit in [false, true] {
        for (before, after) in [
            ("alpha", "alpha\n"),
            ("alpha\n", "alpha"),
            ("alpha\nbeta", "alpha\nbeta\n"),
            ("alpha\nbeta\n", "alpha\nbeta"),
            ("alpha\r\nbeta", "alpha\r\nbeta\r\n"),
            ("alpha\r\nbeta\r\n", "alpha\r\nbeta"),
        ] {
            fs::write(&path, before).unwrap();
            // Both public tools must validate the receipt before committing these byte-only changes.
            // 两个公开工具都必须在提交这些纯字节变化前校验回执。
            let result = if exact_edit {
                execute_file_edit(
                    directory.path().to_path_buf(),
                    json!({
                        "path":"newline.txt", "old_string":before, "new_string":after,
                    }),
                )
                .await
            } else {
                execute_file(
                    directory.path().to_path_buf(),
                    json!({
                        "action":"write", "path":"newline.txt", "content":after,
                    }),
                )
                .await
            }
            .expect("terminator-only mutation must succeed");
            assert_eq!(fs::read(&path).unwrap(), after.as_bytes());
            let persistent = persistent_change_set_from_host_result(&json!({
                "kind":"change_set", "payload":result.change_set.unwrap(),
            }))
            .expect("terminator receipt must satisfy V1")
            .unwrap();
            let file = persistent
                .files
                .iter()
                .find(|file| file.path.as_deref() == Some(display_path(&path).as_str()))
                .expect("receipt must contain the changed path");
            assert_eq!(file.hunks.len(), 1);
            let hunk = &file.hunks[0];
            assert_eq!(
                file.before_text_format.as_ref().unwrap().final_newline,
                Some(before.ends_with('\n'))
            );
            assert_eq!(
                file.after_text_format.as_ref().unwrap().final_newline,
                Some(after.ends_with('\n'))
            );
            assert_eq!(hunk.deleted.len(), 1);
            assert_eq!(hunk.inserted.len(), 1);
            assert_eq!(hunk.deleted[0].content, hunk.inserted[0].content);
            assert_eq!(hunk.deleted[0].line, before.lines().count());
            let unchanged = execute_file(
                directory.path().to_path_buf(),
                json!({
                    "action":"write", "path":"newline.txt", "content":after,
                }),
            )
            .await
            .expect_err("identical bytes remain no_change");
            assert_eq!(unchanged.code, "no_change");
        }
    }
}

/// Confirms ambiguous reads cannot silently authorize mutation and explicit codecs survive durable projection.
/// 确认歧义读取不能静默授权修改，且显式编码在持久投影中保持不变。
/// No parameters or return value; checks exact legacy and BOM-less UTF-16 bytes through public tools.
/// 无参数或返回值；通过公开工具核对传统编码和无 BOM UTF-16 的精确字节。
#[tokio::test]
async fn native_explicit_encoding_closes_read_edit_write_recovery() {
    use crate::text_encoding::{TextBom, encode_text, parse_encoding_label};
    let directory = tempdir().unwrap();
    let path = directory.path().join("encoded.txt");
    for (label, original, replacement) in [
        ("gbk", "中文测试\n", "中文修改\n"),
        ("shift_jis", "テストです\n", "変更です\n"),
        ("big5", "測試\n", "修改\n"),
        ("utf-16le", "中文测试\n", "中文修改\n"),
        ("utf-16be", "中文测试\n", "中文修改\n"),
    ] {
        let codec = parse_encoding_label(label).unwrap();
        let bytes = encode_text(original, codec, TextBom::None).unwrap();
        fs::write(&path, &bytes).unwrap();
        let auto_read = execute_file(
            directory.path().to_path_buf(),
            json!({"action":"read", "path":"encoded.txt"}),
        )
        .await;
        match auto_read {
            Ok(read) => {
                assert_eq!(
                    read.read_metadata.as_ref().unwrap()["encoding_certain"],
                    false
                );
                assert!(
                    read.model_output
                        .as_str()
                        .unwrap()
                        .contains("[encoding_unverified]")
                );
            }
            Err(error) => {
                assert_eq!(error.code, "binary_file");
                assert!(
                    error.as_value()["hint"]
                        .as_str()
                        .unwrap()
                        .contains("UTF-16")
                );
            }
        }
        let denied = execute_file_edit(
            directory.path().to_path_buf(),
            json!({
                "path":"encoded.txt", "old_string":original, "new_string":replacement,
            }),
        )
        .await
        .expect_err("uncertain source must not be rewritten automatically");
        assert!(matches!(
            denied.code.as_str(),
            "encoding_ambiguous" | "binary_file"
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        // Selecting a destination codec must not bypass the same source guard used by file_edit.
        // 选择目标编码不得绕过与 file_edit 相同的源编码防护。
        let denied_write = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action":"write", "path":"encoded.txt", "content":replacement,
                "encoding":"utf-8",
            }),
        )
        .await
        .expect_err("output encoding alone must not authorize an uncertain source");
        assert_eq!(denied_write.code, denied.code);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let explicit_read = execute_file(
            directory.path().to_path_buf(),
            json!({
                "action":"read", "path":"encoded.txt", "encoding":label,
            }),
        )
        .await
        .unwrap();
        assert!(
            explicit_read
                .model_output
                .as_str()
                .unwrap()
                .contains(original.trim_end())
        );
        assert_eq!(
            explicit_read.read_metadata.as_ref().unwrap()["encoding_source"],
            "explicit"
        );
        let edited = execute_file_edit(directory.path().to_path_buf(), json!({
                "path":"encoded.txt", "old_string":original, "new_string":replacement, "encoding":label,
            })).await.unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            encode_text(replacement, codec, TextBom::None).unwrap()
        );
        // Persisted metadata must retain the confirmed codec rather than guessing the changed bytes again.
        // 持久元数据必须保留已确认编码，而不是再次猜测修改后的字节。
        let persistent = persistent_change_set_from_host_result(&json!({
            "kind":"change_set", "payload":edited.change_set.unwrap(),
        }))
        .unwrap()
        .unwrap();
        let file = persistent.files.first().unwrap();
        assert_eq!(
            file.before_text_format
                .as_ref()
                .unwrap()
                .encoding
                .as_deref(),
            Some(codec.label())
        );
        assert_eq!(
            file.after_text_format.as_ref().unwrap().encoding.as_deref(),
            Some(codec.label())
        );
        // An explicit source can be converted to an independently selected destination codec.
        // 显式源编码可以转换成独立选择的目标编码。
        execute_file(
            directory.path().to_path_buf(),
            json!({
                "action":"write", "path":"encoded.txt", "content":replacement,
                "source_encoding":label, "encoding":"utf-8",
            }),
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), replacement.as_bytes());
    }
}

/// Rejects a patch batch before any sibling is committed when one source encoding is unverified.
/// 当任一源编码未确认时，在任何同批文件提交前拒绝整批补丁。
/// No parameters or return value; the recovery hint must name an available explicit-codec tool.
/// 无参数或返回值；恢复提示必须指向实际可用的显式编码工具。
#[tokio::test]
async fn uncertain_patch_source_preserves_all_siblings() {
    // Independent fixtures expose accidental partial publication.
    // 独立夹具用于暴露意外的部分发布。
    let directory = tempdir().unwrap();
    let path = directory.path().join("legacy.txt");
    let bytes = [0xB4, 0xFA, 0xB8, 0xD5, b'\n'];
    fs::write(&path, bytes).unwrap();
    let error = execute_apply_patch(directory.path().to_path_buf(), String::from(
            "*** Begin Patch\n*** Add File: sibling.txt\n+must-not-exist\n*** Update File: legacy.txt\n-測試\n+修改\n*** End Patch"
        )).await.expect_err("uncertain encoding must stop the entire preflight");
    assert_eq!(error.code, "encoding_ambiguous");
    assert!(
        error.as_value()["hint"]
            .as_str()
            .unwrap()
            .contains("file_edit")
    );
    assert!(!directory.path().join("sibling.txt").exists());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

/// Verifies whole-file write accepts empty content for both creation and truncation.
/// 验证整文件写入接受空内容以创建空文件或清空已有文件。
#[tokio::test]
async fn native_write_accepts_empty_content() {
    let directory = tempdir().expect("create temporary directory");
    let existing_path = directory.path().join("existing.txt");
    fs::write(&existing_path, "before\n").expect("write existing fixture");

    // created owns the lifecycle-only result for an empty new file.
    // created 持有空白新文件的纯生命周期结果。
    let created = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "write", "path": "empty.txt", "content": ""}),
    )
    .await
    .expect("empty file creation should succeed");
    // truncated owns the modify-hunk result for clearing an existing file.
    // truncated 持有清空已有文件时的修改 hunk 结果。
    let truncated = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "write", "path": "existing.txt", "content": ""}),
    )
    .await
    .expect("existing file truncation should succeed");

    // created_changeset is the canonical parse of the empty create result.
    // created_changeset 是空文件创建结果的规范解析。
    let created_changeset = persistent_change_set_from_host_result(&json!({
        "kind": "change_set",
        "payload": created.change_set.expect("empty create ChangeSet"),
    }))
    .expect("empty create must satisfy the V1 contract")
    .expect("empty create must return a ChangeSet");
    assert_eq!(
        created_changeset.files[0].change,
        ChangeSetFileChangeKind::Create
    );
    assert!(created_changeset.files[0].hunks.is_empty());
    assert!(matches!(
        created_changeset.files[0].lifecycle_content,
        Some(PersistentLifecycleContent::Full {
            ref content,
            total_line_count: 0,
        }) if content.is_empty()
    ));
    // truncated_changeset is the canonical parse of the empty modify result.
    // truncated_changeset 是空文件修改结果的规范解析。
    let truncated_changeset = persistent_change_set_from_host_result(&json!({
        "kind": "change_set",
        "payload": truncated.change_set.expect("empty modify ChangeSet"),
    }))
    .expect("empty modify must satisfy the V1 contract")
    .expect("empty modify must return a ChangeSet");
    assert_eq!(
        truncated_changeset.files[0].change,
        ChangeSetFileChangeKind::Modify
    );
    assert_eq!(truncated_changeset.files[0].hunks.len(), 1);
    assert!(truncated_changeset.files[0].lifecycle_content.is_none());

    assert_eq!(
        fs::read(directory.path().join("empty.txt")).expect("read created empty file"),
        Vec::<u8>::new()
    );
    assert_eq!(
        fs::read(existing_path).expect("read truncated file"),
        Vec::<u8>::new()
    );
}

/// Verifies lifecycle content uses full mode at the byte limit and safe truncation above it.
/// 验证生命周期正文在字节上限处使用完整模式，超过上限后安全截断。
#[tokio::test]
async fn native_create_lifecycle_content_respects_inline_byte_boundary() {
    // directory isolates both real native file mutations from the repository workspace.
    // directory 把两次真实原生文件变更与仓库工作区隔离。
    let directory = tempdir().expect("create temporary directory");
    // exact_limit contains exactly the maximum allowed inline UTF-8 byte count.
    // exact_limit 精确包含允许内联的最大 UTF-8 字节数。
    let exact_limit = "a".repeat(MAX_INLINE_LIFECYCLE_BYTES);
    // over_limit crosses the boundary with one complete multibyte character.
    // over_limit 使用一个完整多字节字符跨过边界。
    let over_limit = format!("{}中", "a".repeat(MAX_INLINE_LIFECYCLE_BYTES));

    // full_result is the actual host response emitted at the inclusive boundary.
    // full_result 是在包含式边界处发出的真实宿主响应。
    let full_result = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "write", "path": "full.txt", "content": exact_limit}),
    )
    .await
    .expect("exact-limit create should succeed");
    // truncated_result is the actual host response emitted above the byte boundary.
    // truncated_result 是超过字节边界后发出的真实宿主响应。
    let truncated_result = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "write", "path": "truncated.txt", "content": over_limit}),
    )
    .await
    .expect("over-limit create should succeed");
    // full_changeset is the canonical V1 parse of the unmodified real host payload.
    // full_changeset 是对未改写真实宿主载荷执行的规范 V1 解析结果。
    let full_changeset = persistent_change_set_from_host_result(&json!({
        "kind": "change_set",
        "payload": full_result.change_set.expect("full lifecycle ChangeSet"),
    }))
    .expect("full lifecycle payload must be valid")
    .expect("full lifecycle payload must be declared");
    // truncated_changeset is the canonical V1 parse of the over-limit real host payload.
    // truncated_changeset 是对超限真实宿主载荷执行的规范 V1 解析结果。
    let truncated_changeset = persistent_change_set_from_host_result(&json!({
        "kind": "change_set",
        "payload": truncated_result
            .change_set
            .expect("truncated lifecycle ChangeSet"),
    }))
    .expect("truncated lifecycle payload must be valid")
    .expect("truncated lifecycle payload must be declared");

    assert!(matches!(
        full_changeset.files[0].lifecycle_content,
        Some(PersistentLifecycleContent::Full {
            ref content,
            total_line_count: 1,
        }) if content.len() == MAX_INLINE_LIFECYCLE_BYTES
    ));
    assert!(matches!(
        truncated_changeset.files[0].lifecycle_content,
        Some(PersistentLifecycleContent::Truncated {
            ref head,
            ref tail,
            total_line_count: 1,
        }) if head.chars().count() == LIFECYCLE_EXCERPT_CHARS
            && tail.chars().count() == LIFECYCLE_EXCERPT_CHARS
            && tail.ends_with('中')
    ));
    assert_eq!(
        fs::read_to_string(directory.path().join("truncated.txt"))
            .expect("read over-limit created file"),
        over_limit
    );
}

/// Verifies all six public file actions and their action-specific rejection paths.
/// 验证六个公开 file 动作及其动作专属拒绝路径。
#[tokio::test]
async fn native_file_actions_enforce_single_path_contracts() {
    let directory = tempdir().expect("create temporary directory");
    fs::create_dir(directory.path().join("nested")).expect("create nested directory");
    fs::write(directory.path().join("nested/item.txt"), "value\n").expect("write nested fixture");

    let listed = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "list", "path": ".", "recursive": true, "glob": "*.txt"}),
    )
    .await
    .expect("list should succeed");
    assert_eq!(listed.model_output["action"], json!("list"));
    assert_eq!(
        listed.model_output["entries"].as_array().map(Vec::len),
        Some(1)
    );
    fs::write(directory.path().join("nested/文.rs"), "fn main() {}\n")
        .expect("write Unicode fixture");
    let unicode_glob = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "list", "path": "nested", "glob": "?.rs"}),
    )
    .await
    .expect("Unicode scalar glob should succeed");
    assert_eq!(
        unicode_glob.model_output["entries"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    let stat = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "stat", "path": "nested/item.txt"}),
    )
    .await
    .expect("stat should succeed");
    assert_eq!(stat.model_output["action"], json!("stat"));
    assert_eq!(stat.model_output["kind"], json!("file"));
    assert_eq!(stat.model_output["preferred_line_ending"], json!("lf"));

    let invalid_read = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "read", "path": "nested/item.txt", "recursive": true}),
    )
    .await
    .expect_err("read must reject list-only fields");
    assert_eq!(invalid_read.code, "invalid_arguments");

    let missing_content = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "write", "path": "missing-content.txt"}),
    )
    .await
    .expect_err("write must require content");
    assert_eq!(missing_content.code, "invalid_arguments");

    let directory_delete = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "delete", "path": "nested"}),
    )
    .await
    .expect_err("delete must reject directories");
    assert_eq!(directory_delete.code, "unsupported_file_type");

    fs::write(directory.path().join("source.txt"), "source\n").expect("write move source");
    fs::write(directory.path().join("destination.txt"), "destination\n")
        .expect("write move destination");
    let destination_exists = execute_file(
        directory.path().to_path_buf(),
        json!({
            "action": "move",
            "path": "source.txt",
            "destination": "destination.txt"
        }),
    )
    .await
    .expect_err("move must reject an existing destination");
    assert_eq!(destination_exists.code, "destination_exists");

    let deleted = execute_file(
        directory.path().to_path_buf(),
        json!({"action": "delete", "path": "source.txt"}),
    )
    .await
    .expect("regular file delete should succeed");
    assert_eq!(deleted.model_output["action"], json!("delete"));
    assert!(!directory.path().join("source.txt").exists());
}

/// Verifies native mutation results expose stable operation and resource identities.
/// 验证原生变更结果暴露稳定的操作与资源标识。
#[tokio::test]
async fn native_change_set_v2_maps_operations_to_resources() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(directory.path().join("sample.txt"), "old\n").expect("write fixture");

    let response = execute_file_edit(
        directory.path().to_path_buf(),
        json!({
            "path": "sample.txt",
            "old_string": "old",
            "new_string": "new"
        }),
    )
    .await
    .expect("exact edit should succeed");
    let extension = response
        .change_set
        .as_ref()
        .and_then(|change_set| change_set.pointer("/extensions/vulcan.file_tools.v2"))
        .expect("native v2 extension");

    assert_eq!(extension["source_tool"], json!("file_edit"));
    assert_eq!(
        extension["operation_results"][0]["operation_id"],
        json!("file-edit-0")
    );
    assert_eq!(
        extension["operation_results"][0]["resource_id"],
        json!("resource-1")
    );
    assert_eq!(
        extension["resources"][0]["request_path"],
        json!("sample.txt")
    );
    assert!(extension["resources"][0]["before_text_format"].is_object());
    assert!(extension["resources"][0]["after_text_format"].is_object());
    assert_eq!(
        extension["resources"][0]["before_text_format"]["preferred_line_ending"],
        json!("lf")
    );
    assert_eq!(
        extension["resources"][0]["after_text_format"]["preferred_line_ending"],
        json!("lf")
    );
    assert_eq!(
        extension["resources"][0]["actual_ranges"][0]["operation_id"],
        json!("file-edit-0")
    );
    assert_eq!(extension["transaction"]["commit_scope"], json!("all"));
}

/// Verifies multiple anchored hunks and a move share one atomic Apply Patch resource.
/// 验证多个锚定 hunk 与移动操作共享一个原子 Apply Patch 资源。
#[tokio::test]
async fn apply_patch_supports_multiple_hunks_with_move() {
    let directory = tempdir().expect("create temporary directory");
    fs::write(
        directory.path().join("source.txt"),
        "section-a\r\nold-a\r\nsection-b\r\nold-b\r\n",
    )
    .expect("write fixture");

    let response = execute_apply_patch(
            directory.path().to_path_buf(),
            String::from(
                "*** Begin Patch\n*** Update File: source.txt\n*** Move to: moved.txt\n@@ section-a\n-old-a\n+new-a\n@@ section-b\n-old-b\n+new-b\n*** End Patch\n",
            ),
        )
        .await
        .expect("multi-hunk move should succeed");

    assert!(!directory.path().join("source.txt").exists());
    assert_eq!(
        fs::read_to_string(directory.path().join("moved.txt")).expect("read moved result"),
        "section-a\r\nnew-a\r\nsection-b\r\nnew-b\r\n"
    );
    let extension = response
        .change_set
        .as_ref()
        .and_then(|change_set| change_set.pointer("/extensions/vulcan.file_tools.v2"))
        .expect("native v2 extension");
    assert_eq!(extension["source_tool"], json!("apply_patch"));
    assert_eq!(
        extension["resources"][0]["operation_indexes"],
        json!([0, 0, 0])
    );
    assert_eq!(extension["resources"][0]["hunk_indexes"], json!([0, 1]));
}

/// Verifies byte-version changes are rejected immediately before publication.
/// 验证原始字节版本变化会在发布前立即被拒绝。
#[test]
fn native_precondition_detects_stale_file_bytes() {
    let directory = tempdir().expect("create temporary directory");
    let path = directory.path().join("sample.txt");
    fs::write(&path, "before\n").expect("write fixture");
    let snapshot = read_raw_snapshot(&path).expect("read raw snapshot");
    let prepared = PreparedChange {
        text_formats: None,
        change_kind: String::from("modify"),
        source_path: path.clone(),
        destination_path: None,
        original_version: snapshot.version,
        before_bytes: Some(snapshot.bytes),
        original_permissions: Some(snapshot.permissions),
        after_bytes: Some(b"after\n".to_vec()),
        file_payload: whole_file_modify_payload(
            &path,
            &parse_document_lines("before\n"),
            &parse_document_lines("after\n"),
        ),
        operation_results: Vec::new(),
    };
    fs::write(&path, "external\n").expect("mutate fixture externally");

    let error = verify_preconditions(&[prepared]).expect_err("stale bytes must fail");
    assert_eq!(error.code, "stale_file");
    assert_eq!(
        fs::read_to_string(path).expect("read external content"),
        "external\n"
    );
}

/// Verifies an invalid lifecycle row is rejected before any file is created.
/// 验证无效生命周期记录会在任何文件创建前被拒绝。
#[test]
fn invalid_changeset_is_rejected_before_commit() {
    // directory isolates the negative commit assertion from the repository workspace.
    // directory 把负向提交断言与仓库工作区隔离。
    let directory = tempdir().expect("create temporary directory");
    // path is the exact destination that must remain absent after validation fails.
    // path 是校验失败后必须保持不存在的精确目标路径。
    let path = directory.path().join("must-not-exist.txt");
    // prepared intentionally violates the create lifecycle contract with one modify hunk.
    // prepared 故意通过一条修改 hunk 违反创建生命周期契约。
    let prepared = PreparedChange {
        text_formats: None,
        change_kind: String::from("create"),
        source_path: path.clone(),
        destination_path: None,
        original_version: String::new(),
        before_bytes: None,
        original_permissions: None,
        after_bytes: Some(b"unsafe\n".to_vec()),
        file_payload: json!({
            "resource_id": "resource-1",
            "change": "create",
            "path": display_path(&path),
            "hunks": [{
                "before": "",
                "delete": [],
                "insert": [{"line": 1, "content": "unsafe"}],
                "after": "",
            }],
        }),
        operation_results: Vec::new(),
    };

    // error is the pre-commit contract rejection returned by the shared validator.
    // error 是共享校验器在提交前返回的契约拒绝。
    let error = commit_validated_changes("applied", &[prepared], "file")
        .expect_err("invalid ChangeSet must fail before mutation");
    assert_eq!(error.code, "invalid_change_set");
    // Internal contract defects must not invite argument changes or automatic replay.
    // 内部契约缺陷不得诱导修改参数或自动重放。
    let diagnostic = error.as_value();
    assert_eq!(diagnostic["retryable"], false);
    assert_eq!(diagnostic["requires_read"], false);
    assert!(
        diagnostic["hint"]
            .as_str()
            .unwrap()
            .contains("internal defect")
    );
    assert!(!path.exists());
}

/// Rejects duplicate native resource metadata before deletion; returns no value on success.
/// 在删除前拒绝重复原生资源元数据；测试成功时无返回值。
#[test]
fn duplicate_patch_resources_are_rejected_before_commit() {
    // Isolate two independent files whose invalid metadata must never authorize deletion.
    // 隔离两份独立文件，其无效元数据绝不能允许删除。
    let directory = tempdir().expect("temporary directory");
    // Construct individually valid delete rows that collide only in the extension resource table.
    // 构建单独有效的删除记录，仅在扩展资源表中制造冲突。
    let changes = ["first.txt", "second.txt"]
        .iter()
        .enumerate()
        .map(|(index, name)| {
            // Use a real snapshot so all filesystem preconditions would otherwise pass.
            // 使用真实快照，使所有文件系统前置条件原本都能通过。
            let path = directory.path().join(name);
            fs::write(&path, "keep\n").expect("write fixture");
            // Preserve the producer-assigned duplicate identifier to exercise the validation barrier.
            // 保留产出方分配的重复标识，以覆盖校验屏障。
            let snapshot = read_raw_snapshot(&path).expect("read snapshot");
            build_patch_delete_change(index, name, path, snapshot, "duplicate-resource")
        })
        .collect::<Vec<_>>();
    // The canonical parser must reject the same metadata used after commit, before touching files.
    // 规范解析器必须在触碰文件前拒绝提交后使用的同一份元数据。
    let error = commit_validated_changes("applied", &changes, "apply_patch")
        .expect_err("duplicate resources must fail before mutation");
    assert_eq!(error.code, "invalid_change_set");
    assert!(error.message.contains("duplicate resource identifier"));
    for name in ["first.txt", "second.txt"] {
        assert_eq!(
            fs::read_to_string(directory.path().join(name)).expect("file must remain"),
            "keep\n"
        );
    }
}

/// Verifies a failed second publication restores the first file and reports a transaction error.
/// 验证第二次发布失败时会恢复第一个文件并报告事务错误。
#[test]
fn commit_failure_rolls_back_and_reports_transaction_failed() {
    let directory = tempdir().expect("create temporary directory");
    let first_path = directory.path().join("first.txt");
    let blocked_path = directory.path().join("blocked");
    fs::write(&first_path, "before\n").expect("write first fixture");
    fs::create_dir(&blocked_path).expect("create blocking directory");
    let first = read_raw_snapshot(&first_path).expect("read first snapshot");
    let changes = vec![
        PreparedChange {
            text_formats: None,
            change_kind: String::from("modify"),
            source_path: first_path.clone(),
            destination_path: None,
            original_version: first.version,
            before_bytes: Some(first.bytes),
            original_permissions: Some(first.permissions),
            after_bytes: Some(b"after\n".to_vec()),
            file_payload: json!({}),
            operation_results: Vec::new(),
        },
        PreparedChange {
            text_formats: None,
            change_kind: String::from("create"),
            source_path: blocked_path,
            destination_path: None,
            original_version: String::new(),
            before_bytes: None,
            original_permissions: None,
            after_bytes: Some(b"blocked\n".to_vec()),
            file_payload: json!({}),
            operation_results: Vec::new(),
        },
    ];

    let error = commit_changes(&changes).expect_err("second publish should fail");
    assert_eq!(error.code, "transaction_failed");
    assert_eq!(error.details["rollback_state"], json!("complete"));
    assert_eq!(
        fs::read_to_string(first_path).expect("read rolled-back first file"),
        "before\n"
    );
}

/// Verifies public errors retain actionable facts once and never invent candidate discovery.
/// 验证公开错误仅保留一份可操作事实，且不虚构候选发现。
#[tokio::test]
async fn native_error_contract_is_flat_and_actionable() {
    // Use real dispatch so error enrichment and serialization are both exercised.
    // 使用真实调度，同时覆盖错误补全与序列化。
    let directory = tempdir().expect("workspace");
    fs::write(directory.path().join("same.txt"), "unchanged\n").expect("fixture");
    for action in ["read", "stat", "delete"] {
        let error = execute_file(
            directory.path().to_path_buf(),
            json!({"action": action, "path": "missing.txt"}),
        )
        .await
        .expect_err("missing path")
        .as_value();
        assert_eq!(error["code"], "path_not_found");
        assert_eq!(error["requires_read"], false);
        assert!(error["hint"].as_str().unwrap().contains("action=list"));
        assert!(error.get("details").is_none());
    }
    // No-op hints name only parameters accepted by their respective public schemas.
    // 无变化提示仅使用各自公开 Schema 接受的参数名。
    let write = execute_file(
        directory.path().to_path_buf(),
        json!({"action":"write", "path":"same.txt", "content":"unchanged\n"}),
    )
    .await
    .expect_err("no write")
    .as_value();
    let edit = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path":"same.txt", "old_string":"unchanged", "new_string":"unchanged"}),
    )
    .await
    .expect_err("no edit")
    .as_value();
    for (error, parameter) in [(&write, "content"), (&edit, "new_string")] {
        assert_eq!(error["code"], "no_change");
        assert_eq!(error["requires_read"], false);
        assert!(error["hint"].as_str().unwrap().contains(parameter));
        for obsolete in [
            "details",
            "requires_reread",
            "recovery_hint",
            "candidate_count",
            "candidate_ranges",
        ] {
            assert!(
                error.get(obsolete).is_none(),
                "unexpected field {obsolete}: {error}"
            );
        }
        assert!(!error.to_string().contains("new_lines"));
    }
    let same = execute_file(
        directory.path().to_path_buf(),
        json!({"action":"move", "path":"same.txt", "destination":"./same.txt"}),
    )
    .await
    .expect_err("same path")
    .as_value();
    assert_eq!(same["code"], "same_path");
    assert_eq!(
        fs::read_to_string(directory.path().join("same.txt")).unwrap(),
        "unchanged\n"
    );
    let unknown = super::FileToolError::new("execution_failed", "worker stopped").as_value();
    assert_eq!(unknown["commit_scope"], "unknown");
    assert_eq!(unknown["rollback_state"], "unknown");
    assert!(
        unknown["hint"]
            .as_str()
            .unwrap()
            .contains("may already have been applied")
    );
}

/// Verifies bounded ambiguity ranges retain Unicode and mixed-ending source coordinates without editing.
/// 验证有界歧义范围保留 Unicode 与混合换行的源坐标，且不修改文件。
#[tokio::test]
async fn native_ambiguity_reports_real_candidate_lines() {
    // The repeated two-line Unicode substring has known separated source positions.
    // 重复的两行 Unicode 子串具有已知且分隔的源位置。
    let directory = tempdir().expect("workspace");
    let source = "head\r\n你好\r\n世界\rgap\n你好\n世界\n";
    fs::write(directory.path().join("matches.txt"), source).unwrap();
    let error = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path":"matches.txt", "old_string":"你好\n世界", "new_string":"replacement"}),
    )
    .await
    .expect_err("ambiguous")
    .as_value();
    assert_eq!(error["match_count"], 2);
    assert_eq!(error["candidate_count"], 2);
    assert_eq!(
        error["candidate_ranges"],
        json!([{"start_line":2,"line_count":2},{"start_line":5,"line_count":2}])
    );
    assert_eq!(error["candidates_truncated"], false);
    assert_eq!(error["commit_scope"], "none");
    assert_eq!(
        fs::read_to_string(directory.path().join("matches.txt")).unwrap(),
        source
    );
    // Candidate truncation reports the total separately from returned ranges.
    // 候选截断将总数与返回范围数量分开报告。
    let total = super::MAX_ERROR_CANDIDATE_RANGES + 5;
    fs::write(directory.path().join("matches.txt"), "x\n".repeat(total)).unwrap();
    let bounded = execute_file_edit(
        directory.path().to_path_buf(),
        json!({"path":"matches.txt", "old_string":"x", "new_string":"y"}),
    )
    .await
    .expect_err("bounded ambiguity")
    .as_value();
    assert_eq!(bounded["match_count"], total);
    assert_eq!(
        bounded["candidate_count"],
        super::MAX_ERROR_CANDIDATE_RANGES
    );
    assert_eq!(
        bounded["candidate_ranges"].as_array().unwrap().len(),
        super::MAX_ERROR_CANDIDATE_RANGES
    );
    assert_eq!(bounded["candidates_truncated"], true);
}

/// Verifies default byte-bounded pages reconstruct the exact source and explicit ranges keep the hard budget.
/// 验证默认字节有界分页重建精确源码，显式范围仍保留硬预算。
#[tokio::test]
async fn native_read_pages_and_encoding_sources_are_explicit() {
    // Wide Unicode lines exhaust the byte budget before the default line budget.
    // 宽 Unicode 行会先耗尽字节预算，再到默认行数预算。
    let directory = tempdir().expect("workspace");
    let lines = (0..super::DEFAULT_READ_LINES + 1)
        .map(|index| format!("{index}:{}", "中".repeat(100)))
        .collect::<Vec<_>>();
    fs::write(directory.path().join("pages.txt"), lines.join("\n")).unwrap();
    let mut start = 1;
    let mut recovered = Vec::new();
    loop {
        let response = execute_file(
            directory.path().to_path_buf(),
            json!({"action":"read", "path":"pages.txt", "start_line":start}),
        )
        .await
        .expect("page");
        assert!(serde_json::to_vec(&response).unwrap().len() <= super::DEFAULT_READ_RESPONSE_BYTES);
        let metadata = response.read_metadata.unwrap();
        let body = response.model_output.as_str().unwrap();
        let count = metadata["range"]["line_count"].as_u64().unwrap() as usize;
        assert!(count > 0 && count <= super::DEFAULT_READ_LINES);
        assert_eq!(metadata["read_status"], "content");
        for line in body
            .lines()
            .skip(metadata["render"]["output_start_line"].as_u64().unwrap() as usize - 1)
            .take(count)
        {
            recovered.push(line.split_once(": ").expect("numbered line").1.to_string());
        }
        if metadata["truncated"] == false {
            break;
        }
        let next = metadata["next_start_line"].as_u64().unwrap() as usize;
        assert_eq!(next, start + count);
        start = next;
    }
    assert_eq!(recovered, lines);
    let explicit = execute_file(
        directory.path().to_path_buf(),
        json!({"action":"read", "path":"pages.txt", "end_line":lines.len()}),
    )
    .await
    .unwrap();
    assert_eq!(
        explicit.read_metadata.unwrap()["range"]["line_count"],
        lines.len()
    );
    fs::write(directory.path().join("ascii.txt"), "ascii\n").unwrap();
    let encoded = execute_file(
        directory.path().to_path_buf(),
        json!({"action":"read", "path":"ascii.txt", "encoding":"gbk"}),
    )
    .await
    .unwrap();
    assert_eq!(
        encoded.read_metadata.unwrap()["encoding_source"],
        "explicit"
    );
    assert!(
        encoded
            .model_output
            .as_str()
            .unwrap()
            .contains("GBK (explicit)")
    );
    fs::write(directory.path().join("empty.txt"), "").unwrap();
    let empty = execute_file(
        directory.path().to_path_buf(),
        json!({"action":"read", "path":"empty.txt"}),
    )
    .await
    .unwrap();
    assert_eq!(empty.read_metadata.unwrap()["read_status"], "empty_file");
}

/// Verifies rollback failure reports every path as requiring a fresh read.
/// 验证回滚失败会报告每个需要重新读取的路径。
#[test]
fn rollback_failure_reports_disk_state_uncertain() {
    let directory = tempdir().expect("create temporary directory");
    let source_directory = directory.path().join("a-source-directory");
    let destination = directory.path().join("z-destination.txt");
    fs::create_dir(&source_directory).expect("create invalid source directory");
    let change = PreparedChange {
        text_formats: None,
        change_kind: String::from("rename"),
        source_path: source_directory,
        destination_path: Some(destination.clone()),
        original_version: String::from("test-only"),
        before_bytes: Some(b"original\n".to_vec()),
        original_permissions: None,
        after_bytes: Some(b"published\n".to_vec()),
        file_payload: json!({}),
        operation_results: Vec::new(),
    };

    let error = commit_changes(&[change]).expect_err("rollback should be uncertain");
    assert_eq!(error.code, "disk_state_uncertain");
    assert_eq!(error.details["rollback_state"], json!("failed"));
    assert_eq!(error.as_value()["commit_scope"], json!("unknown"));
    assert!(!destination.exists());
}

/// Verifies the permission boundary: relative, new, and absolute in-workspace paths are inside;
/// parent escapes, outside absolute paths, and symbolic-link escapes are outside.
/// 验证权限边界：相对、新建及工作区内绝对路径在内；父级逃逸、外部绝对路径及符号链接逃逸在外。
#[test]
fn permission_boundary_follows_canonical_ancestors() {
    let parent = tempdir().expect("create shared parent directory");
    let workspace = parent.path().join("workspace");
    fs::create_dir_all(workspace.join("src")).expect("create workspace");
    let outside = parent.path().join("outside");
    fs::create_dir_all(&outside).expect("create outside directory");

    for inside in [
        "src/main.rs",
        "new/dir/file.txt",
        ".vulcan/temp/x.txt",
        &display_path(&workspace.join("src/lib.rs")),
    ] {
        assert!(
            path_resolves_inside_workspace(&workspace, inside).unwrap(),
            "{inside}"
        );
    }
    for outside_path in [
        "../outside/file.txt",
        "src/../../outside/file.txt",
        &display_path(&outside.join("file.txt")),
    ] {
        assert!(
            !path_resolves_inside_workspace(&workspace, outside_path).unwrap(),
            "{outside_path}"
        );
    }

    let link = workspace.join("outside-link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).expect("create directory symlink");
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir(&outside, &link) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("create directory symlink: {error}");
    }
    assert!(!path_resolves_inside_workspace(&workspace, "outside-link/new.txt").unwrap());
}

/// Verifies patch write targets include every operation path and move destination.
/// 验证补丁写入目标包含每个操作路径及移动目标。
#[test]
fn apply_patch_write_paths_include_move_destinations() {
    let patch = "*** Begin Patch\n*** Add File: a.txt\n+a\n*** Delete File: b.txt\n*** Update File: c.txt\n*** Move to: ../d.txt\n@@\n-old\n+new\n*** End Patch";
    assert_eq!(
        apply_patch_write_paths(patch).unwrap(),
        ["a.txt", "b.txt", "c.txt", "../d.txt"]
    );
    assert_eq!(
        apply_patch_write_paths("not a patch").unwrap_err().code,
        "patch_syntax_error"
    );
}

/// Verifies end-of-file insertions keep the old last line separate and keep explicit trailing blank lines.
/// 验证文件末尾插入保持原末行独立，并保留显式尾部空行。
#[test]
fn end_of_file_replacements_keep_line_boundaries() {
    let directory = tempdir().expect("create temporary directory");
    let render = |content: &str, start: usize, end: usize, new_lines: &[&str]| {
        let path = directory.path().join("sample.txt");
        fs::write(&path, content).expect("write fixture");
        let snapshot = read_edit_snapshot(&path, None).expect("read snapshot");
        let plan = ReplacementPlan {
            operation_id: String::from("op"),
            start,
            end,
            old_lines: Vec::new(),
            new_lines: new_lines.iter().map(|line| (*line).to_owned()).collect(),
            match_mode: String::from("exact"),
        };
        let bytes = render_snapshot_replacements(&snapshot, &[plan], snapshot.final_newline)
            .expect("render replacement");
        String::from_utf8(bytes).expect("utf-8 output")
    };

    assert_eq!(
        render(
            "HEAD
TAIL",
            2,
            2,
            &["A"]
        ),
        "HEAD
TAIL
A"
    );
    assert_eq!(
        render(
            "HEAD
TAIL",
            2,
            2,
            &["A"]
        ),
        "HEAD
TAIL
A"
    );
    assert_eq!(
        render(
            "HEAD
TAIL
",
            2,
            2,
            &["A"]
        ),
        "HEAD
TAIL
A
"
    );
    assert_eq!(
        render(
            "HEAD
TAIL
",
            1,
            2,
            &["A", ""]
        ),
        "HEAD
A

"
    );
    assert_eq!(
        render(
            "HEAD
TAIL
",
            1,
            2,
            &[""]
        ),
        "HEAD

"
    );
    assert_eq!(
        render(
            "HEAD
TAIL
NEXT
",
            1,
            2,
            &["A", ""]
        ),
        "HEAD
A

NEXT
"
    );
}

/// Rollback restores every changed path even when another path cannot be restored,
/// and leaves unchanged paths untouched.
/// 回滚即使有路径无法恢复也会恢复其他所有已改路径，并保持未改路径不变。
#[test]
fn rollback_restores_every_changed_path_despite_one_failure() {
    let directory = tempdir().expect("create temporary directory");
    let changed = directory.path().join("a-changed.txt");
    let unchanged = directory.path().join("b-unchanged.txt");
    let stuck = directory.path().join("z-stuck");
    fs::write(&changed, "after\n").expect("write changed fixture");
    fs::write(&unchanged, "same\n").expect("write unchanged fixture");
    // A directory where a file must be restored makes that one restore fail.
    // 需要恢复为文件的位置是目录，使这一次恢复失败。
    fs::create_dir(&stuck).expect("create stuck fixture");
    let backup = |bytes: &str| super::FileBackup {
        bytes: Some(bytes.as_bytes().to_vec()),
        permissions: None,
    };
    let backups = std::collections::BTreeMap::from([
        (changed.clone(), backup("before\n")),
        (unchanged.clone(), backup("same\n")),
        (stuck.clone(), backup("file\n")),
    ]);
    let result = super::rollback_paths(
        &backups,
        &std::collections::HashSet::new(),
        &std::collections::BTreeSet::new(),
    );
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&changed).unwrap(), "before\n");
    assert_eq!(fs::read_to_string(&unchanged).unwrap(), "same\n");
}

/// A plan that starts where an earlier insertion starts is shifted by that insertion.
/// 与前一个插入同起点的计划会被该插入偏移。
#[test]
fn same_start_plans_are_shifted_by_earlier_ones() {
    let plan = |id: &str, end: usize, old: &[&str], new: &[&str]| ReplacementPlan {
        operation_id: id.to_string(),
        start: 1,
        end,
        old_lines: old.iter().map(|line| line.to_string()).collect(),
        new_lines: new.iter().map(|line| line.to_string()).collect(),
        match_mode: String::from("apply_patch"),
    };
    let plans = vec![
        plan("insert", 1, &[], &["new-1", "new-2"]),
        plan("replace", 2, &["old"], &["changed"]),
    ];
    let result = super::operation_result(&plans[1], &plans, false, "resource-1");
    assert_eq!(result["final_range"]["start_line"], json!(4));
    let first = super::operation_result(&plans[0], &plans, false, "resource-1");
    assert_eq!(first["final_range"]["start_line"], json!(2));
}
