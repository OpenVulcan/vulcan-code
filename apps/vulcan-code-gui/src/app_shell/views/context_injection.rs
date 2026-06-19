//! Context-injection side panel view composition.
//! 上下文注入侧面板视图组合逻辑。

use super::*;
use iced::widget::{column, row, tooltip};
use vulcan_context_injection::{InjectionKind, InjectionPanelCard, InjectionPanelItem};

/// Stable profile card tag shown on the static context tab.
/// 静态上下文页签中展示画像卡片使用的稳定标签。
const STATIC_VMM_PROFILE_CARD_TAG: &str = "vmm_profile";

impl VulcanWorkbench {
    /// Builds the read-only context-injection panel for the selected session.
    /// 构建当前选中会话的只读上下文注入面板。
    pub(super) fn view_session_context_injection_panel(&self) -> Element<'_, Message> {
        let header = container(
            column![
                text("上下文注入").size(18),
                text("当前会话仍有效的隐藏上下文内容。").size(12),
                self.view_context_injection_panel_tabs(),
            ]
            .spacing(8),
        )
        .padding([4, 2]);

        let cards = self.context_injection_cards_for_active_tab();

        let body: Element<'_, Message> = if cards.is_empty() {
            container(column![text(self.context_injection_empty_text()).size(14)].spacing(8))
                .padding(16)
                .style(container::bordered_box)
                .width(Fill)
                .into()
        } else {
            let card_list = cards
                .into_iter()
                .fold(column![].spacing(12), |column, card| {
                    column.push(self.view_context_injection_card(card))
                });
            scrollable(card_list).height(Fill).into()
        };

        column![header, body].spacing(12).height(Fill).into()
    }

    /// Builds the dynamic/static switch row for the context-injection panel.
    /// 构建上下文注入面板的动态/静态切换行。
    fn view_context_injection_panel_tabs(&self) -> Element<'_, Message> {
        row![
            self.view_context_injection_panel_tab_button(ContextInjectionPanelTab::Dynamic, "动态"),
            self.view_context_injection_panel_tab_button(ContextInjectionPanelTab::Static, "静态"),
        ]
        .spacing(8)
        .into()
    }

    /// Builds one tab button for the context-injection side panel.
    /// 构建上下文注入侧栏中的单个页签按钮。
    fn view_context_injection_panel_tab_button(
        &self,
        tab: ContextInjectionPanelTab,
        label: &'static str,
    ) -> Element<'_, Message> {
        let mut tab_button = button(text(label).size(12)).padding([6, 14]);
        tab_button = if self.context_injection_panel_tab == tab {
            tab_button.style(button::primary)
        } else {
            tab_button.style(button::secondary)
        };
        tab_button
            .on_press(Message::ContextInjectionPanelTabSelected(tab))
            .into()
    }

    /// Returns the panel cards that belong to the active context-injection tab.
    /// 返回当前上下文注入页签对应的卡片集合。
    fn context_injection_cards_for_active_tab(&self) -> Vec<InjectionPanelCard> {
        let Some(session) = self.selected_session() else {
            return Vec::new();
        };
        let Some(state) = self.session_runtime_state(session.id) else {
            return Vec::new();
        };
        match self.context_injection_panel_tab {
            ContextInjectionPanelTab::Dynamic => {
                crate::agent_runtime::context_injection_runtime::context_injection_panel_cards_for_kind(
                    &state.context_injection_state,
                    Some(InjectionKind::Transient),
                )
            }
            ContextInjectionPanelTab::Static => {
                let mut cards = static_vmm_profile_panel_card(
                    state.vmm_profile_sync.base_profile_content.as_deref(),
                    state.vmm_profile_sync.updated_at.as_deref(),
                )
                .into_iter()
                .collect::<Vec<_>>();
                cards.extend(
                    crate::agent_runtime::context_injection_runtime::context_injection_panel_cards_for_kind(
                        &state.context_injection_state,
                        Some(InjectionKind::Persistent),
                    ),
                );
                cards
            }
        }
    }

    /// Returns the empty-state copy for the active context-injection tab.
    /// 返回当前上下文注入页签对应的空状态文案。
    fn context_injection_empty_text(&self) -> &'static str {
        match self.context_injection_panel_tab {
            ContextInjectionPanelTab::Dynamic => "当前会话没有有效的动态 TTL 注入。",
            ContextInjectionPanelTab::Static => "当前会话没有有效的静态上下文。",
        }
    }

    /// Builds one tag card for active context-injection items.
    /// 为活动上下文注入内容构建单个标签卡片。
    fn view_context_injection_card(&self, card: InjectionPanelCard) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let mut item_list = column![].spacing(8);
        for item in card.items {
            let turn_label = item
                .turn_label
                .as_deref()
                .filter(|label| !label.trim().is_empty())
                .map(str::to_string);
            let source_label = item
                .source_display
                .as_deref()
                .filter(|source| !source.trim().is_empty())
                .map(|source| format!("来源：{source} · {}", item.ttl_label))
                .unwrap_or(item.ttl_label);
            let meta_label = turn_label
                .map(|turn| format!("{turn} · {source_label}"))
                .unwrap_or(source_label);
            let item_box = container(
                column![
                    text(item.content)
                        .size(13)
                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                    text(meta_label)
                        .size(11)
                        .color(palette.background.base.text.scale_alpha(0.54)),
                ]
                .spacing(8),
            )
            .padding([10, 12])
            .width(Fill)
            .style(container::rounded_box);
            let detail = item.detail.trim().to_string();
            let item_element: Element<'_, Message> = if detail.is_empty() {
                item_box.into()
            } else {
                tooltip(
                    item_box,
                    container(
                        text(detail)
                            .size(12)
                            .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                    )
                    .padding(10)
                    .style(container::bordered_box)
                    .width(iced::Length::Fixed(320.0)),
                    tooltip::Position::Left,
                )
                .into()
            };
            item_list = item_list.push(item_element);
        }

        container(
            column![
                text(card.tag).size(18),
                text(card.description)
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.64))
                    .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                item_list,
            ]
            .spacing(10),
        )
        .padding(16)
        .width(Fill)
        .style(container::bordered_box)
        .into()
    }
}

