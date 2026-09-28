//! Production-path local GitHub installation acceptance tests.
//! 生产路径本地 GitHub 安装验收测试。

use super::*;
use sha2::{Digest, Sha256};
use std::io::{Cursor, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// Skill identity used by every local release fixture.
/// 每个本地 release 夹具使用的技能身份。
const FIXTURE_SKILL_ID: &str = "fixture-skill";

/// GitHub repository locator whose final segment matches the skill identity.
/// 最后一段与技能身份匹配的 GitHub 仓库定位值。
const FIXTURE_REPOSITORY: &str = "fixture-owner/fixture-skill";

/// Temporary directory removed recursively after one acceptance test.
/// 单次验收测试结束后递归删除的临时目录。
struct TestRoot {
    /// Exact isolated filesystem root owned by the fixture.
    /// 夹具拥有的精确隔离文件系统根。
    path: PathBuf,
}

impl TestRoot {
    /// Creates one unique local runtime root.
    /// 创建一个唯一的本地运行时根。
    ///
    /// # Returns
    /// 返回值
    /// Returns a newly created directory owned by the fixture.
    /// 返回一个由夹具拥有的新建目录。
    fn new() -> Self {
        static SEQUENCE: AtomicUsize = AtomicUsize::new(1);
        let path = std::env::temp_dir().join(format!(
            "vulcan-luaskills-local-github-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create isolated LuaSkills root");
        Self { path }
    }
}

impl Drop for TestRoot {
    /// Removes the exact fixture-owned tree.
    /// 删除夹具拥有的精确目录树。
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One deterministic release served by the loopback GitHub fixture.
/// 回环 GitHub 夹具提供的单个确定性 release。
struct ReleaseFixture {
    /// Normalized semantic version without the tag prefix.
    /// 不含 tag 前缀的规范化语义版本。
    version: &'static str,
    /// Complete skill archive bytes.
    /// 完整技能归档字节。
    archive: Vec<u8>,
    /// Lowercase SHA-256 digest of the archive.
    /// 归档的小写 SHA-256 摘要。
    sha256: String,
    /// Exact HTTP delivery behavior used for the archive asset.
    /// 归档资产使用的精确 HTTP 投递行为。
    delivery: ArchiveDelivery,
}

/// Deterministic archive delivery behavior exposed by one local release.
/// 单个本地 release 暴露的确定性归档投递行为。
#[derive(Clone, Copy)]
enum ArchiveDelivery {
    /// Deliver the complete archive with an exact content length.
    /// 使用精确正文长度投递完整归档。
    Complete,
    /// Close after the archive bytes while advertising additional bytes.
    /// 声明额外字节，但在归档字节结束后关闭连接。
    Truncated,
}

impl ReleaseFixture {
    /// Builds one release whose Lua result identifies its version.
    /// 构建一个 Lua 返回值可标识版本的 release。
    ///
    /// # Parameters
    /// 参数
    /// `version` is the exact skill manifest version.
    /// `version` 是技能清单中的精确版本。
    ///
    /// # Returns
    /// 返回值
    /// Returns a complete archive plus its checksum.
    /// 返回完整归档及其校验和。
    fn valid(version: &'static str) -> Self {
        let archive = build_skill_archive(version);
        let sha256 = sha256_hex(&archive);
        Self {
            version,
            archive,
            sha256,
            delivery: ArchiveDelivery::Complete,
        }
    }

    /// Builds one valid archive whose advertised digest can never match.
    /// 构建一个有效归档，但其声明摘要永远无法匹配。
    ///
    /// # Parameters
    /// 参数
    /// `version` is the advertised release version.
    /// `version` 是对外声明的 release 版本。
    ///
    /// # Returns
    /// 返回值
    /// Returns a release that reaches checksum verification and fails there.
    /// 返回一个能够到达校验和验证并在该处失败的 release。
    fn checksum_mismatch(version: &'static str) -> Self {
        let archive = build_skill_archive(version);
        Self {
            version,
            archive,
            sha256: "0".repeat(64),
            delivery: ArchiveDelivery::Complete,
        }
    }

    /// Builds one valid archive delivered through an intentionally truncated response.
    /// 构建一个通过故意截断的响应投递的有效归档。
    ///
    /// # Parameters
    /// 参数
    /// `version` is the advertised release version.
    /// `version` 是对外声明的 release 版本。
    ///
    /// # Returns
    /// 返回值
    /// Returns a release that fails while streaming the asset.
    /// 返回一个在流式读取资产时失败的 release。
    fn interrupted(version: &'static str) -> Self {
        let archive = build_skill_archive(version);
        let sha256 = sha256_hex(&archive);
        Self {
            version,
            archive,
            sha256,
            delivery: ArchiveDelivery::Truncated,
        }
    }

    /// Builds one checksum-valid payload that is not a ZIP archive.
    /// 构建一个校验和有效但不是 ZIP 归档的载荷。
    ///
    /// # Parameters
    /// 参数
    /// `version` is the advertised release version.
    /// `version` 是对外声明的 release 版本。
    ///
    /// # Returns
    /// 返回值
    /// Returns a release that reaches extraction and fails there.
    /// 返回一个能够到达解压阶段并在该处失败的 release。
    fn invalid_archive(version: &'static str) -> Self {
        let archive = b"this is intentionally not a zip archive".to_vec();
        let sha256 = sha256_hex(&archive);
        Self {
            version,
            archive,
            sha256,
            delivery: ArchiveDelivery::Complete,
        }
    }

    /// Builds one ZIP that stages successfully but cannot reload as Lua.
    /// 构建一个可成功暂存但无法作为 Lua 重载的 ZIP。
    ///
    /// # Parameters
    /// 参数
    /// `version` is the advertised release version.
    /// `version` 是对外声明的 release 版本。
    ///
    /// # Returns
    /// 返回值
    /// Returns a release that exercises staged-update rollback after reload failure.
    /// 返回一个用于验证重载失败后暂存更新回滚的 release。
    fn invalid_lua(version: &'static str) -> Self {
        let archive = build_skill_archive_with_lua(version, "return function(_");
        let sha256 = sha256_hex(&archive);
        Self {
            version,
            archive,
            sha256,
            delivery: ArchiveDelivery::Complete,
        }
    }

    /// Returns the managed skill archive asset name.
    /// 返回受管技能归档资产名。
    fn archive_name(&self) -> String {
        format!("{FIXTURE_SKILL_ID}-v{}-skill.zip", self.version)
    }

    /// Returns the matching checksum manifest asset name.
    /// 返回匹配的校验清单资产名。
    fn checksum_name(&self) -> String {
        format!("{FIXTURE_SKILL_ID}-v{}-checksums.txt", self.version)
    }
}

/// Renders one SHA-256 digest as lowercase hexadecimal text.
/// 把一个 SHA-256 摘要渲染为小写十六进制文本。
///
/// # Parameters
/// 参数
/// `bytes` is the exact payload covered by the digest.
/// `bytes` 是摘要覆盖的精确载荷。
///
/// # Returns
/// 返回值
/// Returns the 64-character checksum form consumed by LuaSkills.
/// 返回 LuaSkills 消费的 64 字符校验和形式。
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Finite loopback server implementing the GitHub endpoints used by production code.
/// 实现生产代码所用 GitHub 端点的有限回环服务器。
struct LocalGithubServer {
    /// Base URL used for both GitHub site and API overrides.
    /// 同时用于 GitHub 站点与 API 覆盖的基址。
    base_url: String,
    /// Current release selected by the `latest` endpoint.
    /// `latest` 端点当前选择的 release。
    latest_index: Arc<AtomicUsize>,
    /// Ordered request paths observed by the server.
    /// 服务器观察到的有序请求路径。
    requests: Arc<Mutex<Vec<String>>>,
    /// Cooperative server shutdown flag.
    /// 服务器协作关闭标志。
    shutdown: Arc<AtomicBool>,
    /// Background accept loop joined during fixture drop.
    /// 夹具丢弃时等待的后台接受循环。
    handle: Option<JoinHandle<()>>,
}

impl LocalGithubServer {
    /// Starts a loopback GitHub API and asset server.
    /// 启动回环 GitHub API 与资产服务器。
    ///
    /// # Returns
    /// 返回值
    /// Returns a running server with two valid releases and one invalid update.
    /// 返回一个含两个有效 release 与一个无效更新的运行中服务器。
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local GitHub server");
        listener
            .set_nonblocking(true)
            .expect("set local GitHub listener nonblocking");
        let address = listener.local_addr().expect("read local GitHub address");
        let base_url = format!("http://{address}");
        let latest_index = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_latest_index = Arc::clone(&latest_index);
        let thread_requests = Arc::clone(&requests);
        let thread_shutdown = Arc::clone(&shutdown);
        let releases = vec![
            ReleaseFixture::valid("1.0.0"),
            ReleaseFixture::valid("2.0.0"),
            ReleaseFixture::invalid_archive("3.0.0"),
            ReleaseFixture::checksum_mismatch("4.0.0"),
            ReleaseFixture::interrupted("5.0.0"),
            ReleaseFixture::invalid_lua("6.0.0"),
        ];
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            while !thread_shutdown.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Windows may propagate the listener's nonblocking mode to an accepted
                        // socket, while this fixture intentionally serves one request at a time.
                        // Windows 可能把监听器的非阻塞模式传播给已接受套接字，而本夹具刻意逐个
                        // 请求提供服务。
                        stream
                            .set_nonblocking(false)
                            .expect("set accepted local GitHub stream blocking");
                        let path = read_request_path(&mut stream);
                        thread_requests
                            .lock()
                            .expect("record local GitHub request")
                            .push(path.clone());
                        let response = response_for_path(
                            &path,
                            &releases,
                            thread_latest_index.load(Ordering::Acquire),
                        );
                        stream
                            .write_all(&response)
                            .expect("write local GitHub response");
                        stream.flush().expect("flush local GitHub response");
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "local GitHub server exceeded its bounded lifetime"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept local GitHub request: {error}"),
                }
            }
        });
        Self {
            base_url,
            latest_index,
            requests,
            shutdown,
            handle: Some(handle),
        }
    }

    /// Selects which release the `latest` endpoint advertises.
    /// 选择 `latest` 端点对外声明的 release。
    ///
    /// # Parameters
    /// 参数
    /// `index` addresses the fixed release fixture list.
    /// `index` 指向固定 release 夹具列表。
    fn select_latest(&self, index: usize) {
        self.latest_index.store(index, Ordering::Release);
    }

    /// Counts requests whose path ends with one exact suffix.
    /// 统计路径以某个精确后缀结尾的请求数。
    ///
    /// # Parameters
    /// 参数
    /// `suffix` identifies one release asset.
    /// `suffix` 标识单个 release 资产。
    ///
    /// # Returns
    /// 返回值
    /// Returns the observed matching request count.
    /// 返回观察到的匹配请求数量。
    fn request_count_ending_with(&self, suffix: &str) -> usize {
        self.requests
            .lock()
            .expect("read local GitHub requests")
            .iter()
            .filter(|path| path.ends_with(suffix))
            .count()
    }

    /// Returns the total number of observed requests.
    /// 返回观察到的请求总数。
    fn request_count(&self) -> usize {
        self.requests
            .lock()
            .expect("read local GitHub requests")
            .len()
    }
}

