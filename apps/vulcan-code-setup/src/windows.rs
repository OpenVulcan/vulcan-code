//! The few Windows registry and message calls the setup program needs; the only unsafe code in
//! this crate.
//! 安装器所需的少量 Windows 注册表与消息调用；本 crate 中唯一的 unsafe 代码。
#![allow(unsafe_code)]

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS,
    HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegQueryValueExW,
    RegSetValueExW,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    GetCurrentProcess, GetExitCodeProcess, INFINITE, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROCESS_INFORMATION, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, QueryFullProcessImageNameW, STARTF_USESTDHANDLES,
    STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
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

/// The AppId-derived entry written by scripts/windows-installer.mjs.
/// scripts/windows-installer.mjs 按 AppId 生成的注册项。
pub const INNO_ENTRY: &str = "{4AA6A407-B39F-4AF7-9F9B-167D14D64368}_is1";

/// Reports whether Explorer started the installer without a shared interactive console.
/// 判断资源管理器是否在没有共享交互控制台的情况下启动了安装器。
pub fn standalone_console() -> bool {
    // The buffer can hold two IDs, enough to identify a shared console.
    // 缓冲区可容纳两个进程 ID，足以判断控制台是否共享。
    let mut processes = [0; 2];
    let count = unsafe {
        windows_sys::Win32::System::Console::GetConsoleProcessList(processes.as_mut_ptr(), 2)
    };
    count <= 1
}

/// Releases a console opened for a desktop launch, leaving the caller's console attached.
/// 释放桌面启动附带的控制台，保留调用方的控制台。
pub fn release_standalone_console() {
    if standalone_console() {
        // No handles from a shared terminal are detached.
        // 不分离来自共享终端的句柄。
        unsafe {
            let console = windows_sys::Win32::System::Console::GetConsoleWindow();
            if !console.is_null() {
                windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                    console,
                    windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
                );
            }
            windows_sys::Win32::System::Console::FreeConsole();
        }
    }
}

/// Reports a window initialization failure when no console is available.
/// 无控制台时使用系统对话框报告窗口初始化错误。
pub fn setup_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    // Both UTF-16 buffers outlive the modal, synchronous call.
    // 两个 UTF-16 缓冲区在同步模态调用期间始终有效。
    let title = wide("Vulcan Code Setup");
    let detail = wide(message);
    unsafe {
        MessageBoxW(
            null_mut(),
            detail.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        )
    };
}

/// Returns the primary monitor work area's logical origin and size, excluding the taskbar.
/// 返回主显示器工作区的逻辑原点及尺寸，排除任务栏。
pub fn setup_work_area() -> Option<(iced::Point, iced::Size)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_SYSTEM_AWARE, GetDpiForSystem, SetThreadDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETWORKAREA, SystemParametersInfoW, USER_DEFAULT_SCREEN_DPI,
    };

    // SPI_GETWORKAREA always reports physical coordinates, even before Iced starts its event loop.
    // 即使 Iced 事件循环尚未启动，SPI_GETWORKAREA 也始终报告物理坐标。
    let mut area = RECT::default();
    if unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut area as *mut RECT).cast(), 0) } == 0
    {
        return None;
    }
    // Read the real system DPI without changing the awareness that Winit configures for its windows.
    // 读取真实系统 DPI，同时保留 Winit 为窗口设置 DPI 感知的能力。
    let previous = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE) };
    if previous.is_null() {
        return None;
    }
    let dpi = unsafe { GetDpiForSystem() };
    unsafe { SetThreadDpiAwarenessContext(previous) };
    if dpi == 0 || area.right <= area.left || area.bottom <= area.top {
        return None;
    }
    // Convert physical work-area coordinates to the logical units used by Iced settings.
    // 将物理工作区坐标转换为 Iced 设置所用的逻辑单位。
    let scale = USER_DEFAULT_SCREEN_DPI as f32 / dpi as f32;
    Some((
        iced::Point::new(area.left as f32 * scale, area.top as f32 * scale),
        iced::Size::new(
            (area.right - area.left) as f32 * scale,
            (area.bottom - area.top) as f32 * scale,
        ),
    ))
}

/// Starts the Win32 move loop with the mouse still pressed on the custom installer.
/// 在鼠标仍按住自绘安装器时启动 Win32 移动循环。
pub fn drag_setup(owner: std::num::NonZeroIsize) {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, HTCAPTION, PostMessageW, SC_MOVE, WM_SYSCOMMAND,
    };

    // A mouse-initiated SC_MOVE takes the pointer's screen coordinates as packed signed words.
    // 鼠标发起的 SC_MOVE 接收以有符号字打包的屏幕坐标。
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return;
    }
    // The low nibble marks a caption drag, as produced by DefWindowProc for a native title bar.
    // 低四位标识标题栏拖动，复现 DefWindowProc 处理原生标题栏时的行为。
    let coords = i32::from(point.x as u16) | (i32::from(point.y as u16) << 16);
    unsafe {
        ReleaseCapture();
        PostMessageW(
            owner.get() as _,
            WM_SYSCOMMAND,
            (SC_MOVE | HTCAPTION) as usize,
            coords as isize,
        );
    }
}

