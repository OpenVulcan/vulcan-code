//! `cli\vulcan-tui.exe`: starts the terminal client of the current installed version from PATH.
//! `cli\vulcan-tui.exe`：从 PATH 启动当前已安装版本的终端客户端。

fn main() {
    std::process::exit(vulcan_code_launcher::run(
        vulcan_code_launcher::Role::Terminal,
    ));
}
