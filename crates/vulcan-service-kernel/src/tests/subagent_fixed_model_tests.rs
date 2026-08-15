//! subagent fixed model validation tests.
//! 子代理固定模型校验测试。

use super::*;
use std::path::PathBuf;

/// Builds one request carrying the provided fixed-model fields.
/// 构建一个携带指定固定模型字段的保存请求。
fn fixed_model_save_request(
    // Optional provider identifier.
    // 可选供应商标识。
    provider_id: Option<String>,
    // Optional model identifier.
    // 可选模型标识。
    model_id: Option<String>,
) -> SubagentDefinitionSaveRequest {
    SubagentDefinitionSaveRequest {
        agent_id: String::from("engineering/test-agent"),
        name: String::from("Test Agent"),
        enabled: true,
        provider_model: vulcan_service_api::SubagentProviderModelSummary {
            provider_id,
            model_id,
            ..vulcan_service_api::SubagentProviderModelSummary::default()
        },
        ..SubagentDefinitionSaveRequest::default()
    }
}

/// Builds one public-AI configuration and runtime catalog containing a single visible model.
/// 构建包含单个可见模型的公共 AI 配置与运行时目录。
fn single_visible_public_model_configuration() -> (
    vulcan_provider_adapters::PersistedAiConfiguration,
    std::collections::BTreeMap<String, vulcan_provider_adapters::ConnectionModelCatalog>,
    String,
    String,
) {
    let provider_id = String::from("openrouter");
    let model_id = String::from("vendor/model");
    let connection_id = String::from("connection-a");
    let mut configuration = vulcan_provider_adapters::PersistedAiConfiguration::default();
    configuration.connection_graph.connections.insert(
        connection_id.clone(),
        vulcan_provider_adapters::CredentialConnection {
            id: connection_id.clone(),
            provider_id: provider_id.clone(),
            service_entry_id: String::from("api"),
            credential_profile_id: String::from("api-key"),
            credential_mode: vulcan_provider_adapters::CredentialMode::Token,
            credential_id: String::from("credential-a"),
            display_name: String::from("Account A"),
            secret_reference: String::from("secret-a"),
            management_secret_reference: None,
            service_origin_override: None,
            prompt_cache_by_protocol: std::collections::BTreeMap::new(),
        },
    );
    let catalog = vulcan_provider_adapters::parse_openrouter_model_catalog(
        connection_id.as_str(),
        1,
        r#"{"data":[{"id":"vendor/model","name":"Model","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}}]}"#,
    )
    .expect("public model fixture should parse");
    let catalogs = std::collections::BTreeMap::from([(connection_id, catalog)]);
    (configuration, catalogs, provider_id, model_id)
}

/// Verifies the WebUI agent catalog summary exposes the shared child-agent forbidden tools.
/// 验证 WebUI 智能体目录摘要会暴露共享的智能体禁用工具。
#[test]
fn subagent_catalog_summary_projects_child_forbidden_tools() {
    let summary = subagent_catalog_summary_from_catalog(
        &vulcan_task_session::SubagentCatalog::default(),
        None,
    );
    let expected = vulcan_session_core::child_agent_forbidden_tool_ids()
        .iter()
        .map(|tool_id| (*tool_id).to_string())
        .collect::<Vec<_>>();

    assert_eq!(summary.child_agent_forbidden_tool_ids, expected);
    assert!(
        summary
            .child_agent_forbidden_tool_ids
            .contains(&String::from("todolist")),
        "agent catalog summary must keep todolist hidden from agent tool configuration"
    );
}

/// Builds one loaded definition with a saved fixed-model pair.
/// 构建一个带已保存固定模型组合的已加载定义。
fn loaded_definition_with_fixed_model(
    // Saved provider identifier.
    // 已保存供应商标识。
    provider_id: String,
    // Saved model identifier.
    // 已保存模型标识。
    model_id: String,
) -> vulcan_task_session::SubagentDefinition {
    loaded_definition_with_provider_model(Some(provider_id), Some(model_id))
}

/// Builds one loaded definition with a provider/model snapshot.
/// 构建一个带供应商/模型快照的已加载定义。
fn loaded_definition_with_provider_model(
    // Saved provider identifier.
    // 已保存供应商标识。
    provider_id: Option<String>,
    // Saved model identifier.
    // 已保存模型标识。
    model_id: Option<String>,
) -> vulcan_task_session::SubagentDefinition {
    vulcan_task_session::SubagentDefinition {
        id: String::from("engineering/test-agent"),
        name: String::from("Test Agent"),
        description: None,
        locales: BTreeMap::new(),
        origin: None,
        emoji: String::from("AI"),
        color: String::from("#64748b"),
        vibe: None,
        mode: vulcan_task_session::SubagentDefinitionMode::Expert,
        system_prompt: None,
        system_prompt_append: None,
        provider_model: vulcan_session_core::ProviderModelSnapshot {
            provider_id,
            model_id,
            reasoning_effort: None,
            execution_mode: None,
        },
        tool_exposure: vulcan_session_core::ToolExposureSnapshot::default(),
        prompt_skills: vulcan_session_core::PromptSkillSnapshot::default(),
        context_compaction: vulcan_session_core::ContextCompactionPolicyRequest::default(),
        runtime_policy: vulcan_session_core::ExecutionRuntimePolicyRequest::default(),
        max_depth: None,
        enabled: true,
        source: vulcan_task_session::SubagentDefinitionSource {
            scope: vulcan_task_session::SubagentDefinitionScope::User,
            file: PathBuf::from("engineering/test-agent.md"),
            sidecar_file: None,
            source_key: String::from("user:engineering/test-agent"),
            category_id: Some(String::from("engineering")),
            package_id: None,
            remote_url: None,
            override_of: None,
            source_hash: None,
            version: None,
            etag: None,
            last_modified: None,
            updated_at: None,
        },
        extra: serde_json::Value::Null,
    }
}

