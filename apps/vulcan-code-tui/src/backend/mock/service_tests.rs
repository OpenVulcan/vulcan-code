// The synthetic authority is tested through the same public API builders as the UI.
// 使用与界面相同的公开 API 构建器测试合成权威数据。

use super::*;
use crate::service_api::{memory_actions as vmm, settings_actions as settings};
use vulcan_service_api::SystemPromptScope;
/// Full-save and metadata-only mutations are observable through their original loaders.
/// 完整保存与仅元数据变更均可通过其原始加载器观察。
#[test]
fn settings_writes_reload_authority_without_retaining_credentials() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    service
        .answer(&settings::save_git_commit_settings(
            json!({"branchPrefix":"feature/","prompt":"demo","modelId":"demo-model"}),
        ))
        .unwrap();
    assert_eq!(service.workbench["gitCommit"]["branchPrefix"], "feature/");
    service.answer(&settings::add_public_ai_connection(json!({"providerId":"demo-provider","serviceEntryId":"demo-entry",
        "credentialProfileId":"demo-profile","credentialMode":"token","displayName":"Second","secret":"discard-me"}))).unwrap();
    assert!(!service.loads.to_string().contains("discard-me"));
    // Public AI settings.
    // 公开 AI 设置。
    let ai = &service.loads["service_public_ai_settings_snapshot"];
    // Replacement.
    // 准确替代默认连接身份。
    let replacement = ai["connections"].as_array().unwrap().last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        service
            .answer(&settings::remove_public_ai_connection(
                "demo-connection",
                None
            ))
            .is_err()
    );
    service
        .answer(&settings::remove_public_ai_connection(
            "demo-connection",
            Some(&replacement),
        ))
        .unwrap();
    assert_eq!(
        service.loads["service_public_ai_settings_snapshot"]["providerDefaults"]["demo-provider"],
        replacement
    );
}

/// Provider-task cancellation and an MCP sign-in follow the service phases through the public builders.
/// 供应商任务取消与 MCP 登录经公开构建器遵循服务阶段。
#[test]
fn provider_task_cancel_and_mcp_sign_in_follow_service_phases() {
    let mut service = FakeService::new();
    service
        .answer(&settings::cancel_provider_task("demo-task-video", None))
        .unwrap();
    let tasks = service.answer(&settings::list_provider_tasks()).unwrap();
    let task = tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["executionId"] == "demo-task-video")
        .expect("the cancelled task is still listed");
    assert_eq!(task["phase"], "cancellation_requested");
    assert!(
        service
            .answer(&settings::cancel_provider_task("missing-task", None))
            .is_err()
    );

    use crate::service_api::mcp_settings as mcp;
    service.answer(&mcp::start_mcp_oauth("demo-mcp")).unwrap();
    let status = service
        .answer(&mcp::load_mcp_oauth_status("demo-mcp"))
        .unwrap();
    assert_eq!(status["serverId"], "demo-mcp");
    assert_eq!(status["flow"]["phase"], "awaiting_callback");
    let flow_id = status["flow"]["flowId"].as_str().unwrap().to_string();
    assert!(
        service
            .answer(&mcp::finish_mcp_oauth(json!({
                "serverId": "demo-mcp", "flowId": "stale-flow", "callbackUrl": "https://example.invalid/callback?code=x"
            })))
            .is_err()
    );
    service
        .answer(&mcp::finish_mcp_oauth(json!({
            "serverId": "demo-mcp", "flowId": flow_id, "callbackUrl": "https://example.invalid/callback?code=x"
        })))
        .unwrap();
    let status = service
        .answer(&mcp::load_mcp_oauth_status("demo-mcp"))
        .unwrap();
    assert_eq!(status["authStatus"], "authenticated");
    service.answer(&mcp::remove_mcp_oauth("demo-mcp")).unwrap();
    let status = service
        .answer(&mcp::load_mcp_oauth_status("demo-mcp"))
        .unwrap();
    assert_eq!(status["authStatus"], "not_authenticated");
    assert!(status["flow"].is_null());
}

