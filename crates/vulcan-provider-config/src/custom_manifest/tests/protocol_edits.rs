//! Protocol edits must persist a complete engine contract without erasing authored dependencies.
//! 协议编辑必须持久化完整引擎契约，同时保留已编写的依赖。

use super::*;
use vulcan_provider_adapters::{
    CustomProviderAuthentication, CustomProviderDefinition, CustomProviderModelDiscoveryMode,
    CustomProviderProtocol,
};

/// Builds a current discovery-enabled descriptor for `provider_id` and its explicit `protocol`.
/// 为 `provider_id` 及其显式 `protocol` 构建启用发现的当前描述符。
/// Returns the production descriptor projection; invalid fixture data fails immediately.
/// 返回生产描述符投影；夹具数据无效时立即失败。
fn descriptor(provider_id: &str, protocol: CustomProviderProtocol) -> ProviderDescriptor {
    CustomProviderDefinition {
        id: provider_id.into(),
        display_name: "Protocol edit fixture".into(),
        base_url: "https://protocol-edit.example.invalid/v1".into(),
        protocols: vec![protocol],
        authentication: CustomProviderAuthentication::BearerAuthorization,
        model_discovery_mode: CustomProviderModelDiscoveryMode::Standard,
        model_discovery_protocol:
            vulcan_provider_adapters::CustomProviderModelDiscoveryProtocol::OpenAi,
        model_discovery_path: None,
        protocol_routes: Vec::new(),
        request_headers: BTreeMap::new(),
        session_header: None,
    }
    .into_descriptor()
    .expect("custom descriptor should validate")
}

/// Verifies catalog-only edits survive disk reload without changing conversation bindings.
/// 验证仅目录编辑可经磁盘重载保留，且不改变会话绑定。
/// Uses private temporary roots and performs no provider or credential requests.
/// 使用私有临时根目录，不发起供应商或凭据请求。
#[test]
fn custom_catalog_selection_and_path_survive_save_and_reload() {
    // directory isolates all disk writes from live settings.
    // directory 将全部磁盘写入与真实设置隔离。
    let directory = tempfile::tempdir().expect("private configuration root");
    // configuration carries successive explicit catalog edits across reloads.
    // configuration 承载跨重载的连续显式目录编辑。
    let mut configuration = PersistedAiConfiguration::default();
    // provider_id fixes the identity throughout catalog-format changes.
    // provider_id 在目录格式变更期间固定供应商身份。
    let provider_id = "custom:catalog-selection";
    // provider exposes only Anthropic conversation calls while allowing independent catalog formats.
    // provider 只提供 Anthropic 会话调用，同时允许独立目录格式。
    let provider = descriptor(provider_id, CustomProviderProtocol::AnthropicMessages);
    // compiled_original includes normal enrichment before comparing later catalog-only edits.
    // compiled_original 在比较后续仅目录编辑前包含正常补全。
    let compiled_original = vulcan_provider_adapters::compile_manifest(
        &vulcan_provider_adapters::project_custom_descriptor_manifest(&provider).unwrap(),
        ManifestOwnership::Custom,
    )
    .unwrap();
    // original_routes records the unchanged execution contract after normal compilation.
    // original_routes 记录正常编译后应保持不变的执行契约。
    let original_routes = compiled_original
        .provider()
        .service_entries
        .first()
        .unwrap()
        .protocol_routes
        .clone();
    configuration
        .custom_providers
        .insert(provider_id.into(), provider);
    for kind in [
        vulcan_provider_adapters::ModelDiscoveryKind::OpenAi,
        vulcan_provider_adapters::ModelDiscoveryKind::AnthropicCompatible,
        vulcan_provider_adapters::ModelDiscoveryKind::Gemini,
    ] {
        // entry changes only the chosen directory contract; conversation routes remain identical.
        // entry 只改变所选目录契约；会话路由保持相同。
        let entry = configuration
            .custom_providers
            .get_mut(provider_id)
            .unwrap()
            .service_entries
            .first_mut()
            .unwrap();
        entry.model_discovery = kind;
        entry.model_discovery_path = Some(String::from("/shared-catalog"));
        synchronize_custom_provider_manifest(&mut configuration, provider_id).unwrap();
        crate::config::save_public_ai_connections_with_custom_manifests(
            &configuration,
            directory.path(),
        )
        .unwrap();
        configuration = crate::config::load_public_ai_connections_file_at_path(
            &directory.path().join("ai-connections.toml"),
        )
        .unwrap()
        .configuration;
        // saved is reloaded through the same standalone-manifest path used at startup.
        // saved 通过与启动相同的独立清单路径重新加载。
        let saved = configuration
            .custom_providers
            .get(provider_id)
            .expect("saved provider must load")
            .service_entries
            .first()
            .unwrap();
        assert_eq!(saved.model_discovery, kind);
        assert_eq!(
            saved.model_discovery_path.as_deref(),
            Some("/shared-catalog")
        );
        assert_eq!(saved.protocol_routes, original_routes);
    }
}

