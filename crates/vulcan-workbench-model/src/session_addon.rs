//! Session-scoped addon state models shared by WebUI, service, and persistence layers.
//! 由 WebUI、服务与持久化层共享的会话级附属状态模型。

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_context::ContextCompactionWindowNodeId;
use vulcan_provider_config::config::ReasoningEffortLevel;

use crate::{
    SessionId, SessionPlanModeState, SessionProviderToolTargetSelection, SessionRuntimeStateEntry,
    SessionTodolistState,
};

/// Returns whether a persisted boolean is false so default addon values stay omitted.
/// 判断持久化布尔值是否为 false，以便默认附属状态值继续省略。
fn is_false(value: &bool) -> bool {
    !*value
}

/// Current schema version for the durable session addon document.
/// 当前持久化会话附属状态文档的结构版本。
pub const SESSION_ADDON_SCHEMA_VERSION: u32 = 4;

/// One stable namespace inside the session addon document.
/// 会话附属状态文档中的一个稳定命名空间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SessionAddonNamespace {
    /// Session index metadata mirrored for addon-level notifications.
    /// 用于附属状态级通知的会话索引元信息镜像。
    Metadata,
    /// User-selected provider, model, reasoning, and temperature controls.
    /// 用户选择的供应商、模型、推理档位与温度控制。
    RuntimeControls,
    /// Session-scoped AI capability routing configuration.
    /// 会话级 AI 扩展能力路由配置。
    /// Session-scoped user work mode such as planning or building.
    /// 会话级用户工作模式，例如计划或构建。
    WorkMode,
    /// Session-level output-style override and applied prompt snapshot.
    /// 会话级输出风格覆盖与已应用提示词快照。
    OutputStyle,
    /// Session-level prompt skill overrides.
    /// 会话级提示词技能覆盖。
    PromptSkills,
    /// Session-level context-injection state.
    /// 会话级上下文注入状态。
    ContextInjection,
    /// Session-level VMM state and memory settings.
    /// 会话级 VMM 状态与记忆设置。
    Vmm,
    /// Session-level knowledge-library selection, separate from conversational memory.
    /// 与会话记忆分离的会话级知识库选择。
    KnowledgeBases,
    /// Session-level todolist updated by the model-facing todolist tool.
    /// 由模型可见 todolist 工具更新的会话级 todolist。
    Todolist,
    /// Session composer and pending prompt UI state.
    /// 会话输入框与待发送提示词界面状态。
    Composer,
    /// Last provider request preview rendered for the session.
    /// 会话最近一次渲染的 Provider 请求预览。
    RequestPreview,
    /// Context reconstruction anchor and budget fingerprints.
    /// 上下文重建锚点与预算指纹。
    ContextAnchor,
    /// Unknown namespace retained only long enough to discard legacy addon payloads.
    /// 仅用于丢弃旧附属状态载荷的未知命名空间。
    Unknown,
}

impl SessionAddonNamespace {
    /// Parses one durable namespace key from JSON documents and events.
    /// 从 JSON 文档与事件中解析一个持久化命名空间键。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the raw snake_case namespace key.
    /// `key` 是原始 snake_case 命名空间键。
    ///
    /// # Returns
    /// 返回值
    /// The known namespace when the key belongs to the current schema.
    /// 当键属于当前结构时返回已知命名空间。
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "metadata" => Some(Self::Metadata),
            "runtime_controls" => Some(Self::RuntimeControls),
            "work_mode" => Some(Self::WorkMode),
            "output_style" => Some(Self::OutputStyle),
            "prompt_skills" => Some(Self::PromptSkills),
            "context_injection" => Some(Self::ContextInjection),
            "vmm" => Some(Self::Vmm),
            "knowledge_bases" => Some(Self::KnowledgeBases),
            "todolist" => Some(Self::Todolist),
            "composer" => Some(Self::Composer),
            "request_preview" => Some(Self::RequestPreview),
            "context_anchor" => Some(Self::ContextAnchor),
            _ => None,
        }
    }

    /// Returns the durable string key used in JSON documents and events.
    /// 返回 JSON 文档与事件中使用的持久化字符串键。
    ///
    /// # Returns
    /// 返回值
    /// The stable snake_case namespace key.
    /// 稳定的 snake_case 命名空间键。
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::RuntimeControls => "runtime_controls",
            Self::WorkMode => "work_mode",
            Self::OutputStyle => "output_style",
            Self::PromptSkills => "prompt_skills",
            Self::ContextInjection => "context_injection",
            Self::Vmm => "vmm",
            Self::KnowledgeBases => "knowledge_bases",
            Self::Todolist => "todolist",
            Self::Composer => "composer",
            Self::RequestPreview => "request_preview",
            Self::ContextAnchor => "context_anchor",
            Self::Unknown => "unknown",
        }
    }

    /// Returns whether this namespace is part of the current persisted schema.
    /// 返回该命名空间是否属于当前持久化结构。
    ///
    /// # Returns
    /// 返回值
    /// `true` for namespaces that may be stored back to disk.
    /// 对允许重新写入磁盘的命名空间返回 `true`。
    pub fn is_known(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

impl Serialize for SessionAddonNamespace {
    /// Serializes the namespace as its durable JSON object key.
    /// 将命名空间序列化为持久化 JSON 对象键。
    ///
    /// # Parameters
    /// 参数
    /// `serializer` is the serde serializer receiving the namespace key.
    /// `serializer` 是接收命名空间键的 serde 序列化器。
    ///
    /// # Returns
    /// 返回值
    /// The serializer result for the stable key string.
    /// 稳定键字符串的序列化结果。
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_key())
    }
}

