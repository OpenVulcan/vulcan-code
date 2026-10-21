//! Activation publication, staged visibility, concurrent CAS, and compensation regressions.
//! 激活发布、暂存可见性、并发比较交换与补偿回归测试。

use super::super::test_support::{
    TestStaticSecretStore, connection, install_source_catalog, two_account_runtime,
};
use super::*;
use crate::{
    ActiveAiModelSelection, CredentialConnection, CredentialMode, parse_openai_model_catalog,
};
use vulcan_ai_contract::AccountUsageSnapshot;

/// Verifies isolated preparation remains invisible until one compare-and-swap publication.
/// 验证隔离准备在一次比较交换发布前始终不可见。
#[test]
fn isolated_catalog_candidate_publishes_as_one_runtime_image() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let before_image = runtime
        .capture_activation_snapshot()
        .expect("before-image should capture");
    let isolated_runtime = runtime.isolated_catalog_activation_runtime(
        &before_image,
        Arc::clone(&before_image.resolved_provider_generation.manifests),
    );
    let catalog = parse_openai_model_catalog(
        "isolated-account",
        1,
        r#"{"data":[{"id":"isolated-model"}]}"#,
    )
    .expect("isolated catalog should parse");
    install_source_catalog(&isolated_runtime, catalog.clone());
    isolated_runtime
        .replace_catalog(catalog)
        .expect("isolated effective catalog should install");
    let candidate = isolated_runtime
        .capture_activation_snapshot()
        .expect("candidate should capture");

    assert!(
        runtime
            .staged_source_catalog("isolated-account")
            .expect("main staged source should read")
            .is_none(),
        "isolated preparation must not mutate the main runtime"
    );
    runtime
        .publish_isolated_catalog_candidate(&before_image, &candidate, None)
        .expect("isolated candidate should publish");
    assert!(
        runtime
            .staged_source_catalog("isolated-account")
            .expect("published staged source should read")
            .is_some(),
        "the complete candidate must become visible after the single publication"
    );
    assert_eq!(
        runtime.catalog_state_revision.load(Ordering::Acquire),
        before_image.state_revision + 1,
        "one complete publication must advance the main revision exactly once"
    );
}

/// Verifies compensation restores usage and refuses to overwrite a later runtime mutation.
/// 验证补偿会恢复用量，并拒绝覆盖更晚发生的运行时变更。
#[test]
fn activation_compensation_is_complete_and_compare_and_swap_guarded() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // usage builds one exact provider-reported account snapshot for compensation assertions.
    // usage 构造一份精确的供应商账户快照，用于补偿断言。
    let usage = |plan_name: &str| AccountUsageSnapshot {
        credential_id: String::from("credential-compensation"),
        availability: vulcan_ai_contract::AccountUsageAvailability::Supported,
        plan_name: Some(plan_name.to_string()),
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: Vec::new(),
        observed_at: None,
        refresh_message: None,
    };
    runtime
        .cache_usage_snapshot("compensation-account", usage("before"), false)
        .expect("before usage should cache");
    let before_image = runtime
        .capture_activation_snapshot()
        .expect("before-image should capture");
    runtime
        .cache_usage_snapshot("compensation-account", usage("activated"), false)
        .expect("activated usage should cache");
    let activated_image = runtime
        .capture_activation_snapshot()
        .expect("activated image should capture");
    runtime
        .restore_activation_snapshot(&before_image, &activated_image)
        .expect("matching compensation should restore");
    assert_eq!(
        runtime.usage_cache_snapshot().expect("usage snapshot")["compensation-account"]
            .plan_name
            .as_deref(),
        Some("before"),
        "runtime compensation must include plan-scoped usage evidence"
    );

    runtime
        .cache_usage_snapshot("compensation-account", usage("newer"), false)
        .expect("newer usage should cache");
    assert!(
        runtime
            .restore_activation_snapshot(&before_image, &activated_image)
            .expect_err("stale compensation must fail closed")
            .contains("compensation refused"),
        "a stale before-image must never overwrite newer runtime state"
    );
}

