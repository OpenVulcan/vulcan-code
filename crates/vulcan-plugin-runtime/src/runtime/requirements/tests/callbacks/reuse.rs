//! Exercise callback replacement across real warmed reusable modules.
//! 跨真实已热身可复用模块验证回调替换。

use super::*;

/// Replace the exact callback registration without a catalog refresh and verify new module identity and state.
/// 无目录刷新地替换精确回调注册，并验证新模块身份及状态。
#[tokio::test]
async fn host_callback_requirements_replacement_retires_warm_binding() {
    // Configure a real reusable bundle before any business invocation or native pool exists.
    // 在任何业务调用或原生池存在前，配置真实可复用 bundle。
    let (root, manager, path) = fixture(json!([callback_requirement()]));
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest bytes"))
            .expect("manifest mapping");
    // Locate by stable tool identity, since manifest ordering is not part of the execution contract.
    // 按稳定工具身份定位，因为清单排序不属于执行契约。
    let tool = manifest["lua_runtime"]["tools"]
        .as_array_mut()
        .expect("declared tools")
        .iter_mut()
        .find(|tool| tool["id"] == "idle_ping")
        .expect("ping declaration");
    tool["execution"]["reusable"] = json!({
        "before_call":"before_call","after_call":"after_call","reuse_check":"reuse_check",
        "ordering":"key_serial","max_resident_vms":1,"max_running_calls":1,
        "idle_ttl_ms":60_000,"max_uses":10
    });
    std::fs::write(
        &path,
        serde_yaml::to_string(&manifest).expect("manifest encoding"),
    )
    .expect("write reusable declaration");
    std::fs::write(
        root.path().join("bundled/idle-plugin/runtime/plugin.bundle.lua"),
        r#"
-- State counts actual calls in one original module and cannot survive a replacement VM.
-- 状态统计单个原始模块的真实调用次数，不能跨替代 VM 存活。
local M, count = {}, 0
-- Prepare the stateless envelope; no request-local state is retained, so return true.
-- 准备无状态信封；不保留请求局部状态，因此返回真。
function M.before_call(_envelope) return true end
-- Return both the exact host callback value and the current module-local invocation count.
-- 同时返回精确宿主回调值及当前模块局部调用次数。
function M.dispatch(_envelope)
    count = count + 1
    return vulcan.json.encode({ok=true,result={value=vulcan.host.call('test.required',{}),count=count}})
end
-- No request-local state needs disposal; return the successful cleanup decision.
-- 无请求局部状态需要处置；返回成功清理决定。
function M.after_call(_envelope, _succeeded) return true end
-- Declare the known-clean instance reusable; the private module counter intentionally persists.
-- 声明已知清洁实例可复用；私有模块计数器刻意保留。
function M.reuse_check() return true end
-- Finalization has no dependency on the business callback that may already be revoked.
-- 关闭不依赖可能已被撤销的业务回调。
function M.shutdown(_envelope) return '{"ok":true,"result":{}}' end
return M
"#,
    )
    .expect("real warmed callback bundle");
    // The first callback remains in exactly one warmed module across successive requests.
    // 首个回调在连续请求间保持于精确的单个已热身模块中。
    let first = register(&manager, "1.0.0", BTreeSet::new());
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish reusable callback");
    for count in 1..=2 {
        assert_eq!(
            manager
                .invoke(request(root.path(), "idle_ping"))
                .await
                .expect("warm callback"),
            json!({"value":"1.0.0","count":count})
        );
    }
    // Retain the old public identities before maintenance may retire and remove them.
    // 在维护可能退役并移除旧身份前，保留其公开身份。
    let old_domains = manager
        .reusable_tool_domains("callback-owner")
        .expect("old domains");
    assert_eq!(old_domains.len(), 1);
    let old = old_domains.first().expect("one old domain");
    assert!(!old.closing);
    assert_eq!(old.borrowers, 0);
    let registry = manager
        .formal_capabilities()
        .expect("public callback registry");
    registry.unregister(&first).expect("revoke first callback");
    registry
        .register(vec![CapabilityRegistrationRequest {
            descriptor: callback_descriptor("1.0.0", CapabilityExecution::Native, BTreeSet::new()),
            native: Some(Arc::new(|_| CapabilityOutcome {
                result: Ok(json!("replacement")),
                effects: EffectState::NotApplicable,
            })),
        }])
        .expect("compatible replacement under the same public name");
    assert_eq!(
        manager
            .invoke(request(root.path(), "idle_ping"))
            .await
            .expect("replacement callback"),
        json!({"value":"replacement","count":1})
    );
    // Actual core retirement must remove the original domain; no manual close hides a stale snapshot.
    // 实际核心退役必须移除原域；不通过手工关闭隐藏陈旧快照。
    let current = tokio::time::timeout(WAIT, async {
        loop {
            let domains = manager
                .reusable_tool_domains("callback-owner")
                .expect("current domains");
            if !domains
                .iter()
                .any(|domain| domain.domain_id == old.domain_id)
            {
                return domains;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("old binding physically retires");
    assert_eq!(current.len(), 1);
    let replacement = current.first().expect("one replacement domain");
    assert_ne!(replacement.domain_id, old.domain_id);
    assert_ne!(replacement.pool_id, old.pool_id);
    assert_eq!(replacement.plugin_generation, old.plugin_generation);
    assert!(!replacement.closing);
    assert_eq!(replacement.borrowers, 0);
    assert!(registry.status(&first).expect("old registration").drained);
    close(&manager).await;
}
