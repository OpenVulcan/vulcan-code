//! Original global controls survive access replacement without retaining execution leases.
//! 原全局控制在访问替换后继续存在，不保留执行租约。

use super::owner_drain_tests::{OBSERVE, child_holds, child_released, server};
use super::*;

/// Start a real global child that owns witness; return its original control after complete discovery.
/// 启动拥有 witness 的真实全局子进程；完整发现后返回其原控制。
/// Plugin selects the immutable declaration; witness remains exclusively locked until actual child exit.
/// plugin 选择不可变声明；实际子进程退出之前 witness 保持排他锁定。
async fn original(plugin: &str, witness: &tempfile::NamedTempFile) -> McpRuntimeHandle {
    // The fixture process, rather than this observer, owns the actual exclusive file lease.
    // 实际排他文件租约由夹具进程拥有，不由此观察者拥有。
    let runtime =
        super::super::handle::bootstrap_mcp_runtime(vec![server(plugin, witness.path(), true)])
            .await
            .expect("create original global");
    runtime
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual Ready");
    child_holds(witness).await;
    runtime
}

/// Wrap the actual original using its canonical declared source partition; return a published access.
/// 按规范声明来源分区包装实际原实例；返回已发布访问。
/// Plugin selects only routing; runtime remains the same actual control and owns no execution lease itself.
/// plugin 仅选择路由；runtime 保持同一实际控制，本身不拥有执行租约。
fn published(plugin: &str, runtime: &McpRuntimeHandle) -> McpRuntimeAccess {
    McpRuntimeAccess::new(
        runtime.overview(),
        BTreeMap::from([(
            McpServerSource::Plugin {
                plugin_id: plugin.into(),
                generation: 5,
            }
            .partition_key(),
            runtime.clone(),
        )]),
    )
}

/// Verify a still-published original accepts an actual remote call while its child retains witness.
/// 验证仍发布的原实例接受实际远端调用，同时子进程保留 witness。
/// Plugin selects the fixture tool; success proves the business gate was not closed by observation.
/// plugin 选择夹具工具；成功证明观察没有关闭业务门。
async fn remains_open(plugin: &str, runtime: &McpRuntimeHandle, witness: &tempfile::NamedTempFile) {
    assert!(
        !runtime.shutdown_requested(),
        "observation must not request shutdown"
    );
    // Invoke the actual child so an open flag alone cannot masquerade as retained business access.
    // 调用实际子进程，避免仅开放标志伪装成仍有业务访问。
    let result = runtime
        .call_tool_by_id(&format!("{plugin}:owner/probe"), serde_json::json!({}))
        .await
        .expect("old live child still accepts business");
    // The fixture returns plain text; the actual Machine projection returns that exact JSON string scalar.
    // 夹具返回纯文本；实际 Machine 投影返回其精确 JSON 字符串标量。
    assert_eq!(
        result,
        serde_json::Value::String(String::from("ready")),
        "expected exact machine projection of the actual fixture probe reply"
    );
    child_holds(witness).await;
}

/// Initial controls remain queryable after the last execution lease drops and the real child exits.
/// 最后执行租约析构及真实子进程退出后，初始控制仍可查询。
#[tokio::test]
async fn original_inventory_initial_registration_survives_actual_lease_drop() {
    // Real native ownership proves that control retention does not keep a child or its file alive.
    // 真实原生所有权证明保留控制不会保活子进程或其文件。
    let witness = tempfile::NamedTempFile::new().expect("initial witness");
    // This control is the same original constructed by the runtime bootstrap.
    // 此控制就是运行时引导构造的同一原实例。
    let runtime = original("initial", &witness).await;
    // The admission registry remains retained independently of the execution access.
    // 准入注册表独立于执行访问保持保留。
    let access = published("initial", &runtime);
    // Cloning this observation registry does not clone any McpRuntimePartitionLease.
    // 克隆此观察注册表不会克隆任何 McpRuntimePartitionLease。
    let admission = Arc::clone(&access.blocked_owner_plugins);
    assert!(
        admission.lock().expect("admission").managed_globals["initial"]
            .iter()
            .any(|observed| observed.same_instance(&runtime))
    );
    drop(access);
    runtime
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("original real release");
    child_released(&witness);
    assert!(
        admission.lock().expect("admission").managed_globals["initial"]
            .iter()
            .any(|observed| observed.same_instance(&runtime) && observed.is_fully_stopped())
    );
}

