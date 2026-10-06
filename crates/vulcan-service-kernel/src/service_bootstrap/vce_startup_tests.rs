//! Full service bootstrap and process restart with isolated user paths and real project persistence.
//! 使用隔离用户路径及真实项目持久化的完整服务引导与进程重启。

use super::*;
use std::path::Path;
use std::time::Duration;
use vulcan_service_api::{VceJobAction, VceJobSubmitRequest, VceProjectRequest};
use vulcan_workspace_store::vce_data_plane::VceJobState;

/// Host the production browser protocol over a private, explicitly supplied test profile.
/// 在明确指定的私有测试配置上承载生产浏览器协议。
/// No arguments; requires an empty absolute test root and exits when its stop marker is created.
/// 无参数；要求空的绝对测试根目录，创建其停止标记后退出。
#[test]
#[ignore = "manual browser acceptance host; requires VULCAN_VCE_BROWSER_TEST_ROOT"]
fn real_browser_vce_lifecycle_host() {
    // The manual host never falls back to a user directory or an existing profile.
    // 手动宿主绝不回退到用户目录或已有配置。
    let root = std::path::PathBuf::from(
        std::env::var_os("VULCAN_VCE_BROWSER_TEST_ROOT").expect("explicit private browser root"),
    );
    assert!(root.is_absolute());
    assert_eq!(
        std::fs::read_dir(&root).unwrap().count(),
        0,
        "empty test root"
    );
    run_bootstrap_phase(&root, "browser");
}

/// Run two separate service processes over one private persisted profile and verify their receipts.
/// 在同一私有持久配置上运行两个独立服务进程，并验证其回执。
/// No inputs; one explicit empty index is created, and ninety-nine absent source roots stay untouched.
/// 无输入；只明确创建一个空索引，其余九十九个不存在的源码根保持不变。
#[test]
#[ignore = "requires plugins, the LuaSkills runtime and two spawned service processes"]
fn real_startup_and_restart_leave_uninitialized_projects_cold() {
    // Only child commands receive this task-specific environment; the parent profile is never redirected.
    // 仅子命令接收本任务专用环境；父进程配置目录不会被重定向。
    if let Some(root) = std::env::var_os("VULCAN_VCE_BOOTSTRAP_TEST_ROOT") {
        let phase = std::env::var("VULCAN_VCE_BOOTSTRAP_TEST_PHASE").expect("explicit child phase");
        assert!(phase == "initialize" || phase == "restore");
        run_bootstrap_phase(Path::new(&root), &phase);
        return;
    }
    // A short root also keeps native Lance filenames within the Windows path budget.
    // 短根目录同时使原生 Lance 文件名保持在 Windows 路径预算内。
    let directory = tempfile::tempdir().expect("private startup profile");
    for phase in ["initialize", "restore"] {
        // The exact test filter prevents sibling tests from inheriting the process-wide path installation.
        // 精确测试筛选防止其他用例继承进程级路径安装。
        let mut command =
            std::process::Command::new(std::env::current_exe().expect("test executable"));
        command.args(["--exact", "service_bootstrap::vce_startup_tests::real_startup_and_restart_leave_uninitialized_projects_cold",
            "--ignored", "--test-threads=1", "--nocapture"])
            .env("VULCAN_VCE_BOOTSTRAP_TEST_ROOT", directory.path())
            .env("VULCAN_VCE_BOOTSTRAP_TEST_PHASE", phase);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(vulcan_host_tools::process_creation::WINDOWS_CREATE_NO_WINDOW);
        }
        // Retain the exact child handle so timeout cleanup cannot affect any user or parallel process.
        // 持有精确子进程句柄，确保超时清理不会影响用户或并行进程。
        let mut child = command.spawn().expect("spawn isolated full bootstrap");
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect("observe owned bootstrap process") {
                assert!(status.success(), "bootstrap phase {phase}: {status}");
                break;
            }
            if started.elapsed() > Duration::from_secs(120) {
                child.kill().expect("stop only timed-out bootstrap child");
                child.wait().expect("join timed-out bootstrap child");
                panic!("full bootstrap phase {phase} exceeded its deadline");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    // Receipts prove both exact child entries ran, rather than an empty test-filter success.
    // 回执证明两个精确子入口确实运行，而不是空测试筛选产生的成功。
    let first: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("initialize.json")).unwrap())
            .unwrap();
    let second: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("restore.json")).unwrap())
            .unwrap();
    assert_ne!(first["processId"], second["processId"]);
    assert_eq!(first["jobId"], second["jobId"]);
    assert_eq!(first["checkpoint"], second["checkpoint"]);
    assert_eq!(first["lastSuccess"], second["lastSuccess"]);
}

