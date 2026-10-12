//! Persisted permission-judge settings: the model is chosen per machine, the policy per project.
//! 持久化的权限判定设置：模型按机器选择，策略按项目选择。
//!
//! Nothing here is written unless the user saves it; the judge is configured once a backend is
//! saved, and auto-approve mode is available only then.
//! 只有用户保存时才写入；保存后端后判定器即为已配置，自动审批模式也只在此时可用。

use super::*;
pub use vulcan_permission_judge::protocol::JudgeDevice;
pub use vulcan_permission_judge::system::JudgeBackend;

/// Returns whether a persisted boolean is false so the default stays out of JSON.
/// 判断持久化布尔值是否为 false，使默认值不写入 JSON。
fn is_false(value: &bool) -> bool {
    !*value
}

/// One registered local model package; the files stay where the user keeps them.
/// 一条已登记的本机模型包；文件保留在用户存放的位置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisteredJudgeModel {
    /// Digest of the package manifest, which pins every file; also the registration identity.
    /// 固定每个文件的模型包清单摘要，同时是登记身份。
    pub id: String,
    /// Name given by the user.
    /// 用户给出的名称。
    pub name: String,
    /// Absolute package directory.
    /// 绝对模型包目录。
    pub path: String,
    /// Official package the checkpoint belongs to; absent for a custom model.
    /// 检查点所属的官方模型包；自定义模型缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_package_id: Option<String>,
    /// Digest of the trained checkpoint the package was converted from.
    /// 模型包转换来源的训练检查点摘要。
    pub parent_sha256: String,
    /// Policy revision the weights were trained with.
    /// 权重训练所用的策略修订。
    pub policy_version: String,
    /// Weight precision.
    /// 权重精度。
    pub precision: String,
    /// Total size of the package files.
    /// 模型包文件总大小。
    pub total_bytes: u64,
    /// Registration time in Unix milliseconds.
    /// 登记时间（Unix 毫秒）。
    pub registered_at_ms: u64,
}

/// System-level judge settings stored in the user's global configuration.
/// 存放在用户全局配置中的系统级判定设置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedPermissionJudgeConfiguration {
    /// Configured backend; absent until the user finishes setting one up.
    /// 已配置的后端；用户完成设置前缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<JudgeBackend>,
    /// Device chosen by the user; absent means the recommended device at load time.
    /// 用户选择的设备；缺省表示加载时使用推荐设备。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<JudgeDevice>,
    /// CUDA library directory locked when the user chose CUDA.
    /// 用户选择 CUDA 时锁定的 CUDA 库目录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda_library_directory: Option<String>,
    /// Registered local model packages.
    /// 已登记的本机模型包。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<RegisteredJudgeModel>,
    /// Registration in use.
    /// 使用中的登记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model_id: Option<String>,
    /// Registration in use before the last switch, for one-step rollback.
    /// 上次切换前使用的登记，用于一步回退。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_model_id: Option<String>,
    /// Lets the review LLM allow calls the judge blocked; off unless the user turns it on.
    /// 允许复核 LLM 放行判定器拦截的调用；用户开启前保持关闭。
    #[serde(default, skip_serializing_if = "is_false")]
    pub llm_may_allow_blocked: bool,
    /// Keeps a local decision log with the original call text; off unless the user turns it on.
    /// 保存含原始调用文本的本机判定记录；用户开启前保持关闭。
    #[serde(default, skip_serializing_if = "is_false")]
    pub record_decisions: bool,
    /// System default deny policy offered when a session turns unattended mode on; empty means
    /// none. Projects may replace it, and a session keeps only the text the user confirmed.
    /// 会话开启无人值守模式时提供的系统默认拒绝策略；为空表示没有。项目可以替换它，会话只保存用户
    /// 确认的文本。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unattended_deny_policy: String,
}

/// Why a registry change was refused.
/// 登记表变更被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeRegistryError {
    /// No registration has this identity.
    /// 没有该身份的登记。
    UnknownModel(String),
    /// The registration is in use and cannot be removed.
    /// 该登记正在使用，不能移除。
    ModelInUse(String),
    /// No earlier registration to return to.
    /// 没有可返回的先前登记。
    NoPreviousModel,
}

impl std::fmt::Display for JudgeRegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownModel(id) => write!(formatter, "No judge model is registered as `{id}`."),
            Self::ModelInUse(id) => {
                write!(
                    formatter,
                    "Judge model `{id}` is in use and cannot be removed."
                )
            }
            Self::NoPreviousModel => {
                formatter.write_str("No previously used judge model is registered.")
            }
        }
    }
}

/// Result of registering a verified package.
/// 登记已验证模型包的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeRegistration {
    /// A new registration was added.
    /// 新增了一条登记。
    Added,
    /// The same package was already registered; its name and path were updated.
    /// 同一模型包已登记；已更新其名称与路径。
    Updated,
}

impl PersistedPermissionJudgeConfiguration {
    /// Returns whether nothing has been saved, so the block stays out of the file.
    /// 返回是否尚未保存任何内容，以便该块不写入文件。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// The registration with `id`.
    /// 身份为 `id` 的登记。
    pub fn model(&self, id: &str) -> Option<&RegisteredJudgeModel> {
        self.models.iter().find(|model| model.id == id)
    }

    /// The registration in use.
    /// 使用中的登记。
    pub fn active_model(&self) -> Option<&RegisteredJudgeModel> {
        self.active_model_id
            .as_deref()
            .and_then(|id| self.model(id))
    }

