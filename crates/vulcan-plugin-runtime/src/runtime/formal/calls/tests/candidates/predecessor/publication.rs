//! Real selected targets exercise closed staging and exact original route handoff.
//! 真实已选目标验证关闭暂存及精确原路由交接。

use super::callbacks::callback_fixture_with_target;
use super::*;

/// Handoff revision exhaustion cannot wrap identity, mutate the closed catalog or invoke the publisher.
/// 交接修订耗尽不能回绕身份、变更关闭目录或调用发布器。
/// No inputs or return value; both staging and publication use the actual sole manager counter.
/// 无输入与返回值；暂存及发布均使用实际唯一管理器计数器。
#[test]
fn formal_target_handoff_revision_exhaustion_keeps_business_closed() {
    for phase in ["staging", "publication"] {
        // Every scenario has one complete real selected original target.
        // 每个场景具有单个完整真实已选原目标。
        let (fixture, _store, transaction) = published_enabled_target();
        // The declared enabled target uses the same explicit host policy at both stages.
        // 声明启用目标在两个阶段均使用相同明确宿主策略。
        let enabled = BTreeSet::from(["hook-plugin".to_owned()]);
        // Preserve exact pointer identity for the expected closed failure state.
        // 为预期关闭失败状态保留精确指针身份。
        let closed = if phase == "publication" {
            fixture
                .manager
                .stage_activation_target_handoff(&transaction, &BTreeSet::new(), &enabled)
                .expect("closed target before exhaustion")
        } else {
            fixture.manager.snapshot()
        };
        fixture
            .manager
            .state
            .next_revision
            .store(u64::MAX, Ordering::Relaxed);
        // Actual exhaustion must be diagnosed before consuming any external publisher.
        // 实际耗尽必须在消费任何外部发布器前被诊断。
        let error = if phase == "publication" {
            fixture
                .manager
                .publish_activation_target_handoff(&transaction, &closed, |_| {
                    panic!("exhausted publication must never execute")
                })
                .expect_err("publication exhaustion")
        } else {
            fixture
                .manager
                .stage_activation_target_handoff(&transaction, &BTreeSet::new(), &enabled)
                .expect_err("staging exhaustion")
        };
        assert!(
            error.message.contains("revision is exhausted"),
            "{}",
            error.message
        );
        assert_eq!(
            fixture.manager.state.next_revision.load(Ordering::Relaxed),
            u64::MAX
        );
        assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
        assert_eq!(
            transaction.record().phase(),
            crate::VulcanPluginActivationTransactionPhase::Published
        );
    }
}

