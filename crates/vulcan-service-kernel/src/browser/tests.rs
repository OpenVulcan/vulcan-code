use super::*;
use crate::tests::test_support::test_service_state_with_parent_session;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
    thread,
};
use vulcan_agent_runtime::model::SessionId;

/// In-process browser host that speaks the documented control protocol.
/// 使用文档控制协议的进程内浏览器宿主。
///
/// It exists so host ownership can be tested deterministically without starting a desktop
/// runtime, while still exercising the real sockets, framing, and handshake code paths.
/// 它的存在使宿主归属可以在不启动桌面运行时的前提下被确定性测试，同时仍然走真实的套接字、
/// 帧编解码与握手代码路径。
struct FakeBrowserHost {
    /// Loopback address the fake host listens on.
    /// 伪宿主监听的地址。
    address: SocketAddr,
}

/// Spawner that publishes a fake host endpoint instead of starting a desktop runtime.
/// 发布伪宿主端点而不是启动桌面运行时的启动器。
struct FakeBrowserHostSpawner {
    /// Loopback address of the fake host.
    /// 伪宿主的回环地址。
    address: SocketAddr,
    /// Liveness flag shared with the fake host thread and the process handle.
    /// 与伪宿主线程及进程句柄共享的存活标志。
    alive: Arc<AtomicBool>,
    /// Whether process liveness inspection must fail.
    /// 进程存活检查是否必须失败。
    poll_error: Arc<AtomicBool>,
}

/// Process handle for a fake host that stays alive until it is killed.
/// 在未被终止前保持存活的伪宿主进程句柄。
struct FakeBrowserHostProcess {
    /// Whether the fake process is still running.
    /// 伪进程是否仍在运行。
    alive: Arc<AtomicBool>,
    /// Whether process liveness inspection must fail.
    /// 进程存活检查是否必须失败。
    poll_error: Arc<AtomicBool>,
    /// Operating-system process id reported to Core.
    /// 报告给 Core 的操作系统进程标识。
    pid: u32,
}

impl BrowserHostProcessHandle for FakeBrowserHostProcess {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn try_wait(&mut self) -> Result<Option<i32>, String> {
        if self.poll_error.swap(false, AtomicOrdering::SeqCst) {
            return Err(String::from("injected host process inspection failure"));
        }
        if self.alive.load(AtomicOrdering::SeqCst) {
            Ok(None)
        } else {
            Ok(Some(1))
        }
    }

    fn kill(&mut self) {
        self.alive.store(false, AtomicOrdering::SeqCst);
    }
}

impl FakeBrowserHost {
    /// Starts one fake host that answers requests after the configured handshake delay.
    /// 启动一个在配置的握手延迟后开始应答请求的伪宿主。
    ///
    /// The host accepts any number of sequential control connections so tests can start more
    /// than one session against it, exactly as separate real host processes would serve them.
    /// 该宿主接受任意数量的顺序控制连接，使测试可以针对它启动多个会话，就像多个真实宿主进程
    /// 分别服务它们一样。
    fn start(handshake_delay: Duration, alive: Arc<AtomicBool>) -> Self {
        Self::start_with_page_list_failure(handshake_delay, alive, false)
    }

    /// Starts a host that can reject the mandatory initial page enumeration.
    /// 启动一个可拒绝必需的初始页面枚举的宿主。
    fn start_with_page_list_failure(
        handshake_delay: Duration,
        alive: Arc<AtomicBool>,
        fail_page_list: bool,
    ) -> Self {
        Self::start_with_failures(handshake_delay, alive, fail_page_list, false)
    }

    /// Starts a fake host with precisely selected protocol failures.
    /// 启动一个带精确协议故障选择的伪宿主。
    /// `handshake_delay` delays the hello, `fail_page_list` refuses page enumeration, and
    /// `fail_input_release_after_first` refuses ownership cleanup after initial acquisition.
    /// `handshake_delay` 延迟 hello，`fail_page_list` 拒绝页面枚举，
    /// `fail_input_release_after_first` 在首次获取租约后拒绝归属清理。
    /// Returns the listener endpoint used by the production discovery path.
    /// 返回供生产发现路径使用的监听端点。
    fn start_with_failures(
        handshake_delay: Duration,
        alive: Arc<AtomicBool>,
        fail_page_list: bool,
        fail_input_release_after_first: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake host");
        let address = listener.local_addr().expect("fake host address");
        thread::spawn(move || {
            loop {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let alive = Arc::clone(&alive);
                thread::spawn(move || {
                    let mut decoder = BrowserHostFrameDecoder::new();
                    let mut buffer = [0_u8; 8192];
                    // The first frame is the owning Core's hello, which names the session and
                    // generation the host must answer for.
                    // 首帧是归属 Core 的 hello，其中指明宿主必须为其应答的会话与代际。
                    let mut hello = serde_json::Value::Null;
                    while hello.is_null() {
                        match stream.read(&mut buffer) {
                            Ok(0) | Err(_) => return,
                            Ok(read) => {
                                if let Ok(values) = decoder.push(&buffer[..read])
                                    && let Some(value) = values.into_iter().next()
                                {
                                    hello = value;
                                }
                            }
                        }
                    }
                    thread::sleep(handshake_delay);
                    let handshake = json!({
                        "protocolVersion": BROWSER_HOST_PROTOCOL_VERSION,
                        "sessionId": hello["sessionId"],
                        "hostGeneration": hello["hostGeneration"],
                        "pid": 4321,
                        "runtimeVersion": "0.1.0",
                        "capabilities": ["playwright-endpoint-on-demand", "page.create"],
                    });
                    let Ok(frame) = encode_host_frame(&handshake) else {
                        return;
                    };
                    if stream.write_all(&frame).is_err() {
                        return;
                    }
                    // Acquisition performs one defensive release before issuing a lease. A
                    // later refusal therefore exercises an ownership transition rather than
                    // preventing the initial controller from being established.
                    // 获取租约前会先执行一次防御性释放，因此后续拒绝检验的是真实归属交接，而不是
                    // 阻止初始控制者建立。
                    let mut input_release_count = 0_usize;
                    loop {
                        match stream.read(&mut buffer) {
                            // Core closed the channel, which is how a host learns to exit.
                            // Core 关闭通道，宿主由此得知应当退出。
                            Ok(0) | Err(_) => {
                                alive.store(false, AtomicOrdering::SeqCst);
                                return;
                            }
                            Ok(read) => {
                                let Ok(values) = decoder.push(&buffer[..read]) else {
                                    return;
                                };
                                for value in values {
                                    let request_id =
                                        value["requestId"].as_str().unwrap_or_default().to_string();
                                    if fail_page_list
                                        && value["operation"].as_str() == Some("page.list")
                                    {
                                        let response = json!({
                                            "requestId": request_id,
                                            "ok": false,
                                            "error": {
                                                "code": "fixture_page_list_failure",
                                                "message": "initial page list failed",
                                            },
                                        });
                                        let Ok(frame) = encode_host_frame(&response) else {
                                            return;
                                        };
                                        if stream.write_all(&frame).is_err() {
                                            return;
                                        }
                                        continue;
                                    }
                                    if value["operation"].as_str() == Some("input.releaseAll") {
                                        input_release_count += 1;
                                        if fail_input_release_after_first && input_release_count > 1
                                        {
                                            let response = json!({
                                                "requestId": request_id,
                                                "ok": false,
                                                "error": {
                                                    "code": "fixture_input_release_failure",
                                                    "message": "pressed inputs were not released",
                                                },
                                            });
                                            let Ok(frame) = encode_host_frame(&response) else {
                                                return;
                                            };
                                            if stream.write_all(&frame).is_err() {
                                                return;
                                            }
                                            continue;
                                        }
                                    }
                                    let result = match value["operation"].as_str() {
                                        Some("page.list") => json!({ "pages": [] }),
                                        Some("page.navigate") => json!({
                                            "targetId": value["params"]["targetId"],
                                            "url": value["params"]["url"],
                                        }),
                                        Some("viewport.set") => json!({
                                            "width": value["params"]["width"],
                                            "height": value["params"]["height"],
                                        }),
                                        Some("input.dispatch") => json!({
                                            "targetId": value["params"]["targetId"],
                                        }),
                                        Some("host.closeContext") => json!({ "closed": true }),
                                        Some("host.shutdown") => {
                                            alive.store(false, AtomicOrdering::SeqCst);
                                            json!({ "acknowledged": true })
                                        }
                                        _ => json!({ "operation": value["operation"] }),
                                    };
                                    let response = json!({
                                        "requestId": request_id,
                                        "ok": true,
                                        "result": result,
                                    });
                                    let Ok(frame) = encode_host_frame(&response) else {
                                        return;
                                    };
                                    if stream.write_all(&frame).is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                });
            }
        });
        Self { address }
    }
}

impl BrowserHostSpawner for FakeBrowserHostSpawner {
    fn spawn(
        &self,
        plan: &BrowserHostLaunchPlan,
    ) -> Result<Box<dyn BrowserHostProcessHandle>, String> {
        // The real host publishes this document itself; the fake publishes it on spawn so the
        // production discovery-wait path is exercised unchanged.
        // 真实宿主自行发布该文档；伪宿主在启动时发布它，使生产发现等待路径被原样验证。
        std::fs::create_dir_all(&plan.user_data_directory)
            .map_err(|error| format!("fake host profile: {error}"))?;
        let document = json!({
            "schemaVersion": BROWSER_HOST_CONTROL_DISCOVERY_SCHEMA_VERSION,
            "protocolVersion": BROWSER_HOST_PROTOCOL_VERSION,
            "controlHost": self.address.ip().to_string(),
            "controlPort": self.address.port(),
            "pid": 4321,
            "sessionId": plan.session_id,
            "hostGeneration": plan.host_generation,
        });
        std::fs::write(
            plan.control_discovery_path(),
            serde_json::to_string(&document).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("fake host discovery: {error}"))?;
        Ok(Box::new(FakeBrowserHostProcess {
            alive: Arc::clone(&self.alive),
            poll_error: Arc::clone(&self.poll_error),
            pid: 4321,
        }))
    }
}

/// Builds one manager wired to a fake host with an isolated profile root.
/// 构建一个连接到伪宿主且使用隔离画像根的管理器。
fn manager_with_fake_host(handshake_delay: Duration) -> (BrowserManager, Arc<AtomicBool>) {
    manager_with_fake_host_options(handshake_delay, false)
}

/// Builds a fake-host manager with an optional initial page-list refusal.
/// 构建一个可选择拒绝初始页面枚举的伪宿主管理器。
fn manager_with_fake_host_options(
    handshake_delay: Duration,
    fail_page_list: bool,
) -> (BrowserManager, Arc<AtomicBool>) {
    let (manager, alive, _) = manager_with_fake_host_poll(handshake_delay, fail_page_list);
    (manager, alive)
}

/// Builds a fake-host manager with an observable process-inspection failure switch.
/// 构建带可观察进程检查失败开关的伪宿主管理器。
fn manager_with_fake_host_poll(
    handshake_delay: Duration,
    fail_page_list: bool,
) -> (BrowserManager, Arc<AtomicBool>, Arc<AtomicBool>) {
    manager_with_fake_host_behavior(handshake_delay, fail_page_list, false)
}

/// Builds a fake-host manager with precise page-list and input-release failures.
/// 构建一个带精确页面清单及输入释放故障的伪宿主管理器。
/// The two failure flags map directly to the fake host operations; the returned liveness and
/// poll-error controls expose process supervision state to each test.
/// 两个故障标志直接对应伪宿主操作；返回的存活与检查错误控制量向测试暴露进程监督状态。
fn manager_with_fake_host_behavior(
    handshake_delay: Duration,
    fail_page_list: bool,
    fail_input_release_after_first: bool,
) -> (BrowserManager, Arc<AtomicBool>, Arc<AtomicBool>) {
    let alive = Arc::new(AtomicBool::new(true));
    let poll_error = Arc::new(AtomicBool::new(false));
    let host = FakeBrowserHost::start_with_failures(
        handshake_delay,
        Arc::clone(&alive),
        fail_page_list,
        fail_input_release_after_first,
    );
    let profiles_root =
        std::env::temp_dir().join(format!("vulcan-w06-profiles-{}", Uuid::new_v4()));
    let manager = BrowserManager::with_host_spawner(Arc::new(FakeBrowserHostSpawner {
        address: host.address,
        alive: Arc::clone(&alive),
        poll_error: Arc::clone(&poll_error),
    }));
    manager
        .configure_host_runtime(BrowserHostRuntimeConfig {
            executable: std::path::PathBuf::from("fake-electron.exe"),
            entry: std::path::PathBuf::from("fake-host.js"),
            chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
            profiles_root: profiles_root.clone(),
            staging_root: profiles_root.join("staging"),
            discovery_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(5),
        })
        .expect("configure host runtime");
    (manager, alive, poll_error)
}

/// Verifies a refused host input release blocks lease handoff without minting a new owner.
/// 验证宿主拒绝释放输入时会阻止租约交接，且不会签发新的所有者。
#[test]
fn refused_host_input_release_blocks_lease_handoff() {
    let (manager, _alive, _poll_error) =
        manager_with_fake_host_behavior(Duration::ZERO, false, true);
    let session = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
        .expect("create browser");
    manager.start_host(&session.session_id).expect("start host");
    let (_, lease_secret) = manager
        .acquire_lease(&session.session_id, "first-controller", None)
        .expect("acquire initial lease after the first successful defensive release");

    let error = manager
        .release_lease(&session.session_id, &lease_secret)
        .expect_err("a refused input release must block ownership handoff");
    assert!(error.contains(BROWSER_ERROR_LEASE_BLOCKED), "{error}");
    assert!(error.contains("fixture_input_release_failure"), "{error}");
    let projection = manager
        .session_projection(&session.session_id)
        .expect("blocked projection");
    assert_eq!(projection.lease_state, BrowserLeaseState::Blocked);
    assert!(
        projection
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("fixture_input_release_failure"))
    );
    assert_eq!(
        manager
            .takeover_lease(&session.session_id, "second-controller", None)
            .expect_err("a second controller must not receive a fabricated lease"),
        format!(
            "{BROWSER_ERROR_LEASE_BLOCKED}: {}",
            projection.blocked_reason.expect("blocked reason")
        )
    );
}

/// Keeps a rejected initial page inventory from publishing a live browser or leaking its host.
/// 防止初始页面清单被拒后仍发布运行中浏览器或遗留宿主。
#[test]
fn rejected_initial_page_list_closes_the_new_host() {
    let (manager, alive) = manager_with_fake_host_options(Duration::ZERO, true);
    let session = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
        .expect("create browser");

    let error = manager
        .start_host(&session.session_id)
        .expect_err("initial page enumeration must fail");
    assert!(error.contains("fixture_page_list_failure"), "{error}");
    assert!(!manager.has_live_host(&session.session_id));
    assert!(!alive.load(AtomicOrdering::SeqCst));
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("closed projection")
            .status,
        BrowserSessionStatus::Closed
    );
}

/// Verifies uncertain AI work is recoverable only by closing the host browser context.
/// 验证未知 AI 操作只能在关闭宿主浏览器上下文后恢复。
#[test]
fn blocked_ai_gateway_closes_the_browser_before_releasing_ownership() {
    let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
    let session = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
        .expect("create owned browser");
    manager.start_host(&session.session_id).expect("start host");
    let (_, lease_secret) = manager
        .acquire_lease(&session.session_id, "automation", None)
        .expect("acquire automation lease");
    manager
        .attach_automation(&session.session_id, &lease_secret, "connection-a")
        .expect("attach automation");
    manager
        .mark_operation_unconfirmed(&session.session_id, "in-flight CDP")
        .expect("block uncertain work");
    assert!(
        manager
            .close_uncertain_automation_session(
                &session.session_id,
                "conversation-b",
                "connection-a",
            )
            .is_err()
    );
    assert!(
        manager
            .close_uncertain_automation_session(
                &session.session_id,
                "conversation-a",
                "connection-b",
            )
            .is_err()
    );
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("still blocked")
            .lease_state,
        BrowserLeaseState::Blocked
    );
    let closed = manager
        .close_uncertain_automation_session(&session.session_id, "conversation-a", "connection-a")
        .expect("host confirms context closure");
    assert_eq!(closed.status, BrowserSessionStatus::Closed);
    assert_eq!(closed.lease_state, BrowserLeaseState::Free);
    assert_eq!(closed.automation_state, BrowserAutomationState::Detached);
    assert_eq!(closed.blocked_reason, None);
    assert!(!manager.has_live_host(&session.session_id));
}

/// Verifies the host supervisor finishes a confirmed closure after a slow process exit.
/// 验证宿主监督器在进程延迟退出后完成已确认的关闭。
#[test]
fn supervisor_finishes_confirmed_blocked_retirement_as_close_not_crash() {
    let (manager, alive) = manager_with_fake_host(Duration::ZERO);
    let session = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
        .expect("create owned browser");
    manager.start_host(&session.session_id).expect("start host");
    manager
        .mark_operation_unconfirmed(&session.session_id, "in-flight CDP")
        .expect("block uncertain work");
    manager
        .inner
        .lock()
        .expect("lock browser state")
        .sessions
        .get_mut(&session.session_id)
        .expect("browser record")
        .recovery_context_closed = true;
    alive.store(false, AtomicOrdering::SeqCst);
    let closed = manager
        .reap_host(&session.session_id)
        .expect("reap host")
        .expect("retirement is complete");
    assert_eq!(closed.status, BrowserSessionStatus::Closed);
    assert_eq!(closed.blocked_reason, None);
    assert!(!manager.has_live_host(&session.session_id));
}

/// Verifies a host crash cannot erase an unconfirmed write or reopen the same profile.
/// 验证宿主崩溃不能抹去未确认写入或重开同一画像。
#[test]
fn crashed_host_keeps_uncertain_profile_blocked() {
    let (manager, alive) = manager_with_fake_host(Duration::ZERO);
    let first = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
        .expect("create first browser");
    manager
        .start_host(&first.session_id)
        .expect("start first host");
    manager
        .mark_operation_unconfirmed(&first.session_id, "in-flight CDP")
        .expect("block uncertain work");
    alive.store(false, AtomicOrdering::SeqCst);
    let crashed = manager
        .reap_host(&first.session_id)
        .expect("reap first host")
        .expect("host exited");
    assert_eq!(crashed.status, BrowserSessionStatus::Crashed);
    assert_eq!(crashed.lease_state, BrowserLeaseState::Blocked);
    assert_eq!(
        manager.start_host(&first.session_id).unwrap_err(),
        BROWSER_ERROR_LEASE_BLOCKED
    );
    let second = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
        .expect("create second browser");
    assert_eq!(
        manager.start_host(&second.session_id).unwrap_err(),
        BROWSER_ERROR_PROFILE_IN_USE
    );
}

/// Verifies a started host is adopted with a confirmed generation and process id.
/// 验证已启动宿主被接管且代际与进程标识已确认。
#[test]
fn a_started_host_is_adopted_with_a_confirmed_generation() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    let running = manager.start_host(&session.session_id).expect("start host");
    assert_eq!(running.host_state, BrowserHostSupervisionState::Running);
    assert_eq!(running.host_generation, 1);
    assert!(manager.has_live_host(&session.session_id));
    assert_eq!(
        manager.confirmed_host_generation(&session.session_id),
        Some(1)
    );
}

