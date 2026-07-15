//! Default base prompt loading helpers for the execution runtime.
//! 执行运行时使用的默认基础提示词加载辅助模块。

use crate::runtime_assets;
use std::{fs, path::Path};

/// Relative resource directory name that stores the default base system prompt.
/// 存放默认基础系统提示词的相对资源目录名称。
const SYSTEM_PROMPTS_DIR_NAME: &str = "system-prompts";

/// Default system prompt file name shared by every model.
/// 所有模型共享的默认系统提示词文件名。
const DEFAULT_SYSTEM_PROMPT_FILE_NAME: &str = "default.txt";

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

/// Loads the model-independent default base system prompt from the editable resource directory.
/// 从可编辑资源目录加载与模型无关的默认基础系统提示词。
///
/// # Returns
/// # 返回值
/// Returns sanitized prompt text, or a default-resource-path failure.
/// 返回清洗后的提示词文本，或带默认资源路径的失败。
pub(crate) fn load_provider_base_system_prompt() -> Result<String, String> {
    load_provider_base_system_prompt_with_source().map(|prompt| prompt.text)
}

/// Loads the model-independent default base system prompt with source information.
/// 加载一份带来源信息且与模型无关的默认基础系统提示词。
///
/// # Returns
/// # 返回值
/// Returns sanitized prompt text with its source label, or a default-resource failure.
/// 返回带来源标签的清洗后提示词文本，或默认资源失败。
pub(crate) fn load_provider_base_system_prompt_with_source()
-> Result<LoadedProviderBaseSystemPrompt, String> {
    // resources_directory is the single effective root selected by the runtime-assets domain.
    // resources_directory 是 runtime-assets 领域选择的唯一生效资源根目录。
    let resources_directory = runtime_assets::resolve_app_resources_directory();
    load_provider_base_system_prompt_from_directory(&resources_directory)
}

/// Loads the fixed default prompt below one already selected resource directory.
/// 从一个已选定资源目录下加载固定的默认提示词。
///
/// # Parameters
/// # 参数
/// - `resources_directory`: Effective application resource root selected by the caller.
/// - `resources_directory`：调用方选择的生效应用资源根目录。
///
/// # Returns
/// # 返回值
/// Returns the sanitized default prompt and source label, or the default file's failure.
/// 返回清洗后的默认提示词及来源标签，或默认文件的失败。
fn load_provider_base_system_prompt_from_directory(
    resources_directory: &Path,
) -> Result<LoadedProviderBaseSystemPrompt, String> {
    // prompt_path is the single product-owned default prompt resource.
    // prompt_path 是产品持有的唯一默认提示词资源。
    let prompt_path = resources_directory
        .join(SYSTEM_PROMPTS_DIR_NAME)
        .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);
    // prompt is valid only after UTF-8 decoding, sanitization, and nonempty validation.
    // prompt 仅在完成 UTF-8 解码、清洗与非空校验后才有效。
    let prompt = read_non_empty_system_prompt_file(&prompt_path)?;
    Ok(LoadedProviderBaseSystemPrompt {
        text: prompt,
        source_label: format!("Provider prompt resource `{}`", prompt_path.display()),
    })
}

/// Reads one prompt text file, removes duplicated legacy tool guidance, and trims the outer blank region.
/// 读取单个提示词文本文件，移除重复的旧工具引导段，并裁剪外围空白区域。
///
/// # Parameters
/// # 参数
/// - `path`: Exact default resource file selected by the effective resource root.
/// - `path`：由生效资源根选择的精确默认资源文件。
///
/// # Returns
/// # 返回值
/// Returns sanitized nonempty UTF-8 prompt text, or an exact read or content-validation failure.
/// 返回清洗后的非空 UTF-8 提示词文本，或精确的读取与内容校验失败。
fn read_non_empty_system_prompt_file(path: &Path) -> Result<String, String> {
    // content is the exact UTF-8 body of the shared default resource.
    // content 是共享默认资源的精确 UTF-8 正文。
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

    /// Verifies the loader reads only the fixed default resource and preserves its source label.
    /// 验证加载器只读取固定的默认资源，并保留其来源标签。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails if loading changes the default file or skips sanitization.
    /// 此测试无返回值；若加载过程改变默认文件或跳过清洗则失败。
    #[test]
    fn loader_reads_default_resource() {
        // resource_root isolates the default-resource fixture from every other test.
        // resource_root 将默认资源夹具与其他所有测试隔离。
        let resource_root = unique_test_resource_root("exact");
        // prompt_path is the single default resource selected by the loader.
        // prompt_path 是加载器选择的唯一默认资源。
        let prompt_path = resource_root
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);
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
        let loaded = load_provider_base_system_prompt_from_directory(&resource_root)
            .expect("default provider prompt should load");

        assert!(loaded.text.contains("Keep this."));
        assert!(!loaded.text.contains("Remove this."));
        assert!(
            loaded
                .source_label
                .contains(&prompt_path.display().to_string())
        );
        fs::remove_dir_all(resource_root).expect("remove exact-resource fixture");
    }

    /// Verifies an obsolete model-specific resource cannot replace a missing default resource.
    /// 验证旧模型专用资源不能替代缺失的默认资源。
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
    fn loader_rejects_missing_default_resource_without_model_fallback() {
        // resource_root contains an obsolete model-specific file but deliberately omits default.txt.
        // resource_root 包含旧模型专用文件，但刻意缺失 default.txt。
        let resource_root = unique_test_resource_root("missing");
        // obsolete_path proves an old model-specific prompt cannot become an implicit fallback.
        // obsolete_path 证明旧模型专用提示词不能成为隐式回退。
        let obsolete_path = resource_root
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join("codex.txt");
        fs::create_dir_all(
            obsolete_path
                .parent()
                .expect("obsolete prompt should have a parent"),
        )
        .expect("create provider prompt directory");
        fs::write(&obsolete_path, "Obsolete model prompt").expect("write obsolete prompt");
        // expected_path is the missing default resource that must own the diagnostic.
        // expected_path 是必须拥有诊断的缺失默认资源。
        let expected_path = resource_root
            .join(SYSTEM_PROMPTS_DIR_NAME)
            .join(DEFAULT_SYSTEM_PROMPT_FILE_NAME);

        // error must name default.txt rather than loading an obsolete model-specific file.
        // error 必须指明 default.txt，而不是加载旧模型专用文件。
        let error = load_provider_base_system_prompt_from_directory(&resource_root)
            .expect_err("missing default provider prompt should fail");

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
