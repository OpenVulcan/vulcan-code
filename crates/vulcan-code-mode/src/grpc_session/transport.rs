// Vendored from openai/codex (codex-rs/code-mode/src/grpc_session/transport.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
// VULCAN MODIFICATION: upstream routed URL endpoints through `codex_http_client` (proxy, custom CA
// and per-account network policy). Vulcan connects URL endpoints with tonic's own transport and
// otherwise keeps the upstream channel handling unchanged.
use tonic::Status;
use tonic::body::Body;
use tonic::codegen::http::Request;
use tonic::codegen::http::Response;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tower::ServiceExt;
use tower::util::BoxCloneSyncService;
use vulcan_code_mode_protocol::grpc::code_mode_host_client::CodeModeHostClient;
use vulcan_code_mode_protocol::host::MAX_FRAME_BYTES;

use super::GrpcClient;

pub(super) type GrpcTransport = BoxCloneSyncService<Request<Body>, Response<Body>, Status>;

pub(super) enum SharedTransport {
    Url {
        endpoint: String,
        client: tokio::sync::Mutex<Option<GrpcClient>>,
    },
    Connected(GrpcClient),
}

impl SharedTransport {
    pub(super) fn new(endpoint: String) -> Self {
        Self::Url {
            endpoint,
            client: tokio::sync::Mutex::new(None),
        }
    }

    pub(super) fn with_channel(channel: Channel) -> Self {
        Self::Connected(client_for_channel(channel))
    }

    pub(super) async fn client(&self) -> Result<GrpcClient, String> {
        let (endpoint, client) = match self {
            Self::Url { endpoint, client } => (endpoint, client),
            Self::Connected(client) => return Ok(client.clone()),
        };
        let mut cached = client.lock().await;
        if let Some(client) = cached.as_ref() {
            return Ok(client.clone());
        }
        // VULCAN MODIFICATION: URL validation mirrors upstream, but the connection itself is a
        // plain tonic channel instead of the upstream route-aware reqwest pool.
        if !endpoint.starts_with("unix:") {
            let target = reqwest::Url::parse(endpoint)
                .map_err(|error| format!("invalid gRPC code-mode host URL: {error}"))?;
            if !matches!(target.scheme(), "http" | "https") {
                return Err("gRPC code-mode host URL must use http or https".to_string());
            }
            if !target.username().is_empty() || target.password().is_some() {
                return Err("gRPC code-mode host URL must not include credentials".to_string());
            }
            if target.path() != "/" || target.query().is_some() || target.fragment().is_some() {
                return Err(
                    "gRPC code-mode host URL must not include a path, query, or fragment"
                        .to_string(),
                );
            }
        }
        let channel = Endpoint::from_shared(endpoint.clone())
            .map_err(|error| format!("invalid gRPC code-mode host endpoint: {error}"))?
            .connect_lazy();
        // VULCAN MODIFICATION: a tonic channel fails a call only before response headers arrive
        // (connect failure or a reset connection). Upstream reported those sends as
        // `Status::unavailable` so session admission retries them; keep that classification.
        let transport = channel.map_err(|error| {
            let mut status = Status::unavailable(error.to_string());
            status.set_source(std::sync::Arc::new(error));
            status
        });
        let client = client_for_transport(BoxCloneSyncService::new(transport));
        *cached = Some(client.clone());
        Ok(client)
    }
}

fn client_for_channel(channel: Channel) -> GrpcClient {
    let transport = channel.map_err(|error| Status::from_error(Box::new(error)));
    client_for_transport(BoxCloneSyncService::new(transport))
}

fn client_for_transport(transport: GrpcTransport) -> GrpcClient {
    CodeModeHostClient::new(transport)
        .max_decoding_message_size(MAX_FRAME_BYTES)
        .max_encoding_message_size(MAX_FRAME_BYTES)
}
