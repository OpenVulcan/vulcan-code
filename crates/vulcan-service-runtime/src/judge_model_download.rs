//! Downloads one published format of an official judge model package from its pinned address.
//! 从固定地址下载官方判定模型包的一个已发布格式。
//!
//! The catalog pins the manifest digest; the manifest pins every file's size and digest, so nothing
//! unverified reaches the package directory. Files verified by an interrupted download are reused,
//! and the finished package is published with one rename. Registration stays with the caller.
//! 目录固定清单摘要；清单固定每个文件的大小与摘要，因此未经校验的内容不会进入模型包目录。中断下载中
//! 已校验的文件会被复用，完成的模型包通过一次重命名发布。登记由调用方负责。

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use reqwest::Url;
use vulcan_permission_judge::package::MANIFEST_FILE;
use vulcan_permission_judge::{ManifestFile, manifest_files};

use crate::managed_runtime_installer::{
    commit_staging_directory, download_file, official_download_client, remove_path_if_exists,
    sha256_file_hex, sibling_work_path, verify_sha256,
};

/// Lock file serializing downloads under one models root.
/// 串行化同一模型根目录下载的锁文件。
const DOWNLOAD_LOCK_FILE: &str = ".download.lock";

/// Hex digits of the manifest digest kept in a package directory name.
/// 模型包目录名中保留的清单摘要十六进制位数。
const DIRECTORY_DIGEST_CHARS: usize = 12;

/// Pinned location of one published package.
/// 一个已发布模型包的固定位置。
#[derive(Debug, Clone, Copy)]
pub struct ModelPackageSource<'a> {
    /// HTTPS directory holding `MANIFEST.json` and the files it lists, ending with `/`.
    /// 存放 `MANIFEST.json` 及其所列文件的 HTTPS 目录，以 `/` 结尾。
    pub base_url: &'a str,
    /// SHA-256 of the published `MANIFEST.json`.
    /// 已发布 `MANIFEST.json` 的 SHA-256。
    pub manifest_sha256: &'a str,
    /// Sum of the sizes of the files the manifest lists.
    /// 清单所列文件的大小之和。
    pub total_bytes: u64,
}

/// Stage of one download.
/// 一次下载的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelDownloadPhase {
    /// Fetching and checking the manifest.
    /// 获取并检查清单。
    Manifest,
    /// Fetching the listed files.
    /// 获取所列文件。
    Downloading,
    /// Checking a fetched or reused file.
    /// 检查已获取或复用的文件。
    Verifying,
    /// Publishing the finished package directory.
    /// 发布完成的模型包目录。
    Installing,
}

impl ModelDownloadPhase {
    /// Stable key shown to clients.
    /// 提供给客户端的稳定键。
    pub fn key(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Downloading => "downloading",
            Self::Verifying => "verifying",
            Self::Installing => "installing",
        }
    }
}

/// One progress event; byte counts cover the whole package.
/// 一个进度事件；字节数覆盖整个模型包。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelDownloadProgress {
    /// Current stage.
    /// 当前阶段。
    pub phase: ModelDownloadPhase,
    /// Bytes fetched or reused so far.
    /// 目前已获取或复用的字节数。
    pub completed_bytes: u64,
    /// Size of the package.
    /// 模型包大小。
    pub total_bytes: u64,
}

/// Directory name of one downloaded package; the manifest digest keeps every release apart, so a
/// new release never replaces a registered one.
/// 一个已下载模型包的目录名；清单摘要使每次发布互相独立，新发布绝不替换已登记的模型包。
pub fn package_directory_name(package_id: &str, format: &str, manifest_sha256: &str) -> String {
    let digest: String = manifest_sha256
        .chars()
        .take(DIRECTORY_DIGEST_CHARS)
        .collect();
    format!("{package_id}-{format}-{digest}")
}

