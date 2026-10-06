//! Core predecessor publication seals exact catalogs while Pending and physical owners keep business closed.
//! 核心前驱发布封闭精确目录，Pending 及物理所有者继续保持业务关闭。

use super::*;
use crate::VulcanPluginPredecessorRestorationSource;

mod completion;

/// Prepare the actual independent owner, aborted closed route and original source through existing production APIs.
/// 经既有生产 API 准备实际独立所有者、已中止关闭路由及原来源。
/// No inputs; return fixture, store, transaction, restoration and source without starting a new recovery framework.
/// 无输入；返回夹具、存储、事务、恢复对象及来源，不建立新恢复框架。
fn publication_fixture() -> (
    Fixture,
    VulcanPluginGenerationStore,
    VulcanPluginActivationTransaction,
    VulcanPluginPredecessorRestoration,
    Arc<VulcanPluginPredecessorRestorationSource>,
) {
    // The existing fixture owns genuine managed old configuration, credentials and completed resource drain.
    // 既有夹具拥有真正托管旧配置、凭据及已完成资源排空。
    let (fixture, store, mut transaction, mut restoration) = private_owner_fixture();
    fixture
        .manager
        .prepare_activation_predecessor_private_owner(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual independent Pending owner");
    fixture
        .manager
        .abort_and_stage_activation_predecessor_restoration(
            &mut transaction,
            &mut restoration,
            false,
            false,
        )
        .expect("actual aborted closed route");
    // Source signing retains the same real reservation before any Hook initialization.
    // 来源签发在任何 Hook 初始化前保留相同真实预留权。
    let source = fixture
        .manager
        .prepare_activation_predecessor_source(&transaction, &mut restoration)
        .expect("same original physical source");
    (fixture, store, transaction, restoration, source)
}

/// Borrow the real initialized registry Hook, returning its retained allocation without constructing a substitute.
/// 借用真实已初始化注册表 Hook，返回保留分配，不构造替代物。
/// fixture supplies the actual formal parent; missing runtime fails the test.
/// fixture 提供实际正式父级；运行时缺失使测试失败。
fn initialized_hook(fixture: &Fixture) -> Arc<crate::runtime::formal::PersistentRuntime> {
    fixture
        .owner()
        .hooks
        .lock()
        .expect("actual Hook registry")
        .get("hook-plugin")
        .cloned()
        .expect("actual initialized old Hook")
}

/// Validate publication against manager/transaction/source under the required actual registration exclusion.
/// 在要求的实际注册互斥下，按 manager／transaction／source 校验 publication。
/// Return the production validator result without minting evidence or changing initialization authority.
/// 返回生产校验器结果，不签发凭据或改变初始化权威。
fn validate_publication(
    publication: &crate::VulcanPluginActivationPredecessorPublication,
    manager: &VulcanPluginRuntimeManager,
    transaction: &VulcanPluginActivationTransaction,
    source: &Arc<VulcanPluginPredecessorRestorationSource>,
) -> Result<(), VulcanPluginRuntimeError> {
    // The internal production contract requires the caller to hold this same real manager gate.
    // 内部生产契约要求调用方持有此相同真实管理器门。
    let _registration = manager
        .state
        .session_registration_gate
        .lock()
        .expect("actual publication validation gate");
    publication.validate_current(manager, transaction, source)
}

/// Failed publication restores the same closed Arc; successful sealing retains actual old instances and closed business.
/// 发布失败恢复相同关闭 Arc；成功封闭保留实际旧实例及关闭业务。
/// No inputs or return value; this core callback proves catalog sealing, not actual service Full or native MCP Ready.
/// 无输入或返回值；此核心回调证明目录封闭，不证明实际服务 Full 或原生 MCP 就绪。
#[test]
fn predecessor_publication_seals_actual_final_arc_without_transfer_or_reinitialization() {
    // Every retained resource originates in the existing actual restoration fixture.
    // 每个保留资源均来自既有实际恢复夹具。
    let (fixture, store, transaction, restoration, source) = publication_fixture();
    // Preserve the exact closed allocation emitted by original abort staging.
    // 保留原中止暂存产出的精确关闭分配。
    let closed = Arc::clone(source.staged_catalog());
    // Physical Pending bytes must not change during either publisher outcome.
    // 两种发布器结果期间，物理 Pending 字节均不得变化。
    let pending_path = restoration
        .private_owner_test_path(true)
        .expect("actual Pending path");
    // A byte observation detects any new sidecar phase rather than trusting equivalent parsed identity.
    // 字节观测检测任何新附属阶段，不依赖等价解析身份。
    let pending_bytes = std::fs::read(&pending_path).expect("original Pending bytes");
    // The independent File probe observes the actual original reservation, never a guessed namespace.
    // 独立文件探测观测实际原预留权，绝不使用猜测命名空间。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual independent lease path"),
        )
        .expect("actual independent probe");
    // A declared but unprepared Hook must reject publication before any external pointer can be changed.
    // 已声明但未准备的 Hook 必须在任何外部指针可以改变前拒绝发布。
    let mut unprepared_callback = false;
    assert!(
        fixture
            .manager
            .publish_activation_predecessor_handoff(&transaction, &source, |_| {
                unprepared_callback = true;
                Ok(())
            })
            .is_err()
    );
    assert!(!unprepared_callback);
    assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("actual old initialization")
        );
        // Actual registered pool/session/initialization identities establish the original VM provenance.
        // 实际注册池／会话／初始化身份建立原虚拟机归属。
        let original = initialized_hook(&fixture);
        // Capture historical identities before publication while preserving physical private ownership.
        // 发布前捕获历史身份，同时保留物理私有所有权。
        let identities = original.target_test_identity();
        assert!(identities.3 && !identities.4);
        // The restricted observer reads the same actual native domain without adding another owner.
        // 受限观测者读取相同实际原生域，不增加另一所有者。
        let domain = fixture
            .owner()
            .host
            .target_test_domain_identity(identities.0.as_deref().expect("actual pool"));
        // Query the original session and operation rather than inferring readiness from a descriptor.
        // 查询原会话及操作，不从描述符推断就绪。
        let owner = fixture.owner();
        assert_eq!(
            owner
                .host
                .runtime()
                .session(identities.1.as_deref().expect("actual session"))
                .expect("actual idle session")
                .phase,
            luaskills::runtime::embedded::EmbeddedSessionPhase::Ready
        );
        assert_eq!(
            owner
                .host
                .runtime()
                .operation(identities.2.as_deref().expect("actual initialization"))
                .expect("actual initialization operation")
                .snapshot()
                .expect("actual operation snapshot")
                .phase,
            luaskills::runtime::embedded::OperationPhase::Succeeded
        );
        // A normal failing synchronous publisher must be invoked once and restore the exact original pointer.
        // 正常失败的同步发布器必须仅调用一次，并恢复精确原指针。
        let mut failed_callback = false;
        assert!(
            fixture
                .manager
                .publish_activation_predecessor_handoff(&transaction, &source, |provisional| {
                    failed_callback = true;
                    assert!(Arc::ptr_eq(&provisional, &fixture.manager.snapshot()));
                    assert_eq!(
                        provisional.plugins["hook-plugin"].runtime_state,
                        VulcanPluginRuntimeState::Enabled
                    );
                    Err("controlled predecessor publisher failure".into())
                })
                .is_err()
        );
        assert!(failed_callback);
        assert!(Arc::ptr_eq(&closed, &fixture.manager.snapshot()));
        assert_eq!(original.target_test_identity(), identities);
        // Source retry remains tied to this same actual initialized instance after failed publication.
        // 发布失败后，来源重试仍绑定此相同实际已初始化实例。
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("same actual old Hook")
        );
        assert_eq!(original.target_test_identity(), identities);
        // Retain exactly the pointer accepted by the successful synchronous callback.
        // 保留成功同步回调所接受的精确指针。
        let mut accepted = None;
        // The sealed value is issued only by the successful core publisher, with no synthetic constructor.
        // 密封值仅由成功核心发布器签发，不使用合成构造器。
        let publication = fixture
            .manager
            .publish_activation_predecessor_handoff(&transaction, &source, |final_catalog| {
                assert!(
                    fixture
                        .manager
                        .state
                        .session_registration_gate
                        .try_lock()
                        .is_err()
                );
                assert!(Arc::ptr_eq(&final_catalog, &fixture.manager.snapshot()));
                accepted = Some(final_catalog);
                Ok(())
            })
            .expect("actual successful core publisher");
        assert!(Arc::ptr_eq(
            publication.catalog(),
            accepted.as_ref().expect("actual accepted Arc")
        ));
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        assert!(!Arc::ptr_eq(publication.catalog(), &closed));
        assert_eq!(
            publication.catalog().plugins["hook-plugin"].generation,
            source.plugin().generation
        );
        assert!(
            publication.catalog().plugins["hook-plugin"]
                .managed_package()
                .expect("actual old final selection")
                .matches_activation(
                    transaction
                        .record()
                        .predecessor()
                        .expect("original full old selection")
                )
        );
        validate_publication(&publication, &fixture.manager, &transaction, &source)
            .expect("exact sealed final authority");
        assert_eq!(
            transaction.record().phase(),
            crate::VulcanPluginActivationTransactionPhase::Aborted
        );
        assert_eq!(
            store
                .activation_transaction("hook-plugin", transaction.record().request_id())
                .expect("actual durable original")
                .expect("original exists"),
            *transaction.record()
        );
        assert_eq!(
            std::fs::read(&pending_path).expect("unchanged Pending bytes"),
            pending_bytes
        );
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(
            store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("Pending still fences business")
        );
        assert_eq!(
            store
                .deactivate(
                    "hook-plugin",
                    transaction
                        .record()
                        .predecessor()
                        .expect("actual old selection")
                        .revision
                )
                .expect_err("ordinary writer must remain fenced")
                .code,
            crate::VulcanPluginStoreErrorCode::Busy
        );
        // Even the final enabled surface cannot bypass original Pending or private Hook ownership.
        // 即使最终启用暴露面也不能绕过原 Pending 或私有 Hook 所有权。
        let hook = publication.catalog().plugins["hook-plugin"]
            .hooks
            .iter()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual old declared Hook");
        assert!(
            fixture
                .manager
                .invoke_formal_hook(
                    hook,
                    json!({"payload_json":"{}", "host_timeout_ms":1000}),
                    None,
                    Instant::now() + WAIT
                )
                .await
                .is_err()
        );
        // Full never widens the strict closed-only initialization authority into a second initializer.
        // Full 绝不将严格仅关闭初始化权威扩大为第二个初始化器。
        let failure = fixture
            .manager
            .prewarm_activation_predecessor_hook(&source)
            .await
            .expect_err("final publication must not grant fresh old initialization");
        assert!(
            failure
                .message
                .contains("requires its exact closed catalog"),
            "{}",
            failure.message
        );
        assert_eq!(original.target_test_identity(), identities);
        assert!(Arc::ptr_eq(&original, &initialized_hook(&fixture)));
        assert_eq!(
            fixture
                .owner()
                .host
                .target_test_domain_identity(identities.0.as_deref().expect("same pool")),
            domain
        );
        assert!(
            crate::runtime::environment::lock_plugin_execution_environment_cleanup(
                &fixture.manager.state,
                "hook-plugin"
            )
            .is_err()
        );
    });
}

