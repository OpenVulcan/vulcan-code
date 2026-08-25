use super::*;
use serde_json::{Value, json};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Verifies LuaSkills host-result capabilities explicitly enable only the canonical `change_set` bridge surface.
/// 验证 LuaSkills 宿主结果能力会显式启用且仅暴露 canonical `change_set` 桥接面。
#[test]
fn luaskills_capabilities_enable_change_set_only() {
    let capabilities = luaskills_change_set_host_result_capabilities();
    assert_eq!(capabilities["host_result"]["enabled"], Value::Bool(true));
    assert_eq!(
        capabilities["host_result"]["allowed_kinds"],
        json!(["change_set"])
    );
    assert_eq!(
        capabilities["host_result"]["max_payload_bytes"],
        json!(LUASKILLS_CHANGE_SET_HOST_RESULT_MAX_PAYLOAD_BYTES)
    );
}

/// Verifies raw canonical `change_set` payloads are compacted into summary-ready metadata with aggregated counters while keeping filesystem verification transient to the current run.
/// 验证原始 canonical `change_set` 载荷会被压缩成带聚合计数的摘要元数据，同时让文件系统校验仅保留为当前运行期的瞬时提示。
#[test]
fn compact_change_set_host_result_builds_summary_metadata() {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current timestamp")
        .as_millis();
    let root = std::env::temp_dir().join(format!("vulcan-change-set-helper-test-{unique_suffix}"));
    fs::create_dir_all(&root).expect("create test root");
    let modified_path = root.join("modified.lua");
    let created_path = root.join("created.lua");
    let deleted_path = root.join("deleted.lua");
    let renamed_old_path = root.join("old.lua");
    let renamed_new_path = root.join("new.lua");
    fs::write(&modified_path, "local value = 2\nreturn value\n").expect("write modified file");
    fs::write(&created_path, "return created\n").expect("write created file");
    fs::write(&deleted_path, "return deleted\n").expect("write deleted file");
    fs::write(&renamed_old_path, "return old\n").expect("write rename source file");
    fs::write(&renamed_new_path, "return renamed\n").expect("write renamed file");

    let host_result = json!({
        "kind": "change_set",
        "payload": {
            "mode": "applied",
            "summary": "Applied file edits.",
            "files": [
                {
                    "change": "modify",
                    "path": modified_path.display().to_string(),
                    "hunks": [{
                        "before": "local value = 1",
                        "delete": [{ "line": 1, "content": "local value = 1" }],
                        "insert": [{ "line": 1, "content": "local value = 2" }],
                        "after": "return value"
                    }]
                },
                {
                    "change": "create",
                    "path": created_path.display().to_string(),
                    "content": "return created\n"
                },
                {
                    "change": "delete",
                    "path": deleted_path.display().to_string(),
                    "content": "return deleted\n"
                },
                {
                    "change": "rename",
                    "old_path": renamed_old_path.display().to_string(),
                    "new_path": renamed_new_path.display().to_string()
                }
            ],
            "diagnostics": [{
                "level": "warning",
                "message": "One warning"
            }]
        }
    });

    let compacted = compact_change_set_host_result(&host_result).expect("change_set host result");
    let payload = compacted["payload"].clone();
    let summary =
        change_set_modal_summary_from_payload(&payload).expect("compact summary should parse");
    let runtime_notice = change_set_host_verification_notice(&host_result["payload"])
        .expect("runtime notice should still reflect the current filesystem state");
    assert_eq!(summary.mode, ChangeSetMode::Applied);
    assert_eq!(summary.stats.file_count, 4);
    assert_eq!(summary.stats.modify_count, 1);
    assert_eq!(summary.stats.create_count, 1);
    assert_eq!(summary.stats.delete_count, 1);
    assert_eq!(summary.stats.rename_count, 1);
    assert_eq!(summary.stats.inserted_line_count, 1);
    assert_eq!(summary.stats.deleted_line_count, 1);
    assert_eq!(summary.files.len(), 4);
    assert_eq!(
        summary.files[2].delete_content_mode,
        Some(ChangeSetDeleteContentMode::Full)
    );
    assert_eq!(summary.files[2].content_lines, Some(1));
    assert_eq!(summary.diagnostics.len(), 1);
    assert_eq!(
        summary.verification.status,
        ChangeSetVerificationStatus::Skipped
    );
    assert_eq!(summary.verification.checked_file_count, 0);
    assert!(summary.verification.warnings.is_empty());
    assert!(runtime_notice.contains("Deleted file still exists"));
    assert!(runtime_notice.contains("Renamed source path still exists"));

    let _ = fs::remove_dir_all(&root);
}

