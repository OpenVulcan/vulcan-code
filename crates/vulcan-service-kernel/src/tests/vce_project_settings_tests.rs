//! Explicit project settings and initialization through the production service boundary.
//! 通过生产服务边界验证明确项目设置与初始化。

use super::test_support::TestHostRuntimeRoot;
use super::vce_configuration_readiness_tests::configured_state;
use crate::{ServiceEventEmitter, vce};
use futures_util::FutureExt;
use std::sync::Arc;
use std::time::Duration;
use vulcan_service_api::{
    VceIndexUpdateMode, VceJobAction, VceJobQueryRequest, VceJobStateSummary, VceJobSubmitRequest,
    VceProjectLifecycleState, VceProjectRequest, VceProjectSettingsInput,
    VceProjectSettingsUpdateRequest,
};
use vulcan_workbench_model::{ProjectVceSettings, ProjectVceUpdateMode, TaskProjectSummary};
use vulcan_workspace_store::data_plane::HostRuntimeHandle;
use vulcan_workspace_store::vce_data_plane::VceDataPlaneFactory;
use vulcan_workspace_store::workspace_store::{load_workspace_index_snapshot, persist_new_project};

/// Copy exactly the user-editable fields into the real settings RPC contract.
/// 将用户可编辑字段精确复制到真实设置 RPC 契约。
/// Settings supplies a typed baseline; returns input without revision or migration authority.
/// 设置提供强类型基线；返回不含修订或迁移权威的输入。
fn editable_settings(settings: &ProjectVceSettings) -> VceProjectSettingsInput {
    VceProjectSettingsInput {
        enabled: settings.enabled,
        update_mode: match settings.update_mode {
            ProjectVceUpdateMode::Manual => VceIndexUpdateMode::Manual,
            ProjectVceUpdateMode::Automatic => VceIndexUpdateMode::Automatic,
        },
        auto_update_min_interval_seconds: settings.auto_update_min_interval_seconds,
        max_file_bytes: settings.max_file_bytes,
        extra_ignore_patterns: settings.extra_ignore_patterns.clone(),
        allow_query_rewrite: settings.allow_query_rewrite,
        allow_rerank: settings.allow_rerank,
        embedding_max_concurrency: settings.embedding_max_concurrency,
        query_max_concurrency: settings.query_max_concurrency,
        max_results: settings.max_results,
        max_context_chars: settings.max_context_chars,
    }
}

/// Read one project's actual durable settings through the production workspace loader.
/// 通过生产工作区加载器读取一个项目的实际持久设置。
/// Controller identifies private APP storage and project_id its exact row. Returns the stored project.
/// 控制器标识私有应用存储，项目编号标识精确记录。返回已存项目。
async fn saved_project(controller: &HostRuntimeHandle, project_id: &str) -> TaskProjectSummary {
    load_workspace_index_snapshot(controller.clone())
        .await
        .expect("read private workspace")
        .task_projects
        .into_iter()
        .find(|project| project.id == project_id)
        .expect("exact persisted project")
}

/// Read both complete stored rows without a join that can conceal a missing counterpart.
/// 读取两条完整存储记录，不使用可能隐藏缺失对应记录的连接。
/// `controller` owns private storage and `project_id` selects one exact identity; returns both row arrays.
/// `controller` 持有私有存储，`project_id` 选择精确身份；返回两个记录数组。
async fn project_persistence_rows(
    controller: &HostRuntimeHandle,
    project_id: &str,
) -> [serde_json::Value; 2] {
    // Preserve every project column, including timestamps and optional metadata, for rollback comparison.
    // 保留每个项目列，包括时间戳与可选元数据，以供回滚比较。
    let project_rows = vulcan_workspace_store::local_sql::query_json_async(
        &controller.local,
        vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
        "SELECT * FROM projects WHERE id = ?1;",
        vec![vulcan_sqlite_engine::sql_exec::SqliteValue::Text(
            project_id.to_owned(),
        )],
    )
    .await
    .expect("read full private project row");
    // Read the semantic row separately so either missing half remains visible in the test result.
    // 单独读取语义记录，使缺失的任一部分都在测试结果中可见。
    let settings_rows = vulcan_workspace_store::local_sql::query_json_async(
        &controller.local,
        vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
        "SELECT * FROM project_vce_settings WHERE project_id = ?1;",
        vec![vulcan_sqlite_engine::sql_exec::SqliteValue::Text(
            project_id.to_owned(),
        )],
    )
    .await
    .expect("read full private semantic row");
    [
        serde_json::from_str(&project_rows.json_data).expect("project row JSON"),
        serde_json::from_str(&settings_rows.json_data).expect("settings row JSON"),
    ]
}

