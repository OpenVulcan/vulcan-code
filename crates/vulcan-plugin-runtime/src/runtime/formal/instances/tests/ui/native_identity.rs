//! Original transient Action and persistent Hook/UI identities remain separate after successful cleanup.
//! 成功清理后，原瞬时 Action 及持久 Hook/UI 身份保持独立。

use super::*;
use crate::{VulcanPluginNativeIdentityEvent, install_native_identity_observer};

/// Transient Action keeps its real request and invocation; internal Hook/UI never fabricate controller ownership.
/// 瞬时 Action 保留真实请求及调用；内部 Hook/UI 绝不虚构控制器所有权。
#[test]
fn native_identity_action_hook_and_ui_keep_original_internal_ownership() {
    // Record only safe metadata before any production instance admission begins.
    // 任何生产实例入场之前，仅记录安全元数据。
    let records = Arc::new(Mutex::new(Vec::new()));
    // The service callback owns only the finite evidence vector.
    // 服务回调仅拥有有限证据向量。
    let captured = Arc::clone(&records);
    // The same subscription covers independent transient and persistent native domains.
    // 同一订阅覆盖独立瞬时及持久原生域。
    let subscription = install_native_identity_observer(Arc::new(move |event| {
        captured.lock().expect("identity recorder").push(event);
    }))
    .expect("identity observer");
    // Existing UI fixture declares the real bundle and persistent Hook entry.
    // 既有 UI 夹具声明真实 bundle 及持久 Hook 入口。
    let fixture = Fixture::ui(10_000, false, VulcanPluginRuntimeSettings::default());
    // The observer owns no executor; use the existing finite production-test executor.
    // 观察器不拥有执行器；使用既有有限生产测试执行器。
    let runtime = executor();
    // Retain only the real UI session string for post-forget identity verification.
    // 仅保留真实 UI 会话字符串，用于遗忘后的身份验证。
    let ui_session = runtime.block_on(async {
        // Transient UI dispatch uses ordinary Action initialization plus an independent business operation.
        // 瞬时 UI 分发使用普通 Action 初始化及独立业务操作。
        let mut transient = fixture.ui_request(json!({}));
        transient.reuse_enabled_runtime = false;
        fixture
            .manager
            .invoke_action(transient)
            .await
            .expect("transient Action");
        fixture
            .manager
            .invoke_action(fixture.ui_request(json!({})))
            .await
            .expect("persistent UI");
        // Read the sole UI instance created by the successful production invocation.
        // 读取成功生产调用创建的唯一 UI 实例。
        let ui_session = {
            // The existing manager supplies the original formal parent.
            // 既有管理器提供原正式父级。
            let owner = fixture.owner();
            // Clone the exact declared UI row only while reading its recorded Core session.
            // 仅在读取已记录 Core 会话时克隆精确声明的 UI 行。
            let original = owner
                .ui_actions
                .lock()
                .expect("original UI registry")
                .get("hook-plugin")
                .expect("successful invocation must retain the declared UI instance")
                .clone();
            // Initialization owns this real session; no identity is inferred from callback ordering.
            // 初始化持有此真实会话，不根据回调顺序推导身份。

            original
                .lock()
                .expect("original UI state")
                .session
                .clone()
                .expect("successful UI invocation must have its original Core session")
        };
        fixture
            .manager
            .prewarm_plugin_runtime("hook-plugin")
            .await
            .expect("persistent Hook initialization");
        fixture
            .manager
            .invoke_hook(&fixture.hook(), fixture.invocation(json!({}), None))
            .await
            .expect("internal Hook business");
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("original drain");
        // Real cleanup must finish before its original operation records can be forgotten.
        // 原操作记录可以遗忘之前，真实清理必须完成。
        let report = fixture
            .manager
            .drain_plugin_sessions("hook-plugin")
            .await
            .expect("physical drain");
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        ui_session
    });
    // The original parent supplies exact IDs even after all internal host rows have been reclaimed.
    // 全部内部宿主行回收后，原父级仍提供精确身份。
    let owner = fixture
        .manager
        .formal_runtime_owner()
        .expect("parent")
        .expect("attached");
    // Examine retained immutable events after actual production cleanup.
    // 实际生产清理后检查保留不可变事件。
    let records = records.lock().expect("identity recorder");
    // Core sessions distinguish Action and persistent owners without parsing any security partition.
    // Core 会话区分 Action 及持久所有者，不解析任何安全分区。
    let mut internal_sessions = BTreeSet::new();
    // Both transient initialization and business must share one real host invocation.
    // 瞬时初始化及业务必须共享单个真实宿主调用。
    let mut action_invocations = BTreeSet::new();
    // The trusted Action request exists only on its actual business operation.
    // 可信 Action 请求仅存在于实际业务操作。
    let mut action_request = false;
    // Only the actual UI business request can satisfy this set, never UI initialization.
    // 仅实际 UI 业务请求能够满足此集合，UI 初始化不能替代。
    let mut ui_business_operations = BTreeSet::new();
    // Hook independent closing has real instance attribution and no inherited request.
    // Hook 独立关闭具有真实实例归属，不继承请求。
    let mut independent_closing = false;
    for event in records.iter() {
        if let VulcanPluginNativeIdentityEvent::OperationBound {
            operation_id,
            core_session_id,
            controller_session_id,
            host_invocation_id,
            host_session_id,
            trusted_request_id,
            finalization_instance_id,
            ..
        } = event
        {
            assert!(
                controller_session_id.is_none(),
                "internal ownership cannot invent controller"
            );
            assert!(host_session_id.is_none());
            if let Some(invocation) = host_invocation_id {
                action_invocations.insert(invocation.clone());
                action_request |= trusted_request_id.is_some();
            } else if let Some(session) = core_session_id {
                internal_sessions.insert(session.clone());
            }
            if finalization_instance_id.is_some() {
                independent_closing = true;
                assert!(trusted_request_id.is_none());
                assert!(host_invocation_id.is_none());
            }
            if core_session_id.as_ref() == Some(&ui_session) && trusted_request_id.is_some() {
                ui_business_operations.insert(operation_id.clone());
                assert!(host_invocation_id.is_none());
                assert!(finalization_instance_id.is_none());
            }
            assert!(
                owner.host.runtime().operation(operation_id).is_err(),
                "successful original internal operation must be forgotten"
            );
        }
    }
    assert_eq!(
        action_invocations.len(),
        1,
        "expected one real transient Action invocation"
    );
    assert!(
        action_request,
        "expected trusted transient business request"
    );
    assert_eq!(
        internal_sessions.len(),
        2,
        "expected separate persistent Hook and UI Core sessions"
    );
    assert_eq!(
        ui_business_operations.len(),
        1,
        "expected the actual persistent UI business operation and trusted request after its original operation was forgotten"
    );
    assert!(independent_closing);
    assert_eq!(subscription.observation_failures(), 0);
}
