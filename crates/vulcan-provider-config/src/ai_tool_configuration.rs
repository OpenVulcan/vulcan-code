//! Dedicated persistence helpers for the standalone AI capability tool configuration.
//! 独立 AI 能力工具配置的专用持久化辅助模块。

use crate::provider_config::{PersistedAiToolConfiguration, ProviderConfigurationFile};
use std::fs;
use std::path::{Path, PathBuf};

/// Dedicated load source used to explain where the effective AI tool configuration came from.
/// 用于说明生效 AI 工具配置来源位置的独立加载来源枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiToolConfigurationLoadSource {
    /// The dedicated standalone AI capability configuration file.
    /// 独立 AI 能力配置文件。
    DedicatedFile,
    /// The legacy embedded provider configuration field kept only for one-time migration.
    /// 仅为一次性迁移保留的旧版供应商内嵌配置字段。
    LegacyProviderFile,
    /// The built-in default configuration used when no persisted state exists.
    /// 当不存在任何持久化状态时使用的内置默认配置。
    Default,
}

/// Loaded AI tool configuration payload paired with its resolved source metadata.
/// 携带已解析来源元数据的 AI 工具配置载荷。
#[derive(Debug, Clone)]
pub struct LoadedAiToolConfiguration {
    /// Effective AI tool configuration snapshot used by the host runtime.
    /// 供宿主运行时使用的生效 AI 工具配置快照。
    pub configuration: PersistedAiToolConfiguration,
    /// Resolved source that produced the effective configuration snapshot.
    /// 产出生效配置快照的已解析来源。
    pub source: AiToolConfigurationLoadSource,
    /// Whether the loaded configuration had to be normalized into the canonical standalone shape.
    /// 已加载配置是否需要被归一化为规范独立形态。
    pub normalized: bool,
}

/// Returns the dedicated standalone AI capability configuration file path.
/// 返回独立 AI 能力配置文件路径。
pub fn ai_tool_configuration_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_path_buf()
    });

    home.join(".vulcan").join("vulcan-ai-capabilities.json")
}

/// Loads the standalone AI capability configuration, falling back to the legacy embedded field only when the dedicated file is absent.
/// 加载独立 AI 能力配置；仅在独立文件缺失时才回退到旧版内嵌字段。
pub fn load_ai_tool_configuration_file(
    provider_configuration: &ProviderConfigurationFile,
) -> Result<LoadedAiToolConfiguration, String> {
    let path = ai_tool_configuration_file_path();

    if path.exists() {
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("读取 AI 能力配置文件失败：{}。", error))?;
        let mut configuration: PersistedAiToolConfiguration = serde_json::from_str(&text)
            .map_err(|error| format!("解析 AI 能力配置文件失败：{}。", error))?;
        let normalized = configuration.normalize();
        return Ok(LoadedAiToolConfiguration {
            configuration,
            source: AiToolConfigurationLoadSource::DedicatedFile,
            normalized,
        });
    }

    Ok(resolve_fallback_ai_tool_configuration(
        provider_configuration,
    ))
}

/// Saves the standalone AI capability configuration file back to disk, creating the parent directory when needed.
/// 将独立 AI 能力配置文件写回磁盘，并在需要时创建父目录。
pub fn save_ai_tool_configuration_file(
    configuration: &PersistedAiToolConfiguration,
) -> Result<(), String> {
    let path = ai_tool_configuration_file_path();
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    fs::create_dir_all(&parent)
        .map_err(|error| format!("创建 AI 能力配置目录失败：{}。", error))?;

    let content = serde_json::to_string_pretty(configuration)
        .map_err(|error| format!("序列化 AI 能力配置失败：{}。", error))?;

    fs::write(&path, content).map_err(|error| format!("写入 AI 能力配置失败：{}。", error))
}

/// Resolves the best-effort fallback AI capability configuration when the dedicated file does not exist yet.
/// 在独立文件尚不存在时解析最合适的回退 AI 能力配置。
fn resolve_fallback_ai_tool_configuration(
    provider_configuration: &ProviderConfigurationFile,
) -> LoadedAiToolConfiguration {
    if !provider_configuration.ai_tools.is_default() {
        let mut configuration = provider_configuration.ai_tools.clone();
        let normalized = configuration.normalize();
        return LoadedAiToolConfiguration {
            configuration,
            source: AiToolConfigurationLoadSource::LegacyProviderFile,
            normalized,
        };
    }

    LoadedAiToolConfiguration {
        configuration: PersistedAiToolConfiguration::default(),
        source: AiToolConfigurationLoadSource::Default,
        normalized: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{AiToolConfigurationLoadSource, resolve_fallback_ai_tool_configuration};
    use crate::provider_config::{
        PersistedAiToolConfiguration, PersistedWebSearchConfiguration, ProviderConfigurationFile,
        WebSearchRoutingMode,
    };

    /// Prefers the legacy embedded AI tool configuration only when it actually carries user data.
    /// 仅当旧版内嵌 AI 工具配置确实携带用户数据时，才优先使用该旧配置。
    #[test]
    fn fallback_loader_uses_legacy_embedded_ai_tool_configuration() {
        let provider_configuration = ProviderConfigurationFile {
            ai_tools: PersistedAiToolConfiguration {
                web_search: PersistedWebSearchConfiguration {
                    mode: WebSearchRoutingMode::ThirdPartyOnly,
                    ..PersistedWebSearchConfiguration::default()
                },
                ..PersistedAiToolConfiguration::default()
            },
            ..ProviderConfigurationFile::default()
        };

        let loaded = resolve_fallback_ai_tool_configuration(&provider_configuration);

        assert_eq!(
            loaded.source,
            AiToolConfigurationLoadSource::LegacyProviderFile
        );
        assert_eq!(loaded.configuration, provider_configuration.ai_tools);
    }

    /// Falls back to the built-in default configuration when no dedicated file or legacy embedded data is available.
    /// 当既没有独立文件也没有旧版内嵌数据时，回退到内置默认配置。
    #[test]
    fn fallback_loader_uses_default_when_legacy_embedded_state_is_empty() {
        let loaded = resolve_fallback_ai_tool_configuration(&ProviderConfigurationFile::default());

        assert_eq!(loaded.source, AiToolConfigurationLoadSource::Default);
        assert_eq!(
            loaded.configuration,
            PersistedAiToolConfiguration::default()
        );
    }
}
