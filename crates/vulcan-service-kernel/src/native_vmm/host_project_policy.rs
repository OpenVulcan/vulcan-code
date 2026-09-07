//! Caller-owned native overrides, separate from memory-project retention and profile ownership.
//! 调用方持有的原生覆盖，独立于记忆项目的保留与画像归属。

#[cfg(test)]
#[path = "host_project_policy_tests.rs"]
mod tests;

use super::super::{MAX_SETTINGS_BYTES, persist_file_atomically, read_optional_regular_file};
use super::{NativeVmmResourceStore, apply_recall_policy, core_text_policy, retrieval_policy};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vulcan_memorymesh_core::{ConfigBundle, MemoryMeshCore};
use vulcan_service_api::VmmNativeHostProjectPolicyOverrides;

/// Closed document keyed by the exact host project tag; names and memory-project IDs are not keys.
/// 以准确宿主项目标签为键的封闭文档；名称与记忆项目 ID 不作为键。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostProjectPolicies {
    /// Exact supported user-document version.
    /// 准确支持的用户文档版本。
    schema_version: u32,
    /// Independently optional groups for each host project.
    /// 每个宿主项目的独立可选配置组。
    projects: BTreeMap<String, VmmNativeHostProjectPolicyOverrides>,
}

/// Applies only declared caller groups; global fixed-user and memory-owner rules cannot be replaced here.
/// 仅应用声明的调用方配置组；不能在此替换全局固定用户与记忆归属规则。
fn apply(
    bundle: &mut ConfigBundle,
    overrides: &VmmNativeHostProjectPolicyOverrides,
) -> Result<(), String> {
    if let Some(recall) = &overrides.recall {
        apply_recall_policy(&mut bundle.memory_policy, recall);
    }
    if let Some(retrieval) = &overrides.retrieval {
        retrieval_policy::apply(&mut bundle.memory_policy.retrieval_policy, retrieval)?;
    }
    if let Some(text) = &overrides.text {
        bundle.memory_policy.postaction_text = core_text_policy(text);
    }
    Ok(())
}

impl NativeVmmResourceStore {
    /// Projects current effective values while preserving explicit group inheritance metadata.
    /// 投影当前有效值，同时保留显式配置组继承元数据。
    pub(in crate::native_vmm) fn host_project_policy_summary(
        &self,
        core: &MemoryMeshCore,
        host_project_id: &str,
    ) -> Result<vulcan_service_api::VmmNativeHostProjectPolicySummary, String> {
        let overrides = self.host_project_policy(host_project_id)?;
        let mut effective = self.load_runtime_policy()?;
        if let Some(recall) = &overrides.recall {
            effective.recall = recall.clone();
        }
        if let Some(retrieval) = &overrides.retrieval {
            effective.retrieval = retrieval.clone();
        }
        if let Some(text) = &overrides.text {
            effective.text = text.clone();
        }
        self.validate_runtime_candidate(
            core,
            &effective.recall,
            &effective.retrieval,
            &effective.text,
            &effective.profiles,
        )?;
        Ok(vulcan_service_api::VmmNativeHostProjectPolicySummary {
            host_project_id: host_project_id.to_owned(),
            overrides,
            effective,
        })
    }

    /// Reads the sole caller-override document without creating defaults or probing candidate paths.
    /// 读取唯一调用方覆盖文档，不创建默认文件，也不探测候选路径。
    fn load_host_project_policies(&self) -> Result<HostProjectPolicies, String> {
        let path = self.config_root.join("host-project-policies.json");
        let Some(bytes) = read_optional_regular_file(&self.config_root, &path, MAX_SETTINGS_BYTES)?
        else {
            return Ok(HostProjectPolicies {
                schema_version: 1,
                projects: BTreeMap::new(),
            });
        };
        let document: HostProjectPolicies = serde_json::from_slice(&bytes)
            .map_err(|error| format!("解析原生宿主项目策略失败：{error}"))?;
        if document.schema_version != 1 {
            return Err("不支持的原生宿主项目策略版本".to_owned());
        }
        if document
            .projects
            .keys()
            .any(|key| key.trim().is_empty() || key.trim() != key)
        {
            return Err("原生宿主项目策略包含非法项目标签".to_owned());
        }
        Ok(document)
    }

    /// Returns the exact saved override or explicit inheritance for an unconfigured host project.
    /// 返回准确已保存覆盖；未配置的宿主项目返回显式继承。
    pub(in crate::native_vmm) fn host_project_policy(
        &self,
        host_project_id: &str,
    ) -> Result<VmmNativeHostProjectPolicyOverrides, String> {
        if host_project_id.trim().is_empty() || host_project_id.trim() != host_project_id {
            return Err("原生宿主项目策略需要准确项目标签".to_owned());
        }
        Ok(self
            .load_host_project_policies()?
            .projects
            .remove(host_project_id)
            .unwrap_or_default())
    }

    /// Builds a command bundle from current globals and the exact originating host project's override.
    /// 从当前全局配置和准确发起宿主项目覆盖构建命令配置。
    /// Empty host identity is the documented maintenance/global-only request path, never inferred from a memory ID.
    /// 空宿主身份是已定义的维护/仅全局请求路径，绝不从记忆 ID 推断宿主身份。
    pub(in crate::native_vmm) fn current_host_project_bundle(
        &self,
        fingerprint: String,
        dimension: u32,
        host_project_id: &str,
    ) -> Result<ConfigBundle, String> {
        let mut bundle = self.current_bundle(fingerprint, dimension)?;
        if !host_project_id.is_empty() {
            apply(&mut bundle, &self.host_project_policy(host_project_id)?)?;
        }
        Ok(bundle)
    }

    /// Validates and persists one explicit caller override without altering global rules or active commands.
    /// 校验并持久化一份显式调用方覆盖，不改变全局规则或已准入命令。
    /// The caller must validate host-project existence and hold the shared configuration gate.
    /// 调用者必须校验宿主项目存在，并持有共享配置锁。
    pub(in crate::native_vmm) fn update_host_project_policy(
        &self,
        core: &MemoryMeshCore,
        host_project_id: &str,
        overrides: VmmNativeHostProjectPolicyOverrides,
    ) -> Result<VmmNativeHostProjectPolicyOverrides, String> {
        self.host_project_policy(host_project_id)?;
        let active = core.active_snapshot().map_err(|error| error.to_string())?;
        let mut bundle = self.current_bundle(
            active.vector_space_fingerprint.clone(),
            active.embedding_dimension,
        )?;
        apply(&mut bundle, &overrides)?;
        super::super::validate_candidate_bundle(bundle)?;
        let mut document = self.load_host_project_policies()?;
        if overrides == VmmNativeHostProjectPolicyOverrides::default() {
            document.projects.remove(host_project_id);
        } else {
            document
                .projects
                .insert(host_project_id.to_owned(), overrides.clone());
        }
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("序列化原生宿主项目策略失败：{error}"))?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err("原生宿主项目策略文档超过大小上限".to_owned());
        }
        persist_file_atomically(
            &self.config_root,
            &self.config_root.join("host-project-policies.json"),
            &bytes,
        )?;
        Ok(overrides)
    }
}
