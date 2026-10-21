//! Provider-tool helpers for callers outside a conversation turn, such as the MCP service.
//! 供会话回合之外的调用方（如 MCP 服务）使用的供应商工具辅助函数。
//!
//! These reuse the turn bridge's public model identity, parameter rendering, and bounded resource
//! download, but save outputs to a caller-owned directory instead of a session's managed attachments.
//! 这些函数复用回合桥接层的公开模型身份、参数渲染与有界资源下载，但把输出保存到调用方拥有的目录，
//! 而不是会话的托管附件。

use super::*;

/// One file saved from a provider-tool result.
/// 从供应商工具结果保存的一个文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedProviderToolOutput {
    /// Absolute path of the saved file.
    /// 已保存文件的绝对路径。
    pub path: PathBuf,
    /// Provider resource kind such as `image` or `video`.
    /// 供应商资源类型，如 `image` 或 `video`。
    pub kind: String,
    /// MIME type.
    /// MIME 类型。
    pub mime_type: String,
    /// Size in bytes.
    /// 字节大小。
    pub byte_len: u64,
}

/// Files and text saved from one provider-tool result.
/// 从一次供应商工具结果保存的文件与文本。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SavedProviderToolResult {
    /// Saved files in provider order.
    /// 按供应商顺序排列的已保存文件。
    pub files: Vec<SavedProviderToolOutput>,
    /// Provider text returned beside or instead of files.
    /// 供应商随文件或代替文件返回的文本。
    pub text: Option<String>,
}

/// Returns the public `provider/upstream_target_id` identifier model extensions use.
/// 返回模型扩展使用的公开 `provider/upstream_target_id` 标识。
pub fn model_extension_public_model_id(target: &AvailableProviderToolTarget) -> String {
    provider_tool_public_model_id(target)
}

/// Renders one target's reviewed profile and exact argument schema, as the turn lookup tool does.
/// 渲染一个目标的已审核说明与精确参数 Schema，与回合查询工具一致。
///
/// # Returns
/// 返回值
/// Markdown, or an error when the target has no reviewed extension profile.
/// 返回 Markdown；目标没有已审核扩展说明时返回错误。
pub fn model_extension_parameters_markdown(
    target: &AvailableProviderToolTarget,
) -> Result<String, String> {
    let profile = target
        .target
        .contract
        .extension_profile
        .as_ref()
        .ok_or_else(|| {
            format!(
                "Native model-extension target `{}` has no reviewed extension profile.",
                target.target.target_id
            )
        })?;
    result_markdown::extension_parameters::model(
        &provider_tool_public_model_id(target),
        &target.target.display_name,
        profile,
        &target.target.contract.input_schema,
    )
}

/// Saves the generated resources and text of one provider-tool result into `directory`.
/// 把一次供应商工具结果中的生成资源与文本保存到 `directory`。
///
/// # Parameters
/// 参数
/// `result` is the provider-normalized result; `directory` must not exist yet and is created.
/// `result` 是供应商归一化结果；`directory` 必须尚不存在，并由本函数创建。
///
/// # Returns
/// 返回值
/// The saved files and text. On failure the directory is removed and the first error returned.
/// 返回已保存文件与文本。失败时删除该目录并返回首个错误。
pub async fn save_provider_tool_result_to_directory(
    result: ProviderToolExecutionResult,
    directory: &Path,
) -> Result<SavedProviderToolResult, String> {
    let (resources, text) = match result.output {
        ProviderToolNormalizedOutput::Text(text) => (Vec::new(), Some(text)),
        ProviderToolNormalizedOutput::Resources(resources) => (resources, None),
        ProviderToolNormalizedOutput::Composite(composite) => {
            let text = (!composite.text.is_empty()).then(|| composite.text.join("\n\n"));
            (composite.resources, text)
        }
        ProviderToolNormalizedOutput::Search(_)
        | ProviderToolNormalizedOutput::Extract(_)
        | ProviderToolNormalizedOutput::Embedding(_)
        | ProviderToolNormalizedOutput::Rerank(_) => {
            return Err(String::from(
                "The provider returned a non-media result where media output was expected.",
            ));
        }
    };
    if resources.is_empty() {
        return Ok(SavedProviderToolResult {
            files: Vec::new(),
            text,
        });
    }
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|error| format!("Media output directory could not be created: {error}"))?;
    let saved = save_resources(resources, directory).await;
    match saved {
        Ok(files) => Ok(SavedProviderToolResult { files, text }),
        Err(error) => {
            if let Err(cleanup) = tokio::fs::remove_dir_all(directory).await {
                return Err(format!(
                    "{error} The partial output directory could not be removed: {cleanup}"
                ));
            }
            Err(error)
        }
    }
}