/// Verifies installing identical Manifest content succeeds without creating a new generation.
/// 验证安装相同 Manifest 内容时成功返回且不创建新代次。
#[test]
fn identical_system_manifest_replacement_is_a_generation_noop() {
    // runtime owns the current bundled Manifest catalog and its process-local generation.
    // runtime 持有当前内置 Manifest 目录及其进程内代次。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let before = runtime
        .bundled_manifests
        .read()
        .expect("manifest catalog lock should remain healthy")
        .clone();
    let deepseek = before
        .manifest_arc("deepseek")
        .expect("bundled DeepSeek manifest should exist");

    runtime
        .replace_system_manifest("deepseek", deepseek)
        .expect("identical replacement should be a successful no-op");

    let after = runtime
        .bundled_manifests
        .read()
        .expect("manifest catalog lock should remain healthy")
        .clone();
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.content_sha256, before.content_sha256);
}

/// Verifies a malformed prepared generation fails before any staging catalog publication.
/// 验证畸形候选代际会在任何暂存目录发布前失败。
#[test]
fn provider_manifest_generation_build_failure_preserves_current_catalog() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let generation_before = runtime.catalog_generation();
    let digest_before = runtime.catalog_content_hash();
    let openrouter = runtime
        .system_manifest("openrouter")
        .expect("openrouter manifest must exist");

    let result = runtime.prepare_system_manifest_replacement("deepseek", openrouter);
    let error = match result {
        Ok(_) => panic!("identity-mismatched generation preparation must fail"),
        Err(error) => error,
    };
    assert!(error.contains("declares provider `openrouter`"), "{error}");
    assert_eq!(runtime.catalog_generation(), generation_before);
    assert_eq!(runtime.catalog_content_hash(), digest_before);
}

/// Verifies compare-and-swap rejects a stale prepared generation without losing a newer update.
/// 验证比较交换拒绝过期候选代际且不会丢失较新的更新。
#[test]
fn provider_manifest_generation_publish_conflict_preserves_newer_update() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let compile_revision = |provider_id: &str, revision: u64| {
        let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
            .expect("bundled manifest must exist");
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("manifest must parse");
        value["revision"] = serde_json::json!(revision);
        Arc::new(
            crate::provider_manifest::compile_manifest_text(
                &serde_json::to_string(&value).expect("manifest must serialize"),
                crate::provider_manifest::ManifestOwnership::System,
            )
            .expect("replacement manifest must compile"),
        )
    };
    let stale = runtime
        .prepare_system_manifest_replacement("deepseek", compile_revision("deepseek", 301))
        .expect("candidate generation should prepare");
    runtime
        .replace_system_manifest("openrouter", compile_revision("openrouter", 302))
        .expect("newer provider update should publish");

    assert!(
        !runtime
            .publish_prepared_system_manifest(stale)
            .expect("stale publication should return a benign conflict"),
        "the stale base generation must be rejected"
    );
    assert_eq!(
        runtime
            .staged_system_manifest("openrouter")
            .expect("openrouter manifest must remain")
            .revision,
        302,
        "the newer update must remain authoritative"
    );
    assert_ne!(
        runtime
            .staged_system_manifest("deepseek")
            .expect("deepseek manifest must remain")
            .revision,
        301,
        "the stale candidate must not leak into the staging catalog"
    );
}

/// Verifies compensation restores the exact staged catalog Arc, generation, and digest.
/// 验证补偿会恢复精确的暂存目录 Arc、代次与摘要。
#[test]
fn staged_manifest_compensation_restores_exact_before_image() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    // replacementText changes only the reviewed DeepSeek revision for a deterministic mutation.
    // replacementText 只修改已审核 DeepSeek 修订号，以产生确定性变更。
    let text = std::fs::read_to_string(directory.join("deepseek.json"))
        .expect("bundled manifest must exist");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("manifest must parse");
    value["revision"] = serde_json::json!(9_001_u64);
    let replacement = Arc::new(
        crate::provider_manifest::compile_manifest_text(
            &serde_json::to_string(&value).expect("manifest must serialize"),
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("replacement manifest must compile"),
    );

    let mutation = runtime
        .replace_system_manifest_transactional("deepseek", replacement)
        .expect("replacement must publish");
    assert_eq!(
        mutation.after.generation,
        mutation.before.generation + 1,
        "the staged replacement must advance exactly one generation"
    );
    runtime
        .restore_staged_system_manifest_catalog(&mutation)
        .expect("exact compensation must succeed");
    let restored = runtime
        .bundled_manifests
        .read()
        .expect("staging lock must remain healthy")
        .clone();
    assert!(
        Arc::ptr_eq(&restored, &mutation.before),
        "compensation must reinstall the original immutable catalog"
    );
    assert_eq!(restored.generation, mutation.before.generation);
    assert_eq!(restored.content_sha256, mutation.before.content_sha256);
}

