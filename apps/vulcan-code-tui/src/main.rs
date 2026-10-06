//! `vulcan-tui` binary entry; the command line client lives in the library's `cli` module.
//! `vulcan-tui` 二进制入口；命令行客户端位于库的 `cli` 模块。

use std::process::ExitCode;

fn main() -> ExitCode {
    vulcan_code_tui::main()
}
