//! Actual published children bind source identity before launch and remain drainable after observer cancellation.
//! 实际已发布子进程在启动前绑定来源身份，并在观察者取消后仍可排空。

use super::*;
use crate::mcp::plugin::tests::credential_binding::candidates::runtime::{
    OBSERVE,
    published_fixture::{PublishedFixture, published_fixture},
    recorded_starts,
};
use futures_util::FutureExt;

mod bootstrap;
mod publication;

/// Prove one actual child consumed original values and the same pure source, while another equal source is rejected.
/// 证明单个实际子进程消费原值及同一纯来源，同时拒绝另一等值来源。
/// No inputs or return value; actual protocol readiness, startup PID, original drain and environment repair are evidence.
/// 无输入及返回值；实际协议就绪、启动进程标识、原排空及环境修复构成证据。
#[tokio::test]
async fn published_native_original_source_and_profile_do_not_pin_retired_environment() {
    // The shared producer has genuinely aborted its unused candidate before any published startup.
    // 任何已发布启动之前，共享生产者已真实中止其未使用候选。
    let PublishedFixture {
        manager,
        configs,
        authority,
        marker,
        environment,
        _temp,
        ..
    } = published_fixture("ready");
    // This actual empty access carries the original observation and admission lineage.
    // 此实际空访问携带原观察及入场谱系。
    let access = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual empty access");
    // The control's real resources and input source are consumed together before its first task.
    // 首个任务之前，控制的真实资源及输入来源被共同消费。
    let runtime = PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority)
        .expect("actual proven startup");
    // Observers keep only original diagnostics, cleanup receipts and pure source values.
    // 观察者仅保留原诊断、清理回执及纯来源值。
    let observation = runtime.observer();
    // Retain this source through true native retirement and subsequent real environment repair.
    // 跨真实原生退役及后续实际环境修复保留此来源。
    let profile = observation.profile();
    observation
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual child ready");
    observation
        .verify_source("bound-plugin:local", &profile)
        .expect("same original actual launch source");
    // A separately prepared equal full selection remains a different actual source object.
    // 独立准备的等值完整选择仍是不同实际来源对象。
    let other = manager
        .prepare_published_mcp_resources(&manager.snapshot(), "bound-plugin", &authority)
        .expect("separate actual prepared source")
        .host_profile(&authority)
        .expect("separate pure profile");
    assert!(
        observation
            .verify_source("bound-plugin:local", &other)
            .expect_err("equal values must not certify original source")
            .contains("DifferentPluginSource")
    );
    // The real child wrote one complete startup record before its initialization completed.
    // 真实子进程在初始化完成前写入单个完整启动记录。
    let starts = recorded_starts(&marker).await;
    assert_eq!(
        starts.len(),
        1,
        "the expected single actual startup may have changed"
    );
    // Locate the unique completed producer record rather than a mutable registry index.
    // 定位唯一完成的生产者记录，而非可变注册表下标。
    let actual = starts.first().expect("actual original child record");
    assert!(actual["pid"].as_u64().expect("actual child PID") > 0);
    assert_eq!(actual["label"], "public-value");
    assert_eq!(actual["token"], "first-token");
    // The access must find and stop the original even though it has never published a live partition.
    // 即使尚未发布存活分区，访问也必须找到并停止原实例。
    access
        .begin_plugin_mcp_drain("bound-plugin")
        .expect("actual registered original drain");
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("original native physical release");
    access
        .wait_plugin_mcp_drain("bound-plugin", OBSERVE)
        .await
        .expect("actual original tables retired");
    assert!(
        observation
            .verify_source("bound-plugin:local", &profile)
            .is_err()
    );
    manager
        .begin_draining("bound-plugin")
        .expect("actual disabled transition");
    manager
        .finish_lifecycle_transition(
            "bound-plugin",
            &BTreeSet::from([String::from("bound-plugin")]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("actual disabled source");
    assert!(
        environment.starts_with(
            _temp
                .path()
                .canonicalize()
                .expect("literal isolated fixture root")
        )
    );
    // Remove only this fixture's declared binary after real child retirement, requiring production repair to acquire its actual cleanup File.
    // 真实子进程退役后，仅移除此夹具声明二进制，要求生产修复取得其实际清理 File。
    std::fs::remove_file(environment.join("bin/server.bin"))
        .expect("remove this isolated retired binary");
    assert_eq!(
        manager
            .prepare_plugin_environment("bound-plugin")
            .expect("profile and control must not retain environment File"),
        environment
    );
    assert!(profile.same_source(&observation.profile()));
    assert_eq!(
        recorded_starts(&marker).await.len(),
        1,
        "environment repair must not spawn a replacement child"
    );
}

/// Real failed and parked starts preserve original registration through cancelled observation and actual cleanup.
/// 真实失败及停驻启动跨取消观察和实际清理保留原登记。
/// No inputs or return value; actual child records and same-lineage drain prove no automatic replay.
/// 无输入及返回值；实际子进程记录及同谱系排空证明没有自动重播。
#[tokio::test]
async fn published_native_failed_and_cancelled_observation_drain_original_once() {
    for mode in ["fail", "stall"] {
        // Each actual source has independent immutable selection and physical original ownership.
        // 每个实际来源具有独立不可变选择及物理原所有权。
        let PublishedFixture {
            manager,
            configs,
            authority,
            marker,
            _temp,
            ..
        } = published_fixture(mode);
        // The actual observation registry outlives cancellation of a foreground readiness future.
        // 实际观察注册表跨前台就绪 future 的取消存续。
        let access = bootstrap_mcp_runtime_access(Vec::new())
            .await
            .expect("actual empty access");
        // Start consumes the sole sealed definitions exactly once.
        // 启动恰好消费一次唯一密封定义。
        let runtime = PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority)
            .expect("actual original attempt");
        // Keeping this clone must never keep the original package environment locked after retirement.
        // 保留此克隆绝不得在退役后使原插件包环境持续加锁。
        let observation = runtime.observer();
        // Wait for the genuine native producer before cancelling or interpreting an initialization failure.
        // 取消或解释初始化失败之前，等待真实原生生产者。
        let starts = recorded_starts(&marker).await;
        assert_eq!(
            starts.len(),
            1,
            "the expected original child record may have changed"
        );
        if mode == "fail" {
            // Preserve the actual diagnostic so a failed expectation identifies the real protocol outcome.
            // 保留实际诊断，使预期失败指出真实协议结果。
            let failure = observation
                .wait_until_ready(OBSERVE)
                .await
                .expect_err("real initialize failure");
            // The real sanitizer hides the declared mode inside "failure"; require its exact protocol code and safe prefix.
            // 真实脱敏器隐藏 failure 中的声明模式；要求其精确协议码及安全前缀。
            assert!(
                failure
                    .contains("JSON-RPC error: -32603: Error: Declared candidate initialization"),
                "original declared protocol failure was not observed: {failure}"
            );
            assert!(
                observation.server_states().iter().any(|state| {
                    state.id == "bound-plugin:local"
                        && state.status == McpRuntimeServerStatus::Failed
                        && state.retry_count == 1
                }),
                "the expected failed original and single attempt may have changed"
            );
        } else {
            // Poll then drop the real pending observer; this is cancellation without owner confirmation or replacement.
            // 轮询后丢弃真实待定观察者；这是不确认所有者且不替换实例的取消。
            let mut waiter = Box::pin(observation.wait_until_ready(OBSERVE));
            assert!(waiter.as_mut().now_or_never().is_none());
            drop(waiter);
        }
        assert!(
            observation
                .verify_source("bound-plugin:local", &observation.profile())
                .is_err()
        );
        access
            .begin_plugin_mcp_drain("bound-plugin")
            .expect("original drain after observation loss");
        // Lost foreground control still leaves the actual original in the same observation tables.
        // 前台控制丢失后，实际原实例仍保留在相同观察表内。
        drop(runtime);
        access
            .wait_plugin_mcp_drain("bound-plugin", OBSERVE)
            .await
            .expect("registered original physical cleanup");
        observation
            .wait_until_released(OBSERVE)
            .await
            .expect("original independent cleanup observation");
        assert_eq!(
            recorded_starts(&marker).await.len(),
            1,
            "an original failed or cancelled preparation must not replay"
        );
    }
}

