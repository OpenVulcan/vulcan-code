//! VMM settings view helpers shared by global and project configuration surfaces.
//! 全局与项目配置界面共享的 VMM 设置视图辅助。

use super::*;
use iced::widget::column;

impl VulcanWorkbench {
    /// Renders the top-level VulcanMemoryMesh configuration panel.
    /// 渲染顶层 VulcanMemoryMesh 配置面板。
    pub(super) fn view_vulcan_memory_mesh_panel(&self) -> Element<'_, Message> {
        let selected_session = self.selected_session();
        let selected_session_id = selected_session.map(|session| session.id);
        let selected_project_id = selected_session.and_then(|session| session.project_id.clone());
        let selected_project = selected_project_id
            .as_deref()
            .and_then(|project_id| self.task_project_by_id(project_id));
        let session_layer = selected_session_id
            .and_then(|session_id| self.session_runtime_state.get(&session_id))
            .map(|state| &state.vmm);
        let project_layer = self.current_project_vmm_configuration(self.selected_session);
        let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
            &self.provider_configuration.vmm,
            Some(&project_layer),
            session_layer,
        );
        let diagnostic = effective.diagnose_memory_flow(!effective.grpc_target.trim().is_empty());
        let project_button: Element<'_, Message> = if let Some(project) = selected_project {
            button("打开当前项目设置")
                .style(button::secondary)
                .on_press(Message::OpenTaskProjectSettings(project.id.clone()))
                .into()
        } else {
            button("打开当前项目设置").style(button::secondary).into()
        };
        let effective_card = container(
            column![
                row![
                    column![
                        text("当前会话有效配置").size(16),
                        text("这里展示全局、项目和会话覆盖合并后的实际 VMM 状态。").size(12),
                    ]
                    .spacing(3)
                    .width(Fill),
                    project_button,
                ]
                .spacing(10)
                .align_y(Center),
                row![
                    text(format!(
                        "gRPC：{}",
                        if effective.grpc_target.trim().is_empty() {
                            "未配置"
                        } else {
                            effective.grpc_target.as_str()
                        }
                    ))
                    .size(12),
                    text(format!(
                        "user/project：{}/{}",
                        effective
                            .user_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| String::from("-")),
                        effective
                            .project_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| String::from("-")),
                    ))
                    .size(12),
                    text(format!(
                        "记忆：{}",
                        if effective.enabled {
                            "启用"
                        } else {
                            "关闭"
                        }
                    ))
                    .size(12),
                    text(format!(
                        "AI 工具：{}",
                        if effective.ai_tools_enabled {
                            "启用"
                        } else {
                            "关闭"
                        }
                    ))
                    .size(12),
                ]
                .spacing(12)
                .wrap(),
                text(format!("诊断：{diagnostic:?}")).size(12),
                text(format!(
                    "当前项目：{}",
                    selected_project
                        .map(|project| project.title.as_str())
                        .unwrap_or("未绑定项目")
                ))
                .size(12),
            ]
            .spacing(10),
        )
        .padding(12)
        .style(container::bordered_box);

        container(
            column![
                container(
                    column![
                        text(
                            MainDisplayPanel::VulcanMemoryMesh
                                .localized_section_title(self.current_locale())
                        )
                        .size(22),
                        text(
                            MainDisplayPanel::VulcanMemoryMesh
                                .localized_description(self.current_locale())
                        )
                        .size(13),
                    ]
                    .spacing(4),
                )
                .padding([4, 2]),
                scrollable(
                    column![effective_card, self.view_global_vmm_settings_card()].spacing(12)
                )
                .height(Fill),
            ]
            .spacing(12)
            .height(Fill),
        )
        .padding(16)
        .height(Fill)
        .width(Fill)
        .into()
    }

    /// Renders the global VMM configuration card inside the AI extension settings surface.
    /// 在 AI 扩展设置界面中渲染全局 VMM 配置卡片。
    pub(super) fn view_global_vmm_settings_card(&self) -> Element<'_, Message> {
        container(
            column![
                row![
                    column![
                        text("VulcanMemoryMesh").size(16),
                        text("配置 gRPC 地址、业务标识与注入策略；项目与会话可继续覆盖。").size(12),
                    ]
                    .spacing(3)
                    .width(Fill),
                    button("重载")
                        .style(button::secondary)
                        .on_press(Message::ReloadGlobalVmmConfiguration),
                    button("保存").on_press(Message::SaveGlobalVmmConfiguration),
                ]
                .spacing(10)
                .align_y(Center),
                self.view_vmm_settings_fields(
                    &self.vmm_settings_draft,
                    VmmSettingsMessageMap {
                        enabled: Message::GlobalVmmEnabledChanged,
                        grpc_target: Message::GlobalVmmGrpcTargetChanged,
                        api_key: Message::GlobalVmmApiKeyChanged,
                        user_id: Message::GlobalVmmUserIdChanged,
                        project_id: Message::GlobalVmmProjectIdChanged,
                        management_user_ref: Message::GlobalVmmManagementUserRefChanged,
                        management_project_ref: Message::GlobalVmmManagementProjectRefChanged,
                        implicit_turns: Message::GlobalVmmImplicitTurnsChanged,
                        profile_refresh_turns: Message::GlobalVmmProfileRefreshTurnsChanged,
                        visible_memory: Message::GlobalVmmVisibleMemoryChanged,
                        compact_recall: Message::GlobalVmmSessionCompactRecallChanged,
                        ai_tools: Message::GlobalVmmAiToolsChanged,
                        injection_tag: Message::GlobalVmmInjectionTagChanged,
                        injection_description: Message::GlobalVmmInjectionDescriptionChanged,
                    },
                ),
                self.view_vmm_management_buttons(Message::RunGlobalVmmManagement),
            ]
            .spacing(12),
        )
        .padding(12)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the project VMM override card inside the project settings modal.
    /// 在项目设置弹窗中渲染项目级 VMM 覆盖卡片。
    pub(super) fn view_project_vmm_settings_card(&self) -> Element<'_, Message> {
        let draft = &self.project_vmm_settings_draft;
        let fields = if draft.project_override_enabled {
            self.view_vmm_settings_fields(
                &draft.settings,
                VmmSettingsMessageMap {
                    enabled: Message::ProjectVmmEnabledChanged,
                    grpc_target: Message::ProjectVmmGrpcTargetChanged,
                    api_key: Message::ProjectVmmApiKeyChanged,
                    user_id: Message::ProjectVmmUserIdChanged,
                    project_id: Message::ProjectVmmProjectIdChanged,
                    management_user_ref: Message::ProjectVmmManagementUserRefChanged,
                    management_project_ref: Message::ProjectVmmManagementProjectRefChanged,
                    implicit_turns: Message::ProjectVmmImplicitTurnsChanged,
                    profile_refresh_turns: Message::ProjectVmmProfileRefreshTurnsChanged,
                    visible_memory: Message::ProjectVmmVisibleMemoryChanged,
                    compact_recall: Message::ProjectVmmSessionCompactRecallChanged,
                    ai_tools: Message::ProjectVmmAiToolsChanged,
                    injection_tag: Message::ProjectVmmInjectionTagChanged,
                    injection_description: Message::ProjectVmmInjectionDescriptionChanged,
                },
            )
        } else {
            container(text("当前继承全局 VMM 配置。开启项目覆盖后可为此项目设置独立 user/project id 或关闭记忆。").size(12))
                .padding(10)
                .style(container::rounded_box)
                .into()
        };
        container(
            column![
                row![
                    column![
                        text("项目级 VMM 记忆").size(14),
                        text("未开启覆盖时继承全局配置；开启后当前项目优先。").size(12),
                    ]
                    .spacing(3)
                    .width(Fill),
                    checkbox(draft.project_override_enabled)
                        .on_toggle(Message::ProjectVmmOverrideToggled),
                    button("重载")
                        .style(button::secondary)
                        .on_press(Message::ReloadProjectVmmConfiguration),
                    button("保存").on_press(Message::SaveProjectVmmConfiguration),
                ]
                .spacing(10)
                .align_y(Center),
                fields,
                self.view_vmm_management_buttons(Message::RunProjectVmmManagement),
            ]
            .spacing(12),
        )
        .padding(12)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the common VMM field grid.
    /// 渲染通用 VMM 字段网格。
    fn view_vmm_settings_fields<'a>(
        &'a self,
        draft: &'a VmmSettingsDraft,
        messages: VmmSettingsMessageMap,
    ) -> Element<'a, Message> {
        column![
            row![
                checkbox(draft.enabled).on_toggle(messages.enabled),
                text("启用记忆连接").size(13),
                space::horizontal(),
                checkbox(draft.ai_tools_enabled).on_toggle(messages.ai_tools),
                text("启用 AI 专属工具").size(13),
            ]
            .spacing(8)
            .align_y(Center),
            row![
                self.view_vmm_text_field(
                    "gRPC 地址",
                    "127.0.0.1:50051",
                    &draft.grpc_target,
                    messages.grpc_target,
                ),
                self.view_vmm_text_field(
                    "API Key",
                    "可留空",
                    &draft.grpc_api_key,
                    messages.api_key
                ),
            ]
            .spacing(10),
            row![
                self.view_vmm_text_field("用户 ID", "数字", &draft.user_id, messages.user_id),
                self.view_vmm_text_field("项目 ID", "数字", &draft.project_id, messages.project_id),
            ]
            .spacing(10),
            row![
                self.view_vmm_text_field(
                    "管理用户引用",
                    "ID 或用户名",
                    &draft.management_user_ref,
                    messages.management_user_ref,
                ),
                self.view_vmm_text_field(
                    "管理项目引用",
                    "ID 或 Team/Space/Project",
                    &draft.management_project_ref,
                    messages.management_project_ref,
                ),
            ]
            .spacing(10),
            row![
                self.view_vmm_text_field(
                    "临时记忆 TTL",
                    "3",
                    &draft.implicit_memory_turns,
                    messages.implicit_turns,
                ),
                self.view_vmm_text_field(
                    "画像刷新间隔",
                    "5",
                    &draft.profile_refresh_turns,
                    messages.profile_refresh_turns,
                ),
            ]
            .spacing(10),
            row![
                checkbox(draft.visible_memory_injection).on_toggle(messages.visible_memory),
                text("可见记忆模式").size(13),
                space::horizontal(),
                checkbox(draft.session_compact_recall).on_toggle(messages.compact_recall),
                text("压缩感知召回").size(13),
            ]
            .spacing(8)
            .align_y(Center),
            self.view_vmm_text_field(
                "注入标签",
                "vmm_inject",
                &draft.injection_tag,
                messages.injection_tag
            ),
            column![
                text("注入说明").size(12),
                text_input("用于包裹召回内容的说明文本", &draft.injection_description)
                    .on_input(messages.injection_description)
                    .padding(8)
                    .width(Fill),
            ]
            .spacing(5),
            text(&draft.notice).size(12),
        ]
        .spacing(10)
        .into()
    }

    /// Renders one labeled VMM text input field.
    /// 渲染一个带标签的 VMM 文本输入字段。
    fn view_vmm_text_field<'a>(
        &'a self,
        label: &'a str,
        placeholder: &'a str,
        value: &'a str,
        message: fn(String) -> Message,
    ) -> Element<'a, Message> {
        column![
            text(label).size(12),
            text_input(placeholder, value)
                .on_input(message)
                .padding(8)
                .width(Fill),
        ]
        .spacing(5)
        .width(Fill)
        .into()
    }

    /// Renders VMM management action buttons for one settings surface.
    /// 为一个设置界面渲染 VMM 管理动作按钮。
    fn view_vmm_management_buttons(
        &self,
        message: fn(VmmManagementAction) -> Message,
    ) -> Element<'_, Message> {
        row![
            button(VmmManagementAction::Healthz.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::Healthz)),
            button(VmmManagementAction::ListUsersAndProjects.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::ListUsersAndProjects)),
            button(VmmManagementAction::ResolveUserAndProject.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::ResolveUserAndProject)),
            button(VmmManagementAction::EnsureProjectPreview.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::EnsureProjectPreview)),
            button(VmmManagementAction::ProfileBundlePreview.label())
                .style(button::secondary)
                .on_press(message(VmmManagementAction::ProfileBundlePreview)),
        ]
        .spacing(8)
        .wrap()
        .into()
    }
}

