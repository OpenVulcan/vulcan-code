//! Real manifest and bundle execution through the shared restricted loader.
//! 经共享受限加载器执行真实清单及 bundle。

use super::*;
use luaskills::runtime::embedded::OperationSnapshot;
use std::path::Path;

/// The formal loader cannot bypass the catalog requirement for a Lua dependency declaration.
/// 正式加载器不能绕过目录对 Lua 依赖声明的要求。
#[test]
fn restricted_bundle_rejects_missing_required_dependency_declaration() {
    // Start from a real enabled package, then remove only its required declaration.
    // 从真实已启用包开始，仅移除其必需声明。
    let fixture = Fixture::new();
    // The package path comes from the actual catalog, not a guessed fixture layout.
    // 包路径来自真实目录，不猜测夹具布局。
    let package = fixture.manager.snapshot().plugins["hook-plugin"]
        .package_root
        .clone();
    // Keep the physical dependency file to prove its presence cannot substitute for a declaration.
    // 保留物理依赖文件，证明其存在不能替代声明。
    let original = fs::read_to_string(package.join("plugin.yaml")).expect("read fixture manifest");
    // Remove the exact known field while leaving bundled install mode and Lua exports intact.
    // 删除精确已知字段，保持内置安装模式及 Lua 导出不变。
    let changed = original.replace("    manifest: dependencies.yaml\n", "");
    assert_ne!(
        changed, original,
        "fixture dependency declaration may have changed"
    );
    fs::write(package.join("plugin.yaml"), changed).expect("remove manifest declaration");
    // Production catalog validation is the authority for whether this package may execute.
    // 生产目录校验是此包是否可执行的权威。
    let catalog = fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("refresh invalid package");
    // Find the semantic diagnostic without binding the test to a diagnostic array position.
    // 按语义查找诊断，不将测试绑定到诊断数组位置。
    let plugin = &catalog.plugins["hook-plugin"];
    assert_ne!(plugin.runtime_state, VulcanPluginRuntimeState::Enabled);
    assert!(
        plugin.diagnostics.iter().any(|message| message
            .contains("Lua, Node, and Python components require an execution dependency manifest")),
        "missing dependency diagnostic: {:?}",
        plugin.diagnostics
    );
    // A current-generation request must still fail before acquiring a formal execution domain.
    // 当前代次请求仍必须在取得正式执行域前失败。
    let rejected = fixture
        .host
        .register_bundle_pool(VulcanPluginBundlePoolRequest {
            plugin_id: plugin.id.clone(),
            plugin_generation: plugin.generation,
            workspace_root: fixture.root.path().to_path_buf(),
            security_partition: "invalid-dependency-fixture".to_string(),
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            policy: policy(),
        })
        .expect_err("reject unavailable Lua package");
    assert_eq!(rejected.code, EmbeddedErrorCode::PermissionDenied);
    assert!(
        fixture
            .host
            .authority
            .domains
            .lock()
            .expect("inspect execution authority")
            .is_empty()
    );
    assert_eq!(
        fixture
            .host
            .runtime()
            .resources()
            .expect("inspect actual VM ownership")
            .resident,
        0
    );
    assert!(package.join("dependencies.yaml").is_file());
    fixture.close();
}

/// Checked-in packages initialize through the formal loader, and Context7 reads its real defaults.
/// 仓库内包经正式加载器初始化，Context7 读取其真实默认配置。
#[test]
fn restricted_bundle_loads_production_packages_and_native_config() {
    // Resolve authoritative packages without copying or rewriting their manifest or bundle bytes.
    // 解析权威包，不复制或重写其清单及 bundle 字节。
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("resolve repository for production packages");
    for plugin_id in ["context7", "vulcan-usage-statistics"] {
        // Only mutable host state belongs to the temporary directory.
        // 仅可变宿主状态属于临时目录。
        let fixture = Fixture::from_catalog(
            tempfile::tempdir().expect("create production bundle host state"),
            repository.join("plugins"),
            plugin_id,
        );
        // The request is resolved from the actual production catalog, including every export.
        // 请求从真实生产目录解析，包含每个导出。
        let pool = fixture
            .host
            .register_bundle_pool(VulcanPluginBundlePoolRequest {
                plugin_id: plugin_id.to_string(),
                plugin_generation: fixture.manager.snapshot().plugins[plugin_id].generation,
                workspace_root: fixture.root.path().to_path_buf(),
                security_partition: "production-bundle-fixture".to_string(),
                scope: VulcanPluginBridgeLeaseScope::Enabled,
                policy: policy(),
            })
            .expect("register production bundle");
        if plugin_id == "context7" {
            // This real UI export reaches the native configuration capability without loading unused native modules.
            // 此真实界面导出到达原生配置能力，不加载未使用的原生模块。
            let settings = decoded(invoke_bundle(
                &fixture,
                &pool,
                "settings_get",
                json!({"arguments":{}}),
            ));
            // The authoritative package defaults provide the expected value without a copied configuration snapshot.
            // 权威包默认配置提供期望值，不维护复制的配置快照。
            let defaults: Value = serde_json::from_str(
                &fs::read_to_string(repository.join("plugins/context7/config/defaults.json"))
                    .expect("read actual Context7 defaults"),
            )
            .expect("decode actual Context7 defaults");
            assert_eq!(settings["result"], defaults);
        }
        assert_eq!(
            decoded(invoke_bundle(&fixture, &pool, "shutdown", json!({})))["result"]["closed"],
            true
        );
        fixture.close();
    }
}

