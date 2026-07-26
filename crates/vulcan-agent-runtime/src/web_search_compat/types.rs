//! Shared web-search invocation and response types.
//! 网页搜索调用与响应的共享类型。

/// Parsed invocation arguments accepted by the semantic `web_search` tool.
/// 语义 `web_search` 工具接受的已解析调用参数。
#[derive(Debug, Clone)]
pub(super) struct WebSearchInvocation {
    /// Search query text.
    /// 搜索查询文本。
    pub(super) query: String,
    /// Optional caller-requested result count.
    /// 调用方请求的可选结果数量。
    pub(super) max_results: Option<usize>,
}

/// Normalized search result returned by structured providers.
/// 由结构化供应商返回的规范化搜索结果。
#[derive(Debug, Clone)]
pub(super) struct WebSearchResult {
    /// Result title.
    /// 结果标题。
    pub(super) title: String,
    /// Result URL.
    /// 结果链接。
    pub(super) url: String,
    /// Result snippet or summary.
    /// 结果摘要或片段。
    pub(super) snippet: String,
}

/// Normalized citation returned by narrative providers.
/// 由叙述型供应商返回的规范化引用。
#[derive(Debug, Clone)]
pub(super) struct WebSearchCitation {
    /// Citation URL.
    /// 引用链接。
    pub(super) url: String,
    /// Optional citation title.
    /// 可选引用标题。
    pub(super) title: String,
}

/// Provider response normalized into either a structured result list or one narrative answer.
/// 将供应商响应规范化为结构化结果列表或单段叙述答案。
#[derive(Debug, Clone)]
pub(super) enum WebSearchProviderResponse {
    /// Structured result list.
    /// 结构化结果列表。
    ResultList(Vec<WebSearchResult>),
    /// Narrative answer with optional citations.
    /// 携带可选引用的叙述型答案。
    Narrative {
        /// Human-readable answer content.
        /// 面向人类可读的答案正文。
        content: String,
        /// Structured citations collected from the provider response.
        /// 从供应商响应中收集到的结构化引用。
        citations: Vec<WebSearchCitation>,
    },
}