/// Install private paths, persist isolated provider routes and run the unmodified startup orchestration.
/// 安装私有路径、持久化隔离的供应商路由并运行未替换的启动编排。
/// `root` owns both phases; `phase` selects explicit initialization or read-only process restoration.
/// `root` 持有两个阶段；`phase` 选择明确初始化或只读进程恢复。
fn run_bootstrap_phase(root: &Path, phase: &str) {
    use vulcan_provider_config::config::ProviderConfigurationFile;
    use vulcan_workbench_model::{ProjectVceSettings, ProjectVceUpdateMode, TaskProjectSummary};
    // Install before creating any service state or reading any configuration.
    // 在创建任何服务状态或读取任何配置之前安装。
    assert!(
        runtime_assets::test_support::install_isolated_process_root(Path::new(
            "relative-test-root"
        ))
        .is_err()
    );
    runtime_assets::test_support::install_isolated_process_root(root)
        .expect("isolate startup paths");
    assert!(runtime_assets::test_support::install_isolated_process_root(root).is_err());
    // Database files deliberately survive between the two phases of this acceptance.
    // 数据库文件在两个验收阶段之间刻意保留。
    let data_root = vulcan_workspace_store::data_plane::application_data_root();
    assert_eq!(
        data_root,
        root.canonicalize().unwrap().join("user-vulcan/code")
    );
    // Browser acceptance needs only two projects; restart acceptance deliberately retains one hundred.
    // 浏览器验收只需两个项目；重启验收刻意保留一百个项目。
    let project_count = if phase == "browser" { 2 } else { 100 };
    if phase != "restore" {
        // No real user credential or VMM configuration is copied into this test.
        // 不向此测试复制任何真实用户凭据或记忆配置。
        let (_, _, configuration) =
            crate::tests::vce_configuration_readiness_tests::configured_state();
        let mut provider = ProviderConfigurationFile::default();
        provider.vmm.enabled = Some(false);
        vulcan_provider_config::config::save_configuration_file(&provider)
            .expect("private disabled VMM config");
        vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
            &configuration,
            &data_root,
        )
        .expect("private VCE routes");
        // Seed real project rows before starting the full service, not into its in-memory snapshot.
        // 在完整服务启动前播种真实项目记录，不向其内存快照注入项目。
        let seed_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        seed_runtime.block_on(async {
            let report = vulcan_workspace_store::data_plane::bootstrap_application_data_plane()
                .await
                .expect("seed data plane");
            for index in 0..project_count {
                let project = TaskProjectSummary::new(
                    format!("startup-project-{index}"),
                    format!("Startup project {index}"),
                    root.join(format!("source-{index}")).display().to_string(),
                    false,
                    index,
                    None,
                    None,
                    Vec::new(),
                )
                .with_vce_settings(ProjectVceSettings {
                    enabled: phase != "browser",
                    update_mode: if phase == "browser" {
                        ProjectVceUpdateMode::Manual
                    } else {
                        ProjectVceUpdateMode::Automatic
                    },
                    ..Default::default()
                });
                vulcan_workspace_store::workspace_store::persist_new_project(
                    report.handle.clone(),
                    &project,
                )
                .await
                .expect("persist cold project");
            }
        });
    }
    // Capture configuration bytes before full startup, including across the second process.
    // 在完整启动之前捕获配置字节，包含第二个进程的边界。
    let config_path = vulcan_provider_config::config::public_ai_connections_file_path();
    let before_bytes = std::fs::read(&config_path).unwrap();
    let state =
        crate::create_service_bridge_state(crate::tests::test_support::test_service_shell_host());
    // Browser subscribers receive the same production event sink used by the normal service host.
    // 浏览器订阅者使用正常服务宿主相同的生产事件接收器。
    let (event_bus, _) = tokio::sync::broadcast::channel(2048);
    let emitter =
        ServiceEventEmitter::new(vec![vulcan_service_host::webui_service_event_bus_sink(
            event_bus.clone(),
        )]);
    let boot_started = Instant::now();
    run_service_bootstrap(&state, &emitter).expect("actual complete service bootstrap");
    // Freeze startup timing before any explicit indexing or interactive browser acceptance begins.
    // 在任何显式索引或交互浏览器验收开始前固定启动耗时。
    let bootstrap_elapsed_ms = boot_started.elapsed().as_millis();
    // Only clone existing owners; all subsequent work uses the services installed by the real bootstrap.
    // 仅克隆既有所有者；后续全部操作使用真实引导安装的服务。
    let (executor, _controller, manager) = {
        let guard = state.lock().unwrap();
        assert!(guard.startup.ready, "startup: {:?}", guard.startup);
        assert_eq!(
            guard
                .workspace_snapshot
                .as_ref()
                .unwrap()
                .task_projects
                .len(),
            project_count
        );
        // Startup may prepare an inactive native backend; no VMM consumer may be enabled for this fixture.
        // 启动可以准备停用的原生后端；此夹具不得启用任何记忆消费者。
        assert!(
            !vulcan_vmm::VmmEffectiveConfig::resolve(
                &guard
                    .agent_runtime
                    .as_ref()
                    .unwrap()
                    .provider_configuration
                    .vmm,
                None,
                None,
            )
            .enabled
        );
        assert!(
            guard
                .vmm_controller_registry
                .snapshots()
                .unwrap()
                .is_empty()
        );
        (
            guard.service_runtime.clone().unwrap(),
            guard.controller_runtime.clone().unwrap(),
            guard.vce_runtime.clone().unwrap(),
        )
    };
    // Run cleanup after assertions as well, so a failing test does not keep its managed controller alive.
    // 断言失败后也执行清理，避免失败测试保持托管控制器存活。
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        executor.block_on(async {
        assert!(crate::vce::vce_configuration_readiness(&state).status.ready, "startup VCE readiness");
        if phase == "browser" {
            std::fs::create_dir(root.join("source-0")).unwrap();
            std::fs::create_dir(root.join("source-1")).unwrap();
            run_browser_acceptance(&state, &emitter, event_bus, root).await;
            return;
        }
        for index in 0..100 {
            let project_id = format!("startup-project-{index}");
            assert!(manager.project_status(&project_id).await.is_none(), "startup must stay lazy: {project_id}");
            if index != 0 || phase == "initialize" {
                assert!(manager.get_job(&project_id, None).await.unwrap().is_none());
                assert!(!root.join(format!("source-{index}")).exists());
            }
        }
        if phase == "initialize" {
            assert!(!data_root.join("vce/projects").exists());
            std::fs::create_dir(root.join("source-0")).unwrap();
            // Explicit submission is the only first-index trigger in either process.
            // 在两个进程中，明确提交都是唯一首次索引触发点。
            let job = crate::vce::service_submit_vce_job(state.clone(), VceJobSubmitRequest {
                project_id: "startup-project-0".into(), action: VceJobAction::Initialize,
                idempotency_key: "full-startup-explicit-index".into(), retry_of_job_id: None,
                expected_settings_revision: ProjectVceSettings::default().revision, expected_checkpoint_version: None,
            }).await.expect("explicit first index");
            tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    let receipt = manager.get_job("startup-project-0", Some(&job.job_id)).await.unwrap().unwrap();
                    if receipt.state.is_terminal() { assert_eq!(receipt.state, VceJobState::Succeeded, "{receipt:?}"); break; }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }).await.expect("explicit index finishes");
        }
        // Reading activity after restart may restore the known index, but cannot submit another job.
        // 重启后读取活动状态可以恢复已知索引，但不能提交另一个作业。
        let activity = crate::vce::service_get_vce_project_activity(state.clone(), VceProjectRequest { project_id: "startup-project-0".into() }).await.expect("restored activity");
        assert!(activity.query_available);
        let job = manager.get_job("startup-project-0", None).await.unwrap().unwrap();
        assert_eq!(job.state, VceJobState::Succeeded);
        assert_eq!(std::fs::read_dir(data_root.join("vce/projects")).unwrap().count(), 1);
        for index in 1..100 {
            assert!(manager.project_status(&format!("startup-project-{index}")).await.is_none());
            assert!(!root.join(format!("source-{index}")).exists());
        }
        assert_eq!(std::fs::read(&config_path).unwrap(), before_bytes);
        // Persist observed facts only after all assertions, so the parent can prove both phases executed.
        // 仅在全部断言后持久化观测事实，使父进程可以证明两个阶段确实执行。
        std::fs::write(root.join(format!("{phase}.json")), serde_json::to_vec(&serde_json::json!({
            "processId": std::process::id(), "jobId": job.job_id, "checkpoint": activity.status.checkpoint_version,
            "lastSuccess": activity.last_successful_update_unix_ms, "bootstrapElapsedMs": bootstrap_elapsed_ms
        })).unwrap()).unwrap();
    })
    }));
    crate::service_prepare_for_app_shutdown(&state).expect("actual service shutdown preparation");
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

