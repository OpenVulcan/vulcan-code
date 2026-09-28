//! Actual bundle shutdown semantics across the formal core lifecycle.
//! 正式核心生命周期中的真实 bundle 关闭语义。

use super::*;
use luaskills::runtime::embedded::ModuleFinalizer;

/// Build one fixture bundle whose chosen shutdown reports its VM-local call count.
/// 构造选定关闭函数报告 VM 内调用计数的夹具 bundle。
/// `shutdown` is the exact test-selected manifest export; `closing_body` receives its host envelope.
/// `shutdown` 是测试选择的精确清单导出；`closing_body` 接收其宿主信封。
fn finalizing_source(shutdown: &str, closing_body: &str) -> String {
    format!(
        r#"local M = {{}}
-- Count business calls on this physical VM only.
-- 仅统计此物理 VM 上的业务调用。
local count = 0
-- Return a business frame with the VM-local counter.
-- 返回包含 VM 内计数器的业务帧。
-- args selects the deliberate business error; returns a version-one frame.
-- args 选择主动业务错误；返回版本一帧。
function M.dispatch(args)
    count = count + 1
    if args.fail then error('original-business-failure') end
    return vulcan.json.encode({{ok=true,result={{count=count}}}})
end
-- Reject an accidentally selected shutdown route.
-- 拒绝被意外选择的关闭路由。
-- No arguments are consumed; no successful result is returned.
-- 不消费参数；不返回成功结果。
function M.shutdown() error('wrong-tool-shutdown') end
-- Reject an accidentally selected Hook shutdown route.
-- 拒绝被意外选择的 Hook 关闭路由。
-- No arguments are consumed; no successful result is returned.
-- 不消费参数；不返回成功结果。
function M.shutdown_hooks() error('wrong-hook-shutdown') end
-- These required declarations share the same fixture business function.
-- 这些必需声明共享同一夹具业务函数。
M.on_precheck = M.dispatch
M.ui_action = M.dispatch
M.on_disable = M.dispatch
-- Validate the exact frozen host envelope and observe one actual closing side effect.
-- 校验精确冻结宿主信封并观测一次真实关闭副作用。
-- args contains the host payload; returns its declared frame or deliberate test failure.
-- args 包含宿主载荷；返回声明帧或主动测试失败。
M.{shutdown} = function(args)
    local envelope = vulcan.json.decode(args.payload_json)
    assert(args.host_timeout_ms == envelope.expected_timeout_ms, 'closing budget changed')
    assert(envelope.marker == '原始关闭参数', 'closing arguments changed')
    vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic='{shutdown}',revision=count}})
    {closing_body}
end
return M
"#
    )
}

/// Construct the host's explicit envelope and finite budget without consulting plugin business arguments.
/// 构造宿主显式信封及有限预算，不读取插件业务参数。
/// `export` selects the declared shutdown entry; `timeout_ms` supplies the sole timeout value.
/// `export` 选择已声明关闭入口；`timeout_ms` 提供唯一超时值。
fn finalizer(export: &str, timeout_ms: u64) -> ModuleFinalizer {
    ModuleFinalizer {
        export: export.to_string(),
        arguments: json!({
            "payload_json": json!({
                "marker": "原始关闭参数",
                "expected_timeout_ms": timeout_ms,
            }).to_string(),
            "host_timeout_ms": timeout_ms,
        }),
        timeout_ms,
    }
}

