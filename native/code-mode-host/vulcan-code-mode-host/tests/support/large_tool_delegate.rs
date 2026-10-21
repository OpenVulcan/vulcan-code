// Vendored from openai/codex (codex-rs/code-mode-host/tests/support/large_tool_delegate.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
use vulcan_code_mode::CellId;
use vulcan_code_mode::CodeModeNestedToolCall;
use vulcan_code_mode::CodeModeSessionDelegate;
use vulcan_code_mode::NotificationFuture;
use vulcan_code_mode::ToolInvocationFuture;
// VULCAN MODIFICATION: ToolName is vendored into vulcan-code-mode-protocol.
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use vulcan_code_mode_protocol::ToolName;

pub(crate) struct LargeToolResultDelegate {
    pub(crate) started: Semaphore,
    pub(crate) release: Semaphore,
}

impl CodeModeSessionDelegate for LargeToolResultDelegate {
    fn invoke_tool<'a>(
        &'a self,
        invocation: CodeModeNestedToolCall,
        _cancellation: CancellationToken,
    ) -> ToolInvocationFuture<'a> {
        Box::pin(async move {
            if invocation.tool_name == ToolName::plain("fast") {
                return Ok(json!({ "value": "isolated" }));
            }
            assert_eq!(invocation.tool_name, ToolName::plain("large"));
            self.started.add_permits(/*n*/ 1);
            self.release
                .acquire()
                .await
                .map_err(|_| "large tool release closed".to_string())?
                .forget();
            Ok(json!({ "value": "x".repeat(8 * 1024 * 1024) }))
        })
    }

    fn notify<'a>(
        &'a self,
        _call_id: String,
        _cell_id: CellId,
        _text: String,
        _cancellation: CancellationToken,
    ) -> NotificationFuture<'a> {
        Box::pin(async { Ok(()) })
    }

    fn cell_closed(&self, _cell_id: &CellId) {}
}
