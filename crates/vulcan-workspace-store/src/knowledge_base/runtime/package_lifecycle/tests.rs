//! Reserved staging names and exact replacement-journal identity regressions.
//! 保留暂存名称与精确覆盖日志身份的回归测试。

use super::*;
use crate::knowledge_base::models::KnowledgeLibrarySettings;

/// Verifies startup cleanup recognizes only reserved extraction directory names with a suffix.
/// 验证启动清理仅识别带后缀的保留解压目录名称。
#[test]
fn package_inspection_temp_name_requires_reserved_prefix_and_suffix() {
    assert!(is_package_inspection_temp_directory_name(
        ".package-inspect-AbCd1234"
    ));
    assert!(!is_package_inspection_temp_directory_name(
        ".package-inspect-"
    ));
    assert!(!is_package_inspection_temp_directory_name(
        "package-inspect-AbCd1234"
    ));
}

/// Verifies replacement journals bind one canonical target, operation, backup name, and phase.
/// 验证覆盖恢复日志绑定唯一的规范目标、操作、备份名称与阶段。
#[test]
fn replacement_journal_rejects_inconsistent_identity_or_phase() {
    // targetId and operationId are distinct canonical UUIDs encoded in every durable artifact.
    // targetId 与 operationId 是编码到每个持久化产物中的不同规范 UUID。
    let target_id = String::from("550e8400-e29b-41d4-a716-446655440000");
    let operation_id = String::from("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
    // previous captures the exact active registry row that prepared recovery must restore.
    // previous 捕获 prepared 恢复必须还原的精确活动注册记录。
    let previous = KnowledgeLibrarySummary {
        id: target_id.clone(),
        name: String::from("恢复测试知识库"),
        description: String::from("fixture"),
        settings: KnowledgeLibrarySettings::default(),
        lifecycle_state: String::from("active"),
        document_count: 2,
        chunk_count: 3,
        index_status: String::from("ready"),
        last_completed_import_at_ms: Some(1),
        created_at_ms: 1,
        updated_at_ms: 2,
    };
    // journal is the valid baseline mutated by each negative assertion.
    // journal 是每个反向断言都会修改的有效基线。
    let journal = KnowledgeReplacementJournal {
        version: KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION,
        target_library_id: target_id.clone(),
        backup_directory_name: format!(".replace-{target_id}-{operation_id}"),
        phase: String::from(KNOWLEDGE_REPLACEMENT_PHASE_PREPARED),
        previous,
    };
    assert_eq!(
        validate_replacement_journal(&journal).expect("validate replacement journal"),
        operation_id
    );

    let mut wrong_target = journal.clone();
    wrong_target.previous.id = String::from("67e55044-10b1-426f-9247-bb680e5fe0c8");
    assert!(validate_replacement_journal(&wrong_target).is_err());

    let mut invalid_backup = journal.clone();
    invalid_backup.backup_directory_name = String::from("../outside");
    assert!(validate_replacement_journal(&invalid_backup).is_err());

    let mut invalid_phase = journal;
    invalid_phase.phase = String::from("unknown");
    assert!(validate_replacement_journal(&invalid_phase).is_err());
}
