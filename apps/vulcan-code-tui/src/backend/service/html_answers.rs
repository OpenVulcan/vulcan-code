//! Preference and view-link handling owned by the serial service worker.
//! 由串行服务工作线程持有的偏好与查看链接处理。

use super::*;
use crate::service_api::html_answers as requests;
use vulcan_session_core::v1::{AssistantSemanticStatus, EventVisibility};

impl Worker {
    /// Publish one exact window or draft; unknown sessions have no authority to publish.
    /// 发布一个精确窗口或草稿；未知会话没有可发布权威。
    pub(super) fn emit_html_answers(&self, id: &str) {
        let state = if let Some(draft) = self.drafts.get(id) {
            HtmlAnswerSession {
                generation: Some(0),
                mode: Some(draft.html_answers.clone()),
                projections: Default::default(),
            }
        } else if let Some(window) = self.windows.get(id) {
            window.html_answers.clone()
        } else {
            return;
        };
        self.emit(BackendEvent::HtmlAnswers {
            conversation_id: id.to_owned(),
            state,
        });
    }

    /// Accept only annotations matching current provider-visible terminal assistant facts.
    /// 仅接纳匹配当前 Provider 可见终态助手事实的标注。
    /// The input originates in confirmed Core publication; memory revisions never prove durability.
    /// 输入来自确认的 Core 发布；内存修订绝不能证明可靠保存。
    pub(super) fn accept_html_projections(
        &mut self,
        id: &str,
        projections: Vec<HtmlAnswerEventProjection>,
    ) {
        let Some(window) = self.windows.get_mut(id) else {
            return;
        };
        for projection in projections {
            let Some(event) = window.events.get(&projection.seq.value()) else {
                continue;
            };
            if event.event_revision != projection.event_revision
                || event.visibility != EventVisibility::Provider
                || !matches!(&event.event, CoreEvent::AssistantText(text) if text.status != AssistantSemanticStatus::Streaming)
            {
                continue;
            }
            window
                .html_answers
                .projections
                .insert(projection.seq.value(), projection);
        }
    }

    /// Save a durable preference or update a local draft; CAS conflicts never overwrite authority.
    /// 保存可靠偏好或更新本地草稿；比较交换冲突绝不覆盖权威。
    pub(super) fn set_html_answer_mode(
        &mut self,
        request: api::SessionHtmlAnswerModeUpdateRequest,
    ) {
        if let Some(draft) = self.drafts.get_mut(&request.session_id) {
            if draft.html_answers.revision == request.expected_revision
                && draft.html_answers.enabled != request.enabled
            {
                draft.html_answers.enabled = request.enabled;
                draft.html_answers.revision += 1;
            }
            self.emit_html_answers(&request.session_id);
            return;
        }
        let (method, params) = requests::set_session_html_answer_mode(&request);
        let Some(response) =
            self.call_as::<api::SessionHtmlAnswerModeUpdateResponse>(method, params)
        else {
            return;
        };
        let Some(window) = self.windows.get_mut(&request.session_id) else {
            return;
        };
        if response.session_id != request.session_id
            || response.window_generation != window.generation
        {
            self.hydrate(&request.session_id);
            return;
        }
        if window
            .html_answers
            .mode
            .as_ref()
            .is_none_or(|mode| response.html_answers.revision >= mode.revision)
        {
            window.html_answers.mode = Some(response.html_answers);
        }
        self.emit_html_answers(&request.session_id);
        if response.conflict {
            self.toast(NoticeSeverity::Warning, crate::tr!("htmlAnswers.conflict"));
        }
    }

    /// Issue an explicit source selection after checking it is still current in the loaded window.
    /// 确认显式选中的源码仍属于已加载窗口后签发授权。
    pub(super) fn create_html_answer_link(
        &mut self,
        source: api::HtmlAnswerSourceIdentity,
        delivery: HtmlAnswerDelivery,
        locale: String,
    ) {
        let id = source.session_id.to_string();
        let current = self.windows.get(&id).is_some_and(|window| {
            window
                .html_answers
                .projections
                .get(&source.event_seq)
                .is_some_and(|projection| {
                    projection.answers.iter().any(|answer| {
                        window
                            .html_answers
                            .source(&id, source.event_seq, answer.ordinal)
                            .as_ref()
                            == Some(&source)
                    })
                })
        });
        if !current {
            self.toast(NoticeSeverity::Warning, crate::tr!("htmlAnswers.stale"));
            return;
        }
        let request = api::HtmlAnswerViewLinkRequest {
            client_id: self.client_id.parse().expect("worker client ID is a UUID"),
            source: source.clone(),
            locale,
        };
        let (method, params) = requests::create_html_answer_view_link(&request);
        if let Some(link) = self.call_as::<api::HtmlAnswerViewLinkResponse>(method, params) {
            self.emit(BackendEvent::HtmlAnswerLink {
                source,
                link: HtmlAnswerLink(link),
                delivery,
            });
        }
    }

