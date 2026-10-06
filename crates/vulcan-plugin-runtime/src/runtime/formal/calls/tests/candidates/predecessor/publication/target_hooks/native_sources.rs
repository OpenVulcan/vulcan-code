//! Real private target initialization binds process permissions and cannot edit settings before publication.
//! 真实私有目标初始化绑定进程权限，且发布前不能编辑设置。

use super::*;

/// The real target proves process-session grants and settings ownership through actual native callbacks.
/// 真实目标通过实际原生回调证明进程会话授权及设置所有权。
const NATIVE_TARGET: &str = r#"
-- Read the full target selection before any business or initialization effect.
-- 在任何业务或初始化副作用前读取完整目标选择。
assert(vulcan.host.call('vulcan.plugin.config.get', {}).label == 'new-selected')
-- Resolve the actual declared binary in session mode; the predecessor has no process-session grant.
-- 以会话模式解析实际声明二进制；前驱没有进程会话授权。
-- A named supported option preserves an object through the Lua-to-JSON boundary.
-- 具名受支持选项在 Lua 转 JSON 边界保持对象形状。
local spec = vulcan.host.call('vulcan.plugin.process.resolve', {entry_id='fixture',mode='session',options={encoding='utf-8'}})
assert(type(spec) == 'table' and spec.encoding == 'utf-8')
-- Attempt a mutable write; candidate generations are also rejected by the ordinary current-owner gate.
-- 尝试可变写入；候选代次也会被普通当前所有者门拒绝。
local changed, reason = pcall(vulcan.host.call, 'vulcan.plugin.config.update', {value={label='forbidden'}})
assert(not changed, tostring(reason))
-- Record the rejection separately so private initialization must prove this native denial.
-- 单独记录拒绝，使私有初始化必须证明此原生拒绝。
vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='target-immutable-config-denied',revision=0})
-- Record actual initialization only after process resolution and immutable-write rejection both complete.
-- 仅进程解析及不可变写入拒绝均完成后记录实际初始化。
vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='target-native-init',revision=0})
-- One actual module supplies every declared export.
-- 单个实际模块提供每个声明导出。
local M = {}
-- Attempt a settings edit on a real ordinary callback after private source transfer; report it with the call's own snapshot.
-- 私有来源转交后，在真实普通回调内尝试编辑设置；连同本次调用自身快照报告结果。
function M.dispatch()
    -- A published managed plugin edits its settings section in place once no transaction owner remains.
    -- 已无交易所有者时，已发布受管插件就地编辑其设置段。
    local changed = pcall(vulcan.host.call, 'vulcan.plugin.config.update', {value={label='ordinary-edited'}})
    local current = vulcan.host.call('vulcan.plugin.config.get', {})
    return vulcan.json.encode({ok=true,result={label=current.label,edited=changed}})