/// Shows the native folder picker owned by the installer window and returns the chosen path.
/// 打开由安装器窗口拥有的系统目录选择器并返回选中的路径。
pub fn select_directory(
    owner: std::num::NonZeroIsize,
    initial: &Path,
) -> windows::core::Result<Option<PathBuf>> {
    use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem,
        SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, PCWSTR};

    // The window event thread owns the modal dialog and its COM objects until Show returns.
    // 窗口事件线程持有模态对话框及其 COM 对象，直到 Show 返回。
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    // Drop the interfaces before balancing the apartment initialization.
    // 在平衡线程单元初始化之前释放接口。
    let result = unsafe {
        (|| {
            // Request filesystem folders so the engine always receives an ordinary path.
            // 只允许选择文件系统目录，使安装引擎始终收到普通路径。
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetOptions(dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)?;
            // An uncreated install directory cannot be preselected by the system dialog.
            // 系统对话框无法预选尚未创建的安装目录。
            if initial.is_dir() {
                // Keep the wide string alive while Shell resolves it.
                // Shell 解析路径时保留宽字符串。
                let path = wide(initial.as_os_str());
                let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None)?;
                dialog.SetFolder(&item)?;
            }
            match dialog.Show(Some(HWND(owner.get() as *mut std::ffi::c_void))) {
                Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                    return Ok(None);
                }
                other => other?,
            }
            // COM allocates the string; copy it before releasing the task memory.
            // 字符串由 COM 分配，释放任务内存前先复制。
            let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = PathBuf::from(OsString::from_wide(name.as_wide()));
            CoTaskMemFree(Some(name.0.cast()));
            Ok(Some(path))
        })()
    };
    unsafe { CoUninitialize() };
    result
}

/// Per-user key of the programs Windows starts at sign-in.
/// Windows 在登录时启动的程序所在的每用户键。
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The value the release service writes under [`RUN_KEY`] (`login_run::value_name` of
/// `vulcan-code-service`).
/// 发行版服务在 [`RUN_KEY`] 下写入的值（`vulcan-code-service` 的 `login_run::value_name`）。
const RUN_VALUE: &str = "Vulcan Code";

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
    /// Opens an existing user key for inspection without creating it.
    /// 只读打开已有的用户注册项，避免创建注册项。
    fn existing(path: &str) -> Result<Option<Self>, String> {
        // Keep the name alive through the native call.
        // 在原生调用结束前保留名称。
        let name = wide(path);
        let mut key = null_mut();
        // The API writes a handle through the supplied pointer only on success.
        // API 只在成功时通过给定指针写入句柄。
        let status = unsafe {
            windows_sys::Win32::System::Registry::RegOpenKeyExW(
                HKEY_CURRENT_USER,
                name.as_ptr(),
                0,
                KEY_READ,
                &mut key,
            )
        };
        match status {
            ERROR_SUCCESS => Ok(Some(Self(key))),
            ERROR_FILE_NOT_FOUND => Ok(None),
            _ => Err(format!("cannot open HKCU\\{path}: Windows error {status}")),
        }
    }

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

    /// Deletes a value; a missing one is not an error.
    /// 删除一个值；值不存在不算错误。
    fn delete_value(&self, value: &str) -> Result<(), String> {
        let name = wide(value);
        // SAFETY: the value name is a valid NUL-terminated string.
        // SAFETY：值名为有效的以 NUL 结尾的字符串。
        let status = unsafe { RegDeleteValueW(self.0, name.as_ptr()) };
        if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("cannot delete {value} (error {status})"))
        }
    }
}

/// Whether `path` lies below the directory `dir`, compared as Windows compares names.
/// `path` 是否位于目录 `dir` 之下，按 Windows 比较名称的方式比较。
fn below(path: &str, dir: &Path) -> bool {
    let prefix = format!("{}\\", dir.display().to_string().trim_end_matches('\\'));
    path.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(&prefix))
}

/// Removes the sign-in value of the service when it starts a program below `versions`; a missing
/// value, or one that starts another installation, is left alone.
/// 登录值启动的是 `versions` 之下的程序时将其移除；值不存在或启动的是其他安装时保持不变。
pub fn remove_login_run(versions: &Path) -> Result<(), String> {
    let key = Key::open(RUN_KEY)?;
    let Some(command) = key.string(RUN_VALUE)? else {
        return Ok(());
    };
    if below(command.trim().trim_start_matches('"'), versions) {
        key.delete_value(RUN_VALUE)?;
    }
    Ok(())
}

