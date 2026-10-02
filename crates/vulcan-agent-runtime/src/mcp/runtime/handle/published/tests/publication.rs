//! Real closed preparation transfers its exact process only through the full pointer and then releases without historical owners.
//! 真实关闭准备仅通过完整指针转交其精确进程，随后无需历史所有者释放。

use super::*;
use crate::agent_runtime::conversation_hooks::{
    ConversationHookRuntime, VulcanPluginConversationHooks,
};
use vulcan_plugin_runtime::VulcanPluginCatalogSnapshot;

mod empty;

/// Prepare genuine frozen conversation handlers and a complete extension from the specified original resources.
/// 从指定原资源准备真实冻结会话处理器及完整扩展。
/// manager and catalog are actual producers; revision and access are exact host candidates; returns the full inert Arc.
/// manager 及 catalog 为实际生产者；revision 及 access 为精确宿主候选；返回完整尚未执行 Arc。
fn full_extension(
    manager: &VulcanPluginRuntimeManager,
    catalog: &Arc<VulcanPluginCatalogSnapshot>,
    revision: u64,
    access: McpRuntimeAccess,
) -> Arc<ExtensionGeneration> {
    // The real host policies supply registration semantics without invoking a plugin callback.
    // 真实宿主策略提供登记语义，不调用插件回调。
    let mut host = ConversationHookRuntime::new();
    // The actual catalog and manager are passed to the same frozen-handler factory used by the service bridge.
    // 实际目录及管理器传入服务桥使用的同一冻结处理器工厂。
    let hooks = VulcanPluginConversationHooks::prepare(
        host.dispatcher_mut(),
        manager.clone(),
        Arc::clone(catalog),
    )
    .expect("actual frozen plugin handlers");
    Arc::new(ExtensionGeneration::new_with_plugin_conversation_hooks(
        revision,
        Arc::new(hooks),
        access,
        None,
    ))
}

/// Capture one binding from real protocol discovery while retaining the exact original preparation lease.
/// 保留精确原准备租约时，从真实协议发现捕获单个绑定。
/// native is the actual single-shot producer; returns its closed original_process binding, never a readiness receipt.
/// native 为实际单次生产者；返回其关闭 original_process 绑定，绝非就绪回执。
fn original_binding(native: &PublishedPluginMcpRuntime) -> McpRuntimeToolBinding {
    // The original control owns exactly one actual lease before full publication.
    // 完整发布之前，原控制拥有恰好一个实际租约。
    let state = native.lease.lock().expect("actual preparation ownership");
    // Access only this exact preparation; no independently allocated wrapping lease is accepted.
    // 仅访问此精确准备；不接受独立分配的包装租约。
    let PublishedMcpLeaseState::Preparing(lease) = &*state else {
        panic!("expected actual unpublished preparation");
    };
    // Find the real declaration by stable origin identity rather than a mutable discovery position.
    // 按稳定原身份查找真实声明，不使用可变发现位置。
    let view = native
        .observation
        .runtime
        .snapshot()
        .filtered_tool_views(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|view| view.origin_tool_id() == "bound-plugin:local/original_process")
        .expect("actual original_process declaration may have changed");
    McpRuntimeToolBinding::new(
        view,
        Arc::clone(lease),
        format!(
            "plugin:{}:{}",
            native.observation.plugin_id, native.observation.generation
        ),
    )
}