/// Immutable typed source determines initial registration even when a caller supplies an unrelated map label.
/// 即使调用方提供无关映射标签，初始登记仍由不可变类型来源决定。
#[tokio::test]
async fn original_inventory_initial_registration_uses_actual_typed_source() {
    // The child has genuine plugin configuration while its map label deliberately looks user-owned.
    // 子进程具有真实插件配置，而映射标签刻意表现为用户所有。
    let witness = tempfile::NamedTempFile::new().expect("typed source witness");
    // The actual registry, not its externally supplied map label, declares ownership.
    // 归属由实际注册表声明，不由外部提供的映射标签声明。
    let runtime = original("declared", &witness).await;
    // A genuinely user-configured child remains outside plugin observation despite a plugin-looking map label.
    // 即使映射标签表现为插件，真正用户配置的子进程仍不进入插件观察。
    let user_witness = tempfile::NamedTempFile::new().expect("user source witness");
    // Change the typed source before creating this separate actual runtime, never after capture.
    // 创建此独立实际运行时之前更改类型来源，绝不在捕获后更改。
    let mut user_configuration = server("user-declared", user_witness.path(), true);
    user_configuration.source = McpServerSource::UserConfigured;
    // This actual original has only the immutable user declaration.
    // 此实际原实例仅具有不可变用户声明。
    let user_runtime = super::super::handle::bootstrap_mcp_runtime(vec![user_configuration])
        .await
        .expect("actual user runtime");
    user_runtime
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual user Ready");
    child_holds(&user_witness).await;
    // The infallible constructor must use the immutable typed declaration without parsing labels.
    // 不可失败构造器必须使用不可变类型声明，不解析标签。
    let access = McpRuntimeAccess::new(
        runtime.overview(),
        BTreeMap::from([
            (String::from("user"), runtime.clone()),
            (String::from("plugin:map-only:5"), user_runtime.clone()),
        ]),
    );
    assert_eq!(
        access
            .blocked_owner_plugins
            .lock()
            .expect("admission")
            .managed_globals
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![String::from("declared")]
    );
    drop(access);
    runtime
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("typed original release");
    child_released(&witness);
    user_runtime
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("actual user release");
    child_released(&user_witness);
}

/// Removing an access generation records the real original without closing its still-retained business lease.
/// 移除访问代次会记录真实原实例，不关闭其仍保留的业务租约。
#[tokio::test]
async fn original_inventory_remove_preserves_open_original_until_actual_drop() {
    // One actual child owns the lock throughout construction of the successor access.
    // 构造后继访问期间，一个实际子进程持续拥有锁。
    let witness = tempfile::NamedTempFile::new().expect("remove witness");
    // This original is still published by old access after successor removal.
    // 后继移除后，此原实例仍由旧访问发布。
    let runtime = original("removed", &witness).await;
    // Only this access owns the old execution lease.
    // 仅此访问拥有旧执行租约。
    let current = published("removed", &runtime);
    // The successor has no old execution lease and retains only shared observation storage.
    // 后继没有旧执行租约，仅保留共享观察存储。
    let next = current.without_plugin_partition("removed");
    remains_open("removed", &runtime, &witness).await;
    assert!(
        next.retired_plugin_partitions.lock().expect("retired")["removed"]
            .iter()
            .any(|observed| observed.same_instance(&runtime))
    );
    drop(current);
    runtime
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("removed original release");
    child_released(&witness);
    assert!(
        next.retired_plugin_partitions.lock().expect("retired")["removed"]
            .iter()
            .any(|observed| observed.same_instance(&runtime) && observed.is_fully_stopped())
    );
}

