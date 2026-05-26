//! Panel and settings navigation state extracted from the main workbench module.
//! 从主工作台模块中抽取的面板与设置导航状态。

use crate::provider_config::AiToolKind;
use crate::{AppLocale, I18nArg, I18nKey, format_for_locale, text_for_locale};

/// Top-level panels that can be displayed inside the main workbench region.
/// 可以显示在主工作台区域中的顶层面板类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MainDisplayPanel {
    /// Default workbench panel containing the pane-grid workspace.
    /// 包含分栏工作区的默认主面板。
    Main,
    /// Regular-skill panel placeholder used to host general prompt and skill orchestration later.
    /// 用于后续承载常规提示词与技能编排的技能（常规）面板占位。
    PromptSkills,
    /// LuaSkills panel placeholder used to host Lua-based capabilities later.
    /// 用于后续承载基于 LuaSkills 的能力配置的技能（LuaSkills）面板占位。
    LuaSkills,
    /// Automation panel placeholder used to host recurring and scheduled flows later.
    /// 用于后续承载定时与自动化流程的自动化面板占位。
    Automation,
    /// Agent panel used to manage user-authored primary-agent profiles.
    /// 用于管理用户自定义主代理配置的代理面板。
    Agents,
    /// Plugin panel placeholder used to host plugin management later.
    /// 用于后续承载插件管理的插件面板占位。
    Plugins,
    /// VulcanMemoryMesh panel placeholder used to host memory-mesh capabilities later.
    /// 用于后续承载 VulcanMemoryMesh 记忆网络能力的面板占位。
    VulcanMemoryMesh,
    /// Example panel that wraps the current workspace surface for iterative redesign.
    /// 用于包裹当前工作区主体并便于重构迭代的示例面板。
    Example,
    /// Settings panel that focuses on provider configuration and runtime tuning.
    /// 聚焦供应商配置与运行调优的设置面板。
    Settings,
}

impl MainDisplayPanel {
    /// Returns the localization key backing the top-level panel label.
    /// 返回顶层面板标签对应的本地化键。
    const fn label_key(self) -> I18nKey {
        match self {
            Self::Main => I18nKey::WorkbenchPanelMainLabel,
            Self::PromptSkills => I18nKey::WorkbenchPanelPromptSkillsLabel,
            Self::LuaSkills => I18nKey::WorkbenchPanelLuaSkillsLabel,
            Self::Automation => I18nKey::WorkbenchPanelAutomationLabel,
            Self::Agents => I18nKey::WorkbenchPanelAgentsLabel,
            Self::Plugins => I18nKey::WorkbenchPanelPluginsLabel,
            Self::VulcanMemoryMesh => I18nKey::WorkbenchPanelMemoryMeshLabel,
            Self::Example => I18nKey::WorkbenchPanelExampleLabel,
            Self::Settings => I18nKey::WorkbenchPanelSettingsLabel,
        }
    }

    /// Returns the localization key backing the top-level panel description.
    /// 返回顶层面板说明文案对应的本地化键。
    const fn description_key(self) -> I18nKey {
        match self {
            Self::Main => I18nKey::WorkbenchPanelMainDescription,
            Self::PromptSkills => I18nKey::WorkbenchPanelPromptSkillsDescription,
            Self::LuaSkills => I18nKey::WorkbenchPanelLuaSkillsDescription,
            Self::Automation => I18nKey::WorkbenchPanelAutomationDescription,
            Self::Agents => I18nKey::WorkbenchPanelAgentsDescription,
            Self::Plugins => I18nKey::WorkbenchPanelPluginsDescription,
            Self::VulcanMemoryMesh => I18nKey::WorkbenchPanelMemoryMeshDescription,
            Self::Example => I18nKey::WorkbenchPanelExampleDescription,
            Self::Settings => I18nKey::WorkbenchPanelSettingsDescription,
        }
    }

    /// Returns the localized label shown in the top-level panel switcher.
    /// 返回展示在顶层面板切换入口中的本地化标签。
    pub(super) fn localized_label(self, locale: AppLocale) -> String {
        text_for_locale(locale, self.label_key())
    }

    /// Returns the localized contextual caption shown beside the top-level panel switcher.
    /// 返回展示在顶层面板切换器旁侧的本地化说明文本。
    pub(super) fn localized_description(self, locale: AppLocale) -> String {
        text_for_locale(locale, self.description_key())
    }

