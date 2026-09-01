//! Typed, non-secret failures preserved across media preparation and upload boundaries.
//! 跨媒体准备及上传边界保留的强类型非秘密失败。

/// Exact network stage that produced a media-upload failure.
/// 产生媒体上传失败的精确网络阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaUploadStage {
    /// Provider signing or temporary-policy acquisition before the object transfer.
    /// 对象传输之前的供应商签名或临时策略获取。
    Signing,
    /// Uploading the actual source object or creating its provider file resource.
    /// 上传实际来源对象或创建供应商文件资源。
    Transfer,
}

impl std::fmt::Display for MediaUploadStage {
    /// Writes the safe stage label to `formatter`, returning its formatting result.
    /// 向`formatter`写入安全阶段标签，返回格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Signing => "signing",
            Self::Transfer => "transfer",
        })
    }
}

/// Closed media failure facts; never stores provider bodies, credentials, local paths or native references.
/// 封闭媒体失败事实；绝不保存供应商正文、凭据、本地路径或原生引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaUploadFailure {
    /// The owning request or transfer was explicitly cancelled.
    /// 所属请求或传输被显式取消。
    Cancelled,
    /// The exact channel's complete lifecycle exceeded its declared timeout.
    /// 精确通道的完整生命周期超过其声明超时。
    Timeout {
        /// Validated non-secret channel identity from the frozen declaration.
        /// 来自冻结声明的已验证非秘密通道身份。
        channel_id: String,
        /// Declared complete lifecycle bound, not a resettable per-step timeout.
        /// 声明的完整生命周期限制，不是可重置逐步骤超时。
        timeout_ms: u64,
    },
    /// An actual HTTP response rejected the request before body interpretation.
    /// 真实HTTP响应在正文解释之前拒绝请求。
    Http {
        /// Exact signing or transfer stage that returned the response.
        /// 返回响应的精确签名或传输阶段。
        stage: MediaUploadStage,
        /// Status read directly from the HTTP response, never from an error string.
        /// 直接从HTTP响应读取的状态，绝不从错误字符串读取。
        status: u16,
    },
    /// A typed HTTP-client transport error occurred without a successful response.
    /// 未成功取得响应时出现强类型HTTP客户端传输错误。
    Transport {
        /// Exact network stage owning the failed request.
        /// 持有失败请求的精确网络阶段。
        stage: MediaUploadStage,
        /// The HTTP client explicitly classified the error as a timeout.
        /// HTTP客户端明确将错误分类为超时。
        timed_out: bool,
    },
    /// The selected interface does not accept this MIME type.
    /// 选中接口不接受此MIME类型。
    UnsupportedMime,
    /// Source bytes exceed the actual selected hard limit.
    /// 来源字节超过实际选中硬限制。
    SourceTooLarge {
        /// Actual admitted source bytes.
        /// 实际准入来源字节数。
        actual_bytes: u64,
        /// Exact selected maximum raw byte count.
        /// 精确选中最大原始字节数。
        max_bytes: u64,
    },
    /// The actual request contains more resources than the selected interface accepts.
    /// 实际请求包含的资源数超过选中接口接受数量。
    TooManyResources {
        /// Exact resource count for this single upload operation.
        /// 此单次上传操作的精确资源数量。
        actual_count: u64,
        /// Declared maximum resource count for this operation.
        /// 此操作声明的最大资源数量。
        max_count: u64,
    },
    /// Source availability or integrity could not be verified at the owning read boundary.
    /// 所属读取边界无法验证来源可用性或完整性。
    SourceUnavailable,
    /// Direct byte, length or digest comparison proved the original source changed.
    /// 直接字节、长度或摘要比较证明原来源已变化。
    SourceChanged,
    /// A frozen declaration, route or authentication projection is invalid.
    /// 冻结声明、路线或认证投影无效。
    InvalidContract,
    /// A source-selected provider upload is unavailable and cannot fall back to inline data.
    /// 来源已选供应商上传不可用，且不能回退内联数据。
    MissingUploadChannel,
    /// The selected primitive requires an explicit registered signing implementation.
    /// 选中原语要求显式已注册签名实现。
    MissingSigningPrimitive,
    /// The selected account's credential could not be resolved.
    /// 无法解析选中账号的凭据。
    CredentialUnavailable,
    /// The response body or declared structure could not be safely read or decoded.
    /// 无法安全读取或解码响应正文或声明结构。
    InvalidResponse {
        /// Exact network stage owning the invalid response.
        /// 持有无效响应的精确网络阶段。
        stage: MediaUploadStage,
    },
    /// The successful response omitted its required declared provider reference.
    /// 成功响应遗漏必需的已声明供应商引用。
    MissingReference,
    /// The returned reference violates its declared shape or network authority.
    /// 返回引用违反其声明形态或网络权限。
    InvalidReference,
}