    /// Revoke this process's exact non-secret grant ID, without retaining any URL.
    /// 撤销本进程精确的非秘密授权 ID，不保留任何地址。
    pub(super) fn revoke_html_answer_link(&mut self, grant_id: uuid::Uuid) {
        let request = api::HtmlAnswerRevokeRequest {
            client_id: self.client_id.parse().expect("worker client ID is a UUID"),
            grant_id,
        };
        let (method, params) = requests::revoke_html_answer_view_link(&request);
        let _ = self.call(method, params);
    }
}

#[cfg(test)]
#[path = "html_answers_acceptance.rs"]
mod acceptance;

#[cfg(test)]
mod tests {
    use super::super::super::test_support::RpcServer;
    use super::*;
    use vulcan_session_core::v1::service_dto::ConversationEventWindowBatch;

    /// Build an isolated worker with one canonical source and no discovery or paid operations.
    /// 构建含一条权威源码的隔离工作线程，不执行发现或付费操作。
    fn worker(
        client: RpcClient,
    ) -> (
        Worker,
        Receiver<BackendEvent>,
        String,
        HtmlAnswerEventProjection,
    ) {
        let (out, events) = std::sync::mpsc::channel();
        let (inbox, _) = std::sync::mpsc::channel();
        let mut worker = Worker::new(out, inbox, None);
        worker.client = Some(client);
        let (id, event, projection) = crate::model::html_answers::fixture();
        worker.windows.insert(
            id.clone(),
            Window {
                generation: 8,
                epoch: "synthetic".into(),
                memory_revision: 20,
                html_answers: HtmlAnswerSession {
                    generation: Some(8),
                    mode: Some(HtmlAnswerMode::default()),
                    projections: Default::default(),
                },
                events: [(
                    event.seq.value(),
                    ConversationEventSummary::from_event(&event),
                )]
                .into(),
            },
        );
        (worker, events, id, projection)
    }

    /// Durable acknowledgements may lag document memory while still matching the exact event.
    /// 可靠确认可以落后于文档内存，同时仍匹配精确事件。
    #[test]
    fn html_durable_annotations_survive_unrelated_memory_revisions_and_drop_on_source_change() {
        let server = RpcServer::start(0, |_| unreachable!());
        let (mut worker, events, id, projection) = worker(server.client.clone());
        let patch = api::ConversationEventLiveDeltaV1 {
            batch: ConversationEventWindowBatch {
                session_id: id.clone(),
                window_generation: 8,
                events: Vec::new(),
            },
            service_epoch: "synthetic".into(),
            revision: 10,
            base_memory_revision: None,
            base_durable_revision: Some(9),
            durability: api::ConversationEventPatchDurabilityV1::Durable,
            text_appends: Vec::new(),
            html_answers: None,
            html_answer_projections: Some(Box::new(vec![projection.clone()])),
        };
        worker.event_delta(patch.clone());
        assert!(events.try_iter().any(|event| matches!(event, BackendEvent::HtmlAnswers { state, .. } if state.projections.contains_key(&projection.seq.value()))));
        let mut changed = worker.windows[&id].events[&projection.seq.value()].clone();
        changed.event_revision += 1;
        worker.event_delta(api::ConversationEventLiveDeltaV1 {
            batch: ConversationEventWindowBatch {
                session_id: id.clone(),
                window_generation: 8,
                events: vec![changed],
            },
            revision: 21,
            base_memory_revision: Some(20),
            durability: api::ConversationEventPatchDurabilityV1::Volatile,
            html_answer_projections: None,
            ..patch.clone()
        });
        worker.event_delta(patch);
        assert!(worker.windows[&id].html_answers.projections.is_empty());
        server.thread.join().unwrap();
    }

