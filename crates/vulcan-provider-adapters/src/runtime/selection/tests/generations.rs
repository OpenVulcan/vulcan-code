//! Immutable selection across unpublished mutations and concurrent activation.
//! 未发布变更及并发激活期间的不可变选择。

use crate::runtime::PublicAiRuntime;
use crate::runtime::test_support::{
    connection, custom_connection, install_current_custom_provider, install_source_catalog,
    multi_protocol_custom_definition, with_protocol_variants,
};
use crate::{PersistedAiConfiguration, parse_openai_model_catalog};
use std::sync::Arc;
use vulcan_llm_core::{
    ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// R0/P1-03: an unactivated configuration mutation is invisible to selection.
/// R0/P1-03：未激活的配置内存变更对选择不可见。
///
/// Business invariant (plan §6.2/§6.3): the effective catalog is generated at an explicit
/// activation boundary (save/manifest activation/manual refresh). Mutating the in-memory
/// configuration object after activation must not change existing or new selections until
/// the next activation; selection must read the activated effective catalog instead of
/// rebuilding overlays at the request boundary.
/// 业务不变量（方案 §6.2/§6.3）：有效目录在显式激活边界（保存/清单激活/手动刷新）生成。
/// 激活后修改内存配置对象不得改变既有或新的选择，直到下一次激活；选择必须读取
/// 已激活有效目录，而不是在请求边界重建覆盖。
#[test]
fn unactivated_configuration_mutation_is_invisible_to_selection() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let provider = multi_protocol_custom_definition(
        "custom:frozen",
        &[
            crate::CustomProviderProtocol::OpenAiChat,
            crate::CustomProviderProtocol::AnthropicMessages,
            crate::CustomProviderProtocol::OpenAiResponses,
        ],
    );
    install_current_custom_provider(&mut configuration, provider);
    let effective_providers = runtime
        .effective_providers(&configuration)
        .expect("effective providers should resolve");
    configuration
        .connection_graph
        .add_connection(
            &effective_providers,
            custom_connection("frozen-account", "custom:frozen"),
        )
        .expect("frozen connection should register");
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "frozen-account",
            1,
            r#"{"data":[{"id":"frozen/model","context_window":8192,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("frozen catalog should parse"),
        &[
            OPENAI_CHAT_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
        ],
    );
    install_source_catalog(&runtime, catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("frozen catalog should install");
    configuration.set_model_enabled("custom:frozen", "frozen/model", true);
    // The activation boundary freezes the auto binding (highest fixed priority).
    // 激活边界冻结 Auto 绑定（最高固定优先级）。
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("overlays should reapply");
    let binding = runtime
        .resolve_connection_model_selection(
            &configuration,
            "frozen-account",
            "custom:frozen",
            "frozen/model",
        )
        .expect("frozen model must resolve")
        .protocol_binding;
    assert_eq!(binding.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID);

    // Mutating the in-memory configuration without any activation must be invisible to
    // the next selection; only an explicit activation may rebind.
    // 未经过任何激活的内存配置变更对下一次选择必须不可见；只有显式激活才可重新绑定。
    configuration.custom_model_protocol_preferences.insert(
        PersistedAiConfiguration::provider_model_key("custom:frozen", "frozen/model"),
        crate::CustomModelProtocolPreference::Explicit {
            protocol: crate::CustomProviderProtocol::AnthropicMessages,
        },
    );
    let still_frozen = runtime
        .resolve_connection_model_selection(
            &configuration,
            "frozen-account",
            "custom:frozen",
            "frozen/model",
        )
        .expect("frozen model must resolve")
        .protocol_binding;
    assert_eq!(
        still_frozen.protocol_profile_id, OPENAI_RESPONSES_PROFILE_ID,
        "an unactivated preference mutation must not rebind the model"
    );
}

