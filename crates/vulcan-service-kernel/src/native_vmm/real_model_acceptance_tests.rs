//! Explicit native VMM acceptance through the user's saved inference routes.
//! 使用用户已保存推理路由执行原生 VMM 的显式真实模型验收。

use super::*;
use futures_util::FutureExt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tokio::sync::{Notify, RwLock};
use vulcan_provider_adapters::PublicAiRuntime;
use vulcan_vmm::{
    ApplyProfileInstructionRequest, DEFAULT_NATIVE_USER_ID, GetProfileBundleRequest,
    PostActionRequest, PreCheckRecallMode, PreCheckRequest, ProfileBundleMode, ProfileTarget,
    VmmClient,
};
use vulcan_workspace_store::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};
use vulcan_workspace_store::vmm_data_plane::bootstrap_vmm_data_plane;

/// Runs the real native VMM flow only after explicit billed-call authorization.
/// 仅在显式授权真实计费调用后运行原生 VMM 真实链路。
///
/// The provider, connection, model, endpoint, credential and embedding dimensions are read from
/// the production configuration and credential database; none is embedded in this test.
/// 供应商、连接、模型、端点、凭据及嵌入维度均从生产配置和凭据数据库读取，测试文件不固化任何一项。
#[test]
#[ignore = "Requires explicit user approval and NATIVE_VMM_LIVE_ACCEPTANCE=1"]
fn live_saved_routes_execute_native_vmm_contract_and_lifecycle() {
    // --ignored alone must never spend user credits or invoke a saved provider route.
    // 单独使用 --ignored 绝不能消耗用户额度或调用已保存供应商路由。
    assert_eq!(
        std::env::var("NATIVE_VMM_LIVE_ACCEPTANCE").as_deref(),
        Ok("1")
    );

    // The production resolver selects the active debug/release configuration without guessing a path.
    // 生产解析器选择当前调试/正式配置，不猜测配置路径。
    let config_path = vulcan_provider_config::config::public_ai_connections_file_path();
    let config_before = std::fs::read(&config_path).expect("read saved AI configuration");
    let config_file: vulcan_provider_config::config::PublicAiConnectionsFile =
        toml::from_str(std::str::from_utf8(&config_before).expect("UTF-8 AI configuration"))
            .expect("parse saved AI configuration");
    assert_eq!(config_file.schema_version, 1);
    let configuration = config_file.configuration;

    // The managed controller and every VMM file live below a fresh temporary directory.
    // 托管控制器及全部 VMM 文件均位于全新临时目录下。
    let temporary = tempfile::tempdir().expect("isolated native VMM directory");
    let data_root = temporary.path().join("data");
    std::fs::create_dir_all(&data_root).expect("temporary native VMM data root");

    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("isolated live VMM executor");
    executor.block_on(async {
        let outcome = std::panic::AssertUnwindSafe(async {
            // Resolve the same saved credential database as production, against a private copy.
            // The controller used to attach the user's file read-only; the in-process runtime has no
            // per-binding read-only mode, so the copy is what keeps this acceptance test from opening a
            // database the user owns. Saved routes live in the file, so a copy preserves what is validated.
            // 解析与生产相同的已保存凭据数据库，但针对其私有副本。控制器过去以只读方式附着用户的文件；
            // 进程内运行时没有按绑定的只读模式，因此正是这份副本使本验收测试不会打开用户拥有的数据库。
            // 已保存路由就在该文件里，故副本保留了被验证的内容。
            let user_root = config_path.parent().expect("configuration parent");
            let saved_sqlite_path = user_root.join("sqlite/app-workspace.sqlite3");
            assert!(
                saved_sqlite_path.is_file(),
                "saved credential database is required"
            );
            let sqlite_path = data_root.join("saved-credentials.sqlite3");
            std::fs::copy(&saved_sqlite_path, &sqlite_path)
                .expect("copy saved credential database");
            let credential_binding_id = format!("native-vmm-live-{}", uuid::Uuid::new_v4());
            let controller = HostRuntimeHandle::new_for_test(
                HostRuntimeMetadata {
                    data_root: data_root.clone(),
                    sqlite_path,
                    sqlite_binding_id: credential_binding_id,
                    system_statistics_sqlite_path: data_root.join("unused-statistics.sqlite3"),
                    system_statistics_sqlite_binding_id: "unused-native-vmm-live-statistics".into(),
                },
            );

            // This is the production inference composition root backed by saved local routes.
            // 这是由本地已保存路由驱动的生产推理组合根。
            let ai_runtime = Arc::new(
                PublicAiRuntime::first_batch(Arc::new(
                    crate::ai_credential_secret_store::SqliteAiCredentialSecretStore::new(
                        controller.clone(),
                    ),
                ))
                .expect("saved-credential AI runtime"),
            );
            ai_runtime
                .replace_configuration(configuration.clone())
                .expect("activate saved AI routes in memory");
            let inference = Arc::new(
                crate::configured_inference::ConfiguredInferenceService::new(ai_runtime.clone()),
            );
            let ai = Arc::new(crate::internal_ai::ConfiguredInferenceHostAiPort::new(
                inference,
            ));
            let data_plane = bootstrap_vmm_data_plane(&controller)
                .await
                .expect("bootstrap private native VMM data plane");
            let project_tag = format!("native-vmm-live-{}", uuid::Uuid::new_v4());
            let project = data_plane
                .create_project("Native VMM live acceptance", &project_tag)
                .await
                .expect("create private native VMM project");
            let host_project_id = format!("native-vmm-live-host-{}", uuid::Uuid::new_v4());
            data_plane
                .bind_vulcan_project(&host_project_id, project.id)
                .await
                .expect("bind private VulcanCode project identity");

            let runtime = Arc::new(
                NativeVmmCoreRuntime::bootstrap(ai, data_plane.clone())
                    .await
                    .expect("bootstrap native VMM with saved model routes"),
            );
            let operation_gate = Arc::new(RwLock::new(()));
            let native_backend_enabled = Arc::new(AtomicBool::new(true));
            let pending_analysis_notify = Arc::new(Notify::new());
            let client = Arc::new(NativeVmmClient::new(
                runtime.clone(),
                data_plane.clone(),
                operation_gate,
                native_backend_enabled,
                pending_analysis_notify,
            ));
            client.start_pending_analysis_worker(&tokio::runtime::Handle::current());

            let profile = client
                .apply_profile_instruction(ApplyProfileInstructionRequest {
                    target: ProfileTarget::ProfileTargetUser,
                    user_id: DEFAULT_NATIVE_USER_ID,
                    project_id: project.id,
                    host_project_id: host_project_id.clone(),
                    instruction: "用户偏好：以后使用中文回答；代码示例必须包含中英文双语注释。"
                        .into(),
                })
                .await
                .expect("real model profile instruction");
            assert!(!profile.trace_id.is_empty());
            assert!(
                !profile.accepted_nodes.is_empty(),
                "real model profile instruction produced no accepted nodes: {profile:?}"
            );
            assert!(profile
                .accepted_nodes
                .iter()
                .all(|node| node.target == ProfileTarget::ProfileTargetUser));

            let project_profile = client
                .apply_profile_instruction(ApplyProfileInstructionRequest {
                    target: ProfileTarget::ProfileTargetProject,
                    user_id: DEFAULT_NATIVE_USER_ID,
                    project_id: project.id,
                    host_project_id: host_project_id.clone(),
                    instruction: "项目规则：这个项目的记忆验收记录必须优先使用中文，并保留数据库迁移的上下文。"
                        .into(),
                })
                .await
                .expect("real model project profile instruction");
            assert!(!project_profile.accepted_nodes.is_empty());
            assert!(project_profile
                .accepted_nodes
                .iter()
                .all(|node| node.target == ProfileTarget::ProfileTargetProject));

            // Full and Split are real profile consumers, so verify their mutually exclusive payloads.
            // Full 与 Split 都是真实画像消费方，因此验证两种互斥响应形态。
            let full_bundle = client
                .get_profile_bundle(GetProfileBundleRequest {
                    user_id: DEFAULT_NATIVE_USER_ID,
                    project_id: project.id,
                    host_project_id: host_project_id.clone(),
                    mode: ProfileBundleMode::ProfileBundleModeFull,
                    include_explanation: Some(true),
                })
                .await
                .expect("real model full profile bundle");
            assert_eq!(
                full_bundle.mode,
                ProfileBundleMode::ProfileBundleModeFull
            );
            assert!(!full_bundle.combined_text.is_empty());
            assert!(full_bundle.team_profile.is_empty());
            assert!(full_bundle.space_profile.is_empty());
            assert!(full_bundle.project_profile.is_empty());
            assert!(full_bundle.user_profile.is_empty());

            let split_bundle = client
                .get_profile_bundle(GetProfileBundleRequest {
                    user_id: DEFAULT_NATIVE_USER_ID,
                    project_id: project.id,
                    host_project_id: host_project_id.clone(),
                    mode: ProfileBundleMode::ProfileBundleModeSplit,
                    include_explanation: Some(true),
                })
                .await
                .expect("real model split profile bundle");
            assert_eq!(
                split_bundle.mode,
                ProfileBundleMode::ProfileBundleModeSplit
            );
            assert!(split_bundle.combined_text.is_empty());
            assert!(!split_bundle.user_profile.is_empty());
            assert!(!split_bundle.project_profile.is_empty());

            let session_id = format!("native-vmm-live-session-{}", uuid::Uuid::new_v4());
            let precheck = client
                .pre_check(PreCheckRequest {
                    operation_id: format!("native-vmm-live-precheck-{}", uuid::Uuid::new_v4()),
                    session_id: session_id.clone(),
                    user_id: DEFAULT_NATIVE_USER_ID,
                    project_id: project.id,
                    host_project_id: host_project_id.clone(),
                    user_content: "请根据已保存的用户偏好，用中文简要说明你将如何回答。".into(),
                    recall_mode: PreCheckRecallMode::PreCheckRecallModeFull,
                })
                .await
                .expect("real model precheck");
            assert!(!precheck.trace_id.is_empty());
            assert!(
                !precheck.degraded,
                "real model precheck degraded: {precheck:?}"
            );

            let operation_id = format!("native-vmm-live-postaction-{}", uuid::Uuid::new_v4());
            let post_action = client
                .post_action(PostActionRequest {
                    operation_id: operation_id.clone(),
                    session_id,
                    user_id: DEFAULT_NATIVE_USER_ID,
                    project_id: project.id,
                    host_project_id: host_project_id.clone(),
                    user_content:
                        "我明确决定：这个项目的原生记忆验收必须优先保留用户明确的中文回答偏好。"
                            .into(),
                    assistant_content: "已记录该项目的中文回答偏好，并将在后续回合优先遵守。"
                        .into(),
                    ..Default::default()
                })
                .await
                .expect("real model post-action intake");
            assert!(post_action.accepted);

            let completed = tokio::time::timeout(Duration::from_secs(240), async {
                loop {
                    let job = data_plane
                        .load_pending_analysis_job(&operation_id)
                        .await
                        .expect("read real model analysis job")
                        .expect("real model analysis job exists");
                    if job.state == VMM_PENDING_ANALYSIS_COMPLETED {
                        break job;
                    }
                    assert!(
                        job.state == VMM_PENDING_ANALYSIS_READY
                            || job.state == VMM_PENDING_ANALYSIS_INTAKING,
                        "real model analysis reached terminal failure: {job:?}"
                    );
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            })
            .await
            .expect("real model post-action deadline");
            // The durable policy permits ordinary model-contract failures to retry before completion.
            // 持久化策略允许普通模型契约失败在完成前重试。
            eprintln!(
                "real model post-action completed after {} persisted failed attempt(s)",
                completed.attempt_count
            );
            assert!(
                completed.attempt_count < PENDING_ANALYSIS_FAILURE_PASS_THRESHOLD,
                "real model post-action must complete before the analysis-bypass threshold: {:?}",
                completed
            );
            assert!(completed.payload_json.is_empty());

            let snapshot = runtime
                .storage_snapshot()
                .expect("read real model native storage snapshot");
            assert!(
                completed.turn_id.as_ref().is_some_and(|turn_id| {
                    snapshot.dataset.turns.iter().any(|turn| {
                        turn.id.as_str() == turn_id && turn.status == TurnStatus::Completed
                    })
                }),
                "real model post-action did not finalize its durable turn: {:?}",
                snapshot.dataset.turns
            );
            assert!(
                snapshot
                    .dataset
                    .profile_nodes
                    .iter()
                    .any(|node| node.content.contains("中文")),
                "real model profile instruction did not persist its accepted node: {:?}",
                snapshot.dataset.profile_nodes
            );

            assert_eq!(
                ai_runtime
                    .configuration_snapshot()
                    .expect("saved AI configuration snapshot"),
                configuration
            );
        })
        .catch_unwind()
        .await;
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    });

    // The user's saved configuration is read-only input to this acceptance.
    // 用户已保存配置只是本验收的只读输入。
    assert_eq!(
        std::fs::read(config_path).expect("reread saved AI configuration"),
        config_before,
        "live native VMM acceptance must not rewrite saved configuration"
    );
}