/// Host registration must reject business exports and invalid closing budgets before any Lua executes.
/// 宿主注册必须在任何 Lua 执行前拒绝业务导出及无效关闭预算。
#[test]
fn restricted_bundle_finalization_rejects_undeclared_shutdown_and_invalid_budget() {
    let fixture = Fixture::new();
    let source = finalizing_source("shutdown", "return '{\"ok\":true}'");
    let request = bundle_request(&fixture, &source);
    for (export, timeout_ms) in [
        ("dispatch", WAIT.as_millis() as u64),
        ("private_shutdown", WAIT.as_millis() as u64),
        ("shutdown", 0),
    ] {
        let result = fixture
            .host
            .register_bundle_pool(VulcanPluginBundlePoolRequest {
                plugin_id: request.plugin_id.clone(),
                plugin_generation: request.plugin_generation,
                workspace_root: request.workspace_root.clone(),
                security_partition: request.security_partition.clone(),
                scope: VulcanPluginBridgeLeaseScope::Enabled,
                policy: policy(),
                finalizer: Some(finalizer(export, timeout_ms)),
            });
        assert_eq!(
            result.expect_err("invalid shutdown must reject").code,
            EmbeddedErrorCode::InvalidArgument
        );
    }
    assert!(
        fixture
            .services
            .events
            .lock()
            .expect("event observation")
            .is_empty()
    );
    fixture.close();
}

/// Tool and Hook shutdown run automatically once on their original VM in every reuse mode.
/// 工具及 Hook 关闭在每种复用模式下自动于原 VM 执行一次。
#[test]
fn restricted_bundle_finalization_runs_automatically_on_original_vm() {
    for reuse in [
        InstanceReuse::SingleCall,
        InstanceReuse::Reusable,
        InstanceReuse::Session,
    ] {
        for shutdown in ["shutdown", "shutdown_hooks"] {
            let fixture = Fixture::new();
            let source = finalizing_source(
                shutdown,
                "return vulcan.json.encode({ok=true,result={closed_count=count}})",
            );
            let mut request = bundle_request(&fixture, &source);
            request.policy.reuse = reuse;
            request.finalizer = Some(finalizer(shutdown, WAIT.as_millis() as u64));
            let pool = fixture
                .host
                .register_bundle_pool(request)
                .expect("register finalizing bundle");
            // A session opening initializes the exact VM before its subsequent business call.
            // 会话打开在后续业务调用前初始化精确 VM。
            let business = if reuse == InstanceReuse::Session {
                let opening = fixture
                    .host
                    .runtime()
                    .open_session(&pool, WAIT)
                    .expect("open bundle session");
                assert_eq!(
                    opening
                        .operation
                        .wait(WAIT)
                        .expect("wait initialization")
                        .phase,
                    OperationPhase::Succeeded
                );
                fixture
                    .host
                    .runtime()
                    .forget_operation(opening.operation.id())
                    .expect("forget initialization");
                let operation = fixture
                    .host
                    .runtime()
                    .submit_session(
                        &opening.session_id,
                        "dispatch".to_string(),
                        json!({}),
                        LuaInvocationContext::default(),
                        WAIT,
                    )
                    .expect("submit session business");
                let snapshot = operation.wait(WAIT).expect("wait session business");
                fixture
                    .host
                    .runtime()
                    .forget_operation(operation.id())
                    .expect("forget session business");
                snapshot
            } else {
                invoke_bundle(&fixture, &pool, "dispatch", json!({}))
            };
            assert_eq!(business.phase, OperationPhase::Succeeded, "{business:?}");
            assert_eq!(decoded(business.clone())["result"]["count"], 1);
            // Runtime close must preserve native callbacks until automatic shutdown and actual retirement finish.
            // 运行时关闭必须保留原生回调，直到自动关闭及真实退役完成。
            fixture.close();
            let closing = if reuse == InstanceReuse::SingleCall {
                business
                    .finalization
                    .expect("single-call shutdown evidence")
            } else {
                let page = fixture
                    .host
                    .runtime()
                    .list_operations(Some(&pool), None, 1)
                    .expect("discover closing");
                assert_eq!(page.operation_ids.len(), 1);
                assert!(!page.has_more, "only independent shutdown should remain");
                fixture
                    .host
                    .runtime()
                    .operation(&page.operation_ids[0])
                    .expect("closing identity")
                    .snapshot()
                    .expect("closing snapshot")
                    .finalization
                    .expect("independent shutdown evidence")
            };
            let value = closing
                .outcome
                .expect("closing outcome")
                .result()
                .expect("successful shutdown");
            let frame: Value = serde_json::from_str(value.as_str().expect("version-one frame"))
                .expect("decode frame");
            assert_eq!(frame["result"]["closed_count"], 1);
            assert_eq!(closing.export, shutdown);
            assert_eq!(
                fixture
                    .services
                    .events
                    .lock()
                    .expect("closing event")
                    .as_slice(),
                &[VulcanPluginUiInvalidation {
                    plugin_id: "hook-plugin".to_string(),
                    topic: shutdown.to_string(),
                    revision: 1
                }]
            );
        }
    }
}

