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

/// Verifies the canonical persistent change-set round-trips and derives hunk
/// statistics without re-parsing any other text.
/// 验证 canonical 持久化 change-set 可以往返序列化，并直接从 hunk 推导统计，
/// 不再解析其他任何文本。
#[test]
fn persistent_change_set_round_trip_and_stats() {
    let changeset = PersistentChangeSet {
        mode: ChangeSetMode::Applied,
        summary: Some("one exact edit".to_string()),
        files: vec![PersistentChangeSetFile {
            resource_id: None,
            change: ChangeSetFileChangeKind::Modify,
            path: Some("a.txt".to_string()),
            old_path: None,
            new_path: None,
            hunks: vec![PersistentChangeSetHunk {
                before: String::new(),
                after: String::new(),
                deleted: vec![
                    PersistentChangeSetLine {
                        line: 1,
                        content: "old".to_string(),
                    },
                    PersistentChangeSetLine {
                        line: 2,
                        content: "gone".to_string(),
                    },
                ],
                inserted: vec![PersistentChangeSetLine {
                    line: 1,
                    content: "new".to_string(),
                }],
            }],
            lifecycle_content: None,
            before_text_format: Some(PersistentTextFormat {
                encoding: Some("utf-8".to_string()),
                encoding_source: Some("bom".to_string()),
                encoding_certain: Some(true),
                bom: Some(false),
                line_ending: Some("LF".to_string()),
                preferred_line_ending: Some("LF".to_string()),
                mixed_line_endings: Some(false),
                final_newline: Some(true),
            }),
            after_text_format: None,
        }],
        diagnostics: Vec::new(),
    };
    let encoded = serde_json::to_value(&changeset).expect("persistent change-set should serialize");
    let decoded: PersistentChangeSet =
        serde_json::from_value(encoded).expect("persistent change-set should deserialize");
    assert_eq!(decoded, changeset);
    let stats = persistent_change_set_stats(&changeset);
    assert_eq!(stats.file_count, 1);
    assert_eq!(stats.modify_count, 1);
    assert_eq!(stats.inserted_line_count, 1);
    assert_eq!(stats.deleted_line_count, 2);
    assert_eq!(stats.create_count, 0);
}

/// Verifies an empty persistent change-set yields zero counters.
/// 验证空 persistent change-set 产生零计数。
#[test]
fn persistent_change_set_empty_stats() {
    let changeset = PersistentChangeSet {
        mode: ChangeSetMode::Preview,
        summary: None,
        files: Vec::new(),
        diagnostics: Vec::new(),
    };
    let stats = persistent_change_set_stats(&changeset);
    assert_eq!(stats.file_count, 0);
    assert_eq!(stats.inserted_line_count, 0);
    assert_eq!(stats.deleted_line_count, 0);
}

/// Verifies lifecycle files derive `+/-` counters from the same persisted body metadata shown by the card.
/// 验证生命周期文件从卡片展示所用的同一份持久化正文元数据推导 `+/-` 计数。
#[test]
fn persistent_lifecycle_stats_use_declared_complete_line_counts() {
    let changeset = PersistentChangeSet {
        mode: ChangeSetMode::Applied,
        summary: None,
        files: vec![
            PersistentChangeSetFile {
                resource_id: Some("created".to_string()),
                change: ChangeSetFileChangeKind::Create,
                path: Some("README.md".to_string()),
                old_path: None,
                new_path: None,
                hunks: Vec::new(),
                lifecycle_content: Some(PersistentLifecycleContent::Truncated {
                    head: "first\nsecond".to_string(),
                    tail: "penultimate\nlast".to_string(),
                    total_line_count: 140,
                }),
                before_text_format: None,
                after_text_format: None,
            },
            PersistentChangeSetFile {
                resource_id: Some("deleted".to_string()),
                change: ChangeSetFileChangeKind::Delete,
                path: Some("README.old.md".to_string()),
                old_path: None,
                new_path: None,
                hunks: Vec::new(),
                lifecycle_content: Some(PersistentLifecycleContent::Truncated {
                    head: "deleted 1\ndeleted 2".to_string(),
                    tail: "deleted 207\ndeleted 208".to_string(),
                    total_line_count: 208,
                }),
                before_text_format: None,
                after_text_format: None,
            },
        ],
        diagnostics: Vec::new(),
    };

    let stats = persistent_change_set_stats(&changeset);
    assert_eq!(stats.inserted_line_count, 140);
    assert_eq!(stats.deleted_line_count, 208);
}