/// Foreign managers, sources, full originals and field-identical catalog clones never reach the synchronous publisher.
/// 外来管理器、来源、完整原请求及字段相同目录克隆均不能到达同步发布器。
/// No inputs or return value; after success the same sealed final Arc remains mandatory for current validation.
/// 无输入或返回值；成功之后，当前校验仍必须使用相同密封最终 Arc。
#[test]
fn predecessor_publication_rejects_foreign_authority_and_catalog_allocations() {
    // Both originals come from independent actual stores and managed restoration flows.
    // 两个原请求均来自独立实际存储及托管恢复流程。
    let (fixture, _store, transaction, _restoration, source) = publication_fixture();
    // Foreign owners retain their own genuine original Aborted record and source allocation.
    // 外来所有者保留自身真正原 Aborted 记录及来源分配。
    let (foreign, _foreign_store, foreign_transaction, _foreign_restoration, foreign_source) =
        publication_fixture();
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("actual original Hook")
        );
        // Keep the real original instance and exact resource identities through every rejected call.
        // 跨每次拒绝调用保留真实原实例及精确资源身份。
        let original = initialized_hook(&fixture);
        // Every rejected authority must preserve this actual pool/session/operation allocation.
        // 每个被拒绝权威均必须保留此实际池／会话／操作分配。
        let identities = original.target_test_identity();
        // This callback counter observes only synchronous invocation and does not manufacture Full evidence.
        // 此回调计数器仅观测同步调用，不制造 Full 凭据。
        let callbacks = std::cell::Cell::new(0usize);
        assert!(
            foreign
                .manager
                .publish_activation_predecessor_handoff(&transaction, &source, |_| {
                    callbacks.set(callbacks.get() + 1);
                    Ok(())
                })
                .is_err()
        );
        assert!(
            fixture
                .manager
                .publish_activation_predecessor_handoff(&transaction, &foreign_source, |_| {
                    callbacks.set(callbacks.get() + 1);
                    Ok(())
                })
                .is_err()
        );
        assert!(
            fixture
                .manager
                .publish_activation_predecessor_handoff(&foreign_transaction, &source, |_| {
                    callbacks.set(callbacks.get() + 1);
                    Ok(())
                })
                .is_err()
        );
        // A clone has identical values but cannot replace the manager-issued closed allocation authority.
        // 克隆具有相同值，但不能替换管理器签发关闭分配权威。
        let closed = Arc::clone(source.staged_catalog());
        // Replace only the test fixture's actual pointer to isolate allocation validation.
        // 仅替换测试夹具实际指针，以隔离分配校验。
        let clone = Arc::new(closed.as_ref().clone());
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("test cloned closed Arc") = Arc::clone(&clone);
        assert!(
            fixture
                .manager
                .publish_activation_predecessor_handoff(&transaction, &source, |_| {
                    callbacks.set(callbacks.get() + 1);
                    Ok(())
                })
                .is_err()
        );
        assert!(Arc::ptr_eq(&fixture.manager.snapshot(), &clone));
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("restore exact issued closed Arc") = Arc::clone(&closed);
        assert_eq!(callbacks.get(), 0);
        assert_eq!(original.target_test_identity(), identities);
        // Success returns an actual seal only after the exact original allocation has been restored.
        // 仅恢复精确原分配之后，成功才返回实际密封值。
        let publication = fixture
            .manager
            .publish_activation_predecessor_handoff(&transaction, &source, |_| {
                callbacks.set(callbacks.get() + 1);
                Ok(())
            })
            .expect("original successful seal");
        assert_eq!(callbacks.get(), 1);
        validate_publication(&publication, &fixture.manager, &transaction, &source)
            .expect("original seal");
        assert!(
            validate_publication(&publication, &foreign.manager, &transaction, &source).is_err()
        );
        assert!(
            validate_publication(
                &publication,
                &fixture.manager,
                &foreign_transaction,
                &source
            )
            .is_err()
        );
        assert!(
            validate_publication(
                &publication,
                &fixture.manager,
                &transaction,
                &foreign_source
            )
            .is_err()
        );
        // A value-identical final clone cannot impersonate the pointer accepted by the original publisher.
        // 值相同最终克隆不能冒充原发布器接受的指针。
        let cloned_final = Arc::new(publication.catalog().as_ref().clone());
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("test cloned final Arc") = Arc::clone(&cloned_final);
        assert!(
            validate_publication(&publication, &fixture.manager, &transaction, &source).is_err()
        );
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("restore actual accepted final Arc") = Arc::clone(publication.catalog());
        validate_publication(&publication, &fixture.manager, &transaction, &source)
            .expect("same original seal after pointer restoration");
        assert_eq!(original.target_test_identity(), identities);
    });
}