/// Verifies navigation and viewport state change only through an authorized host response.
/// 验证导航和视口状态仅通过已授权的宿主应答改变。
#[test]
fn host_backed_navigation_and_viewport_require_the_live_lease() {
    let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");
    let page = manager
        .adopt_page(&session.session_id, "target-a", "about:blank", None)
        .expect("adopt page");
    let (_, secret) = manager
        .acquire_lease(&session.session_id, "user", None)
        .expect("acquire lease");
    assert_eq!(
        manager
            .navigate_page_on_host(
                &session.session_id,
                &page.page_id,
                "https://example.test/new",
                "wrong",
            )
            .unwrap_err(),
        BROWSER_ERROR_LEASE_REQUIRED
    );
    assert_eq!(
        manager
            .navigate_page_on_host(
                &session.session_id,
                &page.page_id,
                "file:///secret.txt",
                &secret,
            )
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("before")
            .page_generation,
        1
    );
    let navigated = manager
        .navigate_page_on_host(
            &session.session_id,
            &page.page_id,
            "https://example.test/new",
            &secret,
        )
        .expect("navigate host");
    assert_eq!(navigated.page_generation, 2);
    assert_eq!(
        navigated.current_url.as_deref(),
        Some("https://example.test/new")
    );
    assert_eq!(
        manager.list_pages(&session.session_id).expect("pages")[0].page_id,
        page.page_id
    );
    assert_eq!(
        manager
            .set_host_viewport(&session.session_id, 1024, 768, "wrong")
            .unwrap_err(),
        BROWSER_ERROR_LEASE_REQUIRED
    );
    let resized = manager
        .set_host_viewport(&session.session_id, 1024, 768, &secret)
        .expect("resize host");
    assert_eq!(resized.viewport_generation, 2);
    assert_eq!(
        resized.viewport_size,
        Some(BrowserViewportSize {
            width: 1024,
            height: 768
        })
    );
}

/// Verifies Core resolves a stable page id and discards a forged host target in input.
/// 验证 Core 解析稳定页面标识并丢弃输入中伪造的宿主目标。
#[test]
fn host_input_uses_core_page_identity_instead_of_client_target() {
    let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");
    let page = manager
        .adopt_page(&session.session_id, "target-a", "about:blank", None)
        .expect("adopt page");
    let (_, secret) = manager
        .acquire_lease(&session.session_id, "user", None)
        .expect("acquire lease");
    let event = BrowserInputEvent {
        sequence: String::from("1"),
        kind: String::from("pointer"),
        payload: json!({ "targetId": "target-b", "x": 10, "y": 10 }),
    };
    let applied = manager
        .dispatch_host_input(
            &session.session_id,
            &page.page_id,
            &secret,
            1,
            1,
            event.clone(),
        )
        .expect("dispatch input");
    assert_eq!(applied.outcome, BrowserInputOutcome::Applied);
    assert_eq!(applied.detail.as_ref().unwrap()["targetId"], "target-a");
    let rejected = manager
        .dispatch_host_input(&session.session_id, "missing-page", &secret, 1, 1, event)
        .expect("reject unknown page");
    assert_eq!(rejected.outcome, BrowserInputOutcome::Rejected);
    assert_eq!(
        rejected.reason.as_deref(),
        Some(BROWSER_ERROR_INVALID_REQUEST)
    );
}

/// Verifies slow host startup never blocks the global manager state lock.
/// 验证缓慢的宿主启动绝不阻塞全局管理器状态锁。
#[test]
fn slow_host_startup_does_not_block_the_global_state_lock() {
    // The fake host delays its handshake long enough that a blocking implementation would
    // hold the state lock for the whole window.
    // 伪宿主延迟握手足够久，以致阻塞式实现会在整个窗口内持有状态锁。
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(600));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    let worker = {
        let manager = manager.clone();
        let session_id = session.session_id;
        thread::spawn(move || manager.start_host(&session_id))
    };
    // While the host is starting, unrelated state reads must stay responsive.
    // 宿主启动期间，无关的状态读取必须保持可响应。
    thread::sleep(Duration::from_millis(150));
    let started = std::time::Instant::now();
    let sessions = manager
        .list_sessions()
        .expect("list sessions during startup");
    let elapsed = started.elapsed();
    assert_eq!(sessions.len(), 1);
    assert!(
        elapsed < Duration::from_millis(300),
        "listing sessions during host startup took {elapsed:?}; the state lock was held across host I/O"
    );
    worker
        .join()
        .expect("host startup thread")
        .expect("start host");
}

/// Verifies only browsers of one conversation reuse its persistent login state.
/// 验证只有同一对话的浏览器复用其持久登录态。
#[test]
fn persistent_profiles_are_isolated_by_conversation() {
    let profiles_root = std::env::temp_dir().join(format!("vulcan-w06-modes-{}", Uuid::new_v4()));
    let config = BrowserHostRuntimeConfig {
        executable: std::path::PathBuf::from("fake-electron.exe"),
        entry: std::path::PathBuf::from("fake-host.js"),
        chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
        profiles_root: profiles_root.clone(),
        staging_root: profiles_root.join("staging"),
        discovery_timeout: Duration::from_secs(1),
        request_timeout: Duration::from_secs(1),
    };
    // Two browsers of one conversation reuse login state; a different conversation cannot.
    // 同一对话的两个浏览器复用登录态；不同对话不能复用。
    let first = config
        .launch_plan(
            "session-a",
            "conversation-a",
            BrowserProfileMode::Persistent,
            1,
            "secret",
        )
        .expect("first persistent plan");
    let second = config
        .launch_plan(
            "session-b",
            "conversation-a",
            BrowserProfileMode::Persistent,
            1,
            "secret",
        )
        .expect("second persistent plan");
    assert_eq!(first.user_data_directory, second.user_data_directory);
    let other = config
        .launch_plan(
            "session-c",
            "conversation-b",
            BrowserProfileMode::Persistent,
            1,
            "secret",
        )
        .expect("different conversation persistent plan");
    assert_ne!(first.user_data_directory, other.user_data_directory);
    assert_eq!(
        first.user_data_directory.parent(),
        Some(profiles_root.join("persistent").as_path())
    );
    let path_like_owner = config
        .launch_plan(
            "session-d",
            "../conversation-a",
            BrowserProfileMode::Persistent,
            1,
            "secret",
        )
        .expect("path-like owner remains confined");
    assert_eq!(
        path_like_owner.user_data_directory.parent(),
        Some(profiles_root.join("persistent").as_path())
    );
    // Ephemeral sessions must never share a directory with each other or with the persistent one.
    // 临时会话之间以及与会话持久目录之间绝不共享目录。
    let ephemeral_a = config
        .launch_plan(
            "session-a",
            "conversation-a",
            BrowserProfileMode::Ephemeral,
            1,
            "secret",
        )
        .expect("ephemeral plan a");
    let ephemeral_b = config
        .launch_plan(
            "session-b",
            "conversation-a",
            BrowserProfileMode::Ephemeral,
            1,
            "secret",
        )
        .expect("ephemeral plan b");
    assert_ne!(
        ephemeral_a.user_data_directory,
        ephemeral_b.user_data_directory
    );
    assert_ne!(ephemeral_a.user_data_directory, first.user_data_directory);
    assert_ne!(ephemeral_b.user_data_directory, first.user_data_directory);
}

/// Verifies a second concurrent persistent session is refused rather than sharing one profile.
/// 验证第二个并发持久会话被拒绝，而不是共享同一画像。
#[test]
fn a_second_concurrent_persistent_session_is_refused() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let first = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("first persistent session");
    manager.start_host(&first.session_id).expect("first host");
    let second = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("second persistent session");
    assert_eq!(
        manager.start_host(&second.session_id).unwrap_err(),
        BROWSER_ERROR_PROFILE_IN_USE,
        "two concurrent hosts must never share one Chromium user-data directory"
    );
    // An ephemeral session is unaffected because it owns its own directory.
    // 临时会话不受影响，因为它拥有自己的目录。
    let ephemeral = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("ephemeral session");
    assert!(manager.start_host(&ephemeral.session_id).is_ok());
}

/// Verifies a startup reserves its owner's profile before the slow host handshake completes.
/// 验证缓慢宿主握手完成前，启动已预留所属对话的画像。
#[test]
fn in_flight_host_start_reserves_only_its_owners_persistent_profile() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(400));
    let first = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
        .expect("first browser");
    let worker = {
        let manager = manager.clone();
        let session_id = first.session_id.clone();
        thread::spawn(move || manager.start_host(&session_id))
    };
    for _ in 0..100 {
        if manager
            .inner
            .lock()
            .expect("manager state")
            .starting_hosts
            .contains(&first.session_id)
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        manager
            .inner
            .lock()
            .expect("manager state")
            .starting_hosts
            .contains(&first.session_id)
    );
    let same_owner = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
        .expect("second browser of same conversation");
    assert_eq!(
        manager.start_host(&same_owner.session_id).unwrap_err(),
        BROWSER_ERROR_PROFILE_IN_USE
    );
    let other_owner = manager
        .create_owned_session("conversation-b", BrowserProfileMode::Persistent)
        .expect("browser of another conversation");
    manager
        .start_host(&other_owner.session_id)
        .expect("another conversation may start independently");
    worker.join().expect("start worker").expect("first host");
}

