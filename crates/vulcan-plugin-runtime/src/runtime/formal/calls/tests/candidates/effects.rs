//! Real candidate effects retain exact request evidence through failure, cancellation and compensation.
//! 真实候选副作用跨失败、取消及补偿保留精确请求证据。

use super::*;
use crate::{VulcanPluginActivationEffect as Effect, VulcanPluginEffectObservation as Observation};

/// Malformed effect evidence blocks query, publication and startup recovery without rewriting original bytes.
/// 损坏副作用证据阻止查询、发布及启动恢复，不重写原字节。
#[test]
fn formal_candidate_effects_invalid_evidence_fails_closed() {
    // A real completed probe supplies authoritative bytes before individual corruption cases.
    // 真实已完成探测在各损坏用例之前提供权威字节。
    let (fixture, store, mut transaction) = candidate_fixture(false, false, false);
    // Exact original identity determines the sole sidecar location.
    // 精确原身份决定唯一附属文件位置。
    let request = transaction.record().request_id();
    executor().block_on(async {
        // This invocation executes real Lua initialization once.
        // 此调用单次执行真实 Lua 初始化。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        fixture
            .manager
            .probe_activation_candidate(&candidate)
            .await
            .expect("actual probe");
    });
    // The path follows the declared storage format and stays in the fixture's temporary root.
    // 路径遵循声明存储格式，且留在夹具临时根中。
    let path = fixture
        .manager
        .user_plugin_root()
        .join(".vulcan-generations/transactions/hook-plugin")
        .join(format!("{request}.effects"));
    // Preserve the genuine bytes for independent fault injection and eventual explicit abandonment.
    // 保留真实字节，用于独立故障注入及最终明确放弃。
    let original = std::fs::read(&path).expect("written receipt");
    // Parse only the generated fixture record; no arbitrary user secrets are logged.
    // 仅解析生成夹具记录；不记录任意用户秘密。
    let record: serde_json::Value = serde_json::from_slice(&original).expect("typed receipt");
    for (field, value) in [
        ("schema_version", json!(u32::MAX)),
        ("plugin_id", json!("another-plugin")),
        ("request_id", json!(Uuid::new_v4().to_string())),
        ("target_revision", json!(u64::MAX)),
        ("probe", json!(null)),
        ("probe", json!("unknown")),
        ("unknown", json!("sensitive-marker")),
        ("enable_rollback", json!("started")),
    ] {
        // Alter exactly one field per case and demand no normalization or repair.
        // 每种情况精确改变一个字段，要求不规范化或修复。
        let mut changed = record.clone();
        changed[field] = value;
        // Byte preservation is checked after every rejecting operation.
        // 每次拒绝操作后检查字节保留。
        let encoded = serde_json::to_vec(&changed).expect("mutation");
        std::fs::write(&path, &encoded).expect("inject malformed receipt");
        assert!(store.activation_effects("hook-plugin", request).is_err());
        assert!(store.activation_recovery_records("hook-plugin").is_err());
        assert!(
            store
                .commit_activation_transaction(&mut transaction)
                .is_err()
        );
        executor().block_on(async {
            // Valid package inspection must not hide invalid original effect evidence.
            // 有效插件包检查不能隐藏无效原副作用证据。
            let candidate = store
                .inspect_activation_candidate(&transaction)
                .expect("unchanged package");
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("invalid evidence cannot authorize replay");
        });
        assert_eq!(std::fs::read(&path).expect("preserved evidence"), encoded);
    }
    // Duplicate keys are rejected by typed deserialization, without last-value-wins semantics.
    // 类型化反序列化拒绝重复键，不使用末值获胜语义。
    let duplicate = String::from_utf8(original.clone()).expect("JSON").replacen(
        "{",
        "{\"probe\":\"started\",",
        1,
    );
    std::fs::write(&path, duplicate).expect("duplicate field");
    assert!(store.activation_effects("hook-plugin", request).is_err());
    assert!(store.activation_recovery_records("hook-plugin").is_err());
    std::fs::write(&path, &original).expect("restore only fixture corruption");
    store
        .abort_activation_transaction(&mut transaction)
        .expect("terminal original");
    // A terminal original still owns its effects; orphan evidence cannot disappear from startup inventory.
    // 终态原请求仍拥有副作用；孤立证据不能从启动清单消失。
    let orphan = path.with_file_name(format!("{}.effects", Uuid::new_v4()));
    std::fs::write(&orphan, &original).expect("orphan fixture");
    assert!(store.activation_recovery_records("hook-plugin").is_err());
}