/// The same request UUID cannot authorize publication when the actual journal loses Aborted or changes old configuration.
/// 实际日志失去 Aborted 或改变旧配置时，相同请求 UUID 不能授权发布。
/// No inputs or return value; exact original bytes are restored before assertions and no callback or owner changes.
/// 无输入或返回值；断言前恢复精确原字节，且不改变回调或所有者。
#[test]
fn predecessor_publication_rejects_actual_original_phase_and_complete_old_selection_changes() {
    // Genuine original restoration provides the actual journal namespace through its retained sidecar path.
    // 真正原恢复通过其保留附属记录路径提供实际日志命名空间。
    let (fixture, store, transaction, restoration, source) = publication_fixture();
    // The sole production sidecar resolver supplies an existing physical path tied to this same reservation.
    // 唯一生产附属记录解析器提供绑定此相同预留权的既有物理路径。
    let pending_path = restoration
        .private_owner_test_path(true)
        .expect("actual Pending path");
    // predecessor_handoff_directory is a child of transaction_directory in the authoritative store implementation.
    // 权威存储实现中，predecessor_handoff_directory 为 transaction_directory 的子目录。
    // Reuse those actual ancestors and the original UUID, never another namespace or candidate path.
    // 复用这些实际祖先及原 UUID，绝不使用另一命名空间或候选路径。
    let journal_path = pending_path
        .parent()
        .expect("actual sidecar directory")
        .parent()
        .expect("actual original transaction directory")
        .join(format!("{}.json", transaction.record().request_id()));
    // Read the uniquely derived existing journal and confirm the complete Aborted value before any fault.
    // 任何故障前，读取唯一派生既有日志并确认完整 Aborted 值。
    let original_bytes = std::fs::read(&journal_path).expect("actual original journal bytes");
    // Parsing the exact existing file prevents a guessed path from satisfying this negative test.
    // 解析精确既有文件，避免猜测路径满足此负例测试。
    let actual: crate::VulcanPluginActivationTransactionRecord =
        serde_json::from_slice(&original_bytes).expect("actual typed original journal");
    assert_eq!(&actual, transaction.record());
    assert_eq!(
        actual.phase(),
        crate::VulcanPluginActivationTransactionPhase::Aborted
    );
    // Preserve the unchanged Pending bytes independently of the original journal fault.
    // 独立于原日志故障，保留未变 Pending 字节。
    let pending_bytes = std::fs::read(&pending_path).expect("actual Pending bytes");
    // Actual independent file contention must survive every rejected corrupted observation.
    // 实际独立文件争用必须跨每次被拒绝损坏观测存续。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual retained lease path"),
        )
        .expect("actual probe");
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("actual old Ready")
        );
        // Capture the genuine original resources before introducing only journal observation faults.
        // 仅引入日志观测故障前，捕获真正原资源。
        let original = initialized_hook(&fixture);
        // Resource IDs remain separate from original request UUID and exact selection authority.
        // 资源身份独立于原请求 UUID 及精确选择权威。
        let identities = original.target_test_identity();
        // Each branch modifies one declared original field while preserving the same UUID and actual owner.
        // 每个分支修改一个声明原字段，同时保留相同 UUID 及实际所有者。
        for changed_configuration in [true, false] {
            // Serialize the verified existing typed record instead of assembling a replacement request.
            // 序列化已校验既有类型化记录，而非组装替代请求。
            let mut changed = serde_json::to_value(&actual).expect("actual record JSON");
            if changed_configuration {
                // The complete predecessor field is declared by ActivationTransactionRecord's actual serde contract.
                // 完整 predecessor 字段由 ActivationTransactionRecord 实际 serde 契约声明。
                let old = changed
                    .get_mut("predecessor")
                    .expect("declared full predecessor");
                *old.pointer_mut("/settings/configuration")
                    .expect("declared original configuration") =
                    serde_json::to_value(VulcanPluginConfigurationSelection::Document {
                        value: json!({"label":"different-old-selection"}),
                    })
                    .expect("typed changed old document");
            } else {
                *changed.get_mut("phase").expect("declared original phase") =
                    serde_json::to_value(crate::VulcanPluginActivationTransactionPhase::Prepared)
                        .expect("typed changed phase");
            }
            // The changed record remains well typed with its exact original request ID.
            // 改变后记录仍保持正确类型及精确原请求身份。
            let changed_record: crate::VulcanPluginActivationTransactionRecord =
                serde_json::from_value(changed.clone()).expect("typed journal fault");
            assert_eq!(changed_record.request_id(), actual.request_id());
            assert_ne!(changed_record, actual);
            std::fs::write(
                &journal_path,
                serde_json::to_vec(&changed).expect("changed actual journal bytes"),
            )
            .expect("write only the actual original fixture journal");
            // This observation counter must remain false because full original validation precedes publication.
            // 此观测计数器必须保持假，因为完整原请求校验先于发布。
            let mut callback = false;
            // Capture the rejection before restoring original bytes, so later assertions cannot leak a fault.
            // 恢复原字节之前捕获拒绝，使后续断言不会泄漏故障。
            let rejected = fixture.manager.publish_activation_predecessor_handoff(
                &transaction,
                &source,
                |_| {
                    callback = true;
                    Ok(())
                },
            );
            std::fs::write(&journal_path, &original_bytes)
                .expect("restore exact original journal bytes");
            assert!(rejected.is_err());
            assert!(
                !callback,
                "changed full original must reject before the publisher"
            );
            assert!(Arc::ptr_eq(
                &fixture.manager.snapshot(),
                source.staged_catalog()
            ));
            assert_eq!(original.target_test_identity(), identities);
            assert_eq!(
                std::fs::read(&pending_path).expect("same Pending bytes"),
                pending_bytes
            );
            assert!(matches!(
                competing.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            assert_eq!(
                store
                    .activation("hook-plugin")
                    .expect("unchanged real old selection"),
                transaction.record().predecessor().cloned()
            );
        }
        // Restoring exact existing journal bytes permits only the same original resource observation.
        // 恢复精确既有日志字节后，仅允许相同原资源观测。
        assert!(
            fixture
                .manager
                .prewarm_activation_predecessor_hook(&source)
                .await
                .expect("same original source after exact byte restoration")
        );
        assert_eq!(original.target_test_identity(), identities);
        assert_eq!(
            std::fs::read(journal_path).expect("actual restored journal"),
            original_bytes
        );
        assert!(
            store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("original Pending remains")
        );
    });
}