/// Downloads `source` into `models_root/directory_name` and returns the package directory. A
/// complete package already there is returned as is.
/// 将 `source` 下载到 `models_root/directory_name` 并返回模型包目录。该位置已有完整模型包时直接返回。
pub fn download_model_package<F>(
    source: ModelPackageSource<'_>,
    models_root: &Path,
    directory_name: &str,
    progress: &mut F,
) -> Result<PathBuf, String>
where
    F: FnMut(&ModelDownloadProgress),
{
    fs::create_dir_all(models_root).map_err(|error| {
        format!(
            "Failed to create the judge model directory {}: {error}.",
            models_root.display()
        )
    })?;
    // lock serializes downloads; it is released when dropped.
    // lock 串行化下载；释放于 drop 时。
    let lock = File::create(models_root.join(DOWNLOAD_LOCK_FILE))
        .map_err(|error| format!("Failed to open the judge model download lock: {error}."))?;
    lock.try_lock_exclusive().map_err(|_| {
        String::from("Another judge model download is running; wait for it to finish.")
    })?;
    let target = models_root.join(directory_name);
    let total = source.total_bytes;
    let mut report = |phase, completed_bytes| {
        progress(&ModelDownloadProgress {
            phase,
            completed_bytes,
            total_bytes: total,
        });
    };
    report(ModelDownloadPhase::Manifest, 0);
    if let Some(files) = complete_package(&target, source)? {
        report(ModelDownloadPhase::Installing, sum_bytes(&files));
        return Ok(target);
    }
    let work = sibling_work_path(&target, "download")?;
    fs::create_dir_all(&work).map_err(|error| {
        format!(
            "Failed to create the download directory {}: {error}.",
            work.display()
        )
    })?;
    let client = official_download_client("judge-model")?;
    let manifest_path = work.join(MANIFEST_FILE);
    download_file(
        &client,
        file_url(source.base_url, MANIFEST_FILE)?.as_str(),
        &manifest_path,
        "judge-model",
        "Downloading the model manifest",
        0,
        0,
        &mut |_| {},
    )?;
    verify_sha256(&manifest_path, source.manifest_sha256)?;
    let files = read_manifest(&manifest_path, source)?;
    let mut completed = 0_u64;
    for file in &files {
        let local = entry_path(&work, &file.path);
        report(ModelDownloadPhase::Verifying, completed);
        // A file verified by an interrupted download is reused instead of fetched again.
        // 中断下载中已校验的文件直接复用，不重新获取。
        if !file_matches(&local, file) {
            let before = completed;
            download_file(
                &client,
                file_url(source.base_url, &file.path)?.as_str(),
                &local,
                "judge-model",
                &format!("Downloading {}", file.path),
                0,
                0,
                &mut |event| {
                    report(
                        ModelDownloadPhase::Downloading,
                        before + event.completed_bytes.unwrap_or(0).min(file.bytes),
                    )
                },
            )?;
            report(ModelDownloadPhase::Verifying, before + file.bytes);
            if !file_matches(&local, file) {
                remove_path_if_exists(&local)?;
                return Err(format!(
                    "The downloaded file {} does not match the published manifest.",
                    file.path
                ));
            }
        }
        completed += file.bytes;
    }
    report(ModelDownloadPhase::Installing, completed);
    commit_staging_directory(&work, &target)?;
    Ok(target)
}

/// Files of `target` when it already holds the pinned manifest and every listed file intact.
/// `target` 已包含固定清单且所列文件全部完好时返回这些文件。
fn complete_package(
    target: &Path,
    source: ModelPackageSource<'_>,
) -> Result<Option<Vec<ManifestFile>>, String> {
    let manifest_path = target.join(MANIFEST_FILE);
    if verify_sha256(&manifest_path, source.manifest_sha256).is_err() {
        return Ok(None);
    }
    let files = read_manifest(&manifest_path, source)?;
    Ok(files
        .iter()
        .all(|file| file_matches(&entry_path(target, &file.path), file))
        .then_some(files))
}

