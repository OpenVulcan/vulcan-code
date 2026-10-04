//! Mixed bootstrap validates producers before dispatch and observes raw and sealed originals concurrently.
//! 混合引导在派发前校验生产者，并并发观察原始及密封原实例。

use super::*;
use crate::mcp::{McpServerSource, McpServerTransportConfig};

/// Foreign issuers and duplicate raw/sealed owners reject before any real native task starts.
/// 外来签发者及重复原始／密封所有者在任何真实原生任务启动前拒绝。
/// No inputs or return; real prepared authority and absence of the actual child marker are verified for both errors.
/// 无输入或返回；对两项错误验证真实已准备权威及实际子进程标记不存在。
#[tokio::test]
async fn published_bootstrap_rejects_foreign_issuer_and_double_producer_before_dispatch() {
    for foreign_issuer in [true, false] {
        // Each case owns a distinct actual selected package and original issuer, without native work.
        // 每个用例拥有不同实际已选插件包及原签发者，没有原生工作。
        let PublishedFixture {
            manager,
            configs,
            authority,
            marker,
            _temp,
            ..
        } = published_fixture("ready");
        // One raw declaration would actually launch the fixture if pre-dispatch validation were missing.
        // 若缺少派发前校验，单个原始声明将实际启动夹具。
        let mut raw = configs
            .servers()
            .iter()
            .find(|server| server.id == "bound-plugin:local")
            .expect("expected original local declaration may have changed")
            .clone();
        // The issuer error targets an unrelated ordinary user process; the overlap error targets the exact sealed owner.
        // 签发者错误针对无关普通用户进程；重叠错误针对精确密封所有者。
        let checked_authority = if foreign_issuer {
            raw.source = McpServerSource::UserConfigured;
            manager.register_native_mcp_host_adapter()
        } else {
            authority.clone()
        };
        assert!(
            bootstrap_mcp_runtime_access_with_published(
                vec![raw],
                &manager,
                &checked_authority,
                vec![configs],
            )
            .await
            .is_err()
        );
        assert!(
            !marker.exists(),
            "no actual child may start before exact producer validation"
        );
    }
}

/// The shared batch initializer dispatches a sealed child before waiting for a slow legacy partition and cancellation closes both originals.
/// 共享批次初始化器在等待慢旧分区之前派发密封子进程，取消关闭两个原实例。
/// No inputs or return; exact registered access, independent source observation and true physical retirement are evidence.
/// 无输入或返回；精确已登记访问、独立来源观察及真实物理退役为证据。
#[tokio::test]
async fn published_bootstrap_mixed_originals_start_before_readiness_and_cancel_without_replay() {
    // The production fixture has genuinely released its unused candidate before any native dispatch.
    // 生产夹具在任何原生派发前已真实释放未使用候选。
    let PublishedFixture {
        manager,
        configs,
        authority,
        marker,
        environment,
        _temp,
        ..
    } = published_fixture("ready");
    // This distinct raw declaration deliberately uses the documented unproven legacy producer.
    // 此不同原始声明刻意使用文档规定的未证明旧生产者。
    let mut legacy = configs
        .servers()
        .iter()
        .find(|server| server.id == "bound-plugin:local")
        .expect("expected actual local source may have changed")
        .clone();
    legacy.id = String::from("explicit-legacy:local");
    legacy.source = McpServerSource::Plugin {
        plugin_id: String::from("explicit-legacy"),
        generation: 1,
    };
    // The exact raw child has its own real producer record rather than sharing the sealed marker.
    // 精确原始子进程具有自身真实生产者记录，不共享密封标记。
    let legacy_marker = _temp.path().join("legacy-starts.jsonl");
    match &mut legacy.transport {
        McpServerTransportConfig::Stdio { env, .. } => {
            env.insert(String::from("CANDIDATE_MODE"), String::from("stall"));
            env.insert(
                String::from("CANDIDATE_MARKER"),
                legacy_marker
                    .to_str()
                    .expect("UTF-8 fixture path")
                    .to_owned(),
            );
        }
        _ => panic!("the expected actual stdio fixture declaration may have changed"),
    }
    // Observe actual registrations before readiness without constructing a lease, transport or publication receipt.
    // 在就绪之前观察实际登记，不构造租约、传输或发布回执。
    let (registered_tx, registered_rx) = tokio::sync::oneshot::channel();
    // The one original manager stays alive across this cancellable mixed bootstrap future.
    // 单个原管理器跨此可取消混合启动 future 保持存活。
    let original_manager = manager.clone();
    // The actual single batch helper is shared by both formal sealed and explicit raw public factories.
    // 实际单批次辅助方法由正式密封及明确原始公开工厂共同使用。
    let pending = tokio::spawn(async move {
        bootstrap_mcp_runtime_access_with_originals(vec![legacy], |access| {
            // This is the same actual sealed factory consumed by the formal public wrapper.
            // 此为正式公开包装消费的同一实际密封工厂。
            let native =
                PublishedPluginMcpRuntime::start(access, &original_manager, configs, &authority)?;
            registered_tx
                .send((access.clone(), native.observer()))
                .unwrap_or_else(|_| panic!("actual registration observer was lost"));
            Ok(vec![native])
        })
        .await
    });
    // The existing shorter observation budget must see registration before the stalled legacy deadline completes.
    // 既有较短观察预算必须在停滞旧期限完成前看见登记。
    let (access, observation) = tokio::time::timeout(OBSERVE, registered_rx)
        .await
        .expect("sealed original must register before legacy readiness wait")
        .expect("actual original registration");
    observation
        .wait_until_ready(OBSERVE)
        .await
        .expect("sealed original ready alongside stalled legacy");
    observation
        .verify_source("bound-plugin:local", &observation.profile())
        .expect("same actual sealed input source");
    assert!(
        !pending.is_finished(),
        "actual stalled legacy wait must still be pending"
    );
    assert_eq!(
        recorded_starts(&marker).await.len(),
        1,
        "expected sealed original count may have changed"
    );
    assert_eq!(
        recorded_starts(&legacy_marker).await.len(),
        1,
        "expected raw original count may have changed"
    );
    pending.abort();
    assert!(matches!(pending.await, Err(error) if error.is_cancelled()));
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("actual sealed original release after cancellation");
    // The retained actual access prevents natural last-lease drop, proving explicit startup cancellation owns cleanup.
    // 保留实际访问阻止自然最后租约销毁，证明明确启动取消拥有清理责任。
    access
        .begin_plugin_mcp_drain("explicit-legacy")
        .expect("actual raw original inventory");
    access
        .wait_plugin_mcp_drain("explicit-legacy", OBSERVE)
        .await
        .expect("same actual raw original retired");
    assert_eq!(
        recorded_starts(&marker).await.len(),
        1,
        "cancelled bootstrap cannot replay sealed initialization"
    );
    assert_eq!(
        recorded_starts(&legacy_marker).await.len(),
        1,
        "cancelled bootstrap cannot replay raw initialization"
    );
    manager
        .begin_draining("bound-plugin")
        .expect("actual disabling source");
    manager
        .finish_lifecycle_transition(
            "bound-plugin",
            &BTreeSet::from([String::from("bound-plugin")]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("actual disabled source");
    assert!(environment.starts_with(_temp.path().canonicalize().expect("isolated fixture root")));
    std::fs::remove_file(environment.join("bin/server.bin"))
        .expect("remove isolated retired binary");
    assert_eq!(
        manager
            .prepare_plugin_environment("bound-plugin")
            .expect("original startup observer cannot pin environment"),
        environment
    );
}
