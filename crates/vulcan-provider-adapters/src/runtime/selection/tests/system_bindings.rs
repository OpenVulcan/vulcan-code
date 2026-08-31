//! System-manifest binding authority and executable variant admission.
//! 系统清单绑定权威与可执行变体准入。

use crate::runtime::PublicAiRuntime;
use crate::runtime::test_support::{connection, install_source_catalog, with_protocol_variants};
use crate::{
    ModelCapabilityEvidence, PersistedAiConfiguration, parse_openai_model_catalog,
    parse_openrouter_model_catalog,
};
use std::collections::BTreeSet;
use std::sync::Arc;
use vulcan_ai_contract::ModelCapabilities;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Verifies model-level protocol routing: two models on one credential bind to their own
/// manifest-declared protocols.
/// 验证模型级协议路由：同一凭据上的两个模型绑定各自 Manifest 声明协议。
#[test]
fn model_level_protocol_routing_uses_declared_bindings_per_model() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            connection("one"),
        )
        .expect("connection should register");
    let catalog = with_protocol_variants(
        parse_openrouter_model_catalog(
            "one",
            1,
            r#"{"data":[
                    {"id":"openai/gpt-5.6-sol","name":"OpenAI: GPT-5.6 Sol","context_length":1050000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":128000}},
                    {"id":"anthropic/claude-sonnet-5","name":"Anthropic: Claude Sonnet 5","context_length":1000000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":64000}}
                ]}"#,
        )
        .expect("openrouter catalog should parse"),
        &[
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("catalog should install");
    configuration.set_model_enabled("openrouter", "openai/gpt-5.6-sol", true);
    configuration.set_model_enabled("openrouter", "anthropic/claude-sonnet-5", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("protocol-routing generation should publish");

    runtime
        .validate_model_execution_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            OPENAI_RESPONSES_PROFILE_ID,
            &crate::ModelExecutionOverrides::default(),
        )
        .expect("model settings must validate against the frozen Responses binding");
    let wrong_protocol_error = runtime
        .validate_model_execution_override_candidate(
            &configuration,
            "openrouter",
            "openai/gpt-5.6-sol",
            OPENAI_CHAT_PROFILE_ID,
            &crate::ModelExecutionOverrides::default(),
        )
        .expect_err("another protocol must not authorize model settings");
    assert!(
        wrong_protocol_error.contains("effective protocol"),
        "unexpected wrong-protocol error: {wrong_protocol_error}"
    );

    let published_catalogs = runtime
        .catalog_snapshot()
        .expect("published catalogs should remain readable");
    let published_models = &published_catalogs
        .get("one")
        .expect("connection catalog must exist")
        .models;
    for published_model in published_models {
        let published_binding = published_model
            .resolved_protocol_binding
            .as_ref()
            .expect("reviewed system models must freeze a catalog binding");
        let published_variant = published_model
            .protocol_variants
            .get(published_binding.protocol_profile_id.as_str())
            .expect("the frozen system binding must own an exact variant");
        assert_eq!(published_model.capabilities, published_variant.capabilities);
        assert_eq!(
            published_model.capability_evidence,
            published_variant.capability_evidence
        );
    }

    let sol = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "openai/gpt-5.6-sol",
        )
        .expect("GPT-5.6 Sol must resolve");
    assert_eq!(
        sol.route.protocol_route.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "GPT-5.6 Sol binds to its declared Responses protocol"
    );
    let binding = &sol.protocol_binding;
    assert_eq!(binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID);
    assert!(binding.manifest_revision >= 1);

    let claude = runtime
        .resolve_connection_model_selection(
            &configuration,
            "one",
            "openrouter",
            "anthropic/claude-sonnet-5",
        )
        .expect("Claude Sonnet 5 must resolve");
    assert_eq!(
        claude.route.protocol_route.protocol_profile_id, ANTHROPIC_MESSAGES_PROFILE_ID,
        "Claude Sonnet 5 binds to its declared Anthropic protocol"
    );
    assert_ne!(
        sol.route_revision, claude.route_revision,
        "distinct protocol bindings must produce distinct route revisions"
    );
}