/// Canceling an ordinary replacement publication preserves old business and releases the distinct new child.
/// 取消普通替换发布会保留旧业务，并释放不同的新子进程。
#[tokio::test]
async fn original_inventory_replace_cancellation_preserves_original_controls() {
    // Separate actual child-owned locks distinguish original and replacement resources.
    // 独立实际子进程所拥有的锁区分原实例与替换资源。
    let old_witness = tempfile::NamedTempFile::new().expect("old replacement witness");
    // The new actual process receives its own independent witness.
    // 新实际进程取得其独立见证。
    let new_witness = tempfile::NamedTempFile::new().expect("new replacement witness");
    // The original lease stays published until the caller drops current.
    // 调用方析构 current 之前，原租约保持发布。
    let old = original("replaced", &old_witness).await;
    // This access retains old execution, separately from shared controls.
    // 此访问保留旧执行，与共享控制分离。
    let current = published("replaced", &old);
    // Use the existing unique unpublished start registration, rather than another inventory.
    // 使用既有唯一未发布启动登记，不创建另一清单。
    let replacement = current
        .start_preparing_plugin_partition(
            "replaced",
            vec![server("replaced", new_witness.path(), true)],
        )
        .expect("registered replacement");
    replacement
        .wait_until_ready(OBSERVE)
        .await
        .expect("new actual Ready");
    child_holds(&new_witness).await;
    current
        .finish_preparing_plugin_partition("replaced", &replacement)
        .expect("same original registration");
    // Publication is reviewable but canceled by dropping this prospective generation.
    // 发布结果可供检查，但通过析构此次拟发布代次取消。
    let next = current.with_replaced_plugin_partition(
        "replaced",
        5,
        replacement.overview(),
        replacement.clone(),
    );
    remains_open("replaced", &old, &old_witness).await;
    assert!(
        next.retired_plugin_partitions.lock().expect("retired")["replaced"]
            .iter()
            .any(|observed| observed.same_instance(&old))
    );
    drop(next);
    replacement
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("canceled new publication releases child");
    child_released(&new_witness);
    assert!(
        current
            .retired_plugin_partitions
            .lock()
            .expect("retired after cancellation")["replaced"]
            .iter()
            .any(|observed| observed.same_instance(&old))
    );
    remains_open("replaced", &old, &old_witness).await;
    drop(current);
    old.wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("original replacement release");
    child_released(&old_witness);
}

/// Canceling prepared access drops its same actual preparing lease while preserving the old published process.
/// 取消准备访问会析构其同一实际准备租约，同时保留旧已发布进程。
/// This tests the lease constructor boundary, not a forged core target authority or a completed target handoff.
/// 此测试验证租约构造边界，不伪造 Core 目标权威或已完成目标交接。
#[tokio::test]
async fn original_inventory_prepared_access_cancellation_preserves_old_control() {
    // Distinct child locks prove that preparing and old execution are independently owned.
    // 不同子进程锁证明准备执行与旧执行分别拥有。
    let old_witness = tempfile::NamedTempFile::new().expect("old prepared witness");
    // This witness belongs only to the actual preparing process.
    // 此见证仅属于实际准备进程。
    let prepared_witness = tempfile::NamedTempFile::new().expect("prepared witness");
    // The old published process remains callable until its own lease drops.
    // 旧已发布进程在自身租约析构之前保持可调用。
    let old = original("prepared", &old_witness).await;
    // This is the original access lineage shared with the preparing start.
    // 此为与准备启动共享的原访问谱系。
    let current = published("prepared", &old);
    // The source-neutral resources deliberately claim no core package or native host authority.
    // 中立来源资源明确不声称 Core 插件包或原生宿主权威。
    let (resources, release) = vulcan_mcp_runtime::McpTransportResources::new(());
    // Use the real unique preparing start so unpublished observation remains unchanged.
    // 使用真实唯一准备启动，保持未发布观察路径不变。
    let preparing = current
        .start_preparing_plugin_partition_with_resources(
            "prepared",
            vec![server("prepared", prepared_witness.path(), true)],
            resources,
            true,
        )
        .expect("registered preparing start");
    preparing
        .wait_until_ready(OBSERVE)
        .await
        .expect("preparing actual Ready");
    child_holds(&prepared_witness).await;
    // This actual lease is transferred unchanged into the prospective publication access.
    // 此实际租约原样移交拟发布访问。
    let lease = Arc::new(McpRuntimePartitionLease::new(preparing.clone()));
    // The prospective access removes old routing but must retain old control observation.
    // 拟发布访问移除旧路由，但必须保留旧控制观察。
    let next = current.with_prepared_plugin_lease("prepared", 5, Arc::clone(&lease));
    assert!(Arc::ptr_eq(
        &next.live_partitions["plugin:prepared:5"],
        &lease
    ));
    remains_open("prepared", &old, &old_witness).await;
    assert!(
        next.retired_plugin_partitions.lock().expect("retired")["prepared"]
            .iter()
            .any(|observed| observed.same_instance(&old))
    );
    drop(next);
    drop(lease);
    preparing
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("actual preparing lease releases");
    assert!(
        release.is_released(),
        "observation must not retain native resources"
    );
    child_released(&prepared_witness);
    assert!(
        current
            .retired_plugin_partitions
            .lock()
            .expect("retired after prepared cancellation")["prepared"]
            .iter()
            .any(|observed| observed.same_instance(&old))
    );
    remains_open("prepared", &old, &old_witness).await;
    drop(current);
    old.wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("old preparing access release");
    child_released(&old_witness);
}

