//! Concurrent native question waits retain independent observations of one durable answer.
//! 并发原生问题等待各自独立观察同一个可靠答案。

use super::*;
use vulcan_agent_runtime::tooling::collaboration::WaitQuestionArgs;

/// Prepares four distinct wait ToolCalls on `source` in one atomic provider batch.
/// 在 `source` 的一个原子供应商批次中准备四个独立等待工具调用。
/// Returns authenticated contexts sharing one API and preserving the current business turn.
/// 返回共享单个 API 且保留当前业务回合的已认证上下文。
fn prepare_concurrent_wait_contexts(
    source: &Arc<SessionHandle>,
) -> Vec<VulcanToolInvocationContext> {
    // Capture the real canonical frontier for this single source API.
    // 为此单个来源 API 捕获真实规范前沿。
    let input = source
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: vulcan_session_core::v1::event::ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: vulcan_session_core::v1::ids::Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 12,
            }
        })
        .unwrap();
    // One physical response owns one atomic tool batch; adding to an existing batch is semantic drift.
    // 一个物理响应持有一个原子工具批次；向已有批次追加会构成语义漂移。
    let api_call_id = source
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    let names = ["wait-one", "wait-two", "wait-cancelled", "wait-later"];
    let preparations = names
        .iter()
        .map(|name| vulcan_session_core::v1::document::ToolPreparation {
            request_stream_id: RequestStreamId::try_new(format!("{name}-stream")).unwrap(),
            tool_name: WAIT_QUESTION_TOOL_ID.into(),
            input: ToolPayload::empty(),
            provider_tool_call_id: Some((*name).into()),
        })
        .collect();
    // The receipt supplies exact event identities without positional event lookup.
    // 回执提供精确事件身份，无需按事件位置查找。
    let receipt = source
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document
                    .prepare_tool_batch(api_call_id, preparations, 12)
                    .map(|result| result.into_value())
            },
        )
        .unwrap()
        .value;
    receipt
        .tools
        .into_iter()
        .zip(names)
        .map(|(tool, name)| VulcanToolInvocationContext {
            session_id: source.session_id().to_string(),
            request_stream_id: tool.request_stream_id,
            api_call_id,
            tool_call_id: tool.tool_call_id,
            tool_seq: tool.tool_seq,
            controller_tool_call_id: name.into(),
            provider_tool_call_id: Some(name.into()),
        })
        .collect()
}

/// Cancelling one native wait must neither cancel the question nor consume another waiter's answer.
/// 取消一个原生等待既不能取消问题，也不能消费其他等待者的答案。
/// Uses three authenticated ToolCalls and a real reply; returns after every surviving observer settles.
/// 使用三个已认证工具调用及真实回复，全部存活观察者完成后返回。
#[tokio::test]
async fn concurrent_waiters_survive_one_cancel_and_observe_the_same_answer() {
    // The existing file-backed fixture owns both participants and their accepted question.
    // 既有文件后端夹具持有双方参与者及已接纳问题。
    let (_storage, invocation, target, question, _operation) =
        unacknowledged_settlement_fixture(false).await;
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| document.prepare_question_execution_before_api(question.clone(), 10),
        )
        .unwrap();
    // The reply is authenticated against the target's actual auxiliary API.
    // 回复针对目标真实辅助 API 认证。
    let reply_context = start_question_execution_tool_call(
        &target,
        ApiPurposeKind::QuestionExecution,
        11,
        "concurrent-wait-reply",
    );
    // All waits coexist within one source API without replacing each other's invocation authority.
    // 全部等待共存于同一来源 API，不替换彼此的调用权限。
    let source = invocation.source_handle.clone();
    let mut contexts = prepare_concurrent_wait_contexts(&source);
    let later_context = contexts.pop().unwrap();
    let port = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        ServiceEventEmitter::new(Vec::new()),
    );
    // Own each spawned wait so one can be cancelled independently from the other observers.
    // 独立持有每个已启动等待，使其中一个可单独取消。
    let mut waiters = Vec::new();
    for context in contexts {
        let waiting_port = port.clone();
        let question_id = question.question_id.as_str().to_owned();
        waiters.push(tokio::spawn(async move {
            waiting_port
                .invoke(
                    context,
                    AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
                        question_id,
                        after_revision: None,
                        timeout_ms: 30_000,
                    }),
                )
                .await
        }));
    }
    // Each admitted wait remains pending before the answer, rather than returning an unrelated revision.
    // 每个已准入等待在答案到达前保持挂起，不因无关修订而返回。
    for waiter in &mut waiters {
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(25), waiter)
                .await
                .is_err(),
            "an unanswered native wait must remain pending"
        );
    }
    // Dropping one caller's wait must not mutate the shared question.
    // 放弃一个调用方的等待不得修改共享问题。
    let cancelled_wait = waiters.pop().unwrap();
    cancelled_wait.abort();
    assert!(cancelled_wait.await.unwrap_err().is_cancelled());
    assert_eq!(
        source
            .read(|document| document
                .question_snapshot(&question.question_id)
                .unwrap()
                .state)
            .unwrap(),
        CollaborationQuestionState::Pending
    );
    // A real reply must acquire both lifecycle gates while the other waits remain active.
    // 其他等待仍活动时，真实回复必须能够取得双方生命周期锁。
    let answer = "The same durable answer belongs to every authorized observer.";
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        port.invoke(
            reply_context,
            AgentCollaborationCommand::ReplyQuestion(ReplyQuestionArgs {
                question_id: question.question_id.as_str().to_owned(),
                answer: answer.to_owned(),
                unable_to_answer: false,
            }),
        ),
    )
    .await
    .expect("active waits must not hold lifecycle gates")
    .unwrap();
    assert_eq!(reply["question_state"], "answered");
    // All surviving calls must see the full exact answer; their operation identities remain distinct.
    // 所有存活调用必须看到完整精确答案，且各自操作身份保持独立。
    let mut operations = std::collections::BTreeSet::new();
    for waiter in waiters {
        let observed = tokio::time::timeout(std::time::Duration::from_secs(5), waiter)
            .await
            .expect("every observer must receive the answer")
            .unwrap()
            .unwrap();
        assert_eq!(observed["status"], "ok");
        assert_eq!(observed["question_state"], "answered");
        assert_eq!(observed["answer"]["body"], answer);
        assert!(operations.insert(observed["operation_id"].as_str().unwrap().to_owned()));
    }
    // A later observer still sees the answer; the completed waits did not consume a unique message.
    // 后续观察者仍能看到答案，已完成等待没有消费唯一消息。
    let later = port
        .invoke(
            later_context,
            AgentCollaborationCommand::WaitQuestion(WaitQuestionArgs {
                question_id: question.question_id.as_str().to_owned(),
                after_revision: None,
                timeout_ms: 1,
            }),
        )
        .await
        .unwrap();
    assert_eq!(later["answer"]["body"], answer);
}