    /// Returns the localized title rendered inside one top-level panel body.
    /// 返回渲染在某个顶层面板主体内部的本地化标题。
    pub(super) fn localized_section_title(self, locale: AppLocale) -> String {
        self.localized_label(locale)
    }

    /// Returns the localized tooltip copy including the keyboard shortcut.
    /// 返回包含键盘快捷键的本地化提示文案。
    pub(super) fn localized_shortcut_hint(self, locale: AppLocale) -> String {
        format_for_locale(
            locale,
            I18nKey::WorkbenchPanelShortcutHint,
            &[
                I18nArg::named("label", self.localized_label(locale)),
                I18nArg::named("digit", self.shortcut_digit()),
            ],
        )
    }

    /// Returns the keyboard shortcut digit used by one top-level panel.
    /// 返回某个顶层面板使用的键盘快捷数字。
    const fn shortcut_digit(self) -> &'static str {
        match self {
            Self::Main => "1",
            Self::PromptSkills => "2",
            Self::LuaSkills => "3",
            Self::Automation => "4",
            Self::Agents => "5",
            Self::Plugins => "6",
            Self::VulcanMemoryMesh => "7",
            Self::Example => "8",
            Self::Settings => "9",
        }
    }

    /// Returns the panel matched by one keyboard shortcut digit.
    /// 返回某个键盘快捷数字所对应的面板。
    pub(super) const fn from_shortcut_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(Self::Main),
            '2' => Some(Self::PromptSkills),
            '3' => Some(Self::LuaSkills),
            '4' => Some(Self::Automation),
            '5' => Some(Self::Agents),
            '6' => Some(Self::Plugins),
            '7' => Some(Self::VulcanMemoryMesh),
            '8' => Some(Self::Example),
            '9' => Some(Self::Settings),
            _ => None,
        }
    }
}

/// Settings navigation sections rendered in the left tree of the settings workspace.
/// 渲染在设置工作区左侧树导航中的设置分区。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsSection {
    /// Chat-provider credentials and active-model configuration.
    /// 对话模型供应商凭据与已启用模型配置。
    ModelProviders,
    /// Enabled-model overview grouped by provider with per-model capability editing.
    /// 按供应商分组的已启用模型总览与逐模型能力编辑入口。
    ModelList,
    /// Runtime tuning, execution diagnostics, and engineering guardrails.
    /// 运行调优、执行诊断与工程约束设置。
    RuntimeDiagnostics,
    /// Prompt composer and personality configuration.
    /// 提示词组合器与人格配置。
    PromptComposer,
    /// Shared AI capability vendor credentials reused by multiple semantic tools.
    /// 被多个语义工具复用的共享 AI 能力供应商凭据。
    AiCapabilityVendors,
    /// Unified hosted or third-party web-search capability routing.
    /// 统一原生或第三方网页搜索能力路由。
    AiCapabilityWebSearch,
    /// Image-understanding capability routing delegated to external vision vendors.
    /// 委派给外部视觉供应商的图片理解能力路由。
    AiCapabilityUnderstandImage,
    /// Video-understanding capability routing placeholder reserved for future multimodal ingestion.
    /// 为未来多模态接入预留的视频理解能力路由占位。
    AiCapabilityUnderstandVideo,
    /// Audio-understanding capability routing placeholder reserved for future multimodal ingestion.
    /// 为未来多模态接入预留的音频理解能力路由占位。
    AiCapabilityUnderstandAudio,
    /// Image-generation capability routing.
    /// 图片生成能力路由。
    AiCapabilityImageGeneration,
    /// Video-generation capability routing.
    /// 视频生成能力路由。
    AiCapabilityVideoGeneration,
    /// Speech-synthesis capability routing.
    /// 语音合成能力路由。
    AiCapabilitySpeechSynthesis,
    /// Music-generation capability routing.
    /// 音乐生成能力路由。
    AiCapabilityMusicGeneration,
    /// Music-cover capability routing.
    /// 音乐翻唱能力路由。
    AiCapabilityMusicCover,
    /// MCP server configuration and OAuth management.
    /// MCP 服务器配置与 OAuth 管理。
    McpServers,
    /// About page for application licensing, generated notices, and bundled compliance references.
    /// 展示应用许可证、生成声明与内置合规参考的关于页面。
    AboutLicenses,
    /// Archived session and project restoration surface.
    /// 归档会话与归档项目恢复界面。
    ArchivedRestore,
}

