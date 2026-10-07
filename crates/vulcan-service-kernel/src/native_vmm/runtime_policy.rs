//! Explicitly persisted native business policy layered into immutable Core configurations.
//! 将显式持久化的原生业务策略叠加到不可变 Core 配置。

#[path = "host_project_policy.rs"]
mod host_project_policy;
#[path = "profile_policy.rs"]
mod profile_policy;
#[path = "project_profile_policy.rs"]
mod project_profile_policy;
#[path = "project_retention_policy.rs"]
mod project_retention_policy;
#[path = "retrieval_policy.rs"]
mod retrieval_policy;
use crate::native_vmm::retention_policy;

use super::{
    MAX_SETTINGS_BYTES, NativeVmmResourceStore, PersistedResourceSettings, config_bundle,
    embedded_paths, persist_file_atomically, read_optional_regular_file, validate_candidate_bundle,
    validate_settings,
};
use serde::{Deserialize, Serialize};
use vulcan_memorymesh_core::{
    ConfigBundle, MemoryLevel, MemoryMeshCore, MemoryPolicy, MemoryStability, ResourceLayer,
};
use vulcan_service_api::{
    VmmNativeExtractedMemoryLevel, VmmNativeExtractedMemoryStability,
    VmmNativeProfilePolicySummary, VmmNativeRecallPolicySummary, VmmNativeRetentionSettingsSummary,
    VmmNativeRetrievalPolicySummary, VmmNativeRuntimePolicySummary, VmmNativeTextPolicySummary,
};

/// Closed user document; resources remain independently owned by settings.json and file overrides.
/// 封闭的用户文档；资源仍由 settings.json 及文件覆盖独立持有。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedRuntimePolicy {
    /// Exact format version supported by this host.
    /// 当前宿主支持的准确格式版本。
    schema_version: u32,
    /// Recall and extraction fields that have concrete Core execution consumers.
    /// 具有准确 Core 执行消费者的召回与提炼字段。
    recall: VmmNativeRecallPolicySummary,
    /// Retrieval ranking has one owner in the same atomic user document.
    /// 检索排序在同一原子用户文档中具有唯一归属。
    retrieval: VmmNativeRetrievalPolicySummary,
    /// Cleanup rules share the single atomic document but execute without model routes.
    /// 清洗规则共用唯一原子文档，但执行不依赖模型路由。
    retention: VmmNativeRetentionSettingsSummary,
    /// Intake reduction rules have the same explicit user-document ownership.
    /// 入库缩减规则具有同一显式用户文档归属。
    text: VmmNativeTextPolicySummary,
    /// Both profile roles are configured independently in this same atomic user document.
    /// 两种画像角色在同一原子用户文档中独立配置。
    profiles: VmmNativeProfilePolicySummary,
}

/// Projects Core-owned text defaults without persisting or inferring a second default source.
/// 投影 Core 持有的文本默认值，不持久化或推断第二个默认来源。
fn default_text_policy() -> VmmNativeTextPolicySummary {
    let policy = vulcan_memorymesh_core::PostActionTextPolicy::default();
    VmmNativeTextPolicySummary {
        fold_machine_text: policy.fold_machine_text,
        max_fragment_tokens: policy.max_fragment_tokens,
    }
}

/// Maps the exact two public text controls into the immutable Core policy.
/// 将准确的两个公开文本控制项映射到不可变 Core 策略。
fn core_text_policy(
    text: &VmmNativeTextPolicySummary,
) -> vulcan_memorymesh_core::PostActionTextPolicy {
    vulcan_memorymesh_core::PostActionTextPolicy {
        fold_machine_text: text.fold_machine_text,
        max_fragment_tokens: text.max_fragment_tokens,
    }
}

/// Reads UI defaults from the same Core policy used for command execution.
/// 从命令执行使用的同一 Core 策略读取界面默认值。
fn default_recall_policy() -> VmmNativeRecallPolicySummary {
    // Keep default ownership in Core rather than maintaining a second set of UI constants.
    // 将默认值归属保留在 Core，不另外维护一套界面常量。
    let policy = MemoryPolicy::default();
    VmmNativeRecallPolicySummary {
        precheck_history_turn_limit: policy.precheck_history_turn_limit,
        precheck_max_input_tokens: policy.precheck_max_input_tokens,
        precheck_recall_top_k: policy.precheck_policy.recall_top_k,
        precheck_review_candidate_limit: policy.precheck_policy.review_candidate_limit,
        precheck_min_review_score: policy.precheck_policy.min_review_score,
        max_search_queries: policy.retrieval_policy.max_query_items,
        postaction_history_turn_limit: policy.postaction_history_turn_limit,
        postaction_max_input_tokens: policy.postaction_max_input_tokens,
        postaction_memory_priority: policy.postaction_memory_priority,
        postaction_memory_stability: match policy.postaction_memory_stability {
            MemoryStability::Working => VmmNativeExtractedMemoryStability::Working,
            MemoryStability::Stable => VmmNativeExtractedMemoryStability::Stable,
            MemoryStability::Archival => VmmNativeExtractedMemoryStability::Archival,
        },
        postaction_memory_level: match policy.postaction_memory_level {
            MemoryLevel::Session => VmmNativeExtractedMemoryLevel::Session,
            MemoryLevel::Phase => VmmNativeExtractedMemoryLevel::Phase,
            MemoryLevel::Stable => VmmNativeExtractedMemoryLevel::Stable,
            MemoryLevel::Persistent => VmmNativeExtractedMemoryLevel::Persistent,
        },
        postaction_memory_refresh_weight: policy.postaction_memory_refresh_weight,
        postaction_memory_expiry_ms: policy.postaction_memory_expiry_ms,
    }
}

