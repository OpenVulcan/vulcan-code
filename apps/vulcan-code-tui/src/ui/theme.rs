// Vulcan color tokens for the terminal.
// 终端使用的 Vulcan 颜色令牌。
//
// Contract owner: `apps/vulcan-code-web/src/styles/foundations/theme.css` in vulcan-code. Values are
// transcribed from the light and dark token sets there; re-read that file and update these tokens
// when the WebUI palette changes.
// 契约归属：vulcan-code 的 `apps/vulcan-code-web/src/styles/foundations/theme.css`。取值转录自其中
// 亮色与暗色令牌；WebUI 调色板变化时回读该文件并同步更新。

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,
    pub bg: Color,
    pub fg: Color,
    pub card: Color,
    pub popover: Color,
    pub primary: Color,
    pub primary_fg: Color,
    pub muted: Color,
    pub muted_fg: Color,
    pub accent: Color,
    pub border: Color,
    pub input: Color,
    pub sidebar: Color,
    pub sidebar_fg: Color,
    pub sidebar_accent: Color,
    pub sidebar_border: Color,
    pub success: Color,
    pub danger: Color,
    pub info: Color,
    pub warning: Color,
    pub violet: Color,
    pub teal: Color,
    pub flame_top: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Context usage from which the gauge warns, as in the WebUI composer (`threshold-90`).
/// 上下文用量达到此百分比时进度条开始警示，对应 WebUI 输入框（`threshold-90`）。
pub const USAGE_WARNING_PERCENT: u8 = 90;
/// Context usage from which the gauge is critical (`threshold-95`).
/// 上下文用量达到此百分比时进度条进入危急状态（`threshold-95`）。
pub const USAGE_CRITICAL_PERCENT: u8 = 95;

/// Command screen surface. It stays dark in both themes, like the WebUI emulator theme in
/// `components/terminal/terminal-viewport-runtime.tsx`; re-read that file when it changes.
/// 命令屏幕表面，与 WebUI `components/terminal/terminal-viewport-runtime.tsx` 的模拟器主题一样在两种主题下
/// 都保持深色；该文件变化时回读同步。
pub const TERMINAL_SURFACE: Color = rgb(0x111318);
/// Plain output on the command screen, from the same WebUI emulator theme.
/// 命令屏幕上的普通输出，取自同一 WebUI 模拟器主题。
pub const TERMINAL_FG: Color = rgb(0xd9dee8);
/// Share of `muted_fg` in faint secondary text; the rest is the surface beneath it.
/// 弱化次要文字中 `muted_fg` 的占比；其余为其下方表面色。
const FAINT_TEXT_WEIGHT: f32 = 0.75;
/// Share of the tone color in a badge's text; the rest is `fg`.
/// 标签片文字中语气颜色的占比；其余为 `fg`。
const BADGE_TEXT_WEIGHT: f32 = 0.8;
/// Share of the tone color in a badge's background; the rest is `bg`.
/// 标签片底色中语气颜色的占比；其余为 `bg`。
const BADGE_TINT_WEIGHT: f32 = 0.12;

impl Theme {
    /// Color of a usage gauge at `percent`: normal, warning, then critical.
    /// 用量进度条在 `percent` 时的颜色：正常、警示、危急。
    pub fn usage_color(&self, percent: u8) -> Color {
        if percent >= USAGE_CRITICAL_PERCENT {
            self.danger
        } else if percent >= USAGE_WARNING_PERCENT {
            self.warning
        } else {
            self.info
        }
    }

    pub fn dark() -> Self {
        Theme {
            mode: ThemeMode::Dark,
            bg: rgb(0x16191d),
            fg: rgb(0xeef0f3),
            card: rgb(0x1d2025),
            popover: rgb(0x23272c),
            primary: rgb(0xf87331),
            primary_fg: rgb(0x16191d),
            muted: rgb(0x262a30),
            muted_fg: rgb(0x9aa0a8),
            accent: rgb(0x2a2e34),
            border: rgb(0x2f3339),
            input: rgb(0x3f444b),
            sidebar: rgb(0x111418),
            sidebar_fg: rgb(0xced1d6),
            sidebar_accent: rgb(0x22262b),
            sidebar_border: rgb(0x202328),
            success: rgb(0x2ea043),
            danger: rgb(0xf85149),
            info: rgb(0x85a9db),
            warning: rgb(0xe1b66b),
            violet: rgb(0xbe9bdf),
            teal: rgb(0x6ec4af),
            flame_top: rgb(0xff6527),
        }
    }

    pub fn light() -> Self {
        Theme {
            mode: ThemeMode::Light,
            bg: rgb(0xf7f7f5),
            fg: rgb(0x1a1917),
            card: rgb(0xfdfdfc),
            popover: rgb(0xfdfdfc),
            primary: rgb(0xc84709),
            primary_fg: rgb(0xffffff),
            muted: rgb(0xefeeeb),
            muted_fg: rgb(0x5f5d5a),
            accent: rgb(0xeae9e6),
            border: rgb(0xe3e1de),
            input: rgb(0xd1cfcb),
            sidebar: rgb(0xf0efec),
            sidebar_fg: rgb(0x31302d),
            sidebar_accent: rgb(0xe5e3e0),
            sidebar_border: rgb(0xe3e1de),
            success: rgb(0x1f883d),
            danger: rgb(0xcf222e),
            info: rgb(0x547cac),
            warning: rgb(0xbc8838),
            violet: rgb(0x9970b8),
            teal: rgb(0x368b7c),
            flame_top: rgb(0xff6527),
        }
    }

    pub fn toggled(&self) -> Self {
        match self.mode {
            ThemeMode::Dark => Theme::light(),
            ThemeMode::Light => Theme::dark(),
        }
    }