impl<'de> Deserialize<'de> for SessionAddonNamespace {
    /// Deserializes one namespace key while tolerating obsolete addon namespaces.
    /// 反序列化命名空间键，同时容忍已废弃的附属状态命名空间。
    ///
    /// # Parameters
    /// 参数
    /// `deserializer` is the serde deserializer that yields a namespace key string.
    /// `deserializer` 是产生命名空间键字符串的 serde 反序列化器。
    ///
    /// # Returns
    /// 返回值
    /// A known namespace or `Unknown` when the key belongs to an obsolete schema.
    /// 已知命名空间；若键属于废弃结构则返回 `Unknown`。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let key = String::deserialize(deserializer)?;
        Ok(Self::from_key(&key).unwrap_or(Self::Unknown))
    }
}

/// Versioned JSON payload stored for one session addon namespace.
/// 单个会话附属状态命名空间存储的带版本 JSON 载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionedSessionAddonValue {
    /// Monotonic namespace revision incremented when the namespace value changes.
    /// 命名空间值变化时递增的单调命名空间版本。
    pub revision: u64,
    /// Stable hash of the normalized JSON value.
    /// 规范化 JSON 值的稳定 hash。
    pub hash: String,
    /// Unix-millis text capturing the latest namespace update time.
    /// 捕获命名空间最近更新时间的 Unix 毫秒文本。
    pub updated_at: String,
    /// Raw namespace payload kept schema-flexible for gradual migration.
    /// 为渐进迁移保留结构弹性的原始命名空间载荷。
    pub value: Value,
}

impl VersionedSessionAddonValue {
    /// Builds a versioned namespace value from a serializable payload.
    /// 根据可序列化载荷构建带版本的命名空间值。
    ///
    /// # Parameters
    /// 参数
    /// `previous` is the existing namespace value used to decide whether the revision should increase.
    /// `previous` 是用于判断版本是否应递增的现有命名空间值。
    /// `value` is the normalized JSON payload for the namespace.
    /// `value` 是命名空间的规范化 JSON 载荷。
    /// `updated_at` is the host-provided update timestamp.
    /// `updated_at` 是宿主提供的更新时间戳。
    ///
    /// # Returns
    /// 返回值
    /// The new versioned namespace value and whether it changed.
    /// 新的带版本命名空间值以及是否发生变化。
    pub fn from_json_value(
        previous: Option<&Self>,
        value: Value,
        updated_at: String,
    ) -> (Self, bool) {
        let hash = stable_json_hash(&value);
        let changed = previous.map(|entry| entry.hash.as_str()) != Some(hash.as_str());
        let revision = if changed {
            previous
                .map(|entry| entry.revision.saturating_add(1))
                .unwrap_or(1)
        } else {
            previous.map(|entry| entry.revision).unwrap_or(1)
        };
        let updated_at = if changed {
            updated_at
        } else {
            previous
                .map(|entry| entry.updated_at.clone())
                .unwrap_or(updated_at)
        };

        (
            Self {
                revision,
                hash,
                updated_at,
                value,
            },
            changed,
        )
    }
}

/// Durable session addon document split into independently versioned namespaces.
/// 拆分为独立版本命名空间的持久化会话附属状态文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAddonState {
    /// Addon document schema version used for future migrations.
    /// 用于未来迁移的附属状态文档结构版本。
    pub schema_version: u32,
    /// Session that owns this addon document.
    /// 拥有该附属状态文档的会话。
    pub session_id: SessionId,
    /// Monotonic document revision incremented when any namespace changes.
    /// 任意命名空间变化时递增的单调文档版本。
    pub revision: u64,
    /// Versioned namespace payloads keyed by stable namespace enum.
    /// 按稳定命名空间枚举索引的带版本载荷。
    #[serde(default)]
    pub namespaces: BTreeMap<SessionAddonNamespace, VersionedSessionAddonValue>,
}

impl<'de> Deserialize<'de> for SessionAddonState {
    /// Deserializes an addon state document and drops namespaces no longer in the current schema.
    /// 反序列化附属状态文档，并丢弃当前结构中已不存在的命名空间。
    ///
    /// # Parameters
    /// 参数
    /// `deserializer` is the serde deserializer for the addon document.
    /// `deserializer` 是附属状态文档的 serde 反序列化器。
    ///
    /// # Returns
    /// 返回值
    /// A cleaned addon state containing only known namespaces.
    /// 仅包含已知命名空间的清理后附属状态。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawSessionAddonState {
            schema_version: u32,
            session_id: SessionId,
            revision: u64,
            #[serde(default)]
            namespaces: BTreeMap<SessionAddonNamespace, VersionedSessionAddonValue>,
        }

        let raw = RawSessionAddonState::deserialize(deserializer)?;
        let mut namespaces = raw
            .namespaces
            .into_iter()
            .filter(|(namespace, _)| namespace.is_known())
            .collect::<BTreeMap<_, _>>();
        if raw.schema_version < SESSION_ADDON_SCHEMA_VERSION {
            // Schema 4 intentionally drops the incompatible provider-tool selection payload.
            // Schema 4 有意丢弃不兼容的 Provider Tool 选择载荷。
            if let Some(runtime_controls) =
                namespaces.get_mut(&SessionAddonNamespace::RuntimeControls)
                && let Some(object) = runtime_controls.value.as_object_mut()
            {
                for field in [
                    "providerToolSelections",
                    "providerToolEnabledCapabilities",
                    "providerExtensionTargetSets",
                    "providerToolSettingsInitialized",
                    "providerToolSelectionsOverridden",
                ] {
                    object.remove(field);
                }
                runtime_controls.hash = stable_json_hash(&runtime_controls.value);
            }
        }

        Ok(Self {
            schema_version: raw.schema_version.max(SESSION_ADDON_SCHEMA_VERSION),
            session_id: raw.session_id,
            revision: raw.revision,
            namespaces,
        })
    }
}

