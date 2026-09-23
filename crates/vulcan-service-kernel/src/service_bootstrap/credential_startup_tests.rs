//! Regression coverage for persisted credentials entering a fresh runtime generation.
//! 持久化凭据进入新运行时代际的回归覆盖。

use super::*;
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, PersistedAiConfiguration, PublicAiRuntime,
};

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
