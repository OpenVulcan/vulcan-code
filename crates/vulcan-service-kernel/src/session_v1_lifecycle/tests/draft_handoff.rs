//! Actual prompt registration links a pending draft to its eventual private receipt.
//! 实际提示词注册把待完成草稿关联到最终私有回执。

use super::*;
use crate::{
    create_service_bridge_state, project_conversation_services::begin_project_lifecycle_transition,
    tests::test_support::test_service_shell_host,
};
use vulcan_sqlite_engine::sql_exec::SqliteValue;
use vulcan_workspace_store::{
    data_plane::prepare_application_schema_for_test,
    local_database_runtime::LogicalDatabase,
    local_sql::{execute_script_async, query_json_async},
    workspace_store::{delete_active_project_row_v1, ensure_session_v1_storage_schema},
};

mod dropped_composer_http;
mod recovery_http;

/// The exact project inserted into each isolated recovery database.
/// 写入每个隔离恢复数据库的精确项目。
const RECOVERY_PROJECT_ID: &str = "project-explicit";

/// The draft and original timestamp that must survive a recovery retry.
/// 恢复重试必须保留的草稿和原创建时间。
const RECOVERY_TRANSCRIPT: &str = "User:\nOriginal question\n\nAssistant:\nOriginal answer";
/// First publication time for the draft and its usage aggregate.
/// 草稿及其累计用量的首次发布时间。
const RECOVERY_CREATED_AT_MS: i64 = 1_787_788_800_000;

/// Initializes the application schema and adds the exact recovery project's owner row.
/// 初始化应用结构并写入恢复项目的精确归属记录。
/// Returns after the fixture controller can resolve both the project and session index.
/// 夹具控制器能够解析项目和会话索引后返回。
async fn prepare_recovery_project(fixture: &LifecycleStorageFixture) {
    prepare_application_schema_for_test(&fixture.controller)
        .await
        .unwrap();
    ensure_session_v1_storage_schema(&fixture.controller)
        .await
        .unwrap();
    // The workbench loader requires an absolute project path inside the fixture.
    // 工作台加载器要求使用夹具内的绝对项目路径。
    let root_path = fixture._temporary.path().to_string_lossy().into_owned();
    execute_script_async(
        &fixture.controller.local,
        LogicalDatabase::AppState,
        "INSERT INTO projects (id, name, root_path, created_at, updated_at)
         VALUES (?1, 'Recovery project', ?2, '1', '1');",
        vec![
            SqliteValue::Text(RECOVERY_PROJECT_ID.into()),
            SqliteValue::Text(root_path),
        ],
    )
    .await
    .unwrap();
}

