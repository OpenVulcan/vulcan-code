mod answer_mode;
mod collaboration_publication;
mod context_generation;
mod input_dispatch_authority;
mod request_preparation;
use super::*;
use std::path::PathBuf;
use std::sync::Barrier;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

use vulcan_service_api::ServiceEventSink;
use vulcan_session_core::v1::config::{ActiveRequestConfig, PendingRequestConfig};
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::ApiCompletion;
use vulcan_session_core::v1::event::{
    ApiCallStatus, ApiEndReason, ApiErrorCategory, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
    ContinuationEligibility, DisplayNoticeKind, EventBody, NoticeSeverity, ProviderProjectionProof,
    RedactedApiError, ToolCallStatus,
};
use vulcan_session_core::v1::ids::{ClientActionId, RequestStreamId};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::stats::UsageSnapshot;
use vulcan_workspace_store::workspace_store::SessionV1StorageError;

/// In-memory sink that captures the exact service events delivered by the publisher.
/// 捕获发布器投递的精确服务事件的内存 sink。
#[derive(Default)]
struct RecordingEventSink {
    /// Service events in synchronous delivery order.
    /// 按同步投递顺序保存的服务事件。
    events: Mutex<Vec<ServiceEvent>>,
}

impl RecordingEventSink {
    /// Returns one defensive copy of all delivered service events.
    /// 返回全部已投递服务事件的一份防御性副本。
    fn events(&self) -> Vec<ServiceEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Removes all previously delivered events while preserving the installed sink.
    /// 清除此前投递的全部事件，同时保留已经安装的 sink。
    fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

impl ServiceEventSink for RecordingEventSink {
    /// Captures one delivered service event without transforming its payload.
    /// 在不转换载荷的情况下捕获一条已投递服务事件。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events.lock().unwrap().push(event);
    }
}

/// In-memory backend that captures the latest complete candidate.
/// 捕获最新完整候选的内存后端。
#[derive(Default)]
struct RecordingPersistenceBackend {
    /// Latest candidate accepted as durable.
    /// 最近一次被接纳为可靠状态的候选。
    latest: Mutex<Option<FlushCandidate>>,
    /// Number of physical flush calls accepted by this backend.
    /// 该后端接纳的物理 flush 调用次数。
    flush_count: AtomicUsize,
}

impl RecordingPersistenceBackend {
    /// Returns the exact event tail retained by the latest durable candidate.
    /// 返回最近可靠候选保留的精确事件尾段。
    fn persisted_events(&self) -> Vec<vulcan_session_core::v1::event::ConversationEvent> {
        self.latest
            .lock()
            .unwrap()
            .as_ref()
            .map(|candidate| candidate.tail_events.clone())
            .unwrap_or_default()
    }

    /// Returns the number of physical flushes observed by the backend.
    /// 返回后端观察到的物理 flush 次数。
    fn flush_count(&self) -> usize {
        self.flush_count.load(Ordering::SeqCst)
    }
}

impl SessionPersistenceBackend for RecordingPersistenceBackend {
    /// Captures one immutable candidate and publishes its exact metadata.
    /// 捕获一份不可变候选并发布其精确 metadata。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        self.flush_count.fetch_add(1, Ordering::SeqCst);
        *self.latest.lock().unwrap() = Some(candidate.clone());
        Ok(SessionV1FlushResult {
            written_file_count: candidate.dirty.len(),
            published_metadata: Some(candidate.metadata.clone()),
        })
    }
}

/// Backend that fails a fixed number of calls before delegating to disk.
/// 在委托磁盘写入前固定失败若干次的后端。
struct FailThenPersistBackend {
    /// Remaining injected failures.
    /// 剩余注入失败次数。
    remaining_failures: AtomicUsize,
    /// Recording backend used after failures are exhausted.
    /// 失败耗尽后使用的记录后端。
    delegate: Arc<RecordingPersistenceBackend>,
}

impl SessionPersistenceBackend for FailThenPersistBackend {
    /// Fails deterministically, then writes the latest complete candidate.
    /// 确定性失败，随后写入最新完整候选。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        let remaining = self.remaining_failures.load(Ordering::SeqCst);
        if remaining > 0 {
            self.remaining_failures.fetch_sub(1, Ordering::SeqCst);
            return Err(SessionV1StoreError::Storage(SessionV1StorageError::io(
                PathBuf::from("injected-session-v1.json"),
                "injected registry writer failure",
            )));
        }
        self.delegate.flush(candidate)
    }
}

/// Backend that commits the candidate but corrupts one returned publication receipt.
/// 提交候选后破坏一次返回发布回执的后端。
struct CorruptPublishedMetadataBackend {
    /// Whether the next successful publication receipt must carry a foreign session id.
    /// 下一份成功发布回执是否必须携带外部会话 id。
    corrupt_next: AtomicBool,
    /// Recording backend that proves the candidate was physically accepted first.
    /// 用于证明候选已经先被物理接纳的记录后端。
    delegate: Arc<RecordingPersistenceBackend>,
}

impl SessionPersistenceBackend for CorruptPublishedMetadataBackend {
    /// Persists first, then injects one impossible post-commit metadata identity.
    /// 先持久化，再注入一份不可能成立的提交后 metadata 身份。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        let mut outcome = self.delegate.flush(candidate)?;
        if self.corrupt_next.swap(false, Ordering::SeqCst)
            && let Some(metadata) = outcome.published_metadata.as_mut()
        {
            metadata.session_id = String::from("foreign-session");
        }
        Ok(outcome)
    }
}

/// Backend that pauses its first candidate after capture and before publication.
/// 在首份候选捕获后、发布前暂停的后端。
struct BlockingPersistBackend {
    /// Shared `(entered, released)` state for deterministic coordination.
    /// 用于确定性协调的共享 `(已进入, 已释放)` 状态。
    gate: Arc<(Mutex<(bool, bool)>, Condvar)>,
    /// Recording backend used after the test releases publication.
    /// 测试释放发布后使用的记录后端。
    delegate: Arc<RecordingPersistenceBackend>,
}

impl SessionPersistenceBackend for BlockingPersistBackend {
    /// Blocks publication without retaining the session document mutex.
    /// 在不持有会话文档互斥锁的情况下阻塞发布。
    fn flush(
        &self,
        candidate: &FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        let (state, changed) = &*self.gate;
        let mut state = state.lock().unwrap();
        state.0 = true;
        changed.notify_all();
        while !state.1 {
            state = changed.wait(state).unwrap();
        }
        drop(state);
        self.delegate.flush(candidate)
    }
}

/// Creates one empty V1 session around an explicit deterministic backend.
/// 围绕显式确定性后端创建一个空 V1 会话。
fn published_handle(backend: Arc<dyn SessionPersistenceBackend>) -> Arc<SessionHandle> {
    let session_id = SessionId::new_v4();
    let document = SessionDocument::new(session_id.to_string(), "project-1", "title", 1);
    SessionHandle::with_backend(session_id, document, backend)
}

/// Creates one recording backend and its session handle.
/// 创建一个记录后端及其会话句柄。
fn recording_handle() -> (Arc<RecordingPersistenceBackend>, Arc<SessionHandle>) {
    let backend = Arc::new(RecordingPersistenceBackend::default());
    let handle = published_handle(backend.clone());
    (backend, handle)
}

/// Builds one explicit client submission with a stable test idempotency identity.
/// 构建一条带稳定测试幂等身份的显式客户端输入。
fn test_client_input(action_id: &str, text: &str) -> ClientInputSubmission {
    ClientInputSubmission::new(
        ClientActionId::try_new(action_id).expect("valid test client action id"),
        text,
        Vec::new(),
    )
}

/// Queue and draft wrappers publish each committed Core revision exactly once and preserve no-op receipts.
/// 队列与草稿包装器会把每个已提交的 Core 修订精确发布一次，并保留空操作回执。
#[test]
fn queue_and_draft_wrappers_flush_only_committed_revisions() {
    let (recording, handle) = recording_handle();
    let first = handle
        .enqueue_input(test_client_input("registry-queue-first", "first"), 2)
        .expect("enqueue first input");
    let second = handle
        .enqueue_input(test_client_input("registry-queue-second", "second"), 3)
        .expect("enqueue second input");
    let first_identity = QueuedInputIdentity {
        client_action_id: first.value.client_action_id,
        input_id: first.value.input_id,
    };
    let second_identity = QueuedInputIdentity {
        client_action_id: second.value.client_action_id,
        input_id: second.value.input_id,
    };
    assert_eq!(recording.flush_count(), 2);

    let reordered = handle
        .reorder_queued_inputs(vec![second_identity.clone(), first_identity.clone()], 4)
        .expect("reorder queued inputs");
    assert!(reordered.changed);
    let repeated_order = handle
        .reorder_queued_inputs(vec![second_identity.clone(), first_identity.clone()], 5)
        .expect("repeat exact queue order");
    assert!(!repeated_order.changed);
    assert_eq!(recording.flush_count(), 3);

    let moved = handle
        .move_queued_input_to_draft(second_identity.clone(), 6)
        .expect("move queued input to draft");
    assert!(moved.changed);
    let repeated_move = handle
        .move_queued_input_to_draft(second_identity, 7)
        .expect("repeat exact move");
    assert!(!repeated_move.changed);
    assert_eq!(repeated_move.value, moved.value);
    assert_eq!(recording.flush_count(), 4);

    let removed = handle
        .remove_queued_input(first_identity.clone(), 8)
        .expect("remove queued input");
    assert!(removed.changed);
    let repeated_remove = handle
        .remove_queued_input(first_identity, 9)
        .expect("repeat exact removal");
    assert!(!repeated_remove.changed);
    assert_eq!(repeated_remove.value, removed.value);
    assert_eq!(recording.flush_count(), 5);
    let (memory_revision, durable_revision, queue_len, draft_text) = handle
        .read(|document| {
            (
                document.memory_revision(),
                document.durable_revision(),
                document.metadata.input_state.queued_inputs.len(),
                document.metadata.input_state.draft.text.clone(),
            )
        })
        .expect("read final queue and draft state");
    assert_eq!(memory_revision, durable_revision);
    assert_eq!(queue_len, 0);
    assert_eq!(draft_text, "second");
}

/// Builds one exact Provider preparation input from the current authoritative document.
/// 从当前权威文档构建一份精确 Provider 准备输入。
fn test_prepare_api_input(
    document: &SessionDocument,
    purpose: ApiPurposeKind,
    request: ApiRequestSnapshot,
    now_ms: i64,
) -> PrepareAndStartApiInput {
    let through_seq = document
        .events()
        .last()
        .map(|event| event.seq)
        .expect("test provider projection requires one root event");
    PrepareAndStartApiInput {
        purpose,
        request,
        request_budget_evidence: test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: document.segment_generation(),
            document_revision: document.memory_revision(),
            retained_from_seq: Seq::try_new(1).expect("valid retained seq"),
            through_seq,
            fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                .expect("valid provider-visible fingerprint"),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms,
    }
}

/// Creates the explicit initialized-empty test baseline and starts one physical API.
/// 创建显式“已初始化但来源为空”的测试基线，并启动一次物理 API。
fn start_initialized_test_api(handle: &SessionHandle) -> ApiCallId {
    handle
        .submit_root_input(test_client_input("registry-start-api-root", "root"), 2)
        .expect("submit test root input");
    let input = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                4,
            )
        })
        .expect("build test API preparation");
    handle
        .prepare_and_start_api_durable(input)
        .expect("start initialized test API")
        .value
        .api_call_id
}

