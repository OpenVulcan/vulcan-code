//! Entry point of `vulcan-code-setup`.
//! `vulcan-code-setup` 的入口。

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(vulcan_code_setup::run(&arguments));
}