/// Reads the verified manifest and checks that it lists the size the catalog published.
/// 读取已校验的清单，并检查其所列大小与目录发布的一致。
fn read_manifest(path: &Path, source: ModelPackageSource<'_>) -> Result<Vec<ManifestFile>, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "Failed to read the model manifest {}: {error}.",
            path.display()
        )
    })?;
    let files = manifest_files(&bytes).map_err(|error| error.to_string())?;
    if sum_bytes(&files) != source.total_bytes {
        return Err(String::from(
            "The published model manifest does not list the size the catalog pins.",
        ));
    }
    Ok(files)
}

/// Total size of `files`.
/// `files` 的总大小。
fn sum_bytes(files: &[ManifestFile]) -> u64 {
    files.iter().map(|file| file.bytes).sum()
}

/// Whether `path` has the size and digest the manifest lists.
/// `path` 是否具有清单所列的大小与摘要。
fn file_matches(path: &Path, file: &ManifestFile) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() == file.bytes)
        && sha256_file_hex(path).is_ok_and(|digest| digest == file.sha256)
}

/// Local path of a manifest entry; entries are already checked to stay inside the package.
/// 清单条目的本地路径；条目已检查不会逃出模型包。
fn entry_path(root: &Path, entry: &str) -> PathBuf {
    entry
        .split('/')
        .fold(root.to_path_buf(), |path, segment| path.join(segment))
}