/// Verifies staged compensation refuses to overwrite a later independent replacement.
/// 验证暂存补偿拒绝覆盖后续独立替换。
#[test]
fn staged_manifest_compensation_rejects_newer_mutation() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    // compileRevision builds one valid provider-specific replacement at a chosen revision.
    // compileRevision 为指定修订构建一份有效的供应商专属替换件。
    let compile_revision = |provider_id: &str, revision: u64| {
        let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
            .expect("bundled manifest must exist");
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("manifest must parse");
        value["revision"] = serde_json::json!(revision);
        Arc::new(
            crate::provider_manifest::compile_manifest_text(
                &serde_json::to_string(&value).expect("manifest must serialize"),
                crate::provider_manifest::ManifestOwnership::System,
            )
            .expect("replacement manifest must compile"),
        )
    };
    let first = runtime
        .replace_system_manifest_transactional("deepseek", compile_revision("deepseek", 9_101))
        .expect("first replacement must publish");
    let second = runtime
        .replace_system_manifest_transactional("openrouter", compile_revision("openrouter", 9_102))
        .expect("second replacement must publish");

    let error = runtime
        .restore_staged_system_manifest_catalog(&first)
        .expect_err("out-of-order compensation must fail closed");
    assert!(error.contains("compensation refused"), "error: {error}");
    // Reverse-order unwind proves a multi-provider transaction can restore both exact Arcs.
    // 逆序展开证明多供应商事务能够恢复两个精确 Arc。
    runtime
        .restore_staged_system_manifest_catalog(&second)
        .expect("latest mutation must restore first");
    runtime
        .restore_staged_system_manifest_catalog(&first)
        .expect("earlier mutation must restore after its successor");
    let restored = runtime
        .bundled_manifests
        .read()
        .expect("staging lock must remain healthy")
        .clone();
    assert!(Arc::ptr_eq(&restored, &first.before));
}

/// Verifies configuration identities and effective catalogs always share one Arc publication.
/// 验证配置身份与有效目录始终共享同一个 Arc 发布边界。
#[test]
fn concurrent_configuration_activation_never_mixes_catalog_generations() {
    // runtime begins with two fully activated credential catalogs.
    // runtime 初始持有两个已完整激活的凭据目录。
    let (runtime, mut old_configuration) = two_account_runtime();
    // oldActive identifies the first complete publication state.
    // oldActive 标识第一份完整发布状态。
    old_configuration.active = Some(ActiveAiModelSelection {
        provider_id: String::from("openrouter"),
        model_id: String::from("openai/gpt-5.6-sol"),
        connection_id: String::from("one"),
    });
    runtime
        .replace_configuration(old_configuration.clone())
        .expect("old configuration generation should publish");
    // newConfiguration removes account one and selects account two in one candidate image.
    // newConfiguration 在同一候选镜像中移除账号 one 并选择账号 two。
    let mut new_configuration = old_configuration.clone();
    new_configuration.connection_graph.connections.remove("one");
    new_configuration.active = Some(ActiveAiModelSelection {
        provider_id: String::from("openrouter"),
        model_id: String::from("openai/gpt-5.6-sol"),
        connection_id: String::from("two"),
    });
    // runtime is shared by concurrent readers and the alternating writer.
    // runtime 由并发读取方与交替写入方共享。
    let runtime = Arc::new(runtime);
    // readers repeatedly retain one immutable generation and validate both halves together.
    // readers 重复保留一个不可变代际，并共同校验其配置与目录两半。
    let mut readers = Vec::new();
    for _reader_index in 0..8 {
        let runtime = Arc::clone(&runtime);
        readers.push(std::thread::spawn(move || {
            for _sample_index in 0..400 {
                // generation is the sole atomic observation made by this sample.
                // generation 是本次采样进行的唯一原子观测。
                let generation = runtime
                    .selection_catalog_snapshot()
                    .expect("provider generation should remain readable");
                // connectionIds comes from the configuration frozen in the retained Arc.
                // connectionIds 来自保留 Arc 中冻结的配置。
                let connection_ids = generation
                    .configuration
                    .connection_graph
                    .connections
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                // catalogIds comes from the effective catalogs frozen in the same Arc.
                // catalogIds 来自同一 Arc 中冻结的有效目录。
                let catalog_ids = generation
                    .activated_effective_catalogs
                    .snapshot()
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>();
                assert_eq!(connection_ids, catalog_ids);
                // activeConnection must belong to the same exact connection set.
                // activeConnection 必须属于同一精确连接集合。
                let active_connection = generation
                    .configuration
                    .active
                    .as_ref()
                    .expect("each test generation should own an active selection")
                    .connection_id
                    .as_str();
                assert!(connection_ids.contains(active_connection));
            }
        }));
    }
    // writer alternates both complete configurations while readers sample.
    // writer 在读取方采样期间交替发布两份完整配置。
    for activation_index in 0..200 {
        let candidate = if activation_index % 2 == 0 {
            &new_configuration
        } else {
            &old_configuration
        };
        runtime
            .replace_configuration(candidate.clone())
            .expect("complete configuration generation should publish atomically");
    }
    for reader in readers {
        reader.join().expect("configuration reader should finish");
    }
}

