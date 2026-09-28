//! Real host capacity membership preserves catalog authority and module generations.
//! 真实宿主容量成员关系保留目录权威及模块代次。

use super::*;
use luaskills::runtime::embedded::{EmbeddedCapacityConfig, VmCapacityConfig};

/// Derive complete capacity limits from fixture's registered plugin; return its explicit dedicated policy.
/// 从 fixture 已注册插件派生完整容量限制；返回其显式专用策略。
fn capacity_config(fixture: &Fixture) -> EmbeddedCapacityConfig {
    // The actual aggregate is the only source of maxima in this fixture.
    // 实际聚合预算是此夹具上限的唯一来源。
    let plugin = fixture
        .host
        .runtime()
        .plugin("hook-plugin")
        .expect("plugin");
    EmbeddedCapacityConfig {
        resources: VmCapacityConfig {
            kind: PoolKind::Dedicated,
            min_resident_vms: 1,
            max_resident_vms: plugin.config.max_resident_vms,
            max_running_calls: 1,
        },
        max_queued_calls: plugin.config.max_queued_calls,
        max_queued_bytes: plugin.config.max_queued_bytes,
    }
}

/// Both bundle generations share one physical owner while keeping state and live catalog authority separate.
/// 两个 bundle 代次共享单个物理所有者，同时保持状态及实时目录权威独立。
#[test]
fn restricted_bundle_capacity_preserves_generation_authority_and_retirement() {
    // Keep the actual manager, catalog, native services and Lua engine alive throughout cleanup.
    // 在整个清理期间保持真实管理器、目录、原生服务及 Lua 引擎存活。
    let fixture = Fixture::new();
    // One capacity identity survives replacement of the bundle generation.
    // 单个容量身份在 bundle 代次替换期间保持存活。
    let capacity = fixture
        .host
        .runtime()
        .register_capacity("hook-plugin", capacity_config(&fixture))
        .expect("capacity");
    // Each module owns its own counter and uses actual native configuration on every call.
    // 每个模块拥有独立计数器，并在每次调用中使用实际原生配置。
    let source = r#"
-- Hold state only in this exact module generation.
-- 仅在此精确模块代次中保留状态。
local count = 0
-- Export only the functions declared by the actual fixture manifest.
-- 仅导出实际夹具清单声明的函数。
local M = {}
-- Read the original host authority and return its owner plus the local counter; no arguments are consumed.
-- 读取原宿主权威并返回其所有者及局部计数；不消费参数。
function M.dispatch()
    count = count + 1
    -- Native access must recheck the exact generation, even though capacity remains shared.
    -- 即使容量持续共享，原生访问仍须复核精确代次。
    local config = vulcan.host.call('vulcan.plugin.config.get', {})
    return vulcan.json.encode({ok=true,result={count=count,owner=config.owner}})
end
-- Required declarations share the same test business function; no finalizer is selected.
-- 必需声明共享同一测试业务函数；不选择关闭器。
M.shutdown = M.dispatch
M.on_precheck = M.dispatch
M.ui_action = M.dispatch
M.shutdown_hooks = M.dispatch
M.on_disable = M.dispatch
return M
"#;
    // Register the current catalog generation without changing its reusable state semantics.
    // 注册当前目录代次，不改变其可复用状态语义。
    let mut first_request = bundle_request(&fixture, source);
    first_request.policy.kind = PoolKind::Dedicated;
    // The first pool pins its own original authority and package descriptor.
    // 第一个池固定自身原始权威及包描述符。
    let first = fixture
        .host
        .register_bundle_pool_in_capacity(&capacity, first_request)
        .expect("first member");
    for expected in [1, 2] {
        // Observe native-backed business results before publishing the new generation.
        // 发布新代次前观测由原生服务支持的业务结果。
        let result = decoded(invoke_bundle(&fixture, &first, "dispatch", json!({})));
        assert_eq!(
            result["result"],
            json!({"count":expected,"owner":"initial"})
        );
    }
    // A real source change makes the catalog publish a new generation while the old VM remains resident.
    // 真实源码变化使目录发布新代次，同时旧 VM 保持常驻。
    let mut second_request = bundle_request(
        &fixture,
        &format!("{source}\n-- New bundle generation.\n-- 新 bundle 代次。\n"),
    );
    second_request.policy.kind = PoolKind::Dedicated;
    // Register a second immutable member in the existing capacity, without acquiring another minimum.
    // 在既有容量中注册第二个不可变成员，不取得另一份最小预留。
    let second = fixture
        .host
        .register_bundle_pool_in_capacity(&capacity, second_request)
        .expect("second member");
    assert_eq!(
        decoded(invoke_bundle(&fixture, &second, "dispatch", json!({})))["result"]["count"],
        1
    );
    // Actual usage accounts for both retained generations under the same exact capacity identity.
    // 实际用量在同一精确容量身份下计入两个保留代次。
    let snapshot = fixture
        .host
        .runtime()
        .capacity(&capacity)
        .expect("capacity status");
    assert_eq!(snapshot.retained_pools, 2);
    assert_eq!(snapshot.resources.resident, 2);
    assert_eq!(snapshot.committed_resident_vms, 2);
    // Sharing capacity must never authorize an old module to use the new catalog generation's services.
    // 共享容量绝不能授权旧模块使用新目录代次的服务。
    let stale = invoke_bundle(&fixture, &first, "dispatch", json!({}));
    assert_eq!(stale.phase, OperationPhase::Failed);
    fixture
        .host
        .runtime()
        .close_capacity(&capacity)
        .expect("close member metadata and admission");
    fixture.close();
    for member in [&first, &second] {
        fixture
            .host
            .runtime()
            .forget_pool(member)
            .expect("forget retired member");
    }
    // Empty capacity still owns its guarantee until explicit removal, independently of module metadata.
    // 空容量仍持有保证直至显式移除，独立于模块元数据。
    let empty = fixture
        .host
        .runtime()
        .capacity(&capacity)
        .expect("empty capacity");
    assert_eq!(empty.committed_resident_vms, 1);
    assert_eq!(empty.resources.resident, 0);
    fixture
        .host
        .runtime()
        .forget_capacity(&capacity)
        .expect("forget capacity");
    assert_eq!(
        fixture
            .host
            .runtime()
            .plugin("hook-plugin")
            .expect("plugin status")
            .committed_resident_vms,
        0
    );
    assert!(
        fixture
            .host
            .authority
            .domains
            .lock()
            .expect("authority")
            .values()
            .all(|domain| domain.strong_count() == 0)
    );
}

