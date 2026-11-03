//! Office document to PDF conversion for the read-only PDF viewer, using the host's own LibreOffice.
//! 使用宿主自身安装的 LibreOffice 将 Office 文档转换为 PDF，供只读 PDF 查看器显示。

use std::{
    ffi::OsString,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};
use vulcan_host_tools::process_creation::new_background_tokio_command;
use vulcan_host_tools::process_tree::{
    ChildProcessTreeDropGuard, isolate_child_process_tree, terminate_child_process_tree,
};

use super::*;

/// Extensions converted to PDF, the Office formats no browser viewer reads.
/// 转换为 PDF 的扩展名，即没有浏览器查看器可读取的 Office 格式。
const WORKSPACE_OFFICE_PDF_EXTENSIONS: &[&str] = &["doc", "ppt", "pptx"];

/// Longest one LibreOffice conversion may run before its process tree is killed.
/// 单次 LibreOffice 转换在其进程树被终止前可运行的最长时间。
const WORKSPACE_OFFICE_PDF_TIMEOUT: Duration = Duration::from_secs(120);

/// Age after which cached PDFs and abandoned work folders are removed.
/// 缓存 PDF 与遗留工作文件夹在超过此时长后被移除。
const WORKSPACE_OFFICE_PDF_CACHE_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Prefix of the per-conversion work folders inside the cache.
/// 缓存中每次转换使用的工作文件夹前缀。
const WORKSPACE_OFFICE_PDF_WORK_PREFIX: &str = "work-";

/// One conversion runs at a time, because LibreOffice locks its user profile.
/// 同一时间只运行一次转换，因为 LibreOffice 会锁定其用户配置。
static WORKSPACE_OFFICE_PDF_CONVERSION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Converts the Office document at an absolute path to a cached PDF.
/// 将绝对路径处的 Office 文档转换为缓存的 PDF。
///
/// # Parameters
/// 参数
/// `file_path` is the absolute document path; it may lie outside any project, like the local media route.
/// `file_path` 是文档绝对路径；与本地媒体路由一样，它可以位于任何项目之外。
///
/// # Returns
/// 返回值
/// The cached PDF (reused while the document's size and modification time are unchanged), the missing-converter
/// status when LibreOffice is not installed, or an English error.
/// 返回缓存的 PDF（文档大小与修改时间不变时复用）、未安装 LibreOffice 时的缺少转换器状态，或英文错误。
pub(crate) async fn workspace_office_pdf(file_path: &str) -> Result<WorkspaceOfficePdf, String> {
    let source = Path::new(file_path);
    if !source.is_absolute() {
        return Err(format!("The document path is not absolute: {file_path}"));
    }
    let extension = workspace_file_extension(source).unwrap_or_default();
    if !WORKSPACE_OFFICE_PDF_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!(
            "The file is not a document converted to PDF: {file_path}"
        ));
    }
    let metadata = fs::metadata(source)
        .map_err(|error| format!("Failed to read document metadata `{file_path}`: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("The document path is not a file: {file_path}"));
    }
    let cache = office_pdf_cache_dir();
    let cached = cache.join(format!("{}.pdf", office_pdf_cache_key(source, &metadata)));
    let converted = |path: &Path| WorkspaceOfficePdf::Converted {
        pdf_path: path.to_string_lossy().into_owned(),
    };
    if cached.is_file() {
        return Ok(converted(&cached));
    }
    let Some(program) = libreoffice_program() else {
        return Ok(WorkspaceOfficePdf::ConverterMissing);
    };
    let _conversion = WORKSPACE_OFFICE_PDF_CONVERSION.lock().await;
    // An earlier request may have converted the same document while this one waited.
    // 本请求等待期间，之前的请求可能已转换同一文档。
    if cached.is_file() {
        return Ok(converted(&cached));
    }
    fs::create_dir_all(&cache).map_err(|error| {
        format!(
            "Failed to create the PDF conversion cache `{}`: {error}",
            cache.display()
        )
    })?;
    prune_office_pdf_cache(&cache);
    let work = tempfile::Builder::new()
        .prefix(WORKSPACE_OFFICE_PDF_WORK_PREFIX)
        .tempdir_in(&cache)
        .map_err(|error| format!("Failed to create a PDF conversion folder: {error}"))?;
    run_libreoffice(&program, &cache, source, work.path()).await?;
    let mut produced_name = source.file_stem().map(OsString::from).unwrap_or_default();
    produced_name.push(".pdf");
    let produced = work.path().join(produced_name);
    if !produced.is_file() {
        return Err(format!(
            "LibreOffice finished without writing a PDF for `{file_path}`."
        ));
    }
    fs::rename(&produced, &cached)
        .map_err(|error| format!("Failed to store the converted PDF for `{file_path}`: {error}"))?;
    Ok(converted(&cached))
}