impl SessionAddonState {
    /// Creates an empty addon document for one session.
    /// 为单个会话创建空附属状态文档。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the session that owns the document.
    /// `session_id` 是拥有文档的会话。
    ///
    /// # Returns
    /// 返回值
    /// An empty schema-versioned addon state.
    /// 一个带结构版本的空附属状态。
    pub fn new(session_id: SessionId) -> Self {
        Self {
            schema_version: SESSION_ADDON_SCHEMA_VERSION,
            session_id,
            revision: 0,
            namespaces: BTreeMap::new(),
        }
    }

    /// Builds an addon document by extracting addon-owned fields from legacy runtime state.
    /// 通过从旧运行态提取附属状态字段来构建附属状态文档。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the session that owns the converted state.
    /// `session_id` 是拥有转换后状态的会话。
    /// `runtime_state` is the legacy mixed runtime state.
    /// `runtime_state` 是旧的混合运行态。
    /// `updated_at` is the timestamp applied to migrated namespaces.
    /// `updated_at` 是应用到迁移命名空间的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// A populated addon document with compatible namespace payloads.
    /// 一个填充了兼容命名空间载荷的附属状态文档。
    pub fn from_runtime_state(
        session_id: SessionId,
        runtime_state: &SessionRuntimeStateEntry,
        updated_at: String,
    ) -> Self {
        let mut state = Self::new(session_id);

        if let Some(controls) = SessionRuntimeControls::from_runtime_state(runtime_state) {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::RuntimeControls,
                &controls,
                updated_at.clone(),
            );
        }

