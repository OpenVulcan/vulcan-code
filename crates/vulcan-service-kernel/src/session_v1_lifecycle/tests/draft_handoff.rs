//! Actual prompt registration links a pending draft to its eventual private receipt.
//! 实际提示词注册把待完成草稿关联到最终私有回执。

use super::*;

/// Verifies that recovery publishes an unsent, text-only draft with the chosen first-request route.
/// 验证恢复操作发布带有指定首次请求路由且尚未发送的纯文本草稿。
#[tokio::test]
async fn reasoning_recovery_draft_survives_reload_before_first_submission() {
    // Use the real index and a second registry to exercise cold load.
    // 使用真实索引和第二个注册表验证冷加载。
    let fixture = LifecycleStorageFixture::start().await;
    let registry = SessionRuntimeRegistry::default();
    let selection = vulcan_provider_adapters::ActiveAiModelSelection {
        provider_id: "deepseek".into(),
        model_id: "deepseek-flash".into(),
        connection_id: "deepseek-test".into(),
    };
    let transcript = "User:\nOriginal question\n\nAssistant:\nVisible answer";
    let now_ms = 1_787_788_800_000;
    let requested_session_id = SessionId::new_v4();
    let session_id = create_or_get_recovery_draft_session_v1(
        &fixture.controller,
        &registry,
        requested_session_id,
        std::future::ready(Ok(RecoveryDraftSessionV1 {
            project_id: "project-explicit".into(),
            title: "Recovered conversation".into(),
            text: transcript.into(),
            selection: selection.clone(),
            now_ms,
        })),
    )
    .await
    .unwrap();
    assert_eq!(session_id, requested_session_id);
    let reload_registry = SessionRuntimeRegistry::default();
    // Model a lost reply by repeating creation with a fresh registry and unavailable source input.
    // 用新注册表和不可用的来源输入重复创建，模拟响应丢失。
    let retry = create_or_get_recovery_draft_session_v1(
        &fixture.controller,
        &reload_registry,
        requested_session_id,
        std::future::ready(Err(String::from("source unavailable on retry"))),
    )
    .await
    .unwrap();
    assert_eq!(retry, session_id);
    assert_eq!(
        list_active_sessions_v1(&fixture.controller, "project-explicit")
            .await
            .unwrap()
            .len(),
        1
    );
    let reloaded = get_or_load_session_v1(&fixture.controller, &reload_registry, session_id)
        .await
        .unwrap();
    reloaded
        .read(|document| {
            assert!(document.events().is_empty());
            assert!(document.active_turn().is_none());
            assert_eq!(document.metadata.input_state.draft.text, transcript);
            assert!(document.metadata.input_state.draft.resource_ids.is_empty());
        })
        .unwrap();
    let next = crate::session_request_config_v1::capture_next_request_config_v1(&reloaded).unwrap();
    assert_eq!(
        next.provider.as_deref(),
        Some(selection.provider_id.as_str())
    );
    assert_eq!(
        next.connection_id.as_deref(),
        Some(selection.connection_id.as_str())
    );
    assert_eq!(next.model.as_deref(), Some(selection.model_id.as_str()));
    reloaded
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("send-recovered-text").unwrap(),
                transcript,
                Vec::new(),
            )
            .with_bound_request_config(next),
            now_ms + 1,
        )
        .unwrap();
    reloaded
        .read(|document| {
            assert!(document.events().iter().any(|event| matches!(
                &event.event,
                EventBody::UserInput(input) if input.text == transcript
            )));
            assert!(document.active_api().is_none());
        })
        .unwrap();
    fixture.shutdown().await;
}

