//! Two-phase Provider preparation concurrency and cancellation regressions.
//! 两阶段 Provider 准备的并发与取消回归。

use super::*;
use tokio::sync::oneshot;

mod completion;
mod continuation;

/// Creates one real temporary V1 store with an open root turn and a typed request snapshot.
/// 创建一个真实临时 V1 存储，其中包含开放根回合及强类型请求快照。
/// Returns the directory owner, unique session handle and request; no provider is contacted.
/// 返回目录所有者、唯一会话句柄与请求，不联系供应商。
fn preparation_fixture() -> (tempfile::TempDir, Arc<SessionHandle>, ApiRequestSnapshot) {
    preparation_fixture_with_resources(Vec::new())
}

/// Creates the same durable fixture with exact ordered `resources` attached to its root prompt.
/// 创建相同可靠夹具，并将精确有序`resources`附着到其根提示词。
/// Returns the store owner, unique handle and request without any network side effects.
/// 返回存储所有者、唯一句柄及请求，不产生网络副作用。
fn preparation_fixture_with_resources(
    resources: Vec<vulcan_session_core::v1::ResourceId>,
) -> (tempfile::TempDir, Arc<SessionHandle>, ApiRequestSnapshot) {
    // Keep the actual storage locator alive for the full preparation and durable commit.
    // 为完整准备与可靠提交保留实际存储定位器。
    let directory = tempfile::tempdir().expect("temporary preparation store");
    let session_id = SessionId::new_v4();
    let locator = create_session_v1_storage_fixture(directory.path(), &session_id.to_string(), 1)
        .expect("create exact session store");
    let loaded = load_session_v1_full(&locator).expect("load initial document");
    let handle = SessionHandle::new(session_id, locator, loaded.document);
    let mut submission = test_client_input("prepare-root", "root");
    submission.resource_ids = resources;
    handle
        .submit_root_input(submission, 2)
        .expect("open root turn");
    let request = ApiRequestSnapshot {
        provider: String::from("provider-1"),
        profile: None,
        model: String::from("model-1"),
        connection_id: Some(String::from("connection-1")),
        thinking_level: None,
        temperature: None,
    };
    (directory, handle, request)
}

/// Builds the test transport and proof for exact `snapshot` without reading or mutating the live document.
/// 为精确 `snapshot` 构建测试传输与证明，不读取或修改实时文档。
/// Returns a transport fixture accepted by the same durable commit gate as production requests.
/// 返回由生产请求相同可靠提交门接受的传输夹具。
fn prepared_fixture(
    snapshot: SessionProviderProjectionSnapshot,
) -> SessionPreparedPhysicalRequest<()> {
    SessionPreparedPhysicalRequest {
        user_prompt_media: Vec::new(),
        transport: (),
        projection: ProviderProjectionProof {
            segment_generation: snapshot.segment_generation,
            document_revision: snapshot.document_revision,
            retained_from_seq: snapshot.retained_from_seq,
            through_seq: snapshot.through_seq,
            fingerprint_sha256: "a".repeat(64),
        },
        request_budget_evidence: test_provider_request_budget_evidence(),
    }
}

/// Counts actual durable-start events on `handle`; returns no derived UI or legacy state.
/// 统计 `handle` 上的实际可靠起始事件，不读取派生 UI 或旧状态。
fn started_count(handle: &SessionHandle) -> usize {
    handle
        .read(|document| {
            document
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventBody::ApiStarted(_)))
                .count()
        })
        .expect("read authoritative starts")
}

/// Verifies media work runs outside both synchronous locks and only a durable start releases the result.
/// 验证媒体工作在两个同步锁之外运行，且仅可靠起始会放行结果。
#[tokio::test]
async fn physical_preparation_releases_writer_and_publisher_during_build() {
    let (_directory, handle, request) = preparation_fixture();
    // The builder checks the very same owner, not a second handle or a mock lock.
    // 构建者检查同一所有者，不使用第二句柄或模拟锁。
    let builder_handle = Arc::clone(&handle);
    let receipt = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            3,
            move |snapshot, _preparation| async move {
                assert!(
                    builder_handle.writer.try_lock().is_ok(),
                    "writer held during media work"
                );
                assert!(
                    builder_handle.mutation_publisher.try_lock().is_ok(),
                    "publisher held during media work"
                );
                assert_eq!(started_count(&builder_handle), 0);
                tokio::task::yield_now().await;
                Ok(Some(prepared_fixture(snapshot)))
            },
        )
        .await
        .expect("prepare result")
        .expect("unchanged frontier");
    assert!(receipt.changed);
    assert_eq!(started_count(&handle), 1);
    assert!(
        handle
            .read(|document| document.memory_revision() == document.durable_revision())
            .expect("durable frontier")
    );
    // A fresh disk load must observe the same committed API, not just matching in-memory counters.
    // 全新磁盘加载必须观察到相同已提交 API，而非仅仅内存计数相等。
    let locator = handle.storage_locator().expect("durable locator");
    let loaded = load_session_v1_full(&locator).expect("reload committed API");
    assert_eq!(
        loaded
            .document
            .active_api()
            .expect("persisted active API")
            .api_call_id,
        receipt.value.core_start.api_call_id
    );
}

