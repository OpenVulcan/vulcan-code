//! Real unpublished Lua initialization retains the original parent and transaction retirement fence.
//! 真实未发布 Lua 初始化保留原父级及事务退役屏障。

/// Candidate retention consumes the original parent budget and request draining preserves published work.
/// 候选保留消耗原父级预算，按请求排空保留已发布工作。
#[test]
fn formal_candidate_probe_shares_retention_and_drains_only_original_request() {
    // Publish a real predecessor so ordinary and candidate admissions share the same plugin identity.
    // 发布真实前驱，使普通与候选入场共享相同插件身份。
    let (fixture, store, mut transaction) = candidate_fixture(false, false, true);
    fixture
        .manager
        .begin_draining("hook-plugin")
        .expect("stop predecessor admission");
    fixture
        .manager
        .rollback_draining_to_enabling("hook-plugin")
        .expect("prepare published probe");
    // Every record is registered into the original parent; no synthetic quota counter is used.
    // 每个记录登记到原父级；不使用合成配额计数器。
    let owner = fixture.owner();
    // One deadline bounds all unstarted records independently from the drain observation budget.
    // 单个截止时间约束全部未启动记录，独立于排空观测预算。
    let deadline = Instant::now() + WAIT;
    // Ordinary authority must come from the published catalog, never from a candidate descriptor.
    // 普通权威必须来自已发布目录，绝不来自候选描述符。
    let published = fixture.manager.snapshot().plugins["hook-plugin"].clone();
    // Retain an unstarted ordinary probe to prove exact request selection does not cancel it.
    // 保留未启动普通探测，证明精确请求选择不取消它。
    let ordinary = fixture
        .manager
        .register_owned_formal_call(
            &owner,
            published,
            Preparation {
                workspace: CallWorkspace::LifecycleEnvironment,
                arguments: json!({}),
                finalizer: None,
                scope: VulcanPluginBridgeLeaseScope::EnablingProbe,
                security_partition: "published-test".into(),
                deadline,
                admission: CallAdmission::Probe,
            },
        )
        .expect("register original ordinary probe");
    // Original storage identity selects all candidate records and no published probe.
    // 原存储身份选择全部候选记录，不选择已发布探测。
    let request_id = transaction.record().request_id();
    {
        // A genuine view retains the original transaction lock before any formal admission.
        // 在任何正式入场前，真实视图保留原事务锁。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect candidate");
        // All private call authority derives from that exact view.
        // 全部私有调用权威从该精确视图派生。
        let authority = CandidateExecution::new(&fixture.manager.state, &candidate)
            .expect("candidate authority");
        for _ in 1..owner.settings.parent.max_operations {
            // Register genuine inert preparation; each receipt consumes real parent history.
            // 登记真实惰性准备；每个回执消耗实际父级历史容量。
            let call = fixture
                .manager
                .register_owned_formal_call(
                    &owner,
                    authority.plugin.clone(),
                    Preparation {
                        workspace: CallWorkspace::LifecycleEnvironment,
                        arguments: json!({}),
                        finalizer: None,
                        scope: VulcanPluginBridgeLeaseScope::EnablingProbe,
                        security_partition: format!("candidate:hook-plugin:{request_id}"),
                        deadline,
                        admission: CallAdmission::CandidateProbe {
                            authority: Arc::clone(&authority),
                        },
                    },
                )
                .expect("register original candidate probe");
            // These deliberately unstarted observers are already done; actual retirement still gates reaping.
            // 这些刻意未启动的观测者已结束；实际退役仍约束回收。
            call.acknowledged.store(true, Ordering::Release);
        }
        executor().block_on(async {
            // Public candidate admission must fail at the original retained-history ceiling.
            // 公开候选入场必须在原保留历史上限处失败。
            let failure = fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("shared parent retention is full");
            assert!(failure.message.contains("retention capacity"));
            // Only original candidate records are drained, even with an identical plugin namespace.
            // 即使插件命名空间相同，也只排空原候选记录。
            let report = fixture
                .manager
                .drain_activation_candidate_probes(request_id)
                .await
                .expect("drain original candidate records");
            assert!(report.failures.is_empty());
            assert_eq!(
                report.closed_formal_calls,
                owner.settings.parent.max_operations - 1
            );
            assert!(
                fixture
                    .manager
                    .activation_candidate_calls(request_id)
                    .expect("candidate query")
                    .is_empty()
            );
        });
        assert!(!ordinary.cancelled.load(Ordering::Acquire));
        assert!(!ordinary.lock().expect("ordinary state").retired);
        assert_eq!(owner.calls.lock().expect("original registry").len(), 1);
        assert_eq!(
            fixture.owner().host.runtime().id(),
            owner.host.runtime().id()
        );
    }
    // Ordinary published records do not retain the candidate transaction's readiness lock.
    // 普通已发布记录不保留候选事务的就绪锁。
    store
        .abort_activation_transaction(&mut transaction)
        .expect("candidate ownership retired");
    ordinary.acknowledged.store(true, Ordering::Release);
    executor().block_on(async {
        fixture
            .manager
            .drain_selected_formal_calls(&owner, vec![ordinary])
            .await
            .expect("clean up only remaining published record");
    });
    assert!(owner.calls.lock().expect("final registry").is_empty());
}