/// The physical Provider permit consumes active request facts and leaves newer picker intent pending.
/// 物理 Provider 许可只消费活动请求事实，并让更新的选择器意图继续处于待生效状态。
#[tokio::test]
async fn physical_provider_permit_uses_active_thinking_without_pending_promotion() {
    // User admission activates both historical encodings; later Provider preparation consumes
    // that active value while preserving a newer picker intent.
    // 用户准入会激活两种历史编码；后续 Provider 准备消费该活动值并保留更新的选择器意图。
    for (stored_level, request_level) in [
        ("max", Some("max")),
        ("off", None),
        ("xhigh", Some("x-high")),
        ("x-high", Some("xhigh")),
    ] {
        let temporary = tempfile::tempdir().expect("temporary session root");
        let session_id = SessionId::new_v4();
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
                .expect("create exact V1 session fixture");
        let loaded = load_session_v1_full(&locator).expect("load complete initial session");
        let handle = SessionHandle::new(session_id, locator, loaded.document);
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.metadata.config.active_request = ActiveRequestConfig {
                        provider: Some(String::from("provider-1")),
                        connection_id: Some(String::from("connection-1")),
                        model: Some(String::from("model-1")),
                        thinking_level: Some(String::from("high")),
                        temperature: Some(0.2),
                    };
                    let changed = document.set_pending_request(
                        PendingRequestConfig {
                            provider: Some(String::from("provider-1")),
                            connection_id: Some(String::from("connection-1")),
                            model: Some(String::from("model-1")),
                            thinking_level: Some(String::from(stored_level)),
                            temperature: Some(0.2),
                            set_at_ms: 2,
                        },
                        2,
                    )?;
                    assert!(changed, "pending request fixture must change metadata");
                    Ok(SessionMutationChange::Changed(()))
                },
            )
            .expect("publish active and pending request fixture");
        handle
            .submit_root_input(
                test_client_input("registry-pending-thinking-root", "root"),
                3,
            )
            .expect("submit root input");
        handle
            .mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| {
                    document.set_pending_request(
                        PendingRequestConfig {
                            provider: Some(String::from("future-provider")),
                            connection_id: Some(String::from("future-connection")),
                            model: Some(String::from("future-model")),
                            thinking_level: Some(String::from("low")),
                            temperature: Some(0.3),
                            set_at_ms: 4,
                        },
                        4,
                    )?;
                    Ok(SessionMutationChange::Changed(()))
                },
            )
            .expect("publish a newer picker intent after user admission");
        let fingerprint_sha256 = handle
            .read(|document| {
                let through_seq = document.events().last().expect("root event").seq();
                provider_visible_fingerprint(document.events(), through_seq)
            })
            .expect("read provider projection")
            .expect("fingerprint provider projection");
        let request = ApiRequestSnapshot {
            provider: String::from("provider-1"),
            profile: None,
            model: String::from("model-1"),
            connection_id: Some(String::from("connection-1")),
            thinking_level: request_level.map(String::from),
            temperature: Some(0.2),
        };

        // Simulate stale construction for every active request control.
        // 模拟每一种活动请求控制项上的陈旧构建。
        let revision_before = handle
            .read(|document| document.memory_revision())
            .expect("read pre-preparation revision");
        let event_count_before = handle
            .read(|document| document.events().len())
            .expect("read pre-preparation event count");
        for field in 0..5 {
            let mut stale = request.clone();
            match field {
                0 => stale.provider = String::from("old-provider"),
                1 => stale.connection_id = Some(String::from("old-connection")),
                2 => stale.model = String::from("old-model"),
                3 => stale.thinking_level = Some(String::from("high")),
                _ => stale.temperature = Some(0.1),
            }
            let receipt = handle
                .prepare_physical_api_durable::<(), _>(
                    ApiPurposeKind::MainGeneration,
                    stale,
                    LockedBaselineProjection::empty(),
                    None,
                    4,
                    |_, _preparation| async {
                        panic!("stale candidate must be rebuilt before projection or promotion")
                    },
                )
                .await
                .expect("stale selection returns a rebuild without starting an API");
            assert!(receipt.is_none());
            handle
                .read(|document| {
                    assert_eq!(document.memory_revision(), revision_before);
                    assert!(document.metadata.config.pending_request.is_some());
                    assert!(document.active_api().is_none());
                    assert_eq!(document.events().len(), event_count_before);
                })
                .expect("verify unchanged stale-request state");
        }

        let receipt = handle
            .prepare_physical_api_durable(
                ApiPurposeKind::MainGeneration,
                request.clone(),
                LockedBaselineProjection::empty(),
                None,
                4,
                move |snapshot, _preparation| async move {
                    Ok(Some(SessionPreparedPhysicalRequest {
                        user_prompt_media: Vec::new(),
                        transport: (),
                        projection: ProviderProjectionProof {
                            segment_generation: snapshot.segment_generation,
                            document_revision: snapshot.document_revision,
                            retained_from_seq: snapshot.retained_from_seq,
                            through_seq: snapshot.through_seq,
                            fingerprint_sha256,
                        },
                        request_budget_evidence: test_provider_request_budget_evidence(),
                    }))
                },
            )
            .await
            .expect("prepare physical API from the active user-boundary selection")
            .expect("candidate matches the active selection");

        assert!(receipt.changed);
        handle
            .read(|document| {
                let pending = document
                    .metadata
                    .config
                    .pending_request
                    .as_ref()
                    .expect("newer picker intent must remain pending");
                assert_eq!(pending.model.as_deref(), Some("future-model"));
                assert_eq!(
                    document
                        .metadata
                        .config
                        .active_request
                        .thinking_level
                        .as_deref(),
                    Some(stored_level)
                );
                assert!(document.events().iter().any(|event| {
                    matches!(
                        &event.event,
                        EventBody::ApiStarted(started)
                            if started.api_call_id == receipt.value.core_start.api_call_id
                                && started.request == request
                    )
                }));
            })
            .expect("read active request facts");
        assert!(
            handle
                .read(|document| document.memory_revision() == document.durable_revision())
                .expect("read durable revision")
        );
    }
}

/// Builds one retryable network terminal that can end before transport observation.
/// 构建一份可重试的网络终态，用于表达传输观测前结束。
fn retryable_network_completion(now_ms: i64) -> ApiCompletion {
    ApiCompletion {
        request: ApiRequestSnapshot::new("openai", "gpt-5"),
        usage: UsageSnapshot::default(),
        status: ApiCallStatus::Failed,
        end_reason: Some(ApiEndReason::NetworkError),
        error: Some(RedactedApiError {
            category: ApiErrorCategory::Network,
            error_code: None,
            message: String::from("redacted pre-wire network failure"),
        }),
        continuation_eligibility: ContinuationEligibility::Allowed,
        timing: ApiTimings {
            completed_at_ms: Some(now_ms),
            ..ApiTimings::default()
        },
        provider_source: None,
        completed_at_ms: now_ms,
    }
}

/// Admits one Continue action after a retryable terminal on the initial physical API.
/// 在初始物理 API 的可重试终态之后准入一个 Continue 动作。
async fn admit_test_continue_action(
    handle: &Arc<SessionHandle>,
    action_id: &str,
) -> ClientActionId {
    let initial_api_call_id = start_initialized_test_api(handle);
    Arc::clone(handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(initial_api_call_id, retryable_network_completion(5))
            },
        )
        .await
        .expect("complete initial API before Continue");
    let action_id = ClientActionId::try_new(action_id).expect("valid Continue action id for test");
    handle
        .continue_turn(
            ContinuationRequest::new(action_id.clone(), "", Vec::new()),
            6,
        )
        .expect("admit Continue action");
    action_id
}

/// Builds one exact Continue API preparation from the current authoritative test document.
/// 从当前权威测试文档构建一份精确 Continue API 准备输入。
fn test_continue_prepare_input(
    handle: &SessionHandle,
    action_id: &ClientActionId,
) -> PrepareAndStartApiInput {
    let mut input = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::Continue,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                7,
            )
        })
        .expect("build Continue API preparation");
    input.continuation_action_id = Some(action_id.clone());
    input
}

/// Async durable mutation returns only after the captured candidate is published.
/// 异步可靠修改仅在捕获候选完成发布后返回。
#[tokio::test]
async fn async_durable_mutation_publishes_before_return() {
    let (recording, handle) = recording_handle();
    Arc::clone(&handle)
        .mutate_durable_async(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document.submit_root_input(test_client_input("registry-root", "root"), 2),
        )
        .await
        .expect("async durable mutation");

    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .expect("read revisions");
    assert_eq!(memory_revision, durable_revision);
    assert!(
        recording
            .persisted_events()
            .iter()
            .any(|event| matches!(&event.event, EventBody::UserInput(_)))
    );
}

/// The process-local Continue owner excludes a second physical dispatch until release.
/// 进程内 Continue 所有者在释放前排除第二次物理分派。
#[tokio::test]
async fn continuation_dispatch_guard_is_single_flight_per_session() {
    let (_recording, handle) = recording_handle();
    let first = handle.acquire_continuation_dispatch().await;
    let acquired = Arc::new(AtomicUsize::new(0));
    let follower_handle = Arc::clone(&handle);
    let follower_acquired = Arc::clone(&acquired);
    let follower = tokio::spawn(async move {
        let _guard = follower_handle.acquire_continuation_dispatch().await;
        follower_acquired.store(1, Ordering::SeqCst);
    });
    tokio::task::yield_now().await;
    assert_eq!(acquired.load(Ordering::SeqCst), 0);

    drop(first);
    follower.await.expect("follower dispatch owner");
    assert_eq!(acquired.load(Ordering::SeqCst), 1);
}

/// A durable pre-wire terminal wakes the detached Continue owner and releases its gate.
/// 可靠的传输前终态会唤醒分离 Continue owner，并释放其门锁。
#[tokio::test]
async fn terminal_before_transport_wakes_continue_owner_and_releases_gate() {
    let (_recording, handle) = recording_handle();
    let initial_api_call_id = start_initialized_test_api(&handle);
    Arc::clone(&handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(initial_api_call_id, retryable_network_completion(5))
            },
        )
        .await
        .expect("complete initial API before Continue");

    let action_id =
        ClientActionId::try_new("continue-pre-wire-terminal").expect("valid Continue action id");
    handle
        .continue_turn(
            ContinuationRequest::new(action_id.clone(), "", Vec::new()),
            6,
        )
        .expect("admit Continue action");
    let mut prepare_input = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::Continue,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                7,
            )
        })
        .expect("build Continue API preparation");
    prepare_input.continuation_action_id = Some(action_id.clone());
    let continued_api_call_id = handle
        .prepare_and_start_api_durable(prepare_input)
        .expect("prepare Continue API")
        .value
        .api_call_id;

    let durable_edge = handle.continuation_dispatch_notify.notified();
    tokio::pin!(durable_edge);
    durable_edge.as_mut().enable();
    let waiter_handle = Arc::clone(&handle);
    let waiter_action_id = action_id.clone();
    let waiter_started = Arc::new(Notify::new());
    let waiter_started_signal = Arc::clone(&waiter_started);
    let waiter = tokio::spawn(async move {
        let _guard = waiter_handle.acquire_continuation_dispatch().await;
        waiter_started_signal.notify_one();
        waiter_handle
            .wait_until_continuation_dispatch_settled(&waiter_action_id)
            .await
    });
    waiter_started.notified().await;
    tokio::task::yield_now().await;

    Arc::clone(&handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(continued_api_call_id, retryable_network_completion(8))
            },
        )
        .await
        .expect("persist terminal before transport observation");

    tokio::time::timeout(Duration::from_secs(1), durable_edge)
        .await
        .expect("durable terminal must notify registered waiters");
    let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("detached Continue waiter must settle")
        .expect("detached Continue waiter task must join")
        .expect_err("pre-wire terminal must not report transport observation");
    assert!(matches!(
        waiter_error,
        SessionRuntimeError::ContinuationTerminatedBeforeTransport {
            client_action_id,
            api_call_id,
            projection: ContinuationTerminalProjection::InlineNotice { .. },
        } if client_action_id == action_id && api_call_id == continued_api_call_id
    ));

    let _next_owner = tokio::time::timeout(
        Duration::from_secs(1),
        handle.acquire_continuation_dispatch(),
    )
    .await
    .expect("terminal waiter must release the per-session Continue gate");
}