/// Applies editable values without altering score producers, model routing, or resource composition.
/// 应用可编辑值，不改变评分生产者、模型路由或资源组合。
fn apply_recall_policy(policy: &mut MemoryPolicy, recall: &VmmNativeRecallPolicySummary) {
    policy.precheck_history_turn_limit = recall.precheck_history_turn_limit;
    policy.precheck_max_input_tokens = recall.precheck_max_input_tokens;
    policy.precheck_policy.recall_top_k = recall.precheck_recall_top_k;
    policy.precheck_policy.review_candidate_limit = recall.precheck_review_candidate_limit;
    policy.precheck_policy.min_review_score = recall.precheck_min_review_score;
    policy.retrieval_policy.max_query_items = recall.max_search_queries;
    policy.postaction_history_turn_limit = recall.postaction_history_turn_limit;
    policy.postaction_max_input_tokens = recall.postaction_max_input_tokens;
    policy.postaction_memory_priority = recall.postaction_memory_priority;
    policy.postaction_memory_stability = match recall.postaction_memory_stability {
        VmmNativeExtractedMemoryStability::Working => MemoryStability::Working,
        VmmNativeExtractedMemoryStability::Stable => MemoryStability::Stable,
        VmmNativeExtractedMemoryStability::Archival => MemoryStability::Archival,
    };
    policy.postaction_memory_level = match recall.postaction_memory_level {
        VmmNativeExtractedMemoryLevel::Session => MemoryLevel::Session,
        VmmNativeExtractedMemoryLevel::Phase => MemoryLevel::Phase,
        VmmNativeExtractedMemoryLevel::Stable => MemoryLevel::Stable,
        VmmNativeExtractedMemoryLevel::Persistent => MemoryLevel::Persistent,
    };
    policy.postaction_memory_refresh_weight = recall.postaction_memory_refresh_weight;
    policy.postaction_memory_expiry_ms = recall.postaction_memory_expiry_ms;
}

impl NativeVmmResourceStore {
    /// Loads the exact policy file or returns in-memory defaults without creating any file.
    /// 加载准确的策略文件；未保存时返回内存默认值，不创建任何文件。
    /// Returns an error for an unsafe path, unsupported schema, or invalid document shape.
    /// 路径不安全、Schema 不支持或文档结构错误时返回错误。
    fn load_runtime_policy(&self) -> Result<VmmNativeRuntimePolicySummary, String> {
        // The single filename is host-owned, never supplied by a maintenance request.
        // 唯一文件名由宿主持有，不由维护请求提供。
        let path = self.config_root.join("runtime-policy.json");
        let Some(bytes) = read_optional_regular_file(&self.config_root, &path, MAX_SETTINGS_BYTES)?
        else {
            return Ok(VmmNativeRuntimePolicySummary {
                recall: default_recall_policy(),
                defaults: default_recall_policy(),
                retrieval: retrieval_policy::defaults(),
                retrieval_defaults: retrieval_policy::defaults(),
                retention: retention_policy::defaults(),
                retention_defaults: retention_policy::defaults(),
                text: default_text_policy(),
                text_defaults: default_text_policy(),
                profiles: profile_policy::defaults(),
                profile_defaults: profile_policy::defaults(),
                user_override: false,
            });
        };
        let document: PersistedRuntimePolicy = serde_json::from_slice(&bytes)
            .map_err(|error| format!("解析原生 VMM 运行策略失败：{error}"))?;
        if document.schema_version != 1 {
            return Err(format!(
                "不支持的原生 VMM 运行策略版本：{}",
                document.schema_version
            ));
        }
        retention_policy::validate(&document.retention)?;
        Ok(VmmNativeRuntimePolicySummary {
            recall: document.recall,
            defaults: default_recall_policy(),
            retrieval: document.retrieval,
            retrieval_defaults: retrieval_policy::defaults(),
            retention: document.retention,
            retention_defaults: retention_policy::defaults(),
            text: document.text,
            text_defaults: default_text_policy(),
            profiles: document.profiles,
            profile_defaults: profile_policy::defaults(),
            user_override: true,
        })
    }