impl Drop for LocalGithubServer {
    /// Stops and joins the loopback server.
    /// 停止并等待回环服务器。
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let join_result = handle.join();
            // Preserve a server-thread panic as a normal test failure, but never panic again
            // from Drop while another assertion is already unwinding.
            // 保留服务器线程 panic 作为普通测试失败，但另一个断言已在展开时绝不从 Drop
            // 再次 panic。
            if !thread::panicking() {
                join_result.expect("join local GitHub server");
            }
        }
    }
}

/// Reads the request path from one small HTTP request.
/// 从一个小型 HTTP 请求中读取请求路径。
///
/// # Parameters
/// 参数
/// `stream` is the accepted loopback connection.
/// `stream` 是已接受的回环连接。
///
/// # Returns
/// 返回值
/// Returns the exact origin-form request target.
/// 返回精确的 origin-form 请求目标。
fn read_request_path(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set local GitHub read timeout");
    let mut buffer = [0_u8; 8192];
    let count = stream.read(&mut buffer).expect("read local GitHub request");
    let request = std::str::from_utf8(&buffer[..count]).expect("HTTP request must be UTF-8");
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(String::from)
        .expect("HTTP request line must contain a path")
}

/// Builds one complete connection-closing HTTP response.
/// 构造一个完整的关闭连接式 HTTP 响应。
///
/// # Parameters
/// 参数
/// `status` is the status line suffix, `content_type` describes the body, and `body` is exact bytes.
/// `status` 是状态行后缀，`content_type` 描述正文，`body` 是精确字节。
///
/// # Returns
/// 返回值
/// Returns complete HTTP/1.1 response bytes.
/// 返回完整 HTTP/1.1 响应字节。
fn http_response(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

/// Resolves one local GitHub request to its deterministic response.
/// 把一个本地 GitHub 请求解析为确定性响应。
///
/// # Parameters
/// 参数
/// `path` is the request target, `releases` is the immutable catalog, and `latest_index`
/// selects the current latest release.
/// `path` 是请求目标，`releases` 是不可变目录，`latest_index` 选择当前最新 release。
///
/// # Returns
/// 返回值
/// Returns a release API document, checksum, archive, or a 404 response.
/// 返回 release API 文档、校验和、归档或 404 响应。
fn response_for_path(path: &str, releases: &[ReleaseFixture], latest_index: usize) -> Vec<u8> {
    let release = if path.ends_with("/releases/latest") {
        releases.get(latest_index)
    } else {
        releases.iter().find(|release| {
            path.ends_with(&format!("/releases/tags/{}", release.version))
                || path.ends_with(&format!("/releases/tags/v{}", release.version))
        })
    };
    if let Some(release) = release {
        let archive_name = release.archive_name();
        let checksum_name = release.checksum_name();
        let body = serde_json::json!({
            "tag_name": format!("v{}", release.version),
            "assets": [
                {
                    "name": archive_name,
                    "browser_download_url": format!(
                        "https://github.com/{FIXTURE_REPOSITORY}/releases/download/v{}/{}",
                        release.version,
                        release.archive_name()
                    )
                },
                {
                    "name": checksum_name,
                    "browser_download_url": format!(
                        "https://github.com/{FIXTURE_REPOSITORY}/releases/download/v{}/{}",
                        release.version,
                        release.checksum_name()
                    )
                }
            ]
        })
        .to_string();
        return http_response("200 OK", "application/json", body.as_bytes());
    }
    for release in releases {
        if path.ends_with(&release.archive_name()) {
            return match release.delivery {
                ArchiveDelivery::Complete => {
                    http_response("200 OK", "application/zip", &release.archive)
                }
                ArchiveDelivery::Truncated => http_response_with_declared_length(
                    "200 OK",
                    "application/zip",
                    &release.archive,
                    release.archive.len() + 128,
                ),
            };
        }
        if path.ends_with(&release.checksum_name()) {
            let body = format!("{}  {}\n", release.sha256, release.archive_name());
            return http_response("200 OK", "text/plain", body.as_bytes());
        }
    }
    http_response("404 Not Found", "text/plain", b"not found")
}

/// Builds one connection-closing HTTP response with an explicit declared length.
/// 构造一个带显式声明长度的关闭连接式 HTTP 响应。
///
/// # Parameters
/// 参数
/// `status` and `content_type` describe the response, `body` contains emitted bytes, and
/// `declared_length` may intentionally differ for interruption tests.
/// `status` 与 `content_type` 描述响应，`body` 包含实际发送字节，`declared_length` 可在中断测试中故意不同。
///
/// # Returns
/// 返回值
/// Returns complete headers followed by the supplied body bytes.
/// 返回完整响应头及给定正文。
fn http_response_with_declared_length(
    status: &str,
    content_type: &str,
    body: &[u8],
    declared_length: usize,
) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Type: {content_type}\r\nContent-Length: {declared_length}\r\n\r\n"
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

/// Builds the exact managed skill ZIP shape accepted by LuaSkills.
/// 构建 LuaSkills 接受的精确受管技能 ZIP 形状。
///
/// # Parameters
/// 参数
/// `version` becomes both the manifest version and the Lua response text.
/// `version` 同时成为清单版本与 Lua 返回文本。
///
/// # Returns
/// 返回值
/// Returns a ZIP containing one top-level skill directory.
/// 返回一个包含单个顶层技能目录的 ZIP。
fn build_skill_archive(version: &str) -> Vec<u8> {
    build_skill_archive_with_lua(
        version,
        &format!("return function(_) return '{version}' end\n"),
    )
}

/// Builds one managed skill ZIP using exact Lua source supplied by the fixture.
/// 使用夹具给定的精确 Lua 源码构建一个受管技能 ZIP。
///
/// # Parameters
/// 参数
/// `version` becomes the manifest version and `lua_source` becomes the entry module bytes.
/// `version` 成为清单版本，`lua_source` 成为入口模块字节。
///
/// # Returns
/// 返回值
/// Returns a ZIP containing one top-level skill directory.
/// 返回一个包含单个顶层技能目录的 ZIP。
fn build_skill_archive_with_lua(version: &str, lua_source: &str) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer
        .start_file(format!("{FIXTURE_SKILL_ID}/skill.yaml"), options)
        .expect("start skill manifest entry");
    writer
        .write_all(
            format!(
                "name: Fixture Skill\nversion: {version}\nenable: true\ndebug: false\nentries:\n  - name: ping\n    description: Fixture ping.\n    lua_entry: runtime/ping.lua\n    lua_module: fixture_skill.ping\n"
            )
            .as_bytes(),
        )
        .expect("write skill manifest entry");
    writer
        .start_file(format!("{FIXTURE_SKILL_ID}/runtime/ping.lua"), options)
        .expect("start skill Lua entry");
    writer
        .write_all(lua_source.as_bytes())
        .expect("write skill Lua entry");
    writer.finish().expect("finish skill archive").into_inner()
}