/// A durable pre-API Turn terminal wakes the Continue waiter without waiting for its deadline.
/// 可靠的 API 前回合终态会唤醒 Continue 等待器，而不会等待截止时间。
#[tokio::test]
async fn terminal_before_api_wakes_continue_waiter_immediately() {
    // handle owns the real durable V1 document observed by both mutation and waiter paths.
    // handle 持有修改与等待路径共同观察的真实可靠 V1 文档。
    let (_recording, handle) = recording_handle();
    // action_id identifies the admitted Continue attempt that will fail before API preparation.
    // action_id 标识将在 API 准备前失败的已准入 Continue 尝试。
    let action_id = admit_test_continue_action(&handle, "continue-pre-api-terminal").await;
    // waiter_handle transfers the same registered document owner into the asynchronous waiter.
    // waiter_handle 将同一已注册文档所有者传入异步等待器。
    let waiter_handle = Arc::clone(&handle);
    // waiter_action_id transfers the exact browser identity without borrowing the test stack.
    // waiter_action_id 传递精确浏览器身份且不借用测试栈。
    let waiter_action_id = action_id.clone();
    // waiter starts before the terminal mutation so notification timing is exercised rather than replay only.
    // waiter 在终态修改前启动，从而验证通知时序而不只是重放。
    let waiter = tokio::spawn(async move {
        waiter_handle
            .wait_for_continuation_transport_observed(&waiter_action_id, Duration::from_secs(30))
            .await
    });

    Arc::clone(&handle)
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                Ok(
                    if document.fail_turn_without_active_api(
                        "stream_finished_without_v1_api_terminal",
                        Some("controller seed was unavailable"),
                        7,
                    )? {
                        SessionMutationChange::Changed(())
                    } else {
                        SessionMutationChange::Unchanged(())
                    },
                )
            },
        )
        .await
        .expect("persist Continue pre-API terminal");

    // waiter_error must arrive from the durable terminal notification well before the configured deadline.
    // waiter_error 必须由可靠终态通知在配置截止时间前及时返回。
    let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("durable pre-API terminal must wake the Continue waiter")
        .expect("Continue waiter task must join")
        .expect_err("pre-API terminal must not report transport observation");
    assert!(matches!(
        waiter_error,
        SessionRuntimeError::ContinuationTerminatedBeforeApi {
            client_action_id,
            projection: ContinuationTerminalProjection::InlineNotice { .. },
        } if client_action_id == action_id
    ));
}

/// An in-memory-only terminal stays invisible until its exact candidate becomes durable.
/// 仅存在于内存的终态在其精确候选可靠落盘前保持不可见。
#[tokio::test]
async fn continuation_waiter_never_exposes_in_memory_only_terminal() {
    let (_recording, handle) = recording_handle();
    let initial_api_call_id = start_initialized_test_api(&handle);
    Arc::clone(&handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(initial_api_call_id, retryable_network_completion(5))
            },
        )
        .await
        .expect("complete initial API before Continue");
    let action_id =
        ClientActionId::try_new("continue-in-memory-terminal").expect("valid Continue action id");
    handle
        .continue_turn(
            ContinuationRequest::new(action_id.clone(), "", Vec::new()),
            6,
        )
        .expect("admit Continue action");
    let mut prepare_input = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::Continue,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                7,
            )
        })
        .expect("build Continue API preparation");
    prepare_input.continuation_action_id = Some(action_id.clone());
    let continued_api_call_id = handle
        .prepare_and_start_api_durable(prepare_input)
        .expect("prepare Continue API")
        .value
        .api_call_id;
    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(continued_api_call_id, retryable_network_completion(8))
            },
        )
        .expect("append terminal without physical flush");

    let before_flush = handle
        .wait_for_continuation_transport_observed(&action_id, Duration::from_millis(25))
        .await
        .expect_err("in-memory-only terminal must remain invisible");
    assert!(matches!(
        before_flush,
        SessionRuntimeError::ContinuationTransportObservationTimedOut {
            client_action_id,
        } if client_action_id == action_id
    ));

    handle
        .flush_all()
        .expect("publish exact terminal candidate");
    let after_flush = handle
        .wait_until_continuation_dispatch_settled(&action_id)
        .await
        .expect_err("durable terminal must settle without transport observation");
    assert!(matches!(
        after_flush,
        SessionRuntimeError::ContinuationTerminatedBeforeTransport {
            client_action_id,
            api_call_id,
            projection: ContinuationTerminalProjection::InlineNotice { .. },
        } if client_action_id == action_id && api_call_id == continued_api_call_id
    ));
}

/// A failed Continue terminal flush releases waiters, while admission retry exposes terminal.
/// Continue 终态 flush 失败会释放等待者，而准入重试会暴露该终态。
#[tokio::test]
async fn continuation_terminal_flush_failure_releases_gate_and_retry_clears_failure() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(0),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend.clone());
    let initial_api_call_id = start_initialized_test_api(&handle);
    Arc::clone(&handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(initial_api_call_id, retryable_network_completion(5))
            },
        )
        .await
        .expect("complete initial API before Continue");
    let action_id = ClientActionId::try_new("continue-terminal-flush-failure")
        .expect("valid Continue action id");
    handle
        .continue_turn(
            ContinuationRequest::new(action_id.clone(), "", Vec::new()),
            6,
        )
        .expect("admit Continue action");
    let mut prepare_input = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::Continue,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                7,
            )
        })
        .expect("build Continue API preparation");
    prepare_input.continuation_action_id = Some(action_id.clone());
    let continued_api_call_id = handle
        .prepare_and_start_api_durable(prepare_input)
        .expect("prepare Continue API")
        .value
        .api_call_id;

    let waiter_handle = Arc::clone(&handle);
    let waiter_action_id = action_id.clone();
    let waiter_started = Arc::new(Notify::new());
    let waiter_started_signal = Arc::clone(&waiter_started);
    let waiter = tokio::spawn(async move {
        let _guard = waiter_handle.acquire_continuation_dispatch().await;
        waiter_started_signal.notify_one();
        waiter_handle
            .wait_until_continuation_dispatch_settled(&waiter_action_id)
            .await
    });
    waiter_started.notified().await;
    backend.remaining_failures.store(1, Ordering::SeqCst);
    let flush_error = Arc::clone(&handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(continued_api_call_id, retryable_network_completion(8))
            },
        )
        .await
        .expect_err("injected terminal flush must fail");
    assert!(matches!(flush_error, SessionRuntimeError::Storage(_)));
    let failed_revision = handle
        .read(SessionDocument::memory_revision)
        .expect("read failed candidate revision");
    let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("failed persistence must wake detached waiter")
        .expect("detached waiter task must join")
        .expect_err("failed persistence cannot report a durable settlement");
    assert!(matches!(
        waiter_error,
        SessionRuntimeError::ContinuationDispatchPersistenceFailed {
            client_action_id,
            candidate_revision,
            ..
        } if client_action_id == action_id && candidate_revision == failed_revision
    ));
    let bounded_error = handle
        .wait_for_continuation_transport_observed(&action_id, Duration::from_secs(1))
        .await
        .expect_err("bounded waiter must see the same failed attempt");
    assert!(matches!(
        bounded_error,
        SessionRuntimeError::ContinuationDispatchPersistenceFailed {
            client_action_id,
            candidate_revision,
            ..
        } if client_action_id == action_id && candidate_revision == failed_revision
    ));
    let next_owner = tokio::time::timeout(
        Duration::from_secs(1),
        handle.acquire_continuation_dispatch(),
    )
    .await
    .expect("failed waiter must release the per-session Continue gate");
    drop(next_owner);

    let durable_error = handle
        .reconcile_continuation_dispatch_for_action(&action_id)
        .expect_err("admission retry must expose durable terminal before dispatch");
    assert!(matches!(
        durable_error,
        SessionRuntimeError::ContinuationTerminatedBeforeTransport {
            client_action_id,
            api_call_id,
            projection: ContinuationTerminalProjection::InlineNotice { .. },
        } if client_action_id == action_id && api_call_id == continued_api_call_id
    ));
    assert!(recording.flush_count() >= 6);
}

/// Quarantine wakes one detached Continue waiter and releases its session gate.
/// 隔离会唤醒一个分离 Continue 等待者并释放其会话门。
#[tokio::test]
async fn quarantine_wakes_continue_waiter_and_releases_gate() {
    let (_recording, handle) = recording_handle();
    let action_id =
        ClientActionId::try_new("continue-quarantine").expect("valid Continue action id");
    let quarantine_edge = handle.continuation_dispatch_notify.notified();
    tokio::pin!(quarantine_edge);
    quarantine_edge.as_mut().enable();
    let waiter_handle = Arc::clone(&handle);
    let waiter_action_id = action_id.clone();
    let waiter = tokio::spawn(async move {
        let _guard = waiter_handle.acquire_continuation_dispatch().await;
        waiter_handle
            .wait_until_continuation_dispatch_settled(&waiter_action_id)
            .await
    });
    let reason = SessionQuarantineReason::MutationRevisionMismatch {
        before_revision: 3,
        after_revision: 5,
    };
    handle
        .enter_quarantine(reason.clone())
        .expect("enter deterministic quarantine");
    tokio::time::timeout(Duration::from_secs(1), quarantine_edge)
        .await
        .expect("quarantine transition must notify registered waiters");
    let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("quarantine must settle detached waiter")
        .expect("detached waiter task must join")
        .expect_err("quarantine cannot produce a dispatch receipt");
    assert!(matches!(
        waiter_error,
        SessionRuntimeError::SessionQuarantined {
            session_id,
            reason: actual_reason,
        } if session_id == handle.session_id && actual_reason == reason
    ));
    let _next_owner = tokio::time::timeout(
        Duration::from_secs(1),
        handle.acquire_continuation_dispatch(),
    )
    .await
    .expect("quarantined waiter must release the per-session Continue gate");
}

