//! Entry point of `vulcan-code-setup`.
//! `vulcan-code-setup` 的入口。

/// Runs the installer's command and returns its exit status to the caller.
/// 执行安装器命令并向调用方返回退出状态。
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(vulcan_code_setup::run(&arguments));
}