impl SettingsSection {
    /// Returns the localization key backing the compact settings-tree label.
    /// 返回设置树紧凑标签对应的本地化键。
    const fn label_key(self) -> I18nKey {
        match self {
            Self::ModelProviders => I18nKey::SettingsSectionModelProvidersLabel,
            Self::ModelList => I18nKey::SettingsSectionModelListLabel,
            Self::RuntimeDiagnostics => I18nKey::SettingsSectionRuntimeDiagnosticsLabel,
            Self::PromptComposer => I18nKey::SettingsSectionPromptComposerLabel,
            Self::AiCapabilityVendors => I18nKey::SettingsSectionAiCapabilityVendorsLabel,
            Self::AiCapabilityWebSearch => I18nKey::SettingsSectionAiCapabilityWebSearchLabel,
            Self::AiCapabilityUnderstandImage => {
                I18nKey::SettingsSectionAiCapabilityUnderstandImageLabel
            }
            Self::AiCapabilityUnderstandVideo => {
                I18nKey::SettingsSectionAiCapabilityUnderstandVideoLabel
            }
            Self::AiCapabilityUnderstandAudio => {
                I18nKey::SettingsSectionAiCapabilityUnderstandAudioLabel
            }
            Self::AiCapabilityImageGeneration => {
                I18nKey::SettingsSectionAiCapabilityImageGenerationLabel
            }
            Self::AiCapabilityVideoGeneration => {
                I18nKey::SettingsSectionAiCapabilityVideoGenerationLabel
            }
            Self::AiCapabilitySpeechSynthesis => {
                I18nKey::SettingsSectionAiCapabilitySpeechSynthesisLabel
            }
            Self::AiCapabilityMusicGeneration => {
                I18nKey::SettingsSectionAiCapabilityMusicGenerationLabel
            }
            Self::AiCapabilityMusicCover => I18nKey::SettingsSectionAiCapabilityMusicCoverLabel,
            Self::McpServers => I18nKey::SettingsSectionMcpServersLabel,
            Self::AboutLicenses => I18nKey::SettingsSectionAboutLicensesLabel,
            Self::ArchivedRestore => I18nKey::SettingsSectionArchivedRestoreLabel,
        }
    }

    /// Returns the localization key backing the selected settings-section description.
    /// 返回所选设置分区说明文案对应的本地化键。
    const fn description_key(self) -> I18nKey {
        match self {
            Self::ModelProviders => I18nKey::SettingsSectionModelProvidersDescription,
            Self::ModelList => I18nKey::SettingsSectionModelListDescription,
            Self::RuntimeDiagnostics => I18nKey::SettingsSectionRuntimeDiagnosticsDescription,
            Self::PromptComposer => I18nKey::SettingsSectionPromptComposerDescription,
            Self::AiCapabilityVendors => I18nKey::SettingsSectionAiCapabilityVendorsDescription,
            Self::AiCapabilityWebSearch => I18nKey::SettingsSectionAiCapabilityWebSearchDescription,
            Self::AiCapabilityUnderstandImage => {
                I18nKey::SettingsSectionAiCapabilityUnderstandImageDescription
            }
            Self::AiCapabilityUnderstandVideo => {
                I18nKey::SettingsSectionAiCapabilityUnderstandVideoDescription
            }
            Self::AiCapabilityUnderstandAudio => {
                I18nKey::SettingsSectionAiCapabilityUnderstandAudioDescription
            }
            Self::AiCapabilityImageGeneration => {
                I18nKey::SettingsSectionAiCapabilityImageGenerationDescription
            }
            Self::AiCapabilityVideoGeneration => {
                I18nKey::SettingsSectionAiCapabilityVideoGenerationDescription
            }
            Self::AiCapabilitySpeechSynthesis => {
                I18nKey::SettingsSectionAiCapabilitySpeechSynthesisDescription
            }
            Self::AiCapabilityMusicGeneration => {
                I18nKey::SettingsSectionAiCapabilityMusicGenerationDescription
            }
            Self::AiCapabilityMusicCover => {
                I18nKey::SettingsSectionAiCapabilityMusicCoverDescription
            }
            Self::McpServers => I18nKey::SettingsSectionMcpServersDescription,
            Self::AboutLicenses => I18nKey::SettingsSectionAboutLicensesDescription,
            Self::ArchivedRestore => I18nKey::SettingsSectionArchivedRestoreDescription,
        }
    }

    /// Returns the localized compact tree label rendered in the left settings navigation.
    /// 返回渲染在左侧设置导航中的本地化紧凑树标签。
    pub(super) fn localized_label(self, locale: AppLocale) -> String {
        text_for_locale(locale, self.label_key())
    }