/// Verifies an explicit close cannot be overwritten by a late host-start response.
/// 验证显式关闭不会被迟到的宿主启动响应覆盖。
#[test]
fn closing_during_host_start_cannot_revive_the_browser() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(300));
    let session = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Persistent)
        .expect("browser session");
    let worker = {
        let manager = manager.clone();
        let session_id = session.session_id.clone();
        thread::spawn(move || manager.start_host(&session_id))
    };
    for _ in 0..100 {
        if manager
            .inner
            .lock()
            .expect("manager state")
            .starting_hosts
            .contains(&session.session_id)
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        manager
            .inner
            .lock()
            .expect("manager state")
            .starting_hosts
            .contains(&session.session_id)
    );
    let closed = manager
        .close_session(&session.session_id)
        .expect("close browser");
    worker
        .join()
        .expect("start worker")
        .expect("startup completed before close");
    assert_eq!(closed.status, BrowserSessionStatus::Closed);
    assert!(!manager.has_live_host(&session.session_id));
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("projection")
            .status,
        BrowserSessionStatus::Closed
    );
}

/// Verifies closing a persistent session frees the shared profile for the next session.
/// 验证关闭持久会话会为下一个会话释放共享画像。
#[test]
fn closing_a_persistent_session_frees_the_shared_profile() {
    let (manager, alive) = manager_with_fake_host(Duration::from_millis(0));
    let first = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("first persistent session");
    manager.start_host(&first.session_id).expect("first host");
    manager
        .close_session(&first.session_id)
        .expect("close first");
    assert!(!alive.load(AtomicOrdering::SeqCst));
    // Closing ends the host, so the shared profile is no longer occupied.
    // 关闭会结束宿主，因此共享画像不再被占用。
    assert!(!manager.has_live_host(&first.session_id));
    let second = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("second persistent session");
    assert!(
        manager.start_host(&second.session_id).is_ok(),
        "a later persistent session must be able to reuse the shared profile"
    );
}

/// Verifies repeated open-close cycles cannot grow retained browser history without a bound.
/// 验证反复开启和关闭不会让浏览器保留历史无界增长。
#[test]
fn closed_browser_history_is_bounded() {
    let manager = BrowserManager::new();
    let mut oldest = String::new();
    let mut newest = String::new();
    for index in 0..BROWSER_MAX_RETAINED_CLOSED_SESSIONS + 5 {
        let browser = manager
            .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
            .expect("create browser");
        if index == 0 {
            oldest = browser.session_id.clone();
        }
        newest = browser.session_id.clone();
        manager
            .close_session(&browser.session_id)
            .expect("close browser");
    }
    assert_eq!(
        manager
            .list_owned_sessions("conversation-a")
            .expect("list browsers")
            .len(),
        BROWSER_MAX_RETAINED_CLOSED_SESSIONS
    );
    assert_eq!(
        manager.session_projection(&oldest).unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
    assert_eq!(
        manager
            .session_projection(&newest)
            .expect("newest history")
            .status,
        BrowserSessionStatus::Closed
    );
}

/// Verifies an expired AI lease cannot hand a still-attached Playwright socket to another holder.
/// 验证到期的 AI 租约不会把仍挂接的 Playwright 套接字交给其他控制者。
#[test]
fn expired_attached_automation_lease_blocks_new_control_until_revoked() {
    let (manager, _alive) = manager_with_fake_host(Duration::ZERO);
    let session = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
        .expect("create browser");
    manager.start_host(&session.session_id).expect("start host");
    let (_, ai_secret) = manager
        .acquire_lease(
            &session.session_id,
            "ai:conversation-a",
            Some(Duration::from_millis(20)),
        )
        .expect("acquire AI lease");
    manager
        .attach_automation(&session.session_id, &ai_secret, "connection-a")
        .expect("attach automation");
    thread::sleep(Duration::from_millis(40));

    assert_eq!(
        manager
            .acquire_lease(&session.session_id, "user", None)
            .unwrap_err(),
        BROWSER_ERROR_AUTOMATION_CONFLICT
    );
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("projection")
            .lease_state,
        BrowserLeaseState::Blocked
    );
    assert!(
        manager
            .release_lease(&session.session_id, &ai_secret)
            .is_err()
    );
    manager
        .confirm_operations_stopped(&session.session_id, None)
        .expect("ordinary confirmation");
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("projection")
            .lease_state,
        BrowserLeaseState::Blocked,
        "general drain confirmation must not clear an attached gateway's expired lease"
    );

    manager
        .detach_automation(&session.session_id, "connection-a")
        .expect("confirmed gateway revocation");
    manager
        .confirm_operations_stopped(
            &session.session_id,
            Some(BROWSER_AUTOMATION_LEASE_EXPIRED_OPERATION),
        )
        .expect("confirm expired gateway stopped");
    manager
        .release_lease(&session.session_id, &ai_secret)
        .expect("release revoked AI lease");
    manager
        .acquire_lease(&session.session_id, "user", None)
        .expect("new holder after confirmed revocation");
}

/// Verifies one session's host requests serialize on a single dispatch point.
/// 验证单个会话的宿主请求在唯一调度点上串行化。
#[test]
fn same_session_requests_serialize_on_one_dispatch_point() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");

    let workers: Vec<_> = (0..4)
        .map(|index| {
            let manager = manager.clone();
            let session_id = session.session_id.clone();
            thread::spawn(move || {
                manager
                    .request_host(&session_id, "host.ping", json!({ "index": index }))
                    .expect("request host")
            })
        })
        .collect();
    for worker in workers {
        let outcome = worker.join().expect("request thread");
        assert!(
            matches!(outcome, BrowserHostRequestOutcome::Completed(_)),
            "every serialized request must complete, got {outcome:?}"
        );
    }
}

/// Verifies an exited host is reaped as a crash with a retained snapshot.
/// 验证已退出宿主被作为崩溃回收并保留快照。
#[test]
fn an_exited_host_is_reaped_as_a_crash() {
    let (manager, alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");
    manager
        .adopt_page(
            &session.session_id,
            "target-a",
            "https://example.test/",
            None,
        )
        .expect("adopt page");

    // The process disappears without an explicit close, which is a crash.
    // 进程在没有显式关闭的情况下消失，即崩溃。
    alive.store(false, AtomicOrdering::SeqCst);
    let reaped = manager
        .reap_host(&session.session_id)
        .expect("reap host")
        .expect("a dead host must be reaped");
    assert_eq!(reaped.status, BrowserSessionStatus::Crashed);
    assert!(reaped.requires_explicit_reopen);
    assert_eq!(reaped.page_count, 0);
    assert!(!manager.has_live_host(&session.session_id));
    // Reaping again must not invent a second crash.
    // 再次回收不得凭空产生第二次崩溃。
    assert!(
        manager
            .reap_host(&session.session_id)
            .expect("reap again")
            .is_none()
    );
}

/// Keeps the owned host available when the OS cannot determine whether it exited.
/// 操作系统无法确认宿主是否退出时保留归属句柄，以供后续复查。
#[test]
fn host_process_inspection_failure_preserves_ownership() {
    let (manager, alive, poll_error) = manager_with_fake_host_poll(Duration::ZERO, false);
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");

    poll_error.store(true, AtomicOrdering::SeqCst);
    let error = manager
        .reap_host(&session.session_id)
        .expect_err("inspection failure must not mean the process is running");
    assert!(error.contains("injected host process inspection failure"));
    assert!(manager.has_live_host(&session.session_id));
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("projection after failed inspection")
            .status,
        BrowserSessionStatus::Running
    );

    poll_error.store(false, AtomicOrdering::SeqCst);
    alive.store(false, AtomicOrdering::SeqCst);
    let reaped = manager
        .reap_host(&session.session_id)
        .expect("inspect after recovery")
        .expect("exited host must be reaped");
    assert_eq!(reaped.status, BrowserSessionStatus::Crashed);
    assert!(!manager.has_live_host(&session.session_id));
}

/// Continues checking independent hosts after one transient process-inspection error.
/// 一个进程检查暂时失败后继续检查其他独立宿主。
#[test]
fn one_host_inspection_failure_does_not_starve_other_sessions() {
    let (manager, alive, poll_error) = manager_with_fake_host_poll(Duration::ZERO, false);
    let first = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create first session");
    let second = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create second session");
    manager
        .start_host(&first.session_id)
        .expect("start first host");
    manager
        .start_host(&second.session_id)
        .expect("start second host");

    alive.store(false, AtomicOrdering::SeqCst);
    poll_error.store(true, AtomicOrdering::SeqCst);
    let error = manager
        .reap_hosts()
        .expect_err("one liveness inspection must fail explicitly");
    assert!(error.contains("injected host process inspection failure"));
    assert!(error.contains("reaped sessions:"));
    let remaining = [&first.session_id, &second.session_id]
        .into_iter()
        .filter(|session_id| manager.has_live_host(session_id))
        .count();
    assert_eq!(remaining, 1, "the other exited host must be reaped");

    let recovered = manager.reap_hosts().expect("retry remaining host");
    assert_eq!(recovered.len(), 1);
    assert!(!manager.has_live_host(&first.session_id));
    assert!(!manager.has_live_host(&second.session_id));
}

/// Blocks explicit close until a previously uninspectable host is confirmed exited.
/// 无法检查宿主进程时阻塞显式关闭，直到确认该进程退出。
#[test]
fn explicit_close_blocks_on_process_inspection_failure() {
    let (manager, alive, poll_error) = manager_with_fake_host_poll(Duration::ZERO, false);
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");

    poll_error.store(true, AtomicOrdering::SeqCst);
    let error = manager
        .close_session(&session.session_id)
        .expect_err("process inspection cannot confirm closure");
    assert!(error.contains(BROWSER_ERROR_LEASE_BLOCKED), "{error}");
    assert!(error.contains("injected host process inspection failure"));
    assert!(manager.has_live_host(&session.session_id));
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("blocked projection")
            .lease_state,
        BrowserLeaseState::Blocked
    );
    assert!(!alive.load(AtomicOrdering::SeqCst));

    let closed = manager
        .reap_host(&session.session_id)
        .expect("recheck exact host exit")
        .expect("the stopped host must be retired");
    assert_eq!(closed.status, BrowserSessionStatus::Closed);
    assert!(!manager.has_live_host(&session.session_id));
}

/// Verifies a live host is not reaped by a liveness check.
/// 验证存活检查不会回收仍在运行的宿主。
#[test]
fn a_live_host_is_not_reaped() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");
    assert!(
        manager
            .reap_host(&session.session_id)
            .expect("reap")
            .is_none()
    );
    assert!(manager.has_live_host(&session.session_id));
    assert!(
        manager.reap_hosts().expect("reap all").is_empty(),
        "no running host may be reported as crashed"
    );
}

/// Verifies Core shutdown closes every host channel so no host is orphaned.
/// 验证 Core 关闭会关闭所有宿主通道，因此没有宿主被遗留为孤儿。
#[test]
fn core_shutdown_closes_every_host_channel() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");
    assert_eq!(manager.shutdown_hosts().expect("shutdown hosts"), 1);
    assert!(!manager.has_live_host(&session.session_id));
    // A second shutdown is a harmless no-op.
    // 第二次关闭是无害的空操作。
    assert_eq!(manager.shutdown_hosts().expect("shutdown again"), 0);
}

/// Verifies starting a host without runtime configuration reports a real error.
/// 验证未配置运行时时启动宿主返回真实错误。
#[test]
fn starting_a_host_without_configuration_reports_a_real_error() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    assert_eq!(
        manager.start_host(&session.session_id).unwrap_err(),
        BROWSER_ERROR_CAPABILITY_UNSUPPORTED
    );
    assert_eq!(
        manager.start_host("missing").unwrap_err(),
        BROWSER_ERROR_CAPABILITY_UNSUPPORTED
    );
}

/// Verifies host requests reject a session that owns no live host.
/// 验证宿主请求拒绝没有活动宿主的会话。
#[test]
fn host_requests_reject_a_session_without_a_live_host() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    assert_eq!(
        manager
            .request_host(&session.session_id, "host.ping", json!({}))
            .unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
}

/// Verifies an explicit takeover revokes the previous holder and rotates its credentials.
/// 验证明确接管撤销前任持有者并轮换其凭据。
#[test]
fn takeover_revokes_the_previous_holder_and_rotates_credentials() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (before, old_secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("alpha acquires");
    let old_epoch = before.control_epoch;

    let (after, new_secret) = manager
        .takeover_lease(&session.session_id, "beta", None)
        .expect("beta takes over");
    assert_ne!(old_secret, new_secret, "a takeover must rotate the secret");
    assert!(
        after.control_epoch > old_epoch,
        "a takeover must advance the control epoch"
    );
    assert_eq!(after.lease_state, BrowserLeaseState::Held);

    // The previous holder's later commands must fail against the new epoch.
    // 前任持有者随后的命令必须在新代次下失败。
    assert_eq!(
        manager
            .dispatch_input(
                &session.session_id,
                &old_secret,
                after.page_generation,
                BrowserInputEvent {
                    sequence: String::from("1"),
                    kind: String::from("pointer"),
                    payload: json!({}),
                },
            )
            .unwrap_err(),
        BROWSER_ERROR_LEASE_REQUIRED
    );
    // The new holder can act.
    // 新持有者可以操作。
    assert!(
        manager
            .dispatch_input(
                &session.session_id,
                &new_secret,
                after.page_generation,
                BrowserInputEvent {
                    sequence: String::from("2"),
                    kind: String::from("pointer"),
                    payload: json!({}),
                },
            )
            .is_ok()
    );
}

/// Verifies an AI heartbeat retains its long lease instead of reverting to the short UI TTL.
/// 验证 AI 心跳保持长租约，而不会退回较短的界面租约时长。
#[test]
fn automation_heartbeat_preserves_its_explicit_ttl() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create browser session");
    let (_, secret) = manager
        .acquire_lease(
            &session.session_id,
            "ai-owner",
            Some(Duration::from_secs(1)),
        )
        .expect("acquire short initial lease");
    manager
        .heartbeat_lease_with_ttl(&session.session_id, &secret, Duration::from_secs(600))
        .expect("renew AI lease");
    let state = manager.inner.lock().expect("lock browser manager");
    let remaining = state.sessions[&session.session_id]
        .lease_expires_at
        .expect("lease expiry")
        .saturating_duration_since(Instant::now());
    assert!(remaining > Duration::from_secs(590));
}

