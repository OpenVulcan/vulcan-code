//! Original owner starts remain observable until real processes and caller scopes release.
//! 原所有者启动保持可观察，直至真实进程及调用者范围释放。

use super::*;
use std::fs::{OpenOptions, TryLockError};
use std::io::{BufRead, Write};

/// Shared fixture observation deadline; it bounds tests rather than production policy.
/// 共享夹具观察期限；它限制测试，不属于生产策略。
pub(in crate::mcp::runtime) const OBSERVE: Duration = Duration::from_secs(10);

/// Creates isolated access with no global server partitions.
/// 创建不包含全局服务器分区的独立访问。
pub(in crate::mcp::runtime) fn access() -> McpRuntimeAccess {
    McpRuntimeAccess::new(
        McpRuntimeOverview::new(Vec::new(), Vec::new(), McpRuntimeDiscoveryReport::default()),
        BTreeMap::new(),
    )
}

/// Builds one exact owner server; its real child locks the supplied file until exit.
/// 构造一个精确所有者服务器；其真实子进程锁定给定文件直至退出。
/// Plugin selects identity, witness selects the OS lock, and ready selects protocol replies or parked initialization.
/// plugin 选择身份，witness 选择操作系统锁，ready 选择协议回复或驻留初始化。
pub(in crate::mcp::runtime) fn server(
    plugin: &str,
    witness: &std::path::Path,
    ready: bool,
) -> McpServerConfig {
    McpServerConfig {
        source: McpServerSource::Plugin {
            plugin_id: plugin.into(),
            generation: 5,
        },
        id: format!("{plugin}:owner"),
        name: format!("{plugin} owner"),
        enabled: true,
        transport: McpServerTransportConfig::Stdio {
            command: std::env::current_exe()
                .expect("test executable")
                .to_str()
                .expect("UTF-8 executable")
                .into(),
            args: vec![
                "--exact".into(),
                "mcp::runtime::tools::owner_drain_tests::owner_resource_child".into(),
                "--ignored".into(),
                "--nocapture".into(),
            ],
            cwd: None,
            env: BTreeMap::from([
                (
                    "VULCAN_OWNER_LEASE".into(),
                    witness.to_str().expect("UTF-8 witness").into(),
                ),
                (
                    "VULCAN_OWNER_READY".into(),
                    if ready { "true" } else { "false" }.into(),
                ),
            ]),
            env_passthrough: Vec::new(),
            inherit_environment: true,
        },
    }
}

/// Runs only as the explicitly selected native fixture, retaining a real exclusive file lock.
/// 仅作为明确选中的原生夹具运行，保留真实排他文件锁。
/// Ready mode serves the minimum MCP contract; parked mode never finishes initialization.
/// 就绪模式提供最小 MCP 契约；驻留模式永远不完成初始化。
#[test]
#[ignore = "launched explicitly by owner drain parent tests"]
fn owner_resource_child() {
    // This handle belongs only to the actual child, never the supervisor.
    // 此句柄仅属于实际子进程，绝不属于监督者。
    let witness = OpenOptions::new()
        .read(true)
        .write(true)
        .open(std::env::var_os("VULCAN_OWNER_LEASE").expect("child witness"))
        .expect("open witness");
    witness.lock().expect("exclusive child witness");
    if std::env::var("VULCAN_OWNER_READY").expect("child mode") == "false" {
        loop {
            std::thread::park();
        }
    }
    // Complete protocol replies do not release the witness while the child remains alive.
    // 完整协议回复不会在子进程仍存活时释放见证。
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        // Requests are emitted by the real runtime, never fabricated by a test transport.
        // 请求由实际运行时发出，绝不由测试传输伪造。
        let request: serde_json::Value =
            serde_json::from_str(&line.expect("request line")).expect("request JSON");
        // Notifications have no response identity under the MCP JSON-RPC contract.
        // 根据 MCP JSON-RPC 契约，通知没有回复身份。
        let Some(id) = request.get("id") else {
            continue;
        };
        // This fixture advertises only tools; optional capabilities stay absent.
        // 此夹具仅声明工具；可选能力保持缺失。
        let result = match request["method"].as_str().expect("method") {
            "initialize" => {
                serde_json::json!({"protocolVersion":request["params"]["protocolVersion"],
                "capabilities":{"tools":{}},"serverInfo":{"name":"owner-drain-fixture","version":"1.0.0"}})
            }
            "tools/list" => {
                serde_json::json!({"tools":[{"name":"probe","description":"Probe owner",
                "inputSchema":{"type":"object","properties":{}}}]})
            }
            "tools/call" => serde_json::json!({"content":[{"type":"text","text":"ready"}]}),
            "ping" => serde_json::json!({}),
            method => panic!("unexpected method: {method}"),
        };
        println!(
            "{}",
            serde_json::json!({"jsonrpc":"2.0","id":id,"result":result})
        );
        std::io::stdout().flush().expect("reply flush");
    }
}