/// Message constructors used by the reusable VMM field renderer.
/// 可复用 VMM 字段渲染器使用的消息构造器集合。
#[derive(Debug, Clone, Copy)]
struct VmmSettingsMessageMap {
    /// Enabled-toggle message constructor.
    /// 启用开关消息构造器。
    enabled: fn(bool) -> Message,
    /// gRPC target message constructor.
    /// gRPC 目标消息构造器。
    grpc_target: fn(String) -> Message,
    /// API-key message constructor.
    /// API key 消息构造器。
    api_key: fn(String) -> Message,
    /// User-id message constructor.
    /// 用户标识消息构造器。
    user_id: fn(String) -> Message,
    /// Project-id message constructor.
    /// 项目标识消息构造器。
    project_id: fn(String) -> Message,
    /// Management user-reference message constructor.
    /// 管理用户引用消息构造器。
    management_user_ref: fn(String) -> Message,
    /// Management project-reference message constructor.
    /// 管理项目引用消息构造器。
    management_project_ref: fn(String) -> Message,
    /// Transient TTL message constructor.
    /// 临时 TTL 消息构造器。
    implicit_turns: fn(String) -> Message,
    /// Profile refresh interval message constructor.
    /// 画像刷新间隔消息构造器。
    profile_refresh_turns: fn(String) -> Message,
    /// Visible-memory toggle message constructor.
    /// 可见记忆开关消息构造器。
    visible_memory: fn(bool) -> Message,
    /// Compact-recall toggle message constructor.
    /// 压缩召回开关消息构造器。
    compact_recall: fn(bool) -> Message,
    /// AI-tool exposure toggle message constructor.
    /// AI 工具暴露开关消息构造器。
    ai_tools: fn(bool) -> Message,
    /// Injection-tag message constructor.
    /// 注入标签消息构造器。
    injection_tag: fn(String) -> Message,
    /// Injection-description message constructor.
    /// 注入说明消息构造器。
    injection_description: fn(String) -> Message,
}
