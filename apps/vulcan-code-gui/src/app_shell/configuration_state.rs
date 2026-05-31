//! Workbench configuration and selection state extracted from the main workbench module.
//! 从主工作台模块中抽取的工作台配置与选择状态。

use super::*;
use crate::provider_config::{
    PersistedPromptConfiguration, PersistedPromptPersonalitySelection, PersistedProviderOptions,
    ProviderFieldKey,
};
use crate::tooling::shell_host::HostCommandShell;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_vmm::{
    DEFAULT_GRPC_KEEPALIVE_PERMIT_WITHOUT_CALLS, DEFAULT_GRPC_KEEPALIVE_TIME_MS,
    DEFAULT_GRPC_KEEPALIVE_TIMEOUT_MS, DEFAULT_HANDSHAKE_TIMEOUT_MS, DEFAULT_IMPLICIT_MEMORY_TURNS,
    DEFAULT_PROFILE_NODE_LIMIT, DEFAULT_PROFILE_REFRESH_TURNS, DEFAULT_RECEIVE_TIMEOUT_MS,
    DEFAULT_VMM_INJECTION_DESCRIPTION, DEFAULT_VMM_INJECTION_TAG, MAX_PROFILE_NODE_LIMIT,
    VmmConfigLayer,
};

/// Maximum accepted gRPC keepalive value in milliseconds.
/// 允许保存的 gRPC keepalive 最大毫秒值。
const MAX_GRPC_KEEPALIVE_MS: u64 = 3_600_000;

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

/// Editable VMM configuration draft shared by global and project settings surfaces.
/// 供全局与项目设置界面复用的可编辑 VMM 配置草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VmmSettingsDraft {
    /// Whether the VMM memory flow is enabled for this layer.
    /// 当前层是否启用 VMM 记忆流程。
    pub(super) enabled: bool,
    /// Editable gRPC target such as `127.0.0.1:50051`.
    /// 可编辑 gRPC 目标，例如 `127.0.0.1:50051`。
    pub(super) grpc_target: String,
    /// Editable optional gRPC API key.
    /// 可编辑的可选 gRPC API key。
    pub(super) grpc_api_key: String,
    /// Editable numeric VMM user id.
    /// 可编辑 VMM 数字用户标识。
    pub(super) user_id: String,
    /// Editable numeric VMM project id.
    /// 可编辑 VMM 数字项目标识。
    pub(super) project_id: String,
    /// Editable user reference used only by VMM management RPC helpers.
    /// 仅供 VMM 管理 RPC 辅助功能使用的可编辑用户引用。
    pub(super) management_user_ref: String,
    /// Editable project reference or Team/Space/Project path used only by VMM management RPC helpers.
    /// 仅供 VMM 管理 RPC 辅助功能使用的可编辑项目引用或 Team/Space/Project 路径。
    pub(super) management_project_ref: String,
    /// Editable VMM request timeout in milliseconds.
    /// 可编辑 VMM 请求超时时间，单位毫秒。
    pub(super) receive_timeout_ms: String,
    /// Editable VMM handshake timeout in milliseconds.
    /// 可编辑 VMM 握手超时时间，单位毫秒。
    pub(super) handshake_timeout_ms: String,
    /// Editable optional VMM max receive message bytes.
    /// 可编辑 VMM 可选最大接收消息字节数。
    pub(super) max_receive_message_bytes: String,
    /// Editable VMM gRPC keepalive interval in milliseconds.
    /// 可编辑 VMM gRPC keepalive 间隔，单位毫秒。
    pub(super) grpc_keepalive_time_ms: String,
    /// Editable VMM gRPC keepalive acknowledgement timeout in milliseconds.
    /// 可编辑 VMM gRPC keepalive ACK 超时时间，单位毫秒。
    pub(super) grpc_keepalive_timeout_ms: String,
    /// Whether gRPC keepalive may run while no RPC is active.
    /// 没有活跃 RPC 时是否允许 gRPC keepalive。
    pub(super) grpc_keepalive_permit_without_calls: bool,
    /// Editable transient memory TTL.
    /// 可编辑临时记忆持续回合数。
    pub(super) implicit_memory_turns: String,
    /// Editable profile refresh accepted-turn interval.
    /// 可编辑画像刷新 accepted turn 间隔。
    pub(super) profile_refresh_turns: String,
    /// Editable per-target profile node fetch limit.
    /// 可编辑每个目标画像节点拉取上限。
    pub(super) profile_node_limit: String,
    /// Whether VMM should use visible memory mode.
    /// VMM 是否使用可见记忆模式。
    pub(super) visible_memory_injection: bool,
    /// Whether compact-aware recall is enabled.
    /// 是否启用压缩感知召回。
    pub(super) session_compact_recall: bool,
    /// Whether VMM AI-only tools are exposed.
    /// 是否暴露 VMM AI 专属工具。
    pub(super) ai_tools_enabled: bool,
    /// Whether automatic VMM precheck recall is enabled.
    /// 是否启用 VMM 自动前置召回。
    pub(super) precheck_enabled: bool,
    /// Editable context-injection tag.
    /// 可编辑上下文注入标签。
    pub(super) injection_tag: String,
    /// Editable context-injection description.
    /// 可编辑上下文注入说明。
    pub(super) injection_description: String,
    /// Local settings notice shown near VMM controls.
    /// 显示在 VMM 控件附近的局部设置提示。
    pub(super) notice: String,
}

