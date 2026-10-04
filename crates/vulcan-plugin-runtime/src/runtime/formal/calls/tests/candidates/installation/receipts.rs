//! Real native installation is observed once even across cancellation and storage reconstruction.
//! 即使经过取消及存储重建，真实原生安装也只观测一次。

use super::*;
use crate::VulcanPluginInstallationPhase as InstallPhase;

/// Successful and failed callbacks retain their exact request envelope, survive reopen, and reject replay.
/// 成功及失败回调保留精确请求信封，重开后仍存续并拒绝重放。
#[test]
fn formal_candidate_install_receipt_survives_reopen_without_replay() {
    for fail in [false, true] {
        // Use genuine immutable storage and an enabled predecessor throughout candidate execution.
        // 在候选执行期间使用真实不可变存储及已启用前驱。
        let (fixture, store, mut transaction) = installation_fixture(false, fail, true);
        // Retain the original identity independently from all callback observations.
        // 独立于全部回调观测保留原身份。
        let request_id = transaction.record().request_id();
        // Native effects must not change the durable selection before explicit publication.
        // 在明确发布之前，原生副作用不得改变持久选择。
        let predecessor = store.activation("hook-plugin").expect("predecessor");
        assert!(
            store
                .installation_receipt("hook-plugin", request_id)
                .expect("not attempted")
                .is_none()
        );
        executor().block_on(async {
            // The candidate is bound to the exact original transaction.
            // 候选绑定至精确原事务。
            let candidate = store
                .inspect_activation_candidate(&transaction)
                .expect("candidate");
            assert_eq!(
                fixture
                    .manager
                    .install_activation_candidate(&candidate)
                    .await
                    .is_err(),
                fail
            );
        });
        assert_eq!(
            store.activation("hook-plugin").expect("selection"),
            predecessor
        );
        // Opening a fresh store must read persisted evidence rather than process-local status.
        // 打开新存储必须读取持久证据，而非进程内状态。
        let reopened =
            VulcanPluginGenerationStore::open(fixture.manager.user_plugin_root()).expect("reopen");
        // The durable phase is based on the actual callback outcome.
        // 持久阶段依据实际回调结果。
        let receipt = reopened
            .installation_receipt("hook-plugin", request_id)
            .expect("read receipt")
            .expect("attempt");
        assert_eq!(receipt.request_id(), request_id);
        assert_eq!(receipt.plugin_id(), "hook-plugin");
        assert_eq!(
            receipt.phase(),
            if fail {
                InstallPhase::ObservationFailed
            } else {
                InstallPhase::Succeeded
            }
        );
        assert!(
            reopened
                .activation_transaction_ids("hook-plugin")
                .expect("discovery")
                .contains(&request_id)
        );
        executor().block_on(async {
            // Reinspection must never authorize replay of the same completed request.
            // 重新检查绝不能授权重放同一个已完成请求。
            let candidate = reopened
                .inspect_activation_candidate(&transaction)
                .expect("same candidate");
            assert!(
                fixture
                    .manager
                    .install_activation_candidate(&candidate)
                    .await
                    .expect_err("no replay")
                    .message
                    .contains("already attempted")
            );
        });
        assert_eq!(
            reopened
                .installation_receipt("hook-plugin", request_id)
                .expect("same receipt"),
            Some(receipt)
        );
        // Lua emits its actual host-supplied identity through a real native service.
        // Lua 通过真实原生服务发出实际宿主传入身份。
        let topics = fixture
            .events
            .values
            .lock()
            .expect("native events")
            .iter()
            .map(|event| event.topic.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            topics
                .iter()
                .filter(|topic| **topic == format!("install-request:{request_id}"))
                .count(),
            1
        );
        assert_eq!(
            topics
                .iter()
                .filter(|topic| topic.as_str() == if fail { "install-failed" } else { "install" })
                .count(),
            1
        );
        if fail {
            assert_eq!(
                reopened
                    .commit_activation_transaction(&mut transaction)
                    .expect_err("failed installation")
                    .code,
                VulcanPluginStoreErrorCode::Conflict
            );
            assert_eq!(
                reopened.activation("hook-plugin").expect("old selection"),
                predecessor
            );
            reopened
                .abort_activation_transaction(&mut transaction)
                .expect("abort selection only");
            assert_eq!(
                reopened
                    .installation_receipt("hook-plugin", request_id)
                    .expect("retained failure")
                    .expect("historical attempt")
                    .phase(),
                InstallPhase::ObservationFailed
            );
        } else {
            reopened
                .commit_activation_transaction(&mut transaction)
                .expect("confirmed successful attempt");
        }
    }
}

