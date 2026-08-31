//! Exact account-owned reference cleanup regressions.
//! 精确账号所属引用清理的回归测试。

use super::super::oauth::PublicAiOAuthCommitRequest;
use super::*;
use std::collections::BTreeSet;
use vulcan_provider_adapters::{
    ActiveAiModelSelection, MediaUnderstandingBackendOptions, ProviderToolCapabilityKind,
    ProviderToolTargetSelection,
};

/// Verifies account removal clears every exact media and native-extension reference it owns.
/// 验证账号移除会清理其持有的全部精确媒体与原生扩展引用。
#[test]
fn connection_removal_clears_exact_media_and_extension_references() {
    // removedConnection is the only configured account and therefore needs no replacement default.
    // removedConnection 是唯一已配置账号，因此不需要替代默认账号。
    let removed_connection = CredentialConnection {
        id: String::from("removed-account"),
        provider_id: String::from("removed-provider"),
        service_entry_id: String::from("chat"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("removed-credential"),
        display_name: String::from("Removed Account"),
        secret_reference: String::from("removed-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    // removedTarget and retainedTarget prove cleanup is exact rather than capability-wide.
    // removedTarget 与 retainedTarget 证明清理按精确账号执行，而不是整项能力清空。
    let removed_target = ProviderToolTargetSelection {
        connection_id: removed_connection.id.clone(),
        target_id: String::from("removed-target"),
    };
    let retained_target = ProviderToolTargetSelection {
        connection_id: String::from("retained-account"),
        target_id: String::from("retained-target"),
    };
    // configuration contains both media backend modes plus a mixed native-extension set.
    // configuration 同时包含两种媒体后端模式与一个混合原生扩展集合。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .connections
        .insert(removed_connection.id.clone(), removed_connection.clone());
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::ImageUnderstand,
        MediaUnderstandingBackendSelection::ProviderTool {
            selection: removed_target.clone(),
        },
    );
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::VideoUnderstand,
        MediaUnderstandingBackendSelection::ModelReplacement {
            selection: ActiveAiModelSelection {
                provider_id: removed_connection.provider_id.clone(),
                model_id: String::from("video-model"),
                connection_id: removed_connection.id.clone(),
            },
            reasoning_effort: None,
        },
    );
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::AudioUnderstand,
        MediaUnderstandingBackendSelection::ProviderTool {
            selection: retained_target.clone(),
        },
    );
    configuration.media_understanding_backend_options.insert(
        ProviderToolCapabilityKind::ImageUnderstand,
        MediaUnderstandingBackendOptions {
            provider_tool: Some(removed_target.clone()),
            ..MediaUnderstandingBackendOptions::default()
        },
    );
    configuration.media_understanding_backend_options.insert(
        ProviderToolCapabilityKind::VideoUnderstand,
        MediaUnderstandingBackendOptions {
            model_replacement: Some(
                vulcan_provider_adapters::MediaUnderstandingModelReplacementSelection {
                    selection: ActiveAiModelSelection {
                        provider_id: removed_connection.provider_id.clone(),
                        model_id: String::from("video-model"),
                        connection_id: removed_connection.id.clone(),
                    },
                    reasoning_effort: None,
                },
            ),
            ..MediaUnderstandingBackendOptions::default()
        },
    );
    configuration.media_understanding_backend_options.insert(
        ProviderToolCapabilityKind::AudioUnderstand,
        MediaUnderstandingBackendOptions {
            provider_tool: Some(retained_target.clone()),
            ..MediaUnderstandingBackendOptions::default()
        },
    );
    configuration.provider_extension_target_sets.insert(
        ProviderToolCapabilityKind::ImageGenerate,
        BTreeSet::from([removed_target, retained_target.clone()]),
    );

    remove_connection_from_configuration(&mut configuration, removed_connection.id.as_str(), None)
        .expect("connection removal should succeed");

    assert!(
        !configuration
            .connection_graph
            .connections
            .contains_key(removed_connection.id.as_str())
    );
    assert_eq!(configuration.media_understanding_selections.len(), 1);
    assert!(
        configuration
            .media_understanding_selections
            .contains_key(&ProviderToolCapabilityKind::AudioUnderstand)
    );
    assert_eq!(configuration.media_understanding_backend_options.len(), 1);
    assert_eq!(
        configuration
            .media_understanding_backend_options
            .get(&ProviderToolCapabilityKind::AudioUnderstand)
            .and_then(|options| options.provider_tool.as_ref()),
        Some(&retained_target)
    );
    assert_eq!(
        configuration
            .provider_extension_target_sets
            .get(&ProviderToolCapabilityKind::ImageGenerate),
        Some(&BTreeSet::from([retained_target]))
    );
}

/// Verifies connection mutation RPCs never accept a user-selected protocol authority.
/// 验证连接变更 RPC 绝不接受用户选择的协议权威。
#[test]
fn connection_requests_reject_user_selected_protocol() {
    // requests are the exact browser payloads after connection-level protocol selection was removed.
    // requests 是移除连接级协议选择后的精确浏览器载荷。
    let requests = [
        (
            "add",
            serde_json::json!({
                "providerId": "openrouter",
                "serviceEntryId": "public-api",
                "credentialProfileId": "api-key",
                "credentialMode": "token",
                "displayName": "Primary",
                "secret": "test-secret"
            }),
        ),
        (
            "update",
            serde_json::json!({
                "connectionId": "connection",
                "displayName": "Primary",
                "serviceEntryId": "public-api",
                "credentialProfileId": "api-key",
                "credentialMode": "token"
            }),
        ),
        (
            "oauth",
            serde_json::json!({
                "transactionId": "transaction",
                "providerId": "openai",
                "serviceEntryId": "codex-subscription",
                "credentialProfileId": "chatgpt-oauth",
                "displayName": "Subscription"
            }),
        ),
    ];

    for (kind, request) in requests {
        let accepted = match kind {
            "add" => {
                serde_json::from_value::<PublicAiConnectionAddRequest>(request.clone()).map(|_| ())
            }
            "update" => serde_json::from_value::<PublicAiConnectionUpdateRequest>(request.clone())
                .map(|_| ()),
            "oauth" => {
                serde_json::from_value::<PublicAiOAuthCommitRequest>(request.clone()).map(|_| ())
            }
            _ => unreachable!("closed request fixture kind"),
        };
        accepted.unwrap_or_else(|error| panic!("{kind} request should deserialize: {error}"));

        let mut forbidden = request;
        forbidden["selectedProtocol"] =
            serde_json::Value::String(String::from("openai-chat-completions.v1"));
        let rejected = match kind {
            "add" => serde_json::from_value::<PublicAiConnectionAddRequest>(forbidden).map(|_| ()),
            "update" => {
                serde_json::from_value::<PublicAiConnectionUpdateRequest>(forbidden).map(|_| ())
            }
            "oauth" => serde_json::from_value::<PublicAiOAuthCommitRequest>(forbidden).map(|_| ()),
            _ => unreachable!("closed request fixture kind"),
        };
        let error = rejected.expect_err("selectedProtocol must be rejected");
        assert!(error.to_string().contains("selectedProtocol"));
    }
}
