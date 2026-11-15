//! Exercises two signed releases over HTTP against a disposable installation.
//! 通过 HTTP 对可丢弃的安装验证两个连续签名发布。

use super::*;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use vulcan_install_core::assemble::DirectoryPackSource;
use vulcan_install_core::install_root::InstallRoot;
use vulcan_install_core::layout::{index_key, manifest_key, pack_key, pointer_key, signature_key};
use vulcan_install_core::manifest::ComponentRef;
use vulcan_install_core::names::{ComponentName, PRODUCT, Platform, SCHEMA_VERSION};
use vulcan_install_core::pack::{FileLocation, PackBuilder, write_object};
use vulcan_install_core::path::RelativePath;
use vulcan_install_core::pointer::{ChannelPointer, DocumentRef};
use vulcan_install_core::signing::{public_key_of, sign_detached};

/// Fixture signing data, independent of release credentials.
/// 测试签名数据，与发行凭据独立。
const TEST_SECRET: [u8; 32] = [19; 32];
const TEST_KEY_ID: &str = "local-update-test";

/// Captures the actual pack key and compressed byte range served to the updater.
/// 记录实际向更新器发送的压缩包键及字节范围。
type Range = (String, usize, usize);

/// Serves immutable release objects and the mutable channel pointer over loopback HTTP.
/// 通过回环 HTTP 提供不可变发布对象及可变通道指针。
struct Source {
    origin: String,
    ranges: Arc<Mutex<Vec<Range>>>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Source {
    /// Opens a bounded local origin for the objects in `directory`.
    /// 为 `directory` 内的对象启动有界的本地发布源。
    fn new(directory: PathBuf) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind synthetic feed");
        let origin = format!("http://{}", listener.local_addr().expect("address"));
        listener.set_nonblocking(true).expect("nonblocking feed");
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (requests, stopped) = (Arc::clone(&ranges), Arc::clone(&stop));
        let worker = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => respond(stream, &directory, &requests),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("local feed: {error}"),
                }
            }
        });
        Self {
            origin,
            ranges,
            stop,
            worker: Some(worker),
        }
    }

    /// Takes all previously received pack ranges so the next phase can be measured separately.
    /// 取出此前收到的所有文件包范围，以单独测量下一阶段。
    fn take_ranges(&self) -> Vec<Range> {
        std::mem::take(&mut *self.ranges.lock().expect("pack ranges"))
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.worker
            .take()
            .expect("feed thread")
            .join()
            .expect("stop feed");
    }
}