        if let Some(work_mode) = SessionWorkMode::from_runtime_state(runtime_state) {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::WorkMode,
                &work_mode,
                updated_at.clone(),
            );
        }

        if !runtime_state.output_style.is_empty() {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::OutputStyle,
                &runtime_state.output_style,
                updated_at.clone(),
            );
        }

        if !runtime_state.prompt_skill_overrides.is_empty() {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::PromptSkills,
                &runtime_state.prompt_skill_overrides,
                updated_at.clone(),
            );
        }

        if !runtime_state.context_injection_state.is_empty() {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::ContextInjection,
                &runtime_state.context_injection_state,
                updated_at.clone(),
            );
        }

        if !runtime_state.vmm.is_empty() {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::Vmm,
                &runtime_state.vmm,
                updated_at.clone(),
            );
        }

        if !runtime_state.knowledge_library_selection.is_default() {
            let _ = state.set_knowledge_library_selection(
                &runtime_state.knowledge_library_selection,
                updated_at.clone(),
            );
        }

        if !runtime_state.todolist.is_empty() {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::Todolist,
                &runtime_state.todolist,
                updated_at.clone(),
            );
        }

        if let Some(preview) = runtime_state
            .last_provider_request_preview_markdown
            .as_ref()
        {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::RequestPreview,
                preview,
                updated_at.clone(),
            );
        }

        let anchor = SessionContextAnchor::from_runtime_state(runtime_state);
        if !anchor.is_empty() {
            let _ = state.set_typed_namespace(
                SessionAddonNamespace::ContextAnchor,
                &anchor,
                updated_at,
            );
        }

        state
    }

    /// Applies addon-owned namespaces back onto legacy runtime state for compatibility.
    /// 为兼容旧链路，将附属状态拥有的命名空间回填到旧运行态。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the legacy mixed runtime state to update.
    /// `runtime_state` 是要更新的旧混合运行态。
    pub fn apply_to_runtime_state(&self, runtime_state: &mut SessionRuntimeStateEntry) {
        if let Some(controls) = self.runtime_controls() {
            controls.apply_to_runtime_state(runtime_state);
        }
        if let Some(work_mode) = self.work_mode() {
            work_mode.apply_to_runtime_state(runtime_state);
        }
        if let Some(output_style) = self
            .typed_namespace::<crate::SessionOutputStyleState>(SessionAddonNamespace::OutputStyle)
        {
            runtime_state.output_style = output_style;
        }
        if let Some(prompt_skill_overrides) =
            self.typed_namespace(SessionAddonNamespace::PromptSkills)
        {
            runtime_state.prompt_skill_overrides = prompt_skill_overrides;
        }
        if let Some(context_injection_state) =
            self.typed_namespace(SessionAddonNamespace::ContextInjection)
        {
            runtime_state.context_injection_state = context_injection_state;
        }
        if let Some(vmm) = self.typed_namespace(SessionAddonNamespace::Vmm) {
            runtime_state.vmm = vmm;
        }
        runtime_state.knowledge_library_selection = self.knowledge_library_selection();
        if let Some(todolist) = self.todolist() {
            runtime_state.todolist = todolist;
        }
        if let Some(preview) = self.typed_namespace(SessionAddonNamespace::RequestPreview) {
            runtime_state.last_provider_request_preview_markdown = Some(preview);
        }
        if let Some(anchor) =
            self.typed_namespace::<SessionContextAnchor>(SessionAddonNamespace::ContextAnchor)
        {
            anchor.apply_to_runtime_state(runtime_state);
        }
    }

    /// Reads one raw namespace payload.
    /// 读取一个原始命名空间载荷。
    ///
    /// # Parameters
    /// 参数
    /// `namespace` is the namespace to inspect.
    /// `namespace` 是要检查的命名空间。
    ///
    /// # Returns
    /// 返回值
    /// The versioned namespace payload when present.
    /// 存在时返回带版本的命名空间载荷。
    pub fn namespace(
        &self,
        namespace: SessionAddonNamespace,
    ) -> Option<&VersionedSessionAddonValue> {
        self.namespaces.get(&namespace)
    }

    /// Stores one typed namespace payload and updates revisions only when the value changes.
    /// 存储一个类型化命名空间载荷，并仅在值变化时更新版本。
    ///
    /// # Parameters
    /// 参数
    /// `namespace` is the namespace being written.
    /// `namespace` 是正在写入的命名空间。
    /// `value` is the typed payload to serialize.
    /// `value` 是待序列化的类型化载荷。
    /// `updated_at` is the timestamp applied when the namespace changed.
    /// `updated_at` 是命名空间变化时应用的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the namespace value changed and the document revision advanced.
    /// 当命名空间值变化且文档版本递增时返回 `true`。
    pub fn set_typed_namespace<T>(
        &mut self,
        namespace: SessionAddonNamespace,
        value: &T,
        updated_at: String,
    ) -> bool
    where
        T: Serialize,
    {
        let Ok(value) = serde_json::to_value(value) else {
            return false;
        };
        self.set_json_namespace(namespace, value, updated_at)
    }

    /// Stores one raw namespace payload and updates revisions only when the value changes.
    /// 存储一个原始命名空间载荷，并仅在值变化时更新版本。
    ///
    /// # Parameters
    /// 参数
    /// `namespace` is the namespace being written.
    /// `namespace` 是正在写入的命名空间。
    /// `value` is the raw JSON payload.
    /// `value` 是原始 JSON 载荷。
    /// `updated_at` is the timestamp applied when the namespace changed.
    /// `updated_at` 是命名空间变化时应用的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the namespace value changed and the document revision advanced.
    /// 当命名空间值变化且文档版本递增时返回 `true`。
    pub fn set_json_namespace(
        &mut self,
        namespace: SessionAddonNamespace,
        value: Value,
        updated_at: String,
    ) -> bool {
        if !namespace.is_known() {
            return false;
        }
        let previous = self.namespaces.get(&namespace);
        let (entry, changed) =
            VersionedSessionAddonValue::from_json_value(previous, value, updated_at);
        self.namespaces.insert(namespace, entry);
        if changed {
            self.revision = self.revision.saturating_add(1).max(1);
        }
        changed
    }

    /// Removes one exact addon namespace and advances the document revision when present.
    /// 移除一个精确附属状态命名空间，并在其存在时推进文档版本。
    ///
    /// # Parameters
    /// 参数
    /// `namespace` is the exact known namespace to remove.
    /// `namespace` 是需要移除的精确已知命名空间。
    ///
    /// # Returns
    /// 返回值
    /// `true` when a persisted namespace was physically removed.
    /// 物理移除了一个持久化命名空间时返回 `true`。
    pub fn remove_namespace(&mut self, namespace: SessionAddonNamespace) -> bool {
        let removed = self.namespaces.remove(&namespace).is_some();
        if removed {
            self.revision = self.revision.saturating_add(1).max(1);
        }
        removed
    }

    /// Reads runtime controls from the addon namespace.
    /// 从附属状态命名空间读取运行控制配置。
    ///
    /// # Returns
    /// 返回值
    /// The deserialized runtime controls when present and valid.
    /// 存在且有效时返回反序列化后的运行控制配置。
    pub fn runtime_controls(&self) -> Option<SessionRuntimeControls> {
        self.typed_namespace(SessionAddonNamespace::RuntimeControls)
    }

    /// Writes runtime controls to the addon namespace.
    /// 将运行控制配置写入附属状态命名空间。
    ///
    /// # Parameters
    /// 参数
    /// `controls` are the provider/model/reasoning controls to persist.
    /// `controls` 是要持久化的供应商、模型与推理控制配置。
    /// `updated_at` is the timestamp applied when changed.
    /// `updated_at` 是变化时应用的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the namespace changed.
    /// 当命名空间发生变化时返回 `true`。
    pub fn set_runtime_controls(
        &mut self,
        controls: &SessionRuntimeControls,
        updated_at: String,
    ) -> bool {
        self.set_typed_namespace(SessionAddonNamespace::RuntimeControls, controls, updated_at)
    }

    /// Reads work mode from the addon namespace.
    /// 从附属状态命名空间读取工作模式。
    ///
    /// # Returns
    /// 返回值
    /// The deserialized work mode when present and valid.
    /// 存在且有效时返回反序列化后的工作模式。
    pub fn work_mode(&self) -> Option<SessionWorkMode> {
        self.typed_namespace(SessionAddonNamespace::WorkMode)
    }

    /// Reads the session todolist from the addon namespace.
    /// 从附属状态命名空间读取会话 todolist。
    ///
    /// # Returns
    /// 返回值
    /// The deserialized todolist when present and valid.
    /// 存在且有效时返回反序列化后的 todolist。
    pub fn todolist(&self) -> Option<SessionTodolistState> {
        self.typed_namespace(SessionAddonNamespace::Todolist)
    }

    /// Writes the session todolist namespace.
    /// 写入会话 todolist命名空间。
    ///
    /// # Parameters
    /// 参数
    /// `todolist` is the latest session-scoped todolist state.
    /// `todolist` 是最新的会话级 todolist 状态。
    /// `updated_at` is the timestamp applied when changed.
    /// `updated_at` 是变化时应用的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the namespace changed.
    /// 当命名空间发生变化时返回 `true`。
    pub fn set_todolist(&mut self, todolist: &SessionTodolistState, updated_at: String) -> bool {
        self.set_typed_namespace(SessionAddonNamespace::Todolist, todolist, updated_at)
    }

    /// Reads the session knowledge-library selection from its independent namespace.
    /// 从独立命名空间读取会话知识库选择。
    ///
    /// # Returns
    /// 返回值
    /// The persisted selection, defaulting to project inheritance when absent or invalid.
    /// 返回持久化选择；缺失或无效时默认继承项目配置。
    pub fn knowledge_library_selection(&self) -> SessionKnowledgeLibrarySelection {
        self.typed_namespace(SessionAddonNamespace::KnowledgeBases)
            .unwrap_or_default()
    }

    /// Writes a normalized session knowledge-library selection.
    /// 写入一份规范化的会话知识库选择。
    ///
    /// # Parameters
    /// 参数
    /// `selection` is the project-inherit or explicit selection to persist.
    /// `selection` 是要持久化的项目继承或显式选择。
    /// `updated_at` is the timestamp applied when the value changes.
    /// `updated_at` 是值变化时应用的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the durable namespace changed.
    /// 持久化命名空间发生变化时返回 `true`。
    pub fn set_knowledge_library_selection(
        &mut self,
        selection: &SessionKnowledgeLibrarySelection,
        updated_at: String,
    ) -> bool {
        let normalized = selection.clone().normalized();
        self.set_typed_namespace(
            SessionAddonNamespace::KnowledgeBases,
            &normalized,
            updated_at,
        )
    }

    /// Writes work mode to the addon namespace.
    /// 将工作模式写入附属状态命名空间。
    ///
    /// # Parameters
    /// 参数
    /// `work_mode` is the session-scoped work mode payload.
    /// `work_mode` 是会话级工作模式载荷。
    /// `updated_at` is the timestamp applied when changed.
    /// `updated_at` 是变化时应用的时间戳。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the namespace changed.
    /// 当命名空间发生变化时返回 `true`。
    pub fn set_work_mode(&mut self, work_mode: &SessionWorkMode, updated_at: String) -> bool {
        self.set_typed_namespace(SessionAddonNamespace::WorkMode, work_mode, updated_at)
    }

    /// Reads one namespace as a typed payload.
    /// 将一个命名空间读取为类型化载荷。
    ///
    /// # Parameters
    /// 参数
    /// `namespace` is the namespace to deserialize.
    /// `namespace` 是要反序列化的命名空间。
    ///
    /// # Returns
    /// 返回值
    /// The typed payload when the namespace exists and deserializes successfully.
    /// 命名空间存在且反序列化成功时返回类型化载荷。
    pub fn typed_namespace<T>(&self, namespace: SessionAddonNamespace) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.namespaces
            .get(&namespace)
            .and_then(|entry| serde_json::from_value(entry.value.clone()).ok())
    }
}