/// Verifies concurrent replacements of distinct providers cannot lose either update.
/// 验证不同供应商的并发替换不会丢失任一更新。
#[test]
fn concurrent_distinct_provider_replacements_preserve_both_updates() {
    let runtime = std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime"));
    let generation_before = runtime.catalog_generation();
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let compile_revision = |provider_id: &str, revision: u64| {
        let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
            .expect("bundled manifest must exist");
        let mut value: serde_json::Value =
            serde_json::from_str(&text).expect("manifest must parse");
        value["revision"] = serde_json::json!(revision);
        Arc::new(
            crate::provider_manifest::compile_manifest_text(
                &serde_json::to_string(&value).expect("manifest must serialize"),
                crate::provider_manifest::ManifestOwnership::System,
            )
            .expect("replacement manifest must compile"),
        )
    };
    let deepseek = compile_revision("deepseek", 101);
    let openrouter = compile_revision("openrouter", 202);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let deepseek_handle = {
        let runtime = std::sync::Arc::clone(&runtime);
        let barrier = std::sync::Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            runtime
                .replace_system_manifest("deepseek", deepseek)
                .expect("deepseek replacement must succeed");
        })
    };
    let openrouter_handle = {
        let runtime = std::sync::Arc::clone(&runtime);
        let barrier = std::sync::Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            runtime
                .replace_system_manifest("openrouter", openrouter)
                .expect("openrouter replacement must succeed");
        })
    };
    barrier.wait();
    deepseek_handle.join().expect("deepseek writer must join");
    openrouter_handle
        .join()
        .expect("openrouter writer must join");

    let catalog = runtime
        .bundled_manifests
        .read()
        .expect("catalog lock must not be poisoned");
    assert_eq!(
        catalog.manifest("deepseek").map(|value| value.revision),
        Some(101)
    );
    assert_eq!(
        catalog.manifest("openrouter").map(|value| value.revision),
        Some(202)
    );
    assert_eq!(
        catalog.generation,
        generation_before + 2,
        "each serialized whole-generation replacement must be retained"
    );
}