/// A rejected post-commit receipt quarantines the resident owner and releases Continue waiters.
/// 被拒绝的提交后回执会隔离常驻 owner，并释放 Continue 等待者。
#[tokio::test]
async fn post_commit_receipt_rejection_quarantines_and_releases_continue_gate() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(CorruptPublishedMetadataBackend {
        corrupt_next: AtomicBool::new(false),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend.clone());
    let action_id =
        admit_test_continue_action(&handle, "continue-post-commit-receipt-rejected").await;
    let continued_api_call_id = handle
        .prepare_and_start_api_durable(test_continue_prepare_input(&handle, &action_id))
        .expect("prepare Continue API")
        .value
        .api_call_id;
    let waiter_handle = Arc::clone(&handle);
    let waiter_action_id = action_id.clone();
    let waiter_started = Arc::new(Notify::new());
    let waiter_started_signal = Arc::clone(&waiter_started);
    let waiter = tokio::spawn(async move {
        let _guard = waiter_handle.acquire_continuation_dispatch().await;
        waiter_started_signal.notify_one();
        waiter_handle
            .wait_until_continuation_dispatch_settled(&waiter_action_id)
            .await
    });
    waiter_started.notified().await;
    tokio::task::yield_now().await;

    backend.corrupt_next.store(true, Ordering::SeqCst);
    let completion_error = Arc::clone(&handle)
        .mutate_durable_async(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            move |document| {
                document.complete_api_group(continued_api_call_id, retryable_network_completion(8))
            },
        )
        .await
        .expect_err("foreign post-commit metadata must reject the resident owner");
    assert!(matches!(
        completion_error,
        SessionRuntimeError::Domain(error)
            if matches!(*error, DomainError::ImmutableFieldMutation {
                entity: "metadata",
                field: "session_id",
            })
    ));
    assert!(
        recording.persisted_events().iter().any(|event| matches!(
            &event.event,
            EventBody::ApiStats(stats) if stats.api_call_id == continued_api_call_id
        )),
        "the injected failure must happen after physical candidate acceptance"
    );

    let waiter_error = tokio::time::timeout(Duration::from_secs(1), waiter)
        .await
        .expect("post-commit quarantine must settle detached waiter")
        .expect("detached waiter task must join")
        .expect_err("quarantined owner cannot return a dispatch receipt");
    assert!(matches!(
        waiter_error,
        SessionRuntimeError::SessionQuarantined {
            session_id,
            reason: SessionQuarantineReason::DurableCommitApplicationRejected {
                candidate_revision: _,
                detail: _,
            },
        } if session_id == handle.session_id
    ));
    let _next_owner = tokio::time::timeout(
        Duration::from_secs(1),
        handle.acquire_continuation_dispatch(),
    )
    .await
    .expect("post-commit quarantine must release the per-session Continue gate");
}

/// A failed ApiStarted publication is reconciled before the first physical request is allowed.
/// ApiStarted 发布失败会在允许首次物理请求前被协调落盘。
#[tokio::test]
async fn continuation_api_start_flush_failure_is_retained_until_covering_retry() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(0),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend.clone());
    let action_id = admit_test_continue_action(&handle, "continue-api-start-flush-failure").await;
    let prepare_input = test_continue_prepare_input(&handle, &action_id);
    backend.remaining_failures.store(1, Ordering::SeqCst);
    let prepare_error = handle
        .prepare_and_start_api_durable(prepare_input)
        .expect_err("injected ApiStarted flush must fail");
    assert!(matches!(prepare_error, SessionRuntimeError::Storage(_)));
    assert!(
        handle
            .continuation_transport_observed_for_action(&action_id)
            .expect("query only the durable prefix")
            .is_none(),
        "failed ApiStarted candidate must not appear durable"
    );
    let wait_error = handle
        .wait_for_continuation_transport_observed(&action_id, Duration::from_secs(1))
        .await
        .expect_err("late waiter must receive retained persistence failure");
    assert!(matches!(
        wait_error,
        SessionRuntimeError::ContinuationDispatchPersistenceFailed {
            client_action_id,
            ..
        } if client_action_id == action_id
    ));

    let observed = handle
        .reconcile_continuation_dispatch_for_action(&action_id)
        .expect("retry must reconcile the exact ApiStarted candidate");
    assert!(
        observed.is_none(),
        "an ApiStarted-only retry must still allow the first physical dispatch"
    );
    let after_retry = handle
        .wait_for_continuation_transport_observed(&action_id, Duration::from_millis(25))
        .await
        .expect_err("retried ApiStarted still awaits transport observation");
    assert!(matches!(
        after_retry,
        SessionRuntimeError::ContinuationTransportObservationTimedOut {
            client_action_id,
        } if client_action_id == action_id
    ));
    assert!(recording.flush_count() >= 5);
}

/// A failed transport-observation flush is reconciled before retry can dispatch again.
/// 传输观察 flush 失败会在重试能够再次分派前被协调落盘。
#[tokio::test]
async fn continuation_transport_observation_flush_failure_suppresses_second_dispatch() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(0),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend.clone());
    let action_id =
        admit_test_continue_action(&handle, "continue-transport-observation-flush-failure").await;
    let continued_api_call_id = handle
        .prepare_and_start_api_durable(test_continue_prepare_input(&handle, &action_id))
        .expect("prepare Continue API")
        .value
        .api_call_id;
    let failed_edge = handle.continuation_dispatch_notify.notified();
    tokio::pin!(failed_edge);
    failed_edge.as_mut().enable();
    backend.remaining_failures.store(1, Ordering::SeqCst);
    let observation_error = handle
        .mark_api_transport_observed_durable(continued_api_call_id, 8)
        .await
        .expect_err("injected transport-observation flush must fail");
    assert!(matches!(observation_error, SessionRuntimeError::Storage(_)));
    tokio::time::timeout(Duration::from_secs(1), failed_edge)
        .await
        .expect("failed transport observation must notify registered waiters");
    let waiter_error = handle
        .wait_until_continuation_dispatch_settled(&action_id)
        .await
        .expect_err("failed transport observation cannot return a durable receipt");
    assert!(matches!(
        waiter_error,
        SessionRuntimeError::ContinuationDispatchPersistenceFailed {
            client_action_id,
            ..
        } if client_action_id == action_id
    ));

    let observed = handle
        .reconcile_continuation_dispatch_for_action(&action_id)
        .expect("admission retry must reconcile the exact transport observation")
        .expect("reconciled observation must suppress a second physical dispatch");
    assert_eq!(observed.api_call_id, continued_api_call_id);
    assert_eq!(observed.continuation_action_id, Some(action_id));
    assert!(recording.flush_count() >= 6);
}

/// An unchanged mutation on a clean document succeeds without physical I/O.
/// 干净文档上的未变化修改会成功，且不产生物理 I/O。
#[test]
fn idempotent_durable_mutation_does_not_fabricate_revision_or_flush() {
    let (recording, handle) = recording_handle();
    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |_document| Ok(SessionMutationChange::Unchanged("already-set")),
        )
        .expect("unchanged durable mutation");

    assert_eq!(receipt.value, "already-set");
    assert!(!receipt.changed);
    assert_eq!(receipt.revision, 0);
    assert_eq!(receipt.flush_reason, SessionFlushReason::MetadataChanged);
    assert_eq!(recording.flush_count(), 0);
}

/// A context-window change persists once, while repeating the same node is a clean no-op.
/// 上下文窗口变更只持久化一次，重复选择同一节点是干净的幂等操作。
#[test]
fn context_compaction_window_change_is_durable_and_idempotent() {
    let (recording, handle) = recording_handle();
    let node_id = vulcan_context::ContextWindowSelection::Node(
        "compression-window-4"
            .parse::<vulcan_context::ContextCompactionWindowNodeId>()
            .expect("valid context-window node"),
    );
    let first = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let changed = document.set_context_window_selection(Some(node_id), 2);
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .expect("first context-window change");
    let revision_after_first = first.revision;
    assert!(first.changed);
    assert_eq!(recording.flush_count(), 1);

    let repeated = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let changed = document.set_context_window_selection(Some(node_id), 3);
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .expect("repeated context-window selection");

    assert!(!repeated.changed);
    assert_eq!(repeated.revision, revision_after_first);
    assert_eq!(recording.flush_count(), 1);
    assert_eq!(
        handle
            .read(|document| document.metadata.config.context_window_selection)
            .expect("read context-window node"),
        Some(node_id)
    );
}

/// An output-style change persists once, while the normalized repeated selection is a no-op.
/// 输出风格变更只持久化一次，重复选择同一规范值时保持幂等。
#[test]
fn output_style_change_is_durable_and_idempotent() {
    let (recording, handle) = recording_handle();
    let first = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let changed = document.set_output_style(Some(String::from(" concise ")), 2)?;
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .expect("first output-style change");
    let revision_after_first = first.revision;
    assert!(first.changed);
    assert_eq!(recording.flush_count(), 1);

    let repeated = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let changed = document.set_output_style(Some(String::from("concise")), 3)?;
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .expect("repeated output-style selection");

    assert!(!repeated.changed);
    assert_eq!(repeated.revision, revision_after_first);
    assert_eq!(recording.flush_count(), 1);
    assert_eq!(
        handle
            .read(|document| document.metadata.config.selection.output_style.clone())
            .expect("read output-style selection")
            .as_deref(),
        Some("concise")
    );
}

/// An unchanged invocation still publishes a revision dirtied beforehand.
/// 未变化调用仍会发布调用前已经变脏的修订号。
#[test]
fn idempotent_durable_mutation_flushes_preexisting_dirty_revision() {
    let (recording, handle) = recording_handle();
    let dirty = handle
        .mutate_in_memory(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document.submit_root_input(test_client_input("registry-root", "root"), 2),
        )
        .expect("create dirty revision");
    assert_eq!(recording.flush_count(), 0);

    let receipt = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |_document| Ok(SessionMutationChange::Unchanged(())),
        )
        .expect("flush existing dirty revision");
    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .expect("read durable revisions");

    assert!(!receipt.changed);
    assert_eq!(receipt.revision, dirty.revision);
    assert_eq!(memory_revision, durable_revision);
    assert_eq!(recording.flush_count(), 1);
}

/// Concurrent unchanged requests share one serialized publication of prior dirt.
/// 并发未变化请求共享一次对既有脏状态的串行发布。
#[test]
fn concurrent_idempotent_mutations_publish_preexisting_dirt_once() {
    let (recording, handle) = recording_handle();
    handle
        .mutate_in_memory(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document.submit_root_input(test_client_input("registry-root", "root"), 2),
        )
        .expect("create shared dirty revision");
    let barrier = Arc::new(Barrier::new(8));
    let mut workers = Vec::new();
    for _ in 0..8 {
        let handle = Arc::clone(&handle);
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            barrier.wait();
            handle.mutate_durable_if_changed(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |_document| Ok(SessionMutationChange::Unchanged(())),
            )
        }));
    }

    for worker in workers {
        let receipt = worker
            .join()
            .expect("join idempotent worker")
            .expect("idempotent worker mutation");
        assert!(!receipt.changed);
    }
    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .expect("read concurrent durable revisions");
    assert_eq!(memory_revision, durable_revision);
    assert_eq!(recording.flush_count(), 1);
}

/// The typed change decision must exactly match the revision effect.
/// 强类型变化判定必须与修订号效果精确一致。
#[test]
fn conditional_mutation_rejects_false_change_claims() {
    let (recording, handle) = recording_handle();
    let result = handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        |_document| Ok(SessionMutationChange::Changed(())),
    );

    assert!(matches!(
        result,
        Err(SessionRuntimeError::ConditionalMutationRevisionMismatch {
            reported_changed: true,
            before_revision: 0,
            after_revision: 0,
        })
    ));
    assert_eq!(recording.flush_count(), 0);
    assert!(matches!(
        handle.flush_all(),
        Err(SessionRuntimeError::SessionQuarantined { .. })
    ));
}