/// URL of `entry` under `base_url`, each segment percent-encoded.
/// `base_url` 下 `entry` 的 URL，每个路径段均经过百分号编码。
fn file_url(base_url: &str, entry: &str) -> Result<Url, String> {
    let mut url = Url::parse(base_url)
        .map_err(|error| format!("Invalid model download address `{base_url}`: {error}."))?;
    url.path_segments_mut()
        .map_err(|()| format!("Invalid model download address `{base_url}`."))?
        .pop_if_empty()
        .extend(entry.split('/'));
    Ok(url)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{ErrorKind, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use serde_json::json;
    use sha2::{Digest, Sha256};

    use super::*;

    /// Loopback server answering GET requests from a fixed table of paths.
    /// 按固定路径表应答 GET 请求的回环服务器。
    struct FileServer {
        base_url: String,
        requests: Arc<Mutex<Vec<String>>>,
        shutdown: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl FileServer {
        fn start(files: BTreeMap<String, Vec<u8>>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let base_url = format!("http://{}/models/", listener.local_addr().unwrap());
            let requests = Arc::new(Mutex::new(Vec::new()));
            let shutdown = Arc::new(AtomicBool::new(false));
            let (thread_requests, thread_shutdown) = (Arc::clone(&requests), Arc::clone(&shutdown));
            let handle = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(60);
                while !thread_shutdown.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream.set_nonblocking(false).unwrap();
                            let path = read_path(&mut stream);
                            thread_requests.lock().unwrap().push(path.clone());
                            let response = match path
                                .strip_prefix("/models/")
                                .and_then(|entry| files.get(entry))
                            {
                                Some(body) => {
                                    let mut response = format!(
                                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                        body.len()
                                    )
                                    .into_bytes();
                                    response.extend_from_slice(body);
                                    response
                                }
                                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                            };
                            let _ = stream.write_all(&response);
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < deadline,
                                "the test server outlived its bound"
                            );
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept: {error}"),
                    }
                }
            });
            Self {
                base_url,
                requests,
                shutdown,
                handle: Some(handle),
            }
        }

        fn take_requests(&self) -> Vec<String> {
            std::mem::take(&mut self.requests.lock().unwrap())
        }
    }

    impl Drop for FileServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Release);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn read_path(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        let text = String::from_utf8_lossy(&request);
        text.split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string()
    }

    fn hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Published files with their manifest; returns the table and the manifest digest.
    /// 已发布文件及其清单；返回路径表与清单摘要。
    fn publication() -> (BTreeMap<String, Vec<u8>>, String, u64) {
        let contents = [
            ("runtime-config.json", b"{}".to_vec()),
            ("models/graph.onnx", vec![7_u8; 4096]),
            ("assets/tokenizer.json", b"tokens".to_vec()),
        ];
        let mut listed = serde_json::Map::new();
        let mut files = BTreeMap::new();
        let mut total = 0;
        for (path, body) in contents {
            listed.insert(
                path.to_string(),
                json!({ "bytes": body.len(), "sha256": hex(&body) }),
            );
            total += body.len() as u64;
            files.insert(path.to_string(), body);
        }
        let manifest = serde_json::to_vec(&json!({
            "format_version": 1, "status": "release", "parent_sha256": "abc", "files": listed
        }))
        .unwrap();
        let digest = hex(&manifest);
        files.insert(MANIFEST_FILE.to_string(), manifest);
        (files, digest, total)
    }

    // A download verifies every file, publishes the package, and reuses it or verified files later.
    // 下载会校验每个文件并发布模型包，之后复用完整模型包或已校验文件。
    #[test]
    fn downloads_verify_publish_and_resume() {
        let (files, digest, total) = publication();
        let server = FileServer::start(files.clone());
        let root = tempfile::tempdir().unwrap();
        let source = ModelPackageSource {
            base_url: &server.base_url,
            manifest_sha256: &digest,
            total_bytes: total,
        };
        let name = package_directory_name("risk-164m", "fp32", &digest);
        assert_eq!(name.len(), "risk-164m-fp32-".len() + DIRECTORY_DIGEST_CHARS);

        let mut events = Vec::new();
        let package =
            download_model_package(source, root.path(), &name, &mut |event| events.push(*event))
                .unwrap();
        assert_eq!(package, root.path().join(&name));
        for (path, body) in &files {
            assert_eq!(&fs::read(entry_path(&package, path)).unwrap(), body);
        }
        assert!(!sibling_work_path(&package, "download").unwrap().exists());
        let last = events.last().unwrap();
        assert_eq!(last.phase, ModelDownloadPhase::Installing);
        assert_eq!(last.completed_bytes, total);
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].completed_bytes <= pair[1].completed_bytes)
        );
        assert_eq!(server.take_requests().len(), files.len());

        // A complete package is returned without contacting the server.
        // 完整模型包直接返回，不联系服务器。
        download_model_package(source, root.path(), &name, &mut |_| {}).unwrap();
        assert!(server.take_requests().is_empty());

        // An interrupted download keeps verified files and fetches only what is missing.
        // 中断的下载保留已校验文件，只获取缺失部分。
        let resumed = format!("{name}-resumed");
        let work = sibling_work_path(&root.path().join(&resumed), "download").unwrap();
        fs::create_dir_all(work.join("models")).unwrap();
        fs::write(
            work.join("models").join("graph.onnx"),
            &files["models/graph.onnx"],
        )
        .unwrap();
        download_model_package(source, root.path(), &resumed, &mut |_| {}).unwrap();
        let requests = server.take_requests();
        assert!(!requests.iter().any(|path| path.ends_with("graph.onnx")));
        assert_eq!(requests.len(), files.len() - 1);
    }

    // The pinned manifest digest and catalog size both guard the download.
    // 固定的清单摘要与目录大小都会保护下载。
    #[test]
    fn mismatched_pins_refuse_the_download() {
        let (files, digest, total) = publication();
        let server = FileServer::start(files);
        let root = tempfile::tempdir().unwrap();
        let wrong_digest = "0".repeat(64);
        let wrong_manifest = ModelPackageSource {
            base_url: &server.base_url,
            manifest_sha256: &wrong_digest,
            total_bytes: total,
        };
        assert!(download_model_package(wrong_manifest, root.path(), "a", &mut |_| {}).is_err());
        let wrong_size = ModelPackageSource {
            base_url: &server.base_url,
            manifest_sha256: &digest,
            total_bytes: total + 1,
        };
        assert!(download_model_package(wrong_size, root.path(), "b", &mut |_| {}).is_err());
        assert!(!root.path().join("a").exists());
        assert!(!root.path().join("b").exists());
    }

    // Entry segments are encoded under the base directory.
    // 条目路径段在基础目录下编码。
    #[test]
    fn file_urls_stay_under_the_base_directory() {
        let url = file_url("https://example.com/judge/v1/", "models/a b#c.onnx").unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/judge/v1/models/a%20b%23c.onnx"
        );
    }
}
