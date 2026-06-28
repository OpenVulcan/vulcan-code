//! Submit-plan native tool implementation.
//! 提交计划原生工具实现。

use super::*;

/// Hidden marker proving a Markdown plan was created by the submit_plan tool.
/// 证明 Markdown 计划由 submit_plan 工具创建的隐藏标记。
pub(crate) const SUBMIT_PLAN_MARKER: &str = "<!-- generated-by: vulcan-submit-plan -->";

/// Parsed argument payload accepted by the `submit_plan` tool.
/// `submit_plan` 工具接受的已解析参数载荷。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitPlanArgs {
    /// Human-readable plan name used for the generated filename.
    /// 用于生成文件名的人类可读计划名称。
    name: String,
    /// Complete Markdown plan content to write.
    /// 需要写入的完整 Markdown 计划内容。
    content: String,
    /// Optional target directory under the workspace root.
    /// 工作区根目录下的可选目标目录。
    directory: Option<String>,
    /// Whether to replace a previous tool-created plan file.
    /// 是否替换此前由工具创建的计划文件。
    replace: Option<bool>,
    /// Exact previous plan path required when replacing.
    /// 替换时必需的此前计划完整路径。
    path: Option<String>,
}

/// Builds the Plan-mode plan artifact submission tool.
/// 构建计划模式计划产物提交工具。
pub(crate) fn build_submit_plan_tool() -> Arc<NativeToolDefinition> {
    let descriptor = NativeToolDescriptor {
        id: String::from(SUBMIT_PLAN_TOOL_ID),
        name: String::from(SUBMIT_PLAN_TOOL_ID),
        description: String::from(
            "Submit a complete Plan-mode Markdown plan artifact. Use this only in Plan mode after the implementation plan is ready. For revisions, set replace=true and pass the exact path returned by the previous submit_plan call.",
        ),
        help_text: None,
        ai_visible: true,
        available_in_plan: true,
    };

    build_native_tool_definition(
        descriptor,
        submit_plan_args_schema(),
        move |request: NativeToolExecutionRequest| {
            Box::pin(async move { execute_submit_plan_tool(request).await })
        },
    )
}

/// Builds the JSON Schema for `submit_plan`.
/// 构建 `submit_plan` 的 JSON Schema。
///
/// # Returns
/// 返回值
/// The model-facing argument schema for plan artifact submission.
/// 面向模型的计划产物提交参数结构。
fn submit_plan_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Short human-readable plan name used for the generated filename.",
                "minLength": 1
            },
            "content": {
                "type": "string",
                "description": "Complete self-contained Markdown plan content.",
                "minLength": 1
            },
            "directory": {
                "type": "string",
                "description": "Optional target directory under the current workspace root. Defaults to docs/plan."
            },
            "replace": {
                "type": "boolean",
                "description": "Set true only when replacing an existing tool-created plan artifact."
            },
            "path": {
                "type": "string",
                "description": "Exact previous plan file path returned by submit_plan. Required when replace=true."
            }
        },
        "required": ["name", "content"],
        "additionalProperties": false
    })
}

