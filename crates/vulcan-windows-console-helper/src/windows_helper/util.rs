//! Windows string, command-line, and error helpers.
//! Windows 字符串、命令行与错误辅助。

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;

/// Renders a Windows command line from program and arguments.
/// 根据程序与参数渲染 Windows 命令行。
pub(super) fn windows_command_line(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .map(|value| quote_windows_argument(&value))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Quotes one Windows command-line argument.
/// 引用一个 Windows 命令行参数。
fn quote_windows_argument(value: &str) -> String {
    if value.is_empty() {
        return String::from("\"\"");
    }
    if !value.contains([' ', '\t', '"', '\\']) {
        return value.to_string();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0usize;
    for character in value.chars() {
        if character == '\\' {
            backslashes += 1;
        } else if character == '"' {
            quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
            quoted.push('"');
            backslashes = 0;
        } else {
            quoted.push_str(&"\\".repeat(backslashes));
            backslashes = 0;
            quoted.push(character);
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

/// Converts a string into a null-terminated UTF-16 buffer.
/// 将字符串转换为以空字符结尾的 UTF-16 缓冲区。
pub(super) fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Formats the last operating-system error with context.
/// 使用上下文格式化最后一个操作系统错误。
pub(super) fn last_os_error(context: &str) -> String {
    format!("{context}: {}", io::Error::last_os_error())
}