/// Builds the static VMM profile card from cached profile sync state.
/// 根据缓存的画像同步状态构建静态 VMM 画像卡片。
fn static_vmm_profile_panel_card(
    base_profile_content: Option<&str>,
    updated_at: Option<&str>,
) -> Option<InjectionPanelCard> {
    let content = base_profile_content?.trim();
    if content.is_empty() {
        return None;
    }
    Some(InjectionPanelCard {
        tag: String::from(STATIC_VMM_PROFILE_CARD_TAG),
        description: String::from(
            "VMM 画像基点。该静态上下文由系统提示词 hook 持续复用，面板仅用于查看。",
        ),
        items: vec![InjectionPanelItem {
            content: compact_panel_preview(content),
            detail: static_vmm_profile_detail(content, updated_at),
            dedupe_key: None,
            memory_id: None,
            turn_id: None,
            score_label: None,
            has_dialogue: false,
            created_datetime: None,
            updated_at: updated_at.map(str::to_string),
            turn_label: None,
            ttl_label: String::from("静态"),
            source_display: Some(String::from("VMM profile")),
        }],
    })
}

/// Builds the tooltip detail for a static VMM profile card.
/// 构建静态 VMM 画像卡片的悬停详情。
fn static_vmm_profile_detail(content: &str, updated_at: Option<&str>) -> String {
    let mut lines = Vec::new();
    if let Some(updated_at) = updated_at.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("更新时间：{updated_at}"));
    }
    lines.push(String::from("来源：VMM profile system hook"));
    lines.push(String::from("内容："));
    lines.push(content.to_string());
    lines.join("\n")
}

/// Builds a compact one-line-ish preview for long static context content.
/// 为较长静态上下文内容构建紧凑预览。
fn compact_panel_preview(content: &str) -> String {
    let compact = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("\n");
    if content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
        > 6
    {
        format!("{compact}\n...")
    } else {
        compact
    }
}
