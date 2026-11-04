//! Downloading release objects over HTTPS from an ordered list of origins.
//! 通过 HTTPS 从有序的下载源列表下载发布对象。
//!
//! Every origin serves the same object layout (`layout.rs`) and none is trusted: callers verify
//! what they receive against signatures and digests. An origin that fails is skipped for the rest
//! of the call, and the last origin that answered is tried first next time. Proxies come from the
//! usual environment variables, and certificates from the operating system, as elsewhere in the app.
//!
//! 每个下载源提供相同的对象布局（`layout.rs`），且都不受信任：调用方按签名与摘要校验收到的内容。失败的
//! 下载源在本次调用中被跳过，最近一次成功应答的下载源下次优先尝试。代理取自常用的环境变量，证书取自操作
//! 系统，与应用其他部分一致。

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::assemble::PackSource;
use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::layout::{CHINA_ORIGIN, PRIMARY_ORIGIN, pack_key};

/// How long connecting to one origin may take before the next one is tried.
/// 连接某个下载源的最长时间，超时后尝试下一个。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// How long an origin may take to start answering a request.
/// 下载源开始应答一个请求的最长时间。
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// How long one response body may take; large downloads are split into requests of
/// [`DOWNLOAD_CHUNK_BYTES`] so that a stalled connection fails instead of hanging.
/// 一个响应正文的最长时间；大文件按 [`DOWNLOAD_CHUNK_BYTES`] 分成多个请求，使停滞的连接失败而不是挂起。
const BODY_TIMEOUT: Duration = Duration::from_secs(600);

/// Size of each request of a resumable file download.
/// 可续传文件下载中每个请求的大小。
const DOWNLOAD_CHUNK_BYTES: u64 = 16 << 20;

/// Which origins to use, in order (plan section 18.2).
/// 使用哪些下载源及其顺序（方案 18.2 节）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Route {
    /// The primary origin, then the mainland China mirror.
    /// 先主下载源，再中国大陆镜像。
    #[default]
    Auto,
    /// Only the primary origin.
    /// 只用主下载源。
    International,
    /// Only the mainland China mirror.
    /// 只用中国大陆镜像。
    China,
}

impl Route {
    /// The origins of this route, in the order they are tried.
    /// 此线路的下载源，按尝试顺序排列。
    pub fn origins(self) -> Vec<String> {
        let origins: &[&str] = match self {
            Self::Auto => &[PRIMARY_ORIGIN, CHINA_ORIGIN],
            Self::International => &[PRIMARY_ORIGIN],
            Self::China => &[CHINA_ORIGIN],
        };
        origins.iter().map(|origin| (*origin).to_owned()).collect()
    }
}

/// Downloads objects from ordered origins, counting received bytes and honouring cancellation.
/// 从有序下载源下载对象，统计已接收字节并响应取消。
pub struct Downloader {
    /// The HTTP agent.
    /// HTTP 代理对象。
    agent: ureq::Agent,
    /// Origins in their configured order.
    /// 按配置顺序排列的下载源。
    origins: Vec<String>,
    /// Index of the origin tried first.
    /// 优先尝试的下载源下标。
    preferred: usize,
    /// Set by another thread to stop at the next read.
    /// 由其他线程设置，在下一次读取时停止。
    cancel: Arc<AtomicBool>,
    /// Body bytes received so far.
    /// 至今已接收的正文字节数。
    received: Arc<AtomicU64>,
}

