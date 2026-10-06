//! Real invocation and concurrent drain diagnostic acceptance without a second execution path.
//! 真实调用及并发排空诊断验收，不引入第二条执行路径。

use super::stateless::{Fixture, WAIT};
use super::*;

/// Await an available readonly sample for manager/plugin_id/generation under the existing fixture observation bound.
/// 在既有夹具观测期限内，为 manager/plugin_id/generation 等待可用只读样本。
/// Return real metadata; only the explicit transient Busy contract may retry, all other failures fail the test.
/// 返回真实元数据；仅明确暂时忙碌契约可以重试，其他失败均使测试失败。
pub(in crate::runtime::formal) async fn observed(
    manager: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    generation: Option<u64>,
) -> VulcanPluginResourceDiagnostics {
    tokio::time::timeout(WAIT, async {
        loop {
            match manager.formal_plugin_resource_diagnostics(plugin_id, generation) {
                Ok(Some(observation)) => return observation,
                Err(error) if error.message == "Plugin resource diagnostics are busy." => {
                    tokio::task::yield_now().await;
                }
                other => panic!("unexpected resource observation: {other:?}"),
            }
        }
    })
    .await
    .expect("readonly evidence must become observable within fixture budget")
}

/// Read metadata-only manager without creating a parent; return no resource observation before attachment.
/// 读取纯元数据管理器，不创建父级；附加前不返回资源观测。
#[test]
fn formal_resource_diagnostics_never_attach_a_runtime() {
    // Existing fixture discovery creates no formal execution owner.
    // 既有夹具发现不会创建正式执行所有者。
    let root = tempfile::tempdir().expect("diagnostic root");
    // Exact fixture namespace is known from its real package writer.
    // 精确夹具命名空间来自真实包写入器。
    let bundled = root.path().join("bundled");
    write_hook_package(&bundled);
    // This manager has no attached parent before or after readonly management.
    // 只读管理前后，此管理器都没有附加父级。
    let manager = test_manager(root.path(), bundled);
    assert!(
        manager
            .formal_plugin_resource_diagnostics("hook-plugin", None)
            .expect("readonly absence")
            .is_none()
    );
    assert!(
        manager
            .formal_runtime_status()
            .expect("still unattached")
            .is_none()
    );
}

/// Read a locked real registry as explicit Busy; return neither empty resources nor invented terminal success.
/// 将已锁真实注册表读取为明确忙碌；不返回空资源或虚构终态成功。
#[test]
fn formal_resource_diagnostics_registry_contention_is_explicit() {
    // The existing fixture attaches its real bounded parent.
    // 既有夹具附加其真实有界父级。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    // Cloning ownership does not create another runtime or change registry state.
    // 克隆所有权不创建另一运行时，也不改变注册表状态。
    let owner = fixture
        .manager
        .formal_runtime_owner()
        .expect("owner")
        .expect("attached parent");
    // Hold the actual pinned registry while executing the independent readonly path.
    // 执行独立只读路径期间持有实际固定注册表。
    let guard = owner.pinned.lock().expect("original registry lock");
    assert_eq!(
        fixture
            .manager
            .formal_plugin_resource_diagnostics("idle-plugin", None)
            .expect_err("cannot certify unknown state")
            .message,
        "Plugin resource diagnostics are busy."
    );
    drop(guard);
    assert!(
        fixture
            .manager
            .formal_plugin_resource_diagnostics("idle-plugin", None)
            .expect("released registry")
            .expect("parent")
            .resources
            .is_empty()
    );
}