impl Default for VmmSettingsDraft {
    /// Builds a draft from built-in VMM defaults.
    /// 基于内置 VMM 默认值构建草稿。
    fn default() -> Self {
        Self::from_layer(
            &VmmConfigLayer::default(),
            String::from("尚未载入 VMM 配置。"),
        )
    }
}

impl VmmSettingsDraft {
    /// Builds an editable draft from one optional VMM config layer.
    /// 基于一层可选 VMM 配置构建可编辑草稿。
    pub(super) fn from_layer(layer: &VmmConfigLayer, notice: String) -> Self {
        let user_id = layer
            .user_id
            .map(|value| value.to_string())
            .unwrap_or_default();
        let project_id = layer
            .project_id
            .map(|value| value.to_string())
            .unwrap_or_default();
        Self {
            enabled: layer.enabled.unwrap_or(true),
            grpc_target: layer.grpc_target.clone().unwrap_or_default(),
            grpc_api_key: layer.grpc_api_key.clone().unwrap_or_default(),
            user_id: user_id.clone(),
            project_id: project_id.clone(),
            management_user_ref: user_id,
            management_project_ref: project_id,
            receive_timeout_ms: layer
                .receive_timeout_ms
                .unwrap_or(DEFAULT_RECEIVE_TIMEOUT_MS)
                .to_string(),
            handshake_timeout_ms: layer
                .handshake_timeout_ms
                .unwrap_or(DEFAULT_HANDSHAKE_TIMEOUT_MS)
                .to_string(),
            max_receive_message_bytes: layer
                .max_receive_message_bytes
                .map(|value| value.to_string())
                .unwrap_or_default(),
            grpc_keepalive_time_ms: layer
                .grpc_keepalive_time_ms
                .unwrap_or(DEFAULT_GRPC_KEEPALIVE_TIME_MS)
                .to_string(),
            grpc_keepalive_timeout_ms: layer
                .grpc_keepalive_timeout_ms
                .unwrap_or(DEFAULT_GRPC_KEEPALIVE_TIMEOUT_MS)
                .to_string(),
            grpc_keepalive_permit_without_calls: layer
                .grpc_keepalive_permit_without_calls
                .unwrap_or(DEFAULT_GRPC_KEEPALIVE_PERMIT_WITHOUT_CALLS),
            implicit_memory_turns: layer
                .implicit_memory_turns
                .unwrap_or(DEFAULT_IMPLICIT_MEMORY_TURNS)
                .to_string(),
            profile_refresh_turns: layer
                .profile_refresh_turns
                .unwrap_or(DEFAULT_PROFILE_REFRESH_TURNS)
                .to_string(),
            profile_node_limit: layer
                .profile_node_limit
                .unwrap_or(DEFAULT_PROFILE_NODE_LIMIT)
                .to_string(),
            visible_memory_injection: false,
            session_compact_recall: true,
            ai_tools_enabled: layer.ai_tools_enabled.unwrap_or(true),
            precheck_enabled: layer.precheck_enabled.unwrap_or(true),
            injection_tag: layer
                .injection_tag
                .clone()
                .unwrap_or_else(|| DEFAULT_VMM_INJECTION_TAG.to_string()),
            injection_description: layer
                .injection_description
                .clone()
                .unwrap_or_else(|| DEFAULT_VMM_INJECTION_DESCRIPTION.to_string()),
            notice,
        }
    }

