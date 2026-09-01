//! Actual HTTPS upload completion retained by the original prompt when its prepared request becomes stale.
//! 准备请求变陈旧时，由原提示词保留的实际HTTPS上传完成。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
};
use vulcan_session_core::v1::{
    ClientActionId, ClientInputSubmission, LockedBaselineProjection,
    config::PendingRequestConfig,
    event::{ApiPurposeKind, ApiRequestSnapshot, EventBody},
};
use vulcan_workbench_model::SessionId;
use vulcan_workspace_store::workspace_store::{
    create_session_v1_storage_fixture, load_session_v1_full,
};

/// Completes a real verified HTTPS upload, changes the selected model before prompt persistence, then proves B-address and A-original after disk reload.
/// 完成真实已验证HTTPS上传，在提示词持久化前切换选中模型，随后证明磁盘重载后的B地址及A原引用。
#[tokio::test]
async fn v1_https_completed_upload_survives_stale_request_and_restores_original()
-> Result<(), Box<dyn std::error::Error>> {
    // The fixture owns its temporary data root and never opens a real user session.
    // 夹具持有临时数据根目录，绝不打开真实用户会话。
    let directory = tempfile::tempdir()?;
    // Original session identity is shared by prompt persistence and provider receipt scope.
    // 原会话身份由提示词持久化及供应商回执作用域共享。
    let session_id = SessionId::new_v4();
    // Use the repository's actual session layout rather than assuming the catalog is at the temporary root.
    // 使用仓库真实会话布局，而非假设目录位于临时根目录。
    let locator = create_session_v1_storage_fixture(directory.path(), &session_id.to_string(), 1)?;
    // The storage locator is authoritative for the project resource partition.
    // 存储定位器是项目资源分区权威。
    let project = locator.project_directory();
    // The original V1 owner is the same object used by capture, completion and stale-request rejection.
    // 原V1所有者是捕获、完成及陈旧请求拒绝使用的同一对象。
    let handle = SessionHandle::new(
        session_id,
        locator.clone(),
        load_session_v1_full(&locator)?.document,
    );
    // Source registration and receipt publication use the existing shared catalog owner.
    // 来源注册及回执发布使用既有共享目录所有者。
    let registry = Arc::new(ProjectResourceCatalogRegistry::default());
    // Register real bytes in the exact project partition before admitting their prompt occurrence.
    // 在准入其提示词出现之前，将真实字节注册到精确项目分区。
    let entry = register(&project, &registry, b"abc")?;
    handle.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("https-stale-upload")?,
            "inspect this image",
            vec![entry.resource_id],
        ),
        2,
    )?;
    // B independently supports uploading, which must not grant it A's earlier occurrence.
    // B独立支持上传，但这不能向其授予A更早出现。
    let (mut second, second_secrets) = context(true, true).await?;
    second.session_scope_id = session_id.to_string();
    // The exact next model identity comes from its real frozen execution fixture.
    // 精确下一模型身份来自其真实冻结执行夹具。
    let second_route = original_route(&second);
    // A single TLS-protected upload produces a real reference; no inference request is made.
    // 单次TLS保护上传产生真实引用；不执行推理请求。
    let (http, server) =
        https_upload_fixture(200, r#"{"id":"file-durable-original-A"}"#, None).await?;
    // The actual terminal observer supplies a deterministic point after upload success but before the caller publishes prompt proof.
    // 实际终态观察器提供上传成功后、调用方发布提示词证明前的确定性时间点。
    let changed = Arc::new(AtomicBool::new(false));
    // The callback performs the real pending-model mutation on the same session owner.
    // 回调在同一会话所有者上执行真实待生效模型修改。
    let changing_handle = Arc::clone(&handle);
    // Keep the once-only marker alive outside the observer for the final assertion.
    // 在观察器之外保留仅一次标记，以供最终断言。
    let changed_in_observer = Arc::clone(&changed);
    // Install the observer before freezing A's actual execution, preserving ordinary runtime progress ownership.
    // 冻结A实际执行之前安装观察器，保留普通运行时进度所有权。
    let (mut first, first_secrets) = context_with_http(
        false,
        true,
        Some(Arc::new(move |event| {
            if event.phase == RuntimeMediaUploadPhase::Completed {
                assert!(!changed_in_observer.swap(true, Ordering::SeqCst));
                changing_handle
                    .mutate_durable_if_changed(
                        SessionMutationAdmission::MetadataOnly,
                        SessionFlushReason::MetadataChanged,
                        |document| {
                            Ok(
                                if document.set_pending_request(
                                    PendingRequestConfig {
                                        provider: Some(second_route.provider_id.clone()),
                                        connection_id: Some(second_route.connection_id.clone()),
                                        model: Some(second_route.model_id.clone()),
                                        thinking_level: None,
                                        temperature: None,
                                        set_at_ms: 3,
                                    },
                                    3,
                                )? {
                                    SessionMutationChange::Changed(())
                                } else {
                                    SessionMutationChange::Unchanged(())
                                },
                            )
                        },
                    )
                    .expect("model change while upload result is pending");
            }
        })),
        Some(http),
    )
    .await?;
    first.session_scope_id = session_id.to_string();
    // The physical request identity is frozen from A, before the callback switches the session to B.
    // 物理请求身份在回调将会话切到B之前从A冻结。
    let route = original_route(&first);
    // No guessed connection or protocol fields enter the API gate.
    // API门不接收猜测的连接或协议字段。
    let request = ApiRequestSnapshot {
        provider: route.provider_id.clone(),
        profile: Some(route.protocol_profile_id.clone()),
        model: route.model_id.clone(),
        connection_id: Some(route.connection_id.clone()),
        thinking_level: None,
        temperature: None,
    };
    // The builder uses the same frozen execution and catalog used by the production sink.
    // 构建者使用与生产接收器相同的冻结执行及目录。
    let build_context = first.clone();
    // The outer test retains the registry to inspect the exact persisted receipt afterwards.
    // 外层测试保留注册表，以在之后检查精确持久回执。
    let build_registry = Arc::clone(&registry);
    // Drive the actual gate and durable media producer, not a preconstructed partial-state fixture.
    // 驱动实际门及可靠媒体生产者，而非预先构造的部分状态夹具。
    let result = handle
        .prepare_physical_api_durable::<(), _>(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            4,
            move |snapshot, preparation| async move {
                // Even though A cannot dispatch, its valid result is materialized and durably associated with this exact prompt.
                // 即使A不能分派，其有效结果仍被物化并可靠关联到本精确提示词。
                let media = prepare_v1_user_media_durable(
                    &snapshot,
                    Some(&build_context),
                    build_registry,
                    Arc::clone(&preparation),
                );
                assert!(
                    std::mem::size_of_val(&media) < 4096,
                    "durable media wrapper must not inline the large upload future"
                );
                // Poll the same production future after checking its enclosing frame, without a larger thread-stack override.
                // 检查外围帧之后轮询同一生产Future，不提高线程栈配置。
                let prepared = media.await?;
                assert_eq!(native(&prepared, 0).id, "file-durable-original-A");
                tokio::task::spawn_blocking(move || {
                    assert!(
                        preparation.recapture()?.is_none(),
                        "model change must still invalidate A's transport"
                    );
                    Ok(None)
                })
                .await
                .map_err(|error| SessionRuntimeError::BlockingTaskJoin(error.to_string()))?
            },
        )
        .await;
    assert_eq!(server.await?["content_base64"], "YWJj");
    assert!(result?.is_none());
    assert!(changed.load(Ordering::SeqCst));
    // A real disk reload must contain original A proof but no phantom first API or claimed prompt-wide route.
    // 真实磁盘重载必须包含A原证明，但没有虚假首次API或声称的整条提示词路线。
    let loaded = load_session_v1_full(&locator)?;
    assert!(
        !loaded
            .document
            .events()
            .iter()
            .any(|event| matches!(event.event, EventBody::ApiStarted(_)))
    );
    // The exact root event still owns the completion and immutable empty admission proof.
    // 精确根事件仍持有完成及不可变空准入证明。
    let EventBody::UserInput(input) = &loaded.document.events()[0].event else {
        panic!("root")
    };
    // Extract only the one known completed occurrence, never search by latest resource usage.
    // 仅提取唯一已知完成出现，绝不按最近资源使用搜索。
    let parameters = input.parameters.as_ref().expect("prompt media");
    assert!(parameters.prepared_resources.is_empty());
    // The producer, not the test, populated the original per-occurrence completion state.
    // 由生产者而非测试填充原逐出现完成状态。
    let UserPromptMediaDelivery::Preparing { resources } = &parameters.media_delivery else {
        panic!("completion must remain before first API")
    };
    assert_eq!(
        resources[0]
            .as_ref()
            .expect("completed upload")
            .original_route,
        route
    );
    // All following reads use the actual persisted snapshot and private receipt from the completed network transfer.
    // 后续读取全部使用已完成网络传输的实际持久快照及私有回执。
    let replay = handle.capture_context_compaction_planning_snapshot()?;
    // Capture credential counts after the one genuine upload; restoration must not perform another one.
    // 在唯一真实上传之后捕获凭据计数；恢复不能再次上传。
    let first_reads = first_secrets.reads.load(Ordering::SeqCst);
    // B may not resolve its upload credentials for A's older occurrence.
    // B不能为A较早出现解析其上传凭据。
    let second_reads = second_secrets.reads.load(Ordering::SeqCst);
    assert!(
        metadata(
            &prepare_v1_user_media(&replay, Some(&second), Arc::clone(&registry)).await?,
            0
        )
        .contains("another model")
    );
    assert_eq!(
        native(
            &prepare_v1_user_media(&replay, Some(&first), registry).await?,
            0
        )
        .id,
        "file-durable-original-A"
    );
    assert_eq!(first_secrets.reads.load(Ordering::SeqCst), first_reads);
    assert_eq!(second_secrets.reads.load(Ordering::SeqCst), second_reads);
    assert_eq!(
        load_project_resource_catalog(&project)?.entries[0]
            .provider_bindings
            .len(),
        1
    );
    Ok(())
}