/// Executes one `submit_plan` request and writes the Markdown artifact.
/// 执行单次 `submit_plan` 请求并写入 Markdown 产物。
///
/// # Parameters
/// 参数
/// `request` carries raw model arguments and the workspace root.
/// `request` 携带模型原始参数与工作区根目录。
///
/// # Returns
/// 返回值
/// A model response and host projection pointing at the generated plan artifact.
/// 返回模型响应以及指向已生成计划产物的宿主投影。
async fn execute_submit_plan_tool(
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let args: SubmitPlanArgs =
        serde_json::from_value(request.arguments.clone()).map_err(|error| {
            NativeToolError::new(format!("failed to parse function arguments: {error}"))
        })?;
    let name = args.name.trim();
    let content = args.content.trim();
    if name.is_empty() {
        return Err(NativeToolError::new("Plan name cannot be empty."));
    }
    if content.is_empty() {
        return Err(NativeToolError::new("Plan content cannot be empty."));
    }
    let workspace_root = request
        .workspace_root
        .as_deref()
        .ok_or_else(|| NativeToolError::new("submit_plan requires a workspace root."))?;
    let workspace_root = canonicalize_existing_path(workspace_root).await?;
    let replace = args.replace.unwrap_or(false);
    let target_path = if replace {
        let path = args
            .path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| NativeToolError::new("replace=true requires the original plan path."))?;
        let path = PathBuf::from(path);
        ensure_workspace_child_path(&workspace_root, &path).await?;
        let existing = tokio_fs::read_to_string(&path).await.map_err(|error| {
            NativeToolError::new(format!("failed to read original plan: {error}"))
        })?;
        if !existing.contains(SUBMIT_PLAN_MARKER) {
            return Err(NativeToolError::new(
                "Refusing to replace a file that was not created by submit_plan.",
            ));
        }
        path
    } else {
        if args
            .path
            .as_deref()
            .is_some_and(|path| !path.trim().is_empty())
        {
            return Err(NativeToolError::new(
                "Pass path only with replace=true; new plans choose a fresh filename automatically.",
            ));
        }
        let directory = args
            .directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("docs/plan");
        let directory_path = resolve_new_plan_directory(&workspace_root, directory).await?;
        tokio_fs::create_dir_all(&directory_path)
            .await
            .map_err(|error| {
                NativeToolError::new(format!("failed to create plan directory: {error}"))
            })?;
        unique_plan_path(&directory_path, name).await?
    };
    let plan_text = format!("{SUBMIT_PLAN_MARKER}\n\n{content}\n");
    tokio_fs::write(&target_path, plan_text)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to write plan file: {error}")))?;
    let path_text = display_submit_plan_path(&target_path);
    let updated_at = unix_millis_now().to_string();
    Ok(VulcanToolInvocationResult::from_model_output(json!({
        "path": path_text,
        "name": name,
        "message": "Plan artifact saved."
    }))
    .with_host_result(VulcanToolHostResult {
        kind: String::from("plan_artifact"),
        payload: json!({
            "name": name,
            "path": path_text,
            "content": content,
            "updatedAt": updated_at,
        }),
    }))
}

/// Formats a submitted Plan path for model-visible and UI-visible tool feedback.
/// 将已提交计划路径格式化为模型可见与界面可见的工具反馈文本。
///
/// # Parameters
/// 参数
/// `path` is the actual filesystem path used for writing the artifact.
/// `path` 是实际用于写入计划产物的文件系统路径。
///
/// # Returns
/// 返回值
/// A display path with Windows extended-path prefixes removed.
/// 返回移除 Windows 扩展路径前缀后的展示路径。
pub(crate) fn display_submit_plan_path(path: &Path) -> String {
    normalized_workspace_path_text_from_path(path)
}

/// Returns the current Unix epoch milliseconds.
/// 返回当前 Unix 纪元毫秒数。
///
/// # Returns
/// 返回值
/// The current timestamp in milliseconds, or zero if the system clock is before epoch.
/// 当前毫秒时间戳；如果系统时间早于纪元则返回零。
fn unix_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

/// Canonicalizes an existing path and converts failures into native-tool errors.
/// 规范化已存在路径，并将失败转换为原生工具错误。
///
/// # Parameters
/// 参数
/// `path` is the filesystem path to canonicalize.
/// `path` 是需要规范化的文件系统路径。
///
/// # Returns
/// 返回值
/// The canonical absolute path.
/// 规范化后的绝对路径。
async fn canonicalize_existing_path(path: &Path) -> Result<PathBuf, NativeToolError> {
    tokio_fs::canonicalize(path)
        .await
        .map_err(|error| NativeToolError::new(format!("failed to resolve path: {error}")))
}

