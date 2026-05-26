//! Workbench configuration and selection state extracted from the main workbench module.
//! 从主工作台模块中抽取的工作台配置与选择状态。

use super::*;
use crate::provider_config::{
    PersistedPromptConfiguration, PersistedPromptPersonalitySelection, PersistedProviderOptions,
    ProviderFieldKey,
};
use crate::tooling::shell_host::HostCommandShell;
use std::collections::{BTreeMap, BTreeSet};

/// Editable provider form state kept in sync with the persisted provider configuration file.
/// 与持久化供应商配置文件保持同步的可编辑供应商表单状态。
#[derive(Debug, Clone, Default)]
pub(super) struct ProviderFormState {
    /// Provider API key or access token value.
    /// 供应商 API Key 或访问 Token 值。
    api_key: String,
    /// Optional provider base URL override value.
    /// 可选的供应商基址覆盖值。
    base_url: String,
    /// Azure-style resource name value when the provider requires it.
    /// 当供应商需要时使用的 Azure 风格资源名称值。
    resource_name: String,
    /// Cloudflare-style account identifier value when the provider requires it.
    /// 当供应商需要时使用的 Cloudflare 风格账户标识值。
    account_id: String,
}

impl ProviderFormState {
    /// Builds a form state from persisted provider options.
    /// 从持久化供应商选项构建表单状态。
    pub(super) fn from_options(options: &PersistedProviderOptions) -> Self {
        Self {
            api_key: options.get(ProviderFieldKey::ApiKey),
            base_url: options.get(ProviderFieldKey::BaseUrl),
            resource_name: options.get(ProviderFieldKey::ResourceName),
            account_id: options.get(ProviderFieldKey::AccountId),
        }
    }

    /// Reads one field value from the editable form state.
    /// 从可编辑表单状态中读取一个字段值。
    pub(super) fn get(&self, key: ProviderFieldKey) -> &str {
        match key {
            ProviderFieldKey::ApiKey => &self.api_key,
            ProviderFieldKey::BaseUrl => &self.base_url,
            ProviderFieldKey::ResourceName => &self.resource_name,
            ProviderFieldKey::AccountId => &self.account_id,
        }
    }

    /// Writes one field value into the editable form state.
    /// 向可编辑表单状态中写入一个字段值。
    pub(super) fn set(&mut self, key: ProviderFieldKey, value: String) {
        match key {
            ProviderFieldKey::ApiKey => self.api_key = value,
            ProviderFieldKey::BaseUrl => self.base_url = value,
            ProviderFieldKey::ResourceName => self.resource_name = value,
            ProviderFieldKey::AccountId => self.account_id = value,
        }
    }

    /// Converts the editable form state back into persisted provider options.
    /// 将可编辑表单状态转换回持久化供应商选项。
    pub(super) fn to_options(&self) -> PersistedProviderOptions {
        let mut options = PersistedProviderOptions::default();
        options.set(ProviderFieldKey::ApiKey, self.api_key.clone());
        options.set(ProviderFieldKey::BaseUrl, self.base_url.clone());
        options.set(ProviderFieldKey::ResourceName, self.resource_name.clone());
        options.set(ProviderFieldKey::AccountId, self.account_id.clone());
        options
    }
}

/// Blocking provider-editor modal state used when one model provider is being created or updated.
/// 新增或更新模型供应商时使用的阻塞式供应商编辑弹窗状态。
#[derive(Debug, Clone)]
pub(super) struct ProviderEditorState {
    /// Whether the current editor session is creating a brand-new provider entry.
    /// 当前编辑会话是否正在创建一条全新的供应商配置。
    pub(super) is_new: bool,
}

/// Blocking provider model-picker modal state used to edit one provider's enabled-model draft before explicit save.
/// 在显式保存前编辑某个供应商启用模型草稿时使用的阻塞式供应商模型配置弹窗状态。
#[derive(Debug, Clone)]
pub(super) struct ProviderModelPickerState {
    /// Provider identifier whose complete catalog is currently being edited.
    /// 当前正在编辑其完整目录的供应商标识。
    pub(super) selected_provider_id: String,
    /// Live free-text filter applied to the catalog list.
    /// 应用于目录列表的实时自由文本过滤值。
    pub(super) filter: String,
    /// Per-provider draft enabled-model identifier sets that only become persistent after the user clicks save.
    /// 按供应商分组、仅在用户点击保存后才会真正持久化的启用模型标识草稿集合。
    pub(super) draft_enabled_model_ids_by_provider: BTreeMap<String, BTreeSet<String>>,
}

