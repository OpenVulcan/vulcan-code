//! Exercises RTK clearing with the production plugin bridge, native SQLite adapter, and isolated history stores.
//! 使用生产插件桥、原生 SQLite 适配器和隔离历史存储验证 RTK 清理。

use crate::plugin_storage::PluginStorageService;
use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use vulcan_plugin_runtime::{
    VulcanPluginActionCaller, VulcanPluginActionRequest, VulcanPluginHostServices,
    VulcanPluginRuntimeManager, VulcanPluginRuntimeSettings, VulcanPluginUiEventPublisher,
    VulcanPluginUiInvalidation,
};
use vulcan_workspace_store::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};

/// Events records only authenticated invalidation metadata, never RTK command contents.
/// Events 仅记录经过认证的失效元数据，不记录 RTK 命令内容。
#[derive(Default)]
struct Events {
    /// values retains revisions published by separate production Lua instances.
    /// values 保留不同生产 Lua 实例发布的修订号。
    values: Mutex<Vec<VulcanPluginUiInvalidation>>,
}

impl VulcanPluginUiEventPublisher for Events {
    /// Accepts event metadata and returns success after recording it.
    /// 接受事件元数据，并在记录后返回成功。
    /// event contains the host-owned plugin identity, topic, and revision.
    /// event 包含宿主持有的插件身份、主题和修订号。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|error| error.to_string())?
            .push(event);
        Ok(())
    }
}

/// Invokes a real RTK UI action and returns its schema-validated output.
/// 调用真实 RTK 界面 Action，并返回通过 Schema 校验的输出。
/// runtime drives the fixture; manager owns the production bridge; repository is its trusted workspace.
/// runtime 驱动夹具；manager 持有生产桥；repository 为其可信工作区。
/// action_id selects a manifest action, and reuse chooses the persistent or fresh Lua execution path.
/// action_id 选择清单 Action，reuse 选择持久或全新 Lua 执行路径。
fn invoke(
    runtime: &tokio::runtime::Runtime,
    manager: &VulcanPluginRuntimeManager,
    repository: &Path,
    action_id: &str,
    reuse: bool,
) -> Value {
    runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: "vulcan-rtk".into(),
            action_id: action_id.into(),
            arguments: json!({}),
            caller: VulcanPluginActionCaller::PluginUi,
            workspace_root: repository.to_path_buf(),
            reuse_enabled_runtime: reuse,
        }))
        .unwrap_or_else(|error| panic!("RTK action {action_id} failed: {error}"))
}

