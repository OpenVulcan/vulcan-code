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
