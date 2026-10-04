//! Required host contracts gate real activation and preserve explicit cleanup after replacement.
//! 必需宿主契约门控真实激活，并在替换后保留明确清理。

use super::*;
use luaskills::runtime::embedded::EffectState;
use luaskills::runtime::embedded::capabilities::{
    CapabilityDescriptor, CapabilityEffects, CapabilityExecution, CapabilityIdempotency,
    CapabilityOutcome, CapabilityRegistrationRequest, CapabilityScope,
};
use std::sync::Arc;

mod reuse;

/// Verify public queued dispatch carries trusted identity and retained registry handles cannot reopen a closed parent.
/// 验证公开排队分发携带可信身份，且保留的注册表句柄不能重新开启已关闭父级。
#[tokio::test]
async fn host_callback_requirements_dispatch_queued_through_public_registry() {
    // Declare the exact queued transport while retaining the same input/output version contract.
    // 声明精确排队传输方式，同时保留相同输入输出版本契约。
    let mut required = callback_requirement();
    required["execution"] = json!("queued");
    let (root, manager, _) = fixture(json!([required]));
    // All manager clones expose the same core registry, not independent callback namespaces.
    // 全部管理器克隆公开同一个核心注册表，而非独立回调命名空间。
    let registry = manager.formal_capabilities().expect("public registry");
    assert!(Arc::ptr_eq(
        &registry,
        &manager
            .clone()
            .formal_capabilities()
            .expect("clone registry")
    ));
    // Queued registration owns no Rust callback; the host must explicitly dispatch and complete each request.
    // 排队注册不持有 Rust 回调；宿主必须显式分发并完成每个请求。
    let token = registry
        .register(vec![CapabilityRegistrationRequest {
            descriptor: callback_descriptor("1.0.0", CapabilityExecution::Queued, BTreeSet::new()),
            native: None,
        }])
        .expect("queued registration")
        .pop()
        .expect("queued token");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish queued callback");
    // The invocation enters the real formal scheduler and Lua bridge before appearing in this broker.
    // 调用进入真实正式调度器及 Lua 桥后，才会出现在此代理中。
    let invoking = manager.clone();
    let invocation = request(root.path(), "idle_ping");
    let task = tokio::spawn(async move { invoking.invoke(invocation).await });
    let broker = registry.host_requests();
    let queued = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(request) = broker.take(1).expect("take real host request").pop() {
                return request;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queued host dispatch");
    assert_eq!(queued.registration_id, token);
    assert_eq!(queued.name, "test.required");
    assert_eq!(queued.version, "1.0.0");
    assert_eq!(queued.arguments, json!({}));
    assert_eq!(queued.caller.plugin_id, "idle-plugin");
    assert_eq!(
        queued.caller.runtime_id,
        manager
            .formal_runtime_status()
            .expect("formal status")
            .expect("attached parent")
            .runtime_id
    );
    assert!(queued.caller.session_id.is_none());
    assert!(queued.effect_id.is_some());
    broker
        .complete(
            &queued.request_id,
            CapabilityOutcome {
                result: Ok(json!("1.0.0")),
                effects: EffectState::NotApplicable,
            },
        )
        .expect("complete original request");
    assert_eq!(
        tokio::time::timeout(WAIT, task)
            .await
            .expect("actual invocation completes")
            .expect("observer task")
            .expect("queued callback result"),
        json!("1.0.0")
    );
    assert!(
        broker
            .complete(
                &queued.request_id,
                CapabilityOutcome {
                    result: Ok(json!("replacement")),
                    effects: EffectState::NotApplicable,
                },
            )
            .is_err()
    );
    close(&manager).await;
    // A retained management handle still observes the old registration after real parent shutdown.
    // 真实父级关闭后，保留管理句柄仍观测原注册。
    let closed = registry
        .status(&token)
        .expect("retained registration status");
    assert!(!closed.accepting);
    assert!(closed.drained);
    assert_eq!(
        registry
            .register(vec![CapabilityRegistrationRequest {
                descriptor: callback_descriptor(
                    "1.0.0",
                    CapabilityExecution::Queued,
                    BTreeSet::new()
                ),
                native: None,
            }])
            .expect_err("closed registry cannot reopen")
            .code,
        luaskills::runtime::embedded::EmbeddedErrorCode::Closed
    );
}

/// Return an exact native invocation contract; no permission is granted by this declaration.
/// 返回精确原生调用契约；此声明不授予任何权限。
fn callback_requirement() -> Value {
    json!({"name":"test.required","version":"1.0.0","execution":"native","scope":"invocation"})
}

/// Keep the published callback fragment aligned with strict Rust decoding for valid and malformed objects.
/// 对有效及畸形对象，保持已发布回调片段与严格 Rust 解码一致。
#[test]
fn host_callback_requirements_schema_matches_structural_decoding() {
    // This offline fragment describes structure; semantic versions and live registration remain Rust-owned.
    // 此离线片段描述结构；语义版本及实时注册仍由 Rust 拥有。
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../../../docs/contracts/plugin-runtime-requirements.schema.json"
    ))
    .expect("runtime requirements authoring schema");
    let validator = jsonschema::validator_for(&schema).expect("offline requirement schema");
    for callback in [
        callback_requirement(),
        json!({"name":"test.required","version":"1.0.0","execution":"queued","scope":"session"}),
    ] {
        // Both languages of dispatch and both core scopes retain their exact wire spellings.
        // 两种分发方式及两个核心作用域保留其精确线上拼写。
        let mut value = requirements();
        value["host_capabilities"] = json!([callback]);
        assert!(validator.is_valid(&value));
        serde_json::from_value::<crate::VulcanPluginRuntimeRequirements>(value)
            .expect("strict callback shape");
    }
    for callback in [
        Value::Null,
        json!({"name":"test.required","version":"1.0.0","execution":"native"}),
        json!({"name":"test.required","version":"1.0.0","execution":null,"scope":"invocation"}),
        json!({"name":"test.required","version":"1.0.0","execution":"native","scope":"global"}),
        json!({"name":"test.required","version":"1.0.0","execution":"native","scope":"invocation","extra":1}),
    ] {
        // Unknown or incomplete callback objects must fail before activation or native work.
        // 未知或不完整回调对象必须在激活或原生工作前失败。
        let mut value = requirements();
        value["host_capabilities"] = json!([callback]);
        assert!(!validator.is_valid(&value), "{value}");
        assert!(serde_json::from_value::<crate::VulcanPluginRuntimeRequirements>(value).is_err());
    }
}

