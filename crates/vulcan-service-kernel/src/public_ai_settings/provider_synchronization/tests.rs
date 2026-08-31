//! Provider-scoped failure classification and staged cancellation recovery regressions.
//! 逐供应商失败分类与暂存取消恢复回归。

use super::*;

/// Verifies exact compensation becomes a provider-local rejection instead of a batch error.
/// 验证精确补偿会成为逐供应商拒绝，而不是批次错误。
#[test]
fn compensated_projection_becomes_bounded_provider_rejection() {
    // report represents the staged activation before its Workbench projection fails.
    // report 表示工作台投影失败前的暂存激活。
    let report = crate::provider_manifest_sync::ProviderSyncReport {
        provider_id: String::from("deepseek"),
        outcome: "activated",
        revision: 7,
        failure_kind: None,
        failure_message: None,
        activation_snapshot: None,
    };
    let rejected = compensated_provider_projection_report(
        report,
        "failed to republish C:\\Users\\operator\\workbench.json",
    );
    assert_eq!(rejected.outcome, "rejected");
    assert_eq!(
        rejected.failure_kind.as_deref(),
        Some("activation_publication_failed")
    );
    assert_eq!(
        rejected.failure_message.as_deref(),
        Some("<redacted-local-path>")
    );
    assert!(rejected.activation_snapshot.is_none());
}

/// Verifies projection cannot start without the exact activation before-image contract.
/// 验证缺少精确激活前镜像契约时不得开始投影。
#[test]
fn provider_projection_rejects_missing_activation_before_image() {
    // report deliberately violates the internal activated-report contract.
    // report 有意违反内部 activated 报告契约。
    let report = crate::provider_manifest_sync::ProviderSyncReport {
        provider_id: String::from("deepseek"),
        outcome: "activated",
        revision: 7,
        failure_kind: None,
        failure_message: None,
        activation_snapshot: None,
    };
    let provider_ids = std::collections::BTreeSet::from([String::from("deepseek")]);
    let error =
        required_provider_activation_snapshots(std::slice::from_ref(&report), &provider_ids)
            .expect_err("missing exact before-image must fail closed");
    assert!(matches!(
        error,
        ProviderProjectionPublicationFailure::Uncertain(message)
            if message.contains("missing its exact before-image")
    ));
}