use super::*;
use crate::{
    VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase,
    VulcanPluginConfigurationSelection, VulcanPluginCredentialBackend, VulcanPluginCredentialKey,
    VulcanPluginCredentialRevision, VulcanPluginCredentialValues, VulcanPluginGenerationStore,
    VulcanPluginStoreErrorCode,
};

/// No secret slots are declared; any unexpected backend access makes staging fail explicitly.
/// 未声明秘密槽位；任何意外后端访问都会使暂存明确失败。
struct NoSecrets;

impl VulcanPluginCredentialBackend for NoSecrets {
    /// Reject reading a secret for this empty inventory; returns the fixture error.
    /// 拒绝为此空清单读取秘密；返回夹具错误。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Err("Candidate fixture declares no secret reads.".into())
    }
    /// Reject writing a secret for this empty inventory; returns the fixture error.
    /// 拒绝为此空清单写入秘密；返回夹具错误。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err("Candidate fixture declares no secret writes.".into())
    }
    /// Reject deleting a secret for this empty inventory; returns the fixture error.
    /// 拒绝为此空清单删除秘密；返回夹具错误。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err("Candidate fixture declares no secret deletions.".into())
    }
}

/// Prepare source under fixture's actual user root and return a genuine unpublished configured transaction.
/// 在 fixture 实际用户根目录下准备 source，并返回真正未发布的配置事务。
/// publish_predecessor selects an update whose prior configuration intentionally differs from the candidate.
/// publish_predecessor 选择前驱配置刻意不同于候选的更新。
fn transaction_for(
    fixture: &Fixture,
    source: &Path,
    publish_predecessor: bool,
) -> (
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
) {
    // Production inspection is the sole authority for source bytes and required permissions.
    // 生产检查为来源字节及所需权限的唯一权威。
    let inspected = crate::inspect_user_plugin_package(source).expect("inspect candidate fixture");
    // The store belongs to the same user namespace configured on the manager.
    // 存储属于管理器配置的相同用户命名空间。
    let store = VulcanPluginGenerationStore::open(&fixture.manager.state.user_plugin_root)
        .expect("open actual user generation store");
    // Preparation returns the actual physical lease, rather than a guessed package path.
    // 准备返回实际物理租约，而非猜测的插件包路径。
    let package = store
        .prepare(source, &inspected.content_fingerprint)
        .expect("prepare immutable candidate");
    // Revision ownership is genuine even though the fixture has no external secret slots.
    // 尽管夹具没有外部秘密槽位，修订所有权仍是真实的。
    let credentials = VulcanPluginCredentialRevision::allocate();
    store
        .begin_credential_staging(
            &package,
            credentials.clone(),
            &VulcanPluginCredentialValues::new(),
        )
        .expect("begin empty credential staging")
        .write_and_finish(&NoSecrets, &VulcanPluginCredentialValues::new())
        .expect("complete empty credential staging");
    // Configured fixtures use a value different from their package default.
    // 配置夹具使用与插件包默认值不同的值。
    let configuration = if inspected
        .manifest
        .as_ref()
        .expect("validated manifest")
        .config
        .is_some()
    {
        VulcanPluginConfigurationSelection::Document {
            value: json!({"label":"selected-candidate"}),
        }
    } else {
        VulcanPluginConfigurationSelection::NotDeclared {}
    };
    // Freeze permissions once for both the original selection and this candidate.
    // 为原选择及此候选一次性固定权限。
    let permissions = crate::runtime::plugin_required_permissions(&inspected)
        .expect("exact approved permissions");
    // An update keeps an intentionally different prior document visible during candidate initialization.
    // 更新在候选初始化期间持续公开刻意不同的先前文档。
    let expected_revision = if publish_predecessor {
        // Configuration ownership comes from the declared selection, never from a guessed file location.
        // 配置归属来自声明选择，绝不来自猜测的文件位置。
        let previous = match &configuration {
            VulcanPluginConfigurationSelection::Document { .. } => {
                VulcanPluginConfigurationSelection::Document {
                    value: json!({"label":"old-published"}),
                }
            }
            VulcanPluginConfigurationSelection::NotDeclared {} => configuration.clone(),
        };
        // Use production publication to create the full predecessor before beginning the request.
        // 使用生产发布在开始请求之前创建完整前驱。
        let active = store
            .activate_configured(
                &package,
                0,
                permissions.clone(),
                credentials.clone(),
                previous,
            )
            .expect("publish exact prior selection");
        fixture
            .manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish old user catalog");
        active.revision
    } else {
        0
    };
    // The original request identity is allocated once and retained through cancellation recovery.
    // 原请求身份只分配一次，并在取消恢复期间保留。
    let transaction = store
        .begin_activation_transaction(
            Uuid::new_v4(),
            &package,
            expected_revision,
            permissions,
            credentials,
            configuration,
        )
        .expect("prepare unpublished activation");
    (store, transaction)
}

