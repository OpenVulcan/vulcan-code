//! About-page license view composition helpers.
//! 关于页许可证视图组合辅助逻辑。

use super::*;
use iced::widget::column;

impl VulcanWorkbench {
    /// Returns one short locale-dependent string used by the About-page licensing surface.
    /// 返回关于页许可证界面使用的一段简短语言相关字符串。
    pub(super) fn about_licenses_text<'a>(&self, zh_cn: &'a str, en_us: &'a str) -> &'a str {
        match self.current_locale() {
            AppLocale::EnUs => en_us,
            _ => zh_cn,
        }
    }

    /// Renders the dedicated About-page surface that now loads only list metadata by default.
    /// 渲染新的关于页许可证界面，并默认只加载列表元数据。
    pub(super) fn view_about_licenses_page(&self) -> Element<'_, Message> {
        let mut page = column![self.view_about_licenses_overview_card()].spacing(14);
        if self.about_license_catalog_loading {
            page = page.push(self.view_about_licenses_loading_card());
        }
        if let Some(error) = self.about_license_catalog_load_error.as_ref() {
            page = page.push(self.view_about_licenses_error_card(error));
        }
        if !self.about_license_catalog.documents.is_empty() {
            page = page.push(self.view_about_licenses_document_list_card());
        } else if !self.about_license_catalog_loading {
            page = page.push(self.view_about_licenses_empty_card());
        }

