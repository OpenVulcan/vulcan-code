//! Provider base prompt loading helpers for the execution runtime.
//! 执行运行时使用的 Provider 基础提示词加载辅助模块。

use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::runtime_assets;
use std::{fs, path::Path};

/// Relative resource directory name that stores provider base system prompts.
/// 存放 Provider 基础系统提示词的相对资源目录名称。
const SYSTEM_PROMPTS_DIR_NAME: &str = "system-prompts";

/// Default provider prompt file name.
/// 默认 Provider 提示词文件名。
const DEFAULT_SYSTEM_PROMPT_FILE_NAME: &str = "default.txt";

/// GPT-family provider prompt file name.
/// GPT 家族 Provider 提示词文件名。
const GPT_SYSTEM_PROMPT_FILE_NAME: &str = "gpt.txt";

/// Codex-family provider prompt file name.
/// Codex 家族 Provider 提示词文件名。
const CODEX_SYSTEM_PROMPT_FILE_NAME: &str = "codex.txt";

/// Beast-style provider prompt file name.
/// Beast 风格 Provider 提示词文件名。
const BEAST_SYSTEM_PROMPT_FILE_NAME: &str = "beast.txt";

/// Anthropic provider prompt file name.
/// Anthropic Provider 提示词文件名。
const ANTHROPIC_SYSTEM_PROMPT_FILE_NAME: &str = "anthropic.txt";

/// Gemini provider prompt file name.
/// Gemini Provider 提示词文件名。
const GEMINI_SYSTEM_PROMPT_FILE_NAME: &str = "gemini.txt";

/// Kimi-family provider prompt file name.
/// Kimi 家族 Provider 提示词文件名。
const KIMI_SYSTEM_PROMPT_FILE_NAME: &str = "kimi.txt";

/// Trinity-family provider prompt file name.
/// Trinity 家族 Provider 提示词文件名。
const TRINITY_SYSTEM_PROMPT_FILE_NAME: &str = "trinity.txt";

/// Loaded provider base system prompt and its source label.
/// 已加载的 Provider 基础系统提示词及其来源标签。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedProviderBaseSystemPrompt {
    /// Sanitized prompt body text.
    /// 已清洗的提示词正文。
    pub text: String,
    /// Human-readable source label for diagnostics.
    /// 供诊断使用的人类可读来源标签。
    pub source_label: String,
}

/// Loads one provider-family base system prompt from the editable resource directory.
/// 从可编辑资源目录中加载对应 Provider 家族的基础系统提示词。
pub(crate) fn load_provider_base_system_prompt(
    blueprint: &ProviderRuntimeBlueprint,
) -> Option<String> {
    load_provider_base_system_prompt_with_source(blueprint).map(|prompt| prompt.text)
}

/// Loads one provider-family base system prompt with source information.
/// 加载一份带来源信息的 Provider 家族基础系统提示词。
pub(crate) fn load_provider_base_system_prompt_with_source(
    blueprint: &ProviderRuntimeBlueprint,
) -> Option<LoadedProviderBaseSystemPrompt> {
    let file_name = resolve_provider_base_prompt_file_name(blueprint);
    let packaged_prompt_path = runtime_assets::resolve_app_resources_directory()
        .join(SYSTEM_PROMPTS_DIR_NAME)
        .join(file_name);
    if let Some(prompt) = read_non_empty_system_prompt_file(&packaged_prompt_path) {
        return Some(LoadedProviderBaseSystemPrompt {
            text: prompt,
            source_label: format!(
                "Provider prompt resource `{}`",
                packaged_prompt_path.display()
            ),
        });
    }

    let source_prompt_path = runtime_assets::source_resource_directory()
        .join(SYSTEM_PROMPTS_DIR_NAME)
        .join(file_name);
    if source_prompt_path != packaged_prompt_path
        && let Some(prompt) = read_non_empty_system_prompt_file(&source_prompt_path)
    {
        return Some(LoadedProviderBaseSystemPrompt {
            text: prompt,
            source_label: format!(
                "Provider prompt resource `{}`",
                source_prompt_path.display()
            ),
        });
    }

    if file_name != DEFAULT_SYSTEM_PROMPT_FILE_NAME {
        let packaged_default_path = runtime_assets::resolve_app_resources_directory()
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);
        if let Some(prompt) = read_non_empty_system_prompt_file(&packaged_default_path) {
            return Some(LoadedProviderBaseSystemPrompt {
                text: prompt,
                source_label: format!(
                    "Provider default prompt resource `{}`",
                    packaged_default_path.display()
                ),
            });
        }

        let source_default_path = runtime_assets::source_resource_directory()
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);
        if source_default_path != packaged_default_path
            && let Some(prompt) = read_non_empty_system_prompt_file(&source_default_path)
        {
            return Some(LoadedProviderBaseSystemPrompt {
                text: prompt,
                source_label: format!(
                    "Provider default prompt resource `{}`",
                    source_default_path.display()
                ),
            });
        }
    }

    None
}