/// Serve real HTTP, WebSocket and RPC owners without publishing discovery into user or shared temp paths.
/// 承载真实 HTTP、WebSocket 及 RPC 所有者，不向用户或共享临时路径发布发现文件。
/// Inputs are the bootstrapped state, event owners and private root; returns after an explicit stop marker.
/// 输入为已引导状态、事件所有者及私有根；显式停止标记出现后返回。
async fn run_browser_acceptance(
    state: &SharedServiceBridgeState,
    emitter: &ServiceEventEmitter,
    event_bus: tokio::sync::broadcast::Sender<vulcan_service_api::ServiceEvent>,
    root: &Path,
) {
    // Bind a new loopback endpoint directly; no default service port or discovery fallback is involved.
    // 直接绑定新的回环端点；不涉及默认服务端口或发现回退。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    // This capability belongs only to the private host and is never printed to logs.
    // 此能力仅属于私有宿主，绝不输出到日志。
    let service_token = uuid::Uuid::new_v4().to_string();
    let discovery = vulcan_service_api::webui_service_ready_discovery_snapshot(
        address.port(),
        std::process::id(),
        crate::current_unix_millis_text(),
        service_token.clone(),
    );
    let shared_discovery = Arc::new(Mutex::new(discovery.clone()));
    state.lock().unwrap().webui_service_discovery = Some(shared_discovery.clone());
    // Retain the exact production dispatcher and resource resolver, not a response stub.
    // 保留精确生产分发器及资源解析器，不使用响应桩。
    let rpc_state = state.clone();
    let rpc_emitter = emitter.clone();
    let resource_state = state.clone();
    let config = vulcan_service_host::WebuiServiceHttpConfig {
        discovery: shared_discovery,
        service_token,
        event_bus,
        static_root: None,
        rpc_dispatcher: Arc::new(move |method, params| {
            Box::pin(crate::rpc_dispatch::dispatch_service_rpc(
                rpc_emitter.clone(),
                rpc_state.clone(),
                method,
                params,
            ))
        }),
        session_resource_resolver: Arc::new(move |request| {
            Box::pin(
                crate::conversation_resources_v1::service_session_resource_content_v1(
                    resource_state.clone(),
                    request,
                ),
            )
        }),
        client_disconnected: Arc::new(|client_id| {
            crate::cancel_command_session_displays_for_client(client_id);
            crate::cancel_project_terminal_displays_for_client(client_id);
        }),
        status_provider: None,
        browser_stream: None,
        browser_download_resolver: None,
    };
    let server = tokio::spawn(vulcan_service_host::serve_webui_service_http_listener(
        listener, config,
    ));
    std::fs::write(
        root.join("browser-discovery.json"),
        serde_json::to_vec(&discovery).unwrap(),
    )
    .unwrap();
    eprintln!(
        "[vce-browser-acceptance] ready at http://{address}; root={}",
        root.display()
    );
    // A finite deadline prevents an abandoned manual test from holding the private browser host indefinitely.
    // 有限截止时间防止被遗弃的手动测试无限持有私有浏览器宿主。
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
    while !root.join("stop-browser-host").exists() && tokio::time::Instant::now() < deadline {
        assert!(
            !server.is_finished(),
            "private browser server stopped unexpectedly"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    server.abort();
    assert!(
        server
            .await
            .expect_err("owned server aborted")
            .is_cancelled()
    );
    assert!(
        root.join("stop-browser-host").exists(),
        "manual browser acceptance expired"
    );
}