    /// Converts the draft into a concrete config layer.
    /// 将草稿转换成具体配置层。
    pub(super) fn to_layer(&self) -> Result<VmmConfigLayer, String> {
        let user_id = parse_optional_u64(&self.user_id, "VMM 用户 ID")?;
        let project_id = parse_optional_u64(&self.project_id, "VMM 项目 ID")?;
        let receive_timeout_ms = parse_required_positive_u64(
            &self.receive_timeout_ms,
            "VMM 请求超时时间",
            DEFAULT_RECEIVE_TIMEOUT_MS,
        )?;
        let handshake_timeout_ms = parse_required_positive_u64(
            &self.handshake_timeout_ms,
            "VMM 握手超时时间",
            DEFAULT_HANDSHAKE_TIMEOUT_MS,
        )?;
        let max_receive_message_bytes =
            parse_optional_usize(&self.max_receive_message_bytes, "VMM 最大接收消息字节数")?;
        let grpc_keepalive_time_ms = parse_required_positive_u64(
            &self.grpc_keepalive_time_ms,
            "VMM keepalive 间隔",
            DEFAULT_GRPC_KEEPALIVE_TIME_MS,
        )?;
        let grpc_keepalive_timeout_ms = parse_required_positive_u64(
            &self.grpc_keepalive_timeout_ms,
            "VMM keepalive 超时",
            DEFAULT_GRPC_KEEPALIVE_TIMEOUT_MS,
        )?;
        if grpc_keepalive_time_ms > MAX_GRPC_KEEPALIVE_MS {
            return Err(format!(
                "VMM keepalive 间隔不能超过 {MAX_GRPC_KEEPALIVE_MS} ms。"
            ));
        }
        if grpc_keepalive_timeout_ms > MAX_GRPC_KEEPALIVE_MS {
            return Err(format!(
                "VMM keepalive 超时不能超过 {MAX_GRPC_KEEPALIVE_MS} ms。"
            ));
        }
        if grpc_keepalive_timeout_ms >= grpc_keepalive_time_ms {
            return Err(String::from("VMM keepalive 超时必须小于 keepalive 间隔。"));
        }
        let implicit_memory_turns = parse_required_u32(
            &self.implicit_memory_turns,
            "临时记忆持续回合数",
            DEFAULT_IMPLICIT_MEMORY_TURNS,
        )?;
        let profile_refresh_turns = parse_required_u32(
            &self.profile_refresh_turns,
            "画像刷新间隔",
            DEFAULT_PROFILE_REFRESH_TURNS,
        )?;
        let profile_node_limit = parse_required_u32(
            &self.profile_node_limit,
            "单条目最大画像数量",
            DEFAULT_PROFILE_NODE_LIMIT,
        )?;
        if profile_node_limit > MAX_PROFILE_NODE_LIMIT {
            return Err(format!(
                "单条目最大画像数量不能超过 {MAX_PROFILE_NODE_LIMIT}。"
            ));
        }
        Ok(VmmConfigLayer {
            enabled: Some(self.enabled),
            grpc_target: optional_trimmed_string(&self.grpc_target),
            grpc_api_key: optional_trimmed_string(&self.grpc_api_key),
            handshake_timeout_ms: Some(handshake_timeout_ms),
            receive_timeout_ms: Some(receive_timeout_ms),
            max_receive_message_bytes,
            grpc_keepalive_time_ms: Some(grpc_keepalive_time_ms),
            grpc_keepalive_timeout_ms: Some(grpc_keepalive_timeout_ms),
            grpc_keepalive_permit_without_calls: Some(self.grpc_keepalive_permit_without_calls),
            user_id,
            project_id,
            implicit_memory_turns: Some(implicit_memory_turns),
            profile_refresh_turns: Some(profile_refresh_turns),
            profile_node_limit: Some(profile_node_limit),
            visible_memory_injection: None,
            session_compact_recall: None,
            ai_tools_enabled: Some(self.ai_tools_enabled),
            precheck_enabled: Some(self.precheck_enabled),
            injection_tag: optional_trimmed_string(&self.injection_tag),
            injection_description: optional_trimmed_string(&self.injection_description),
            ..VmmConfigLayer::default()
        })
    }
}

