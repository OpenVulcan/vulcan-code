//! Caller override composition, persistence and immutable admission regression coverage.
//! 调用方覆盖组合、持久化与不可变准入回归覆盖。

use super::*;
use vulcan_memorymesh_core::{MemoryLevel, MemoryStability};
use vulcan_service_api::{VmmNativeExtractedMemoryLevel, VmmNativeExtractedMemoryStability};

/// Saving A cannot replace B, global rules, fixed-user profiles, or a previously admitted snapshot.
/// 保存 A 不能替换 B、全局规则、固定用户画像或此前准入快照。
#[test]
fn caller_groups_roundtrip_and_inherit_current_globals_without_cross_project_leaks() {
    let directory = tempfile::tempdir().expect("isolated root");
    let store = NativeVmmResourceStore::new(directory.path());
    let core = MemoryMeshCore::new().expect("core");
    let global = store.runtime_policy(&core).expect("global");
    assert!(!directory.path().join("config").exists());
    let mut a = VmmNativeHostProjectPolicyOverrides::default();
    let mut a_recall = global.recall.clone();
    a_recall.precheck_history_turn_limit = 1;
    a_recall.postaction_memory_priority = 81;
    a_recall.postaction_memory_stability = VmmNativeExtractedMemoryStability::Working;
    a_recall.postaction_memory_level = VmmNativeExtractedMemoryLevel::Phase;
    a_recall.postaction_memory_refresh_weight = 7;
    a_recall.postaction_memory_expiry_ms = 123_456;
    a.recall = Some(a_recall.clone());
    store
        .update_host_project_policy(&core, "host-a", a.clone())
        .expect("save a");
    let mut b = VmmNativeHostProjectPolicyOverrides::default();
    let mut b_ranking = global.retrieval.clone();
    b_ranking.rrf_k = "7".to_owned();
    b.retrieval = Some(b_ranking.clone());
    store
        .update_host_project_policy(&core, "host-b", b.clone())
        .expect("save b");
    let frozen_bundle = store
        .current_host_project_bundle("vector-fixture".to_owned(), 2, "host-a")
        .expect("admit a");
    let frozen = core.register_config(frozen_bundle).expect("frozen a");
    let mut global_recall = global.recall.clone();
    global_recall.precheck_history_turn_limit = 5;
    global_recall.postaction_memory_priority = 62;
    global_recall.postaction_memory_stability = VmmNativeExtractedMemoryStability::Archival;
    global_recall.postaction_memory_level = VmmNativeExtractedMemoryLevel::Persistent;
    global_recall.postaction_memory_refresh_weight = 3;
    global_recall.postaction_memory_expiry_ms = 654_321;
    store
        .update_runtime_policy(&core, global_recall.clone())
        .expect("global edit");
    let reopened = NativeVmmResourceStore::new(directory.path());
    let effective_a = reopened
        .host_project_policy_summary(&core, "host-a")
        .expect("a");
    let effective_b = reopened
        .host_project_policy_summary(&core, "host-b")
        .expect("b");
    assert_eq!(effective_a.overrides, a);
    assert_eq!(effective_b.overrides, b);
    assert_eq!(effective_a.effective.recall, a_recall);
    assert_eq!(effective_b.effective.recall, global_recall);
    assert_eq!(effective_b.effective.retrieval, b_ranking);
    assert_eq!(effective_a.effective.profiles, global.profiles);
    assert_eq!(effective_b.effective.profiles, global.profiles);
    assert_eq!(effective_a.effective.retention, global.retention);
    assert_eq!(frozen.memory_policy.precheck_history_turn_limit, 1);
    assert_eq!(frozen.memory_policy.postaction_memory_priority, 81);
    assert_eq!(
        frozen.memory_policy.postaction_memory_stability,
        MemoryStability::Working
    );
    assert_eq!(
        frozen.memory_policy.postaction_memory_level,
        MemoryLevel::Phase
    );
    assert_eq!(frozen.memory_policy.postaction_memory_refresh_weight, 7);
    assert_eq!(frozen.memory_policy.postaction_memory_expiry_ms, 123_456);
    let actual_b = reopened
        .current_host_project_bundle("vector-fixture".to_owned(), 2, "host-b")
        .expect("runtime b");
    assert_eq!(actual_b.memory_policy.precheck_history_turn_limit, 5);
    assert_eq!(actual_b.memory_policy.postaction_memory_priority, 62);
    assert_eq!(
        actual_b.memory_policy.postaction_memory_stability,
        MemoryStability::Archival
    );
    assert_eq!(
        actual_b.memory_policy.postaction_memory_level,
        MemoryLevel::Persistent
    );
    assert_eq!(actual_b.memory_policy.postaction_memory_refresh_weight, 3);
    assert_eq!(actual_b.memory_policy.postaction_memory_expiry_ms, 654_321);
    assert_eq!(actual_b.memory_policy.retrieval_policy.rrf_k, 7);
    let absent = reopened
        .current_host_project_bundle("vector-fixture".to_owned(), 2, "host-new")
        .expect("unconfigured caller");
    assert_eq!(absent.memory_policy.precheck_history_turn_limit, 5);
    assert_eq!(
        absent.memory_policy.profiles,
        actual_b.memory_policy.profiles
    );
    store
        .update_host_project_policy(
            &core,
            "host-a",
            VmmNativeHostProjectPolicyOverrides::default(),
        )
        .expect("restore inheritance");
    assert_eq!(
        store
            .host_project_policy_summary(&core, "host-a")
            .expect("restored")
            .effective
            .recall,
        global_recall
    );
    assert_eq!(store.host_project_policy("host-b").expect("unchanged b"), b);
    assert_eq!(frozen.memory_policy.precheck_history_turn_limit, 1);
}

/// Invalid edits preserve exact file bytes and cannot smuggle global or memory-owner groups.
/// 非法编辑保留准确文件字节，且不能夹带全局或记忆归属配置组。
#[test]
fn caller_override_validation_is_strict_and_non_mutating() {
    let directory = tempfile::tempdir().expect("isolated root");
    let store = NativeVmmResourceStore::new(directory.path());
    let core = MemoryMeshCore::new().expect("core");
    let global = store.runtime_policy(&core).expect("global");
    let good = VmmNativeHostProjectPolicyOverrides {
        text: Some(global.text.clone()),
        ..Default::default()
    };
    store
        .update_host_project_policy(&core, "host-a", good.clone())
        .expect("save");
    let path = directory.path().join("config/host-project-policies.json");
    let before = std::fs::read(&path).expect("bytes");
    let mut invalid = good;
    invalid
        .text
        .as_mut()
        .expect("explicit text")
        .max_fragment_tokens = 0;
    assert!(
        store
            .update_host_project_policy(&core, "host-a", invalid)
            .is_err()
    );
    assert!(store.host_project_policy(" host-a").is_err());
    assert!(store.host_project_policy("").is_err());
    assert_eq!(std::fs::read(&path).expect("unchanged"), before);
    for forbidden in ["profiles", "retention", "backendMode", "userId"] {
        let value = serde_json::json!({"recall":null,"retrieval":null,"text":null, forbidden: {}});
        assert!(
            serde_json::from_value::<VmmNativeHostProjectPolicyOverrides>(value).is_err(),
            "{forbidden}"
        );
    }
    assert_eq!(
        store
            .runtime_policy(&core)
            .expect("globals remain unchanged"),
        global
    );
}