/// Resolves one provider/model combination into the matching base prompt file name.
/// 将单个 Provider/模型组合解析为对应的基础提示词文件名。
fn resolve_provider_base_prompt_file_name(blueprint: &ProviderRuntimeBlueprint) -> &'static str {
    let model_id = blueprint.model_id.to_ascii_lowercase();

    if model_id.contains("gpt-4") || model_id.contains("o1") || model_id.contains("o3") {
        return BEAST_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("gpt") {
        if model_id.contains("codex") {
            return CODEX_SYSTEM_PROMPT_FILE_NAME;
        }
        return GPT_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("gemini-") {
        return GEMINI_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("claude") {
        return ANTHROPIC_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("trinity") {
        return TRINITY_SYSTEM_PROMPT_FILE_NAME;
    }
    if model_id.contains("kimi") {
        return KIMI_SYSTEM_PROMPT_FILE_NAME;
    }
    DEFAULT_SYSTEM_PROMPT_FILE_NAME
}

/// Reads one prompt text file, removes duplicated legacy tool guidance, and trims the outer blank region.
/// 读取单个提示词文本文件，移除重复的旧工具引导段，并裁剪外围空白区域。
fn read_non_empty_system_prompt_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|content| sanitize_provider_base_system_prompt(&content))
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
}

/// Removes legacy tool-registration and foreign-tool sections from provider base prompts.
/// 从 Provider 基础提示词中移除旧工具注册与外部工具段落。
fn sanitize_provider_base_system_prompt(content: &str) -> String {
    let mut sanitized_lines = Vec::new();
    let mut skipped_heading_level: Option<usize> = None;

    for line in content.lines() {
        if let Some((heading_level, heading_title)) = parse_markdown_heading(line) {
            if skipped_heading_level
                .map(|level| heading_level <= level)
                .unwrap_or(false)
            {
                skipped_heading_level = None;
            }
            if should_skip_prompt_section(&heading_title) {
                skipped_heading_level = Some(heading_level);
                continue;
            }
        }

        if skipped_heading_level.is_some() {
            continue;
        }

        if is_legacy_tool_catalog_line(line) {
            if let Some((heading_level, _)) = parse_markdown_heading(line) {
                skipped_heading_level = Some(heading_level);
            }
            continue;
        }

        sanitized_lines.push(line);
    }

    sanitized_lines.join("\n")
}

/// Parses one Markdown ATX heading into a level and normalized title.
/// 将单个 Markdown ATX 标题解析为层级与规范化标题。
fn parse_markdown_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let heading_level = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if heading_level == 0usize || heading_level > 6usize {
        return None;
    }
    if !trimmed
        .chars()
        .nth(heading_level)
        .map(char::is_whitespace)
        .unwrap_or(false)
    {
        return None;
    }
    let heading_title = trimmed[heading_level..]
        .trim()
        .trim_matches('#')
        .trim()
        .to_ascii_lowercase();
    Some((heading_level, heading_title))
}

/// Returns whether one provider prompt section duplicates runtime-registered tool behavior.
/// 返回某个 Provider 提示词段落是否重复描述运行时已注册的工具行为。
fn should_skip_prompt_section(heading_title: &str) -> bool {
    matches!(
        heading_title,
        "prompt and tool use" | "task management" | "tool usage" | "tool usage policy"
    )
}

/// Returns whether one prompt line belongs to an old injected tool catalog block.
/// 返回某行提示词是否属于旧版注入工具目录块。
fn is_legacy_tool_catalog_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains("AGENT OVERRIDE PROMPT")
        || trimmed.contains("原生工具已真实注册")
        || trimmed.contains("LuaSkills 工具目录")
        || trimmed.contains("MCP 工具已真实注册")
        || trimmed.contains("可委派代理")
        || trimmed.contains("Glob and Grep tools")
        || trimmed.contains("multi_tool_use.parallel")
        || trimmed.contains("Use Read to view files")
        || trimmed.contains("Use Glob to find files")
        || trimmed.contains("Use Bash for terminal operations")
        || trimmed.contains("TodoWrite")
        || trimmed.contains("Task tool")
        || trimmed.contains("available tools (e.g.")
}

#[cfg(test)]
mod tests {
    //! Provider prompt sanitizer regression tests.
    //! Provider 提示词清洗器回归测试。

    use super::sanitize_provider_base_system_prompt;

    /// Removes stale tool-policy sections while preserving later product guidance.
    /// 移除陈旧工具策略段落，同时保留后续产品引导。
    #[test]
    fn sanitizer_removes_tool_policy_sections() {
        let raw = "\
# Tone and style
Keep this guidance.

# Tool usage policy
- Prefer the Task tool.

# Code References
Keep code reference guidance.";

        let sanitized = sanitize_provider_base_system_prompt(raw);

        assert!(sanitized.contains("# Tone and style"));
        assert!(sanitized.contains("# Code References"));
        assert!(!sanitized.contains("Prefer the Task tool"));
        assert!(!sanitized.contains("# Tool usage policy"));
    }

    /// Removes legacy injected tool-catalog headings without dropping following unrelated sections.
    /// 移除旧版注入工具目录标题，同时不丢弃后续无关段落。
    #[test]
    fn sanitizer_removes_legacy_tool_catalog_sections() {
        let raw = "\
# AGENT OVERRIDE PROMPT
Do not keep this injected prompt.

# Tone
Keep tone guidance.

## LuaSkills 工具目录
- stale skill catalog

## Code References
Keep references.";

        let sanitized = sanitize_provider_base_system_prompt(raw);

        assert!(sanitized.contains("# Tone"));
        assert!(sanitized.contains("## Code References"));
        assert!(!sanitized.contains("Do not keep this injected prompt"));
        assert!(!sanitized.contains("stale skill catalog"));
    }

    /// Removes stale foreign-tool instruction lines that are not backed by the current runtime registry.
    /// 移除当前运行时注册表不支持的陈旧外部工具指令行。
    #[test]
    fn sanitizer_removes_legacy_foreign_tool_instruction_lines() {
        let raw = "\
Intro.
- When searching for text or files, prefer using Glob and Grep tools.
- Parallelize tool calls with multi_tool_use.parallel.
Tail.";

        let sanitized = sanitize_provider_base_system_prompt(raw);

        assert!(sanitized.contains("Intro."));
        assert!(sanitized.contains("Tail."));
        assert!(!sanitized.contains("Glob and Grep tools"));
        assert!(!sanitized.contains("multi_tool_use.parallel"));
    }
}