/// Verifies input without a lease is rejected and changes nothing.
/// 验证无租约的输入被拒绝且不改变任何状态。
#[test]
fn input_without_a_lease_is_rejected_and_changes_nothing() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    assert_eq!(
        manager
            .dispatch_input(
                &session.session_id,
                "not-a-secret",
                session.page_generation,
                BrowserInputEvent {
                    sequence: String::from("1"),
                    kind: String::from("pointer"),
                    payload: json!({}),
                },
            )
            .unwrap_err(),
        BROWSER_ERROR_LEASE_REQUIRED
    );
    // No lease exists, so nothing was queued for the host.
    // 不存在租约，因此没有内容被排队给宿主。
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("projection")
            .lease_state,
        BrowserLeaseState::Free
    );
}

/// Rejects expired credentials at dispatch even when no competing client renewed or acquired.
/// 即使没有竞争客户端续租或获取，也在派发处拒绝已到期凭据。
#[test]
fn input_checks_expiry_without_another_lease_operation() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .unwrap();
    let (_, secret) = manager
        .acquire_lease(&session.session_id, "alpha", Some(Duration::ZERO))
        .unwrap();
    let error = manager
        .dispatch_input(
            &session.session_id,
            &secret,
            session.page_generation,
            BrowserInputEvent {
                sequence: "1".into(),
                kind: "text".into(),
                payload: json!({"text":"must not arrive"}),
            },
        )
        .unwrap_err();
    assert_eq!(error, BROWSER_ERROR_LEASE_REQUIRED);
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .unwrap()
            .lease_state,
        BrowserLeaseState::Free
    );
}

/// Keeps uncertain work blocked after its holder releases credentials.
/// 持有者释放凭据后，结果不确定的工作仍保持阻塞。
#[test]
fn release_cannot_bypass_an_unconfirmed_operation() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .unwrap();
    let (_, secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .unwrap();
    manager
        .mark_operation_unconfirmed(&session.session_id, "input:unknown")
        .unwrap();
    assert_eq!(
        manager
            .release_lease(&session.session_id, &secret)
            .unwrap()
            .lease_state,
        BrowserLeaseState::Blocked
    );
    assert_eq!(
        manager
            .acquire_lease(&session.session_id, "beta", None)
            .unwrap_err(),
        BROWSER_ERROR_LEASE_CONFLICT
    );
}

/// Verifies a takeover blocked by unconfirmed work issues no lease and stays visible.
/// 验证被未确认工作阻塞的接管不签发租约且保持可见。
#[test]
fn a_blocked_takeover_issues_no_new_lease() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (_projection, holder_secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("alpha acquires");
    // An in-flight script whose completion could not be confirmed.
    // 一个无法确认是否结束的在途脚本。
    let blocked = manager
        .mark_operation_unconfirmed(&session.session_id, "automation.attach:script-7")
        .expect("mark unconfirmed");
    assert_eq!(blocked.lease_state, BrowserLeaseState::Blocked);
    assert_eq!(
        blocked.blocked_reason.as_deref(),
        Some("automation.attach:script-7")
    );

    // The takeover is refused, and no new credentials are fabricated.
    // 接管被拒绝，且不伪造新凭据。
    let error = manager
        .takeover_lease(&session.session_id, "beta", None)
        .unwrap_err();
    assert!(
        error.starts_with(BROWSER_ERROR_LEASE_BLOCKED),
        "a blocked takeover must report lease_blocked, got {error}"
    );
    // Retaining the former identity for diagnosis must not allow further page mutations.
    // 为诊断保留原身份不能允许继续修改页面。
    let projection = manager
        .session_projection(&session.session_id)
        .expect("projection");
    assert_eq!(projection.lease_state, BrowserLeaseState::Blocked);
    assert_eq!(
        manager
            .dispatch_input(
                &session.session_id,
                &holder_secret,
                projection.page_generation,
                BrowserInputEvent {
                    sequence: String::from("1"),
                    kind: String::from("pointer"),
                    payload: json!({}),
                },
            )
            .unwrap_err(),
        BROWSER_ERROR_LEASE_BLOCKED
    );
    assert_eq!(
        manager
            .unconfirmed_operations(&session.session_id)
            .expect("unconfirmed"),
        vec![String::from("automation.attach:script-7")]
    );
}

/// Verifies confirming the stopped work unblocks and then allows the takeover.
/// 验证确认工作已停止后解除阻塞并允许接管。
#[test]
fn confirming_stopped_work_unblocks_the_takeover() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("alpha acquires");
    manager
        .mark_operation_unconfirmed(&session.session_id, "automation.attach:script-7")
        .expect("mark unconfirmed");
    assert!(
        manager
            .takeover_lease(&session.session_id, "beta", None)
            .is_err()
    );

    // Only an explicit confirmation clears the block; nothing guesses that work stopped.
    // 只有明确确认才会解除阻塞；绝不猜测工作已经停止。
    let cleared = manager
        .confirm_operations_stopped(&session.session_id, None)
        .expect("confirm stopped");
    assert!(cleared.blocked_reason.is_none());
    assert_eq!(cleared.lease_state, BrowserLeaseState::Held);

    let (taken, _secret) = manager
        .takeover_lease(&session.session_id, "beta", None)
        .expect("takeover after confirmation");
    assert_eq!(taken.lease_state, BrowserLeaseState::Held);
}

/// Verifies a single confirmed operation does not clear the others.
/// 验证确认单项操作不会清除其他项。
#[test]
fn confirming_one_operation_keeps_the_others() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .mark_operation_unconfirmed(&session.session_id, "operation-a")
        .expect("mark a");
    manager
        .mark_operation_unconfirmed(&session.session_id, "operation-b")
        .expect("mark b");
    // Re-marking the same operation must not duplicate it.
    // 重复标记同一操作不得使其重复。
    manager
        .mark_operation_unconfirmed(&session.session_id, "operation-a")
        .expect("mark a again");
    assert_eq!(
        manager
            .unconfirmed_operations(&session.session_id)
            .expect("unconfirmed"),
        vec![String::from("operation-a"), String::from("operation-b")]
    );

    let partial = manager
        .confirm_operations_stopped(&session.session_id, Some("operation-a"))
        .expect("confirm a");
    assert_eq!(partial.lease_state, BrowserLeaseState::Blocked);
    assert_eq!(
        manager
            .unconfirmed_operations(&session.session_id)
            .expect("unconfirmed"),
        vec![String::from("operation-b")]
    );
}