/// Builds one isolated production handle using the supplied network policy.
/// 使用给定网络策略构建一个隔离的生产句柄。
///
/// # Parameters
/// 参数
/// `root` owns every runtime path, `server` supplies GitHub endpoints, and `allow_downloads`
/// selects the host network policy.
/// `root` 拥有所有运行时路径，`server` 提供 GitHub 端点，`allow_downloads` 选择宿主网络策略。
///
/// # Returns
/// 返回值
/// Returns a handle accepted by the production management functions.
/// 返回一个可供生产管理函数使用的句柄。
fn isolated_handle(
    root: &Path,
    server: &LocalGithubServer,
    allow_downloads: bool,
) -> TauriLuaSkillsHandle {
    let runtime_root = root.join("runtime");
    let managed_runtime_distribution_root = root.join("managed-distributions");
    let managed_runtime_environment_root = root.join("managed-environments");
    let output_config_directory = root.join("output-config");
    let skill_config_root = root.join("skill-config");
    let root_skills = root.join("root-skills");
    let user_skills = root.join("user-skills");
    for directory in [
        runtime_root.as_path(),
        managed_runtime_distribution_root.as_path(),
        managed_runtime_environment_root.as_path(),
        output_config_directory.as_path(),
        skill_config_root.as_path(),
        root_skills.as_path(),
        user_skills.as_path(),
    ] {
        std::fs::create_dir_all(directory).expect("create isolated handle directory");
    }
    let skill_roots = vec![
        RuntimeSkillRoot {
            name: String::from("ROOT"),
            skills_dir: root_skills,
        },
        RuntimeSkillRoot {
            name: String::from("USER"),
            skills_dir: user_skills,
        },
    ];
    let mut runtime_settings = LuaSkillsRuntimeSettings::default();
    runtime_settings.network.allow_downloads = allow_downloads;
    runtime_settings.network.github_base_url = Some(server.base_url.clone());
    runtime_settings.network.github_api_base_url = Some(server.base_url.clone());
    let host_options = build_host_options(
        &runtime_root,
        &managed_runtime_distribution_root,
        &skill_config_root,
        &runtime_settings,
    );
    let pool_config = LuaVmPoolConfig {
        min_size: 1,
        max_size: 2,
        idle_ttl_secs: 30,
    };
    // Use the same production ownership split in management fixtures.
    // 在管理夹具中使用与生产相同的所有权分离。
    let plugin_engine = Arc::new(
        LuaEngine::new(LuaEngineOptions::new(pool_config, host_options.clone()))
            .expect("create isolated plugin execution engine"),
    );
    let mut engine = LuaEngine::new(LuaEngineOptions::new(pool_config, host_options))
        .expect("create isolated LuaSkills engine");
    engine
        .load_from_roots(&skill_roots)
        .expect("load empty isolated skill roots");
    TauriLuaSkillsHandle {
        engine: Arc::new(RwLock::new(engine)),
        plugin_engine,
        runtime_root,
        managed_runtime_distribution_root,
        managed_runtime_environment_root,
        output_config_directory,
        skill_roots,
        managed_runtime_report: Arc::new(Mutex::new(ManagedRuntimeBootstrapReport::default())),
        managed_runtime_progress: Arc::new(Mutex::new(None)),
        config_capability_cache: Arc::new(Mutex::new(LuaSkillConfigCapabilityCache::default())),
        effective_runtime_settings: runtime_settings,
    }
}

