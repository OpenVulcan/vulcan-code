//! Real isolated service ownership through the concrete tray shutdown worker.
//! 经具体托盘关闭工作器验证真实隔离服务所有权。

use super::{HeadlessService, TrayEvent, run_shutdown_worker};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// Exact child root marker; only this dedicated child installs process-global ownership.
/// 精确子进程根标记；仅此专用子进程安装进程级所有权。
const CHILD_ROOT_ENV: &str = "VULCAN_TRAY_SHUTDOWN_CHILD_ROOT";
/// One deadline bounds the owned child and its concrete worker receipt.
/// 一个截止时间约束自有子进程及其具体工作器回执。
const CHILD_TIMEOUT: Duration = Duration::from_secs(30);
/// Shared polling interval for the owned child's process status.
/// 自有子进程状态使用的共享轮询间隔。
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Run normal and first-failure cases in distinct owned child processes; no inputs or return value.
/// 在分别拥有的子进程中运行正常及首次失败场景；无参数或返回值。
/// The child uses the production worker, checks terminal output before exit, and never invokes Drop.
/// 子进程使用生产工作器，退出前检查终态输出，且不执行 Drop。
#[test]
fn real_owned_worker_finishes_capture_before_completion() {
    // The exact root passed only to this child case, before any service path is read.
    // 在读取任何服务路径之前，仅向此子进程场景传入精确根。
    if let Some(root) = std::env::var_os(CHILD_ROOT_ENV) {
        run_child(&PathBuf::from(root));
        // Exit while the real service Arc is retained; Drop cannot manufacture the missing receipt.
        // 在保留真实服务 Arc 时退出；Drop 无法补造缺失回执。
        unreachable!("run_child exits while the service owner is alive");
    }
    run_owned_child(false);
    run_owned_child(true);
}

/// Start one dedicated child with real test-support root/layout contracts; return after assertions.
/// 按真实测试支持根及布局契约启动一个专用子进程；断言后返回。
/// `fail_first` selects the existing one-shot pre-coordinator failure injection.
/// `fail_first` 选择既有协调器前一次性失败注入。
fn run_owned_child(fail_first: bool) {
    // Preserve the original child root and logs even when an assertion fails; never erase failure evidence.
    // 即使断言失败也保留原子进程根及日志；绝不抹除失败证据。
    let root = tempfile::tempdir()
        .expect("create owned tray worker root")
        .keep();
    println!("OWNED_TRAY_WORKER_ROOT={}", root.display());
    // Explicit minimal layout whose only required startup asset is the declared WebUI directory.
    // 显式最小布局，其唯一必需启动资源是已声明的 WebUI 目录。
    let layout = prepare_layout(&root);
    // Entire child output is captured directly; native failure classification must remain observable.
    // 直接捕获全部子进程输出；原生失败分类必须保持可观测。
    let stdout_path = root.join("worker.stdout.log");
    // Independent synchronous stderr evidence survives a failure of the service JSON writer.
    // 独立同步 stderr 证据可在服务 JSON 写入器失败时保留。
    let stderr_path = root.join("worker.stderr.log");
    // Reserve an OS-selected nonzero loopback port; the service override rejects zero as an override.
    // 预留操作系统分配的非零环回端口；服务覆盖不接受零作为覆盖值。
    let port_probe = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("reserve isolated nonzero service port");
    // Read the actual address, without guessing a free port or falling back to the service range.
    // 读取真实地址，不猜测空闲端口，也不退回服务端口范围。
    let port = port_probe
        .local_addr()
        .expect("read reserved loopback port")
        .port();
    println!("OWNED_TRAY_WORKER_PORT={port}");
    // This exact test executable avoids running or signaling an existing user service.
    // 此精确测试程序避免运行或发送信号给既有用户服务。
    let mut command = Command::new(std::env::current_exe().expect("locate exact test executable"));
    command
        .args([
            "--exact",
            "tray::shutdown_tests::real_owned_worker_finishes_capture_before_completion",
            "--nocapture",
        ])
        .env(CHILD_ROOT_ENV, &root)
        .env(crate::runtime_layout::RUNTIME_LAYOUT_ENV, &layout)
        .env(vulcan_service_api::WEBUI_SERVICE_PORT_ENV, port.to_string())
        .env("VULCAN_TEST_HOLD_STARTING", "1")
        .env(
            "RUST_LOG",
            "info,vulcan_code_service::native_diagnostics=info",
        )
        .env_remove("VULCAN_TEST_FAIL_FIRST_HEADLESS_SHUTDOWN")
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout_path).expect("capture owned stdout"))
        .stderr(fs::File::create(&stderr_path).expect("capture owned stderr"));
    if fail_first {
        command.env("VULCAN_TEST_FAIL_FIRST_HEADLESS_SHUTDOWN", "1");
    }
    // The sole owned process handle used for polling and deadline cleanup.
    // 用于轮询及截止清理的唯一自有进程句柄。
    // Release only the private port probe immediately before the real child binds its declared port.
    // 真实子进程绑定已声明端口之前，仅释放私有端口探针。
    // A competing bind fails this fixture explicitly; the nonzero override has no fallback range.
    // 竞争绑定会让夹具显式失败；非零覆盖没有备用端口范围。
    drop(port_probe);
    let mut child = command.spawn().expect("start isolated tray worker child");
    // One deadline derives from the shared child budget.
    // 从共享子进程预算派生的唯一截止时间。
    let deadline = Instant::now() + CHILD_TIMEOUT;
    // Actual exit status, observed without waiting indefinitely on a pipe.
    // 不在管道上无限等待而观测到的真实退出状态。
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll owned child") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop only timed-out owned test child");
            child.wait().expect("reap timed-out owned test child");
            panic!("owned tray worker child exceeded its deadline");
        }
        std::thread::sleep(CHILD_POLL_INTERVAL);
    };
    // Full stderr is required because a terminal checkpoint precedes the owner's final flush ack.
    // 完整 stderr 是必需证据，因为终态检查点早于所有者最终刷新回执。
    let stderr = fs::read_to_string(&stderr_path).expect("read complete owned stderr");
    assert!(
        status.success(),
        "owned tray worker child failed ({status}): {stderr}"
    );
    assert!(
        !stderr.contains("native_capture_incomplete") && !stderr.contains("panicked"),
        "completion must have no synchronous capture failure or panic: {stderr}"
    );
}

