//! Original pinned identity remains observable after successful Core records are forgotten.
//! 成功 Core 记录遗忘后，原固定身份仍可观测。

use super::*;
use crate::{VulcanPluginNativeIdentityEvent, install_native_identity_observer};

/// One service observer covers multiple managers and keeps Core, controller and host sessions distinct after forget.
/// 单个服务观察器覆盖多个管理器，并在遗忘后保持 Core、控制器及宿主会话各自独立。
#[test]
fn native_identity_pinned_original_operations_survive_forget_across_managers() {
    // This finite recorder owns safe events only, never the native owners.
    // 此有限记录器仅拥有安全事件，绝不拥有原生所有者。
    let records = Arc::new(Mutex::new(Vec::new()));
    // The service installs once before either manager starts native admission.
    // 服务在两个管理器开始原生入场之前仅安装一次。
    let captured = Arc::clone(&records);
    // Keep the same guard alive across both independent parent runtimes.
    // 在两个独立父运行时期间保持同一守卫存活。
    let subscription = install_native_identity_observer(Arc::new(move |event| {
        captured.lock().expect("identity recorder").push(event);
    }))
    .expect("single service observer");
    // Exact parent identities must differ while their observation sink remains shared.
    // 精确父级身份必须不同，而其观测接收器保持共享。
    let mut runtimes = BTreeSet::new();
    for _ in 0..2 {
        // Existing production fixtures exercise actual initialization, business and closing.
        // 既有生产夹具覆盖实际初始化、业务及关闭。
        let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
        fixture.runtime.block_on(async {
            // The host session is returned by the actual production tool route.
            // 宿主会话由实际生产工具路径返回。
            let id = opened(&fixture).await;
            // Retain the real parent for later NotFound checks without adding historical ownership.
            // 保留实际父级用于后续 NotFound 检查，不新增历史所有权。
            let owner = fixture
                .manager
                .formal_runtime_owner()
                .expect("parent")
                .expect("attached");
            runtimes.insert(owner.host.runtime().id().to_owned());
            fixture
                .manager
                .invoke(request(&fixture, "idle_call", Some(&id), "ok"))
                .await
                .expect("business success");
            fixture
                .manager
                .invoke(request(&fixture, "idle_close", Some(&id), "ok"))
                .await
                .expect("closing success");
            assert!(
                fixture
                    .manager
                    .formal_sessions("owner")
                    .expect("host acknowledgement")
                    .is_empty()
            );
            // Look up exact original bindings by host identity, never current catalog or structural index.
            // 按宿主身份查找精确原绑定，绝不读取当前目录或结构下标。
            let records = records.lock().expect("identity recorder");
            // A set deduplicates legal repeated observations while preserving each distinct original operation.
            // 集合消除合法重复观测，同时保留每个不同原操作。
            let mut operations = BTreeSet::new();
            // Independent closing must keep request absent and identify its original instance.
            // 独立关闭必须保持请求为空并标识原实例。
            let mut closing = false;
            for event in records.iter() {
                if let VulcanPluginNativeIdentityEvent::OperationBound {
                    runtime_id,
                    operation_id,
                    core_session_id,
                    controller_session_id,
                    host_session_id,
                    host_invocation_id,
                    trusted_request_id,
                    finalization_instance_id,
                    ..
                } = event
                {
                    if host_session_id.as_deref() != Some(&id) {
                        continue;
                    }
                    assert_eq!(runtime_id, owner.host.runtime().id());
                    assert_eq!(controller_session_id.as_deref(), Some("owner"));
                    assert_ne!(core_session_id.as_deref(), Some(id.as_str()));
                    assert!(core_session_id.is_some());
                    assert!(host_invocation_id.is_none());
                    if finalization_instance_id.is_some() {
                        closing = true;
                        assert!(trusted_request_id.is_none());
                    }
                    operations.insert(operation_id.clone());
                    assert!(
                        owner.host.runtime().operation(operation_id).is_err(),
                        "original successful operation must already be forgotten"
                    );
                }
            }
            assert!(closing, "expected original independent closing binding");
            assert_eq!(
                operations.len(),
                4,
                "expected initialization, opening, business and independent closing"
            );
        });
    }
    assert_eq!(runtimes.len(), 2);
    subscription.detach();
    assert_eq!(subscription.pending_observations(), 0);
    assert_eq!(subscription.observation_failures(), 0);
}
