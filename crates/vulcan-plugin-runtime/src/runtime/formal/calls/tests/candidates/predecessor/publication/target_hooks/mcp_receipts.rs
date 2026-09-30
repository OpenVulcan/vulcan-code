//! Actual declared MCP targets reject missing participants and durable startup replay.
//! 实际声明 MCP 的目标拒绝缺失参与者及持久启动重放。

use super::*;

/// An actual global MCP declaration cannot complete through either the empty wrapper or an evidence-free participant.
/// 实际全局 MCP 声明不能通过空包装器或无证据参与者完成。
/// No inputs or return value; the actual original request remains Published and no confirmation runs.
/// 无输入与返回值；实际原请求保持 Published，且不执行完成确认。
#[test]
fn formal_private_target_mcp_noop_participant_cannot_complete_declared_global() {
    // Prepare the immutable declaration before the real package approval and selection.
    // 在真实包审批及选择之前准备不可变声明。
    let (fixture, store, mut transaction) = published_enabled_target_surfaces(false, true);
    // Reserve the actual closed route for the original full target.
    // 为原完整目标预留实际关闭路由。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("actual global MCP target stage");
    assert!(staged.plugins["hook-plugin"].hooks.is_empty());
    assert!(
        staged.plugins["hook-plugin"]
            .mcp_servers
            .iter()
            .any(|server| server.manifest.scope == crate::VulcanPluginMcpScopeManifest::Global)
    );
    // Publish the actual catalog without claiming a transport or Ready observation.
    // 发布实际目录，不声称存在传输或 Ready 观测。
    let publication = fixture
        .manager
        .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
        .expect("actual complete catalog");
    assert!(
        fixture
            .manager
            .complete_activation_target_handoff(&mut transaction, &publication)
            .is_err()
    );
    // Even a caller-supplied participant cannot bypass the genuine source evidence requirement.
    // 即使调用方提供参与者，也不能绕过真实来源证据要求。
    let confirmed = AtomicBool::new(false);
    // The evidence-free adapter has no registered issuer and cannot prepare a global target.
    // 无证据适配器没有已注册签发者，不能准备全局目标。
    let probe = CompletionProbe::new(&confirmed, &transaction, publication.catalog());
    assert!(
        fixture
            .manager
            .complete_activation_target_handoff_with(&mut transaction, &publication, &probe)
            .is_err()
    );
    assert!(!confirmed.load(Ordering::Acquire));
    assert_eq!(
        store
            .activation_request("hook-plugin", transaction.record().request_id())
            .expect("strict original request")
            .expect("original request")
            .phase(),
        crate::VulcanPluginActivationTransactionPhase::Published
    );
    assert!(
        store
            .target_mcp_initialization_receipt("hook-plugin", transaction.record().request_id())
            .expect("no attempted transport")
            .is_none()
    );
}