/// Verifies that an index retry across a cold load publishes the exact first recovery draft.
/// 验证冷加载后的索引重试发布精确的首次恢复草稿。
#[tokio::test]
async fn reasoning_recovery_retries_sqlite_index_without_rebuilding_the_draft() {
    use vulcan_workspace_store::{
        local_database_runtime::LogicalDatabase, local_sql::execute_script_async,
        workspace_store::ensure_session_v1_storage_schema,
    };

    let fixture = LifecycleStorageFixture::start().await;
    let controller = &fixture.controller;
    ensure_session_v1_storage_schema(controller).await.unwrap();
    // Fail the actual SQLite insert, after the store's atomic directory publication.
    // 让实际 SQLite 插入在存储层原子发布目录后失败。
    execute_script_async(
        &controller.local,
        LogicalDatabase::AppState,
        "CREATE TRIGGER fail_recovery_index BEFORE INSERT ON session_index_v1
         BEGIN SELECT RAISE(ABORT, 'injected recovery index failure'); END;",
        Vec::new(),
    )
    .await
    .unwrap();
    let session_id = SessionId::new_v4();
    let project_id = "recovery-index-project";
    let transcript = "User:\nOriginal question\n\nAssistant:\nOriginal answer";
    let created_at_ms = 1_787_788_800_000;
    let selection = vulcan_provider_adapters::ActiveAiModelSelection {
        provider_id: "deepseek".into(),
        model_id: "deepseek-flash".into(),
        connection_id: "first-connection".into(),
    };
    let first_registry = SessionRuntimeRegistry::default();
    let error = create_or_get_recovery_draft_session_v1(
        controller,
        &first_registry,
        session_id,
        std::future::ready(Ok(RecoveryDraftSessionV1 {
            project_id: project_id.into(),
            title: "First recovery".into(),
            text: transcript.into(),
            selection: selection.clone(),
            now_ms: created_at_ms,
        })),
    )
    .await
    .unwrap_err();
    assert!(error.contains("injected recovery index failure"), "{error}");
    assert!(
        locate_optional_session_v1_storage(controller, &session_id.to_string())
            .await
            .unwrap()
            .is_none()
    );
    execute_script_async(
        &controller.local,
        LogicalDatabase::AppState,
        "DROP TRIGGER fail_recovery_index;",
        Vec::new(),
    )
    .await
    .unwrap();
    drop(first_registry);
    let reload_registry = SessionRuntimeRegistry::default();
    // The source is deliberately unavailable on retry to prove the first tree is authoritative.
    // 重试时特意让来源不可用，以证明首次目录树是权威。
    let retry = create_or_get_recovery_draft_session_v1(
        controller,
        &reload_registry,
        session_id,
        std::future::ready(Err(
            "Source unavailable after the first file publication.".into()
        )),
    )
    .await
    .unwrap();
    assert_eq!(retry, session_id);
    assert_eq!(
        list_active_sessions_v1(controller, project_id)
            .await
            .unwrap()
            .len(),
        1
    );
    let session = get_or_load_session_v1(controller, &reload_registry, retry)
        .await
        .unwrap();
    session
        .read(|document| {
            assert_eq!(document.metadata.created_at_ms, created_at_ms);
            assert_eq!(document.metadata.input_state.draft.text, transcript);
            assert!(document.events().is_empty());
        })
        .unwrap();
    let route = crate::session_request_config_v1::capture_next_request_config_v1(&session).unwrap();
    assert_eq!(
        route.connection_id.as_deref(),
        Some(selection.connection_id.as_str())
    );
    fixture.shutdown().await;
}
use vulcan_agent_runtime::{
    attachment_asset_index::{
        AttachmentDraftMediaDelivery, AttachmentDraftMediaOrigin, acquire_staged_attachment_ref,
        complete_attachment_draft_upload, promote_staged_attachment_to_message_ref,
        record_attachment_draft_media,
    },
    attachment_assets::{generic_attachment_descriptor, stage_attachment_bytes},
};
use vulcan_session_core::v1::UserPromptMediaRoute;
use vulcan_workspace_store::workspace_store::{ProjectResourceUploadReceipt, ProviderBinding};

