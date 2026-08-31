//! Regression coverage for global account rebinding, pending selection, and exact session revocation.
//! 全局账号重绑、待生效选择与精确会话撤销的回归覆盖。

use super::*;
use vulcan_ai_contract::{AiOperation, ModelCapabilities};
use vulcan_provider_adapters::{CredentialConnection, CredentialMode};

/// Verifies an invalid global account rebinds to the same model on its preferred account.
/// 验证失效的全局账号会重新绑定到首选账号上的同一模型。
#[test]
fn invalid_active_account_rebinds_same_model_before_model_fallback() {
    // providerId and modelId identify the user choice that must survive account discovery drift.
    // providerId 与 modelId 标识在账号发现变化后仍必须保留的用户选择。
    let provider_id = String::from("shared-provider");
    let model_id = String::from("shared-model");
    // staleConnection is the credential that no longer exposes the selected model.
    // staleConnection 是已不再公开所选模型的凭据。
    let stale_connection = CredentialConnection {
        id: String::from("stale-connection"),
        provider_id: provider_id.clone(),
        service_entry_id: String::from("shared-entry"),
        credential_profile_id: String::from("shared-profile"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("stale-credential"),
        display_name: String::from("Stale account"),
        secret_reference: String::from("stale-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    // availableConnection is the exact preferred credential that still exposes the model.
    // availableConnection 是仍公开该模型的精确首选凭据。
    let available_connection = CredentialConnection {
        id: String::from("available-connection"),
        provider_id: provider_id.clone(),
        service_entry_id: String::from("shared-entry"),
        credential_profile_id: String::from("shared-profile"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("available-credential"),
        display_name: String::from("Available account"),
        secret_reference: String::from("available-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    // configuration retains the stale exact active account and the valid model-level preference.
    // configuration 保留失效的精确活动账号与有效的模型级账号偏好。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .connections
        .insert(stale_connection.id.clone(), stale_connection.clone());
    configuration.connection_graph.connections.insert(
        available_connection.id.clone(),
        available_connection.clone(),
    );
    configuration
        .connection_graph
        .remember_model_connection(
            provider_id.as_str(),
            model_id.as_str(),
            available_connection.id.as_str(),
        )
        .expect("preferred model account should persist");
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: provider_id.clone(),
        model_id: model_id.clone(),
        connection_id: stale_connection.id,
    });
    configuration.set_model_enabled(provider_id.as_str(), model_id.as_str(), true);
    // capabilities prove the surviving model remains a valid conversation model.
    // capabilities 证明保留下来的模型仍是有效会话模型。
    let capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        ..ModelCapabilities::default()
    };
    // catalogs contains the selected model only under the available credential.
    // catalogs 只在可用凭据下包含所选模型。
    let catalogs = BTreeMap::from([(
        available_connection.id.clone(),
        ConnectionModelCatalog {
            connection_id: available_connection.id.clone(),
            models: vec![vulcan_provider_adapters::ConnectionModel {
                id: model_id.clone(),
                display_name: String::from("Shared model"),
                capabilities,
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            }],
            generation: 2,
            page_count: 1,
            discovery_schema_revision: String::from("account-rebind-test-v1"),
        },
    )]);
    // runtime supplies provider-level access defaults without remote execution.
    // runtime 提供供应商级访问默认值，不执行远程请求。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");

    ensure_active_model_selection(&runtime, &mut configuration, &catalogs)
        .expect("invalid account should rebind");

    assert_eq!(
        configuration.active,
        Some(ActiveAiModelSelection {
            provider_id,
            model_id,
            connection_id: available_connection.id,
        })
    );
}

/// Verifies a session picker action preserves the frozen route and records one pending selection.
/// 验证会话选择器操作会保留冻结路由，并记录一个待生效选择。
#[test]
fn session_model_selection_waits_for_next_provider_request_and_is_idempotent() {
    // sessionId identifies the sole workspace owner under test.
    // sessionId 标识本测试唯一的工作区所有者。
    let session_id = crate::SessionId::from_u128(0x50f0);
    // frozenLease models the route already owned by the current or most recently prepared turn.
    // frozenLease 模拟当前或最近已准备回合拥有的路由。
    let frozen_lease = vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
        schema_version: 1,
        provider_id: String::from("basis-provider"),
        connection_id: String::from("basis-account"),
        model_id: String::from("deepseek"),
        protocol_profile_id: String::from("openai-responses.v1"),
        manifest_revision: 1,
        manifest_generation: 1,
        catalog_revision: String::from("catalog-basis"),
        route_revision: String::from("route-basis"),
    };
    // runtimeState starts on the frozen provider/account/model tuple.
    // runtimeState 从冻结的供应商、账号、模型三元组开始。
    let runtime_state = vulcan_workbench_model::SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("basis-provider")),
        active_connection_id: Some(String::from("basis-account")),
        active_model_id: Some(String::from("deepseek")),
        provider_route_lease: Some(frozen_lease.clone()),
        ..vulcan_workbench_model::SessionRuntimeStateEntry::default()
    };
    // workspace provides both canonical and display transcript stores.
    // workspace 提供规范转录与显示转录两类存储。
    let mut workspace = vulcan_workspace_store::workspace_store::WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::from([(session_id, runtime_state)]),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: session_id,
        hydrated_session_ids: BTreeSet::new(),
    };
    // target is the exact Command Code account choice that must activate only at the next real Provider request.
    // target 是只能在下一次真实 Provider 请求边界激活的精确 Command Code 账号选择。
    let target = SessionPendingModelSelection {
        provider_id: String::from("command-code"),
        connection_id: String::from("command-account"),
        model_id: String::from("deepseek"),
    };

    // firstPlan records no durable legacy mutation; V1 metadata owns persistence.
    // firstPlan 不记录可靠旧链修改；持久化由 V1 metadata 持有。
    let first_plan =
        plan_session_model_selection_for_next_provider_request(&mut workspace, session_id, &target)
            .expect("session selection should be accepted for the next Provider request");

    assert!(first_plan.changed_session_ids.is_empty());
    assert!(first_plan.revoked_session_ids.is_empty());
    // retainedState proves current route authority stayed untouched while pending authority changed.
    // retainedState 证明待生效权威改变时，当前路由权威保持不变。
    let retained_state = workspace
        .session_runtime_state
        .get(&session_id)
        .expect("session runtime state should remain present");
    assert_eq!(
        retained_state.active_provider_id.as_deref(),
        Some("basis-provider")
    );
    assert_eq!(
        retained_state.active_connection_id.as_deref(),
        Some("basis-account")
    );
    assert_eq!(retained_state.active_model_id.as_deref(), Some("deepseek"));
    assert_eq!(
        retained_state.provider_route_lease.as_ref(),
        Some(&frozen_lease)
    );
    assert_eq!(
        retained_state.pending_model_selection.as_ref(),
        Some(&target)
    );
    assert!(retained_state.conversation_state.records.is_empty());

    // secondPlan proves selecting the same future tuple does not append a second audit row.
    // secondPlan 证明再次选择同一未来三元组不会追加第二条审计记录。
    let second_plan =
        plan_session_model_selection_for_next_provider_request(&mut workspace, session_id, &target)
            .expect("repeated session selection should be idempotent");
    assert!(second_plan.changed_session_ids.is_empty());
    assert_eq!(
        workspace
            .session_runtime_state
            .get(&session_id)
            .map(|state| state.conversation_state.records.len()),
        Some(0)
    );
}

