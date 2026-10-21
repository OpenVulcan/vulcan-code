// VULCAN MODIFICATION: upstream propagated W3C trace context between the client and the host
// through `codex_otel`. Vulcan does not export OpenTelemetry traces, so these functions keep the
// upstream call sites intact while carrying no trace context. The `traceparent` gRPC metadata and
// proto fields stay in the wire contract and are simply left empty.

/// W3C trace context, shaped like `codex_protocol::protocol::W3cTraceContext`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct W3cTraceContext {
    pub traceparent: Option<String>,
    pub tracestate: Option<String>,
}

/// Returns no trace context; Vulcan does not export spans.
pub fn span_w3c_trace_context(_span: &tracing::Span) -> Option<W3cTraceContext> {
    None
}

/// Returns no trace context; Vulcan does not export spans.
pub fn current_span_w3c_trace_context() -> Option<W3cTraceContext> {
    None
}

/// Ignores the remote parent; Vulcan does not export spans.
pub fn set_parent_from_w3c_trace_context(_span: &tracing::Span, _trace: &W3cTraceContext) {}
