//! Tests for persistent category isolation and plugin-generation retirement.
//! 持久运行时类别隔离与插件代次退役测试。

use super::super::VulcanPluginActionRequest;
use super::super::tests::{test_manager, write_hook_package};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use uuid::Uuid;
use vulcan_hooks::{
    HookActor, HookCapabilitySet, HookContext, HookInvocation, HookMode, HookPayload,
    HookRegistration, HookScope,
};

/// Retirement while initialization is blocked cannot publish an old VM; concurrent new readers share one VM.
/// 初始化受阻时退役不能发布旧 VM；并发新读取共享一个 VM。
#[test]
fn initialization_retirement_rejects_old_publication_and_reopen_shares_one_lease() {
    use super::super::VulcanPluginPersistentRuntimeKind::UiAction;
    use crate::{VulcanPluginHostServices, VulcanPluginVldbRequest, VulcanPluginVldbService};
    use serde_json::Value;
    use std::sync::{Arc, Mutex, atomic::AtomicUsize, mpsc};
    // One bounded wait controls both the real callback and its observer.
    // 同一有界等待控制真实回调及其观察者。
    const WAIT: std::time::Duration = std::time::Duration::from_secs(5);
    /// Blocks only the first real source-initialization callback, allowing the successor to reopen.
    /// 只阻塞首次真实源码初始化回调，允许后继重新开启。
    struct InitializationBarrier {
        /// Signals actual native entry instead of observing an earlier scheduling phase.
        /// 标记实际原生进入，而非观察更早的调度阶段。
        entered: tokio::sync::Notify,
        /// Retains the first callback until the test explicitly retires its generation.
        /// 保留首次回调，直至测试显式退役其代次。
        release: Mutex<mpsc::Receiver<()>>,
        /// Distinguishes the old initialization from the later shared successor.
        /// 区分旧初始化与后续共享的后继。
        entries: AtomicUsize,
    }
    impl VulcanPluginVldbService for InitializationBarrier {
        /// Handles the fixture's read request, returning rows after the first entry is released.
        /// 处理夹具的读取请求，在首次进入被释放后返回行结果。
        fn execute(&self, _request: VulcanPluginVldbRequest) -> Result<Value, String> {
            if self
                .entries
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                == 0
            {
                self.entered.notify_one();
                self.release
                    .lock()
                    .map_err(|error| error.to_string())?
                    .recv_timeout(WAIT)
                    .map_err(|error| error.to_string())?;
            }
            Ok(json!({"rows": []}))
        }
    }
    // All package and engine writes remain in one disposable fixture.
    // 所有插件包及引擎写入保持在同一个可丢弃夹具中。
    let temp = tempfile::tempdir().expect("create initialization fixture");
    // Build the existing real hook/UI bundle before adding an initialization-only host call.
    // 先构造既有真实 Hook/UI 包，再增加仅在初始化执行的宿主调用。
    let plugins = temp.path().join("plugins");
    fs::create_dir_all(&plugins).expect("create plugin root");
    write_hook_package(&plugins);
    // The call uses the explicitly declared host permission through the production bridge.
    // 调用通过生产桥使用显式声明的宿主权限。
    let manifest_path = plugins.join("hook-plugin/plugin.yaml");
    fs::write(
        &manifest_path,
        fs::read_to_string(&manifest_path)
            .expect("read fixture manifest")
            .replace(
                "default_enabled: true",
                "default_enabled: true\npermissions: [system_vldb_access]",
            ),
    )
    .expect("declare initialization callback permission");
    // Initialization is genuinely in progress while its real native handler holds the release channel.
    // 真实原生处理器持有释放通道期间，初始化确实正在执行。
    let bundle_path = plugins.join("hook-plugin/runtime/plugin.bundle.lua");
    fs::write(&bundle_path, format!("vulcan.host.call('vulcan.vldb.sqlite.query.v1', {{ binding = 'system.statistics', sql = 'SELECT 1', params = {{}} }})\n{}",
        fs::read_to_string(&bundle_path).expect("read fixture bundle")))
        .expect("add real initialization callback");
    // The immutable engine belongs only to this plugin manager.
    // 不可变引擎只归此插件管理器所有。
    let manager = test_manager(temp.path(), plugins);
    // Dropping the sender on assertion failure also releases the waiting handler with an explicit error.
    // 断言失败后丢弃发送端也会以显式错误释放等待处理器。
    let (release, receive) = mpsc::channel();
    // Host registration is installed before catalog publication and the first call.
    // 宿主注册在目录发布及首次调用前安装。
    let barrier = Arc::new(InitializationBarrier {
        entered: tokio::sync::Notify::new(),
        release: Mutex::new(receive),
        entries: AtomicUsize::new(0),
    });
    manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: Some(barrier.clone()),
            ui_events: None,
        })
        .expect("install initialization barrier");
    // Catalog generation remains the authority for the action's original identity.
    // 目录代次仍是操作原始身份的权威。
    let catalog = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish fixture");
    // Every action receives distinct exact ownership identifiers.
    // 每个操作都收到不同的精确所有权标识。
    let identity = crate::VulcanPluginActionIdentity {
        client_id: Uuid::new_v4().to_string(),
        invocation_id: Uuid::new_v4().to_string(),
    };
    // Preserve the original published generation through the concurrent retirement.
    // 在并发退役期间保留原始已发布代次。
    let runtime_identity = format!(
        "{}:{}",
        catalog.service_epoch, catalog.plugins["hook-plugin"].generation
    );
    // The async executor remains available while the native handler waits.
    // 原生处理器等待时，异步执行器仍保持可用。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create test runtime");
    runtime.block_on(async {
        // The spawned owner retains execution independently of this observer.
        // 派生所有者独立于此观察者保留执行。
        let worker = manager.clone();
        // Move only cloned identity and the trusted workspace into the spawned owner.
        // 仅将克隆身份及可信工作区移动到派生所有者中。
        let child_identity = identity.clone();
        // Freeze the original runtime identity before retirement.
        // 在退役前冻结原运行时身份。
        let child_runtime = runtime_identity.clone();
        // The workspace remains owned by the fixture for the entire native call.
        // 整个原生调用期间工作区保持由夹具所有。
        let root = temp.path().to_owned();
        // This starts real bridge initialization and waits inside the registered native handler.
        // 此处启动真实桥初始化，并在注册的原生处理器中等待。
        let initializing = tokio::spawn(async move {
            worker
                .invoke_ui_action(
                    VulcanPluginActionRequest {
                        plugin_id: "hook-plugin".into(),
                        action_id: "ui-action".into(),
                        arguments: json!({}),
                        caller: crate::VulcanPluginActionCaller::PluginUi,
                        workspace_root: root,
                        reuse_enabled_runtime: true,
                    },
                    &child_runtime,
                    child_identity,
                )
                .await
        });
        tokio::time::timeout(WAIT, barrier.entered.notified())
            .await
            .expect("real source initialization must enter");
        assert!(
            manager
                .find_action_status("hook-plugin", &runtime_identity, &identity, false)
                .expect("read original action")
                .is_some_and(|status| status.phase == "initializing")
        );
        manager
            .begin_draining("hook-plugin")
            .expect("retire initializing generation");
        release.send(()).expect("release old source initializer");
        assert!(initializing.await.expect("join original action").is_err());
        assert!(
            manager
                .state
                .ui_action_runtimes
                .lock()
                .expect("inspect published runtimes")
                .is_empty()
        );
        assert!(
            manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("drain old generation")
                .failures
                .is_empty()
        );
        // The successor is published only after the original initialization and cleanup have left.
        // 原初始化及清理离开后才发布后继。
        let current = manager
            .finish_lifecycle_transition(
                "hook-plugin",
                &BTreeSet::new(),
                &BTreeSet::new(),
                Vec::new(),
            )
            .expect("publish successor");
        // Concurrent reopen calls must share exactly one new VM.
        // 并发重新开启调用必须共享恰好一个新 VM。
        let generation = current.plugins["hook-plugin"].generation;
        // Join both actual initialization requests rather than comparing scheduling promises.
        // 等待两个实际初始化请求，而非比较调度承诺。
        let (first, second) = tokio::join!(
            manager.ensure_enabled_plugin_runtime("hook-plugin", generation, UiAction),
            manager.ensure_enabled_plugin_runtime("hook-plugin", generation, UiAction)
        );
        assert!(Arc::ptr_eq(
            &first.expect("first successor"),
            &second.expect("shared successor")
        ));
        assert_eq!(
            barrier.entries.load(std::sync::atomic::Ordering::Acquire),
            2
        );
    });
}