/// Verifies a concurrent document or picker write invalidates prepared bytes without a phantom API start.
/// 验证并发文档或选择器写入会使已准备字节失效，而不会产生虚假 API 起始。
#[tokio::test]
async fn physical_preparation_rejects_frontier_changes_during_build() {
    for (change_model, requires_compaction) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let (_directory, handle, request) = preparation_fixture();
        // Paired channels pause the resource phase at a deterministic boundary.
        // 成对通道把资源阶段暂停在确定性边界。
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let preparing_handle = Arc::clone(&handle);
        let preparing = tokio::spawn(async move {
            preparing_handle
                .prepare_physical_api_durable(
                    ApiPurposeKind::MainGeneration,
                    request,
                    LockedBaselineProjection::empty(),
                    None,
                    4,
                    move |snapshot, _preparation| async move {
                        entered_tx.send(()).expect("builder entered");
                        release_rx.await.expect("release builder");
                        if requires_compaction {
                            Err(SessionRuntimeError::ProviderContextCompactionRequired(
                                "stale final budget".into(),
                            ))
                        } else {
                            Ok(Some(prepared_fixture(snapshot)))
                        }
                    },
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), entered_rx)
            .await
            .expect("builder entry deadline")
            .expect("entry signal");
        // A real durable mutation must complete while media preparation is paused.
        // 媒体准备暂停时，真实可靠修改必须能够完成。
        let editing_handle = Arc::clone(&handle);
        let mutation = tokio::task::spawn_blocking(move || {
            if change_model {
                editing_handle
                    .mutate_durable_if_changed(
                        SessionMutationAdmission::MetadataOnly,
                        SessionFlushReason::MetadataChanged,
                        |document| {
                            let changed = document.set_pending_request(
                                PendingRequestConfig {
                                    provider: Some(String::from("provider-1")),
                                    connection_id: Some(String::from("connection-1")),
                                    model: Some(String::from("model-2")),
                                    thinking_level: None,
                                    temperature: None,
                                    set_at_ms: 3,
                                },
                                3,
                            )?;
                            assert!(changed);
                            Ok(SessionMutationChange::Changed(()))
                        },
                    )
                    .map(|_| ())
            } else {
                editing_handle
                    .set_draft(
                        String::from("next input"),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        3,
                    )
                    .map(|_| ())
            }
        });
        tokio::time::timeout(Duration::from_secs(5), mutation)
            .await
            .expect("mutation is not blocked by media")
            .expect("mutation worker")
            .expect("durable mutation");
        release_tx.send(()).expect("resume builder");
        let receipt = preparing
            .await
            .expect("preparing task")
            .expect("stale result is controlled");
        assert!(
            receipt.is_none(),
            "changed frontier must discard prepared transport"
        );
        assert_eq!(started_count(&handle), 0);
        assert!(
            handle
                .read(|document| document.active_api().is_none())
                .expect("no active API")
        );
    }
}