/// Native removal includes accurate impact and restores only moved records, preserving new content.
/// 原生移除包含准确影响，恢复时仅恢复已移动记录并保留新内容。
#[test]
fn removal_and_restore_preserve_independent_and_new_records() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    // Exact service call.
    // 准确服务调用。
    let call = vmm::native_removal_preview(101, "sessions", vec![json!("demo-session")]);
    // Plan.
    // 已审查操作计划。
    let plan = service.answer(&call).unwrap();
    assert_eq!(plan["data"]["value"]["impact"]["turnIds"], json!(["201"]));
    assert_eq!(plan["data"]["value"]["impact"]["memoryIds"], json!(["301"]));
    // Confirmation.
    // 明确预检确认请求。
    let confirmation = vmm::native_removal_recycle(&plan).unwrap();
    service.answer(&confirmation).unwrap();
    assert!(service.answer(&confirmation).is_err());
    // Removal changes active/index counts and retained counts through their original loaders.
    // 移除通过原始加载器改变活动、索引及保留计数。
    let vector = service.answer(&vmm::native_vector_status()).unwrap();
    assert_eq!(vector["data"]["value"]["activeMemoryCount"], 0);
    assert_eq!(vector["data"]["value"]["recycleBinMemoryCount"], 1);
    assert_eq!(
        service.answer(&vmm::native_lexical_status()).unwrap()["data"]["value"]["documentCount"],
        0
    );
    assert_eq!(
        service.answer(&vmm::native_retention_status()).unwrap()["data"]["value"]["recycleBatchCount"],
        1
    );
    assert!(
        service.records[&101]["memories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service.memory["native_profile_records"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // New content is appended after removal to exercise restoration without rollback.
    // 移除后追加新内容，验证恢复不会回滚这些记录。
    service.records.get_mut(&101).unwrap()["memories"]
        .as_array_mut()
        .unwrap()
        .push(json!({"memoryId":302,"sourceTurnId":202,"sessionSourceId":"new-session"}));
    // Batch.
    // 回收批次身份。
    let batch = service.memory["native_recycle_bin"]["batches"][0]["batchId"]
        .as_str()
        .unwrap()
        .to_string();
    service
        .answer(&vmm::native_recycle_restore(&batch, service.version))
        .unwrap();
    assert_eq!(
        service.records[&101]["memories"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        service.records[&102]["memories"].as_array().unwrap().len(),
        0
    );
    // Restoration merges current and retained content, and its observations report both.
    // 恢复合并当前及保留内容，观察结果应同时统计两者。
    let vector = service.answer(&vmm::native_vector_status()).unwrap();
    assert_eq!(vector["data"]["value"]["activeMemoryCount"], 2);
    assert_eq!(vector["data"]["value"]["recycleBinMemoryCount"], 0);
    assert_eq!(
        service.answer(&vmm::native_lexical_status()).unwrap()["data"]["value"]["indexedRowCount"],
        2
    );
    assert_eq!(
        service.answer(&vmm::native_retention_status()).unwrap()["data"]["value"]["recycleBatchCount"],
        0
    );
    assert!(
        service.memory["native_recycle_bin"]["batches"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

/// A changed content version invalidates destructive previews while derived lexical rebuilds keep it.
/// 内容版本变更使破坏性预检失效，而派生词法重建保持该版本。
#[test]
fn preview_versions_and_derived_rebuilds_are_distinct() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    // Exact service call.
    // 准确服务调用。
    let call = vmm::native_project_clear_preview(101);
    // Preview.
    // 只读预检。
    let preview = service.answer(&call).unwrap();
    assert_eq!(preview["data"]["kind"], "native_clear_preview");
    // Confirm.
    // 准确预检确认请求。
    let confirm = vmm::native_project_confirmation(&call, &preview).unwrap();
    service
        .answer(&vmm::native_lexical_rebuild(service.version))
        .unwrap();
    assert_eq!(service.version, 1);
    // Retention.
    // 保留策略组。
    let retention = service.answer(&vmm::native_retention_preview()).unwrap();
    service
        .answer(&vmm::native_project_rename(102, "Changed"))
        .unwrap();
    assert!(service.answer(&confirm).is_err());
    assert!(
        service
            .answer(&vmm::native_retention_run(&retention).unwrap())
            .is_err()
    );
    assert_eq!(
        service.records[&101]["memories"].as_array().unwrap().len(),
        1
    );
}

/// Migration reviews and applies the same session collision rename to every moved source record.
/// 迁移审查并将相同会话冲突改名应用到每条移动的来源记录。
#[test]
fn migration_keeps_reviewed_collision_renames_and_target_content() {
    // Service.
    // 合成服务权威状态。
    let mut service = FakeService::new();
    // A target-owned session collides by name, but its existing memory must stay untouched.
    // 目标持有的会话名称发生冲突，但其已有记忆必须保持不变。
    service.records.get_mut(&102).unwrap()["sessions"] =
        json!([{"sessionSourceId":"demo-session","displayTitle":"Target session"}]);
    service.records.get_mut(&102).unwrap()["memories"] =
        json!([{"memoryId":999,"summary":"Target memory","sessionSourceId":"demo-session"}]);
    // Exact service call.
    // 准确服务调用。
    let call = vmm::native_project_migration_preview(101, 102);
    // Preview.
    // 只读预检。
    let preview = service.answer(&call).unwrap();
    // Renamed.
    // 审查中已冻结的冲突新名称。
    let renamed = preview["data"]["value"]["sessionRenames"][0]["to"].clone();
    assert_eq!(preview["data"]["kind"], "native_migration_preview");
    assert_eq!(
        preview["data"]["value"]["records"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // Confirm.
    // 准确预检确认请求。
    let mut confirm = vmm::native_project_confirmation(&call, &preview).unwrap();
    confirm.params["request"]["payload"]["acceptSessionRenames"] = json!(false);
    assert!(service.answer(&confirm).is_err());
    confirm.params["request"]["payload"]["acceptSessionRenames"] = json!(true);
    service.answer(&confirm).unwrap();
    assert_eq!(service.records[&102]["memories"][0]["memoryId"], 999);
    assert_eq!(
        service.records[&102]["memories"][1]["sessionSourceId"],
        renamed
    );
    assert_eq!(
        service.records[&102]["turns"][0]["sessionSourceId"],
        renamed
    );
    assert!(
        service.records[&101]["memories"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

/// System prompt overrides follow project, user, built-in precedence and reset to the next layer.
/// 系统提示词覆盖遵循项目、用户、内置优先级，重置后回到下一层。
#[test]
fn system_prompt_layers_save_reset_and_reject_invalid_targets() {
    // Synthetic service authority with registered host projects.
    // 带已登记宿主项目的合成服务权威状态。
    let mut service = FakeService::new();
    let initial = service
        .answer(&settings::load_system_prompt_settings(None))
        .unwrap();
    assert_eq!(initial["effectiveSource"], "builtIn");
    assert!(initial["projectPath"].is_null());
    let user = service
        .answer(&settings::save_system_prompt(
            SystemPromptScope::User,
            None,
            "  user rules \n",
        ))
        .unwrap();
    assert_eq!(user["userText"], "user rules");
    assert_eq!(user["effectiveSource"], "user");
    let project = service
        .answer(&settings::save_system_prompt(
            SystemPromptScope::Project,
            Some("p-core"),
            "project rules",
        ))
        .unwrap();
    assert_eq!(project["projectId"], "p-core");
    assert_eq!(project["effectiveSource"], "project");
    // Other projects keep inheriting the user layer.
    // 其他项目继续继承用户层级。
    assert_eq!(
        service
            .answer(&settings::load_system_prompt_settings(Some("p-tui")))
            .unwrap()["effectiveSource"],
        "user"
    );
    let reset = service
        .answer(&settings::reset_system_prompt(
            SystemPromptScope::Project,
            Some("p-core"),
        ))
        .unwrap();
    assert!(reset["projectText"].is_null());
    assert_eq!(reset["effectiveSource"], "user");
    assert!(
        service
            .answer(&settings::save_system_prompt(
                SystemPromptScope::User,
                None,
                "   "
            ))
            .is_err()
    );
    assert!(
        service
            .answer(&settings::reset_system_prompt(
                SystemPromptScope::Project,
                None
            ))
            .is_err()
    );
    assert!(
        service
            .answer(&settings::load_system_prompt_settings(Some("p-missing")))
            .is_err()
    );
}