/// Session selection strategy for project-authorized knowledge libraries.
/// 项目已授权知识库的会话选择策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionKnowledgeLibrarySelectionMode {
    /// Inherit libraries marked default-enabled for the current project.
    /// 继承当前项目中标记为默认启用的知识库。
    #[default]
    Inherit,
    /// Use only explicitly listed libraries after authorization intersection.
    /// 与授权集合求交后，仅使用显式列出的知识库。
    Explicit,
}

/// Durable per-session knowledge-library selection.
/// 持久化的每会话知识库选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionKnowledgeLibrarySelection {
    /// Selection strategy.
    /// 选择策略。
    #[serde(default)]
    pub mode: SessionKnowledgeLibrarySelectionMode,
    /// Explicit library IDs, used only when mode is explicit.
    /// 显式知识库标识符，仅在 explicit 模式下使用。
    #[serde(default)]
    pub library_ids: Vec<String>,
    /// Optional retrieval policy explicitly selected for the session tools.
    /// 会话工具显式选择的可选检索策略。
    #[serde(default)]
    pub retrieval_mode: Option<String>,
}

impl SessionKnowledgeLibrarySelection {
    /// Returns a canonical selection with deterministic unique identifiers.
    /// 返回具有确定性唯一标识符的规范选择。
    ///
    /// # Returns
    /// 返回值
    /// A sorted and deduplicated explicit selection, or an identifier-free inherited selection.
    /// 返回排序去重后的显式选择，或不含标识符的继承选择。
    pub fn normalized(mut self) -> Self {
        self.library_ids.sort();
        self.library_ids.dedup();
        if self.mode == SessionKnowledgeLibrarySelectionMode::Inherit {
            self.library_ids.clear();
        }
        if self
            .retrieval_mode
            .as_deref()
            .is_some_and(|mode| !matches!(mode, "raw_bm25" | "display_bm25" | "display_hybrid"))
        {
            self.retrieval_mode = None;
        }
        self
    }

