//! Answer mode handling owned by the serial service worker.
//! 由串行服务工作线程持有的回答模式处理。

use super::*;
use crate::service_api::answer_mode as requests;

impl Worker {
    /// Publish one exact window or draft; unknown sessions have no authority to publish.
    /// 发布一个精确窗口或草稿；未知会话没有可发布权威。
    pub(super) fn emit_answer_mode(&self, id: &str) {
        let state = if let Some(draft) = self.drafts.get(id) {
            AnswerModeSession {
                mode: Some(draft.answer_mode.clone()),
            }
        } else if let Some(window) = self.windows.get(id) {
            window.answer_mode.clone()
        } else {
            return;
        };
        self.emit(BackendEvent::AnswerMode {
            conversation_id: id.to_owned(),
            state,
        });
    }

    /// Return the global default copied into a new draft.
    /// 返回复制到新草稿的全局默认值。
    pub(super) fn default_answer_mode(&self) -> AnswerModePreference {
        AnswerModePreference {
            mode: self
                .wb
                .as_ref()
                .map(|wb| wb.default_answer_mode)
                .unwrap_or_default(),
            ..AnswerModePreference::default()
        }
    }

    /// Save a durable answer mode or update a local draft; CAS conflicts never overwrite authority.
    /// 保存可靠回答模式或更新本地草稿；比较交换冲突绝不覆盖权威。
    pub(super) fn set_answer_mode(&mut self, request: api::SessionAnswerModeUpdateRequest) {
        if let Some(draft) = self.drafts.get_mut(&request.session_id) {
            let conflict = draft.answer_mode.revision != request.expected_revision;
            if !conflict && draft.answer_mode.mode != request.mode {
                draft.answer_mode.mode = request.mode;
                draft.answer_mode.revision += 1;
            }
            self.emit_answer_mode(&request.session_id);
            if conflict {
                self.toast(NoticeSeverity::Warning, crate::tr!("answerMode.conflict"));
            }
            return;
        }
        let Some(client) = self.client.clone() else {
            self.emit(BackendEvent::Notice(Notice::NotConnected));
            return;
        };
        let (method, params) = requests::set_session_answer_mode(&request);
        let response = match client.call(method, params) {
            Ok(value) => serde_json::from_value::<api::SessionAnswerModeUpdateResponse>(value).ok(),
            Err(error) => {
                if error.transport {
                    self.disconnected();
                }
                None
            }
        };
        let Some(response) = response else {
            self.toast(NoticeSeverity::Error, crate::tr!("answerMode.saveFailed"));
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
        window.answer_mode.accept_mode(response.answer_mode);
        let pending = window.answer_mode.awaiting_request();
        self.emit_answer_mode(&request.session_id);
        if response.conflict {
            self.toast(NoticeSeverity::Warning, crate::tr!("answerMode.conflict"));
        } else if pending {
            self.toast(NoticeSeverity::Info, crate::tr!("answerMode.pending"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::RpcServer;
    use super::*;

    /// Stale CAS replies restore authority and never optimistically preserve the requested state.
    /// 过期比较交换回执恢复权威，绝不乐观保留请求状态。
    #[test]
    fn answer_mode_conflict_is_authoritative() {
        let server = RpcServer::start(1, |request| {
            assert_eq!(request.body["method"], "service_set_session_answer_mode");
            assert_eq!(request.body["params"]["request"]["expectedRevision"], 0);
            assert_eq!(request.body["params"]["request"]["mode"], "interactive");
            json!({"ok":true,"result":{"session_id":request.body["params"]["request"]["sessionId"],"window_generation":8,"revision":21,
                "answer_mode":{"mode":"standard","revision":3,"prepared":null},"conflict":true}})
        });
        let (out, events) = std::sync::mpsc::channel();
        let (inbox, _) = std::sync::mpsc::channel();
        let mut worker = Worker::new(out, inbox, None);
        worker.client = Some(server.client.clone());
        let id = uuid::Uuid::new_v4().to_string();
        worker.windows.insert(
            id.clone(),
            Window {
                generation: 8,
                epoch: "synthetic".into(),
                memory_revision: 20,
                permission_mode: PermissionMode::WorkspaceWrite,
                unattended: None,
                answer_mode: AnswerModeSession {
                    mode: Some(AnswerModePreference::default()),
                },
                events: Default::default(),
            },
        );
        worker.set_answer_mode(api::SessionAnswerModeUpdateRequest {
            session_id: id.clone(),
            mode: AnswerMode::Interactive,
            expected_revision: 0,
        });
        let mode = worker.windows[&id].answer_mode.mode.as_ref().unwrap();
        assert_eq!(mode.mode, AnswerMode::Standard);
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

    /// Draft answer modes travel in the real first-submit envelope and survive a failed call.
    /// 草稿回答模式包含于真实首轮提交信封，调用失败后仍被保留。
    #[test]
    fn answer_mode_draft_is_captured_by_first_submission() {
        let server = RpcServer::start(1, |request| {
            assert_eq!(request.body["method"], "service_submit_conversation_prompt");
            assert!(request.body["params"]["conversationId"].is_null());
            assert_eq!(
                request.body["params"]["sessionSettings"]["answerMode"],
                "interactive"
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
                answer_mode: AnswerModePreference::default(),
                permission_mode: None,
                project_id: None,
                model_key: None,
            },
        );
        worker.set_answer_mode(api::SessionAnswerModeUpdateRequest {
            session_id: id.clone(),
            mode: AnswerMode::Interactive,
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
        assert_eq!(worker.drafts[&id].answer_mode.mode, AnswerMode::Interactive);
        server.thread.join().unwrap();
    }
}