/// Plugin error frames, invalid output, Lua errors and deadline failures must never become successful shutdown.
/// 插件错误帧、无效输出、Lua 错误及超时绝不能变成成功关闭。
#[test]
fn restricted_bundle_finalization_preserves_business_and_reports_closing_failures() {
    for closing_body in [
        "return '{\"ok\":false,\"error\":\"plugin-close-error\"}'",
        "return '{\"ok\":false,\"error\":{\"code\":\"close-error\"}}'",
        "return '{\"result\":true}'",
        "return 'invalid-json'",
        "return 17",
        "error('closing-lua-error')",
        "while true do end",
    ] {
        let fixture = Fixture::new();
        let source = finalizing_source("shutdown", closing_body);
        let mut request = bundle_request(&fixture, &source);
        request.policy.reuse = InstanceReuse::SingleCall;
        request.finalizer = Some(finalizer("shutdown", (WAIT / 100).as_millis() as u64));
        let pool = fixture
            .host
            .register_bundle_pool(request)
            .expect("register failing finalizer");
        let business = invoke_bundle(&fixture, &pool, "dispatch", json!({}));
        let closing = business
            .finalization
            .as_ref()
            .expect("retained closing stage");
        assert!(closing.business.result().is_ok(), "{business:?}");
        assert!(
            closing
                .outcome
                .as_ref()
                .expect("closing outcome")
                .result()
                .is_err(),
            "{closing_body}: {business:?}"
        );
        assert_eq!(business.phase, OperationPhase::Failed, "{business:?}");
        // The overall operation fails, while the independent business evidence retains its original value.
        // 整体操作失败，而独立业务证据保留其原始值。
        let original_value = closing
            .business
            .result()
            .expect("original successful business");
        let original_frame: Value =
            serde_json::from_str(original_value.as_str().expect("business frame"))
                .expect("decode original business frame");
        assert_eq!(original_frame["result"]["count"], 1);
        fixture.close();
        assert_eq!(
            fixture
                .services
                .events
                .lock()
                .expect("one closing effect")
                .len(),
            1
        );
    }
    // A failed business call still closes its original initialized VM and retains its original error.
    // 失败业务调用仍关闭其原始已初始化 VM，并保留原始错误。
    let fixture = Fixture::new();
    let source = finalizing_source(
        "shutdown",
        "return '{\"ok\":true,\"result\":{\"closed\":true}}'",
    );
    let mut request = bundle_request(&fixture, &source);
    request.policy.reuse = InstanceReuse::SingleCall;
    request.finalizer = Some(finalizer("shutdown", WAIT.as_millis() as u64));
    let pool = fixture
        .host
        .register_bundle_pool(request)
        .expect("register shutdown after failure");
    let business = invoke_bundle(&fixture, &pool, "dispatch", json!({"fail":true}));
    assert_eq!(business.phase, OperationPhase::Failed);
    assert!(
        business
            .error
            .as_ref()
            .expect("business error")
            .message
            .contains("original-business-failure")
    );
    let closing = business
        .finalization
        .expect("closing after failed business");
    assert!(closing.business.result().is_err());
    assert!(closing.outcome.expect("closing outcome").result().is_ok());
    fixture.close();
    assert_eq!(
        fixture
            .services
            .events
            .lock()
            .expect("closing after failure")
            .len(),
        1
    );
}