/// A real legacy connected transport stays unproven even when passed the exact full current source values.
/// 即使传入精确完整当前来源值，真实旧已连接传输也保持未证明。
/// No inputs or return value; actual child readiness and strict source rejection prevent retrospective certification.
/// 无输入及返回值；实际子进程就绪及严格来源拒绝阻止追认认证。
#[tokio::test]
async fn published_native_raw_connected_original_cannot_be_retroactively_proven() {
    // Keep the physical owner alive for the deliberately raw fixture launch, without giving that producer a profile.
    // 为刻意原始夹具启动保留物理所有者，不向该生产者提供画像。
    let PublishedFixture {
        manager,
        configs,
        authority,
        marker,
        _temp,
        ..
    } = published_fixture("ready");
    // This profile is genuinely issued from the same full publication but was not bound to the raw producer.
    // 此画像从同一完整发布真实签发，但未绑定至原始生产者。
    let profile = manager
        .prepare_published_mcp_resources(&manager.snapshot(), "bound-plugin", &authority)
        .expect("actual full source values")
        .host_profile(&authority)
        .expect("source-only profile");
    // The existing unproven bootstrap deliberately consumes only detached configurations.
    // 既有未证明引导刻意仅消费独立配置。
    let raw = bootstrap_mcp_runtime(configs.servers().to_vec())
        .await
        .expect("actual raw startup");
    // The existing startup guard retains only control and requests cleanup on an assertion failure.
    // 既有启动守卫仅保留控制，并在断言失败时请求清理。
    let mut cleanup = McpRuntimeStartupGuard::new();
    cleanup.track(&raw);
    raw.wait_until_ready(OBSERVE)
        .await
        .expect("actual raw child connected");
    assert_eq!(
        raw.with_connected_published_launch_inputs("bound-plugin:local", &profile, |_, _| ()),
        Err(crate::mcp::registry::McpLaunchInputCaptureError::UnprovenPluginSource)
    );
    assert_eq!(recorded_starts(&marker).await.len(), 1);
    raw.shutdown();
    raw.wait_until_drained(tokio::time::Instant::now() + OBSERVE)
        .await
        .expect("actual raw child retirement");
    drop(configs);
}

