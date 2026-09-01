//! Host-owned project-resource address resolution for explicit media tool invocations.
//! 显式媒体工具调用使用的宿主项目资源地址解析。

use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_session_core::v1::ResourceId;

/// The host's exact session-to-project authority, shared by root and child executions without global path discovery.
/// 宿主精确的会话至项目权威，由根与子执行共享，不进行全局路径查找。
#[async_trait::async_trait]
pub trait MediaResourceRuntime: Send + Sync + std::fmt::Debug {
    /// Resolves `resource_id` under exact host `session_id`, retaining catalog kind, MIME and source identity.
    /// 在精确宿主`session_id`下解析`resource_id`，保留目录类型、MIME和来源身份。
    /// Returns metadata only; dynamic operation bindings validate supported modalities before materialization.
    /// 只返回元数据；动态操作绑定在物化前验证所支持模态。
    async fn resolve_project_resource(
        &self,
        session_id: &str,
        resource_id: ResourceId,
    ) -> Result<ConversationAttachment, String>;

    /// Resolves `resource_id` in the project owned by host `session_id`, enforcing the tool's `expected_kind`.
    /// 在宿主`session_id`所属项目中解析`resource_id`，执行工具的`expected_kind`约束。
    /// Returns metadata-checked private source facts; policy must admit them before bounded content verification or upload.
    /// 返回通过元数据检查的私有来源事实；有界内容验证或上传前必须先由策略准入。
    async fn resolve_project_media(
        &self,
        session_id: &str,
        resource_id: ResourceId,
        expected_kind: ConversationAttachmentKind,
    ) -> Result<ConversationAttachment, String> {
        // Fixed media facades retain their exact modality requirement on top of the same project authority.
        // 固定媒体外壳在相同项目权威之上保留精确模态要求。
        let attachment = self
            .resolve_project_resource(session_id, resource_id)
            .await?;
        if attachment.kind != expected_kind {
            return Err(String::from(
                "Project resource MIME does not match the requested media tool.",
            ));
        }
        Ok(attachment)
    }
}

/// Invocation-bound adapter exposing the existing project authority to frozen declarative operation bindings.
/// 向冻结声明式操作绑定暴露既有项目权威的调用绑定适配器。
#[derive(Debug)]
pub(crate) struct SessionProviderToolMediaSourceResolver<'a> {
    /// Optional exact project resource port captured by the agent execution; managed ids use their separate index authority.
    /// 代理执行捕获的可选精确项目资源端口；托管ID使用其独立索引权威。
    pub(crate) runtime: Option<&'a dyn MediaResourceRuntime>,
    /// Exact execution workspace required only for the managed attachment index, never for project-id inference.
    /// 仅托管附件索引需要的精确执行工作区，绝不用于推断项目ID。
    pub(crate) workspace_root: Option<&'a std::path::Path>,
    /// Calling session; never taken from model arguments or another project locator.
    /// 发起会话；绝不取自模型参数或另一项目定位符。
    pub(crate) session_id: &'a str,
}

#[async_trait::async_trait]
impl vulcan_provider_adapters::ProviderToolMediaSourceResolver
    for SessionProviderToolMediaSourceResolver<'_>
{
    /// Resolves canonical `reference` through the captured owner and returns private, metadata-only source proof.
    /// 通过捕获的所有者解析规范`reference`，返回私有、仅元数据来源证明。
    async fn resolve_source(
        &self,
        reference: &str,
    ) -> Result<vulcan_provider_adapters::ProviderToolMediaSource, String> {
        // The two explicit locator schemes have distinct authorities; neither is a fallback for the other.
        // 两个显式定位符方案具有不同权威；两者都不是对方的回退。
        let attachment = if let Some(resource_id) =
            vulcan_workspace_store::workspace_store::parse_project_resource_locator(reference)?
        {
            // Only the captured session-to-project port may select project entities.
            // 只有捕获的会话至项目端口可以选择项目实体。
            let runtime = self.runtime.ok_or_else(|| {
                String::from(
                    "Project media resource resolution is unavailable for this invocation.",
                )
            })?;
            runtime
                .resolve_project_resource(self.session_id, resource_id)
                .await?
        } else {
            if !reference.starts_with("attachment:") {
                return Err(String::from(
                    "Declared host source must use a canonical project-resource or attachment locator.",
                ));
            }
            // Legacy managed ids use the frozen workspace and exact session owner, not a model-supplied path.
            // 旧托管ID使用冻结工作区及精确会话所有者，不使用模型提供的路径。
            let workspace_root = self.workspace_root.ok_or_else(|| {
                String::from("Managed attachment resolution requires an active workspace root.")
            })?;
            // Own only the exact immutable arguments needed by the blocking metadata lookup.
            // 仅持有阻塞元数据查询所需的精确不可变参数。
            let workspace = vulcan_runtime_assets::native_host_path_text_from_path(workspace_root);
            // The session remains invocation-owned even if caller JSON contains an alternative id.
            // 即使调用方JSON包含另一ID，会话仍由调用持有。
            let session_id = self.session_id.to_string();
            // Canonical spelling is validated before any asset-index lookup.
            // 在任何资产索引查询之前验证规范拼写。
            let reference = reference.to_string();
            tokio::task::spawn_blocking(move || {
                crate::attachment_asset_index::resolve_owned_attachment_resource_metadata(
                    &workspace,
                    &session_id,
                    &reference,
                )
            })
            .await
            .map_err(|_| String::from("Managed attachment metadata resolution task failed."))??
        };
        Ok(vulcan_provider_adapters::ProviderToolMediaSource {
            reference: reference.to_string(),
            local_path: std::path::PathBuf::from(attachment.local_path),
            file_name: attachment.file_name,
            mime_type: attachment.mime_type,
            source_identity: vulcan_provider_adapters::PreparedMediaSourceIdentity {
                byte_len: attachment.byte_len.ok_or_else(|| {
                    String::from("Resolved host source has no registered byte length.")
                })?,
                content_sha256: attachment.source_sha256.ok_or_else(|| {
                    String::from("Resolved host source has no registered digest.")
                })?,
            },
        })
    }
}
