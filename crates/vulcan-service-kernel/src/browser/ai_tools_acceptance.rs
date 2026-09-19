//! Real Core browser, host grant, plugin configuration, and official Playwright MCP acceptance.
//! 真实 Core 浏览器、宿主授权、插件配置及官方 Playwright MCP 的验收。

use super::{BrowserHostRuntimeConfig, BrowserManager, BrowserProfileMode};
use crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
use base64::Engine as _;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use uuid::Uuid;
use vulcan_agent_runtime::mcp::{
    McpRuntimeAccess, McpRuntimeToolAllowlist, McpRuntimeToolBinding, McpServerConfig,
    McpServerTransportConfig, bootstrap_mcp_runtime_access, plugin_conversation_mcp_server_config,
};
use vulcan_plugin_runtime::VulcanPluginRuntimeManager;

/// Returns the repository checkout containing the current host build and bundled runtimes.
/// 返回包含当前宿主构建及随包运行时的仓库检出目录。
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

/// Launches a deterministic local page and returns its URL, shutdown flag, and server thread.
/// 启动确定性的本地网页并返回地址、关闭标记及服务线程。
fn start_page_fixture() -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind page fixture");
    listener
        .set_nonblocking(true)
        .expect("set nonblocking fixture");
    let url = format!(
        "http://{}/",
        listener.local_addr().expect("fixture address")
    );
    let stopping = Arc::new(AtomicBool::new(false));
    let thread_stopping = Arc::clone(&stopping);
    let worker = std::thread::spawn(move || {
        while !thread_stopping.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .expect("fixture read timeout");
                    let mut request = [0_u8; 2048];
                    let count = stream.read(&mut request).unwrap_or(0);
                    // The second owner uses a distinct URL and visible heading to prove that
                    // two simultaneous official MCP processes do not share a browser target.
                    // 第二个所有者使用不同 URL 和可见标题，证明两个并存的官方 MCP 进程不共用浏览器目标。
                    let heading = if request[..count].starts_with(b"GET /?stranger ") {
                        "Stranger browser fixture"
                    } else {
                        "Owner browser fixture"
                    };
                    let body = format!(
                        "<!doctype html><html><head><title>Vulcan W08</title></head><body><h1>{heading}</h1><button onclick=\"document.querySelector('#status').textContent='Clicked'\">Click here</button><p id=\"status\">Ready</p></body></html>"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("serve fixture page");
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("fixture accept failed: {error}"),
            }
        }
    });
    (url, stopping, worker)
}

