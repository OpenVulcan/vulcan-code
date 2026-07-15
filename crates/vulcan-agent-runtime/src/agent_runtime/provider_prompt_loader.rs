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
///
/// # Parameters
/// # 参数
/// - `blueprint`: Resolved provider blueprint whose model id selects the exact resource file.
/// - `blueprint`：其模型标识选择精确资源文件的已解析 Provider 蓝图。
///
/// # Returns
/// # 返回值
/// Returns sanitized prompt text, or an exact resource-path failure.
/// 返回清洗后的提示词文本，或带精确资源路径的失败。
pub(crate) fn load_provider_base_system_prompt(
    blueprint: &ProviderRuntimeBlueprint,
) -> Result<String, String> {
    load_provider_base_system_prompt_with_source(blueprint).map(|prompt| prompt.text)
}

/// Loads one provider-family base system prompt with source information.
/// 加载一份带来源信息的 Provider 家族基础系统提示词。
///
/// # Parameters
/// # 参数
/// - `blueprint`: Resolved provider blueprint whose model id selects the exact resource file.
/// - `blueprint`：其模型标识选择精确资源文件的已解析 Provider 蓝图。
///
/// # Returns
/// # 返回值
/// Returns sanitized prompt text with its unique source label, or an exact resource failure.
/// 返回带唯一来源标签的清洗后提示词文本，或精确资源失败。
pub(crate) fn load_provider_base_system_prompt_with_source(
    blueprint: &ProviderRuntimeBlueprint,
) -> Result<LoadedProviderBaseSystemPrompt, String> {
    // resources_directory is the single effective root selected by the runtime-assets domain.
    // resources_directory 是 runtime-assets 领域选择的唯一生效资源根目录。
    let resources_directory = runtime_assets::resolve_app_resources_directory();
    load_provider_base_system_prompt_from_directory(&blueprint.model_id, &resources_directory)
}

/// Loads the exact model-mapped provider prompt below one already selected resource directory.
/// 从一个已选定资源目录下加载模型精确映射的 Provider 提示词。
///
/// # Parameters
/// # 参数
/// - `model_id`: Stable model identifier used by the static file mapping.
/// - `model_id`：静态文件映射使用的稳定模型标识。
/// - `resources_directory`: Effective application resource root selected by the caller.
/// - `resources_directory`：调用方选择的生效应用资源根目录。
///
/// # Returns
/// # 返回值
/// Returns the sanitized prompt and exact source label, or the selected file's failure.
/// 返回清洗后的提示词及精确来源标签，或所选文件的失败。
fn load_provider_base_system_prompt_from_directory(
    model_id: &str,
    resources_directory: &Path,
) -> Result<LoadedProviderBaseSystemPrompt, String> {
    // file_name is the unique static resource selected for the model family.
    // file_name 是为模型家族选择的唯一静态资源文件名。
    let file_name = resolve_provider_base_prompt_file_name(model_id);
    // prompt_path is the sole file allowed by the effective resource contract.
    // prompt_path 是生效资源契约允许的唯一文件。
    let prompt_path = resources_directory
        .join(SYSTEM_PROMPTS_DIR_NAME)
        .join(file_name);
    // prompt is valid only after UTF-8 decoding, sanitization, and nonempty validation.
    // prompt 仅在完成 UTF-8 解码、清洗与非空校验后才有效。
    let prompt = read_non_empty_system_prompt_file(&prompt_path)?;
    Ok(LoadedProviderBaseSystemPrompt {
        text: prompt,
        source_label: format!("Provider prompt resource `{}`", prompt_path.display()),
    })
}

