//! Typed normalized provider tool outputs shared with host consumers.
//! 与宿主消费方共享的强类型归一化供应商工具输出。

use super::resource_download::ProviderToolResourceDownloadPolicy;
use serde::{Deserialize, Serialize};

/// One normalized generated resource returned by a provider tool.
/// 供应商工具返回的一项归一化生成资源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolGeneratedResource {
    /// Stable output identifier scoped to this invocation.
    /// 作用域限定在当前调用内的稳定输出标识。
    pub output_id: String,
    /// Unified semantic resource kind.
    /// 统一语义资源类别。
    pub kind: String,
    /// Exact MIME type.
    /// 精确 MIME 类型。
    pub mime_type: String,
    /// Private provider bytes when returned inline.
    /// 供应商以内联形式返回的私有字节。
    pub bytes: Option<Vec<u8>>,
    /// Public provider download URL when returned by reference.
    /// 供应商以引用形式返回的公开下载 URL。
    pub download_url: Option<String>,
    /// Frozen download authority required exactly when `download_url` is present.
    /// 仅在存在 `download_url` 时必须携带的冻结下载权限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_policy: Option<ProviderToolResourceDownloadPolicy>,
}

/// One normalized provider web-search result.
/// 一条归一化供应商网页搜索结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolSearchResult {
    /// One-based provider result rank.
    /// 从一开始的供应商结果排名。
    pub rank: usize,
    /// Provider-returned result title.
    /// 供应商返回的结果标题。
    pub title: String,
    /// Validated absolute HTTP(S) result URL.
    /// 已校验的绝对 HTTP(S) 结果地址。
    pub url: String,
    /// Normalized source hostname.
    /// 归一化来源主机名。
    pub source_domain: String,
    /// Provider-returned result excerpt.
    /// 供应商返回的结果摘要。
    pub snippet: String,
}

/// Unified structured provider web-search output.
/// 统一结构化供应商网页搜索输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolSearchOutput {
    /// Exact submitted query.
    /// 精确提交的查询。
    pub query: String,
    /// Ordered normalized results.
    /// 有序归一化结果。
    pub results: Vec<ProviderToolSearchResult>,
}

/// Closed normalized output returned by provider-native tools.
/// 供应商原生工具返回的封闭归一化输出。
/// One normalized successfully extracted web page.
/// 一条归一化的成功网页提取结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractResult {
    /// Validated absolute HTTPS source URL.
    /// 已验证的绝对 HTTPS 来源地址。
    pub url: String,
    /// Provider-returned Markdown or plain-text content.
    /// 供应商返回的 Markdown 或纯文本内容。
    pub content: String,
    /// Validated absolute HTTPS image URLs returned for this page.
    /// 当前页面返回的已验证绝对 HTTPS 图片地址。
    pub images: Vec<String>,
    /// Optional validated absolute HTTPS favicon URL.
    /// 可选的已验证绝对 HTTPS 站点图标地址。
    pub favicon: Option<String>,
}

/// One normalized provider-reported extraction failure.
/// 一条归一化的供应商报告提取失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractFailure {
    /// Validated absolute HTTPS source URL.
    /// 已验证的绝对 HTTPS 来源地址。
    pub url: String,
    /// Controlled provider failure message.
    /// 受控的供应商失败消息。
    pub error: String,
}

/// Unified structured provider web-extraction output.
/// 统一结构化供应商网页提取输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractOutput {
    /// Ordered successful extraction results.
    /// 有序的成功提取结果。
    pub results: Vec<ProviderToolExtractResult>,
    /// Ordered per-URL failures retained for user visibility.
    /// 为用户可见性保留的有序逐 URL 失败。
    pub failures: Vec<ProviderToolExtractFailure>,
}

/// One normalized embedding vector and its source index.
/// 一条归一化向量及其来源索引。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolEmbeddingVector {
    /// Zero-based input index associated with this vector.
    /// 与该向量关联的从零开始输入索引。
    pub index: usize,
    /// Ordered floating-point vector values returned by the provider.
    /// 供应商返回的有序浮点向量值。
    pub values: Vec<f64>,
}