/// A local page that accepts navigation but deliberately never completes its response.
/// 接受导航但刻意不完成响应的本地页面。
struct StallingPageFixture {
    /// URL targeted by an in-flight Playwright navigation.
    /// 在途 Playwright 导航指向的地址。
    url: String,
    /// Whether the server observed the browser's HTTP request.
    /// 服务端是否观察到浏览器的 HTTP 请求。
    seen: Arc<AtomicBool>,
    /// Stops the listener and any held connection during teardown.
    /// 清理时停止监听器及所持连接。
    stopping: Arc<AtomicBool>,
    /// Worker that owns the listening socket and hanging response.
    /// 持有监听套接字及悬挂响应的工作线程。
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for StallingPageFixture {
    /// Stops the deterministic fixture without leaving a child listener after a test failure.
    /// 测试失败时也停止确定性夹具，避免遗留监听器。
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Starts a page whose request stays in flight until the fixture is dropped.
/// 启动一个请求会保持在途状态直到夹具销毁的页面。
fn start_stalling_page_fixture() -> StallingPageFixture {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stalling fixture");
    listener
        .set_nonblocking(true)
        .expect("set stalling listener nonblocking");
    let url = format!(
        "http://{}/",
        listener.local_addr().expect("stalling fixture address")
    );
    let seen = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let worker_seen = Arc::clone(&seen);
    let worker_stopping = Arc::clone(&stopping);
    let worker = std::thread::spawn(move || {
        while !worker_stopping.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((_stream, _)) => {
                    // The unique fixture port proves Chromium began navigation at accept time;
                    // withholding all response bytes keeps the browser operation in flight.
                    // 独立夹具端口在接受连接时证明 Chromium 已开始导航；不回复任何字节使操作保持在途。
                    worker_seen.store(true, Ordering::Release);
                    while !worker_stopping.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("stalling fixture accept failed: {error}"),
            }
        }
    });
    StallingPageFixture {
        url,
        seen,
        stopping,
        worker: Some(worker),
    }
}

/// Selects one raw official MCP tool from the exact conversation partition.
/// 从精确会话分区中选取一个官方 MCP 原始工具。
fn tool(
    access: &McpRuntimeAccess,
    generation: u64,
    owner: Uuid,
    name: &str,
) -> McpRuntimeToolBinding {
    let allowlist = McpRuntimeToolAllowlist::new(
        Some(BTreeSet::from([String::from("vulcan-browser:playwright")])),
        Some(BTreeSet::from([String::from(name)])),
    );
    access
        .owned_stdio_tool_bindings_following_replacement(
            "vulcan-browser",
            generation,
            owner,
            &allowlist,
        )
        .expect("owner MCP bindings")
        .into_iter()
        .find(|binding| binding.origin_tool_name() == Some(name))
        .unwrap_or_else(|| panic!("official MCP omitted {name}"))
}

/// Calls an owner tool and rejects a protocol-level MCP tool error.
/// 调用会话工具并拒绝 MCP 协议层的工具错误。
async fn call(
    binding: &McpRuntimeToolBinding,
    owner: Uuid,
    args: serde_json::Value,
) -> serde_json::Value {
    let value = binding
        .invoke_for_owner(owner, args)
        .await
        .expect("MCP tool transport");
    assert_ne!(
        value.get("isError").and_then(serde_json::Value::as_bool),
        Some(true),
        "MCP tool failed: {value}"
    );
    value
}

/// Collects textual MCP content without depending on a transport-specific wrapper.
/// 汇集 MCP 文本内容，且不依赖特定传输包装。
fn text_content(value: &serde_json::Value) -> String {
    if let Some(text) = value.as_str() {
        return String::from(text);
    }
    value
        .get("content")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("MCP text or media content expected, received {value}"))
        .iter()
        .filter_map(|part| part.get("text").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Releases real browser hosts, MCP processes, and the fixture even if an assertion panics.
/// 即使断言异常，也会释放真实浏览器宿主、MCP 进程与测试网页。
struct AcceptanceCleanup {
    /// Core host ownership shared with the test body.
    /// 与测试主体共享的 Core 宿主所有权。
    manager: BrowserManager,
    /// Optional owner MCP access installed after plugin preparation.
    /// 插件准备完成后安装的可选会话 MCP 访问对象。
    access: Option<McpRuntimeAccess>,
    /// Signal stopping the local fixture thread.
    /// 停止本地网页线程的信号。
    stopping: Arc<AtomicBool>,
    /// Worker serving the local deterministic page.
    /// 提供本地确定性网页的工作线程。
    page_thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for AcceptanceCleanup {
    /// Shuts down child processes before deleting the isolated temporary root.
    /// 在删除隔离临时目录前关闭子进程。
    fn drop(&mut self) {
        if let Some(access) = self.access.as_ref() {
            access.shutdown();
        }
        let _ = self.manager.shutdown_hosts();
        self.stopping.store(true, Ordering::Release);
        if let Some(page_thread) = self.page_thread.take() {
            let _ = page_thread.join();
        }
    }
}

/// Confirms a retired one-use Core host endpoint cannot accept a fresh WebSocket upgrade.
/// 确认已退役的一次性 Core 宿主端点无法再次接受 WebSocket 升级。
fn assert_grant_refused(endpoint: &str) {
    let authority_and_path = endpoint
        .strip_prefix("ws://127.0.0.1:")
        .expect("loopback grant");
    let (port, path) = authority_and_path.split_once('/').expect("grant path");
    let Ok(mut socket) =
        TcpStream::connect(("127.0.0.1", port.parse::<u16>().expect("grant port")))
    else {
        return;
    };
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("grant response timeout");
    let request = format!(
        "GET /{path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    socket
        .write_all(request.as_bytes())
        .expect("request old grant");
    let mut response = [0_u8; 1024];
    let count = socket.read(&mut response).unwrap_or(0);
    assert!(
        !String::from_utf8_lossy(&response[..count]).starts_with("HTTP/1.1 101"),
        "old Core grant was accepted again"
    );
}

/// Probes the exact materialized launcher without logging its bearer endpoint.
/// 探测精确的已物化启动命令，同时不记录其承载式授权端点。
#[cfg(target_os = "windows")]
fn diagnose_mcp_launcher(config: &McpServerConfig, endpoint: &str) {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let McpServerTransportConfig::Stdio {
        command,
        args,
        cwd,
        env,
        inherit_environment,
        ..
    } = &config.transport
    else {
        panic!("browser plugin must use stdio");
    };
    eprintln!(
        "W08 launcher: command={command}, cwd={cwd:?}, inherit_environment={inherit_environment}, arg_count={}, arg_flags={:?}, env_keys={:?}",
        args.len(),
        args.iter()
            .filter(|arg| arg.starts_with("--"))
            .collect::<Vec<_>>(),
        env.keys().collect::<Vec<_>>(),
    );
    let environment_sets = [
        vec![],
        vec!["SystemRoot"],
        vec!["WINDIR"],
        vec!["TEMP"],
        vec!["TMP"],
        vec!["USERPROFILE"],
        vec!["SystemRoot", "WINDIR"],
        vec!["SystemRoot", "WINDIR", "TEMP", "TMP", "USERPROFILE"],
    ];
    for keys in environment_sets {
        let mut process = Command::new(command);
        process
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = cwd {
            process.current_dir(cwd);
        }
        process.env_clear();
        for key in &keys {
            if let Some(value) = std::env::var_os(key) {
                process.env(key, value);
            }
        }
        process.envs(env).creation_flags(0x08000000);
        let mut child = process.spawn().expect("probe official MCP launcher");
        child.stdin.take().expect("MCP stdin").write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"w08-diagnostic\",\"version\":\"1.0.0\"}}\n").expect("probe initialize");
        std::thread::sleep(Duration::from_millis(800));
        if child.try_wait().expect("probe child status").is_none() {
            let _ = child.kill();
        }
        let result = child.wait_with_output().expect("probe child output");
        eprintln!(
            "W08 launcher env_keys={keys:?} exit={:?} stdout={} stderr={}",
            result.status.code(),
            String::from_utf8_lossy(&result.stdout)
                .replace(endpoint, "[redacted]")
                .chars()
                .take(500)
                .collect::<String>(),
            String::from_utf8_lossy(&result.stderr)
                .replace(endpoint, "[redacted]")
                .chars()
                .take(500)
                .collect::<String>(),
        );
    }
    let mut process = Command::new(command);
    process
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        process.current_dir(cwd);
    }
    process.envs(env).creation_flags(0x08000000);
    let mut child = process
        .spawn()
        .expect("probe inherited official MCP launcher");
    child.stdin.take().expect("MCP stdin").write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"w08-diagnostic\",\"version\":\"1.0.0\"}}\n").expect("probe initialize");
    std::thread::sleep(Duration::from_millis(800));
    if child
        .try_wait()
        .expect("probe inherited child status")
        .is_none()
    {
        let _ = child.kill();
    }
    let result = child
        .wait_with_output()
        .expect("probe inherited child output");
    eprintln!(
        "W08 launcher inherit=true exit={:?} stdout={} stderr={}",
        result.status.code(),
        String::from_utf8_lossy(&result.stdout)
            .replace(endpoint, "[redacted]")
            .chars()
            .take(500)
            .collect::<String>(),
        String::from_utf8_lossy(&result.stderr)
            .replace(endpoint, "[redacted]")
            .chars()
            .take(500)
            .collect::<String>(),
    );
}

/// Validates the real Core-owned host and official plugin MCP across one browser switch.
/// 验证真实 Core 所属宿主及官方插件 MCP 在一次浏览器切换前后的行为。
/// This explicitly requires current desktop build artifacts and managed package distribution.
/// 此测试明确要求当前桌面构建产物及受管软件包发行目录。
#[tokio::test]
#[cfg(target_os = "windows")]
#[ignore = "requires the packaged Chromium, compiled host, and registry-installed Playwright MCP"]
async fn real_core_host_plugin_mcp_browser_switch_revokes_old_grant() {
    let repository = repository_root();
    // An explicit isolated package avoids relying on a stale shared output directory.
    // 显式隔离发行包避免依赖可能陈旧的共享 output 目录。
    let output = std::env::var_os("VULCAN_W08_ASSET_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository.join("output"));
    // The package layout, not an old Electron filename, owns the exact host and Chromium paths.
    // 由打包布局而非旧版 Electron 文件名决定宿主与 Chromium 的精确路径。
    let layout: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("runtime-layout.json")).expect("current runtime layout"),
    )
    .expect("decode current runtime layout");
    let package_path = |name: &str| {
        output.join(
            layout[name]
                .as_str()
                .unwrap_or_else(|| panic!("missing package path {name}")),
        )
    };
    let temp = tempfile::tempdir_in(repository.join("target")).expect("isolated test root");
    let manager = BrowserManager::new();
    manager
        .configure_host_runtime(BrowserHostRuntimeConfig {
            executable: package_path("desktopExecutable"),
            entry: package_path("browserHostEntry"),
            chromium_executable: package_path("chromiumExecutable"),
            profiles_root: temp.path().join("profiles"),
            staging_root: temp.path().join("staging"),
            discovery_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(30),
        })
        .expect("configure current browser host");
    let owner = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    let (page_url, stopping, page_thread) = start_page_fixture();
    let mut cleanup = AcceptanceCleanup {
        manager: manager.clone(),
        access: None,
        stopping,
        page_thread: Some(page_thread),
    };
    let plugins = VulcanPluginRuntimeManager::without_runtime(
        repository.join("plugins"),
        temp.path().join("user-plugins"),
        temp.path().join("bridge"),
        output.join("dependencies/runtimes"),
        temp.path().join("plugin-state"),
        temp.path().join("plugin-runtime"),
    );
    let enabled = BTreeSet::from([String::from("vulcan-browser")]);
    let catalog = plugins
        .refresh_catalog(&BTreeSet::new(), &enabled)
        .expect("enable plugin catalog");
    let generation = catalog
        .enabled_mcp_servers
        .get("vulcan-browser:playwright")
        .expect("enabled browser plugin server")
        .plugin_generation;
    let access = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("empty global MCP runtime");
    cleanup.access = Some(access.clone());
    // AI turn preparation starts the plugin before any short-lived host grant is issued.
    // AI 回合准备会在颁发任何短时宿主授权之前启动插件。
    let template = plugin_conversation_mcp_server_config(
        &plugins,
        "vulcan-browser",
        "playwright",
        &OsPluginMcpSecretStore,
        &owner.to_string(),
        "ws://127.0.0.1:1/devtools/browser/unavailable",
    )
    .expect("materialize discovery template and install plugin dependencies");
    access
        .open_owned_stdio_partition("vulcan-browser", generation, owner, vec![template])
        .await
        .expect("start official Playwright MCP discovery process");