/// Observe the same original Hook cleanup across the fixture's deliberately short finite waits.
/// 跨夹具刻意设置的短有限等待，观测相同原 Hook 清理。
/// fixture owns the sole instance; returns only after actual retirement and rejects all non-timeout failures.
/// fixture 拥有唯一实例；仅在实际退役后返回，并拒绝全部非超时失败。
async fn retire_original_hook(fixture: &Fixture) {
    tokio::time::timeout(WAIT, async {
        loop {
            // Repeated observation uses the retained original instance rather than starting another shutdown.
            // 重复观测使用保留原实例，不启动另一关闭。
            let report = fixture
                .manager
                .drain_plugin_sessions("hook-plugin")
                .await
                .expect("original Hook cleanup");
            if report.failures.is_empty() {
                assert_eq!(report.closed_hook_runtimes, 1);
                break;
            }
            assert!(
                report.failures.iter().all(|failure| failure
                    == "persistent cleanup timed out; original ownership remains retained."),
                "{:?}",
                report.failures
            );
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual original Hook retirement");
}

/// Exact target exports keep lifecycle evidence distinct while persistent initialization reads target configuration.
/// 精确目标导出区分生命周期证据，持久初始化读取目标配置。
const TARGET: &str = r#"
-- Verify the exact selected target configuration through a real native callback.
-- 通过真实原生回调校验精确已选目标配置。
assert(vulcan.host.call('vulcan.plugin.config.get', {}).label == 'new-selected')
-- One module owns all declared exports.
-- 单个模块拥有全部声明导出。
local M = {}
-- No arguments are needed; return the version-one empty success frame.
-- 不需要参数；返回版本一空成功帧。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- Fixed Hook, UI and cleanup exports have no external effects.
-- 固定 Hook、界面及清理导出没有外部副作用。
M.on_precheck, M.ui_action, M.shutdown, M.shutdown_hooks = M.dispatch, M.dispatch, M.dispatch, M.dispatch
-- args contains the exact original request and event; return success after one native event.
-- args 包含精确原请求及事件；在单个原生事件之后返回成功。
function M.on_lifecycle(args)
    assert(type(args.request_id) == 'string' and #args.request_id == 36)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='target-' .. args.event,revision=0})
    return '{"ok":true,"result":{}}'
end
return M
"#;

/// Post-staging package corruption is rejected before any extension publisher executes.
/// 暂存后的插件包损坏在任何扩展发布器执行前被拒绝。
/// No inputs or return value; exact disk bytes and the original published record remain inspectable.
/// 无输入与返回值；精确磁盘字节及原已发布记录保持可检查。
#[test]
fn formal_target_handoff_revalidates_physical_package_before_publication() {
    // Real lifecycle effects and complete selection precede this deliberate external corruption.
    // 真实生命周期副作用及完整选择先于此次刻意外部损坏。
    let (fixture, store, transaction) = published_enabled_target();
    // Preserve the authoritative closed target before changing immutable source bytes.
    // 改变不可变来源字节之前保留权威关闭目标。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".to_owned()]),
        )
        .expect("closed target");
    // Resolve the sole selected physical package from the staged descriptor, without candidate paths.
    // 从暂存描述符解析唯一已选物理插件包，不使用候选路径。
    let bundle = staged.plugins["hook-plugin"]
        .package_root
        .join("runtime/plugin.bundle.lua");
    std::fs::write(&bundle, "return {}\n").expect("corrupt exact selected Lua bytes");
    assert!(
        fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| {
                panic!("corrupted target must never invoke publisher")
            })
            .is_err()
    );
    assert!(Arc::ptr_eq(&staged, &fixture.manager.snapshot()));
    assert_eq!(
        std::fs::read_to_string(bundle).expect("unchanged external corruption"),
        "return {}\n"
    );
    assert_eq!(
        store
            .activation_request("hook-plugin", transaction.record().request_id())
            .expect("original readable")
            .expect("original retained")
            .phase(),
        crate::VulcanPluginActivationTransactionPhase::Published
    );
}

/// Execute real old disable, target probe, installation and enable before committing the exact target.
/// 提交精确目标之前执行真实旧停用、目标探测、安装及启用。
/// No inputs; returns the actual runtime, store and still-owned published original request.
/// 无输入；返回实际运行时、存储及仍被持有的已发布原请求。
fn published_enabled_target() -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
) {
    // All source bytes and original selected configuration are frozen by the existing production fixture.
    // 全部来源字节及原选定配置由既有生产夹具固定。
    let (fixture, store, mut transaction, drain) =
        callback_fixture_with_target(false, true, false, TARGET);
    assert!(
        fixture
            .manager
            .stage_activation_target_handoff(
                &transaction,
                &BTreeSet::new(),
                &BTreeSet::from(["hook-plugin".to_owned()])
            )
            .is_err(),
        "preparation does not grant target publication"
    );
    {
        // The same original borrowed view authorizes every real pre-publication operation.
        // 相同原借用视图授权每个真实发布前操作。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        executor().block_on(async {
            fixture
                .manager
                .disable_activation_predecessor(&candidate, &drain)
                .await
                .expect("old disable");
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect("target probe");
            fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .expect("target install");
            fixture
                .manager
                .enable_activation_candidate(&candidate)
                .await
                .expect("target enable");
            fixture.empty().await;
        });
    }
    store
        .commit_activation_transaction(&mut transaction)
        .expect("complete target selected");
    (fixture, store, transaction)
}

/// Preparation preserves old business, while ordinary lifecycle cannot bypass the same original request.
/// 准备保留旧业务，而普通生命周期不能绕过相同原请求。
/// No inputs or return value; the real fixed Hook survives preparation without opening the private target.
/// 无输入与返回值；真实固定 Hook 跨准备存活，不开放私有目标。
#[test]
fn formal_target_handoff_preparation_preserves_old_business() {
    // The managed predecessor is already enabled and has a valid declared Hook.
    // 受管前驱已经启用，并具有有效声明 Hook。
    let (fixture, store) = finalizer_fixture();
    fixture.events.release();
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_plugin_runtime("hook-plugin")
                .await
                .expect("old real Hook")
        );
        // Original preparation remains independent from this already-live old instance.
        // 原准备保持与此已经存活的旧实例独立。
        let mut transaction = prepare_configured(
            &fixture,
            &store,
            VulcanPluginConfigurationSelection::NotDeclared {},
        );
        // Discovery identity belongs to the old selected package throughout preparation.
        // 准备全程，发现身份属于旧已选插件包。
        let original = fixture.manager.snapshot();
        // Select the sole declared plugin Hook by semantic ownership rather than an evolving position.
        // 按语义归属而非会变化的位置选择唯一声明插件 Hook。
        let hook = original
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("old declared Hook");
        fixture
            .manager
            .ensure_current_hook_generation(hook)
            .expect("old business admission");
        assert!(
            fixture
                .manager
                .prewarm_plugin_runtime("hook-plugin")
                .await
                .expect("reuse original Hook")
        );
        assert!(
            fixture.manager.begin_draining("hook-plugin").is_err(),
            "ordinary lifecycle must retain original gate"
        );
        assert!(Arc::ptr_eq(&original, &fixture.manager.snapshot()));
        store
            .abort_activation_transaction(&mut transaction)
            .expect("unused private target aborted");
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("ordinary drain after original abort");
        retire_original_hook(&fixture).await;
    });
}

