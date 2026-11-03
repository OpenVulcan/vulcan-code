//! `VulcanCode.exe`: starts the desktop app of the current installed version.
//! `VulcanCode.exe`：启动当前已安装版本的桌面应用。
#![windows_subsystem = "windows"]

fn main() {
    std::process::exit(vulcan_code_launcher::run(
        vulcan_code_launcher::Role::Desktop,
    ));
}