    let first = manager
        .create_owned_session(&owner.to_string(), BrowserProfileMode::Ephemeral)
        .expect("create first owner browser");
    manager
        .start_host(&first.session_id)
        .expect("start first Core host");
    manager
        .select_ai_browser(&owner.to_string(), &first.session_id)
        .expect("select first owner browser");
    assert!(
        manager
            .list_owned_sessions(&stranger.to_string())
            .expect("foreign list")
            .is_empty()
    );
    let (_, first_lease) = manager
        .acquire_lease(
            &first.session_id,
            "w08-ai-owner",
            Some(Duration::from_secs(600)),
        )
        .expect("lease first host");
    let first_connection = Uuid::new_v4().to_string();
    let first_grant = manager
        .open_automation_gateway(
            &first.session_id,
            &owner.to_string(),
            &first_lease,
            &first_connection,
        )
        .expect("open Core grant");
    let first_config = plugin_conversation_mcp_server_config(
        &plugins,
        "vulcan-browser",
        "playwright",
        &OsPluginMcpSecretStore,
        &owner.to_string(),
        &first_grant.endpoint,
    )
    .expect("materialize the official plugin declaration");
    if std::env::var_os("VULCAN_W08_DIAGNOSE_LAUNCHER").is_some() {
        diagnose_mcp_launcher(&first_config, &first_grant.endpoint);
    }
    access
        .replace_owned_stdio_partition("vulcan-browser", generation, owner, vec![first_config])
        .await
        .expect("launch official Playwright MCP");
    let navigate = tool(&access, generation, owner, "browser_navigate");
    let snapshot = tool(&access, generation, owner, "browser_snapshot");
    let screenshot = tool(&access, generation, owner, "browser_take_screenshot");
    let click = tool(&access, generation, owner, "browser_click");
    assert!(
        navigate
            .invoke_for_owner(stranger, serde_json::json!({"url": &page_url}))
            .await
            .is_err()
    );
    // MCP discovery does not consume the 30-second one-use CDP grant; the first snapshot does.
    // MCP 工具发现不会消耗 30 秒的一次性 CDP 授权；首个快照调用会消耗它。
    call(&snapshot, owner, serde_json::json!({})).await;
    tokio::time::sleep(Duration::from_secs(31)).await;
    call(&navigate, owner, serde_json::json!({"url": &page_url})).await;
    let first_snapshot = text_content(&call(&snapshot, owner, serde_json::json!({})).await);
    let reference = first_snapshot
        .split("button \"Click here\" [ref=")
        .nth(1)
        .and_then(|tail| tail.split(']').next())
        .expect("official button ref");
    let image_result = call(&screenshot, owner, serde_json::json!({"type": "png"})).await;
    let image = image_result
        .get("content")
        .and_then(serde_json::Value::as_array)
        .expect("screenshot content")
        .iter()
        .find(|part| part.get("type").and_then(serde_json::Value::as_str) == Some("image"))
        .expect("official MCP screenshot image");
    let screenshot_data = image
        .get("data")
        .and_then(serde_json::Value::as_str)
        .expect("official MCP image data");
    let screenshot_bytes = base64::engine::general_purpose::STANDARD
        .decode(screenshot_data)
        .expect("standard Base64 screenshot");
    assert!(screenshot_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    call(
        &click,
        owner,
        serde_json::json!({"element": "Click here button", "target": reference}),
    )
    .await;
    assert!(text_content(&call(&snapshot, owner, serde_json::json!({})).await).contains("Clicked"));

    // A second real conversation receives a different Core host and official MCP partition.
    // 第二个真实会话取得不同的 Core 宿主与官方 MCP 分区。
    let stranger_browser = manager
        .create_owned_session(&stranger.to_string(), BrowserProfileMode::Ephemeral)
        .expect("create stranger browser");
    manager
        .start_host(&stranger_browser.session_id)
        .expect("start stranger host");
    assert_eq!(
        manager
            .select_ai_browser(&stranger.to_string(), &first.session_id)
            .expect_err("foreign browser selection must be denied"),
        manager
            .select_ai_browser(&stranger.to_string(), "nonexistent-browser")
            .expect_err("absent browser selection must be denied")
    );
    manager
        .select_ai_browser(&stranger.to_string(), &stranger_browser.session_id)
        .expect("select own stranger browser");
    let (_, stranger_lease) = manager
        .acquire_lease(
            &stranger_browser.session_id,
            "w08-stranger-owner",
            Some(Duration::from_secs(600)),
        )
        .expect("lease stranger host");
    let stranger_connection = Uuid::new_v4().to_string();
    assert_eq!(
        manager
            .open_automation_gateway(
                &first.session_id,
                &stranger.to_string(),
                &first_lease,
                &stranger_connection,
            )
            .expect_err("foreign owner must not open another host gateway"),
        super::BROWSER_ERROR_UNKNOWN_SESSION
    );
    let stranger_grant = manager
        .open_automation_gateway(
            &stranger_browser.session_id,
            &stranger.to_string(),
            &stranger_lease,
            &stranger_connection,
        )
        .expect("open stranger Core grant");
    let stranger_config = plugin_conversation_mcp_server_config(
        &plugins,
        "vulcan-browser",
        "playwright",
        &OsPluginMcpSecretStore,
        &stranger.to_string(),
        &stranger_grant.endpoint,
    )
    .expect("materialize stranger plugin declaration");
    access
        .open_owned_stdio_partition(
            "vulcan-browser",
            generation,
            stranger,
            vec![stranger_config],
        )
        .await
        .expect("launch stranger official Playwright MCP");
    let stranger_snapshot = tool(&access, generation, stranger, "browser_snapshot");
    let stranger_navigate = tool(&access, generation, stranger, "browser_navigate");
    call(&stranger_snapshot, stranger, serde_json::json!({})).await;
    call(
        &stranger_navigate,
        stranger,
        serde_json::json!({"url": format!("{page_url}?stranger")}),
    )
    .await;
    let stranger_page =
        text_content(&call(&stranger_snapshot, stranger, serde_json::json!({})).await);
    assert!(
        stranger_page.contains("Stranger browser fixture"),
        "{stranger_page}"
    );
    assert!(
        !stranger_page.contains("Owner browser fixture"),
        "{stranger_page}"
    );
    let owner_page = text_content(&call(&snapshot, owner, serde_json::json!({})).await);
    assert!(owner_page.contains("Owner browser fixture"), "{owner_page}");
    assert!(
        !owner_page.contains("Stranger browser fixture"),
        "{owner_page}"
    );
    assert!(
        stranger_navigate
            .invoke_for_owner(owner, serde_json::json!({"url": &page_url}))
            .await
            .is_err(),
        "an owner cannot invoke the other conversation's MCP binding"
    );

    let second = manager
        .create_owned_session(&owner.to_string(), BrowserProfileMode::Ephemeral)
        .expect("create second owner browser");
    manager
        .start_host(&second.session_id)
        .expect("start second Core host");
    manager
        .select_ai_browser(&owner.to_string(), &second.session_id)
        .expect("switch owner browser");
    let (_, second_lease) = manager
        .acquire_lease(
            &second.session_id,
            "w08-ai-owner",
            Some(Duration::from_secs(600)),
        )
        .expect("lease second host");
    let second_connection = Uuid::new_v4().to_string();
    let second_grant = manager
        .open_automation_gateway(
            &second.session_id,
            &owner.to_string(),
            &second_lease,
            &second_connection,
        )
        .expect("open second Core grant");
    let second_config = plugin_conversation_mcp_server_config(
        &plugins,
        "vulcan-browser",
        "playwright",
        &OsPluginMcpSecretStore,
        &owner.to_string(),
        &second_grant.endpoint,
    )
    .expect("materialize replacement plugin grant");
    access
        .replace_owned_stdio_partition("vulcan-browser", generation, owner, vec![second_config])
        .await
        .expect("replace owner MCP partition");
    call(
        &tool(&access, generation, owner, "browser_snapshot"),
        owner,
        serde_json::json!({}),
    )
    .await;
    // A locator from the old MCP process must not act on the replacement browser.
    // 旧 MCP 进程的节点引用不得作用于替换后的浏览器。
    let stale_error = click
        .invoke_for_owner(
            owner,
            serde_json::json!({"element": "old Click here button", "target": reference}),
        )
        .await
        .expect_err("old snapshot reference must be rejected by the replacement browser");
    assert!(
        stale_error.contains("Ref") && stale_error.contains("snapshot"),
        "old snapshot reference failed for an unexpected reason: {stale_error}"
    );
    manager
        .revoke_automation_gateway(&first.session_id, &owner.to_string(), &first_connection)
        .expect("revoke first host grant");
    manager
        .release_lease(&first.session_id, &first_lease)
        .expect("release first lease");
    assert_grant_refused(&first_grant.endpoint);
    assert!(
        navigate
            .invoke_for_owner(stranger, serde_json::json!({"url": &page_url}))
            .await
            .is_err()
    );
    // Model bindings deliberately route a same-owner next tool call to the new current partition.
    // 模型工具绑定会刻意把同一所有者的下一次调用路由到新的当前分区。
    call(&navigate, owner, serde_json::json!({"url": &page_url})).await;
    let current_navigate = tool(&access, generation, owner, "browser_navigate");
    assert!(
        text_content(
            &call(
                &tool(&access, generation, owner, "browser_snapshot"),
                owner,
                serde_json::json!({})
            )
            .await
        )
        .contains("Ready")
    );
    // A real navigation to an unanswered HTTP page remains in flight beyond the gateway.
    // 导航到不回复的真实 HTTP 页面会使操作在宿主网关之外保持在途状态。
    let stalling_page = start_stalling_page_fixture();
    let stalled_binding = tool(&access, generation, owner, "browser_navigate");
    let stalled_url = stalling_page.url.clone();
    let stalled_call = tokio::spawn(async move {
        stalled_binding
            .invoke_for_owner(owner, serde_json::json!({"url": stalled_url}))
            .await
    });
    let observed_stalling_request = tokio::time::timeout(Duration::from_secs(20), async {
        while !stalling_page.seen.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if observed_stalling_request.is_err() {
        if stalled_call.is_finished() {
            panic!(
                "navigation ended before the stalling page request: {:?}",
                stalled_call.await
            );
        }
        stalled_call.abort();
        panic!(
            "browser did not reach the stalling page before takeover: {:?}",
            manager.session_projection(&second.session_id)
        );
    }
    assert_eq!(
        manager
            .revoke_automation_gateway(&second.session_id, &owner.to_string(), &second_connection)
            .expect_err("an unconfirmed real Playwright operation must block revocation"),
        super::BROWSER_ERROR_LEASE_BLOCKED
    );
    assert_eq!(
        manager
            .session_projection(&second.session_id)
            .expect("blocked projection")
            .lease_state,
        super::BrowserLeaseState::Blocked
    );
    assert!(
        manager
            .takeover_lease(&second.session_id, "w08-new-controller", None)
            .is_err(),
        "an in-flight script must not produce a new control lease"
    );
    manager
        .close_uncertain_automation_session(
            &second.session_id,
            &owner.to_string(),
            &second_connection,
        )
        .expect("close the blocked Chromium context before retiring ownership");
    stalled_call.abort();
    let _ = stalled_call.await;
    access
        .shutdown_owned_stdio_partition("vulcan-browser", generation, owner)
        .expect("stop owner MCP process");
    assert_grant_refused(&second_grant.endpoint);
    assert!(
        current_navigate
            .invoke_for_owner(owner, serde_json::json!({"url": &page_url}))
            .await
            .is_err()
    );
    // Retiring one owner's blocked connection must not stop the other live owner's MCP process.
    // 退休一名所有者的阻塞连接，不得停止另一名活动所有者的 MCP 进程。
    let surviving_stranger =
        text_content(&call(&stranger_snapshot, stranger, serde_json::json!({})).await);
    assert!(surviving_stranger.contains("Stranger browser fixture"));
    access
        .shutdown_owned_stdio_partition("vulcan-browser", generation, stranger)
        .expect("stop stranger MCP process");
    manager
        .revoke_automation_gateway(
            &stranger_browser.session_id,
            &stranger.to_string(),
            &stranger_connection,
        )
        .expect("revoke stranger host grant");
    manager
        .release_lease(&stranger_browser.session_id, &stranger_lease)
        .expect("release stranger lease");
    assert_grant_refused(&stranger_grant.endpoint);
    manager
        .close_session(&stranger_browser.session_id)
        .expect("close stranger browser");
    manager
        .close_session(&first.session_id)
        .expect("close first browser");
    fs::metadata(temp.path()).expect("isolated root remained available through cleanup");
}
