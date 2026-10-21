// Vendored from openai/codex (codex-rs/code-mode/src/remote_session/connection/reader.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
use tokio::process::ChildStdout;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use vulcan_code_mode_protocol::host::FramedReader;

use super::driver::DriverEvent;

pub(super) async fn drive_reader(
    mut reader: FramedReader<ChildStdout>,
    events: mpsc::Sender<DriverEvent>,
    cancellation: CancellationToken,
) -> Result<(), String> {
    loop {
        let message = tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            result = reader.read() => result,
        };
        let message = match message {
            Ok(Some(message)) => message,
            Ok(None) => return Err("code-mode host closed its stdout".to_string()),
            Err(err) => return Err(format!("failed to read code-mode host message: {err}")),
        };
        events
            .send(DriverEvent::HostMessage(message))
            .await
            .map_err(|_| "code-mode connection driver closed".to_string())?;
    }
}
