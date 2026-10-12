//! Regression coverage for persisted credentials entering a fresh runtime generation.
//! 持久化凭据进入新运行时代际的回归覆盖。

use super::*;
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, PersistedAiConfiguration, PublicAiRuntime,
};

/// Reproduces unsupported persisted credentials through the actual startup publication boundary.
/// 通过真实启动发布边界复现不受支持的持久化凭据。
/// No inputs; checks repair visibility, healthy siblings, strict update atomicity, and unchanged storage.
/// 无输入；检查修复可见性、健康连接、严格更新原子性以及存储不变。
#[test]
fn credential_repair_startup_isolates_old_accounts_without_rewriting_configuration() {
    // async_runtime initializes only private SQLite fixtures and catalog operations.
    // async_runtime 仅初始化私有 SQLite 夹具和目录操作。
    let async_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("fixture runtime");
    // fixture owns a production-shaped database that never resolves the user profile.
    // fixture 持有生产形态数据库，绝不解析用户配置。
    let fixture = async_runtime
        .block_on(crate::ai_credential_secret_store::repair_tests::CredentialRepairFixture::new());
    // saved_records retain exact evidence that startup neither migrates nor removes old credentials.
    // saved_records 保留启动既不迁移也不删除旧凭据的精确证据。
    let mut saved_records = Vec::new();
    // configuration contains the same three unsupported credential connections as the reported failure.
    // configuration 包含与所报告故障数量一致的三个不受支持凭据连接。
    let mut configuration = PersistedAiConfiguration::default();
    for index in 0..3 {
        // connection_id provides an exact stable identity for both storage and runtime assertions.
        // connection_id 为存储和运行时断言提供精确稳定身份。
        let connection_id = format!("old-account-{index}");
        saved_records.push(async_runtime.block_on(fixture.insert(&connection_id, 1)));
        configuration.connection_graph.connections.insert(
            connection_id.clone(),
            CredentialConnection {
                id: connection_id.clone(),
                provider_id: String::from("openrouter"),
                service_entry_id: String::from("public-api"),
                credential_profile_id: String::from("api-key"),
                credential_mode: CredentialMode::Token,
                credential_id: connection_id.clone(),
                display_name: connection_id.clone(),
                secret_reference: connection_id,
                management_secret_reference: None,
                service_origin_override: None,
                prompt_cache_by_protocol: BTreeMap::new(),
            },
        );
    }
    for include_healthy in [false, true] {
        if include_healthy {
            // healthy uses a signed static model catalog so the test performs no provider network calls.
            // healthy 使用签名静态模型目录，使测试不执行供应商网络调用。
            let mut healthy = configuration
                .connection_graph
                .connections
                .values()
                .next()
                .expect("fixture connection")
                .clone();
            healthy.id = String::from("healthy-account");
            healthy.credential_id = healthy.id.clone();
            healthy.secret_reference = healthy.id.clone();
            healthy.provider_id = String::from("alibaba");
            healthy.service_entry_id = String::from("token-plan-personal-cn");
            healthy.credential_profile_id = String::from("token-plan-api-key");
            async_runtime.block_on(fixture.insert(
                &healthy.secret_reference,
                vulcan_provider_adapters::CREDENTIAL_SECRET_FORMAT_VERSION,
            ));
            configuration
                .connection_graph
                .connections
                .insert(healthy.id.clone(), healthy);
        }
        vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
            &configuration,
            fixture.directory.path(),
        )
        .expect("persist configuration");
        // path and bytes identify the authoritative persisted image before both fresh boots.
        // path 和 bytes 标识两次全新启动之前的权威持久化镜像。
        let path = fixture.directory.path().join("ai-connections.toml");
        let bytes = std::fs::read(&path).expect("saved bytes");
        for _ in 0..2 {
            // runtime starts empty exactly as the real bootstrap does.
            // runtime 与真实引导一样从空状态启动。
            let runtime = PublicAiRuntime::first_batch(fixture.store.clone())
                .expect("fresh provider runtime");
            // loaded is read back through the production configuration parser.
            // loaded 通过生产配置解析器回读。
            let loaded =
                vulcan_provider_config::config::load_public_ai_connections_file_at_path(&path)
                    .expect("load configuration")
                    .configuration;
            // ids select every configured account for the strict-update regression.
            // ids 选择全部已配置账号供严格更新回归使用。
            let ids = loaded
                .connection_graph
                .connections
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            assert!(
                async_runtime
                    .block_on(runtime.activate_staged_manifest_catalogs(&loaded, &ids))
                    .is_err()
            );
            assert!(
                runtime
                    .configuration_snapshot()
                    .expect("unchanged publication")
                    .connection_graph
                    .connections
                    .is_empty()
            );
            // logs are the same controlled diagnostics exposed by the real startup entry.
            // logs 与真实启动入口公开的受控诊断一致。
            let mut logs = Vec::new();
            refresh_public_ai_catalogs_at_startup(&runtime, &loaded, &mut logs)
                .expect("workspace must remain repairable");
            assert_eq!(
                runtime
                    .configuration_snapshot()
                    .expect("published configuration"),
                loaded
            );
            assert_eq!(
                std::fs::read(&path).expect("unchanged configuration"),
                bytes
            );
            // failures and catalogs must separate failed accounts from a healthy executable sibling.
            // failures 和 catalogs 必须将失败账号与健康可执行连接分开。
            let failures = runtime
                .catalog_error_snapshot()
                .expect("repair diagnostics");
            let catalogs = runtime.catalog_snapshot().expect("published catalogs");
            assert_eq!(failures.len(), saved_records.len());
            for original in &saved_records {
                assert!(failures.contains_key(&original.secret_reference));
                assert!(!catalogs.contains_key(&original.secret_reference));
                assert!(
                    async_runtime
                        .block_on(fixture.record(&original.secret_reference))
                        .as_ref()
                        == Some(original)
                );
            }
            if include_healthy {
                assert!(
                    !catalogs
                        .get("healthy-account")
                        .expect("healthy sibling catalog")
                        .models
                        .is_empty()
                );
            } else {
                assert!(catalogs.is_empty());
            }
        }
    }
}