/// Create a real package requiring callbacks and return its root, manager and untouched manifest path.
/// 创建要求 callbacks 的真实插件包，并返回其根、管理器及未被宿主改写的清单路径。
fn fixture(callbacks: Value) -> (tempfile::TempDir, VulcanPluginRuntimeManager, PathBuf) {
    // Independent filesystem roots keep actual package authority isolated across test cases.
    // 独立文件系统根保持各测试用例的实际插件包权威隔离。
    let root = tempfile::tempdir().expect("callback fixture root");
    let bundled = root.path().join("bundled");
    let mut required = requirements();
    required["host_capabilities"] = callbacks;
    let path = package(&bundled, required);
    // Cleanup exports deliberately do not depend on the business callback.
    // 清理导出刻意不依赖业务回调。
    let mut manifest: Value =
        serde_yaml::from_str(&std::fs::read_to_string(&path).expect("manifest bytes"))
            .expect("manifest mapping");
    manifest["lua_runtime"]["lifecycle"] = json!({"disable":"shutdown","uninstall":"shutdown"});
    std::fs::write(
        &path,
        serde_yaml::to_string(&manifest).expect("manifest encoding"),
    )
    .expect("write manifest");
    // Business invokes the real typed host boundary, while shutdown returns the ordinary contract frame.
    // 业务调用真实类型化宿主边界，关闭返回普通契约帧。
    std::fs::write(
        bundled.join("idle-plugin/runtime/plugin.bundle.lua"),
        r#"
-- Define one exact module returned to the trusted host loader.
-- 定义返回可信宿主加载器的单个精确模块。
local M = {}
-- Call the required native handler and return the trusted bridge's projected value.
-- 调用必需原生处理器并返回可信桥投影后的值。
function M.dispatch(_envelope)
    return vulcan.json.encode({ok=true,result=vulcan.host.call('test.required',{})})
end
-- Cleanup needs no business service; return successful physical shutdown evidence.
-- 清理不需要业务服务；返回成功物理关闭证据。
function M.shutdown(_envelope) return '{"ok":true,"result":{"closed":true}}' end
return M
"#,
    )
    .expect("real callback bundle");
    let manager = test_manager(root.path(), bundled);
    manager
        .initialize_formal_runtime(VulcanPluginRuntimeSettings::default())
        .expect("formal parent");
    (root, manager, path)
}