/// Returns the service host's PDF conversion cache, kept in the system temp folder because every entry can be
/// converted again.
/// 返回服务宿主的 PDF 转换缓存；由于每个条目都可重新转换，因此放在系统临时文件夹中。
fn office_pdf_cache_dir() -> PathBuf {
    std::env::temp_dir().join("vulcan-code-office-pdf")
}

/// Names the cached PDF of one document version: the same path, size and modification time give the same name.
/// 为单个文档版本命名缓存 PDF：路径、大小与修改时间相同则名称相同。
fn office_pdf_cache_key(source: &Path, metadata: &fs::Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    let mut digest = Sha256::new();
    digest.update(source.to_string_lossy().as_bytes());
    digest.update(b"\0");
    digest.update(metadata.len().to_le_bytes());
    digest.update(modified.to_le_bytes());
    hex::encode(&digest.finalize()[..16])
}

/// Removes cached PDFs and work folders older than the cache age limit; removal failures leave the entry for a
/// later pass.
/// 移除超过缓存时长上限的缓存 PDF 与工作文件夹；移除失败的条目留待之后处理。
fn prune_office_pdf_cache(cache: &Path) {
    let Ok(entries) = fs::read_dir(cache) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let expired = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                now.duration_since(modified)
                    .is_ok_and(|age| age > WORKSPACE_OFFICE_PDF_CACHE_MAX_AGE)
            });
        if !expired {
            continue;
        }
        if name.ends_with(".pdf") {
            let _ = fs::remove_file(&path);
        } else if name.starts_with(WORKSPACE_OFFICE_PDF_WORK_PREFIX) {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Finds the LibreOffice program in its standard install locations and then on `PATH`.
/// 依次在标准安装位置与 `PATH` 中查找 LibreOffice 程序。
fn libreoffice_program() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(windows)]
    for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(root).join(r"LibreOffice\program\soffice.exe"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from(
            "/Applications/LibreOffice.app/Contents/MacOS/soffice",
        ));
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(
                PathBuf::from(home).join("Applications/LibreOffice.app/Contents/MacOS/soffice"),
            );
        }
    }
    let names: &[&str] = if cfg!(windows) {
        &["soffice.exe"]
    } else {
        &["soffice", "libreoffice"]
    };
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.extend(names.iter().map(|name| directory.join(name)));
        }
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// Runs one headless LibreOffice conversion of `source` into `out_dir`, killing its process tree on timeout or
/// when the request is cancelled.
/// 以无界面方式运行一次 LibreOffice，将 `source` 转换到 `out_dir`；超时或请求被取消时终止其进程树。
///
/// LibreOffice runs with its own user profile in the cache, so a LibreOffice the user has open neither blocks
/// the conversion nor sees its settings change.
/// LibreOffice 使用缓存中的独立用户配置运行，因此用户已打开的 LibreOffice 既不会阻塞转换，也不会被改动设置。
async fn run_libreoffice(
    program: &Path,
    cache: &Path,
    source: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let profile = cache.join(if cfg!(debug_assertions) {
        "profile-debug"
    } else {
        "profile-release"
    });
    let mut command = new_background_tokio_command(program);
    command
        .args([
            "--headless",
            "--norestore",
            "--nolockcheck",
            "--nologo",
            "--nodefault",
        ])
        .arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .args(["--convert-to", "pdf", "--outdir"])
        .arg(out_dir)
        .arg(source)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    isolate_child_process_tree(&mut command);
    let mut child = command.spawn().map_err(|error| {
        format!(
            "Failed to start LibreOffice `{}`: {error}",
            program.display()
        )
    })?;
    let mut guard = ChildProcessTreeDropGuard::arm(&child);
    let outcome = tokio::time::timeout(WORKSPACE_OFFICE_PDF_TIMEOUT, child.wait()).await;
    let status = match outcome {
        Ok(status) => status.map_err(|error| format!("Failed to wait for LibreOffice: {error}")),
        Err(_) => {
            let _ = terminate_child_process_tree(&mut child).await;
            let _ = child.wait().await;
            Err(format!(
                "LibreOffice did not finish converting `{}` within {} seconds.",
                source.display(),
                WORKSPACE_OFFICE_PDF_TIMEOUT.as_secs()
            ))
        }
    };
    guard.disarm();
    let status = status?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "LibreOffice could not convert `{}` ({status}).",
            source.display()
        ))
    }
}