/// Verify a rejected semantic-settings insert cannot commit its owning project independently.
/// 验证被拒绝的语义设置插入不能使所属项目独立提交。
/// No arguments or return value; all SQL, failure injection, and rows belong to a private controller.
/// 无参数和返回值；全部 SQL、故障注入和记录属于私有宿主存储。
#[test]
fn real_host_storage_project_creation_is_atomic_with_vce_settings() {
    // The executor and temporary root own this fixture independently from the running application.
    // 执行器和临时根独立于运行中的应用持有此夹具。
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("private project persistence executor");
    // Every database and session fixture is removed only after the controller lease is released.
    // 仅在控制器租约释放后移除各数据库和会话夹具。
    let storage = TestHostRuntimeRoot::new(uuid::Uuid::new_v4());
    executor.block_on(async {
        // This client and APP binding never attach to user storage.
        // 此客户端和应用绑定绝不挂接用户存储。
        let controller = storage.attached_host_runtime_handle().await;
        // Catch assertion failures so the managed controller is released on the red regression too.
        // 捕获断言失败，确保红色回归同样释放托管控制器。
        let outcome = std::panic::AssertUnwindSafe(async {
            vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&controller)
                .await
                .expect("private application schema");
            // The exact target does not exist before the production creation command.
            // 生产创建命令之前不存在此精确目标。
            let mut project = TaskProjectSummary::new(
                "atomic-vce-project",
                "Atomic VCE project",
                storage.data_root().join("atomic-source").display().to_string(),
                false,
                0,
                None,
                None,
                Vec::new(),
            );
            // Reuse the identical private rejection for new rows and existing metadata updates.
            // 为新建记录与既有元数据更新复用相同的私有拒绝条件。
            let rejection_statement = "CREATE TRIGGER reject_test_vce_insert BEFORE INSERT ON project_vce_settings
                 WHEN NEW.project_id = 'atomic-vce-project'
                 BEGIN SELECT RAISE(ABORT, 'test_vce_settings_rejected'); END;";
            vulcan_workspace_store::local_sql::execute_script_async(
                &controller.local,
                vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
                rejection_statement,
                Vec::new(),
            ).await.expect("install exact private failure condition");
            // The failure occurs in the second logical write, not before project creation is attempted.
            // 失败发生于第二个逻辑写入，而非尝试创建项目之前。
            let error = persist_new_project(controller.clone(), &project)
                .await
                .expect_err("the semantic insert must be rejected");
            assert!(error.contains("test_vce_settings_rejected"), "{error}");
            // Query the raw tables instead of the production inner join that could hide a partial row.
            // 查询原始表，避免生产内连接掩盖部分写入记录。
            let raw = vulcan_workspace_store::local_sql::query_json_async(
                &controller.local,
                vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
                "SELECT (SELECT COUNT(*) FROM projects WHERE id = ?1) AS project_count,
                 (SELECT COUNT(*) FROM project_vce_settings WHERE project_id = ?1) AS settings_count;",
                vec![vulcan_sqlite_engine::sql_exec::SqliteValue::Text(project.id.clone())],
            ).await.expect("read exact post-failure state");
            // A declared failure must leave neither half of this newly created project durable.
            // 明确失败后，此新建项目的任一部分都不得持久保留。
            let rows: serde_json::Value = serde_json::from_str(&raw.json_data).expect("typed counts");
            assert_eq!(rows, serde_json::json!([{ "project_count": 0, "settings_count": 0 }]));
            vulcan_workspace_store::local_sql::execute_script_async(
                &controller.local,
                vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
                "DROP TRIGGER reject_test_vce_insert;",
                Vec::new(),
            ).await.expect("remove only the private failure condition");

            // An unrepresentable revision must be rejected before even the first new row is written.
            // 无法表示的修订号必须在写入第一条新记录之前被拒绝。
            let mut invalid_new = project.clone();
            invalid_new.id = String::from("preflight-vce-project");
            invalid_new.vce.revision = u64::MAX;
            assert!(persist_new_project(controller.clone(), &invalid_new).await
                .expect_err("revision conversion must fail").contains("SQLite 整数范围"));
            assert_eq!(project_persistence_rows(&controller, &invalid_new.id).await,
                [serde_json::json!([]), serde_json::json!([])]);

            // Quoted SQL-looking text, UTF-8, and literal NUL must remain stored data, not statements.
            // 类似 SQL 的引号文本、UTF-8 与字面空字符必须保持存储数据，而非语句。
            project.title = String::from("项目'); DROP TABLE projects; --\0尾部");
            project.path = storage.data_root().join("源码'目录").display().to_string();
            project.icon_pack = Some(String::from("pack'中文"));
            project.icon_key = Some(String::from("key\0尾"));
            project.vce.enabled = true;
            project.vce.update_mode = ProjectVceUpdateMode::Automatic;
            project.vce.auto_update_min_interval_seconds = 600;
            project.vce.revision = 7;
            project.vce.extra_ignore_patterns = vec![String::from("generated/'quoted/**")];
            persist_new_project(controller.clone(), &project).await.expect("retry complete creation");
            // The production loader must recover exact metadata and the unchanged complete VCE settings.
            // 生产加载器必须恢复精确元数据及保持完整的 VCE 设置。
            let saved = saved_project(&controller, &project.id).await;
            assert_eq!(saved.title, project.title);
            assert_eq!(saved.path, project.path);
            assert_eq!(saved.icon_pack, project.icon_pack);
            assert_eq!(saved.icon_key, project.icon_key);
            assert_eq!(saved.vce, project.vce);
            // Both before-images include every column so rejection cannot silently alter timestamps either.
            // 两份前快照包含全部列，确保拒绝后连时间戳也不会被静默改变。
            let before = project_persistence_rows(&controller, &project.id).await;
            // This attempted upsert changes actual first-row fields and requests different semantic defaults.
            // 此次尝试的更新改变真实首表字段，并请求不同的语义默认值。
            let mut replacement = project.clone();
            replacement.title = String::from("Replacement metadata");
            replacement.path = storage.data_root().join("replacement-source").display().to_string();
            replacement.pinned = true;
            replacement.order = 5;
            replacement.icon_pack = None;
            replacement.icon_key = None;
            replacement.vce = ProjectVceSettings::default();
            // Preflight failure must also preserve an existing project's full metadata.
            // 预检失败同样必须保留既有项目的完整元数据。
            let mut invalid_existing = replacement.clone();
            invalid_existing.vce.revision = u64::MAX;
            assert!(persist_new_project(controller.clone(), &invalid_existing).await.is_err());
            assert_eq!(project_persistence_rows(&controller, &project.id).await, before);

            vulcan_workspace_store::local_sql::execute_script_async(
                &controller.local,
                vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
                rejection_statement,
                Vec::new(),
            ).await.expect("reject the existing project's second logical write");
            // An existing first-row update is rolled back even though its original settings already exist.
            // 即使原设置已存在，既有首表更新也必须回滚。
            let rejected = persist_new_project(controller.clone(), &replacement).await
                .expect_err("existing project update must fail atomically");
            assert!(rejected.contains("test_vce_settings_rejected"), "{rejected}");
            assert_eq!(project_persistence_rows(&controller, &project.id).await, before);
            vulcan_workspace_store::local_sql::execute_script_async(
                &controller.local,
                vulcan_workspace_store::local_database_runtime::LogicalDatabase::AppState,
                "DROP TRIGGER reject_test_vce_insert;",
                Vec::new(),
            ).await.expect("release private rejection before retry");
            persist_new_project(controller.clone(), &replacement).await.expect("retry existing project");
            // Existing semantic settings and icon overrides retain the original upsert contract.
            // 既有语义设置及图标覆盖保持原更新插入契约。
            let updated = saved_project(&controller, &project.id).await;
            assert_eq!(updated.title, replacement.title);
            assert_eq!(updated.path, replacement.path);
            assert!(updated.pinned);
            assert_eq!(updated.order, replacement.order);
            assert_eq!(updated.icon_pack, project.icon_pack);
            assert_eq!(updated.icon_key, project.icon_key);
            assert_eq!(updated.vce, project.vce);
            // The complete settings row, including revision and timestamp, must remain unchanged on conflict.
            // 冲突时完整设置记录必须保持不变，包括修订号和时间戳。
            let after = project_persistence_rows(&controller, &project.id).await;
            assert_eq!(after[1], before[1]);
            assert_eq!(after[0][0]["created_at"], before[0][0]["created_at"]);
            assert!(!storage.data_root().join("vce/projects").exists());
        }).catch_unwind().await;
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    });
}