/// Verifies only one same-session builder runs and a later duplicate cannot prepare media for an active API.
/// 验证同会话仅运行一个构建者，后续重复者不能为活动 API 再次准备媒体。
#[tokio::test]
async fn physical_preparation_is_single_flight_before_any_resource_side_effect() {
    let (_directory, handle, request) = preparation_fixture();
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let first_handle = Arc::clone(&handle);
    let first_request = request.clone();
    let first = tokio::spawn(async move {
        first_handle
            .prepare_physical_api_durable(
                ApiPurposeKind::MainGeneration,
                first_request,
                LockedBaselineProjection::empty(),
                None,
                3,
                move |snapshot, _preparation| async move {
                    entered_tx.send(()).expect("first builder entry");
                    release_rx.await.expect("first builder release");
                    Ok(Some(prepared_fixture(snapshot)))
                },
            )
            .await
    });
    entered_rx.await.expect("first build paused");
    // The second callback represents any read or upload side effect and must never be entered.
    // 第二回调代表任意读取或上传副作用，绝不能进入。
    let second_entered = Arc::new(AtomicBool::new(false));
    let second_marker = Arc::clone(&second_entered);
    let second_handle = Arc::clone(&handle);
    let second = tokio::spawn(async move {
        second_handle
            .prepare_physical_api_durable(
                ApiPurposeKind::MainGeneration,
                request,
                LockedBaselineProjection::empty(),
                None,
                3,
                move |snapshot, _preparation| async move {
                    second_marker.store(true, Ordering::SeqCst);
                    Ok(Some(prepared_fixture(snapshot)))
                },
            )
            .await
    });
    tokio::task::yield_now().await;
    assert!(!second_entered.load(Ordering::SeqCst));
    release_tx.send(()).expect("finish first build");
    assert!(
        first
            .await
            .expect("first task")
            .expect("first preparation")
            .is_some()
    );
    assert!(second.await.expect("second task").is_err());
    assert!(!second_entered.load(Ordering::SeqCst));
    assert_eq!(started_count(&handle), 1);
}

/// Verifies cancellation during resource preparation releases the gate without persisting an API start.
/// 验证资源准备期间取消会释放准备门，且不持久化 API 起始。
#[tokio::test]
async fn physical_preparation_cancellation_releases_gate_without_api_start() {
    let (_directory, handle, request) = preparation_fixture();
    let (entered_tx, entered_rx) = oneshot::channel();
    let preparing_handle = Arc::clone(&handle);
    let preparing_request = request.clone();
    let preparing = tokio::spawn(async move {
        preparing_handle
            .prepare_physical_api_durable(
                ApiPurposeKind::MainGeneration,
                preparing_request,
                LockedBaselineProjection::empty(),
                None,
                3,
                move |_snapshot, _preparation| async move {
                    entered_tx.send(()).expect("cancelled builder entered");
                    std::future::pending::<
                        Result<Option<SessionPreparedPhysicalRequest<()>>, SessionRuntimeError>,
                    >()
                    .await
                },
            )
            .await
    });
    entered_rx.await.expect("resource work started");
    preparing.abort();
    assert!(
        preparing
            .await
            .expect_err("preparation cancelled")
            .is_cancelled()
    );
    assert_eq!(started_count(&handle), 0);
    // A subsequent explicit preparation must not inherit a permanently held gate.
    // 后续显式准备不能继承永久占用的准备门。
    let next = tokio::time::timeout(
        Duration::from_secs(5),
        handle.prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            3,
            |snapshot, _preparation| async move { Ok(Some(prepared_fixture(snapshot))) },
        ),
    )
    .await
    .expect("released gate deadline")
    .expect("next preparation");
    assert!(next.is_some());
    assert_eq!(started_count(&handle), 1);
}

/// Verifies a builder cannot forge the durable source frontier even when no concurrent write occurred.
/// 验证即使未发生并发写入，构建者也不能伪造可靠来源前沿。
#[tokio::test]
async fn physical_preparation_rejects_mismatched_proof_without_starting_api() {
    let (_directory, handle, request) = preparation_fixture();
    let error = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            3,
            |snapshot, _preparation| async move {
                let mut prepared = prepared_fixture(snapshot);
                prepared.projection.document_revision += 1;
                Ok(Some(prepared))
            },
        )
        .await
        .expect_err("proof mismatch");
    assert!(error.to_string().contains("proof does not match"));
    assert_eq!(started_count(&handle), 0);
}

/// Verifies a failed resource builder leaves no API and releases the single-flight gate for a later explicit attempt.
/// 验证资源构建失败不留下 API，并释放单一准备门以供后续显式尝试。
#[tokio::test]
async fn physical_preparation_builder_failure_preserves_state_and_releases_gate() {
    // The actual durable frontier must remain unchanged because this fixture has no pending selection to promote.
    // 因为夹具没有需要提升的待生效选择，实际可靠前沿必须保持不变。
    let (_directory, handle, request) = preparation_fixture();
    let original_revision = handle
        .read(|document| document.durable_revision())
        .expect("original durable revision");
    let error = handle
        .prepare_physical_api_durable::<(), _>(
            ApiPurposeKind::MainGeneration,
            request.clone(),
            LockedBaselineProjection::empty(),
            None,
            3,
            |_snapshot, _preparation| async {
                Err(SessionRuntimeError::ProviderProjectionInvariant(
                    String::from("simulated resource preparation failure"),
                ))
            },
        )
        .await
        .expect_err("resource builder failed");
    assert!(
        error
            .to_string()
            .contains("simulated resource preparation failure")
    );
    assert_eq!(started_count(&handle), 0);
    assert_eq!(
        handle
            .read(|document| document.durable_revision())
            .expect("after failure"),
        original_revision
    );
    // This second request is explicit test intent, not an automatic upload retry or representation fallback.
    // 第二请求是明确测试意图，不是自动上传重试或表示形式回退。
    let next = tokio::time::timeout(
        Duration::from_secs(5),
        handle.prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            4,
            |snapshot, _preparation| async move { Ok(Some(prepared_fixture(snapshot))) },
        ),
    )
    .await
    .expect("released preparation gate")
    .expect("explicit next preparation");
    assert!(next.is_some());
    assert_eq!(started_count(&handle), 1);
}