/// Build block-controlled initialization with optional config reads, removing only the fixture's bundled source from discovery.
/// 构建受控阻塞初始化及可选配置读取，仅从发现中移走夹具自身内置来源。
/// Returns the original formal manager, store and prepared transaction; no second parent is constructed.
/// 返回原正式管理器、存储及准备事务；不构造第二个父级。
/// publish_predecessor chooses first installation or an existing user selection.
/// publish_predecessor 选择首次安装或已有用户选择。
fn candidate_fixture(
    block: bool,
    config: bool,
    publish_predecessor: bool,
) -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
) {
    // Reuse the real module, native publisher and explicit finite cleanup budget.
    // 复用真实模块、原生发布器及显式有限清理预算。
    let fixture = Fixture::transient(block, false, 1000);
    // Both paths are descendants of this fixture's own temporary directory.
    // 两个路径均为此夹具自有临时目录的后代。
    let original = fixture._root.path().join("bundled/hook-plugin");
    // Preserve source bytes outside public discovery before creating the immutable target.
    // 创建不可变目标之前，在公开发现之外保留来源字节。
    let source = fixture._root.path().join("source/hook-plugin");
    std::fs::create_dir_all(source.parent().expect("fixture source parent"))
        .expect("create source parent");
    std::fs::rename(original, &source).expect("move only isolated fixture source");
    if config {
        // Add the exact configuration permission and declaration before production preparation.
        // 在生产准备前添加精确配置权限及声明。
        let manifest = std::fs::read_to_string(source.join("plugin.yaml")).expect("fixture manifest")
            .replace("permissions: [process_binary, ui_host_context]", "permissions: [process_binary, ui_host_context, config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json");
        std::fs::write(source.join("plugin.yaml"), manifest)
            .expect("configured candidate declaration");
        std::fs::write(source.join("config.schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#)
            .expect("candidate configuration schema");
        std::fs::write(
            source.join("config.defaults.json"),
            r#"{"label":"package-default"}"#,
        )
        .expect("candidate configuration defaults");
        // Initialization must read the selected transaction document through the real native service.
        // 初始化必须通过真实原生服务读取事务选定文档。
        let bundle = std::fs::read_to_string(source.join("runtime/plugin.bundle.lua"))
            .expect("fixture bundle");
        std::fs::write(source.join("runtime/plugin.bundle.lua"), format!(
            "-- Read the exact prepared configuration; return no business result.\n-- 读取精确准备配置；不返回业务结果。\nlocal selected = vulcan.host.call('vulcan.plugin.config.get', {{}})\nassert(selected.label == 'selected-candidate', 'wrong candidate configuration')\n{bundle}"
        )).expect("candidate initialization with real config read");
    }
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("remove source from public discovery");
    // Actual preparation validates all modified bytes and freezes the complete target selection.
    // 实际准备校验全部修改字节并固定完整目标选择。
    let (store, transaction) = transaction_for(&fixture, &source, publish_predecessor);
    (fixture, store, transaction)
}

/// Wait until original authority has no actual call, preparation or native pool owner; returns only after real retirement.
/// 等待原权威不再具有实际调用、准备或原生池所有者；仅在真实退役后返回。
async fn authority_retired(authority: &std::sync::Weak<CandidateExecution>) {
    tokio::time::timeout(WAIT, async {
        while authority.strong_count() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("original candidate authority physically retired");
}

/// A real candidate reads selected configuration, initializes in the original parent, and leaves public routes untouched.
/// 真实候选读取选定配置、在原父级初始化，并保持公开路由不变。
#[test]
fn formal_candidate_probe_uses_original_parent_and_selected_configuration() {
    for publish_predecessor in [false, true] {
        // Native effects reveal any accidental business or implicit finalizer execution.
        // 原生副作用揭示任何意外业务执行或隐式关闭器执行。
        let (fixture, store, mut transaction) = candidate_fixture(false, true, publish_predecessor);
        // Preserve exact public and parent identities before candidate admission.
        // 在候选入场前保留精确公开目录及父级身份。
        let catalog = fixture.manager.snapshot();
        // The complete original disk selection must survive the unpublished probe unchanged.
        // 完整原磁盘选择必须在未发布探测后保持不变。
        let previous = store
            .activation("hook-plugin")
            .expect("original disk selection");
        // The same attached parent must remain after every candidate invocation.
        // 每次候选调用后都必须保持同一附加父级。
        let owner = fixture.owner();
        // These host observations do not themselves allocate any VM.
        // 这些宿主观测本身不分配任何 VM。
        let runtime_id = owner.host.runtime().id().to_owned();
        executor().block_on(async {
            // The inspected borrow remains live until this exact invocation and its cleanup complete.
            // 已检查借用保持存活，直到此精确调用及其清理完成。
            let candidate = store
                .inspect_activation_candidate(&transaction)
                .expect("inspect runnable candidate");
            assert!(fixture.manager.probe_plugin("hook-plugin").await.is_err());
            fixture
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect("real candidate initialization");
            assert!(
                owner
                    .calls
                    .lock()
                    .expect("original call registry")
                    .is_empty()
            );
            assert_eq!(
                owner
                    .host
                    .runtime()
                    .resources()
                    .expect("original parent usage")
                    .resident,
                0
            );
            assert!(
                fixture
                    .manager
                    .formal_capacity_status(
                        "hook-plugin",
                        VulcanPluginCapacityPurpose::ActivationProbe
                    )
                    .expect("original purpose capacity")
                    .is_some()
            );
        });
        assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
        assert_eq!(fixture.owner().host.runtime().id(), runtime_id);
        assert_eq!(
            store
                .activation("hook-plugin")
                .expect("selection after probe"),
            previous
        );
        // Initialization is the only permitted event; business and shutdown declarations were never invoked.
        // 初始化为唯一允许事件；业务及关闭声明从未被调用。
        let topics = fixture
            .events
            .values
            .lock()
            .expect("actual initialization events")
            .iter()
            .map(|event| event.topic.clone())
            .collect::<Vec<_>>();
        assert_eq!(topics, ["initialized"]);
        store
            .commit_activation_transaction(&mut transaction)
            .expect("commit only after physical readiness retirement");
        assert_eq!(
            transaction.record().phase(),
            VulcanPluginActivationTransactionPhase::Published
        );
        // Publication advances exactly once from the original selection after readiness physically retires.
        // 就绪实际退役后，发布从原选择恰好推进一次。
        assert_eq!(
            store
                .activation("hook-plugin")
                .expect("committed selection")
                .expect("published target")
                .revision,
            previous.map_or(1, |active| active.revision + 1)
        );
    }
}

/// Cancelling the public observer releases its Rust borrow but cannot release an in-flight native worker's transaction fence.
/// 取消公开观测者释放其 Rust 借用，但不能释放在途原生工作者的事务屏障。
#[test]
fn formal_candidate_probe_cancellation_retains_original_request_ownership() {
    // Native initialization is held at an actual host callback, independently from Tokio observation.
    // 原生初始化停在实际宿主回调，独立于 Tokio 观测。
    let (fixture, store, transaction) = candidate_fixture(true, false, false);
    // Original request identity survives dropping the caller's exclusive owner.
    // 原请求身份在释放调用方排他所有者后仍保留。
    let request_id = transaction.record().request_id();
    executor().block_on(async {
        // The observer task owns the original transaction and its borrowed candidate view.
        // 观测任务拥有原事务及其借用候选视图。
        let manager = fixture.manager.clone();
        // A store handle is not a request lease and may remain after the task is cancelled.
        // 存储句柄不是请求租约，可在任务取消后保留。
        let probing_store = store.clone();
        // Cancellation must not fabricate native completion.
        // 取消不得伪造原生完成。
        let observer = tokio::spawn(async move {
            // Borrow before starting the production probe.
            // 在开始生产探测前取得借用。
            let candidate = probing_store
                .inspect_activation_candidate(&transaction)
                .expect("inspect blocking candidate");
            manager.probe_activation_candidate(&candidate).await
        });
        fixture.entered().await;
        // Retain only weak evidence so the test itself cannot pin readiness after real cleanup.
        // 仅保留弱证据，避免测试自身在真实清理后固定就绪所有权。
        let authority = {
            // The original formal receipt points to the actual initialized core session.
            // 原正式回执指向实际初始化核心会话。
            let call = fixture.admitted().await;
            assert!(
                call.lock()
                    .expect("actual initialized checkpoint")
                    .initialized
                    .is_some()
            );
            Arc::downgrade(
                call.candidate
                    .as_ref()
                    .expect("explicit candidate authority"),
            )
        };
        assert_eq!(
            fixture
                .owner()
                .host
                .runtime()
                .resources()
                .expect("live original parent usage")
                .resident,
            1
        );
        observer.abort();
        assert!(
            observer
                .await
                .expect_err("observer cancelled")
                .is_cancelled()
        );
        // The original exclusive request lock remains owned by actual work after its caller disappears.
        // 原排他请求锁在调用方消失后仍由实际工作持有。
        assert_eq!(
            store
                .resume_activation_transaction("hook-plugin", request_id)
                .expect_err("live candidate still owns original request")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        // Public inspection keeps original initialization queryable without taking transaction ownership.
        // 公开检查无需取得事务所有权即可查询原初始化。
        let statuses = fixture
            .manager
            .activation_candidate_calls(request_id)
            .expect("query original candidate call");
        assert_eq!(statuses.len(), 1);
        assert!(
            statuses
                .iter()
                .all(|status| status.initialization.is_some() && !status.retired)
        );
        assert!(
            fixture
                .manager
                .activation_candidate_calls(Uuid::new_v4())
                .expect("different request query")
                .is_empty()
        );
        // Another request's drain cannot claim this actual operation.
        // 另一请求的排空不能声称拥有此实际操作。
        let unrelated = fixture
            .manager
            .drain_activation_candidate_probes(Uuid::new_v4())
            .await
            .expect("different request drain");
        assert_eq!(unrelated.closed_formal_calls, 0);
        assert_eq!(
            store
                .abort_activation_request("hook-plugin", request_id)
                .expect_err("native worker blocks recovery abort")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            store
                .activation_transaction("hook-plugin", request_id)
                .expect("unchanged journal")
                .expect("original request")
                .phase(),
            VulcanPluginActivationTransactionPhase::Prepared
        );
        assert!(
            store
                .activation("hook-plugin")
                .expect("no early publication")
                .is_none()
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_candidate_probes(request_id)
            .await
            .expect("finish original cleanup");
        authority_retired(&authority).await;
        store
            .abort_activation_request("hook-plugin", request_id)
            .expect("abort only after actual retirement");
        assert!(
            fixture
                .owner()
                .calls
                .lock()
                .expect("call history after cleanup")
                .is_empty()
        );
    });
}

/// A finite probe deadline reports failure while preserving the original real readiness owner until its native callback exits.
/// 有限探测截止时间报告失败，同时保留原实际就绪所有者，直到其原生回调退出。
#[test]
fn formal_candidate_probe_deadline_preserves_native_ownership() {
    // The fixture callback blocks longer than a deliberately short invocation deadline.
    // 夹具回调阻塞超过刻意设置的短调用截止时间。
    let (fixture, store, mut transaction) = candidate_fixture(true, false, false);
    executor().block_on(async {
        // The view outlives observation, then is explicitly dropped before testing the real storage fence.
        // 视图比观测存活更久，然后在测试实际存储屏障前明确释放。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect timed candidate");
        // Use a deadline that leaves room for real package preparation without waiting the production maximum.
        // 使用允许实际插件包准备的截止时间，无需等待生产最长预算。
        let deadline = Instant::now() + Duration::from_millis(250);
        // Run the exact production implementation, retaining its original host receipt.
        // 运行精确生产实现，保留其原宿主回执。
        let result = fixture
            .manager
            .invoke_formal_candidate_probe(&candidate, deadline)
            .await;
        assert!(fixture.events.entered.load(Ordering::Acquire));
        assert!(
            result
                .expect_err("original candidate deadline")
                .message
                .contains("timed out")
        );
        // A weak reference identifies actual retained work without prolonging its lifetime.
        // 弱引用标识实际保留工作，不延长其寿命。
        let authority = {
            // The exact original call must still be present after the timeout reply.
            // 超时应答后，精确原调用仍须存在。
            let calls = fixture.owner();
            // Registry ownership survives the departed observation.
            // 注册表所有权在观测离开后持续存在。
            let calls = calls.calls.lock().expect("retained timed calls");
            // Resolve the candidate by its private authority, never by an evolving registry position.
            // 按私有权威解析候选，绝不按会变化的注册表位置解析。
            let call = calls
                .values()
                .find(|call| call.candidate.is_some())
                .expect("original candidate call retained");
            Arc::downgrade(call.candidate.as_ref().expect("candidate readiness owner"))
        };
        drop(candidate);
        assert_eq!(
            store
                .commit_activation_transaction(&mut transaction)
                .expect_err("timeout cannot release native ownership")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        assert_eq!(
            store
                .abort_activation_transaction(&mut transaction)
                .expect_err("native work also blocks owned abort")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        // Finite draining must report retained native work without claiming closure or releasing the request.
        // 有限排空必须报告保留原生工作，不能声称关闭或释放请求。
        let blocked = tokio::time::timeout(
            WAIT,
            fixture
                .manager
                .drain_activation_candidate_probes(transaction.record().request_id()),
        )
        .await
        .expect("finite candidate drain observation")
        .expect("blocked drain report");
        assert_eq!(blocked.closed_formal_calls, 0);
        assert!(!blocked.failures.is_empty());
        assert!(authority.strong_count() > 0);
        assert_eq!(
            store
                .commit_activation_transaction(&mut transaction)
                .expect_err("finite drain cannot fake retirement")
                .code,
            VulcanPluginStoreErrorCode::Busy
        );
        fixture.events.release();
        fixture
            .manager
            .drain_activation_candidate_probes(transaction.record().request_id())
            .await
            .expect("drain timed original call");
        authority_retired(&authority).await;
        store
            .abort_activation_transaction(&mut transaction)
            .expect("retired failed candidate can abort");
    });
}

/// Another manager and a bundled shadow are rejected before allocating candidate VM work.
/// 另一管理器及内置遮蔽在分配候选 VM 工作前被拒绝。
#[test]
fn formal_candidate_probe_rejects_foreign_catalog_and_bundled_shadow() {
    // Candidate belongs exclusively to the first manager's actual user catalog.
    // 候选仅属于第一个管理器的实际用户目录。
    let (fixture, store, transaction) = candidate_fixture(false, false, false);
    // A distinct real manager has a different configured user root and no attached formal parent.
    // 独立真实管理器具有不同配置用户根目录，并且没有附加正式父级。
    let foreign_root = tempfile::tempdir().expect("foreign manager fixture");
    // Ownership rejection must precede any attempt to attach a new runtime.
    // 归属拒绝必须先于任何附加新运行时的尝试。
    let foreign = test_manager(foreign_root.path(), foreign_root.path().join("bundled"));
    executor().block_on(async {
        // The same inspected view cannot authorize a different user namespace.
        // 同一已检查视图不能授权另一用户命名空间。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect foreign test candidate");
        assert!(
            foreign
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("foreign root denied")
                .message
                .contains("another user catalog")
        );
        assert!(
            foreign
                .formal_runtime_status()
                .expect("no invented parent")
                .is_none()
        );
        assert!(
            fixture
                .owner()
                .calls
                .lock()
                .expect("original registry unchanged")
                .is_empty()
        );
    });
    // This independent fixture retains a published bundled plugin with the same ID.
    // 此独立夹具保留相同标识的已发布内置插件。
    let bundled = Fixture::transient(false, false, 1000);
    // Immutable preparation is legal; host execution must still enforce bundled precedence.
    // 不可变准备合法；宿主执行仍须约束内置优先级。
    let (shadow_store, shadow_transaction) = transaction_for(
        &bundled,
        &bundled._root.path().join("bundled/hook-plugin"),
        false,
    );
    executor().block_on(async {
        // Probe-only authority cannot replace a bundled owner.
        // 仅探测权威不能替换内置所有者。
        let candidate = shadow_store
            .inspect_activation_candidate(&shadow_transaction)
            .expect("inspect shadow candidate");
        assert!(
            bundled
                .manager
                .probe_activation_candidate(&candidate)
                .await
                .expect_err("bundled owner denied")
                .message
                .contains("shadow a bundled")
        );
        assert!(
            bundled
                .owner()
                .calls
                .lock()
                .expect("bundled registry unchanged")
                .is_empty()
        );
    });
}