/// Serves one release-object request, supporting the HTTP Range header emitted by Downloader.
/// 应答一个发布对象请求，支持 Downloader 发出的 HTTP Range 请求头。
fn respond(mut stream: TcpStream, directory: &Path, requests: &Mutex<Vec<Range>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("read timeout");
    let mut reader = BufReader::new(&mut stream);
    let mut request = String::new();
    reader.read_line(&mut request).expect("HTTP request");
    let key = request
        .split_whitespace()
        .nth(1)
        .expect("HTTP path")
        .trim_start_matches('/');
    let mut range = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).expect("HTTP header") == 0 || header == "\r\n" {
            break;
        }
        if let Some(value) = header
            .trim()
            .to_ascii_lowercase()
            .strip_prefix("range: bytes=")
        {
            let (start, end) = value.split_once('-').expect("exact byte range");
            range = Some((
                start.parse::<usize>().expect("start"),
                end.parse::<usize>().expect("end"),
            ));
        }
    }
    let bytes = std::fs::read(directory.join(key));
    let (status, body, content_range) = match (&bytes, range) {
        (Ok(bytes), Some((start, end))) => {
            requests
                .lock()
                .expect("pack ranges")
                .push((key.to_owned(), start, end));
            (
                "206 Partial Content",
                &bytes[start..=end],
                format!("Content-Range: bytes {start}-{end}/{}\r\n", bytes.len()),
            )
        }
        (Ok(bytes), None) => ("200 OK", bytes.as_slice(), String::new()),
        (Err(_), _) => ("404 Not Found", &[][..], String::new()),
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\n{content_range}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

/// A synthetic release with the index and exact locations of each payload.
/// 包含索引及各载荷确切位置的合成版本。
struct Release {
    manifest: ReleaseManifest,
    index: ComponentIndex,
    locations: HashMap<Sha256Digest, FileLocation>,
}

/// Publishes signed objects and advances the stable channel pointer to `version`.
/// 发布签名对象，并将稳定通道指针推进至 `version`。
fn publish(directory: &Path, version: &str, files: &[(&str, &[u8])]) -> Release {
    let platform = Platform::current().expect("supported test platform");
    let mut packs = PackBuilder::new(directory, HashMap::new());
    for (_, content) in files {
        packs.add(content).expect("pack file");
    }
    let locations = packs.finish().expect("write compressed packs").locations;
    let name = ComponentName::new("core").expect("component");
    let index = ComponentIndex::new(
        name.clone(),
        files
            .iter()
            .map(|(path, content)| {
                let digest = Sha256Digest::of(content);
                IndexEntry::file(
                    RelativePath::new(*path).expect("relative path"),
                    0o755,
                    content.len() as u64,
                    digest,
                    locations[&digest],
                )
            })
            .collect(),
    )
    .expect("component index");
    let (digest, object) = index.to_object().expect("index object");
    write_object(directory, &index_key(digest), &object).expect("publish index");
    let manifest = ReleaseManifest {
        schema: SCHEMA_VERSION,
        product: PRODUCT.to_owned(),
        version: version.parse().expect("version"),
        platform,
        install_epoch: 1,
        released_at: "2026-11-15T00:00:00Z".to_owned(),
        notes: None,
        previous_version: None,
        downloads: Vec::new(),
        min_setup_version: None,
        inno_update: None,
        key_id: TEST_KEY_ID.to_owned(),
        components: BTreeMap::from([(
            name,
            ComponentRef {
                index: digest,
                size: index.installed_size(),
                fingerprint: None,
            },
        )]),
    };
    let bytes = manifest.to_document().expect("manifest bytes");
    let pointer = ChannelPointer {
        schema: SCHEMA_VERSION,
        product: PRODUCT.to_owned(),
        channel: Channel::Stable,
        platform,
        version: manifest.version.clone(),
        manifest: DocumentRef::of(&bytes),
        key_id: TEST_KEY_ID.to_owned(),
    };
    for (key, document) in [
        (manifest_key(&manifest.version, platform), bytes),
        (
            pointer_key(Channel::Stable, platform),
            pointer.to_document().expect("pointer bytes"),
        ),
    ] {
        write_object(directory, &key, &document).expect("publish document");
        write_object(
            directory,
            &signature_key(&key),
            &sign_detached(&document, &TEST_SECRET),
        )
        .expect("publish signature");
    }
    Release {
        manifest,
        index,
        locations,
    }
}

/// Captures the production updater's event stream and reboot request without stopping the test runner.
/// 捕获生产更新器的事件流和重启请求，避免停止测试进程。
#[derive(Clone, Default)]
struct Host {
    snapshots: Arc<Mutex<Vec<AppUpdateSnapshot>>>,
    restarts: Arc<Mutex<Vec<ServiceRestartingEvent>>>,
}

impl UpdateHost for Host {
    fn changed(&self, snapshot: &AppUpdateSnapshot) {
        self.snapshots
            .lock()
            .expect("snapshots")
            .push(snapshot.clone());
    }

    fn restart(&self, event: ServiceRestartingEvent, plan: RestartPlan) -> Result<(), String> {
        assert!(matches!(plan, RestartPlan::Service { executable } if executable.is_file()));
        self.restarts.lock().expect("restarts").push(event);
        Ok(())
    }
}

/// Starts from a real assembled installation with the specified download preference.
/// 从实际已组装的安装及指定下载偏好开始。
fn fixture(
    directory: &Path,
    source: &Source,
    auto_download: bool,
) -> (AppUpdater, Installed, Host) {
    let objects = directory.join("feed");
    let running = publish(
        &objects,
        "1.0.0",
        &[("core.exe", b"initial-core"), ("base.bin", b"from-running")],
    );
    let root = InstallRoot::new(directory.join("install"));
    {
        let lock = root.lock().expect("running installation lock");
        Assembly::new(&root, &lock, &running.manifest, vec![running.index.clone()])
            .expect("assembly")
            .prepare()
            .expect("prepare")
            .complete(&mut DirectoryPackSource::new(&objects))
            .expect("assemble running release");
        root.switch_to(&lock, &running.manifest.version)
            .expect("install running release");
    }
    let installed = Installed {
        root,
        version: running.manifest.version,
        platform: running.manifest.platform,
        install_epoch: 1,
        components: vec![running.index.component],
        executable: "core.exe".into(),
    };
    let host = Host::default();
    let updater = open(&installed, &source.origin, &host, auto_download);
    (updater, installed, host)
}

/// Recreates the updater state that is lost on process exit, retaining only the real install root.
/// 重建退出后会丢失的更新器状态，仅保留实际安装根目录。
fn open(installed: &Installed, origin: &str, host: &Host, auto_download: bool) -> AppUpdater {
    static KEYS: std::sync::OnceLock<[TrustedKey; 1]> = std::sync::OnceLock::new();
    let updater = AppUpdater {
        inner: Arc::new(Inner {
            keys: KEYS.get_or_init(|| {
                [TrustedKey {
                    key_id: TEST_KEY_ID,
                    public_key: public_key_of(&TEST_SECRET),
                }]
            }),
            origin: Some(origin.to_owned()),
            installed: Ok(installed.clone()),
            desktop_profile: None,
            user_agent: "signed-loopback-fixture".into(),
            host: Box::new(host.clone()),
            state: Mutex::new(State {
                settings: AppUpdateSettings {
                    auto_download,
                    ..AppUpdateSettings::default()
                },
                ..State::default()
            }),
        }),
    };
    updater.inner.finish_startup(installed).expect("startup");
    updater
}

/// Runs the exact worker used by the automatic check schedule.
/// 运行自动检查调度器使用的同一个工作入口。
fn check(updater: &AppUpdater) {
    updater.inner.run(
        AppUpdateActivity::Checking,
        AppUpdateOperation::Check,
        Inner::check,
    );
    assert!(
        updater.snapshot().error.is_none(),
        "{:?}",
        updater.snapshot().error
    );
}

/// Requires the installed version to remain current and unmarked until confirmation.
/// 要求确认前仍指向已安装版本，且未写入试运行标记。
fn assert_waiting(installed: &Installed, host: &Host) {
    assert_eq!(
        installed.root.current_version().expect("current"),
        Some(installed.version.clone())
    );
    assert!(
        installed
            .root
            .read_marker::<StartupTrial>(STARTUP_TRIAL_FILE)
            .expect("trial")
            .is_none()
    );
    assert!(host.restarts.lock().expect("restarts").is_empty());
}

/// Verifies signed consecutive downloads, offline restore, range reuse and the explicit commit point.
/// 验证连续签名下载、离线恢复、字节范围复用及明确的安装确认点。
#[test]
fn consecutive_releases_only_fetch_missing_ranges_and_wait_for_confirmation() {
    let directory = tempfile::tempdir().expect("private installation");
    let objects = directory.path().join("feed");
    let source = Source::new(objects.clone());
    let (updater, installed, host) = fixture(directory.path(), &source, true);
    let first = publish(
        &objects,
        "1.0.1",
        &[
            ("core.exe", b"first-core"),
            ("cached.bin", b"from-prepared"),
        ],
    );
    check(&updater);
    assert_eq!(updater.snapshot().ready_version.as_deref(), Some("1.0.1"));
    assert!(!source.take_ranges().is_empty());
    assert!(
        host.snapshots
            .lock()
            .expect("snapshots")
            .iter()
            .all(|event| {
                event.offer.is_none() || event.activity == AppUpdateActivity::Downloading
            })
    );
    assert_waiting(&installed, &host);

    drop(updater);
    let updater = open(&installed, &source.origin, &host, true);
    assert_eq!(updater.snapshot().ready_version.as_deref(), Some("1.0.1"));
    assert!(source.take_ranges().is_empty());
    assert_waiting(&installed, &host);
    updater.inner.lock().last_check_finished =
        Some(Instant::now() - CHECK_INTERVAL + Duration::from_secs(5));
    assert!(!updater.inner.wants_automatic_check(Instant::now()));
    updater.inner.lock().last_check_finished = Some(Instant::now() - CHECK_INTERVAL);
    assert!(updater.inner.wants_automatic_check(Instant::now()));

    let second = publish(
        &objects,
        "1.0.2",
        &[
            ("core.exe", b"second-core"),
            ("moved.bin", b"from-prepared"),
            ("extra.bin", b"second-extra"),
            ("base.bin", b"from-running"),
        ],
    );
    check(&updater);
    let mut actual = source.take_ranges();
    actual.sort_unstable();
    let mut expected = [b"second-core".as_slice(), b"second-extra".as_slice()].map(|bytes| {
        let location = second.locations[&Sha256Digest::of(bytes)];
        (
            pack_key(location.pack),
            location.offset as usize,
            (location.offset + location.length - 1) as usize,
        )
    });
    expected.sort_unstable();
    assert_eq!(actual, expected);
    let compressed_bytes = actual
        .iter()
        .map(|(_, start, end)| end - start + 1)
        .sum::<usize>();
    eprintln!(
        "Second release fetched {} missing frames ({compressed_bytes} compressed bytes).",
        actual.len()
    );
    assert_eq!(
        std::fs::read(
            installed
                .root
                .version_dir(&second.manifest.version)
                .join("moved.bin")
        )
        .expect("reused prepared file"),
        b"from-prepared"
    );
    assert_eq!(
        std::fs::read(
            installed
                .root
                .version_dir(&second.manifest.version)
                .join("base.bin")
        )
        .expect("reused running file"),
        b"from-running"
    );
    assert!(!installed.root.version_dir(&first.manifest.version).exists());
    assert_eq!(updater.snapshot().ready_version.as_deref(), Some("1.0.2"));
    assert_waiting(&installed, &host);

    // The feed goes offline before reopening; a click still applies the verified local version.
    // 重开之前发布源离线；点击依然能应用已验证的本地版本。
    let offline_origin = source.origin.clone();
    drop(updater);
    drop(source);
    let updater = open(&installed, &offline_origin, &host, true);
    assert_eq!(updater.snapshot().ready_version.as_deref(), Some("1.0.2"));
    assert_waiting(&installed, &host);
    assert!(updater.restart(Some("1.0.1")).is_err());
    assert_waiting(&installed, &host);
    updater
        .restart(Some("1.0.2"))
        .expect("clicked update and restart");
    assert_eq!(
        installed.root.current_version().expect("current"),
        Some(second.manifest.version)
    );
    assert_eq!(host.restarts.lock().expect("restarts").len(), 1);
}

/// Verifies that disabling automatic downloads keeps the manual offer, download and restart flow.
/// 验证关闭自动下载后保留手动提示、下载和重启流程。
#[test]
fn disabled_automatic_download_leaves_the_offer_for_manual_action() {
    let directory = tempfile::tempdir().expect("private installation");
    let objects = directory.path().join("feed");
    let source = Source::new(objects.clone());
    let (updater, installed, host) = fixture(directory.path(), &source, true);
    updater.set_settings(AppUpdateSettings {
        auto_download: false,
        ..AppUpdateSettings::default()
    });
    publish(&objects, "1.0.1", &[("core.exe", b"manual update")]);
    check(&updater);
    assert!(source.take_ranges().is_empty());
    assert!(updater.snapshot().offer.is_some());
    assert!(updater.snapshot().ready_version.is_none());
    assert_waiting(&installed, &host);
    updater.download().expect("clicked manual download");
    let deadline = Instant::now() + Duration::from_secs(5);
    while updater.snapshot().activity != AppUpdateActivity::Idle {
        assert!(
            Instant::now() < deadline,
            "manual download did not complete"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!source.take_ranges().is_empty());
    assert_eq!(updater.snapshot().ready_version.as_deref(), Some("1.0.1"));
    assert_waiting(&installed, &host);
}
