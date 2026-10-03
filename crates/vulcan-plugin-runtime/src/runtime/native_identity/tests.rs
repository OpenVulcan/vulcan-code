//! Privacy, lazy observation and exact subscription lifetime regression tests.
//! 隐私、惰性观测及精确订阅寿命回归测试。

use super::*;
use luaskills::runtime::embedded::capabilities::CapabilityCaller;
use luaskills::runtime::embedded::{EffectState, ModuleOperationContext, OperationPhase};
use serde_json::json;

/// Return a module snapshot carrying deliberately private context and business data for projection checks.
/// 返回携带刻意私有上下文及业务数据的模块快照，用于投影检查。
fn private_snapshot() -> OperationSnapshot {
    OperationSnapshot {
        finalization: None,
        context: OperationContext::Module(Box::new(ModuleOperationContext {
            prewarm: false,
            finalization_instance_id: Some("original-vm".into()),
            pool_id: "original-pool".into(),
            caller: CapabilityCaller {
                runtime_id: "original-runtime".into(),
                plugin_id: "original-plugin".into(),
                package_generation: "42".into(),
                execution_revision: "original-revision".into(),
                security_partition: "private-partition-marker".into(),
                operation_id: "original-operation".into(),
                request_id: None,
                session_id: Some("core-session".into()),
                workspace_root: Some("private-workspace-marker".into()),
            },
            capability_revision: "private-capabilities-marker".into(),
            export: Some("close".into()),
        })),
        host_effects: Vec::new(),
        operation_id: "original-operation".into(),
        phase: OperationPhase::Succeeded,
        cancellation_requested: false,
        effects: EffectState::Committed,
        value: Some(json!({"secret":"private-business-marker"})),
        error: None,
    }
}

/// Whitelist projection preserves separate original identities while omitting private context and business output.
/// 白名单投影保留分别保存的原身份，同时排除私有上下文及业务输出。
#[test]
fn native_identity_projection_preserves_original_nullable_owners_and_privacy() {
    // Only serialized whitelist records are retained by this test consumer.
    // 此测试消费者仅保留序列化白名单记录。
    let records = Arc::new(Mutex::new(Vec::new()));
    // The callback keeps only the bounded fixture recorder.
    // 回调仅保留有界夹具记录器。
    let captured = Arc::clone(&records);
    // A local sink avoids any dependence on a process subscription in this pure projection test.
    // 局部接收器使纯投影测试不依赖进程订阅。
    let sink = Sink {
        callback: Arc::new(move |event| {
            captured
                .lock()
                .expect("records")
                .push(serde_json::to_value(event).expect("safe record"))
        }),
        failures: AtomicU64::new(0),
    };
    // Closing has an actual Core session but no business request or host invocation.
    // 关闭具有实际 Core 会话，但没有业务请求或宿主调用。
    let snapshot = private_snapshot();
    sink.operation(
        "original-runtime",
        &snapshot,
        NativeIdentityOwner {
            plugin_generation: 42,
            controller_session_id: Some("controller-session"),
            host_invocation_id: None,
            host_session_id: Some("host-session"),
        },
    );
    // Read the only emitted immutable record after projection.
    // 投影后读取唯一发送的不可变记录。
    let records = records.lock().expect("records");
    // Locate by the stable event discriminator, never a mutable structural index.
    // 通过稳定事件判别值定位，绝不使用可变结构下标。
    let record = records
        .iter()
        .find(|record| record["event"] == "operation_bound")
        .expect("original operation binding");
    assert_eq!(record["core_session_id"], "core-session");
    assert_eq!(record["controller_session_id"], "controller-session");
    assert_eq!(record["host_session_id"], "host-session");
    assert_eq!(record["host_invocation_id"], json!(null));
    assert_eq!(record["trusted_request_id"], json!(null));
    assert_eq!(record["plugin_generation"], 42);
    assert_eq!(record["package_generation"], "42");
    assert_eq!(record["finalization_instance_id"], "original-vm");
    // The serialized record cannot accidentally include any source-private marker.
    // 序列化记录不能意外包含任何源码私有标记。
    let serialized = serde_json::to_string(record).expect("whitelist JSON");
    assert!(!serialized.contains("private-"));
    assert_eq!(sink.failures.load(Ordering::Acquire), 0);
}

/// Disabled observation stays lazy, in-flight snapshots survive detach, and old guards never revoke a newer subscription.
/// 禁用观测保持惰性，在途快照跨越撤销存活，旧守卫绝不撤销新订阅。
#[test]
fn native_identity_subscription_detaches_exact_owner_and_counts_panic() {
    observe_pool(|| panic!("disabled observer must not construct metadata"));
    // Count caught callback panics without making any business path fail.
    // 计入已捕获回调 panic，不使任何业务路径失败。
    let subscription =
        install_native_identity_observer(Arc::new(|_| panic!("controlled sink panic")))
            .expect("install original");
    assert!(install_native_identity_observer(Arc::new(|_| {})).is_err());
    // Simulate an observation that upgraded the original weak slot before detachment.
    // 模拟撤销前已经升级原弱槽的观测。
    let pending = sink().expect("original snapshot");
    subscription.detach();
    assert_eq!(subscription.pending_observations(), 1);
    pending.unavailable("original-runtime", "original-operation", None);
    assert_eq!(subscription.observation_failures(), 2);
    drop(pending);
    assert_eq!(subscription.pending_observations(), 0);
    // A new service window may begin after detachment; dropping the old guard cannot clear it.
    // 撤销后新服务窗口可以开始；丢弃旧守卫不能清除它。
    let replacement =
        install_native_identity_observer(Arc::new(|_| {})).expect("install replacement");
    drop(subscription);
    assert!(sink().is_some());
    replacement.detach();
    assert!(sink().is_none());
    assert_eq!(replacement.observation_failures(), 0);
}

/// Explicit unbound Core context is reported as unavailable, without fabricated module ownership.
/// 明确未绑定 Core 上下文报告为不可取得，不虚构模块所有权。
#[test]
fn native_identity_unbound_context_counts_incomplete_capture() {
    // One local sink verifies the missing-authority event independently of global installation.
    // 单个局部接收器独立于全局安装验证缺失权威事件。
    let sink = Sink {
        callback: Arc::new(|event| {
            assert!(matches!(
                event,
                VulcanPluginNativeIdentityEvent::ObservationUnavailable {
                    error_code: None,
                    ..
                }
            ))
        }),
        failures: AtomicU64::new(0),
    };
    // Keep the known operation ID but remove authority explicitly.
    // 保持已知操作身份，但明确移除权威。
    let mut snapshot = private_snapshot();
    snapshot.context = OperationContext::Unbound;
    sink.operation(
        "original-runtime",
        &snapshot,
        NativeIdentityOwner {
            plugin_generation: 42,
            controller_session_id: None,
            host_invocation_id: None,
            host_session_id: None,
        },
    );
    assert_eq!(sink.failures.load(Ordering::Acquire), 1);
}