/// Verifies takeover rejects an empty holder or an unknown session.
/// 验证接管拒绝空持有者或未知会话。
#[test]
fn takeover_rejects_an_empty_holder_or_unknown_session() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    assert_eq!(
        manager
            .takeover_lease(&session.session_id, "  ", None)
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert_eq!(
        manager.takeover_lease("missing", "beta", None).unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
}

/// Verifies input computed for an old viewport is rejected after the viewport changes.
/// 验证视口改变后，针对旧视口计算的输入被拒绝。
#[test]
fn input_from_a_stale_viewport_is_rejected() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (projection, secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("acquire");
    let original_viewport = projection.viewport_generation;

    // The viewport changes, which invalidates every coordinate measured before it.
    // 视口发生变化，使此前测量的每个坐标失效。
    let resized = manager
        .set_viewport(&session.session_id, 1280, 800)
        .expect("set viewport");
    assert!(resized.viewport_generation > original_viewport);
    assert_eq!(resized.viewport_size.map(|size| size.width), Some(1280));

    // Input computed against the old viewport must be rejected, not silently reinterpreted.
    // 针对旧视口计算的输入必须被拒绝，而不是被静默重新解释。
    let stale = manager
        .dispatch_input_outcome(
            &session.session_id,
            &secret,
            resized.page_generation,
            original_viewport,
            BrowserInputEvent {
                sequence: String::from("1"),
                kind: String::from("pointer"),
                payload: json!({ "x": 10, "y": 20 }),
            },
        )
        .expect("dispatch stale");
    assert_eq!(stale.outcome, BrowserInputOutcome::Rejected);
    assert_eq!(stale.reason.as_deref(), Some(BROWSER_ERROR_STALE_VIEWPORT));
    assert!(stale.sequence.is_none());

    // Input against the new viewport is accepted, so the client can act after a fresh frame.
    // 针对新视口的输入被接受，因此客户端可以在取得新帧后操作。
    let fresh = manager
        .dispatch_input_outcome(
            &session.session_id,
            &secret,
            resized.page_generation,
            resized.viewport_generation,
            BrowserInputEvent {
                sequence: String::from("2"),
                kind: String::from("pointer"),
                payload: json!({ "x": 10, "y": 20 }),
            },
        )
        .expect("dispatch fresh");
    assert_eq!(fresh.outcome, BrowserInputOutcome::Accepted);
    assert!(fresh.sequence.is_some());
}

/// Verifies the four outcomes carry their real meaning.
/// 验证四种结果各自承载其真实含义。
#[test]
fn input_outcomes_carry_their_real_meaning() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (projection, secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("acquire");
    let event = |sequence: &str| BrowserInputEvent {
        sequence: String::from(sequence),
        kind: String::from("pointer"),
        payload: json!({}),
    };

    // Rejected: a failed check means nothing was attempted.
    // Rejected：检查失败意味着未尝试任何操作。
    let rejected = manager
        .dispatch_input_outcome(
            &session.session_id,
            "wrong-secret",
            projection.page_generation,
            projection.viewport_generation,
            event("1"),
        )
        .expect("dispatch");
    assert_eq!(rejected.outcome, BrowserInputOutcome::Rejected);
    assert_eq!(
        rejected.reason.as_deref(),
        Some(BROWSER_ERROR_LEASE_REQUIRED)
    );

    // Rejected: a stale page generation is refused before it reaches the host.
    // Rejected：过时页面代次在到达宿主之前即被拒绝。
    let stale = manager
        .dispatch_input_outcome(
            &session.session_id,
            &secret,
            projection.page_generation + 5,
            projection.viewport_generation,
            event("2"),
        )
        .expect("dispatch");
    assert_eq!(stale.outcome, BrowserInputOutcome::Rejected);
    assert_eq!(
        stale.reason.as_deref(),
        Some(BROWSER_ERROR_STALE_GENERATION)
    );

    // Accepted: handed to the host, application not yet confirmed.
    // Accepted：已交给宿主，应用结果尚未确认。
    let accepted = manager
        .dispatch_input_outcome(
            &session.session_id,
            &secret,
            projection.page_generation,
            projection.viewport_generation,
            event("3"),
        )
        .expect("dispatch");
    assert_eq!(accepted.outcome, BrowserInputOutcome::Accepted);
    let sequence = accepted.sequence.expect("accepted sequence");
    assert_eq!(
        manager
            .input_outcome(&session.session_id, sequence)
            .expect("outcome"),
        Some(BrowserInputOutcome::Accepted)
    );

    // Applied: the host confirmed it reached the page.
    // Applied：宿主确认其已到达页面。
    manager
        .record_input_applied(&session.session_id, sequence)
        .expect("record applied");
    assert_eq!(
        manager
            .input_outcome(&session.session_id, sequence)
            .expect("outcome"),
        Some(BrowserInputOutcome::Applied)
    );

    // Unknown: the outcome cannot be determined, and the session blocks a takeover rather than
    // pretending the input did not happen.
    // Unknown：结果无法确定，会话改为阻塞接管，而不是假定该输入未发生。
    let unknown_projection = manager
        .record_input_unknown(&session.session_id, sequence)
        .expect("record unknown");
    assert_eq!(
        manager
            .input_outcome(&session.session_id, sequence)
            .expect("outcome"),
        Some(BrowserInputOutcome::Unknown)
    );
    assert_eq!(unknown_projection.lease_state, BrowserLeaseState::Blocked);
    assert!(
        manager
            .takeover_lease(&session.session_id, "beta", None)
            .is_err(),
        "an input of unknown effect must block a takeover"
    );
    // Confirming the operation stopped clears both the block and the outstanding operation.
    // 确认操作已停止会同时清除阻塞与未结束操作。
    manager
        .confirm_operations_stopped(
            &session.session_id,
            Some(&format!("input.dispatch:{sequence}")),
        )
        .expect("confirm stopped");
    assert!(
        manager
            .unconfirmed_operations(&session.session_id)
            .expect("list")
            .is_empty()
    );
}

/// Verifies input sequence bookkeeping stays bounded as inputs accumulate.
/// 验证输入序号记录随着输入累积保持有界。
#[test]
fn input_outcome_bookkeeping_stays_bounded() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (projection, secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("acquire");
    for index in 0..(BROWSER_MAX_QUEUED_INPUTS + 40) {
        let outcome = manager
            .dispatch_input_outcome(
                &session.session_id,
                &secret,
                projection.page_generation,
                projection.viewport_generation,
                BrowserInputEvent {
                    sequence: index.to_string(),
                    kind: String::from("pointer"),
                    payload: json!({}),
                },
            )
            .expect("dispatch");
        assert_eq!(outcome.outcome, BrowserInputOutcome::Accepted);
    }
    // The queue and the outcome map are both bounded, so a fast client cannot grow memory.
    // 队列与结果映射均为有界，因此高速客户端无法使内存增长。
    let state = manager.inner.lock().expect("lock");
    let record = state.sessions.get(&session.session_id).expect("session");
    assert!(record.queued_inputs.len() <= BROWSER_MAX_QUEUED_INPUTS);
    assert!(record.input_outcomes.len() <= BROWSER_MAX_QUEUED_INPUTS);
}

/// Verifies an empty sequence or a zero-sized viewport is refused.
/// 验证空序号或零尺寸视口被拒绝。
#[test]
fn empty_sequences_and_zero_sized_viewports_are_refused() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (projection, secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("acquire");
    let empty = manager
        .dispatch_input_outcome(
            &session.session_id,
            &secret,
            projection.page_generation,
            projection.viewport_generation,
            BrowserInputEvent {
                sequence: String::from("   "),
                kind: String::from("pointer"),
                payload: json!({}),
            },
        )
        .expect("dispatch");
    assert_eq!(empty.outcome, BrowserInputOutcome::Rejected);
    assert_eq!(empty.reason.as_deref(), Some(BROWSER_ERROR_INVALID_REQUEST));
    assert_eq!(
        manager
            .set_viewport(&session.session_id, 0, 800)
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert_eq!(
        manager.set_viewport("missing", 100, 100).unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
}

/// Verifies a stream ticket authorises exactly one connection.
/// 验证流凭据仅授权一次连接。
#[test]
fn a_stream_ticket_authorises_exactly_one_connection() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let ticket = manager
        .issue_stream_ticket(&session.session_id)
        .expect("issue ticket");
    assert_eq!(
        manager
            .consume_stream_ticket(&ticket.ticket)
            .expect("first use"),
        session.session_id
    );
    // A replayed ticket must never open a second stream.
    // 重放的凭据绝不能被用来打开第二条流。
    assert_eq!(
        manager.consume_stream_ticket(&ticket.ticket).unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    // An invented ticket is refused just like a spent one.
    // 伪造的凭据与已用尽的凭据同样被拒绝。
    assert_eq!(
        manager.consume_stream_ticket("not-a-ticket").unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert_eq!(
        manager.issue_stream_ticket("missing").unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
}

/// Verifies unconsumed tickets remain bounded and a consumed slot becomes reusable.
/// 验证未消费凭据数量有界，消费后名额可以复用。
#[test]
fn pending_stream_tickets_have_a_per_session_limit() {
    // manager owns a single isolated browser session with no host process.
    // manager 持有一个无需宿主进程的隔离浏览器会话。
    let manager = BrowserManager::new();
    // session identifies the exact owner of the bounded ticket map.
    // session 标识有界凭据表的精确所属浏览器。
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    // tickets retain the grant IDs until the limit is reached.
    // tickets 保留签发 ID，直至达到上限。
    let tickets = (0..BROWSER_MAX_PENDING_STREAM_TICKETS)
        .map(|_| {
            manager
                .issue_stream_ticket(&session.session_id)
                .expect("issue ticket")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        manager
            .issue_stream_ticket(&session.session_id)
            .unwrap_err(),
        BROWSER_ERROR_STREAM_TICKET_LIMIT,
    );
    assert_eq!(
        manager
            .consume_stream_ticket(&tickets[0].ticket)
            .expect("consume one ticket"),
        session.session_id,
    );
    manager
        .issue_stream_ticket(&session.session_id)
        .expect("issue after consumption");
}

/// Verifies direct and resumed observers share one bound and unsubscribe clears stream ownership.
/// 验证直接观察与恢复观察共用一个上限，取消订阅也清除流所有权。
#[test]
fn observation_subscriptions_have_a_per_session_limit() {
    // manager owns the one session whose frame queues are bounded.
    // manager 持有帧队列受限的单个会话。
    let manager = BrowserManager::new();
    // session identifies the browser whose observer resources are allocated.
    // session 标识分配观察资源的浏览器。
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    // subscriptions retain every allocated observer slot until one is released.
    // subscriptions 保留已分配的全部观察名额，直至释放其中一个。
    let subscriptions = (0..BROWSER_MAX_OBSERVERS)
        .map(|_| {
            manager
                .subscribe_observer(&session.session_id)
                .expect("subscribe")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        manager.subscribe_observer(&session.session_id).unwrap_err(),
        BROWSER_ERROR_OBSERVER_LIMIT,
    );
    assert_eq!(
        manager
            .resume_stream(&session.session_id, "another-stream", None)
            .unwrap_err(),
        BROWSER_ERROR_OBSERVER_LIMIT,
    );
    manager
        .unsubscribe_observer(&session.session_id, &subscriptions[0])
        .expect("unsubscribe");
    manager
        .resume_stream(&session.session_id, "replacement-stream", None)
        .expect("resume after slot release");
    manager
        .unsubscribe_observer(&session.session_id, "replacement-stream")
        .expect("unsubscribe resumed stream");
    assert!(
        !manager
            .release_stream_subscription(&session.session_id, "replacement-stream")
            .expect("stream ownership was cleared")
    );
}

/// Verifies deferred host frames respect exact page generations and declared byte lengths.
/// 验证异步宿主画面严格遵守页面代次与声明的字节长度。
#[test]
fn deferred_host_frames_reject_stale_or_malformed_results() {
    use base64::Engine as _;

    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 4242)
        .expect("record host");
    let page = manager
        .adopt_page(&session.session_id, "target-frame", "about:blank", None)
        .expect("adopt page");
    manager
        .bind_stream_page(&session.session_id, &page.page_id, "frame-stream")
        .expect("bind observer");
    let projection = manager
        .session_projection(&session.session_id)
        .expect("projection");
    let image = [0xff, 0xd8, 0xff, 0xd9];
    let event = BrowserHostEvent {
        event_sequence: String::from("1"),
        kind: String::from("pageCaptured"),
        payload: serde_json::json!({
            "targetId": "target-frame",
            "pageGeneration": projection.page_generation.to_string(),
            "viewportGeneration": projection.viewport_generation.to_string(),
            "width": 1280,
            "height": 800,
            "mimeType": "image/jpeg",
            "byteLength": image.len(),
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(image),
            "capturedAt": "1",
        }),
    };
    assert!(
        !manager
            .apply_host_event(&session.session_id, &event)
            .expect("publish frame")
    );
    assert_eq!(
        manager
            .retained_frame_count(&session.session_id, "frame-stream")
            .expect("retained frames"),
        1
    );
    let mut stale = event.clone();
    stale.payload["pageGeneration"] =
        serde_json::json!((projection.page_generation + 1).to_string());
    assert!(
        !manager
            .apply_host_event(&session.session_id, &stale)
            .expect("discard stale frame")
    );
    let mut malformed = event;
    malformed.payload["byteLength"] = serde_json::json!(image.len() + 1);
    assert_eq!(
        manager
            .apply_host_event(&session.session_id, &malformed)
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert_eq!(
        manager
            .retained_frame_count(&session.session_id, "frame-stream")
            .expect("retained frames"),
        1
    );
}

/// Verifies a client with no cursor must resynchronise from a snapshot.
/// 验证没有游标的客户端必须从快照重新同步。
#[test]
fn a_stream_without_a_cursor_resynchronises_from_a_snapshot() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let subscription = manager
        .subscribe_observer(&session.session_id)
        .expect("subscribe");
    manager
        .publish_frame(
            &session.session_id,
            session.page_generation,
            "image/webp",
            vec![1, 2, 3],
        )
        .expect("publish frame");

    let resume = manager
        .resume_stream(&session.session_id, &subscription, None)
        .expect("resume");
    assert_eq!(resume.mode, BrowserStreamResumeMode::Snapshot);
    assert_eq!(
        resume.frames.len(),
        1,
        "a snapshot carries the retained frames"
    );
    assert_eq!(resume.discarded_inputs, 0);
}

/// Verifies a stale page generation forces a snapshot instead of a silent continuation.
/// 验证过时页面代次强制快照，而不是静默继续。
#[test]
fn a_stale_generation_forces_a_snapshot() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let subscription = manager
        .subscribe_observer(&session.session_id)
        .expect("subscribe");
    manager
        .publish_frame(
            &session.session_id,
            session.page_generation,
            "image/webp",
            vec![9],
        )
        .expect("publish frame");
    let resume = manager
        .resume_stream(
            &session.session_id,
            &subscription,
            Some(BrowserStreamCursor {
                // A generation that is not the current one cannot be extended.
                // 非当前代次无法被延续。
                page_generation: session.page_generation + 3,
                frame_sequence: Some(1),
            }),
        )
        .expect("resume");
    assert_eq!(resume.mode, BrowserStreamResumeMode::Snapshot);
}

/// Verifies a contiguous cursor resumes incrementally and delivers only newer frames.
/// 验证连续游标以增量方式恢复且只交付更新的帧。
#[test]
fn a_contiguous_cursor_resumes_incrementally() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let subscription = manager
        .subscribe_observer(&session.session_id)
        .expect("subscribe");
    for marker in 0..4_u8 {
        manager
            .publish_frame(
                &session.session_id,
                session.page_generation,
                "image/webp",
                vec![marker],
            )
            .expect("publish frame");
    }
    // Frames 1..4 exist; a client that confirmed frame 2 needs only frames 3 and 4.
    // 已存在帧 1..4；已确认帧 2 的客户端只需要帧 3 与 4。
    let resume = manager
        .resume_stream(
            &session.session_id,
            &subscription,
            Some(BrowserStreamCursor {
                page_generation: session.page_generation,
                frame_sequence: Some(2),
            }),
        )
        .expect("resume");
    assert_eq!(resume.mode, BrowserStreamResumeMode::Incremental);
    assert_eq!(resume.frames.len(), 2);
    assert_eq!(resume.frames[0].sequence, "3");
    assert_eq!(resume.frames[1].sequence, "4");
    assert_eq!(resume.dropped_frames, 0);
}

/// Verifies an observer snapshot never consumes another controller's pending input.
/// 验证观察者快照绝不消费另一个控制者的待处理输入。
#[test]
fn a_snapshot_resume_preserves_controller_input_without_replaying_it() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (projection, secret) = manager
        .acquire_lease(&session.session_id, "alpha", None)
        .expect("acquire");
    for index in 0..3 {
        manager
            .dispatch_input(
                &session.session_id,
                &secret,
                projection.page_generation,
                BrowserInputEvent {
                    sequence: index.to_string(),
                    kind: String::from("pointer"),
                    payload: json!({}),
                },
            )
            .expect("dispatch");
    }
    let resume = manager
        .resume_stream(&session.session_id, "observer-a", None)
        .expect("resume");
    assert_eq!(resume.mode, BrowserStreamResumeMode::Snapshot);
    assert_eq!(resume.discarded_inputs, 0);
    // Nothing is replayed by the stream, while controller-owned input remains untouched.
    // 流不重放任何输入，同时控制者拥有的输入保持不变。
    let state = manager.inner.lock().expect("lock");
    let record = state.sessions.get(&session.session_id).expect("session");
    assert_eq!(record.queued_inputs.len(), 3);
}

/// Verifies a resume rejects an unnamed subscription or unknown session.
/// 验证恢复拒绝未命名订阅或未知会话。
#[test]
fn stream_resume_rejects_an_unnamed_subscription_or_unknown_session() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    assert_eq!(
        manager
            .resume_stream(&session.session_id, "  ", None)
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert_eq!(
        manager
            .resume_stream("missing", "observer", None)
            .unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
}

/// Verifies a stream subscriber receives frames without a prior explicit subscribe call.
/// 验证流订阅者无需事先显式订阅即可收到帧。
///
/// Regression guard: a stream connection used to appear healthy while frame publication had no
/// destination for it, which is indistinguishable from a frozen picture.
/// 回归保护：流连接曾经看似正常，但帧发布对它没有投递目标——这与画面冻结无法区分。
#[test]
fn a_stream_subscriber_receives_published_frames() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    // The first resume is what a stream connection performs.
    // 首次恢复正是流连接所执行的动作。
    let initial = manager
        .resume_stream(&session.session_id, "observer-a", None)
        .expect("initial resume");
    assert_eq!(initial.mode, BrowserStreamResumeMode::Snapshot);
    assert!(initial.frames.is_empty());

    manager
        .publish_frame(
            &session.session_id,
            session.page_generation,
            "image/png",
            vec![7, 7, 7],
        )
        .expect("publish frame");

    let resumed = manager
        .resume_stream(
            &session.session_id,
            "observer-a",
            Some(BrowserStreamCursor {
                page_generation: session.page_generation,
                frame_sequence: Some(0),
            }),
        )
        .expect("resume after publish");
    assert_eq!(
        resumed.frames.len(),
        1,
        "a stream subscriber must receive frames published after it connected"
    );
    assert_eq!(resumed.frames[0].mime_type, "image/png");
}

/// Verifies a disconnecting observer releases only its own subscription.
/// 验证断开的观察端只释放它自己的订阅。
#[test]
fn a_disconnecting_observer_releases_only_its_own_subscription() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    // Two stream observers plus one explicit observer subscription.
    // 两个流观察端，外加一个显式观察订阅。
    manager
        .resume_stream(&session.session_id, "observer-a", None)
        .expect("observer a");
    manager
        .resume_stream(&session.session_id, "observer-b", None)
        .expect("observer b");
    let explicit = manager
        .subscribe_observer(&session.session_id)
        .expect("explicit observer");
    manager
        .publish_frame(
            &session.session_id,
            session.page_generation,
            "image/png",
            vec![1],
        )
        .expect("publish");

    // Observer A goes away.
    // 观察端 A 断开。
    assert!(
        manager
            .release_stream_subscription(&session.session_id, "observer-a")
            .expect("release a")
    );
    let remaining = manager
        .observation_subscription_ids(&session.session_id)
        .expect("subscriptions");
    assert!(!remaining.contains(&String::from("observer-a")));
    // The session, the other stream observer, and the explicit subscription all survive.
    // 会话、另一个流观察端与显式订阅全部存活。
    assert!(remaining.contains(&String::from("observer-b")));
    assert!(remaining.contains(&explicit));
    assert_eq!(
        manager
            .retained_frame_count(&session.session_id, "observer-b")
            .expect("count b"),
        1,
        "the surviving observer keeps its retained frames"
    );
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("projection")
            .status,
        BrowserSessionStatus::Running,
        "closing an observer must not end the session"
    );
    // Released twice is not an error, and an explicit subscription is never released this way.
    // 重复释放不是错误，且显式订阅绝不会以此方式被释放。
    assert!(
        !manager
            .release_stream_subscription(&session.session_id, "observer-a")
            .expect("release again")
    );
    assert!(
        !manager
            .release_stream_subscription(&session.session_id, &explicit)
            .expect("release explicit")
    );
}

/// Verifies cancellation during first-frame capture cannot recreate the stream on initial resume.
/// 验证首帧抓取期间发生取消时，初次恢复不能重建流。
#[test]
fn cancelled_page_bound_stream_cannot_reappear_on_initial_resume() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 4242)
        .expect("record host");
    let page = manager
        .adopt_page(&session.session_id, "target-1", "about:blank", None)
        .expect("adopt page");
    manager
        .bind_stream_page(&session.session_id, &page.page_id, "initial-stream")
        .expect("bind before capture");
    manager
        .unsubscribe_observer(&session.session_id, "initial-stream")
        .expect("cancel during capture");
    assert_eq!(
        manager
            .resume_bound_stream(&session.session_id, "initial-stream", None)
            .expect_err("cancelled initial stream must not restart"),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert!(
        !manager
            .observation_subscription_ids(&session.session_id)
            .expect("subscriptions")
            .contains(&String::from("initial-stream"))
    );
    assert_eq!(
        manager
            .bind_stream_page(&session.session_id, &page.page_id, "initial-stream")
            .expect_err("old socket still owns the cancelled identity"),
        BROWSER_ERROR_LEASE_CONFLICT
    );
    assert!(
        manager
            .release_stream_subscription(&session.session_id, "initial-stream")
            .expect("old socket releases cancelled identity")
    );
    manager
        .bind_stream_page(&session.session_id, &page.page_id, "initial-stream")
        .expect("identity can be reused after old socket releases it");
}

