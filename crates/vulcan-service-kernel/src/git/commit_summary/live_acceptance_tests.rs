//! Explicitly authorized live Git summary acceptance with private settings and a disposable repository.
//! 显式授权的真实 Git 总结验收，使用私有设置和临时仓库。

use super::*;
use crate::tests::test_support::{
    TestHostRuntimeRoot, test_agent_runtime_bridge, test_service_state_with_parent_session,
};
use std::sync::Mutex;
use vulcan_provider_adapters::PublicAiRuntime;
use vulcan_sqlite_engine::{
    runtime::{SqliteOpenOptions, open_sqlite_connection},
    sql_exec::{SqliteValue, query_json},
};
use vulcan_workspace_store::workspace_store::{
    AiCredentialSecretRecord, restore_ai_credential_secret_record,
};

/// Saves an explicit model privately and generates a real message through the production Git service.
/// 私有保存明确模型，并通过生产 Git 服务生成真实提交信息。
///
/// Requires GIT_SUMMARY_LIVE_ACCEPTANCE=1 and exact provider/model environment values.
/// 需要 GIT_SUMMARY_LIVE_ACCEPTANCE=1 以及精确的供应商、模型环境变量。
/// Reads only one credential row without logging it; writes and Git initialization stay in a temp root.
/// 仅读取一行凭据且不记录其内容；所有写入和 Git 初始化均在临时根内进行。
/// Returns normally only after real generation, precise model accounting and original-file checks pass.
/// 仅在真实生成、精确模型统计及原文件检查均通过后正常返回。
#[test]
#[ignore = "Requires explicit billed-call authorization and GIT_SUMMARY_LIVE_ACCEPTANCE=1"]
fn live_saved_fixed_model_generates_git_summary_without_mutating_user_settings() {
    // The debug service future needs a bounded 16 MiB stack; only this explicitly requested test owns it.
    // 调试版服务 Future 需要受限的 16 MiB 栈，仅此显式执行的测试持有该资源。
    std::thread::Builder::new()
        .name(String::from("git-summary-live-acceptance"))
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("single-worker acceptance runtime")
                .block_on(run_live_saved_fixed_model_acceptance());
        })
        .expect("bounded acceptance worker")
        .join()
        .expect("live acceptance result");
}

