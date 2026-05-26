//! Shared runtime prompt-composer access for GUI agent execution.
//! GUI 代理执行使用的共享运行时提示词组合器入口。

use std::sync::{Arc, OnceLock, RwLock};

use vulcan_prompt_composer::{
    PromptAssemblyInput, PromptComposerEngine, PromptComposerError, PromptComposerResourceConfig,
};

/// User-level Vulcan root directory name.
/// 用户级 Vulcan 根目录名。
const USER_VULCAN_DIR_NAME: &str = ".vulcan";
/// User-level Vulcan Code namespace directory name.
/// 用户级 Vulcan Code 命名空间目录名。
const USER_VULCAN_CODE_DIR_NAME: &str = "code";
/// Prompt-composer resource directory name.
/// 提示词组合资源目录名。
const PROMPT_RESOURCE_DIR_NAME: &str = "prompt";
/// Custom personality resource directory name.
/// 自定义人格资源目录名。
const PROMPT_PERSONALITY_DIR_NAME: &str = "personalities";
/// Tool-help resource directory name.
/// 工具帮助资源目录名。
const PROMPT_TOOL_HELP_DIR_NAME: &str = "tool-help";

/// Process-wide prompt composer used by main agents and subagents.
/// 主代理与子代理共享的进程级提示词组合器。
static RUNTIME_PROMPT_COMPOSER_ENGINE: OnceLock<RwLock<Arc<PromptComposerEngine>>> =
    OnceLock::new();

/// Return the shared runtime prompt composer engine.
/// 返回共享运行时提示词组合器引擎。
pub(crate) fn runtime_prompt_composer_engine() -> Arc<PromptComposerEngine> {
    let slot = RUNTIME_PROMPT_COMPOSER_ENGINE
        .get_or_init(|| RwLock::new(Arc::new(build_default_runtime_prompt_composer_engine())));
    let guard = slot
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Arc::clone(&guard)
}

/// Build the default runtime prompt-composer engine.
/// 构建默认运行时提示词组合器引擎。
fn build_default_runtime_prompt_composer_engine() -> PromptComposerEngine {
    default_prompt_composer_resource_config()
        .as_ref()
        .and_then(|resources| PromptComposerEngine::with_resources(resources).ok())
        .unwrap_or_else(PromptComposerEngine::with_builtins)
}

/// Return default user-level prompt-composer resource directories.
/// 返回默认用户级提示词组合资源目录。
fn default_prompt_composer_resource_config() -> Option<PromptComposerResourceConfig> {
    let prompt_root = dirs::home_dir()?
        .join(USER_VULCAN_DIR_NAME)
        .join(USER_VULCAN_CODE_DIR_NAME)
        .join(PROMPT_RESOURCE_DIR_NAME);
    Some(PromptComposerResourceConfig {
        personality_directory: Some(prompt_root.join(PROMPT_PERSONALITY_DIR_NAME)),
        tool_help_directory: Some(prompt_root.join(PROMPT_TOOL_HELP_DIR_NAME)),
    })
}

/// Reload the shared runtime prompt composer from resource directories.
/// 根据资源目录重载共享运行时提示词组合器。
#[allow(dead_code)]
pub(crate) fn reload_runtime_prompt_composer_engine(
    resources: &PromptComposerResourceConfig,
) -> Result<(), PromptComposerError> {
    let next_engine = Arc::new(PromptComposerEngine::with_resources(resources)?);
    let slot = RUNTIME_PROMPT_COMPOSER_ENGINE
        .get_or_init(|| RwLock::new(Arc::new(build_default_runtime_prompt_composer_engine())));
    let mut guard = slot
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = next_engine;
    Ok(())
}

/// Compose one prompt through the current shared engine.
/// 通过当前共享引擎组合一份提示词。
#[allow(dead_code)]
pub(crate) fn compose_with_runtime_prompt_composer(
    input: &PromptAssemblyInput,
) -> Result<vulcan_prompt_composer::PromptAssemblyOutput, PromptComposerError> {
    runtime_prompt_composer_engine().compose(input)
}

