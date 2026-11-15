//! Native setup pages styled with the Vulcan Code desktop design tokens.
//! 使用 Vulcan Code 桌面设计令牌样式的原生安装页面。

use iced::alignment::Vertical;
use iced::widget::{
    Button, Column, Row, button, checkbox, column, container, image, mouse_area, pick_list,
    progress_bar, row, rule, scrollable, space, text, text_input, tooltip,
};
use iced::{Color, Element, Font, Length, Theme, color, font, theme};
use vulcan_install_core::fetch::Route;
use vulcan_install_core::names::Channel;

use super::{Confirmation, Message, Operation, OptionField, RouteChoice, Setup, Stage, filled};
use crate::prompt::megabytes;
use crate::report::{Action, Outcome, Review};
use crate::screen::{component_names, location_lines};

// Mirror the built-in light desktop tokens from apps/vulcan-code-web/src/ui-kit/styles/tokens.css.
// 对齐 apps/vulcan-code-web/src/ui-kit/styles/tokens.css 内置的桌面浅色令牌。
const GROUND: Color = color!(0xf2f3f5);
// White card surface and inverse label text.
// 白色卡片表面及反色标签文字。
const PANEL: Color = Color::WHITE;
// Recessed input surface.
// 内凹的输入控件表面。
const SUNKEN: Color = color!(0xf6f7f9);
// Neutral hover surface.
// 中性的悬停表面。
const HOVER: Color = color!(0xe8ebef);
// Borders between panels and controls.
// 面板和控件之间的分隔线。
const LINE: Color = color!(0xdde1e6);
// Strong control borders distinguish controls on a white panel.
// 加强控件边线，以便在白色面板上识别控件。
const LINE_STRONG: Color = color!(0xc3c9d1);
// Primary UI text.
// 界面主要文字。
const INK: Color = color!(0x14171c);
// Secondary UI text.
// 界面次要文字。
const MUTED: Color = color!(0x4a515c);
// Vulcan Code interaction accent.
// Vulcan Code 交互强调色。
const SIGNAL: Color = color!(0x2e58cc);
// Vulcan Code light selection surface.
// Vulcan Code 浅色选中表面。
const SELECT: Color = color!(0xe4eafa);
// Destructive action text on light surfaces.
// 浅色表面上的危险操作文字。
const DANGER: Color = color!(0xc3303d);
// Branded navigation rail, using the built-in dark surface.
// 使用内置深色表面的品牌导航侧栏。
const RAIL: Color = color!(0x15181d);
// Navigation items outside their active state.
// 非当前状态的导航条目文字。
const RAIL_TEXT: Color = color!(0xaeb6c3);
// Current navigation step on the dark surface.
// 深色表面上当前导航步骤的底色。
const RAIL_ACTIVE: Color = color!(0x23282f);
// Frame underneath the fallback one-pixel edge of the custom window.
// 自绘窗口的一像素外框底色。
const FRAME: Color = color!(0x343942);
// Default body text uses the platform's UI family.
// 正文默认使用平台界面字体族。
#[cfg(windows)]
pub(super) const BODY_FONT: Font = Font::with_name("Segoe UI");
#[cfg(unix)]
pub(super) const BODY_FONT: Font = Font::DEFAULT;

// Installer display headings in the native UI family.
// 使用原生界面字体的安装器标题。
const HEADING: Font = Font {
    weight: font::Weight::Semibold,
    ..BODY_FONT
};
// Directory and command text share the desktop monospace fallback.
// 目录和命令文字共用桌面等宽后备字体。
const MONO: Font = Font::MONOSPACE;

/// Decodes the existing Vulcan Code mark into RGBA for the window and branded navigation.
/// 将现有 Vulcan Code 标志解码为 RGBA，供窗口和品牌导航使用。
pub(super) fn brand_pixels() -> Option<::image::RgbaImage> {
    ::image::load_from_memory_with_format(
        include_bytes!("../../../vulcan-code-web/public/vulcan-logo-startup.png"),
        ::image::ImageFormat::Png,
    )
    .ok()
    .map(|image| image.into_rgba8())
}

/// Returns the matching Iced palette used by inputs, focus and selection controls.
/// 返回用于输入、焦点及选择控件的对应 Iced 调色板。
pub(super) fn theme() -> Theme {
    Theme::custom(
        "Vulcan Code",
        theme::Palette {
            background: GROUND,
            text: INK,
            primary: SIGNAL,
            success: color!(0x18774a),
            warning: color!(0x945d09),
            danger: DANGER,
        },
    )
}

/// Wraps related controls in a square-cornered Vulcan surface.
/// 使用 Vulcan 的方角面板包裹相关控件。
fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .padding([10, 14])
        .style(|_| container::Style {
            background: Some(PANEL.into()),
            border: iced::border::color(LINE).width(1),
            ..container::Style::default()
        })
        .into()
}