    /// Composes resources and the persisted policy for validation or the next command snapshot.
    /// 为校验或下一命令快照组合资源与持久化策略。
    /// Returns an error when the persisted policy cannot be read; Core validates all numeric bounds.
    /// 持久化策略不可读时返回错误；全部数值约束由 Core 校验。
    pub(super) fn compose_bundle(
        &self,
        settings: PersistedResourceSettings,
        user_layer: ResourceLayer,
        vector_space_fingerprint: String,
        embedding_dimension: u32,
    ) -> Result<ConfigBundle, String> {
        let policy = self.load_runtime_policy()?;
        let mut bundle = config_bundle(
            settings,
            user_layer,
            vector_space_fingerprint,
            embedding_dimension,
        );
        apply_recall_policy(&mut bundle.memory_policy, &policy.recall);
        bundle.memory_policy.postaction_text = core_text_policy(&policy.text);
        bundle.memory_policy.profiles = profile_policy::resolve(&policy.profiles);
        retrieval_policy::apply(
            &mut bundle.memory_policy.retrieval_policy,
            &policy.retrieval,
        )?;
        Ok(bundle)
    }

    /// Reads validated effective settings using the currently admitted vector configuration.
    /// 使用当前已准入的向量配置读取经过校验的有效设置。
    /// Returns an error instead of displaying corrupt user settings as defaults.
    /// 用户设置损坏时返回错误，不将其显示成默认值。
    pub(in crate::native_vmm) fn runtime_policy(
        &self,
        core: &MemoryMeshCore,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let policy = self.load_runtime_policy()?;
        self.validate_runtime_candidate(
            core,
            &policy.recall,
            &policy.retrieval,
            &policy.text,
            &policy.profiles,
        )?;
        Ok(policy)
    }

    /// Validates explicit edits, atomically saves only the user document, and activates a new snapshot.
    /// 校验显式编辑，仅原子保存用户文档，并激活新快照。
    /// Invalid values or I/O failures return an error without accepting a silently clamped policy.
    /// 无效值或 I/O 失败时返回错误，不接纳静默截断的策略。
    pub(in crate::native_vmm) fn update_runtime_policy(
        &self,
        core: &MemoryMeshCore,
        recall: VmmNativeRecallPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        // Read the other group under the caller's configuration gate, preserving its current saved values.
        // 在调用者的配置锁内读取另一组，保留其当前保存值。
        let current = self.load_runtime_policy()?;
        self.save_runtime_policy(
            core,
            recall,
            current.retrieval,
            current.retention,
            current.text,
            current.profiles,
        )
    }

    /// Saves ranking edits without replacing concurrently serialized recall defaults.
    /// 保存排序编辑，不替换在同一配置锁下串行保存的召回默认值。
    /// Returns the complete saved policy or a validation/persistence error.
    /// 返回完整已保存策略或校验/持久化错误。
    pub(in crate::native_vmm) fn update_retrieval_policy(
        &self,
        core: &MemoryMeshCore,
        retrieval: VmmNativeRetrievalPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let current = self.load_runtime_policy()?;
        self.save_runtime_policy(
            core,
            current.recall,
            retrieval,
            current.retention,
            current.text,
            current.profiles,
        )
    }

    /// Reads cleanup settings without compiling prompts or looking up model capabilities.
    /// 读取清洗设置，不编译提示词或查询模型能力。
    /// Returns validated settings or the exact document read error.
    /// 返回经过校验的设置或准确的文档读取错误。
    pub(in crate::native_vmm) fn retention_settings(
        &self,
    ) -> Result<VmmNativeRetentionSettingsSummary, String> {
        Ok(self.load_runtime_policy()?.retention)
    }

    /// Saves explicit cleanup edits while preserving the other groups under the caller's gate.
    /// 在调用者锁内保存显式清洗修改并保留其他配置组。
    /// Returns the complete effective policy or a validation or persistence failure.
    /// 返回完整有效策略或校验、持久化失败。
    pub(in crate::native_vmm) fn update_retention_settings(
        &self,
        core: &MemoryMeshCore,
        retention: VmmNativeRetentionSettingsSummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let current = self.load_runtime_policy()?;
        self.save_runtime_policy(
            core,
            current.recall,
            current.retrieval,
            retention,
            current.text,
            current.profiles,
        )
    }

    /// Saves explicit text settings while retaining every other business group under the caller's gate.
    /// 在调用者锁内保存显式文本设置，并保留其他全部业务组。
    /// Returns the accepted complete policy or an error before accepting invalid text rules.
    /// 返回已接纳的完整策略，或在接纳非法文本规则前返回错误。
    pub(in crate::native_vmm) fn update_text_policy(
        &self,
        core: &MemoryMeshCore,
        text: VmmNativeTextPolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let current = self.load_runtime_policy()?;
        self.save_runtime_policy(
            core,
            current.recall,
            current.retrieval,
            current.retention,
            text,
            current.profiles,
        )
    }