/// Create a typed schema-two layout below `root`; return its exact manifest path.
/// 在 `root` 下创建类型化第二版布局；返回其精确清单路径。
/// Browser/client entries are declared fixture paths and never executed by this worker-only test.
/// 浏览器及客户端条目是已声明夹具路径，此仅工作器测试绝不执行它们。
fn prepare_layout(root: &Path) -> PathBuf {
    // The declared WebUI root must actually exist for HeadlessService.start.
    // HeadlessService.start 要求已声明的 WebUI 根真实存在。
    let webui_root = root.join("webui");
    fs::create_dir(&webui_root).expect("create declared WebUI directory");
    // Fields follow the actual RuntimeLayoutManifest rather than guessing alternate asset locations.
    // 字段遵循真实 RuntimeLayoutManifest，而非猜测其他资源位置。
    let manifest = crate::runtime_layout::RuntimeLayoutManifest {
        schema_version: crate::runtime_layout::RUNTIME_LAYOUT_SCHEMA_VERSION,
        profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        service_executable: "service-fixture.exe".into(),
        desktop_executable: "desktop-fixture.exe".into(),
        desktop_entry: "desktop-fixture.cjs".into(),
        tui_executable: "tui-fixture.exe".into(),
        browser_host_entry: "browser-fixture.cjs".into(),
        browser_host_executable: "browser-fixture.exe".into(),
        chromium_executable: None,
        webui_root: "webui".into(),
        runtime_version: "owned-worker-fixture".into(),
        platform: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        capabilities: Vec::new(),
    };
    // One explicit manifest path avoids the normal executable/repository candidate selection.
    // 唯一显式清单路径避免正常程序及仓库候选选择。
    let path = root.join(crate::runtime_layout::RUNTIME_LAYOUT_FILENAME);
    fs::write(
        &path,
        serde_json::to_vec(&manifest).expect("serialize actual layout schema"),
    )
    .expect("write explicit isolated layout");
    path
}

