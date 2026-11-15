//! Identifies the installed Core by its executable path and operating system process identity.
//! 通过可执行文件路径及操作系统进程标识识别已安装的 Core。
use std::path::{Path, PathBuf};

use vulcan_install_core::install_root::{RUNTIME_LAYOUT_FILE, VERSIONS_DIR};

/// A running executable and its operating system process identity.
/// 运行中的可执行文件及其操作系统进程标识。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Running {
    /// The process identifier.
    /// 进程标识符。
    pub pid: u32,
    /// The program's absolute executable path from the operating system.
    /// 操作系统返回的程序可执行文件绝对路径。
    pub program: PathBuf,
    /// The kernel's process start marker, compared within the current platform.
    /// 内核记录的进程启动标记，仅在当前平台内比较。
    birth: (u64, u64),
}

/// Finds live Core processes under the install root, including older installed versions.
/// 查找安装根目录下运行中的 Core，包括较早的已安装版本。
fn scan_core(root: &Path) -> Result<Vec<Running>, String> {
    let versions = root.join(VERSIONS_DIR);
    let entries = match std::fs::read_dir(&versions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot inspect {}: {error}", versions.display())),
    };
    let mut targets = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot scan installed versions: {error}"))?;
        let dir = entry.path();
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            continue;
        }
        let canonical_dir = dir.canonicalize().map_err(|error| error.to_string())?;
        if !dir
            .join(RUNTIME_LAYOUT_FILE)
            .try_exists()
            .map_err(|error| error.to_string())?
        {
            continue;
        }
        let program = crate::root::Version::read(&dir)?.service();
        match program.canonicalize() {
            Ok(path) if path.starts_with(&canonical_dir) => targets.push(path),
            Ok(_) => {
                return Err(format!(
                    "{} leaves its installed version",
                    program.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(format!("cannot inspect {}: {error}", program.display())),
        }
    }
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(windows)]
    let processes =
        crate::windows::process_snapshot()?
            .into_iter()
            .map(|(pid, program, created)| Running {
                pid,
                program,
                birth: (created, 0),
            });
    #[cfg(unix)]
    let processes = unix::pids()?.into_iter().filter_map(unix::identify);
    let mut found = Vec::new();
    for process in processes {
        if targets.iter().any(|target| {
            process
                .program
                .file_name()
                .zip(target.file_name())
                .is_some_and(|(left, right)| same_path(Path::new(left), Path::new(right)))
        }) && let Ok(path) = process.program.canonicalize()
            && targets.iter().any(|target| same_path(&path, target))
        {
            found.push(process);
        }
    }
    found.sort_unstable_by_key(|process| process.pid);
    Ok(found)
}

/// Stops Core processes of this installation, returning whether any were running.
/// 停止此安装的 Core 进程，返回是否有运行中的进程。
pub fn stop_core(root: &Path) -> Result<bool, String> {
    let approved = scan_core(root)?;
    if approved.is_empty() {
        return Ok(false);
    }
    let remaining = force_core(root, &approved)?;
    if !remaining.is_empty() {
        return Err(String::from("Core is still running or was restarted"));
    }
    Ok(true)
}

/// Compares native executable paths according to the platform's path behavior.
/// 按平台路径规则比较可执行文件路径。
fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    return crate::windows::same_install_dir(left, right);
    #[cfg(unix)]
    return left == right;
}

/// Stops the approved Core process identities and returns any remaining Core processes.
/// 停止已获授权的 Core 进程，并返回其余 Core 进程。
fn force_core(root: &Path, approved: &[Running]) -> Result<Vec<Running>, String> {
    use std::time::{Duration, Instant};

    let present = scan_core(root)?;
    #[cfg(unix)]
    unix::stop_managed_service(&present, approved)?;
    for process in present.iter().filter(|process| approved.contains(process)) {
        #[cfg(windows)]
        crate::windows::end_matching_process(process.pid, &process.program, process.birth.0);
        #[cfg(unix)]
        unix::terminate(process);
    }
    // Bound the wait so that permissions errors and automatically restarted services can be shown.
    // 为等待设置上限，以便显示权限错误或自动拉起的服务。
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let present = scan_core(root)?;
        if !present.iter().any(|process| approved.contains(process)) || Instant::now() >= deadline {
            return Ok(present);
        }
        std::thread::sleep(Duration::from_millis(125));
    }
}

