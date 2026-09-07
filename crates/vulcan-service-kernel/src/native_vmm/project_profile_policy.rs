//! Stable memory-project profile policies, independent of personal profiles and host bindings.
//! 稳定记忆项目画像策略，独立于个人画像和宿主绑定。

use super::super::{
    MAX_SETTINGS_BYTES, NativeVmmResourceStore, persist_file_atomically, read_optional_regular_file,
};
use super::profile_policy::core_lifetimes;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vulcan_memorymesh_core::ConfigBundle;
use vulcan_service_api::{VmmNativeProfileLifetimes, VmmNativeProjectProfilePolicySummary};

/// Closed user configuration keyed by the immutable memory-project ID.
/// 以不可变记忆项目 ID 为键的封闭用户配置。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectProfilePolicies {
    /// Exact supported format version; no legacy data is inferred or migrated.
    /// 准确支持的格式版本，不推断或迁移旧数据。
    schema_version: u32,
    /// Complete project-only lifetime groups; omitted IDs inherit current global defaults.
    /// 完整项目专属生命周期组；未列出的 ID 继承当前全局默认。
    projects: BTreeMap<u64, VmmNativeProfileLifetimes>,
}

impl NativeVmmResourceStore {
    /// Reads the optional user file and validates every complete project group.
    /// 读取可选用户文件并校验每个完整项目配置组。
    /// Returns explicit inheritance when absent, or an error for invalid persisted content.
    /// 文件不存在时返回显式继承，已保存内容无效时返回错误。
    fn load_project_profile_document(&self) -> Result<ProjectProfilePolicies, String> {
        // Only this user-owned file stores memory-project profile overrides.
        // 仅此用户持有文件存储记忆项目画像覆盖。
        let path = self.config_root.join("memory-project-profiles.json");
        let Some(bytes) = read_optional_regular_file(&self.config_root, &path, MAX_SETTINGS_BYTES)?
        else {
            return Ok(ProjectProfilePolicies {
                schema_version: 1,
                projects: BTreeMap::new(),
            });
        };
        // Reject unknown fields, unsupported versions and reserved project identities explicitly.
        // 明确拒绝未知字段、不支持的版本和保留项目身份。
        let document: ProjectProfilePolicies = serde_json::from_slice(&bytes)
            .map_err(|error| format!("解析记忆项目画像规则失败：{error}"))?;
        if document.schema_version != 1 || document.projects.contains_key(&0) {
            return Err("记忆项目画像规则版本或项目 ID 无效。".to_owned());
        }
        for rules in document.projects.values() {
            core_lifetimes(rules).validate()?;
        }
        Ok(document)
    }

    /// Resolves one stable memory project's saved and effective profile rules.
    /// 解析一个稳定记忆项目已保存及实际生效的画像规则。
    /// Takes a nonzero project ID; returns project-only values and explicit inheritance metadata.
    /// 接收非零项目 ID，返回仅项目值与显式继承元数据。
    pub(in crate::native_vmm) fn project_profile_policy_summary(
        &self,
        project_id: u64,
    ) -> Result<VmmNativeProjectProfilePolicySummary, String> {
        if project_id == 0 {
            return Err("记忆项目 ID 必须大于零。".to_owned());
        }
        // Global personal rules never enter the project editor or its persisted payload.
        // 全局个人规则绝不进入项目编辑器或其持久化载荷。
        let global = self.load_runtime_policy()?.profiles.project;
        let overrides = self
            .load_project_profile_document()?
            .projects
            .remove(&project_id);
        let effective = match &overrides {
            Some(rules) => rules.clone(),
            None => global.clone(),
        };
        Ok(VmmNativeProjectProfilePolicySummary {
            project_id,
            overrides,
            effective,
            global,
        })
    }

