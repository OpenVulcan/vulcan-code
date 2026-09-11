//! Actual prompt registration links a pending draft to its eventual private receipt.
//! 实际提示词注册把待完成草稿关联到最终私有回执。

use super::*;
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
        connection_id: Some("connection".into()),
        media_id: "file-exact-original".into(),
        upload_receipt: Some(ProjectResourceUploadReceipt {
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
        }),
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