// Each unsafe operation is confined to a native process API with an owned buffer or handle.
// 将每个 unsafe 操作限定在使用独立缓冲区或句柄的原生进程 API。
#[cfg(unix)]
#[allow(unsafe_code)]
mod unix {
    use super::Running;

    /// Stops an approved systemd user service so that its restart policy cannot race a force quit.
    /// 停止已经获准关闭的 systemd 用户服务，防止重启策略与强制退出发生竞争。
    #[cfg(target_os = "linux")]
    pub fn stop_managed_service(present: &[Running], approved: &[Running]) -> Result<(), String> {
        let unit = crate::paths::systemd_user_unit()?;
        let unit = unit
            .file_name()
            .ok_or("the systemd user service has no unit name")?;
        let unit_text = unit
            .to_str()
            .ok_or("the systemd user service has an invalid unit name")?;
        // Only talk to systemd when an approved process belongs to the installed user service cgroup.
        // 仅在已获批准的进程属于该用户服务 cgroup 时与 systemd 通信。
        let is_service = present
            .iter()
            .filter(|process| approved.contains(process))
            .any(|process| {
                std::fs::read_to_string(format!("/proc/{}/cgroup", process.pid)).is_ok_and(
                    |entry| {
                        entry
                            .lines()
                            .any(|line| line.rsplit('/').next() == Some(unit_text))
                    },
                )
            });
        if !is_service {
            return Ok(());
        }
        let manager = |args: &[&str]| {
            std::process::Command::new("systemctl")
                .arg("--user")
                .args(args)
                .arg(unit)
                .output()
                .map_err(|error| format!("cannot query systemd user service: {error}"))
        };
        let status = manager(&["show", "--property=MainPID", "--value"])?;
        if !status.status.success() {
            return Err(String::from(
                "unable to verify the running systemd user service",
            ));
        }
        let pid = String::from_utf8_lossy(&status.stdout)
            .trim()
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        if present.iter().any(|process| {
            process.pid == pid
                && approved.contains(process)
                && identify(pid).as_ref() == Some(process)
        }) {
            let output = manager(&["stop", "--no-block"])?;
            if !output.status.success() {
                return Err(format!(
                    "systemd could not stop the approved service: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        Ok(())
    }

    /// Unloads the approved macOS agent before force closing so launchd cannot restart it.
    /// 强制关闭之前卸载已获批准的 macOS 代理，以防 launchd 再次启动。
    #[cfg(target_os = "macos")]
    pub fn stop_managed_service(present: &[Running], approved: &[Running]) -> Result<(), String> {
        // SAFETY: getuid has no preconditions or pointers.
        // 安全性：getuid 无前置条件和指针参数。
        let user = unsafe { libc::getuid() };
        let job = format!("gui/{user}/com.openvulcan.vulcancode.service");
        let manager = |action: &str| {
            std::process::Command::new("/bin/launchctl")
                .args([action, &job])
                .output()
                .map_err(|error| format!("cannot run launchctl: {error}"))
        };
        let status = manager("print")?;
        if !status.status.success() {
            return Ok(());
        }
        let details = String::from_utf8_lossy(&status.stdout);
        let pid = details
            .lines()
            .find_map(|line| line.trim().strip_prefix("pid = "))
            .and_then(|pid| pid.parse::<u32>().ok());
        if let Some(pid) = pid
            && present.iter().any(|process| {
                process.pid == pid
                    && approved.contains(process)
                    && identify(pid).as_ref() == Some(process)
            })
        {
            let output = manager("bootout")?;
            if !output.status.success() {
                return Err(format!(
                    "launchd could not stop the approved service: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        Ok(())
    }

    /// Lists candidate Linux process IDs from the kernel's process filesystem.
    /// 从 Linux 内核进程文件系统列出候选进程号。
    #[cfg(target_os = "linux")]
    pub fn pids() -> Result<Vec<u32>, String> {
        let entries =
            std::fs::read_dir("/proc").map_err(|error| format!("cannot scan /proc: {error}"))?;
        Ok(entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
            .collect())
    }

    /// Reads an executable and start time for an active Linux PID.
    /// 读取存活 Linux 进程的可执行文件路径及启动时间。
    #[cfg(target_os = "linux")]
    pub fn identify(pid: u32) -> Option<Running> {
        let directory = std::path::Path::new("/proc").join(pid.to_string());
        let stat = std::fs::read_to_string(directory.join("stat")).ok()?;
        // The parenthesized name can contain spaces or closing parentheses. The last one ends it.
        // 括号中的进程名可能包含空格或右括号，最后一个右括号才是结束符。
        let fields = stat.rsplit_once(')')?.1;
        let birth = fields.split_ascii_whitespace().nth(19)?.parse().ok()?;
        let program = std::fs::read_link(directory.join("exe")).ok()?;
        Some(Running {
            pid,
            program,
            birth: (birth, 0),
        })
    }

    /// Sends SIGKILL through a Linux pidfd after rechecking the approved identity.
    /// 复核授权的进程标识后，通过 Linux pidfd 发送 SIGKILL。
    #[cfg(target_os = "linux")]
    pub fn terminate(process: &Running) {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        // SAFETY: pidfd_open takes a PID and zero flags and returns a new file descriptor on success.
        // 安全性：pidfd_open 接收进程号和零标志，成功时返回新文件描述符。
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, process.pid, 0u32) };
        if fd < 0 {
            return;
        }
        // SAFETY: a successful pidfd_open returned a live, uniquely owned descriptor.
        // 安全性：pidfd_open 成功返回了有效且独占的描述符。
        let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        if identify(process.pid).as_ref() == Some(process) {
            // SAFETY: the descriptor pins this process and a null signal info is supported.
            // 安全性：描述符固定该进程，内核允许信号信息为空。
            unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    0u32,
                );
            }
        }
    }

    /// Lists macOS process IDs with room for new processes appearing during enumeration.
    /// 列出 macOS 进程号，为枚举期间出现的新进程预留空间。
    #[cfg(target_os = "macos")]
    pub fn pids() -> Result<Vec<u32>, String> {
        // SAFETY: the null-buffer form returns the estimated process count.
        // 安全性：空缓冲区形式返回估计的进程数。
        let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if count <= 0 {
            return Err(format!(
                "cannot enumerate Mac processes: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut buffer = vec![0i32; count as usize + 128];
        // SAFETY: the writable buffer is passed with its byte size.
        // 安全性：传入可写缓冲区及其字节长度。
        let actual = unsafe {
            libc::proc_listallpids(
                buffer.as_mut_ptr().cast(),
                std::mem::size_of_val(buffer.as_slice()) as i32,
            )
        };
        if actual <= 0 || actual as usize >= buffer.len() {
            return Err(String::from(
                "could not obtain a complete Mac process list; retry",
            ));
        }
        buffer.truncate(actual as usize);
        Ok(buffer
            .into_iter()
            .filter(|pid| *pid > 0)
            .map(|pid| pid as u32)
            .collect())
    }

    /// Queries macOS process birth metadata and the executable path from libproc.
    /// 使用 libproc 查询 macOS 进程启动标记及可执行文件路径。
    #[cfg(target_os = "macos")]
    pub fn identify(pid: u32) -> Option<Running> {
        use std::os::unix::ffi::OsStrExt;
        let mut birth = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        // SAFETY: the output pointer has space for the exact BSD info structure.
        // 安全性：输出指针指向与 BSD 信息结构体大小一致的空间。
        if unsafe {
            libc::proc_pidinfo(
                pid as i32,
                libc::PROC_PIDTBSDINFO,
                0,
                birth.as_mut_ptr().cast(),
                size,
            )
        } != size
        {
            return None;
        }
        // SAFETY: libproc reported that it initialized the whole structure.
        // 安全性：libproc 已报告写入完整结构体。
        let birth = unsafe { birth.assume_init() };
        let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: the path buffer has the size required by the libproc API.
        // 安全性：路径缓冲区符合 libproc API 的大小要求。
        if unsafe { libc::proc_pidpath(pid as i32, path.as_mut_ptr().cast(), path.len() as u32) }
            <= 0
        {
            return None;
        }
        let path = std::ffi::CStr::from_bytes_until_nul(&path).ok()?;
        let program = std::path::PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes()));
        Some(Running {
            pid,
            program,
            birth: (birth.pbi_start_tvsec, birth.pbi_start_tvusec),
        })
    }

    /// Rechecks the selected macOS process and requests termination only for that identity.
    /// 重新核对选中的 macOS 进程，只对该进程发出终止请求。
    #[cfg(target_os = "macos")]
    pub fn terminate(process: &Running) {
        if identify(process.pid).as_ref() == Some(process) {
            // SAFETY: the positive process ID and SIGKILL are valid process-signal arguments.
            // 安全性：正进程号及 SIGKILL 均为有效的进程信号参数。
            unsafe {
                libc::kill(process.pid as i32, libc::SIGKILL);
            }
        }
    }
}