/// Original ordinary startup rejects foreign registration and real plugin drain before any actual child dispatch.
/// 原普通启动在任何实际子进程派发之前拒绝外来注册及真实插件排空。
/// No inputs or return value; real source preparation and absence of native producer work are checked.
/// 无输入及返回值；检查真实来源准备及没有原生生产者工作。
#[tokio::test]
async fn published_native_first_dispatch_rejects_foreign_host_and_blocked_plugin() {
    for blocked in [false, true] {
        // Each rejected attempt owns a real full source and no existing child.
        // 每个被拒绝尝试拥有真实完整来源，且没有既有子进程。
        let PublishedFixture {
            manager,
            configs,
            authority,
            marker,
            _temp,
            ..
        } = published_fixture("ready");
        // This actual access fixes one admission lineage before its new owner can register.
        // 新所有者可能登记之前，此实际访问固定单个入场谱系。
        let access = bootstrap_mcp_runtime_access(Vec::new())
            .await
            .expect("actual empty access");
        if blocked {
            access
                .begin_plugin_mcp_drain("bound-plugin")
                .expect("actual plugin gate closure");
            assert!(
                PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority).is_err()
            );
        } else {
            // A genuine distinct registered issuer must not be treated as a clone of the original.
            // 真实不同已注册签发者不得被视为原签发者克隆。
            let foreign = manager.register_native_mcp_host_adapter();
            assert!(
                PublishedPluginMcpRuntime::start(&access, &manager, configs, &foreign).is_err()
            );
            access
                .begin_plugin_mcp_drain("bound-plugin")
                .expect("close empty original observation scope");
        }
        assert!(
            !marker.exists(),
            "rejected first dispatch produced an actual child"
        );
        access
            .wait_plugin_mcp_drain("bound-plugin", OBSERVE)
            .await
            .expect("no unregistered native work");
    }
}

/// Reject actual Enabling state and its stale sealed predecessor before any ordinary native startup.
/// 在任何普通原生启动之前拒绝实际 Enabling 状态及其过期密封前驱。
/// No inputs or return value; real lifecycle transitions and the unchanged persistent selection supply evidence.
/// 无输入及返回值；真实生命周期转换及未改变持久选择提供证据。
#[tokio::test]
async fn published_native_first_dispatch_rejects_actual_enabling_selection() {
    // The source has a genuine full publication, then enters the production enable transition.
    // 来源具有真实完整发布，随后进入生产启用转换。
    let PublishedFixture {
        manager,
        configs,
        authority,
        store,
        marker,
        _temp,
        ..
    } = published_fixture("ready");
    // This exact durable selection remains unchanged while the runtime state changes.
    // 运行时状态改变期间，此精确持久选择保持不变。
    let selected = store
        .activation("bound-plugin")
        .expect("actual durable source")
        .expect("full managed selection");
    manager
        .begin_draining("bound-plugin")
        .expect("actual enabled predecessor drain");
    manager
        .finish_lifecycle_transition(
            "bound-plugin",
            &BTreeSet::from([String::from("bound-plugin")]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("actual disabled predecessor");
    manager
        .begin_enabling("bound-plugin")
        .expect("actual enable transition");
    // The current immutable Arc is truly Enabling, rather than a manufactured copy or a stale descriptor.
    // 当前不可变 Arc 确为 Enabling，而非制造副本或过期描述符。
    let enabling = manager.snapshot();
    assert_eq!(
        enabling.plugins["bound-plugin"].runtime_state,
        vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabling,
        "the expected production transition may have changed"
    );
    assert_eq!(
        store
            .activation("bound-plugin")
            .expect("same durable source"),
        Some(selected)
    );
    assert!(
        manager
            .prepare_published_mcp_resources(&enabling, "bound-plugin", &authority)
            .err()
            .expect("ordinary preparation must reject actual Enabling")
            .to_string()
            .contains("Published MCP requires an enabled managed selection.")
    );
    // The previously prepared source is also rejected by its real dispatch fence without starting a child.
    // 先前准备来源也由其真实派发屏障拒绝，不启动子进程。
    let access = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("actual empty access");
    assert!(PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority).is_err());
    assert!(
        !marker.exists(),
        "Enabling unexpectedly started a native child"
    );
    access
        .begin_plugin_mcp_drain("bound-plugin")
        .expect("close empty original observation scope");
    access
        .wait_plugin_mcp_drain("bound-plugin", OBSERVE)
        .await
        .expect("no native work escaped Enabling rejection");
}