/// Write `source` to the fixture's real bundle, publish its changed fingerprint, and return its pool.
/// 将 `source` 写入夹具真实 bundle，发布已变化指纹，并返回其池。
fn bundle_pool(fixture: &Fixture, source: &str) -> String {
    // Resolve the actual catalog path before changing its source content.
    // 在改变源码内容前解析实际目录路径。
    let package = fixture.manager.snapshot().plugins["hook-plugin"]
        .package_root
        .clone();
    fs::write(package.join("runtime/plugin.bundle.lua"), source).expect("write real bundle");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish bundle generation");
    // The preparation API receives no caller-provided source or export definitions.
    // 准备 API 不接收调用方提供的源码或导出定义。
    let request = VulcanPluginBundlePoolRequest {
        plugin_id: "hook-plugin".to_string(),
        plugin_generation: fixture.manager.snapshot().plugins["hook-plugin"].generation,
        workspace_root: fixture.root.path().to_path_buf(),
        security_partition: "fixture-bundle".to_string(),
        scope: VulcanPluginBridgeLeaseScope::Enabled,
        policy: policy(),
    };
    fixture
        .host
        .register_bundle_pool(request)
        .expect("register actual manifest bundle")
}

/// Execute exact `export` with `arguments` in the real `pool` and return terminal evidence.
/// 在真实 `pool` 中使用 `arguments` 执行精确 `export`，返回终态证据。
fn invoke_bundle(
    fixture: &Fixture,
    pool: &str,
    export: &str,
    arguments: Value,
) -> OperationSnapshot {
    // One stable core operation owns the complete initialization and function call.
    // 一个稳定核心操作拥有完整初始化及函数调用。
    let operation = fixture
        .host
        .runtime()
        .submit(
            EmbeddedCall {
                pool_id: pool.to_string(),
                export: export.to_string(),
                arguments,
                context: LuaInvocationContext::default(),
            },
            WAIT,
        )
        .expect("submit bundle operation");
    // Verify actual terminal state before forgetting the bounded result record.
    // 遗忘有界结果记录前验证真实终态。
    let snapshot = operation.wait(WAIT).expect("wait bundle operation");
    assert!(
        snapshot.phase.is_terminal(),
        "bundle operation did not finish: {snapshot:?}"
    );
    fixture
        .host
        .runtime()
        .forget_operation(operation.id())
        .expect("forget bundle operation");
    snapshot
}

/// Parse the successful version-one JSON frame from `snapshot`; report complete failure evidence.
/// 从 `snapshot` 解析成功的版本一 JSON 帧；报告完整失败证据。
fn decoded(snapshot: OperationSnapshot) -> Value {
    assert_eq!(snapshot.phase, OperationPhase::Succeeded, "{snapshot:?}");
    serde_json::from_str(
        snapshot
            .value
            .as_ref()
            .and_then(Value::as_str)
            .expect("version-one JSON frame"),
    )
    .expect("decode version-one result")
}

