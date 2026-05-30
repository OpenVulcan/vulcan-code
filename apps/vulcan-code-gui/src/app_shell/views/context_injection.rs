//! Context-injection side panel view composition.
//! 上下文注入侧面板视图组合逻辑。

use super::*;
use iced::widget::{column, tooltip};

impl VulcanWorkbench {
    /// Builds the read-only context-injection panel for the selected session.
    /// 构建当前选中会话的只读上下文注入面板。
    pub(super) fn view_session_context_injection_panel(&self) -> Element<'_, Message> {
        let header = container(
            column![
                text("上下文注入").size(18),
                text("当前会话仍有效的隐藏上下文内容。").size(12),
            ]
            .spacing(4),
        )
        .padding([4, 2]);

        let cards = self
            .selected_session()
            .and_then(|session| self.session_runtime_state(session.id))
            .map(|state| {
                crate::agent_runtime::context_injection_runtime::context_injection_panel_cards(
                    &state.context_injection_state,
                )
            })
            .unwrap_or_default();

        let body: Element<'_, Message> = if cards.is_empty() {
            container(column![text("当前会话没有有效的上下文注入。").size(14)].spacing(8))
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

    /// Builds one tag card for active context-injection items.
    /// 为活动上下文注入内容构建单个标签卡片。
    fn view_context_injection_card(
        &self,
        card: vulcan_context_injection::InjectionPanelCard,
    ) -> Element<'_, Message> {
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