#[cfg(test)]
mod tests {
    //! Shared prompt-composer runtime tests.
    //! 共享提示词组合器运行时测试。

    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use vulcan_prompt_composer::{
        ModelDescriptor, PersonalitySelection, PromptAssemblyMode, ToolCatalogSnapshot,
        ToolDescriptor,
    };

    /// Verifies reloading resources makes custom personalities and tool help visible.
    /// 验证重载资源后自定义人格与工具帮助可见。
    #[test]
    fn prompt_composer_runtime_reloads_resource_directories() {
        let root = unique_temp_dir("vulcan-runtime-prompt-composer-resources");
        let personality_dir = root.join("personalities");
        let tool_help_dir = root.join("tool-help");
        fs::create_dir_all(&personality_dir).expect("create personality directory");
        fs::create_dir_all(&tool_help_dir).expect("create tool help directory");
        fs::write(
            personality_dir.join("custom.md"),
            "Custom runtime personality.",
        )
        .expect("write custom personality");
        fs::write(
            tool_help_dir.join("custom_tool.md"),
            "Custom runtime tool guidance.",
        )
        .expect("write custom tool guidance");

        reload_runtime_prompt_composer_engine(&PromptComposerResourceConfig {
            personality_directory: Some(personality_dir),
            tool_help_directory: Some(tool_help_dir),
        })
        .expect("runtime composer should reload resources");

        let output = runtime_prompt_composer_engine()
            .compose(&PromptAssemblyInput {
                mode: PromptAssemblyMode::Primary,
                model: ModelDescriptor {
                    family: String::from("openai"),
                    model_id: String::from("gpt-test"),
                    supports_native_personality: false,
                },
                provider_prompt: None,
                agent_prompt: None,
                personality: PersonalitySelection::Named(String::from("custom")),
                project_rules: None,
                environment_context: None,
                task_goal: None,
                tool_catalog: ToolCatalogSnapshot::new(vec![ToolDescriptor {
                    id: String::from("custom_tool"),
                    title: String::from("Custom Tool"),
                    description: String::from("Custom tool description"),
                    help_text: None,
                    schema_fingerprint: None,
                }]),
                subagent: None,
                custom_developer_segments: Vec::new(),
                custom_user_segments: Vec::new(),
            })
            .expect("runtime composer should compose with custom resources");

        assert!(
            output
                .render_developer_text()
                .contains("Custom runtime personality.")
        );
        assert!(
            output
                .render_tool_metadata_text()
                .contains("Custom runtime tool guidance.")
        );

        reload_runtime_prompt_composer_engine(&PromptComposerResourceConfig::default())
            .expect("runtime composer should reset to built-ins");
        let _ = fs::remove_dir_all(root);
    }

    /// Verifies default prompt resource directories use the user `.vulcan/code` namespace.
    /// 验证默认提示词资源目录使用用户级 `.vulcan/code` 命名空间。
    #[test]
    fn default_prompt_resource_directories_use_vulcan_code_namespace() {
        let Some(resources) = default_prompt_composer_resource_config() else {
            return;
        };
        let personality_directory = resources
            .personality_directory
            .expect("personality directory should be available when home exists");
        let tool_help_directory = resources
            .tool_help_directory
            .expect("tool-help directory should be available when home exists");

        assert!(
            personality_directory.ends_with(Path::new(".vulcan/code/prompt/personalities")),
            "{}",
            personality_directory.display()
        );
        assert!(
            tool_help_directory.ends_with(Path::new(".vulcan/code/prompt/tool-help")),
            "{}",
            tool_help_directory.display()
        );
    }

    /// Builds one unique temporary directory for tests.
    /// 为测试构建一个唯一临时目录。
    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{suffix}"))
    }
}
