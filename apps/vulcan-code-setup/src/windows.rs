//! The few Windows registry and message calls the setup program needs; the only unsafe code in
//! this crate.
//! 安装器所需的少量 Windows 注册表与消息调用；本 crate 中唯一的 unsafe 代码。
#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegQueryValueExW, RegSetValueExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

/// Key of the per-user environment.
/// 每用户环境变量所在的键。
const ENVIRONMENT_KEY: &str = "Environment";

/// Parent key of the per-user uninstall entries.
/// 每用户卸载项的父键。
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

/// Encodes `text` as a NUL-terminated UTF-16 string.
/// 将 `text` 编码为以 NUL 结尾的 UTF-16 字符串。
fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// An open registry key, closed when dropped.
/// 已打开的注册表键，释放时关闭。
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by RegCreateKeyExW and is closed exactly once.
        // SAFETY：句柄由 RegCreateKeyExW 打开，且只关闭一次。
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

impl Key {
    /// Opens or creates `HKCU\<path>` for reading and writing.
    /// 以读写方式打开或创建 `HKCU\<path>`。
    fn open(path: &str) -> Result<Self, String> {
        let name = wide(path);
        let mut key: HKEY = null_mut();
        // SAFETY: every pointer is valid for the call; the key handle is written on success.
        // SAFETY：每个指针在调用期间有效；成功时写入键句柄。
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                name.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                null(),
                &mut key,
                null_mut(),
            )
        };
        if status == ERROR_SUCCESS {
            Ok(Self(key))
        } else {
            Err(format!("cannot open HKCU\\{path} (error {status})"))
        }
    }

    /// Reads a string value; `None` when it does not exist.
    /// 读取字符串值；不存在时为 `None`。
    fn string(&self, value: &str) -> Result<Option<String>, String> {
        let name = wide(value);
        let mut size = 0u32;
        // SAFETY: a null data pointer asks only for the size.
        // SAFETY：数据指针为空时只查询大小。
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                null_mut(),
                null_mut(),
                &mut size,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(format!("cannot read {value} (error {status})"));
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        let mut kind = 0u32;
        // SAFETY: the buffer holds `size` bytes as reported by the previous call.
        // SAFETY：缓冲区可容纳上一次调用报告的 `size` 字节。
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                &mut kind,
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(format!("cannot read {value} (error {status})"));
        }
        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return Err(format!("{value} is not a string value"));
        }
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..end])))
    }

    /// Writes a string value of `kind`.
    /// 写入 `kind` 类型的字符串值。
    fn set_string(&self, value: &str, text: &str, kind: u32) -> Result<(), String> {
        let name = wide(value);
        let data = wide(text);
        // SAFETY: the data pointer covers the whole NUL-terminated string.
        // SAFETY：数据指针覆盖整个以 NUL 结尾的字符串。
        let status = unsafe {
            RegSetValueExW(
                self.0,
                name.as_ptr(),
                0,
                kind,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("cannot write {value} (error {status})"))
        }
    }
}

/// Whether two PATH entries name the same directory.
/// 两个 PATH 条目是否指同一目录。
fn same_dir(left: &str, right: &str) -> bool {
    left.trim_end_matches('\\')
        .eq_ignore_ascii_case(right.trim_end_matches('\\'))
}

/// Adds `dir` to the user `PATH`; returns false when it was already there.
/// 将 `dir` 加入用户 `PATH`；已存在时返回 false。
pub fn add_user_path(dir: &Path) -> Result<bool, String> {
    let key = Key::open(ENVIRONMENT_KEY)?;
    let current = key.string("Path")?.unwrap_or_default();
    let dir = dir.display().to_string();
    if current.split(';').any(|entry| same_dir(entry, &dir)) {
        return Ok(false);
    }
    let next = if current.is_empty() {
        dir
    } else {
        format!("{};{dir}", current.trim_end_matches(';'))
    };
    key.set_string("Path", &next, REG_EXPAND_SZ)?;
    broadcast_environment();
    Ok(true)
}

/// Removes `dir` from the user `PATH`, keeping every other entry as it is.
/// 从用户 `PATH` 中移除 `dir`，其他条目保持原样。
pub fn remove_user_path(dir: &Path) -> Result<(), String> {
    let key = Key::open(ENVIRONMENT_KEY)?;
    let Some(current) = key.string("Path")? else {
        return Ok(());
    };
    let dir = dir.display().to_string();
    let next: Vec<&str> = current
        .split(';')
        .filter(|entry| !same_dir(entry, &dir))
        .collect();
    let next = next.join(";");
    if next != current {
        key.set_string("Path", &next, REG_EXPAND_SZ)?;
        broadcast_environment();
    }
    Ok(())
}

/// Writes the per-user uninstall entry `key`.
/// 写入每用户卸载项 `key`。
pub fn write_uninstall_entry(
    key: &str,
    display_version: &str,
    install_location: &Path,
    uninstall: &str,
) -> Result<(), String> {
    let entry = Key::open(&format!("{UNINSTALL_KEY}\\{key}"))?;
    entry.set_string("DisplayName", "Vulcan Code", REG_SZ)?;
    entry.set_string("DisplayVersion", display_version, REG_SZ)?;
    entry.set_string("Publisher", "OpenVulcan", REG_SZ)?;
    entry.set_string(
        "InstallLocation",
        &install_location.display().to_string(),
        REG_SZ,
    )?;
    entry.set_string("UninstallString", uninstall, REG_SZ)
}

/// Deletes the per-user uninstall entry `key`; a missing one is not an error.
/// 删除每用户卸载项 `key`；不存在不算错误。
pub fn delete_uninstall_entry(key: &str) -> Result<(), String> {
    let path = wide(format!("{UNINSTALL_KEY}\\{key}"));
    // SAFETY: the subkey name is a valid NUL-terminated string.
    // SAFETY：子键名为有效的以 NUL 结尾的字符串。
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, path.as_ptr()) };
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(format!(
            "cannot delete the uninstall entry {key} (error {status})"
        ))
    }
}

/// Tells running programs that the user environment changed, so new shells see the PATH.
/// 通知运行中的程序用户环境已变化，使新开的 Shell 看到新的 PATH。
fn broadcast_environment() {
    let name = wide(ENVIRONMENT_KEY);
    // SAFETY: the parameter points at a NUL-terminated string that outlives the call; the result
    // pointer may be null.
    // SAFETY：参数指向在调用期间有效的以 NUL 结尾字符串；结果指针可以为空。
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            name.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            null_mut(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entries_compare_without_case_or_trailing_separator() {
        assert!(same_dir(
            r"C:\Users\a\VulcanCode\cli\",
            r"c:\users\A\vulcancode\CLI"
        ));
        assert!(!same_dir(r"C:\a\cli", r"C:\a\cli2"));
    }
}