/// Queued read cancellation is acknowledged, while a submitted mutation survives its original waiter.
/// 排队读取的取消得到确认，而已提交变更在原始等待方离开后继续。
#[test]
fn explicit_cancellation_and_mutation_detachment_have_distinct_outcomes() {
    use super::super::VulcanPluginPersistentRuntimeKind::UiAction;
    for mutation in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let plugins = temp.path().join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        write_hook_package(&plugins);
        if mutation {
            let path = plugins.join("hook-plugin/plugin.yaml");
            fs::write(
                &path,
                fs::read_to_string(&path)
                    .unwrap()
                    .replace("effect: read", "effect: mutation"),
            )
            .unwrap();
        }
        let manager = test_manager(temp.path(), plugins);
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .unwrap();
        let generation = catalog.plugins["hook-plugin"].generation;
        let runtime_identity = format!("{}:{}", catalog.service_epoch, generation);
        let identity = crate::VulcanPluginActionIdentity {
            client_id: Uuid::new_v4().to_string(),
            invocation_id: Uuid::new_v4().to_string(),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let lease = manager
                .ensure_enabled_plugin_runtime("hook-plugin", generation, UiAction)
                .await
                .unwrap();
            let held = lease.operation_gate.lock().await;
            let worker = manager.clone();
            let child_identity = identity.clone();
            let child_runtime = runtime_identity.clone();
            let request = VulcanPluginActionRequest {
                plugin_id: "hook-plugin".into(),
                action_id: "ui-action".into(),
                arguments: json!({}),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: temp.path().to_owned(),
                reuse_enabled_runtime: true,
            };
            let submitted_request = request.clone();
            let submitted = tokio::spawn(async move {
                worker
                    .invoke_ui_action(submitted_request, &child_runtime, child_identity)
                    .await
            });
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    if manager
                        .find_action_status("hook-plugin", &runtime_identity, &identity, false)
                        .unwrap()
                        .is_some_and(|status| status.phase == "operation_wait")
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            manager
                .action_status("hook-plugin", &runtime_identity, &identity, true)
                .unwrap();
            if mutation {
                submitted.abort();
                assert!(submitted.await.unwrap_err().is_cancelled());
            } else {
                assert!(submitted.await.unwrap().is_err());
            }
            drop(held);
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while !manager
                    .action_status("hook-plugin", &runtime_identity, &identity, false)
                    .unwrap()
                    .terminal
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            let status = manager
                .action_status("hook-plugin", &runtime_identity, &identity, false)
                .unwrap();
            assert_eq!(
                status.phase,
                if mutation { "succeeded" } else { "cancelled" }
            );
            assert_eq!(
                manager.invoke_action(request).await.unwrap()["count"],
                if mutation { 200 } else { 100 }
            );
        });
    }
}

