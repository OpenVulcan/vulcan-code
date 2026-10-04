//! Every original prewarm attempt and its independent closing retain safe identity evidence.
//! 每次原预热尝试及其独立关闭均保留安全身份证据。

use super::*;
use crate::{VulcanPluginNativeIdentityEvent, install_native_identity_observer};

/// Every target attempt survives successful acknowledgement; reusable closing has no last-call attribution.
/// 每次目标尝试跨越成功确认保留；复用关闭不归给最后调用。
#[tokio::test]
async fn native_identity_records_each_prewarm_and_reusable_finalization_before_forget() {
    // Install before original pools and all prewarm operations are admitted.
    // 在原池及全部预热操作入场之前安装。
    let records = Arc::new(Mutex::new(Vec::new()));
    // Only small immutable metadata enters this finite test recorder.
    // 此有限测试记录器仅接收小型不可变元数据。
    let captured = Arc::clone(&records);
    // One guard spans prewarming, acknowledgement and actual resource retirement.
    // 单个守卫覆盖预热、确认及实际资源退役。
    let subscription = install_native_identity_observer(Arc::new(move |event| {
        captured.lock().expect("recorder").push(event);
    }))
    .expect("observer");
    // The production fixture's explicit target permits two separate real initialization attempts.
    // 生产夹具明确目标允许两次独立真实初始化尝试。
    let fixture = Fixture::new(10_000, 100);
    fixture.target_capacity(2);
    // Preserve the original host invocation before starting its real target.
    // 启动真实目标之前保留原宿主调用。
    let prepared = fixture
        .manager
        .prepare_reusable_tool_prewarm(fixture.target(2))
        .expect("target");
    fixture
        .manager
        .start_reusable_tool_prewarm(&prepared.invocation_id, "owner")
        .expect("start target");
    // Existing target observation reads actual retirement, without a second driver.
    // 既有目标观测读取实际退役，不建立第二驱动器。
    let completed = targets::wait_target(&fixture, &prepared.invocation_id).await;
    assert!(completed.error.is_none(), "{completed:?}");
    fixture
        .manager
        .forget_formal_call(&prepared.invocation_id, "owner")
        .expect("acknowledge each attempt");
    fixture.close().await;
    // Read the immutable capture window only after original resources actually close.
    // 原资源实际关闭后才读取不可变采集窗口。
    let records = records.lock().expect("recorder");
    // Stable operation IDs identify each distinct prewarm attempt despite repeated observation.
    // 即使存在重复观测，稳定操作身份仍标识每次不同预热尝试。
    let mut attempts = BTreeSet::new();
    // Independent finalizers identify real instances, never the previous host call.
    // 独立关闭器标识真实实例，绝不标识此前宿主调用。
    let mut instances = BTreeSet::new();
    for event in records.iter() {
        if let VulcanPluginNativeIdentityEvent::OperationBound {
            operation_id,
            prewarm,
            core_session_id,
            host_invocation_id,
            trusted_request_id,
            controller_session_id,
            finalization_instance_id,
            ..
        } = event
        {
            if *prewarm {
                assert_eq!(
                    host_invocation_id.as_deref(),
                    Some(prepared.invocation_id.as_str())
                );
                assert_eq!(controller_session_id.as_deref(), Some("owner"));
                assert!(core_session_id.is_none());
                attempts.insert(operation_id.clone());
                assert!(
                    fixture
                        .owner()
                        .host
                        .runtime()
                        .operation(operation_id)
                        .is_err(),
                    "acknowledged original attempt must be forgotten"
                );
            }
            if let Some(instance) = finalization_instance_id {
                assert!(host_invocation_id.is_none());
                assert!(trusted_request_id.is_none());
                instances.insert(instance.clone());
            }
        }
    }
    assert_eq!(attempts.len(), 2, "expected every actual target attempt");
    assert_eq!(instances.len(), 2, "expected each independent finalization");
    assert_eq!(subscription.observation_failures(), 0);
}