/// R3/§R3.8: every replacement swaps the whole catalog generation with a new content hash.
/// R3/§R3.8：每次替换整体交换目录代数并产生新内容哈希。
///
/// Business invariant (plan §6.2/§R3.5): the catalog is published whole; the generation
/// explains the content because the digest covers every manifest identity, revision, and
/// source digest. A generation bump without a content change is impossible.
/// 业务不变量（方案 §6.2/§R3.5）：目录整体发布；摘要覆盖每份清单身份、修订号与来源
/// 摘要，使代数能解释内容。不存在只递增代数却无内容变化的情况。
#[tokio::test]
async fn hot_replaced_catalog_swaps_whole_generation_with_content_hash() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let generation_before = runtime.catalog_generation();
    let hash_before = runtime.catalog_content_hash();
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join("deepseek.json"))
        .expect("bundled deepseek manifest must exist");
    let mut value: serde_json::Value = serde_json::from_str(&text).expect("manifest should parse");
    value["revision"] = serde_json::json!(value["revision"].as_u64().unwrap_or(1) + 1);
    // replacementDisplayName proves registry reads follow the published generation.
    // replacementDisplayName 证明注册表读取跟随已发布代际。
    let replacement_display_name = "DeepSeek Hot-Replaced";
    value["display"]["name"] = serde_json::json!(replacement_display_name);
    let replacement_text = serde_json::to_string(&value).expect("manifest should serialize");
    let replaced = crate::provider_manifest::compile_manifest_text(
        &replacement_text,
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("replacement manifest must compile");
    runtime
        .replace_system_manifest("deepseek", Arc::new(replaced))
        .expect("hot replace should succeed");
    assert_eq!(
        runtime.catalog_generation(),
        generation_before,
        "a staged catalog must not change the externally visible generation"
    );
    assert_eq!(
        runtime.catalog_content_hash(),
        hash_before,
        "a staged catalog must not change the externally visible digest"
    );
    runtime
        .activate_staged_manifest_catalogs(&PersistedAiConfiguration::default(), &[])
        .await
        .expect("the complete generation should publish");
    assert_eq!(
        runtime.catalog_generation(),
        generation_before + 1,
        "the explicit activation boundary publishes the next whole generation"
    );
    let hash_after = runtime.catalog_content_hash();
    assert_ne!(
        hash_after, hash_before,
        "a content change must produce a new content digest"
    );
    assert_eq!(
        runtime
            .providers()
            .expect("published provider registry should resolve")
            .provider("deepseek")
            .expect("DeepSeek must remain registered")
            .display_name
            .as_str(),
        replacement_display_name,
        "registry snapshots must never retain the initial bundled descriptor"
    );
    // After activation the discovery client and published runtime converge on one generation.
    // 激活后发现客户端与已发布运行时收敛到同一代数。
    assert_eq!(
        runtime.metadata_client().catalog_generation(),
        runtime.catalog_generation(),
        "runtime and discovery client must share one catalog generation"
    );
}

/// Verifies atomic generation activation excludes provider-tool connections from model projection.
/// 验证原子代际激活会从模型投影中排除供应商工具连接。
#[tokio::test]
async fn atomic_manifest_activation_skips_provider_tool_connections() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let connection_id = String::from("tavily-provider-tool-account");
    let mut tavily_connection = connection(connection_id.as_str());
    tavily_connection.provider_id = String::from("tavily");
    tavily_connection.service_entry_id = String::from("public-api");
    tavily_connection.credential_profile_id = String::from("api-key");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            tavily_connection,
        )
        .expect("Tavily provider-tool connection should register");

    runtime
        .activate_staged_manifest_catalogs(&configuration, std::slice::from_ref(&connection_id))
        .await
        .expect("provider-tool connection must not block generation activation");

    assert!(
        !runtime
            .catalog_snapshot()
            .expect("catalog snapshot should resolve")
            .contains_key(connection_id.as_str()),
        "provider-tool connection must not acquire a conversation model catalog"
    );
}