/// Real blocked native work remains queryable while plugin drain waits; retired failure history is nonblocking.
/// 插件排空等待期间真实阻塞原生工作仍可查询；退役失败历史不阻塞。
#[test]
fn formal_resource_diagnostics_read_during_real_drain_and_exclude_terminal_ownership() {
    // A short drain budget leaves authentic pending physical ownership, never fabricated state.
    // 较短排空预算保留真实待完成物理所有权，绝不伪造状态。
    let settings = VulcanPluginRuntimeSettings {
        shutdown_timeout_ms: 30,
        ..VulcanPluginRuntimeSettings::default()
    };
    // The fixture's callback waits until explicit release regardless of cooperative cancellation.
    // 夹具回调无论协作取消状态如何，都等待显式放行。
    let fixture = Fixture::new(settings);
    fixture.runtime.block_on(async {
        // Freeze the real request and manager before starting production invocation.
        // 启动生产调用前固定真实请求及管理器。
        let manager = fixture.manager.clone();
        // Business payload remains entirely outside the diagnostics DTO.
        // 业务载荷完全位于诊断 DTO 之外。
        let request = fixture.request("block");
        // Actual production invocation enters the controlled native callback.
        // 实际生产调用进入受控原生回调。
        let invocation = tokio::spawn(async move { manager.invoke(request).await });
        fixture.wait_entered().await;
        // Read the real retained host ID rather than parse an error or assume an array position.
        // 读取真实保留宿主 ID，不解析错误，也不假定数组位置。
        let before = observed(&fixture.manager, "idle-plugin", None).await;
        assert_eq!(
            before.pinned_session_count, 0,
            "ordinary invocation is never a pinned tool"
        );
        // Feature lookup remains stable if other retained record categories are added.
        // 即使新增其他保留记录类别，特征查找仍保持稳定。
        let id = before
            .resources
            .iter()
            .find(|row| row.resource_kind == VulcanPluginResourceKind::FormalCall)
            .expect("expected formal call may have changed")
            .resource_id
            .clone();
        // The existing drain supervisor owns the real cancellation and physical wait.
        // 既有排空监督器拥有真实取消及物理等待。
        let manager = fixture.manager.clone();
        // Keep actual drain in flight while a separate management worker samples it.
        // 独立管理工作者采样期间，保持实际排空正在进行。
        let drain = tokio::spawn(async move { manager.drain_plugin_sessions("idle-plugin").await });
        tokio::time::timeout(WAIT, async {
            loop {
                // Read independently of the native callback and asynchronous drain gate.
                // 独立于原生回调及异步排空门读取。
                let manager = fixture.manager.clone();
                // Blocking worker mirrors the kernel's synchronous management boundary.
                // 阻塞工作者对应内核同步管理边界。
                let read = tokio::task::spawn_blocking(move || {
                    manager.formal_plugin_resource_diagnostics("idle-plugin", None)
                })
                .await
                .expect("reader worker");
                // Short metadata contention is explicit and cannot fabricate an empty ownership snapshot.
                // 短暂元数据争用明确呈现，不能伪造空所有权快照。
                let observation = match read {
                    Ok(Some(observation)) => observation,
                    Err(error) if error.message == "Plugin resource diagnostics are busy." => {
                        tokio::task::yield_now().await;
                        continue;
                    }
                    other => panic!("unexpected concurrent diagnostic read: {other:?}"),
                };
                // Preserve real ownership during cancellation rather than treat closing as closed.
                // 取消期间保留真实所有权，不把关闭中视为已关闭。
                let row = observation
                    .resources
                    .iter()
                    .find(|row| row.resource_id == id)
                    .expect("original ownership must remain while callback blocks");
                assert!(!row.retired);
                assert!(row.is_drain_blocking);
                assert_eq!(row.owner_session_id.as_deref(), Some("owner"));
                if row.closing {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("management reader must finish during actual drain");
        assert!(
            !drain
                .await
                .expect("drain supervisor")
                .expect("drain report")
                .failures
                .is_empty()
        );
        fixture.events.release();
        let _ = invocation.await.expect("original invocation");
        tokio::time::timeout(WAIT, async {
            loop {
                // Terminal evidence stays inspectable after physical retirement.
                // 物理退役后终态证据仍可检查。
                let observation = observed(&fixture.manager, "idle-plugin", None).await;
                // The stable original ID identifies the same retained failure history.
                // 稳定原 ID 标识相同保留失败历史。
                let row = observation
                    .resources
                    .iter()
                    .find(|row| row.resource_id == id)
                    .expect("retained cancelled record");
                if row.retired && !row.is_drain_blocking {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("physical retirement");
        fixture
            .manager
            .forget_formal_call(&id, "owner")
            .expect("acknowledge original");
    });
}