/// A false unchanged claim quarantines memory without persisting the leaked revision.
/// 错误的未变化声明会隔离内存，且不持久化泄漏的修订号。
#[test]
fn conditional_mutation_quarantines_revision_leaked_by_false_unchanged_claim() {
    let (recording, handle) = recording_handle();
    let result = handle.mutate_durable_if_changed(
        SessionMutationAdmission::RootTurn,
        SessionFlushReason::InputAccepted,
        |document| {
            document
                .submit_root_input(test_client_input("registry-leaked-root", "leaked root"), 2)?;
            Ok(SessionMutationChange::Unchanged(()))
        },
    );
    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .expect("read leaked revision durability");

    assert!(matches!(
        result,
        Err(SessionRuntimeError::ConditionalMutationRevisionMismatch {
            reported_changed: false,
            before_revision: 0,
            after_revision: 1,
        })
    ));
    assert_eq!(memory_revision, 1);
    assert_eq!(durable_revision, 0);
    assert_eq!(recording.flush_count(), 0);
    assert!(matches!(
        handle.mutate_in_memory(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |_document| Ok(())
        ),
        Err(SessionRuntimeError::SessionQuarantined { .. })
    ));
}

/// A failure-atomic Core error leaves both revision and storage untouched.
/// 具备失败原子性的 Core 错误不会改变修订号或存储。
#[test]
fn conditional_mutation_preserves_failure_atomic_domain_errors() {
    let (recording, handle) = recording_handle();
    let result = handle.mutate_durable_if_changed(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        |_document| Err::<SessionMutationChange<()>, _>(DomainError::NoActiveApi),
    );
    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .expect("read failure-atomic revisions");

    assert!(matches!(
        result,
        Err(SessionRuntimeError::Domain(error)) if *error == DomainError::NoActiveApi
    ));
    assert_eq!(memory_revision, 0);
    assert_eq!(durable_revision, 0);
    assert_eq!(recording.flush_count(), 0);
}

/// A malformed failing mutation quarantines its changed revision without persisting it.
/// 错误实现的失败修改会隔离其变化修订，且不将其持久化。
#[test]
fn conditional_mutation_quarantines_revision_leaked_before_domain_error() {
    let (recording, handle) = recording_handle();
    let result = handle.mutate_durable_if_changed(
        SessionMutationAdmission::RootTurn,
        SessionFlushReason::InputAccepted,
        |document| {
            document
                .submit_root_input(test_client_input("registry-leaked-root", "leaked root"), 2)?;
            Err::<SessionMutationChange<()>, _>(DomainError::NoActiveApi)
        },
    );
    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .expect("read failed mutation durability");

    assert!(matches!(
        result,
        Err(SessionRuntimeError::FailedMutationProducedRevision {
            before_revision: 0,
            after_revision: 1,
            error,
        }) if *error == DomainError::NoActiveApi
    ));
    assert_eq!(memory_revision, 1);
    assert_eq!(durable_revision, 0);
    assert_eq!(recording.flush_count(), 0);
    assert!(matches!(
        handle.flush_all(),
        Err(SessionRuntimeError::SessionQuarantined { .. })
    ));
}

/// A successful ordinary mutation without one revision quarantines the handle.
/// 成功但未精确推进一个修订的普通修改会隔离句柄。
#[test]
fn ordinary_durable_mutation_quarantines_success_without_revision() {
    let (recording, handle) = recording_handle();
    let result = handle.mutate_durable(
        SessionMutationAdmission::MetadataOnly,
        SessionFlushReason::MetadataChanged,
        |_document| Ok(()),
    );

    assert!(matches!(
        result,
        Err(SessionRuntimeError::MutationRevisionMismatch {
            before_revision: 0,
            after_revision: 0,
        })
    ));
    assert_eq!(recording.flush_count(), 0);
    assert!(matches!(
        handle.flush_all(),
        Err(SessionRuntimeError::SessionQuarantined { .. })
    ));
}

/// An ordinary error that leaks a revision quarantines without publishing or flushing.
/// 泄漏修订号的普通错误会隔离句柄，且不发布、不 flush。
#[test]
fn ordinary_in_memory_error_quarantines_without_publish_or_flush() {
    let (recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    let result = handle.mutate_in_memory(
        SessionMutationAdmission::RootTurn,
        SessionFlushReason::InputAccepted,
        |document| {
            document
                .submit_root_input(test_client_input("registry-leaked-root", "leaked root"), 2)?;
            Err::<(), _>(DomainError::NoActiveApi)
        },
    );

    assert!(matches!(
        result,
        Err(SessionRuntimeError::FailedMutationProducedRevision {
            before_revision: 0,
            after_revision: 1,
            error,
        }) if *error == DomainError::NoActiveApi
    ));
    assert_eq!(recording.flush_count(), 0);
    assert!(sink.events().is_empty());
    assert!(matches!(
        handle.flush_all(),
        Err(SessionRuntimeError::SessionQuarantined { .. })
    ));
}

/// A failure-atomic ordinary domain error does not quarantine the handle.
/// 具备失败原子性的普通领域错误不会隔离句柄。
#[test]
fn ordinary_in_memory_failure_atomic_error_keeps_handle_admissible() {
    let (recording, handle) = recording_handle();
    let result = handle.mutate_in_memory(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::ApiCompleted,
        |_document| Err::<(), _>(DomainError::NoActiveApi),
    );

    assert!(matches!(
        result,
        Err(SessionRuntimeError::Domain(error)) if *error == DomainError::NoActiveApi
    ));
    let root = handle
        .submit_root_input(test_client_input("registry-root", "root"), 2)
        .expect("failure-atomic error must not quarantine handle");
    assert_eq!(root.revision, 1);
    assert_eq!(recording.flush_count(), 1);
}

/// One durable mutation emits a volatile batch and an idempotent durable confirmation.
/// 一次可靠修改先发出 volatile 批次，再发出可幂等的 durable 确认。
#[test]
fn durable_flush_publishes_canonical_live_delta_after_storage_success() {
    let (recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    let registry = SessionRuntimeRegistry::default();
    registry
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    registry.register(Arc::clone(&handle)).unwrap();

    handle
        .submit_root_input(test_client_input("registry-root", "root"), 2)
        .unwrap();

    let events = sink.events();
    let (volatile, durable) = match events.as_slice() {
        [
            ServiceEvent::ConversationEventDeltaV1(volatile),
            ServiceEvent::ConversationEventDeltaV1(durable),
        ] => (volatile, durable),
        actual => panic!("expected volatile and durable V1 deltas, got {actual:?}"),
    };
    let generation = handle
        .read(SessionDocument::segment_generation)
        .expect("read segment generation");
    assert_eq!(
        volatile.batch,
        ConversationEventWindowBatch::from_events(
            handle.session_id().to_string(),
            generation,
            &recording.persisted_events(),
        )
    );
    assert_eq!(
        volatile.durability,
        ConversationEventPatchDurabilityV1::Volatile
    );
    assert_eq!(
        durable.durability,
        ConversationEventPatchDurabilityV1::Durable
    );
    assert_eq!(durable.revision, volatile.revision);
    assert_eq!(durable.batch.session_id, volatile.batch.session_id);
    assert_eq!(
        durable.batch.window_generation,
        volatile.batch.window_generation
    );
    assert!(durable.batch.events.is_empty());
}

/// One edit generation atomically replaces the resident owner and publishes a rebase signal.
/// 一次编辑 generation 会原子替换常驻 owner，并发布 rebase 信号。
#[test]
fn edit_generation_replaces_resident_owner_and_publishes_rebase() {
    let temporary = tempfile::tempdir().expect("temporary session root");
    let session_id = SessionId::new_v4();
    let locator = create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
        .expect("create exact V1 session fixture");
    let loaded = load_session_v1_full(&locator).expect("load complete initial session");
    let handle = SessionHandle::new(session_id, locator.clone(), loaded.document);
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .expect("install live event emitter");
    handle
        .submit_root_input(test_client_input("edit-root", "before edit"), 2)
        .expect("publish durable root input");
    assert!(
        crate::session_request_config_v1::set_pending_reasoning_request_v1(
            handle.as_ref(),
            vulcan_provider_config::config::ReasoningEffortLevel::Adaptive,
            crate::session_request_config_v1::SessionRequestFallback {
                provider: Some(String::from("minimax")),
                connection_id: Some(String::from("minimax-account")),
                model: Some(String::from("MiniMax-M2.7-highspeed")),
                temperature: None,
            },
            3,
        )
        .expect("stage the exact next-generation reasoning selection")
    );
    let target_seq = handle
        .read(|document| {
            document
                .events()
                .iter()
                .find(|event| matches!(&event.event, EventBody::UserInput(_)))
                .map(|event| event.seq)
                .expect("root input seq")
        })
        .expect("read root input seq");
    let prior_revision = handle
        .read(SessionDocument::memory_revision)
        .expect("read prior revision");
    sink.clear();

    let receipt = handle
        .publish_edit_generation(
            1,
            EditGenerationInput {
                target_seq,
                replacement: RootInputReplacement::new("after edit", Vec::new()),
                now_ms: 4,
            },
        )
        .expect("publish edited generation");

    assert_eq!(receipt.target_seq, target_seq);
    assert_eq!(receipt.segment_generation, 2);
    assert_eq!(receipt.document_revision, prior_revision + 1);
    handle
            .read(|document| {
                assert_eq!(document.segment_generation(), 2);
                assert_eq!(document.memory_revision(), receipt.document_revision);
                assert_eq!(document.durable_revision(), receipt.document_revision);
                assert_eq!(
                    document.runtime_lock.initialization_state,
                    vulcan_session_core::v1::context::RuntimeInitializationState::BaselineRefreshPending
                );
                let EventBody::UserInput(input) =
                    &document.events()[target_seq.value() as usize - 1].event
                else {
                    panic!("target seq must remain a root user input");
                };
                assert_eq!(input.text, "after edit");
                assert!(document.metadata.config.pending_request.is_none());
                // active is already frozen by the durable edit, before any physical dispatch.
                // active 已由可靠编辑冻结，先于任何物理分发。
                let active = &document.metadata.config.active_request;
                assert_eq!(active.provider.as_deref(), Some("minimax"));
                assert_eq!(
                    active.model.as_deref(),
                    Some("MiniMax-M2.7-highspeed")
                );
                assert_eq!(active.connection_id.as_deref(), Some("minimax-account"));
                assert_eq!(active.thinking_level.as_deref(), Some("adaptive"));
            })
            .expect("inspect replaced resident generation");
    let durable = load_session_v1_full(&locator).expect("strict reload edited generation");
    assert_eq!(durable.document.segment_generation(), 2);
    // The reopened root survives strict reload as Running, so its successful answer can close it.
    // 重新打开的根回合经严格重载后仍为运行中，其成功回答因此可以关闭回合。
    assert_eq!(
        durable.document.collaboration_run_control(),
        vulcan_session_core::v1::collaboration::CollaborationRunControl::Running
    );
    assert_eq!(
        durable.document.memory_revision(),
        receipt.document_revision
    );
    assert_eq!(
        durable
            .document
            .metadata
            .config
            .active_request
            .thinking_level
            .as_deref(),
        Some("adaptive")
    );
    let events = sink.events();
    let delta = match events.as_slice() {
        [ServiceEvent::ConversationEventDeltaV1(delta)] => delta,
        actual => panic!("expected one generation rebase delta, got {actual:?}"),
    };
    assert_eq!(delta.batch.window_generation, 2);
    assert_eq!(delta.base_durable_revision, Some(prior_revision));
    assert_eq!(delta.revision, receipt.document_revision);
    assert_eq!(
        delta.durability,
        ConversationEventPatchDurabilityV1::Durable
    );
    assert!(delta.batch.events.is_empty());

    sink.clear();
    let error = handle
        .publish_edit_generation(
            1,
            EditGenerationInput {
                target_seq,
                replacement: RootInputReplacement::new("stale retry", Vec::new()),
                now_ms: 5,
            },
        )
        .expect_err("stale generation must be rejected");
    assert!(matches!(
        error,
        SessionRuntimeError::GenerationSourceChanged {
            expected: 1,
            actual: 2
        }
    ));
    assert!(sink.events().is_empty());
    assert_eq!(
        handle
            .read(SessionDocument::segment_generation)
            .expect("read unchanged generation"),
        2
    );
}

/// Metadata-only revisions publish empty edges so the next event patch has an exact base.
/// 仅 metadata 修订会发布空边，使下一份事件补丁拥有精确前驱。
#[test]
fn metadata_revision_publishes_empty_edges_before_next_event_patch() {
    let (_recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    let api_call_id = start_initialized_test_api(&handle);
    let prior_revision = handle
        .read(SessionDocument::memory_revision)
        .expect("read revision before metadata mutation");
    sink.clear();

    let metadata = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let changed = document.set_output_style(Some("concise".to_string()), 5)?;
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .expect("publish metadata-only revision");
    let semantic = handle
        .update_reasoning(
            api_call_id,
            "next semantic event",
            AssistantSemanticStatus::Streaming,
            6,
        )
        .expect("publish next event revision");

    assert_eq!(metadata.revision, prior_revision + 1);
    assert_eq!(semantic.revision, prior_revision + 2);
    let deltas = sink
        .events()
        .into_iter()
        .map(|event| match event {
            ServiceEvent::ConversationEventDeltaV1(delta) => delta,
            actual => panic!("expected V1 live delta, got {actual:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(deltas.len(), 3);
    assert_eq!(deltas[0].base_memory_revision, Some(prior_revision));
    assert_eq!(deltas[0].revision, prior_revision + 1);
    assert!(deltas[0].batch.events.is_empty());
    assert_eq!(
        deltas[0].durability,
        ConversationEventPatchDurabilityV1::Volatile
    );
    assert_eq!(deltas[1].revision, prior_revision + 1);
    assert!(deltas[1].batch.events.is_empty());
    assert_eq!(
        deltas[1].durability,
        ConversationEventPatchDurabilityV1::Durable
    );
    assert_eq!(deltas[2].base_memory_revision, Some(prior_revision + 1));
    assert_eq!(deltas[2].revision, prior_revision + 2);
    assert!(!deltas[2].batch.events.is_empty());
}

/// A failed metadata flush preserves its empty volatile edge for a durable retry.
/// metadata flush 失败时保留其空 volatile 边，供后续可靠重试确认。
#[test]
fn failed_metadata_flush_retries_with_empty_durable_ack() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(0),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend.clone());
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    start_initialized_test_api(&handle);
    let prior_revision = handle
        .read(SessionDocument::memory_revision)
        .expect("read revision before failed metadata mutation");
    sink.clear();
    backend.remaining_failures.store(1, Ordering::SeqCst);

    assert!(matches!(
        handle.mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let changed = document.set_output_style(Some("compact".to_string()), 5)?;
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        ),
        Err(SessionRuntimeError::Storage(_))
    ));
    let volatile_only = sink.events();
    assert_eq!(volatile_only.len(), 1);
    assert!(matches!(
        volatile_only.as_slice(),
        [ServiceEvent::ConversationEventDeltaV1(delta)]
            if delta.revision == prior_revision + 1
                && delta.base_memory_revision == Some(prior_revision)
                && delta.batch.events.is_empty()
                && delta.durability == ConversationEventPatchDurabilityV1::Volatile
    ));

    handle.flush_all().expect("retry metadata flush");
    let retried = sink.events();
    assert_eq!(retried.len(), 2);
    assert!(matches!(
        retried.last(),
        Some(ServiceEvent::ConversationEventDeltaV1(delta))
            if delta.revision == prior_revision + 1
                && delta.batch.events.is_empty()
                && delta.durability == ConversationEventPatchDurabilityV1::Durable
    ));
}

/// Repeating an already durable metadata no-op emits no fabricated empty edge.
/// 重复已经可靠落盘的 metadata 空操作不会伪造空边。
#[test]
fn repeated_metadata_noop_emits_no_duplicate_empty_edge() {
    let (_recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let changed = document.set_output_style(Some("concise".to_string()), 2)?;
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .expect("publish first metadata revision");
    sink.clear();

    let repeated = handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                let changed = document.set_output_style(Some("concise".to_string()), 3)?;
                Ok(if changed {
                    SessionMutationChange::Changed(())
                } else {
                    SessionMutationChange::Unchanged(())
                })
            },
        )
        .expect("accept metadata no-op");
    handle.flush_all().expect("flush already durable no-op");

    assert!(!repeated.changed);
    assert!(sink.events().is_empty());
}

/// A failed physical flush preserves the volatile frame and later confirms it durably.
/// 物理 flush 失败时保留 volatile 帧，并在稍后以 durable 方式确认。
#[test]
fn failed_flush_keeps_volatile_delta_before_retry_succeeds() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(1),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend);
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();

    assert!(matches!(
        handle.submit_root_input(test_client_input("registry-root", "root"), 2),
        Err(SessionRuntimeError::Storage(_))
    ));
    assert!(matches!(
        sink.events().as_slice(),
        [ServiceEvent::ConversationEventDeltaV1(_)]
    ));

    handle.flush_all().unwrap();
    let events = sink.events();
    let (volatile, durable) = match events.as_slice() {
        [
            ServiceEvent::ConversationEventDeltaV1(volatile),
            ServiceEvent::ConversationEventDeltaV1(durable),
        ] => (volatile, durable),
        actual => panic!("expected volatile and durable retry deltas, got {actual:?}"),
    };
    assert_eq!(
        volatile.durability,
        ConversationEventPatchDurabilityV1::Volatile
    );
    assert_eq!(
        durable.durability,
        ConversationEventPatchDurabilityV1::Durable
    );
    assert_eq!(durable.revision, volatile.revision);
    assert!(!volatile.batch.events.is_empty());
    assert!(durable.batch.events.is_empty());
}