/// Build the shared fixture descriptor for version, execution transport and required grants.
/// 按 version、execution 传输方式及必需 grants 构造共享夹具描述符。
/// Returns one bounded invocation contract; queued registration deliberately has no native closure.
/// 返回单个有界调用契约；排队注册明确不包含原生闭包。
fn callback_descriptor(
    version: &str,
    execution: CapabilityExecution,
    grants: BTreeSet<String>,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        name: "test.required".into(),
        version: version.into(),
        description: "Return the registered callback version.".into(),
        input_schema: json!({"type":"object"}),
        output_schema: json!({"type":"string"}),
        execution,
        permissions: grants,
        scope: CapabilityScope::Invocation,
        max_concurrent: 1,
        max_call_ms: 1000,
        max_input_bytes: 1024,
        max_output_bytes: 1024,
        effects: CapabilityEffects::ReadOnly,
        idempotency: CapabilityIdempotency::None,
    }
}

/// Register version with exact grants in manager's real parent and return its revocable native token.
/// 在 manager 的真实父级中按精确 grants 注册 version，并返回其可撤销原生令牌。
fn register(
    manager: &VulcanPluginRuntimeManager,
    version: &str,
    grants: BTreeSet<String>,
) -> String {
    // Production embedders use this public entry to reach the exact attached registry.
    // 生产嵌入方通过此公开入口访问精确附加注册表。
    let registry = manager.formal_capabilities().expect("attached registry");
    // The closure owns its version independently from later registration replacement.
    // 闭包独立持有其版本，不受后续注册替换影响。
    let value = version.to_owned();
    registry
        .register(vec![CapabilityRegistrationRequest {
            descriptor: callback_descriptor(version, CapabilityExecution::Native, grants),
            native: Some(Arc::new(move |_| CapabilityOutcome {
                result: Ok(json!(value)),
                effects: EffectState::NotApplicable,
            })),
        }])
        .expect("register native callback")
        .pop()
        .expect("one registration token")
}

/// Build a request to tool using the fixture's exact workspace and stable controller.
/// 使用夹具精确工作区及稳定控制器构造对 tool 的请求。
fn request(root: &Path, tool: &str) -> VulcanPluginInvocationRequest {
    VulcanPluginInvocationRequest {
        tool_id: tool.into(),
        arguments: json!({}),
        owner_session_id: "callback-owner".into(),
        workspace_root: root.to_owned(),
    }
}

/// Reject missing, malformed, duplicated and unsupported callback structures before catalog activation.
/// 在目录激活前拒绝缺失字段、畸形、重复和不支持的回调结构。
#[test]
fn host_callback_requirements_are_structurally_strict() {
    // Each invalid manifest is validated by the actual catalog loader.
    // 每个无效清单由实际目录加载器校验。
    for callbacks in [
        Value::Null,
        json!([null]),
        json!([{"name":"test.required","version":"1.0.0","execution":"native"}]),
        json!([{"name":"test.required","version":"1.0.0","execution":"native","scope":"invocation","unknown":1}]),
        json!([{"name":"","version":"1.0.0","execution":"native","scope":"invocation"}]),
        json!([{"name":"test.required","version":"^1","execution":"native","scope":"invocation"}]),
        json!([{"name":"test.required","version":"1.0.0","execution":"background","scope":"invocation"}]),
        json!([{"name":"test.required","version":"1.0.0","execution":"native","scope":"global"}]),
        json!([callback_requirement(), callback_requirement()]),
    ] {
        let root = tempfile::tempdir().expect("invalid callback fixture");
        let mut required = requirements();
        required["host_capabilities"] = callbacks.clone();
        package(root.path(), required);
        let plugin = discover(root.path());
        assert!(
            plugin.manifest.is_none(),
            "{callbacks}: {:?}",
            plugin.diagnostics
        );
        assert!(!plugin.diagnostics.is_empty());
    }
    assert!(
        serde_yaml::from_str::<crate::VulcanPluginHostCapabilityRequirement>(
            "name: test.required\nname: other\nversion: 1.0.0\nexecution: native\nscope: invocation"
        )
        .is_err(),
        "duplicate original YAML fields must not be normalized away"
    );
}

