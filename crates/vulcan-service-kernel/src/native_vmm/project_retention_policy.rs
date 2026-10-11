//! Memory-owner retention overrides, independent of calling host projects and worker schedules.
//! 记忆归属方保留覆盖，独立于发起调用的宿主项目和工作器调度。

use super::super::{
    MAX_SETTINGS_BYTES, NativeVmmResourceStore, persist_file_atomically, read_optional_regular_file,
};
use crate::native_vmm::retention_policy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vulcan_memorymesh_core::RetentionPolicy;
use vulcan_service_api::{VmmNativeProjectRetentionRules, VmmNativeProjectRetentionSummary};

/// Closed user document keyed only by immutable numeric memory-project IDs.
/// 仅以不可变数字记忆项目 ID 为键的封闭用户文档。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectRetentionPolicies {
    /// Exact supported document revision, without legacy guessing.
    /// 准确支持的文档版本，不猜测旧版兼容。
    schema_version: u32,
    /// Whole business-rule groups; absent project keys explicitly inherit the admitted global policy.
    /// 完整业务规则组；缺失项目键明确继承已准入全局策略。
    projects: BTreeMap<u64, VmmNativeProjectRetentionRules>,
}

/// Overlays only six allowed data-retention values while preserving every host-owned execution flag.
/// 仅覆盖六个获准的数据保留值，保留全部宿主持有的执行标志。
/// Returns an independently validated project policy, never changing the input global policy.
/// 返回独立校验后的项目策略，绝不修改输入的全局策略。
fn resolve(
    base: &RetentionPolicy,
    rules: &VmmNativeProjectRetentionRules,
) -> Result<RetentionPolicy, String> {
    // Reuse the authoritative range validator without granting project ownership of its scheduling fields.
    // 复用权威范围校验器，但不授予项目对调度字段的所有权。
    let mut validation = retention_policy::defaults();
    validation.turn_hot_window_size = rules.turn_hot_window_size;
    validation.session_idle_ms = rules.session_idle_ms;
    validation.trash_retention_ms = rules.trash_retention_ms;
    validation.protected_priority_floor = rules.protected_priority_floor;
    validation.protected_memory_level_floor = rules.protected_memory_level_floor;
    validation.skip_protected_shared_memories = rules.skip_protected_shared_memories;
    retention_policy::validate(&validation)?;
    let mut policy = base.clone();
    policy.turn_hot_window_size = rules.turn_hot_window_size;
    policy.session_idle_ms = rules.session_idle_ms;
    policy.trash_retention_ms = rules.trash_retention_ms;
    policy.protected_priority_floor = rules.protected_priority_floor;
    policy.protected_memory_level_floor = rules.protected_memory_level_floor;
    policy.skip_protected_shared_memories = rules.skip_protected_shared_memories;
    Ok(policy)
}

impl NativeVmmResourceStore {
    /// Reads and validates the sole project-rule document, preserving absence as explicit inheritance.
    /// 读取并校验唯一项目规则文档，将文件缺失保留为显式继承。
    fn load_project_retention_document(
        &self,
        base: &RetentionPolicy,
    ) -> Result<ProjectRetentionPolicies, String> {
        let path = self.config_root.join("memory-project-retention.json");
        let Some(bytes) = read_optional_regular_file(&self.config_root, &path, MAX_SETTINGS_BYTES)?
        else {
            return Ok(ProjectRetentionPolicies {
                schema_version: 1,
                projects: BTreeMap::new(),
            });
        };
        let document: ProjectRetentionPolicies =
            serde_json::from_slice(&bytes).map_err(|error| {
                format!("Failed to parse the native memory project retention policy: {error}")
            })?;
        if document.schema_version != 1 || document.projects.contains_key(&0) {
            return Err(
                "Invalid native memory project retention policy version or project ID.".to_owned(),
            );
        }
        for rule in document.projects.values() {
            resolve(base, rule)?;
        }
        Ok(document)
    }