/// Multiple reasoning deltas publish exact same-seq volatile patches without token I/O.
/// 多次思考增量会发布精确的同 seq volatile 补丁，且不产生逐 token I/O。
#[test]
fn reasoning_deltas_publish_exact_volatile_same_seq_patches() {
    let (recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    let api_call_id = start_initialized_test_api(&handle);
    sink.clear();
    let flush_count_before_stream = recording.flush_count();

    let first = handle
        .update_reasoning(api_call_id, "first", AssistantSemanticStatus::Streaming, 5)
        .expect("publish first reasoning delta");
    // Replay the exact cumulative snapshot and require a true memory no-op.
    // 完全重放同一累计快照，并要求它成为真实的内存 no-op。
    let duplicate = handle
        .update_reasoning(api_call_id, "first", AssistantSemanticStatus::Streaming, 6)
        .expect("accept exact reasoning replay");
    let second = handle
        .update_reasoning(
            api_call_id,
            "first second",
            AssistantSemanticStatus::Completed,
            7,
        )
        .expect("publish completed reasoning delta");

    assert_eq!(first.value, second.value);
    assert_eq!(duplicate.value, first.value);
    assert!(!duplicate.changed);
    assert_eq!(duplicate.revision, first.revision);
    assert!(duplicate.changed_event_seqs.is_empty());
    assert_eq!(recording.flush_count(), flush_count_before_stream);
    let deltas = sink
        .events()
        .into_iter()
        .map(|event| match event {
            ServiceEvent::ConversationEventDeltaV1(delta) => delta,
            actual => panic!("expected V1 live delta, got {actual:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(deltas.len(), 2);
    assert!(
        deltas
            .iter()
            .all(|delta| delta.durability == ConversationEventPatchDurabilityV1::Volatile)
    );
    assert_eq!(deltas[0].batch.events[0].seq, deltas[1].batch.events[0].seq);
    assert_eq!(deltas[0].batch.events[0].event_revision, 1);
    assert_eq!(deltas[1].batch.events[0].event_revision, 2);
    assert!(matches!(
        &deltas[0].batch.events[0].event,
        EventBody::AssistantReasoning(reasoning) if reasoning.text == "first"
    ));
    assert!(matches!(
        &deltas[1].batch.events[0].event,
        EventBody::AssistantReasoning(reasoning) if reasoning.text == "first second"
    ));
}

/// Streaming text extensions publish appends; other changes publish complete events.
/// 流式正文延长发布追加；其他变化发布完整事件。
#[test]
fn streaming_text_extensions_publish_appends_that_rebuild_the_document() {
    let (_recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .expect("install live emitter");
    let api_call_id = start_initialized_test_api(&handle);
    sink.clear();

    let streaming = AssistantSemanticStatus::Streaming;
    // (text, status, expect append, published record diverges first)
    // （正文、状态、是否期望追加、发布记录是否先偏离）
    let steps = [
        ("答", streaming, false, false),
        ("答案😀", streaming, true, false),
        ("答案😀是", streaming, true, false),
        ("答案😀是的", streaming, false, true),
        ("答案😀是的。", streaming, true, false),
        (
            "答案😀是的。",
            AssistantSemanticStatus::Completed,
            false,
            false,
        ),
    ];
    let mut document_states = Vec::with_capacity(steps.len());
    for (index, (text, status, _, diverge)) in steps.iter().enumerate() {
        if *diverge {
            // A published base the document never had must not be extended.
            // 不得在文档从未有过的发布基线上追加。
            let mut published = handle
                .published_streaming_text
                .lock()
                .expect("published record");
            let record = published
                .events
                .values_mut()
                .next()
                .expect("published record");
            match &mut record.event {
                EventBody::AssistantText(text) => text.text = "别的".to_string(),
                actual => panic!("expected assistant text record, got {actual:?}"),
            }
        }
        let seq = handle
            .update_assistant_text(api_call_id, *text, *status, 10 + index as i64)
            .expect("publish assistant text")
            .value;
        let event = handle
            .read(|document| {
                document
                    .events()
                    .iter()
                    .find(|event| event.seq == seq)
                    .expect("document event")
                    .clone()
            })
            .expect("read document");
        document_states.push(
            ConversationEventWindowBatch::from_events(String::new(), 0, &[event])
                .events
                .remove(0),
        );
    }
    assert!(
        handle
            .published_streaming_text
            .lock()
            .expect("published record")
            .events
            .is_empty(),
        "completed text leaves the streaming record"
    );

    let deltas = sink
        .events()
        .into_iter()
        .map(|event| match event {
            ServiceEvent::ConversationEventDeltaV1(delta) => delta,
            actual => panic!("expected V1 live delta, got {actual:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(deltas.len(), steps.len());
    let mut rebuilt: BTreeMap<Seq, ConversationEventSummary> = BTreeMap::new();
    for ((delta, (text, _, expect_append, _)), expected) in
        deltas.iter().zip(steps.iter()).zip(document_states.iter())
    {
        assert_eq!(
            delta.durability,
            ConversationEventPatchDurabilityV1::Volatile
        );
        if *expect_append {
            assert!(
                delta.batch.events.is_empty(),
                "step {text} should be an append"
            );
            assert_eq!(delta.text_appends.len(), 1);
        } else {
            assert!(
                delta.text_appends.is_empty(),
                "step {text} should be complete"
            );
            assert_eq!(delta.batch.events.len(), 1);
        }
        for event in &delta.batch.events {
            rebuilt.insert(event.seq, event.clone());
        }
        for append in &delta.text_appends {
            let extended = append
                .apply(rebuilt.get(&append.seq).expect("append base was published"))
                .expect("append applies to the published base");
            rebuilt.insert(append.seq, extended);
        }
        assert_eq!(
            rebuilt.get(&expected.seq),
            Some(expected),
            "rebuilt event diverged at step {text}"
        );
    }
}

/// An exact assistant-text replay does not publish or flush a synthetic revision.
/// 完全相同的 assistant 正文重放不会发布或 flush 伪修订。
#[test]
fn assistant_text_exact_replay_is_a_memory_noop() {
    // Create one live owner and isolate events emitted after the API start.
    // 创建一个 live owner，并隔离 API 启动之后发布的事件。
    let (recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    let api_call_id = start_initialized_test_api(&handle);
    sink.clear();
    let flush_count_before_stream = recording.flush_count();

    // Commit one cumulative snapshot and replay it byte-for-byte.
    // 提交一份累计快照，并逐字节重放它。
    let first = handle
        .update_assistant_text(api_call_id, "answer", AssistantSemanticStatus::Streaming, 5)
        .expect("publish first assistant snapshot");
    let duplicate = handle
        .update_assistant_text(api_call_id, "answer", AssistantSemanticStatus::Streaming, 6)
        .expect("accept exact assistant replay");

    assert!(first.changed);
    assert!(!duplicate.changed);
    assert_eq!(duplicate.value, first.value);
    assert_eq!(duplicate.revision, first.revision);
    assert!(duplicate.changed_event_seqs.is_empty());
    assert_eq!(recording.flush_count(), flush_count_before_stream);
    assert_eq!(sink.events().len(), 1);
}

/// A running tool grows from no output to structured output on its original seq.
/// 运行中工具会在原始 seq 上从无输出增长为结构化输出。
#[test]
fn running_tool_output_growth_publishes_same_seq_patch() {
    let (_recording, handle) = recording_handle();
    let sink = Arc::new(RecordingEventSink::default());
    handle
        .install_live_event_emitter(ServiceEventEmitter::new(vec![sink.clone()]))
        .unwrap();
    let api_call_id = start_initialized_test_api(&handle);
    sink.clear();
    let tool_call_id = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    RequestStreamId::try_new("registry-running-tool-stream")?,
                    "file",
                    ToolPayload::empty(),
                    Some("provider-tool".to_string()),
                    5,
                )
            },
        )
        .expect("prepare tool")
        .value;
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| document.start_prepared_tool(api_call_id, tool_call_id, 5),
        )
        .expect("start prepared tool");
    let grown_output = ToolPayload::try_new(serde_json::json!({"bytes": 64})).unwrap();
    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.update_running_tool_output(
                    api_call_id,
                    tool_call_id,
                    Some(grown_output.clone()),
                    6,
                )
            },
        )
        .expect("publish running tool output growth");

    let tool_patches = sink
        .events()
        .into_iter()
        .filter_map(|event| match event {
            ServiceEvent::ConversationEventDeltaV1(delta)
                if delta.durability == ConversationEventPatchDurabilityV1::Volatile
                    && matches!(
                        delta.batch.events.first().map(|event| &event.event),
                        Some(EventBody::ToolCall(_))
                    ) =>
            {
                Some(delta)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tool_patches.len(), 3);
    assert!(
        tool_patches
            .iter()
            .all(|patch| { patch.batch.events[0].seq == tool_patches[0].batch.events[0].seq })
    );
    assert!(matches!(
        &tool_patches[0].batch.events[0].event,
        EventBody::ToolCall(tool) if tool.output.is_none()
    ));
    assert!(matches!(
        &tool_patches[2].batch.events[0].event,
        EventBody::ToolCall(tool) if tool.output.as_ref() == Some(&grown_output)
    ));
}

/// Repeating one exact tool terminal returns the persisted Core receipt without a new flush.
/// 重复同一精确工具终态时返回已持久化的 Core 回执，且不产生新的 flush。
#[test]
fn repeated_exact_tool_finish_is_idempotent_at_the_session_handle() {
    let (recording, handle) = recording_handle();
    let api_call_id = start_initialized_test_api(&handle);
    let tool_call_id = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    RequestStreamId::try_new("registry-idempotent-finish-stream")?,
                    "ask",
                    ToolPayload::try_new(serde_json::json!({"question": "confirm"}))?,
                    None,
                    5,
                )
            },
        )
        .expect("prepare tool")
        .value;
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| document.start_prepared_tool(api_call_id, tool_call_id, 5),
        )
        .expect("start prepared tool");
    let output = ToolPayload::try_new(serde_json::json!({"answer": "yes"})).unwrap();
    let first = handle
        .finish_tool(
            api_call_id,
            tool_call_id,
            ToolCompletion {
                output: Some(output.clone()),
                changeset: None,
                status: ToolCallStatus::Completed,
                finished_at_ms: 6,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        )
        .expect("commit first terminal tool result");
    let flushes_after_first = recording.flush_count();
    let repeated = handle
        .finish_tool(
            api_call_id,
            tool_call_id,
            ToolCompletion {
                output: Some(output),
                changeset: None,
                status: ToolCallStatus::Completed,
                finished_at_ms: 99,
                external_refs: Vec::new(),
                generated_resources: Vec::new(),
            },
        )
        .expect("repeat exact terminal tool result");

    assert!(first.changed);
    assert!(!repeated.changed);
    assert_eq!(repeated.value, first.value);
    assert_eq!(repeated.value.finished_at_ms, 6);
    assert_eq!(recording.flush_count(), flushes_after_first);
}

/// Volatile reasoning remains outside the durable candidate used by a restart.
/// volatile 思考不会进入重启所读取的最后一份可靠候选。
#[test]
fn restart_projection_excludes_unflushed_volatile_reasoning() {
    let (recording, handle) = recording_handle();
    let api_call_id = start_initialized_test_api(&handle);
    handle
        .update_reasoning(
            api_call_id,
            "not durable",
            AssistantSemanticStatus::Streaming,
            5,
        )
        .expect("append volatile reasoning");

    assert!(
        handle
            .read(|document| {
                document.events().iter().any(|event| {
                    matches!(
                        &event.event,
                        EventBody::AssistantReasoning(reasoning) if reasoning.text == "not durable"
                    )
                })
            })
            .unwrap()
    );
    assert!(!recording.persisted_events().iter().any(|event| matches!(
        &event.event,
        EventBody::AssistantReasoning(reasoning) if reasoning.text == "not durable"
    )));
}

/// Two failed flushes retain memory; the third persists the newest state.
/// 两次 flush 失败保留内存；第三次把最新状态持久化。
#[test]
fn failed_flush_retries_latest_complete_candidate() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(2),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend);
    let first = handle.mutate_durable(
        SessionMutationAdmission::RootTurn,
        SessionFlushReason::InputAccepted,
        |document| document.submit_root_input(test_client_input("registry-root", "root"), 2),
    );
    assert!(matches!(first, Err(SessionRuntimeError::Storage(_))));
    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                let turn_id = document.active_turn().map(|turn| turn.turn_id);
                document.append_display_note(
                    turn_id,
                    DisplayNoticeKind::SystemError,
                    NoticeSeverity::Info,
                    "latest",
                    None,
                    None,
                    None,
                    None,
                    3,
                )
            },
        )
        .unwrap();
    assert!(matches!(
        handle.flush_all(),
        Err(SessionRuntimeError::Storage(_))
    ));
    handle.flush_all().unwrap();
    let memory_events = handle.read(|document| document.events().to_vec()).unwrap();
    assert_eq!(recording.persisted_events(), memory_events);
}

