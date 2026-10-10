//! Permission mode handling owned by the serial service worker.
//! 由串行服务工作线程持有的权限模式处理。

use super::*;
use crate::service_api::permission_mode as requests;

impl Worker {
    /// Publish one exact window or draft; unknown sessions have no authority to publish.
    /// 发布一个精确窗口或草稿；未知会话没有可发布权威。
    pub(super) fn emit_permission_mode(&self, id: &str) {
        let mode = if let Some(draft) = self.drafts.get(id) {
            draft.permission_mode.unwrap_or_default()
        } else if let Some(window) = self.windows.get(id) {
            window.permission_mode
        } else {
            return;
        };
        self.emit(BackendEvent::PermissionMode {
            conversation_id: id.to_owned(),
            mode,
        });
    }

    /// Save a durable permission mode or capture an explicit draft choice for the first submission.
    /// 保存可靠权限模式，或为首次提交捕获显式草稿选择。
    pub(super) fn set_permission_mode(&mut self, request: api::SessionPermissionModeUpdateRequest) {
        if let Some(draft) = self.drafts.get_mut(&request.session_id) {
            draft.permission_mode = Some(request.mode);
            self.emit_permission_mode(&request.session_id);
            return;
        }
        let Some(client) = self.client.clone() else {
            self.emit(BackendEvent::Notice(Notice::NotConnected));
            return;
        };
        let (method, params) = requests::set_session_permission_mode(&request);
        let response = match client.call(method, params) {
            Ok(value) => {
                serde_json::from_value::<api::SessionPermissionModeUpdateResponse>(value).ok()
            }
            Err(error) => {
                if error.transport {
                    self.disconnected();
                }
                None
            }
        };
        let Some(response) = response else {
            self.toast(
                NoticeSeverity::Error,
                crate::tr!("permissionMode.saveFailed"),
            );
            return;
        };
        let Some(window) = self.windows.get_mut(&request.session_id) else {
            return;
        };
        if response.session_id != request.session_id {
            self.hydrate(&request.session_id);
            return;
        }
        window.permission_mode = response.permission_mode;
        self.emit_permission_mode(&request.session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::RpcServer;
    use super::*;

    /// Existing sessions adopt only the durable receipt returned by the service.
    /// 已有会话只采用服务返回的可靠回执。
    #[test]
    fn permission_mode_adopts_the_durable_receipt() {
        let server = RpcServer::start(1, |request| {
            assert_eq!(
                request.body["method"],
                "service_set_session_permission_mode"
            );
            assert_eq!(request.body["params"]["request"]["mode"], "read_only");
            json!({"ok":true,"result":{"session_id":request.body["params"]["request"]["sessionId"],
                "revision":4,"permission_mode":"read_only"}})
        });
        let (out, events) = std::sync::mpsc::channel();
        let (inbox, _) = std::sync::mpsc::channel();
        let mut worker = Worker::new(out, inbox, None);
        worker.client = Some(server.client.clone());
        let id = uuid::Uuid::new_v4().to_string();
        worker.windows.insert(
            id.clone(),
            Window {
                generation: 1,
                epoch: "synthetic".into(),
                memory_revision: 0,
                answer_mode: AnswerModeSession::default(),
                permission_mode: PermissionMode::WorkspaceWrite,
                events: Default::default(),
            },
        );
        worker.set_permission_mode(api::SessionPermissionModeUpdateRequest {
            session_id: id.clone(),
            mode: PermissionMode::ReadOnly,
        });
        assert_eq!(
            worker.windows[&id].permission_mode,
            PermissionMode::ReadOnly
        );
        assert!(events.try_iter().any(|event| matches!(
            event,
            BackendEvent::PermissionMode {
                mode: PermissionMode::ReadOnly,
                ..
            }
        )));
        server.thread.join().unwrap();
    }

    /// An explicit draft choice travels in the real first-submit envelope.
    /// 显式草稿选择包含于真实首轮提交信封。
    #[test]
    fn permission_mode_draft_is_captured_by_first_submission() {
        let server = RpcServer::start(1, |request| {
            assert_eq!(request.body["method"], "service_submit_conversation_prompt");
            assert_eq!(
                request.body["params"]["sessionSettings"]["permissionMode"],
                "full_access"
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
        worker.set_permission_mode(api::SessionPermissionModeUpdateRequest {
            session_id: id.clone(),
            mode: PermissionMode::FullAccess,
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
        assert_eq!(
            worker.drafts[&id].permission_mode,
            Some(PermissionMode::FullAccess)
        );
        server.thread.join().unwrap();
    }
}