/// Verifies real disk persistence commits prompt-owned media parameters and API start in the same document revision.
/// 验证真实磁盘持久化在同一文档修订内提交提示词所属媒体参数和API起始。
#[tokio::test]
async fn physical_preparation_commits_user_prompt_parameters_with_api_start() {
    use vulcan_session_core::v1::{
        ResourceId, UserPromptMediaBinding, UserPromptMediaDelivery, UserPromptMediaOrigin,
        UserPromptMediaRoute, UserPromptParameters, UserPromptResourceDelivery,
        UserPromptResourceDeliveryKind,
    };

    // The project resource identity belongs to this exact root occurrence, not a later cache owner.
    // 项目资源身份属于此次精确根输入出现位置，不属于后来的缓存所有者。
    let resource_id = ResourceId::try_new(7).expect("project resource identity");
    let (_directory, handle, mut request) = preparation_fixture_with_resources(vec![resource_id]);
    request.profile = Some(String::from("openai-responses.v1"));
    let (prompt_seq, revision_before) = handle
        .read(|document| (document.events()[0].seq(), document.memory_revision()))
        .expect("root and captured revision");
    let origin = UserPromptMediaOrigin {
        route: UserPromptMediaRoute {
            provider_id: request.provider.clone(),
            service_entry_id: String::from("test-service"),
            connection_id: request.connection_id.clone().expect("fixture connection"),
            model_id: request.model.clone(),
            protocol_profile_id: request.profile.clone().expect("fixture protocol"),
        },
        resources: vec![UserPromptResourceDelivery {
            resource_id,
            original_route: UserPromptMediaRoute {
                provider_id: request.provider.clone(),
                service_entry_id: String::from("test-service"),
                connection_id: request.connection_id.clone().expect("fixture connection"),
                model_id: request.model.clone(),
                protocol_profile_id: request.profile.clone().expect("fixture protocol"),
            },
            delivery: UserPromptResourceDeliveryKind::Inline,
        }],
    };
    let expected_origin = origin.clone();
    let receipt = handle
        .prepare_physical_api_durable(
            ApiPurposeKind::MainGeneration,
            request,
            LockedBaselineProjection::empty(),
            None,
            3,
            move |snapshot, _preparation| async move {
                assert_eq!(snapshot.sourced_items[0].source_from_seq, prompt_seq);
                assert_eq!(
                    snapshot.sourced_items[0].user_prompt_parameters,
                    UserPromptParameters::for_new_resources(&[resource_id])
                );
                let mut prepared = prepared_fixture(snapshot);
                prepared.user_prompt_media = vec![UserPromptMediaBinding { prompt_seq, origin }];
                Ok(Some(prepared))
            },
        )
        .await
        .expect("atomic prompt preparation")
        .expect("unchanged capture");
    assert_eq!(receipt.revision, revision_before + 1);
    let locator = handle.storage_locator().expect("prompt store locator");
    let loaded = load_session_v1_full(&locator).expect("reload persisted prompt and API");
    let EventBody::UserInput(prompt) = &loaded.document.events()[0].event else {
        panic!("the original event must remain the user prompt")
    };
    assert_eq!(loaded.document.events()[0].seq(), prompt_seq);
    assert_eq!(prompt.resource_ids, vec![resource_id]);
    assert_eq!(
        prompt.parameters,
        Some(UserPromptParameters {
            unproven_resource_positions: Vec::new(),
            version: 1,
            prepared_resources: Vec::new(),
            media_delivery: UserPromptMediaDelivery::Frozen {
                origin: expected_origin
            },
        })
    );
    assert_eq!(
        loaded
            .document
            .active_api()
            .expect("durable API")
            .api_call_id,
        receipt.value.core_start.api_call_id
    );
    assert_eq!(loaded.document.memory_revision(), receipt.revision);
}