/// Verifies APIKEY.FUN routes a credential-only connection through its signed model binding.
/// 验证 APIKEY.FUN 会通过已签名模型绑定路由仅含凭据身份的连接。
#[test]
fn apikeyfun_manifest_uses_model_protocol_binding() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    // apikeyfunConnection stores only credential and service identity.
    // apikeyfunConnection 仅保存凭据与服务身份，协议由当前 Manifest 模型绑定提供。
    let mut apikeyfun_connection = connection("apikeyfun-account");
    apikeyfun_connection.provider_id = String::from("apikeyfun");
    apikeyfun_connection.service_entry_id = String::from("public-api");
    configuration
        .connection_graph
        .connections
        .insert(apikeyfun_connection.id.clone(), apikeyfun_connection);

    let model_id = "deepseek-v4-flash";
    // sourceCatalog represents one `/models` identity projected through the signed Responses rule.
    // sourceCatalog 表示一个通过已签名 Responses 规则投影的 `/models` 身份。
    let source_catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "apikeyfun-account",
            1,
            format!(r#"{{"data":[{{"id":"{model_id}"}}]}}"#).as_str(),
        )
        .expect("compatible catalog should parse"),
        &[OPENAI_RESPONSES_PROFILE_ID],
    );
    install_source_catalog(&runtime, source_catalog.clone());
    runtime
        .replace_catalog(source_catalog)
        .expect("catalog should install");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("APIKEY.FUN Manifest catalog should publish");

    let selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "apikeyfun-account",
            "apikeyfun",
            model_id,
        )
        .expect("declared APIKEY.FUN model should resolve");
    assert_eq!(
        selection.route.protocol_route.protocol_profile_id,
        OPENAI_RESPONSES_PROFILE_ID
    );
    assert_eq!(
        selection.protocol_binding.source,
        crate::provider_manifest::ModelProtocolBindingSource::SystemManifest
    );
}

/// Verifies APIKEY.FUN rejects a discovered model outside the reviewed declaration whitelist.
/// 验证 APIKEY.FUN 会拒绝已发现但不在已审核声明白名单中的模型。
#[test]
fn apikeyfun_rejects_discovered_model_outside_declared_whitelist() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let mut apikeyfun = connection("apikeyfun-account");
    apikeyfun.provider_id = String::from("apikeyfun");
    apikeyfun.service_entry_id = String::from("public-api");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            apikeyfun,
        )
        .expect("APIKEY.FUN connection should register");

    let model_id = "future-provider-model";
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "apikeyfun-account",
            1,
            format!(r#"{{"data":[{{"id":"{model_id}"}}]}}"#).as_str(),
        )
        .expect("compatible catalog should parse"),
        &[OPENAI_CHAT_PROFILE_ID],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("catalog should install");
    configuration.set_model_enabled("apikeyfun", model_id, true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("reviewed provider catalog should publish");

    let error = runtime
        .resolve_connection_model_selection(
            &configuration,
            "apikeyfun-account",
            "apikeyfun",
            model_id,
        )
        .expect_err("undeclared APIKEY.FUN model must not resolve");
    assert!(
        error.contains("not present in the effective catalog"),
        "rejection must identify the exact effective-catalog boundary: {error}"
    );
}

/// Verifies an undeclared model on a `declared_rule_required` provider is not executable.
/// 验证 `declared_rule_required` 供应商上未声明模型不可执行。
#[test]
fn undeclared_model_on_declared_rule_required_provider_is_not_executable() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            connection("one"),
        )
        .expect("connection should register");
    let catalog = parse_openrouter_model_catalog(
        "one",
        1,
        r#"{"data":[{"id":"openai/gpt-4o","name":"OpenAI: GPT-4o","context_length":128000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":16384}}]}"#,
    )
    .expect("openrouter catalog should parse");
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("catalog should install");
    configuration.set_model_enabled("openrouter", "openai/gpt-4o", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("undeclared-model generation should publish");

    let published_catalogs = runtime
        .catalog_snapshot()
        .expect("published catalogs should remain readable");
    let published_model = published_catalogs
        .get("one")
        .and_then(|catalog| catalog.models.first())
        .expect("the unmatched identity remains visible for diagnostics");
    assert!(published_model.resolved_protocol_binding.is_none());
    assert_eq!(published_model.capabilities, ModelCapabilities::default());
    assert_eq!(
        published_model.capability_evidence,
        ModelCapabilityEvidence::default()
    );

    let error = match runtime.resolve_connection_model_selection(
        &configuration,
        "one",
        "openrouter",
        "openai/gpt-4o",
    ) {
        Ok(_) => panic!("undeclared model must not execute"),
        Err(error) => error,
    };
    assert!(
        error.contains("not executable"),
        "expected a binding-missing error, got: {error}"
    );
}