/// Reopens a saved configuration in two fresh runtimes and checks exact credential preservation.
/// 在两个全新运行时重开保存的配置，验证凭据精确保留。
/// Uses a disabled connection so startup publication is exercised without secret or network access.
/// 使用停用连接，在不访问密钥或网络的情况下验证启动发布。
#[test]
fn startup_restores_saved_credentials_into_fresh_generation() {
    // The isolated file represents the real persisted startup input, never an already-published runtime.
    // 隔离文件代表真实持久化启动输入，而非已发布运行时。
    let directory = tempfile::tempdir().expect("isolated configuration directory");
    let path = directory.path().join("ai-connections.toml");
    let mut configuration = PersistedAiConfiguration::default();
    let connection_id = "saved-startup-account";
    configuration.connection_graph.connections.insert(
        connection_id.into(),
        CredentialConnection {
            id: connection_id.into(),
            provider_id: "openrouter".into(),
            service_entry_id: "public-api".into(),
            credential_profile_id: "api-key".into(),
            credential_mode: CredentialMode::Token,
            credential_id: "saved-startup-credential".into(),
            display_name: "Saved account".into(),
            secret_reference: "saved-startup-secret".into(),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: BTreeMap::new(),
        },
    );
    configuration
        .set_connection_enabled(connection_id, false)
        .expect("disable fixture network access");
    vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
        &configuration,
        directory.path(),
    )
    .expect("persist fixture before startup");
    let saved_bytes = std::fs::read(&path).expect("saved file bytes");
    // Repeat startup with no shared runtime: both boots must publish the durable candidate.
    // 两次启动不共享运行时：两次都必须发布持久候选。
    for _ in 0..2 {
        let runtime = PublicAiRuntime::unavailable().expect("fresh runtime");
        let loaded = vulcan_provider_config::config::load_public_ai_connections_file_at_path(&path)
            .expect("load saved configuration")
            .configuration;
        let mut logs = Vec::new();
        refresh_public_ai_catalogs_at_startup(&runtime, &loaded, &mut logs)
            .expect("publish startup generation");
        assert_eq!(
            runtime
                .configuration_snapshot()
                .expect("published configuration"),
            loaded,
            "Startup must publish saved credentials instead of the fresh empty runtime configuration"
        );
        assert_eq!(
            std::fs::read(&path).expect("unchanged saved bytes"),
            saved_bytes
        );
        assert!(
            runtime
                .runtime_configuration_snapshot(&loaded)
                .expect("runtime configuration")
                .connection_graph
                .connections
                .contains_key(connection_id)
        );
    }
}
