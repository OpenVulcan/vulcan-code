//! Real isolated in-process tests for VMM's dedicated Jieba/BM25 storage.
//! VMM 独立 Jieba/BM25 存储的真实隔离进程内测试。

use super::*;
use crate::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};
use crate::vmm_data_plane::bootstrap_vmm_data_plane;

/// Exercises actual bundled SQLite/Jieba, not a mocked tokenizer or a substring scoring function.
/// 验证实际随附 SQLite/Jieba，而不是模拟分词器或子串评分函数。
#[tokio::test]
async fn in_process_native_lexical_index_lifecycle() {
    // temporary owns every database used by this test; the user's memory directory is never opened.
    // temporary 拥有本测试的全部数据库；绝不打开用户记忆目录。
    let temporary = tempfile::tempdir().expect("isolated VMM FTS root");
    // data_root and app_path are real test bindings for the standard VMM bootstrap owner.
    // data_root 与 app_path 是标准 VMM 引导宿主使用的真实测试绑定。
    let data_root = temporary.path().join("data");
    std::fs::create_dir_all(&data_root).expect("data root");
    let app_path = data_root.join("app.sqlite3");
    // runtime uses the actual host bootstrap, including VMM's separate SQLite and LanceDB paths.
    // runtime 使用实际宿主引导，包含 VMM 分离的 SQLite 与 LanceDB 路径。
    let runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root,
        sqlite_path: app_path.clone(),
        sqlite_binding_id: "test-app".to_owned(),
        system_statistics_sqlite_path: app_path,
        system_statistics_sqlite_binding_id: "test-app".to_owned(),
    });
    let vmm = bootstrap_vmm_data_plane(&runtime)
        .await
        .expect("VMM bootstrap");
    vmm.ensure_lexical_index().await.expect("Jieba FTS index");
    vmm.ensure_lexical_index()
        .await
        .expect("idempotent schema ensure");
    // documents include summary-only Chinese, English details, code identifiers and identical-score ties.
    // documents 包含仅摘要命中的中文、英文详情、代码标识符及同分样本。
    let documents = vec![
        VmmLexicalDocument {
            memory_id: "vmm-memory-1".to_owned(),
            summary: "用户喜欢西瓜".to_owned(),
            details: "bound text ' OR 1=1; --".to_owned(),
        },
        VmmLexicalDocument {
            memory_id: "vmm-memory-2".to_owned(),
            summary: "Database guide".to_owned(),
            details: "database migration and schema auditing".to_owned(),
        },
        VmmLexicalDocument {
            memory_id: "vmm-memory-3".to_owned(),
            summary: "Code guide".to_owned(),
            details: "getskilllevelup(actor, 10)".to_owned(),
        },
        VmmLexicalDocument {
            memory_id: "vmm-memory-4".to_owned(),
            summary: "用户喜欢西瓜".to_owned(),
            details: "bound text ' OR 1=1; --".to_owned(),
        },
    ];
    vmm.upsert_lexical_documents(&documents)
        .await
        .expect("bound bulk index write");
    // chinese must come from the indexed summary even though details contain no Chinese text.
    // chinese 必须来自被索引的摘要，即使详情中没有中文文本。
    let chinese = vmm
        .search_lexical_page("西瓜", 10, 0)
        .await
        .expect("Chinese segmentation");
    assert_eq!(
        chinese
            .iter()
            .map(|hit| hit.memory_id.as_str())
            .collect::<Vec<_>>(),
        vec!["vmm-memory-1", "vmm-memory-4"]
    );
    assert!(chinese.iter().all(|hit| hit.raw_score < 0.0));
    assert_eq!(chinese[0].raw_score, chinese[1].raw_score);
    assert_eq!(
        vmm.search_lexical_page("西瓜", 1, 1)
            .await
            .expect("next physical page"),
        vec![chinese[1].clone()]
    );
    assert_eq!(
        vmm.search_lexical_page("migration", 10, 0)
            .await
            .expect("English details")[0]
            .memory_id,
        "vmm-memory-2"
    );
    assert_eq!(
        vmm.search_lexical_page("getskilllevelup", 10, 0)
            .await
            .expect("code identifier")[0]
            .memory_id,
        "vmm-memory-3"
    );
    vmm.search_lexical_page("\" OR * NEAR", 10, 0)
        .await
        .expect("safe query tokenization");
    assert!(vmm.search_lexical_page("西瓜", 0, 0).await.is_err());
    assert!(vmm.search_lexical_page("西瓜", 1_025, 0).await.is_err());
    assert!(
        vmm.upsert_lexical_documents(&[documents[0].clone(), documents[0].clone()])
            .await
            .is_err()
    );
    // updated preserves the same stable ID while replacing every old indexed token.
    // updated 保留同一稳定 ID，同时替换全部旧索引词元。
    let updated = VmmLexicalDocument {
        summary: "用户喜欢葡萄".to_owned(),
        ..documents[0].clone()
    };
    vmm.upsert_lexical_documents(&[updated])
        .await
        .expect("update under same ID");
    assert_eq!(
        vmm.search_lexical_page("西瓜", 10, 0)
            .await
            .expect("old summary removed")
            .len(),
        1
    );
    assert_eq!(
        vmm.search_lexical_page("葡萄", 10, 0)
            .await
            .expect("new summary indexed")[0]
            .memory_id,
        "vmm-memory-1"
    );
    vmm.delete_lexical_documents(&["vmm-memory-4".to_owned()])
        .await
        .expect("delete exact memory");
    assert!(
        vmm.search_lexical_page("西瓜", 10, 0)
            .await
            .expect("deleted index invisibility")
            .is_empty()
    );
    vmm.upsert_lexical_documents(&documents[3..])
        .await
        .expect("restore same memory ID");
    assert_eq!(
        vmm.search_lexical_page("西瓜", 10, 0)
            .await
            .expect("restore visibility")[0]
            .memory_id,
        "vmm-memory-4"
    );
    // Removing an early row creates physical gaps; rebuild must not break later ID-based triggers.
    // 删除前部行产生物理间隙；重建不能破坏后续基于 ID 的触发器。
    vmm.delete_lexical_documents(&["vmm-memory-2".to_owned()])
        .await
        .expect("create row gap");
    assert_eq!(
        vmm.rebuild_lexical_index()
            .await
            .expect("real upstream rebuild"),
        3
    );
    assert_eq!(
        vmm.search_lexical_page("西瓜", 10, 0)
            .await
            .expect("rebuild preserves text")[0]
            .memory_id,
        "vmm-memory-4"
    );
    // changed targets an identity whose rebuilt physical row may belong to a different document row.
    // changed 指向重建后的物理行可能不同于文档行的身份。
    let changed = VmmLexicalDocument {
        summary: "用户喜欢苹果".to_owned(),
        ..documents[3].clone()
    };
    vmm.upsert_lexical_documents(&[changed])
        .await
        .expect("update after rebuild");
    assert!(
        vmm.search_lexical_page("西瓜", 10, 0)
            .await
            .expect("old terms gone")
            .is_empty()
    );
    assert_eq!(
        vmm.search_lexical_page("苹果", 10, 0)
            .await
            .expect("new terms indexed")
            .len(),
        1
    );
    assert_eq!(
        vmm.search_lexical_page("getskilllevelup", 10, 0)
            .await
            .expect("unrelated record retained")[0]
            .memory_id,
        "vmm-memory-3"
    );
    vmm.delete_lexical_documents(&["vmm-memory-4".to_owned()])
        .await
        .expect("delete after rebuild");
    assert!(
        vmm.search_lexical_page("苹果", 10, 0)
            .await
            .expect("deleted rebuilt record gone")
            .is_empty()
    );
    assert_eq!(
        vmm.rebuild_lexical_index().await.expect("repeat rebuild"),
        2
    );
    // Full replacement repairs missing/stale documents while preserving quoted text as data.
    // 完整替换修复缺失及陈旧文档，同时将带引号文本保持为数据。
    vmm.replace_lexical_documents(&documents)
        .await
        .expect("replace derived set");
    let counts = vmm.lexical_index_counts().await.expect("actual counts");
    assert_eq!(counts.document_count, 4);
    assert_eq!(counts.indexed_row_count, 4);
    assert_eq!(
        vmm.rebuild_lexical_index()
            .await
            .expect("rebuild complete set"),
        4
    );
    assert_eq!(
        vmm.search_lexical_page("西瓜", 10, 0)
            .await
            .expect("restored Chinese")
            .len(),
        2
    );
    assert!(
        vmm.search_lexical_page("苹果", 10, 0)
            .await
            .expect("stale terms removed")
            .is_empty()
    );
    assert!(
        vmm.replace_lexical_documents(&[documents[0].clone(), documents[0].clone()])
            .await
            .is_err()
    );
    assert_eq!(
        vmm.lexical_index_counts()
            .await
            .expect("invalid input is nonmutating"),
        counts
    );
    vmm.replace_lexical_documents(&[])
        .await
        .expect("empty authority clears derived set");
    assert_eq!(
        vmm.lexical_index_counts()
            .await
            .expect("empty counts")
            .indexed_row_count,
        0
    );
    assert_eq!(vmm.rebuild_lexical_index().await.expect("empty rebuild"), 0);
}