/// Verifies the DeepSeek manifest supplies exact protocol variants for its current identities.
/// 验证 DeepSeek Manifest 为当前模型身份提供精确协议变体。
#[test]
fn deepseek_manifest_identity_supplies_current_protocol_variants() {
    // runtime exposes the same compiled bundled manifest used by production startup.
    // runtime 暴露生产启动使用的同一份已编译内置 Manifest。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // configuration owns the persisted connection snapshot activated by the runtime.
    // configuration 持有由运行时激活的持久化连接快照。
    let mut configuration = PersistedAiConfiguration::default();
    // deepseekConnection reproduces an existing credential created before Responses was added.
    // deepseekConnection 复现一个在加入 Responses 前创建的既有凭据。
    let mut deepseek_connection = connection("deepseek-existing-account");
    deepseek_connection.provider_id = String::from("deepseek");
    deepseek_connection.service_entry_id = String::from("official-api");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            deepseek_connection,
        )
        .expect("existing DeepSeek connection should register");
    configuration.set_model_enabled("deepseek", "deepseek-v4-flash", true);
    configuration.set_model_enabled("deepseek", "deepseek-v4-flash-vision-exp", true);
    configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("DeepSeek fixed manifest catalog should publish");
    // catalogs is the production projection consumed by workbench selection and execution.
    // catalogs 是工作台选择与执行消费的生产投影。
    let catalogs = runtime
        .catalog_snapshot()
        .expect("published DeepSeek catalog should remain readable");
    // catalog is the exact existing-account projection built without model discovery.
    // catalog 是无需模型发现构建的精确既有账号投影。
    let catalog = catalogs
        .get("deepseek-existing-account")
        .expect("existing DeepSeek account catalog should exist");
    assert_eq!(catalog.models.len(), 3);
    // flash must bind Responses and retain all three protocol variants.
    // flash 必须绑定 Responses 并保留全部三个协议变体。
    let flash = catalog
        .models
        .iter()
        .find(|model| model.id == "deepseek-v4-flash")
        .expect("DeepSeek V4 Flash must exist");
    assert_eq!(
        flash
            .resolved_protocol_binding
            .as_ref()
            .map(|binding| binding.protocol_profile_id.as_str()),
        Some(OPENAI_RESPONSES_PROFILE_ID)
    );
    assert_eq!(
        flash
            .protocol_variants
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ])
    );
    // vision must retain the Flash protocol contract while adding exact image input support.
    // vision 必须保留 Flash 协议契约，同时增加精确的图片输入支持。
    let vision = catalog
        .models
        .iter()
        .find(|model| model.id == "deepseek-v4-flash-vision-exp")
        .expect("DeepSeek V4 Flash Vision Exp must exist");
    assert_eq!(
        vision
            .resolved_protocol_binding
            .as_ref()
            .map(|binding| binding.protocol_profile_id.as_str()),
        Some(OPENAI_RESPONSES_PROFILE_ID)
    );
    assert_eq!(
        vision
            .protocol_variants
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ])
    );
    assert_eq!(
        vision.capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::Image]
    );
    // pro must bind Responses and retain the same three protocol variants as flash.
    // pro 必须绑定 Responses，并保留与 flash 相同的三个协议变体。
    let pro = catalog
        .models
        .iter()
        .find(|model| model.id == "deepseek-v4-pro")
        .expect("DeepSeek V4 Pro must exist");
    assert_eq!(
        pro.resolved_protocol_binding
            .as_ref()
            .map(|binding| binding.protocol_profile_id.as_str()),
        Some(OPENAI_RESPONSES_PROFILE_ID)
    );
    assert_eq!(
        pro.protocol_variants
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ])
    );
    // flashSelection proves the stale persisted protocol set cannot block a newer signed route.
    // flashSelection 证明陈旧的持久化协议集合无法阻止较新的签名路由。
    let flash_selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "deepseek-existing-account",
            "deepseek",
            "deepseek-v4-flash",
        )
        .expect("existing DeepSeek account should execute Flash through Responses");
    assert_eq!(
        flash_selection.route.protocol_route.protocol_profile_id,
        OPENAI_RESPONSES_PROFILE_ID
    );
    assert_eq!(
        flash_selection.model.capabilities.provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch],
        "the executable Flash Responses route must carry native web search"
    );
    // visionSelection proves the executable route carries both image constraints and web search.
    // visionSelection 证明可执行路由同时携带图片约束与联网搜索能力。
    let vision_selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "deepseek-existing-account",
            "deepseek",
            "deepseek-v4-flash-vision-exp",
        )
        .expect("existing DeepSeek account should execute Vision through Responses");
    assert_eq!(
        vision_selection.route.protocol_route.protocol_profile_id,
        OPENAI_RESPONSES_PROFILE_ID
    );
    assert_eq!(
        vision_selection.model.capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::Image]
    );
    assert_eq!(
        vision_selection.model.capabilities.provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch],
        "the executable Vision Responses route must carry native web search"
    );
    // imageConstraint is the route-level attachment contract consumed by request preparation.
    // imageConstraint 是请求准备阶段消费的路由级附件契约。
    let image_constraint = vision_selection
        .route
        .protocol_route
        .input_resource_constraints
        .first()
        .expect("Vision route must carry its image input constraint");
    assert_eq!(
        image_constraint.resource_kind,
        vulcan_ai_contract::ResourceKind::Image
    );
    assert_eq!(image_constraint.max_bytes, Some(32 * 1024 * 1024));
    assert_eq!(image_constraint.max_count, Some(600));
    assert_eq!(
        image_constraint.reference_policy,
        crate::ResourceReferencePolicy::LocalOnly
    );
    assert!(image_constraint.upload_channel_refs.is_empty());
    // proSelection proves V4 Pro now follows the exact same Responses route contract.
    // proSelection 证明 V4 Pro 现在遵循完全相同的 Responses 路由契约。
    let pro_selection = runtime
        .resolve_connection_model_selection(
            &configuration,
            "deepseek-existing-account",
            "deepseek",
            "deepseek-v4-pro",
        )
        .expect("existing DeepSeek account should execute Pro through Responses");
    assert_eq!(
        pro_selection.route.protocol_route.protocol_profile_id,
        OPENAI_RESPONSES_PROFILE_ID
    );
    assert_eq!(
        pro_selection.model.capabilities.provider_native_tools,
        vec![vulcan_ai_contract::NativeToolKind::WebSearch],
        "the executable Pro Responses route must carry native web search"
    );
}