/// Verifies strict host-result conversion preserves truncated deleted rows and exact file formats.
/// 验证严格宿主结果转换会保留截断删除行与精确文件格式。
#[test]
fn persistent_host_result_preserves_truncated_delete_and_formats() {
    let host_result = json!({
        "kind": "change_set",
        "payload": {
            "mode": "applied",
            "summary": "deleted one file",
            "files": [{
                "resource_id": "resource-1",
                "change": "delete",
                "path": "README.md",
                "content_mode": "truncated",
                "total_line_count": 208,
                "content_head": "deleted 1\ndeleted 2",
                "content_tail": "deleted 207\ndeleted 208"
            }],
            "extensions": {
                "vulcan.file_tools.v2": {
                    "resources": [{
                        "resource_id": "resource-1",
                        "before_text_format": {
                            "encoding": "utf-8",
                            "encoding_source": "bom",
                            "encoding_certain": true,
                            "bom": false,
                            "line_ending": "LF",
                            "preferred_line_ending": "LF",
                            "mixed_line_endings": false,
                            "final_newline": true
                        },
                        "after_text_format": null
                    }]
                }
            },
            "diagnostics": [{"level": "warning", "message": "example"}]
        }
    });

    let changeset = persistent_change_set_from_host_result(&host_result)
        .expect("canonical host result should parse")
        .expect("change-set kind should be recognized");
    assert_eq!(changeset.summary.as_deref(), Some("deleted one file"));
    assert_eq!(changeset.diagnostics.len(), 1);
    assert!(matches!(
        changeset.files[0].lifecycle_content,
        Some(PersistentLifecycleContent::Truncated {
            ref head,
            ref tail,
            total_line_count: 208,
        }) if head.contains("deleted 1") && tail.contains("deleted 208")
    ));
    assert_eq!(
        changeset.files[0]
            .before_text_format
            .as_ref()
            .and_then(|format| format.encoding.as_deref()),
        Some("utf-8")
    );
    assert_eq!(
        persistent_change_set_stats(&changeset).deleted_line_count,
        208
    );
}

/// Verifies one malformed file fails the declared change-set instead of being silently filtered out.
/// 验证单条畸形文件会使已声明的 change-set 失败，而不是被静默过滤。
#[test]
fn persistent_host_result_rejects_one_malformed_file_without_partial_output() {
    let host_result = json!({
        "kind": "change_set",
        "payload": {
            "mode": "applied",
            "files": [
                {"change": "modify", "path": "good.txt", "hunks": []},
                {"change": "modify", "path": "bad.txt", "hunks": "not-an-array"}
            ]
        }
    });

    let error = persistent_change_set_from_host_result(&host_result)
        .expect_err("malformed declared file should fail the exact conversion");
    assert_eq!(error.location, "payload.files[1].hunks");
}

/// Verifies the negative golden rejects structurally inconsistent producer contracts at exact locations.
/// 验证负向 golden 会在精确位置拒绝结构不一致的产出方契约。
#[test]
fn persistent_change_set_negative_golden_rejects_root_contract_violations() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/persistent_change_set_invalid.golden.json"
    ))
    .expect("negative change-set golden should parse");
    let cases = fixture
        .as_array()
        .expect("negative change-set golden must be one array");

    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let error = persistent_change_set_from_host_result(&case["host_result"])
            .expect_err("negative golden must fail");
        assert_eq!(
            error.location,
            case["expected_location"]
                .as_str()
                .expect("expected location"),
            "case {name} returned another location"
        );
        assert_eq!(
            error.detail,
            case["expected_detail"].as_str().expect("expected detail"),
            "case {name} returned another detail"
        );
    }
}

/// Line breaks pair up left to right exactly as the host text document reads them.
/// 换行对按与宿主文本文档完全相同的从左到右方式配对。
#[test]
fn line_breaks_pair_like_the_host_document() {
    assert_eq!(normalize_text("a\n\rb"), "a\nb");
    assert_eq!(normalize_text("a\r\nb"), "a\nb");
    assert_eq!(normalize_text("a\r\n\rb"), "a\n\nb");
    assert_eq!(normalize_text("a\n\r\nb"), "a\n\nb");
    assert_eq!(normalize_text("a\rb\n"), "a\nb\n");
}
