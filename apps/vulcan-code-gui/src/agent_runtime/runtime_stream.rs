//! Runtime streaming entrypoint for provider execution and final report assembly.
//! Provider 执行与最终报告组装的运行时流式入口模块。

use crate::agent_profile::AgentProfileCatalog;
use crate::agent_runtime::events::ExecutionStreamEvent;
use crate::agent_runtime::execution_models::ExecutionPreparationRequest;
use crate::agent_runtime::runtime_stream_driver::drive_runtime_stream_launch;
use crate::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::mcp::McpRuntimeHandle;
use crate::provider_config::ProviderConfigurationFile;
use async_stream::stream;
use futures_util::{Stream, StreamExt};

/// Streams one real provider run and yields preparation, thinking, output, and final-report events incrementally.
/// 以增量方式流式执行一次真实 Provider 调用，并依次产出准备、思考、输出与最终报告事件。
pub fn stream_runtime_run(
    config: ProviderConfigurationFile,
    agent_profile_catalog: AgentProfileCatalog,
    luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    mcp_runtime: Option<McpRuntimeHandle>,
    request: ExecutionPreparationRequest,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    stream! {
        let launch = match prepare_runtime_stream_launch(
            &config,
            &agent_profile_catalog,
            luaskills_runtime.clone(),
            mcp_runtime.clone(),
            request,
        ).await {
            Ok(launch) => launch,
            Err(report) => {
                yield ExecutionStreamEvent::Finished(report);
                return;
            }
        };
        let runtime_stream = drive_runtime_stream_launch(config, launch);
        futures_util::pin_mut!(runtime_stream);
        while let Some(event) = runtime_stream.next().await {
            yield event;
        }
    }
}
