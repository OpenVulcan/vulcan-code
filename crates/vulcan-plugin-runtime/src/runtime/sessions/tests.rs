//! Regression tests for bounded idle retirement and stale-generation close.
//! 有界空闲退役与陈旧代次关闭的回归测试。

use super::super::tests::{test_manager, write_idle_session_package};
use super::VulcanPluginInvocationRequest;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::time::Duration;

/// Verifies the host reaps an idle owner-bound session without another tool invocation.
/// 验证宿主无需额外工具调用即可回收空闲的所有者绑定会话。
#[test]
fn idle_session_timeout_retires_formal_session() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_idle_session_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach formal stateless runtime");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load idle plugin catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Tokio test runtime");
    let stateless = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_ping"),
            arguments: json!({}),
            owner_session_id: String::from("owner-1"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("invoke stateless tool");
    assert_eq!(stateless["accepted"], true);
    assert_eq!(
        manager
            .active_session_count("idle-plugin")
            .expect("actual live reservation count"),
        0
    );
    runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_open"),
            arguments: json!({}),
            owner_session_id: String::from("owner-1"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("open idle session");
    assert_eq!(
        manager
            .active_session_count("idle-plugin")
            .expect("actual live reservation count"),
        1
    );
    runtime.block_on(async {
        // The reaper starts at the one-second deadline, while asynchronous shutdown may finish later under full-workspace load.
        // 回收器在一秒截止点启动，而异步关闭在全仓负载下可能稍后才完成。
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if manager
                    .active_session_count("idle-plugin")
                    .expect("actual live reservation count")
                    == 0
                    && manager
                        .formal_sessions("owner-1")
                        .expect("retained session registry")
                        .is_empty()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("idle session should be reaped within the bounded shutdown window");
    });
    assert_eq!(
        manager
            .active_session_count("idle-plugin")
            .expect("actual live reservation count"),
        0
    );
}

/// Verifies a current close tool retires an owned session from an older package generation.
/// 验证当前关闭工具可以回收属于旧插件代次的所有者会话。
#[test]
fn close_tool_retires_stale_generation_session() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    let vulcan_plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
    write_idle_session_package(&vulcan_plugin_root);
    let manager = test_manager(&runtime_root, vulcan_plugin_root.clone());
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach formal session parent");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load idle plugin catalog");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Tokio test runtime");
    let opened = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_open"),
            arguments: json!({}),
            owner_session_id: String::from("owner-stale"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("open stale fixture session");
    let session_id = opened["session_id"]
        .as_str()
        .expect("read opened session id")
        .to_owned();
    fs::write(
        vulcan_plugin_root.join("idle-plugin/dependencies.yaml"),
        "# generation change\n",
    )
    .expect("change package fingerprint");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("refresh changed package");

    let closed = runtime
        .block_on(manager.invoke(VulcanPluginInvocationRequest {
            tool_id: String::from("idle_close"),
            arguments: json!({ "session_id": session_id }),
            owner_session_id: String::from("owner-stale"),
            workspace_root: temp.path().to_path_buf(),
        }))
        .expect("close stale fixture session");
    assert_eq!(closed["closed"], true);
    assert_eq!(closed["result"]["closed_stale_generation"], true);
    assert_eq!(
        manager
            .active_session_count("idle-plugin")
            .expect("actual live reservation count"),
        0
    );
}