/// A newer mutation remains dirty when an older captured candidate publishes.
/// 旧候选发布时，更新的内存修改仍保持脏状态。
#[test]
fn mutation_during_flush_remains_dirty_until_next_publication() {
    let gate = Arc::new((Mutex::new((false, false)), Condvar::new()));
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(BlockingPersistBackend {
        gate: Arc::clone(&gate),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend);
    let writer_handle = Arc::clone(&handle);
    let writer = thread::spawn(move || {
        writer_handle.mutate_durable(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document.submit_root_input(test_client_input("registry-root", "root"), 2),
        )
    });

    let (state, changed) = &*gate;
    let mut state = state.lock().unwrap();
    while !state.0 {
        state = changed.wait(state).unwrap();
    }
    drop(state);

    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                let turn_id = document.active_turn().map(|turn| turn.turn_id);
                document.append_display_note(
                    turn_id,
                    DisplayNoticeKind::SystemError,
                    NoticeSeverity::Info,
                    "arrived during flush",
                    None,
                    None,
                    None,
                    None,
                    3,
                )
            },
        )
        .unwrap();

    let mut state = state_for_release(&gate);
    state.1 = true;
    changed.notify_all();
    drop(state);
    writer.join().unwrap().unwrap();

    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .unwrap();
    assert!(memory_revision > durable_revision);
    handle.flush_all().unwrap();
    let memory_events = handle.read(|document| document.events().to_vec()).unwrap();
    assert_eq!(recording.persisted_events(), memory_events);
}

/// Returns the locked gate state used to release a blocked publication.
/// 返回用于释放阻塞发布的已加锁门状态。
fn state_for_release(
    gate: &Arc<(Mutex<(bool, bool)>, Condvar)>,
) -> std::sync::MutexGuard<'_, (bool, bool)> {
    gate.0.lock().unwrap()
}

