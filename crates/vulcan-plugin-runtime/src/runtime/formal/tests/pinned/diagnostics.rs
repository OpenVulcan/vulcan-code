//! Authentic pinned counts, immutable generation selection and safe terminal evidence.
//! 真实固定计数、不可变代次选择及安全终态证据。

use super::*;
use crate::runtime::formal::tests::diagnostics::observed;

/// Query exact pinned-tool identity and generation while excluding other plugins and generations.
/// 查询精确固定工具身份及代次，同时排除其他插件及代次。
#[test]
fn formal_resource_diagnostics_pin_count_and_generation_are_original() {
    // Existing production fixture creates the real pinned session through normal tool admission.
    // 既有生产夹具通过正常工具准入创建真实固定会话。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        // Opening returns the public host identity, distinct from core session and pool IDs.
        // 开启返回公开宿主身份，与核心会话及池 ID 区分。
        let id = opened(&fixture).await;
        // Read the original descriptor generation from authoritative session status.
        // 从权威会话状态读取原描述符代次。
        let status = fixture
            .manager
            .formal_session_status(&id, "owner")
            .expect("original host status");
        // Explicit generation selects this exact ownership rather than current same-name routing.
        // 明确代次选择此精确所有权，而非当前同名路由。
        let observation = observed(
            &fixture.manager,
            "idle-plugin",
            Some(status.plugin_generation),
        )
        .await;
        assert_eq!(observation.pinned_session_count, 1);
        assert!(observation.non_atomic_snapshot);
        // Resource IDs provide feature lookup without unstable sequence assertions.
        // 资源 ID 提供特征查找，避免不稳定序号断言。
        let row = observation
            .resources
            .iter()
            .find(|row| row.resource_id == id)
            .expect("expected pinned identity may have changed");
        assert_eq!(row.resource_kind, VulcanPluginResourceKind::PinnedSession);
        assert_eq!(row.owner_session_id.as_deref(), Some("owner"));
        assert_eq!(row.plugin_generation, status.plugin_generation.to_string());
        // Live native snapshot proves the two independently issued identities.
        // 实时原生快照证明两个独立签发身份。
        let core = status.core.expect("original native session");
        assert_eq!(row.core_session_id.as_ref(), Some(&core.session_id));
        assert_eq!(row.original_pool_id.as_ref(), Some(&core.pool_id));
        assert!(row.is_drain_blocking);
        assert!(
            fixture
                .manager
                .formal_plugin_resource_diagnostics("another-plugin", None)
                .expect("other plugin")
                .expect("parent")
                .resources
                .is_empty()
        );
        assert!(
            fixture
                .manager
                .formal_plugin_resource_diagnostics(
                    "idle-plugin",
                    Some(
                        status
                            .plugin_generation
                            .checked_add(1)
                            .expect("next distinct generation")
                    )
                )
                .expect("other generation")
                .expect("parent")
                .resources
                .is_empty()
        );
        fixture
            .manager
            .invoke(request(&fixture, "idle_close", Some(&id), "ok"))
            .await
            .expect("close original");
    });
}

/// Physical retirement removes pinned ownership from counts while failure metadata never leaks its values or messages.
/// 物理退役从计数中移除固定所有权，失败元数据绝不泄露其值或文案。
#[test]
fn formal_resource_diagnostics_retired_pinned_history_is_safe_and_nonblocking() {
    // An actual failed independent shutdown keeps the original host record until acknowledgement.
    // 实际失败的独立关闭保留原宿主记录，直到确认。
    let fixture = Fixture::new(VulcanPluginRuntimeSettings::default());
    fixture.runtime.block_on(async {
        // Real fixture business sets the failing finalizer in the original VM.
        // 真实夹具业务在原 VM 中设置失败关闭函数。
        let opened = fixture
            .manager
            .invoke(request(&fixture, "idle_open", None, "shutdown_error"))
            .await
            .expect("original opening");
        // Session identity is taken from the actual public opening result.
        // 会话身份从实际公开开启结果取得。
        let id = opened["session_id"].as_str().expect("real host identity");
        fixture
            .manager
            .invoke(request(&fixture, "idle_close", Some(id), "ok"))
            .await
            .expect("physical close");
        retired(&fixture, id).await;
        // Management retains readonly safe history, but no fixed live tool count.
        // 管理保留只读安全历史，但不保留固定活动工具计数。
        let observation = observed(&fixture.manager, "idle-plugin", None).await;
        assert_eq!(observation.pinned_session_count, 0);
        // Lookup by original identity remains independent from evolving record order.
        // 按原身份查找，保持独立于演进记录顺序。
        let row = observation
            .resources
            .iter()
            .find(|row| row.resource_id == id)
            .expect("expected shutdown failure history may have changed");
        assert!(row.retired);
        assert!(!row.is_drain_blocking);
        assert!(!row.cleanup_recovery);
        assert!(row.finalization_operation_id.is_some());
        // Serialized output cannot carry the fixture's application error marker or raw values.
        // 序列化输出不能携带夹具应用错误标记或原始值。
        let encoded = serde_json::to_string(&observation).expect("safe DTO serialization");
        for forbidden in [
            "shutdown-marker",
            "payload_json",
            "host_effects",
            "\"value\"",
            "\"message\"",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "diagnostic leaked {forbidden}"
            );
        }
        fixture
            .manager
            .forget_formal_session(id, "owner")
            .expect("acknowledge original history");
    });
}