/// Builds one exact host invocation for the fixture hook.
/// 为夹具 hook 构建一次精确宿主调用。
/// registration_id selects the fixture registration; returns one complete host-owned invocation.
/// registration_id选择夹具注册项；返回一份完整的宿主调用。
fn hook_invocation(registration_id: &str) -> HookInvocation {
    HookInvocation {
        registration: HookRegistration::new(
            registration_id,
            "conversation.turn.precheck",
            HookMode::Transform,
        ),
        context: HookContext {
            run_id: Uuid::new_v4().to_string(),
            point: String::from("conversation.turn.precheck"),
            scope: HookScope::Session,
            actor: HookActor::new("runtime-test"),
            workspace_id: None,
            project_id: None,
            session_id: Some(String::from("session-1")),
            conversation_id: Some(String::from("session-1")),
            user_id: None,
            capabilities: HookCapabilitySet::default(),
            metadata: BTreeMap::new(),
        },
        payload: HookPayload::empty_object(),
    }
}

/// Verifies hook calls reuse persistent Lua state and draining rejects stale generations.
/// 验证 hook 调用会复用持久 Lua 状态，且 draining 会拒绝陈旧代次。
#[test]
fn hook_runtime_reuses_state_and_drains_by_plugin_generation() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_hook_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach formal Hook parent");
    let catalog = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load hook catalog");
    let hook = catalog
        .enabled_hooks
        .values()
        .next()
        .cloned()
        .expect("resolve enabled hook");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Tokio test runtime");
    let first = runtime
        .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
        .expect("invoke first hook");
    let ui_action = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("hook-plugin"),
            action_id: String::from("ui-action"),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: runtime_root.clone(),
            reuse_enabled_runtime: true,
        }))
        .expect("invoke persistent UI action");
    let second = runtime
        .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
        .expect("invoke second hook");
    assert_eq!(first.payload.expect("first payload").value["count"], 1);
    assert_eq!(ui_action["count"], 100);
    assert_eq!(second.payload.expect("second payload").value["count"], 2);
    manager
        .begin_draining("hook-plugin")
        .expect("begin plugin draining");
    let stale_error = runtime
        .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
        .expect_err("stale hook must be rejected");
    assert!(stale_error.message.contains("not enabled"));
    let report = runtime
        .block_on(manager.drain_plugin_sessions("hook-plugin"))
        .expect("drain hook runtime");
    assert_eq!(report.closed_hook_runtimes, 1);
    assert_eq!(report.closed_ui_action_runtimes, 1);
    assert!(report.failures.is_empty());
}