/// Draws the main action with the same rectangular treatment as secondary controls.
/// 使用与次要控件一致的矩形样式绘制主操作。
fn action<'a>(
    title: &'a str,
    message: Option<Message>,
    style: fn(&Theme, button::Status) -> button::Style,
) -> Button<'a, Message> {
    button(text(title).font(HEADING))
        .style(move |theme, status| {
            // Keep action styles rectangular while preserving their border width and color.
            // 操作样式使用直角，并保留各自的边线宽度和颜色。
            let style = style(theme, status);
            button::Style {
                border: iced::border::color(style.border.color).width(style.border.width),
                ..style
            }
        })
        .padding([8, 15])
        .on_press_maybe(message)
}

/// Puts secondary actions on a neutral outline and preserves interaction feedback.
/// 次要操作使用中性色描边并保留交互反馈。
fn secondary(_: &Theme, status: button::Status) -> button::Style {
    // Match the same neutral active, hover and disabled fills as the product controls.
    // 对齐产品控件的中性默认、悬停和禁用表面。
    let (fill, ink) = match status {
        button::Status::Active => (PANEL, INK),
        button::Status::Hovered | button::Status::Pressed => (HOVER, INK),
        button::Status::Disabled => (SUNKEN, MUTED),
    };
    button::Style {
        background: Some(fill.into()),
        text_color: ink,
        border: iced::border::color(LINE_STRONG).width(1),
        ..button::Style::default()
    }
}

/// Maps a text input's interaction state to Vulcan's field surface and focus outline.
/// 将文本输入框的交互状态映射为 Vulcan 的输入表面及焦点描边。
fn input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    // Keep the active field neutral and differentiate keyboard focus with the signal color.
    // 默认输入框采用中性色，以强调色区分键盘焦点。
    let outline = match status {
        text_input::Status::Hovered | text_input::Status::Focused { .. } => SIGNAL,
        _ => LINE_STRONG,
    };
    text_input::Style {
        background: SUNKEN.into(),
        border: iced::border::color(outline).width(1),
        selection: SELECT,
        ..text_input::default(theme, status)
    }
}

/// Maps a source selector's state to the same surface language as the text input.
/// 下载源选择器沿用文本输入框的表面及边线样式。
fn selector_style(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    // The open selector has the same accent outline as a focused field.
    // 展开的选择器与聚焦输入框使用同款强调色边线。
    let border = if status == pick_list::Status::Active {
        LINE_STRONG
    } else {
        SIGNAL
    };
    pick_list::Style {
        background: SUNKEN.into(),
        border: iced::border::color(border).width(1),
        ..pick_list::default(theme, status)
    }
}

/// Draws a caption control with a localized tooltip and the desktop hover treatment.
/// 绘制带本地化提示及桌面悬停样式的窗口按钮。
fn caption_button<'a>(
    symbol: &'static str,
    hint: &'a str,
    message: Message,
) -> Element<'a, Message> {
    // Keep destructive styling exclusive to the close control's hover and pressed states.
    // 仅在关闭按钮悬停和按下时使用危险操作色。
    let close = matches!(message, Message::Close);
    tooltip(
        button(container(text(symbol).size(19)).center(Length::Fill))
            .width(42)
            .height(36)
            .padding(0)
            .on_press(message)
            .style(move |_, status| button::Style {
                background: match status {
                    button::Status::Hovered | button::Status::Pressed => {
                        Some(if close { DANGER } else { FRAME }.into())
                    }
                    _ => None,
                },
                text_color: PANEL,
                ..button::Style::default()
            }),
        text(hint).size(12),
        tooltip::Position::Bottom,
    )
    .delay(std::time::Duration::from_millis(400))
    .padding(5)
    .style(|_| container::Style::default().background(PANEL).color(INK))
    .into()
}

