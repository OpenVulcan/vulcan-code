//! Closed provider resource download scheme policy.
//! 封闭的供应商资源下载 Scheme 策略。

use serde::{Deserialize, Serialize};

/// Closed host-side download policy carried with one provider resource URL.
/// 随单个供应商资源 URL 携带的封闭宿主侧下载策略。
///
/// The configured provider already owns the response, so any host it returns is accepted;
/// only the scheme family and credential-free form are enforced.
/// 已配置的供应商本身拥有该响应，因此接受其返回的任何宿主；只强制 Scheme 范围与不含凭据的形式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolResourceDownloadPolicy {
    /// Reviewed URL encoding, including the exact permitted scheme family.
    /// 已审核的 URL 编码，包含精确允许的 Scheme 范围。
    pub encoding: crate::ProviderToolResourceEncoding,
}

impl ProviderToolResourceDownloadPolicy {
    /// Validates one provider-returned URL against the frozen scheme family.
    /// 根据冻结的 Scheme 范围校验供应商返回的 URL。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the provider locator and `label` scopes controlled diagnostics.
    /// `value` 是供应商定位符，`label` 用于限定受控诊断信息。
    ///
    /// # Returns
    /// 返回值
    /// A parsed URL with an absolute host, the declared scheme, and no embedded credentials.
    /// 返回带绝对宿主、声明 Scheme 且不含内嵌凭据的解析 URL。
    pub fn validate_url(&self, value: &str, label: &str) -> Result<reqwest::Url, String> {
        let url = reqwest::Url::parse(value).map_err(|_| format!("{label} is not a valid URL."))?;
        let scheme_allowed = match self.encoding {
            crate::ProviderToolResourceEncoding::HttpsUrl => url.scheme() == "https",
            crate::ProviderToolResourceEncoding::PublicHttpUrl => {
                matches!(url.scheme(), "http" | "https")
            }
            _ => false,
        };
        if !scheme_allowed {
            return Err(format!("{label} URL must use the declared scheme."));
        }
        if url.host_str().is_none() {
            return Err(format!("{label} must declare an absolute host."));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(format!("{label} URL must not embed credentials."));
        }
        Ok(url)
    }
}
