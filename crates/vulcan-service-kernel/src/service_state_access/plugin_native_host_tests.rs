//! Service reads preserve the actual first native issuer and reject incoherent manager replacement.
//! 服务读取保留实际首次原生签发者，并拒绝不一致管理器替换。

use super::*;
use crate::tests::test_support::{
    test_agent_runtime_bridge, test_service_state_with_parent_session,
};
use std::sync::Mutex;

/// Verify bridge clones, repeated reads and actual replacement use one coherent original manager and issuer pair.
/// 校验桥克隆、重复读取及实际替换使用单个一致原管理器和签发者配对。
/// No inputs or return value; real registrations remain opaque and never grant startup by copied identifiers.
/// 无输入及返回值；真实注册保持不透明，绝不由复制标识授予启动。
#[test]
fn service_native_host_pair_reuses_first_issuer_and_rejects_manager_only_replacement() {
    // This established fixture genuinely registers its first host before publishing its bridge.
    // 此既有夹具在发布桥之前真实注册其首次宿主。
    let original = test_agent_runtime_bridge();
    // Cloning a runtime bridge must retain the same issuer rather than register another incarnation.
    // 克隆运行时桥必须保留同一签发者，而非注册另一实例。
    let clone = original.clone();
    assert!(
        original
            .vulcan_plugin_native_mcp_host
            .same_issuer(&clone.vulcan_plugin_native_mcp_host)
    );
    // Both objects come from the actual same original manager, but a repeat registration remains distinct.
    // 两个对象来自实际同一原管理器，但重复注册仍保持不同。
    let another = original
        .vulcan_plugin_runtime
        .register_native_mcp_host_adapter();
    assert!(!original.vulcan_plugin_native_mcp_host.same_issuer(&another));
    // The established shared state owns exactly this actual bridge pair.
    // 既有共享状态拥有精确此实际桥配对。
    let mut service = test_service_state_with_parent_session(uuid::Uuid::new_v4());
    service.agent_runtime = Some(original.clone());
    // This genuine publication mutex is the same one used by production getters and updates.
    // 此真实发布互斥量与生产读取器和更新使用的互斥量相同。
    let state = Arc::new(Mutex::new(service));
    for _ in 0..2 {
        // Each read must clone the first issuer from the same state generation.
        // 每次读取须从同一状态代次克隆首次签发者。
        let (manager, authority) =
            service_vulcan_plugin_native_mcp_host(&state).expect("actual original pair");
        assert!(authority.same_issuer(&original.vulcan_plugin_native_mcp_host));
        assert!(authority.matches_manager(&manager));
    }
    // A second real manager is distinct even though its empty catalog has the same revision values.
    // 即使空目录具有相同修订值，第二个真实管理器仍保持不同。
    let replacement = test_agent_runtime_bridge();
    {
        // Deliberately replace only the manager to exercise explicit incoherence rejection.
        // 刻意仅替换管理器，以执行明确不一致拒绝。
        let mut service = state.lock().expect("actual state lock");
        service
            .agent_runtime
            .as_mut()
            .expect("actual bridge")
            .vulcan_plugin_runtime = replacement.vulcan_plugin_runtime.clone();
    }
    assert!(
        service_vulcan_plugin_native_mcp_host(&state)
            .err()
            .expect("manager-only replacement must be rejected")
            .contains("disagrees with the native host issuer")
    );
    {
        // Pair the same replacement issuer under the original state lock, without registering a third host.
        // 在原状态锁内配对同一替代签发者，不注册第三个宿主。
        let mut service = state.lock().expect("actual state lock");
        service
            .agent_runtime
            .as_mut()
            .expect("actual bridge")
            .vulcan_plugin_native_mcp_host = replacement.vulcan_plugin_native_mcp_host.clone();
    }
    // The repaired pair exposes only the actual replacement; the old issuer cannot authorize its manager.
    // 修复配对仅暴露实际替代实例；旧签发者不能授权其管理器。
    let (manager, authority) =
        service_vulcan_plugin_native_mcp_host(&state).expect("coherent actual replacement");
    assert!(authority.same_issuer(&replacement.vulcan_plugin_native_mcp_host));
    assert!(!authority.same_issuer(&original.vulcan_plugin_native_mcp_host));
    assert!(
        !original
            .vulcan_plugin_native_mcp_host
            .matches_manager(&manager)
    );
}