/// Editable project-scoped VMM override draft.
/// 可编辑项目级 VMM 覆盖草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectVmmSettingsDraft {
    /// Workspace path owning this project-level VMM draft.
    /// 拥有该项目级 VMM 草稿的工作区路径。
    pub(super) workspace_path: String,
    /// Whether this project should persist its own VMM layer.
    /// 当前项目是否应保存自己的 VMM 配置层。
    pub(super) project_override_enabled: bool,
    /// Editable VMM settings for the project layer.
    /// 项目层使用的可编辑 VMM 设置。
    pub(super) settings: VmmSettingsDraft,
}

impl Default for ProjectVmmSettingsDraft {
    /// Builds an empty project VMM settings draft.
    /// 构建空项目 VMM 设置草稿。
    fn default() -> Self {
        Self {
            workspace_path: String::new(),
            project_override_enabled: false,
            settings: VmmSettingsDraft::from_layer(
                &VmmConfigLayer::default(),
                String::from("尚未载入项目 VMM 配置。"),
            ),
        }
    }
}

impl ProjectVmmSettingsDraft {
    /// Builds a project draft from its persisted layer and inherited global fallback.
    /// 基于持久化项目层与继承的全局回退构建项目草稿。
    pub(super) fn from_project_layer(
        workspace_path: String,
        project_layer: &VmmConfigLayer,
        inherited_layer: &VmmConfigLayer,
        notice: String,
    ) -> Self {
        let has_project_override = !project_layer.is_empty();
        let source = if has_project_override {
            project_layer
        } else {
            inherited_layer
        };
        Self {
            workspace_path,
            project_override_enabled: has_project_override,
            settings: VmmSettingsDraft::from_layer(source, notice),
        }
    }
}

/// Returns a trimmed string only when it contains visible content.
/// 仅在字符串包含可见内容时返回修剪后的字符串。
fn optional_trimmed_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Parses an optional unsigned integer field.
/// 解析可选无符号整数字段。
fn parse_optional_u64(value: &str, label: &str) -> Result<Option<u64>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u64>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

/// Parses an optional `usize` field while preserving blank as none.
/// 解析可选 `usize` 字段，并将空输入保留为无值。
fn parse_optional_usize(value: &str, label: &str) -> Result<Option<usize>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<usize>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

/// Parses a required unsigned integer field with a default fallback for blank input.
/// 解析必填无符号整数字段，并在空输入时使用默认回退。
fn parse_required_u32(value: &str, label: &str, default_value: u32) -> Result<u32, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(default_value);
    }
    trimmed
        .parse::<u32>()
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

/// Parses a required positive 64-bit integer field with a default fallback for blank input.
/// 解析必填正 64 位无符号整数字段，并在空输入时使用默认回退。
fn parse_required_positive_u64(
    value: &str,
    label: &str,
    default_value: u64,
) -> Result<u64, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(default_value);
    }
    let parsed = trimmed
        .parse::<u64>()
        .map_err(|_| format!("{label} 必须是正整数。"))?;
    if parsed == 0 {
        return Err(format!("{label} 必须大于 0。"));
    }
    Ok(parsed)
}