    /// Returns whether the selection uses the implicit project defaults.
    /// 返回该选择是否使用隐式项目默认值。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the selection is the default inherit mode without explicit identifiers.
    /// 当选择为默认继承模式且不含显式标识符时返回 `true`。
    pub fn is_default(&self) -> bool {
        self.mode == SessionKnowledgeLibrarySelectionMode::Inherit && self.library_ids.is_empty()
    }
}

/// Session-scoped provider/model/reasoning controls managed outside transcript hydration.
/// 在转录水合之外管理的会话级供应商、模型与推理控制配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRuntimeControls {
    /// Last active agent identifier used by the session.
    /// 最近一次被该会话使用的激活代理标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_agent_id: Option<String>,
    /// Last active provider identifier used by the session.
    /// 最近一次被该会话使用的激活供应商标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
    /// Last credential connection identifier selected by the session.
    /// 该会话最近一次选择的凭据连接标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_connection_id: Option<String>,
    /// Last active model identifier used by the session.
    /// 最近一次被该会话使用的激活模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_model_id: Option<String>,
    /// Exact model selection waiting for the next user-submission boundary.
    /// 等待下一次用户提交边界生效的精确模型选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_model_selection: Option<crate::SessionPendingModelSelection>,
    /// Persisted non-secret identity of the session's frozen provider route.
    /// 当前会话冻结供应商路由的持久化非秘密身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_route_lease: Option<crate::SessionProviderRouteLeaseIdentity>,
    /// Stable context-compaction window node selected only for this session.
    /// 仅为当前会话选择的稳定上下文压缩窗口节点。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_compaction_window_node_id: Option<ContextCompactionWindowNodeId>,
    /// Last reasoning-effort preset selected while this session was active.
    /// 最近一次该会话激活时选择的推理档位预设。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_reasoning_effort: Option<ReasoningEffortLevel>,
    /// Records whether the session reasoning preference was explicitly chosen by the user.
    /// 记录会话思考偏好是否由用户明确选择，用于区分旧版默认值与显式关闭。
    #[serde(default, skip_serializing_if = "is_false")]
    pub active_reasoning_effort_explicit: bool,
    /// Last optional temperature override selected while this session was active.
    /// 最近一次该会话激活时选择的可选温度覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_temperature: Option<f32>,
    /// Session-selected provider tool targets keyed by semantic capability.
    /// 按语义能力索引的会话级供应商工具目标选择。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_tool_selections: BTreeMap<String, SessionProviderToolTargetSelection>,
    /// Provider tool capabilities enabled only for this session.
    /// 仅为当前会话启用的供应商工具能力。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub provider_tool_enabled_capabilities: BTreeSet<String>,
    /// Session-selected native model-extension target sets keyed by extension capability.
    /// 按扩展能力索引的会话级原生模型扩展目标集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_extension_target_sets:
        BTreeMap<String, BTreeSet<SessionProviderToolTargetSelection>>,
    /// Whether provider tool defaults have been materialized into this session.
    /// 供应商工具默认值是否已经物化到当前会话。
    #[serde(default)]
    pub provider_tool_settings_initialized: bool,
    /// Whether this session owns an explicit provider-tool selection instead of inheriting global defaults.
    /// 当前会话是否拥有显式供应商工具选择，而不是继承全局默认值。
    #[serde(default)]
    pub provider_tool_selections_overridden: bool,
}

impl SessionRuntimeControls {
    /// Builds runtime controls from legacy mixed runtime state when any controlled field exists.
    /// 当任一控制字段存在时，从旧混合运行态构建运行控制配置。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the legacy runtime state to inspect.
    /// `runtime_state` 是要检查的旧运行态。
    ///
    /// # Returns
    /// 返回值
    /// Runtime controls when at least one controlled value exists.
    /// 至少存在一个控制值时返回运行控制配置。
    pub fn from_runtime_state(runtime_state: &SessionRuntimeStateEntry) -> Option<Self> {
        let controls = Self {
            active_agent_id: runtime_state.active_agent_id.clone(),
            active_provider_id: runtime_state.active_provider_id.clone(),
            active_connection_id: runtime_state.active_connection_id.clone(),
            active_model_id: runtime_state.active_model_id.clone(),
            pending_model_selection: runtime_state.pending_model_selection.clone(),
            provider_route_lease: runtime_state.provider_route_lease.clone(),
            context_compaction_window_node_id: runtime_state.context_compaction_window_node_id,
            active_reasoning_effort: runtime_state.active_reasoning_effort,
            active_reasoning_effort_explicit: runtime_state.active_reasoning_effort_explicit,
            active_temperature: runtime_state.active_temperature,
            provider_tool_selections: runtime_state.provider_tool_selections.clone(),
            provider_tool_enabled_capabilities: runtime_state
                .provider_tool_enabled_capabilities
                .clone(),
            provider_extension_target_sets: runtime_state.provider_extension_target_sets.clone(),
            provider_tool_settings_initialized: runtime_state.provider_tool_settings_initialized,
            provider_tool_selections_overridden: runtime_state.provider_tool_selections_overridden,
        };
        controls.has_any_value().then_some(controls)
    }

