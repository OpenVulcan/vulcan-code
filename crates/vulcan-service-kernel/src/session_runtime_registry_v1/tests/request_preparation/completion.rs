//! Durable partial-completion ownership during real request preparation and cancellation.
//! 真实请求准备和取消期间的可靠部分完成所有权。

use super::*;
use std::future::Future;
use vulcan_session_core::v1::{
    UserPromptMediaBinding, UserPromptMediaDelivery, UserPromptMediaOrigin, UserPromptMediaRoute,
    UserPromptResourceBinding, UserPromptResourceDelivery, UserPromptResourceDeliveryKind,
};

/// Creates one real session and a validated opaque upload result matching its original `request`, without contacting a provider.
/// 创建一个真实会话及匹配原`request`的已验证不透明上传结果，不联系供应商。
/// Returns the directory, sole handle, request and result for its single resource occurrence.
/// 返回目录、唯一句柄、请求及其单次资源出现结果。
fn completion_fixture() -> (
    tempfile::TempDir,
    Arc<SessionHandle>,
    ApiRequestSnapshot,
    UserPromptResourceDelivery,
) {
    // Exact resource identity remains attached to the original chronological user input.
    // 精确资源身份保持附着于原时序用户输入。
    let resource_id = ResourceId::try_new(7).expect("resource");
    // Use the same production-backed fixture as the original two-phase preparation tests.
    // 使用原两阶段准备测试相同的生产存储夹具。
    let (directory, handle, mut request) = preparation_fixture_with_resources(vec![resource_id]);
    request.profile = Some("openai-responses.v1".into());
    // The result has a real declared route shape and an opaque receipt identity, not a URL.
    // 结果具有真实声明路线结构及不透明回执身份，而非URL。
    let original = UserPromptResourceDelivery {
        resource_id,
        original_route: UserPromptMediaRoute {
            provider_id: request.provider.clone(),
            service_entry_id: "test-service".into(),
            connection_id: request.connection_id.clone().expect("connection"),
            model_id: request.model.clone(),
            protocol_profile_id: request.profile.clone().expect("profile"),
        },
        delivery: UserPromptResourceDeliveryKind::Uploaded {
            upload_channel_id: "files".into(),
            upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
            reference_fingerprint: "b".repeat(64),
        },
    };
    (directory, handle, request, original)
}

/// Reads the exact disk-backed original result in `handle`, returning its prompt parameters for durable-state assertions.
/// 读取`handle`中精确磁盘原结果，返回提示词参数以断言可靠状态。
fn durable_parameters(handle: &SessionHandle) -> vulcan_session_core::v1::UserPromptParameters {
    // A fresh physical load proves that a memory-only update cannot satisfy the test.
    // 全新物理加载证明仅内存更新不能满足测试。
    let loaded = load_session_v1_full(&handle.storage_locator().expect("locator")).expect("reload");
    // The original root event is fixed by this fixture's chronological construction.
    // 原根事件由本夹具时序构造固定。
    let EventBody::UserInput(input) = &loaded.document.events()[0].event else {
        panic!("root prompt")
    };
    input.parameters.clone().expect("media parameters")
}

/// Permits an API after its own completion write only by rebuilding from a real durable recapture.
/// 仅通过真实可靠重捕获重建，才在自身完成写入后许可API。
#[tokio::test]
async fn physical_preparation_own_completion_recaptures_and_commits() {
    // The real persistence owner starts with a pending prompt and no draft proof.
    // 真实持久化所有者起始具有待准备提示词且没有草稿证明。
    let (_directory, handle, request, original) = completion_fixture();
    // Assert exact original facts after the final API transaction.
    // 最终API事务后断言精确原事实。
    let expected = original.clone();
    // Track the actual durable revision rather than guessing the number of writes.
    // 跟踪实际可靠修订，而非猜测写入次数。
    let before = handle
        .read(|document| document.durable_revision())
        .expect("revision");
    // The builder uses the same scoped authority supplied to the production media path.
    // 构建者使用与生产媒体路径相同的作用域权限。
    let result = handle.prepare_physical_api_durable(ApiPurposeKind::MainGeneration, request, LockedBaselineProjection::empty(), None, 4, move |snapshot, preparation| async move {
        tokio::task::spawn_blocking(move || {
            // Record completion at the original occurrence before asking for a new snapshot.
            // 在请求新快照之前，于原出现位置记录完成。
            let seq = snapshot.preparing_user_prompt_media_seqs[0];
            assert!(preparation.record_resource(UserPromptResourceBinding { prompt_seq: seq, resource_position: 0, resource: original.clone() }, 3)?);
            // Repeated worker completion is idempotent, including the durable revision.
            // 重复工作线程完成幂等，包括可靠修订。
            assert!(preparation.record_resource(UserPromptResourceBinding { prompt_seq: seq, resource_position: 0, resource: original.clone() }, 3)?);
            // This recapture is read from the real persisted event range, not a changed revision on the old snapshot.
            // 本重捕获来自真实持久事件区间，不是在旧快照上修改修订。
            let refreshed = preparation.recapture()?.expect("only own writes");
            assert_eq!(refreshed.document_revision, snapshot.document_revision + 1);
            assert!(matches!(&refreshed.sourced_items[0].user_prompt_parameters.as_ref().expect("partial").media_delivery, UserPromptMediaDelivery::Preparing { resources } if resources == &vec![Some(original.clone())]));
            // The first actual API freezes the complete origin while retaining the earlier completion.
            // 首次实际API冻结完整原身份，同时保留较早完成。
            let mut prepared = prepared_fixture(refreshed);
            prepared.user_prompt_media = vec![UserPromptMediaBinding { prompt_seq: seq, origin: UserPromptMediaOrigin { route: original.original_route.clone(), resources: vec![original] } }];
            Ok(Some(prepared))
        }).await.expect("completion worker")
    }).await.expect("prepare").expect("valid transport");
    assert_eq!(result.revision, before + 2);
    assert_eq!(started_count(&handle), 1);
    assert_eq!(
        durable_parameters(&handle).media_delivery,
        UserPromptMediaDelivery::Frozen {
            origin: UserPromptMediaOrigin {
                route: expected.original_route.clone(),
                resources: vec![expected]
            }
        }
    );
}

