//! Service bootstrap ai tool config helpers.
//! 服务引导 ai tool config 辅助逻辑。

use super::*;

pub(crate) fn load_service_ai_tool_configuration(
    provider_configuration: &ProviderConfigurationFile,
    logs: &mut Vec<String>,
) -> vulcan_agent_runtime::provider_config::PersistedAiToolConfiguration {
    match vulcan_agent_runtime::ai_tool_configuration::load_ai_tool_configuration_file(
        provider_configuration,
    ) {
        Ok(loaded) => {
            if loaded.normalized
                && let Err(error) =
                    vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    )
            {
                logs.push(format!(
                    "[webui-service] ai capability configuration normalization save failed: {error}"
                ));
            }
            match loaded.source {
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::DedicatedFile => logs.push(String::from(
                    "[webui-service] ai capability configuration loaded from ~/.vulcan/vulcan-ai-capabilities.json",
                )),
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::LegacyProviderFile => {
                    match vulcan_agent_runtime::ai_tool_configuration::save_ai_tool_configuration_file(
                        &loaded.configuration,
                    ) {
                        Ok(()) => logs.push(String::from(
                            "[webui-service] legacy embedded ai capability configuration migrated to ~/.vulcan/vulcan-ai-capabilities.json",
                        )),
                        Err(error) => logs.push(format!(
                            "[webui-service] legacy ai capability configuration migration failed: {error}"
                        )),
                    }
                }
                vulcan_agent_runtime::ai_tool_configuration::AiToolConfigurationLoadSource::Default => logs.push(String::from(
                    "[webui-service] ai capability configuration uses built-in defaults",
                )),
            }
            loaded.configuration
        }
        Err(error) => {
            logs.push(format!(
                "[webui-service] ai capability configuration load failed, falling back: {error}"
            ));
            if provider_configuration.ai_tools.is_default() {
                vulcan_agent_runtime::provider_config::PersistedAiToolConfiguration::default()
            } else {
                provider_configuration.ai_tools.clone()
            }
        }
    }
}