/// Same-key lifecycle gates stay stable while unrelated plugins/categories remain independent.
/// 同键生命周期门保持稳定，同时其他插件及类别相互独立。
#[test]
fn persistent_gates_are_local_and_reclaim_unused_entries() {
    use super::super::VulcanPluginPersistentRuntimeKind::{Hook, UiAction};
    let temp = tempfile::tempdir().unwrap();
    let manager = test_manager(temp.path(), temp.path().join("plugins"));
    let first = manager.persistent_runtime_gate("first", UiAction).unwrap();
    let same = manager.persistent_runtime_gate("first", UiAction).unwrap();
    let other = manager.persistent_runtime_gate("second", UiAction).unwrap();
    let hook = manager.persistent_runtime_gate("first", Hook).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &same));
    let _held = first.try_lock().unwrap();
    assert!(same.try_lock().is_err());
    assert!(other.try_lock().is_ok());
    assert!(hook.try_lock().is_ok());
    drop(other);
    drop(hook);
    let _next = manager.persistent_runtime_gate("next", UiAction).unwrap();
    assert_eq!(
        manager.state.persistent_runtime_gates.lock().unwrap().len(),
        2
    );
}

/// Cancelling a queued caller must remove its invocation before Lua state changes.
/// 取消排队调用方必须在 Lua 状态变更前移除调用。
#[test]
fn cancelled_queued_action_does_not_execute_after_gate_release() {
    use super::super::VulcanPluginPersistentRuntimeKind::UiAction;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("runtime");
    let plugins = root.join("plugins");
    fs::create_dir_all(&plugins).unwrap();
    write_hook_package(&plugins);
    let manager = test_manager(&root, plugins);
    let catalog = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .unwrap();
    let generation = catalog.plugins["hook-plugin"].generation;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let lease = manager
            .ensure_enabled_plugin_runtime("hook-plugin", generation, UiAction)
            .await
            .unwrap();
        let held = lease.operation_gate.lock().await;
        let request = VulcanPluginActionRequest {
            plugin_id: "hook-plugin".into(),
            action_id: "ui-action".into(),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: root,
            reuse_enabled_runtime: true,
        };
        let worker = manager.clone();
        let queued_request = request.clone();
        let queued = tokio::spawn(async move { worker.invoke_action(queued_request).await });
        // Wait for admission, not an arbitrary sleep, before dropping the original waiter.
        // 取消原等待方前等待入场证据，不使用任意睡眠。
        let semaphore = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let semaphore = manager
                    .state
                    .action_admissions
                    .lock()
                    .unwrap()
                    .get("hook-plugin")
                    .and_then(std::sync::Weak::upgrade);
                if let Some(semaphore) = semaphore {
                    break semaphore;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        queued.abort();
        assert!(queued.await.unwrap_err().is_cancelled());
        drop(held);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while semaphore.available_permits() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let result = manager.invoke_action(request).await.unwrap();
        assert_eq!(
            result["count"], 100,
            "cancelled action must not increment persistent state"
        );
    });
}