/// Repeated observations deduplicate the same control while retaining genuinely different same-generation originals.
/// 重复观察对同一控制去重，同时保留真正不同的同代次原实例。
#[tokio::test]
async fn original_inventory_deduplicates_actual_instance_not_generation() {
    // These files distinguish actual processes even though all route labels use generation five.
    // 即使全部路由标签使用代次五，这些文件仍区分实际进程。
    let first_witness = tempfile::NamedTempFile::new().expect("first identity witness");
    // The second process has its own real file ownership.
    // 第二个进程拥有自身真实文件所有权。
    let second_witness = tempfile::NamedTempFile::new().expect("second identity witness");
    // The first actual original is published independently.
    // 第一个实际原实例独立发布。
    let first = original("identity", &first_witness).await;
    // The second is a distinct actual runtime of the same declared generation.
    // 第二个是同一声明代次的不同实际运行时。
    let second = original("identity", &second_witness).await;
    assert!(!first.same_instance(&second));
    // This access owns the first execution lease.
    // 此访问拥有第一个执行租约。
    let current = published("identity", &first);
    drop(current.without_plugin_partition("identity"));
    drop(current.without_plugin_partition("identity"));
    assert_eq!(
        current.retired_plugin_partitions.lock().expect("retired")["identity"].len(),
        1
    );
    // Direct constructor transfer uses the same generation but a different actual process control.
    // 直接构造转交使用相同代次，但实际进程控制不同。
    let replaced =
        current.with_replaced_plugin_partition("identity", 5, second.overview(), second.clone());
    // Removing the distinct original must add it without conflating either process with the generation label.
    // 移除不同原实例必须将其加入，不将任何进程与代次标签混同。
    let removed = replaced.without_plugin_partition("identity");
    {
        // Hold observation storage only for comparison, never across a native await or lease drop.
        // 仅为比较持有观察存储，绝不跨原生等待或租约析构。
        let registry = removed.retired_plugin_partitions.lock().expect("retired");
        assert_eq!(registry["identity"].len(), 2);
        assert!(
            registry["identity"]
                .iter()
                .any(|observed| observed.same_instance(&first))
        );
        assert!(
            registry["identity"]
                .iter()
                .any(|observed| observed.same_instance(&second))
        );
    }
    remains_open("identity", &first, &first_witness).await;
    remains_open("identity", &second, &second_witness).await;
    drop(current);
    first
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("first actual release");
    child_released(&first_witness);
    drop(replaced);
    second
        .wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("second actual release");
    child_released(&second_witness);
}