/// An open kernel handle, closed when dropped.
/// 已打开的内核句柄，释放时关闭。
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by this module and is closed exactly once.
        // SAFETY：句柄由本模块打开，且只关闭一次。
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// The program of the process `pid`, when this user may query it.
/// 进程 `pid` 的程序路径（当本用户有权查询时）。
fn program_of(pid: u32) -> Option<String> {
    // SAFETY: OpenProcess takes plain values and returns null on failure.
    // SAFETY：OpenProcess 接收普通值，失败时返回空。
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let process = Handle(process);
    let mut buffer = vec![0u16; 32_768];
    let mut size = buffer.len() as u32;
    // SAFETY: the buffer holds `size` UTF-16 units; `size` receives the length written.
    // SAFETY：缓冲区可容纳 `size` 个 UTF-16 单元；`size` 接收写入的长度。
    let queried = unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    (queried != 0).then(|| {
        OsString::from_wide(&buffer[..size as usize])
            .to_string_lossy()
            .into_owned()
    })
}

/// Processes other than this one whose program lies below `root`, as process id and program.
/// 程序位于 `root` 之下的其他进程，以进程号与程序路径表示。
pub fn processes_below(root: &Path) -> Vec<(u32, PathBuf)> {
    // SAFETY: the snapshot call takes plain values and returns INVALID_HANDLE_VALUE on failure.
    // SAFETY：快照调用接收普通值，失败时返回 INVALID_HANDLE_VALUE。
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let snapshot = Handle(snapshot);
    // SAFETY: PROCESSENTRY32W is plain data, valid when zeroed.
    // SAFETY：PROCESSENTRY32W 为普通数据，全零时有效。
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let own = std::process::id();
    let mut found = Vec::new();
    // SAFETY: `entry` is a valid PROCESSENTRY32W whose size field is set.
    // SAFETY：`entry` 是已设置大小字段的有效 PROCESSENTRY32W。
    let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    while more {
        let pid = entry.th32ProcessID;
        if pid != 0
            && pid != own
            && let Some(program) = program_of(pid).filter(|program| below(program, root))
        {
            found.push((pid, PathBuf::from(program)));
        }
        // SAFETY: as for Process32FirstW above.
        // SAFETY：与上面的 Process32FirstW 相同。
        more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    found
}

/// Ends the process `pid` and waits up to `wait` for it to exit; a process that is gone or cannot
/// be opened is left to the caller's next scan.
/// 结束进程 `pid` 并最多等待 `wait` 让其退出；已退出或无法打开的进程留给调用方的下一次扫描。
pub fn end_process(pid: u32, wait: Duration) {
    // SAFETY: OpenProcess takes plain values and returns null on failure.
    // SAFETY：OpenProcess 接收普通值，失败时返回空。
    let process = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid) };
    if process.is_null() {
        return;
    }
    let process = Handle(process);
    let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: the handle is open with the terminate and synchronize rights.
    // SAFETY：句柄以终止与同步权限打开。
    unsafe {
        TerminateProcess(process.0, 1);
        WaitForSingleObject(process.0, millis);
    }
}