impl Setup {
    /// Composes fixed navigation and actions around the scrollable, state-specific content.
    /// 将固定的导航及操作区布置在可滚动的当前步骤内容周围。
    pub(super) fn view(&self) -> Element<'_, Message> {
        // The active step follows the worker state and never gets ahead of signed review.
        // 当前步骤跟随安装线程状态，始终等待实际签名清单审阅。
        let (step, mut body, footer) = if let Some(confirmation) = &self.confirmation {
            (
                1,
                self.review_page(confirmation),
                self.review_footer(confirmation),
            )
        } else if let Some(result) = &self.result {
            (3, self.result_page(result), self.result_footer(result))
        } else if self.worker.is_some() {
            (
                if self.approved { 2 } else { 1 },
                self.progress_page(),
                self.progress_footer(),
            )
        } else {
            (0, self.options_page(), self.options_footer())
        };
        if !self.warnings.is_empty() {
            body = body.push(card(text(self.warnings.join("\n")).size(13).color(DANGER)));
        }
        // Iced offers each press to the children first, so normal controls keep their own input.
        // Iced 先将按下事件交给子控件，因此正常控件保留自己的输入行为。
        mouse_area(
            container(column![
                self.caption(),
                row![
                    self.navigation(step),
                    column![
                        scrollable(
                            container(body.spacing(11))
                                .width(Length::Fill)
                                .padding([14, 20])
                        )
                        .height(Length::Fill)
                        .width(Length::Fill),
                        divider(),
                        container(footer.align_y(Vertical::Center))
                            .padding([8, 20])
                            .style(|_| container::Style::default().background(PANEL))
                            .width(Length::Fill)
                    ]
                    .width(Length::Fill)
                    .height(Length::Fill)
                ]
                .width(Length::Fill)
                .height(Length::Fill)
            ])
            .padding(1)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(GROUND.into()),
                border: iced::border::color(FRAME).width(1),
                ..container::Style::default()
            }),
        )
        .on_press(Message::DragWindow)
        .into()
    }

    /// Places the brand, window title and caption actions in the undivided top strip.
    /// 将品牌、窗口标题及窗口操作放在同一条顶栏上。
    fn caption(&self) -> Element<'_, Message> {
        // Keep a consistent title inset even if the brand image cannot be decoded.
        // 即使品牌图像无法解码，也维持一致的标题内距。
        let mut title = row![].spacing(8).align_y(Vertical::Center);
        if let Some(logo) = &self.brand {
            title = title.push(image(logo.clone()).width(20).height(20));
        }
        container(
            row![
                container(
                    title.push(
                        text(self.text("setup.title"))
                            .font(HEADING)
                            .size(13)
                            .color(PANEL)
                    )
                )
                .padding(iced::padding::left(12)),
                space::horizontal(),
                caption_button(
                    "−",
                    self.text("setup.gui.minimize"),
                    Message::MinimizeWindow
                ),
                caption_button("×", self.text("setup.gui.close"), Message::Close),
            ]
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .height(36)
        .style(|_| container::Style::default().background(RAIL))
        .into()
    }

    /// Shows product identity and the current step without discarding the main pane's space.
    /// 在保留主内容空间的同时展示产品标识与当前步骤。
    fn navigation(&self, step: usize) -> Element<'_, Message> {
        // Navigation entries reflect actual screen transitions.
        // 导航条目对应真实页面切换。
        let steps = [
            "setup.gui.options",
            "setup.gui.review",
            "setup.gui.progress",
            "setup.done.overline",
        ];
        // The count and tint identify the active page independently of color vision.
        // 步骤编号以及高亮共同标识当前页面。
        let mut items = column![].spacing(3);
        for (index, key) in steps.into_iter().enumerate() {
            // Use the step's slug as catalog key and its display order as the counter.
            // 以步骤目录键标识文案，以显示顺序生成编号。
            let active = index == step;
            items = items.push(
                container(
                    row![
                        text(format!("{:02}", index + 1))
                            .font(MONO)
                            .size(13)
                            .color(if active { color!(0x7d9dff) } else { RAIL_TEXT }),
                        text(self.text(key))
                            .font(HEADING)
                            .size(14)
                            .color(if active { PANEL } else { RAIL_TEXT })
                    ]
                    .spacing(8)
                    .align_y(Vertical::Center),
                )
                .padding([9, 8])
                .width(Length::Fill)
                .style(move |_| {
                    container::Style::default().background(if active { RAIL_ACTIVE } else { RAIL })
                }),
            );
        }
        container(
            column![
                items,
                space::vertical().height(Length::Fill),
                text(format!(
                    "{}  ·  v{}",
                    std::env::consts::OS,
                    crate::selfupdate::SETUP_VERSION
                ))
                .font(MONO)
                .size(12)
                .color(RAIL_TEXT)
            ]
            .spacing(12),
        )
        .padding([13, 10])
        .width(156)
        .height(Length::Fill)
        .style(|_| container::Style::default().background(RAIL))
        .into()
    }

    /// Shows location, source and role-specific installation controls.
    /// 显示安装位置、下载来源及操作对应的安装控件。
    fn options_page(&self) -> Column<'_, Message> {
        // Existing installations primarily offer update and removal in place.
        // 现有安装主要提供原地更新和卸载。
        let editing = self.before_edit.is_some();
        let manage = (self.installed || self.initial.is_some()) && !editing;
        // An explicit Windows uninstall entry takes the removal path immediately.
        // Windows 显式卸载入口直接进入卸载路径。
        let removing = self.initial == Some(Operation::Uninstall);
        // Use the appropriate title and a useful summary for the chosen path.
        // 按当前路径呈现对应标题及摘要。
        let title = if removing {
            "setup.gui.uninstall"
        } else if editing {
            "setup.gui.modifyInstall"
        } else if manage {
            "setup.gui.manage"
        } else {
            "setup.gui.install"
        };
        // Attach the installed version to the existing-install summary.
        // 在现有安装摘要中带上已安装版本。
        let detail = if let Some(version) = &self.installed_version {
            filled(
                &self.language,
                "setup.gui.installedVersion",
                &[("version", version)],
            )
        } else {
            String::from(self.text(if removing {
                "setup.gui.keepData"
            } else if self.legacy {
                "setup.gui.adopt"
            } else if manage {
                "setup.gui.manageDetail"
            } else {
                "setup.gui.welcome"
            }))
        };
        // Windows Settings supplies the exact registered directory when it starts removal.
        // Windows 设置启动卸载时提供准确的已登记安装目录。
        let path: Element<'_, Message> = if removing || editing {
            text(&self.directory).font(MONO).size(13).into()
        } else {
            row![
                text_input(self.text("setup.location.title"), &self.directory)
                    .on_input(Message::Directory)
                    .style(input_style)
                    .font(MONO)
                    .padding(7)
                    .size(14),
                action(
                    self.text("setup.gui.browse"),
                    Some(Message::Browse),
                    secondary
                )
            ]
            .spacing(8)
            .align_y(Vertical::Center)
            .into()
        };
        // The path and source use one panel because both determine what gets installed.
        // 目录和下载源共同决定安装内容，因此布置在同一面板。
        let mut locations =
            column![text(self.text("setup.location.title")).font(HEADING), path].spacing(6);
        if !removing {
            // The release engine owns each source's meaning and host list.
            // 各下载源的含义及主机列表由发行引擎负责。
            let routes = [
                RouteChoice(Route::Auto, self.text("setup.route.auto").into()),
                RouteChoice(
                    Route::International,
                    self.text("setup.route.international").into(),
                ),
                RouteChoice(Route::China, self.text("setup.route.china").into()),
            ];
            // The catalog label follows the currently selected route.
            // 目录标签跟随当前选择的下载线路。
            let source = routes.iter().find(|route| route.0 == self.route).cloned();
            // The download source and release channel belong together.
            // 下载源和发行通道放在同一个区域。
            let mut download = row![
                text(self.text("setup.route.title")).font(HEADING),
                pick_list(routes, source, Message::Route)
                    .padding([6, 9])
                    .style(selector_style),
                space::horizontal(),
            ]
            .spacing(10)
            .align_y(Vertical::Center);
            if !manage {
                download = download.push(
                    tooltip(
                        checkbox(self.options.channel == Some(Channel::Preview))
                            .label(self.text("setup.channel.preview"))
                            .size(15)
                            .on_toggle(|checked| Message::Option(OptionField::Preview, checked)),
                        text(self.text("setup.channel.previewDetail")).size(13),
                        tooltip::Position::Bottom,
                    )
                    .style(|_| container::Style::default().background(PANEL).color(INK)),
                );
            }
            locations = locations.push(download);
        }
        // The title is outside the card so that installation purpose stays distinct from settings.
        // 将标题置于卡片外，便于区分安装目的与具体设置。
        let mut page = column![heading(self.text(title), detail), card(locations)];
        if removing {
            return page.push(card(self.option(
                "setup.gui.eraseData",
                Some("setup.gui.purge"),
                OptionField::Purge,
                self.purge,
            )));
        }
        if manage {
            if self.legacy && self.installed_version.is_some() {
                page = page.push(card(
                    column![
                        text(self.text("setup.gui.takeOver")).font(HEADING),
                        text(self.text("setup.gui.adopt")).size(14).color(MUTED)
                    ]
                    .spacing(5),
                ));
            }
            return page;
        }
        if self.advanced {
            return column![
                heading(self.text("setup.advanced.title"), ""),
                card(
                    column![
                        text(self.text("setup.advanced.ports")).font(HEADING),
                        text_input(
                            self.text("setup.summary.portsKeep"),
                            self.options.port_range.as_deref().unwrap_or_default()
                        )
                        .on_input(Message::PortRange)
                        .style(input_style)
                        .font(MONO)
                        .padding(9)
                        .size(14),
                        text(self.text("setup.advanced.portsDetail"))
                            .size(13)
                            .color(MUTED),
                    ]
                    .spacing(8)
                )
            ];
        }
        page.push(self.components()).push(self.integration())
    }

    /// Renders required and optional components with descriptions of their effects.
    /// 展示必装和可选组件，并说明各自用途。
    fn components(&self) -> Element<'_, Message> {
        // The engine always installs the core service and terminal client.
        // 安装引擎始终安装核心服务和终端客户端。
        let required = format!(
            "{}  ·  {}",
            self.text("setup.component.core"),
            self.text("setup.component.tui")
        );
        // Browser selection uses the typed component name recognized by the engine.
        // 浏览器选择使用安装引擎识别的有类型组件名。
        let browser = self
            .options
            .with
            .iter()
            .any(|name| name.as_str() == crate::args::BROWSER_COMPONENT);
        card(
            column![
                text(self.text("setup.summary.components"))
                    .font(HEADING)
                    .size(16),
                container(text(required).size(13).color(SIGNAL))
                    .padding([4, 7])
                    .style(|_| container::Style::default().background(SELECT)),
                row![
                    self.option(
                        "setup.component.gui",
                        None,
                        OptionField::Desktop,
                        self.options.gui.unwrap_or(true)
                    )
                    .width(Length::FillPortion(1)),
                    self.option(
                        "setup.components.browser",
                        Some("setup.components.browserDetail"),
                        OptionField::Browser,
                        browser
                    )
                    .width(Length::FillPortion(1))
                ]
                .spacing(10)
            ]
            .spacing(6),
        )
    }

    /// Groups system integration switches independently from product components.
    /// 将系统集成开关与产品组件分组展示。
    fn integration(&self) -> Element<'_, Message> {
        // Use the same established defaults as the shared install engine.
        // 使用与共用安装引擎一致的既有默认值。
        let desktop = self.options.gui.unwrap_or(true);
        card(
            column![
                text(self.text("setup.integration.title"))
                    .font(HEADING)
                    .size(16),
                row![
                    column![
                        checkbox(desktop && self.options.menu.unwrap_or(true))
                            .label(self.text("setup.integration.menu"))
                            .size(15)
                            .on_toggle_maybe(
                                desktop.then_some(|checked| Message::Option(
                                    OptionField::Menu,
                                    checked
                                ))
                            ),
                        checkbox(desktop && self.options.desktop_shortcut.unwrap_or(false))
                            .label(self.text("setup.integration.desktopShortcut"))
                            .size(15)
                            .on_toggle_maybe((desktop && !self.platform.is_macos()).then_some(
                                |checked| Message::Option(OptionField::Shortcut, checked)
                            ))
                    ]
                    .spacing(6)
                    .width(Length::FillPortion(1)),
                    column![
                        self.option(
                            "setup.integration.path",
                            None,
                            OptionField::Path,
                            self.options.path.unwrap_or(true)
                        ),
                        self.option(
                            "setup.integration.service",
                            None,
                            OptionField::Service,
                            self.options.service.unwrap_or(false)
                        )
                    ]
                    .spacing(6)
                    .width(Length::FillPortion(1))
                ]
                .spacing(14),
            ]
            .spacing(7),
        )
    }

    /// Builds a keyboard-operable option and its optional catalog description.
    /// 构建可用键盘操作的选项和可选目录说明。
    fn option(
        &self,
        title: &'static str,
        detail: Option<&'static str>,
        field: OptionField,
        checked: bool,
    ) -> Column<'_, Message> {
        // Indenting descriptions aligns them to the control's label.
        // 将说明文字缩进至与控件标签对齐。
        let mut option = column![
            checkbox(checked)
                .label(self.text(title))
                .size(15)
                .on_toggle_maybe(
                    (!(self.before_edit.is_some() && matches!(field, OptionField::Browser)))
                        .then_some(move |value| Message::Option(field, value)),
                )
        ]
        .spacing(3);
        if let Some(detail) = detail {
            option = option.push(
                container(text(self.text(detail)).size(13).color(MUTED))
                    .padding(iced::padding::left(25)),
            );
        }
        option
    }

    /// Presents the available action for the current installation state.
    /// 按当前安装状态显示可执行操作。
    fn options_footer(&self) -> Row<'_, Message> {
        if self.advanced
            && (!self.installed || self.before_edit.is_some())
            && self.initial.is_none()
        {
            return row![
                space::horizontal(),
                action(
                    self.text("setup.gui.back"),
                    Some(Message::Advanced),
                    secondary
                )
            ];
        }
        // A path must be present before any action may start.
        // 必须提供目录才能启动任何操作。
        let ready = !self.directory.trim().is_empty();
        // Confirm irreversible removal only after showing its exact scope.
        // 展示确切影响范围后才确认不可逆卸载。
        let (operation, title) = if self.initial == Some(Operation::Uninstall) {
            (Operation::Uninstall, "setup.gui.continue")
        } else if self.before_edit.is_some() {
            (Operation::Install, "setup.gui.continue")
        } else if self.legacy {
            (Operation::Update, "setup.gui.takeOver")
        } else if self.installed || self.initial == Some(Operation::Update) {
            (Operation::Update, "setup.gui.update")
        } else {
            (Operation::Install, "setup.gui.continue")
        };
        // Management exposes removal alongside update.
        // 管理页同时提供更新与卸载。
        let secondary_action = if self.before_edit.is_some() {
            action(
                self.text("setup.gui.back"),
                Some(Message::EditInstallation),
                secondary,
            )
        } else if self.installed && operation != Operation::Uninstall {
            action(
                self.text("setup.gui.uninstall"),
                Some(Message::SelectRemoval),
                secondary,
            )
        } else if self.installed && operation == Operation::Uninstall {
            action(
                self.text("setup.gui.back"),
                Some(Message::LeaveRemoval),
                secondary,
            )
        } else {
            action(
                self.text("setup.gui.cancel"),
                Some(Message::Close),
                secondary,
            )
        };
        // Keep advanced settings reachable without making the common flow scroll.
        // 让高级设置可达，同时避免常用流程必须滚动。
        let mut footer = row![secondary_action].spacing(10);
        if self.installed && !self.legacy && self.initial.is_none() && self.before_edit.is_none() {
            footer = footer.push(action(
                self.text("setup.gui.modifyInstall"),
                Some(Message::EditInstallation),
                secondary,
            ));
        }
        if operation == Operation::Install {
            footer = footer.push(action(
                self.text("setup.advanced.title"),
                Some(Message::Advanced),
                secondary,
            ));
        }
        footer.push(space::horizontal()).push(action(
            self.text(title),
            ready.then_some(Message::Start(operation)),
            button::primary,
        ))
    }

    /// Presents the precise signed plan or the currently requested removal confirmation.
    /// 展示精确签名计划或当前请求的卸载确认。
    fn review_page<'a>(&'a self, confirmation: &'a Confirmation) -> Column<'a, Message> {
        match confirmation {
            Confirmation::Install(review, _) => self.install_review(review),
            Confirmation::WaitForExit(names, offer_force, _) => column![
                heading(
                    self.text("setup.processes.title"),
                    self.text(if *offer_force {
                        "setup.processes.forceDetail"
                    } else {
                        "setup.processes.closeDetail"
                    })
                ),
                card(text(names.join("\n")).font(MONO).size(13)),
            ],
            Confirmation::Remove(purge, _) => column![
                heading(
                    self.text(if *purge {
                        "setup.gui.eraseData"
                    } else {
                        "setup.gui.uninstall"
                    }),
                    if *purge {
                        ""
                    } else {
                        self.text("setup.review.overline")
                    }
                ),
                if *purge {
                    card(text(self.text("setup.gui.purge")).color(DANGER))
                } else {
                    card(
                        column![
                            text(self.text("setup.location.title")).font(HEADING),
                            text(&self.directory).font(MONO).size(13),
                            text(self.text(if self.purge {
                                "setup.gui.purge"
                            } else {
                                "setup.gui.keepData"
                            }))
                            .color(MUTED),
                        ]
                        .spacing(9),
                    )
                }
            ],
        }
    }

    /// Displays the version, components, actual download size and external system changes.
    /// 显示版本、组件、实际下载量及外部系统改动。
    fn install_review(&self, review: &Review) -> Column<'_, Message> {
        // Keep all size and location data sourced directly from the reviewed engine plan.
        // 容量及位置数据全部直接来自待审阅的安装引擎计划。
        let download = if review.download == 0 {
            String::from(self.text("setup.review.downloadNone"))
        } else {
            megabytes(review.download)
        };
        // Each independent field remains readable when a directory or component list wraps.
        // 当目录或组件列表换行时，各字段仍可独立阅读。
        let mut facts = column![
            field(
                self.text("setup.summary.directory"),
                review.directory.display().to_string(),
                true
            ),
            field(
                self.text("setup.summary.components"),
                component_names(&self.language, &review.components),
                false
            ),
            field(
                self.text("setup.summary.channel"),
                self.text(channel_key(review.channel)),
                false
            ),
            field(self.text("setup.review.download"), download, false)
        ]
        .spacing(10);
        if let Some(size) = review.installed_size {
            facts = facts.push(field(
                self.text("setup.review.size"),
                megabytes(size),
                false,
            ));
        }
        if let Some(ports) = &review.port_range {
            facts = facts.push(field(self.text("setup.summary.ports"), ports.clone(), true));
        }
        // Reuse the shared wizard's formatting of every external write.
        // 复用共享向导对每个外部写入位置的格式化结果。
        let mut writes = column![text(self.text("setup.summary.writes")).font(HEADING)].spacing(6);
        for line in location_lines(&self.language, review.platform, &review.locations) {
            writes = writes.push(
                text(line.to_string().trim_start().to_owned())
                    .size(13)
                    .color(MUTED),
            );
        }
        column![
            heading(
                self.action_title(&review.action, &review.version, false),
                self.text("setup.review.overline")
            ),
            card(facts),
            card(writes),
            text(self.text(if matches!(review.action, Action::Refresh) {
                "setup.review.refreshDetail"
            } else {
                "setup.review.detail"
            }))
            .size(13)
            .color(MUTED)
        ]
    }

    /// Provides back and an operation-specific commit action while a review is pending.
    /// 审阅等待答复时提供返回及对应操作的确认按钮。
    fn review_footer(&self, confirmation: &Confirmation) -> Row<'_, Message> {
        // Removal has its own destructive confirmation.
        // 卸载使用独立的危险操作确认。
        if let Confirmation::WaitForExit(_, offer_force, _) = confirmation {
            let mut footer = row![
                action(
                    self.text("setup.gui.cancel"),
                    Some(Message::Answer(false)),
                    secondary
                ),
                space::horizontal(),
                action(
                    self.text("setup.processes.retry"),
                    Some(Message::Answer(true)),
                    button::primary
                ),
            ]
            .spacing(8);
            if *offer_force {
                footer = footer.push(action(
                    self.text("setup.processes.force"),
                    Some(Message::ForceClose),
                    button::danger,
                ));
            }
            return footer;
        }
        let (key, style): (_, fn(&Theme, button::Status) -> button::Style) = match confirmation {
            Confirmation::WaitForExit(..) => {
                unreachable!("process confirmation has its own controls")
            }
            Confirmation::Remove(true, _) => ("setup.gui.eraseData", button::danger),
            Confirmation::Remove(..) => ("setup.gui.uninstall", button::danger),
            Confirmation::Install(review, _) => match review.action {
                Action::Install => ("setup.gui.install", button::primary),
                Action::Update { .. } => ("setup.gui.update", button::primary),
                Action::Refresh => ("setup.gui.continue", button::primary),
            },
        };
        row![
            action(
                self.text(if matches!(confirmation, Confirmation::Remove(true, _)) {
                    "setup.gui.keepDataAction"
                } else {
                    "setup.gui.back"
                }),
                Some(Message::Answer(false)),
                secondary
            ),
            space::horizontal(),
            action(self.text(key), Some(Message::Answer(true)), style)
        ]
    }

    /// Shows real transferred bytes and the current operation's reported phase.
    /// 显示真实传输字节量及当前操作报告的阶段。
    fn progress_page(&self) -> Column<'_, Message> {
        // A cancellation request remains visible until the worker acknowledges it.
        // 取消请求保持可见，直到后台线程确认。
        let cancelling = self
            .cancel
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed));
        // Checks and removals report activity without inventing a transfer percentage.
        // 检查和卸载只报告当前活动，不生成虚构的传输百分比。
        let stage = if cancelling {
            self.text("setup.working.cancelling").to_owned()
        } else if self.operation == Some(Operation::Uninstall) {
            self.text("setup.gui.uninstall").to_owned()
        } else {
            self.stage_text()
        };
        // Only show byte progress during its corresponding download stage.
        // 仅在对应的下载阶段展示字节进度。
        let mut activity = column![text(stage).font(HEADING).size(19)].spacing(12);
        if self.stage == Some(Stage::Downloading)
            && let Some((done, total)) = self.progress.filter(|(_, total)| *total > 0)
        {
            activity = activity
                .push(progress_bar(0.0..=1.0, (done as f64 / total as f64) as f32).girth(6))
                .push(
                    text(filled(
                        &self.language,
                        "setup.progress",
                        &[("received", &megabytes(done)), ("total", &megabytes(total))],
                    ))
                    .font(MONO)
                    .size(13)
                    .color(MUTED),
                );
        }
        column![
            heading(
                self.text(if self.approved {
                    "setup.gui.progress"
                } else {
                    "setup.gui.review"
                }),
                ""
            ),
            card(activity)
        ]
    }

    /// Enables cancel only while a cancellation token exists for the active transfer.
    /// 仅在当前传输具有取消令牌时启用取消。
    fn progress_footer(&self) -> Row<'_, Message> {
        // The service and integration phases cannot safely be interrupted.
        // 服务及集成阶段不能安全中断。
        let can_cancel = self.stage.is_some_and(Stage::cancellable)
            && self
                .cancel
                .as_ref()
                .is_some_and(|flag| !flag.load(std::sync::atomic::Ordering::Relaxed));
        row![
            text(self.text("setup.working.overline"))
                .size(13)
                .color(MUTED),
            space::horizontal(),
            action(
                self.text("setup.gui.cancel"),
                can_cancel.then_some(Message::Close),
                secondary
            )
        ]
        .spacing(12)
    }

    /// Displays the completed installation, completed removal or actionable failure.
    /// 展示安装完成、卸载完成或可处理的失败结果。
    fn result_page<'a>(
        &'a self,
        result: &'a Result<Option<Box<Outcome>>, String>,
    ) -> Column<'a, Message> {
        match result {
            Ok(Some(outcome)) => {
                // Show the installed version and the command the engine actually set up.
                // 显示已安装版本及安装引擎实际配置的命令。
                let mut page = column![
                    heading(
                        self.action_title(&outcome.action, &outcome.version, true),
                        self.text("setup.done.open")
                    ),
                    card(
                        column![
                            field(
                                self.text("setup.location.title"),
                                outcome.directory.display().to_string(),
                                true
                            ),
                            text(filled(
                                &self.language,
                                "setup.done.terminal",
                                &[("command", &outcome.terminal)]
                            ))
                            .size(14)
                        ]
                        .spacing(10)
                    )
                ];
                if outcome.platform.is_windows() {
                    page = page.push(
                        text(self.text("setup.done.uninstallWindows"))
                            .size(13)
                            .color(MUTED),
                    );
                }
                if !outcome.notes.is_empty() {
                    page = page.push(card(text(outcome.notes.join("\n")).size(13).color(MUTED)));
                }
                page
            }
            Ok(None) => column![heading(
                self.text("setup.gui.removed"),
                if self.kept_data {
                    self.text("setup.gui.keepData")
                } else {
                    ""
                }
            )],
            Err(error) if error == crate::CANCELLED => {
                column![heading(self.text("setup.gui.cancelled"), "")]
            }
            Err(error) => column![
                heading(
                    self.text("setup.failed.title"),
                    self.text("setup.failed.detail")
                ),
                card(text(error).font(MONO).size(13))
            ],
        }
    }

    /// Offers launch only when the installed components include the desktop application.
    /// 仅当已安装组件包含桌面应用时提供启动操作。
    fn result_footer(&self, result: &Result<Option<Box<Outcome>>, String>) -> Row<'_, Message> {
        match result {
            Ok(Some(outcome)) if outcome.desktop.is_some() => row![
                action(
                    self.text("setup.gui.close"),
                    Some(Message::Close),
                    secondary
                ),
                space::horizontal(),
                action(
                    self.text("setup.gui.launch"),
                    Some(Message::Launch),
                    button::primary
                )
            ],
            Ok(_) => row![
                space::horizontal(),
                action(
                    self.text("setup.gui.close"),
                    Some(Message::Close),
                    button::primary
                )
            ],
            Err(_) => row![
                action(self.text("setup.gui.back"), Some(Message::Home), secondary),
                space::horizontal(),
                action(
                    self.text("setup.gui.close"),
                    Some(Message::Close),
                    button::primary
                )
            ],
        }
    }

    /// Formats the shared operation as a localized review or completion heading.
    /// 将共用操作格式化为本地化审阅或完成标题。
    fn action_title(&self, action: &Action, version: &str, done: bool) -> String {
        // Updates carry both versions in the signed install plan.
        // 更新在签名安装计划中同时记录两个版本。
        let (key, from) = match (action, done) {
            (Action::Install, false) => ("setup.review.install", ""),
            (Action::Install, true) => ("setup.done.installed", ""),
            (Action::Update { from }, false) => ("setup.review.update", from.as_str()),
            (Action::Update { from }, true) => ("setup.done.updated", from.as_str()),
            (Action::Refresh, false) => ("setup.review.refresh", ""),
            (Action::Refresh, true) => ("setup.done.refreshed", ""),
        };
        filled(&self.language, key, &[("version", version), ("from", from)])
    }
}