/// Verifies a staged system manifest affects new bindings only after explicit activation.
/// 验证暂存系统清单只在显式激活后影响新绑定。
#[tokio::test]
async fn hot_replaced_manifest_drives_new_bindings() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let mut deepseek_connection = connection("one");
    deepseek_connection.provider_id = String::from("deepseek");
    deepseek_connection.service_entry_id = String::from("official-api");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            deepseek_connection,
        )
        .expect("connection should register");
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "one",
            1,
            r#"{"data":[{"id":"deepseek-v4-pro","context_window":131072,"max_output_tokens":8192,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("openai catalog should parse"),
        &[
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_CHAT_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("catalog should install");
    configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("initial deepseek generation should publish");

    // The bundled manifest binds v4-pro to Responses by rule.
    // 内置清单按规则把 v4-pro 绑定到 Responses。
    let before = runtime
        .resolve_connection_model_selection(&configuration, "one", "deepseek", "deepseek-v4-pro")
        .expect("bundled binding must resolve");
    assert_eq!(
        before.route.protocol_route.protocol_profile_id,
        OPENAI_RESPONSES_PROFILE_ID
    );

    // A fully compiled replacement re-binds the same rule to chat; the write lock is held
    // only for the in-memory insertion.
    // 完全编译的替换件把同一规则重新绑定到 chat；写锁仅为内存插入而持有。
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join("deepseek.json"))
        .expect("bundled deepseek manifest must exist");
    let mut manifest: crate::provider_manifest::ProviderManifestV1 =
        serde_json::from_str(&text).expect("manifest should parse");
    for rule in &mut manifest.model_rules {
        let matches_pro = rule.matcher.value == "deepseek-v4-pro";
        if matches_pro {
            rule.selected_protocol = Some(String::from("openai-chat-completions.v1"));
        }
    }
    let replacement_text =
        serde_json::to_string_pretty(&manifest).expect("manifest should serialize");
    let replaced = crate::provider_manifest::compile_manifest_text(
        &replacement_text,
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("replacement manifest must compile");
    runtime
        .replace_system_manifest("deepseek", Arc::new(replaced))
        .expect("hot replace should succeed");
    let staged = runtime
        .resolve_connection_model_selection(&configuration, "one", "deepseek", "deepseek-v4-pro")
        .expect("selection must retain the published generation before activation");
    assert_eq!(
        staged.route.protocol_route.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "a staged manifest must remain invisible before the explicit activation boundary"
    );
    runtime
        .activate_staged_manifest_catalogs(&configuration, &[])
        .await
        .expect("replacement deepseek generation should publish");

    let after = runtime
        .resolve_connection_model_selection(&configuration, "one", "deepseek", "deepseek-v4-pro")
        .expect("replaced binding must resolve");
    assert_eq!(
        after.route.protocol_route.protocol_profile_id, OPENAI_CHAT_PROFILE_ID,
        "the hot-replaced manifest must drive the next resolution"
    );
}