/// Verifies catalog-only reconciliation never rewrites active or archived session selections.
/// 验证仅目录协调绝不会改写活动或归档会话选择。
#[test]
fn missing_session_models_remain_exact_until_explicit_reselection_or_revocation() {
    // provider_id identifies the deterministic provider shared by both fallback candidates.
    // provider_id 标识两个回退候选共同使用的确定性供应商。
    let provider_id = String::from("fallback-provider");
    // connection_id identifies the only available credential-scoped model catalog.
    // connection_id 标识唯一可用的凭据作用域模型目录。
    let connection_id = String::from("fallback-connection");
    // first_model_id sorts before the preferred model and therefore exercises first-model fallback.
    // first_model_id 排在首选模型之前，因此可验证首模型回退。
    let first_model_id = String::from("model-a-first");
    // preferred_model_id represents the currently selected global default.
    // preferred_model_id 表示当前选中的全局默认模型。
    let preferred_model_id = String::from("model-b-preferred");
    // connection supplies the exact provider and account identity required by availability checks.
    // connection 提供可用性检查所需的精确供应商与账号身份。
    let connection = CredentialConnection {
        id: connection_id.clone(),
        provider_id: provider_id.clone(),
        service_entry_id: String::from("fallback-entry"),
        credential_profile_id: String::from("fallback-profile"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("fallback-credential"),
        display_name: String::from("Fallback account"),
        secret_reference: String::from("fallback-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    // conversation_capabilities make both catalog models valid text-conversation candidates.
    // conversation_capabilities 使两个目录模型都成为有效文本会话候选。
    let conversation_capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        ..ModelCapabilities::default()
    };
    // catalog exposes two enabled models in deliberately non-preferred lexical order.
    // catalog 公开两个启用模型，并刻意设置与首选项不同的字典序。
    let catalog = ConnectionModelCatalog {
        connection_id: connection_id.clone(),
        models: vec![
            vulcan_provider_adapters::ConnectionModel {
                id: preferred_model_id.clone(),
                display_name: String::from("Preferred model"),
                capabilities: conversation_capabilities.clone(),
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            },
            vulcan_provider_adapters::ConnectionModel {
                id: first_model_id.clone(),
                display_name: String::from("First model"),
                capabilities: conversation_capabilities,
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                protocol_variants: BTreeMap::new(),
                resolved_protocol_binding: None,
            },
        ],
        generation: 1,
        page_count: 1,
        discovery_schema_revision: String::from("session-fallback-test-v1"),
    };
    // configuration owns one available connection and an explicit preferred global model.
    // configuration 持有一个可用连接与一个显式首选全局模型。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .connections
        .insert(connection_id.clone(), connection);
    configuration.active = Some(ActiveAiModelSelection {
        provider_id: provider_id.clone(),
        model_id: preferred_model_id.clone(),
        connection_id: connection_id.clone(),
    });
    // Explicit model access keeps this isolated reconciliation fixture independent of any
    // system Manifest identity.
    // 显式模型访问使该隔离协调夹具不依赖任何系统 Manifest 身份。
    configuration.set_model_enabled(provider_id.as_str(), preferred_model_id.as_str(), true);
    configuration.set_model_enabled(provider_id.as_str(), first_model_id.as_str(), true);
    // catalogs contains the authoritative process-local account model catalog.
    // catalogs 包含权威的进程内账号模型目录。
    let catalogs = BTreeMap::from([(connection_id.clone(), catalog)]);
    // active_session_id identifies the visible session with a missing model selection.
    // active_session_id 标识模型选择已失效的可见会话。
    let active_session_id = crate::SessionId::from_u128(0x5101);
    // archived_session_id identifies the archived session with the same invalid selection.
    // archived_session_id 标识具有相同失效选择的归档会话。
    let archived_session_id = crate::SessionId::from_u128(0x5102);
    // missing_selection represents a complete but unavailable session model identity.
    // missing_selection 表示完整但已不可用的会话模型身份。
    let missing_selection = vulcan_workbench_model::SessionRuntimeStateEntry {
        active_provider_id: Some(String::from("removed-provider")),
        active_connection_id: Some(String::from("removed-connection")),
        active_model_id: Some(String::from("removed-model")),
        ..vulcan_workbench_model::SessionRuntimeStateEntry::default()
    };
    // workspace contains both runtime-state stores exercised by provider/model removal.
    // workspace 包含供应商或模型移除时需要处理的两类运行态存储。
    let mut workspace = vulcan_workspace_store::workspace_store::WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_runtime_state: BTreeMap::from([(active_session_id, missing_selection.clone())]),
        archived_session_runtime_state: BTreeMap::from([(archived_session_id, missing_selection)]),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: active_session_id,
        hydrated_session_ids: BTreeSet::from([active_session_id, archived_session_id]),
    };

    // changedSessionIds remains empty because catalog observation has no authority to rewrite user choices.
    // changedSessionIds 保持为空，因为目录观测无权改写用户选择。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let changed_session_ids = reconcile_missing_session_model_selections(
        &mut workspace,
        &runtime,
        &configuration,
        &catalogs,
    )
    .expect("catalog-only reconciliation should remain observational");
    assert!(changed_session_ids.is_empty());
    for runtime_state in workspace
        .session_runtime_state
        .values()
        .chain(workspace.archived_session_runtime_state.values())
    {
        assert_eq!(
            runtime_state.active_provider_id.as_deref(),
            Some("removed-provider")
        );
        assert_eq!(
            runtime_state.active_connection_id.as_deref(),
            Some("removed-connection")
        );
        assert_eq!(
            runtime_state.active_model_id.as_deref(),
            Some("removed-model")
        );
    }

    // revokedStates install one explicitly disabled credential route in both stores.
    // revokedStates 在两类存储中安装同一条被显式禁用的凭据路由。
    for runtime_state in workspace
        .session_runtime_state
        .values_mut()
        .chain(workspace.archived_session_runtime_state.values_mut())
    {
        runtime_state.active_provider_id = Some(String::from("revoked-provider"));
        runtime_state.active_connection_id = Some(String::from("revoked-connection"));
        runtime_state.active_model_id = Some(String::from("revoked-model"));
    }
    // revokedSessionIds proves explicit access revocation covers active and archived owners
    // without silently replacing either user-owned route.
    // revokedSessionIds 证明显式访问撤销会覆盖活动与归档所有者，同时不会静默替换
    // 任一用户自有路由。
    let revoked_session_ids =
        reconcile_disabled_connection_sessions(&mut workspace, "revoked-connection", None);
    assert_eq!(
        revoked_session_ids,
        BTreeSet::from([active_session_id, archived_session_id])
    );
    for runtime_state in workspace
        .session_runtime_state
        .values()
        .chain(workspace.archived_session_runtime_state.values())
    {
        assert_eq!(
            runtime_state.active_connection_id.as_deref(),
            Some("revoked-connection")
        );
    }

    configuration.active = None;
    assert_eq!(
        preferred_fallback_model_selection(&runtime, &configuration, &catalogs),
        Ok(Some(ActiveAiModelSelection {
            provider_id,
            model_id: first_model_id,
            connection_id,
        }))
    );
}