/// Concurrent tool completions update their original seq without reordering it.
/// 并发工具完成只更新原 seq，不改变其顺序。
#[test]
fn concurrent_tool_completions_preserve_original_seq_order() {
    const TOOL_COUNT: usize = 12;
    let (_recording, handle) = recording_handle();
    handle
        .submit_root_input(test_client_input("registry-root", "root"), 2)
        .expect("submit root input");
    let preparation = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                3,
            )
        })
        .expect("build API preparation");
    let api_call_id = handle
        .prepare_and_start_api_durable(preparation)
        .unwrap()
        .value
        .api_call_id;
    let mut tool_call_ids = Vec::with_capacity(TOOL_COUNT);
    for index in 0..TOOL_COUNT {
        let tool_call_id = handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| {
                    document.prepare_tool(
                        api_call_id,
                        RequestStreamId::try_new(format!("registry-concurrent-tool-{index}"))?,
                        format!("tool-{index}"),
                        ToolPayload::try_new(serde_json::json!({"index": index}))?,
                        Some(format!("provider-{index}")),
                        4 + index as i64,
                    )
                },
            )
            .unwrap()
            .value;
        handle
            .mutate_in_memory(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ToolBoundary,
                |document| {
                    document.start_prepared_tool(api_call_id, tool_call_id, 4 + index as i64)
                },
            )
            .unwrap();
        tool_call_ids.push(tool_call_id);
    }
    handle.flush_all().unwrap();
    let barrier = Arc::new(Barrier::new(TOOL_COUNT));
    let workers = tool_call_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(index, tool_call_id)| {
            let handle = Arc::clone(&handle);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                handle.finish_tool(
                    api_call_id,
                    tool_call_id,
                    ToolCompletion {
                        output: Some(
                            ToolPayload::try_new(serde_json::json!({"completed": index}))
                                .expect("valid concurrent tool output"),
                        ),
                        changeset: None,
                        status: ToolCallStatus::Completed,
                        finished_at_ms: 100 + index as i64,
                        external_refs: Vec::new(),
                        generated_resources: Vec::new(),
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap().unwrap();
    }
    let persisted_tool_facts = handle
        .read(|document| {
            document
                .events()
                .iter()
                .filter_map(|event| match &event.event {
                    EventBody::ToolCall(tool) => Some((event.seq, tool.tool_call_id, tool.status)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap();
    assert_eq!(persisted_tool_facts.len(), TOOL_COUNT);
    assert_eq!(
        persisted_tool_facts
            .iter()
            .map(|(_, tool_call_id, _)| *tool_call_id)
            .collect::<Vec<_>>(),
        tool_call_ids
    );
    assert!(
        persisted_tool_facts
            .iter()
            .all(|(_, _, status)| *status == ToolCallStatus::Completed)
    );
    assert!(
        persisted_tool_facts
            .windows(2)
            .all(|pair| pair[0].0.value() + 1 == pair[1].0.value())
    );
}

/// Shutdown rejects new roots and reopens admission after a failed close.
/// 关闭拒绝新根输入，并在关闭失败后重新开放准入。
#[test]
fn shutdown_admission_is_explicit_and_reversible() {
    let (_recording, handle) = recording_handle();
    handle.begin_shutdown().unwrap();
    let rejected = handle.mutate_in_memory(
        SessionMutationAdmission::RootTurn,
        SessionFlushReason::InputAccepted,
        |document| document.submit_root_input(test_client_input("registry-blocked", "blocked"), 2),
    );
    assert!(matches!(rejected, Err(SessionRuntimeError::ShuttingDown)));
    handle.resume_after_failed_shutdown().unwrap();
    handle
        .mutate_durable(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| {
                document.submit_root_input(test_client_input("registry-accepted", "accepted"), 3)
            },
        )
        .unwrap();
}

/// The registry rejects duplicate owners and returns the exact Arc.
/// 注册表拒绝重复所有者，并返回精确的 Arc。
#[test]
fn registry_owns_exactly_one_handle_per_session() {
    let (_recording, handle) = recording_handle();
    let session_id = handle.session_id();
    let registry = SessionRuntimeRegistry::default();
    registry.register(Arc::clone(&handle)).unwrap();
    assert!(Arc::ptr_eq(&registry.get(session_id).unwrap(), &handle));
    assert!(matches!(
        registry.register(handle),
        Err(SessionRuntimeError::DuplicateSession(id)) if id == session_id
    ));
}

/// Concurrent exact-session loads execute one loader and return one Arc.
/// 并发精确会话加载只执行一个加载器并返回同一个 Arc。
#[test]
fn concurrent_get_or_load_has_one_physical_owner() {
    const CALLER_COUNT: usize = 16;
    let (_recording, handle) = recording_handle();
    let session_id = handle.session_id();
    let registry = Arc::new(SessionRuntimeRegistry::default());
    let load_count = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(CALLER_COUNT));
    let callers = (0..CALLER_COUNT)
        .map(|_| {
            let registry = Arc::clone(&registry);
            let handle = Arc::clone(&handle);
            let load_count = Arc::clone(&load_count);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                registry
                    .get_or_load(session_id, || {
                        load_count.fetch_add(1, Ordering::SeqCst);
                        Ok(handle)
                    })
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    for caller in callers {
        assert!(Arc::ptr_eq(&caller.join().unwrap(), &handle));
    }
    assert_eq!(load_count.load(Ordering::SeqCst), 1);
}

/// A waiting physical loader resumes after metadata-only publication without a false load failure.
/// 等待的物理加载器在纯元数据发布后继续，不产生虚假的加载失败。
#[test]
fn capability_metadata_serializes_cold_save_and_physical_load() {
    // Channels hold the metadata writer at a deterministic pre-publication boundary.
    // 通道将元数据写入器固定在确定性的发布前边界。
    let (_, handle) = recording_handle();
    let session_id = handle.session_id();
    let registry = Arc::new(SessionRuntimeRegistry::default());
    let published = Arc::new(AtomicUsize::new(0));
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let writer_registry = registry.clone();
    let writer_published = published.clone();
    let writer = thread::spawn(move || {
        writer_registry.with_configuration_access(
            session_id,
            |_| panic!("cold configuration must not have a resident owner"),
            || {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                writer_published.store(1, Ordering::SeqCst);
                Ok(())
            },
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let loader_registry = registry.clone();
    let loader_handle = handle.clone();
    let loader = thread::spawn(move || {
        loader_registry.get_or_load(session_id, || {
            assert_eq!(published.load(Ordering::SeqCst), 1);
            Ok(loader_handle)
        })
    });
    // Three references prove the loader acquired the same gate before it is released.
    // 三个引用证明加载器在门控释放前已取得同一门控。
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if Arc::strong_count(&registry.state.lock().unwrap().loading[&session_id]) >= 3 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "loader did not reach configuration gate"
        );
        thread::yield_now();
    }
    release_tx.send(()).unwrap();
    writer.join().unwrap().unwrap();
    assert!(Arc::ptr_eq(&loader.join().unwrap().unwrap(), &handle));
    assert!(registry.state.lock().unwrap().loading.is_empty());
}

/// Failed metadata publication releases admission and never poisons the session registry.
/// 元数据发布失败会释放准入，绝不污染会话注册表。
#[test]
fn capability_metadata_failure_releases_unloaded_slot() {
    let (_, handle) = recording_handle();
    let session_id = handle.session_id();
    let registry = SessionRuntimeRegistry::default();
    assert!(
        registry
            .with_configuration_access::<()>(
                session_id,
                |_| panic!("unexpected resident"),
                || Err(String::from("atomic write failed"))
            )
            .is_err()
    );
    assert!(registry.state.lock().unwrap().loading.is_empty());
    assert_eq!(
        registry
            .with_configuration_access(session_id, |_| panic!("unexpected resident"), || Ok(42))
            .unwrap(),
        42
    );
    assert!(Arc::ptr_eq(
        &registry
            .get_or_load(session_id, || Ok(handle.clone()))
            .unwrap(),
        &handle
    ));
    assert_eq!(
        registry
            .with_configuration_access(session_id, |_| Ok(43), || panic!("must use resident owner"))
            .unwrap(),
        43
    );
}

/// The physical API start is recoverable before any Provider output exists.
/// 在任何 Provider 输出出现前，物理 API 起始事实即可恢复。
#[test]
fn api_start_is_durable_before_zero_output_crash() {
    let (recording, handle) = recording_handle();
    handle
        .submit_root_input(test_client_input("registry-root", "root"), 2)
        .unwrap();
    let preparation = handle
        .read(|document| {
            test_prepare_api_input(
                document,
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                3,
            )
        })
        .expect("build API preparation");
    let api_call_id = handle
        .prepare_and_start_api_durable(preparation)
        .unwrap()
        .value
        .api_call_id;

    let (memory_revision, durable_revision) = handle
        .read(|document| (document.memory_revision(), document.durable_revision()))
        .unwrap();
    assert_eq!(memory_revision, durable_revision);
    assert!(recording.persisted_events().iter().any(|event| {
        matches!(
            &event.event,
            EventBody::ApiStarted(started) if started.api_call_id == api_call_id
        )
    }));
}

/// Repeated shutdown keeps one handle, permits terminal settlement, and
/// preserves dirty memory across a failed close flush.
/// 重复关闭保持同一句柄、允许终态收束，并在关闭 flush 失败时保留脏内存。
#[test]
fn repeated_shutdown_with_active_tool_has_deterministic_recovery() {
    let recording = Arc::new(RecordingPersistenceBackend::default());
    let backend = Arc::new(FailThenPersistBackend {
        remaining_failures: AtomicUsize::new(1),
        delegate: Arc::clone(&recording),
    });
    let handle = published_handle(backend);
    let session_id = handle.session_id();
    handle
        .mutate_in_memory(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document.submit_root_input(test_client_input("registry-root", "root"), 2),
        )
        .expect("submit root input");
    let api_call_id = handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiStarted,
            |document| {
                document.prepare_and_start_api(test_prepare_api_input(
                    document,
                    ApiPurposeKind::MainGeneration,
                    ApiRequestSnapshot::new("openai", "gpt-5"),
                    3,
                ))
            },
        )
        .expect("prepare API")
        .value
        .into_value()
        .api_call_id;
    let tool_call_id = handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    RequestStreamId::try_new("registry-shutdown-tool-stream")?,
                    "shell_command",
                    ToolPayload::try_new(serde_json::json!({"command": "echo ok"}))?,
                    Some("provider-tool".to_string()),
                    4,
                )
            },
        )
        .expect("prepare tool")
        .value;
    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| document.start_prepared_tool(api_call_id, tool_call_id, 4),
        )
        .expect("start prepared tool");
    let registry = SessionRuntimeRegistry::default();
    registry.register(Arc::clone(&handle)).unwrap();
    let first_snapshot = registry.begin_shutdown().unwrap();
    let second_snapshot = registry.begin_shutdown().unwrap();
    assert_eq!(first_snapshot.len(), 1);
    assert!(Arc::ptr_eq(&first_snapshot[0], &second_snapshot[0]));

    let failed_terminal_flush = handle.mutate_durable(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::ToolBoundary,
        |document| {
            document.finish_tool(
                api_call_id,
                tool_call_id,
                ToolCompletion {
                    output: Some(ToolPayload::try_new(
                        serde_json::json!({"status": "completed"}),
                    )?),
                    changeset: None,
                    status: ToolCallStatus::Completed,
                    finished_at_ms: 5,
                    external_refs: Vec::new(),
                    generated_resources: Vec::new(),
                },
            )
        },
    );
    assert!(matches!(
        failed_terminal_flush,
        Err(SessionRuntimeError::Storage(_))
    ));
    assert!(matches!(
        handle.mutate_in_memory(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::InputAccepted,
            |document| document
                .submit_root_input(test_client_input("registry-blocked-second", "blocked"), 6),
        ),
        Err(SessionRuntimeError::ShuttingDown)
    ));
    handle.flush_all().unwrap();
    assert_eq!(
        recording.persisted_events(),
        handle.read(|document| document.events().to_vec()).unwrap()
    );

    registry.resume_after_failed_shutdown().unwrap();
    assert!(matches!(
        registry.get(session_id),
        Ok(registered) if Arc::ptr_eq(&registered, &handle)
    ));
}
