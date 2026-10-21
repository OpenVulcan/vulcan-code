// Vendored from openai/codex (codex-rs/code-mode/src/remote_session_tests.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use vulcan_code_mode_protocol::CodeModeSessionProvider;

use super::ProcessOwnedCodeModeSession;
use super::ProcessOwnedCodeModeSessionProvider;
use super::connection::ConnectionError;
use crate::NoopCodeModeSessionDelegate;

#[test]
fn provider_reuses_its_live_process_host() {
    // VULCAN MODIFICATION: the provider has no `Default`; the host is never spawned here.
    let provider = ProcessOwnedCodeModeSessionProvider::with_host_program(PathBuf::from(
        "vulcan-code-mode-host",
    ));

    let first = provider.process_host();
    let second = provider.process_host();

    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn missing_host_error_limits_the_displayed_path_to_512_bytes() {
    let executable = "vulcan-code-mode-host-does-not-exist";
    let host_program = format!("{}{executable}", "missing-directory/".repeat(/*n*/ 64));
    let expected_suffix = &host_program[host_program.len() - (512 - "...".len())..];
    let error = ConnectionError::Spawn {
        host_program: PathBuf::from(&host_program),
        error: io::Error::new(io::ErrorKind::NotFound, "host unavailable"),
    };

    assert_eq!(
        error.to_string(),
        format!("failed to spawn code-mode host ...{expected_suffix}: host unavailable")
    );
}

#[test]
fn missing_host_error_preserves_utf8_boundaries_when_truncating_the_path() {
    let executable = "vulcan-code-mode-host-does-not-exist";
    let host_program = format!("{}{executable}", "🦀".repeat(/*n*/ 256));
    let error = ConnectionError::Spawn {
        host_program: PathBuf::from(host_program),
        error: io::Error::new(io::ErrorKind::NotFound, "host unavailable"),
    }
    .to_string();
    let displayed_path = error
        .strip_prefix("failed to spawn code-mode host ")
        .and_then(|message| message.strip_suffix(": host unavailable"))
        .expect("missing-host error should contain the displayed host path");

    assert!(displayed_path.starts_with("..."));
    assert!(displayed_path.ends_with(executable));
    assert!(displayed_path.len() <= 512);
}

#[tokio::test]
async fn provider_returns_missing_host_error() {
    let provider = ProcessOwnedCodeModeSessionProvider::with_host_program(
        "vulcan-code-mode-host-does-not-exist".into(),
    );

    let error = provider
        .create_session()
        .await
        .err()
        .expect("missing host should fail");

    assert!(error.contains("failed to spawn code-mode host vulcan-code-mode-host-does-not-exist"));
}

#[tokio::test]
async fn shutdown_before_open_does_not_spawn_the_host() {
    // VULCAN MODIFICATION: the session takes an explicit host program; it is never spawned here.
    let session = ProcessOwnedCodeModeSession::new(PathBuf::from("vulcan-code-mode-host"));

    session.shutdown().await.expect("shutdown session");
    let error = session
        .execute(
            vulcan_code_mode_protocol::ExecuteRequest {
                tool_call_id: "call-1".to_string(),
                enabled_tools: Vec::new(),
                source: "text('unreachable')".to_string(),
                yield_time_ms: None,
                max_output_tokens: None,
            },
            Arc::new(NoopCodeModeSessionDelegate),
            /*preempt*/ None,
        )
        .await
        .err()
        .expect("shutdown session should reject execution");

    assert_eq!(error, "code mode session is shutting down");
}