impl Downloader {
    /// Downloads from `origins`, identifying itself with `user_agent`.
    /// 从 `origins` 下载，以 `user_agent` 标识自身。
    pub fn new(origins: Vec<String>, user_agent: &str) -> FormatResult<Self> {
        if origins.is_empty() {
            return Err(FormatError::invalid(
                "downloader",
                "no origin is configured",
            ));
        }
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .user_agent(user_agent)
            .tls_config(tls)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
            .timeout_recv_body(Some(BODY_TIMEOUT))
            .http_status_as_error(false)
            .build()
            .new_agent();
        Ok(Self {
            agent,
            origins,
            preferred: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            received: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Tries `origin` first from now on, when it is one of the configured origins.
    /// 此后优先尝试 `origin`（若它是已配置的下载源之一）。
    pub fn prefer(&mut self, origin: &str) {
        if let Some(position) = self.origins.iter().position(|known| known == origin) {
            self.preferred = position;
        }
    }

    /// The origin tried first, which is the last one that answered.
    /// 优先尝试的下载源，即最近一次成功应答的下载源。
    pub fn preferred_origin(&self) -> &str {
        &self.origins[self.preferred]
    }

    /// The flag that cancels every transfer of this downloader at its next read.
    /// 在下一次读取时取消本下载器所有传输的标志。
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    /// The counter of body bytes received so far.
    /// 至今已接收正文字节数的计数器。
    pub fn received_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.received)
    }

    /// Whether the transfers were cancelled.
    /// 传输是否已被取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Reports a failure as [`FormatError::Cancelled`] when it came from cancelling.
    /// 失败由取消引起时将其报告为 [`FormatError::Cancelled`]。
    pub fn settle<T>(&self, result: FormatResult<T>) -> FormatResult<T> {
        match result {
            Err(_) if self.is_cancelled() => Err(FormatError::Cancelled),
            other => other,
        }
    }

    /// Downloads a whole object of at most `limit` bytes.
    /// 下载一个不超过 `limit` 字节的完整对象。
    pub fn fetch(&mut self, key: &str, limit: u64) -> FormatResult<Vec<u8>> {
        let result = self.open(key, None).and_then(|mut reader| {
            let mut bytes = Vec::new();
            (&mut reader)
                .take(limit.saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(|source| FormatError::io("downloading an object", source))?;
            if bytes.len() as u64 > limit {
                return Err(FormatError::TooLarge {
                    what: "downloaded object",
                    limit,
                });
            }
            Ok(bytes)
        });
        self.settle(result)
    }

    /// Downloads `key` into `target`, resuming `<target>.partial` left by an earlier attempt, and
    /// renames it to `target` only once its size and digest match.
    /// 将 `key` 下载到 `target`，续传先前尝试留下的 `<target>.partial`，大小与摘要一致后才改名为 `target`。
    pub fn download_file(
        &mut self,
        key: &str,
        target: &Path,
        size: u64,
        sha256: Sha256Digest,
    ) -> FormatResult<()> {
        let result = self.download_file_inner(key, target, size, sha256);
        self.settle(result)
    }

    /// The body of [`Self::download_file`].
    /// [`Self::download_file`] 的主体。
    fn download_file_inner(
        &mut self,
        key: &str,
        target: &Path,
        size: u64,
        sha256: Sha256Digest,
    ) -> FormatResult<()> {
        let fail = |source| FormatError::io("writing a downloaded file", source);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        let partial = partial_path(target);
        let mut hasher = Sha256::new();
        let mut have = 0_u64;
        if let Ok(mut existing) = File::open(&partial) {
            let mut buffer = vec![0_u8; 64 << 10];
            loop {
                let read = existing.read(&mut buffer).map_err(fail)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                have += read as u64;
            }
        }
        if have > size {
            std::fs::remove_file(&partial).map_err(fail)?;
            hasher = Sha256::new();
            have = 0;
        }
        let file = File::options()
            .create(true)
            .append(true)
            .open(&partial)
            .map_err(fail)?;
        let mut writer = BufWriter::new(file);
        let mut buffer = vec![0_u8; 64 << 10];
        while have < size {
            let length = (size - have).min(DOWNLOAD_CHUNK_BYTES);
            let mut reader = self.open(key, Some((have, length)))?;
            let mut remaining = length;
            while remaining > 0 {
                let want = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(0);
                let read = reader
                    .read(&mut buffer[..want])
                    .map_err(|source| FormatError::io("downloading a file", source))?;
                if read == 0 {
                    return Err(FormatError::SizeMismatch {
                        what: "downloaded range",
                        expected: length,
                        actual: length - remaining,
                    });
                }
                writer.write_all(&buffer[..read]).map_err(fail)?;
                hasher.update(&buffer[..read]);
                remaining -= read as u64;
                have += read as u64;
            }
        }
        writer.flush().map_err(fail)?;
        drop(writer);
        let actual = Sha256Digest::finish(hasher);
        if actual != sha256 {
            let _ = std::fs::remove_file(&partial);
            return Err(FormatError::DigestMismatch {
                what: "downloaded file",
                expected: sha256,
                actual,
            });
        }
        std::fs::rename(&partial, target).map_err(fail)
    }

    /// Opens `key`, or the inclusive byte range `offset..offset + length`, at the first origin that
    /// answers with the expected status, starting with the preferred one.
    /// 从优先的下载源开始，在第一个以期望状态应答的下载源打开 `key` 或字节范围 `offset..offset + length`。
    fn open(&mut self, key: &str, range: Option<(u64, u64)>) -> FormatResult<Box<dyn Read>> {
        let count = self.origins.len();
        let mut answers = Vec::new();
        for step in 0..count {
            if self.is_cancelled() {
                return Err(FormatError::Cancelled);
            }
            let position = (self.preferred + step) % count;
            let url = format!("{}/{key}", self.origins[position]);
            let mut request = self.agent.get(&url);
            if let Some((offset, length)) = range {
                if length == 0 {
                    return Ok(Box::new(std::io::empty()));
                }
                request =
                    request.header("Range", format!("bytes={offset}-{}", offset + length - 1));
            }
            let expected = if range.is_some() { 206 } else { 200 };
            match request.call() {
                Ok(response) if response.status().as_u16() == expected => {
                    self.preferred = position;
                    let reader = response.into_body().into_reader();
                    let limit = range.map_or(u64::MAX, |(_, length)| length);
                    return Ok(Box::new(Counted {
                        inner: reader.take(limit),
                        cancel: Arc::clone(&self.cancel),
                        received: Arc::clone(&self.received),
                    }));
                }
                Ok(response) => answers.push(format!(
                    "{} answered HTTP {}",
                    self.origins[position],
                    response.status().as_u16()
                )),
                Err(error) => answers.push(format!("{}: {error}", self.origins[position])),
            }
        }
        Err(FormatError::Download {
            key: key.to_owned(),
            reason: answers.join("; "),
        })
    }
}

impl PackSource for Downloader {
    fn open_range(
        &mut self,
        pack: Sha256Digest,
        offset: u64,
        length: u64,
    ) -> FormatResult<Box<dyn Read + '_>> {
        self.open(&pack_key(pack), Some((offset, length)))
    }
}

/// The partial file next to a download target.
/// 下载目标旁的未完成文件。
pub fn partial_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    target.with_file_name(name)
}

