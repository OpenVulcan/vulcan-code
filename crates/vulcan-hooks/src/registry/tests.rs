//! Behavioral checks for immutable registration partition projection.
//! 不可变注册分区投影的行为检查。

use super::*;
use crate::{
    FnHookHandler, HookContext, HookDispatchRequest, HookDispatcher, HookInvocation, HookMode,
    HookPayload, HookResult,
};
use std::sync::Mutex;

/// Registers `id` at `point` and records the actual `label` handler identity into `calls`.
/// 在 `point` 注册 `id`，并向 `calls` 记录实际 `label` 处理器身份。
/// Returns after registration; fixture errors fail the test.
/// 注册后返回；夹具错误使测试失败。
fn record(
    registry: &mut HookRegistry,
    id: &str,
    point: &str,
    label: &'static str,
    calls: &Arc<Mutex<Vec<&'static str>>>,
) {
    // Each closure owns the shared observation sink independently of registry lifetime.
    // 每个闭包独立于注册表寿命持有共享观察容器。
    let calls = Arc::clone(calls);
    registry
        .register(
            HookRegistration::new(id, point, HookMode::Observe),
            Arc::new(FnHookHandler::new(move |_invocation: HookInvocation| {
                // The future retains the exact captured handler identity.
                // 异步任务保留精确捕获的处理器身份。
                let calls = Arc::clone(&calls);
                async move {
                    calls
                        .lock()
                        .expect("fixture runtime or observation lock should remain healthy")
                        .push(label);
                    Ok(HookResult::default())
                }
            })),
        )
        .expect("hook generation fixture operation should succeed");
}

/// Dispatches `registry` at `point`, returning only after every actual handler completes.
/// 在 `point` 派发 `registry`，仅在全部实际处理器完成后返回。
async fn dispatch(registry: HookRegistry, point: &str) {
    HookDispatcher::new(registry)
        .dispatch(HookDispatchRequest::new(
            HookContext::new("partition-test", point),
            HookPayload::from_serializable(serde_json::json!({}))
                .expect("hook generation fixture operation should succeed"),
        ))
        .await
        .expect("hook generation fixture operation should succeed");
}

/// Checks source handler identity, insertion order, host preservation, and immutable inputs.
/// 检查来源处理器身份、插入顺序、宿主保留及输入不可变性。
#[tokio::test]
async fn partition_projection_preserves_handlers_order_and_inputs() {
    // All dispatches share an observation sink to distinguish handlers with the same registration ID.
    // 全部派发共用观察容器，以区分注册标识相同的处理器。
    let calls = Arc::new(Mutex::new(Vec::new()));
    // The live registry contains a host entry interleaved with replaced plugin registrations.
    // 实时注册表包含与被替换插件注册项交错的宿主条目。
    let mut live = HookRegistry::new();
    record(&mut live, "plugin/same", "point", "live", &calls);
    record(&mut live, "host", "point", "host", &calls);
    record(&mut live, "plugin/remove", "point", "removed", &calls);
    // Source order intentionally differs from registration ID and point name ordering.
    // 来源顺序故意与注册标识及点位名称排序不同。
    let mut source = HookRegistry::new();
    record(&mut source, "plugin/z", "point", "first", &calls);
    record(&mut source, "other-host", "point", "excluded", &calls);
    record(
        &mut source,
        "plugin/other-point",
        "aaa",
        "other-point",
        &calls,
    );
    record(&mut source, "plugin/same", "point", "captured", &calls);
    // Projection clones handlers rather than reconstructing metadata-only registrations.
    // 投影克隆处理器，而非仅依据元数据重建注册项。
    let projected = live
        .with_registration_prefix_from("plugin/", &source)
        .expect("hook generation fixture operation should succeed");
    dispatch(projected, "point").await;
    assert_eq!(
        *calls
            .lock()
            .expect("fixture runtime or observation lock should remain healthy"),
        ["host", "first", "captured"]
    );
    calls
        .lock()
        .expect("fixture runtime or observation lock should remain healthy")
        .clear();
    dispatch(live, "point").await;
    assert_eq!(
        *calls
            .lock()
            .expect("fixture runtime or observation lock should remain healthy"),
        ["live", "host", "removed"]
    );
    calls
        .lock()
        .expect("fixture runtime or observation lock should remain healthy")
        .clear();
    dispatch(source, "point").await;
    assert_eq!(
        *calls
            .lock()
            .expect("fixture runtime or observation lock should remain healthy"),
        ["first", "excluded", "captured"]
    );
}

/// Rejects incompatible live point policy and an empty namespace without changing inputs.
/// 拒绝不兼容实时点位策略及空命名空间，不修改输入。
#[test]
fn partition_projection_failure_preserves_original_registry() {
    // Observation-only host policy must remain authoritative over source metadata.
    // 仅允许观察的宿主策略必须继续约束来源元数据。
    let mut live = HookRegistry::new();
    live.register_point(HookPointDescriptor::new(
        "point",
        [HookMode::Observe],
        "test point",
    ))
    .expect("hook generation fixture operation should succeed");
    // Source may independently allow transforms; projection must still reject them.
    // 来源可独立允许转换；投影仍必须拒绝它们。
    let mut source = HookRegistry::new();
    source
        .register(
            HookRegistration::new("plugin/invalid", "point", HookMode::Transform),
            Arc::new(FnHookHandler::new(|_: HookInvocation| async {
                Ok(HookResult::default())
            })),
        )
        .expect("hook generation fixture operation should succeed");
    assert!(
        live.with_registration_prefix_from("plugin/", &source)
            .is_err()
    );
    assert!(live.with_registration_prefix_from("", &source).is_err());
    assert_eq!(live.count_for("point"), 0);
    assert_eq!(source.count_for("point"), 1);
}

/// Rejects sequence exhaustion before inserting a handler or publishing a projected subset.
/// 在插入处理器或发布局部投影前拒绝序号耗尽。
#[test]
fn partition_projection_sequence_exhaustion_is_atomic() {
    // The source has one valid entry and the destination has exhausted its sequence space.
    // 来源有一个有效条目，目标的序号空间已耗尽。
    let calls = Arc::new(Mutex::new(Vec::new()));
    // Construct the boundary directly; normal workloads need not loop through all sequences.
    // 直接构造边界；常规负载无须遍历全部序号。
    let mut live = HookRegistry {
        next_sequence: u64::MAX,
        ..HookRegistry::new()
    };
    // The source remains reusable after a failed projection.
    // 投影失败后来源仍可复用。
    let mut source = HookRegistry::new();
    record(&mut source, "plugin/one", "point", "one", &calls);
    assert!(
        live.with_registration_prefix_from("plugin/", &source)
            .is_err()
    );
    assert!(
        live.register(
            HookRegistration::new("host", "point", HookMode::Observe),
            Arc::new(FnHookHandler::new(|_: HookInvocation| async {
                Ok(HookResult::default())
            }))
        )
        .is_err()
    );
    assert_eq!(live.count_for("point"), 0);
    assert_eq!(live.next_sequence, u64::MAX);
}