/// Verifies exact generation binding for all real routes and preserves ordinary current-ID invocation.
/// 验证全部真实路径的精确代次绑定，并保留普通当前标识调用。
/// No parameters; real session counters prove stale calls and closes produced no session effects.
/// 无参数；真实会话计数证明陈旧调用与关闭没有产生会话效果。
#[test]
fn generation_bound_invocation_preserves_all_real_routes() {
    // temp owns the exact package and workspace until physical native closure.
    // temp 拥有精确插件包及工作区直到物理原生关闭。
    let temp = tempfile::tempdir().expect("create generation fixture");
    // runtime_root selects the real engine layout.
    // runtime_root 选择真实引擎布局。
    let runtime_root = temp.path().join("runtime");
    // plugin_root selects the single real source parent.
    // plugin_root 选择唯一真实来源父目录。
    let plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&plugin_root).expect("create plugin root");
    write_idle_session_package(&plugin_root);
    // package is the existing idle fixture whose declared routes are extended explicitly.
    // package 是通过明确声明扩展路径的既有 idle 夹具。
    let package = plugin_root.join("idle-plugin");
    // manifest adds an actual session-call declaration rather than assuming one already exists.
    // manifest 加入真实会话调用声明，而非假定它已存在。
    let mut manifest = fs::read_to_string(package.join("plugin.yaml")).expect("read manifest");
    manifest.push_str("    - id: idle_call\n      group: idle_tools\n      description: Call the real pinned session.\n      parameters_file: schemas/close.json\n      execution:\n        kind: session_call\n        operation: call\n        timeout_ms: 1000\n");
    fs::write(package.join("plugin.yaml"), manifest).expect("write call declaration");
    fs::write(
        package.join("runtime/plugin.bundle.lua"),
        r#"-- M owns the real per-VM module.
-- M 拥有真实单 VM 模块。
local M = {}
-- count observes business execution in this physical VM.
-- count 观察此物理 VM 中的业务执行。
local count = 0
-- Dispatch accepts the original envelope and returns the actual invocation count.
-- Dispatch 接纳原信封并返回真实调用计数。
-- _args is the host input; returns a version-one success result.
-- _args 是宿主输入；返回版本一成功结果。
function M.dispatch(_args)
    count = count + 1
    return '{"ok":true,"result":{"accepted":true,"count":' .. tostring(count) .. '}}'
end
-- Shutdown preserves the original business counter while closing the actual VM.
-- Shutdown 在关闭真实 VM 时保留原业务计数。
-- _args is the cleanup envelope; returns real closure success and the same counter.
-- _args 是清理信封；返回真实关闭成功与同一计数。
function M.shutdown(_args)
    return '{"ok":true,"result":{"closed":true,"count":' .. tostring(count) .. '}}'
end
return M
"#,
    )
    .expect("write actual counter module");
    // manager retains the actual catalog, parent, and owner-bound sessions.
    // manager 保留真实目录、父级与所有者绑定会话。
    let manager = test_manager(&runtime_root, plugin_root);
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("initialize real parent");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish first catalog");
    // generation is the actual descriptor generation captured before any refresh.
    // generation 是任何刷新之前捕获的真实描述符代次。
    let generation = manager.snapshot().enabled_tools["idle_ping"].plugin_generation;
    // runtime executes actual asynchronous native calls.
    // runtime 执行真实异步原生调用。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create executor");
    // request binds the exact tool and JSON arguments to this real owner and workspace.
    // request 将精确工具与 JSON 参数绑定到此真实所有者及工作区。
    let request = |tool_id: &str, arguments| VulcanPluginInvocationRequest {
        tool_id: tool_id.to_owned(),
        arguments,
        owner_session_id: String::from("owner-generation"),
        workspace_root: temp.path().to_path_buf(),
    };
    // stateless proves the explicitly bound route performs real business.
    // stateless 证明显式绑定路径执行真实业务。
    let stateless = runtime
        .block_on(manager.invoke_in_generation(
            request("idle_ping", json!({})),
            "idle-plugin",
            generation,
        ))
        .expect("bound stateless");
    assert_eq!(stateless["accepted"], true);
    // opened is the real bound open result, never a test-synthesized session.
    // opened 是真实绑定打开结果，而非测试合成会话。
    let opened = runtime
        .block_on(manager.invoke_in_generation(
            request("idle_open", json!({})),
            "idle-plugin",
            generation,
        ))
        .expect("bound open");
    // session_id comes only from the actual public open response.
    // session_id 仅来自真实公开打开响应。
    let session_id = opened["session_id"]
        .as_str()
        .expect("real session ID")
        .to_owned();
    // called proves declared session-call uses the same physical VM after its open.
    // called 证明声明的会话调用在打开后使用同一物理 VM。
    let called = runtime
        .block_on(manager.invoke_in_generation(
            request("idle_call", json!({"session_id": session_id})),
            "idle-plugin",
            generation,
        ))
        .expect("bound call");
    assert_eq!(called["result"]["count"], 2);
    // closed proves bound close actually retires the original session.
    // closed 证明绑定关闭实际退役原会话。
    let closed = runtime
        .block_on(manager.invoke_in_generation(
            request("idle_close", json!({"session_id": session_id})),
            "idle-plugin",
            generation,
        ))
        .expect("bound close");
    assert_eq!(closed["closed"], true);
    assert_eq!(
        manager
            .active_session_count("idle-plugin")
            .expect("actual count"),
        0
    );
    fs::write(package.join("dependencies.yaml"), "# generation change\n")
        .expect("change fingerprint");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish next catalog");
    // current_generation is the actual descriptor identity after real source refresh.
    // current_generation 是真实来源刷新后的描述符身份。
    let current_generation = manager.snapshot().enabled_tools["idle_ping"].plugin_generation;
    assert_ne!(generation, current_generation);
    // ordinary_stateless proves the existing API remains current-ID late-bound.
    // ordinary_stateless 证明既有 API 仍按当前标识晚绑定。
    let ordinary_stateless = runtime
        .block_on(manager.invoke(request("idle_ping", json!({}))))
        .expect("ordinary stateless");
    assert_eq!(ordinary_stateless["accepted"], true);
    // ordinary_open uses the actual new generation through the unchanged public API.
    // ordinary_open 通过不变公开 API 使用真实新代次。
    let ordinary_open = runtime
        .block_on(manager.invoke(request("idle_open", json!({}))))
        .expect("ordinary open");
    // current_session identifies that exact currently live session.
    // current_session 标识该精确当前活动会话。
    let current_session = ordinary_open["session_id"]
        .as_str()
        .expect("current session ID")
        .to_owned();
    // tool_id and arguments enumerate the four actual manifest declarations.
    // tool_id 与 arguments 枚举四个真实清单声明。
    for (tool_id, arguments) in [
        ("idle_ping", json!({})),
        ("idle_open", json!({})),
        ("idle_call", json!({"session_id": current_session})),
        ("idle_close", json!({"session_id": current_session})),
    ] {
        // error must be the exact generation rejection before any dispatch.
        // error 必须是任何分派之前的精确代次拒绝。
        let error = runtime
            .block_on(manager.invoke_in_generation(
                request(tool_id, arguments),
                "idle-plugin",
                generation,
            ))
            .expect_err("stale binding must reject every route");
        assert_eq!(
            error.to_string(),
            format!(
                "Vulcan Plugin tool `{tool_id}` was frozen for plugin `idle-plugin` generation {generation}, but current plugin is `idle-plugin` generation {current_generation}."
            )
        );
    }
    assert_eq!(
        manager
            .active_session_count("idle-plugin")
            .expect("still-live actual count"),
        1
    );
    // fresh_call proves stale call did not increment the VM and stale close did not retire it.
    // fresh_call 证明陈旧调用没有增加 VM 计数且陈旧关闭没有退役它。
    let fresh_call = runtime
        .block_on(manager.invoke_in_generation(
            request("idle_call", json!({"session_id": current_session})),
            "idle-plugin",
            current_generation,
        ))
        .expect("fresh bound call");
    assert_eq!(fresh_call["result"]["count"], 2);
    // ordinary_call preserves current-ID call behavior on the same real session.
    // ordinary_call 保留相同真实会话上的当前标识调用行为。
    let ordinary_call = runtime
        .block_on(manager.invoke(request("idle_call", json!({"session_id": current_session}))))
        .expect("ordinary call");
    assert_eq!(ordinary_call["result"]["count"], 3);
    // ordinary_close preserves current-ID cleanup and complete actual retirement.
    // ordinary_close 保留当前标识清理与完整真实退役。
    let ordinary_close = runtime
        .block_on(manager.invoke(request(
            "idle_close",
            json!({"session_id": current_session}),
        )))
        .expect("ordinary close");
    assert_eq!(ordinary_close["closed"], true);
    assert_eq!(ordinary_close["result"]["count"], 3);
    assert_eq!(
        manager
            .active_session_count("idle-plugin")
            .expect("final actual count"),
        0
    );
    manager
        .close_formal_runtime_blocking()
        .expect("close original parent");
}