/// Blocking per-model metadata editor state used to correct one enabled model's capability declaration.
/// 用于修正单个已启用模型能力声明的阻塞式逐模型元数据编辑状态。
#[derive(Debug, Clone)]
pub(super) struct ProviderModelOverrideEditorState {
    /// Provider identifier owning the model currently being edited.
    /// 拥有当前编辑模型的供应商标识。
    pub(super) provider_id: String,
    /// Model identifier whose override metadata is currently being edited.
    /// 当前正在编辑覆盖元数据的模型标识。
    pub(super) model_id: String,
    /// Human-readable model name shown in the modal header.
    /// 显示在弹窗标题中的人类可读模型名称。
    pub(super) model_name: String,
    /// Whether reasoning support should be explicitly declared by the override.
    /// 当前覆盖是否应显式声明推理能力支持。
    pub(super) reasoning: bool,
    /// Whether the catalog-level attachment flag should stay declared after the override is saved.
    /// 保存覆盖后目录级附件标记是否仍应保持声明。
    pub(super) attachment: bool,
    /// Whether text input should stay declared for the model.
    /// 当前模型是否仍应声明支持文本输入。
    pub(super) supports_text: bool,
    /// Whether image input should stay declared for the model.
    /// 当前模型是否仍应声明支持图像输入。
    pub(super) supports_image: bool,
    /// Whether audio input should stay declared for the model.
    /// 当前模型是否仍应声明支持音频输入。
    pub(super) supports_audio: bool,
    /// Whether video input should stay declared for the model.
    /// 当前模型是否仍应声明支持视频输入。
    pub(super) supports_video: bool,
    /// Whether PDF/file input should stay declared for the model.
    /// 当前模型是否仍应声明支持 PDF/文件输入。
    pub(super) supports_pdf: bool,
    /// Editable context-window limit text buffer.
    /// 可编辑的上下文窗口上限文本缓冲区。
    pub(super) context_limit_text: String,
    /// Editable input-token limit text buffer.
    /// 可编辑的输入 Token 上限文本缓冲区。
    pub(super) input_limit_text: String,
    /// Editable output-token limit text buffer.
    /// 可编辑的输出 Token 上限文本缓冲区。
    pub(super) output_limit_text: String,
    /// Editable multi-line JSON buffer for the optional reasoning-profile override.
    /// 可选推理档位覆盖配置使用的可编辑多行 JSON 缓冲区。
    pub(super) reasoning_profile_buffer: text_editor::Content,
}

/// Pick-list choice representing one configurable runtime agent.
/// 表示单个可配置运行时代理的下拉选择项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AgentChoice {
    /// Stable agent identifier.
    /// 稳定代理标识。
    pub(super) id: String,
    /// Human-readable display label.
    /// 人类可读显示标签。
    pub(super) label: String,
}

impl std::fmt::Display for AgentChoice {
    /// Renders the user-facing label of one agent picker item.
    /// 渲染单个代理选择项的面向用户标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Pick-list choice representing one configurable default command shell policy.
/// 表示一条可配置默认命令 shell 策略的下拉选择项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandShellPickerChoice {
    /// Optional explicit shell selected by the user; `None` means system auto resolution.
    /// 用户选择的可选显式 shell；`None` 表示系统自动解析。
    pub(super) shell: Option<HostCommandShell>,
    /// Human-readable display label rendered inside the settings picker.
    /// 渲染在设置选择器中的人类可读显示标签。
    pub(super) label: String,
}

impl std::fmt::Display for CommandShellPickerChoice {
    /// Renders the user-facing label of one default command shell picker item.
    /// 渲染单个默认命令 shell 选择项的面向用户标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Pick-list choice representing one prompt-composer personality mode.
/// 表示单个提示词组合人格模式的下拉选择项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PromptPersonalityChoice {
    /// Persisted personality selection represented by this choice.
    /// 该选项代表的持久化人格选择。
    pub(super) selection: PersistedPromptPersonalitySelection,
    /// Human-readable display label.
    /// 人类可读显示标签。
    pub(super) label: String,
}

impl std::fmt::Display for PromptPersonalityChoice {
    /// Renders the user-facing label of one personality picker item.
    /// 渲染单个人格选择项的面向用户标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Editable project-level prompt configuration draft shown in prompt settings.
/// 提示词设置页中展示的可编辑项目级提示词配置草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectPromptSettingsDraft {
    /// Workspace path owning this project-level configuration draft.
    /// 拥有该项目级配置草稿的工作区路径。
    pub(super) workspace_path: String,
    /// Whether the project prompt should be injected for this workspace.
    /// 当前工作区是否注入项目提示词。
    pub(super) enabled: bool,
    /// Workspace-relative project prompt file path typed by the user.
    /// 用户输入的工作区相对项目提示词文件路径。
    pub(super) path: String,
    /// Whether a project-local configuration file was found while loading the draft.
    /// 载入草稿时是否发现项目本地配置文件。
    pub(super) has_project_override: bool,
    /// Local settings notice shown beside the project prompt controls.
    /// 显示在项目提示词控件旁的局部设置提示。
    pub(super) notice: String,
}

impl Default for ProjectPromptSettingsDraft {
    /// Builds an empty project prompt settings draft before a workspace is resolved.
    /// 在解析出工作区之前构建一份空的项目提示词设置草稿。
    fn default() -> Self {
        Self {
            workspace_path: String::new(),
            enabled: true,
            path: String::from(".vulcan/code/prompt.md"),
            has_project_override: false,
            notice: String::from("尚未载入当前项目提示词配置。"),
        }
    }
}

impl ProjectPromptSettingsDraft {
    /// Builds one draft from the current effective prompt configuration.
    /// 基于当前生效提示词配置构建一份草稿。
    pub(super) fn from_effective_config(
        workspace_path: String,
        config: &PersistedPromptConfiguration,
        has_project_override: bool,
        notice: String,
    ) -> Self {
        Self {
            workspace_path,
            enabled: config.project_prompt_enabled,
            path: config.project_prompt_path.clone(),
            has_project_override,
            notice,
        }
    }
}