impl MediaUploadFailure {
    /// Converts typed `error` at `stage` without copying its URL, source chain or message.
    /// 转换`stage`的强类型`error`，不复制其URL、来源链或消息。
    /// Returns only the HTTP client's explicit timeout classification and the exact stage.
    /// 仅返回HTTP客户端明确超时分类及精确阶段。
    pub(crate) fn transport(stage: MediaUploadStage, error: &reqwest::Error) -> Self {
        Self::Transport {
            stage,
            timed_out: error.is_timeout(),
        }
    }
}

impl std::fmt::Display for MediaUploadFailure {
    /// Writes a safe explanation of these typed facts to `formatter`, returning the formatting result.
    /// 向`formatter`写入这些强类型事实的安全说明，返回格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Media upload cancelled."),
            Self::Timeout { channel_id, timeout_ms } => write!(formatter, "Upload channel `{channel_id}` exceeded its declared {timeout_ms} ms total timeout."),
            Self::Http { stage, status } => write!(formatter, "Media upload {stage} was rejected with HTTP {status}."),
            Self::Transport { stage, timed_out: true } => write!(formatter, "Media upload {stage} transport timed out."),
            Self::Transport { stage, timed_out: false } => write!(formatter, "Media upload {stage} transport failed."),
            Self::UnsupportedMime => formatter.write_str("The selected upload interface rejects MIME type."),
            Self::SourceTooLarge { actual_bytes, max_bytes } => write!(formatter, "The selected upload interface rejects resource of {actual_bytes} bytes (limit {max_bytes})."),
            Self::TooManyResources { actual_count, max_count } => write!(formatter, "The selected upload interface rejects {actual_count} items (limit {max_count})."),
            Self::SourceUnavailable => formatter.write_str("Media upload source could not be verified or read."),
            Self::SourceChanged => formatter.write_str("Media source changed from its canonical identity."),
            Self::InvalidContract => formatter.write_str("The frozen media upload contract is invalid."),
            Self::MissingUploadChannel => formatter.write_str("Planned provider upload has no executable channel; inline fallback is forbidden."),
            Self::MissingSigningPrimitive => formatter.write_str("Media upload requires a registered signing primitive; the engine does not guess signatures."),
            Self::CredentialUnavailable => formatter.write_str("Media upload credential could not be resolved for the frozen account."),
            Self::InvalidResponse { stage } => write!(formatter, "Media upload {stage} returned an unreadable or invalid response."),
            Self::MissingReference => formatter.write_str("Media upload response is missing its required provider reference."),
            Self::InvalidReference => formatter.write_str("Media upload returned an invalid reference for the selected interface."),
        }
    }
}

impl std::error::Error for MediaUploadFailure {}

impl From<MediaUploadFailure> for String {
    /// Converts typed `error` only at an existing string-returning API boundary, without reclassification.
    /// 仅在既有字符串返回API边界转换强类型`error`，不重新分类。
    fn from(error: MediaUploadFailure) -> Self {
        error.to_string()
    }
}