/// Install `root`, retain the real owner through completion assertions, and explicitly exit.
/// 安装 `root`，保留真实所有者至完成断言结束，并显式退出。
/// No normal return occurs; existing bootstrap hold keeps this focused test before asset preparation.
/// 不正常返回；既有引导暂停让此定向测试处于资源准备之前。
fn run_child(root: &Path) {
    // Existing argument entry installs the canonical namespace before any service configuration access.
    // 既有参数入口在任何服务配置访问之前安装规范命名空间。
    let args = vec![
        "owned-tray-worker".to_owned(),
        format!("{}={}", crate::launch::ARG_TEST_ROOT, root.display()),
    ];
    assert!(
        crate::service_runtime::install_isolated_test_root_from_args(&args)
            .expect("install actual test-support root")
    );
    // The real service remains strongly owned across failed and successful worker calls.
    // 真实服务在失败及成功工作器调用之间持续保持强所有权。
    let service = Arc::new(
        HeadlessService::start()
            .expect("start actual owned service")
            .expect("isolated root must have no competing owner"),
    );
    // The authoritative data root comes from the installed resolver, never a reconstructed candidate.
    // 权威数据根来自已安装解析器，绝不重建候选路径。
    let data_root = vulcan_runtime_assets::resolve_user_vulcan_code_root()
        .expect("resolve actual isolated data root");
    // The original logging directory API identifies the file used by both tracing and the collector.
    // 原日志目录 API 标识 tracing 与采集器共同使用的文件。
    let log_path = crate::logging::directory(&data_root).join("service.jsonl");
    // The actual worker flag is retained with this service owner.
    // 实际工作器标志与此服务所有者一同保留。
    let flag = Arc::new(AtomicBool::new(true));
    emit_queue("before-worker");
    // Existing injection is selected only by the parent-owned child environment.
    // 既有注入仅由父进程拥有的子环境选择。
    let fail_first = std::env::var("VULCAN_TEST_FAIL_FIRST_HEADLESS_SHUTDOWN")
        .ok()
        .as_deref()
        == Some("1");
    if fail_first {
        assert!(matches!(
            invoke_worker(&service, &flag),
            TrayEvent::ShutdownFailed(_)
        ));
        assert!(
            !flag.load(Ordering::Acquire),
            "failed worker must restore its original request flag"
        );
        assert!(
            read_records(&log_path).iter().all(|record| {
                record.get("event").and_then(serde_json::Value::as_str)
                    != Some("native_capture_checkpoint")
                    || record.get("terminal") != Some(&serde_json::Value::Bool(true))
            }),
            "failed business shutdown must not produce a terminal capture record"
        );
        // A live unique observer must still reject replacement after the failed business shutdown.
        // 业务关闭失败后，活动唯一观察器仍必须拒绝替换。
        let candidate = Arc::new(|_| {});
        assert!(
            vulcan_service_kernel::luaskills_diagnostics::install_native_identity_observer(
                candidate
            )
            .is_err(),
            "failed worker must retain the original identity observer"
        );
        // A subsequent phase can reach disk only if the first failure retained the original callback.
        // 仅首次失败保留原回调时，后续阶段才能到达磁盘。
        emit_queue("after-failed-worker");
        flag.store(true, Ordering::Release);
    }
    assert!(matches!(
        invoke_worker(&service, &flag),
        TrayEvent::ShutdownComplete
    ));
    // Read immediately after the same production helper returns, while its service owner is still alive.
    // 同生产辅助方法返回后立即读取，此时服务所有者仍然存活。
    let records = read_records(&log_path);
    // This actual terminal record is only one part of the owner control-flow plus complete-stderr proof.
    // 此真实终态记录只是所有者控制流与完整 stderr 组合证明的一部分。
    let terminal = records
        .iter()
        .find(|record| {
            record.get("event").and_then(serde_json::Value::as_str)
                == Some("native_capture_checkpoint")
                && record.get("terminal") == Some(&serde_json::Value::Bool(true))
        })
        .expect("worker completion requires terminal capture before Drop");
    assert_eq!(terminal["native_closed"], true);
    assert_eq!(terminal["incomplete"], false);
    // Every persisted checkpoint must retain the explicitly selected loss and segment boundaries.
    // 每个持久化检查点均须保留明确选定的丢失及分段边界。
    for record in records.iter().filter(|record| {
        record.get("event").and_then(serde_json::Value::as_str) == Some("native_capture_checkpoint")
    }) {
        assert_eq!(record["incomplete"], false);
        assert_eq!(record["counts"]["invalid"], 0);
        assert_eq!(record["counts"]["filtered"], 0);
        assert_eq!(record["counts"]["io_errors"], 0);
        // Each producer loss class has one explicit zero assertion.
        // 每个生产者丢失分类均有一项明确零值断言。
        for key in ["full", "disconnected", "oversize", "observer_missing"] {
            assert_eq!(
                record["handoff_losses"][key], 0,
                "capture handoff loss: {key}"
            );
        }
    }
    assert_phase(&records, "before-worker");
    if fail_first {
        assert_phase(&records, "after-failed-worker");
    }
    // Both Arc owners and the collector remain alive, preventing Drop from repairing an incorrect helper.
    // Arc 所有者及采集器仍然存活，阻止 Drop 修补错误辅助方法。
    assert_eq!(Arc::strong_count(&service), 1);
    std::process::exit(0);
}