/// Cancelling the observer preserves Started through actual retirement and request recovery without rerunning Lua.
/// 取消观察者会在实际退役及请求恢复期间保留 Started，不重新运行 Lua。
#[test]
fn formal_candidate_install_receipt_cancelled_observer_blocks_recovered_commit() {
    // The native callback deliberately outlives its foreground observer.
    // 原生回调刻意比前台观察者存活更久。
    let (fixture, store, transaction) = installation_fixture(true, false, false);
    // Recovery always selects the original durable request.
    // 恢复始终选择原持久请求。
    let request_id = transaction.record().request_id();
    executor().block_on(async {
        // Only the spawned observer owns the transaction; actual work retains its own request lease.
        // 仅派生观察者拥有事务；实际工作保留自己的请求租约。
        let manager = fixture.manager.clone();
        // Store access itself is not a request lease.
        // 存储访问本身不是请求租约。
        let original_store = store.clone();
        // Cancelling this waiter must not manufacture completion evidence.
        // 取消此等待者不能制造完成证据。
        let observer = tokio::spawn(async move {
            // The view belongs to the exact transaction owned by this observer.
            // 视图属于此观察者拥有的精确事务。
            let candidate = original_store
                .inspect_activation_candidate(&transaction)
                .expect("owned candidate");
            manager.install_activation_candidate(&candidate).await
        });
        fixture.entered().await;
        assert_eq!(
            store
                .installation_receipt("hook-plugin", request_id)
                .expect("live receipt")
                .expect("started before native effect")
                .phase(),
            InstallPhase::Started
        );
        observer.abort();
        assert!(
            observer
                .await
                .expect_err("cancelled observer")
                .is_cancelled()
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_candidate_calls(request_id)
            .await
            .expect("drain actual callback");
        // Actual native references may finish after the drain observer returns.
        // 实际原生引用可能在排空观察者返回之后结束。
        let mut recovered = tokio::time::timeout(WAIT, async {
            loop {
                match store.resume_activation_transaction("hook-plugin", request_id) {
                    Ok(owner) => break owner,
                    Err(error) if error.code == VulcanPluginStoreErrorCode::Busy => {
                        tokio::task::yield_now().await
                    }
                    Err(error) => panic!("unexpected recovery failure: {error}"),
                }
            }
        })
        .await
        .expect("original resource retirement");
        // A new store reads the original intent; empty live call status cannot turn it into success.
        // 新存储读取原意图；空存活调用状态不能将其变成成功。
        let reopened =
            VulcanPluginGenerationStore::open(fixture.manager.user_plugin_root()).expect("reopen");
        assert_eq!(
            reopened
                .installation_receipt("hook-plugin", request_id)
                .expect("durable cancelled receipt")
                .expect("original attempt")
                .phase(),
            InstallPhase::Started
        );
        assert_eq!(
            reopened
                .commit_activation_transaction(&mut recovered)
                .expect_err("uncertain installation")
                .code,
            VulcanPluginStoreErrorCode::Conflict
        );
        {
            // Recovery grants inspection, not permission to replay an uncertain callback.
            // 恢复授予检查，不授予重放不确定回调的权限。
            let candidate = reopened
                .inspect_activation_candidate(&recovered)
                .expect("recovered candidate");
            assert!(
                fixture
                    .manager
                    .install_activation_candidate(&candidate)
                    .await
                    .expect_err("no replay after recovery")
                    .message
                    .contains("already attempted")
            );
        }
        reopened
            .abort_activation_transaction(&mut recovered)
            .expect("explicitly abandon selection");
        assert_eq!(
            reopened
                .installation_receipt("hook-plugin", request_id)
                .expect("history")
                .expect("retained uncertain effects")
                .phase(),
            InstallPhase::Started
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("native events")
                .iter()
                .filter(|event| event.topic == format!("install-request:{request_id}"))
                .count(),
            1
        );
    });
}

/// A real Windows sharing violation after native success must preserve uncertain evidence and refuse publication.
/// 原生成功后的真实 Windows 共享冲突必须保留不确定证据并拒绝发布。
#[cfg(windows)]
#[test]
fn formal_candidate_install_receipt_failed_checkpoint_never_claims_success() {
    use std::os::windows::fs::OpenOptionsExt;
    // The callback waits while the test locks its actual durable Started receipt.
    // 回调等待期间，测试锁定其实际持久 Started 回执。
    let (fixture, store, mut transaction) = installation_fixture(true, false, false);
    // Only this original request may name the checkpoint file.
    // 仅此原请求可以命名检查点文件。
    let request_id = transaction.record().request_id();
    // This is the declared receipt layout, not a candidate search or fallback path.
    // 此处为声明的回执布局，不是候选搜索或备用路径。
    let path = fixture
        .manager
        .user_plugin_root()
        .join(".vulcan-generations/transactions/hook-plugin")
        .join(format!("{request_id}.install"));
    executor().block_on(async {
        // The original view remains borrowed until the attempted final acknowledgement returns.
        // 原视图保持借用，直到最终确认尝试返回。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        // Native success and checkpoint denial execute concurrently under the original request.
        // 原生成功及检查点拒绝在原请求下并发执行。
        let (result, held_file) = tokio::join!(
            fixture.manager.install_activation_candidate(&candidate),
            async {
                fixture.entered().await;
                // Read-only sharing permits inspection but denies atomic replacement of this real receipt.
                // 只读共享允许检查，但拒绝原子替换此真实回执。
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .open(&path)
                    .expect("deny replacement");
                fixture.events.release();
                file
            }
        );
        assert!(
            result
                .expect_err("receipt write failed after successful callback")
                .message
                .contains("receipt is uncertain")
        );
        drop(held_file);
    });
    assert_eq!(
        store
            .installation_receipt("hook-plugin", request_id)
            .expect("readback")
            .expect("retained intent")
            .phase(),
        InstallPhase::Started
    );
    assert_eq!(
        fixture
            .events
            .values
            .lock()
            .expect("native effects")
            .iter()
            .filter(|event| event.topic == "install")
            .count(),
        1
    );
    assert_eq!(
        store
            .commit_activation_transaction(&mut transaction)
            .expect_err("uncertain acknowledgement")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    assert!(
        store
            .activation("hook-plugin")
            .expect("no premature publication")
            .is_none()
    );
}

/// Unknown or mismatched receipt bytes must be preserved and cannot authorize another callback or commit.
/// 必须保留未知或不匹配回执字节，且其不能授权再次回调或提交。
#[test]
fn formal_candidate_install_receipt_invalid_evidence_fails_closed() {
    // Each mutation attacks one independent part of a real durable successful receipt.
    // 每次变更攻击真实持久成功回执的单个独立部分。
    for (field, value) in [
        ("schema_version", json!(u32::MAX)),
        ("plugin_id", json!("another-plugin")),
        ("request_id", json!(Uuid::new_v4().to_string())),
        ("target_revision", json!(u64::MAX)),
        ("phase", json!("unknown")),
    ] {
        // One isolated actual callback supplies authentic initial receipt bytes.
        // 单个隔离实际回调提供真实初始回执字节。
        let (fixture, store, mut transaction) = installation_fixture(false, false, false);
        // The exact original request owns both evidence and activation intent.
        // 精确原请求共同拥有证据及激活意图。
        let request_id = transaction.record().request_id();
        executor().block_on(async {
            // Native execution succeeds once before corrupted recovery is inspected.
            // 检查损坏恢复之前，原生执行成功一次。
            let candidate = store
                .inspect_activation_candidate(&transaction)
                .expect("candidate");
            fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .expect("first install");
        });
        // Access only the declared path of this fixture's original receipt.
        // 仅访问此夹具原回执的声明路径。
        let path = fixture
            .manager
            .user_plugin_root()
            .join(".vulcan-generations/transactions/hook-plugin")
            .join(format!("{request_id}.install"));
        // Preserve all other fields from the production-written receipt.
        // 从生产写入回执保留其他全部字段。
        let mut encoded: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("receipt bytes"))
                .expect("receipt JSON");
        encoded[field] = value;
        // Exact corrupted bytes must remain unchanged after every refused operation.
        // 每次拒绝操作后，精确损坏字节必须保持不变。
        let bytes = serde_json::to_vec(&encoded).expect("encode fixture corruption");
        std::fs::write(&path, &bytes).expect("corrupt fixture receipt");
        assert!(
            store
                .installation_receipt("hook-plugin", request_id)
                .is_err()
        );
        assert!(
            store
                .commit_activation_transaction(&mut transaction)
                .is_err()
        );
        executor().block_on(async {
            // Reinspection still proves the package; it cannot repair or ignore corrupt effect evidence.
            // 重新检查仍证明插件包；其不能修复或忽略损坏副作用证据。
            let candidate = store
                .inspect_activation_candidate(&transaction)
                .expect("unchanged package");
            assert!(
                fixture
                    .manager
                    .install_activation_candidate(&candidate)
                    .await
                    .is_err()
            );
        });
        assert_eq!(std::fs::read(&path).expect("preserved bytes"), bytes);
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("native effects")
                .iter()
                .filter(|event| event.topic == "install")
                .count(),
            1
        );
        assert!(
            store
                .activation("hook-plugin")
                .expect("no selection")
                .is_none()
        );
    }
}