/// Verifies a newly selected fixed model must be one of the effective enabled model choices.
/// 验证新选择的固定模型必须来自当前生效启用模型列表。
#[test]
fn validates_new_fixed_model_against_enabled_choices() {
    let (public_ai_configuration, public_ai_catalogs, provider_id, model_id) =
        single_visible_public_model_configuration();
    let request = fixed_model_save_request(Some(provider_id.clone()), Some(model_id.clone()));

    let snapshot = validate_subagent_provider_model_request_with_existing(
        &request,
        None,
        &public_ai_configuration,
        &public_ai_catalogs,
    )
    .expect("enabled model should be accepted");

    assert_eq!(snapshot.provider_id.as_deref(), Some(provider_id.as_str()));
    assert_eq!(snapshot.model_id.as_deref(), Some(model_id.as_str()));
}

/// Verifies invalid new fixed model pairs are rejected before being persisted.
/// 验证无效的新固定模型组合在持久化前会被拒绝。
#[test]
fn rejects_new_fixed_model_outside_enabled_choices() {
    let (public_ai_configuration, public_ai_catalogs, provider_id, _) =
        single_visible_public_model_configuration();
    let request = fixed_model_save_request(
        Some(provider_id),
        Some(String::from("__missing_subagent_model__")),
    );

    let error = validate_subagent_provider_model_request_with_existing(
        &request,
        None,
        &public_ai_configuration,
        &public_ai_catalogs,
    )
    .expect_err("missing model should be rejected");

    assert!(error.contains("不在当前已启用模型列表中"));
}

/// Verifies partial fixed-model requests are rejected instead of becoming ambiguous snapshots.
/// 验证半缺失固定模型请求会被拒绝，而不是写入歧义快照。
#[test]
fn rejects_partial_fixed_model_request() {
    let request = fixed_model_save_request(Some(String::from("openai")), None);

    let error = validate_subagent_provider_model_request_with_existing(
        &request,
        None,
        &vulcan_provider_adapters::PersistedAiConfiguration::default(),
        &std::collections::BTreeMap::new(),
    )
    .expect_err("partial fixed model should be rejected");

    assert!(error.contains("同时提供 Provider 和模型"));
}

/// Verifies an existing invalid pair can be preserved while editing unrelated metadata.
/// 验证编辑无关元数据时可以保留历史中已经存在的无效固定模型组合。
#[test]
fn preserves_existing_invalid_fixed_model_pair() {
    let (public_ai_configuration, public_ai_catalogs, provider_id, _) =
        single_visible_public_model_configuration();
    let stale_model_id = String::from("__removed_subagent_model__");
    let existing = loaded_definition_with_fixed_model(provider_id.clone(), stale_model_id.clone());
    let request = fixed_model_save_request(Some(provider_id.clone()), Some(stale_model_id.clone()));

    let snapshot = validate_subagent_provider_model_request_with_existing(
        &request,
        Some(&existing),
        &public_ai_configuration,
        &public_ai_catalogs,
    )
    .expect("unchanged historical fixed model should be preserved");

    assert_eq!(snapshot.provider_id.as_deref(), Some(provider_id.as_str()));
    assert_eq!(snapshot.model_id.as_deref(), Some(stale_model_id.as_str()));
}

/// Verifies partial historical fixed-model metadata remains visible as an unavailable model diagnostic.
/// 验证历史半缺失固定模型元数据会继续以不可用模型诊断形式展示。
#[test]
fn projects_partial_fixed_model_as_unavailable_diagnostic() {
    let definition = loaded_definition_with_provider_model(
        None,
        Some(String::from("__orphaned_subagent_model__")),
    );
    let catalog = vulcan_task_session::SubagentCatalog {
        definitions: vec![definition],
        ..vulcan_task_session::SubagentCatalog::default()
    };

    let available_model_selectors =
        std::collections::BTreeSet::from([String::from("openai/gpt-test")]);
    let summary = subagent_catalog_summary_from_catalog(&catalog, Some(&available_model_selectors));

    let provider_model = &summary
        .definitions
        .first()
        .expect("summary should contain the test definition")
        .provider_model;
    assert_eq!(provider_model.mode, "fixed");
    assert_eq!(provider_model.available, Some(false));
    assert_eq!(
        provider_model.selector.as_deref(),
        Some("/__orphaned_subagent_model__")
    );
    assert!(
        provider_model
            .unavailable_reason
            .as_deref()
            .unwrap_or_default()
            .contains("固定模型配置不完整")
    );
}
