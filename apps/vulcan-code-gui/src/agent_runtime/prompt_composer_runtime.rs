//! Shared runtime prompt-composer access for GUI agent execution.
//! GUI 代理执行使用的共享运行时提示词组合器入口。

use std::sync::{Arc, OnceLock, RwLock};

use vulcan_prompt_composer::{
    PromptAssemblyInput, PromptAssemblyOutput, PromptCacheReport, PromptComposerEngine,
    PromptComposerError, PromptComposerResourceConfig, PromptSegment,
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
/// Latest prompt-composer report captured for diagnostics.
/// 为诊断捕获的最近一次提示词组合报告。
static LATEST_RUNTIME_PROMPT_COMPOSER_REPORT: OnceLock<
    RwLock<Option<RuntimePromptComposerReport>>,
> = OnceLock::new();

/// Prompt-composer report displayed by runtime diagnostics.
/// 运行时诊断展示的提示词组合报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimePromptComposerReport {
    /// Session identifier that produced the report.
    /// 产生报告的会话标识。
    pub session_id: String,
    /// Session title that produced the report.
    /// 产生报告的会话标题。
    pub session_title: String,
    /// Prompt assembly mode label.
    /// 提示词组合模式标签。
    pub mode_label: String,
    /// Model identifier used by the report.
    /// 报告对应的模型标识。
    pub model_id: String,
    /// Selected strategy identifier.
    /// 选中的策略标识。
    pub strategy_id: String,
    /// Selected personality identifier when any.
    /// 选中的人格标识。
    pub personality_id: Option<String>,
    /// Cache hit/miss counters.
    /// 缓存命中与未命中计数。
    pub cache_report: PromptCacheReport,
    /// Rendered segment summaries.
    /// 渲染后的片段摘要。
    pub segments: Vec<RuntimePromptComposerSegmentReport>,
    /// Fully rendered system-layer prompt text.
    /// 完整渲染后的系统层提示词文本。
    pub system_text: String,
    /// Fully rendered developer-layer prompt text.
    /// 完整渲染后的开发者层提示词文本。
    pub developer_text: String,
    /// Fully rendered tool-metadata-layer prompt text.
    /// 完整渲染后的工具元数据层提示词文本。
    pub tool_metadata_text: String,
    /// Fully rendered user-context-layer prompt text.
    /// 完整渲染后的用户上下文层提示词文本。
    pub user_text: String,
    /// Complete human-readable Markdown report.
    /// 完整的人类可读 Markdown 报告。
    pub markdown: String,
}

/// One prompt segment row displayed in diagnostics.
/// 诊断中展示的一条提示词片段记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimePromptComposerSegmentReport {
    /// Segment node identifier.
    /// 片段节点标识。
    pub node_id: String,
    /// Segment title.
    /// 片段标题。
    pub title: String,
    /// Target layer label.
    /// 目标层标签。
    pub target_label: String,
    /// Segment kind label.
    /// 片段类型标签。
    pub kind_label: String,
    /// Human-readable segment source label.
    /// 人类可读的片段来源标签。
    pub source_label: String,
    /// Whether the segment came from cache.
    /// 片段是否来自缓存。
    pub from_cache: bool,
    /// Approximate character count.
    /// 近似字符数量。
    pub character_count: usize,
    /// Full segment body text.
    /// 完整片段正文文本。
    pub body: String,
    /// Short content preview.
    /// 简短内容预览。
    pub preview: String,
}

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

/// Record the latest main-agent prompt-composer output for diagnostics.
/// 记录最近一次主代理提示词组合输出以供诊断。
pub(crate) fn record_runtime_prompt_composer_report(
    session_id: impl Into<String>,
    session_title: impl Into<String>,
    mode_label: impl Into<String>,
    model_id: impl Into<String>,
    output: &PromptAssemblyOutput,
) {
    let report = RuntimePromptComposerReport::new(
        session_id.into(),
        session_title.into(),
        mode_label.into(),
        model_id.into(),
        output,
    );
    let slot = LATEST_RUNTIME_PROMPT_COMPOSER_REPORT.get_or_init(|| RwLock::new(None));
    let mut guard = slot
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = Some(report);
}