/// R0/P1-01: a system binding without its variant stays visible but cannot execute.
/// R0/P1-01：缺少协议变体的系统绑定保持可见但不能执行。
///
/// Business invariant (plan §6.5, R5): any system execution must prove that its bound
/// protocol, route, variant, and profile agree; an empty `protocol_variants` map is never
/// a compatibility bypass for a system-manifest binding.
/// 业务不变量（方案 §6.5、阶段 R5）：任何系统执行都必须证明其绑定协议、路由、
/// 变体与 Profile 一致；空 `protocol_variants` 映射绝不是系统清单绑定的兼容绕过。
#[test]
fn system_manifest_binding_without_variant_is_model_level_blocked() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let mut deepseek_connection = connection("one");
    deepseek_connection.provider_id = String::from("deepseek");
    deepseek_connection.service_entry_id = String::from("official-api");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            deepseek_connection,
        )
        .expect("connection should register");
    let catalog = parse_openai_model_catalog(
        "one",
        1,
        r#"{"data":[{"id":"deepseek-v4-pro","context_window":131072,"max_output_tokens":8192,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
    )
    .expect("openai catalog should parse");
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("catalog should install");
    configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("one incompatible discovered model must not block the provider generation");
    // brokenCatalog starts from a valid published DeepSeek catalog before removing the exact
    // bound variant to exercise the request-boundary fail-closed guard directly.
    // brokenCatalog 从有效的已发布 DeepSeek 目录开始，再移除精确绑定变体，以直接验证
    // 请求边界的失败关闭守卫。
    let published = runtime
        .catalog_snapshot()
        .expect("catalog should remain readable");
    let mut broken_catalog = published["one"].clone();
    // model is the exact system-bound identity whose selected variant is corrupted by the fixture.
    // model 是夹具刻意破坏所选变体的精确系统绑定身份。
    let model = broken_catalog
        .models
        .iter_mut()
        .find(|model| model.id == "deepseek-v4-pro")
        .expect("DeepSeek V4 Pro should be published");
    // boundProtocol identifies the only variant whose absence must block execution.
    // boundProtocol 标识缺失后必须阻止执行的唯一绑定变体。
    let bound_protocol = model
        .resolved_protocol_binding
        .as_ref()
        .expect("system model must have a frozen binding")
        .protocol_profile_id
        .clone();
    assert!(model.protocol_variants.remove(&bound_protocol).is_some());
    model.capabilities = ModelCapabilities::default();
    // candidateGeneration publishes the intentionally broken catalog atomically for selection.
    // candidateGeneration 为请求选择原子发布刻意破坏的目录。
    let mut candidate_generation = runtime
        .selection_catalog_snapshot()
        .expect("published generation should load")
        .as_ref()
        .clone();
    candidate_generation.configuration = Arc::new(configuration.clone());
    candidate_generation
        .activated_effective_catalogs
        .replace(broken_catalog);
    *runtime
        .resolved_provider_generation
        .write()
        .expect("published generation lock should remain healthy") = Arc::new(candidate_generation);

    let error = runtime
        .resolve_connection_model_selection(&configuration, "one", "deepseek", "deepseek-v4-pro")
        .map(|_| ())
        .expect_err("a bound system model without its variant must fail execution");
    assert!(
        error.contains("variant") || error.contains("capability"),
        "unexpected fail-closed error: {error}"
    );
}