/// Verifies truncated delete lifecycle records preserve the large-delete summary without requiring one inline full body.
/// 验证截断 delete 生命周期记录会保留大删除摘要，而不再要求内联完整正文。
#[test]
fn compact_change_set_host_result_preserves_truncated_delete_summary_metadata() {
    let host_result = json!({
        "kind": "change_set",
        "payload": {
            "mode": "applied",
            "summary": "Applied one large file deletion.",
            "files": [{
                "change": "delete",
                "path": r"D:\demo\large-delete.lua",
                "content_mode": "truncated",
                "total_line_count": 12345,
                "content_head": "deleted line 1\ndeleted line 2",
                "content_tail": "deleted line 12344\ndeleted line 12345"
            }]
        }
    });

    let compacted = compact_change_set_host_result(&host_result).expect("change_set host result");
    let payload = compacted["payload"].clone();
    let summary =
        change_set_modal_summary_from_payload(&payload).expect("compact summary should parse");
    assert_eq!(summary.stats.file_count, 1);
    assert_eq!(summary.stats.delete_count, 1);
    assert_eq!(summary.files.len(), 1);
    assert_eq!(
        summary.files[0].delete_content_mode,
        Some(ChangeSetDeleteContentMode::Truncated)
    );
    assert_eq!(summary.files[0].content_lines, Some(12345));
    assert_eq!(summary.files[0].content_bytes, None);
}

/// Verifies preview payloads skip filesystem verification and therefore do not emit warning notices.
/// 验证预览载荷会跳过文件系统校验，因此不会产出警告提示。
#[test]
fn preview_change_set_skips_verification_notice() {
    let payload = json!({
        "mode": "preview",
        "files": [{
            "change": "create",
            "path": r"D:\demo.txt",
            "content": "demo"
        }]
    });
    let summary =
        change_set_modal_summary_from_payload(&payload).expect("preview summary should parse");
    assert_eq!(
        summary.verification.status,
        ChangeSetVerificationStatus::Skipped
    );
    assert!(change_set_host_verification_notice(&payload).is_none());
}

/// Verifies native resource correlation and bounded extension preservation survive compaction.
/// 验证原生资源关联与有界扩展字段在压缩后仍然保留。
#[test]
fn compact_change_set_preserves_native_resource_extension() {
    let host_result = json!({
        "kind": "change_set",
        "payload": {
            "mode": "applied",
            "files": [{
                "resource_id": "resource-1",
                "change": "modify",
                "path": "src/example.rs",
                "hunks": []
            }],
            "extensions": {
                "vulcan.file_edit.v1": {
                    "operation_results": [{
                        "operation_id": "rewrite",
                        "resource_id": "resource-1",
                        "status": "applied"
                    }],
                    "transaction": {
                        "commit_scope": "all",
                        "rollback_state": "not_needed"
                    }
                },
                "untrusted.extension": {"secret": "drop me"}
            }
        }
    });
    let compacted = compact_change_set_host_result(&host_result).expect("change_set host result");
    let payload = &compacted["payload"];
    let summary = change_set_modal_summary_from_payload(payload).expect("compact summary");
    assert_eq!(summary.files[0].resource_id.as_deref(), Some("resource-1"));
    assert_eq!(
        payload["extensions"]["vulcan.file_edit.v1"]["operation_results"][0]["operation_id"],
        json!("rewrite")
    );
    assert!(payload["extensions"].get("untrusted.extension").is_none());
}

/// Verifies the native v2 operation-to-resource map survives common change-set compaction.
/// 验证原生 v2 操作到资源映射可通过公共 change-set 压缩保留。
#[test]
fn compact_change_set_preserves_native_file_tools_v2_extension() {
    let host_result = json!({
        "kind": "change_set",
        "payload": {
            "mode": "applied",
            "files": [{
                "resource_id": "resource-1",
                "change": "modify",
                "path": "src/example.rs",
                "hunks": []
            }],
            "extensions": {
                "vulcan.file_tools.v2": {
                    "source_tool": "apply_patch",
                    "operation_results": [{
                        "operation_id": "patch-0-0",
                        "operation_index": 0,
                        "hunk_index": 0,
                        "resource_id": "resource-1",
                        "request_path": "src/example.rs"
                    }],
                    "resources": [{
                        "resource_id": "resource-1",
                        "request_path": "src/example.rs",
                        "operation_indexes": [0],
                        "hunk_indexes": [0],
                        "actual_ranges": []
                    }],
                    "transaction": {
                        "commit_scope": "all",
                        "rollback_state": "not_needed"
                    },
                    "ignored": "drop me"
                },
                "untrusted.extension": {"secret": "drop me"}
            }
        }
    });

    let compacted = compact_change_set_host_result(&host_result).expect("change_set host result");
    let extension = &compacted["payload"]["extensions"]["vulcan.file_tools.v2"];
    assert_eq!(extension["source_tool"], json!("apply_patch"));
    assert_eq!(
        extension["operation_results"][0]["operation_id"],
        json!("patch-0-0")
    );
    assert_eq!(
        extension["resources"][0]["resource_id"],
        json!("resource-1")
    );
    assert_eq!(extension["transaction"]["commit_scope"], json!("all"));
    assert!(extension.get("ignored").is_none());
    assert!(
        compacted["payload"]["extensions"]
            .get("untrusted.extension")
            .is_none()
    );
}