/// Writes or downloads every resource into `directory` in provider order.
/// 按供应商顺序把每个资源写入或下载到 `directory`。
async fn save_resources(
    resources: Vec<vulcan_provider_adapters::ProviderToolGeneratedResource>,
    directory: &Path,
) -> Result<Vec<SavedProviderToolOutput>, String> {
    let mut files = Vec::with_capacity(resources.len());
    for (index, resource) in resources.into_iter().enumerate() {
        let descriptor = attachment_descriptor_from_mime_type(resource.mime_type.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider tool resource `{}` has unsupported MIME type `{}`.",
                    resource.output_id, resource.mime_type
                )
            })?;
        let path = directory.join(format!(
            "{:02}-{}.{}",
            index + 1,
            safe_file_stem(&resource.output_id),
            descriptor.extension
        ));
        match (
            resource.bytes,
            resource.download_url.as_deref(),
            resource.download_policy.as_ref(),
        ) {
            (Some(bytes), None, None) => {
                if bytes.is_empty() {
                    return Err(String::from("Provider resource cannot be empty."));
                }
                tokio::fs::write(&path, &bytes)
                    .await
                    .map_err(|error| format!("Provider resource could not be saved: {error}"))?;
            }
            (None, Some(download_url), Some(download_policy)) => {
                let temporary = download_provider_resource_to_temp(
                    download_url,
                    resource.kind.as_str(),
                    download_policy,
                )
                .await?;
                // Copy rather than rename: the temporary directory may live on another volume.
                // 复制而非重命名：临时目录可能位于另一个卷。
                tokio::fs::copy(AsRef::<Path>::as_ref(&temporary), &path)
                    .await
                    .map_err(|error| format!("Provider resource could not be saved: {error}"))?;
            }
            _ => {
                return Err(format!(
                    "Provider tool resource `{}` has an invalid carrier.",
                    resource.output_id
                ));
            }
        }
        let byte_len = tokio::fs::metadata(&path)
            .await
            .map_err(|error| format!("Saved provider resource could not be inspected: {error}"))?
            .len();
        files.push(SavedProviderToolOutput {
            path,
            kind: resource.kind,
            mime_type: descriptor.mime_type.to_string(),
            byte_len,
        });
    }
    Ok(files)
}

/// Reduces a provider output identifier to a portable file-name stem.
/// 把供应商输出标识缩减为可移植的文件名主干。
fn safe_file_stem(output_id: &str) -> String {
    let stem = output_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .take(48)
        .collect::<String>();
    if stem.is_empty() {
        String::from("output")
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_provider_adapters::ProviderToolGeneratedResource;

    /// Builds one inline resource.
    /// 构建一个内联资源。
    fn inline_resource(output_id: &str, bytes: &[u8]) -> ProviderToolGeneratedResource {
        ProviderToolGeneratedResource {
            output_id: output_id.to_string(),
            kind: String::from("image"),
            mime_type: String::from("image/png"),
            bytes: Some(bytes.to_vec()),
            download_url: None,
            download_policy: None,
        }
    }

    #[tokio::test]
    async fn inline_resources_are_saved_in_order_with_safe_names() {
        let root = tempfile::tempdir().expect("temporary root");
        let directory = root.path().join("job");
        let saved = save_provider_tool_result_to_directory(
            ProviderToolExecutionResult {
                upstream_response_id: None,
                output: ProviderToolNormalizedOutput::Resources(vec![
                    inline_resource("a/b", b"one"),
                    inline_resource("", b"two"),
                ]),
            },
            &directory,
        )
        .await
        .expect("saved outputs");
        assert_eq!(saved.files.len(), 2);
        assert_eq!(saved.files[0].byte_len, 3);
        assert!(saved.files[0].path.starts_with(&directory));
        assert!(
            saved.files[0]
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("01-") && !name.contains('/'))
        );
        assert_eq!(std::fs::read(&saved.files[1].path).expect("read"), b"two");
    }

    #[tokio::test]
    async fn a_failed_batch_removes_its_directory() {
        let root = tempfile::tempdir().expect("temporary root");
        let directory = root.path().join("job");
        let error = save_provider_tool_result_to_directory(
            ProviderToolExecutionResult {
                upstream_response_id: None,
                output: ProviderToolNormalizedOutput::Resources(vec![
                    inline_resource("ok", b"one"),
                    inline_resource("empty", b""),
                ]),
            },
            &directory,
        )
        .await
        .expect_err("empty resource fails");
        assert!(!error.is_empty());
        assert!(!directory.exists());
    }
}