/// Resolves one provider/model combination into the matching base prompt file name.
/// 将单个 Provider/模型组合解析为对应的基础提示词文件名。
///
/// # Parameters
/// # 参数
/// - `model_id`: Stable model identifier whose family selects one packaged prompt file.
/// - `model_id`：其家族选择单个打包提示词文件的稳定模型标识。
///
/// # Returns
/// # 返回值
/// Returns the unique static file name assigned to the model family.
/// 返回分配给该模型家族的唯一静态文件名。
fn resolve_provider_base_prompt_file_name(model_id: &str) -> &'static str {
    let model_id = model_id.to_ascii_lowercase();

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
///
/// # Parameters
/// # 参数
/// - `path`: Exact model-mapped resource file selected by the effective resource root.
/// - `path`：由生效资源根选择的精确模型映射资源文件。
///
/// # Returns
/// # 返回值
/// Returns sanitized nonempty UTF-8 prompt text, or an exact read or content-validation failure.
/// 返回清洗后的非空 UTF-8 提示词文本，或精确的读取与内容校验失败。
fn read_non_empty_system_prompt_file(path: &Path) -> Result<String, String> {
    // content is the exact UTF-8 resource body selected for the current model.
    // content 是为当前模型选择的精确 UTF-8 资源正文。
    let content = fs::read_to_string(path).map_err(|error| {
        format!(
            "Failed to read provider prompt resource `{}`: {error}",
            path.display()
        )
    })?;
    if content.trim().is_empty() {
        return Err(format!(
            "Provider prompt resource `{}` is empty.",
            path.display()
        ));
    }
    // sanitized contains only prompt guidance supported by the current runtime registry.
    // sanitized 仅包含当前运行时注册表支持的提示指引。
    let sanitized = sanitize_provider_base_system_prompt(&content);
    // prompt is the final outer-trimmed resource submitted to prompt composition.
    // prompt 是提交给提示组合器的最终外围裁剪资源。
    let prompt = sanitized.trim().to_string();
    if prompt.is_empty() {
        return Err(format!(
            "Provider prompt resource `{}` is empty after sanitization.",
            path.display()
        ));
    }
    Ok(prompt)
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

    use super::*;

    /// Builds one absent, process-unique resource root below the host temp directory.
    /// 在宿主临时目录下构建一个不存在且进程内唯一的资源根目录。
    ///
    /// # Parameters
    /// # 参数
    /// - `label`: Stable test label included in the generated directory name.
    /// - `label`：包含在生成目录名称中的稳定测试标签。
    ///
    /// # Returns
    /// # 返回值
    /// Returns an absent resource-root path suitable for isolated loader tests.
    /// 返回适用于隔离加载器测试的不存在资源根路径。
    fn unique_test_resource_root(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "vulcan-provider-prompt-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("test clock should follow the Unix epoch")
                .as_nanos()
        ))
    }

    /// Verifies every supported model family maps to one deterministic resource file.
    /// 验证每个受支持模型家族都会映射到一个确定的资源文件。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if a model family selects the wrong file.
    /// 此测试无返回值；若模型家族选择错误文件则失败。
    #[test]
    fn model_families_map_to_exact_prompt_files() {
        // cases captures the complete precedence order implemented by the static mapping.
        // cases 捕获静态映射实现的完整优先级顺序。
        let cases = [
            ("gpt-4o", BEAST_SYSTEM_PROMPT_FILE_NAME),
            ("o1-pro", BEAST_SYSTEM_PROMPT_FILE_NAME),
            ("o3-mini", BEAST_SYSTEM_PROMPT_FILE_NAME),
            ("gpt-5-codex", CODEX_SYSTEM_PROMPT_FILE_NAME),
            ("gpt-5", GPT_SYSTEM_PROMPT_FILE_NAME),
            ("gemini-2.5-pro", GEMINI_SYSTEM_PROMPT_FILE_NAME),
            ("claude-sonnet-4", ANTHROPIC_SYSTEM_PROMPT_FILE_NAME),
            ("trinity-large", TRINITY_SYSTEM_PROMPT_FILE_NAME),
            ("kimi-k2", KIMI_SYSTEM_PROMPT_FILE_NAME),
            ("deepseek-v4", DEFAULT_SYSTEM_PROMPT_FILE_NAME),
        ];

        for (model_id, expected_file_name) in cases {
            assert_eq!(
                resolve_provider_base_prompt_file_name(model_id),
                expected_file_name
            );
        }
    }

    /// Verifies the loader reads only the exact model-mapped resource and preserves its source label.
    /// 验证加载器只读取精确的模型映射资源，并保留其来源标签。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if loading changes the selected file or skips sanitization.
    /// 此测试无返回值；若加载过程改变所选文件或跳过清洗则失败。
    #[test]
    fn loader_reads_exact_model_resource() {
        // resource_root isolates the selected-resource fixture from every other test.
        // resource_root 将所选资源夹具与其他所有测试隔离。
        let resource_root = unique_test_resource_root("exact");
        // prompt_path is the unique Codex resource selected by the model mapping.
        // prompt_path 是模型映射选择的唯一 Codex 资源。
        let prompt_path = resource_root
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(CODEX_SYSTEM_PROMPT_FILE_NAME);
        fs::create_dir_all(
            prompt_path
                .parent()
                .expect("provider prompt should have a parent"),
        )
        .expect("create provider prompt directory");
        fs::write(
            &prompt_path,
            "# Tone\nKeep this.\n\n# Tool usage policy\nRemove this.",
        )
        .expect("write provider prompt");

        // loaded is the validated prompt and source identity returned by the loader.
        // loaded 是加载器返回的已验证提示词与来源身份。
        let loaded = load_provider_base_system_prompt_from_directory("gpt-5-codex", &resource_root)
            .expect("exact provider prompt should load");

        assert!(loaded.text.contains("Keep this."));
        assert!(!loaded.text.contains("Remove this."));
        assert!(
            loaded
                .source_label
                .contains(&prompt_path.display().to_string())
        );
        fs::remove_dir_all(resource_root).expect("remove exact-resource fixture");
    }

    /// Verifies a missing family resource does not fall back to an existing default resource.
    /// 验证家族资源缺失时不会回退到现有默认资源。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if a damaged resource set still loads successfully.
    /// 此测试无返回值；若损坏资源集仍成功加载则失败。
    #[test]
    fn loader_rejects_missing_exact_resource_without_default_fallback() {
        // resource_root contains a valid default file but deliberately omits the selected Codex file.
        // resource_root 包含有效默认文件，但刻意缺失所选 Codex 文件。
        let resource_root = unique_test_resource_root("missing");
        // default_path proves the old cross-file fallback would otherwise have succeeded.
        // default_path 证明旧跨文件回退原本会成功。
        let default_path = resource_root
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);
        fs::create_dir_all(
            default_path
                .parent()
                .expect("default prompt should have a parent"),
        )
        .expect("create provider prompt directory");
        fs::write(&default_path, "Default prompt").expect("write default prompt");
        // expected_path is the missing resource that must own the diagnostic.
        // expected_path 是必须拥有诊断的缺失资源。
        let expected_path = resource_root
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(CODEX_SYSTEM_PROMPT_FILE_NAME);

        // error must name the selected Codex file rather than loading the default file.
        // error 必须指明所选 Codex 文件，而不是加载默认文件。
        let error = load_provider_base_system_prompt_from_directory("gpt-5-codex", &resource_root)
            .expect_err("missing exact provider prompt should fail");

        assert!(error.contains(&expected_path.display().to_string()));
        fs::remove_dir_all(resource_root).expect("remove missing-resource fixture");
    }

    /// Verifies invalid UTF-8 provider resources fail with their exact path.
    /// 验证无效 UTF-8 Provider 资源会携带精确路径失败。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if invalid encoding is treated as absence.
    /// 此测试无返回值；若无效编码被视为缺失则失败。
    #[test]
    fn reader_rejects_invalid_utf8_resource() {
        // resource_root isolates the invalid-encoding fixture from other tests.
        // resource_root 将无效编码夹具与其他测试隔离。
        let resource_root = unique_test_resource_root("invalid-utf8");
        // prompt_path is the exact invalid UTF-8 file passed to the reader.
        // prompt_path 是传给读取器的精确无效 UTF-8 文件。
        let prompt_path = resource_root.join("invalid.txt");
        fs::create_dir_all(&resource_root).expect("create invalid-resource directory");
        fs::write(&prompt_path, [0xff, 0xfe, 0xfd]).expect("write invalid provider prompt");

        // error must preserve the selected resource identity.
        // error 必须保留所选资源身份。
        let error = read_non_empty_system_prompt_file(&prompt_path)
            .expect_err("invalid UTF-8 provider prompt should fail");

        assert!(error.contains(&prompt_path.display().to_string()));
        fs::remove_dir_all(resource_root).expect("remove invalid-resource fixture");
    }

    /// Verifies whitespace-only provider resources are rejected explicitly.
    /// 验证仅含空白的 Provider 资源会被明确拒绝。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if blank content is treated as a missing fallback candidate.
    /// 此测试无返回值；若空白内容被视为缺失回退候选则失败。
    #[test]
    fn reader_rejects_blank_resource() {
        // resource_root isolates the blank-content fixture from other tests.
        // resource_root 将空白内容夹具与其他测试隔离。
        let resource_root = unique_test_resource_root("blank");
        // prompt_path is the exact whitespace-only resource under validation.
        // prompt_path 是接受校验的精确纯空白资源。
        let prompt_path = resource_root.join("blank.txt");
        fs::create_dir_all(&resource_root).expect("create blank-resource directory");
        fs::write(&prompt_path, " \n\t").expect("write blank provider prompt");

        // error must identify the blank resource rather than returning no prompt.
        // error 必须标识空白资源，而不是返回无提示词。
        let error = read_non_empty_system_prompt_file(&prompt_path)
            .expect_err("blank provider prompt should fail");

        assert!(error.contains("is empty"));
        assert!(error.contains(&prompt_path.display().to_string()));
        fs::remove_dir_all(resource_root).expect("remove blank-resource fixture");
    }

    /// Verifies resources emptied entirely by sanitization are rejected explicitly.
    /// 验证被清洗过程完全清空的资源会被明确拒绝。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if obsolete-only content reaches prompt composition.
    /// 此测试无返回值；若仅含陈旧内容的资源进入提示组合则失败。
    #[test]
    fn reader_rejects_resource_emptied_by_sanitization() {
        // resource_root isolates the obsolete-only fixture from other tests.
        // resource_root 将仅含陈旧内容的夹具与其他测试隔离。
        let resource_root = unique_test_resource_root("sanitized-empty");
        // prompt_path contains only a section that the current sanitizer removes.
        // prompt_path 仅包含当前清洗器会移除的段落。
        let prompt_path = resource_root.join("sanitized-empty.txt");
        fs::create_dir_all(&resource_root).expect("create sanitized-resource directory");
        fs::write(&prompt_path, "# Tool usage policy\n- Prefer the Task tool.")
            .expect("write obsolete-only provider prompt");

        // error must distinguish sanitization emptiness from a missing file.
        // error 必须区分清洗后为空与文件缺失。
        let error = read_non_empty_system_prompt_file(&prompt_path)
            .expect_err("sanitized-empty provider prompt should fail");

        assert!(error.contains("empty after sanitization"));
        assert!(error.contains(&prompt_path.display().to_string()));
        fs::remove_dir_all(resource_root).expect("remove sanitized-resource fixture");
    }

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