    /// Saves global role-specific profile lifetimes while preserving every unrelated group.
    /// 保存全局角色专属画像生命周期，同时保留全部无关配置组。
    /// Returns accepted settings or a validation/persistence error; existing profile deadlines are not rewritten.
    /// 返回接纳设置或校验/持久化错误；不改写既有画像截止时间。
    pub(in crate::native_vmm) fn update_profile_policy(
        &self,
        core: &MemoryMeshCore,
        profiles: VmmNativeProfilePolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        let current = self.load_runtime_policy()?;
        self.save_runtime_policy(
            core,
            current.recall,
            current.retrieval,
            current.retention,
            current.text,
            profiles,
        )
    }

    /// Validates all business groups and persists them together before activating the next snapshot.
    /// 同时校验全部业务组，一并持久化后激活下一快照。
    /// Returns the complete effective policy or an explicit error; built-in resources are never rewritten.
    /// 返回完整有效策略或明确错误；绝不改写内置资源。
    fn save_runtime_policy(
        &self,
        core: &MemoryMeshCore,
        recall: VmmNativeRecallPolicySummary,
        retrieval: VmmNativeRetrievalPolicySummary,
        retention: VmmNativeRetentionSettingsSummary,
        text: VmmNativeTextPolicySummary,
        profiles: VmmNativeProfilePolicySummary,
    ) -> Result<VmmNativeRuntimePolicySummary, String> {
        retention_policy::validate(&retention)?;
        self.validate_runtime_candidate(core, &recall, &retrieval, &text, &profiles)?;
        let document = PersistedRuntimePolicy {
            schema_version: 1,
            recall,
            retrieval,
            retention,
            text,
            profiles,
        };
        let bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("序列化原生 VMM 运行策略失败：{error}"))?;
        persist_file_atomically(
            &self.config_root,
            &self.config_root.join("runtime-policy.json"),
            &bytes,
        )?;
        self.activate_current(core)?;
        Ok(VmmNativeRuntimePolicySummary {
            recall: document.recall,
            defaults: default_recall_policy(),
            retrieval: document.retrieval,
            retrieval_defaults: retrieval_policy::defaults(),
            retention: document.retention,
            retention_defaults: retention_policy::defaults(),
            text: document.text,
            text_defaults: default_text_policy(),
            profiles: document.profiles,
            profile_defaults: profile_policy::defaults(),
            user_override: true,
        })
    }

    /// Compiles candidate business values with actual resource overrides and exact score contracts.
    /// 将候选业务值与实际资源覆盖及准确评分契约共同编译。
    /// Returns validation errors before persistence or active-snapshot mutation.
    /// 在持久化或修改活动快照前返回校验错误。
    fn validate_runtime_candidate(
        &self,
        core: &MemoryMeshCore,
        recall: &VmmNativeRecallPolicySummary,
        retrieval: &VmmNativeRetrievalPolicySummary,
        text: &VmmNativeTextPolicySummary,
        profiles: &VmmNativeProfilePolicySummary,
    ) -> Result<(), String> {
        let paths = embedded_paths()?;
        let settings = self.load_settings()?;
        validate_settings(&settings, &paths)?;
        let user_layer = self.load_user_layer(&paths)?;
        let active = core.active_snapshot().map_err(|error| error.to_string())?;
        let mut bundle = config_bundle(
            settings,
            user_layer,
            active.vector_space_fingerprint.clone(),
            active.embedding_dimension,
        );
        apply_recall_policy(&mut bundle.memory_policy, recall);
        bundle.memory_policy.postaction_text = core_text_policy(text);
        bundle.memory_policy.profiles = profile_policy::resolve(profiles);
        retrieval_policy::apply(&mut bundle.memory_policy.retrieval_policy, retrieval)?;
        validate_candidate_bundle(bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks text save/reload, sibling preservation, strict rejection and immutable previously admitted values.
    /// 检查文本保存/重载、相邻组保留、严格拒绝及此前准入值不可变。
    #[test]
    fn text_policy_save_is_isolated_and_rejects_invalid_edits() {
        let directory = tempfile::tempdir().expect("temporary user root");
        let store = NativeVmmResourceStore::new(directory.path());
        let core = MemoryMeshCore::new().expect("core");
        let initial = store.runtime_policy(&core).expect("defaults");
        let frozen = core.active_snapshot().expect("frozen default");
        assert!(!directory.path().join("config").exists());
        let edited = VmmNativeTextPolicySummary {
            fold_machine_text: false,
            max_fragment_tokens: 256,
        };
        let saved = store
            .update_text_policy(&core, edited.clone())
            .expect("save text");
        assert_eq!(saved.text, edited);
        assert_eq!(saved.recall, initial.recall);
        assert_eq!(saved.retrieval, initial.retrieval);
        assert_eq!(saved.retention, initial.retention);
        assert_eq!(
            frozen.memory_policy.postaction_text,
            vulcan_memorymesh_core::PostActionTextPolicy::default()
        );
        assert_eq!(
            core.active_snapshot()
                .expect("new snapshot")
                .memory_policy
                .postaction_text,
            core_text_policy(&edited)
        );
        assert_eq!(
            NativeVmmResourceStore::new(directory.path())
                .runtime_policy(&core)
                .expect("reopen")
                .text,
            edited
        );
        store
            .update_runtime_policy(&core, saved.recall)
            .expect("save recall");
        store
            .update_retrieval_policy(&core, saved.retrieval)
            .expect("save ranking");
        store
            .update_retention_settings(&core, saved.retention)
            .expect("save cleanup");
        assert_eq!(store.runtime_policy(&core).expect("preserved").text, edited);
        let path = directory.path().join("config/runtime-policy.json");
        let bytes = std::fs::read(&path).expect("saved bytes");
        assert!(
            store
                .update_text_policy(
                    &core,
                    VmmNativeTextPolicySummary {
                        max_fragment_tokens: 255,
                        ..edited
                    }
                )
                .is_err()
        );
        assert_eq!(std::fs::read(&path).expect("unchanged bytes"), bytes);
        // Semantic text errors must reject intake configuration without disabling independent cleanup reads.
        // 文本语义错误必须拒绝入库配置，但不关闭独立清洗设置读取。
        let mut invalid_document: serde_json::Value =
            serde_json::from_slice(&bytes).expect("valid document");
        invalid_document["text"]["maxFragmentTokens"] = serde_json::json!(255);
        std::fs::write(
            &path,
            serde_json::to_vec(&invalid_document).expect("invalid fixture"),
        )
        .expect("write isolated fixture");
        assert!(store.runtime_policy(&core).is_err());
        assert_eq!(
            store
                .retention_settings()
                .expect("independent cleanup read"),
            initial.retention
        );
        assert!(serde_json::from_value::<VmmNativeTextPolicySummary>(serde_json::json!({"foldMachineText":true,"maxFragmentTokens":1200,"disablePii":true})).is_err());
    }

    /// Verifies explicit cleanup saves, independent reads, group preservation, and immutable admitted values.
    /// 验证显式清洗保存、独立读取、配置组保留及已准入值不可变。
    #[test]
    fn cleanup_policy_save_preserves_other_groups_and_freezes_admission() {
        let directory = tempfile::tempdir().expect("temporary VMM root");
        let store = NativeVmmResourceStore::new(directory.path());
        let core = MemoryMeshCore::new().expect("core");
        let before = store.runtime_policy(&core).expect("defaults");
        assert!(!directory.path().join("config").exists());
        let frozen =
            retention_policy::resolve(&store.retention_settings().expect("read")).expect("admit");
        let mut edited = before.retention.clone();
        edited.enabled = false;
        edited.scan_interval_minutes = 7;
        edited.profile_expiry_interval_seconds = 11;
        edited.turn_hot_window_size = 3;
        edited.session_idle_ms = 100;
        edited.trash_retention_ms = 200;
        edited.protected_priority_floor = 73;
        edited.protected_memory_level_floor = 4;
        edited.skip_protected_shared_memories = false;
        let saved = store
            .update_retention_settings(&core, edited.clone())
            .expect("save");
        assert_eq!(saved.recall, before.recall);
        assert_eq!(saved.retrieval, before.retrieval);
        assert_eq!(saved.retention, edited);
        assert_eq!(frozen, vulcan_memorymesh_core::RetentionPolicy::v1());
        let reopened = NativeVmmResourceStore::new(directory.path());
        assert_eq!(reopened.retention_settings().expect("reopen"), edited);
        store
            .update_runtime_policy(&core, before.recall)
            .expect("recall save");
        store
            .update_retrieval_policy(&core, before.retrieval)
            .expect("ranking save");
        assert_eq!(
            reopened
                .retention_settings()
                .expect("other saves preserve cleanup"),
            edited
        );
        let path = directory.path().join("config/runtime-policy.json");
        let bytes = std::fs::read(&path).expect("saved bytes");
        edited.trash_retention_ms = 0;
        assert!(store.update_retention_settings(&core, edited).is_err());
        assert_eq!(std::fs::read(path).expect("unchanged file"), bytes);
    }

    /// Verifies no implicit writes, exact persisted values, hot reload, and resource-edit preservation.
    /// 验证不隐式写入、准确持久化值、热加载以及资源编辑不丢失策略。
    #[test]
    fn explicit_policy_save_controls_the_next_snapshot() {
        let directory = tempfile::tempdir().expect("temporary VMM root");
        let store = NativeVmmResourceStore::new(directory.path());
        let core = MemoryMeshCore::new().expect("core");
        let defaults = store.runtime_policy(&core).expect("defaults");
        assert!(!defaults.user_override);
        assert!(!directory.path().join("config").exists());
        let original = core.active_snapshot().expect("original snapshot");
        let mut recall = defaults.recall;
        recall.precheck_history_turn_limit = 2;
        recall.precheck_max_input_tokens = 1024;
        recall.precheck_recall_top_k = 7;
        recall.precheck_review_candidate_limit = 19;
        recall.precheck_min_review_score = 0.6;
        recall.max_search_queries = 6;
        recall.postaction_history_turn_limit = 5;
        recall.postaction_max_input_tokens = 4096;
        recall.postaction_memory_priority = 73;
        recall.postaction_memory_stability = VmmNativeExtractedMemoryStability::Archival;
        recall.postaction_memory_level = VmmNativeExtractedMemoryLevel::Persistent;
        recall.postaction_memory_refresh_weight = 9;
        recall.postaction_memory_expiry_ms = 123_456;
        let saved = store
            .update_runtime_policy(&core, recall.clone())
            .expect("save");
        assert!(saved.user_override);
        assert_eq!(saved.recall, recall);
        assert!(!directory.path().join("config/settings.json").exists());
        let active = core.active_snapshot().expect("new snapshot");
        assert_ne!(original.id, active.id);
        assert_eq!(original.memory_policy.precheck_history_turn_limit, 3);
        assert_eq!(original.memory_policy.postaction_memory_priority, 20);
        assert_eq!(active.memory_policy.postaction_memory_priority, 73);
        assert_eq!(
            active.memory_policy.postaction_memory_stability,
            MemoryStability::Archival
        );
        assert_eq!(
            active.memory_policy.postaction_memory_level,
            MemoryLevel::Persistent
        );
        assert_eq!(active.memory_policy.postaction_memory_refresh_weight, 9);
        assert_eq!(active.memory_policy.postaction_memory_expiry_ms, 123_456);
        let mut expected = MemoryPolicy::default();
        apply_recall_policy(&mut expected, &recall);
        expected.retrieval_policy.lexical_contract = crate::native_vmm::lexical::score_contract();
        assert_eq!(active.memory_policy, expected);
        let reopened = NativeVmmResourceStore::new(directory.path());
        assert_eq!(reopened.runtime_policy(&core).expect("reopen"), saved);
        let resource_settings = store.catalog().expect("catalog").settings;
        store
            .update_settings(&core, resource_settings)
            .expect("resource save");
        assert_eq!(
            core.active_snapshot()
                .expect("resource snapshot")
                .memory_policy,
            expected
        );
        let next = reopened
            .current_bundle("next-vector-generation".to_owned(), 2)
            .expect("next admission");
        assert_eq!(next.memory_policy, expected);
        assert_eq!(next.embedding_dimension, 2);
    }

    /// Invalid edits preserve both the active snapshot and the exact persisted file bytes.
    /// 无效编辑保留活动快照及准确的持久化文件字节。
    #[test]
    fn invalid_policy_is_not_saved_or_silently_replaced() {
        let directory = tempfile::tempdir().expect("temporary VMM root");
        let store = NativeVmmResourceStore::new(directory.path());
        let core = MemoryMeshCore::new().expect("core");
        let saved = store
            .update_runtime_policy(&core, default_recall_policy())
            .expect("save");
        let path = directory.path().join("config/runtime-policy.json");
        let bytes = std::fs::read(&path).expect("saved bytes");
        let snapshot_id = core.active_snapshot().expect("snapshot").id.clone();
        // Every rejected extraction field must leave both persistent and admitted policy untouched.
        // 每个被拒绝的提炼字段都必须保持持久策略和已准入策略不变。
        for (field, value) in [
            ("postactionMemoryPriority", serde_json::json!(101)),
            ("postactionMemoryRefreshWeight", serde_json::json!(0)),
            (
                "postactionMemoryRefreshWeight",
                serde_json::json!(1_000_001),
            ),
            ("postactionMemoryExpiryMs", serde_json::json!(0)),
            (
                "postactionMemoryExpiryMs",
                serde_json::json!(315_360_000_001_u64),
            ),
            ("postactionMemoryStability", serde_json::json!("unknown")),
            ("postactionMemoryLevel", serde_json::json!("unknown")),
        ] {
            let mut candidate = serde_json::to_value(&saved.recall).expect("serialize policy");
            assert!(
                candidate.get(field).is_some(),
                "exact declared field: {field}"
            );
            candidate[field] = value;
            if let Ok(candidate) = serde_json::from_value::<VmmNativeRecallPolicySummary>(candidate)
            {
                assert!(
                    store.update_runtime_policy(&core, candidate).is_err(),
                    "{field}"
                );
            }
            assert_eq!(std::fs::read(&path).expect("unchanged bytes"), bytes);
            assert_eq!(
                core.active_snapshot().expect("unchanged snapshot").id,
                snapshot_id
            );
        }
        let mut invalid = saved.recall;
        invalid.precheck_recall_top_k = 33;
        assert!(store.update_runtime_policy(&core, invalid).is_err());
        assert_eq!(std::fs::read(&path).expect("unchanged bytes"), bytes);
        assert_eq!(
            core.active_snapshot().expect("unchanged snapshot").id,
            snapshot_id
        );
        let invalid_json = serde_json::json!({ "schema_version": 999, "recall": default_recall_policy(), "retrieval": retrieval_policy::defaults() });
        persist_file_atomically(
            &store.config_root,
            &path,
            &serde_json::to_vec(&invalid_json).expect("json"),
        )
        .expect("invalid fixture");
        assert!(store.runtime_policy(&core).is_err());
        assert!(store.current_bundle("test".to_owned(), 2).is_err());
    }

    /// Verifies both groups survive independent saves and every ranking field reaches the snapshot exactly.
    /// 验证两组独立保存时互相保留，且全部排序字段准确进入快照。
    #[test]
    fn ranking_save_preserves_recall_and_exact_score_contracts() {
        let directory = tempfile::tempdir().expect("temporary VMM root");
        let store = NativeVmmResourceStore::new(directory.path());
        let core = MemoryMeshCore::new().expect("core");
        let mut recall = default_recall_policy();
        recall.precheck_history_turn_limit = 7;
        store
            .update_runtime_policy(&core, recall.clone())
            .expect("recall save");
        let mut ranking = retrieval_policy::defaults();
        ranking.hybrid_enabled = false;
        ranking.lexical_candidate_floor = 12;
        ranking.rrf_k = "9223372036854775807".to_owned();
        ranking.replace_min_similarity = 0.6;
        ranking.hard_dedupe_cosine_threshold = 0.0;
        ranking.hard_dedupe_pool_top_k = 17;
        ranking.replacement_top_k = 6;
        ranking.rerank_top_n = 0;
        ranking.mmr_enabled = false;
        ranking.mmr_lambda = 0.456789;
        ranking.weibull_enabled = false;
        ranking.weibull_shape = 2.1;
        ranking.weibull_scale_hours = 120.25;
        ranking.weibull_min_multiplier = 0.12345;
        ranking.weibull_reinforce_weight = 0.3333;
        ranking.weibull_cross_session_boost = 0.2222;
        let saved = store
            .update_retrieval_policy(&core, ranking.clone())
            .expect("ranking save");
        assert_eq!(saved.recall, recall);
        assert_eq!(saved.retrieval, ranking);
        recall.precheck_max_input_tokens = 2048;
        let updated = store
            .update_runtime_policy(&core, recall)
            .expect("second recall save");
        assert_eq!(updated.retrieval, ranking);
        let reopened = NativeVmmResourceStore::new(directory.path());
        assert_eq!(reopened.runtime_policy(&core).expect("reopen"), updated);
        let actual = &core
            .active_snapshot()
            .expect("active")
            .memory_policy
            .retrieval_policy;
        assert!(!actual.hybrid_enabled);
        assert_eq!(actual.lexical_candidate_floor, 12);
        assert_eq!(actual.rrf_k, i64::MAX as u64);
        assert_eq!(actual.replace_min_similarity[0].threshold, 0.6);
        assert_eq!(actual.hard_dedupe[0].threshold, 0.0);
        assert_eq!(
            actual.replace_min_similarity[0].contract,
            actual.vector_contract.contract
        );
        assert_eq!(
            actual.hard_dedupe[0].contract,
            actual.vector_contract.contract
        );
        assert_eq!(
            actual.lexical_contract,
            crate::native_vmm::lexical::score_contract()
        );
        assert!(actual.min_similarity.is_empty());
        assert_eq!(
            (
                actual.hard_dedupe_pool_top_k,
                actual.replacement_top_k,
                actual.rerank_top_n
            ),
            (17, 6, 0)
        );
        assert!(!actual.mmr.enabled);
        assert_eq!(actual.mmr.lambda, 0.456789);
        assert!(!actual.decay.enabled);
        assert_eq!(actual.decay.shape, 2.1);
        assert_eq!(actual.decay.scale_hours, 120.25);
        assert_eq!(actual.decay.min_multiplier, 0.12345);
        assert_eq!(actual.decay.reinforce_weight, 0.3333);
        assert_eq!(actual.decay.cross_session_boost, 0.2222);
        assert_eq!(
            actual.decay.level_scales,
            vulcan_memorymesh_core::RetrievalPolicy::v1()
                .decay
                .level_scales
        );
    }

    /// Role-specific profile edits survive sibling saves and reject invalid writes without altering disk or snapshots.
    /// 角色专属画像修改在相邻组保存后保留，非法写入不改变磁盘或快照。
    #[test]
    fn profile_policy_roundtrip_preserves_siblings_and_rejects_invalid_rules() {
        let directory = tempfile::tempdir().expect("temporary VMM root");
        let store = NativeVmmResourceStore::new(directory.path());
        let core = MemoryMeshCore::new().expect("core");
        let original = store.runtime_policy(&core).expect("defaults");
        let frozen = core.active_snapshot().expect("frozen defaults");
        let mut profiles = original.profiles.clone();
        profiles.user.stable.base_ms = 123;
        profiles.project.transient.step_ms = 0;
        let saved = store
            .update_profile_policy(&core, profiles.clone())
            .expect("profile save");
        assert_eq!(saved.profiles, profiles);
        assert_eq!(saved.recall, original.recall);
        assert_eq!(saved.retrieval, original.retrieval);
        assert_eq!(saved.retention, original.retention);
        assert_eq!(saved.text, original.text);
        assert_eq!(
            frozen.memory_policy.profiles,
            vulcan_memorymesh_core::ProfilePolicy::default()
        );
        assert_eq!(
            core.active_snapshot()
                .expect("active")
                .memory_policy
                .profiles,
            profile_policy::resolve(&profiles)
        );
        let mut recall = saved.recall;
        recall.precheck_history_turn_limit = 2;
        let saved = store
            .update_runtime_policy(&core, recall)
            .expect("recall save");
        assert_eq!(saved.profiles, profiles);
        let saved = store
            .update_retrieval_policy(&core, saved.retrieval)
            .expect("ranking save");
        assert_eq!(saved.profiles, profiles);
        let saved = store
            .update_retention_settings(&core, saved.retention)
            .expect("cleanup save");
        assert_eq!(saved.profiles, profiles);
        let saved = store
            .update_text_policy(&core, saved.text)
            .expect("text save");
        assert_eq!(saved.profiles, profiles);
        let reopened = NativeVmmResourceStore::new(directory.path());
        assert_eq!(reopened.runtime_policy(&core).expect("reopened"), saved);
        let path = directory.path().join("config/runtime-policy.json");
        let bytes = std::fs::read(&path).expect("bytes");
        let id = core.active_snapshot().expect("snapshot").id.clone();
        profiles.user.stable.base_ms = 0;
        assert!(store.update_profile_policy(&core, profiles).is_err());
        assert_eq!(std::fs::read(&path).expect("unchanged bytes"), bytes);
        assert_eq!(core.active_snapshot().expect("unchanged snapshot").id, id);
    }

    /// Invalid ranking values fail before changing disk or the active snapshot, including disabled-stage values.
    /// 无效排序值在修改磁盘或活动快照前失败，包括已关闭阶段的参数。
    #[test]
    fn invalid_ranking_changes_are_rejected_without_rounding() {
        let directory = tempfile::tempdir().expect("temporary VMM root");
        let store = NativeVmmResourceStore::new(directory.path());
        let core = MemoryMeshCore::new().expect("core");
        let saved = store
            .update_retrieval_policy(&core, retrieval_policy::defaults())
            .expect("save");
        let path = directory.path().join("config/runtime-policy.json");
        let bytes = std::fs::read(&path).expect("bytes");
        let id = core.active_snapshot().expect("snapshot").id.clone();
        for text in ["0", "01", "+1", " 1", "1.5", "9223372036854775808"] {
            let mut invalid = saved.retrieval.clone();
            invalid.rrf_k = text.to_owned();
            assert!(
                store.update_retrieval_policy(&core, invalid).is_err(),
                "{text}"
            );
        }
        for invalid in [
            VmmNativeRetrievalPolicySummary {
                rerank_top_n: 33,
                ..saved.retrieval.clone()
            },
            VmmNativeRetrievalPolicySummary {
                lexical_candidate_floor: 0,
                ..saved.retrieval.clone()
            },
            VmmNativeRetrievalPolicySummary {
                mmr_enabled: false,
                mmr_lambda: 1.1,
                ..saved.retrieval.clone()
            },
            VmmNativeRetrievalPolicySummary {
                weibull_shape: 0.0,
                ..saved.retrieval.clone()
            },
            VmmNativeRetrievalPolicySummary {
                weibull_scale_hours: f64::NAN,
                ..saved.retrieval.clone()
            },
            VmmNativeRetrievalPolicySummary {
                replace_min_similarity: -0.1,
                ..saved.retrieval.clone()
            },
            VmmNativeRetrievalPolicySummary {
                hard_dedupe_cosine_threshold: 1.1,
                ..saved.retrieval
            },
        ] {
            assert!(store.update_retrieval_policy(&core, invalid).is_err());
        }
        assert_eq!(std::fs::read(&path).expect("unchanged"), bytes);
        assert_eq!(core.active_snapshot().expect("unchanged snapshot").id, id);
    }
}
