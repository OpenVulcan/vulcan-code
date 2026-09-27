//! Production reservation/observer transitions exercised with an existing isolated frozen execution fixture.
//! 使用既有隔离冻结执行夹具验证生产许可与观察者迁移。

use super::*;

/// Creates one isolated original reservation; `execution` supplies real resolved route facts without network work.
/// 创建一份隔离原许可；`execution`提供真实解析的路线事实，不执行网络工作。
fn reservation(
    execution: &Arc<vulcan_provider_adapters::ResolvedPublicAiExecution>,
) -> Arc<DraftUploadReservation> {
    Arc::new(DraftUploadReservation {
        identity: DraftUploadIntentIdentity {
            workspace_path: String::from("isolated-reservation-workspace"),
            draft_scope: format!("composer-draft:{}", uuid::Uuid::new_v4()),
            intent_id: uuid::Uuid::new_v4().to_string(),
            attachment_id: uuid::Uuid::new_v4().to_string(),
        },
        execution: Arc::clone(execution),
        started: OnceLock::new(),
    })
}

/// Publishes a real view plan for `original` and `generation`; returns the exact request accepted by consumption.
/// 为`original`及`generation`发布真实视图计划；返回消费入口接纳的精确请求。
fn publish(
    original: &Arc<DraftUploadReservation>,
    generation: u64,
) -> ComposerAttachmentPreparationSummary {
    // Route identity comes only from the existing frozen fixture.
    // 路线身份只来自既有冻结夹具。
    let execution = &original.execution;
    let preparation = ComposerAttachmentPreparationSummary {
        converted_attachment: None,
        attachment_id: original.identity.attachment_id.clone(),
        generation,
        transfer_id: format!("draft-view:{}", uuid::Uuid::new_v4()),
        physical_transfer_id: original
            .started
            .get()
            .map(|operation| operation.transfer_id.clone()),
        delivery_plan: ComposerAttachmentDeliveryPlan::UploadReference,
        phase: ComposerAttachmentPreparationPhase::UploadQueued,
        blocks_submission: true,
        route_revision: Some(execution.route.route_revision.clone()),
        reason: String::from("provider_upload_required"),
        error: None,
    };
    let plan = DraftMediaUploadPlan {
        reservation: Arc::clone(original),
        identity: DraftMediaUploadPlanIdentity {
            workspace_path: original.identity.workspace_path.clone(),
            conversation_id: None,
            model_selection: SubmissionModelSelection {
                provider_id: execution.route.provider.id.clone(),
                model_id: execution.model.id.clone(),
                connection_id: execution.route.connection.id.clone(),
            },
            attachment_id: preparation.attachment_id.clone(),
            generation,
            route_revision: execution.route.route_revision.clone(),
        },
        execution: Arc::clone(execution),
        created_at: std::time::Instant::now(),
    };
    register_draft_media_upload_plans(vec![(preparation.transfer_id.clone(), plan)])
        .expect("publish real plan");
    preparation
}

/// Consumes a published `view` against its exact original fixture owner and returns the production transition.
/// 根据精确原夹具所有者消费已发布`view`，返回生产迁移。
fn consume(
    original: &Arc<DraftUploadReservation>,
    view: &ComposerAttachmentPreparationSummary,
) -> DraftUploadPreparation {
    let execution = &original.execution;
    consume_draft_media_upload_plan(
        &view.transfer_id,
        &original.identity.workspace_path,
        &None,
        &SubmissionModelSelection {
            provider_id: execution.route.provider.id.clone(),
            model_id: execution.model.id.clone(),
            connection_id: execution.route.connection.id.clone(),
        },
        view,
    )
    .expect("consume exact view")
    .1
}