/// Keeps a completed upload durable while rejecting external input/model revisions before or after recapture.
/// 保持已完成上传可靠落盘，同时拒绝重捕获前后发生的外部输入或模型修订。
#[tokio::test]
async fn physical_preparation_external_revision_never_disappears_behind_own_completion() {
    for (change_model, change_after_recapture) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        // Every case starts from an independent real store and pending resource.
        // 每个用例从独立真实存储及待准备资源开始。
        let (_directory, handle, request, original) = completion_fixture();
        // The same live owner receives a real concurrent-style metadata mutation while no request locks are held.
        // 无请求锁持有时，同一实时所有者接收真实并发形态元数据修改。
        let changing_handle = Arc::clone(&handle);
        // Original result must remain A even when the candidate can no longer run.
        // 即使候选不能再运行，原结果也必须仍为A。
        let expected = original.clone();
        // The builder's timing points are explicit rather than sleep-based races.
        // 构建者时间点显式控制，而非依赖睡眠竞争。
        let result = handle
            .prepare_physical_api_durable(
                ApiPurposeKind::MainGeneration,
                request,
                LockedBaselineProjection::empty(),
                None,
                4,
                move |snapshot, preparation| async move {
                    tokio::task::spawn_blocking(move || {
                        // Apply the actual document mutation used by the existing frontier regression.
                        // 应用既有前沿回归使用的实际文档修改。
                        let change = || -> Result<(), SessionRuntimeError> {
                            if change_model {
                                changing_handle.mutate_durable_if_changed(
                                    SessionMutationAdmission::MetadataOnly,
                                    SessionFlushReason::MetadataChanged,
                                    |document| {
                                        Ok(
                                            if document.set_pending_request(
                                                PendingRequestConfig {
                                                    provider: Some("provider-1".into()),
                                                    connection_id: Some("connection-1".into()),
                                                    model: Some("model-2".into()),
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
                                )?;
                            } else {
                                changing_handle.set_draft(
                                    "next input".into(),
                                    vec![],
                                    vec![],
                                    vec![],
                                    3,
                                )?;
                            }
                            Ok(())
                        };
                        if !change_after_recapture {
                            change()?;
                        }
                        preparation.record_resource(
                            UserPromptResourceBinding {
                                prompt_seq: snapshot.preparing_user_prompt_media_seqs[0],
                                resource_position: 0,
                                resource: original,
                            },
                            3,
                        )?;
                        // Own durable writes cannot erase the fact that a foreign revision occurred earlier.
                        // 自身可靠写入不能抹去先前发生外部修订的事实。
                        let refreshed = preparation.recapture()?;
                        if change_after_recapture {
                            change()?;
                            Ok(Some(prepared_fixture(
                                refreshed.expect("recaptured before foreign write"),
                            )))
                        } else {
                            assert!(refreshed.is_none());
                            Ok(None)
                        }
                    })
                    .await
                    .expect("completion worker")
                },
            )
            .await
            .expect("controlled invalidation");
        assert!(result.is_none());
        assert_eq!(started_count(&handle), 0);
        assert_eq!(
            durable_parameters(&handle).media_delivery,
            UserPromptMediaDelivery::Preparing {
                resources: vec![Some(expected)]
            }
        );
    }
}

/// Retains the same gate after parent abort so terminal persistence cannot overtake an already-known completion.
/// 父任务中止后保留同一门，使终态持久化不能抢先于已知完成。
#[tokio::test]
async fn physical_preparation_cancelled_parent_settles_completion_before_stop() {
    // One isolated resource will complete while its awaiting parent is cancelled.
    // 一个隔离资源将在等待它的父任务取消时完成。
    let (_directory, handle, request, original) = completion_fixture();
    // The durable identity is compared after actual stop and physical reload.
    // 在实际停止及物理重载后比较可靠身份。
    let expected = original.clone();
    // Channels establish worker entry and release without a timing guess or fake writer implementation.
    // 通道建立工作线程进入及放行，不猜测时序或伪造写入器实现。
    let (entered_tx, entered_rx) = oneshot::channel();
    // The blocking worker waits on a bounded synchronous channel while keeping the exact preparation lease.
    // 阻塞工作线程在有界同步通道上等待，同时保留精确准备租约。
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel::<()>(1);
    // A separate task owns the same production preparation future that Stop aborts.
    // 独立任务持有Stop中止的同一生产准备Future。
    let preparing_handle = Arc::clone(&handle);
    // Keep the parent task separate so abort drops its future but not the completed-result worker.
    // 保持父任务独立，使中止丢弃其Future而非已完成结果工作线程。
    let preparing = tokio::spawn(async move {
        preparing_handle
            .prepare_physical_api_durable::<(), _>(
                ApiPurposeKind::MainGeneration,
                request,
                LockedBaselineProjection::empty(),
                None,
                4,
                move |snapshot, preparation| async move {
                    tokio::task::spawn_blocking(move || {
                        entered_tx.send(()).expect("worker entered");
                        release_rx
                            .recv_timeout(Duration::from_secs(5))
                            .expect("release known completion");
                        preparation.record_resource(
                            UserPromptResourceBinding {
                                prompt_seq: snapshot.preparing_user_prompt_media_seqs[0],
                                resource_position: 0,
                                resource: original,
                            },
                            3,
                        )?;
                        Ok(None)
                    })
                    .await
                    .expect("completion worker")
                },
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered_rx)
        .await
        .expect("entry deadline")
        .expect("worker entry");
    preparing.abort();
    assert!(
        preparing
            .await
            .expect_err("parent cancelled")
            .is_cancelled()
    );
    // Poll the actual Stop gate once: it must be pending while the completed-result worker still owns the lease.
    // 实际轮询Stop门一次：已完成结果工作线程仍持有租约时必须等待。
    let mut settlement = Box::pin(handle.acquire_request_preparation_settlement());
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(
            settlement.as_mut().poll(cx).is_pending()
        ))
        .await
    );
    release_tx.send(()).expect("finish completion");
    // Stop retains the returned guard through its durable terminal mutation, exactly as the production stop path does.
    // 与生产停止路径相同，Stop将返回守卫保留至其可靠终态修改完成。
    let _settlement = tokio::time::timeout(Duration::from_secs(5), settlement)
        .await
        .expect("settled completion");
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::TurnCompleted,
            |document| document.close_turn(vulcan_session_core::v1::TurnOutcome::Stopped, 0, 4),
        )
        .expect("stop after completion");
    assert_eq!(started_count(&handle), 0);
    assert_eq!(
        durable_parameters(&handle).media_delivery,
        UserPromptMediaDelivery::Preparing {
            resources: vec![Some(expected)]
        }
    );
}

/// Rejects a manually advanced proof after own completion when no actual durable recapture was performed.
/// 自身完成后若未执行实际可靠重捕获，则拒绝手工推进的证明。
#[tokio::test]
async fn physical_preparation_completion_cannot_forge_recaptured_revision() {
    // A real completed resource alone does not authorize rewriting an older snapshot's revision number.
    // 真实完成资源本身不授权改写旧快照修订号。
    let (_directory, handle, request, original) = completion_fixture();
    // The result remains durable even when the request builder violates its separate proof contract.
    // 即使请求构建者违反其独立证明契约，结果仍保持可靠落盘。
    let expected = original.clone();
    // The final gate must reject this builder's explicit proof violation without losing the completed result.
    // 最终门必须拒绝本构建者显式证明违规，且不丢失已完成结果。
    let error = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            4,
            move |snapshot, preparation| async move {
                tokio::task::spawn_blocking(move || {
                    preparation.record_resource(
                        UserPromptResourceBinding {
                            prompt_seq: snapshot.preparing_user_prompt_media_seqs[0],
                            resource_position: 0,
                            resource: original,
                        },
                        3,
                    )?;
                    // This intentionally forged update must never substitute for recapture from the persisted range.
                    // 本有意伪造更新绝不能替代从持久区间重捕获。
                    let mut prepared = prepared_fixture(snapshot);
                    prepared.projection.document_revision += 1;
                    Ok(Some(prepared))
                })
                .await
                .expect("completion worker")
            },
        )
        .await
        .expect_err("reject forged source proof");
    assert!(error.to_string().contains("proof does not match"));
    assert_eq!(started_count(&handle), 0);
    assert_eq!(
        durable_parameters(&handle).media_delivery,
        UserPromptMediaDelivery::Preparing {
            resources: vec![Some(expected)]
        }
    );
}