/// Verifies cancelled sockets continue occupying a bounded slot until their release completes.
/// 验证已取消套接字在完成释放前仍占用有界名额。
#[test]
fn cancelled_page_bound_streams_cannot_bypass_observer_limit() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 4242)
        .expect("record host");
    let page = manager
        .adopt_page(&session.session_id, "target-1", "about:blank", None)
        .expect("adopt page");
    for index in 0..BROWSER_MAX_OBSERVERS {
        let subscription_id = format!("cancelled-{index}");
        manager
            .bind_stream_page(&session.session_id, &page.page_id, &subscription_id)
            .expect("bind stream");
        manager
            .unsubscribe_observer(&session.session_id, &subscription_id)
            .expect("cancel stream");
    }
    assert_eq!(
        manager
            .subscribe_observer(&session.session_id)
            .expect_err("cancelled sockets must still count against the limit"),
        BROWSER_ERROR_OBSERVER_LIMIT
    );
    assert!(
        manager
            .release_stream_subscription(&session.session_id, "cancelled-0")
            .expect("release old socket")
    );
    manager
        .subscribe_observer(&session.session_id)
        .expect("released slot is available");
}

/// Verifies closure revokes stream tickets and active or cancelled socket identities.
/// 验证关闭会话会撤销流凭据及活动或已取消的套接字标识。
#[test]
fn closing_browser_clears_all_stream_ownership() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 4242)
        .expect("record host");
    let page = manager
        .adopt_page(&session.session_id, "target-1", "about:blank", None)
        .expect("adopt page");
    manager
        .bind_stream_page(&session.session_id, &page.page_id, "active-stream")
        .expect("bind active stream");
    manager
        .bind_stream_page(&session.session_id, &page.page_id, "cancelled-stream")
        .expect("bind stream to cancel");
    manager
        .unsubscribe_observer(&session.session_id, "cancelled-stream")
        .expect("cancel stream");
    let ticket = manager
        .issue_stream_ticket(&session.session_id)
        .expect("issue ticket before close");
    manager
        .close_session(&session.session_id)
        .expect("close session");
    assert_eq!(
        manager.consume_stream_ticket(&ticket.ticket).unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert_eq!(
        manager
            .issue_stream_ticket(&session.session_id)
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
    for subscription_id in ["active-stream", "cancelled-stream"] {
        assert!(
            !manager
                .release_stream_subscription(&session.session_id, subscription_id)
                .expect("late socket release finds no ownership")
        );
    }
}

/// Verifies a closed browser releases staged uploads and its disposable profile on disk.
/// 验证已关闭浏览器会释放磁盘上的上传暂存文件及临时画像。
#[test]
fn closing_browser_removes_temporary_files_but_preserves_persistent_profiles() {
    // The fixture root is unique and removed automatically after the test.
    // 夹具根目录唯一，并在测试结束后自动删除。
    let root = tempfile::tempdir().expect("isolated browser data root");
    // Profile and staging roots mirror the production runtime layout.
    // 画像根目录与暂存根目录对应正式运行时布局。
    let profiles_root = root.path().join("profiles");
    let staging_root = root.path().join("staging");
    // The manager owns only this isolated fixture configuration.
    // 管理器只拥有这份隔离夹具配置。
    let manager = BrowserManager::new();
    manager
        .configure_host_runtime(BrowserHostRuntimeConfig {
            executable: root.path().join("host"),
            entry: root.path().join("entry"),
            chromium_executable: root.path().join("chromium"),
            profiles_root: profiles_root.clone(),
            staging_root: staging_root.clone(),
            discovery_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(1),
        })
        .expect("configure isolated host root");
    // The ephemeral session is the only browser whose files may be removed.
    // 该临时会话是唯一允许删除文件的浏览器。
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create ephemeral browser");
    // The upload record supplies the exact on-disk file asserted below.
    // 上传记录提供下文核对的磁盘文件精确路径。
    let upload = manager
        .stage_upload(&session.session_id, "中文文件.txt", b"content".to_vec())
        .expect("stage upload");
    // These two profile paths distinguish disposable data from persistent data.
    // 两条画像路径用于区分可丢弃数据与持久数据。
    let ephemeral = profiles_root.join("ephemeral").join(&session.session_id);
    let persistent = profiles_root.join("persistent").join("keep");
    std::fs::create_dir_all(&ephemeral).expect("create disposable profile");
    std::fs::create_dir_all(&persistent).expect("create persistent profile");
    assert!(std::path::Path::new(&upload.staged_path).is_file());
    manager
        .close_session(&session.session_id)
        .expect("close browser");
    assert!(!staging_root.join(&session.session_id).exists());
    assert!(!ephemeral.exists());
    assert!(persistent.is_dir());
    assert!(
        manager
            .uploads(&session.session_id)
            .expect("list uploads")
            .is_empty()
    );
}

/// Verifies unsafe names cannot create special files and staged files have a session quota.
/// 验证不安全名称不能创建特殊文件，暂存文件也受到会话额度约束。
#[test]
fn staged_upload_rejects_unsafe_names_and_count_overflow() {
    // The disposable root keeps every staged file outside the real service data tree.
    // 可丢弃根目录使全部暂存文件远离真实服务数据树。
    let root = tempfile::tempdir().expect("isolated browser data root");
    let staging_root = root.path().join("staging");
    let manager = BrowserManager::new();
    manager
        .configure_host_runtime(BrowserHostRuntimeConfig {
            executable: root.path().join("host"),
            entry: root.path().join("entry"),
            chromium_executable: root.path().join("chromium"),
            profiles_root: root.path().join("profiles"),
            staging_root: staging_root.clone(),
            discovery_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(1),
        })
        .expect("configure isolated host root");
    // Production upload RPC holds this gate across authorization, quota, and staging.
    // 正式上传 RPC 在授权、额度校验和暂存期间持有此派发门。
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create browser");
    let gate = manager
        .dispatch_gate(&session.session_id)
        .expect("session gate");
    let dispatch = gate.lock().expect("hold upload dispatch gate");
    // Invalid Windows names must fail before creating a directory or an alternate stream.
    // 无效 Windows 名称必须在创建目录或附加数据流之前被拒绝。
    for name in [
        "file:stream.txt",
        "CON.txt",
        "COM¹.txt",
        "bad?.txt",
        "trailing.",
        "..",
    ] {
        assert_eq!(
            manager
                .stage_upload(&session.session_id, name, b"x".to_vec())
                .unwrap_err(),
            BROWSER_ERROR_INVALID_REQUEST
        );
    }
    assert!(!staging_root.join(&session.session_id).exists());
    assert!(
        manager
            .uploads(&session.session_id)
            .expect("list uploads")
            .is_empty()
    );
    for _ in 0..BROWSER_MAX_STAGED_UPLOADS_PER_SESSION {
        manager
            .stage_upload(&session.session_id, "fixture.txt", b"x".to_vec())
            .expect("stage within limit");
    }
    assert_eq!(
        manager
            .stage_upload(&session.session_id, "overflow.txt", b"x".to_vec())
            .unwrap_err(),
        BROWSER_ERROR_UPLOAD_LIMIT
    );
    assert_eq!(
        manager
            .uploads(&session.session_id)
            .expect("list uploads")
            .len(),
        BROWSER_MAX_STAGED_UPLOADS_PER_SESSION
    );
    drop(dispatch);
    manager
        .close_session(&session.session_id)
        .expect("close browser");
    assert!(!staging_root.join(&session.session_id).exists());
}

/// Verifies failed downloads vanish and completed files survive until history eviction.
/// 验证失败下载被删除，已完成文件则保留至历史记录驱逐。
#[test]
fn download_files_follow_failure_close_and_history_eviction() {
    // The isolated runtime root prevents this test from touching a real browser profile.
    // 隔离的运行时根目录使本测试不会触及真实浏览器画像。
    let root = tempfile::tempdir().expect("isolated browser data root");
    let manager = BrowserManager::new();
    manager
        .configure_host_runtime(BrowserHostRuntimeConfig {
            executable: root.path().join("host"),
            entry: root.path().join("entry"),
            chromium_executable: root.path().join("chromium"),
            profiles_root: root.path().join("profiles"),
            staging_root: root.path().join("staging"),
            discovery_timeout: Duration::from_secs(1),
            request_timeout: Duration::from_secs(1),
        })
        .expect("configure isolated host root");
    // The registered page owns both download lifecycle events.
    // 已登记页面拥有两次下载的生命周期事件。
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create browser");
    manager
        .record_host_running(&session.session_id, 4242)
        .expect("record host");
    manager
        .adopt_page(&session.session_id, "target-1", "about:blank", None)
        .expect("adopt page");
    // The completed identity and path remain available for post-close and eviction checks.
    // 已完成下载的身份与路径保留供关闭后及驱逐后的核对。
    let mut completed = None;
    for (content, status) in [
        (b"failed".as_slice(), "failed"),
        (b"kept".as_slice(), "completed"),
    ] {
        // Every host download uses a distinct Core-checked UUID and storage path.
        // 每次宿主下载都使用独立且经 Core 校验的 UUID 与存储路径。
        let download_id = Uuid::new_v4().to_string();
        let started = BrowserHostEvent {
            event_sequence: String::from("1"),
            kind: String::from("downloadStarted"),
            payload: serde_json::json!({
                "targetId": "target-1",
                "downloadId": download_id,
                "fileName": "fixture.txt"
            }),
        };
        manager
            .apply_host_event(&session.session_id, &started)
            .expect("record download start");
        let path = manager
            .download_storage_path(&session.session_id, &download_id)
            .expect("Core download path");
        std::fs::create_dir_all(path.parent().expect("download directory"))
            .expect("create download directory");
        std::fs::write(&path, content).expect("write host download");
        let finished = BrowserHostEvent {
            event_sequence: String::from("2"),
            kind: String::from("downloadFinished"),
            payload: serde_json::json!({
                "targetId": "target-1",
                "downloadId": download_id,
                "status": status,
                "byteLength": content.len()
            }),
        };
        manager
            .apply_host_event(&session.session_id, &finished)
            .expect("record download completion");
        if status == "completed" {
            completed = Some((download_id.clone(), path.clone()));
            assert!(path.is_file());
            assert_eq!(
                manager
                    .download_content(&download_id, &session.owner_session_id)
                    .expect("completed download remains readable")
                    .1,
                content
            );
        } else {
            assert!(!path.exists());
            assert_eq!(
                manager
                    .download_content(&download_id, &session.owner_session_id)
                    .unwrap_err(),
                BROWSER_ERROR_INVALID_REQUEST
            );
        }
    }
    // A closed history record still authorizes its completed download until it is evicted.
    // 已关闭的历史记录在被驱逐前仍授权读取已完成下载。
    let (completed_id, completed_path) = completed.expect("completed download identity");
    manager
        .close_session(&session.session_id)
        .expect("close downloaded browser");
    assert!(completed_path.is_file());
    assert_eq!(
        manager
            .download_content(&completed_id, &session.owner_session_id)
            .expect("closed history download remains readable")
            .1,
        b"kept"
    );
    // Retained history is bounded; the oldest control directory expires with its record.
    // 历史记录有上限；最旧记录的控制目录与记录一同到期。
    for _ in 1..BROWSER_MAX_RETAINED_CLOSED_SESSIONS {
        let later = manager
            .create_session(BrowserProfileMode::Ephemeral)
            .expect("create later browser");
        manager
            .close_session(&later.session_id)
            .expect("close later browser");
        assert!(completed_path.is_file());
    }
    // Holding the old record's read gate makes the final eviction wait after it removes the
    // metadata, so an already-started download read cannot lose its file mid-transfer.
    // 持有旧记录的读取门，使最终驱逐在删除元数据后等待，从而避免已开始的读取途中丢失文件。
    let old_gate = {
        let state = manager.inner.lock().expect("lock browser manager");
        Arc::clone(
            &state
                .sessions
                .get(&session.session_id)
                .expect("old history remains")
                .download_gate,
        )
    };
    let held_read = old_gate.lock().expect("hold old download read gate");
    let final_session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create final browser");
    let closing_manager = manager.clone();
    let closing = std::thread::spawn(move || {
        closing_manager
            .close_session(&final_session.session_id)
            .expect("close final browser");
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while manager.session_projection(&session.session_id).is_ok() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        manager.session_projection(&session.session_id).unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
    assert!(completed_path.is_file());
    drop(held_read);
    closing.join().expect("history eviction completes");
    assert!(!completed_path.exists());
    assert_eq!(
        manager
            .download_content(&completed_id, &session.owner_session_id)
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
}

/// Verifies cancelling a page-bound stream ends its next poll without restoring a frozen queue.
/// 验证取消已绑定页面的流后，下次轮询明确结束且不会恢复冻结的队列。
#[test]
fn cancelled_page_bound_stream_cannot_reappear_on_poll() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 4242)
        .expect("record host");
    let page = manager
        .adopt_page(&session.session_id, "target-a", "about:blank", None)
        .expect("adopt page");
    manager
        .bind_stream_page(&session.session_id, &page.page_id, "connection-stream")
        .expect("bind stream");
    let cursor = BrowserStreamCursor {
        page_generation: session.page_generation,
        frame_sequence: None,
    };
    manager
        .poll_bound_stream(&session.session_id, "connection-stream", cursor)
        .expect("poll live stream");
    manager
        .unsubscribe_observer(&session.session_id, "connection-stream")
        .expect("cancel stream");
    assert_eq!(
        manager
            .poll_bound_stream(&session.session_id, "connection-stream", cursor)
            .expect_err("cancelled stream must not restart"),
        BROWSER_ERROR_INVALID_REQUEST
    );
    assert!(
        !manager
            .observation_subscription_ids(&session.session_id)
            .expect("subscriptions")
            .contains(&String::from("connection-stream"))
    );
}

/// Verifies a slow observer's retained frames stay bounded and never starve the others.
/// 验证缓慢观察端保留的帧保持有界，且绝不使其他观察端挨饿。
#[test]
fn a_slow_observer_stays_bounded_and_does_not_starve_others() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .resume_stream(&session.session_id, "slow", None)
        .expect("slow observer");
    manager
        .resume_stream(&session.session_id, "fast", None)
        .expect("fast observer");

    // The slow observer never confirms a frame while the fast one keeps consuming.
    // 缓慢观察端从不确认帧，而快速观察端持续消费。
    let total = BROWSER_MAX_QUEUED_FRAMES * 3;
    let mut fast_sequence = Some(0_u64);
    for index in 0..total {
        manager
            .publish_frame(
                &session.session_id,
                session.page_generation,
                "image/png",
                vec![index as u8],
            )
            .expect("publish");
        if let Some(sequence) = fast_sequence {
            let resumed = manager
                .resume_stream(
                    &session.session_id,
                    "fast",
                    Some(BrowserStreamCursor {
                        page_generation: session.page_generation,
                        frame_sequence: Some(sequence),
                    }),
                )
                .expect("fast resume");
            assert_eq!(
                resumed.mode,
                BrowserStreamResumeMode::Incremental,
                "the fast observer must keep resuming incrementally"
            );
            fast_sequence = resumed
                .frames
                .last()
                .and_then(|frame| frame.sequence.parse::<u64>().ok())
                .or(fast_sequence);
        }
    }

    // The stalled observer's memory is bounded, not proportional to what it missed.
    // 停滞观察端的内存是有界的，而不与其错过的帧数成正比。
    let slow_retained = manager
        .retained_frame_count(&session.session_id, "slow")
        .expect("slow count");
    assert!(
        slow_retained <= BROWSER_MAX_QUEUED_FRAMES,
        "a slow observer retained {slow_retained} frames, above the {BROWSER_MAX_QUEUED_FRAMES} bound"
    );
    // Its gap is reported rather than hidden, so the client knows it must resynchronise.
    // 其缺口被如实报告而不是被隐藏，因此客户端知道自己必须重新同步。
    let stalled = manager
        .resume_stream(
            &session.session_id,
            "slow",
            Some(BrowserStreamCursor {
                page_generation: session.page_generation,
                frame_sequence: Some(1),
            }),
        )
        .expect("slow resume");
    assert_eq!(stalled.mode, BrowserStreamResumeMode::Snapshot);
    assert!(
        stalled.dropped_frames > 0,
        "an evicted gap must be reported as dropped frames"
    );
}

/// Verifies a created session starts with no supervised host and no pages.
/// 验证新建会话没有受监督宿主也没有页面。
#[test]
fn created_sessions_begin_without_a_host_or_pages() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    assert_eq!(session.host_state, BrowserHostSupervisionState::NotStarted);
    assert_eq!(session.host_generation, 0);
    assert_eq!(session.page_count, 0);
    assert!(!session.requires_explicit_reopen);
    assert!(
        manager
            .list_pages(&session.session_id)
            .expect("list pages")
            .is_empty()
    );
}