    /// Adds a verified package, or updates the name and path of the same package.
    /// 添加已验证的模型包，或更新同一模型包的名称与路径。
    pub fn register(&mut self, model: RegisteredJudgeModel) -> JudgeRegistration {
        match self
            .models
            .iter_mut()
            .find(|existing| existing.id == model.id)
        {
            Some(existing) => {
                existing.name = model.name;
                existing.path = model.path;
                JudgeRegistration::Updated
            }
            None => {
                self.models.push(model);
                JudgeRegistration::Added
            }
        }
    }

    /// Makes `id` the model in use and remembers the one it replaces.
    /// 将 `id` 设为使用中的模型，并记住被替换的模型。
    pub fn activate(&mut self, id: &str) -> Result<(), JudgeRegistryError> {
        if self.model(id).is_none() {
            return Err(JudgeRegistryError::UnknownModel(id.to_string()));
        }
        if self.active_model_id.as_deref() != Some(id) {
            self.previous_model_id = self.active_model_id.replace(id.to_string());
        }
        Ok(())
    }

    /// The registration a rollback would return to.
    /// 回退将返回的登记。
    pub fn rollback_target(&self) -> Result<&RegisteredJudgeModel, JudgeRegistryError> {
        self.previous_model_id
            .as_deref()
            .and_then(|id| self.model(id))
            .ok_or(JudgeRegistryError::NoPreviousModel)
    }

    /// Removes a registration that is not in use; files on disk are untouched.
    /// 移除未在使用的登记；磁盘上的文件不受影响。
    pub fn remove(&mut self, id: &str) -> Result<(), JudgeRegistryError> {
        if self.active_model_id.as_deref() == Some(id) {
            return Err(JudgeRegistryError::ModelInUse(id.to_string()));
        }
        let before = self.models.len();
        self.models.retain(|model| model.id != id);
        if self.models.len() == before {
            return Err(JudgeRegistryError::UnknownModel(id.to_string()));
        }
        if self.previous_model_id.as_deref() == Some(id) {
            self.previous_model_id = None;
        }
        Ok(())
    }
}

/// Project-level judge policy: one chosen mode per policy option.
/// 项目级判定策略：每个策略选项一个已选档位。
///
/// Options the project never chose follow the model's defaults in memory and are not written.
/// 项目从未选择的选项在内存中采用模型默认值，不写入文件。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectPermissionJudgeConfiguration {
    /// Chosen mode per option key.
    /// 每个选项键的已选档位。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
    /// Project default deny policy for unattended mode; absent means the system default applies,
    /// and an empty text means this project offers none.
    /// 无人值守模式的项目默认拒绝策略；缺省表示采用系统默认值，空文本表示本项目不提供策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unattended_deny_policy: Option<String>,
}

impl ProjectPermissionJudgeConfiguration {
    /// Returns whether the project chose nothing.
    /// 返回项目是否没有任何选择。
    pub fn is_empty(&self) -> bool {
        self.options.is_empty() && self.unattended_deny_policy.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> RegisteredJudgeModel {
        RegisteredJudgeModel {
            id: id.to_string(),
            name: format!("model {id}"),
            path: format!("/models/{id}"),
            known_package_id: None,
            parent_sha256: "0".repeat(64),
            policy_version: String::from("policy-v2"),
            precision: String::from("fp16"),
            total_bytes: 1,
            registered_at_ms: 1,
        }
    }

    #[test]
    fn unconfigured_settings_are_not_written() {
        let file = ProviderConfigurationFile::default();
        let json = serde_json::to_value(&file).unwrap();
        assert!(json.get("permissionJudge").is_none());
        let project = ProjectVulcanCodeConfigurationFile::default();
        let json = serde_json::to_value(&project).unwrap();
        assert!(json.get("permissionJudge").is_none());
    }

    #[test]
    fn settings_round_trip_with_wire_names() {
        let mut settings = PersistedPermissionJudgeConfiguration {
            backend: Some(JudgeBackend::Local),
            device: Some(JudgeDevice::WebGpu),
            ..Default::default()
        };
        settings.register(model("a"));
        settings.activate("a").unwrap();
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["backend"], "local");
        assert_eq!(json["device"], "webgpu");
        assert_eq!(json["activeModelId"], "a");
        let parsed: PersistedPermissionJudgeConfiguration = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, settings);
    }

    #[test]
    fn switching_keeps_one_rollback_step_and_protects_the_active_model() {
        let mut settings = PersistedPermissionJudgeConfiguration::default();
        assert_eq!(settings.register(model("a")), JudgeRegistration::Added);
        assert_eq!(settings.register(model("b")), JudgeRegistration::Added);
        assert!(settings.activate("missing").is_err());
        settings.activate("a").unwrap();
        assert!(settings.rollback_target().is_err());
        settings.activate("b").unwrap();
        assert_eq!(settings.rollback_target().unwrap().id, "a");
        settings.activate("b").unwrap();
        assert_eq!(settings.previous_model_id.as_deref(), Some("a"));
        assert_eq!(
            settings.remove("b"),
            Err(JudgeRegistryError::ModelInUse(String::from("b")))
        );
        settings.remove("a").unwrap();
        assert!(settings.previous_model_id.is_none());
        assert_eq!(settings.models.len(), 1);
    }

    #[test]
    fn registering_the_same_package_updates_its_location() {
        let mut settings = PersistedPermissionJudgeConfiguration::default();
        settings.register(model("a"));
        let mut moved = model("a");
        moved.path = String::from("/elsewhere/a");
        assert_eq!(settings.register(moved), JudgeRegistration::Updated);
        assert_eq!(settings.models.len(), 1);
        assert_eq!(settings.models[0].path, "/elsewhere/a");
    }
}
