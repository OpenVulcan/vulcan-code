//! WebUI RPC method dispatch.
//! WebUI RPC 方法分发。

use super::*;

pub(super) async fn dispatch_service_rpc(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: String,
    params: Option<Value>,
) -> Result<Value, String> {
    let params = params.unwrap_or_else(|| json!({}));
    // Measures one service RPC from dispatch entry to serialized result handoff.
    // 衡量单个 service RPC 从分发入口到序列化结果交接的耗时。
    let rpc_started_at = current_unix_millis_value();
    let traces_submit_rpc = method == "service_submit_conversation_prompt";
    if traces_submit_rpc {
        webui_submit_trace("rpc_received", format!("method={method}"));
    }
    let rpc_result = match method.as_str() {
        "startup.snapshot" | "service_startup_snapshot" => {
            rpc_json_result(service_startup_snapshot(state))
        }
        "workbench.snapshot" | "service_workbench_snapshot" => {
            rpc_json_result(service_workbench_snapshot(state))
        }
        "workspace.diff.snapshot" | "service_workspace_diff_snapshot" => rpc_json_result(
            service_workspace_diff_snapshot(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "request")?,
            )
            .await,
        ),
        "workspace.diff.fileDetail" | "service_workspace_diff_file_detail" => rpc_json_result(
            service_workspace_diff_file_detail(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "request")?,
                rpc_required_param(&params, "file")?,
            )
            .await,
        ),
        "workspace.diff.search" | "service_workspace_diff_search" => rpc_json_result(
            service_workspace_diff_search(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "request")?,
                rpc_required_param(&params, "search")?,
            )
            .await,
        ),
        "workspace.git.fileAction" | "service_workspace_git_file_action" => rpc_json_result(
            service_workspace_git_file_action(
                state,
                rpc_optional_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "workspace.files.list" | "service_workspace_files_list" => rpc_json_result(
            service_workspace_files_list(
                rpc_required_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "directoryPath")?,
                rpc_optional_param(&params, "filters")?,
            )
            .await,
        ),
        "workspace.files.read" | "service_workspace_files_read" => rpc_json_result(
            service_workspace_files_read(
                rpc_required_param(&params, "workspacePath")?,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "filePath")?,
                rpc_optional_param(&params, "filters")?,
            )
            .await,
        ),
        "service_refresh_luaskills_catalog" => {
            rpc_json_result(service_refresh_luaskills_catalog(state).await)
        }
        "service_inspect_local_plugin" => rpc_json_result(
            service_inspect_local_plugin(state, rpc_required_param(&params, "sourcePath")?).await,
        ),
        "service_install_local_plugin" => rpc_json_result(
            service_install_local_plugin(
                state,
                rpc_required_param(&params, "sourcePath")?,
                rpc_required_param(&params, "approvedCapabilities")?,
                rpc_required_param(&params, "allowRegistryDependencies")?,
            )
            .await,
        ),
        "service_uninstall_user_plugin" => rpc_json_result(
            service_uninstall_user_plugin(state, rpc_required_param(&params, "pluginId")?).await,
        ),
        "service_load_plugin_ui_module" => rpc_json_result(service_load_plugin_ui_module(
            state,
            rpc_required_param(&params, "pluginId")?,
            rpc_required_param(&params, "mountId")?,
        )),
        "service_read_plugin_text_asset" => rpc_json_result(service_read_plugin_text_asset(
            state,
            rpc_required_param(&params, "pluginId")?,
            rpc_required_param(&params, "path")?,
        )),
        "service_invoke_plugin_action" => rpc_json_result(
            service_invoke_plugin_action(
                state,
                rpc_required_param(&params, "pluginId")?,
                rpc_required_param(&params, "actionId")?,
                rpc_required_param(&params, "arguments")?,
                rpc_optional_param(&params, "workspacePath")?,
            )
            .await,
        ),
        "service_save_luaskills_runtime_settings" => rpc_json_result(
            service_save_luaskills_runtime_settings(
                state,
                rpc_required_param(&params, "settings")?,
            )
            .await,
        ),
        "service_retry_luaskills_managed_runtimes" => rpc_json_result(
            service_retry_luaskills_managed_runtimes(
                app,
                state,
                rpc_required_param(&params, "target")?,
            )
            .await,
        ),
        "service_install_luaskill" => rpc_json_result(
            service_install_luaskill(
                state,
                rpc_optional_param(&params, "skillId")?,
                rpc_required_param(&params, "source")?,
                rpc_required_param(&params, "sourceType")?,
            )
            .await,
        ),
        "service_update_luaskill" => rpc_json_result(
            service_update_luaskill(state, rpc_required_param(&params, "skillId")?).await,
        ),
        "service_set_luaskill_enabled" => rpc_json_result(
            service_set_luaskill_enabled(
                state,
                rpc_required_param(&params, "skillId")?,
                rpc_required_param(&params, "enabled")?,
            )
            .await,
        ),
        "service_set_luaskills_system_plugin_enabled" => rpc_json_result(
            service_set_luaskills_system_plugin_enabled(
                state,
                rpc_required_param(&params, "pluginId")?,
                rpc_required_param(&params, "enabled")?,
            )
            .await,
        ),
        "service_uninstall_luaskill" => rpc_json_result(
            service_uninstall_luaskill(state, rpc_required_param(&params, "skillId")?).await,
        ),
        "service_set_luaskill_tool_registration_state" => rpc_json_result(
            service_set_luaskill_tool_registration_state(
                state,
                rpc_required_param(&params, "skillId")?,
                rpc_required_param(&params, "localName")?,
                rpc_required_param(&params, "registrationState")?,
            )
            .await,
        ),
        "service_refresh_agent_skills_catalog" => {
            rpc_json_result(service_refresh_agent_skills_catalog(state).await)
        }
        "service_open_agent_skills_user_directory" => {
            rpc_json_result(service_open_agent_skills_user_directory(state).await)
        }
        "service_save_agent_skill" => rpc_json_result(
            service_save_agent_skill(state, rpc_required_param(&params, "skill")?).await,
        ),
        "service_delete_agent_skill" => rpc_json_result(
            service_delete_agent_skill(state, rpc_required_param(&params, "name")?).await,
        ),
        "service_restore_agent_skill_default" => rpc_json_result(
            service_restore_agent_skill_default(state, rpc_required_param(&params, "name")?).await,
        ),
        "service_save_agent_skill_category" => rpc_json_result(
            service_save_agent_skill_category(state, rpc_required_param(&params, "category")?)
                .await,
        ),
        "service_delete_agent_skill_category" => rpc_json_result(
            service_delete_agent_skill_category(state, rpc_required_param(&params, "categoryId")?)
                .await,
        ),
        "service_set_agent_skill_category" => rpc_json_result(
            service_set_agent_skill_category(
                state,
                rpc_required_param(&params, "name")?,
                rpc_required_param(&params, "categoryId")?,
            )
            .await,
        ),
        "service_refresh_subagent_catalog" => {
            rpc_json_result(service_refresh_subagent_catalog(state).await)
        }
        "service_open_subagent_user_directory" => {
            rpc_json_result(service_open_subagent_user_directory(state).await)
        }
        "service_set_subagent_definition_enabled" => rpc_json_result(
            service_set_subagent_definition_enabled(
                state,
                rpc_required_param(&params, "agentId")?,
                rpc_required_param(&params, "enabled")?,
            )
            .await,
        ),
        "service_save_subagent_definition" => rpc_json_result(
            service_save_subagent_definition(state, rpc_required_param(&params, "definition")?)
                .await,
        ),
        "service_delete_subagent_definition" => rpc_json_result(
            service_delete_subagent_definition(state, rpc_required_param(&params, "agentId")?)
                .await,
        ),
        "service_restore_subagent_definition_default" => rpc_json_result(
            service_restore_subagent_definition_default(
                state,
                rpc_required_param(&params, "agentId")?,
            )
            .await,
        ),
        "service_set_agent_skill_enabled" => rpc_json_result(
            service_set_agent_skill_enabled(
                state,
                rpc_required_param(&params, "name")?,
                rpc_required_param(&params, "enabled")?,
            )
            .await,
        ),
        "service_load_project_agent_skill_overrides" => {
            rpc_json_result(service_load_project_agent_skill_overrides(
                state,
                rpc_required_param(&params, "projectId")?,
            ))
        }
        "service_set_agent_skill_project_state" => rpc_json_result(
            service_set_agent_skill_project_state(
                state,
                rpc_required_param(&params, "projectId")?,
                rpc_required_param(&params, "name")?,
                rpc_required_param(&params, "stateKey")?,
            )
            .await,
        ),
        "service_run_vmm_memory_action" => rpc_json_result(
            service_run_vmm_memory_action(state, rpc_required_param(&params, "request")?).await,
        ),
        "service_load_project_vmm_config" => rpc_json_result(service_load_project_vmm_config(
            state,
            rpc_required_param(&params, "projectId")?,
        )),
        "service_preview_project_attachment_cleanup" => rpc_json_result(
            service_preview_project_attachment_cleanup(
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_execute_project_attachment_cleanup" => rpc_json_result(
            service_execute_project_attachment_cleanup(
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_vmm_memory_config" => rpc_json_result(
            service_save_vmm_memory_config(state, rpc_required_param(&params, "request")?).await,
        ),
        "service_session_messages" => rpc_json_result(
            service_session_messages(app, state, rpc_required_param(&params, "sessionId")?).await,
        ),
        "service_session_history_page" => rpc_json_result(
            service_session_history_page(
                app,
                state,
                rpc_required_param(&params, "sessionId")?,
                rpc_required_param(&params, "beforeOrdinal")?,
            )
            .await,
        ),
        "service_session_tool_detail" => rpc_json_result(
            service_session_tool_detail(
                state,
                rpc_required_param(&params, "sessionId")?,
                rpc_optional_param(&params, "recordId")?,
                rpc_optional_param(&params, "toolCallId")?,
                rpc_optional_param(&params, "seq")?,
                rpc_optional_param(&params, "messageIndex")?,
            )
            .await,
        ),
        "service_submit_conversation_prompt" => rpc_json_result(
            service_submit_conversation_prompt(
                app,
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "prompt")?,
                rpc_optional_param(&params, "hiddenModelGuidance")?,
                rpc_optional_param(&params, "executionMode")?,
                rpc_optional_param(&params, "reasoningEffort")?,
                rpc_optional_param(&params, "attachments")?,
                rpc_optional_param(&params, "disabledAiCapabilityKinds")?,
                rpc_optional_param(&params, "disabledAiCapabilityTargetSelectors")?,
                rpc_optional_param(&params, "aiCapabilityTargetOrderOverrides")?,
            )
            .await,
        ),
        "service_stop_conversation_run" => rpc_json_result(
            service_stop_conversation_run(
                app,
                state,
                rpc_required_param(&params, "conversationId")?,
                rpc_optional_param(&params, "mode")?,
            )
            .await,
        ),
        "service_stop_command_session" => rpc_json_result(
            service_stop_command_session(
                app,
                state,
                rpc_required_param(&params, "commandSessionId")?,
                rpc_optional_param(&params, "ownerSessionId")?,
            )
            .await,
        ),
        "service_get_command_session_snapshot" => rpc_json_result(
            service_get_command_session_snapshot(
                state,
                rpc_required_param(&params, "commandSessionId")?,
                rpc_optional_param(&params, "ownerSessionId")?,
            )
            .await,
        ),
        "service_send_command_session_input" => rpc_json_result(
            service_send_command_session_input(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_resize_command_session" => rpc_json_result(
            service_resize_command_session(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_get_user_prompt_edit_draft" => rpc_json_result(
            service_get_user_prompt_edit_draft(
                state,
                rpc_required_param(&params, "conversationId")?,
                rpc_required_param(&params, "messageId")?,
            )
            .await,
        ),
        "service_submit_user_prompt_edit" => rpc_json_result(
            service_submit_user_prompt_edit(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_fork_conversation" => rpc_json_result(
            service_fork_conversation(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_save_session_goal" => rpc_json_result(
            service_save_session_goal(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_resume_session_goal" => rpc_json_result(
            service_resume_session_goal(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_clear_session_goal" => rpc_json_result(
            service_clear_session_goal(app, state, rpc_required_param(&params, "request")?).await,
        ),
        "service_update_session_goal_status" => rpc_json_result(
            service_update_session_goal_status(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_set_session_plan_mode" => rpc_json_result(
            service_set_session_plan_mode(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_set_session_vmm_policy" => rpc_json_result(
            service_set_session_vmm_policy(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_compact_selected_conversation_context" => {
            rpc_json_result(service_compact_selected_conversation_context(app, state).await)
        }
        "service_reply_pending_question" => rpc_json_result(service_reply_pending_question(
            app,
            state,
            rpc_required_param(&params, "requestId")?,
            rpc_required_param(&params, "answers")?,
        )),
        "service_reject_pending_question" => rpc_json_result(service_reject_pending_question(
            app,
            state,
            rpc_required_param(&params, "requestId")?,
        )),
        "service_open_subagent_panel" => rpc_json_result(
            service_open_subagent_panel(
                state,
                rpc_required_param(&params, "parentConversationId")?,
                rpc_required_param(&params, "taskId")?,
            )
            .await,
        ),
        "service_close_subagent_panel" => {
            rpc_json_result(service_close_subagent_panel(app, state).await)
        }
        "service_stop_subagent_task" => rpc_json_result(
            service_stop_subagent_task(
                app,
                state,
                rpc_required_param(&params, "parentConversationId")?,
                rpc_required_param(&params, "taskId")?,
            )
            .await,
        ),
        "service_close_subagent_task" => rpc_json_result(
            service_close_subagent_task(
                app,
                state,
                rpc_required_param(&params, "parentConversationId")?,
                rpc_required_param(&params, "taskId")?,
            )
            .await,
        ),
        "service_edit_pending_prompt" => rpc_json_result(
            service_edit_pending_prompt(app, state, rpc_required_param(&params, "entryId")?).await,
        ),
        "service_insert_pending_prompt" => rpc_json_result(
            service_insert_pending_prompt(app, state, rpc_required_param(&params, "entryId")?)
                .await,
        ),
        "service_remove_pending_prompt" => rpc_json_result(
            service_remove_pending_prompt(app, state, rpc_required_param(&params, "entryId")?)
                .await,
        ),
        "service_reorder_pending_prompts" => rpc_json_result(
            service_reorder_pending_prompts(
                app,
                state,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "draggedEntryId")?,
                rpc_optional_param(&params, "targetEntryId")?,
            )
            .await,
        ),
        "service_pick_composer_attachments" => rpc_json_result(
            service_pick_composer_attachments(
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_optional_param(&params, "kind")?,
            )
            .await,
        ),
        "service_stage_composer_attachment_paths" => rpc_json_result(
            service_stage_composer_attachment_paths(
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "paths")?,
            )
            .await,
        ),
        "service_stage_composer_attachment_bytes" => rpc_json_result(
            service_stage_composer_attachment_bytes(
                state,
                rpc_optional_param(&params, "projectId")?,
                rpc_optional_param(&params, "conversationId")?,
                rpc_required_param(&params, "fileName")?,
                rpc_required_param(&params, "mimeType")?,
                rpc_required_param(&params, "dataBase64")?,
            )
            .await,
        ),
        "service_select_workbench_model" => rpc_json_result(
            service_select_workbench_model(
                app,
                state,
                rpc_required_param(&params, "providerId")?,
                rpc_required_param(&params, "modelId")?,
            )
            .await,
        ),
        "service_list_workbench_model_secondary_routes" => rpc_json_result(
            service_list_workbench_model_secondary_routes(
                state,
                rpc_required_param(&params, "providerId")?,
                rpc_required_param(&params, "modelId")?,
            )
            .await,
        ),
        "service_select_workbench_model_secondary_route" => rpc_json_result(
            service_select_workbench_model_secondary_route(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_select_workbench_reasoning_effort" => rpc_json_result(
            service_select_workbench_reasoning_effort(
                app,
                state,
                rpc_required_param(&params, "effort")?,
            )
            .await,
        ),
        "service_select_workbench_thinking_display_mode" => rpc_json_result(
            service_select_workbench_thinking_display_mode(
                app,
                state,
                rpc_required_param(&params, "mode")?,
            )
            .await,
        ),
        "service_select_webui_locale" => rpc_json_result(
            service_select_webui_locale(app, state, rpc_required_param(&params, "locale")?).await,
        ),
        "service_save_provider_configuration" => rpc_json_result(
            service_save_provider_configuration(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_remove_provider_configuration" => rpc_json_result(
            service_remove_provider_configuration(
                app,
                state,
                rpc_required_param(&params, "providerId")?,
            )
            .await,
        ),
        "service_save_provider_enabled_models" => rpc_json_result(
            service_save_provider_enabled_models(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_provider_model_override" => rpc_json_result(
            service_save_provider_model_override(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_git_commit_settings" => rpc_json_result(
            service_save_git_commit_settings(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_generate_git_commit_message" => rpc_json_result(
            service_generate_git_commit_message(state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_run_git_commit_action" => rpc_json_result(
            service_run_git_commit_action(state, rpc_required_param(&params, "request")?).await,
        ),
        "service_save_ai_capability_vendor" => rpc_json_result(
            service_save_ai_capability_vendor(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_remove_ai_capability_vendor" => rpc_json_result(
            service_remove_ai_capability_vendor(
                app,
                state,
                rpc_required_param(&params, "vendorId")?,
            )
            .await,
        ),
        "service_set_ai_capability_target_enabled" => rpc_json_result(
            service_set_ai_capability_target_enabled(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_ai_capability_targets" => rpc_json_result(
            service_save_ai_capability_targets(app, state, rpc_required_param(&params, "request")?)
                .await,
        ),
        "service_save_session_ai_capability_configuration" => rpc_json_result(
            service_save_session_ai_capability_configuration(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_save_mcp_server_configuration" => rpc_json_result(
            service_save_mcp_server_configuration(
                app,
                state,
                rpc_required_param(&params, "request")?,
            )
            .await,
        ),
        "service_remove_mcp_server_configuration" => rpc_json_result(
            service_remove_mcp_server_configuration(
                app,
                state,
                rpc_required_param(&params, "serverId")?,
            )
            .await,
        ),
        "service_open_project_directory_path" => rpc_json_result(
            service_open_project_directory_path(state, rpc_required_param(&params, "path")?),
        ),
        "service_open_project_directory" => rpc_json_result(service_open_project_directory(
            state,
            rpc_required_param(&params, "projectId")?,
        )),
        "service_browse_project_directories" => rpc_json_result(
            service_browse_project_directories(state, rpc_optional_param(&params, "path")?),
        ),
        "service_add_project_from_path" => rpc_json_result(
            service_add_project_from_path(
                state,
                PathBuf::from(rpc_required_param::<String>(&params, "path")?),
            )
            .await,
        ),
        "service_toggle_project_pinned" => rpc_json_result(
            service_toggle_project_pinned(state, rpc_required_param(&params, "projectId")?).await,
        ),
        "service_reorder_projects" => rpc_json_result(
            service_reorder_projects(state, rpc_required_param(&params, "projectIds")?).await,
        ),
        "service_rename_project" => rpc_json_result(
            service_rename_project(
                state,
                rpc_required_param(&params, "projectId")?,
                rpc_required_param(&params, "name")?,
            )
            .await,
        ),
        "service_set_project_icon" => rpc_json_result(
            service_set_project_icon(
                state,
                rpc_required_param(&params, "projectId")?,
                rpc_required_param(&params, "iconPack")?,
                rpc_required_param(&params, "iconKey")?,
            )
            .await,
        ),
        "service_reset_project_icon" => rpc_json_result(
            service_reset_project_icon(state, rpc_required_param(&params, "projectId")?).await,
        ),
        "service_set_expanded_task_projects" => rpc_json_result(
            service_set_expanded_task_projects(state, rpc_required_param(&params, "projectIds")?)
                .await,
        ),
        "service_archive_project_conversations" => rpc_json_result(
            service_archive_project_conversations(
                app,
                state,
                rpc_required_param(&params, "projectId")?,
            )
            .await,
        ),
        "service_remove_project" => rpc_json_result(
            service_remove_project(app, state, rpc_required_param(&params, "projectId")?).await,
        ),
        "service_rename_conversation" => rpc_json_result(
            service_rename_conversation(
                state,
                rpc_required_param(&params, "conversationId")?,
                rpc_required_param(&params, "name")?,
            )
            .await,
        ),
        "service_archive_conversation" => rpc_json_result(
            service_archive_conversation(
                app,
                state,
                rpc_required_param(&params, "conversationId")?,
            )
            .await,
        ),
        "service_mark_conversation_read" => rpc_json_result(
            service_mark_conversation_read(state, rpc_required_param(&params, "conversationId")?)
                .await,
        ),
        "service_select_conversation" => rpc_json_result(
            service_select_conversation(state, rpc_required_param(&params, "conversationId")?)
                .await,
        ),
        "service_open_external_url" => rpc_json_result(service_open_external_url(
            state,
            rpc_required_param(&params, "url")?,
            rpc_optional_param(&params, "userConfirmed")?.unwrap_or(false),
        )),
        _ => Err(format!("unknown service RPC method: {method}")),
    };
    if traces_submit_rpc {
        webui_submit_trace(
            "rpc_returning",
            format!(
                "method={method} ok={} elapsed_ms={}",
                rpc_result.is_ok(),
                current_unix_millis_value().saturating_sub(rpc_started_at)
            ),
        );
    }
    rpc_result
}