    /// Applies the controls back to the legacy runtime state for compatibility during migration.
    /// 在迁移期间将控制配置回写到旧运行态以保持兼容。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the legacy runtime state to update.
    /// `runtime_state` 是要更新的旧运行态。
    pub fn apply_to_runtime_state(&self, runtime_state: &mut SessionRuntimeStateEntry) {
        runtime_state.active_agent_id = self.active_agent_id.clone();
        runtime_state.active_provider_id = self.active_provider_id.clone();
        runtime_state.active_connection_id = self.active_connection_id.clone();
        runtime_state.active_model_id = self.active_model_id.clone();
        runtime_state.pending_model_selection = self.pending_model_selection.clone();
        runtime_state.provider_route_lease = self.provider_route_lease.clone();
        runtime_state.context_compaction_window_node_id = self.context_compaction_window_node_id;
        runtime_state.active_reasoning_effort = self.active_reasoning_effort;
        runtime_state.active_reasoning_effort_explicit = self.active_reasoning_effort_explicit;
        runtime_state.active_temperature = self.active_temperature;
        runtime_state.provider_tool_selections = self.provider_tool_selections.clone();
        runtime_state.provider_tool_enabled_capabilities =
            self.provider_tool_enabled_capabilities.clone();
        runtime_state.provider_extension_target_sets = self.provider_extension_target_sets.clone();
        runtime_state.provider_tool_settings_initialized = self.provider_tool_settings_initialized;
        runtime_state.provider_tool_selections_overridden =
            self.provider_tool_selections_overridden;
    }

    /// Returns whether any controlled value has been recorded.
    /// 返回是否已经记录任意控制值。
    ///
    /// # Returns
    /// 返回值
    /// `true` when at least one runtime-control field is present.
    /// 至少存在一个运行控制字段时返回 `true`。
    pub fn has_any_value(&self) -> bool {
        self.active_agent_id.is_some()
            || self.active_provider_id.is_some()
            || self.active_connection_id.is_some()
            || self.active_model_id.is_some()
            || self.pending_model_selection.is_some()
            || self.provider_route_lease.is_some()
            || self.context_compaction_window_node_id.is_some()
            || self.active_reasoning_effort.is_some()
            || self.active_reasoning_effort_explicit
            || self.active_temperature.is_some()
            || !self.provider_tool_selections.is_empty()
            || !self.provider_tool_enabled_capabilities.is_empty()
            || !self.provider_extension_target_sets.is_empty()
            || self.provider_tool_settings_initialized
            || self.provider_tool_selections_overridden
    }
}

impl Eq for SessionRuntimeControls {}

#[cfg(test)]
mod knowledge_library_selection_tests {
    use super::*;

    /// Verifies explicit knowledge selections are sorted, deduplicated, and round-trip safely.
    /// 验证显式知识库选择会排序、去重并安全往返。
    #[test]
    fn explicit_selection_is_normalized() {
        let session_id = SessionId::new_v4();
        let mut addon = SessionAddonState::new(session_id);
        let selection = SessionKnowledgeLibrarySelection {
            mode: SessionKnowledgeLibrarySelectionMode::Explicit,
            library_ids: vec![String::from("b"), String::from("a"), String::from("b")],
            retrieval_mode: None,
        };

        assert!(addon.set_knowledge_library_selection(&selection, String::from("1")));
        assert_eq!(
            addon.knowledge_library_selection().library_ids,
            vec![String::from("a"), String::from("b")]
        );
        assert_eq!(selection.clone().normalized().library_ids, vec!["a", "b"]);
    }

    /// Verifies inherited selection never persists irrelevant explicit identifiers.
    /// 验证继承型选择绝不会持久化无关的显式标识符。
    #[test]
    fn inherited_selection_clears_explicit_ids() {
        let mut addon = SessionAddonState::new(SessionId::new_v4());
        let selection = SessionKnowledgeLibrarySelection {
            mode: SessionKnowledgeLibrarySelectionMode::Inherit,
            library_ids: vec![String::from("ignored")],
            retrieval_mode: None,
        };

        addon.set_knowledge_library_selection(&selection, String::from("1"));
        assert!(addon.knowledge_library_selection().library_ids.is_empty());
    }

    /// Verifies legacy addon documents upgrade monotonically while preserving known namespaces.
    /// 验证旧版附属状态文档在保留已知命名空间时单调升级。
    #[test]
    fn legacy_addon_schema_upgrades_to_current_version() {
        let session_id = SessionId::new_v4();
        let payload = serde_json::json!({
            "schemaVersion": 1,
            "sessionId": session_id,
            "revision": 0,
            "namespaces": {}
        });

        let addon: SessionAddonState =
            serde_json::from_value(payload).expect("deserialize legacy addon document");

        assert_eq!(addon.schema_version, SESSION_ADDON_SCHEMA_VERSION);
        assert_eq!(addon.session_id, session_id);
    }
}

/// Session-scoped user work mode managed outside transcript hydration.
/// 在转录水合之外管理的会话级用户工作模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkMode {
    /// Stable work mode key such as `build` or `plan`.
    /// 稳定工作模式键，例如 `build` 或 `plan`。
    pub execution_mode: String,
    /// Session-scoped Plan mode marker and latest generated artifact.
    /// 会话级计划模式标记与最近生成的计划产物。
    #[serde(default, skip_serializing_if = "SessionPlanModeState::is_empty")]
    pub plan_mode: SessionPlanModeState,
}