/// Exercises real registry transitions without source/network IO; the caller owns the existing frozen fixture.
/// 不执行来源或网络IO，验证真实注册表迁移；调用方持有既有冻结夹具。
/// Returns after every owner/observer is settled and every test plan is removed.
/// 所有所有者与观察者结算、全部测试计划移除后返回。
pub(crate) async fn exercise_draft_upload_reservation_lifecycle(
    execution: Arc<vulcan_provider_adapters::ResolvedPublicAiExecution>,
) {
    // Independently proposed concurrent views merge onto one reservation during atomic publication.
    // 独立提出的并发视图在原子发布期间合并到同一许可。
    let original = reservation(&execution);
    let duplicate = Arc::new(DraftUploadReservation {
        identity: original.identity.clone(),
        execution: Arc::clone(&execution),
        started: OnceLock::new(),
    });
    let first = publish(&original, 1);
    let second = publish(&duplicate, 2);
    let third = publish(&original, 3);
    let DraftUploadPreparation::Start(owner) = consume(&original, &first) else {
        panic!("first view owns start");
    };
    let token = owner.cancellation.clone();
    let DraftUploadPreparation::Observe(observer) = consume(&original, &second) else {
        panic!("second view must observe");
    };
    assert!(observer.operation.cancellation.same_operation(&token));
    drop(observer);
    assert!(
        !token.is_cancelled(),
        "dropping a view must preserve its physical owner"
    );
    assert!(
        pending_media_uploads()
            .lock()
            .expect("pending")
            .contains_key(&first.transfer_id)
    );
    // Exercise the new public discard route while the original physical registration is still live.
    // 原实体登记仍活动时验证新的公开丢弃路由。
    let state = Arc::new(StdMutex::new(
        crate::tests::test_support::test_service_state_with_parent_session(
            vulcan_workbench_model::SessionId::new_v4(),
        ),
    ));
    let discarded = crate::rpc_dispatch::dispatch_service_rpc(
        crate::ServiceEventEmitter::new(Vec::new()),
        state,
        String::from("service_discard_composer_attachment_plans"),
        Some(serde_json::json!({"transferIds": [third.transfer_id, first.transfer_id]})),
    )
    .await
    .expect("real discard RPC");
    assert_eq!(discarded, serde_json::json!(1));
    assert_eq!(
        discard_composer_attachment_plans(vec![first.transfer_id.clone()])
            .expect("do not discard active owner"),
        0
    );
    assert!(!token.is_cancelled());
    drop(owner);
    original
        .started
        .get()
        .expect("started once")
        .clone()
        .wait()
        .await
        .expect("settled owner");
    assert!(token.is_cancelled());

    // A retained view still observes the completed first operation; a settled once-cell cannot grant another start.
    // 保留视图仍观察已经完成的首次操作；已结算的一次性单元不能授予再次启动。
    let late = publish(&original, 4);
    let DraftUploadPreparation::Observe(late_observer) = consume(&original, &late) else {
        panic!("settled start must not repeat");
    };
    late_observer
        .operation
        .clone()
        .wait()
        .await
        .expect("late observer sees terminal fact");
    drop(late_observer);

    // Explicit cancellation through an unconsumed observer is different from discarding its view plan.
    // 通过未消费观察者显式取消，与丢弃其视图计划不同。
    let original = reservation(&execution);
    let first = publish(&original, 1);
    let DraftUploadPreparation::Start(owner) = consume(&original, &first) else {
        panic!("owner");
    };
    let token = owner.cancellation.clone();
    let alias = publish(&original, 2);
    let cancelling = tokio::spawn(async move { cancel_media_upload(&alias.transfer_id).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), token.wait())
        .await
        .expect("original receives cancellation");
    assert!(
        !cancelling.is_finished(),
        "cancellation must await physical settlement"
    );
    drop(owner);
    assert!(
        cancelling
            .await
            .expect("cancellation task")
            .expect("settlement")
    );

    // Once no live plan or operation retains an original intent, automatic planning cannot restart it.
    // 一旦没有活动计划或操作保留原意图，自动规划不能重启它。
    assert!(
        resolve_reservation(original.identity.clone(), &execution, false)
            .expect("closed original")
            .is_none()
    );
    assert!(
        resolve_reservation(original.identity.clone(), &execution, true)
            .expect("explicit retry")
            .is_some()
    );

    // A consumed observer's cancellation alias reaches the owner, while its drop remains non-cancelling.
    // 已消费观察者的取消别名可以到达所有者，但其丢弃仍不取消。
    let original = reservation(&execution);
    let first = publish(&original, 1);
    let second = publish(&original, 2);
    let DraftUploadPreparation::Start(owner) = consume(&original, &first) else {
        panic!("owner");
    };
    let DraftUploadPreparation::Observe(observer) = consume(&original, &second) else {
        panic!("observer");
    };
    let token = owner.cancellation.clone();
    let alias_id = observer.alias_id.clone();
    let cancelling = tokio::spawn(async move { cancel_media_upload(&alias_id).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), token.wait())
        .await
        .expect("alias cancels original");
    assert!(!cancelling.is_finished());
    drop(owner);
    assert!(cancelling.await.expect("cancel task").expect("settled"));
    observer
        .operation
        .clone()
        .wait()
        .await
        .expect("observer settled");
    assert!(
        !pending_media_uploads()
            .lock()
            .expect("pending")
            .contains_key(&observer.alias_id)
    );
    drop(observer);

    // The same UUID under a different owner is rejected instead of borrowing another draft's upload.
    // 不同所有者下相同UUID会被拒绝，不能借用其他草稿的上传。
    let original = reservation(&execution);
    let first = publish(&original, 1);
    let mut foreign = original.identity.clone();
    foreign.draft_scope.push_str("-foreign");
    assert!(resolve_reservation(foreign, &execution, false).is_err());
    assert_eq!(
        discard_composer_attachment_plans(vec![first.transfer_id]).expect("fixture cleanup"),
        1
    );

    // Cancelling one queued view revokes the shared start permission even before any physical owner exists.
    // 在尚无实体所有者时，取消一个排队视图也会撤销共享启动许可。
    let original = reservation(&execution);
    let first = publish(&original, 1);
    let second = publish(&original, 2);
    assert!(
        cancel_media_upload(&first.transfer_id)
            .await
            .expect("cancel shared queued start")
    );
    let DraftUploadPreparation::Observe(observer) = consume(&original, &second) else {
        panic!("a cancelled permission must not start through another view");
    };
    observer
        .operation
        .clone()
        .wait()
        .await
        .expect("unstarted cancellation is already settled");
    drop(observer);

    // Downgrading an unavailable original upload refunds its native count before later chronological resources.
    // 将不可用原上传降为地址时，在后续时序资源之前退还其原生数量额度。
    let directory = tempfile::tempdir().expect("isolated count source");
    let descriptor =
        vulcan_agent_runtime::attachment_assets::attachment_descriptor_from_mime_type("image/png")
            .expect("image descriptor");
    let attachment = vulcan_agent_runtime::attachment_assets::stage_attachment_bytes(
        &directory.path().to_string_lossy(),
        &descriptor,
        "count.png",
        None,
        b"abc",
    )
    .expect("canonical source");
    let mut planner =
        ComposerMediaDeliveryPlanner::new(execution.media_delivery_policy).expect("planner");
    assert!(
        planner
            .discard_upload_count(vulcan_ai_contract::ResourceKind::Image)
            .is_err()
    );
    for _ in 0..5 {
        assert_eq!(
            planner
                .plan_draft(&execution, &attachment, None, Some("test-session"))
                .expect("upload decision")
                .0,
            ComposerAttachmentDeliveryPlan::UploadReference
        );
        planner
            .discard_upload_count(vulcan_ai_contract::ResourceKind::Image)
            .expect("refund unavailable upload");
    }
    for _ in 0..4 {
        assert_eq!(
            planner
                .plan_draft(&execution, &attachment, None, Some("test-session"))
                .expect("native occurrence")
                .0,
            ComposerAttachmentDeliveryPlan::UploadReference
        );
    }
    assert_eq!(
        planner
            .plan_draft(&execution, &attachment, None, Some("test-session"))
            .expect("exact count cap")
            .0,
        ComposerAttachmentDeliveryPlan::MetadataOnly
    );
}
