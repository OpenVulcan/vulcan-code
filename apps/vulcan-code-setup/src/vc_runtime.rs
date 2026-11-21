//! The Microsoft runtime prerequisite shared by graphical and terminal Windows setup.
//! Windows 图形与终端安装器共用的 Microsoft 运行库前置依赖。

use std::fs::OpenOptions;
use std::io::Read;
use std::os::windows::fs::OpenOptionsExt;

use vulcan_install_core::Sha256Digest;
use vulcan_install_core::fetch::Downloader;
use windows_sys::Win32::Foundation::{
    ERROR_PRODUCT_VERSION, ERROR_SUCCESS_REBOOT_INITIATED, ERROR_SUCCESS_REBOOT_REQUIRED,
};
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

use crate::report::{Reporter, Stage};

/// Microsoft's immutable download for the Authenticode-verified x64 runtime 14.52.37029.0.
/// 微软发布的 x64 运行库 14.52.37029.0 的固定下载地址，已核验 Authenticode 签名。
const ORIGIN: &str = concat!(
    "https://download.visualstudio.microsoft.com/download/pr/",
    "78d061c5-9ef3-42ab-98ed-3309d1095dc4/",
    "9C02C9ED120F671B900E6B9471D890E3B2414117A7E53F4AE2E855466FA2B7C9",
);

/// Microsoft's filename within the pinned download directory.
/// 固定下载目录下的微软原始文件名。
const FILE: &str = "VC_redist.x64.exe";

/// Exact byte length of the Microsoft installer.
/// 微软安装程序的确切字节长度。
const SIZE: u64 = 18_863_624;

/// SHA-256 of the Microsoft-signed installer.
/// 微软签名安装程序的 SHA-256 摘要。
const SHA256: &str = "9c02c9ed120f671b900e6b9471d890e3b2414117a7e53f4ae2e855466fa2b7c9";

/// Ensures the x64 runtime can load before setup starts Core.
/// 安装器启动 Core 之前确保 x64 运行库能够加载。
/// `reporter` announces installation only when the runtime is missing or broken.
/// 只有运行库缺失或损坏时，才通过 `reporter` 展示安装进度。
/// Returns an error with recovery instructions when the prerequisite remains unavailable.
/// 前置依赖仍不可用时返回包含恢复步骤的错误。
pub fn ensure(reporter: &dyn Reporter) -> Result<(), String> {
    if crate::windows::vc_runtime_available()? {
        return Ok(());
    }
    reporter.stage(Stage::Runtime);
    install().map_err(|error| {
        format!(
            "{error}\nInstall or repair the Microsoft Visual C++ x64 runtime, then retry setup: \
             https://aka.ms/vc14/vc_redist.x64.exe"
        )
    })
}

/// Downloads the pinned package and runs its silent installer, without restarting Windows.
/// 下载固定版本的安装包并静默运行，不自动重启 Windows。
/// Returns success only after confirming the runtime is loadable.
/// 只有确认运行库能够加载后才返回成功。
fn install() -> Result<(), String> {
    // Keep the package in a per-run directory through the end of the elevated installation.
    // 每次运行使用独立目录，保留安装包直到提权安装结束。
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let program = directory.path().join(FILE);
    let digest = SHA256
        .parse::<Sha256Digest>()
        .map_err(|error| error.to_string())?;
    let mut downloader = Downloader::new(vec![String::from(ORIGIN)], "Vulcan Code Setup")
        .map_err(|error| error.to_string())?;
    downloader
        .download_file(FILE, &program, SIZE, digest)
        .map_err(|error| format!("cannot download the Microsoft Visual C++ runtime: {error}"))?;

    // Deny writes and replacement from verification through the elevated launch.
    // 从校验到提权启动结束，拒绝写入或替换安装包。
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&program)
        .map_err(|error| format!("cannot open the Microsoft runtime installer: {error}"))?;
    let mut bytes = Vec::new();
    (&locked)
        .take(SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot verify the Microsoft runtime installer: {error}"))?;
    if bytes.len() as u64 != SIZE {
        return Err(String::from(
            "the Microsoft runtime installer has an unexpected size",
        ));
    }
    digest
        .expect_eq(
            "Microsoft Visual C++ runtime installer",
            Sha256Digest::of(&bytes),
        )
        .map_err(|error| error.to_string())?;
    let status = crate::windows::run_runtime_installer(&program)?;
    drop(locked);
    match status {
        ERROR_SUCCESS_REBOOT_REQUIRED | ERROR_SUCCESS_REBOOT_INITIATED => Err(String::from(
            "Restart Windows to finish installing the Visual C++ runtime.",
        )),
        // MSI's newer-version result may also be returned as an HRESULT by the bundle.
        // 安装包也可能以 HRESULT 返回 MSI 的已安装更新版本状态。
        0 | ERROR_PRODUCT_VERSION | 0x8007_0666 if crate::windows::vc_runtime_available()? => {
            Ok(())
        }
        _ => Err(format!(
            "the Microsoft Visual C++ runtime is unavailable after setup (exit 0x{status:08x})"
        )),
    }
}