/// Explicit membership failures release authority reservations and never register independent replacements.
/// 显式成员关系失败会释放权威预留，绝不注册独立替代池。
#[test]
fn restricted_bundle_capacity_rejects_unknown_foreign_closed_and_conflicting_members() {
    // Every refusal runs through the real native adapter and exact original catalog.
    // 每次拒绝都经过真实原生适配器及精确原目录。
    let fixture = Fixture::new();
    // Derive the second owner from the actual parent-compatible aggregate.
    // 从实际与父级兼容的聚合预算派生第二个所有者。
    let aggregate = fixture
        .host
        .runtime()
        .plugin("hook-plugin")
        .expect("plugin")
        .config;
    fixture
        .host
        .runtime()
        .register_plugin("other-plugin".into(), aggregate)
        .expect("other owner");
    // Foreign capacity intentionally has no guarantee, leaving the original reservation available.
    // 外来容量有意不设保证，使原所有者仍可取得预留。
    let mut foreign_config = capacity_config(&fixture);
    foreign_config.resources.min_resident_vms = 0;
    // Keep the foreign identity exact; do not select it by guessed plugin names.
    // 保留精确外来身份；不通过猜测插件名称选择。
    let foreign = fixture
        .host
        .runtime()
        .register_capacity("other-plugin", foreign_config)
        .expect("foreign capacity");
    // A closed original owner must not be recreated by adapter registration.
    // 适配器注册不得重新创建已关闭原所有者。
    let closed = fixture
        .host
        .runtime()
        .register_capacity("hook-plugin", capacity_config(&fixture))
        .expect("closed capacity");
    fixture
        .host
        .runtime()
        .close_capacity(&closed)
        .expect("close exact capacity");
    // The core's declared errors distinguish identity, ownership and permanent closure.
    // 核心声明错误区分身份、归属及永久关闭。
    let failures = [
        ("missing-capacity", EmbeddedErrorCode::NotFound),
        (foreign.as_str(), EmbeddedErrorCode::InvalidArgument),
        (closed.as_str(), EmbeddedErrorCode::Closed),
    ];
    for (capacity, expected) in failures {
        // Keep valid module semantics so only the requested capacity relationship can fail.
        // 保持有效模块语义，使失败仅来自所请求容量关系。
        let mut member = policy();
        member.kind = PoolKind::Dedicated;
        // This module is never admitted or initialized in any failure case.
        // 此模块在全部失败用例中都不会入场或初始化。
        let rejected = fixture.host.register_pool_in_capacity(capacity, fixture.definition("-- Return a constant without arguments.\n-- 不使用参数，返回常量。\nreturn {call=function() return true end}"), member, VulcanPluginBridgeLeaseScope::Enabled).expect_err("reject exact relationship");
        assert_eq!(rejected.code, expected);
        assert!(
            fixture
                .host
                .authority
                .domains
                .lock()
                .expect("authority after refusal")
                .is_empty()
        );
        assert_eq!(
            fixture
                .host
                .runtime()
                .plugin("hook-plugin")
                .expect("plugin after refusal")
                .retained_pools,
            0
        );
    }
    fixture
        .host
        .runtime()
        .forget_capacity(&closed)
        .expect("forget empty closed capacity");
    // A live original owner still rejects a member declaring a different capacity kind.
    // 存活原所有者仍会拒绝声明不同容量类别的成员。
    let live = fixture
        .host
        .runtime()
        .register_capacity("hook-plugin", capacity_config(&fixture))
        .expect("live capacity");
    // Shared policy must not be silently rewritten to satisfy the dedicated owner.
    // 不得静默改写公共策略以满足专用所有者。
    let conflict = fixture.host.register_pool_in_capacity(&live, fixture.definition("-- Return a constant without arguments.\n-- 不使用参数，返回常量。\nreturn {call=function() return true end}"), policy(), VulcanPluginBridgeLeaseScope::Enabled).expect_err("conflicting kind");
    assert_eq!(conflict.code, EmbeddedErrorCode::InvalidArgument);
    assert!(
        fixture
            .host
            .authority
            .domains
            .lock()
            .expect("authority after conflict")
            .is_empty()
    );
    fixture.close();
}
