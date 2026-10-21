// Vendored from openai/codex (codex-rs/code-mode-host/tests/support/recording_delegate.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
use std::sync::Mutex;
use std::sync::PoisonError;

use serde_json::json;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use vulcan_code_mode::CellId;
use vulcan_code_mode::CodeModeNestedToolCall;
use vulcan_code_mode::CodeModeSessionDelegate;
use vulcan_code_mode::NotificationFuture;
use vulcan_code_mode::ToolInvocationFuture;

#[derive(Default)]
pub(crate) struct RecordingDelegate {
    pub(crate) invocations: Mutex<Vec<CodeModeNestedToolCall>>,
    pub(crate) notifications: Mutex<Vec<(String, CellId, String)>>,
    pub(crate) notification_delivered: Notify,
    pub(crate) closed_cells: Mutex<Vec<CellId>>,
}

impl CodeModeSessionDelegate for RecordingDelegate {
    fn invoke_tool<'a>(
        &'a self,
        invocation: CodeModeNestedToolCall,
        _cancellation: CancellationToken,
    ) -> ToolInvocationFuture<'a> {
        self.invocations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(invocation);
        Box::pin(async { Ok(json!({ "value": "output" })) })
    }

    fn notify<'a>(
        &'a self,
        call_id: String,
        cell_id: CellId,
        text: String,
        _cancellation: CancellationToken,
    ) -> NotificationFuture<'a> {
        self.notifications
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((call_id, cell_id, text));
        self.notification_delivered.notify_one();
        Box::pin(async { Ok(()) })
    }

    fn cell_closed(&self, cell_id: &CellId) {
        self.closed_cells
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(cell_id.clone());
    }
}

pub(crate) fn cell_id(value: &str) -> CellId {
    CellId::new(value.to_string())
}