/// Verifies ordinary refreshes cannot mix a published provider with a staged manifest.
/// 验证普通刷新不能把已发布供应商与暂存清单混合使用。
#[tokio::test]
async fn staged_manifest_catalogs_publish_only_through_activation_batch() {
    let runtime = PublicAiRuntime::first_batch(Arc::new(TestStaticSecretStore::api_key()))
        .expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let connection_id = String::from("alibaba-activation-account");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            CredentialConnection {
                id: connection_id.clone(),
                provider_id: String::from("alibaba"),
                service_entry_id: String::from("token-plan-personal-cn"),
                credential_profile_id: String::from("token-plan-api-key"),
                credential_mode: CredentialMode::Token,
                credential_id: String::from("credential-alibaba-activation"),
                display_name: String::from("Alibaba Activation Account"),
                secret_reference: String::from("secret-alibaba-activation"),
                management_secret_reference: None,
                service_origin_override: None,
                prompt_cache_by_protocol: BTreeMap::new(),
            },
        )
        .expect("Alibaba connection should register");
    let initial = runtime
        .refresh_connection_catalog(&configuration, &connection_id)
        .await
        .expect("bundled static catalog should refresh");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("initial catalog generation should publish");

    let future_model_id = "qwen-manifest-activation-fixture";
    assert!(
        initial
            .models
            .iter()
            .all(|model| model.id != future_model_id),
        "the bundled catalog must not already contain the future fixture"
    );
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join("alibaba.json"))
        .expect("bundled Alibaba manifest must exist");
    let mut document =
        serde_json::from_str::<serde_json::Value>(&text).expect("Alibaba manifest should parse");
    let former_revision = document["revision"]
        .as_u64()
        .expect("Alibaba revision must exist");
    document["revision"] = serde_json::Value::from(former_revision + 1);
    let services = document["services"]
        .as_array_mut()
        .expect("Alibaba services must be an array");
    let token_plan = services
        .iter_mut()
        .find(|service| service["service_entry_id"] == "token-plan-personal-cn")
        .expect("Alibaba Token Plan entry must exist");
    token_plan["model_catalog"]["models"]
        .as_array_mut()
        .expect("Alibaba static models must be an array")
        .push(serde_json::json!({
            "model_id": future_model_id,
            "display_name": "Qwen Manifest Activation Fixture"
        }));
    // base_rule is the reviewed rule for this exact service, not another product sharing the model id.
    // base_rule 是此精确服务的已审核规则，而不是共享模型 ID 的其他产品规则。
    let base_rule = document["model_rules"]
        .as_array()
        .expect("Alibaba model rules must be an array")
        .iter()
        .find(|rule| {
            rule["matcher"]["value"] == "qwen3.7-plus"
                && rule["scope"]["service_entry_ids"]
                    .as_array()
                    .is_some_and(|services| {
                        services
                            .iter()
                            .any(|service| service == "token-plan-personal-cn")
                    })
        })
        .cloned()
        .expect("Alibaba Token Plan Qwen rule must exist");
    // future_rule changes only bounded identity facts and retains the pre-authorized contract.
    // future_rule 仅修改有界身份事实并保留预授权契约。
    let mut future_rule = base_rule;
    future_rule["rule_id"] =
        serde_json::Value::String(String::from("alibaba-manifest-activation-fixture"));
    future_rule["matcher"]["value"] = serde_json::Value::String(future_model_id.to_string());
    document["model_rules"]
        .as_array_mut()
        .expect("Alibaba model rules must be mutable")
        .push(future_rule);
    let replacement = crate::provider_manifest::compile_manifest_text(
        &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("replacement manifest must compile");
    runtime
        .replace_system_manifest("alibaba", Arc::new(replacement))
        .expect("replacement manifest should stage");

    let ordinary = runtime
        .refresh_connection_catalog(&configuration, &connection_id)
        .await
        .expect("ordinary refresh should still use the published manifest");
    assert!(
        ordinary
            .models
            .iter()
            .all(|model| model.id != future_model_id),
        "ordinary refresh must not observe an unpublished manifest"
    );
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("ordinary reapply should retain the published manifest generation");
    assert_eq!(
        runtime
            .system_manifest("alibaba")
            .expect("published Alibaba manifest should exist")
            .revision,
        former_revision,
        "staging must not alter the published generation"
    );

    runtime
        .activate_staged_manifest_catalogs(&configuration, std::slice::from_ref(&connection_id))
        .await
        .expect("staged manifest activation should succeed");
    assert_eq!(
        runtime
            .system_manifest("alibaba")
            .expect("activated Alibaba manifest should exist")
            .revision,
        former_revision + 1
    );
    let activated = runtime.catalog_snapshot().expect("catalog snapshot");
    assert!(
        activated[&connection_id]
            .models
            .iter()
            .any(|model| model.id == future_model_id),
        "the newly declared model must appear in the same published activation"
    );
}