/// Verifies dropping an armed publication guard restores disk and staged runtime ownership.
/// 验证丢弃已启用发布护栏会恢复磁盘与暂存运行时所有权。
#[test]
fn cancelled_provider_publication_restores_staged_before_images() {
    use sha2::Digest;

    // directory owns the exact durable transaction inspected after cancellation.
    // directory 持有取消后检查的精确持久化事务。
    let directory = tempfile::tempdir().expect("temp dir should create");
    // service owns compensation for the staged file transaction.
    // service 持有已暂存文件事务的补偿责任。
    let service = crate::provider_manifest_sync::ProviderManifestSyncService::new(
        directory.path(),
        reqwest::Client::new(),
        Vec::new(),
        None,
    );
    // runtime owns the immutable bundled baseline and mutable staging catalog.
    // runtime 持有不可变内置基线与可变暂存目录。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // bundled_revision is the exact runtime identity before staging.
    // bundled_revision 是暂存前的精确运行时身份。
    let bundled_revision = runtime
        .system_manifest("deepseek")
        .expect("bundled DeepSeek manifest should exist")
        .revision;
    // candidate_value starts from the trusted bundled document and changes only revision.
    // candidate_value 从可信内置文档开始，并且只修改修订号。
    let mut candidate_value = serde_json::from_str::<serde_json::Value>(
        std::fs::read_to_string(
            vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
                .join("deepseek.json"),
        )
        .expect("bundled DeepSeek document should read")
        .as_str(),
    )
    .expect("bundled DeepSeek document should parse");
    // candidate_revision is strictly newer than the bundled authority.
    // candidate_revision 严格晚于内置权威。
    let candidate_revision = bundled_revision + 1;
    candidate_value["revision"] = serde_json::json!(candidate_revision);
    // candidate_bytes are the exact durable candidate payload.
    // candidate_bytes 是精确持久化候选载荷。
    let candidate_bytes = serde_json::to_vec(&candidate_value).expect("candidate should encode");
    // digest pins the exact candidate byte identity.
    // digest 固定精确候选字节身份。
    let digest = sha2::Sha256::digest(&candidate_bytes);
    // entry is the authenticated remote pin consumed by staged activation.
    // entry 是已暂存激活消费的认证远端固定项。
    let entry = vulcan_provider_config::manifest_sync::RemoteProviderEntry {
        provider_id: String::from("deepseek"),
        revision: candidate_revision,
        relative_url: String::from("/deepseek.json"),
        sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        bytes: candidate_bytes.len() as u64,
        minimum_engine_contract: 1,
    };
    // outcome retains the file before-image and compiled staged manifest.
    // outcome 保留文件前镜像与已编译暂存 Manifest。
    let outcome = vulcan_provider_config::manifest_sync::stage_synced_provider_activation(
        &vulcan_provider_config::manifest_store::ManifestStoreLayout::new(directory.path()),
        &entry,
        &candidate_bytes,
        None,
        None,
        None,
    )
    .expect("candidate should stage");
    // activation_parts are the exact ownership transition required by the guard.
    // activation_parts 是护栏所需的精确所有权转换。
    let (manifest, store_snapshot) = match outcome {
        vulcan_provider_config::manifest_sync::SyncOutcome::Activated {
            manifest,
            store_snapshot,
            ..
        } => (manifest, store_snapshot),
        _ => panic!("expected staged activation"),
    };
    // staged_manifest_mutation is the runtime CAS before/after image.
    // staged_manifest_mutation 是运行时 CAS 前后镜像。
    let staged_manifest_mutation = runtime
        .replace_system_manifest_transactional("deepseek", manifest)
        .expect("runtime manifest should stage");
    // reports and snapshots form the complete cancellation transaction.
    // reports 与 snapshots 构成完整取消事务。
    let snapshots = vec![
        crate::provider_manifest_sync::ProviderManifestActivationSnapshot {
            provider_id: String::from("deepseek"),
            store: store_snapshot,
            staged_manifest_mutation,
        },
    ];
    let reports = vec![crate::provider_manifest_sync::ProviderSyncReport {
        provider_id: String::from("deepseek"),
        outcome: "activated",
        revision: candidate_revision,
        failure_kind: None,
        failure_message: None,
        activation_snapshot: Some(snapshots[0].clone()),
    }];

    {
        // guard emulates an RPC Future dropped at an asynchronous projection boundary.
        // guard 模拟在异步投影边界被丢弃的 RPC Future。
        let _guard = StagedProviderActivationCancellationGuard::new(
            &service, &runtime, &reports, &snapshots,
        );
    }

    // store proves both the Active before-image and Applying journal were restored/cleared.
    // store 证明 Active 前镜像与 Applying 日志均已恢复/清除。
    let store = vulcan_provider_config::manifest_store::ManifestStoreLayout::new(directory.path());
    assert_eq!(
        store.read_active("deepseek").expect("active should read"),
        None
    );
    assert!(
        !store
            .recover_system_transaction("deepseek")
            .expect("journal probe should succeed")
    );
    assert_eq!(
        runtime
            .system_manifest("deepseek")
            .expect("restored DeepSeek manifest should exist")
            .revision,
        bundled_revision
    );
}

/// Verifies manifest RPCs accept only identities pinned by the trusted bundled index.
/// 验证 Manifest RPC 仅接受可信内置 Index 固定的身份。
#[test]
fn manifest_rpc_identity_guard_rejects_unknown_and_oversized_values() {
    // runtime supplies the exact trusted system identity set used by production RPCs.
    // runtime 提供生产 RPC 使用的精确可信系统身份集合。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    crate::provider_manifest_sync::require_trusted_system_manifest_provider(&runtime, "openrouter")
        .expect("known bundled provider must pass");
    let unknown =
        "x".repeat(vulcan_provider_adapters::provider_manifest::MAX_PROVIDER_IDENTIFIER_LEN + 1);
    let error = crate::provider_manifest_sync::require_trusted_system_manifest_provider(
        &runtime,
        unknown.as_str(),
    )
    .expect_err("unknown oversized identity must fail before any store operation");
    assert!(!error.contains(unknown.as_str()));
    assert!(error.contains("trusted system provider identifier"));
}