/// Runs the complete private-save and real-generation flow on the explicitly sized test worker.
/// 在明确设置栈大小的测试工作线程上运行完整私有保存与真实生成流程。
/// Reads explicit environment targets and returns only after ownership and persistence assertions pass.
/// 读取明确环境目标，仅在归属和持久化断言通过后返回。
async fn run_live_saved_fixed_model_acceptance() {
    assert_eq!(
        std::env::var("GIT_SUMMARY_LIVE_ACCEPTANCE").as_deref(),
        Ok("1")
    );
    // provider and model are deliberate acceptance targets, never inferred from fallback defaults.
    // provider 与 model 是明确的验收目标，绝不从回退默认值推断。
    let provider = std::env::var("GIT_SUMMARY_LIVE_PROVIDER").expect("explicit provider");
    let model = std::env::var("GIT_SUMMARY_LIVE_MODEL").expect("explicit model");
    // configPath and originalBytes come from the production path resolver and remain read-only.
    // configPath 与 originalBytes 来自生产路径解析器，并保持只读。
    let config_path = vulcan_provider_config::config::public_ai_connections_file_path();
    let original_bytes = std::fs::read(&config_path).expect("read saved AI config");
    let saved: vulcan_provider_config::config::PublicAiConnectionsFile =
        toml::from_str(std::str::from_utf8(&original_bytes).expect("UTF-8 AI config"))
            .expect("parse saved config");
    // configuration intentionally has no global active model, proving generation uses the saved fixed pair.
    // configuration 刻意没有全局活动模型，用于证明生成使用已保存的固定组合。
    let mut configuration = saved.configuration;
    configuration.active = None;
    // owner, temporary and controller isolate accounting, persisted settings and every writable database.
    // owner、temporary 与 controller 隔离统计、持久化设置以及所有可写数据库。
    let owner = SessionId::new_v4();
    let temporary = TestHostRuntimeRoot::new(owner);
    let controller = temporary.host_runtime_handle();
    vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&controller)
        .await
        .expect("private application schema");
    let runtime = Arc::new(
        PublicAiRuntime::first_batch(Arc::new(
            crate::ai_credential_secret_store::SqliteAiCredentialSecretStore::new(
                controller.clone(),
            ),
        ))
        .expect("private runtime"),
    );
    runtime
        .replace_configuration(configuration.clone())
        .expect("private config");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("current signed catalog");
    // selection provides the exact credential reference; no other credential is copied or queried.
    // selection 提供精确的凭据引用，不复制或查询其他凭据。
    let selection = runtime
        .resolve_model_selection(&configuration, &provider, &model)
        .expect("configured fixed model");
    // options enforce SQLite read-only flags and query_only while the user's application may remain open.
    // options 强制 SQLite 只读标志和 query_only，允许用户应用继续运行。
    let mut options = SqliteOpenOptions::default();
    options.hardening.read_only = true;
    let credential_record = {
        // connection is a short-lived, strictly read-only view of the authoritative saved database.
        // connection 是权威持久化数据库的短期严格只读视图。
        let connection = open_sqlite_connection(
            &config_path
                .parent()
                .unwrap()
                .join("sqlite/app-workspace.sqlite3"),
            &options,
        )
        .expect("read-only credential database");
        // rows never leave this process or enter test output; the filter is bound, not interpolated.
        // rows 不离开当前进程且不进入测试输出；筛选条件使用绑定，不进行插值。
        let rows = query_json(
            &connection,
            "SELECT * FROM ai_credential_secrets WHERE secret_reference = ?1 LIMIT 1",
            &[SqliteValue::Text(
                selection.route.connection.secret_reference.clone(),
            )],
        )
        .expect("read exactly selected credential");
        let mut records: Vec<AiCredentialSecretRecord> =
            serde_json::from_str(&rows.json_data).expect("typed credential row");
        assert_eq!(records.len(), 1, "selected credential must exist");
        records.pop().unwrap()
    };
    restore_ai_credential_secret_record(&controller, &credential_record)
        .await
        .expect("private credential copy");
    // repository contains only this public synthetic README; no workspace diff is sent to the model.
    // repository 只包含公开的合成 README，不向模型发送用户工作区差异。
    let repository = temporary.data_root().join("summary-repository");
    std::fs::create_dir_all(&repository).expect("private repository");
    assert!(
        std::process::Command::new("rtk")
            .args(["git", "init", "--quiet"])
            .current_dir(&repository)
            .status()
            .expect("initialize private Git")
            .success()
    );
    std::fs::write(repository.join("README.md"), "# UI acceptance\n\nDisplay average output tokens per second and preserve unknown values as a dash.\n").expect("synthetic diff");
    // bridge and state use real production service functions with only ownership storage isolated.
    // bridge 与 state 使用真实生产服务函数，仅隔离归属存储。
    let mut bridge = test_agent_runtime_bridge();
    bridge.public_ai_runtime = runtime.clone();
    let mut state = test_service_state_with_parent_session(owner);
    state.controller_runtime = Some(controller);
    state.agent_runtime = Some(bridge);
    state
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .session_runtime_state
        .insert(
            owner,
            vulcan_workbench_model::SessionRuntimeStateEntry {
                active_reasoning_effort: Some(ReasoningEffortLevel::Off),
                ..Default::default()
            },
        );
    state
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .task_projects
        .push(vulcan_workbench_model::TaskProjectSummary::new(
            "project-a",
            "Git summary acceptance",
            repository.to_str().unwrap(),
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
    let state = Arc::new(Mutex::new(state));
    // destination scopes the existing persistence seam to the private file, without changing process env paths.
    // destination 将已有持久化接缝限定在私有文件，不更改进程环境中的路径。
    let destination = temporary.data_root().join("provider-config.toml");
    crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH.scope(destination.clone(), async {
        // request uses the same JSON shape as the settings UI and calls its real validation/save service.
        // request 使用与设置 UI 相同的 JSON 形状，调用真实校验与保存服务。
        let request = serde_json::from_value(serde_json::json!({
            "newBranchPrefix":"vulcan/", "summaryModel":{"mode":"fixed","providerId":provider,"modelId":model},
            "summaryExtraPrompt":"仅输出简短中文提交信息。",
        })).expect("settings request");
        super::super::settings::service_save_git_commit_settings(
            vulcan_service_api::ServiceEventEmitter::new(Vec::new()), state.clone(), request,
        ).await.expect("real settings save to private destination");
        assert!(destination.is_file(), "settings must actually persist");
        // response comes from the complete production service: Git diff, model selection, stream and usage.
        // response 来自完整生产服务：Git 差异、模型选择、流执行和用量。
        let response = tokio::time::timeout(std::time::Duration::from_secs(120), service_generate_git_commit_message(
            state.clone(), GitCommitSummaryGenerateRequest {
                conversation_id: owner.to_string(), workspace_path: repository.to_str().unwrap().to_string(),
                repository_key: Some(String::from(".")), include_unstaged: true,
            },
        )).await.expect("bounded live generation").expect("real Git summary");
        assert_eq!(response.provider_id, provider);
        assert_eq!(response.model_id, model);
        assert!(!response.message.trim().is_empty());
        assert!(response.provider_request_count > 0);
        assert!(runtime.session_selection_lease(&owner.to_string()).unwrap().is_none());
        println!("LIVE_GIT_SUMMARY provider={} model={} requests={} estimated_input_tokens={} usage={:?} message={}",
            response.provider_id, response.model_id, response.provider_request_count, response.estimated_input_tokens,
            response.provider_usage, response.message);
    }).await;
    assert_eq!(
        std::fs::read(config_path).unwrap(),
        original_bytes,
        "user configuration must be unchanged"
    );
    // status proves no commit or staging occurred; the only change is the disposable README.
    // status 证明没有提交或暂存，唯一变更是可丢弃的 README。
    let status = std::process::Command::new("rtk")
        .args(["proxy", "git", "status", "--porcelain"])
        .current_dir(repository)
        .output()
        .expect("private Git status");
    assert!(status.status.success());
    assert_eq!(
        String::from_utf8(status.stdout).unwrap().trim(),
        "?? README.md"
    );
}