/// Verifies every Anthropic, Chat, and Responses protocol switch survives synchronization and disk reload.
/// 验证 Anthropic、Chat 与 Responses 之间每种协议切换均可通过同步及磁盘重载。
/// No inputs or return value; all writes use private temporary roots and no provider requests.
/// 无输入或返回值；所有写入使用私有临时根目录，且不请求供应商。
#[test]
fn custom_protocol_edit_persists_complete_engine_dependencies() {
    // protocols cover both the reported Responses transition and the inverse Chat/Anthropic edits.
    // protocols 覆盖已报告的 Responses 切换及反向 Chat、Anthropic 编辑。
    let protocols = [
        CustomProviderProtocol::AnthropicMessages,
        CustomProviderProtocol::OpenAiChat,
        CustomProviderProtocol::OpenAiResponses,
    ];
    for previous_protocol in protocols {
        for next_protocol in protocols {
            if previous_protocol == next_protocol {
                continue;
            }
            // provider_id identifies one isolated custom document across its successive revisions.
            // provider_id 在连续修订间标识同一份隔离自定义文档。
            let provider_id = "custom:protocol-edit";
            // directory owns both the central configuration and standalone custom manifest.
            // directory 持有中央配置与独立自定义清单。
            let directory = tempfile::tempdir().expect("private configuration root");
            // original is projected through the same current schema used by provider creation.
            // original 通过供应商创建所用的同一当前 Schema 投影。
            let original = descriptor(provider_id, previous_protocol);
            // manifest includes an authored engine requirement that a descriptor edit must retain.
            // manifest 包含描述符编辑必须保留的已编写引擎依赖。
            let mut manifest =
                vulcan_provider_adapters::project_custom_descriptor_manifest(&original)
                    .expect("original manifest");
            manifest
                .engine_contract
                .required_transformers
                .push("set_literal".into());
            if previous_protocol == CustomProviderProtocol::OpenAiChat {
                manifest.model_defaults.catalog_membership_policy =
                    vulcan_provider_adapters::provider_manifest::CatalogMembershipPolicy::DeclaredOnly;
            }
            // original_membership is independently authored when discovery mode itself stays unchanged.
            // original_membership 在发现模式本身未改变时属于独立编写的策略。
            let original_membership = manifest.model_defaults.catalog_membership_policy;
            // original_revision is used relationally, never as a catalog snapshot constant.
            // original_revision 用于相对比较，绝不作为目录快照常量。
            let original_revision = manifest.revision;
            // configuration preserves user-owned manual models throughout the protocol edit.
            // configuration 在协议编辑期间保留用户自有手工模型。
            let mut configuration = PersistedAiConfiguration::default();
            configuration
                .custom_providers
                .insert(provider_id.into(), original);
            configuration
                .custom_provider_manifests
                .insert(provider_id.into(), manifest);
            configuration.custom_models.insert(
                provider_id.into(),
                BTreeMap::from([("manual-fixture".into(), custom_model("manual-fixture"))]),
            );
            crate::config::save_public_ai_connections_with_custom_manifests(
                &configuration,
                directory.path(),
            )
            .expect("save original provider");
            configuration = crate::config::load_public_ai_connections_file_at_path(
                &directory.path().join("ai-connections.toml"),
            )
            .expect("load original provider")
            .configuration;
            // edited is the explicit new protocol selected by the user after discovery succeeds.
            // edited 是发现成功后用户明确选择的新协议。
            let edited = descriptor(provider_id, next_protocol);
            configuration
                .custom_providers
                .insert(provider_id.into(), edited.clone());
            synchronize_custom_provider_manifest(&mut configuration, provider_id)
                .expect("protocol edit must synchronize the engine contract");
            crate::config::save_public_ai_connections_with_custom_manifests(
                &configuration,
                directory.path(),
            )
            .expect("save edited provider");
            // reloaded reads the persisted bytes through the same strict startup validation.
            // reloaded 通过与启动相同的严格校验读取持久化字节。
            let mut reloaded = crate::config::load_public_ai_connections_file_at_path(
                &directory.path().join("ai-connections.toml"),
            )
            .expect("reload edited provider")
            .configuration;
            // saved retains the required protocol alongside independently authored requirements.
            // saved 在保留独立编写依赖的同时保留所需协议。
            let saved = reloaded
                .custom_provider_manifests
                .get(provider_id)
                .expect("saved custom manifest");
            assert!(
                saved
                    .engine_contract
                    .required_protocol_profiles
                    .iter()
                    .any(|profile| profile == next_protocol.profile_id())
            );
            assert!(
                saved
                    .engine_contract
                    .required_transformers
                    .iter()
                    .any(|primitive| primitive == "set_literal")
            );
            assert_eq!(saved.revision, original_revision + 1);
            assert_eq!(
                saved.model_defaults.catalog_membership_policy,
                original_membership
            );
            // compiled validates the full dependency closure rather than just a JSON field.
            // compiled 校验完整依赖闭包，而非仅检查 JSON 字段。
            let compiled =
                vulcan_provider_adapters::compile_manifest(saved, ManifestOwnership::Custom)
                    .expect("edited manifest must compile");
            assert!(
                vulcan_provider_adapters::custom_manifest_matches_activated_descriptor(
                    compiled.provider(),
                    &edited
                )
            );
            assert_eq!(
                reloaded.custom_model(provider_id, "manual-fixture"),
                Some(&custom_model("manual-fixture"))
            );
            // before proves the next ordinary save does not manufacture another revision.
            // before 证明下一次普通保存不会制造额外修订。
            let before = saved.clone();
            synchronize_custom_provider_manifest(&mut reloaded, provider_id)
                .expect("unchanged edit should remain stable");
            assert_eq!(
                reloaded.custom_provider_manifests.get(provider_id),
                Some(&before)
            );
        }
    }
}