/// Registers the actual prompt-selected project, releases the lease, completes the original upload, and reloads unchanged prompt proof.
/// 登记实际提示词选中的项目、释放租约、完成原上传，并重载未改写的提示词证明。
#[tokio::test]
async fn project_prompt_pending_draft_handoff_uses_actual_project_owner() {
    // The existing real controller fixture owns session creation, project storage and final shutdown.
    // 既有真实控制器夹具持有会话创建、项目存储及最终关闭。
    let fixture = LifecycleStorageFixture::start().await;
    let registry = SessionRuntimeRegistry::default();
    let resources = ProjectResourceCatalogRegistry::default();
    let workspace = tempfile::tempdir().expect("workspace");
    let scope = "composer-draft:v1:new:actual-owner";
    let attachment = stage_attachment_bytes(
        &workspace.path().to_string_lossy(),
        &generic_attachment_descriptor(),
        "source.bin",
        None,
        b"abc",
    )
    .expect("managed asset");
    acquire_staged_attachment_ref(
        &workspace.path().to_string_lossy(),
        scope,
        std::slice::from_ref(&attachment),
    )
    .expect("live lease");
    let original = record_attachment_draft_media(
        &attachment,
        scope,
        AttachmentDraftMediaOrigin::new(
            &attachment,
            UserPromptMediaRoute {
                provider_id: "provider".into(),
                service_entry_id: "entry".into(),
                connection_id: "connection".into(),
                model_id: "model-A".into(),
                protocol_profile_id: "openai-responses.v1".into(),
            },
            AttachmentDraftMediaDelivery::Upload {
                upload_channel_id: "files".into(),
                upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
                receipt: None,
            },
        )
        .expect("upload intent"),
    )
    .expect("persist intent");
    // The production registration entrypoint, not the test, records the handoff destination.
    // 由生产注册入口而非测试登记交接目标。
    let (ids, parameters) = register_project_prompt_attachments_v1(
        &fixture.controller,
        &resources,
        "actual-prompt-owner",
        std::slice::from_ref(&attachment),
        &[PromptAttachmentOrigin::Staged(Some(Box::new(
            original.clone(),
        )))],
        None,
    )
    .expect("register pending prompt");
    let parameters = parameters.expect("prompt-owned parameters");
    let session_id = SessionId::new_v4();
    create_and_register_root_session_v1(
        &fixture.controller,
        &registry,
        CreateRootSessionV1Request {
            dedicated: None,
            session_id,
            project_id: "actual-prompt-owner".into(),
            title: "Late upload".into(),
            root_submission: ClientInputSubmission::new(
                ClientActionId::try_new("pending-draft").expect("action"),
                "original prompt",
                ids.clone(),
            )
            .with_parameters(parameters.clone())
            .expect("parameters"),
            now_ms: 1_787_788_800_000,
        },
    )
    .await
    .expect("durable root");
    promote_staged_attachment_to_message_ref(
        &workspace.path().to_string_lossy(),
        &session_id.to_string(),
        "pending-draft",
        std::slice::from_ref(&attachment),
        &[scope.into()],
    )
    .expect("release submitted draft");
    let binding = ProviderBinding {
        provider: "provider".into(),
        connection_id: "connection".into(),
        media_id: "file-exact-original".into(),
        upload_receipt: ProjectResourceUploadReceipt {
            draft_intent_id: None,
            version: 1,
            service_entry_id: "entry".into(),
            upload_channel_id: "files".into(),
            upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
            resource_kind: vulcan_ai_contract::ResourceKind::File,
            mime_type: attachment.mime_type.clone(),
            source_kind: vulcan_ai_contract::ResourceSourceKind::ProviderFileId,
            session_scope_id: None,
            created_at_unix_millis: 1,
            expires_at_unix_millis: u64::MAX,
        },
    };
    complete_attachment_draft_upload(&attachment, scope, &original, binding, &resources)
        .expect("late original completion");
    // A separate session registry forces real disk reload; the prompt's original pending linkage remains unchanged.
    // 独立会话注册表强制真实磁盘重载；提示词的原待回执关联保持不变。
    let reload_registry = SessionRuntimeRegistry::default();
    let reloaded = get_or_load_session_v1(&fixture.controller, &reload_registry, session_id)
        .await
        .expect("reload original input");
    assert!(
        matches!(reloaded.read(|document| document.events()[0].event.clone()).expect("input"), EventBody::UserInput(input) if input.resource_ids == ids && input.parameters == Some(parameters))
    );
    let project = project_resource_directory_v1(
        &fixture.controller.data_root,
        SESSION_V1_STORAGE_PARTITION,
        "actual-prompt-owner",
    )
    .expect("exact project");
    let catalog = load_project_resource_catalog(&project).expect("durable receipt");
    let entry = &catalog.entries[0];
    assert_eq!(entry.provider_bindings.len(), 1);
    assert!(
        entry.provider_bindings[0].matches_prompt_upload(
            &original
                .prompt_delivery(ids[0])
                .expect("original pending proof")
                .delivery,
            &entry.sha256,
            entry.size
        )
    );
    assert_eq!(entry.provider_bindings[0].media_id, "file-exact-original");
    fixture.shutdown().await;
}