    /// The actual HTTP client mirrors the WebUI envelope and emits only a redacted receipt.
    /// 实际 HTTP 客户端镜像 WebUI 信封，且仅发布诊断脱敏的回执。
    #[test]
    fn html_link_http_contract_retains_exact_identity_and_redacts_receipt() {
        let server = RpcServer::start(1, |request| {
            assert_eq!(
                request.body["method"],
                "service_create_html_answer_view_link"
            );
            let source = &request.body["params"]["request"]["source"];
            assert_eq!(source["windowGeneration"], 8);
            assert_eq!(source["eventRevision"], 4);
            assert_eq!(request.body["params"]["request"]["locale"], "zh-CN");
            json!({"ok":true,"result":{"grantId":uuid::Uuid::new_v4(),"viewerUrl":"http://127.0.0.1:1/#ticket=synthetic-secret","expiresInSeconds":300}})
        });
        let (mut worker, events, id, projection) = worker(server.client.clone());
        worker.accept_html_projections(&id, vec![projection.clone()]);
        let source = worker.windows[&id]
            .html_answers
            .source(&id, projection.seq.value(), 0)
            .unwrap();
        worker.create_html_answer_link(source.clone(), HtmlAnswerDelivery::Copy, "zh-CN".into());
        let event = events
            .try_iter()
            .find(|event| matches!(event, BackendEvent::HtmlAnswerLink { .. }))
            .unwrap();
        assert!(!format!("{event:?}").contains("synthetic-secret"));
        let BackendEvent::HtmlAnswerLink { source: actual, .. } = event else {
            unreachable!()
        };
        assert_eq!(actual, source);
        server.thread.join().unwrap();
    }

    /// Stale CAS replies restore authority and never optimistically preserve the requested state.
    /// 过期比较交换回执恢复权威，绝不乐观保留请求状态。
    #[test]
    fn html_preference_conflict_is_authoritative() {
        let server = RpcServer::start(1, |request| {
            assert_eq!(
                request.body["method"],
                "service_set_session_html_answer_mode"
            );
            assert_eq!(request.body["params"]["request"]["expectedRevision"], 0);
            json!({"ok":true,"result":{"session_id":request.body["params"]["request"]["sessionId"],"window_generation":8,"revision":21,
                "html_answers":{"enabled":false,"revision":3,"prepared":null},"conflict":true}})
        });
        let (mut worker, events, id, _) = worker(server.client.clone());
        worker.set_html_answer_mode(api::SessionHtmlAnswerModeUpdateRequest {
            session_id: id.clone(),
            enabled: true,
            expected_revision: 0,
        });
        let mode = worker.windows[&id].html_answers.mode.as_ref().unwrap();
        assert!(!mode.enabled);
        assert_eq!(mode.revision, 3);
        assert!(events.try_iter().any(|event| matches!(
            event,
            BackendEvent::Toast {
                severity: NoticeSeverity::Warning,
                ..
            }
        )));
        server.thread.join().unwrap();
    }

    /// Draft preferences travel atomically in the real first-submit envelope and survive a failed call.
    /// 草稿偏好原子包含于真实首轮提交信封，调用失败后仍被保留。
    #[test]
    fn html_draft_preference_is_captured_by_first_submission() {
        let server = RpcServer::start(1, |request| {
            assert_eq!(request.body["method"], "service_submit_conversation_prompt");
            assert!(request.body["params"]["conversationId"].is_null());
            assert_eq!(
                request.body["params"]["sessionSettings"]["htmlAnswersEnabled"],
                true
            );
            json!({"ok":false,"error":{"code":"synthetic","message":"synthetic rejection"}})
        });
        let (out, _) = std::sync::mpsc::channel();
        let (inbox, input) = std::sync::mpsc::channel();
        let mut worker = Worker::new(out, inbox, None);
        worker.client = Some(server.client.clone());
        let id = format!("{DRAFT_PREFIX}synthetic");
        worker.drafts.insert(
            id.clone(),
            Draft {
                html_answers: HtmlAnswerMode::default(),
                project_id: None,
                model_key: None,
            },
        );
        worker.set_html_answer_mode(api::SessionHtmlAnswerModeUpdateRequest {
            session_id: id.clone(),
            enabled: true,
            expected_revision: 0,
        });
        worker.submit(
            id.clone(),
            "synthetic".into(),
            ExecutionMode::Build,
            ReasoningEffort::Off,
            String::new(),
            Vec::new(),
        );
        worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(worker.drafts[&id].html_answers.enabled);
        server.thread.join().unwrap();
    }
}