/// A response body that counts what it reads and stops once cancelled.
/// 统计读取量并在取消后停止的响应正文。
struct Counted<R> {
    /// The body.
    /// 正文。
    inner: R,
    /// Cancellation flag.
    /// 取消标志。
    cancel: Arc<AtomicBool>,
    /// Received byte counter.
    /// 已接收字节计数器。
    received: Arc<AtomicU64>,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("cancelled"));
        }
        let read = self.inner.read(buffer)?;
        self.received.fetch_add(read as u64, Ordering::Relaxed);
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, BufReader};
    use std::net::TcpListener;

    use super::*;

    /// Serves `body` at every path over plain HTTP, honouring single ranges; `broken` answers 503.
    /// 通过明文 HTTP 在每个路径提供 `body`，支持单个范围；`broken` 一律应答 503。
    fn serve(body: Vec<u8>, broken: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut range = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        let (start, end) = value.trim().split_once('-').expect("range");
                        range = Some((
                            start.parse::<usize>().expect("start"),
                            end.parse::<usize>().expect("end"),
                        ));
                    }
                }
                let (status, content) = match (broken, range) {
                    (true, _) => ("503 Service Unavailable", Vec::new()),
                    (false, Some((start, end))) => {
                        ("206 Partial Content", body[start..=end].to_vec())
                    }
                    (false, None) => ("200 OK", body.clone()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    content.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&content);
            }
        });
        origin
    }

    #[test]
    fn a_failing_origin_falls_through_and_the_answering_one_is_preferred() {
        let body = b"hello release".to_vec();
        let broken = serve(Vec::new(), true);
        let working = serve(body.clone(), false);
        let mut downloader =
            Downloader::new(vec![broken, working.clone()], "test").expect("downloader");
        assert_eq!(downloader.fetch("a", 64).expect("fetch"), body);
        assert_eq!(downloader.preferred_origin(), working);
        assert!(matches!(
            downloader.fetch("a", 4),
            Err(FormatError::TooLarge { .. })
        ));
    }

    #[test]
    fn a_file_download_resumes_its_partial_file_and_checks_the_digest() {
        let body: Vec<u8> = (0..100_000_u32).map(|value| value as u8).collect();
        let origin = serve(body.clone(), false);
        let temp = tempfile::tempdir().expect("temp dir");
        let target = temp.path().join("setup.exe");
        std::fs::write(partial_path(&target), &body[..40_000]).expect("partial");
        let mut downloader = Downloader::new(vec![origin], "test").expect("downloader");
        downloader
            .download_file("f", &target, body.len() as u64, Sha256Digest::of(&body))
            .expect("download");
        assert_eq!(std::fs::read(&target).expect("read"), body);
        assert_eq!(
            downloader.received_counter().load(Ordering::Relaxed),
            60_000,
            "only the missing tail is downloaded"
        );
        let wrong = temp.path().join("wrong.exe");
        assert!(matches!(
            downloader.download_file("f", &wrong, body.len() as u64, Sha256Digest::of(b"x")),
            Err(FormatError::DigestMismatch { .. })
        ));
        assert!(!wrong.exists() && !partial_path(&wrong).exists());
    }
}