    /// Returns effective rules and inheritance without writing defaults or modifying live records.
    /// 返回有效规则和继承信息，不写入默认值或修改现有记录。
    /// The caller holds the configuration gate and verifies catalog existence separately.
    /// 调用方持有配置锁，并独立验证目录存在性。
    pub(in crate::native_vmm) fn project_retention_summary(
        &self,
        project_id: u64,
    ) -> Result<VmmNativeProjectRetentionSummary, String> {
        if project_id == 0 {
            return Err("The memory project ID must be greater than zero.".to_owned());
        }
        let base = retention_policy::resolve(&self.retention_settings()?)?;
        let mut document = self.load_project_retention_document(&base)?;
        let overrides = document.projects.remove(&project_id);
        let effective = match &overrides {
            Some(rules) => resolve(&base, rules)?,
            None => base.clone(),
        };
        Ok(VmmNativeProjectRetentionSummary {
            project_id,
            overrides,
            effective: project_rules(&effective),
            global: project_rules(&base),
        })
    }

    /// Atomically saves an explicit whole-group edit while preserving every other project's document entry.
    /// 原子保存显式完整配置组编辑，保留文档中其他所有项目条目。
    /// None is supplied only by the explicit inherit action, never by an omitted request field.
    /// None 仅由显式继承动作提供，绝不来自请求字段遗漏。
    pub(in crate::native_vmm) fn save_project_retention(
        &self,
        project_id: u64,
        rules: Option<VmmNativeProjectRetentionRules>,
    ) -> Result<(), String> {
        if project_id == 0 {
            return Err("The memory project ID must be greater than zero.".to_owned());
        }
        let base = retention_policy::resolve(&self.retention_settings()?)?;
        let mut document = self.load_project_retention_document(&base)?;
        match rules {
            Some(rules) => {
                resolve(&base, &rules)?;
                document.projects.insert(project_id, rules);
            }
            None => {
                document.projects.remove(&project_id);
            }
        }
        let bytes = serde_json::to_vec_pretty(&document).map_err(|error| {
            format!("Failed to serialize the memory project retention policy: {error}")
        })?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err(
                "The memory project retention policy exceeds the file size limit.".to_owned(),
            );
        }
        persist_file_atomically(
            &self.config_root,
            &self.config_root.join("memory-project-retention.json"),
            &bytes,
        )
    }

    /// Freezes all existing project policies from one bounded user-file read and an admitted global base.
    /// 从一次有界用户文件读取和已准入全局基础冻结全部现有项目策略。
    /// The caller supplies catalog IDs under its project gate; deleted IDs never grant access or create projects.
    /// 调用方在项目锁内提供目录 ID；已删除 ID 绝不授予访问或创建项目。
    pub(in crate::native_vmm) fn project_retention_policies(
        &self,
        base: &RetentionPolicy,
        project_ids: &[u64],
    ) -> Result<BTreeMap<u64, RetentionPolicy>, String> {
        let mut rules = BTreeMap::new();
        for (id, rule) in self.load_project_retention_document(base)?.projects {
            rules.insert(id, resolve(base, &rule)?);
        }
        let mut policies = BTreeMap::new();
        for id in project_ids {
            if *id == 0 || policies.contains_key(id) {
                return Err(
                    "Native memory project retention policies need non-zero, unique catalog IDs."
                        .to_owned(),
                );
            }
            // Absence has the explicitly declared inheritance meaning, not a fallback for a parsing error.
            // 缺失具有明确声明的继承含义，不作为解析错误的回退。
            policies.insert(*id, rules.remove(id).unwrap_or_else(|| base.clone()));
        }
        Ok(policies)
    }
}