    /// Returns the localized settings-page title rendered above the selected section content.
    /// 返回渲染在所选分区内容上方的本地化设置页标题。
    pub(super) fn localized_title(self, locale: AppLocale) -> String {
        self.localized_label(locale)
    }

    /// Returns one localized concise description for the selected settings section.
    /// 返回所选设置分区的一句本地化简洁说明。
    pub(super) fn localized_description(self, locale: AppLocale) -> String {
        text_for_locale(locale, self.description_key())
    }

    /// Returns whether the current section belongs to the collapsible AI capability subtree.
    /// 返回当前分区是否属于可折叠的 AI 能力子树。
    pub(super) const fn belongs_to_ai_capabilities(self) -> bool {
        matches!(
            self,
            Self::AiCapabilityVendors
                | Self::AiCapabilityWebSearch
                | Self::AiCapabilityUnderstandImage
                | Self::AiCapabilityUnderstandVideo
                | Self::AiCapabilityUnderstandAudio
                | Self::AiCapabilityImageGeneration
                | Self::AiCapabilityVideoGeneration
                | Self::AiCapabilitySpeechSynthesis
                | Self::AiCapabilityMusicGeneration
                | Self::AiCapabilityMusicCover
        )
    }

    /// Returns whether the current section belongs to the collapsible model-settings subtree.
    /// 返回当前分区是否属于可折叠的模型设置子树。
    pub(super) const fn belongs_to_model_settings(self) -> bool {
        matches!(self, Self::ModelProviders | Self::ModelList)
    }

    /// Returns the semantic capability kind represented by one settings section when the section manages one concrete capability.
    /// 返回某个设置分区所代表的语义能力类型；若该分区并不管理具体能力则返回空值。
    pub(super) const fn capability_kind(self) -> Option<AiToolKind> {
        match self {
            Self::AiCapabilityWebSearch => Some(AiToolKind::WebSearch),
            Self::AiCapabilityUnderstandImage => Some(AiToolKind::UnderstandImage),
            Self::AiCapabilityUnderstandVideo => None,
            Self::AiCapabilityUnderstandAudio => None,
            Self::AiCapabilityImageGeneration => Some(AiToolKind::ImageGeneration),
            Self::AiCapabilityVideoGeneration => Some(AiToolKind::VideoGeneration),
            Self::AiCapabilitySpeechSynthesis => Some(AiToolKind::SpeechSynthesis),
            Self::AiCapabilityMusicGeneration => Some(AiToolKind::MusicGeneration),
            Self::AiCapabilityMusicCover => Some(AiToolKind::MusicCover),
            _ => None,
        }
    }

    /// Returns whether the current settings section should expose a compact routing-strategy selector.
    /// 返回当前设置分区是否应暴露紧凑路由策略选择器。
    pub(super) const fn supports_routing_strategy(self) -> bool {
        matches!(
            self,
            Self::AiCapabilityWebSearch | Self::AiCapabilityUnderstandImage
        )
    }
}

/// Prompt-composer inspector layers shown in runtime diagnostics.
/// 运行诊断中展示的提示词组合检查器分层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PromptComposerInspectorLayer {
    /// System instruction layer sent to the provider when supported.
    /// 发送给 Provider 的系统指令层。
    System,
    /// Developer instruction layer used for personality and policy guidance.
    /// 用于人格与策略指引的开发者指令层。
    Developer,
    /// Tool metadata layer used for tool-help guidance.
    /// 用于工具帮助说明的工具元数据层。
    ToolMetadata,
    /// User-context layer used for project rules and runtime context.
    /// 用于项目规则与运行上下文的用户上下文层。
    UserContext,
}

impl PromptComposerInspectorLayer {
    /// Ordered list of all prompt-composer inspector layers.
    /// 提示词组合检查器全部分层的稳定顺序。
    pub(super) const ALL: [Self; 4] = [
        Self::System,
        Self::Developer,
        Self::ToolMetadata,
        Self::UserContext,
    ];

    /// Returns the Chinese display name for one layer.
    /// 返回单个分层的中文展示名称。
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::System => "系统层",
            Self::Developer => "开发者层",
            Self::ToolMetadata => "工具元数据层",
            Self::UserContext => "用户上下文层",
        }
    }

    /// Returns the debug target label used by prompt-composer segment reports.
    /// 返回提示词组合片段报告使用的调试目标层标签。
    pub(super) const fn target_label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Developer => "Developer",
            Self::ToolMetadata => "ToolMetadata",
            Self::UserContext => "User",
        }
    }
}

