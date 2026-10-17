//! Reauthorization cache ownership and compensation across stable connection identifiers.
//! 稳定连接标识下重新授权的缓存归属与补偿。

use super::*;
use crate::ai_credential_secret_store::repair_tests::CredentialRepairFixture;
use vulcan_provider_adapters::{CredentialConnection, CredentialSecretMaterial, OAuthTokenSet};

/// Builds an isolated runtime with two cached accounts and no provider traffic.
/// 构建具有两个已缓存账号的隔离运行时，不产生供应商流量。
/// No parameters; returns the owned SQLite fixture, runtime, and exact connection configuration.
/// 无参数；返回持有的 SQLite 夹具、运行时及精确连接配置。
async fn cached_runtime() -> (
    CredentialRepairFixture,
    PublicAiRuntime,
    PersistedAiConfiguration,
) {
    // fixture keeps every secret and database file below a private temporary directory.
    // fixture 将全部秘密与数据库文件保存在私有临时目录下。
    let fixture = CredentialRepairFixture::new().await;
    // runtime uses the actual bundled manifests and production SQLite credential adapter.
    // runtime 使用真实随包清单与生产 SQLite 凭据适配器。
    let runtime = PublicAiRuntime::first_batch(fixture.store.clone()).unwrap();
    // configuration starts with two legacy Go Key connections whose usage primitive performs no I/O.
    // configuration 从两个旧 Go Key 连接开始，其用量原语不执行 I/O。
    let mut configuration = PersistedAiConfiguration::default();
    for id in ["reauthorized-account", "retained-account"] {
        fixture
            .store
            .create_secret(
                id,
                &CredentialSecretEnvelope::new_api_key(String::from("sk-synthetic-key")).unwrap(),
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
    (fixture, runtime, configuration)
}

/// Verifies only the replaced account is invalidated, and a failed validation retains a recoverable before-image.
/// 验证仅被替换账号失效，且验证失败时保留可恢复的前镜像。
/// No parameters; asserts cache isolation, real credential replacement, and exact compensation in both cases.
/// 无参数；断言两种情况下的缓存隔离、真实凭据替换及精确补偿。
#[tokio::test]
async fn reauthorization_discards_previous_usage_and_preserves_rollback_image() {
    for invalid_catalog in [false, true] {
        // fixture, runtime, and configuration own this independent authorization attempt.
        // fixture、runtime 与 configuration 持有此次独立授权尝试。
        let (fixture, runtime, mut configuration) = cached_runtime().await;
        // previous_usage and before_image preserve both account observations before replacement.
        // previous_usage 与 before_image 在替换前保留两个账号观测。
        let previous_usage = runtime.usage_cache_snapshot().unwrap();
        let before_image = runtime.capture_activation_snapshot().unwrap();
        // replacement is a synthetic Console grant for a different billing workspace.
        // replacement 是另一计费工作区的合成 Console 授权。
        let (previous_secret, replacement) =
            super::super::credential_repair::replace_credential_material(
                fixture.store.as_ref(),
                "reauthorized-account",
                CredentialSecretMaterial::OpencodeConsoleOAuth {
                    tokens: OAuthTokenSet {
                        access_token: String::from("synthetic-access"),
                        refresh_token: String::from("synthetic-refresh"),
                        issued_at_unix_ms: 1,
                        expires_at_unix_ms: 2,
                        scopes: Default::default(),
                        account_subject: Some(String::from("user_new")),
                        email: None,
                    },
                    organization_id: String::from("org_new"),
                },
            )
            .await
            .unwrap();
        // connection changes to the declared OAuth profile while retaining its stable cache key.
        // connection 切换为声明的 OAuth 配置档，同时保留稳定缓存键。
        let connection = configuration
            .connection_graph
            .connections
            .get_mut("reauthorized-account")
            .unwrap();
        connection.service_entry_id = String::from("go-oauth-api");
        connection.credential_profile_id = String::from("account-oauth");
        connection.credential_mode = CredentialMode::Auth;
        if invalid_catalog {
            connection.service_entry_id = String::from("unregistered-entry");
        }
        // result crosses the exact production invalidation and catalog-validation boundary.
        // result 跨越精确的生产缓存失效与目录验证边界。
        let result = refresh_reauthorized_connection_catalog(
            &runtime,
            &configuration,
            "reauthorized-account",
        )
        .await;
        assert_eq!(result.is_err(), invalid_catalog);
        // current_usage must preserve the other account without reusing the previous workspace's quota.
        // current_usage 必须保留另一账号，且不得复用上一工作区额度。
        let current_usage = runtime.usage_cache_snapshot().unwrap();
        assert!(!current_usage.contains_key("reauthorized-account"));
        assert!(current_usage.contains_key("retained-account"));
        if invalid_catalog {
            rollback_oauth_reauthorization_secret(
                fixture.store.as_ref(),
                "reauthorized-account",
                previous_secret.as_ref(),
                &replacement,
            )
            .await
            .unwrap();
            restore_public_ai_runtime_activation(&runtime, &before_image).unwrap();
            assert_eq!(runtime.usage_cache_snapshot().unwrap(), previous_usage);
            assert_eq!(
                fixture
                    .record("reauthorized-account")
                    .await
                    .unwrap()
                    .material_kind,
                "api_key"
            );
        } else {
            assert_eq!(
                fixture
                    .record("reauthorized-account")
                    .await
                    .unwrap()
                    .account_id
                    .as_deref(),
                Some("org_new")
            );
        }
    }
}