/// Invoke the exact production helper on an OS worker with `service` and `flag`; return its event.
/// 携带 `service` 与 `flag` 在系统工作线程调用精确生产辅助方法；返回其事件。
/// The Arc clones retain the same owner and the real event crosses a channel before any assertion.
/// Arc 克隆保留同所有者，真实事件经通道返回后才进行断言。
fn invoke_worker(service: &Arc<HeadlessService>, flag: &Arc<AtomicBool>) -> TrayEvent {
    // A clone of the actual owner, not a second service or capture subscription.
    // 实际所有者的克隆，不是第二个服务或采集订阅。
    let worker_service = Arc::clone(service);
    // The actual request flag used by production recovery.
    // 生产恢复所使用的实际请求标志。
    let worker_flag = Arc::clone(flag);
    // The concrete returned event is delivered before completion is inspected.
    // 检查完成状态前先交付具体返回事件。
    let (sender, receiver) = mpsc::channel();
    // A real thread also makes Send/Sync requirements compile-time visible.
    // 真实线程也使 Send/Sync 要求在编译时可见。
    let worker = std::thread::spawn(move || {
        sender
            .send(run_shutdown_worker(&worker_service, &worker_flag))
            .expect("deliver actual tray shutdown event");
    });
    // A bounded event receipt cannot hang the test on a failed worker.
    // 有界事件回执不会因失败工作器让测试永久挂起。
    let event = receiver
        .recv_timeout(CHILD_TIMEOUT)
        .expect("receive bounded actual tray event");
    worker.join().expect("join completed tray fixture worker");
    event
}

/// Emit an exact nullable queue fixture for `operation_id` through the public Core callback; return nothing.
/// 经公开 Core 回调发出 `operation_id` 的精确可空队列夹具；无返回值。
/// This probes the real installed collector without fabricating a native operation or mocking shutdown.
/// 此方法探测真实已安装采集器，不虚构原生操作执行或模拟关闭。
fn emit_queue(operation_id: &str) {
    // Fixed schema-only metadata is deliberately free of business arguments, paths and secrets.
    // 固定 Schema 元数据明确不含业务参数、路径及秘密。
    let record = serde_json::json!({
        "luaskills_embedded_diagnostic":1,
        "runtime_id":"tray-worker-fixture",
        "pool_id":"tray-pool-fixture",
        "operation_id":operation_id,
        "instance_id":null,
        "phase":"queue",
        "elapsed_ns":null,
        "lua_heap_bytes":null,
        "host_wait":null,
        "succeeded":null
    });
    vulcan_service_kernel::luaskills_diagnostics::emit_runtime_log(
        vulcan_service_kernel::luaskills_diagnostics::RuntimeLogLevel::Info,
        record.to_string(),
    );
}

/// Parse all actual JSON lines at `path`; return their persisted order or fail explicitly.
/// 解析 `path` 的全部真实 JSON 行；返回持久化顺序或显式失败。
fn read_records(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .expect("read actual service JSON log")
        .lines()
        .map(|line| serde_json::from_str(line).expect("parse actual log record"))
        .collect()
}

/// Require a persisted native phase with exact fixture `operation_id` in `records`; return nothing.
/// 要求 `records` 存在精确夹具 `operation_id` 的持久化原生阶段；无返回值。
fn assert_phase(records: &[serde_json::Value], operation_id: &str) {
    assert!(
        records.iter().any(|record| {
            record.get("event").and_then(serde_json::Value::as_str) == Some("native_phase")
                && record["native"]["operation_id"].as_str() == Some(operation_id)
        }),
        "original callback fixture was not persisted: {operation_id}"
    );
}