/// A timed-out candidate enable retains its actual native owner and cannot be silently retried.
/// 超时候选启用保留实际原生所有者，不能静默重试。
#[test]
fn formal_candidate_effects_enable_timeout_retains_native_authority() {
    // This enable remains blocked inside the actual native publisher until explicit release.
    // 此启用阻塞于实际原生发布器，直至明确释放。
    let (fixture, store, mut transaction) = effect_fixture(false, false);
    // Stable identity allows querying the same live call after the timeout.
    // 稳定身份允许超时后查询同一存活调用。
    let request = transaction.record().request_id();
    // Weak observation measures actual native ownership without extending its lifetime.
    // 弱观察测量实际原生所有权，不延长其寿命。
    let authority = executor().block_on(async {
        // Installation succeeds before the distinct enable attempt.
        // 安装在独立启用尝试之前成功。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        fixture
            .manager
            .install_activation_candidate(&candidate)
            .await
            .expect("install");
        fixture
            .manager
            .invoke_formal_candidate_operation(
                &candidate,
                CandidateOperation::Enable,
                Instant::now() + WAIT / 10,
            )
            .await
            .expect_err("enable observation expires");
        assert!(fixture.events.entered.load(Ordering::Acquire));
        assert_eq!(
            store
                .activation_effects("hook-plugin", request)
                .expect("evidence")
                .expect("enable")
                .observations(),
            [(Effect::Enable, Observation::ObservationFailed)]
        );
        fixture
            .manager
            .enable_activation_candidate(&candidate)
            .await
            .expect_err("no replay during native work");
        // Capture only a weak handle to the original executing authority.
        // 仅捕获原执行权威的弱句柄。
        let call = fixture.admitted().await;
        Arc::downgrade(call.activation.as_ref().expect("original enable authority"))
    });
    assert_eq!(
        store
            .commit_activation_transaction(&mut transaction)
            .expect_err("live enable owns request")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    fixture.events.release();
    executor().block_on(async {
        fixture
            .manager
            .drain_activation_candidate_calls(request)
            .await
            .expect("actual native drain");
        // Physical ownership, rather than the bounded observation returning, permits compensation.
        // 物理所有权退役允许补偿，而非有界观测返回本身。
        authority_retired(&authority).await;
        // Exact old invocation remains uncertain after timeout; compensation is explicit and separately recorded.
        // 超时后精确旧调用仍不确定；补偿为显式操作并独立记录。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("same original");
        fixture
            .manager
            .rollback_activation_candidate_enable(&candidate)
            .await
            .expect("explicit compensation");
    });
    assert_eq!(
        store
            .commit_activation_transaction(&mut transaction)
            .expect_err("compensated original")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    store
        .abort_activation_transaction(&mut transaction)
        .expect("explicit abandonment");
}

/// Build real lifecycle Lua with configurable enable and compensation failures.
/// 构造真实生命周期 Lua，可配置启用及补偿失败。
/// Return the original fixture, immutable store and prepared transaction; no production package is modified.
/// 返回原夹具、不可变存储及已准备事务；不修改生产插件包。
fn effect_fixture(
    fail_enable: bool,
    fail_rollback: bool,
) -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
) {
    // Native services and the original parent are shared with existing lifecycle acceptance fixtures.
    // 原生服务及原父级与既有生命周期验收夹具共享。
    let fixture = Fixture::new();
    // Move only this temporary package out of public discovery before immutable staging.
    // 不可变暂存前，仅将此临时插件包移出公开发现。
    let source = fixture._root.path().join("effect-source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("source parent")).expect("source parent");
    std::fs::rename(fixture._root.path().join("bundled/hook-plugin"), &source)
        .expect("move owned source");
    // Add explicit selected configuration before approval, preserving the declared lifecycle exports.
    // 批准前加入显式选定配置，保留声明生命周期导出。
    let manifest = std::fs::read_to_string(source.join("plugin.yaml")).expect("manifest")
        .replace("permissions: [process_binary, ui_host_context]", "permissions: [process_binary, ui_host_context, config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json");
    std::fs::write(source.join("plugin.yaml"), manifest).expect("manifest");
    std::fs::write(source.join("config.schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#).expect("schema");
    std::fs::write(
        source.join("config.defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("defaults");
    // Check the known source anchors so fixture evolution cannot silently stop exercising real callbacks.
    // 检查已知源码锚点，防止夹具演进静默停止验证真实回调。
    let bundle = std::fs::read_to_string(source.join("runtime/plugin.bundle.lua")).expect("bundle");
    // One declared function serves each host-selected lifecycle event.
    // 单个声明函数服务每个宿主选择的生命周期事件。
    let entry = "function M.on_lifecycle(args)";
    assert_eq!(
        bundle.matches(entry).count(),
        1,
        "lifecycle fixture entry may have changed"
    );
    // Disable the fixture's intentional rollback failure only when success is requested.
    // 仅请求成功时停用夹具刻意设置的回滚失败。
    let bundle = if fail_rollback {
        bundle
    } else {
        bundle.replace(
            "if args.event == 'enable_rollback' then return",
            "if false then return",
        )
    };
    // Every actual callback checks its original UUID and selected configuration through native services.
    // 每次实际回调通过原生服务检查原 UUID 及选定配置。
    let prefix = format!(
        r#"{entry}
    -- The original request identity belongs to this exact candidate invocation.
    -- 原请求身份属于此次精确候选调用。
    assert(type(args.request_id) == 'string')
    vulcan.host.call('vulcan.plugin.ui.invalidate', {{topic=args.event .. ':' .. args.request_id,revision=0}})
    -- Configuration is selected by the transaction, not by the currently published predecessor.
    -- 配置由事务选择，而非当前已发布前驱。
    local selected = vulcan.host.call('vulcan.plugin.config.get', {{}})
    assert(selected.label == 'selected-candidate')
    if {fail_enable} and args.event == 'enable' then return '{{"ok":false,"error":"candidate-enable-marker"}}' end
"#
    );
    std::fs::write(
        source.join("runtime/plugin.bundle.lua"),
        bundle.replace(entry, &prefix),
    )
    .expect("actual callbacks");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove source");
    // Publish a different predecessor configuration before creating the actual request.
    // 创建实际请求前发布不同前驱配置。
    let (store, transaction) = transaction_for(&fixture, &source, true);
    (fixture, store, transaction)
}

/// Successful probe, install and enable keep public routes unchanged and publish only on explicit commit.
/// 成功探测、安装及启用保持公开路由不变，仅在明确提交时发布。
#[test]
fn formal_candidate_effects_enable_uses_original_scope_and_rejects_replay() {
    // The fixture's enable native callback is released intentionally for this success path.
    // 为此成功路径有意释放夹具启用原生回调。
    let (fixture, store, mut transaction) = effect_fixture(false, false);
    fixture.events.release();
    // Preserve both original authorities while the real callbacks execute.
    // 在真实回调执行时保留两个原权威。
    let previous = store.activation("hook-plugin").expect("previous");
    // The public snapshot must not be replaced by unpublished readiness.
    // 公开快照不能被未发布就绪替换。
    let catalog = fixture.manager.snapshot();
    // UUID is observable inside Lua, independently of Rust's durable record.
    // UUID 可在 Lua 内观察，独立于 Rust 持久记录。
    let request = transaction.record().request_id();
    executor().block_on(async {
        // Candidate inspection pins the complete original selection.
        // 候选检查固定完整原选择。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate");
        fixture
            .manager
            .enable_activation_candidate(&candidate)
            .await
            .expect_err("declared install is a prerequisite");
        assert!(
            store
                .activation_effects("hook-plugin", request)
                .expect("unattempted")
                .is_none()
        );
        fixture
            .manager
            .probe_activation_candidate(&candidate)
            .await
            .expect("probe");
        fixture
            .manager
            .install_activation_candidate(&candidate)
            .await
            .expect("install");
        fixture
            .manager
            .enable_activation_candidate(&candidate)
            .await
            .expect("enable");
        for operation in [
            CandidateOperation::Probe,
            CandidateOperation::Install,
            CandidateOperation::Enable,
        ] {
            assert!(
                fixture
                    .manager
                    .invoke_formal_candidate_operation(&candidate, operation, Instant::now() + WAIT)
                    .await
                    .expect_err("no replay")
                    .message
                    .contains("already attempted")
            );
        }
    });
    assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
    assert_eq!(
        store
            .activation("hook-plugin")
            .expect("unchanged selection"),
        previous
    );
    // A fresh store reads durable success; process-local status is not authoritative.
    // 新存储读取持久成功；进程内状态不具权威。
    let reopened =
        VulcanPluginGenerationStore::open(fixture.manager.user_plugin_root()).expect("reopen");
    assert_eq!(
        reopened
            .activation_effects("hook-plugin", request)
            .expect("effects")
            .expect("receipt")
            .observations(),
        [
            (Effect::Probe, Observation::Succeeded),
            (Effect::Enable, Observation::Succeeded)
        ]
    );
    // Native envelope events occur exactly once and no implicit business finalizer executes.
    // 原生信封事件恰好发生一次，且不执行隐式业务关闭器。
    let events = fixture.events.values.lock().expect("events");
    for operation in ["install", "enable"] {
        assert_eq!(
            events
                .iter()
                .filter(|event| event.topic == format!("{operation}:{request}"))
                .count(),
            1
        );
    }
    assert!(!events.iter().any(|event| event.topic == "wrong-finalizer"));
    drop(events);
    store
        .commit_activation_transaction(&mut transaction)
        .expect("confirmed readiness");
}

/// Observed enable failures may be compensated once; any compensation permanently excludes publication.
/// 已观测启用失败可以单次补偿；任何补偿均永久禁止发布。
#[test]
fn formal_candidate_effects_compensation_never_reopens_publication() {
    for (fail_enable, fail_rollback) in [(false, false), (true, false), (false, true)] {
        // Each case has a separate original request and actual immutable package.
        // 每种情况具有独立原请求及实际不可变插件包。
        let (fixture, store, mut transaction) = effect_fixture(fail_enable, fail_rollback);
        fixture.events.release();
        // Capture the identity before external observation.
        // 外部观测前捕获身份。
        let request = transaction.record().request_id();
        executor().block_on(async {
            // No alternative candidate can satisfy these observations.
            // 替代候选不能满足这些观测。
            let candidate = store
                .inspect_activation_candidate(&transaction)
                .expect("candidate");
            fixture
                .manager
                .install_activation_candidate(&candidate)
                .await
                .expect("install");
            assert_eq!(
                fixture
                    .manager
                    .enable_activation_candidate(&candidate)
                    .await
                    .is_err(),
                fail_enable
            );
            assert_eq!(
                fixture
                    .manager
                    .rollback_activation_candidate_enable(&candidate)
                    .await
                    .is_err(),
                fail_rollback
            );
            fixture
                .manager
                .rollback_activation_candidate_enable(&candidate)
                .await
                .expect_err("compensation cannot replay");
            fixture
                .manager
                .enable_activation_candidate(&candidate)
                .await
                .expect_err("enable cannot replay");
        });
        assert_eq!(
            store
                .activation_effects("hook-plugin", request)
                .expect("effects")
                .expect("receipt")
                .observations(),
            [
                (
                    Effect::Enable,
                    if fail_enable {
                        Observation::ObservationFailed
                    } else {
                        Observation::Succeeded
                    }
                ),
                (
                    Effect::EnableRollback,
                    if fail_rollback {
                        Observation::ObservationFailed
                    } else {
                        Observation::Succeeded
                    }
                )
            ]
        );
        assert_eq!(
            store
                .commit_activation_transaction(&mut transaction)
                .expect_err("compensated candidate")
                .code,
            VulcanPluginStoreErrorCode::Conflict
        );
        // Explicit abandonment keeps evidence and does not claim external-state rollback.
        // 明确放弃保留证据，不声称外部状态回滚。
        store
            .abort_activation_transaction(&mut transaction)
            .expect("all real ownership retired");
        assert!(
            store
                .activation_effects("hook-plugin", request)
                .expect("historical")
                .is_some()
        );
    }
}

/// Probe cancellation and timeout retain original locks until native retirement, then still prohibit replay or commit.
/// 探测取消及超时保留原锁至原生退役，此后仍禁止重放或提交。
#[test]
fn formal_candidate_effects_probe_interruption_is_durable() {
    for cancel in [false, true] {
        // Initialization blocks in the real native publisher, independent of async polling.
        // 初始化阻塞于真实原生发布器，独立于异步轮询。
        let (fixture, store, mut transaction) = candidate_fixture(true, false, false);
        // Original identity remains available after observation ends.
        // 观测结束后原身份仍可用。
        let request = transaction.record().request_id();
        // Weak observation measures actual native ownership without extending its lifetime.
        // 弱观察测量实际原生所有权，不延长其寿命。
        let authority = executor().block_on(async {
            // The borrowed candidate and observer have separate actual native ownership.
            // 借用候选及观察者具有独立实际原生所有权。
            let candidate = store.inspect_activation_candidate(&transaction).expect("candidate");
            if cancel {
                // Drop the externally polled future only after a real callback has entered.
                // 仅在真实回调入场后销毁外部轮询 future。
                let observation = fixture.manager.probe_activation_candidate(&candidate);
                tokio::pin!(observation);
                tokio::select! {
                    result = &mut observation => panic!("blocked native probe returned early: {result:?}"),
                    _ = async {
                        tokio::time::timeout(WAIT, async {
                            while !fixture.events.entered.load(Ordering::Acquire) { tokio::task::yield_now().await; }
                        }).await.expect("native entry");
                    } => {}
                }
            } else {
                fixture.manager.invoke_formal_candidate_operation(&candidate, CandidateOperation::Probe, Instant::now() + WAIT / 10)
                    .await.expect_err("short observation deadline");
                assert!(fixture.events.entered.load(Ordering::Acquire));
            }
            assert_eq!(store.activation_effects("hook-plugin", request).expect("durable").expect("attempt").observations(),
                [(Effect::Probe, if cancel { Observation::Started } else { Observation::ObservationFailed })]);
            fixture.manager.probe_activation_candidate(&candidate).await.expect_err("no replay while actual call runs");
            // Preserve weak identity before releasing the controlled native worker.
            // 释放受控原生工作者前保留弱身份。
            let call = fixture.admitted().await;
            Arc::downgrade(call.activation.as_ref().expect("original candidate authority"))
        });
        assert_eq!(
            store
                .abort_activation_transaction(&mut transaction)
                .expect_err("live native owner")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        fixture.events.release();
        executor().block_on(async {
            fixture
                .manager
                .drain_activation_candidate_calls(request)
                .await
                .expect("drain exact original");
            // A bounded drain observation can return before physical retirement under load.
            // 高负载下，有界排空观测可能先于物理退役返回。
            authority_retired(&authority).await;
            // Query actual retained calls; success requires real resource retirement.
            // 查询实际保留调用；成功要求真实资源退役。
            assert!(
                fixture
                    .manager
                    .activation_candidate_calls(request)
                    .expect("calls")
                    .is_empty()
            );
        });
        assert_eq!(
            store
                .commit_activation_transaction(&mut transaction)
                .expect_err("unconfirmed probe cannot publish")
                .code,
            VulcanPluginStoreErrorCode::Conflict
        );
        drop(transaction);
        // Reacquisition is permitted only after actual ownership ends, without replaying the old probe.
        // 仅实际所有权结束后允许重新取得请求，不重放旧探测。
        let mut resumed = store
            .resume_activation_transaction("hook-plugin", request)
            .expect("original ownership");
        executor().block_on(async {
            // Fresh inspection is not fresh authorization to rerun an already attempted module.
            // 全新检查不等于重新运行已尝试模块的全新授权。
            let candidate = store
                .inspect_activation_candidate(&resumed)
                .expect("same original");
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("no replay after reopening");
        });
        store
            .abort_activation_transaction(&mut resumed)
            .expect("explicit abandonment");
    }
}

/// A journal observer itself retains the original request before any formal native admission.
/// 日志观察者本身在任何正式原生入场前保留原请求。
#[test]
fn formal_candidate_effects_observer_retains_original_preparation() {
    // No native work is started; this isolates persistence ownership from execution ownership.
    // 不启动原生工作；此测试隔离持久化所有权与执行所有权。
    let (fixture, store, transaction) = candidate_fixture(false, false, false);
    // Keep only the independent durable observer after dropping the borrowed candidate and transaction.
    // 销毁借用候选及事务后，仅保留独立持久观察者。
    let attempt = store
        .inspect_activation_candidate(&transaction)
        .expect("candidate")
        .begin_effect_attempt(Effect::Probe)
        .expect("intent");
    // Original identity is recorded before dropping foreground ownership.
    // 销毁前台所有权前记录原身份。
    let request = transaction.record().request_id();
    drop(transaction);
    assert_eq!(
        store
            .abort_activation_request("hook-plugin", request)
            .expect_err("observer owns request")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert_eq!(
        store
            .resume_activation_transaction("hook-plugin", request)
            .expect_err("no competing owner")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    drop(attempt);
    // Lost observation remains Started; it does not become proof that nothing ran.
    // 丢失观测保持 Started；它不变为未执行任何工作的证明。
    let reopened =
        VulcanPluginGenerationStore::open(fixture.manager.user_plugin_root()).expect("reopen");
    assert_eq!(
        reopened
            .activation_effects("hook-plugin", request)
            .expect("receipt")
            .expect("intent")
            .observations(),
        [(Effect::Probe, Observation::Started)]
    );
    reopened
        .abort_activation_request("hook-plugin", request)
        .expect("explicit abandonment after ownership ends");
}