/// Verifies host page events register real targets as Core pages.
/// 验证宿主页面事件把真实目标登记为 Core 页面。
#[test]
fn host_page_events_register_real_targets_as_pages() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");

    // Events arrive from the host exactly as the real host reports them.
    // 事件按真实宿主的报告方式到达。
    emit_fake_host_events(
        &manager,
        &session.session_id,
        vec![
            (
                "pageCreated",
                json!({ "targetId": "11", "url": "https://example.test/one", "openerTargetId": null }),
            ),
            (
                "pageCreated",
                json!({ "targetId": "12", "url": "https://example.test/one", "openerTargetId": "11" }),
            ),
        ],
    );

    let pages = manager.list_pages(&session.session_id).expect("list pages");
    assert_eq!(pages.len(), 2, "both targets must become Core pages");
    // Two pages showing the same URL must still be two distinct identities.
    // 显示同一地址的两个页面仍必须是两个不同标识。
    assert_ne!(pages[0].page_id, pages[1].page_id);
    let popup = pages
        .iter()
        .find(|page| page.target_id == "12")
        .expect("popup page");
    let opener = pages
        .iter()
        .find(|page| page.target_id == "11")
        .expect("opener page");
    assert_eq!(
        popup.opener_page_id.as_deref(),
        Some(opener.page_id.as_str()),
        "a page opened by another page must record its Core opener identity"
    );
}

/// Verifies a navigation event refreshes the URL without changing the page identity.
/// 验证导航事件刷新地址但不改变页面标识。
#[test]
fn host_navigation_events_keep_the_page_identity() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");
    emit_fake_host_events(
        &manager,
        &session.session_id,
        vec![(
            "pageCreated",
            json!({ "targetId": "21", "url": "https://example.test/first" }),
        )],
    );
    let before = manager.list_pages(&session.session_id).expect("pages")[0].clone();
    emit_fake_host_events(
        &manager,
        &session.session_id,
        vec![(
            "pageNavigated",
            json!({ "targetId": "21", "url": "https://example.test/second" }),
        )],
    );
    let after = manager.list_pages(&session.session_id).expect("pages");
    assert_eq!(after.len(), 1, "navigation must not create a second page");
    assert_eq!(after[0].page_id, before.page_id);
    assert_eq!(after[0].url, "https://example.test/second");
    // Title changes on the same URL (including SPA updates) keep the exact page identity.
    // 同地址的标题变化（包括 SPA 更新）保持精确页面身份。
    emit_fake_host_events(
        &manager,
        &session.session_id,
        vec![(
            "pageMetadataChanged",
            json!({
                "targetId": "21", "url": "https://example.test/second", "title": "Updated SPA title"
            }),
        )],
    );
    let titled = manager
        .list_pages(&session.session_id)
        .expect("titled pages");
    assert_eq!(titled[0].page_id, before.page_id);
    assert_eq!(titled[0].title.as_deref(), Some("Updated SPA title"));
}

/// Verifies unknown host event kinds are ignored rather than treated as failures.
/// 验证未知宿主事件类型被忽略而不是被当作失败。
#[test]
fn unknown_host_events_are_ignored() {
    let (manager, _alive) = manager_with_fake_host(Duration::from_millis(0));
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager.start_host(&session.session_id).expect("start host");
    emit_fake_host_events(
        &manager,
        &session.session_id,
        vec![("futureEventKind", json!({ "anything": true }))],
    );
    assert!(
        manager
            .list_pages(&session.session_id)
            .expect("pages")
            .is_empty()
    );
}

/// Writes host events into one session's control channel from the test side.
/// 从测试侧把宿主事件写入某个会话的控制通道。
fn emit_fake_host_events(
    manager: &BrowserManager,
    session_id: &str,
    events: Vec<(&str, serde_json::Value)>,
) {
    let ownership = manager.owned_host(session_id).expect("owned host");
    let mut ownership = ownership.lock().expect("lock host");
    for (kind, payload) in events {
        ownership
            .connection
            .push_test_event(kind, payload)
            .expect("push host event");
    }
}

/// Verifies a confirmed host owns a generation and its pages are registered under it.
/// 验证已确认宿主拥有代际，其页面在该代际下登记。
#[test]
fn a_confirmed_host_owns_the_generation_and_its_pages() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");

    let running = manager
        .record_host_running(&session.session_id, 4242)
        .expect("record running");
    assert_eq!(running.host_state, BrowserHostSupervisionState::Running);
    assert_eq!(running.host_generation, 1);

    let first = manager
        .adopt_page(
            &session.session_id,
            "target-a",
            "https://example.test/one",
            None,
        )
        .expect("adopt first page");
    // A page opened by an existing page records its opener.
    // 由既有页面打开的新页面会记录其打开者。
    let opened = manager
        .adopt_page(
            &session.session_id,
            "target-b",
            "https://example.test/two",
            Some(&first.page_id),
        )
        .expect("adopt opened page");
    assert_eq!(
        opened.opener_page_id.as_deref(),
        Some(first.page_id.as_str())
    );

    assert_eq!(
        manager
            .list_pages(&session.session_id)
            .expect("list pages")
            .len(),
        2
    );
    let projection = manager
        .session_projection(&session.session_id)
        .expect("projection");
    assert_eq!(projection.page_count, 2);
}

/// Verifies a navigation keeps the page identity while its URL changes.
/// 验证导航保留页面标识而地址变化。
#[test]
fn navigation_keeps_the_core_page_identity() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 1)
        .expect("record running");
    let page = manager
        .adopt_page(
            &session.session_id,
            "target-a",
            "https://example.test/first",
            None,
        )
        .expect("adopt page");
    let again = manager
        .adopt_page(
            &session.session_id,
            "target-a",
            "https://example.test/second",
            None,
        )
        .expect("re-adopt page");
    assert_eq!(again.page_id, page.page_id);
    assert_eq!(again.url, "https://example.test/second");
    assert_eq!(
        manager
            .list_pages(&session.session_id)
            .expect("list pages")
            .len(),
        1
    );
}

/// Verifies two sessions never share page identity or host state.
/// 验证两个会话绝不共享页面标识或宿主状态。
#[test]
fn two_sessions_do_not_share_pages_or_host_state() {
    let manager = BrowserManager::new();
    let first = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create first session");
    let second = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create second session");
    manager
        .record_host_running(&first.session_id, 11)
        .expect("first host");
    manager
        .record_host_running(&second.session_id, 22)
        .expect("second host");
    manager
        .adopt_page(&first.session_id, "target-a", "https://example.test/", None)
        .expect("first page");

    assert_eq!(
        manager
            .list_pages(&first.session_id)
            .expect("first pages")
            .len(),
        1
    );
    assert!(
        manager
            .list_pages(&second.session_id)
            .expect("second pages")
            .is_empty(),
        "one session must never expose another session's pages"
    );
}

/// Verifies a host crash invalidates page identity and demands an explicit reopen.
/// 验证宿主崩溃使页面标识失效并要求明确重开。
#[test]
fn a_crashed_host_invalidates_pages_and_requires_an_explicit_reopen() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 77)
        .expect("record running");
    manager
        .adopt_page(
            &session.session_id,
            "target-a",
            "https://example.test/",
            None,
        )
        .expect("adopt page");

    let crashed = manager
        .record_host_crash(&session.session_id, Some(9), "host exited unexpectedly")
        .expect("record crash");
    assert_eq!(crashed.status, BrowserSessionStatus::Crashed);
    assert_eq!(crashed.host_state, BrowserHostSupervisionState::Crashed);
    assert_eq!(crashed.page_count, 0);
    assert!(crashed.requires_explicit_reopen);
    assert!(
        manager
            .list_pages(&session.session_id)
            .expect("pages")
            .is_empty(),
        "a crashed generation must not keep its page identities"
    );
    let summary = manager
        .host_supervision_summary(&session.session_id)
        .expect("summary");
    assert_eq!(summary["state"], "crashed");
    assert_eq!(summary["crash"]["exitCode"], 9);
    assert_eq!(summary["requiresExplicitReopen"], true);
}

/// Verifies an explicit close ends the context without looking like a crash.
/// 验证显式结束上下文且不表现为崩溃。
#[test]
fn an_explicit_close_is_not_a_crash() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 5)
        .expect("record running");
    manager
        .adopt_page(
            &session.session_id,
            "target-a",
            "https://example.test/",
            None,
        )
        .expect("adopt page");
    let closed = manager
        .close_session(&session.session_id)
        .expect("close session");
    assert_eq!(closed.status, BrowserSessionStatus::Closed);
    assert_eq!(closed.host_state, BrowserHostSupervisionState::Closed);
    assert!(!closed.requires_explicit_reopen);
    let summary = manager
        .host_supervision_summary(&session.session_id)
        .expect("summary");
    assert_eq!(summary["state"], "closed");
    assert_eq!(summary["crash"], serde_json::Value::Null);
}

/// Verifies supervision operations reject an unknown session instead of inventing one.
/// 验证监督操作拒绝未知会话，而不是凭空创建一个。
#[test]
fn supervision_operations_reject_an_unknown_session() {
    let manager = BrowserManager::new();
    assert_eq!(
        manager.record_host_running("missing", 1).unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
    assert_eq!(
        manager
            .adopt_page("missing", "target-a", "https://example.test/", None)
            .unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
    assert_eq!(
        manager.list_pages("missing").unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
    assert_eq!(
        manager.host_supervision_summary("missing").unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
    // An empty target identity is a request error, not a new page.
    // 空目标标识是请求错误，而不是新页面。
    let session = manager
        .create_session(BrowserProfileMode::Persistent)
        .expect("create session");
    manager
        .record_host_running(&session.session_id, 1)
        .expect("record running");
    assert_eq!(
        manager
            .adopt_page(&session.session_id, "  ", "https://example.test/", None)
            .unwrap_err(),
        BROWSER_ERROR_INVALID_REQUEST
    );
}

/// Builds one shared service state carrying an empty browser manager.
/// 构建一个携带空浏览器管理器的共享服务状态。
fn browser_test_state() -> SharedServiceBridgeState {
    Arc::new(Mutex::new(test_service_state_with_parent_session(
        SessionId::new_v4(),
    )))
}

/// Verifies session creation assigns a running ephemeral session.
/// 验证会话创建分配一个运行中的临时会话。
#[test]
fn browser_session_create_assigns_running_session() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    assert_eq!(session.status, BrowserSessionStatus::Running);
    assert_eq!(session.page_generation, 1);
    assert_eq!(session.control_epoch, 1);
}

/// Verifies the lease lifecycle rejects a second holder.
/// 验证租约生命周期拒绝第二个持有者。
#[test]
fn browser_lease_rejects_second_holder() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (_, secret) = manager
        .acquire_lease(&session.session_id, "user", None)
        .expect("acquire lease");
    assert!(secret.trim().len() > 8);
    assert!(
        manager
            .acquire_lease(&session.session_id, "other", None)
            .is_err()
    );
    manager
        .release_lease(&session.session_id, &secret)
        .expect("release lease");
    assert!(
        manager
            .acquire_lease(&session.session_id, "other", None)
            .is_ok()
    );
}

/// Verifies stale page generations are rejected for input dispatch.
/// 验证输入派发拒绝过期页面代次。
#[test]
fn browser_input_rejects_stale_generation() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let (_, secret) = manager
        .acquire_lease(&session.session_id, "user", None)
        .expect("acquire lease");
    let event = BrowserInputEvent {
        sequence: String::from("1"),
        kind: String::from("pointer"),
        payload: serde_json::json!({}),
    };
    assert!(
        manager
            .dispatch_input(
                &session.session_id,
                &secret,
                session.page_generation + 1,
                event
            )
            .is_err()
    );
}

/// Verifies automation attach requires the lease secret.
/// 验证自动化挂接要求租约凭据。
#[test]
fn browser_automation_requires_lease_secret() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    assert!(
        manager
            .attach_automation(&session.session_id, "wrong-secret", "automation-1")
            .is_err()
    );
}

