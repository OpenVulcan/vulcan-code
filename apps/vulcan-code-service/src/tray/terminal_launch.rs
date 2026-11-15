//! Opens the installed terminal client in a native terminal emulator.
//! 在原生终端模拟器中打开已安装的终端客户端。
use std::path::Path;
use std::process::Command;

/// Opens the command through LaunchServices with Terminal's executable document type.
/// 通过启动服务使用 Terminal 的可执行文稿类型打开命令。
#[cfg(target_os = "macos")]
pub(super) fn open(executable: &Path, service_root: &Path) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let run = || -> std::io::Result<()> {
        let mut script = tempfile::Builder::new().suffix(".command").tempfile()?;
        script
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o700))?;
        script.write_all(b"#!/bin/sh\n/bin/rm -f -- \"$0\"\nexec ")?;
        write_shell_word(&mut script, executable)?;
        script.write_all(b" --service-root ")?;
        write_shell_word(&mut script, service_root)?;
        script.write_all(b"\n")?;
        script.flush()?;
        // Terminal reads the document asynchronously after LaunchServices returns.
        // 启动服务返回后 Terminal 异步读取文稿。
        let (file, path) = script.keep()?;
        drop(file);
        let result = Command::new("/usr/bin/open")
            .args(["-b", "com.apple.Terminal"])
            .arg(&path)
            .output();
        match result {
            Ok(output) if output.status.success() => Ok(()),
            result => {
                let _ = std::fs::remove_file(path);
                let output = result?;
                Err(std::io::Error::other(
                    String::from_utf8_lossy(&output.stderr).trim(),
                ))
            }
        }
    };
    run().map_err(|error| error.to_string())
}

/// Writes one exact Unix path as a quoted shell word, including non-UTF-8 path bytes.
/// 将一个 Unix 路径写为单个带引号的 Shell 字，包括非 UTF-8 路径字节。
#[cfg(target_os = "macos")]
fn write_shell_word(output: &mut impl std::io::Write, path: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    output.write_all(b"'")?;
    for (index, part) in path
        .as_os_str()
        .as_bytes()
        .split(|byte| *byte == b'\'')
        .enumerate()
    {
        if index != 0 {
            output.write_all(b"'\\''")?;
        }
        output.write_all(part)?;
    }
    output.write_all(b"'")
}

/// Opens the client and its discovery root as separate arguments in an installed Linux terminal.
/// 在已安装的 Linux 终端中以独立参数打开客户端并传入发现根目录。
#[cfg(not(target_os = "macos"))]
pub(super) fn open(executable: &Path, service_root: &Path) -> Result<(), String> {
    // The freedesktop helper takes a command directly. Desktop terminals use their documented separator.
    // Freedesktop 辅助程序直接接收命令，桌面终端使用各自规定的参数分隔符。
    let terminal_programs = [
        ("xdg-terminal-exec", &[][..]),
        ("x-terminal-emulator", &["-e"][..]),
        ("gnome-terminal", &["--"][..]),
        ("konsole", &["-e"][..]),
        ("xterm", &["-e"][..]),
    ];
    let path = std::env::var_os("PATH").ok_or("the service has no executable search path")?;
    let (program, args) = terminal_programs
        .into_iter()
        .find_map(|(name, args)| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(name))
                .find(|path| path.is_file())
                .map(|program| (program, args))
        })
        .ok_or("no supported terminal emulator is installed")?;
    Command::new(program)
        .args(args)
        .arg(executable)
        .arg("--service-root")
        .arg(service_root)
        .spawn()
        .map(drop)
        .map_err(|error| error.to_string())
}