/// Returns display text for an inspector layer while preserving empty-layer visibility.
/// 返回检查器分层展示文本，并保证空分层仍然可见。
pub(super) fn prompt_composer_inspector_display_text(text: &str) -> String {
    if text.trim().is_empty() {
        String::from("（空）")
    } else {
        text.to_string()
    }
}

/// Builds a stable per-report key for one prompt-composer segment row.
/// 为单个提示词组合片段行构建稳定的单报告键。
pub(super) fn prompt_composer_inspector_segment_key(
    segment_index: usize,
    target_label: &str,
    node_id: &str,
) -> String {
    format!("{segment_index}:{target_label}:{node_id}")
}

/// Describes why the About-page license catalog is being reloaded so status copy can stay precise.
/// 描述关于页许可证目录重新加载的触发原因，以便状态文案保持准确。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AboutLicenseCatalogLoadTrigger {
    /// The user has just opened the About settings page and needs one fresh asynchronous load.
    /// 用户刚刚打开关于设置页，需要触发一次新的异步加载。
    OpenSection,
}

/// Returns the inline monochrome SVG bytes used by one left activity-bar panel switcher icon.
/// 返回左侧活动条某个面板切换图标使用的内联单色 SVG 字节。
pub(super) fn main_display_panel_icon_svg_bytes(panel: MainDisplayPanel) -> &'static [u8] {
    match panel {
        MainDisplayPanel::Main => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><rect x="3.75" y="4.25" width="6.5" height="15.5" rx="1.25"/><rect x="13.75" y="4.25" width="6.5" height="6.5" rx="1.25"/><rect x="13.75" y="13.25" width="6.5" height="6.5" rx="1.25"/></svg>"#,
        MainDisplayPanel::PromptSkills => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><path d="m12 3 1.1 3.4a2 2 0 0 0 1.3 1.3L17.8 9l-3.4 1.1a2 2 0 0 0-1.3 1.3L12 14.8l-1.1-3.4a2 2 0 0 0-1.3-1.3L6.2 9l3.4-1.3a2 2 0 0 0 1.3-1.3L12 3Z"/><path d="M5 16.5h5"/><path d="M7.5 14v5"/></svg>"#,
        MainDisplayPanel::LuaSkills => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 588.599 588.6" fill="currentColor" xml:space="preserve"><path d="m267.439 251.205-.377.011c-1.87 7.259-3.731 16.487-5.769 23.525l-7.38 25.676 27.886.105-7.977-25.956c-2.267-7.203-4.513-16.199-6.383-23.361"/><path d="M539.573 49.204h-178.2c-.791 0-1.561.076-2.342.124V0L11.227 46.417V540.98l347.804 47.62v-50.814c.781.053 1.551.116 2.342.116h178.2c20.851 0 37.8-16.965 37.8-37.8V86.999c0-20.839-16.95-37.795-37.8-37.795M107.19 348.648l-59.444-2.342V243.11l20.946-.933v85.018l38.499.907v20.546zm96.908-47.271c0 35.891-17.729 52.26-46.461 51.089-26.634-1.086-41.808-16.891-41.808-51.632v-60.756l23.361-1.039v63.825c0 19.079 6.874 28.962 19.246 29.278 12.912.326 20.126-8.976 20.126-29.131v-65.726l25.537-1.134v65.226zm91.955 54.707-10.049-32.016-36.12-.648-8.699 30.502-27.962-1.098 36.661-118.715 37.47-1.667 41.301 124.923zm259.72 144.018c0 8.933-7.268 16.199-16.2 16.199h-178.2c-.802 0-1.571-.11-2.342-.231V368.36l24.09 64.673 93.936-35.005 3.729-13.109-91.23 33.992-30.523-81.97V219.902l98.846-36.822 44.951 120.648c.611-1.561 1.36-3.068 2.352-4.439l2.046-7.162c.401-1.294.981-2.468 1.667-3.554l-44.561-119.62-105.301 39.242V71.046c.771-.114 1.54-.243 2.342-.243h178.2c8.933 0 16.2 7.27 16.2 16.2v413.099z"/><path d="m521.242 339.926 11.386-38.043a9.3 9.3 0 0 0 .29-3.918l2.942-9.039c.454-1.487-.158-2.837-1.255-3.953.147-.287.348-.52.453-.833.718-2.289-.569-4.714-2.857-5.432l-3.85-1.216c-2.278-.696-4.726.567-5.432 2.858-.074.222-.043.454-.074.686-1.413.211-2.562.82-3.006 2.223l-2.668 9.397c-1.065 1.052-1.915 2.344-2.368 3.881l-11.401 38.045c-.585 1.967-.464 3.971.18 5.774l-15.504 54.422c-1.018 3.354-2.985 18.277.38 19.312 3.364 1.012 10.763-12.234 11.791-15.589l17.439-53.715c1.64-1.142 2.932-2.804 3.554-4.86m8.443 12.303a20.1 20.1 0 0 1-4.515 7.905l-.152.464 1.688 4.514-3.586 1.34-4.335 13.331 22.043-8.216-9.387-25.186zm-89.321-32.764c-2.036-.664-3.428-2.441-3.565-4.557l-.363-5.579a5.095 5.095 0 0 1 2.9-4.968c.675-.326 1.191-.896 1.413-1.561s.158-1.424-.158-2.046a5.2 5.2 0 0 1 .686-5.764l3.681-4.208c.928-1.041 2.173-1.655 3.502-1.748l.938-.065 1.075.34c.844.309 1.54.232 2.142-.076a2.7 2.7 0 0 0 1.36-1.572c.611-1.917 2.246-3.238 4.208-3.499l-.248-3.671a1.01 1.01 0 0 0-.675-.854 15.44 15.44 0 0 1-9.181-7.868 15.5 15.5 0 0 1-.696-12.095.95.95 0 0 0-.259-1.034l-8.886-7.768c-.495-.433-.807-.256-1.07-.126a15.43 15.43 0 0 1-12.061.931c-3.965-1.329-7.251-4.285-9.044-8.068-.152-.346-.532-.567-.928-.541l-11.755.789c-.385.01-.728.288-.849.643a15.46 15.46 0 0 1-7.868 9.194c-3.729 1.87-8.179 2.112-12.107.675-.243-.073-.596-.197-1.028.288l-7.773 8.872a.96.96 0 0 0-.116 1.076 15.5 15.5 0 0 1 .929 12.062 15.5 15.5 0 0 1-8.063 9.028.98.98 0 0 0-.554.928l.791 11.773a.95.95 0 0 0 .654.839c3.997 1.255 7.341 4.129 9.207 7.867a15.58 15.58 0 0 1 .665 12.108.96.96 0 0 0 .273 1.033l8.881 7.773c.485.422.812.253 1.065.11a15.4 15.4 0 0 1 6.096-1.703c2.046-.137 4.05.11 5.98.785a15.5 15.5 0 0 1 9.028 8.059.97.97 0 0 0 .928.537l11.76-.78c.4-.026.728-.29.844-.653a15.5 15.5 0 0 1 7.879-9.197 15.5 15.5 0 0 1 11.76-.785 5.1 5.1 0 0 1 .284-1.487 2.7 2.7 0 0 0-.105-2.099 2.68 2.68 0 0 0-1.577-1.338m-26.516-7.146c-12.613.828-23.556-8.743-24.395-21.357-.854-12.614 8.717-23.572 21.342-24.413 12.63-.844 23.572 8.754 24.416 21.368.844 12.616-8.733 23.559-21.363 24.402"/><path d="M482.888 304.826a7.34 7.34 0 0 1-4.351-3.729 7.34 7.34 0 0 1-.327-5.729.47.47 0 0 0-.126-.499l-4.192-3.681c-.243-.2-.396-.113-.517-.065a7.35 7.35 0 0 1-5.722.453 7.4 7.4 0 0 1-4.282-3.828.45.45 0 0 0-.433-.264l-5.579.377c-.174.021-.343.137-.385.309a7.43 7.43 0 0 1-3.729 4.369 7.35 7.35 0 0 1-5.732.319c-.121-.045-.301-.108-.501.124l-3.691 4.198a.49.49 0 0 0-.063.517 7.34 7.34 0 0 1 .454 5.716 7.4 7.4 0 0 1-3.818 4.282.46.46 0 0 0-.274.443l.38 5.568c.011.19.132.349.316.401a7.33 7.33 0 0 1 4.346 3.733 7.3 7.3 0 0 1 .337 5.727.48.48 0 0 0 .116.496l4.219 3.681c.232.2.38.116.496.053a7.4 7.4 0 0 1 2.9-.807 7.5 7.5 0 0 1 2.815.374 7.28 7.28 0 0 1 4.282 3.818.46.46 0 0 0 .443.258l5.568-.369a.45.45 0 0 0 .396-.306 7.3 7.3 0 0 1 3.729-4.361c1.771-.886 3.876-.991 5.742-.326.111.042.29.095.491-.116l3.681-4.214a.51.51 0 0 0 .063-.512 7.3 7.3 0 0 1-.443-5.716 7.3 7.3 0 0 1 3.818-4.282.46.46 0 0 0 .264-.433l-.369-5.579a.45.45 0 0 0-.322-.4m-20.56 15.493c-5.97.396-11.159-4.15-11.56-10.125-.391-5.97 4.134-11.169 10.125-11.575 5.964-.38 11.153 4.16 11.56 10.135.4 5.975-4.135 11.154-10.125 11.565"/></svg>"#,
        MainDisplayPanel::Automation => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8"/><path d="M12 7.5v5l3 2"/></svg>"#,
        MainDisplayPanel::Agents => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="currentColor"><path d="M16 19a6.99 6.99 0 0 1-5.833-3.129l1.666-1.107a5 5 0 0 0 8.334 0l1.666 1.107A6.99 6.99 0 0 1 16 19m4-11a2 2 0 1 0 2 2 1.98 1.98 0 0 0-2-2m-8 0a2 2 0 1 0 2 2 1.98 1.98 0 0 0-2-2"/><path d="M17.736 30 16 29l4-7h6a1.997 1.997 0 0 0 2-2V6a1.997 1.997 0 0 0-2-2H6a1.997 1.997 0 0 0-2 2v14a1.997 1.997 0 0 0 2 2h9v2H6a4 4 0 0 1-4-4V6a4 4 0 0 1 4-4h20a4 4 0 0 1 4 4v14a4 4 0 0 1-4 4h-4.835Z"/><path fill="none" d="M0 0h32v32H0z"/></svg>"#,
        MainDisplayPanel::Plugins => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="6.5" height="6.5" rx="1.25"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.25"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.25"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.25"/></svg>"#,
        MainDisplayPanel::VulcanMemoryMesh => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" fill="currentColor"><path d="M31.332 11.671c-1.15-3.016-4.669-8.072-15.304-8.072-8.681 0-15.865 5.678-16.026 10.38-.042 1.236.421 4.045 5.313 4.726-.053.274-.079.57-.079.894 0 2.005 2.285 4.8 6.005 4.8 1.288 0 2.248-.197 3.052-.498l3.375 4.2a.796.796 0 0 0 .622.299c.176 0 .354-.058.5-.177a.797.797 0 0 0 .122-1.125l-3.154-3.925c.263-.161.521-.326.779-.495.263-.172.53-.345.81-.515l4.426 5.917a.799.799 0 0 0 .639.321.79.79 0 0 0 .478-.159.8.8 0 0 0 .161-1.121l-.909-1.215c1.03-.006 2.14-.292 3.125-.774 2.339-1.145 3.501-3.046 3-4.804.77-.263 1.586-.77 2.657-1.732 1.092-.981 1.558-3.908.409-6.923m-6.765 12.022c-1.537.749-3.486.919-4.134-.072l-1.643-2.196c.763-.317 1.68-.589 2.898-.766 1.746-.256 2.917-.178 3.862-.117.416.028.79.043 1.151.038.006.023.003.047.012.071.44 1.161-.76 2.364-2.146 3.042m5.291-6.289c-1.887 1.696-2.571 1.65-4.203 1.542q-.441-.03-.945-.05c-.231-.7-.602-1.329-.955-1.917-.813-1.359-1.244-2.189-.63-3.422a.8.8 0 0 0-.357-1.074.79.79 0 0 0-1.07.358c-1.036 2.078-.079 3.677.689 4.961.23.384.422.751.586 1.117-.46.029-.952.073-1.514.155-3.012.439-4.49 1.41-5.794 2.264-1.245.815-2.229 1.461-4.423 1.461-2.852 0-4.409-2.114-4.409-3.2 0-2.239 2.124-2.4 5.621-2.4 2.371 0 4.61 0 5.696-1.353.558-.694.71-1.642.453-2.817a.798.798 0 0 0-.779-.63c-.056 0-.114.006-.17.019a.8.8 0 0 0-.61.951c.148.679.101 1.175-.139 1.474-.606.756-2.65.756-4.452.756-2.235 0-5.074 0-6.43 1.58-1.866-.19-4.504-.862-4.426-3.145.038-1.093.684-2.373 1.802-3.614.262 1.498 1.164 2.727 2.224 3.143.259.1.544.16.84.16.828 0 1.738-.462 2.388-1.764a.8.8 0 0 0-.357-1.074.79.79 0 0 0-1.07.358c-.362.728-.769 1-1.22.83-.718-.282-1.499-1.629-1.229-3.118 1.981-1.574 4.717-2.907 7.88-3.467-.173.562-.545 1.232-.858 1.777-.628 1.089-1.169 2.031-.708 2.834.27.466.79.701 1.549.701.441 0 .798-.358.798-.8 0-.462-.431-.763-.857-.802.125-.309.401-.789.6-1.136.507-.883 1.063-1.853 1.163-2.802.488-.045.987-.061 1.489-.061 9.68 0 12.809 4.412 13.813 7.042.951 2.495.51 4.719.016 5.163"/><path d="M22.768 8.318c-2.17-.852-4.817-.769-5.858 1.323a.8.8 0 0 0 .357 1.074c.397.2.873.039 1.07-.358.59-1.181 2.41-1.111 3.85-.55 1.446.567 2.809 1.773 2.486 2.896a.8.8 0 0 0 .546.99.8.8 0 0 0 .221.031c.348 0 .666-.228.767-.578.618-2.15-1.349-4.011-3.438-4.828"/></svg>"#,
        MainDisplayPanel::Example => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.85" stroke-linecap="round" stroke-linejoin="round"><rect x="3.75" y="4.25" width="6.5" height="15.5" rx="1.25"/><rect x="13.75" y="4.25" width="6.5" height="4.75" rx="1.25"/><rect x="13.75" y="11.5" width="6.5" height="8.25" rx="1.25"/></svg>"#,
        MainDisplayPanel::Settings => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" fill="currentColor"><path d="M32 18a14 14 0 1 0 14 14 14.015 14.015 0 0 0-14-14m0 24a10 10 0 1 1 10-10 10.01 10.01 0 0 1-10 10"/><path d="M60 25h-7.163a22 22 0 0 0-1.153-2.784l5.065-5.066a2 2 0 0 0 0-2.828l-7.071-7.071a2 2 0 0 0-2.828 0l-5.065 5.064A22 22 0 0 0 39 11.163V4a2 2 0 0 0-2-2H27a2 2 0 0 0-2 2v7.163a22 22 0 0 0-2.785 1.152L17.15 7.251a2 2 0 0 0-2.828 0l-7.071 7.071a2 2 0 0 0 0 2.828l5.065 5.066A22 22 0 0 0 11.163 25H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h7.163a22 22 0 0 0 1.152 2.784L7.251 46.85a2 2 0 0 0 0 2.828l7.071 7.071a2 2 0 0 0 2.828 0l5.066-5.065A22 22 0 0 0 25 52.837V60a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-7.163a22 22 0 0 0 2.784-1.153l5.066 5.065a2 2 0 0 0 2.828 0l7.071-7.071a2 2 0 0 0 0-2.828l-5.064-5.066A22 22 0 0 0 52.837 39H60a2 2 0 0 0 2-2V27a2 2 0 0 0-2-2m-2 10h-6.655a2 2 0 0 0-1.937 1.5 18 18 0 0 1-1.915 4.626 2 2 0 0 0 .308 2.432l4.706 4.706-4.243 4.243-4.706-4.707a2 2 0 0 0-2.431-.309 18 18 0 0 1-4.627 1.917 2 2 0 0 0-1.5 1.937V58h-6v-6.655a2 2 0 0 0-1.5-1.937 18 18 0 0 1-4.627-1.916 2 2 0 0 0-2.431.309l-4.706 4.706-4.243-4.243 4.707-4.706a2 2 0 0 0 .308-2.432 18 18 0 0 1-1.916-4.626 2 2 0 0 0-1.937-1.5H6v-6h6.655a2 2 0 0 0 1.937-1.5 18 18 0 0 1 1.916-4.627 2 2 0 0 0-.309-2.431l-4.706-4.706 4.243-4.243 4.706 4.707a2 2 0 0 0 2.432.308 17.8 17.8 0 0 1 4.626-1.916 2 2 0 0 0 1.5-1.937V6h6v6.655a2 2 0 0 0 1.5 1.937 17.8 17.8 0 0 1 4.626 1.915 2 2 0 0 0 2.432-.308l4.706-4.706 4.243 4.243-4.707 4.706a2 2 0 0 0-.309 2.431 18 18 0 0 1 1.917 4.627 2 2 0 0 0 1.937 1.5H58Z"/><path d="M26 30h4v4h-4zm8 0h4v4h-4z"/></svg>"#,
    }
}