use crate::PersistedAiConfiguration;
use crate::runtime::PublicAiRuntime;
use crate::runtime::test_support::{
    custom_connection, custom_definition_with_discovery_mode, install_current_custom_provider,
    multi_protocol_custom_definition,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use vulcan_ai_contract::{AiOperation, ModelCapabilities};

/// Verifies one invalid standalone custom provider cannot block a healthy sibling activation.
/// 验证一个无效独立自定义供应商不会阻断健康同级供应商激活。
#[tokio::test]
async fn isolated_custom_provider_connection_is_excluded_from_atomic_activation() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let healthy_provider_id = "custom:healthy-activation";
    let isolated_provider_id = "custom:isolated-activation";
    let healthy_connection_id = "healthy-activation-account";
    let isolated_connection_id = "isolated-activation-account";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_provider_file_refs.extend([
        healthy_provider_id.to_string(),
        isolated_provider_id.to_string(),
    ]);
    let healthy_provider = custom_definition_with_discovery_mode(
        healthy_provider_id,
        &[crate::CustomProviderProtocol::OpenAiChat],
        crate::CustomProviderModelDiscoveryMode::Manual,
    );
    install_current_custom_provider(&mut configuration, healthy_provider);
    configuration.connection_graph.connections.extend([
        (
            healthy_connection_id.to_string(),
            custom_connection(healthy_connection_id, healthy_provider_id),
        ),
        (
            isolated_connection_id.to_string(),
            custom_connection(isolated_connection_id, isolated_provider_id),
        ),
    ]);
    configuration.custom_models.insert(
        healthy_provider_id.to_string(),
        BTreeMap::from([(
            String::from("healthy-model"),
            crate::CustomModelDefinition {
                id: String::from("healthy-model"),
                display_name: String::from("Healthy Model"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
                tool_mode: crate::ToolMode::Direct,
                system_binding: None,
            },
        )]),
    );
    let connection_ids = vec![
        healthy_connection_id.to_string(),
        isolated_connection_id.to_string(),
    ];

    runtime
        .activate_staged_manifest_catalogs(&configuration, &connection_ids)
        .await
        .expect("healthy provider activation should ignore the isolated sibling");

    let catalogs = runtime
        .catalog_snapshot()
        .expect("catalog snapshot should read");
    assert!(catalogs.contains_key(healthy_connection_id));
    assert!(!catalogs.contains_key(isolated_connection_id));
    assert!(
        runtime
            .resolve_connection_model_selection(
                &configuration,
                healthy_connection_id,
                healthy_provider_id,
                "healthy-model",
            )
            .is_ok()
    );
    let isolated_error = runtime
        .resolve_connection_model_selection(
            &configuration,
            isolated_connection_id,
            isolated_provider_id,
            "isolated-model",
        )
        .map(|_| ())
        .expect_err("isolated provider execution must fail closed");
    assert!(
        isolated_error.contains("disabled")
            || isolated_error.contains("no active system manifest")
            || isolated_error.contains("No activated catalog exists"),
        "{isolated_error}"
    );
}

/// Verifies custom descriptor matching excludes only standalone-Manifest Profile payloads.
/// 验证自定义描述符匹配仅排除独立 Manifest 的 Profile 载荷。
#[test]
fn custom_manifest_descriptor_match_excludes_only_manifest_owned_profiles() {
    let provider = multi_protocol_custom_definition(
        "custom:manifest-descriptor-match",
        &[crate::CustomProviderProtocol::OpenAiChat],
    );
    let manifest = crate::provider_manifest::project_custom_descriptor_manifest(&provider)
        .expect("custom descriptor should project to one manifest");
    let compiled = crate::provider_manifest::compile_manifest(
        &manifest,
        crate::provider_manifest::ManifestOwnership::Custom,
    )
    .expect("projected custom manifest should compile");

    assert!(
        crate::provider_manifest::custom_manifest_matches_activated_descriptor(
            compiled.provider(),
            &provider,
        ),
        "manifest-owned standard Usage payloads must not create a false descriptor mismatch"
    );

    let mut conflicting_provider = provider;
    conflicting_provider.service_entries[0].base_url =
        String::from("https://conflicting.example.invalid/v1");
    assert!(
        !crate::provider_manifest::custom_manifest_matches_activated_descriptor(
            compiled.provider(),
            &conflicting_provider,
        ),
        "descriptor-owned service facts must remain fail-closed"
    );
}

/// R3/§R3.2: identical content rebuilds produce the identical content digest.
/// R3/§R3.2：相同内容的重新构建产生相同内容摘要。
#[test]
fn catalog_build_is_deterministic_for_identical_content() {
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join("deepseek.json"))
        .expect("bundled deepseek manifest must exist");
    let resolved = crate::provider_manifest::compile_manifest_text(
        &text,
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("deepseek manifest must compile");
    let mut manifests = BTreeMap::new();
    manifests.insert(String::from("deepseek"), Arc::new(resolved));
    let first = crate::provider_manifest::ResolvedProviderCatalog::build(manifests.clone(), 1)
        .expect("catalog must build");
    let second = crate::provider_manifest::ResolvedProviderCatalog::build(manifests, 2)
        .expect("catalog must build");
    assert_eq!(
        first.content_sha256, second.content_sha256,
        "identical content must share one digest across generations"
    );
    assert_ne!(first.generation, second.generation);
}