/// Waits for the child-owned file lock; failure never masquerades as a skipped process test.
/// 等待子进程持有文件锁；失败绝不伪装成跳过的进程测试。
pub(in crate::mcp::runtime) async fn child_holds(witness: &tempfile::NamedTempFile) {
    tokio::time::timeout(OBSERVE, async {
        loop {
            match witness.as_file().try_lock() {
                Ok(()) => witness.as_file().unlock().expect("release parent probe"),
                Err(TryLockError::WouldBlock) => break,
                Err(error) => panic!("witness failed: {error}"),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("real child holds witness");
}

/// Checks actual file ownership after the original drain, independently of runtime status flags.
/// 在原排空后检查实际文件所有权，独立于运行时状态标志。
pub(in crate::mcp::runtime) fn child_released(witness: &tempfile::NamedTempFile) {
    witness
        .as_file()
        .try_lock()
        .expect("real child exited before drain completion");
    witness.as_file().unlock().expect("release parent witness");
}

/// A paused unpublished caller and its real child both participate in the original drain.
/// 暂停的未发布调用者及其真实子进程都参与原排空。
#[tokio::test]
async fn owner_drain_tracks_unpublished_start_and_preserves_other_plugin() {
    // Distinct original owners must never share a shutdown decision.
    // 不同原所有者绝不能共享关闭决定。
    let access = access();
    // Independent OS witnesses identify the target and unrelated real child.
    // 独立操作系统见证标识目标及无关真实子进程。
    let witness = tempfile::NamedTempFile::new().expect("target witness");
    // This witness must remain locked when the target is drained.
    // 目标排空时，此见证必须保持锁定。
    let other_witness = tempfile::NamedTempFile::new().expect("other witness");
    // Stable owner identity makes diagnostic assertions independent of generated values.
    // 稳定所有者身份使诊断断言不依赖生成值。
    let owner = SessionId::from_u128(311);
    // Hold the original future so cancellation and publication remain under explicit test control.
    // 保留原 future，使取消及发布由测试明确控制。
    let mut opening = Box::pin(access.open_owned_stdio_partition(
        "target",
        5,
        owner,
        vec![server("target", witness.path(), false)],
    ));
    // The unrelated original start continues independently.
    // 无关的原启动独立继续。
    let mut other = Box::pin(access.open_owned_stdio_partition(
        "other",
        5,
        owner,
        vec![server("other", other_witness.path(), false)],
    ));
    tokio::select! {
        result = &mut opening => panic!("parked startup completed: {result:?}"),
        result = &mut other => panic!("other parked startup completed: {result:?}"),
        _ = async { child_holds(&witness).await; child_holds(&other_witness).await; } => {}
    }
    // Even an access created for a new catalog must observe the same original instances.
    // 即使为新目录创建的访问，也必须观察相同原实例。
    let replacement = access.without_plugin_partition("target");
    replacement
        .begin_plugin_owner_drain("target")
        .expect("close admission");
    replacement
        .begin_plugin_owner_drain("target")
        .expect("idempotent close");
    assert!(replacement.finish_plugin_owner_drain("target").is_err());
    // The caller still owns its unpublished scope, so timeout must retain the exact evidence.
    // 调用者仍拥有尚未发布范围，因此超时必须保留精确证据。
    let error = replacement
        .wait_plugin_owner_drain("target", Duration::ZERO)
        .await
        .expect_err("caller scope remains");
    assert!(error.contains(&owner.to_string()));
    assert!(opening.as_mut().await.is_err());
    drop(opening);
    replacement
        .wait_plugin_owner_drain("target", OBSERVE)
        .await
        .expect("original released");
    child_released(&witness);
    assert!(matches!(
        other_witness.as_file().try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    replacement
        .finish_plugin_owner_drain("target")
        .expect("reopen after evidence");
    // Cleanup of the independent plugin uses its own original scope.
    // 独立插件的清理使用其自身原范围。
    drop(other);
    replacement
        .begin_plugin_owner_drain("other")
        .expect("other drain");
    replacement
        .wait_plugin_owner_drain("other", OBSERVE)
        .await
        .expect("other released");
    child_released(&other_witness);
}

/// Global shutdown captures a cancelled unpublished start and permanently rejects reopening.
/// 全局关闭捕获被取消的未发布启动，并永久拒绝重新开放。
#[tokio::test]
async fn owner_drain_global_shutdown_covers_cancelled_start() {
    // Isolated access owns this test's irreversible shutdown decision.
    // 独立访问拥有本测试的不可逆关闭决定。
    let access = access();
    // This OS witness remains owned by the child until actual exit.
    // 此操作系统见证由子进程持有直至实际退出。
    let witness = tempfile::NamedTempFile::new().expect("cancelled witness");
    // The fixed owner identifies this specific unpublished start.
    // 固定所有者标识此精确未发布启动。
    let owner = SessionId::from_u128(312);
    // Retain the original caller until shutdown has captured it.
    // 保留原调用者，直至关闭已经捕获它。
    let mut opening = Box::pin(access.open_owned_stdio_partition(
        "target",
        5,
        owner,
        vec![server("target", witness.path(), false)],
    ));
    tokio::select! {
        result = &mut opening => panic!("parked startup completed: {result:?}"),
        _ = child_holds(&witness) => {}
    }
    access.shutdown();
    drop(opening);
    access
        .wait_plugin_owner_drain("target", OBSERVE)
        .await
        .expect("cancelled original released");
    child_released(&witness);
    assert!(access.finish_plugin_owner_drain("target").is_err());
    assert!(
        access
            .open_owned_stdio_partition("other", 5, owner, Vec::new())
            .await
            .is_err()
    );
}

/// Both original scopes for a racing key survive until each distinct owner releases them.
/// 同一竞态键的两个原范围都保留，直至各自不同的所有者释放它们。
#[tokio::test]
async fn owner_drain_keeps_every_same_key_start_and_cancelled_observer() {
    // Isolated admission observes both scopes without publishing either.
    // 独立入场观察两个范围，但不发布任一范围。
    let access = access();
    // Both originals intentionally share exactly the same registry key.
    // 两个原实例故意共享完全相同的注册表键。
    let key = McpRuntimeOwnedPartitionKey::try_new("target", 5, SessionId::from_u128(313))
        .expect("owner key");
    // The first retained scope must not substitute for the second original.
    // 第一个保留范围不能代替第二个原实例。
    let (_, first) = access
        .start_owned_stdio_runtime(&key, Vec::new())
        .expect("first startup");
    // This second original remains alive after the first one releases.
    // 此第二个原实例在第一个释放后继续存活。
    let (_, second) = access
        .start_owned_stdio_runtime(&key, Vec::new())
        .expect("second startup");
    access
        .begin_plugin_owner_drain("target")
        .expect("close both");
    drop(first);
    // Dropping a pending observer must not consume the retained result.
    // 销毁等待中的观察者绝不能消费保留结果。
    let mut observer = Box::pin(access.wait_plugin_owner_drain("target", OBSERVE));
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(observer.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(observer);
    assert!(access.finish_plugin_owner_drain("target").is_err());
    drop(second);
    access
        .wait_plugin_owner_drain("target", OBSERVE)
        .await
        .expect("both original scopes released");
    access
        .finish_plugin_owner_drain("target")
        .expect("reopen exact key");
}

/// Replacement and keyed removal cannot hide old real processes from a later plugin drain.
/// 替换及按键移除不能使旧真实进程逃过后续插件排空。
#[tokio::test]
async fn owner_drain_tracks_replaced_and_removed_real_instances() {
    // One access retains cleanup evidence across both registry mutations.
    // 一个访问跨两次注册表变更保留清理证据。
    let access = access();
    // Replacement preserves the original owner identity.
    // 替换保留原所有者身份。
    let owner = SessionId::from_u128(314);
    // Separate OS locks distinguish old and new child ownership.
    // 独立操作系统锁区分旧新子进程所有权。
    let first = tempfile::NamedTempFile::new().expect("first witness");
    // The replacement must release this lock as well as the predecessor's.
    // 替换者必须释放此锁及前驱的锁。
    let second = tempfile::NamedTempFile::new().expect("second witness");
    // Publish through the real readiness path before replacing.
    // 替换前通过实际就绪路径发布。
    let partition = access
        .open_owned_stdio_partition(
            "target",
            5,
            owner,
            vec![server("target", first.path(), true)],
        )
        .await
        .expect("real ready owner");
    child_holds(&first).await;
    // Retaining a stale tool lease does not hide its original shutdown from replacement.
    // 保留陈旧工具租约不会使原关闭逃过替换。
    let bindings = partition
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .expect("real tool bindings");
    assert!(!bindings.is_empty());
    // The real replacement publishes only after its own protocol is ready.
    // 实际替换仅在自身协议就绪之后发布。
    let replaced = access
        .replace_owned_stdio_partition(
            "target",
            5,
            owner,
            vec![server("target", second.path(), true)],
        )
        .await
        .expect("real replacement");
    child_holds(&second).await;
    replaced.revoke().expect("remove current registry entry");
    access
        .begin_plugin_owner_drain("target")
        .expect("drain even after removal");
    access
        .wait_plugin_owner_drain("target", OBSERVE)
        .await
        .expect("all real originals released");
    child_released(&first);
    child_released(&second);
    assert!(
        bindings
            .iter()
            .find(|binding| binding.origin_tool_name() == Some("probe"))
            .expect("fixture probe tool may have changed")
            .invoke_for_owner(owner, serde_json::json!({}))
            .await
            .is_err()
    );
    access
        .finish_plugin_owner_drain("target")
        .expect("reopen after all originals");
}
