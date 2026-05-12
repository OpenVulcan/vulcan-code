//! Provider transport facade that re-exports the provider-specific host-controlled adapters.
//! 重新导出各 Provider 专用宿主控制适配器的 Provider 传输门面模块。

pub(crate) use crate::agent_runtime::provider_transport_anthropic_openrouter::{
    execute_anthropic_stream, execute_openrouter_stream,
};
pub(crate) use crate::agent_runtime::provider_transport_openai::{
    execute_openai_compatible_stream, execute_openai_responses_stream,
};