/// One full actual pointer opens the same original PID, and dropping actual published owners retires it despite retained control.
/// 单个实际完整指针开放同一原 PID；即使保留控制，销毁实际已发布所有者也会退役它。
/// No inputs or return value; genuine calls, independent cleanup and production environment repair prove the ownership boundary.
/// 无输入及返回值；真实调用、独立清理及生产环境修复证明所有权边界。
#[tokio::test]
async fn published_full_pointer_opens_original_process_and_retained_control_does_not_pin_it() {
    // The sealed source was genuinely selected and its unused candidate request has already been aborted.
    // 密封来源已真实选定，其未使用候选请求也已中止。
    let PublishedFixture {
        manager,
        configs,
        authority,
        marker,
        environment,
        _temp,
        ..
    } = published_fixture("ready_with_tool");
    // This original immutable catalog is shared by preparation and full handler publication.
    // 此原不可变目录由准备及完整处理器发布共同使用。
    let catalog = manager.snapshot();
    // The real empty bootstrap establishes all admission and observation registries.
    // 真实空引导建立全部入场及观察注册表。
    let access = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual empty bootstrap");
    // The original host slot begins with full frozen handlers but no plugin native partition.
    // 原宿主槽以完整冻结处理器开始，但没有插件原生分区。
    let expected = full_extension(&manager, &catalog, catalog.revision, access.clone());
    // One closed original native producer consumes its exact sealed configuration once.
    // 单个关闭原原生生产者恰好一次消费其精确密封配置。
    let native = PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority)
        .expect("actual original producer");
    // Retained observation and source must survive eventual native and environment retirement.
    // 保留的观察及来源须跨最终原生及环境退役存续。
    let observation = native.observer();
    // This pure source carries no File or strong package/environment resource owner.
    // 此纯来源不携带 File 或强插件包／环境资源所有者。
    let profile = observation.profile();
    observation
        .wait_until_ready(OBSERVE)
        .await
        .expect("real original protocol ready");
    // The captured binding is a genuine view of the same actual process, initially denied by its business gate.
    // 捕获绑定为同一实际进程的真实视图，最初由业务门拒绝。
    let binding = original_binding(&native);
    // Arguments belong to the fixture's actual declared schema and do not contain protected launch inputs.
    // 参数属于夹具实际声明 Schema，不包含受保护启动输入。
    let args = serde_json::json!({"message":"same original publication"});
    assert!(binding.invoke(args.clone()).await.is_err());
    // Only this actual child writes the original complete startup record.
    // 仅此实际子进程写入原完整启动记录。
    let starts = recorded_starts(&marker).await;
    assert_eq!(
        starts.len(),
        1,
        "expected single original startup may have changed"
    );
    // Copy observed PID solely as call-result evidence, never as publication authority.
    // 仅复制观察到的 PID 作为调用结果证据，绝不作为发布权威。
    let pid = starts.first().expect("actual producer record")["pid"]
        .as_u64()
        .expect("actual child PID");
    // Derive from the actual captured current access before any publication guard exists.
    // 任何发布守卫存在之前，从实际捕获当前访问派生。
    let publication_access = native
        .publication_access(&expected.mcp_runtime())
        .expect("exact prepared lease access");
    assert!(
        publication_access
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // The complete successor prepares exact handlers and same native lease before original source exclusion.
    // 原来源互斥之前，完整后继准备精确处理器及同一原生租约。
    let next = full_extension(
        &manager,
        &catalog,
        expected.revision() + 1,
        publication_access,
    );
    // This slot is the fixture host's actual pointer, not a returned boolean or mocked completion record.
    // 此槽为夹具宿主实际指针，不是返回布尔值或模拟完成记录。
    let mut slot = Arc::clone(&expected);
    // Full-pointer transfer consumes the real source and native guards in one synchronous boundary.
    // 完整指针转交在单个同步边界中消费真实来源及原生守卫。
    let committed = native
        .prepare_publication(&manager, &authority, &catalog)
        .expect("actual source and native exclusion")
        .prepare_extension(&expected, next)
        .expect("same complete successor")
        .publish_into(&mut slot)
        .expect("actual full pointer commit");
    assert!(Arc::ptr_eq(&slot, &committed));
    assert!(native.matches_original_publication(&committed));
    // The binding captured before publication now invokes the same original child without replacement.
    // 发布前捕获的绑定现在调用同一原子进程，无需替换。
    let result = binding
        .invoke(args)
        .await
        .expect("same original binding opened");
    assert_eq!(
        result["pid"].as_u64(),
        Some(pid),
        "business result must identify the exact original child PID"
    );
    assert_eq!(result["message"], "same original publication");
    assert_eq!(
        recorded_starts(&marker).await,
        starts,
        "full publication must not replay initialization"
    );
    assert!(
        !slot
            .mcp_runtime()
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // Release every actual published execution owner without explicitly shutting down this retained control.
    // 释放每个实际已发布执行所有者，不显式关闭此保留控制。
    drop(binding);
    drop(committed);
    drop(slot);
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("last actual lease releases original despite historical control");
    assert!(
        observation
            .verify_source("bound-plugin:local", &profile)
            .is_err()
    );
    manager
        .begin_draining("bound-plugin")
        .expect("actual original disabling");
    manager
        .finish_lifecycle_transition(
            "bound-plugin",
            &BTreeSet::from([String::from("bound-plugin")]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("actual disabled catalog");
    assert!(
        environment.starts_with(
            _temp
                .path()
                .canonicalize()
                .expect("isolated actual fixture root")
        )
    );
    std::fs::remove_file(environment.join("bin/server.bin"))
        .expect("remove isolated retired original binary");
    assert_eq!(
        manager
            .prepare_plugin_environment("bound-plugin")
            .expect("retained native control must not pin environment File"),
        environment
    );
    assert!(profile.same_source(&observation.profile()));
    assert_eq!(
        recorded_starts(&marker).await,
        starts,
        "real repair must not replay original startup"
    );
}

/// Foreign full inputs and an actual pointer conflict leave this same real preparation closed and explicitly retryable.
/// 外来完整输入及实际指针冲突使此同一真实准备保持关闭，并可显式重试。
/// No inputs or return value; actual original bindings and unchanged child records prove no partial confirmation or replay.
/// 无输入及返回值；实际原绑定及不变子进程记录证明没有部分确认或重播。
#[tokio::test]
async fn published_full_pointer_rejects_foreign_catalog_lineage_missing_lease_and_pointer_conflict()
{
    // Each complete original source has one genuine manager and first host issuer.
    // 每个完整原来源均具有单个真实管理器及首次宿主签发者。
    let PublishedFixture {
        manager,
        configs,
        authority,
        marker,
        _temp,
        ..
    } = published_fixture("ready_with_tool");
    // Save the original exact catalog before real dispatch fixes its same-source profile.
    // 真实派发固定同来源画像之前，保存原精确目录。
    let catalog = manager.snapshot();
    // Bootstrap actual service lineage and an independent foreign lineage without native tasks.
    // 引导实际服务谱系及独立外来谱系，不产生原生任务。
    let access = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual original access");
    // Independently allocated registries must never be accepted by an equal route.
    // 独立分配的注册表绝不得因相同路由而被接纳。
    let foreign = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual foreign access");
    // Full actual original pointer owns these original captured host resources.
    // 实际完整原指针拥有这些原捕获宿主资源。
    let expected = full_extension(&manager, &catalog, catalog.revision, access.clone());
    // The original starts once, with business admission closed.
    // 原实例仅启动一次，业务入场保持关闭。
    let native = PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority)
        .expect("actual original preparation");
    native
        .observer()
        .wait_until_ready(OBSERVE)
        .await
        .expect("real original readiness");
    // The same binding observes business admission through every rejected publication.
    // 相同绑定跨每次被拒绝发布观察业务入场。
    let binding = original_binding(&native);
    assert!(
        native
            .publication_access(&foreign)
            .expect_err("foreign actual lineage")
            .contains("another access lineage")
    );
    // A copied equal catalog is a different actual source and cannot be paired with the original producer.
    // 复制的等值目录为不同实际来源，不能与原生产者配对。
    let copied = Arc::new((*catalog).clone());
    assert!(
        native
            .prepare_publication(&manager, &authority, &copied)
            .is_err()
    );
    // Full handlers alone do not attest the native lease; this next deliberately lacks the original prepared partition.
    // 仅完整处理器不能证明原生租约；此后继刻意缺少原已准备分区。
    let missing = full_extension(&manager, &catalog, expected.revision() + 1, access.clone());
    assert!(
        native
            .prepare_publication(&manager, &authority, &catalog)
            .expect("same native source remains ready")
            .prepare_extension(&expected, missing)
            .is_err()
    );
    assert!(
        binding
            .invoke(serde_json::json!({"message":"still closed"}))
            .await
            .is_err()
    );
    // The actual host pointer changes independently after the original was captured, even though its values match.
    // 原值捕获之后，实际宿主指针独立变化，即使其值相同。
    let mut slot = full_extension(&manager, &catalog, expected.revision(), access.clone());
    // This exact successor still contains the original ready native lease, but its expected pointer is stale.
    // 此精确后继仍包含原就绪原生租约，但其预期指针已过期。
    let next = full_extension(
        &manager,
        &catalog,
        expected.revision() + 1,
        native
            .publication_access(&access)
            .expect("original prepared access"),
    );
    // Keep the actual replacement pointer to prove no partial full mutation follows the conflict.
    // 保留实际替代指针，证明冲突之后没有部分完整修改。
    let conflicting = Arc::clone(&slot);
    assert!(
        native
            .prepare_publication(&manager, &authority, &catalog)
            .expect("actual source guard")
            .prepare_extension(&expected, next)
            .expect("same exact lease successor")
            .publish_into(&mut slot)
            .expect_err("actual original pointer conflict")
            .contains("pointer changed")
    );
    assert!(Arc::ptr_eq(&slot, &conflicting));
    assert!(!native.matches_original_publication(&slot));
    assert!(
        binding
            .invoke(serde_json::json!({"message":"conflict remains closed"}))
            .await
            .is_err()
    );
    // Explicitly prepare against the actual new original pointer; no automatic retry or child startup occurs.
    // 显式对照实际新原指针准备；不发生自动重试或子进程启动。
    let next = full_extension(
        &manager,
        &catalog,
        conflicting.revision() + 1,
        native
            .publication_access(&conflicting.mcp_runtime())
            .expect("same original lease retained"),
    );
    native
        .prepare_publication(&manager, &authority, &catalog)
        .expect("original remains ready")
        .prepare_extension(&conflicting, next)
        .expect("actual new original pointer")
        .publish_into(&mut slot)
        .expect("explicit same-instance full publication");
    assert!(
        binding
            .invoke(serde_json::json!({"message":"explicit original retry"}))
            .await
            .expect("original opened after explicit commit")
            .to_string()
            .contains("explicit original retry")
    );
    assert_eq!(
        recorded_starts(&marker).await.len(),
        1,
        "rejected or explicitly retried publication must not replay startup"
    );
    native.shutdown();
    native
        .observer()
        .wait_until_released(OBSERVE)
        .await
        .expect("same original cleanup");
}
