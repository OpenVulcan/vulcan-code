// Vendored from openai/codex (codex-rs/code-mode/src/remote_session/connection/driver/types.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use vulcan_code_mode_protocol::CellId;
use vulcan_code_mode_protocol::CodeModeSessionCellExecutionLimits;
use vulcan_code_mode_protocol::CodeModeSessionDelegate;
use vulcan_code_mode_protocol::ExecuteRequest;
use vulcan_code_mode_protocol::RuntimeResponse;
use vulcan_code_mode_protocol::StartedCell;
use vulcan_code_mode_protocol::WaitOutcome;
use vulcan_code_mode_protocol::WaitRequest;
use vulcan_code_mode_protocol::host::DelegateRequestId;
use vulcan_code_mode_protocol::host::DelegateResponse;
use vulcan_code_mode_protocol::host::HostToClient;
use vulcan_code_mode_protocol::host::RequestId;
use vulcan_code_mode_protocol::host::SessionId;
use vulcan_code_mode_protocol::host::WireCellId;
use vulcan_code_mode_protocol::host::WireWaitRequest;

use super::cleanup::SessionCleanup;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::remote_session) struct RemoteSession {
    pub(in crate::remote_session) id: SessionId,
    pub(in crate::remote_session) generation: u64,
}

pub(in crate::remote_session::connection) enum DriverCommand {
    OpenSession {
        session: RemoteSession,
        limits: CodeModeSessionCellExecutionLimits,
        cleanup: SessionCleanup,
        caller_cancellation: CancellationToken,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
    Execute {
        session: RemoteSession,
        delegate: Arc<dyn CodeModeSessionDelegate>,
        request: ExecuteRequest,
        caller_cancellation: CancellationToken,
        yield_signal: Option<CancellationToken>,
        response_tx: oneshot::Sender<Result<DeliveredExecute, String>>,
    },
    Wait {
        session: RemoteSession,
        request: WaitRequest,
        caller_cancellation: CancellationToken,
        yield_signal: Option<CancellationToken>,
        response_tx: oneshot::Sender<Result<WaitOutcome, String>>,
    },
    Terminate {
        session: RemoteSession,
        cell_id: CellId,
        response_tx: oneshot::Sender<Result<WaitOutcome, String>>,
    },
    ShutdownSession {
        session: RemoteSession,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
}

pub(in crate::remote_session::connection) enum DriverEvent {
    HostMessage(HostToClient),
    DelegateCompleted {
        id: DelegateRequestId,
        result: Result<DelegateResponse, String>,
    },
    RequestCancelled(RequestId),
    YieldRequest(RequestId),
    Failed(String),
}

pub(super) struct CancellableRequest {
    caller_cancellation: CancellationToken,
    watcher_stop: CancellationToken,
    reported: bool,
}

impl CancellableRequest {
    pub(super) fn new(caller_cancellation: CancellationToken) -> Self {
        Self {
            caller_cancellation,
            watcher_stop: CancellationToken::new(),
            reported: false,
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.caller_cancellation.is_cancelled()
    }

    pub(super) fn mark_reported(&mut self) -> bool {
        if self.reported {
            return false;
        }
        self.reported = true;
        true
    }

    pub(super) fn spawn_watcher(&self, id: RequestId, event_tx: mpsc::Sender<DriverEvent>) {
        let caller_cancellation = self.caller_cancellation.clone();
        let watcher_stop = self.watcher_stop.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = caller_cancellation.cancelled() => {
                    let _ = event_tx.send(DriverEvent::RequestCancelled(id)).await;
                }
                _ = watcher_stop.cancelled() => {}
            }
        });
    }
}

impl Drop for CancellableRequest {
    fn drop(&mut self) {
        self.watcher_stop.cancel();
    }
}

/// Owns the watcher until an observation's final response is received.
pub(super) struct ObservationYield {
    signal: Option<CancellationToken>,
    watcher: Option<AbortOnDropHandle<()>>,
}

