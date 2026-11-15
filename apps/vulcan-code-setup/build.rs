//! Embeds the console installer and the Windows executable icon.
//! 内嵌终端安装器及 Windows 可执行文件图标。

/// Stages the selected native installer image and compiles the icon on Windows.
/// 暂存选定的原生安装器映像，并在 Windows 上编译图标。
#[allow(clippy::expect_used)]
fn main() {
    println!("cargo:rerun-if-env-changed=VULCAN_SETUP_CONSOLE_IMAGE");
    if std::env::var_os("CARGO_FEATURE_DESKTOP_PACKAGE").is_some() {
        // Cargo provides the output directory for this build.
        // Cargo 提供本次构建的输出目录。
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
        if let Some(image) = std::env::var_os("VULCAN_SETUP_CONSOLE_IMAGE") {
            println!(
                "cargo:rerun-if-changed={}",
                std::path::Path::new(&image).display()
            );
            std::fs::copy(image, out.join("console-setup")).expect("copy the console installer");
        } else {
            // Allow all-features static checks without a prebuilt program.
            // 无预构建安装器时仍允许检查全部功能组合。
            std::fs::write(out.join("console-setup"), []).expect("write empty build input");
        }
    }
    #[cfg(windows)]
    {
        // Use the same icon as the application launcher.
        // 使用与应用启动器相同的图标。
        let icon = "../vulcan-code-service/icons/icon.ico";
        println!("cargo:rerun-if-changed={icon}");
        winresource::WindowsResource::new()
            .set_icon(icon)
            .compile()
            .expect("compile the Vulcan Code icon");
    }
}