/// Exact registered version executes; replacement rejects new business before VM allocation but leaves cleanup possible.
/// 精确注册版本可执行；替换在分配 VM 前拒绝新业务，但仍可清理。
#[tokio::test]
async fn host_callback_requirements_execute_and_reject_replacement_without_blocking_cleanup() {
    // Before registration the real catalog must withdraw active surfaces without running a probe.
    // 注册前真实目录必须撤下活动能力，且不运行探测。
    let (root, manager, path) = fixture(json!([callback_requirement()]));
    let original = std::fs::read(&path).expect("original declaration");
    let missing = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("missing callback catalog");
    assert!(missing.enabled_tools.is_empty());
    assert!(
        missing.plugins["idle-plugin"]
            .diagnostics
            .iter()
            .any(|error| error.contains("unavailable or unauthorized"))
    );
    // Explicit registration plus ordinary refresh makes the real Lua-to-host invocation available.
    // 明确注册加普通刷新，使真实 Lua 到宿主调用可用。
    let token = register(&manager, "1.0.0", BTreeSet::new());
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish matching callback");
    let value = manager
        .invoke(request(root.path(), "idle_ping"))
        .await
        .expect("actual matching callback");
    assert_eq!(value, json!("1.0.0"));
    let owner = manager
        .formal_runtime_owner()
        .expect("query owner")
        .expect("owner");
    manager
        .formal_capabilities()
        .expect("public attached registry")
        .unregister(&token)
        .expect("revoke original callback");
    register(&manager, "2.0.0", BTreeSet::new());
    // A stale published catalog cannot bypass registration-time validation.
    // 陈旧已发布目录不能绕过注册时校验。
    let rejected = manager
        .invoke(request(root.path(), "idle_ping"))
        .await
        .expect_err("changed callback version");
    assert!(rejected.message.contains("version 1.0.0"), "{rejected}");
    for call in manager
        .formal_calls("callback-owner")
        .expect("rejected operation evidence")
    {
        assert!(call.retired);
        manager
            .forget_formal_call(&call.invocation_id, "callback-owner")
            .expect("acknowledge original rejection");
    }
    assert_eq!(
        owner
            .host
            .runtime()
            .resources()
            .expect("native resources")
            .resident,
        0
    );
    let snapshot = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish incompatibility");
    assert!(snapshot.enabled_tools.is_empty());
    assert_eq!(
        snapshot.plugins["idle-plugin"].runtime_state,
        VulcanPluginRuntimeState::Enabled
    );
    manager
        .begin_draining("idle-plugin")
        .expect("business mismatch stays manageable");
    manager
        .invoke_lifecycle("idle-plugin", crate::VulcanPluginLifecycleEvent::Disable)
        .await
        .expect("actual disable callback");
    let disabled = [String::from("idle-plugin")].into();
    manager
        .finish_lifecycle_transition("idle-plugin", &disabled, &BTreeSet::new(), Vec::new())
        .expect("disabled publication");
    manager
        .begin_enabling("idle-plugin")
        .expect_err("incompatible callback cannot reactivate");
    manager
        .invoke_lifecycle("idle-plugin", crate::VulcanPluginLifecycleEvent::Uninstall)
        .await
        .expect("actual uninstall callback");
    assert_eq!(std::fs::read(&path).expect("unchanged manifest"), original);
    close(&manager).await;
}

/// Permissions, transport and caller scope are checked independently without silently rewriting either side.
/// 独立检查权限、传输及调用方作用域，不静默改写任何一方。
#[tokio::test]
async fn host_callback_requirements_check_authorization_transport_and_scope() {
    // All cases register a real native invocation callback and vary exactly one required contract boundary.
    // 所有用例注册真实原生调用回调，每次仅改变一个必需契约边界。
    for (field, value, unauthorized) in [
        ("execution", json!("queued"), false),
        ("scope", json!("session"), false),
        ("version", json!("1.0.0"), true),
    ] {
        let mut requirement = callback_requirement();
        requirement[field] = value;
        let (root, manager, _) = fixture(json!([requirement]));
        let grants = if unauthorized {
            [String::from("test.ungranted")].into()
        } else {
            BTreeSet::new()
        };
        register(&manager, "1.0.0", grants);
        let snapshot = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("incompatible declaration");
        assert!(snapshot.enabled_tools.is_empty());
        assert!(
            snapshot.plugins["idle-plugin"]
                .diagnostics
                .iter()
                .any(|error| error.contains(if unauthorized {
                    "unavailable or unauthorized"
                } else {
                    "authorized snapshot provides"
                })),
            "{:?}",
            snapshot.plugins["idle-plugin"].diagnostics
        );
        manager
            .invoke(request(root.path(), "idle_ping"))
            .await
            .expect_err("contract mismatch before execution");
        assert_eq!(
            manager
                .formal_runtime_status()
                .expect("status")
                .expect("parent")
                .resources
                .resident,
            0
        );
        close(&manager).await;
    }
}
