//! Checking the Authenticode signer of a downloaded Windows installer (plan section 18.3).
//! 校验下载的 Windows 安装包的 Authenticode 签名者（方案 18.3 节）。
//!
//! The installer's SHA-256 already comes from the signed manifest; the signer check is a second,
//! independent gate. When the code-signing certificate is renewed, its new thumbprint must ship
//! in a release before installers signed with it are published; the release environment variable
//! `CERTUM_CERT_SHA1` names the same certificate.
//! 安装包的 SHA-256 已取自已签名的版本清单；签名者校验是第二道独立关卡。代码签名证书续期后，必须先在
//! 某个版本中发布新指纹，再发布用新证书签名的安装包；发布环境变量 `CERTUM_CERT_SHA1` 指明同一张证书。

use std::path::Path;

/// SHA-1 thumbprint of the certificate that signs Vulcan Code installers.
/// 签署 Vulcan Code 安装包的证书的 SHA-1 指纹。
#[cfg(windows)]
pub(crate) const INSTALLER_SIGNER_THUMBPRINT: &str = "19E4E2A3EFAECB6BE966D4C89B28C2C6F5C8DEA9";

/// Fails unless `path` carries a valid Authenticode signature by [`INSTALLER_SIGNER_THUMBPRINT`].
/// 除非 `path` 带有 [`INSTALLER_SIGNER_THUMBPRINT`] 的有效 Authenticode 签名，否则失败。
#[cfg(windows)]
pub(crate) fn verify_installer_signer(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt as _;
    /// Keeps the console of `powershell.exe` hidden.
    /// 隐藏 `powershell.exe` 的控制台。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // The path travels in the environment, so no quoting rule applies to it.
    // 路径经环境变量传递，因此不涉及任何引号规则。
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s = Get-AuthenticodeSignature -LiteralPath $env:VULCAN_UPDATE_INSTALLER; \
             '{0} {1}' -f $s.Status, $s.SignerCertificate.Thumbprint",
        ])
        .env("VULCAN_UPDATE_INSTALLER", path)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("could not run PowerShell to check the installer: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut words = text.split_whitespace();
    match (words.next(), words.next()) {
        (Some("Valid"), Some(thumbprint))
            if thumbprint.eq_ignore_ascii_case(INSTALLER_SIGNER_THUMBPRINT) =>
        {
            Ok(())
        }
        _ => Err(format!(
            "the installer signature is not a valid Vulcan Code signature: {}",
            text.trim()
        )),
    }
}

/// Installers are run in-app only on Windows.
/// 只有 Windows 在应用内运行安装包。
#[cfg(not(windows))]
pub(crate) fn verify_installer_signer(_path: &Path) -> Result<(), String> {
    Err(String::from(
        "only Windows runs a downloaded installer in the app",
    ))
}