/// Writes an absolute path as a `file://` URL, as LibreOffice's `-env:UserInstallation` expects.
/// 将绝对路径写为 LibreOffice `-env:UserInstallation` 所需的 `file://` URL。
fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

#[cfg(test)]
mod tests {
    //! Tests for the conversion request checks and cache naming.
    //! 转换请求校验与缓存命名的测试。

    use super::*;

    /// Verifies requests for relative paths and non-converted formats are refused before any conversion.
    /// 验证相对路径与非转换格式的请求在任何转换前即被拒绝。
    #[tokio::test]
    async fn office_pdf_refuses_relative_paths_and_other_formats() {
        assert!(workspace_office_pdf("slides/deck.pptx").await.is_err());
        let folder = tempfile::tempdir().expect("temp folder");
        let sheet = folder.path().join("budget.xlsx");
        fs::write(&sheet, b"sheet").expect("write sheet");
        assert!(
            workspace_office_pdf(&sheet.to_string_lossy())
                .await
                .is_err()
        );
        let missing = folder.path().join("missing.pptx");
        assert!(
            workspace_office_pdf(&missing.to_string_lossy())
                .await
                .is_err()
        );
    }

    /// Verifies a changed document gets a new cache name while an unchanged one keeps its name.
    /// 验证文档改变后获得新的缓存名称，未改变时保持原名称。
    #[test]
    fn office_pdf_cache_key_follows_the_document_version() {
        let folder = tempfile::tempdir().expect("temp folder");
        let deck = folder.path().join("deck.pptx");
        fs::write(&deck, b"first").expect("write deck");
        let first = office_pdf_cache_key(&deck, &fs::metadata(&deck).expect("metadata"));
        assert_eq!(
            first,
            office_pdf_cache_key(&deck, &fs::metadata(&deck).expect("metadata"))
        );
        fs::write(&deck, b"second version").expect("rewrite deck");
        assert_ne!(
            first,
            office_pdf_cache_key(&deck, &fs::metadata(&deck).expect("metadata"))
        );
    }

    /// Verifies profile paths become `file://` URLs with unsafe characters escaped.
    /// 验证配置路径转为 `file://` URL，且不安全字符被转义。
    #[test]
    fn file_url_escapes_paths_for_libreoffice() {
        assert_eq!(
            file_url(Path::new(r"C:\Users\A B\profile")),
            "file:///C:/Users/A%20B/profile"
        );
        assert_eq!(
            file_url(Path::new("/tmp/café/profile")),
            "file:///tmp/caf%C3%A9/profile"
        );
    }
}
