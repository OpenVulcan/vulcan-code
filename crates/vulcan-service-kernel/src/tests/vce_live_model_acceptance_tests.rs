//! Explicitly opted-in VCE acceptance using saved model routes and a private temporary index.
//! 显式选择运行的 VCE 验收，使用已保存模型路由及私有临时索引。

use super::test_support::{test_agent_runtime_bridge, test_service_state_with_parent_session};
use crate::{ServiceEventEmitter, vce};
use futures_util::FutureExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use vldb_controller_client::{
    ClientRegistration, ControllerClient, ControllerClientConfig, ControllerProcessMode,
    ControllerSqliteEnableRequest, SpaceKind, SpaceRegistration,
};
use vulcan_provider_adapters::PublicAiRuntime;
use vulcan_workbench_model::ProjectVceSettings;
use vulcan_workspace_store::data_plane::{
    APP_SPACE_ID, ControllerRuntimeHandle, ControllerRuntimeMetadata,
};
use vulcan_workspace_store::vce_data_plane::{VceDataPlaneFactory, VceJobKind, VceJobState};

/// Require explicit billed-call authorization and a numeric dimension; never runs in ordinary test suites.
/// 要求显式计费调用授权及数值维度；不会在普通测试组中运行。
/// Takes no arguments; validates human initialization and a non-degraded real query before draining owned storage.
/// 无参数；验证人工初始化及无降级真实查询，最后排空自身持有的存储。
#[test]
#[ignore = "Requires explicit user approval, VCE_LIVE_ACCEPTANCE=1 and VCE_LIVE_DIMENSION"]
fn live_saved_models_index_and_retrieve_temporary_project() {
    // Explicit environment opt-in prevents --ignored alone from spending user credits.
    // 显式环境选择防止仅运行 --ignored 就消耗用户额度。
    assert_eq!(std::env::var("VCE_LIVE_ACCEPTANCE").as_deref(), Ok("1"));
    let dimension: u32 = std::env::var("VCE_LIVE_DIMENSION")
        .expect("explicit live dimension required")
        .parse()
        .expect("numeric live dimension");
    assert!(dimension > 0);
    // The production path resolver selects Debug/Release correctly; raw parsing cannot replay migrations.
    // 生产路径解析器准确选择调试或正式配置；原始解析不会重放迁移。
    let config_path = vulcan_provider_config::config::public_ai_connections_file_path();
    let config_before = std::fs::read(&config_path).expect("read configured model routes");
    let config_file: vulcan_provider_config::config::PublicAiConnectionsFile =
        toml::from_str(std::str::from_utf8(&config_before).expect("UTF-8 model configuration"))
            .expect("parse current model configuration");
    assert_eq!(config_file.schema_version, 1);
    let mut configuration = config_file.configuration;
    configuration
        .inference_profiles
        .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
        .expect("saved VCE profile required")
        .embedding_dimensions = Some(dimension);
    // Both project source and all VCE writes live under a fresh directory, never a saved user project.
    // 项目源码及全部 VCE 写入均在全新目录内，绝不使用已保存用户项目。
    let temporary = tempfile::tempdir().expect("isolated live acceptance directory");
    let source = temporary.path().join("source");
    let data_root = temporary.path().join("data");
    std::fs::create_dir_all(&source).expect("temporary source directory");
    std::fs::create_dir_all(&data_root).expect("temporary host data directory");
    std::fs::write(source.join("cache.rs"),
        "/// Compare the current timestamp with the token deadline; returns whether the token expired.\n/// 比较当前时间与令牌截止时间；返回令牌是否过期。\npub fn token_is_expired(now: u64, expires_at: u64) -> bool { now >= expires_at }\n")
        .expect("write tiny acceptance source");
    std::fs::write(
        source.join("math.rs"),
        "/// Add left and right integers; returns their sum.\n/// 相加左右两个整数；返回它们的和。\npub fn add(left: i32, right: i32) -> i32 { left + right }\n",
    )
    .expect("write unrelated retrieval candidate");
    let executor = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("isolated live executor"),
    );
    executor.block_on(async {
        // The isolated managed controller uses an ephemeral endpoint, not the application's controller.
        // 隔离托管控制器使用临时端点，而非应用控制器。
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve test port");
        let port = listener.local_addr().expect("test address").port();
        drop(listener);
        let executable = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../output/bin/vldb-controller.exe");
        assert!(executable.is_file());
        let endpoint = format!("http://127.0.0.1:{port}");
        let client = ControllerClient::new(
            ControllerClientConfig {
                endpoint: endpoint.clone(),
                auto_spawn: true,
                spawn_executable: Some(executable.display().to_string()),
                spawn_process_mode: ControllerProcessMode::Managed,
                minimum_uptime_secs: 0,
                idle_timeout_secs: 1,
                default_lease_ttl_secs: 30,
                connect_timeout_secs: 30,
                startup_timeout_secs: 15,
                startup_retry_interval_ms: 100,
                lease_renew_interval_secs: 10,
            },
            ClientRegistration {
                client_name: format!("vce-live-{}", uuid::Uuid::new_v4()),
                host_kind: "test".into(),
                process_id: std::process::id(),
                process_name: "vce-live-acceptance".into(),
                lease_ttl_secs: Some(30),
            },
        );
        client.connect().await.expect("connect private controller");
        // Always release this controller even when a live provider or assertion fails.
        // 即使真实供应商调用或断言失败，也始终释放本测试控制器。
        let outcome = std::panic::AssertUnwindSafe(async {
            // Credential reads use the production resolver with an enforced read-only database binding.
            // 凭据读取使用生产解析器，并强制数据库绑定只读。
            let user_root = config_path.parent().expect("configuration parent");
            let sqlite_path = user_root.join("sqlite/app-workspace.sqlite3");
            assert!(
                sqlite_path.is_file(),
                "existing credential database required"
            );
            let binding_id = format!("vce-live-credentials-{}", uuid::Uuid::new_v4());
            client
                .attach_space(SpaceRegistration {
                    space_id: APP_SPACE_ID.into(),
                    space_label: "APP".into(),
                    space_kind: SpaceKind::User,
                    space_root: user_root.display().to_string(),
                })
                .await
                .expect("attach credential read space");
            client
                .enable_sqlite(ControllerSqliteEnableRequest {
                    space_id: APP_SPACE_ID.into(),
                    binding_id: binding_id.clone(),
                    db_path: sqlite_path.display().to_string(),
                    read_only: true,
                    connection_pool_size: 1,
                    ..ControllerSqliteEnableRequest::default()
                })
                .await
                .expect("open read-only credentials");
            let controller = ControllerRuntimeHandle::new(
                client.clone(),
                ControllerRuntimeMetadata {
                    endpoint,
                    executable: executable.display().to_string(),
                    data_root: data_root.clone(),
                    sqlite_path,
                    sqlite_binding_id: binding_id,
                    system_statistics_sqlite_path: data_root.join("unused-statistics.sqlite3"),
                    system_statistics_sqlite_binding_id: "unused-live-statistics".into(),
                },
            );
            let runtime = Arc::new(
                PublicAiRuntime::first_batch(Arc::new(
                    crate::ai_credential_secret_store::SqliteAiCredentialSecretStore::new(
                        controller.clone(),
                    ),
                ))
                .expect("real saved-credential runtime"),
            );
            runtime
                .replace_configuration(configuration.clone())
                .expect("activate isolated saved routes");
            let inference = Arc::new(
                crate::configured_inference::ConfiguredInferenceService::new(runtime.clone()),
            );
            let ai = Arc::new(crate::internal_ai::ConfiguredInferenceHostAiPort::new(
                inference.clone(),
            ));
            // Reuse the production readiness path without bootstrapping plugins, VMM or any real project.
            // 复用生产准备度路径，不启动插件、记忆或任何真实项目。
            let mut bridge = test_agent_runtime_bridge();
            bridge.public_ai_runtime = runtime.clone();
            bridge.configured_inference_service = inference;
            bridge.internal_ai_port = ai.clone();
            let mut service_state = test_service_state_with_parent_session(uuid::Uuid::new_v4());
            service_state.agent_runtime = Some(bridge);
            let state = Arc::new(Mutex::new(service_state));
            let readiness = vce::vce_configuration_readiness(&state);
            assert!(
                readiness.status.ready,
                "live readiness: {:?}",
                readiness.status.issues
            );
            let fingerprint = readiness
                .embedding_physical_fingerprint
                .expect("validated explicit space");
            let factory = VceDataPlaneFactory::bootstrap(&controller)
                .await
                .expect("private VCE storage");
            let expected_fingerprint = fingerprint.clone();
            let manager = vce::VceRuntimeManager::new(
                ai,
                factory,
                ServiceEventEmitter::new(Vec::new()),
                Arc::new(move |contract| {
                    if contract.project_id == "vce-live-temporary"
                        && contract.dimension == dimension
                        && contract.embedding_fingerprint == expected_fingerprint
                    {
                        Ok(())
                    } else {
                        Err(String::from("Live acceptance publication identity changed").into())
                    }
                }),
            );
            let settings = ProjectVceSettings {
                enabled: true,
                allow_query_rewrite: true,
                allow_rerank: true,
                ..ProjectVceSettings::default()
            };
            manager
                .enable_project(
                    "vce-live-temporary",
                    &source,
                    settings.clone(),
                    dimension,
                    &fingerprint,
                    true,
                )
                .await
                .expect("enable temporary project without indexing");
            assert!(!manager.project_can_serve_now(
                "vce-live-temporary",
                settings.revision,
                &fingerprint
            ));
            let job = manager
                .submit_job(
                    "vce-live-temporary",
                    vce::VceJobAdmission {
                        retry_of_job_id: None,
                        kind: VceJobKind::Initialize,
                        idempotency_key: "live-human-initialize".into(),
                        expected_settings_revision: settings.revision,
                        expected_checkpoint_version: None,
                        contract_fingerprint: fingerprint.clone(),
                    },
                    &executor,
                )
                .await
                .expect("explicit human initialization");
            let completed = tokio::time::timeout(Duration::from_secs(180), async {
                loop {
                    let receipt = manager
                        .get_job("vce-live-temporary", Some(&job.job_id))
                        .await
                        .expect("read actual live progress")
                        .expect("durable job exists");
                    if receipt.state.is_terminal() {
                        break receipt;
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            })
            .await
            .expect("live initialization deadline");
            assert_eq!(
                completed.state,
                VceJobState::Succeeded,
                "live indexing: {completed:?}"
            );
            assert!(manager.project_can_serve_now(
                "vce-live-temporary",
                settings.revision,
                &fingerprint
            ));
            let result = tokio::time::timeout(
                Duration::from_secs(180),
                manager.retrieve(
                    "vce-live-temporary",
                    "Where does the code check whether an access token has expired?",
                ),
            )
            .await
            .expect("live retrieval deadline")
            .expect("real model retrieval");
            assert!(
                result.result.query_plan.enable_query_rewrite,
                "live query must exercise rewriting"
            );
            assert!(
                result.result.query_plan.enable_llm_rerank,
                "live query must exercise reranking"
            );
            assert!(
                result.result.degraded_features.is_empty(),
                "live degradation diagnostics: {:?}",
                result.result.degradations
            );
            assert!(
                result.result.formatted_context.contains("token_is_expired"),
                "live query must find the relevant function"
            );
            assert_eq!(
                runtime
                    .configuration_snapshot()
                    .expect("unchanged temporary routes"),
                configuration
            );
            manager
                .shutdown()
                .await
                .expect("drain live query and storage handles");
        })
        .catch_unwind()
        .await;
        client.shutdown().await.expect("release private controller");
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    });
    assert_eq!(
        std::fs::read(config_path).expect("reread original config"),
        config_before,
        "live acceptance must not rewrite user settings"
    );
}
