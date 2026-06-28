//! Console screen resizing, capture, and ANSI rendering helpers.
//! 控制台屏幕尺寸调整、捕获与 ANSI 渲染辅助。

use std::mem::zeroed;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Console::{
    BACKGROUND_BLUE, BACKGROUND_GREEN, BACKGROUND_INTENSITY, BACKGROUND_RED, CHAR_INFO,
    CONSOLE_SCREEN_BUFFER_INFO, COORD, FOREGROUND_BLUE, FOREGROUND_GREEN, FOREGROUND_INTENSITY,
    FOREGROUND_RED, GetConsoleScreenBufferInfo, ReadConsoleOutputW, SMALL_RECT,
    SetConsoleScreenBufferSize, SetConsoleWindowInfo,
};

use super::util::last_os_error;

/// Resizes the attached console screen buffer and visible window.
/// 调整已附着控制台的屏幕缓冲区与可见窗口。
pub(super) fn resize_console(output: HANDLE, rows: u16, cols: u16) -> Result<(), String> {
    let rows = rows.clamp(1, 300) as i16;
    let cols = cols.clamp(1, 500) as i16;
    let size = COORD { X: cols, Y: rows };
    let ok_buffer = unsafe { SetConsoleScreenBufferSize(output, size) } != 0;
    let mut rect = SMALL_RECT {
        Left: 0,
        Top: 0,
        Right: cols.saturating_sub(1),
        Bottom: rows.saturating_sub(1),
    };
    let ok_window = unsafe { SetConsoleWindowInfo(output, 1, &mut rect) } != 0;
    if ok_buffer || ok_window {
        Ok(())
    } else {
        Err(last_os_error("Failed to resize console"))
    }
}

/// Reads the visible console screen as ANSI-styled text.
/// 将可见控制台屏幕读取为带 ANSI 样式的文本。
pub(super) fn read_screen_text(output: HANDLE) -> Result<(u16, u16, String), String> {
    let mut info = unsafe { zeroed::<CONSOLE_SCREEN_BUFFER_INFO>() };
    if unsafe { GetConsoleScreenBufferInfo(output, &mut info) } == 0 {
        return Err(last_os_error("GetConsoleScreenBufferInfo failed"));
    }
    let rows = (info.srWindow.Bottom - info.srWindow.Top + 1).max(1) as u16;
    let cols = (info.srWindow.Right - info.srWindow.Left + 1).max(1) as u16;
    let cell_count = rows as usize * cols as usize;
    let mut buffer = vec![CHAR_INFO::default(); cell_count];
    let mut read_region = info.srWindow;
    let ok = unsafe {
        ReadConsoleOutputW(
            output,
            buffer.as_mut_ptr(),
            COORD {
                X: cols as i16,
                Y: rows as i16,
            },
            COORD { X: 0, Y: 0 },
            &mut read_region,
        )
    } != 0;
    if !ok {
        return Err(last_os_error("ReadConsoleOutputW failed"));
    }
    let mut lines = Vec::new();
    for row in 0..rows as usize {
        let start = row * cols as usize;
        let end = start + cols as usize;
        lines.push(render_screen_row_as_ansi(&buffer[start..end]));
    }
    Ok((rows, cols, lines.join("\n").trim_end().to_string()))
}

/// Renders one console row as text with SGR spans for changed attributes.
/// 将单行控制台单元格渲染为属性变化时带 SGR 片段的文本。
fn render_screen_row_as_ansi(cells: &[CHAR_INFO]) -> String {
    let visible_end = cells
        .iter()
        .rposition(|cell| console_cell_char(*cell) != ' ')
        .map(|index| index + 1)
        .unwrap_or(0);
    let mut rendered = String::new();
    let mut active_attr = None;
    for cell in &cells[..visible_end] {
        let attr = console_visible_color_attr(cell.Attributes);
        if active_attr != Some(attr) {
            rendered.push_str(&console_attr_to_sgr(attr));
            active_attr = Some(attr);
        }
        rendered.push(console_cell_char(*cell));
    }
    if active_attr.is_some() {
        rendered.push_str("\x1b[0m");
    }
    rendered
}

/// Extracts the Unicode character from one console cell.
/// 从单个控制台单元格中提取 Unicode 字符。
fn console_cell_char(cell: CHAR_INFO) -> char {
    let code_unit = unsafe { cell.Char.UnicodeChar };
    char::from_u32(code_unit as u32).unwrap_or(' ')
}

/// Keeps only foreground and background color bits from a console attribute.
/// 仅保留控制台属性中的前景色与背景色位。
fn console_visible_color_attr(attr: u16) -> u16 {
    attr & (FOREGROUND_BLUE
        | FOREGROUND_GREEN
        | FOREGROUND_RED
        | FOREGROUND_INTENSITY
        | BACKGROUND_BLUE
        | BACKGROUND_GREEN
        | BACKGROUND_RED
        | BACKGROUND_INTENSITY)
}

/// Converts a Windows console color attribute into one ANSI SGR sequence.
/// 将 Windows 控制台颜色属性转换为一个 ANSI SGR 序列。
fn console_attr_to_sgr(attr: u16) -> String {
    format!(
        "\x1b[{};{}m",
        console_color_bits_to_sgr(attr, false),
        console_color_bits_to_sgr(attr >> 4, true)
    )
}

/// Converts one four-bit console color nibble into a foreground or background SGR code.
/// 将四位控制台颜色值转换为前景或背景 SGR 码。
fn console_color_bits_to_sgr(bits: u16, background: bool) -> u16 {
    let base = if background { 40 } else { 30 };
    let bright_base = if background { 100 } else { 90 };
    let ansi_index = match bits & 0x07 {
        0 => 0,
        1 => 4,
        2 => 2,
        3 => 6,
        4 => 1,
        5 => 5,
        6 => 3,
        _ => 7,
    };
    if bits & 0x08 != 0 {
        bright_base + ansi_index
    } else {
        base + ansi_index
    }
}
#[cfg(test)]
mod tests {
    use super::{
        BACKGROUND_INTENSITY, BACKGROUND_RED, FOREGROUND_BLUE, FOREGROUND_GREEN,
        FOREGROUND_INTENSITY, FOREGROUND_RED, console_color_bits_to_sgr,
    };

    /// Verifies Windows color bits are converted to standard ANSI SGR color order.
    /// 验证 Windows 颜色位会转换为标准 ANSI SGR 颜色顺序。
    #[test]
    fn console_color_bits_to_sgr_maps_windows_colors_to_ansi_order() {
        assert_eq!(console_color_bits_to_sgr(FOREGROUND_RED, false), 31);
        assert_eq!(console_color_bits_to_sgr(FOREGROUND_GREEN, false), 32);
        assert_eq!(console_color_bits_to_sgr(FOREGROUND_BLUE, false), 34);
        assert_eq!(
            console_color_bits_to_sgr(FOREGROUND_RED | FOREGROUND_INTENSITY, false),
            91
        );
        assert_eq!(
            console_color_bits_to_sgr((BACKGROUND_RED | BACKGROUND_INTENSITY) >> 4, true),
            101
        );
    }
}
