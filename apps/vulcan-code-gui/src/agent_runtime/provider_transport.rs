//! Provider transport facade that re-exports the owned transport request builder and routing entrypoint.
//! 重新导出自有传输请求构建器与路由入口的 Provider 传输门面模块。

pub(crate) use crate::agent_runtime::provider_transport_request::build_provider_transport_execution_request;
pub(crate) use crate::agent_runtime::provider_transport_router::execute_provider_transport_stream;