impl SessionWorkMode {
    /// Builds a work-mode payload from legacy mixed runtime state.
    /// 从旧混合运行态构建工作模式载荷。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the legacy runtime state to inspect.
    /// `runtime_state` 是要检查的旧运行态。
    ///
    /// # Returns
    /// 返回值
    /// Work mode when a mode key exists.
    /// 存在模式键时返回工作模式。
    pub fn from_runtime_state(runtime_state: &SessionRuntimeStateEntry) -> Option<Self> {
        runtime_state
            .active_work_mode
            .as_ref()
            .filter(|mode| !mode.trim().is_empty())
            .map(|mode| Self {
                execution_mode: mode.clone(),
                plan_mode: runtime_state.plan_mode.clone(),
            })
    }

    /// Applies the work mode back to the legacy runtime state for compatibility during migration.
    /// 在迁移期间将工作模式回写到旧运行态以保持兼容。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the legacy runtime state to update.
    /// `runtime_state` 是要更新的旧运行态。
    pub fn apply_to_runtime_state(&self, runtime_state: &mut SessionRuntimeStateEntry) {
        runtime_state.active_work_mode = Some(self.execution_mode.clone());
        runtime_state.plan_mode = self.plan_mode.clone();
    }
}

/// Context reconstruction anchors managed as a session addon namespace.
/// 作为会话附属命名空间管理的上下文重建锚点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextAnchor {
    /// Latest compaction identifier attached to the session.
    /// 附着在该会话上的最近一次压缩标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_compaction_id: Option<String>,
    /// Current message identifier used as the next context reconstruction anchor.
    /// 当前用作下一轮上下文重建锚点的消息标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_start_message_id: Option<String>,
    /// Live budget fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时预算指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_context_budget_fingerprint: Option<String>,
    /// Live layered-context fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时分层上下文指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_context_assembly_fingerprint: Option<String>,
    /// Anchor budget fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点预算指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_context_budget_fingerprint: Option<String>,
    /// Anchor layered-context fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点分层上下文指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_context_assembly_fingerprint: Option<String>,
}

impl SessionContextAnchor {
    /// Builds context anchors from legacy mixed runtime state.
    /// 从旧混合运行态构建上下文锚点。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the legacy runtime state to inspect.
    /// `runtime_state` 是要检查的旧运行态。
    ///
    /// # Returns
    /// 返回值
    /// Context anchor values copied from the legacy runtime state.
    /// 从旧运行态复制出的上下文锚点值。
    pub fn from_runtime_state(runtime_state: &SessionRuntimeStateEntry) -> Self {
        Self {
            last_compaction_id: runtime_state.last_compaction_id.clone(),
            context_start_message_id: runtime_state.context_start_message_id.clone(),
            live_context_budget_fingerprint: runtime_state.live_context_budget_fingerprint.clone(),
            live_context_assembly_fingerprint: runtime_state
                .live_context_assembly_fingerprint
                .clone(),
            anchor_context_budget_fingerprint: runtime_state
                .anchor_context_budget_fingerprint
                .clone(),
            anchor_context_assembly_fingerprint: runtime_state
                .anchor_context_assembly_fingerprint
                .clone(),
        }
    }

    /// Applies context anchors back to the legacy runtime state.
    /// 将上下文锚点回填到旧运行态。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_state` is the legacy runtime state to update.
    /// `runtime_state` 是要更新的旧运行态。
    pub fn apply_to_runtime_state(&self, runtime_state: &mut SessionRuntimeStateEntry) {
        runtime_state.last_compaction_id = self.last_compaction_id.clone();
        runtime_state.context_start_message_id = self.context_start_message_id.clone();
        runtime_state.live_context_budget_fingerprint =
            self.live_context_budget_fingerprint.clone();
        runtime_state.live_context_assembly_fingerprint =
            self.live_context_assembly_fingerprint.clone();
        runtime_state.anchor_context_budget_fingerprint =
            self.anchor_context_budget_fingerprint.clone();
        runtime_state.anchor_context_assembly_fingerprint =
            self.anchor_context_assembly_fingerprint.clone();
    }

    /// Returns whether no anchor value has been captured.
    /// 返回是否尚未捕获任何锚点值。
    ///
    /// # Returns
    /// 返回值
    /// `true` when every context anchor field is empty.
    /// 当所有上下文锚点字段为空时返回 `true`。
    pub fn is_empty(&self) -> bool {
        self.last_compaction_id.is_none()
            && self.context_start_message_id.is_none()
            && self.live_context_budget_fingerprint.is_none()
            && self.live_context_assembly_fingerprint.is_none()
            && self.anchor_context_budget_fingerprint.is_none()
            && self.anchor_context_assembly_fingerprint.is_none()
    }
}

/// Computes a deterministic FNV-1a hash for a JSON value.
/// 为 JSON 值计算确定性的 FNV-1a hash。
///
/// # Parameters
/// 参数
/// `value` is the JSON payload to hash after canonical serialization.
/// `value` 是经过规范序列化后待计算 hash 的 JSON 载荷。
///
/// # Returns
/// 返回值
/// A lowercase hexadecimal hash string.
/// 小写十六进制 hash 字符串。
fn stable_json_hash(value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_default();
    let mut hash = 0xcbf29ce484222325u64;
    for byte in serialized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests;