/// Appends `arg` to the command line `line` so that the program's argument parser reads it back
/// unchanged (the rules of `CommandLineToArgvW`).
/// 把 `arg` 追加到命令行 `line`，使程序的参数解析原样读回（`CommandLineToArgvW` 的规则）。
fn push_argument(line: &mut Vec<u16>, arg: &OsStr) {
    const QUOTE: u16 = b'"' as u16;
    const BACKSLASH: u16 = b'\\' as u16;
    let wide: Vec<u16> = arg.encode_wide().collect();
    if !line.is_empty() {
        line.push(u16::from(b' '));
    }
    let plain = !wide.is_empty()
        && !wide
            .iter()
            .any(|&unit| unit == QUOTE || unit == u16::from(b' ') || unit == u16::from(b'\t'));
    if plain {
        line.extend(wide);
        return;
    }
    line.push(QUOTE);
    let mut backslashes = 0;
    for unit in wide {
        if unit == BACKSLASH {
            backslashes += 1;
        } else {
            if unit == QUOTE {
                line.extend(std::iter::repeat_n(BACKSLASH, backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    line.extend(std::iter::repeat_n(BACKSLASH, backslashes));
    line.push(QUOTE);
}

/// An initialized process attribute list, deleted when dropped.
/// 已初始化的进程属性列表，释放时删除。
struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);

impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: the list was initialized by InitializeProcThreadAttributeList and is deleted
        // exactly once, before its buffer is freed.
        // SAFETY：列表由 InitializeProcThreadAttributeList 初始化，并在其缓冲区释放前只删除一次。
        unsafe { DeleteProcThreadAttributeList(self.0) }
    }
}

/// Duplicates `file`'s handle as an inheritable one.
/// 把 `file` 的句柄复制为可继承句柄。
fn inheritable(file: &std::fs::File) -> Result<Handle, String> {
    let mut copy: HANDLE = null_mut();
    // SAFETY: the source handle is open for the lifetime of `file`; the copy is written on
    // success and owned by the returned value.
    // SAFETY：源句柄在 `file` 的生命周期内保持打开；成功时写入副本，并由返回值拥有。
    let copied = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            file.as_raw_handle() as HANDLE,
            GetCurrentProcess(),
            &mut copy,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if copied == 0 {
        return Err(format!(
            "cannot pass a handle: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(Handle(copy))
}

/// Runs `program` with `args`, waits for it and returns its exit code. The program receives NUL
/// as its input, `stdout` and `stderr` as its output, and no other handle of this process, so
/// nothing it starts in turn can keep a handle of this program's caller open.
/// 以 `args` 运行 `program`，等待其结束并返回退出码。该程序以 NUL 为输入、以 `stdout` 与
/// `stderr` 为输出，不得到本进程的任何其他句柄，因此它随后启动的任何程序都无法使本程序调用方的
/// 句柄保持打开。
pub fn run_with_only(
    program: &Path,
    args: &[&str],
    stdout: &std::fs::File,
    stderr: &std::fs::File,
) -> Result<u32, String> {
    let input = std::fs::File::open("NUL").map_err(|error| format!("cannot open NUL: {error}"))?;
    let passed = [
        inheritable(&input)?,
        inheritable(stdout)?,
        inheritable(stderr)?,
    ];
    let list: [HANDLE; 3] = [passed[0].0, passed[1].0, passed[2].0];

    let mut size = 0usize;
    // SAFETY: a null list with a size pointer only reports the size the list needs.
    // SAFETY：传入空列表与大小指针时只报告列表所需的大小。
    unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size) };
    let mut buffer = vec![0u64; size.div_ceil(std::mem::size_of::<u64>()).max(1)];
    let raw: LPPROC_THREAD_ATTRIBUTE_LIST = buffer.as_mut_ptr().cast();
    // SAFETY: the buffer is at least `size` bytes, aligned, and outlives the list.
    // SAFETY：缓冲区至少 `size` 字节、已对齐，且生命周期长于列表。
    if unsafe { InitializeProcThreadAttributeList(raw, 1, 0, &mut size) } == 0 {
        return Err(format!(
            "cannot prepare the process attributes: {}",
            std::io::Error::last_os_error()
        ));
    }
    let attributes = Attributes(raw);
    // SAFETY: `list` stays alive and unchanged until CreateProcessW below has returned.
    // SAFETY：`list` 在下方 CreateProcessW 返回之前保持存活且不变。
    let updated = unsafe {
        UpdateProcThreadAttribute(
            attributes.0,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            list.as_ptr().cast(),
            std::mem::size_of_val(&list),
            null_mut(),
            null(),
        )
    };
    if updated == 0 {
        return Err(format!(
            "cannot limit the passed handles: {}",
            std::io::Error::last_os_error()
        ));
    }

    // SAFETY: STARTUPINFOEXW is a plain C structure for which all zeroes is a valid value.
    // SAFETY：STARTUPINFOEXW 是普通 C 结构，全零是有效值。
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = list[0];
    startup.StartupInfo.hStdOutput = list[1];
    startup.StartupInfo.hStdError = list[2];
    startup.lpAttributeList = attributes.0;

    let mut line = Vec::new();
    push_argument(&mut line, program.as_os_str());
    for arg in args {
        push_argument(&mut line, OsStr::new(arg));
    }
    line.push(0);
    // SAFETY: PROCESS_INFORMATION is a plain C structure for which all zeroes is a valid value.
    // SAFETY：PROCESS_INFORMATION 是普通 C 结构，全零是有效值。
    let mut started: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the command line is a writable NUL-terminated buffer, the startup information and
    // its attribute list are initialized, and the process information is written on success.
    // SAFETY：命令行是可写且以 NUL 结尾的缓冲区，启动信息及其属性列表已初始化，成功时写入进程信息。
    let created = unsafe {
        CreateProcessW(
            null(),
            line.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
            null(),
            null(),
            &startup.StartupInfo,
            &mut started,
        )
    };
    if created == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let process = Handle(started.hProcess);
    drop(Handle(started.hThread));
    drop(attributes);
    drop(passed);
    let mut code = 0u32;
    // SAFETY: the process handle is open with full access, as CreateProcessW returns it.
    // SAFETY：进程句柄由 CreateProcessW 以完全权限返回并保持打开。
    let finished = unsafe {
        WaitForSingleObject(process.0, INFINITE);
        GetExitCodeProcess(process.0, &mut code)
    };
    if finished == 0 {
        return Err(format!(
            "cannot read its exit code: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(code)
}

/// Whether two PATH entries name the same directory.
/// 两个 PATH 条目是否指同一目录。
fn same_dir(left: &str, right: &str) -> bool {
    left.trim_end_matches('\\')
        .eq_ignore_ascii_case(right.trim_end_matches('\\'))
}

/// Compares registered Windows paths without case or trailing separator differences.
/// 比较已登记的 Windows 路径，忽略大小写及末尾分隔符差异。
pub fn same_install_dir(left: &Path, right: &Path) -> bool {
    same_dir(&left.to_string_lossy(), &right.to_string_lossy())
}

/// Rejects a Windows reparse point at any managed path before writing through it.
/// 写入任一受管理路径前拒绝 Windows 重解析点。
pub fn reject_reparse_point(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    // FILE_ATTRIBUTE_REPARSE_POINT applies to junctions as well as symbolic links.
    // FILE_ATTRIBUTE_REPARSE_POINT 同时涵盖目录联接与符号链接。
    const REPARSE_POINT: u32 = 0x400;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_attributes() & REPARSE_POINT != 0 => {
            Err(format!("{} is a reparse point", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect {}: {error}", path.display())),
    }
}

/// Checks an existing shell link without resolving it or launching the program.
/// 检查现有快捷方式，避免解析或启动其程序。
pub fn shortcut_targets(path: &Path, target: &Path) -> Result<bool, String> {
    shortcut_matches(path, |found| same_install_dir(found, target))
}

/// Reports whether a shortcut launches anything inside the selected installation.
/// 判断快捷方式启动的程序是否位于所选安装内。
pub fn shortcut_belongs_to(path: &Path, root: &Path) -> Result<bool, String> {
    shortcut_matches(path, |found| below(&found.to_string_lossy(), root))
}

/// Reads a shortcut and compares its raw target with the caller's ownership predicate.
/// 读取快捷方式，并用调用方的归属条件核对未经解析的目标。
fn shortcut_matches(path: &Path, accepts: impl FnOnce(&Path) -> bool) -> Result<bool, String> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
    use windows::core::{Interface, PCWSTR};

    if !path.is_file() {
        return Ok(false);
    }
    // Use an STA for this short, synchronous COM operation and balance successful initialization.
    // 使用 STA 执行这项短暂的同步 COM 操作，并在成功初始化后成对退出。
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let result = (|| -> windows::core::Result<bool> {
        let link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
        let file: IPersistFile = link.cast()?;
        let source = wide(path);
        let mut raw_target = vec![0; 32768];
        // The COM object owns no references to these input/output buffers after either call.
        // 两次调用结束后，COM 对象均不会持有这些输入输出缓冲区。
        unsafe {
            file.Load(
                PCWSTR(source.as_ptr()),
                windows::Win32::System::Com::STGM_READ,
            )?;
            link.GetPath(&mut raw_target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
        }
        let path_length = raw_target
            .iter()
            .position(|ch| *ch == 0)
            .unwrap_or(raw_target.len());
        Ok(accepts(Path::new(&String::from_utf16_lossy(
            &raw_target[..path_length],
        ))))
    })();
    unsafe { CoUninitialize() };
    result.map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// Reads the location attached to a specific uninstall identity.
/// 读取指定卸载标识所登记的安装位置。
pub fn uninstall_location(key: &str) -> Result<Option<PathBuf>, String> {
    match Key::existing(&format!("{UNINSTALL_KEY}\\{key}"))? {
        Some(entry) => entry
            .string("InstallLocation")
            .map(|path| path.map(PathBuf::from)),
        None => Ok(None),
    }
}

/// Verifies the old registered uninstaller points at the candidate installation.
/// 验证旧注册项中的卸载程序确实指向待接管的安装。
pub fn registered_inno(root: &Path) -> Result<Option<PathBuf>, String> {
    let Some(entry) = Key::existing(&format!("{UNINSTALL_KEY}\\{INNO_ENTRY}"))? else {
        return Ok(None);
    };
    let location = entry
        .string("InstallLocation")?
        .ok_or("Inno has no InstallLocation")?;
    if !same_dir(&location, &root.to_string_lossy()) {
        return Ok(None);
    }
    let command = entry
        .string("UninstallString")?
        .ok_or("Inno has no UninstallString")?;
    let path = command
        .trim()
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or("Inno uninstall command must be one quoted executable path")?;
    let path = PathBuf::from(path);
    let name = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let valid_name = name.strip_prefix("unins").is_some_and(|digits| {
        digits.len() == 3 && digits.bytes().all(|byte| byte.is_ascii_digit())
    });
    if !valid_name
        || path.extension().and_then(OsStr::to_str) != Some("exe")
        || !path
            .parent()
            .is_some_and(|parent| same_dir(&parent.to_string_lossy(), &location))
    {
        return Err(format!(
            "Inno uninstall command points outside {} or names an unexpected program",
            root.display()
        ));
    }
    Ok(Some(path))
}

/// Reads whether the installation's command directory and service are registered for this user.
/// 读取此安装是否为当前用户登记了命令目录和服务。
pub fn inno_integration(root: &Path, cli: &Path) -> Result<(bool, bool), String> {
    let path = Key::existing(ENVIRONMENT_KEY)?
        .map(|key| key.string("Path"))
        .transpose()?
        .flatten();
    let login = Key::existing(RUN_KEY)?
        .map(|key| key.string(RUN_VALUE))
        .transpose()?
        .flatten();
    Ok((
        path.is_some_and(|value| {
            value
                .split(';')
                .any(|part| same_dir(part, &cli.to_string_lossy()))
        }),
        login.is_some_and(|value| below(value.trim().trim_start_matches('"'), root)),
    ))
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

/// Writes the per-user uninstall entry `key`; `icon` is the program whose icon Settings shows.
/// 写入每用户卸载项 `key`；`icon` 是“设置”中显示其图标的程序。
pub fn write_uninstall_entry(
    key: &str,
    display_version: &str,
    install_location: &Path,
    setup: &Path,
    icon: Option<&Path>,
) -> Result<(), String> {
    if uninstall_location(key)?
        .is_some_and(|existing| !same_install_dir(&existing, install_location))
    {
        return Err(String::from(
            "the Windows uninstall registration belongs to another location",
        ));
    }
    let entry = Key::open(&format!("{UNINSTALL_KEY}\\{key}"))?;
    entry.set_string("DisplayName", "Vulcan Code", REG_SZ)?;
    entry.set_string("DisplayVersion", display_version, REG_SZ)?;
    entry.set_string("Publisher", "OpenVulcan", REG_SZ)?;
    entry.set_string(
        "InstallLocation",
        &install_location.display().to_string(),
        REG_SZ,
    )?;
    entry.set_string(
        "DisplayIcon",
        &format!("\"{}\",0", icon.unwrap_or(setup).display()),
        REG_SZ,
    )?;
    // Explicitly pin the directory because the installed command may manage a custom location.
    // 显式指定目录，因为已安装的命令可能管理自定义位置。
    let command = format!(
        "\"{}\" uninstall --prefix \"{}\"",
        setup.display(),
        install_location.to_string_lossy().trim_end_matches('\\')
    );
    entry.set_string("UninstallString", &format!("{command} --ui=gui"), REG_SZ)?;
    entry.set_string("QuietUninstallString", &format!("{command} --yes"), REG_SZ)
}

/// Deletes `HKCU\<path>` with its subkeys; a missing key is not an error.
/// 删除 `HKCU\<path>` 及其子键；键不存在不算错误。
fn delete_tree(path: &str) -> Result<(), String> {
    let name = wide(path);
    // SAFETY: the subkey name is a valid NUL-terminated string.
    // SAFETY：子键名为有效的以 NUL 结尾的字符串。
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, name.as_ptr()) };
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(format!("cannot delete HKCU\\{path} (error {status})"))
    }
}

/// Deletes the per-user uninstall entry `key`; a missing one is not an error.
/// 删除每用户卸载项 `key`；不存在不算错误。
pub fn delete_uninstall_entry(key: &str) -> Result<(), String> {
    delete_tree(&format!("{UNINSTALL_KEY}\\{key}"))
}

/// Deletes the COM registration of the notification activator `clsid` (in braces), which the
/// desktop app writes at its first start; a missing one is not an error.
/// 删除通知激活器 `clsid`（带花括号）的 COM 注册，该注册由桌面应用在首次启动时写入；不存在不算错误。
pub fn delete_toast_activator(clsid: &str, root: &Path) -> Result<(), String> {
    let valid = clsid.len() == 38
        && clsid.starts_with('{')
        && clsid.ends_with('}')
        && clsid[1..37]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-');
    if !valid {
        return Err(format!("{clsid} is not a CLSID"));
    }
    let registration = format!(r"Software\Classes\CLSID\{clsid}");
    if let Some(server) = Key::existing(&format!(r"{registration}\LocalServer32"))?
        && server
            .string("")?
            .is_some_and(|command| below(command.trim().trim_start_matches('"'), root))
    {
        delete_tree(&registration)?;
    }
    Ok(())
}

/// Notification identity a shortcut carries.
/// 快捷方式携带的通知身份。
pub struct ShortcutIdentity<'a> {
    /// Application user model ID.
    /// 应用用户模型 ID。
    pub app_user_model_id: &'a str,
    /// CLSID of the notification activator, in braces.
    /// 通知激活器的 CLSID，带花括号。
    pub toast_activator_clsid: &'a str,
}

/// Writes the shortcut `path` that starts `target` in `working_dir`, carrying `identity` so that
/// Windows groups the window and its notifications with it.
/// 写入在 `working_dir` 中启动 `target` 的快捷方式 `path`，并携带 `identity`，使 Windows 将窗口及其
/// 通知与它归为一组。
pub fn write_shortcut(
    path: &Path,
    target: &Path,
    working_dir: &Path,
    identity: Option<ShortcutIdentity<'_>>,
) -> Result<(), String> {
    use windows::Win32::Storage::EnhancedStorage::{
        PKEY_AppUserModel_ID, PKEY_AppUserModel_ToastActivatorCLSID,
    };
    use windows::Win32::System::Com::StructuredStorage::{
        InitPropVariantFromCLSID, PROPVARIANT, PropVariantClear,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, SHStrDupW, ShellLink};
    use windows::core::{GUID, Interface, PCWSTR};

    let target_name = wide(target);
    let working_name = wide(working_dir);
    let path_name = wide(path);
    // SAFETY: every string passed to COM is a NUL-terminated buffer that outlives the call; both
    // property values are allocated by COM (`SHStrDupW`, `InitPropVariantFromCLSID`), as the
    // property store expects, and each is cleared exactly once.
    // SAFETY：传给 COM 的每个字符串都是在调用期间有效、以 NUL 结尾的缓冲区；两个属性值都按属性存储的要求由
    // COM 分配（`SHStrDupW`、`InitPropVariantFromCLSID`），且各自只清除一次。
    unsafe {
        // COM may already be initialized on this thread, in which mode does not matter here.
        // 本线程可能已初始化 COM，此处对其模式没有要求。
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| -> windows::core::Result<()> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(PCWSTR(target_name.as_ptr()))?;
            link.SetWorkingDirectory(PCWSTR(working_name.as_ptr()))?;
            if let Some(identity) = identity {
                let store: IPropertyStore = link.cast()?;
                let id = wide(identity.app_user_model_id);
                let mut value = PROPVARIANT::default();
                (*value.Anonymous.Anonymous).Anonymous.pwszVal = SHStrDupW(PCWSTR(id.as_ptr()))?;
                (*value.Anonymous.Anonymous).vt = VT_LPWSTR;
                let set = store.SetValue(&PKEY_AppUserModel_ID, &value);
                let _ = PropVariantClear(&mut value);
                set?;
                let clsid = GUID::try_from(
                    identity
                        .toast_activator_clsid
                        .trim_start_matches('{')
                        .trim_end_matches('}'),
                )?;
                let mut activator = InitPropVariantFromCLSID(&clsid)?;
                let set = store.SetValue(&PKEY_AppUserModel_ToastActivatorCLSID, &activator);
                let _ = PropVariantClear(&mut activator);
                set?;
                store.Commit()?;
            }
            let file: IPersistFile = link.cast()?;
            file.Save(PCWSTR(path_name.as_ptr()), true)
        })();
        if initialized {
            CoUninitialize();
        }
        result.map_err(|error| format!("cannot write the shortcut {}: {error}", path.display()))
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

    /// Verifies real registry and installer behavior on copies of published Windows releases.
    /// 使用已发布 Windows 版本的副本验证真实注册表及安装器行为。
    #[test]
    #[ignore = "requires extracted official Windows releases and network access"]
    fn published_inno_installations_upgrade_directly() {
        // Redirect HKCU in this process so live registration, PATH and login entries stay isolated.
        // 在当前进程重定向 HKCU，使真实安装的注册项、PATH 和登录项保持隔离。
        struct IsolatedRegistry(Key, String);
        impl Drop for IsolatedRegistry {
            /// Restores the predefined handle and removes only this test's registry subtree.
            /// 恢复预定义句柄，并仅删除本测试的注册表子树。
            fn drop(&mut self) {
                unsafe {
                    windows_sys::Win32::System::Registry::RegOverridePredefKey(
                        HKEY_CURRENT_USER,
                        null_mut(),
                    )
                };
                let _ = &self.0;
                let _ = delete_tree(&self.1);
            }
        }
        let root = PathBuf::from(
            std::env::var_os("VULCAN_SETUP_MIGRATION_FIXTURE")
                .expect("isolated copied installation"),
        );
        // The ignored test exercises real removal; accept only a dedicated temporary fixture.
        // 此忽略测试执行真实移除，因此只接受专用临时夹具。
        let fixtures = std::env::temp_dir()
            .join("vulcan-setup-validation")
            .canonicalize()
            .expect("fixture directory");
        let canonical = root.canonicalize().expect("fixture root");
        assert!(canonical != fixtures && canonical.starts_with(fixtures));
        let target = std::env::var("VULCAN_SETUP_MIGRATION_TARGET")
            .expect("published target version")
            .parse()
            .expect("version syntax");
        let key_name = format!(
            "Software\\OpenVulcan\\SetupMigrationTest-{}",
            std::process::id()
        );
        let key = Key::open(&key_name).expect("temporary registry");
        assert_eq!(
            unsafe {
                windows_sys::Win32::System::Registry::RegOverridePredefKey(HKEY_CURRENT_USER, key.0)
            },
            ERROR_SUCCESS
        );
        let _registry = IsolatedRegistry(key, key_name);
        let entry = Key::open(&format!("{UNINSTALL_KEY}\\{INNO_ENTRY}"))
            .expect("fixture uninstall registry");
        entry
            .set_string("InstallLocation", &format!("{}\\", root.display()), REG_SZ)
            .expect("registered path");
        entry
            .set_string(
                "UninstallString",
                &format!("\"{}\"", root.join("unins000.exe").display()),
                REG_SZ,
            )
            .expect("registered uninstaller");
        drop(entry);
        let installed = vulcan_install_core::install_root::InstallRoot::new(&root);
        let previous = installed
            .current_version()
            .expect("current pointer")
            .expect("old version");
        assert!(target > previous);
        assert!(root.is_absolute());
        assert!(
            crate::state::SetupState::read(&root)
                .expect("fixture state")
                .is_none()
        );
        assert_eq!(
            crate::install::resolve_root(None, vulcan_install_core::names::Platform::WindowsX64)
                .expect("registered discovery"),
            root
        );
        // The recent updater supplies a complete target directory before invoking its bridge.
        // 较新的更新器会在调用兼容入口前提供完整目标目录。
        let cached_target = installed.version_dir(&target).is_dir();
        // The fixture's unrelated file represents user content kept even through uninstall.
        // 夹具中无关的文件代表即使卸载也应保留的用户内容。
        let user_file = root.join("user-data-keep.txt");
        std::fs::write(&user_file, "keep me").expect("user content");
        crate::install::adopt_release(&root, &target, vulcan_install_core::names::Channel::Stable)
            .expect("direct upgrade");
        assert_eq!(
            installed.current_version().expect("updated pointer"),
            Some(target.clone())
        );
        assert!(installed.version_dir(&previous).is_dir());
        assert!(installed.setup_owned());
        assert!(uninstall_location(INNO_ENTRY).expect("old key").is_none());
        assert!(!root.join("unins000.exe").exists());
        let key = crate::integrate::windows_uninstall_key(&root).expect("new key");
        let entry = Key::existing(&format!("{UNINSTALL_KEY}\\{key}"))
            .expect("key read")
            .expect("new uninstall registration");
        assert_eq!(
            entry.string("DisplayVersion").expect("registry version"),
            Some(target.to_string())
        );
        let interactive = entry
            .string("UninstallString")
            .expect("registry string")
            .expect("GUI command");
        let quiet = entry
            .string("QuietUninstallString")
            .expect("registry string")
            .expect("quiet command");
        assert!(interactive.ends_with("--ui=gui"));
        assert!(quiet.ends_with("--yes"));
        for command in [interactive, quiet] {
            assert!(command.contains(&format!("--prefix \"{}\"", root.display())));
        }
        drop(entry);
        crate::uninstall::uninstall(&crate::args::UninstallOptions {
            common: crate::args::CommonOptions {
                prefix: Some(root.clone()),
                yes: true,
                ..Default::default()
            },
            purge: false,
        })
        .expect("uninstall adopted root");
        assert!(uninstall_location(&key).expect("new key").is_none());
        assert_eq!(
            std::fs::read_to_string(user_file).expect("preserved user content"),
            "keep me"
        );
        assert!(!root.join("versions").exists());
        println!(
            "Verified published Windows {previous} -> {target}; cached target: {cached_target}; registry and preserved data verified"
        );
    }

    #[test]
    fn arguments_survive_the_command_line() {
        let mut line = Vec::new();
        for arg in [
            r"C:\Program Files\x.exe",
            "plain",
            "",
            r#"a"b"#,
            r"tail\ ",
            r"end\",
        ] {
            push_argument(&mut line, OsStr::new(arg));
        }
        let text = String::from_utf16(&line).expect("ASCII stays UTF-16");
        assert_eq!(
            text,
            r#""C:\Program Files\x.exe" plain "" "a\"b" "tail\ " end\"#
        );
    }

    #[test]
    fn a_program_gets_only_the_given_output() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let out = std::fs::File::create(dir.path().join("out")).expect("output file");
        let err = std::fs::File::create(dir.path().join("err")).expect("error file");
        let cmd = Path::new(&std::env::var_os("SystemRoot").expect("SystemRoot"))
            .join(r"System32\cmd.exe");
        let code = run_with_only(
            &cmd,
            &["/d", "/c", "echo out& echo err 1>&2& exit 3"],
            &out,
            &err,
        )
        .expect("cmd runs");
        assert_eq!(code, 3);
        let read = |name| std::fs::read_to_string(dir.path().join(name)).expect("readable");
        assert_eq!(read("out").trim(), "out");
        assert_eq!(read("err").trim(), "err");
    }

    #[test]
    fn path_entries_compare_without_case_or_trailing_separator() {
        assert!(same_dir(
            r"C:\Users\a\VulcanCode\cli\",
            r"c:\users\A\vulcancode\CLI"
        ));
        assert!(!same_dir(r"C:\a\cli", r"C:\a\cli2"));
    }

    #[test]
    fn programs_below_a_root_compare_without_case_and_whole_names_only() {
        let root = Path::new(r"C:\Users\a\VulcanCode");
        assert!(below(
            r"c:\users\A\vulcancode\versions\0.1.1\bin\x.exe",
            root
        ));
        assert!(below(r"C:\Users\a\VulcanCode\VulcanCode.exe", root));
        assert!(!below(r"C:\Users\a\VulcanCode2\VulcanCode.exe", root));
        assert!(!below(r"C:\Users\a\VulcanCode", root));
    }
}