/// Proves real typed plugin execution survives an ordinary-engine write lock and catalog reload.
/// 证明真实类型化插件执行不受普通引擎写锁及目录重载影响。
#[test]
fn plugin_execution_engine_is_independent_of_skill_management() {
    use luaskills::LuaInvocationContext;
    use luaskills::runtime::embedded::{
        CallControl, ModuleDefinition, ModuleExport, ModuleInvocation,
    };
    use std::sync::mpsc;
    // One budget governs initialization, each call, and bounded observation in this scenario.
    // 同一预算约束此场景的初始化、每次调用及有界观察。
    const WAIT: Duration = Duration::from_secs(5);
    // Reuse the real handle fixture, including production host-option construction.
    // 复用真实句柄夹具，包括生产宿主选项构造。
    let root = TestRoot::new();
    // The loopback server supplies only the fixture configuration; this test must not contact it.
    // 回环服务仅提供夹具配置；此测试不得向它发起请求。
    let server = LocalGithubServer::start();
    // Management and plugin execution must receive independent engine instances.
    // 管理与插件执行必须取得独立引擎实例。
    let handle = isolated_handle(&root.path, &server, false);
    // Authorize an immutable-generation layout outside the engine's legacy System trust root.
    // 授权位于引擎旧 System 信任根外的不可变代次布局。
    let package = root.path.join(
        "user-plugins/.vulcan-generations/packages/engine-isolation/physical-one/engine-isolation",
    );
    fs::create_dir_all(&package).expect("create host-authorized external package");
    fs::write(package.join("dependencies.yaml"), "{}\n").expect("write package dependencies");
    // The exact declaration and source are fixed before the ordinary engine is locked.
    // 精确声明及源码在普通引擎加锁前固定。
    let definition = ModuleDefinition {
        plugin_id: "engine-isolation".into(),
        generation: "retained-plugin-generation".into(),
        package_root: vulcan_runtime_assets::native_host_path(&package.canonicalize().expect("canonicalize package")).to_string_lossy().into_owned(),
        dependencies_file: "dependencies.yaml".into(),
        workspace_root: None,
        cwd: None,
        mounts: serde_json::json!({}),
        security_partition: "engine-isolation-fixture".into(),
        source: "-- Preserve state through ordinary skill reload.\n-- 在普通技能重载期间保留状态。\nlocal count = 0\n-- Return the next value without consulting mutable skill state.\n-- 返回下一个值，不读取可变技能状态。\nreturn {call=function() count=count+1; return count end}".into(),
        exports: vec![ModuleExport {
            name: "call".into(),
            input_schema: serde_json::json!(true),
            output_schema: serde_json::json!({"type":"integer"}),
        }],
    };
    // Hold the actual production management lock across both plugin execution and ordinary reload.
    // 在插件执行及普通重载期间持续持有真实生产管理锁。
    let mut management = handle
        .engine
        .write()
        .expect("hold ordinary management write lock");
    // Only the immutable plugin engine moves into the native worker.
    // 只有不可变插件引擎移动到原生工作线程。
    let plugin_engine = Arc::clone(&handle.plugin_engine);
    // The worker reports each real result, not an admission or scheduling acknowledgement.
    // 工作线程报告每个真实结果，而非入场或调度确认。
    let (completed, observed) = mpsc::channel();
    // Advance the second invocation only after the ordinary catalog has really reloaded.
    // 只有普通目录真实重载后才推进第二次调用。
    let (continue_call, continuation) = mpsc::channel();
    // The real Lua module remains in this thread across the management mutation.
    // 真实 Lua 模块在管理变更期间持续存在于此线程中。
    let worker = thread::spawn(move || {
        // Initial allocation also happens while the management write guard is held.
        // 初始分配同样发生在管理写守卫持有期间。
        let mut module = plugin_engine
            .create_embedded_module(
                definition,
                "engine-isolation-instance",
                Arc::new(CallControl::new(WAIT).expect("initialization budget")),
            )
            .expect("initialize independent module");
        for operation in ["before-management-reload", "after-management-reload"] {
            if operation == "after-management-reload" {
                continuation
                    .recv_timeout(WAIT)
                    .expect("wait for actual ordinary reload");
            }
            // Each real call has its own control budget and keeps the same module instance.
            // 每次真实调用有独立控制预算，并保留同一模块实例。
            let value = module
                .invoke(ModuleInvocation {
                    operation_id: operation,
                    session_id: None,
                    export: "call",
                    arguments: &serde_json::Value::Null,
                    context: &LuaInvocationContext::default(),
                    control: Arc::new(CallControl::new(WAIT).expect("call budget")),
                })
                .expect("execute while ordinary management is locked");
            completed.send(value).expect("publish actual plugin result");
        }
        module.close().expect("close actual plugin module");
    });
    assert_eq!(
        observed
            .recv_timeout(WAIT)
            .expect("first call cannot wait on skill management"),
        serde_json::json!(1)
    );
    management
        .reload_from_roots(&handle.skill_roots)
        .expect("reload ordinary skill catalog");
    continue_call
        .send(())
        .expect("allow call after real reload");
    assert_eq!(
        observed
            .recv_timeout(WAIT)
            .expect("same plugin state survives ordinary reload"),
        serde_json::json!(2)
    );
    worker
        .join()
        .expect("native module and its worker actually exit");
    drop(management);
    assert_eq!(
        server.request_count(),
        0,
        "engine ownership test must remain offline"
    );
}

