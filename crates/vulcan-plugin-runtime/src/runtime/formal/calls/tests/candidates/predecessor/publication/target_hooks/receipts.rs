//! Durable original target receipts prevent replay after runtime or process ownership has disappeared.
//! 持久原目标回执在运行时或进程所有权消失后防止重放。

use super::*;

/// Reopening a real Started original request never submits a new initialization or allocates a registry instance.
/// 重开真实 Started 原请求绝不提交新初始化或分配注册表实例。
/// No inputs or result; actual disk and request-lock resume provide the restart boundary evidence.
/// 无输入与返回值；实际磁盘及请求锁恢复提供重启边界证据。
#[test]
fn formal_private_target_hook_started_receipt_survives_store_reopen_without_replay() {
    // Prepare and publish the real complete selected target before reserving its route.
    // 预留路由之前，准备并发布真实完整已选目标。
    let (fixture, store, transaction) = published_enabled_target();
    // The exact production staging Arc supplies the actual route generation.
    // 精确生产暂存 Arc 提供实际路由代次。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("actual stage");
    // Persist actual Started without a fabricated core identity, modeling interruption before submission acknowledgement.
    // 持久保存实际 Started，不虚构核心身份，模拟提交确认之前的中断。
    let owner = transaction
        .target_hook_initialization_owner(staged.plugins["hook-plugin"].generation)
        .expect("same original request owner");
    drop(owner.begin().expect("actual durable one-attempt boundary"));
    // Keep the exact original request UUID across physical lock release and store reopen.
    // 跨物理锁释放及存储重开保留精确原请求 UUID。
    let request_id = transaction.record().request_id();
    drop(owner);
    drop(transaction);
    // Reopen the existing store without inventing a different request or transaction index.
    // 重开既有存储，不虚构不同请求或事务索引。
    let reopened =
        crate::VulcanPluginGenerationStore::open_existing(&fixture.manager.state.user_plugin_root)
            .expect("actual reopen")
            .expect("original store");
    assert_eq!(
        reopened
            .target_hook_initialization_receipt("hook-plugin", request_id)
            .expect("read original receipt")
            .expect("Started original")
            .phase(),
        crate::VulcanPluginTargetHookInitializationPhase::Started
    );
    // Actual request recovery takes its original exclusive file after all previous owners were dropped.
    // 全部先前所有者已销毁后，实际请求恢复取得原排他文件。
    let mut resumed = reopened
        .resume_activation_transaction("hook-plugin", request_id)
        .expect("same original exclusive request");
    assert!(
        reopened
            .complete_activation_transaction(&mut resumed)
            .is_err(),
        "standalone completion cannot release a Started private target"
    );
    // Observe existing actual initialization events to detect any prohibited replay.
    // 观测既有实际初始化事件，以检测任何禁止重放。
    let count = target_initializations(&fixture);
    executor().block_on(async {
        assert!(
            fixture
                .manager
                .prewarm_activation_target_hook(&resumed, &staged)
                .await
                .is_err()
        );
    });
    assert_eq!(target_initializations(&fixture), count);
    assert!(
        fixture
            .owner()
            .hooks
            .lock()
            .expect("actual original registry")
            .is_empty()
    );
    assert_eq!(
        store
            .activation_request("hook-plugin", request_id)
            .expect("real request inspection")
            .expect("original record")
            .phase(),
        crate::VulcanPluginActivationTransactionPhase::Published
    );
}

/// Historical success cannot allocate another VM after the actual original private Hook has drained and been forgotten.
/// 实际原私有 Hook 已排空并遗忘后，历史成功不可分配另一 VM。
/// No inputs or result; real production drain and resume prove that durable replay refusal survives released locks.
/// 无输入与返回值；真实生产排空及恢复证明持久拒绝重放跨越已释放锁。
#[test]
fn formal_private_target_hook_succeeded_receipt_rejects_reinitialization_after_forget() {
    // Select the actual published request and its exact closed routing catalog.
    // 选择实际已发布请求及其精确关闭路由目录。
    let (fixture, store, transaction) = published_enabled_target();
    // Preserve the actual original staged Arc before initialization.
    // 初始化之前保留实际原暂存 Arc。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("actual stage");
    executor().block_on(async {
        fixture
            .manager
            .prewarm_activation_target_hook(&transaction, &staged)
            .await
            .expect("actual successful original initialization");
        // Success is durable while the actual original instance remains private and retained.
        // 实际原实例仍私有并保留时，成功已经持久保存。
        let request_id = transaction.record().request_id();
        assert_eq!(
            store
                .target_hook_initialization_receipt("hook-plugin", request_id)
                .expect("durable success")
                .expect("actual original attempt")
                .phase(),
            crate::VulcanPluginTargetHookInitializationPhase::Succeeded
        );
        // Capture actual VM initialization count before its real physical retirement.
        // 真实物理退役之前捕获实际 VM 初始化次数。
        let count = target_initializations(&fixture);
        retire_original_hook(&fixture).await;
        assert!(
            fixture
                .owner()
                .hooks
                .lock()
                .expect("actual forgotten registry")
                .is_empty()
        );
        drop(transaction);
        // Recover the real released original request instead of substituting a new request identity.
        // 恢复真实已释放原请求，不替换为新请求身份。
        let mut resumed = store
            .resume_activation_transaction("hook-plugin", request_id)
            .expect("actual request lock is released");
        assert!(
            store.complete_activation_transaction(&mut resumed).is_err(),
            "historical success after actual retirement still requires original domain handoff"
        );
        assert!(
            fixture
                .manager
                .prewarm_activation_target_hook(&resumed, &staged)
                .await
                .is_err()
        );
        assert_eq!(target_initializations(&fixture), count);
        assert!(
            fixture
                .owner()
                .hooks
                .lock()
                .expect("registry remains empty")
                .is_empty()
        );
    });
}

