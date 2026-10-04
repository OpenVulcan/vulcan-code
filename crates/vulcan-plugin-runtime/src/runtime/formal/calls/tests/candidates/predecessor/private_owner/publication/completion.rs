//! Actual no-global-MCP completion transfers the same old Hook and uniquely releases its independent reservation.
//! 实际无全局 MCP 完成转交相同旧 Hook，并唯一释放其独立预留权。

use super::*;

mod activity;

/// Abandoning an undelivered real prewarm waiter after actual transfer must not cancel the same published old Hook.
/// 实际转交后放弃尚未交付的真实预热等待者，不得取消相同已发布旧 Hook。
/// No inputs or return value; actual background preparation, core observations and old business supply all readiness evidence.
/// 无输入或返回值；实际后台准备、核心观测及旧业务提供全部就绪凭据。
#[test]
fn predecessor_completion_pending_prewarm_drop_preserves_transferred_old_hook() {
    // Reuse the real original immutable source without adding a second initialization or restoration ledger.
    // 复用真实原不可变来源，不增加第二次初始化或恢复账本。
    let (fixture, store, transaction, _restoration, source) = publication_fixture();
    executor().block_on(async {
        // The executor's one actual blocking slot prevents preparation from completing inside the first waiter poll.
        // 执行器唯一实际阻塞槽位防止准备在等待者首次轮询内完成。
        let (release, blocked_until_release) = std::sync::mpsc::channel();
        // This entry acknowledgement proves the actual slot is occupied before the original prewarm is polled.
        // 此入场确认在原预热被轮询前证明实际槽位已被占用。
        let (entered, slot_entered) = tokio::sync::oneshot::channel();
        // The actual task only delays Tokio preparation and never supplies fabricated Hook or native readiness.
        // 实际任务仅延迟 Tokio 准备，绝不提供伪造 Hook 或原生就绪。
        let blocker = tokio::task::spawn_blocking(move || {
            entered.send(()).expect("acknowledge actual blocking slot");
            blocked_until_release
                .recv_timeout(WAIT)
                .expect("actual preparation slot released");
        });
        tokio::time::timeout(WAIT, slot_entered)
            .await
            .expect("finite blocking slot entry")
            .expect("actual slot entered");
        // This is the actual public prewarm future; it stays stored and is never repolled after initial registration.
        // 此为实际公共预热 Future；初始注册之后持续保存，绝不再次轮询。
        let mut pending_waiter =
            Box::pin(fixture.manager.prewarm_activation_predecessor_hook(&source));
        std::future::poll_fn(|context| {
            assert!(std::future::Future::poll(pending_waiter.as_mut(), context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        // Read the actual newly registered allocation before permitting its real background preparation.
        // 允许真实后台准备之前，读取实际新登记分配。
        let original = initialized_hook(&fixture);
        assert!(original.target_test_identity().3);
        release
            .send(())
            .expect("allow only real preparation to proceed");
        blocker.await.expect("actual slot blocker joined");
        // Retain the actual parent solely for read-only core session and initialization observations.
        // 仅为只读核心会话及初始化观测保留实际父级。
        let owner = fixture.owner();
        tokio::time::timeout(WAIT, async {
            loop {
                // Optional identities reflect the actual preparation stages, not guessed readiness or substitute owners.
                // 可选身份反映实际准备阶段，而非猜测就绪或替代所有者。
                let observed = original.target_test_identity();
                if let (Some(pool), Some(session), Some(initialization)) =
                    (&observed.0, &observed.1, &observed.2)
                {
                    // The exact recorded initialization operation must actually succeed before publication readiness is checked.
                    // 检查发布就绪之前，精确已记录初始化操作必须实际成功。
                    let operation = owner
                        .host
                        .runtime()
                        .operation(initialization)
                        .expect("actual initialization handle")
                        .snapshot()
                        .expect("actual initialization snapshot");
                    if operation.phase.is_terminal() {
                        assert_eq!(
                            operation.phase,
                            luaskills::runtime::embedded::OperationPhase::Succeeded
                        );
                        assert!(operation.error.is_none());
                        // The same actual core session must be Ready and idle, independently of the retained waiting future.
                        // 相同实际核心会话必须就绪且空闲，独立于保留等待 Future。
                        let actual_session = owner
                            .host
                            .runtime()
                            .session(session)
                            .expect("actual original session");
                        if actual_session.phase
                            == luaskills::runtime::embedded::EmbeddedSessionPhase::Ready
                            && actual_session.active_operation.is_none()
                            && actual_session.queued_calls == 0
                        {
                            assert_eq!(&actual_session.pool_id, pool);
                            // This existing read-only preflight also observes background preparation's final state publication.
                            // 此既有只读预检也观测后台准备最终状态发布。
                            // The actual preflight requires the existing registration exclusion even for read-only observation.
                            // 即使只读观测，实际预检也要求既有注册互斥。
                            let readiness = {
                                // Keep this real synchronous gate only around physical readiness, never across an await.
                                // 仅围绕物理就绪保持此真实同步门，绝不跨 await 持有。
                                let _registration = fixture
                                    .manager
                                    .state
                                    .session_registration_gate
                                    .lock()
                                    .expect("actual readiness registration gate");
                                fixture
                                    .manager
                                    .activation_predecessor_hook_for_publication(&source)
                            };
                            match readiness {
                                Ok(Some(ready)) => {
                                    assert!(Arc::ptr_eq(&ready, &original));
                                    break;
                                }
                                Ok(None) => panic!("the actual source declares its original Hook"),
                                Err(error) => assert_eq!(
                                    error.message,
                                    "Original private target Hook is not ready for handoff.",
                                    "unexpected physical readiness failure"
                                ),
                            }
                        }
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("real background old Hook becomes Ready without delivering the retained waiter");
        // Capture the same actual pool/session/initialization identities after independent physical readiness observation.
        // 独立物理就绪观测之后，捕获相同实际池／会话／初始化身份。
        let identities = original.target_test_identity();
        assert!(identities.3 && !identities.4);
        // Observe the original actual domain allocation without introducing another strong physical owner.
        // 观测原实际域分配，不引入另一强物理所有者。
        let domain = owner
            .host
            .target_test_domain_identity(identities.0.as_deref().expect("actual original pool"));
        // This bundle has no initializer UI event; exact-pool retained operations are the real initialization evidence.
        // 此包没有初始化 UI 事件；精确池保留操作才是真实初始化凭据。
        let initialization_operations = owner
            .host
            .runtime()
            .list_operations(
                identities.0.as_deref(),
                None,
                owner.settings.parent.max_operations,
            )
            .expect("complete actual original pool operation page");
        assert!(!initialization_operations.has_more);
        assert_eq!(
            initialization_operations.operation_ids,
            vec![
                identities
                    .2
                    .clone()
                    .expect("actual original initialization ID")
            ]
        );
        // The actual context explicitly distinguishes fixed-session opening from ordinary exports and additional prewarming.
        // 实际上下文明确定义固定会话开启，区分普通导出及额外预热。
        let initialization_context = owner
            .host
            .runtime()
            .operation(identities.2.as_deref().expect("actual opening operation"))
            .expect("actual retained initialization")
            .snapshot()
            .expect("actual initialization authority")
            .context;
        match initialization_context {
            luaskills::runtime::embedded::OperationContext::Module(context) => {
                assert_eq!(Some(context.pool_id.as_str()), identities.0.as_deref());
                assert_eq!(
                    context.caller.session_id.as_deref(),
                    identities.1.as_deref()
                );
                assert!(context.export.is_none() && !context.prewarm);
            }
            luaskills::runtime::embedded::OperationContext::Unbound => {
                panic!("the actual fixed-session initialization must retain its module authority")
            }
        }
        // Full publication consumes only actual Ready owners, while the original prewarm future remains undelivered.
        // 原预热 Future 仍未交付时，完整发布仅消费实际就绪所有者。
        let publication = fixture
            .manager
            .publish_activation_predecessor_handoff(&transaction, &source, |catalog| {
                assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
                Ok(())
            })
            .expect("actual Full with pending original prewarm waiter");
        fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect("real original transfer before prewarm delivery");
        drop(pending_waiter);
        // The old waiter's cancellation must preserve the same now-published registry allocation and actual core identities.
        // 旧等待者取消必须保留相同现已发布注册表分配及实际核心身份。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&transferred.0, &transferred.1, &transferred.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!transferred.3 && !transferred.4);
        assert!(Arc::ptr_eq(&original, &initialized_hook(&fixture)));
        assert_eq!(
            owner
                .host
                .target_test_domain_identity(identities.0.as_deref().expect("same actual pool")),
            domain
        );
        assert_eq!(
            store
                .predecessor_handoff_record("hook-plugin", source.request_id())
                .expect("actual durable transfer")
                .expect("same original record")
                .phase(),
            crate::VulcanPluginPredecessorHandoffPhase::Transferred
        );
        // Before admitting new business, the exhaustive page must retain exactly the same sole initialization operation.
        // 接纳新业务之前，完整操作页必须精确保留相同唯一初始化操作。
        let after_waiter_drop = owner
            .host
            .runtime()
            .list_operations(
                identities.0.as_deref(),
                None,
                owner.settings.parent.max_operations,
            )
            .expect("actual original operation page after transfer and cancellation");
        assert!(!after_waiter_drop.has_more);
        assert_eq!(
            after_waiter_drop.operation_ids,
            initialization_operations.operation_ids
        );
        // Ordinary invocation itself checks the actual closing flag, proving cancellation did not close the transferred VM.
        // 普通调用自身检查实际关闭标志，证明取消没有关闭已转交虚拟机。
        let hook = publication
            .catalog()
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual published old Hook");
        assert_eq!(
            fixture
                .manager
                .invoke_formal_hook(
                    hook,
                    json!({"payload_json":"{}", "host_timeout_ms":1000}),
                    None,
                    Instant::now() + WAIT
                )
                .await
                .expect("same old business survives pending waiter drop"),
            json!({})
        );
        assert_eq!(original.target_test_identity(), transferred);
        assert_eq!(
            owner
                .host
                .runtime()
                .operation(
                    identities
                        .2
                        .as_deref()
                        .expect("same initialization operation after business")
                )
                .expect("actual original initialization remains retained")
                .snapshot()
                .expect("actual original initialization still succeeded")
                .phase,
            luaskills::runtime::embedded::OperationPhase::Succeeded
        );
        assert_eq!(
            owner
                .host
                .runtime()
                .session(identities.1.as_deref().expect("same actual session"))
                .expect("same session after business")
                .phase,
            luaskills::runtime::embedded::EmbeddedSessionPhase::Ready
        );
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
    });
}

/// Closing the actual parent after sealed publication must reject joint transfer and retain the original Pending File.
/// 封闭发布后关闭实际父级必须拒绝联合转交，并保留原 Pending 文件。
/// No inputs or return value; real close may retire the Hook, so assertions do not assume its later physical lifetime.
/// 无输入或返回值；真实关闭可能退役 Hook，因此断言不假定其后续物理寿命。
#[test]
fn predecessor_completion_rejects_actual_closed_parent_without_consuming_pending() {
    // The existing immutable restoration path creates every original owner through actual production APIs.
    // 既有不可变恢复路径通过实际生产 API 创建每个原所有者。
    let (fixture, store, transaction, restoration, source) = publication_fixture();
    // Capture the existing Pending sidecar while its independent original reservation is physically retained.
    // 独立原预留权仍被物理保留时，捕获既有 Pending 附属记录。
    let pending_path = restoration
        .private_owner_test_path(true)
        .expect("actual original Pending sidecar");
    // Exact byte equality proves that actual closure cannot consume or rewrite the original handoff decision.
    // 精确字节相等证明实际关闭不能消费或改写原转交决定。
    let pending_bytes = std::fs::read(&pending_path).expect("actual Pending bytes");
    // An independently opened descriptor observes the same actual original lease without inventing another reservation.
    // 独立打开的描述符观测相同实际原租约，不编造另一预留权。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual independent original lease"),
        )
        .expect("actual competing descriptor");
    // The complete original activation decision must remain Aborted despite closure and rejected transfer.
    // 即使关闭及转交被拒绝，完整原激活决定仍须保持 Aborted。
    let aborted = transaction.record().clone();
    executor().block_on(async {
        // Only an actually initialized old Hook can produce this successful exact final publication seal.
        // 只有实际初始化的旧 Hook 才能产生此成功精确最终发布凭据。
        let publication = ready_publication(&fixture, &transaction, &source).await;
        // Capture actual initialized identity before close; later asynchronous retirement is explicitly permitted.
        // 关闭前捕获实际初始化身份；明确允许后续异步退役。
        let identities = initialized_hook(&fixture).target_test_identity();
        assert!(identities.0.is_some() && identities.1.is_some() && identities.2.is_some());
        assert!(identities.3 && !identities.4);
        // Keep the genuinely attached parent for actual core closing observations, not a synthetic flag.
        // 保留真正附加父级以观测实际核心关闭状态，而非合成标志。
        let owner = fixture.owner();
        fixture
            .manager
            .request_formal_runtime_close()
            .expect("actual parent close request");
        assert!(
            owner
                .host
                .runtime()
                .usage()
                .expect("actual core usage")
                .closing
        );
        // The exact original seal remains current; rejection must specifically arise from the actually closed parent.
        // 精确原封闭凭据仍为当前值；拒绝必须具体来自实际已关闭父级。
        let rejected = fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect_err("closed actual parent cannot transfer");
        assert_eq!(
            rejected.message,
            "Predecessor handoff cannot transfer a closed formal parent."
        );
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        assert_eq!(
            std::fs::read(&pending_path).expect("retained Pending bytes"),
            pending_bytes
        );
        assert_eq!(
            store
                .predecessor_handoff_record("hook-plugin", source.request_id())
                .expect("actual durable handoff observation")
                .expect("same original record remains")
                .phase(),
            crate::VulcanPluginPredecessorHandoffPhase::Pending
        );
        assert!(restoration.private_owner_test_path(false).is_ok());
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(
            store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("closed parent retains independent original fence")
        );
        assert_eq!(
            store
                .activation_transaction("hook-plugin", aborted.request_id())
                .expect("original activation journal")
                .expect("complete original remains"),
            aborted
        );
        assert_eq!(transaction.record(), &aborted);
        // Actual permanent parent closure and the retained independent owner must not admit published old business.
        // 实际永久父级关闭及保留独立所有者不得接纳已发布旧业务。
        let hook = publication
            .catalog()
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual declared old Hook");
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
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert_eq!(
            std::fs::read(&pending_path).expect("business rejection preserves Pending"),
            pending_bytes
        );
    });
}

/// Prepare source's real old Hook and accept its exact final catalog using the core synchronous publisher.
/// 准备 source 真实旧 Hook，并使用核心同步发布器接纳其精确最终目录。
/// fixture and transaction are original production owners; return a real seal without synthetic native participants.
/// fixture 及 transaction 为原生产所有者；返回真实密封值，不合成原生参与者。
async fn ready_publication(
    fixture: &Fixture,
    transaction: &VulcanPluginActivationTransaction,
    source: &Arc<VulcanPluginPredecessorRestorationSource>,
) -> crate::VulcanPluginActivationPredecessorPublication {
    assert!(
        source.plugin().mcp_servers.is_empty(),
        "fixture has no global native MCP participant"
    );
    assert!(
        fixture
            .manager
            .prewarm_activation_predecessor_hook(source)
            .await
            .expect("real old Hook initialization")
    );
    fixture
        .manager
        .publish_activation_predecessor_handoff(transaction, source, |catalog| {
            assert!(Arc::ptr_eq(&catalog, &fixture.manager.snapshot()));
            Ok(())
        })
        .expect("actual successful core publication")
}

/// True joint completion transfers the same actual VM and releases the independent File while the original stays Aborted.
/// 真实联合完成转交相同实际虚拟机并释放独立文件，同时原请求保持 Aborted。
/// No inputs or return value; real old Hook business proves the old module, without claiming native MCP or service Full.
/// 无输入或返回值；真实旧 Hook 业务证明旧模块，不宣称原生 MCP 或服务 Full。
#[test]
fn predecessor_completion_transfers_actual_old_hook_and_releases_unique_reservation() {
    // Existing preparation owns the real old configuration, independent lease and original aborted transaction.
    // 既有准备拥有真实旧配置、独立租约及原中止事务。
    let (fixture, store, transaction, restoration, source) = publication_fixture();
    // The actual path must be captured before completion takes the unique original reservation.
    // 完成取走唯一原预留权之前，必须捕获实际路径。
    let lease_path = restoration
        .private_owner_test_path(false)
        .expect("original independent lease path");
    // This descriptor independently observes the actual operating-system exclusive File lifetime.
    // 此描述符独立观测实际操作系统排他文件寿命。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lease_path)
        .expect("actual independent probe");
    // Keep the complete Aborted record for strict durable equality after actual transfer.
    // 保留完整 Aborted 记录，以便实际转交后严格比较持久相等性。
    let aborted = transaction.record().clone();
    executor().block_on(async {
        // The successful publication is produced only after actual old Hook initialization.
        // 成功发布仅在实际旧 Hook 初始化之后产生。
        let publication = ready_publication(&fixture, &transaction, &source).await;
        // Observe the original registered Hook rather than constructing a substitute instance.
        // 观测原已注册 Hook，而非构造替代实例。
        let original = initialized_hook(&fixture);
        // These actual core IDs stay unchanged when ownership changes from private to published.
        // 所有权从私有变为已发布时，这些实际核心身份保持不变。
        let identities = original.target_test_identity();
        assert!(identities.3 && !identities.4);
        // Observe the same actual native domain without retaining a replacement owner.
        // 观测相同实际原生域，不保留替代所有者。
        let domain = fixture
            .owner()
            .host
            .target_test_domain_identity(identities.0.as_deref().expect("actual pool"));
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(
            store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("original Pending fence")
        );
        fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect("actual no-global-MCP joint completion");
        // Canonical durable evidence confirms transfer separately from the unchanged aborted activation decision.
        // 规范持久凭据独立于未变中止激活决定，确认转交。
        let transferred_record = store
            .predecessor_handoff_record("hook-plugin", source.request_id())
            .expect("actual handoff evidence")
            .expect("actual transfer record");
        assert_eq!(
            transferred_record.phase(),
            crate::VulcanPluginPredecessorHandoffPhase::Transferred
        );
        assert_eq!(transferred_record.request_id(), aborted.request_id());
        assert_eq!(
            store
                .activation_transaction("hook-plugin", aborted.request_id())
                .expect("actual original journal")
                .expect("actual original remains"),
            aborted
        );
        assert_eq!(transaction.record(), &aborted);
        assert!(
            restoration.private_owner_test_path(false).is_err(),
            "the same physical reservation slot was taken"
        );
        competing
            .try_lock()
            .expect("joint completion released the independent original File");
        competing
            .unlock()
            .expect("release only the observing lock before checking ordinary business");
        assert!(
            !store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("original writer fence actually reopened")
        );
        // Historical core identities retain the same real instances while the private-origin flag is cleared.
        // 历史核心身份保留相同真实实例，同时清除私有来源标志。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&transferred.0, &transferred.1, &transferred.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!transferred.3 && !transferred.4);
        assert!(Arc::ptr_eq(&original, &initialized_hook(&fixture)));
        assert_eq!(
            fixture
                .owner()
                .host
                .target_test_domain_identity(identities.0.as_deref().expect("same pool")),
            domain
        );
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        // Query the same actual live session and initialization rather than inferring readiness from Transferred.
        // 查询相同实际存活会话及初始化，不从 Transferred 推断就绪。
        let owner = fixture.owner();
        assert_eq!(
            owner
                .host
                .runtime()
                .session(identities.1.as_deref().expect("same actual session"))
                .expect("actual transferred session")
                .phase,
            luaskills::runtime::embedded::EmbeddedSessionPhase::Ready
        );
        assert_eq!(
            owner
                .host
                .runtime()
                .operation(
                    identities
                        .2
                        .as_deref()
                        .expect("same initialization operation")
                )
                .expect("actual original initialization")
                .snapshot()
                .expect("actual original observation")
                .phase,
            luaskills::runtime::embedded::OperationPhase::Succeeded
        );
        // The old callback succeeds; the deliberately distinct target module would throw instead.
        // 旧回调成功；刻意不同的目标模块则会抛出错误。
        let hook = publication
            .catalog()
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual published old Hook");
        assert_eq!(
            fixture
                .manager
                .invoke_formal_hook(
                    hook,
                    json!({"payload_json":"{}", "host_timeout_ms":1000}),
                    None,
                    Instant::now() + WAIT
                )
                .await
                .expect("actual old business on same VM"),
            json!({})
        );
        // Successful completion is terminal; retry must not reconstruct a source or release another owner.
        // 成功完成为终态；重试不得重建来源或释放另一所有者。
        let rejected = fixture.manager.complete_activation_predecessor_handoff(
            &transaction,
            &source,
            &publication,
        );
        assert!(rejected.is_err());
        assert_eq!(original.target_test_identity(), transferred);
        assert_eq!(
            store
                .predecessor_handoff_record("hook-plugin", aborted.request_id())
                .expect("unchanged terminal record")
                .expect("actual record remains"),
            transferred_record
        );
        assert_eq!(
            store
                .activation_transaction("hook-plugin", aborted.request_id())
                .expect("unchanged original after terminal retry")
                .expect("original remains"),
            aborted
        );
        competing
            .try_lock()
            .expect("terminal retry keeps the reservation File released");
        competing.unlock().expect("release final observer lock");
        assert!(
            crate::runtime::environment::lock_plugin_execution_environment_cleanup(
                &fixture.manager.state,
                "hook-plugin"
            )
            .is_err(),
            "actual published VM retains its own original environment owner"
        );
    });
}

/// Foreign managers, sources, originals, seals and final clones reject completion without consuming Pending.
/// 外来管理器、来源、原请求、密封值及最终克隆拒绝完成，不消费 Pending。
/// No inputs or return value; the exact original remains completable after all independently rejected substitutions.
/// 无输入或返回值；全部独立替换被拒绝后，精确原请求仍可完成。
#[test]
fn predecessor_completion_rejects_foreign_owners_and_catalog_before_transfer() {
    // Each source originates in its own actual immutable restoration flow.
    // 每个来源均来自其自有实际不可变恢复流程。
    let (fixture, store, transaction, restoration, source) = publication_fixture();
    // Genuine foreign owners can supply a real seal but cannot substitute its actual allocation identity.
    // 真正外来所有者可以提供真实密封值，但不能替换其实际分配身份。
    let (foreign, _foreign_store, foreign_transaction, _foreign_restoration, foreign_source) =
        publication_fixture();
    // Preserve exact existing Pending bytes before any completion is attempted.
    // 任何完成尝试之前，保留精确既有 Pending 字节。
    let pending_path = restoration
        .private_owner_test_path(true)
        .expect("actual Pending path");
    // Byte equality detects a forbidden durable phase change under rejected authority.
    // 字节相等性检测被拒绝权威下任何禁止的持久阶段变化。
    let pending_bytes = std::fs::read(&pending_path).expect("original Pending bytes");
    // The independent physical probe observes the same original retained File through all negative calls.
    // 独立物理探测跨全部负例调用观测相同原保留文件。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual original lease path"),
        )
        .expect("actual probe");
    executor().block_on(async {
        // Real private Hook initialization precedes both real synchronous publishers.
        // 真实私有 Hook 初始化先于两个真实同步发布器。
        let publication = ready_publication(&fixture, &transaction, &source).await;
        // The foreign seal is also produced through actual Ready and publication, never a fake constructor.
        // 外来密封值也通过实际就绪及发布产生，绝不使用伪构造器。
        let foreign_publication =
            ready_publication(&foreign, &foreign_transaction, &foreign_source).await;
        // Capture actual original VM identity before testing wrong completion inputs.
        // 测试错误完成输入之前，捕获实际原虚拟机身份。
        let original = initialized_hook(&fixture);
        // The same actual pool/session/operation must survive every failed authority substitution.
        // 相同实际池／会话／操作必须跨每次失败权威替换存续。
        let identities = original.target_test_identity();
        assert!(
            foreign
                .manager
                .complete_activation_predecessor_handoff(&transaction, &source, &publication)
                .is_err()
        );
        assert!(
            fixture
                .manager
                .complete_activation_predecessor_handoff(
                    &transaction,
                    &foreign_source,
                    &publication
                )
                .is_err()
        );
        assert!(
            fixture
                .manager
                .complete_activation_predecessor_handoff(
                    &foreign_transaction,
                    &source,
                    &publication
                )
                .is_err()
        );
        assert!(
            fixture
                .manager
                .complete_activation_predecessor_handoff(
                    &transaction,
                    &source,
                    &foreign_publication
                )
                .is_err()
        );
        // Equal final values cannot replace the exact current allocation accepted by the original publisher.
        // 相等最终值不能替换原发布器接受的精确当前分配。
        let cloned_final = Arc::new(publication.catalog().as_ref().clone());
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("test allocation substitution") = Arc::clone(&cloned_final);
        assert!(
            fixture
                .manager
                .complete_activation_predecessor_handoff(&transaction, &source, &publication)
                .is_err()
        );
        assert!(Arc::ptr_eq(&fixture.manager.snapshot(), &cloned_final));
        *fixture
            .manager
            .state
            .snapshot
            .write()
            .expect("restore exact accepted original Arc") = Arc::clone(publication.catalog());
        assert_eq!(
            std::fs::read(&pending_path).expect("no rejected writer changes"),
            pending_bytes
        );
        assert_eq!(original.target_test_identity(), identities);
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert_eq!(
            store
                .predecessor_handoff_record("hook-plugin", source.request_id())
                .expect("real still Pending")
                .expect("actual original record")
                .phase(),
            crate::VulcanPluginPredecessorHandoffPhase::Pending
        );
        fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect("same actual original remains completable");
        // Completion changes ownership only after the correctly matched original seal and source are used.
        // 仅使用正确匹配原密封值及来源之后，完成才改变所有权。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&transferred.0, &transferred.1, &transferred.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!transferred.3 && !transferred.4);
        competing
            .try_lock()
            .expect("correct original released its actual File");
        competing.unlock().expect("release observer lock");
    });
}

/// Unknown actual Transferred write keeps the same owners; exact read-only retry completes without another write.
/// 未知实际 Transferred 写入保留相同所有者；精确只读重试不再次写入地完成。
/// No inputs or return value; real disk, operating-system locks and old business distinguish Unknown from completion.
/// 无输入或返回值；真实磁盘、操作系统锁及旧业务区分未知状态与完成。
#[test]
fn predecessor_completion_unknown_transferred_retains_owner_and_retries_exact_read_only() {
    // Existing source preparation owns the genuine original independent reservation and old immutable inputs.
    // 既有来源准备拥有真正原独立预留权及旧不可变输入。
    let (fixture, store, transaction, restoration, source) = publication_fixture();
    // Retain the sole actual sidecar path before completion can release its physical reservation.
    // 完成可以释放物理预留权之前，保留唯一实际附属记录路径。
    let sidecar_path = restoration
        .private_owner_test_path(true)
        .expect("actual original sidecar path");
    // An independently opened descriptor proves owner retention rather than relying on in-memory flags alone.
    // 独立打开描述符证明所有者保留，而非仅依赖内存标志。
    let competing = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            restoration
                .private_owner_test_path(false)
                .expect("actual independent lease path"),
        )
        .expect("actual probe");
    // Full immutable Aborted values must remain unchanged through both Unknown and confirmed completion.
    // 完整不可变 Aborted 值必须跨未知及已确认完成保持不变。
    let aborted = transaction.record().clone();
    executor().block_on(async {
        // The actual old session becomes Ready before the real sealed core publication.
        // 实际旧会话在真实密封核心发布之前进入就绪。
        let publication = ready_publication(&fixture, &transaction, &source).await;
        // Capture the actual registered VM before the controlled storage acknowledgement loss.
        // 受控存储确认丢失前，捕获实际已注册虚拟机。
        let original = initialized_hook(&fixture);
        // These real identities remain independent of every durable phase observation.
        // 这些真实身份独立于每个持久阶段观测。
        let identities = original.target_test_identity();
        // The actual weak-domain observer reads the same pool allocation without creating another resource.
        // 实际弱域观测者读取相同池分配，不创建另一资源。
        let domain = fixture
            .owner()
            .host
            .target_test_domain_identity(identities.0.as_deref().expect("actual original pool"));
        // This existing fault loses immediate observation only after the real requested sidecar bytes were committed.
        // 此既有故障仅在真实请求附属字节提交后丢失即时观测。
        let loss = store.lose_predecessor_handoff_observation_once(source.request_id());
        // The actual joint writer returns uncertainty while retaining all prevalidated original resources.
        // 实际联合写入器返回不确定状态，同时保留全部已预校验原资源。
        let failure = fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect_err("actual transfer observation is unknown");
        drop(loss);
        assert!(
            failure
                .message
                .contains("Controlled loss of predecessor handoff immediate readback"),
            "{}",
            failure.message
        );
        // Canonical read confirms the real Transferred bytes, without claiming actual in-memory transfer.
        // 规范读取确认真实 Transferred 字节，不宣称实际内存转交。
        let transferred_record = store
            .predecessor_handoff_record("hook-plugin", source.request_id())
            .expect("canonical actual sidecar")
            .expect("actual Transferred record");
        assert_eq!(
            transferred_record.phase(),
            crate::VulcanPluginPredecessorHandoffPhase::Transferred
        );
        // Preserve committed bytes to detect forbidden rewriting during exact reconciliation.
        // 保留已提交字节，以检测精确对账期间禁止的重写。
        let transferred_bytes =
            std::fs::read(&sidecar_path).expect("real committed Transferred bytes");
        assert_eq!(original.target_test_identity(), identities);
        assert!(identities.3 && !identities.4);
        assert!(Arc::ptr_eq(
            publication.catalog(),
            &fixture.manager.snapshot()
        ));
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        assert!(
            store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("Unknown keeps actual owner fence")
        );
        assert_eq!(
            store
                .activation_transaction("hook-plugin", aborted.request_id())
                .expect("Unknown original journal")
                .expect("original remains"),
            aborted
        );
        // The enabled final catalog cannot admit old business while the same original transfer remains unconfirmed.
        // 相同原转交未确认时，启用最终目录不能接纳旧业务。
        let hook = publication
            .catalog()
            .enabled_hooks
            .values()
            .find(|hook| hook.plugin_id == "hook-plugin")
            .expect("actual declared old Hook");
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
        // If retry wrote another sidecar, this real existing writer obstruction would make it fail.
        // 若重试再次写入附属记录，此真实既有写入器阻挡将使其失败。
        let no_rewrite = store.obstruct_predecessor_handoff_write_once(source.request_id());
        fixture
            .manager
            .complete_activation_predecessor_handoff(&transaction, &source, &publication)
            .expect("same actual original confirms exact Transferred read-only");
        drop(no_rewrite);
        assert_eq!(
            std::fs::read(&sidecar_path).expect("unchanged exact Transferred bytes"),
            transferred_bytes
        );
        assert_eq!(
            store
                .predecessor_handoff_record("hook-plugin", source.request_id())
                .expect("same confirmed terminal record")
                .expect("actual record remains"),
            transferred_record
        );
        // Confirmation clears private ownership without changing any actual core instance identity.
        // 确认清除私有所有权，不改变任何实际核心实例身份。
        let transferred = original.target_test_identity();
        assert_eq!(
            (&transferred.0, &transferred.1, &transferred.2),
            (&identities.0, &identities.1, &identities.2)
        );
        assert!(!transferred.3 && !transferred.4);
        assert_eq!(
            fixture
                .owner()
                .host
                .target_test_domain_identity(identities.0.as_deref().expect("same actual pool")),
            domain
        );
        assert!(restoration.private_owner_test_path(false).is_err());
        competing
            .try_lock()
            .expect("confirmed actual memory transfer releases unique File");
        competing
            .unlock()
            .expect("release observer lock before ordinary business");
        assert!(
            !store
                .predecessor_handoff_blocks_admission("hook-plugin")
                .expect("confirmed admission fence reopened")
        );
        assert_eq!(
            fixture
                .manager
                .invoke_formal_hook(
                    hook,
                    json!({"payload_json":"{}", "host_timeout_ms":1000}),
                    None,
                    Instant::now() + WAIT
                )
                .await
                .expect("actual old callback after exact retry"),
            json!({})
        );
        assert_eq!(
            store
                .activation_transaction("hook-plugin", aborted.request_id())
                .expect("confirmed unchanged original journal")
                .expect("original remains"),
            aborted
        );
        assert_eq!(transaction.record(), &aborted);
    });
}