/// Projects only declared project-owned fields from a validated executable policy.
/// 从已校验可执行策略中仅投影声明由项目持有的字段。
fn project_rules(policy: &RetentionPolicy) -> VmmNativeProjectRetentionRules {
    VmmNativeProjectRetentionRules {
        turn_hot_window_size: policy.turn_hot_window_size,
        session_idle_ms: policy.session_idle_ms,
        trash_retention_ms: policy.trash_retention_ms,
        protected_priority_floor: policy.protected_priority_floor,
        protected_memory_level_floor: policy.protected_memory_level_floor,
        skip_protected_shared_memories: policy.skip_protected_shared_memories,
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::persist_file_atomically;
    use super::*;

    /// Writes only an isolated temporary policy file to simulate the documented user override path.
    /// 仅写入隔离临时策略文件，模拟已定义的用户覆盖路径。
    fn save(store: &NativeVmmResourceStore, value: serde_json::Value) {
        persist_file_atomically(
            &store.config_root,
            &store.config_root.join("memory-project-retention.json"),
            &serde_json::to_vec(&value).expect("fixture JSON"),
        )
        .expect("fixture policy");
    }

    /// Returns a complete six-field fixture, never adding worker or profile fields to the project group.
    /// 返回完整六字段夹具，绝不向项目组加入工作器或画像字段。
    fn rules() -> VmmNativeProjectRetentionRules {
        VmmNativeProjectRetentionRules {
            turn_hot_window_size: 2,
            session_idle_ms: 17,
            trash_retention_ms: 23,
            protected_priority_floor: 81,
            protected_memory_level_floor: 4,
            skip_protected_shared_memories: false,
        }
    }

    /// Explicit saves preserve unrelated owners and rejected edits preserve exact persistent bytes.
    /// 显式保存保留无关归属方，被拒绝编辑保留准确持久字节。
    #[test]
    fn explicit_save_roundtrip_and_inherit_are_non_mutating_for_siblings() {
        let directory = tempfile::tempdir().expect("isolated root");
        let store = NativeVmmResourceStore::new(directory.path());
        let initial = store.project_retention_summary(1).expect("inherited");
        assert!(initial.overrides.is_none());
        assert_eq!(initial.effective, initial.global);
        assert!(!directory.path().join("config").exists());
        store
            .save_project_retention(1, Some(rules()))
            .expect("save a");
        let mut b = rules();
        b.trash_retention_ms = 51;
        store
            .save_project_retention(2, Some(b.clone()))
            .expect("save b");
        let path = directory
            .path()
            .join("config/memory-project-retention.json");
        let before = std::fs::read(&path).expect("saved bytes");
        let mut invalid = rules();
        invalid.turn_hot_window_size = 0;
        assert!(store.save_project_retention(1, Some(invalid)).is_err());
        assert_eq!(std::fs::read(&path).expect("unchanged"), before);
        let reopened = NativeVmmResourceStore::new(directory.path());
        assert_eq!(
            reopened.project_retention_summary(1).expect("a").overrides,
            Some(rules())
        );
        reopened
            .save_project_retention(1, None)
            .expect("explicit inheritance");
        assert_eq!(
            store.project_retention_summary(1).expect("restored"),
            initial
        );
        assert_eq!(
            store
                .project_retention_summary(2)
                .expect("untouched b")
                .overrides,
            Some(b)
        );
    }

    /// Mode and complete owner rules are mandatory; omitted fields never request an implicit reset.
    /// 模式和完整归属规则必须提供；遗漏字段绝不请求隐式重置。
    #[test]
    fn edit_contract_requires_an_explicit_closed_mode() {
        use vulcan_service_api::VmmNativeProjectRetentionEdit;
        for value in [
            serde_json::json!({"projectId":1}),
            serde_json::json!({"projectId":1,"rules":null}),
            serde_json::json!({"projectId":1,"mode":"override"}),
            serde_json::json!({"projectId":1,"mode":"inherit","rules":rules()}),
            serde_json::json!({"projectId":1,"mode":"override","rules":{},"enabled":true}),
        ] {
            assert!(serde_json::from_value::<VmmNativeProjectRetentionEdit>(value).is_err());
        }
        assert!(
            serde_json::from_value::<VmmNativeProjectRetentionEdit>(
                serde_json::json!({"projectId":1,"mode":"inherit"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<VmmNativeProjectRetentionEdit>(
                serde_json::json!({"projectId":1,"mode":"override","rules":rules()})
            )
            .is_ok()
        );
    }

    /// Independent projects, live inheritance and frozen admission survive reload without implicit writes.
    /// 独立项目、实时继承和冻结准入在重新加载后保留，不产生隐式写入。
    #[test]
    fn owner_rules_are_isolated_and_inheritance_is_live() {
        let directory = tempfile::tempdir().expect("isolated root");
        let store = NativeVmmResourceStore::new(directory.path());
        let mut global = RetentionPolicy::v1();
        assert_eq!(
            store
                .project_retention_policies(&global, &[1, 2])
                .expect("defaults")[&1],
            global
        );
        assert!(!directory.path().join("config").exists());
        save(
            &store,
            serde_json::json!({"schema_version":1,"projects":{"1":rules()}}),
        );
        let frozen = store
            .project_retention_policies(&global, &[1, 2])
            .expect("admitted");
        assert_eq!(frozen[&1].turn_hot_window_size, 2);
        assert_eq!(frozen[&1].session_idle_ms, 17);
        assert_eq!(frozen[&1].trash_retention_ms, 23);
        assert_eq!(frozen[&1].protected_priority_floor, 81);
        assert_eq!(frozen[&1].protected_memory_level_floor, 4);
        assert!(!frozen[&1].skip_protected_shared_memories);
        assert_eq!(frozen[&2], global);
        global.turn_hot_window_size = 5;
        let reopened = NativeVmmResourceStore::new(directory.path());
        let current = reopened
            .project_retention_policies(&global, &[1, 2])
            .expect("reload");
        assert_eq!(current[&1], frozen[&1]);
        assert_eq!(current[&2].turn_hot_window_size, 5);
        assert_ne!(frozen[&2].turn_hot_window_size, 5);
        assert_eq!(current[&1].cold_turn_job_limit, global.cold_turn_job_limit);
        save(
            &store,
            serde_json::json!({"schema_version":1,"projects":{}}),
        );
        assert_eq!(
            store
                .project_retention_policies(&global, &[1, 2])
                .expect("inherit")[&1],
            global
        );
        assert_eq!(frozen[&1].turn_hot_window_size, 2);
    }

    /// Unknown fields, invalid values and invalid identities fail closed without rewriting the file.
    /// 未知字段、非法值和非法身份明确失败，不改写文件。
    #[test]
    fn invalid_project_rules_never_become_global_fallback() {
        let directory = tempfile::tempdir().expect("isolated root");
        let store = NativeVmmResourceStore::new(directory.path());
        let global = RetentionPolicy::v1();
        for (field, value) in [
            ("enabled", serde_json::json!(false)),
            ("profiles", serde_json::json!({})),
            ("scanIntervalMinutes", serde_json::json!(1)),
            ("turnHotWindowSize", serde_json::json!(0)),
            ("protectedPriorityFloor", serde_json::json!(101)),
            ("trashRetentionMs", serde_json::json!(0)),
        ] {
            let mut invalid = serde_json::to_value(rules()).expect("rule");
            invalid[field] = value;
            save(
                &store,
                serde_json::json!({"schema_version":1,"projects":{"1":invalid}}),
            );
            let path = store.config_root.join("memory-project-retention.json");
            let before = std::fs::read(&path).expect("before");
            assert!(
                store.project_retention_policies(&global, &[1, 2]).is_err(),
                "{field}"
            );
            assert_eq!(std::fs::read(path).expect("after"), before);
        }
        for document in [
            serde_json::json!({"schema_version":2,"projects":{}}),
            serde_json::json!({"schema_version":1,"projects":{"0":rules()}}),
        ] {
            save(&store, document);
            assert!(store.project_retention_policies(&global, &[1]).is_err());
        }
    }
}