/// Builds a page heading and optional explanatory text.
/// 构建页面标题及可选说明文字。
fn heading<'a>(
    title: impl iced::widget::text::IntoFragment<'a>,
    detail: impl iced::widget::text::IntoFragment<'a>,
) -> Column<'a, Message> {
    column![
        text(title).font(HEADING).size(27),
        text(detail).size(14).color(MUTED)
    ]
    .spacing(4)
}

/// Shows a label and its authoritative reviewed value.
/// 显示标签及审阅计划中的权威值。
fn field<'a>(
    title: &'a str,
    value: impl iced::widget::text::IntoFragment<'a>,
    mono: bool,
) -> Row<'a, Message> {
    // Technical values use the same fixed-width treatment as the desktop UI.
    // 技术值沿用桌面界面的等宽排版。
    let value = text(value)
        .size(14)
        .wrapping(iced::widget::text::Wrapping::WordOrGlyph);
    row![
        text(title).size(13).color(MUTED).width(122),
        if mono { value.font(MONO) } else { value }
    ]
    .spacing(12)
}

/// Returns the shared localized display label for a release channel.
/// 返回发布通道的共用本地化显示标签。
fn channel_key(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "setup.channel.stable",
        Channel::Preview => "setup.channel.preview",
    }
}

/// Separates sections with the same one-pixel border as a card.
/// 使用与卡片边线一致的一像素线条划分区域。
fn divider() -> iced::widget::Rule<'static, Theme> {
    rule::horizontal(1).style(|_| rule::Style {
        color: LINE,
        radius: 0.0.into(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    })
}