/// Ensures a target path remains inside the workspace root.
/// 确保目标路径仍位于工作区根目录内。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化后的工作区根目录。
/// `path` is the target path to validate.
/// `path` 是需要校验的目标路径。
///
/// # Returns
/// 返回值
/// `Ok(())` when the path is accepted.
/// 路径通过校验时返回 `Ok(())`。
async fn ensure_workspace_child_path(
    workspace_root: &Path,
    path: &Path,
) -> Result<(), NativeToolError> {
    let candidate = if path.exists() {
        canonicalize_existing_path(path).await?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| NativeToolError::new("target path has no parent directory."))?;
        let canonical_parent = canonicalize_existing_path(parent).await?;
        let file_name = path
            .file_name()
            .ok_or_else(|| NativeToolError::new("target path has no filename."))?;
        canonical_parent.join(file_name)
    };
    if !candidate.starts_with(workspace_root) {
        return Err(NativeToolError::new(
            "Plan path must stay inside the current workspace.",
        ));
    }
    Ok(())
}

/// Resolves a new plan directory while keeping it inside the workspace root.
/// 解析新的计划目录，并确保其位于工作区根目录内。
///
/// # Parameters
/// 参数
/// `workspace_root` is the canonical workspace root.
/// `workspace_root` 是规范化后的工作区根目录。
/// `directory` is the model-supplied relative or absolute directory.
/// `directory` 是模型提供的相对或绝对目录。
///
/// # Returns
/// 返回值
/// A workspace-contained directory path that may not exist yet.
/// 一个位于工作区内且可能尚不存在的目录路径。
async fn resolve_new_plan_directory(
    workspace_root: &Path,
    directory: &str,
) -> Result<PathBuf, NativeToolError> {
    let requested = PathBuf::from(directory);
    if requested
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(NativeToolError::new(
            "Plan directory cannot contain parent-directory components.",
        ));
    }
    if requested.is_absolute() {
        let parent = requested
            .parent()
            .ok_or_else(|| NativeToolError::new("plan directory has no parent."))?;
        let canonical_parent = canonicalize_existing_path(parent).await?;
        let file_name = requested
            .file_name()
            .ok_or_else(|| NativeToolError::new("plan directory has no final component."))?;
        let resolved = canonical_parent.join(file_name);
        if !resolved.starts_with(workspace_root) {
            return Err(NativeToolError::new(
                "Plan directory must stay inside the current workspace.",
            ));
        }
        return Ok(resolved);
    }
    Ok(workspace_root.join(requested))
}

/// Builds a unique Markdown path for a new plan artifact.
/// 为新的计划产物构建唯一 Markdown 路径。
///
/// # Parameters
/// 参数
/// `directory` is the already-created target directory.
/// `directory` 是已经创建的目标目录。
/// `name` is the human-readable plan name.
/// `name` 是人类可读计划名称。
///
/// # Returns
/// 返回值
/// A non-existing Markdown path.
/// 一个尚不存在的 Markdown 路径。
async fn unique_plan_path(directory: &Path, name: &str) -> Result<PathBuf, NativeToolError> {
    let base = slugify_plan_name(name);
    for suffix in 0..1000 {
        let filename = if suffix == 0 {
            format!("{base}.md")
        } else {
            format!("{base}_{suffix}.md")
        };
        let path = directory.join(filename);
        if tokio_fs::metadata(&path).await.is_err() {
            return Ok(path);
        }
    }
    Err(NativeToolError::new(
        "failed to allocate a unique plan filename after 1000 attempts.",
    ))
}

/// Converts one human-readable plan name into a conservative filename stem.
/// 将单个人类可读计划名称转换为保守文件名主体。
///
/// # Parameters
/// 参数
/// `name` is the model-supplied plan name.
/// `name` 是模型提供的计划名称。
///
/// # Returns
/// 返回值
/// A lowercase ASCII filename stem.
/// 小写 ASCII 文件名主体。
fn slugify_plan_name(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        String::from("plan")
    } else {
        slug.chars().take(80).collect()
    }
}
