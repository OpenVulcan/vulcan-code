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
    configuration
        .disabled_connection_ids
        .insert(removed_connection.id.clone());
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
    assert!(
        !configuration
            .disabled_connection_ids
            .contains(removed_connection.id.as_str())
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

/// Verifies the enabled-state request accepts only the exact closed wire contract.
/// 验证启用状态请求仅接受精确封闭的线协议契约。
#[test]
fn connection_enabled_request_is_closed_and_explicit() {
    // accepted contains both required fields and no credential secret material.
    // accepted 包含两个必填字段且不含任何凭据秘密材料。
    let accepted = serde_json::json!({
        "connectionId": "connection-a",
        "enabled": false
    });
    serde_json::from_value::<PublicAiConnectionEnabledSaveRequest>(accepted.clone())
        .expect("exact enabled-state request should deserialize");

    // forbidden proves callers cannot smuggle unrelated mutation authority into this endpoint.
    // forbidden 证明调用方不能把无关变更权限夹带进此端点。
    let mut forbidden = accepted;
    forbidden["secret"] = serde_json::Value::String(String::from("must-not-be-accepted"));
    assert!(serde_json::from_value::<PublicAiConnectionEnabledSaveRequest>(forbidden).is_err());
}

/// Verifies the backend rejects every non-dynamic directory before synchronization can dispatch.
/// 验证后端在同步可能分派前拒绝全部非动态目录。
#[test]
fn model_synchronization_requires_dynamic_directory() {
    assert_eq!(
        validate_model_synchronization_directory(
            "connection-a",
            PublicAiModelDirectoryKind::Dynamic,
        ),
        Ok(())
    );
    for kind in [
        PublicAiModelDirectoryKind::Fixed,
        PublicAiModelDirectoryKind::Manual,
        PublicAiModelDirectoryKind::Unavailable,
    ] {
        let error = validate_model_synchronization_directory("connection-a", kind)
            .expect_err("non-dynamic directories must reject synchronization");
        assert!(error.contains("non-dynamic model directory"));
    }
}

/// Verifies Console key changes discard old usage while preserving unrelated accounts and rollback.
/// 验证 Console 密钥变更丢弃旧用量，同时保留无关账号与回滚能力。
/// No parameters; validates successful changes and restores the exact before-image after rejection.
/// 无参数；验证成功变更，并在拒绝后恢复精确前镜像。
#[tokio::test]
async fn credential_identity_validation_discards_previous_usage_and_preserves_rollback_image() {
    use super::*;
    use crate::ai_credential_secret_store::repair_tests::CredentialRepairFixture;
    for invalid_catalog in [false, true] {
        // fixture owns a private SQLite database; no live credentials or provider requests are used.
        // fixture 持有私有 SQLite 数据库；不使用在线凭据或供应商请求。
        let fixture = CredentialRepairFixture::new().await;
        // runtime uses the production credential adapter and the actual bundled declarations.
        // runtime 使用生产凭据适配器与真实随包声明。
        let runtime = PublicAiRuntime::first_batch(fixture.store.clone()).unwrap();
        // configuration seeds two observations through the legacy unsupported, network-free query.
        // configuration 通过旧版不受支持且无网络访问的查询植入两个观测。
        let mut configuration = PersistedAiConfiguration::default();
        for id in ["changed-account", "retained-account"] {
            fixture
                .store
                .create_secret(
                    id,
                    &CredentialSecretEnvelope::new_api_key(String::from("sk-synthetic-old"))
                        .unwrap(),
                )
                .await
                .unwrap();
            configuration.connection_graph.connections.insert(
                id.to_string(),
                CredentialConnection {
                    id: id.to_string(),
                    provider_id: String::from("opencode-go"),
                    service_entry_id: String::from("go-api"),
                    credential_profile_id: String::from("api-key"),
                    credential_mode: CredentialMode::Token,
                    credential_id: id.to_string(),
                    display_name: id.to_string(),
                    secret_reference: id.to_string(),
                    management_secret_reference: None,
                    service_origin_override: None,
                    prompt_cache_by_protocol: Default::default(),
                },
            );
            runtime
                .query_connection_usage(&configuration, id)
                .await
                .unwrap();
        }
        // previous_usage and before_image preserve account observations for transaction compensation.
        // previous_usage 与 before_image 为事务补偿保留账号观测。
        let previous_usage = runtime.usage_cache_snapshot().unwrap();
        let before_image = runtime.capture_activation_snapshot().unwrap();
        // replacement installs a different workspace-bound Console key under the same connection ID.
        // replacement 在相同连接 ID 下安装另一个已绑定工作区的 Console 密钥。
        let replacement = super::super::credential_repair::replace_credential_material(
            fixture.store.as_ref(),
            "changed-account",
            CredentialSecretMaterial::ApiKey {
                value: String::from("oc_sk_synthetic_new"),
            },
        )
        .await
        .unwrap();
        // connection selects the new static-key profile without changing its stable cache key.
        // connection 选择新的静态密钥凭据档，不改变稳定缓存键。
        let connection = configuration
            .connection_graph
            .connections
            .get_mut("changed-account")
            .unwrap();
        connection.service_entry_id = String::from("go-oauth-api");
        connection.credential_profile_id = String::from("console-api-key");
        if invalid_catalog {
            connection.service_entry_id = String::from("unregistered-entry");
        }
        // result crosses the production entry used only after an explicit credential identity change.
        // result 经过仅在显式凭据身份变更后使用的生产入口。
        let result = validate_connection_credential(
            &runtime,
            &configuration,
            "changed-account",
            ServiceEntryKind::Conversation,
        )
        .await;
        assert_eq!(result.is_err(), invalid_catalog);
        // current_usage must neither retain the changed account nor disturb the other account.
        // current_usage 不得保留变更账号，也不得影响另一账号。
        let current_usage = runtime.usage_cache_snapshot().unwrap();
        assert!(!current_usage.contains_key("changed-account"));
        assert_eq!(
            current_usage.get("retained-account"),
            previous_usage.get("retained-account"),
        );
        if invalid_catalog {
            rollback_replaced_secret(
                fixture.store.as_ref(),
                "changed-account",
                Some(&replacement),
            )
            .await
            .unwrap();
            restore_public_ai_runtime_activation(&runtime, &before_image).unwrap();
            assert_eq!(runtime.usage_cache_snapshot().unwrap(), previous_usage);
        }
    }
}