/// Verifies that recovery publishes an unsent, text-only draft with the chosen first-request route.
/// 验证恢复操作发布带有指定首次请求路由且尚未发送的纯文本草稿。
#[tokio::test]
async fn reasoning_recovery_draft_survives_reload_before_first_submission() {
    // Use the real index and a second registry to exercise cold load.
    // 使用真实索引和第二个注册表验证冷加载。
    let fixture = LifecycleStorageFixture::start().await;
    prepare_recovery_project(&fixture).await;
    // Use the same project admission on the original request and its retry.
    // 首次请求和重试使用同一个项目准入。
    let state = create_service_bridge_state(test_service_shell_host());
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
        &state,
        &fixture.controller,
        &registry,
        requested_session_id,
        std::future::ready(Ok(RecoveryDraftSessionV1 {
            project_id: RECOVERY_PROJECT_ID.into(),
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
        &state,
        &fixture.controller,
        &reload_registry,
        requested_session_id,
        std::future::ready(Err(String::from("source unavailable on retry"))),
    )
    .await
    .unwrap();
    assert_eq!(retry, session_id);
    assert_eq!(
        list_active_sessions_v1(&fixture.controller, RECOVERY_PROJECT_ID)
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

/// Leaves one published draft without a locator by failing the real SQLite insert.
/// 通过拒绝实际 SQLite 插入保留一份已发布但缺少定位记录的草稿。
/// Returns the original session id and model selection after removing the injected failure.
/// 撤销注入失败后返回原会话 ID 和模型选择。
async fn publish_unindexed_recovery(
    fixture: &LifecycleStorageFixture,
    state: &SharedServiceBridgeState,
) -> (SessionId, vulcan_provider_adapters::ActiveAiModelSelection) {
    let controller = &fixture.controller;
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
    let selection = vulcan_provider_adapters::ActiveAiModelSelection {
        provider_id: "deepseek".into(),
        model_id: "deepseek-flash".into(),
        connection_id: "first-connection".into(),
    };
    let first_registry = SessionRuntimeRegistry::default();
    let error = create_or_get_recovery_draft_session_v1(
        state,
        controller,
        &first_registry,
        session_id,
        std::future::ready(Ok(RecoveryDraftSessionV1 {
            project_id: RECOVERY_PROJECT_ID.into(),
            title: "First recovery".into(),
            text: RECOVERY_TRANSCRIPT.into(),
            selection: selection.clone(),
            now_ms: RECOVERY_CREATED_AT_MS,
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
    (session_id, selection)
}

/// Verifies a cold retry restores the original draft and can persist new usage.
/// 验证冷重试恢复原草稿且能持久化新增用量。
#[tokio::test]
async fn reasoning_recovery_retries_sqlite_index_without_rebuilding_the_draft() {
    use vulcan_session_core::v1::lifetime_usage::SessionUsageBucket;

    let fixture = LifecycleStorageFixture::start().await;
    prepare_recovery_project(&fixture).await;
    // Use real Core admission while recreating the session registry for a cold retry.
    // 冷重试重建会话注册表，同时使用真实的 Core 准入。
    let state = create_service_bridge_state(test_service_shell_host());
    let controller = &fixture.controller;
    let (session_id, selection) = publish_unindexed_recovery(&fixture, &state).await;
    // Model a draft left by a build that published its session tree before writing usage.
    // 模拟旧构建先发布会话目录树、尚未写入用量时留下的草稿。
    let project_directory = vulcan_workspace_store::workspace_store::project_resource_directory_v1(
        &controller.data_root,
        SESSION_V1_STORAGE_PARTITION,
        RECOVERY_PROJECT_ID,
    )
    .unwrap();
    let bucket = session_v1_date_bucket(RECOVERY_CREATED_AT_MS).unwrap();
    std::fs::remove_file(
        project_directory
            .join(format!("{bucket}-{session_id}"))
            .join("usage.json"),
    )
    .unwrap();
    let reload_registry = SessionRuntimeRegistry::default();
    // The source is deliberately unavailable on retry to prove the first tree is authoritative.
    // 重试时特意让来源不可用，以证明首次目录树是权威。
    let retry = create_or_get_recovery_draft_session_v1(
        &state,
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
        list_active_sessions_v1(controller, RECOVERY_PROJECT_ID)
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
            assert_eq!(document.metadata.created_at_ms, RECOVERY_CREATED_AT_MS);
            assert_eq!(
                document.metadata.input_state.draft.text,
                RECOVERY_TRANSCRIPT
            );
            assert!(document.events().is_empty());
        })
        .unwrap();
    let route = crate::session_request_config_v1::capture_next_request_config_v1(&session).unwrap();
    assert_eq!(
        route.connection_id.as_deref(),
        Some(selection.connection_id.as_str())
    );
    // Record an actual request increment through the same owner used by provider terminal settlement.
    // 通过供应商终态结算使用的同一所有者记录一次请求增量。
    let usage = session.lifetime_usage_snapshot().unwrap();
    assert_eq!(usage.updated_at_ms, RECOVERY_CREATED_AT_MS);
    assert_eq!(usage.main.request_count, 0);
    session.record_lifetime_usage(
        &SessionUsageBucket {
            request_count: 1,
            ..SessionUsageBucket::default()
        },
        RECOVERY_CREATED_AT_MS + 1,
    );
    // Hydrate a fresh registry to prove that the count was persisted, not merely cached.
    // 水合新的注册表，证明计数已经持久化。
    let later_registry = SessionRuntimeRegistry::default();
    let later = get_or_load_session_v1(controller, &later_registry, retry)
        .await
        .unwrap();
    assert_eq!(
        later.lifetime_usage_snapshot().unwrap().main.request_count,
        1
    );
    fixture.shutdown().await;
}

/// Verifies recovery cannot publish between the final archive check and project deletion, or after deletion.
/// 验证恢复无法在最后一次归档检查和项目删除之间或删除后发布。
#[tokio::test]
async fn reasoning_recovery_rejects_index_retry_during_and_after_project_removal() {
    let fixture = LifecycleStorageFixture::start().await;
    prepare_recovery_project(&fixture).await;
    // Share Core admission between the simulated removal and recovery requests.
    // 模拟移除和恢复请求共用 Core 准入。
    let state = create_service_bridge_state(test_service_shell_host());
    let controller = &fixture.controller;
    let (session_id, _) = publish_unindexed_recovery(&fixture, &state).await;
    let registry = SessionRuntimeRegistry::default();
    // Pause the production removal sequence after archiving, while its reservation is still held.
    // 在生产移除流程完成归档后暂停，此时仍持有项目预留。
    let removal = begin_project_lifecycle_transition(&state, RECOVERY_PROJECT_ID, [], "remove")
        .expect("reserve the project for removal");
    assert!(
        archive_project_sessions_lifecycle_v1(
            controller,
            &registry,
            RECOVERY_PROJECT_ID,
            RECOVERY_CREATED_AT_MS + 1,
        )
        .await
        .expect("complete the final archive check")
        .is_empty()
    );
    // While the project row still exists, only project admission can reject the retry.
    // 此时项目行仍然存在，只有项目准入能够拒绝补写。
    let conflict = create_or_get_recovery_draft_session_v1(
        &state,
        controller,
        &registry,
        session_id,
        std::future::ready(Err("The published draft must not be rebuilt.".into())),
    )
    .await
    .expect_err("reject recovery after the archive check and before project deletion");
    assert!(
        conflict.contains(
            crate::state::ProjectAdmissionReservationKind::LifecycleTransition.diagnostic_label()
        ),
        "{conflict}"
    );
    delete_active_project_row_v1(controller, RECOVERY_PROJECT_ID)
        .await
        .unwrap();
    drop(removal);
    let error = create_or_get_recovery_draft_session_v1(
        &state,
        controller,
        &registry,
        session_id,
        std::future::ready(Err("The published draft must not be rebuilt.".into())),
    )
    .await
    .unwrap_err();
    // Assert the typed store error as rendered at the lifecycle boundary.
    // 断言类型化存储错误在生命周期边界的渲染结果。
    assert_eq!(
        error,
        vulcan_workspace_store::workspace_store::SessionIndexV1Error::RecoveryProjectMissing {
            session_id: session_id.to_string(),
            project_id: RECOVERY_PROJECT_ID.into(),
        }
        .to_string(),
    );
    assert!(
        locate_optional_session_v1_storage(controller, &session_id.to_string())
            .await
            .unwrap()
            .is_none()
    );
    let list_state = query_json_async(
        &controller.local,
        LogicalDatabase::AppState,
        "SELECT session_id FROM session_list_state_v1;",
        Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(list_state.row_count, 0);
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