/// Finds one effective skill version in a projected catalog.
/// 在已投影目录中查找一个生效技能版本。
///
/// # Parameters
/// 参数
/// `catalog` is the production projection and `skill_id` is the exact package identity.
/// `catalog` 是生产投影，`skill_id` 是精确技能包身份。
///
/// # Returns
/// 返回值
/// Returns the matching version or panics when the expected fixture is absent.
/// 返回匹配版本；预期夹具不存在时触发 panic。
fn catalog_version<'catalog>(
    catalog: &'catalog TauriLuaSkillsCatalog,
    skill_id: &str,
) -> &'catalog str {
    catalog
        .skills
        .iter()
        .find(|skill| skill.skill_id == skill_id)
        .map(|skill| skill.version.as_str())
        .expect("fixture skill must exist in projected catalog")
}

/// Verifies the production GitHub install and update pipeline against local HTTP.
/// 验证生产 GitHub 安装与更新管线能够通过本地 HTTP 完成。
///
/// Cold install, warm reinstall, successful update, extraction failure rollback, and disabled
/// network policy are asserted from final disk state rather than HTTP status alone.
/// 冷安装、热重装、成功更新、解压失败回滚与禁用网络策略均从最终磁盘状态断言，而不是只看 HTTP 状态。
#[test]
fn production_install_and_update_use_local_github_routes_safely() {
    let root = TestRoot::new();
    let server = LocalGithubServer::start();
    let handle = isolated_handle(&root.path.join("enabled"), &server, true);

    let (install_message, installed) = install_luaskill(
        &handle,
        Some(String::from(FIXTURE_SKILL_ID)),
        String::from(FIXTURE_REPOSITORY),
        String::from("github"),
    )
    .expect("cold local GitHub install");
    assert!(
        install_message.contains("was installed"),
        "{install_message}"
    );
    assert_eq!(catalog_version(&installed, FIXTURE_SKILL_ID), "1.0.0");
    let installed_lua = handle.skill_roots[1]
        .skills_dir
        .join(FIXTURE_SKILL_ID)
        .join("runtime")
        .join("ping.lua");
    assert!(
        std::fs::read_to_string(&installed_lua)
            .expect("read installed Lua entry")
            .contains("1.0.0")
    );

    server.select_latest(1);
    let (_, updated) =
        update_luaskill(&handle, String::from(FIXTURE_SKILL_ID)).expect("local GitHub update");
    assert_eq!(catalog_version(&updated, FIXTURE_SKILL_ID), "2.0.0");
    assert!(
        std::fs::read_to_string(&installed_lua)
            .expect("read updated Lua entry")
            .contains("2.0.0")
    );

    // A no-op upstream update does not reload; the host must still project the current catalog.
    // 上游无版本变化的更新不会重载，宿主仍须正确投影当前目录。
    let (_, unchanged) = update_luaskill(&handle, String::from(FIXTURE_SKILL_ID))
        .expect("updating the current version must preserve its catalog");
    assert_eq!(catalog_version(&unchanged, FIXTURE_SKILL_ID), "2.0.0");

    let (_, _) = uninstall_luaskill(&handle, String::from(FIXTURE_SKILL_ID))
        .expect("remove fixture before warm reinstall");
    let warm_asset_requests = server.request_count_ending_with("fixture-skill-v2.0.0-skill.zip");
    let (_, warm_catalog) = install_luaskill(
        &handle,
        Some(String::from(FIXTURE_SKILL_ID)),
        String::from(FIXTURE_REPOSITORY),
        String::from("github"),
    )
    .expect("warm local GitHub reinstall");
    assert_eq!(catalog_version(&warm_catalog, FIXTURE_SKILL_ID), "2.0.0");
    assert_eq!(
        server.request_count_ending_with("fixture-skill-v2.0.0-skill.zip"),
        warm_asset_requests,
        "warm reinstall must reuse the verified archive without another asset request"
    );

    server.select_latest(2);
    let error = update_luaskill(&handle, String::from(FIXTURE_SKILL_ID))
        .expect_err("invalid archive update must fail");
    assert!(error.contains("更新 LuaSkills 技能失败"), "{error}");
    let after_failure = load_luaskills_catalog(&handle).expect("catalog after failed update");
    assert_eq!(catalog_version(&after_failure, FIXTURE_SKILL_ID), "2.0.0");
    assert!(
        std::fs::read_to_string(&installed_lua)
            .expect("read Lua entry after failed update")
            .contains("2.0.0"),
        "a failed staged update must preserve the committed version"
    );

    server.select_latest(3);
    let checksum_error = update_luaskill(&handle, String::from(FIXTURE_SKILL_ID))
        .expect_err("checksum-mismatched update must fail");
    assert!(
        checksum_error.contains("checksum") || checksum_error.contains("SHA-256"),
        "{checksum_error}"
    );
    assert_eq!(
        catalog_version(
            &load_luaskills_catalog(&handle).expect("catalog after checksum failure"),
            FIXTURE_SKILL_ID,
        ),
        "2.0.0"
    );

    server.select_latest(4);
    let interruption_error = update_luaskill(&handle, String::from(FIXTURE_SKILL_ID))
        .expect_err("interrupted update must fail");
    assert!(
        interruption_error.contains("Failed to stream")
            || interruption_error.contains("request or response body error")
            || interruption_error.contains("end of file before message length reached"),
        "{interruption_error}"
    );
    assert_eq!(
        catalog_version(
            &load_luaskills_catalog(&handle).expect("catalog after interrupted update"),
            FIXTURE_SKILL_ID,
        ),
        "2.0.0"
    );

    server.select_latest(5);
    let reload_error = update_luaskill(&handle, String::from(FIXTURE_SKILL_ID))
        .expect_err("invalid Lua update must fail during runtime reload");
    assert!(
        reload_error.contains("Failed to reload LuaSkills after Update"),
        "{reload_error}"
    );
    assert_eq!(
        catalog_version(
            &load_luaskills_catalog(&handle).expect("catalog after reload rollback"),
            FIXTURE_SKILL_ID,
        ),
        "2.0.0"
    );
    assert!(
        std::fs::read_to_string(&installed_lua)
            .expect("read Lua entry after reload rollback")
            .contains("2.0.0"),
        "reload rollback must restore the previously committed Lua source"
    );

    server.select_latest(1);
    let v2_requests_before_recovery =
        server.request_count_ending_with("fixture-skill-v2.0.0-skill.zip");
    let (_, _) = uninstall_luaskill(&handle, String::from(FIXTURE_SKILL_ID))
        .expect("remove fixture before post-failure cache recovery");
    let (_, recovered_catalog) = install_luaskill(
        &handle,
        Some(String::from(FIXTURE_SKILL_ID)),
        String::from(FIXTURE_REPOSITORY),
        String::from("github"),
    )
    .expect("reinstall last verified release after failed updates");
    assert_eq!(
        catalog_version(&recovered_catalog, FIXTURE_SKILL_ID),
        "2.0.0"
    );
    assert_eq!(
        server.request_count_ending_with("fixture-skill-v2.0.0-skill.zip"),
        v2_requests_before_recovery,
        "failed updates must preserve the last verified archive cache"
    );

    let requests_before_disabled = server.request_count();
    let disabled = isolated_handle(&root.path.join("disabled"), &server, false);
    let disabled_error = install_luaskill(
        &disabled,
        Some(String::from(FIXTURE_SKILL_ID)),
        String::from(FIXTURE_REPOSITORY),
        String::from("github"),
    )
    .expect_err("disabled network policy must reject installation");
    assert!(
        disabled_error.contains("network download is disabled by host policy"),
        "{disabled_error}"
    );
    assert_eq!(
        server.request_count(),
        requests_before_disabled,
        "disabled network policy must fail before sending a request"
    );
    assert!(
        !disabled.skill_roots[1]
            .skills_dir
            .join(FIXTURE_SKILL_ID)
            .exists()
    );
}