/// A synchronous host call stays owned after its waiter disappears, while another plugin proceeds.
/// 同步宿主调用在等待方消失后仍被持有，同时其他插件继续执行。
#[test]
fn abandoned_running_action_keeps_lease_exclusive_and_other_plugin_progresses() {
    use crate::{VulcanPluginHostServices, VulcanPluginVldbRequest, VulcanPluginVldbService};
    use std::sync::{Arc, Mutex, mpsc};
    // Explicit entry and release channels prove ordering without timing guesses.
    // 显式进入与释放通道证明先后顺序，不依赖计时猜测。
    struct BlockingVldb {
        entered: tokio::sync::Notify,
        release: Mutex<mpsc::Receiver<()>>,
    }
    impl VulcanPluginVldbService for BlockingVldb {
        fn execute(&self, request: VulcanPluginVldbRequest) -> Result<serde_json::Value, String> {
            assert!(
                request.deadline.is_some(),
                "action deadline must reach the host callback"
            );
            self.entered.notify_one();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            Ok(json!({"rows": []}))
        }
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("runtime");
    let plugins = root.join("plugins");
    fs::create_dir_all(&plugins).unwrap();
    write_hook_package(&plugins);
    let manifest_path = plugins.join("hook-plugin/plugin.yaml");
    let original_manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::create_dir_all(plugins.join("fast-plugin/runtime")).unwrap();
    fs::write(
        plugins.join("fast-plugin/plugin.yaml"),
        original_manifest.replace("hook-plugin", "fast-plugin"),
    )
    .unwrap();
    fs::copy(
        plugins.join("hook-plugin/dependencies.yaml"),
        plugins.join("fast-plugin/dependencies.yaml"),
    )
    .unwrap();
    fs::copy(
        plugins.join("hook-plugin/runtime/plugin.bundle.lua"),
        plugins.join("fast-plugin/runtime/plugin.bundle.lua"),
    )
    .unwrap();
    fs::write(
        &manifest_path,
        original_manifest
            .replace(
                "default_enabled: true",
                "default_enabled: true\npermissions: [system_vldb_access]",
            )
            .replace("timeout_ms: 1000", "timeout_ms: 4000"),
    )
    .unwrap();
    let bundle = plugins.join("hook-plugin/runtime/plugin.bundle.lua");
    fs::write(&bundle, fs::read_to_string(&bundle).unwrap().replace("count = count + 100", "vulcan.host.call('vulcan.vldb.sqlite.query.v1', { binding = 'system.statistics', sql = 'SELECT 1', params = {} })\n    count = count + 100")).unwrap();
    let manager = test_manager(&root, plugins);
    let (release, receive) = mpsc::channel();
    let host = Arc::new(BlockingVldb {
        entered: tokio::sync::Notify::new(),
        release: Mutex::new(receive),
    });
    manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: Some(host.clone()),
            ui_events: None,
        })
        .unwrap();
    let catalog = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .unwrap();
    let runtime_identity = format!(
        "{}:{}",
        catalog.service_epoch, catalog.plugins["hook-plugin"].generation
    );
    let identity = crate::VulcanPluginActionIdentity {
        client_id: Uuid::new_v4().to_string(),
        invocation_id: Uuid::new_v4().to_string(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let worker = manager.clone();
        let request = VulcanPluginActionRequest {
            plugin_id: "hook-plugin".into(),
            action_id: "ui-action".into(),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: root,
            reuse_enabled_runtime: true,
        };
        let running_request = request.clone();
        let running_identity = identity.clone();
        let running_runtime = runtime_identity.clone();
        let running = tokio::spawn(async move {
            worker
                .invoke_ui_action(running_request, &running_runtime, running_identity)
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), host.entered.notified())
            .await
            .unwrap();
        running.abort();
        assert!(running.await.unwrap_err().is_cancelled());
        let lease = manager.state.ui_action_runtimes.lock().unwrap()["hook-plugin"].clone();
        assert!(
            lease.operation_gate.try_lock().is_err(),
            "real host callback must retain the VM gate"
        );
        assert!(
            !manager
                .action_status("hook-plugin", &runtime_identity, &identity, false)
                .unwrap()
                .terminal
        );
        let mut other_request = request.clone();
        other_request.plugin_id = "fast-plugin".into();
        let other = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            manager.invoke_action(other_request),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            other["count"], 100,
            "other plugin must finish before releasing the slow host call"
        );
        let mut foreign = identity.clone();
        foreign.client_id = Uuid::new_v4().to_string();
        assert!(
            manager
                .action_status("hook-plugin", &runtime_identity, &foreign, true)
                .is_err()
        );
        release.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !manager
                .action_status("hook-plugin", &runtime_identity, &identity, false)
                .unwrap()
                .terminal
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(lease.operation_gate.try_lock().is_ok());
        let status = manager
            .action_status("hook-plugin", &runtime_identity, &identity, false)
            .unwrap();
        assert!(status.stages_ms.contains_key("host_callback"));
    });
}