/// Strict original startup evidence rejects duplicate attempts and survives store reopening without transport creation.
/// 严格原启动证据拒绝重复尝试，并在重开存储后保留，不创建传输。
/// No inputs or return value; this tests the durable admission boundary, not actual network readiness.
/// 无输入与返回值；此测试验证持久准入边界，不验证实际网络就绪。
#[test]
fn formal_private_target_mcp_started_receipt_is_strict_and_cannot_replay_after_reopen() {
    // Use the production-selected immutable global MCP declaration.
    // 使用生产选择的不可变全局 MCP 声明。
    let (fixture, store, mut transaction) = published_enabled_target_surfaces(false, true);
    // Preserve the actual original staged route rather than inventing a generation.
    // 保留实际原暂存路由，不虚构代次。
    let staged = fixture
        .manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from(["hook-plugin".into()]),
        )
        .expect("actual route");
    // Retain the original request lock and persist Started before any transport construction.
    // 保留原请求锁，并在任何传输构造之前持久保存 Started。
    let owner = transaction
        .target_mcp_initialization_owner(staged.plugins["hook-plugin"].generation)
        .expect("same original owner");
    owner.require_unattempted().expect("first admission");
    // This actual pre-construction writer has no runtime or lease identities.
    // 此实际构造前写入者没有运行时或租约身份。
    let mut attempt = owner.begin().expect("durable Started");
    assert!(owner.begin().is_err());
    assert!(owner.require_unattempted().is_err());
    assert!(
        attempt.finish(true).is_err(),
        "Ready requires actual constructed owners"
    );
    assert!(
        store
            .complete_activation_transaction(&mut transaction)
            .is_err(),
        "legacy completion cannot discard private startup ownership"
    );
    // The exact original UUID supplies the existing sidecar path and request lookup.
    // 精确原 UUID 提供既有附属回执路径及请求查询。
    let request_id = transaction.record().request_id();
    // Read original acknowledged bytes for exact restoration after each invalid shape.
    // 读取原已确认字节，以在每个无效形状之后精确恢复。
    let path = fixture
        .manager
        .state
        .user_plugin_root
        .join(".vulcan-generations/transactions/hook-plugin")
        .join(format!("{request_id}.target-mcp"));
    // Preserve the real Started bytes rather than writing a replacement valid owner.
    // 保留真实 Started 字节，不写入替代有效所有者。
    let bytes = std::fs::read(&path).expect("actual Started bytes");
    // Corruption tests decode only fixture-owned strict sidecar bytes.
    // 损坏测试仅解码夹具拥有的严格附属回执字节。
    let original: Value = serde_json::from_slice(&bytes).expect("actual receipt JSON");
    for invalid in [
        "version",
        "target",
        "unknown",
        "instances",
        "ready",
        "nil",
        "request_nonce",
        "same_nonce",
        "noncanonical",
    ] {
        // Each controlled corruption changes one independently validated identity or format rule.
        // 每个受控损坏更改一项独立校验的身份或格式规则。
        let mut corrupt = original.clone();
        match invalid {
            "version" => corrupt["schema_version"] = json!(99),
            "target" => {
                corrupt["target"]["revision"] = json!(transaction.record().target().revision + 1)
            }
            "unknown" => corrupt["unrecognized"] = json!(true),
            "instances" => {
                corrupt
                    .as_object_mut()
                    .expect("receipt object")
                    .remove("instances");
            }
            "ready" => corrupt["phase"] = json!("ready"),
            "nil" => {
                corrupt["instances"] = json!({"runtime_instance_id": Uuid::nil(), "partition_lease_instance_id": Uuid::new_v4()})
            }
            "request_nonce" => {
                corrupt["instances"] = json!({"runtime_instance_id": request_id, "partition_lease_instance_id": Uuid::new_v4()})
            }
            "same_nonce" => {
                // This invalid fixture nonce is unrelated to the original request, isolating duplicate owner rejection.
                // 此无效夹具随机标识与原请求无关，独立验证重复所有者拒绝。
                let duplicate = Uuid::new_v4();
                corrupt["instances"] = json!({"runtime_instance_id": duplicate, "partition_lease_instance_id": duplicate});
            }
            "noncanonical" => {
                corrupt["instances"] = json!({"runtime_instance_id": "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA", "partition_lease_instance_id": Uuid::new_v4()})
            }
            _ => unreachable!(),
        }
        std::fs::write(
            &path,
            serde_json::to_vec(&corrupt).expect("invalid fixture bytes"),
        )
        .expect("controlled invalid sidecar");
        assert!(
            store
                .target_mcp_initialization_receipt("hook-plugin", request_id)
                .is_err(),
            "strict MCP receipt rejects {invalid}"
        );
        assert!(
            store.activation_request("hook-plugin", request_id).is_err(),
            "strict original request rejects {invalid}"
        );
        std::fs::write(&path, &bytes).expect("exact original receipt restoration");
    }
    // Duplicate format fields are invalid even when both values individually match the supported version.
    // 即使两个值各自符合支持版本，重复格式字段仍无效。
    let duplicate = String::from_utf8(bytes.clone())
        .expect("receipt UTF-8")
        .replacen('{', "{\"schema_version\":1,", 1);
    std::fs::write(&path, duplicate).expect("controlled duplicate format field");
    assert!(
        store
            .target_mcp_initialization_receipt("hook-plugin", request_id)
            .is_err()
    );
    assert!(store.activation_request("hook-plugin", request_id).is_err());
    std::fs::write(&path, &bytes).expect("restore after duplicate field");
    assert_eq!(
        store
            .target_mcp_initialization_receipt("hook-plugin", request_id)
            .expect("strict Started")
            .expect("actual attempt")
            .phase(),
        crate::VulcanPluginTargetMcpInitializationPhase::Started
    );
    drop(attempt);
    drop(owner);
    drop(transaction);
    // Reopen the same real store and acquire the original released request file.
    // 重开相同真实存储，并取得原已释放请求文件。
    let reopened =
        crate::VulcanPluginGenerationStore::open_existing(&fixture.manager.state.user_plugin_root)
            .expect("reopen")
            .expect("original store");
    // Resuming the durable request does not erase its irreversible startup admission.
    // 恢复持久请求不会清除不可逆启动准入。
    let resumed = reopened
        .resume_activation_transaction("hook-plugin", request_id)
        .expect("same original request lock");
    // Missing physical transport owners cannot use that request to begin another startup.
    // 缺失物理传输所有者不能使用该请求开始另一启动。
    let recovered_owner = resumed
        .target_mcp_initialization_owner(staged.plugins["hook-plugin"].generation)
        .expect("same recovered request");
    assert!(recovered_owner.require_unattempted().is_err());
    assert!(recovered_owner.begin().is_err());
    assert_eq!(
        reopened
            .target_mcp_initialization_receipt("hook-plugin", request_id)
            .expect("reopened receipt")
            .expect("original Started")
            .phase(),
        crate::VulcanPluginTargetMcpInitializationPhase::Started
    );
}