/// Rejects a real replacement plugin with the same tool id and the same per-plugin generation number.
/// 拒绝具有相同工具标识及相同分域代次数字的真实替代插件。
/// No parameters; actual directory rename and catalog refresh produce both real descriptors.
/// 无参数；真实目录重命名与目录刷新产生两个真实描述符。
#[test]
fn generation_bound_invocation_rejects_another_plugin_with_same_number() {
    // temp retains the actual package and workspace until parent closure.
    // temp 保留真实插件包和工作区直到父级关闭。
    let temp = tempfile::tempdir().expect("create identity fixture");
    // runtime_root selects the real engine layout.
    // runtime_root 选择真实引擎布局。
    let runtime_root = temp.path().join("runtime");
    // plugin_root owns the exact deployed package directories.
    // plugin_root 拥有精确部署插件包目录。
    let plugin_root = runtime_root.join("plugins");
    fs::create_dir_all(&plugin_root).expect("create plugin root");
    write_idle_session_package(&plugin_root);
    // manager keeps the same actual native parent across the identity change.
    // manager 在身份变化期间保持相同真实原生父级。
    let manager = test_manager(&runtime_root, plugin_root.clone());
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("initialize original parent");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load original plugin");
    // original_tool captures both identity fields from the actual first descriptor.
    // original_tool 从真实首个描述符捕获两个身份字段。
    let original_tool = manager.snapshot().enabled_tools["idle_ping"].clone();
    // runtime executes actual asynchronous native operations.
    // runtime 执行真实异步原生操作。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create identity executor");
    // request supplies the stable tool id and the same actual owner workspace.
    // request 提供稳定工具标识及相同真实所有者工作区。
    let request = || VulcanPluginInvocationRequest {
        tool_id: String::from("idle_ping"),
        arguments: json!({}),
        owner_session_id: String::from("owner-identity"),
        workspace_root: temp.path().to_path_buf(),
    };
    // original_result proves the original exact identity was usable.
    // original_result 证明原精确身份可用。
    let original_result = runtime.block_on(manager.invoke_in_generation(
        request(),
        &original_tool.plugin_id,
        original_tool.plugin_generation,
    ));
    // original_package identifies the actual first source directory.
    // original_package 标识真实首个来源目录。
    let original_package = plugin_root.join("idle-plugin");
    // replacement_package names the different real source required by manifest identity validation.
    // replacement_package 命名清单身份校验要求的不同真实来源。
    let replacement_package = plugin_root.join("replacement-plugin");
    // manifest preserves the same tool id while replacing only the package identity.
    // manifest 保留相同工具标识，仅替换插件包身份。
    let manifest = fs::read_to_string(original_package.join("plugin.yaml")).expect("read manifest");
    fs::rename(&original_package, &replacement_package).expect("move actual package");
    fs::write(
        replacement_package.join("plugin.yaml"),
        manifest.replacen("id: idle-plugin\n", "id: replacement-plugin\n", 1),
    )
    .expect("write matching real replacement identity");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load replacement");
    // replacement_tool is selected from the actual rebuilt enabled-tool catalog.
    // replacement_tool 从真实重建已启用工具目录选择。
    let replacement_tool = manager.snapshot().enabled_tools["idle_ping"].clone();
    // rejected observes the old exact identity against the real replacement plugin.
    // rejected 观察旧精确身份面对真实替代插件的结果。
    let rejected = runtime.block_on(manager.invoke_in_generation(
        request(),
        &original_tool.plugin_id,
        original_tool.plugin_generation,
    ));
    // fresh executes with the actual replacement identity.
    // fresh 使用真实替代身份执行。
    let fresh = runtime.block_on(manager.invoke_in_generation(
        request(),
        &replacement_tool.plugin_id,
        replacement_tool.plugin_generation,
    ));
    // ordinary preserves intentional stable-ID late binding to the replacement plugin.
    // ordinary 保留有意按稳定标识晚绑定到替代插件的行为。
    let ordinary = runtime.block_on(manager.invoke(request()));
    manager
        .close_formal_runtime_blocking()
        .expect("close original parent");
    assert_eq!(original_tool.plugin_id, "idle-plugin");
    assert_eq!(replacement_tool.plugin_id, "replacement-plugin");
    assert_eq!(
        original_tool.plugin_generation, replacement_tool.plugin_generation,
        "the regression must exercise equal actual per-plugin generation numbers"
    );
    assert_eq!(
        original_result.expect("original identity works")["accepted"],
        true
    );
    assert_eq!(
        rejected
            .expect_err("different plugin must reject")
            .to_string(),
        format!(
            "Vulcan Plugin tool `idle_ping` was frozen for plugin `idle-plugin` generation {}, but current plugin is `replacement-plugin` generation {}.",
            original_tool.plugin_generation, replacement_tool.plugin_generation
        )
    );
    assert_eq!(fresh.expect("replacement identity works")["accepted"], true);
    assert_eq!(
        ordinary.expect("ordinary API stays late-bound")["accepted"],
        true
    );
}