        page.into()
    }

    /// Renders the overview card shown at the top of the About-page licensing surface.
    /// 渲染显示在关于页许可证界面顶部的总览卡片。
    fn view_about_licenses_overview_card(&self) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let metadata_color = palette.background.base.text.scale_alpha(0.62);
        let generated_notice_status = if self.about_license_catalog_loading {
            self.about_licenses_text(
                "Rust 依赖许可证：正在后台读取索引列表。",
                "Rust dependency licenses: reading the list index on a background worker.",
            )
        } else if self.about_license_catalog.has_generated_rust_notices() {
            self.about_licenses_text(
                "Rust 依赖许可证：已检测到分段索引，正文将在查看时按项载入。",
                "Rust dependency licenses: indexed sections detected, and bodies will load only when opened.",
            )
        } else {
            self.about_licenses_text(
                "Rust 依赖许可证：当前未检测到 `output/resources/licenses/THIRD_PARTY_NOTICES.md`，请运行 `./make licenses`。",
                "Rust dependency licenses: `output/resources/licenses/THIRD_PARTY_NOTICES.md` is currently missing; run `./make licenses`.",
            )
        };
        let directory_hint = self
            .about_license_catalog
            .preferred_directory_to_open()
            .map(|path| {
                format!(
                    "{}{}",
                    self.about_licenses_text("许可证资源目录：", "License resource directory: "),
                    path.display()
                )
            })
            .unwrap_or_else(|| {
                self.about_licenses_text(
                    "当前运行环境未提供独立许可证目录，仅能展示内嵌许可证文本。",
                    "The current runtime does not expose a dedicated license directory, so only embedded license text is available.",
                )
                .to_string()
            });

        column![
            text(self.about_licenses_text("关于与许可证", "About & Licenses")).size(18),
            text(format!("Vulcan Code {}", env!("CARGO_PKG_VERSION"))).size(13),
            text(self.about_licenses_text(
                "页面默认只载入许可证列表，点击单项后才在弹窗里按需读取正文，避免整页渲染超长文本。",
                "This page loads only the license list by default and reads each body on demand in a modal to avoid rendering one huge document.",
            ))
            .size(12)
            .color(metadata_color),
            text(format!(
                "{}{}",
                self.about_licenses_text("当前列表项数量：", "Visible entries: "),
                self.about_license_catalog.documents.len()
            ))
            .size(12)
            .color(metadata_color),
            text(generated_notice_status).size(12).color(metadata_color),
            text(directory_hint).size(12).color(metadata_color),
        ]
        .spacing(8)
        .into()
    }

    /// Renders the lightweight loading placeholder shown while the catalog index is fetched on a worker thread.
    /// 渲染在工作线程拉取目录索引期间展示的轻量加载占位卡片。
    fn view_about_licenses_loading_card(&self) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        container(
            column![
                text(self.about_licenses_text("正在加载许可证列表", "Loading License List")).size(16),
                text(self.about_licenses_text(
                    "这里不会预先读取长正文，只会在后台整理列表和索引信息。",
                    "The page does not preload long bodies anymore and only prepares list and index metadata in the background.",
                ))
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.62)),
            ]
            .spacing(8),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the lightweight error card shown when the background catalog load fails.
    /// 渲染后台目录加载失败时显示的轻量错误卡片。
    fn view_about_licenses_error_card<'a>(&self, error: &'a str) -> Element<'a, Message> {
        let palette = self.app_theme.extended_palette();
        container(
            column![
                text(self.about_licenses_text("许可证加载失败", "License Loading Failed")).size(16),
                text(error)
                    .size(12)
                    .color(palette.background.base.text.scale_alpha(0.62))
                    .wrapping(iced::advanced::text::Wrapping::Word),
            ]
            .spacing(8),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the lightweight empty card shown when no About-page license documents are currently available.
    /// 渲染在当前没有可展示的关于页许可证文档时显示的轻量空状态卡片。
    fn view_about_licenses_empty_card(&self) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        container(
            column![
                text(self.about_licenses_text("当前没有可展示的许可证列表", "No License Entries Available"))
                    .size(16),
                text(self.about_licenses_text(
                    "如果缺少 Rust 依赖许可证，请执行 `./make licenses`；静态许可证会在资源目录可用时自动列出。",
                    "If Rust dependency licenses are missing, run `./make licenses`; static notices appear automatically when the resource directory is available.",
                ))
                .size(12)
                .color(palette.background.base.text.scale_alpha(0.62)),
            ]
            .spacing(8),
        )
        .padding(16)
        .style(container::bordered_box)
        .into()
    }

    /// Renders the compact list card that now enumerates every license entry without expanding heavy bodies inline.
    /// 渲染新的紧凑列表卡片，只枚举各许可证条目而不在页面内联展开较重正文。
    fn view_about_licenses_document_list_card(&self) -> Element<'_, Message> {
        let palette = self.app_theme.extended_palette();
        let metadata_color = palette.background.base.text.scale_alpha(0.60);
        let rows = self.about_license_catalog.documents.iter().fold(
            column![].spacing(12),
            |column, document| {
                let mut actions = row![
                    button(self.about_licenses_text("查看许可证", "View License"))
                        .style(button::secondary)
                        .on_press(Message::ToggleAboutLicenseDocumentBody(document.id.clone()))
                ]
                .spacing(8)
                .align_y(Center);
                if document.external_path.is_some() {
                    actions = actions.push(
                        button(self.about_licenses_text("打开原文件", "Open File"))
                            .style(button::secondary)
                            .on_press(Message::OpenAboutLicenseDocumentPath(document.id.clone())),
                    );
                }

                let mut metadata_parts = vec![
                    document
                        .kind
                        .localized_label(self.current_locale())
                        .to_string(),
                    format!(
                        "{} {}",
                        document.line_count,
                        self.about_licenses_text("行", "lines")
                    ),
                ];
                if let Some(used_by_count) = document.used_by_count {
                    metadata_parts.push(format!(
                        "{} {}",
                        used_by_count,
                        self.about_licenses_text("个 crate", "crates")
                    ));
                }

                column.push(
                    container(
                        row![
                            container(
                                column![
                                    text(&document.title).size(16),
                                    text(document.localized_summary(self.current_locale()))
                                        .size(12)
                                        .color(metadata_color),
                                    text(metadata_parts.join(" · "))
                                        .size(11)
                                        .color(metadata_color),
                                ]
                                .spacing(4),
                            )
                            .width(Fill),
                            actions,
                        ]
                        .spacing(12)
                        .align_y(Center),
                    )
                    .padding(16)
                    .style(container::bordered_box),
                )
            },
        );

        column![
            text(self.about_licenses_text("许可证列表", "License List")).size(16),
            rows,
        ]
        .spacing(12)
        .into()
    }

    /// Renders the concrete covered-crate list for one generated Rust dependency document when indexed details are available.
    /// 当生成的 Rust 依赖文档具备索引明细时，渲染其具体覆盖的 crate 列表。
    fn view_about_license_used_by_card<'a>(
        &'a self,
        document: &'a AboutLicenseDocument,
    ) -> Option<Element<'a, Message>> {
        if document.kind != AboutLicenseDocumentKind::RustDependency {
            return None;
        }

        let palette = self.app_theme.extended_palette();
        let metadata_color = palette.background.base.text.scale_alpha(0.62);
        let count = document
            .used_by_count
            .unwrap_or(document.used_by_entries.len());

        if document.used_by_entries.is_empty() {
            return Some(
                container(
                    column![
                        text(self.about_licenses_text("覆盖的 crate", "Covered Crates")).size(14),
                        text(format!(
                            "{} {}",
                            count,
                            self.about_licenses_text("个 crate", "crates")
                        ))
                        .size(12)
                        .color(metadata_color),
                        text(self.about_licenses_text(
                            "当前索引还没有 crate 明细。请重新执行 `./make licenses`，然后再次打开此弹窗。",
                            "The current index does not include crate details yet. Run `./make licenses` again and reopen this modal.",
                        ))
                        .size(12)
                        .color(metadata_color)
                        .wrapping(iced::advanced::text::Wrapping::Word),
                    ]
                    .spacing(8),
                )
                .padding([12, 14])
                .style(container::bordered_box)
                .into(),
            );
        }

        let covered_crate_text = document.used_by_entries.join("\n");
        Some(
            container(
                column![
                    text(self.about_licenses_text("覆盖的 crate", "Covered Crates")).size(14),
                    text(format!(
                        "{} {}",
                        count,
                        self.about_licenses_text("个 crate", "crates")
                    ))
                    .size(12)
                    .color(metadata_color),
                    scrollable(
                        container(
                            text(covered_crate_text)
                                .size(12)
                                .wrapping(iced::advanced::text::Wrapping::Word),
                        )
                        .width(Fill),
                    )
                    .height(160),
                ]
                .spacing(8),
            )
            .padding([12, 14])
            .style(container::bordered_box)
            .into(),
        )
    }

    /// Builds the blocking About-page license detail modal that lazily renders one selected body buffer.
    /// 构建关于页面许可证详情弹窗，并对选中的正文缓冲执行懒渲染。
    pub(super) fn view_about_license_document_modal(&self) -> Option<Element<'_, Message>> {
        let document_id = self.about_license_active_document_id.as_ref()?;
        let document = self.about_license_catalog.document(document_id)?;
        let palette = self.app_theme.extended_palette();
        let close_button = button(
            container(
                codicon::chrome_close()
                    .size(14)
                    .line_height(1.0)
                    .color(palette.background.base.text.scale_alpha(0.72)),
            )
            .width(Fill)
            .height(Fill)
            .center(Fill),
        )
        .width(30)
        .height(30)
        .padding(0)
        .style(style::workbench_notice_modal_close_button_style)
        .on_press(Message::DismissTopLevelOverlay);

        let mut metadata_parts = vec![
            document
                .kind
                .localized_label(self.current_locale())
                .to_string(),
            format!(
                "{} {}",
                document.line_count,
                self.about_licenses_text("行", "lines")
            ),
        ];
        if let Some(used_by_count) = document.used_by_count {
            metadata_parts.push(format!(
                "{} {}",
                used_by_count,
                self.about_licenses_text("个 crate", "crates")
            ));
        }

        let body: Element<'_, Message> = if self.about_license_document_buffer_loading {
            container(
                text(self.about_licenses_text(
                    "正在后台准备许可证正文…",
                    "Preparing the license body on a background worker…",
                ))
                .size(13)
                .color(palette.background.base.text.scale_alpha(0.62)),
            )
            .padding([16, 18])
            .style(container::bordered_box)
            .into()
        } else if let Some(error) = self.about_license_document_buffer_load_error.as_ref() {
            container(
                text(error)
                    .size(13)
                    .color(palette.background.base.text.scale_alpha(0.68))
                    .wrapping(iced::advanced::text::Wrapping::Word),
            )
            .padding([16, 18])
            .style(container::bordered_box)
            .into()
        } else {
            let raw_document_id = document.id.clone();
            container(
                scrollable(
                    container(self.view_workbench_raw_body(
                        self.about_license_document_buffers.get(&document.id),
                        String::new(),
                        move |action| {
                            Message::AboutLicenseDocumentRawAction(raw_document_id.clone(), action)
                        },
                    ))
                    .width(Fill),
                )
                .height(Fill),
            )
            .padding([8, 10])
            .style(container::bordered_box)
            .height(Fill)
            .into()
        };

        let modal_content: Element<'_, Message> =
            if let Some(used_by_card) = self.view_about_license_used_by_card(document) {
                column![used_by_card, body].spacing(12).height(Fill).into()
            } else {
                body
            };

        let action_row = if document.external_path.is_some() {
            row![
                button(self.about_licenses_text("打开原文件", "Open File"))
                    .style(button::secondary)
                    .on_press(Message::OpenAboutLicenseDocumentPath(document.id.clone())),
                space::horizontal(),
                button(self.about_licenses_text("关闭", "Close"))
                    .style(button::primary)
                    .on_press(Message::DismissTopLevelOverlay),
            ]
            .align_y(Center)
            .spacing(10)
        } else {
            row![
                space::horizontal(),
                button(self.about_licenses_text("关闭", "Close"))
                    .style(button::primary)
                    .on_press(Message::DismissTopLevelOverlay),
            ]
            .align_y(Center)
            .spacing(10)
        };

        let modal_card = container(
            column![
                row![
                    container(
                        column![
                            text(&document.title).size(20),
                            text(metadata_parts.join(" · "))
                                .size(12)
                                .color(palette.background.base.text.scale_alpha(0.62)),
                        ]
                        .spacing(4)
                    )
                    .width(Fill),
                    close_button,
                ]
                .spacing(12)
                .align_y(Center)
                .height(WORKBENCH_ABOUT_LICENSE_MODAL_HEADER_HEIGHT),
                modal_content,
                action_row,
            ]
            .spacing(14)
            .height(Fill),
        )
        .padding([20, 22])
        .width(WORKBENCH_ABOUT_LICENSE_MODAL_WIDTH)
        .height(WORKBENCH_ABOUT_LICENSE_MODAL_HEIGHT)
        .style(style::startup_overlay_card_style);

        let backdrop = mouse_area(container(space()).width(Fill).height(Fill))
            .on_press(Message::DismissTopLevelOverlay);

        Some(
            opaque(
                stack([
                    container(backdrop)
                        .width(Fill)
                        .height(Fill)
                        .style(style::startup_overlay_backdrop_style)
                        .into(),
                    container(mouse_area(modal_card).on_press(Message::Noop))
                        .width(Fill)
                        .height(Fill)
                        .center(Fill)
                        .into(),
                ])
                .width(Fill)
                .height(Fill),
            )
            .into(),
        )
    }
}
