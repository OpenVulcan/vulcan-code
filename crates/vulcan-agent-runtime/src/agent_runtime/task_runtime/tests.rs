use super::*;
use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::execution_models::ExecutionPromptPurpose;
use crate::provider_config::{
    PersistedAiToolConfiguration, ProjectPromptSkillsConfiguration, ReasoningEffortLevel,
};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Builds one minimal controller configuration for task-runtime prompt tests.
/// 为 task-runtime 提示词测试构建一份最小控制器配置。
fn build_test_controller_config(system_prompt: &str) -> CoreControllerConfig {
    CoreControllerConfig {
        provider_id: String::from("provider"),
        model_id: String::from("model"),
        system_prompt: system_prompt.to_string(),
        prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
            system_prompt.to_string().clone(),
        ),
        temperature: None,
        max_turns: 8,
        additional_params: None,
        context_injection: None,
    }
}

/// Verifies host payload status parsing rejects unknown execution states.
/// 验证宿主载荷状态解析会拒绝未知执行状态。
#[test]
fn task_session_status_from_execution_status_text_rejects_unknown_status() {
    let status = task_session_status_from_execution_status_text("almost_done");

    assert!(
        status.is_err(),
        "unknown execution status must not be projected as running"
    );
}

/// Verifies failed TaskSession states are stored as terminal lifecycle events.
/// 验证失败的 TaskSession 状态会保存为终态生命周期事件。
#[test]
fn failed_task_session_status_uses_terminal_event_kind() {
    let event_kind = task_session_event_kind_for_status(TaskSessionStatus::Failed);

    assert_eq!(event_kind, SessionEventKind::TaskTerminal);
}

/// Verifies failed TaskSession states are treated as cached terminal records.
/// 验证失败的 TaskSession 状态会被视为可缓存读取的终态记录。
#[test]
fn failed_task_session_status_is_cached_terminal_state() {
    let record = build_task_session_record(TaskSessionRecordInit {
        task_id: Uuid::new_v4(),
        public_task_id: String::from("failed-reviewer"),
        origin_session_id: Uuid::new_v4(),
        parent_task_id: None,
        depth: 1,
        description: Some(String::from("failed reviewer")),
        subagent: None,
        invocation_config: TaskInvocationConfig::main("review failed"),
        display_title: String::from("Failed reviewer"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("review failed"),
        last_status: String::from("failed"),
    });

    assert!(task_session_record_has_cached_status(&record));
    assert!(task_session_metadata_has_cached_status(
        TaskSessionStatus::Failed
    ));
}

/// Verifies stale active updates cannot reopen a failed TaskSession.
/// 验证迟到的活跃状态不能重新打开失败的 TaskSession。
#[test]
fn resolve_persisted_task_session_status_preserves_failed_terminal_state() {
    let resolved = resolve_persisted_task_session_status(
        TaskSessionStatus::Failed,
        TaskSessionStatus::Running,
    );

    assert_eq!(resolved, TaskSessionStatus::Failed);
}

/// Writes the minimal segmented root-session metadata needed by TaskSession store tests.
/// 写入 TaskSession 存储测试所需的最小分段根会话元数据。
fn write_test_segmented_root_metadata(root: &std::path::Path, root_session_id: SessionId) {
    fs::create_dir_all(root).expect("create segmented root");
    let metadata = json!({
        "schema_version": 1,
        "session_id": root_session_id.to_string(),
        "project_tag": "test-project",
        "created_at": "100",
        "updated_at": "100",
        "project_id": "test-project",
        "title": "Test root",
        "title_source": "manual",
        "canonical_source": "task-runtime-test",
        "archived_at": null,
        "active_segment": "s001.jsonl",
        "task_dir": "task",
        "next_global_seq": 1,
        "latest_rollback_seq": null,
        "segments": [
            {
                "file": "s001.jsonl",
                "start_seq": 0,
                "line_count": 1,
                "byte_len": 0,
                "first_timestamp": "100",
                "last_timestamp": "100",
                "first_turn_index": null,
                "last_turn_index": null,
                "last_compaction_seq": null
            }
        ],
        "segment_policy": {
            "max_lines": 3000,
            "max_bytes": 8388608
        },
        "tasks": []
    });
    let file = File::create(root.join("metadata.json")).expect("create root metadata");
    serde_json::to_writer_pretty(file, &metadata).expect("write root metadata");
    let mut segment = File::create(root.join("s001.jsonl")).expect("create root segment");
    writeln!(
        segment,
        "{}",
        json!({
            "seq": 0,
            "timestamp": "100",
            "type": "session_meta",
            "payload": { "session_id": root_session_id.to_string() },
            "turn_index": null
        })
    )
    .expect("write root segment");
}

/// Builds a resolved TaskSession configuration for controller-config tests.
/// 为控制器配置测试构建已解析 TaskSession 配置。
fn test_resolved_config(
    base: &CoreControllerConfig,
    invocation: &TaskInvocationConfig,
) -> TaskSessionResolvedConfig {
    TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
        global_default_config: SessionEffectiveConfig::default(),
        parent_config: Some(core_controller_config_to_effective_config(base)),
        profile: None,
        invocation: invocation.clone(),
    })
}

/// Builds one minimal child-agent argument object for task-runtime tests.
/// 为 task-runtime 测试构建一份最小子代理参数对象。
fn test_task_args(prompt: &str, task_id: Option<&str>) -> TaskToolInvocationArgs {
    TaskToolInvocationArgs {
        prompt: prompt.to_string(),
        description: None,
        task_id: task_id.map(str::to_string),
        subagent: None,
        agent_mode: None,
        subagent_profile_id: None,
        inherit_parent_config: None,
        system_prompt: None,
        system_prompt_append: None,
        provider: None,
        model: None,
        execution_mode: None,
        provider_model: None,
        tools: None,
        skills: None,
        prompt_skills: None,
        luaskills: None,
        mcp: None,
        context_compaction: None,
        runtime_policy: None,
        extra: None,
    }
}

mod persistence_store;
mod preparation_config;
mod registry_mailbox;
mod runtime_flow;
mod tool_args;