/// R3/§R3.4: concurrent selection observes one whole old or one whole new generation.
/// R3/§R3.4：并发选择观察到完整旧代或完整新代。
///
/// Business invariant (plan §R3.4): every selection snapshot pairs the catalog generation
/// with the manifest it read; a reader must never observe the new manifest under the old
/// generation or vice versa.
/// 业务不变量（方案 §R3.4）：每个选择快照把目录代数与它所读清单配对；读取方绝不
/// 会在旧代数下观察到新清单，反之亦然。
#[test]
fn concurrent_selection_observes_whole_generations() {
    let runtime = std::sync::Arc::new(PublicAiRuntime::unavailable().expect("runtime"));
    let mut configuration = PersistedAiConfiguration::default();
    let mut deepseek_connection = connection("generation-account");
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
        .expect("deepseek connection should register");
    let catalog = with_protocol_variants(
        parse_openai_model_catalog(
            "generation-account",
            1,
            r#"{"data":[{"id":"deepseek-v4-pro","context_window":131072,"max_output_tokens":8192,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
        )
        .expect("deepseek catalog should parse"),
        &[ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID],
    );
    install_source_catalog(runtime.as_ref(), catalog.clone());
    runtime
        .replace_catalog(catalog)
        .expect("deepseek catalog should install");
    configuration.set_model_enabled("deepseek", "deepseek-v4-pro", true);
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("initial selection generation should publish");
    let old_revision = runtime
        .system_manifest("deepseek")
        .expect("DeepSeek manifest must exist")
        .revision;
    let new_revision = old_revision.saturating_add(1);
    let configuration = Arc::new(configuration);
    let mut handles = Vec::new();
    for _ in 0..8 {
        let runtime = std::sync::Arc::clone(&runtime);
        let configuration = Arc::clone(&configuration);
        handles.push(std::thread::spawn(move || {
            for _ in 0..200 {
                let selection = runtime
                    .resolve_connection_model_selection(
                        configuration.as_ref(),
                        "generation-account",
                        "deepseek",
                        "deepseek-v4-pro",
                    )
                    .expect("real selection API must resolve");
                let binding = selection.protocol_binding;
                let expected_protocol = match binding.manifest_revision {
                    revision if revision == old_revision => OPENAI_RESPONSES_PROFILE_ID,
                    revision if revision == new_revision => OPENAI_CHAT_PROFILE_ID,
                    revision => panic!("unexpected published revision {revision}"),
                };
                assert_eq!(binding.protocol_profile_id, expected_protocol);
                assert_eq!(
                    selection.route.protocol_route.protocol_profile_id, expected_protocol,
                    "one real selection must not mix manifest and route generations"
                );
            }
        }));
    }
    // The writer stages a new manifest and then publishes it with the existing model
    // catalog at an explicit activation boundary while real selection calls sample.
    // 写入方暂存新 Manifest，并在显式激活边界把它与现有模型目录共同发布，同时真实选择
    // 调用持续采样。
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join("deepseek.json"))
        .expect("bundled deepseek manifest must exist");
    let mut value: crate::provider_manifest::ProviderManifestV1 =
        serde_json::from_str(&text).expect("manifest");
    value.revision = new_revision;
    for rule in &mut value.model_rules {
        if rule.matcher.value == "deepseek-v4-pro" {
            rule.selected_protocol = Some(String::from("openai-chat-completions.v1"));
        }
    }
    let replaced = crate::provider_manifest::compile_manifest_text(
        &serde_json::to_string(&value).expect("serialize"),
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("replacement must compile");
    let async_runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test async runtime should initialize");
    for _ in 0..200 {
        runtime
            .replace_system_manifest("deepseek", Arc::new(replaced.clone()))
            .expect("swap must succeed");
        async_runtime
            .block_on(runtime.activate_staged_manifest_catalogs(configuration.as_ref(), &[]))
            .expect("complete selection generation must publish");
    }
    for handle in handles {
        handle.join().expect("reader thread must not panic");
    }
    let published = runtime
        .resolve_connection_model_selection(
            configuration.as_ref(),
            "generation-account",
            "deepseek",
            "deepseek-v4-pro",
        )
        .expect("the activated selection must resolve");
    assert_eq!(
        published.protocol_binding.manifest_revision, new_revision,
        "the writer must actually publish the staged generation"
    );
}