/// Unified embedding output independent of provider wire fields.
/// 与供应商线级字段无关的统一向量输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolEmbeddingOutput {
    /// Optional exact embedding model reported by the provider.
    /// 供应商报告的可选精确向量模型。
    pub model: Option<String>,
    /// Ordered vectors matched to their original input indexes.
    /// 与原始输入索引匹配的有序向量。
    pub vectors: Vec<ProviderToolEmbeddingVector>,
}

/// One normalized reranked candidate.
/// 一条归一化重排候选项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolRerankResult {
    /// Original zero-based candidate index.
    /// 原始的从零开始候选项索引。
    pub index: usize,
    /// Provider-reported relevance score.
    /// 供应商报告的相关性分数。
    pub score: f64,
    /// Optional provider-returned candidate text.
    /// 供应商可选返回的候选文本。
    pub document: Option<String>,
}

/// Unified rerank output independent of provider wire fields.
/// 与供应商线级字段无关的统一重排输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolRerankOutput {
    /// Optional exact rerank model reported by the provider.
    /// 供应商报告的可选精确重排模型。
    pub model: Option<String>,
    /// Ordered candidates in provider-ranked order.
    /// 按供应商排序顺序排列的候选项。
    pub results: Vec<ProviderToolRerankResult>,
}

/// Flat composite output used when one provider operation returns multiple semantic kinds.
/// 当一个供应商操作返回多种语义类型时使用的扁平复合输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolCompositeOutput {
    /// Ordered text fragments.
    /// 有序文本片段。
    pub text: Vec<String>,
    /// Ordered generated resources.
    /// 有序生成资源。
    pub resources: Vec<ProviderToolGeneratedResource>,
    /// Ordered embedding result groups.
    /// 有序向量结果组。
    pub embeddings: Vec<ProviderToolEmbeddingOutput>,
    /// Ordered rerank result groups.
    /// 有序重排结果组。
    pub reranks: Vec<ProviderToolRerankOutput>,
    /// Ordered search result groups.
    /// 有序搜索结果组。
    pub searches: Vec<ProviderToolSearchOutput>,
    /// Ordered extraction result groups.
    /// 有序提取结果组。
    pub extracts: Vec<ProviderToolExtractOutput>,
}

/// Closed normalized output returned by provider-native tools.
/// 供应商原生工具返回的封闭归一化输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProviderToolNormalizedOutput {
    /// Provider-generated plain text.
    /// 供应商生成的纯文本。
    Text(String),
    /// Ordered generated resources.
    /// 有序生成资源。
    Resources(Vec<ProviderToolGeneratedResource>),
    /// Structured web-search evidence.
    /// 结构化网页搜索证据。
    Search(ProviderToolSearchOutput),
    /// Structured web-extraction content and per-URL failures.
    /// 结构化网页提取内容与逐 URL 失败。
    Extract(ProviderToolExtractOutput),
    /// Embedding vectors with stable source indexes.
    /// 带稳定来源索引的向量结果。
    Embedding(ProviderToolEmbeddingOutput),
    /// Ranked candidates with provider relevance scores.
    /// 带供应商相关性分数的重排候选项。
    Rerank(ProviderToolRerankOutput),
    /// Multiple normalized semantic output kinds from one invocation.
    /// 一次调用返回的多种归一化语义输出。
    Composite(ProviderToolCompositeOutput),
}

/// Unified typed result produced by one provider tool adapter.
/// 一个供应商工具适配器产生的统一强类型结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolExecutionResult {
    /// Optional provider trace or task identifier.
    /// 可选供应商追踪或任务标识。
    pub upstream_response_id: Option<String>,
    /// Closed normalized output independent of the provider wire shape.
    /// 与供应商 Wire 结构无关的封闭归一化输出。
    pub output: ProviderToolNormalizedOutput,
}