impl ObservationYield {
    pub(super) fn new(signal: Option<CancellationToken>) -> Self {
        Self {
            signal,
            watcher: None,
        }
    }

    pub(super) fn spawn_watcher(&mut self, id: RequestId, event_tx: mpsc::Sender<DriverEvent>) {
        let Some(signal) = self.signal.take() else {
            return;
        };
        self.watcher = Some(AbortOnDropHandle::new(tokio::spawn(async move {
            signal.cancelled().await;
            let _ = event_tx.send(DriverEvent::YieldRequest(id)).await;
        })));
    }
}

pub(super) struct InitialResponse {
    pub(super) generation: u64,
    pub(super) cell_id: WireCellId,
    pub(super) response_tx: oneshot::Sender<Result<RuntimeResponse, String>>,
    pub(super) _yield_observation: ObservationYield,
}

pub(in crate::remote_session::connection) struct DeliveredExecute {
    pub(in crate::remote_session::connection) request_id: RequestId,
    pub(in crate::remote_session::connection) started: StartedCell,
}

pub(super) struct UnclaimedExecute {
    pub(super) session: RemoteSession,
    pub(super) cell_id: WireCellId,
    pub(super) cancellation: CancellableRequest,
}

pub(super) enum PendingRequest {
    OpenSession {
        session: RemoteSession,
        cleanup: SessionCleanup,
        cancellation: CancellableRequest,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
    Execute {
        session: RemoteSession,
        delegate: Arc<dyn CodeModeSessionDelegate>,
        response_tx: oneshot::Sender<Result<DeliveredExecute, String>>,
        initial_response_tx: oneshot::Sender<Result<RuntimeResponse, String>>,
        initial_response_rx: oneshot::Receiver<Result<RuntimeResponse, String>>,
        cancellation: CancellableRequest,
        yield_observation: ObservationYield,
    },
    Wait {
        session: RemoteSession,
        cell_id: WireCellId,
        cancellation: CancellableRequest,
        yield_observation: ObservationYield,
        response_tx: oneshot::Sender<Result<WaitOutcome, String>>,
    },
    Terminate {
        session: RemoteSession,
        cell_id: WireCellId,
        response_tx: oneshot::Sender<Result<WaitOutcome, String>>,
    },
    ShutdownSession {
        session: RemoteSession,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
}

pub(super) struct DeferredWait {
    pub(super) session: RemoteSession,
    pub(super) request: WireWaitRequest,
    pub(super) caller_cancellation: CancellationToken,
    pub(super) yield_signal: Option<CancellationToken>,
    pub(super) response_tx: oneshot::Sender<Result<WaitOutcome, String>>,
}

impl PendingRequest {
    pub(super) fn cancellation_mut(&mut self) -> Option<&mut CancellableRequest> {
        match self {
            Self::OpenSession { cancellation, .. }
            | Self::Execute { cancellation, .. }
            | Self::Wait { cancellation, .. } => Some(cancellation),
            Self::Terminate { .. } | Self::ShutdownSession { .. } => None,
        }
    }

    pub(super) fn yield_observation(&mut self) -> Option<&mut ObservationYield> {
        match self {
            Self::Execute {
                yield_observation, ..
            }
            | Self::Wait {
                yield_observation, ..
            } => Some(yield_observation),
            _ => None,
        }
    }

    pub(super) fn fail(self, reason: String) {
        match self {
            Self::OpenSession { response_tx, .. } | Self::ShutdownSession { response_tx, .. } => {
                let _ = response_tx.send(Err(reason));
            }
            Self::Execute { response_tx, .. } => {
                let _ = response_tx.send(Err(reason));
            }
            Self::Wait { response_tx, .. } | Self::Terminate { response_tx, .. } => {
                let _ = response_tx.send(Err(reason));
            }
        }
    }
}