/// Verifies observation subscriptions are isolated per subscriber.
/// 验证观察订阅按订阅者隔离。
#[test]
fn browser_observation_isolated_per_subscriber() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let first = manager
        .subscribe_observer(&session.session_id)
        .expect("subscribe first");
    let second = manager
        .subscribe_observer(&session.session_id)
        .expect("subscribe second");
    assert_ne!(first, second);
    manager
        .publish_frame(
            &session.session_id,
            session.page_generation,
            "image/jpeg",
            vec![1, 2, 3],
        )
        .expect("publish frame");
    manager
        .unsubscribe_observer(&session.session_id, &first)
        .expect("unsubscribe first");
}

/// Verifies monotonic browser counters use decimal text across the JS boundary.
/// 验证单调浏览器计数器跨 JS 边界使用十进制文本。
#[test]
fn browser_counters_serialize_as_decimal_text() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let value = serde_json::to_value(&session).expect("serialize projection");
    assert_eq!(value.get("pageGeneration"), Some(&serde_json::json!("1")));
    assert_eq!(value.get("controlEpoch"), Some(&serde_json::json!("1")));
    let decoded: BrowserSessionProjection =
        serde_json::from_value(value).expect("decode projection");
    assert_eq!(decoded, session);
}

/// Verifies detailed status reports draining admission before bootstrap phases.
/// 验证详细状态在引导阶段之前报告排空准入。
#[test]
fn browser_status_snapshot_reports_stopping_during_drain() {
    let state = browser_test_state();
    let stopping = crate::service_status_snapshot_from_state(&state);
    assert_eq!(
        stopping.lifecycle,
        vulcan_service_api::ServiceLifecycle::Ready
    );
    let _handles = state
        .lock()
        .expect("lock isolated service state")
        .session_runtime_registry
        .begin_shutdown()
        .expect("begin shutdown");
    {
        state
            .lock()
            .expect("lock isolated service state")
            .service_lifecycle_state
            .begin_shutdown_drain();
    }
    let snapshot = crate::service_status_snapshot_from_state(&state);
    assert_eq!(
        snapshot.lifecycle,
        vulcan_service_api::ServiceLifecycle::Stopping
    );
    assert_eq!(snapshot.active_work.active_stream_tasks, 0);
    assert_eq!(snapshot.active_work.vce_jobs, 0);
    assert_eq!(snapshot.active_work.running_terminals, 0);
    crate::service_resume_after_failed_app_shutdown(&state).expect("resume shutdown");
    let recovered = crate::service_status_snapshot_from_state(&state);
    assert_eq!(
        recovered.lifecycle,
        vulcan_service_api::ServiceLifecycle::Ready
    );
    assert!(recovered.last_failure.is_none());
}

/// Verifies state-backed session creation flows through shared service state.
/// 验证经共享服务状态的会话创建流程。
#[tokio::test]
async fn browser_state_backed_session_create_lists() {
    // The state-backed path must own a background host, so the shared manager is replaced by
    // one wired to a deterministic in-process host instead of a desktop runtime.
    // 经状态的路径必须拥有后台宿主，因此共享管理器被替换为连接进程内确定性宿主的管理器，
    // 而不是连接桌面运行时。
    let state = browser_test_state();
    let alive = Arc::new(AtomicBool::new(true));
    let host = FakeBrowserHost::start(Duration::from_millis(0), Arc::clone(&alive));
    let profiles_root = std::env::temp_dir().join(format!("vulcan-w06-state-{}", Uuid::new_v4()));
    {
        let mut guard = state.lock().expect("lock test state");
        guard.browser_manager =
            BrowserManager::with_host_spawner(Arc::new(FakeBrowserHostSpawner {
                address: host.address,
                alive,
                poll_error: Arc::new(AtomicBool::new(false)),
            }));
        guard
            .browser_manager
            .configure_host_runtime(BrowserHostRuntimeConfig {
                executable: std::path::PathBuf::from("fake-electron.exe"),
                entry: std::path::PathBuf::from("fake-host.js"),
                chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
                profiles_root: profiles_root.clone(),
                staging_root: profiles_root.join("staging"),
                discovery_timeout: Duration::from_secs(10),
                request_timeout: Duration::from_secs(5),
            })
            .expect("configure host runtime");
    }
    let created = service_browser_session_create(
        state.clone(),
        BrowserSessionCreateRequest { profile_mode: None },
        String::from("conversation-a"),
    )
    .await
    .expect("create through state");
    // A session handed to a client always names the host that owns it.
    // 交付给客户端的会话总是标明拥有它的宿主。
    assert_eq!(created.host_state, BrowserHostSupervisionState::Running);
    assert_eq!(created.host_generation, 1);
    let listed = service_browser_session_list(state, String::from("conversation-a"))
        .await
        .expect("list through state");
    assert!(
        listed
            .iter()
            .any(|session| session.session_id == created.session_id)
    );
}

/// Verifies a slow host handshake does not stop an unrelated Core timer on one Tokio worker.
/// 验证宿主握手变慢时，单线程 Tokio 上不相关的 Core 定时器仍能按时运行。
#[tokio::test(flavor = "current_thread")]
async fn browser_session_create_does_not_block_async_reactor() {
    let state = browser_test_state();
    let alive = Arc::new(AtomicBool::new(true));
    let host = FakeBrowserHost::start(Duration::from_millis(350), Arc::clone(&alive));
    let profiles_root =
        std::env::temp_dir().join(format!("vulcan-browser-nonblocking-{}", Uuid::new_v4()));
    {
        let mut guard = state.lock().expect("lock test state");
        guard.browser_manager =
            BrowserManager::with_host_spawner(Arc::new(FakeBrowserHostSpawner {
                address: host.address,
                alive,
                poll_error: Arc::new(AtomicBool::new(false)),
            }));
        guard
            .browser_manager
            .configure_host_runtime(BrowserHostRuntimeConfig {
                executable: std::path::PathBuf::from("fake-electron.exe"),
                entry: std::path::PathBuf::from("fake-host.js"),
                chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
                profiles_root: profiles_root.clone(),
                staging_root: profiles_root.join("staging"),
                discovery_timeout: Duration::from_secs(10),
                request_timeout: Duration::from_secs(5),
            })
            .expect("configure fake runtime");
    }
    let create = service_browser_session_create(
        state.clone(),
        BrowserSessionCreateRequest { profile_mode: None },
        String::from("conversation-a"),
    );
    tokio::pin!(create);
    let started = Instant::now();
    tokio::select! {
        biased;
        result = &mut create => panic!("slow host completed before timer: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
    }
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "host startup blocked the single Tokio worker"
    );
    let created = create.await.expect("host eventually starts");
    assert_eq!(created.host_state, BrowserHostSupervisionState::Running);
    let manager = browser_manager_from_state(&state).expect("read browser manager");
    manager
        .close_session(&created.session_id)
        .expect("close fake browser");
    if profiles_root.exists() {
        std::fs::remove_dir_all(&profiles_root).expect("remove isolated profiles");
    }
}

/// Verifies a cancelled create request cannot leave an unclaimed running Core browser.
/// 验证创建请求被取消后不会留下客户端无法认领的运行中 Core 浏览器。
#[tokio::test(flavor = "current_thread")]
async fn browser_session_create_cancellation_closes_unclaimed_host() {
    let state = browser_test_state();
    let alive = Arc::new(AtomicBool::new(true));
    let host = FakeBrowserHost::start(Duration::from_millis(350), Arc::clone(&alive));
    let profiles_root =
        std::env::temp_dir().join(format!("vulcan-browser-cancel-{}", Uuid::new_v4()));
    let manager = BrowserManager::with_host_spawner(Arc::new(FakeBrowserHostSpawner {
        address: host.address,
        alive,
        poll_error: Arc::new(AtomicBool::new(false)),
    }));
    manager
        .configure_host_runtime(BrowserHostRuntimeConfig {
            executable: std::path::PathBuf::from("fake-electron.exe"),
            entry: std::path::PathBuf::from("fake-host.js"),
            chromium_executable: std::path::PathBuf::from("fake-chromium.exe"),
            profiles_root: profiles_root.clone(),
            staging_root: profiles_root.join("staging"),
            discovery_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(5),
        })
        .expect("configure fake runtime");
    state.lock().expect("lock test state").browser_manager = manager.clone();
    let owner = String::from("cancelled-conversation");
    let create = tokio::spawn(service_browser_session_create(
        state,
        BrowserSessionCreateRequest { profile_mode: None },
        owner.clone(),
    ));
    let browser_id = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(session) = manager
                .list_owned_sessions(&owner)
                .expect("list test sessions")
                .first()
            {
                break session.session_id.clone();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("creation reservation appears");
    create.abort();
    assert!(
        create
            .await
            .expect_err("create task was cancelled")
            .is_cancelled()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let settled = loop {
        let session = manager
            .session_projection(&browser_id)
            .expect("read cancelled browser");
        if session.status == BrowserSessionStatus::Closed || Instant::now() >= deadline {
            break session;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    if settled.status != BrowserSessionStatus::Closed {
        manager
            .close_session(&browser_id)
            .expect("clean up leaked host");
    }
    if profiles_root.exists() {
        std::fs::remove_dir_all(&profiles_root).expect("remove isolated profiles");
    }
    assert_eq!(settled.status, BrowserSessionStatus::Closed);
}

/// Verifies cancellation of an in-flight host write blocks another controller's takeover.
/// 验证在途宿主写入取消后会阻止另一控制者接管。
#[tokio::test(flavor = "current_thread")]
async fn cancelled_blocking_host_write_blocks_the_session() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    manager
        .acquire_lease(&session.session_id, "first", None)
        .expect("acquire lease");
    let session_id = session.session_id.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let manager_for_task = manager.clone();
    let task = tokio::spawn(async move {
        manager_for_task
            .run_blocking_host_write(&session_id, "test.write", move |_| {
                started_tx.send(()).expect("report started work");
                release_rx
                    .recv_timeout(Duration::from_secs(3))
                    .expect("release blocked work");
                Ok(())
            })
            .await
    });
    started_rx.await.expect("worker began write");
    task.abort();
    assert!(task.await.expect_err("cancel task").is_cancelled());
    assert_eq!(
        manager
            .unconfirmed_operations(&session.session_id)
            .expect("read uncertain work"),
        vec![String::from("test.write")]
    );
    assert_eq!(
        manager
            .session_projection(&session.session_id)
            .expect("read projection")
            .lease_state,
        BrowserLeaseState::Blocked
    );
    assert!(
        manager
            .takeover_lease(&session.session_id, "second", None)
            .is_err()
    );
    release_tx.send(()).expect("finish worker");
}

/// Verifies cancellation while waiting for admission never dispatches or blocks a host write.
/// 验证等待准入期间取消不会派发或阻塞宿主写入。
#[tokio::test(flavor = "current_thread")]
async fn cancelled_queued_host_write_does_not_block_the_session() {
    let manager = BrowserManager::new();
    let session = manager
        .create_session(BrowserProfileMode::Ephemeral)
        .expect("create session");
    let held_permits = Arc::clone(&manager.blocking_gate)
        .acquire_many_owned(BROWSER_MAX_BLOCKING_CALLS as u32)
        .await
        .expect("reserve all worker permits");
    let session_id = session.session_id.clone();
    let dispatched = Arc::new(AtomicBool::new(false));
    let dispatched_in_worker = Arc::clone(&dispatched);
    let manager_for_task = manager.clone();
    let task = tokio::spawn(async move {
        manager_for_task
            .run_blocking_host_write(&session_id, "test.queued", move |_| {
                dispatched_in_worker.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .await
    });
    tokio::task::yield_now().await;
    task.abort();
    assert!(task.await.expect_err("cancel task").is_cancelled());
    drop(held_permits);
    tokio::task::yield_now().await;
    assert!(!dispatched.load(std::sync::atomic::Ordering::SeqCst));
    assert!(
        manager
            .unconfirmed_operations(&session.session_id)
            .expect("read uncertain work")
            .is_empty()
    );
}

/// Verifies the state-backed path refuses to hand out a session with no background browser.
/// 验证经状态的路径拒绝交付没有后台浏览器的会话。
#[tokio::test]
async fn browser_state_backed_session_create_requires_a_host_runtime() {
    let state = browser_test_state();
    let outcome = service_browser_session_create(
        state.clone(),
        BrowserSessionCreateRequest { profile_mode: None },
        String::from("conversation-a"),
    )
    .await;
    assert_eq!(outcome.unwrap_err(), BROWSER_ERROR_CAPABILITY_UNSUPPORTED);
    // The manager retains closed sessions as history, so the real property is that the refused
    // creation left no live session a client could try to use.
    // 管理器会把已关闭会话作为历史保留，因此真正的性质是：被拒绝的创建没有留下任何客户端可能
    // 试图使用的活动会话。
    assert!(
        service_browser_session_list(state, String::from("conversation-a"))
            .await
            .expect("list through state")
            .iter()
            .all(|session| session.status == BrowserSessionStatus::Closed),
        "a refused creation must not leave a live session"
    );
}

/// Verifies one conversation cannot enumerate or address another conversation's Core browser.
/// 验证一个会话不能枚举或访问另一个会话的 Core 浏览器。
#[test]
fn browser_manager_rejects_cross_conversation_access() {
    // manager owns both browsers, making ownership the only distinguishing boundary.
    // manager 持有两个浏览器，使归属成为唯一区分边界。
    let manager = BrowserManager::new();
    // first belongs only to conversation A.
    // first 只归属会话 A。
    let first = manager
        .create_owned_session("conversation-a", BrowserProfileMode::Ephemeral)
        .expect("create first browser");
    // second belongs only to conversation B.
    // second 只归属会话 B。
    let second = manager
        .create_owned_session("conversation-b", BrowserProfileMode::Ephemeral)
        .expect("create second browser");
    assert_eq!(
        manager
            .list_owned_sessions("conversation-a")
            .expect("list A"),
        vec![first.clone()]
    );
    assert_eq!(
        manager
            .list_owned_sessions("conversation-b")
            .expect("list B"),
        vec![second]
    );
    assert!(
        manager
            .require_owner(&first.session_id, "conversation-a")
            .is_ok()
    );
    assert_eq!(
        manager
            .require_owner(&first.session_id, "conversation-b")
            .unwrap_err(),
        BROWSER_ERROR_UNKNOWN_SESSION
    );
}
