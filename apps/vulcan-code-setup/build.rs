//! Packages the console installer and application icon into the Windows desktop entry.
//! 将终端安装器及应用图标打入 Windows 桌面入口。

/// Embeds the selected Windows installer image and icon.
/// 内嵌选定的 Windows 安装器映像和图标。
#[allow(clippy::expect_used)]
fn main() {
    #[cfg(windows)]
    {
        // Cargo provides the output directory for this build.
        // Cargo 提供本次构建的输出目录。
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
        println!("cargo:rerun-if-env-changed=VULCAN_SETUP_CONSOLE_IMAGE");
        if std::env::var_os("CARGO_FEATURE_DESKTOP_PACKAGE").is_some() {
            if let Some(image) = std::env::var_os("VULCAN_SETUP_CONSOLE_IMAGE") {
                println!(
                    "cargo:rerun-if-changed={}",
                    std::path::Path::new(&image).display()
                );
                std::fs::copy(image, out.join("console-setup.exe"))
                    .expect("copy the console installer");
            } else {
                // Allow all-features static checks without a prebuilt program.
                // 无预构建安装器时仍允许检查全部功能组合。
                std::fs::write(out.join("console-setup.exe"), []).expect("write empty build input");
            }
        }
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