/// Verify settings do not create an index and explicit initialization is the only first-build entry.
/// 验证设置不创建索引，明确初始化是唯一首次建库入口。
/// No input; all source, metadata and vectors belong to a private controller with credential-rejecting AI.
/// 无输入；全部源码、元数据及向量属于私有宿主存储，AI 拒绝凭据访问。
#[test]
fn real_host_storage_project_settings_require_explicit_initialization_and_preserve_indexes() {
    // Bound this fixture's asynchronous jobs and watcher workers independently from the real app.
    // 将夹具异步作业及监听任务约束在独立于真实应用的执行器中。
    let executor = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("project settings executor"),
    );
    let storage = TestHostRuntimeRoot::new(uuid::Uuid::new_v4());
    executor.block_on(async {
        let controller = storage.attached_host_runtime_handle().await;
        // Application-table setup is also covered by controller cleanup, not just later service assertions.
        // 应用表准备同样受控制器清理保护，而不只保护后续服务断言。
        let controller_outcome = std::panic::AssertUnwindSafe(async {
            vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&controller)
                .await
                .expect("production application schema on private binding");
            let (state, runtime, configuration) = configured_state();
            // The source does not exist until the simulated human requests first initialization.
            // 模拟用户请求首次初始化之前，源码目录并不存在。
            let source = storage.data_root().join("settings-source");
            let project = TaskProjectSummary::new(
                "settings-project",
                "Settings project",
                source.display().to_string(),
                false,
                0,
                None,
                None,
                Vec::new(),
            );
            let peer = TaskProjectSummary::new(
                "settings-peer",
                "Untouched peer",
                storage
                    .data_root()
                    .join("untouched-source")
                    .display()
                    .to_string(),
                false,
                1,
                None,
                None,
                Vec::new(),
            );
            persist_new_project(controller.clone(), &project)
                .await
                .expect("seed actual target row");
            persist_new_project(controller.clone(), &peer)
                .await
                .expect("seed actual peer row");
            let snapshot = load_workspace_index_snapshot(controller.clone())
                .await
                .expect("real workspace");
            let ai = Arc::clone(
                &state
                    .lock()
                    .expect("state")
                    .agent_runtime
                    .as_ref()
                    .expect("registered AI")
                    .internal_ai_port,
            );
            let factory = VceDataPlaneFactory::bootstrap(&controller)
                .await
                .expect("private VCE factory");
            let manager = vce::VceRuntimeManager::new(
                ai,
                factory.clone(),
                ServiceEventEmitter::new(Vec::new()),
                vce::vce_publication_authority(&state),
            );
            let center =
                crate::workspace_watcher::WorkspaceWatcherCenter::new().expect("physical watcher");
            let watcher = vce::VceWatcherService::new(
                &center,
                manager.clone(),
                ServiceEventEmitter::new(Vec::new()),
                executor.clone(),
            );
            {
                let mut guard = state.lock().expect("state");
                guard.controller_runtime = Some(controller.clone());
                guard.workspace_snapshot = Some(snapshot);
                guard.service_runtime = Some(executor.clone());
                guard.vce_runtime = Some(manager.clone());
                guard.vce_watcher = Some(watcher.clone());
            }
            // Keep teardown outside assertions so failed service calls cannot leave a private controller running.
            // 将清理保留在断言之外，避免服务断言失败遗留私有宿主存储。
            let outcome = std::panic::AssertUnwindSafe(async {
                let initial = saved_project(&controller, &project.id).await.vce;
                assert!(!initial.enabled);
                assert_eq!(initial.update_mode, ProjectVceUpdateMode::Manual);
                assert_eq!(initial.auto_update_min_interval_seconds, 300);
                assert!(vce::vce_configuration_readiness(&state).status.ready);
                // Every save exercises durable CAS, full workbench refresh, reconciliation and event projection.
                // 每次保存都经过持久比较交换、完整工作台刷新、协调及事件投影。
                let mut expected = initial.clone();
                for (enabled, update_mode, interval) in [
                    (true, ProjectVceUpdateMode::Manual, 300),
                    (true, ProjectVceUpdateMode::Automatic, 300),
                    (false, ProjectVceUpdateMode::Automatic, 300),
                    (true, ProjectVceUpdateMode::Manual, 600),
                ] {
                    let previous_revision = expected.revision;
                    expected.enabled = enabled;
                    expected.update_mode = update_mode;
                    expected.auto_update_min_interval_seconds = interval;
                    let result = vce::service_update_vce_project_settings(
                        ServiceEventEmitter::new(Vec::new()),
                        state.clone(),
                        VceProjectSettingsUpdateRequest {
                            project_id: project.id.clone(),
                            expected_revision: previous_revision,
                            settings: editable_settings(&expected),
                        },
                    )
                    .await
                    .expect("real settings save, refresh and reconciliation");
                    expected.revision += 1;
                    assert_eq!(result.revision, expected.revision);
                    assert_eq!(result.enabled, expected.enabled);
                    assert_eq!(saved_project(&controller, &project.id).await.vce, expected);
                    assert_eq!(saved_project(&controller, &peer.id).await.vce, peer.vce);
                    assert_eq!(
                        state
                            .lock()
                            .expect("state")
                            .workspace_snapshot
                            .as_ref()
                            .expect("workspace")
                            .task_projects
                            .iter()
                            .find(|item| item.id == project.id)
                            .expect("in-memory project")
                            .vce,
                        expected
                    );
                    let activity = vce::service_get_vce_project_activity(
                        state.clone(),
                        VceProjectRequest {
                            project_id: project.id.clone(),
                        },
                    )
                    .await
                    .expect("saved project activity");
                    assert_eq!(
                        activity.status.lifecycle,
                        if enabled {
                            VceProjectLifecycleState::Uninitialized
                        } else {
                            VceProjectLifecycleState::Disabled
                        }
                    );
                    assert!(!activity.query_available);
                    assert!(!activity.source_freshness.watching);
                    assert!(!vce::vce_project_tool_enabled(&state, &project.id));
                    assert!(manager.project_status(&project.id).await.is_none());
                    assert!(
                        manager
                            .get_job(&project.id, None)
                            .await
                            .expect("no initial job")
                            .is_none()
                    );
                    assert!(!source.exists());
                    assert!(!storage.data_root().join("vce/projects").exists());
                }
                // A stale editor and invalid interval cannot overwrite the durable winning revision.
                // 过期编辑器及无效间隔不得覆盖持久化获胜修订。
                let stale = vce::service_update_vce_project_settings(
                    ServiceEventEmitter::new(Vec::new()),
                    state.clone(),
                    VceProjectSettingsUpdateRequest {
                        project_id: project.id.clone(),
                        expected_revision: initial.revision,
                        settings: editable_settings(&initial),
                    },
                )
                .await
                .expect_err("stale settings must fail");
                assert!(stale.contains("设置修订冲突"), "{stale}");
                let mut invalid = editable_settings(&expected);
                invalid.auto_update_min_interval_seconds = 0;
                assert!(
                    vce::service_update_vce_project_settings(
                        ServiceEventEmitter::new(Vec::new()),
                        state.clone(),
                        VceProjectSettingsUpdateRequest {
                            project_id: project.id.clone(),
                            expected_revision: expected.revision,
                            settings: invalid,
                        }
                    )
                    .await
                    .is_err()
                );
                assert_eq!(saved_project(&controller, &project.id).await.vce, expected);
                // Users can retain project intent while models are unavailable, but cannot initialize the project.
                // 模型不可用时用户仍可保留项目意图，但不能初始化项目。
                let mut unavailable = configuration.clone();
                unavailable
                    .inference_profiles
                    .get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("VCE profile")
                    .embedding = None;
                runtime
                    .replace_configuration(unavailable.clone())
                    .expect("revoke embedding in isolated runtime");
                vce::service_update_vce_project_settings(
                    ServiceEventEmitter::new(Vec::new()),
                    state.clone(),
                    VceProjectSettingsUpdateRequest {
                        project_id: project.id.clone(),
                        expected_revision: expected.revision,
                        settings: editable_settings(&expected),
                    },
                )
                .await
                .expect("save intent without models");
                expected.revision += 1;
                assert_eq!(saved_project(&controller, &project.id).await.vce, expected);
                assert!(
                    vce::service_submit_vce_job(
                        state.clone(),
                        VceJobSubmitRequest {
                            retry_of_job_id: None,
                            project_id: project.id.clone(),
                            action: VceJobAction::Initialize,
                            idempotency_key: "blocked-human-initialize".into(),
                            expected_settings_revision: expected.revision,
                            expected_checkpoint_version: None,
                        }
                    )
                    .await
                    .is_err()
                );
                assert!(!source.exists());
                assert!(!storage.data_root().join("vce/projects").exists());
                assert_eq!(
                    runtime
                        .configuration_snapshot()
                        .expect("unchanged unavailable routes"),
                    unavailable
                );
                runtime
                    .replace_configuration(configuration.clone())
                    .expect("restore exact model routes");
                // Only this explicit human command may create the first physical project index.
                // 只有此明确人为命令可以创建首个物理项目索引。
                std::fs::create_dir_all(&source).expect("human-selected empty source");
                let admitted = vce::service_submit_vce_job(
                    state.clone(),
                    VceJobSubmitRequest {
                        retry_of_job_id: None,
                        project_id: project.id.clone(),
                        action: VceJobAction::Initialize,
                        idempotency_key: "explicit-settings-project-initialize".into(),
                        expected_settings_revision: expected.revision,
                        expected_checkpoint_version: None,
                    },
                )
                .await
                .expect("real initialization entry");
                let completed = tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        let receipt = vce::service_get_vce_job(
                            state.clone(),
                            VceJobQueryRequest {
                                project_id: project.id.clone(),
                                job_id: Some(admitted.job_id.clone()),
                            },
                        )
                        .await
                        .expect("real progress query")
                        .expect("stable job receipt");
                        if matches!(
                            receipt.state,
                            VceJobStateSummary::Succeeded
                                | VceJobStateSummary::Failed
                                | VceJobStateSummary::Cancelled
                                | VceJobStateSummary::Interrupted
                        ) {
                            break receipt;
                        }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .expect("explicit empty index completion");
                assert_eq!(
                    completed.state,
                    VceJobStateSummary::Succeeded,
                    "{completed:?}"
                );
                assert!(vce::vce_project_tool_enabled(&state, &project.id));
                let provenance = manager
                    .embedding_provenance(&project.id)
                    .await
                    .expect("initialized origin")
                    .expect("committed index");
                // Closing and reopening the actual settings must retain the same committed index and last job.
                // 通过实际设置关闭及重新开启，必须保留同一个已提交索引及最后作业。
                for enabled in [false, true] {
                    expected.enabled = enabled;
                    vce::service_update_vce_project_settings(
                        ServiceEventEmitter::new(Vec::new()),
                        state.clone(),
                        VceProjectSettingsUpdateRequest {
                            project_id: project.id.clone(),
                            expected_revision: expected.revision,
                            settings: editable_settings(&expected),
                        },
                    )
                    .await
                    .expect("toggle existing index through settings");
                    expected.revision += 1;
                    assert_eq!(saved_project(&controller, &project.id).await.vce, expected);
                    let activity = vce::service_get_vce_project_activity(
                        state.clone(),
                        VceProjectRequest {
                            project_id: project.id.clone(),
                        },
                    )
                    .await
                    .expect("retained index activity");
                    assert_eq!(activity.query_available, enabled);
                    assert_eq!(vce::vce_project_tool_enabled(&state, &project.id), enabled);
                    assert_eq!(
                        manager
                            .embedding_provenance(&project.id)
                            .await
                            .expect("retained origin")
                            .expect("retained index"),
                        provenance
                    );
                    assert_eq!(
                        manager
                            .get_job(&project.id, None)
                            .await
                            .expect("retained job")
                            .expect("initial remains latest")
                            .job_id,
                        admitted.job_id
                    );
                }
                // The original disabled project is a real earlier snapshot, not synthesized field compatibility.
                // 原始禁用项目为实际较早快照，不是合成的字段兼容输入。
                vce::verify_retired_reconciliation_preserves_runtime(
                    &state,
                    project.clone(),
                    vce::vce_configuration_readiness(&state),
                )
                .await;
                // Capture an unavailable global generation while preserving the exact current project revision.
                // 保持精确当前项目修订时，捕获不可用的全局代际。
                runtime
                    .replace_configuration(unavailable.clone())
                    .expect("capture retired global configuration");
                // These paired facts represent the same project before global routes become available again.
                // 此配对事实表示全局路由重新可用之前的同一项目。
                let retired_readiness = vce::vce_configuration_readiness(&state);
                let retained_project = saved_project(&controller, &project.id).await;
                runtime
                    .replace_configuration(configuration.clone())
                    .expect("restore newer global configuration");
                vce::verify_retired_reconciliation_preserves_runtime(
                    &state,
                    retained_project,
                    retired_readiness,
                )
                .await;
                assert_eq!(saved_project(&controller, &peer.id).await.vce, peer.vce);
                assert!(
                    factory
                        .load_active_project_config(&peer.id)
                        .await
                        .expect("peer stays cold")
                        .is_none()
                );
                assert_eq!(
                    runtime
                        .configuration_snapshot()
                        .expect("peer model configuration retained"),
                    configuration
                );
                assert!(
                    state
                        .lock()
                        .expect("state")
                        .native_vmm_maintenance
                        .is_none()
                );
                assert!(
                    state
                        .lock()
                        .expect("state")
                        .vmm_controller_registry
                        .snapshots()
                        .expect("no memory controller activation")
                        .is_empty()
                );
            })
            .catch_unwind()
            .await;
            watcher.shutdown().await;
            manager.shutdown().await.expect("drain private VCE manager");
            factory
                .shutdown()
                .await
                .expect("release private VCE storage");
            if let Err(panic) = outcome {
                std::panic::resume_unwind(panic);
            }
        })
        .catch_unwind()
        .await;
        if let Err(panic) = controller_outcome {
            std::panic::resume_unwind(panic);
        }
    });
}