/// Strict receipt identity rejects corrupt inspection and an unrelated nonzero route without replaying the retained VM.
/// 严格回执身份拒绝损坏检查及无关非零路由，不重放保留 VM。
/// No inputs or result; restoring the original bytes permits the same successful VM to complete normally.
/// 无输入与返回值；恢复原字节允许相同成功 VM 正常完成。
#[test]
fn formal_private_target_hook_receipt_validates_full_target_version_and_exact_route() {
    // The actual transaction supplies complete selection and durable request evidence.
    // 实际事务提供完整选择及持久请求证据。
    let (fixture, store, mut transaction) = published_enabled_target();
    // Preserve the exact production route issued for this original full target.
    // 保留为此原完整目标签发的精确生产路由。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("actual stage");
    executor().block_on(async {
        fixture
            .manager
            .prewarm_activation_target_hook(&transaction, &staged)
            .await
            .expect("original initialization");
        // The strict sidecar path follows the original UUID in the existing request namespace.
        // 严格附属回执路径遵循既有请求命名空间中的原 UUID。
        let request_id = transaction.record().request_id();
        // Resolve only the declared original receipt path for controlled corruption testing.
        // 仅解析声明原回执路径，供受控损坏测试。
        let path = fixture
            .manager
            .state
            .user_plugin_root
            .join(".vulcan-generations/transactions/hook-plugin")
            .join(format!("{request_id}.target-hook"));
        // Keep original acknowledged bytes for exact restoration after each invalid shape.
        // 保留原已确认字节，在每个无效形状之后精确恢复。
        let bytes = std::fs::read(&path).expect("actual original receipt bytes");
        // Decode only fixture-owned bytes to vary strict receipt fields individually.
        // 仅解码夹具拥有字节，以单独变化严格回执字段。
        let original: Value = serde_json::from_slice(&bytes).expect("original typed receipt");
        for invalid in ["version", "target", "core", "unknown"] {
            // Change one exact declared field; do not build production fallback interpretations.
            // 更改单个精确声明字段；不建立生产兜底解释。
            let mut corrupt = original.clone();
            match invalid {
                "version" => corrupt["schema_version"] = json!(99),
                "target" => {
                    corrupt["target"]["revision"] =
                        json!(transaction.record().target().revision + 1)
                }
                "core" => {
                    corrupt
                        .as_object_mut()
                        .expect("receipt object")
                        .remove("core");
                }
                "unknown" => corrupt["unrecognized"] = json!(true),
                _ => unreachable!(),
            }
            std::fs::write(
                &path,
                serde_json::to_vec(&corrupt).expect("controlled invalid bytes"),
            )
            .expect("write invalid fixture sidecar");
            assert!(
                store
                    .target_hook_initialization_receipt("hook-plugin", request_id)
                    .is_err(),
                "strict receipt inspection rejects {invalid}"
            );
            assert!(
                store.activation_request("hook-plugin", request_id).is_err(),
                "original request inspection rejects {invalid}"
            );
            std::fs::write(&path, &bytes).expect("restore exact original bytes");
        }
        // A different valid nonzero route is not this retained actual pool/session/domain.
        // 不同有效非零路由并非此保留实际池／会话／域。
        let mut wrong_route = original.clone();
        wrong_route["routing_generation"] = json!(staged.plugins["hook-plugin"].generation + 1);
        std::fs::write(
            &path,
            serde_json::to_vec(&wrong_route).expect("wrong actual route bytes"),
        )
        .expect("write mismatched route");
        // Publication still refers to the real successful VM; completion performs exact durable route validation.
        // 发布仍指向真实成功 VM；完成执行精确持久路由校验。
        let publication = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
            .expect("actual complete extension catalog");
        // A failed durable boundary cannot confirm any participant or release original memory ownership.
        // 失败持久边界不能确认任何参与者或释放原内存所有权。
        let confirmed = AtomicBool::new(false);
        // Retain the original expectations before entering the scoped GAT adapter.
        // 进入受作用域约束 GAT 适配器之前保留原期望。
        let probe = CompletionProbe::new(&confirmed, &transaction, publication.catalog());
        assert!(
            fixture
                .manager
                .complete_activation_target_handoff_with(&mut transaction, &publication, &probe)
                .is_err()
        );
        assert!(!confirmed.load(Ordering::Acquire));
        assert!(Arc::ptr_eq(&fixture.manager.snapshot(), &staged));
        // The actual original domain and initialization count survive failed completion unchanged.
        // 实际原域及初始化次数跨失败完成保持不变。
        let count = target_initializations(&fixture);
        std::fs::write(&path, &bytes).expect("restore exact original successful receipt");
        fixture
            .manager
            .prewarm_activation_target_hook(&transaction, &staged)
            .await
            .expect("same retained successful VM");
        assert_eq!(target_initializations(&fixture), count);
        // Republish the exact full extension catalog before completing this same original authority.
        // 完成此相同原权威之前，重新发布精确完整扩展目录。
        let final_publication = fixture
            .manager
            .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
            .expect("same original republished extension catalog");
        fixture
            .manager
            .complete_activation_target_handoff(&mut transaction, &final_publication)
            .expect("same original confirmed handoff");
        retire_original_hook(&fixture).await;
    });
}