    pub fn base(&self) -> Style {
        Style::new().fg(self.fg).bg(self.bg)
    }

    pub fn muted_text(&self) -> Style {
        Style::new().fg(self.muted_fg)
    }

    pub fn strong(&self) -> Style {
        Style::new().fg(self.fg).add_modifier(Modifier::BOLD)
    }

    pub fn primary_text(&self) -> Style {
        Style::new().fg(self.primary)
    }

    /// Background tint for added diff lines.
    /// 新增 diff 行的背景色。
    pub fn diff_add_bg(&self) -> Color {
        mix(self.success, self.bg, 0.18)
    }

    pub fn diff_del_bg(&self) -> Color {
        mix(self.danger, self.bg, 0.18)
    }

    pub fn selection_bg(&self) -> Color {
        mix(self.primary, self.bg, 0.16)
    }

    /// Surface of chips, plain buttons and switch tracks: the WebUI input token, so a control stays
    /// visible on the page, the sidebar and a selected row alike.
    /// 按钮片、普通按钮与开关轨道的底色：取 WebUI 输入框令牌，使控件在页面、侧栏与选中行上都清晰可见。
    pub fn control_bg(&self) -> Color {
        mix(self.input, self.bg, 0.75)
    }

    /// Secondary text one step quieter than `muted_fg`, drawn on `surface`; it still keeps
    /// a 3:1 contrast against that surface in both palettes.
    /// 比 `muted_fg` 再弱一级、绘制在 `surface` 上的次要文字；两套调色板中仍保持 3:1 对比度。
    pub fn faint_fg(&self, surface: Color) -> Color {
        mix(self.muted_fg, surface, FAINT_TEXT_WEIGHT)
    }

    /// Tinted label chip in `color`: a light wash of the color behind text pulled toward `fg`.
    /// 以 `color` 着色的标签片：底色为该颜色的浅色淡染，文字向 `fg` 偏移。
    pub fn badge(&self, color: Color) -> Style {
        Style::new()
            .fg(mix(color, self.fg, BADGE_TEXT_WEIGHT))
            .bg(mix(color, self.bg, BADGE_TINT_WEIGHT))
    }

    /// Switch drawn as a track with a knob; both states have the same width so rows never shift.
    /// 以轨道加滑块绘制的开关；两种状态宽度相同，行内容不会跳动。
    /// `on` is the current state and `labels` the localized on/off words; returns the switch spans.
    /// `on` 为当前状态，`labels` 为本地化的开/关文字；返回开关片段。
    pub fn switch(&self, on: bool, labels: (&str, &str)) -> Vec<ratatui::text::Span<'static>> {
        use ratatui::text::Span;
        let (on_label, off_label) = labels;
        let width = crate::ui::text::width_of(on_label).max(crate::ui::text::width_of(off_label));
        let pad = |label: &str| {
            format!(
                " {label}{}",
                " ".repeat(width - crate::ui::text::width_of(label))
            )
        };
        if on {
            vec![
                Span::styled(
                    " ━━● ",
                    Style::new()
                        .fg(self.primary_fg)
                        .bg(self.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    pad(on_label),
                    Style::new().fg(self.success).add_modifier(Modifier::BOLD),
                ),
            ]
        } else {
            vec![
                Span::styled(" ●━━ ", Style::new().fg(self.fg).bg(self.control_bg())),
                Span::styled(pad(off_label), Style::new().fg(self.fg)),
            ]
        }
    }
}

/// Linear blend `a * t + b * (1 - t)` for RGB colors; non-RGB colors return `a`.
/// RGB 颜色线性混合 `a * t + b * (1 - t)`；非 RGB 颜色返回 `a`。
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let t = t.clamp(0.0, 1.0);
            let blend = |x: u8, y: u8| (x as f32 * t + y as f32 * (1.0 - t)).round() as u8;
            Color::Rgb(blend(ar, br), blend(ag, bg), blend(ab, bb))
        }
        _ => a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lowest WCAG contrast ratio any themed text may have against the surface it is drawn on.
    /// 任何主题文字相对其所在表面允许的最低 WCAG 对比度。
    const MIN_TEXT_CONTRAST: f64 = 3.0;

    /// WCAG contrast ratio of two RGB colors.
    /// 两个 RGB 颜色的 WCAG 对比度。
    fn contrast(a: Color, b: Color) -> f64 {
        let luminance = |color| match color {
            Color::Rgb(r, g, b) => {
                let channel = |value: u8| {
                    let value = f64::from(value) / 255.0;
                    if value <= 0.039_28 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
                };
                0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
            }
            other => panic!("theme colors are RGB, got {other:?}"),
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Derived text tiers stay legible on every surface and tone in both palettes.
    /// 派生文字层级在两套调色板的每个表面与语气上都保持可读。
    #[test]
    fn derived_text_keeps_minimum_contrast() {
        for theme in [Theme::dark(), Theme::light()] {
            for surface in [theme.bg, theme.card, theme.popover, theme.sidebar] {
                let ratio = contrast(theme.faint_fg(surface), surface);
                assert!(
                    ratio >= MIN_TEXT_CONTRAST,
                    "{:?} faint text on {surface:?}: {ratio:.2}",
                    theme.mode
                );
            }
            for tone in [
                theme.fg,
                theme.muted_fg,
                theme.primary,
                theme.info,
                theme.success,
                theme.warning,
                theme.danger,
                theme.violet,
                theme.teal,
            ] {
                let style = theme.badge(tone);
                let ratio = contrast(style.fg.unwrap(), style.bg.unwrap());
                assert!(
                    ratio >= MIN_TEXT_CONTRAST,
                    "{:?} badge in {tone:?}: {ratio:.2}",
                    theme.mode
                );
            }
        }
    }
}