    /// Saves or removes exactly one project override without modifying sibling or global policies.
    /// 保存或移除准确的一个项目覆盖，不修改同级项目或全局策略。
    /// Takes a nonzero project ID and an explicit complete group or inheritance; returns persistence status.
    /// 接收非零项目 ID 和显式完整组或继承操作，返回持久化结果。
    pub(in crate::native_vmm) fn save_project_profile_policy(
        &self,
        project_id: u64,
        rules: Option<VmmNativeProfileLifetimes>,
    ) -> Result<(), String> {
        if project_id == 0 {
            return Err("记忆项目 ID 必须大于零。".to_owned());
        }
        // Validate before replacing the file, preserving existing content on every rejected edit.
        // 替换文件前校验，使每次被拒绝的编辑均保留原内容。
        let mut document = self.load_project_profile_document()?;
        match rules {
            Some(rules) => {
                core_lifetimes(&rules).validate()?;
                document.projects.insert(project_id, rules);
            }
            None => {
                document.projects.remove(&project_id);
            }
        }
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("序列化记忆项目画像规则失败：{error}"))?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err("记忆项目画像规则超过文件大小上限。".to_owned());
        }
        persist_file_atomically(
            &self.config_root,
            &self.config_root.join("memory-project-profiles.json"),
            &bytes,
        )
    }

    /// Applies one admitted memory owner's project rules to an otherwise resolved command bundle.
    /// 将一个已准入记忆归属方的项目规则应用到其他配置已解析的命令包。
    /// Takes the immutable owner ID, never a host binding lookup; preserves personal and sibling groups.
    /// 接收不可变归属 ID，绝不查询宿主绑定；保留个人及同级配置组。
    pub(in crate::native_vmm) fn apply_memory_project_profile_policy(
        &self,
        bundle: &mut ConfigBundle,
        project_id: u64,
    ) -> Result<(), String> {
        if project_id == 0 {
            return Err("记忆项目 ID 必须大于零。".to_owned());
        }
        // Absence is declared inheritance, not a fallback for malformed configuration.
        // 缺失代表声明的继承，而非无效配置的兜底。
        if let Some(rules) = self
            .load_project_profile_document()?
            .projects
            .get(&project_id)
        {
            bundle.memory_policy.profiles.project = core_lifetimes(rules);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies whole-group persistence, live inheritance, and strict personal/sibling isolation.
    /// 验证完整组持久化、实时继承以及严格的个人与同级项目隔离。
    #[test]
    fn project_profile_rules_preserve_personal_and_sibling_policies() {
        // Every file belongs to this disposable test root, not the user's VMM directory.
        // 每个文件都属于此可丢弃测试根，而非用户 VMM 目录。
        let root = tempfile::tempdir().expect("isolated root");
        let store = NativeVmmResourceStore::new(root.path());
        let initial = store
            .project_profile_policy_summary(1)
            .expect("global defaults");
        assert!(initial.overrides.is_none());
        assert_eq!(initial.effective, initial.global);
        let mut project_a = initial.global.clone();
        project_a.stable.base_ms = 31;
        project_a.stable.step_ms = 7;
        project_a.stable.max_ms = 99;
        let mut project_b = initial.global.clone();
        project_b.stable.base_ms = 57;
        store
            .save_project_profile_policy(1, Some(project_a.clone()))
            .expect("save a");
        store
            .save_project_profile_policy(2, Some(project_b.clone()))
            .expect("save b");
        let reopened = NativeVmmResourceStore::new(root.path());
        assert_eq!(
            reopened
                .project_profile_policy_summary(1)
                .expect("project profile policy fixture value")
                .effective,
            project_a
        );
        assert_eq!(
            reopened
                .project_profile_policy_summary(2)
                .expect("project profile policy fixture value")
                .effective,
            project_b
        );
        let mut bundle = store
            .current_bundle("test-space".to_owned(), 3)
            .expect("global bundle");
        let personal = bundle.memory_policy.profiles.user.clone();
        store
            .apply_memory_project_profile_policy(&mut bundle, 1)
            .expect("apply a");
        assert_eq!(
            bundle.memory_policy.profiles.project,
            core_lifetimes(&project_a)
        );
        assert_eq!(bundle.memory_policy.profiles.user, personal);
        let mut invalid = project_a.clone();
        invalid.stable.base_ms = 0;
        assert!(store.save_project_profile_policy(1, Some(invalid)).is_err());
        assert_eq!(
            store
                .project_profile_policy_summary(1)
                .expect("project profile policy fixture value")
                .effective,
            project_a
        );
        store
            .save_project_profile_policy(1, None)
            .expect("inherit a");
        assert_eq!(
            store
                .project_profile_policy_summary(1)
                .expect("project profile policy fixture value"),
            initial
        );
        assert_eq!(
            store
                .project_profile_policy_summary(2)
                .expect("project profile policy fixture value")
                .effective,
            project_b
        );
        assert!(store.project_profile_policy_summary(0).is_err());
        assert!(store.save_project_profile_policy(0, None).is_err());
    }

    /// Rejects personal-profile fields and malformed rules rather than silently inheriting defaults.
    /// 拒绝个人画像字段与无效规则，而非静默继承默认值。
    #[test]
    fn malformed_project_profile_document_is_not_global_fallback() {
        // Write only through the production atomic persistence helper under an isolated root.
        // 仅通过生产原子持久化助手写入隔离根目录。
        let root = tempfile::tempdir().expect("isolated root");
        let store = NativeVmmResourceStore::new(root.path());
        let rules = store
            .project_profile_policy_summary(1)
            .expect("project profile policy fixture value")
            .global;
        for document in [
            serde_json::json!({"schema_version":2,"projects":{}}),
            serde_json::json!({"schema_version":1,"projects":{"0":rules}}),
            serde_json::json!({"schema_version":1,"projects":{},"user":rules}),
            serde_json::json!({"schema_version":1,"projects":{"1":{"stable":rules.stable}}}),
        ] {
            let bytes = serde_json::to_vec(&document).expect("document bytes");
            persist_file_atomically(
                &store.config_root,
                &store.config_root.join("memory-project-profiles.json"),
                &bytes,
            )
            .expect("write invalid document");
            assert!(store.project_profile_policy_summary(1).is_err());
        }
    }
}