end
-- Hook and UI declarations use the same tested ordinary callback after publication.
-- Hook 及界面声明在发布后使用同一受测普通回调。
M.on_precheck, M.ui_action = M.dispatch, M.dispatch
-- Cleanup exports consume no arguments and return the exact version-one success frame without writes.
-- 清理导出不消费参数，并返回精确版本一成功帧，不执行写入。
function M.shutdown() return '{"ok":true,"result":{}}' end
M.shutdown_hooks = M.shutdown
-- The exact original lifecycle envelope identifies one callback; return the success frame after native evidence.
-- 精确原生命周期信封标识单个回调；在原生证据之后返回成功帧。
function M.on_lifecycle(args)
    assert(type(args.request_id) == 'string' and #args.request_id == 36)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='target-' .. args.event,revision=0})
    return '{"ok":true,"result":{}}'
end
return M
"#;

/// Prepare different process grants through real inspected source bytes, approved selections and original callbacks.
/// 通过真实已检查来源字节、已批准选择及原回调准备不同进程授权。
/// Return the original published target transaction without staging or initializing its private Hook.
/// 返回原已发布目标事务，不暂存或初始化其私有 Hook。
fn published_native_target() -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
) {
    // The common fixture owns actual native services, declared binaries and the sole formal parent.
    // 公共夹具拥有实际原生服务、声明二进制及唯一正式父级。
    let fixture = Fixture::new();
    // Modify only this known temporary source before production inspection and immutable copying.
    // 仅在生产检查及不可变复制之前修改此已知临时来源。
    let package = fixture._root.path().join("bundled/hook-plugin");
    // This predecessor declares configuration but deliberately lacks process-session permission.
    // 此前驱声明配置，但有意缺少进程会话权限。
    let manifest = std::fs::read_to_string(package.join("plugin.yaml"))
        .expect("actual predecessor manifest")
        .replace("permissions: [process_binary, ui_host_context]", "permissions: [process_binary, ui_host_context, config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json");
    std::fs::write(package.join("plugin.yaml"), manifest)
        .expect("actual predecessor config declaration");
    std::fs::write(package.join("config.schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#).expect("actual predecessor schema");
    std::fs::write(
        package.join("config.defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("actual predecessor defaults");
    // The original callback itself must reject session resolution before the target is prepared.
    // 目标准备之前，原回调本身必须拒绝会话解析。
    let old_bundle = std::fs::read_to_string(package.join("runtime/plugin.bundle.lua"))
        .expect("actual old callback bundle")
        .replace(
            "    local escaped = pcall",
            "    -- Original predecessor grants do not authorize valid process-session requests.\n    -- 原前驱授权不允许合法进程会话请求。\n    local session_allowed, session_error = pcall(vulcan.host.call, 'vulcan.plugin.process.resolve', {entry_id='fixture',mode='session',options={encoding='utf-8'}})\n    assert(not session_allowed and string.find(tostring(session_error), 'did not declare process_session capability', 1, true), tostring(session_error))\n    local escaped = pcall",
        );
    std::fs::write(package.join("runtime/plugin.bundle.lua"), old_bundle)
        .expect("actual old native session denial");
    // The resolved source remains inside this fixture's absolute temporary directory.
    // 已解析来源保持位于此夹具绝对临时目录内。
    let source = fixture._root.path().join("source/hook-plugin");
    assert!(package.starts_with(fixture._root.path()) && source.starts_with(fixture._root.path()));
    std::fs::create_dir_all(source.parent().expect("literal fixture source parent"))
        .expect("actual source parent");
    std::fs::rename(&package, &source).expect("move this actual temporary package");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove bundled fixture discovery");
    // Production publication creates the actual full predecessor and its immutable selected document.
    // 生产发布创建实际完整前驱及其不可变已选文档。
    let (store, initial) = transaction_for(&fixture, &source, true);
    // Close only the unused fixture request before creating the tested update.
    // 创建受测更新前，仅关闭未使用夹具请求。
    let initial_id = initial.record().request_id();
    drop(initial);
    store
        .abort_activation_request("hook-plugin", initial_id)
        .expect("finish unused original request");
    // Actual published permissions remain observable while preparing the different target source.
    // 准备不同目标来源期间，实际已发布权限保持可观测。
    let previous = fixture.manager.snapshot();
    assert!(
        !previous.plugins["hook-plugin"]
            .manifest
            .as_ref()
            .expect("actual old manifest")
            .permissions
            .contains(&crate::VulcanPluginPermission::ProcessSession)
    );
    // Parse and edit the actual source manifest before its fingerprint and approved target grants are produced.
    // 在生成指纹及已批准目标授权之前，解析并编辑实际来源清单。
    let manifest_path = source.join("plugin.yaml");
    // The uniquely declared permission field is the authority for the new required grant.
    // 唯一声明权限字段是新所需授权的权威。
    let mut target_manifest: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(&manifest_path).expect("actual unapproved target manifest"),
    )
    .expect("actual parsed target manifest");
    target_manifest["permissions"]
        .as_sequence_mut()
        .expect("actual permissions field")
        .push(serde_yaml::Value::String(
            crate::runtime::plugin_permission_key(crate::VulcanPluginPermission::ProcessSession)
                .to_string(),
        ));
    std::fs::write(
        &manifest_path,
        serde_yaml::to_string(&target_manifest).expect("encode actual target manifest"),
    )
    .expect("actual changed target declaration");
    std::fs::write(source.join("runtime/plugin.bundle.lua"), NATIVE_TARGET)
        .expect("actual native target source");
    // Production inspection and preparation bind these changed bytes and the complete new document to one request.
    // 生产检查及准备将这些已变字节及完整新文档绑定至单个请求。
    let mut transaction = prepare_configured(
        &fixture,
        &store,
        VulcanPluginConfigurationSelection::Document {
            value: json!({"label":"new-selected"}),
        },
    );
    assert!(
        transaction
            .record()
            .target()
            .approved_permissions()
            .iter()
            .any(|permission| permission
                == crate::runtime::plugin_permission_key(
                    crate::VulcanPluginPermission::ProcessSession,
                ))
    );
    {
        // The real candidate owns the original request lock and approved target permissions.
        // 真实候选拥有原请求锁及已批准目标权限。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("actual changed candidate");
        // The actual predecessor drain closes all old native work before any new callback begins.
        // 实际前驱排空在任何新回调开始前关闭全部旧原生工作。
        let drain = fixture
            .manager
            .begin_activation_predecessor_drain(&candidate, &previous)
            .expect("actual old resource drain");
        executor().block_on(async {
            assert!(
                fixture
                    .manager
                    .drain_activation_predecessor(&drain)
                    .await
                    .expect("actual old retirement")
                    .failures
                    .is_empty()
            );
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .expect("actual old disable");
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect("actual target probe");
            fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .expect("actual target install");
            fixture
                .manager
                .enable_activation_candidate(&candidate)
                .await
                .expect("actual target enable");
            fixture.empty().await;
        });
    }
    store
        .commit_activation_transaction(&mut transaction)
        .expect("actual immutable target publication");
    (fixture, store, transaction)
}

/// Private initialization uses true target process grants and cannot edit settings; an ordinary callback edits only the settings section.
/// 私有初始化使用真实目标进程授权且不能编辑设置；普通回调只编辑设置段。
/// No inputs or return value; real callbacks, persistent files and physical retirement are the evidence.
/// 无输入及返回值；真实回调、持久文件及物理退役构成证据。
#[test]
fn private_target_native_process_and_settings_edits_preserve_package_selection() {
    // The published update carries an approved permission absent from its actual predecessor.
    // 已发布更新携带实际前驱缺少的已批准权限。
    let (fixture, store, mut transaction) = published_native_target();
    // The sole selection readback is captured before private initialization and checked after ordinary callbacks.
    // 唯一选择读回在私有初始化前捕获，并在普通回调后检查。
    let selected = store
        .activation("hook-plugin")
        .expect("actual full target selection")
        .expect("actual selected target");
    assert!(matches!(
        selected.configuration(),
        crate::VulcanPluginConfigurationSelection::Document { .. }
    ));
    // This actual legacy state path must remain absent through every denied mutable write.
    // 此实际旧格式状态路径须在每次被拒绝可变写入期间保持缺失。
    let state_path = crate::runtime::configuration::plugin_config_state_path(
        &fixture.manager.state.plugin_state_root,
        "hook-plugin",
    );
    assert!(
        !state_path.exists(),
        "configured selections must not create a mutable legacy document"
    );
    // Stage the exact closed Arc before real private target initialization.
    // 真实私有目标初始化前暂存精确关闭 Arc。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".to_string()]),
        )
        .expect("actual staged native target");
    // Count real native effects so private prewarming cannot be replaced by earlier candidate execution.
    // 计数真实原生副作用，使私有预热不能被较早候选执行替代。
    let initializations = fixture
        .events
        .values
        .lock()
        .expect("actual initialization events")
        .iter()
        .filter(|event| event.topic == "target-native-init")
        .count();
    // Private initialization must observe its own settings-edit rejection.
    // 私有初始化必须观察到其自身设置编辑被拒绝。
    let immutable_rejections = fixture
        .events
        .values
        .lock()
        .expect("actual immutable rejection events")
        .iter()
        .filter(|event| event.topic == "target-immutable-config-denied")
        .count();
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_target_hook(&transaction, &staged)
                .await
                .expect("actual private target process resolution")
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("actual private initialization event")
                .iter()
                .filter(|event| event.topic == "target-native-init")
                .count(),
            initializations + 1
        );
        assert_eq!(
            fixture
                .events
                .values
                .lock()
                .expect("actual private immutable rejection event")
                .iter()
                .filter(|event| event.topic == "target-immutable-config-denied")
                .count(),
            immutable_rejections + 1
        );
        // The original actual Hook source transfers only after confirmed full publication.
        // 原实际 Hook 来源仅在已确认完整发布后转交。
        let publication = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
            .expect("actual native target extension publication");
        // Retain the exact final Arc emitted by production before consuming completion.
        // 消费完成之前，保留生产发出的精确最终 Arc。
        let catalog = Arc::clone(publication.catalog());
        fixture
            .manager
            .complete_activation_target_handoff(&mut transaction, &publication)
            .expect("actual native target completion");
        // Resolve the real declared Hook by stable plugin identity, never an evolving array position.
        // 通过稳定插件身份解析真实声明 Hook，绝不使用演进数组位置。
        let hook = catalog
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual ordinary target Hook");
        // Ordinary source now runs in the same actual VM; the retained transaction owner still fences settings edits.
        // 普通来源现在在同一实际 VM 中运行；保留的交易所有者仍阻挡设置编辑。
        let invoke = || {
            fixture.manager.invoke_formal_hook(
                hook,
                json!({"payload_json":"{}","host_timeout_ms":1000}),
                None,
                Instant::now() + WAIT,
            )
        };
        let fenced = invoke().await.expect("actual ordinary fenced call");
        assert_eq!(fenced["edited"], false, "{fenced}");
        assert_eq!(
            store
                .activation("hook-plugin")
                .expect("read fenced selection"),
            Some(selected.clone())
        );
        // Releasing the transaction owner permits an in-place settings edit.
        // 释放交易所有者后允许就地编辑设置。
        drop(transaction);
        let result = invoke().await.expect("actual ordinary settings edit");
        assert_eq!(result["edited"], true, "{result}");
        // The running call keeps the snapshot it started with.
        // 运行中的调用保留其开始时的快照。
        assert_eq!(result["label"], "new-selected");
        let edited = store
            .activation("hook-plugin")
            .expect("read edited selection")
            .expect("edited selection");
        assert_eq!(edited.revision, selected.revision);
        assert_eq!(edited.package(), selected.package());
        assert_eq!(edited.settings().revision, selected.settings().revision + 1);
        assert_eq!(
            edited.configuration().value(),
            Some(&json!({"label": "ordinary-edited"}))
        );
        assert!(
            !state_path.exists(),
            "settings edits created mutable legacy state"
        );
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("actual normal target drain");
        retire_original_hook(&fixture).await;
    });
}