/// Return the latest prompt-composer report, if any.
/// 返回最近一次提示词组合报告（如果存在）。
pub(crate) fn latest_runtime_prompt_composer_report() -> Option<RuntimePromptComposerReport> {
    let slot = LATEST_RUNTIME_PROMPT_COMPOSER_REPORT.get_or_init(|| RwLock::new(None));
    let guard = slot
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.clone()
}

impl RuntimePromptComposerReport {
    /// Build a diagnostic report from one composer output.
    /// 从一次组合输出构建诊断报告。
    fn new(
        session_id: String,
        session_title: String,
        mode_label: String,
        model_id: String,
        output: &PromptAssemblyOutput,
    ) -> Self {
        let segments = output
            .system_segments
            .iter()
            .chain(output.developer_segments.iter())
            .chain(output.tool_metadata_segments.iter())
            .chain(output.user_segments.iter())
            .map(RuntimePromptComposerSegmentReport::from_segment)
            .collect::<Vec<_>>();
        let cache_report = output.diagnostics.cache_report.clone();
        let strategy_id = output.diagnostics.strategy_id.clone();
        let personality_id = output.diagnostics.personality_id.clone();
        let system_text = output.render_system_text();
        let developer_text = output.render_developer_text();
        let tool_metadata_text = output.render_tool_metadata_text();
        let user_text = output.render_user_text();
        let markdown = render_prompt_composer_report_markdown(
            &session_id,
            &session_title,
            &mode_label,
            &model_id,
            &strategy_id,
            personality_id.as_deref(),
            &cache_report,
            &segments,
            &system_text,
            &developer_text,
            &tool_metadata_text,
            &user_text,
        );

        Self {
            session_id,
            session_title,
            mode_label,
            model_id,
            strategy_id,
            personality_id,
            cache_report,
            segments,
            system_text,
            developer_text,
            tool_metadata_text,
            user_text,
            markdown,
        }
    }
}

impl RuntimePromptComposerSegmentReport {
    /// Build one segment report row.
    /// 构建一条片段报告行。
    fn from_segment(segment: &PromptSegment) -> Self {
        Self {
            node_id: segment.node_id.clone(),
            title: segment.title.clone(),
            target_label: format!("{:?}", segment.target),
            kind_label: format!("{:?}", segment.kind),
            source_label: segment.source_label.clone(),
            from_cache: segment.from_cache,
            character_count: segment.body.chars().count(),
            body: segment.body.clone(),
            preview: truncate_prompt_report_preview(&segment.body, 96),
        }
    }
}