/// Exact staging and publication keep business closed until durable completion; failed and stale publishers cannot open it.
/// 精确暂存及发布保持业务关闭直至持久完成；失败及陈旧发布器不能开放业务。
/// No inputs or return value; the final real fixed Hook initializes from the committed target configuration.
/// 无输入与返回值；最终真实固定 Hook 从已提交目标配置初始化。
#[test]
fn formal_target_handoff_publication_requires_exact_closed_original() {
    // The complete old and target effects were executed by actual core VMs before selection commit.
    // 选择提交前，实际核心 VM 已执行完整旧及目标副作用。
    let (fixture, store, mut transaction) = published_enabled_target();
    // These exact sets represent the production writer's explicit preserved enabled policy.
    // 这些精确集合表示生产写入器明确保留的启用策略。
    let enabled = BTreeSet::from(["hook-plugin".to_owned()]);
    assert!(
        fixture
            .manager
            .stage_activation_target_handoff(&transaction, &enabled, &BTreeSet::new())
            .is_err(),
        "wrong policy is rejected before staging"
    );
    // The final route identity is issued once while all business maps remain closed.
    // 最终路由身份仅签发一次，此时全部业务表保持关闭。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(&transaction, &BTreeSet::new(), &enabled)
        .expect("closed target staging");
    assert_eq!(
        staged.plugins["hook-plugin"].runtime_state,
        crate::VulcanPluginRuntimeState::Enabling
    );
    assert!(staged.enabled_hooks.is_empty());
    assert!(staged.enabled_tools.is_empty());
    assert!(staged.enabled_mcp_servers.is_empty());
    assert!(staged.enabled_skills.is_empty());
    assert!(staged.enabled_agents.is_empty());
    assert!(staged.prompt_contributions.is_empty());
    assert!(Arc::ptr_eq(
        &staged,
        &fixture
            .manager
            .stage_activation_target_handoff(&transaction, &BTreeSet::new(), &enabled)
            .expect("idempotent exact staging")
    ));
    // A copied catalog has identical fields but is not the authoritative current staged pointer.
    // 复制目录具有相同字段，但不是权威当前暂存指针。
    let copied = Arc::new((*staged).clone());
    assert!(
        fixture
            .manager
            .publish_activation_target_handoff(&transaction, &copied, |_| {
                panic!("stale publisher must never run")
            })
            .is_err()
    );
    assert!(
        fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| Err(
                "intentional complete-extension preparation failure".to_owned()
            ))
            .is_err()
    );
    assert!(Arc::ptr_eq(&staged, &fixture.manager.snapshot()));
    assert_eq!(
        transaction.record().phase(),
        crate::VulcanPluginActivationTransactionPhase::Published
    );
    // Capture the exact publisher output, without treating this runtime-only callback as a full service test.
    // 捕获精确发布器输出，不将此仅运行时回调当作完整服务测试。
    let mut published = None;
    // Registration exclusion spans the synchronous publisher and actual manager replacement.
    // 注册互斥跨越同步发布器及实际管理器替换。
    let final_catalog = fixture
        .manager
        .publish_activation_target_handoff(&transaction, &staged, |catalog| {
            assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
            // Host descriptor validation sees Enabled while the original disk authority still denies Hook execution.
            // 宿主描述符校验看到 Enabled 时，原磁盘权威仍拒绝 Hook 执行。
            let target_hook = catalog
                .enabled_hooks
                .values()
                .find(|hook| hook.plugin_id == "hook-plugin")
                .expect("provisional target Hook descriptor");
            assert_eq!(
                catalog.plugins["hook-plugin"].runtime_state,
                crate::VulcanPluginRuntimeState::Enabled
            );
            assert!(
                fixture
                    .manager
                    .ensure_current_hook_generation(target_hook)
                    .expect_err("publisher cannot admit pending target Hook")
                    .message
                    .contains("route handoff")
            );
            assert_eq!(
                catalog.plugins["hook-plugin"].generation,
                staged.plugins["hook-plugin"].generation
            );
            published = Some(catalog);
            Ok(())
        })
        .expect("exact complete-extension callback");
    assert!(Arc::ptr_eq(
        published.as_ref().expect("publisher output"),
        &final_catalog
    ));
    assert!(Arc::ptr_eq(&final_catalog, &fixture.manager.snapshot()));
    // Runtime Enabled does not authorize business while this exact durable request remains unfinished.
    // 此精确持久请求未完成时，运行时 Enabled 不授权业务。
    let hook = final_catalog
        .enabled_hooks
        .values()
        .find(|hook| hook.plugin_id == "hook-plugin")
        .expect("target Hook surface");
    assert!(
        fixture
            .manager
            .ensure_current_hook_generation(hook)
            .expect_err("pending target business")
            .message
            .contains("route handoff")
    );
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_plugin_runtime("hook-plugin")
                .await
                .expect_err("ordinary target prewarm remains fenced")
                .message
                .contains("route handoff")
        );
    });
    store
        .complete_activation_transaction(&mut transaction)
        .expect("original handoff completed");
    fixture
        .manager
        .ensure_current_hook_generation(hook)
        .expect("completed target business");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_plugin_runtime("hook-plugin")
                .await
                .expect("real target configuration initialization")
        );
        fixture
            .manager
            .begin_draining("hook-plugin")
            .expect("normal target shutdown");
        retire_original_hook(&fixture).await;
    });
}