/// Native initialization uses the real catalog identity while only captured manifest functions are callable.
/// 原生初始化使用真实目录身份，同时仅可调用已捕获清单函数。
#[test]
fn restricted_bundle_preserves_authority_state_and_captured_exports() {
    // All initialization callbacks and function invocations use a real manager and Lua VM.
    // 全部初始化回调及函数调用使用真实管理器与 Lua VM。
    let fixture = Fixture::new();
    // Delimiter-like source text and Unicode remain data in the host-generated wrapper.
    // 类似分隔符的源码文本与 Unicode 在宿主生成包装中保持数据身份。
    let source = r#"
-- Keep one restricted module state across calls.
-- 跨调用保留一个受限模块状态。
local M = {}
local count = 0
local settings = vulcan.host.call('vulcan.plugin.config.get', {})
local forbidden = io == nil and os == nil and debug == nil and package == nil and require == nil and load == nil
local undeclared = pcall(vulcan.runtime.module, 'not.declared')
assert(forbidden and not undeclared)
-- Mutating the returned table must not redirect the core's already captured export.
-- 修改返回表不能重定向核心已经捕获的导出。
function M.dispatch(args)
    count = count + 1
    M.dispatch = function() error('unexpected redirected export') end
    return vulcan.json.encode({ok=true,result={count=count,owner=settings.owner,text='中文 ]] ]=] " \\ \0007',input=args}})
end
-- Preserve every function explicitly declared by the fixture manifest.
-- 保留夹具清单显式声明的每个函数。
function M.shutdown(args) return vulcan.json.encode({ok=true,result={closed=true,count=count}}) end
function M.on_precheck(args) return M.shutdown(args) end
function M.ui_action(args) return M.shutdown(args) end
function M.shutdown_hooks(args) return M.shutdown(args) end
function M.on_disable(args) return M.shutdown(args) end
function M.private_helper() error('private export must be inaccessible') end
return M
"#;
    // Freeze source through the real manifest preparation API rather than a synthetic module definition.
    // 通过真实清单准备 API 冻结源码，不使用合成模块定义。
    let pool = bundle_pool(&fixture, source);
    for expected_count in [1, 2] {
        // Plugin-supplied identity fields cannot replace the native initialization authority.
        // 插件提供的身份字段不能替换原生初始化权威。
        let result = decoded(invoke_bundle(
            &fixture,
            &pool,
            "dispatch",
            json!({"plugin_id":"victim","workspace_root":"victim-workspace","object":{},"array":[]}),
        ));
        assert_eq!(result["result"]["count"], expected_count);
        assert_eq!(result["result"]["owner"], "initial");
        assert_eq!(result["result"]["text"], "中文 ]] ]=] \" \\ \u{0}7");
        assert_eq!(result["result"]["input"]["object"], json!({}));
        assert_eq!(result["result"]["input"]["array"], json!([]));
    }
    // A private helper is absent from the core's declared export set.
    // 私有辅助函数不在核心声明导出集合中。
    let rejected = fixture
        .host
        .runtime()
        .submit(
            EmbeddedCall {
                pool_id: pool.clone(),
                export: "private_helper".to_string(),
                arguments: json!({}),
                context: LuaInvocationContext::default(),
            },
            WAIT,
        )
        .err()
        .expect("reject undeclared export before operation admission");
    assert_eq!(rejected.code, EmbeddedErrorCode::InvalidArgument);
    assert_eq!(rejected.message, "module export is not declared");
    fixture.close();
}

/// Actual bundle initialization cannot escape its restricted globals or omit a declared shutdown function.
/// 实际 bundle 初始化不能逃逸受限全局环境，也不能省略已声明关闭函数。
#[test]
fn restricted_bundle_rejects_missing_exports_and_invalid_source() {
    for source in ["return {}", "return require('os')", "this is not valid lua"] {
        // Each rejected package has an independent core owner so cleanup is observed in isolation.
        // 每个被拒包具有独立核心所有者，使清理得到隔离观测。
        let fixture = Fixture::new();
        // Registration freezes source but does not run initialization ahead of core admission.
        // 注册冻结源码，但不在核心入场前运行初始化。
        let pool = bundle_pool(&fixture, source);
        // Actual execution must fail, rather than falling back to the legacy package loader.
        // 实际执行必须失败，不能回退到旧包加载器。
        let result = invoke_bundle(&fixture, &pool, "dispatch", json!({}));
        assert_eq!(
            result.error.as_ref().expect("rejected initialization").code,
            EmbeddedErrorCode::ExecutionFailed
        );
        assert!(
            fixture
                .services
                .storage
                .lock()
                .expect("no storage work")
                .is_empty()
        );
        fixture.close();
    }
}

/// A selected shutdown export runs on the same module state as ordinary plugin calls.
/// 所选关闭导出在与普通插件调用相同的模块状态上运行。
#[test]
fn restricted_bundle_exposes_real_shutdown_on_the_same_vm() {
    // The original fixture already declares distinct tool, hook and shutdown exports.
    // 原夹具已声明不同工具、Hook 及关闭导出。
    let fixture = Fixture::new();
    // Read its actual source and add an observable host event to the declared hook shutdown function.
    // 读取其实际源码，在已声明 Hook 关闭函数中加入可观测宿主事件。
    let package = fixture.manager.snapshot().plugins["hook-plugin"]
        .package_root
        .clone();
    // Preserve the existing counter and export declarations while changing only shutdown behavior.
    // 仅改变关闭行为，保留现有计数器及导出声明。
    let original =
        fs::read_to_string(package.join("runtime/plugin.bundle.lua")).expect("read fixture bundle");
    // The source replacement targets the one known fixture shutdown declaration.
    // 源码替换定位唯一已知夹具关闭声明。
    let source = original.replace("function M.shutdown_hooks(_args)", "function M.shutdown_hooks(_args)\n    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='shutdown',revision=count})");
    assert_ne!(
        source, original,
        "fixture shutdown declaration may have changed"
    );
    // Registration derives all exports directly from this changed package manifest.
    // 注册直接从此已变化包清单派生全部导出。
    let pool = bundle_pool(&fixture, &source);
    assert_eq!(
        decoded(invoke_bundle(&fixture, &pool, "on_precheck", json!({})))["result"]["payload"]["value"]
            ["count"],
        1
    );
    assert_eq!(
        decoded(invoke_bundle(&fixture, &pool, "shutdown_hooks", json!({})))["ok"],
        true
    );
    assert_eq!(
        fixture
            .services
            .events
            .lock()
            .expect("shutdown observation")
            .as_slice(),
        &[VulcanPluginUiInvalidation {
            plugin_id: "hook-plugin".to_string(),
            topic: "shutdown".to_string(),
            revision: 1
        }]
    );
    fixture.close();
}