/// Render a complete prompt-composer report as Markdown.
/// 将完整提示词组合报告渲染为 Markdown。
fn render_prompt_composer_report_markdown(
    session_id: &str,
    session_title: &str,
    mode_label: &str,
    model_id: &str,
    strategy_id: &str,
    personality_id: Option<&str>,
    cache_report: &PromptCacheReport,
    segments: &[RuntimePromptComposerSegmentReport],
    system_text: &str,
    developer_text: &str,
    tool_metadata_text: &str,
    user_text: &str,
) -> String {
    let segment_rows = if segments.is_empty() {
        String::from("（无提示词片段）")
    } else {
        segments
            .iter()
            .map(|segment| {
                format!(
                    "- `{}` · {} · {} · {} chars · cache={} · source={} · {}",
                    segment.node_id,
                    segment.target_label,
                    segment.title,
                    segment.character_count,
                    if segment.from_cache { "hit" } else { "miss" },
                    segment.source_label,
                    segment.preview
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        concat!(
            "# Prompt Composer Inspector\n\n",
            "## Summary\n",
            "- Session: `{}` · {}\n",
            "- Mode: `{}`\n",
            "- Model: `{}`\n",
            "- Strategy: `{}`\n",
            "- Personality: `{}`\n",
            "- Cache: node hit `{}` / miss `{}` · bundle hit `{}` / miss `{}`\n\n",
            "## Nodes\n",
            "{}\n\n",
            "## System\n",
            "{}\n\n",
            "## Developer\n",
            "{}\n\n",
            "## Tool Metadata\n",
            "{}\n\n",
            "## User Context\n",
            "{}\n"
        ),
        session_id,
        session_title,
        mode_label,
        model_id,
        strategy_id,
        personality_id.unwrap_or("none"),
        cache_report.node_hits,
        cache_report.node_misses,
        cache_report.bundle_hits,
        cache_report.bundle_misses,
        segment_rows,
        fenced_prompt_report_text(system_text),
        fenced_prompt_report_text(developer_text),
        fenced_prompt_report_text(tool_metadata_text),
        fenced_prompt_report_text(user_text),
    )
}

/// Render text into a safe Markdown code fence.
/// 将文本渲染为安全的 Markdown 代码块。
fn fenced_prompt_report_text(text: &str) -> String {
    if text.trim().is_empty() {
        return String::from("（空）");
    }
    format!("````text\n{}\n````", text.trim())
}

/// Truncate one prompt report preview by Unicode scalar count.
/// 按 Unicode 标量数量截断一段提示词报告预览。
fn truncate_prompt_report_preview(text: &str, max_chars: usize) -> String {
    let trimmed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.chars().count() <= max_chars {
        return trimmed;
    }
    format!("{}…", trimmed.chars().take(max_chars).collect::<String>())
}

#[cfg(test)]
mod tests {
    //! Shared prompt-composer runtime tests.
    //! 共享提示词组合器运行时测试。

    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use vulcan_prompt_composer::{
        ModelDescriptor, PersonalitySelection, PromptAssemblyMode, ProviderPromptSnapshot,
        ToolCatalogSnapshot, ToolDescriptor,
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
                provider_prompt: Some(
                    ProviderPromptSnapshot::new("Provider system body")
                        .with_source_label("Provider prompt resource `test-provider.txt`"),
                ),
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

    /// Verifies prompt-composer reports capture nodes, cache counters, and rendered text.
    /// 验证提示词组合报告会捕获节点、缓存计数与渲染文本。
    #[test]
    fn prompt_composer_report_captures_segments_and_markdown() {
        let output = runtime_prompt_composer_engine()
            .compose(&PromptAssemblyInput {
                mode: PromptAssemblyMode::Primary,
                model: ModelDescriptor {
                    family: String::from("openai"),
                    model_id: String::from("gpt-test"),
                    supports_native_personality: false,
                },
                provider_prompt: Some(
                    ProviderPromptSnapshot::new("Provider system body")
                        .with_source_label("Provider prompt resource `test-provider.txt`"),
                ),
                agent_prompt: None,
                personality: PersonalitySelection::Disabled,
                project_rules: None,
                environment_context: None,
                task_goal: None,
                tool_catalog: ToolCatalogSnapshot::new(vec![ToolDescriptor {
                    id: String::from("shell_command"),
                    title: String::from("Shell Command"),
                    description: String::from("Run a shell command"),
                    help_text: None,
                    schema_fingerprint: Some(String::from("schema-1")),
                }]),
                subagent: None,
                custom_developer_segments: Vec::new(),
                custom_user_segments: Vec::new(),
            })
            .expect("runtime composer should compose");

        record_runtime_prompt_composer_report(
            "session-a",
            "Session A",
            "primary",
            "gpt-test",
            &output,
        );
        let report =
            latest_runtime_prompt_composer_report().expect("latest report should be recorded");

        assert_eq!(report.session_id, "session-a");
        assert!(
            report
                .segments
                .iter()
                .any(|segment| segment.node_id == "tool_help_shell_command")
        );
        assert!(report.markdown.contains("Prompt Composer Inspector"));
        assert_eq!(report.system_text, "Provider system body");
        assert!(report.markdown.contains("Provider system body"));
        assert!(
            report
                .segments
                .iter()
                .any(|segment| segment.body.contains("Provider system body"))
        );
        assert!(
            report
                .segments
                .iter()
                .any(|segment| segment.source_label.contains("test-provider.txt"))
        );
        assert!(
            report
                .markdown
                .contains("Use shell_command for concrete local inspection")
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
