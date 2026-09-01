//! Host-owned source metadata carried outside AI-visible operation arguments.
//! 在AI可见操作参数之外传递的宿主持有来源元数据。

use std::path::PathBuf;

/// Read-only local resource authority for one provider-tool invocation; neither field is part of model JSON.
/// 一次供应商工具调用的只读本地资源权威；两个字段均不属于模型JSON。
#[derive(Debug, Clone, Copy)]
pub struct ProviderToolResourceContext<'a> {
    /// Existing workspace boundary used only for ordinary declared local-path inputs.
    /// 仅用于普通已声明本地路径输入的既有工作区边界。
    pub workspace_root: Option<&'a std::path::Path>,
    /// Session-owner-resolved sources that retain their own exact entity and catalog identity.
    /// 会话所有者解析的来源，保留各自精确实体和目录身份。
    pub media_sources: &'a [ProviderToolMediaSource],
    /// Optional invocation-owned resolver for declared host locators; mutually exclusive with pre-resolved sources.
    /// 声明宿主定位符的可选调用所有者解析器；与已解析来源互斥。
    pub source_resolver: Option<&'a dyn ProviderToolMediaSourceResolver>,
}

/// Read-only host authority called only for exact project/attachment locators in frozen operation upload bindings.
/// 只为冻结操作上传绑定中的精确项目/附件定位符调用的只读宿主权威。
#[async_trait::async_trait]
pub trait ProviderToolMediaSourceResolver: Send + Sync + std::fmt::Debug {
    /// Resolves `reference` under the invocation's captured session, returning original metadata without reading content.
    /// 在调用捕获的会话下解析`reference`，返回原始元数据而不读取内容。
    /// Invalid, missing or unauthorized locators return an explicit error; no alternate owner may be selected.
    /// 无效、缺失或未授权定位符返回明确错误；不得选择替代所有者。
    async fn resolve_source(&self, reference: &str) -> Result<ProviderToolMediaSource, String>;
}

/// Resolves only declared `operation` inputs in canonical `arguments` using the captured host `resolver`.
/// 使用捕获的宿主`resolver`只解析规范`arguments`中声明的`operation`输入。
/// Returns unique host proofs in first-occurrence order; duplicate inputs still count separately during policy planning.
/// 按首次出现顺序返回唯一宿主证明；重复输入仍在策略规划时分别计数。
pub(crate) async fn resolve_declared_operation_media_sources(
    operation: Option<&crate::OperationDeclaration>,
    arguments: &serde_json::Value,
    resolver: &dyn ProviderToolMediaSourceResolver,
) -> Result<Vec<ProviderToolMediaSource>, String> {
    // Operations without upload bindings do not authorize arbitrary JSON resource discovery.
    // 没有上传绑定的操作不授权从任意JSON发现资源。
    let Some(operation) = operation else {
        return Ok(Vec::new());
    };
    // Full shape validation occurs before even the first metadata lookup.
    // 完整形态验证发生在第一次元数据查询之前。
    let inputs = crate::upload_engine::operation_upload_inputs(operation, arguments)?;
    // Deduplication concerns immutable host metadata, never chronological delivery or budget entries.
    // 去重仅涉及不可变宿主元数据，不涉及时序投递或预算条目。
    let mut visited = std::collections::BTreeSet::new();
    // Private metadata is never inserted into the caller's JSON value.
    // 私有元数据绝不插入调用方JSON值。
    let mut sources = Vec::new();
    for input in inputs {
        if !(input.reference.starts_with("project-resource:")
            || input.reference.starts_with("attachment:"))
            || !visited.insert(input.reference)
        {
            continue;
        }
        // The host must prove exactly the requested locator, not substitute another source.
        // 宿主必须证明精确请求定位符，不能替换其他来源。
        let source = resolver.resolve_source(input.reference).await?;
        if source.reference != input.reference {
            return Err(String::from(
                "Resolved host media source differs from its declared locator.",
            ));
        }
        sources.push(source);
    }
    Ok(sources)
}

/// One explicitly resolved resource locator and its immutable local source; this type is never deserialized from tool JSON.
/// 一个已显式解析的资源定位符及其不可变本地来源；此类型绝不从工具JSON反序列化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolMediaSource {
    /// Exact locator string present in the projected operation binding, not a candidate lookup key.
    /// 投影操作绑定中存在的精确定位符字符串，而非候选查询键。
    pub reference: String,
    /// Canonical host-owned entity path verified by the invoking session's resource owner.
    /// 由调用会话资源所有者验证的规范宿主持有实体路径。
    pub local_path: PathBuf,
    /// Original display filename retained independently of the digest-derived entity path.
    /// 独立于摘要派生实体路径保留的原始显示文件名。
    pub file_name: String,
    /// Authoritative MIME from the resource catalog or managed attachment index.
    /// 来自资源目录或托管附件索引的权威MIME。
    pub mime_type: String,
    /// Exact registered byte length and digest required for cache reuse and every materialization.
    /// 缓存复用与每次物化必须满足的精确注册字节长度和摘要。
    pub source_identity: crate::PreparedMediaSourceIdentity,
}

#[cfg(test)]
mod tests;