/// Verifies destructive clearing, configuration preservation, and durable invalidation using only disposable data.
/// 仅使用可丢弃数据验证破坏性清理、配置保留及持久失效通知。
/// Takes no parameters and returns after real command history reaches zero through the published UI action.
/// 不接收参数，在真实命令历史通过已发布界面 Action 归零后返回。
#[test]
fn rtk_plugin_clears_isolated_statistics_and_publishes_durable_revisions() {
    // CHILD_MARKER ensures environment changes exist only inside this test's subprocess.
    // CHILD_MARKER 确保环境变更仅存在于本测试的子进程中。
    const CHILD_MARKER: &str = "VULCAN_RTK_CLEAR_INTEGRATION_CHILD";
    if std::env::var_os(CHILD_MARKER).as_deref() != Some(std::ffi::OsStr::new("1")) {
        // history owns both RTK stores; overriding tracking alone would still expose real recall statistics.
        // history 持有两个 RTK 存储；仅覆盖跟踪库仍会暴露真实回读统计。
        let history = tempfile::tempdir().expect("isolated RTK reset stores");
        // qualified derives the harness identity from the real module rather than a mutable source position.
        // qualified 从真实模块派生测试器身份，不依赖会变化的源码位置。
        let qualified = concat!(
            module_path!(),
            "::rtk_plugin_clears_isolated_statistics_and_publishes_durable_revisions"
        );
        // test_name drops only Rust's crate prefix for the native test harness.
        // test_name 仅移除原生测试器不使用的 Rust crate 前缀。
        let (_, test_name) = qualified.split_once("::").expect("qualified RTK test name");
        // output is produced by the same compiled test executable with strictly private tracking and recall paths.
        // output 由同一已编译测试程序产生，跟踪路径和回读路径严格私有。
        let output =
            std::process::Command::new(std::env::current_exe().expect("current test executable"))
                .args([test_name, "--exact", "--test-threads=1", "--nocapture"])
                .env(CHILD_MARKER, "1")
                .env("RTK_DB_PATH", history.path().join("history.db"))
                .env("RTK_RECALL_DB", history.path().join("recall.db"))
                .env("RTK_TELEMETRY_DISABLED", "1")
                .env("RTK_SUPPRESS_HOOK_WARNING", "1")
                .env("NO_COLOR", "1")
                .output()
                .expect("run isolated RTK clearing integration");
        assert!(
            output.status.success(),
            "RTK clearing integration failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(history.path().join("history.db").is_file());
        return;
    }
    // temporary owns the native runtime, private plugin state, and all application binding paths.
    // temporary 持有原生运行时、私有插件状态和全部应用绑定路径。
    let temporary = tempfile::tempdir().expect("RTK plugin storage fixture");
    // root is canonicalized without a Windows verbatim prefix before Lua consumes package paths.
    // root 在 Lua 使用包路径前完成规范化，且不包含 Windows verbatim 前缀。
    let root = vulcan_runtime_assets::native_host_path(&temporary.path().canonicalize().unwrap());
    // repository resolves the production plugin independently from a machine-specific checkout location.
    // repository 解析生产插件，不依赖机器特定的检出位置。
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    // runtime provides two workers so native synchronous bridge callbacks can await actual SQLite operations.
    // runtime 提供两个工作线程，使原生同步桥回调可等待真实 SQLite 操作。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    // host maps every application database binding into this test's disposable directory.
    // host 将全部应用数据库绑定映射到本测试的可丢弃目录。
    let host = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: root.clone(),
        sqlite_path: root.join("app.sqlite3"),
        sqlite_binding_id: "rtk-reset-app".into(),
        system_statistics_sqlite_path: root.join("statistics.sqlite3"),
        system_statistics_sqlite_binding_id: "rtk-reset-statistics".into(),
    });
    // bridge contains the exact shipped bridge module; no alternative plugin protocol is introduced.
    // bridge 包含精确的随包桥模块，不引入替代插件协议。
    let bridge = root.join("system_lua_lib/vulcan-plugin-bridge");
    fs::create_dir_all(bridge.join("runtime")).unwrap();
    fs::write(bridge.join("dependencies.yaml"), "{}\n").unwrap();
    fs::write(
        bridge.join("runtime/bridge.lua"),
        include_str!("../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
    )
    .unwrap();
    // directory names only the two empty runtime roots required by the existing manager constructor.
    // directory 仅指定既有管理器构造函数要求的两个空运行时根目录。
    for directory in ["user_plugins", "managed_runtime"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    // engine is the native LuaSkills implementation used by the real plugin manager.
    // engine 为真实插件管理器使用的原生 LuaSkills 实现。
    let engine = LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
        LuaRuntimeHostOptions::with_runtime_root(root.clone()),
    ))
    .unwrap();
    // manager owns real manifests, permissions, output validation, and plugin generation boundaries.
    // manager 持有真实清单、权限、输出校验和插件代次边界。
    let manager = VulcanPluginRuntimeManager::new(
        Arc::new(engine),
        repository.join("plugins"),
        root.join("user_plugins"),
        bridge,
        root.join("managed_runtime"),
        root.join("plugin_state"),
        root.join("plugin_runtime"),
    );
    // events retains only metadata while storage uses the production service and actual SQLite engine.
    // events 仅保留元数据；storage 使用生产服务和真实 SQLite 引擎。
    let events = Arc::new(Events::default());
    let storage = Arc::new(PluginStorageService::new(
        &host,
        runtime.handle().clone(),
        root.join("plugin_state"),
    ));
    manager
        .install_host_services(VulcanPluginHostServices {
            storage: Some(storage),
            vldb: None,
            ui_events: Some(events.clone()),
        })
        .unwrap();
    manager
        .initialize_formal_runtime(VulcanPluginRuntimeSettings::default())
        .unwrap();
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .unwrap();
    // workspace bounds Git output independently of the developer's changing working tree.
    // workspace 使 Git 输出保持有界，不依赖开发者不断变化的工作树。
    let workspace = root.join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&workspace)
            .status()
            .unwrap()
            .success()
    );
    // command records real Git output into the subprocess-owned RTK history before clearing.
    // command 在清理前将真实 Git 输出记录到子进程持有的 RTK 历史中。
    let command = std::process::Command::new(
        repository.join("plugins/vulcan-rtk/bin/x86_64-pc-windows-msvc/rtk.exe"),
    )
    .args(["git", "status"])
    .current_dir(&workspace)
    .output()
    .unwrap();
    assert!(
        command.status.success(),
        "RTK fixture command failed: {}",
        String::from_utf8_lossy(&command.stderr)
    );
    // config_before captures current routing choices without writing any defaults.
    // config_before 保存当前路由选择，不写入任何默认值。
    let config_before = invoke(&runtime, &manager, repository, "settings-get", true);
    // before_clear proves that the real history contains records rather than testing an already-empty store.
    // before_clear 证明真实历史包含记录，避免仅测试原本为空的存储。
    let before_clear = invoke(&runtime, &manager, repository, "statistics-get", true);
    assert!(before_clear["summary"]["total_commands"].as_u64().unwrap() > 0);
    // modes exercise both persistent and fresh Lua VMs against the same durable native revision store.
    // modes 使用持久和全新 Lua VM 验证同一个持久原生修订号存储。
    let modes = [true, false];
    // reuse switches VM ownership without changing the native storage or RTK history scope.
    // reuse 切换 VM 所有权，不改变原生存储或 RTK 历史作用域。
    for reuse in modes {
        assert_eq!(
            invoke(&runtime, &manager, repository, "statistics-clear", reuse)["cleared"],
            true
        );
        // after_clear reads the real reset database instead of trusting reset process output.
        // after_clear 读取真实重置后的数据库，不依赖重置进程输出。
        let after_clear = invoke(&runtime, &manager, repository, "statistics-get", true);
        assert_eq!(after_clear["available"], true);
        assert_eq!(after_clear["summary"]["total_commands"], 0);
        assert!(after_clear["byCommand"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        invoke(&runtime, &manager, repository, "settings-get", true),
        config_before
    );
    // published proves both UI refresh metadata and revision continuity across native bundle instances.
    // published 验证界面刷新元数据及原生 Bundle 实例之间的修订号连续性。
    let published = events.values.lock().unwrap();
    assert_eq!(published.len(), modes.len());
    assert!(
        published
            .iter()
            .all(|event| event.plugin_id == "vulcan-rtk" && event.topic == "rtk.statistics")
    );
    assert!(
        published
            .windows(2)
            .all(|pair| pair[1].revision > pair[0].revision)
    );
}
