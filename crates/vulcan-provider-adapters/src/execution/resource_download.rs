//! Closed provider resource download authority and host matching.
//! 封闭的供应商资源下载权限与宿主匹配。

use serde::{Deserialize, Serialize};

/// Closed host-side download authority carried with one provider resource URL.
/// 随单个供应商资源 URL 携带的封闭宿主侧下载权限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolResourceDownloadPolicy {
    /// Reviewed URL encoding, including the exact permitted scheme family.
    /// 已审核的 URL 编码，包含精确允许的 Scheme 范围。
    pub encoding: crate::ProviderToolResourceEncoding,
    /// Reviewed exact hosts or dot-prefixed DNS suffixes accepted from the provider response.
    /// 允许供应商响应返回的已审核精确宿主或点前缀 DNS 后缀。
    pub host_suffixes: Vec<String>,
}

impl ProviderToolResourceDownloadPolicy {
    /// Validates one provider-returned URL against the frozen scheme and host authority.
    /// 根据冻结的 Scheme 与宿主权限校验供应商返回的 URL。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the untrusted provider locator and `label` scopes controlled diagnostics.
    /// `value` 是不受信任的供应商定位符，`label` 用于限定受控诊断信息。
    ///
    /// # Returns
    /// 返回值
    /// A parsed URL only when it remains inside the complete declared download boundary.
    /// 仅当 URL 完整处于声明的下载边界内时返回解析结果。
    pub fn validate_url(&self, value: &str, label: &str) -> Result<reqwest::Url, String> {
        self.validate_url_with_optional_exact_origin(value, label, None)
    }

    /// Validates a provider-returned URL while admitting one frozen exact operation origin.
    /// 校验供应商返回 URL，同时允许一个冻结的精确操作 Origin。
    ///
    /// # Parameters
    /// 参数
    /// `value` and `label` retain the ordinary download contract; `exact_origin` is the
    /// already-validated endpoint that initiated this provider task.
    /// `value` 与 `label` 保留普通下载契约；`exact_origin` 是启动该供应商任务的
    /// 已验证端点。
    ///
    /// # Returns
    /// 返回值
    /// A parsed URL only when a non-default port matches the frozen operation origin exactly.
    /// 仅当非默认端口与冻结操作 Origin 精确一致时返回解析 URL。
    pub(crate) fn validate_url_with_exact_origin(
        &self,
        value: &str,
        label: &str,
        exact_origin: &reqwest::Url,
    ) -> Result<reqwest::Url, String> {
        self.validate_url_with_optional_exact_origin(value, label, Some(exact_origin))
    }

    /// Implements URL validation with an optional host-owned exact-origin port grant.
    /// 使用可选的宿主持有精确 Origin 端口授权实现 URL 校验。
    ///
    /// # Parameters
    /// 参数
    /// `value` is untrusted, `label` scopes diagnostics, and `exact_origin` never comes from the
    /// response being checked.
    /// `value` 不受信任，`label` 限定诊断，`exact_origin` 绝不来自当前被校验响应。
    ///
    /// # Returns
    /// 返回值
    /// The same closed result as `validate_url` with no broadened host or scheme authority.
    /// 返回与 `validate_url` 相同的封闭结果，不扩大宿主或 Scheme 权限。
    fn validate_url_with_optional_exact_origin(
        &self,
        value: &str,
        label: &str,
        exact_origin: Option<&reqwest::Url>,
    ) -> Result<reqwest::Url, String> {
        if self.host_suffixes.is_empty()
            || self.host_suffixes.len() > 16
            || self
                .host_suffixes
                .iter()
                .any(|suffix| !provider_resource_host_suffix_is_safe(suffix))
        {
            return Err(format!(
                "{label} download policy has no valid bounded host authority."
            ));
        }
        let url = reqwest::Url::parse(value).map_err(|_| format!("{label} is not a valid URL."))?;
        let scheme_allowed = match self.encoding {
            crate::ProviderToolResourceEncoding::HttpsUrl => url.scheme() == "https",
            crate::ProviderToolResourceEncoding::PublicHttpUrl => {
                matches!(url.scheme(), "http" | "https")
            }
            _ => false,
        };
        let host = url
            .host_str()
            .ok_or_else(|| format!("{label} must declare an absolute host."))?;
        // portAllowed grants the default scheme port or the one exact initiating operation origin;
        // response data can never introduce a third network authority.
        // portAllowed 只授权 Scheme 默认端口或精确的启动操作 Origin；响应数据绝不能
        // 引入第三个网络权限。
        let port_allowed = url.port().is_none()
            || exact_origin.is_some_and(|origin| provider_resource_urls_share_origin(&url, origin));
        if !scheme_allowed
            || !url.username().is_empty()
            || url.password().is_some()
            || !port_allowed
            || url.fragment().is_some()
            || !provider_resource_host_matches_suffixes(host, &self.host_suffixes)
        {
            return Err(format!(
                "{label} URL host `{host}` is outside the declared scheme, default port or exact operation origin, and host suffixes."
            ));
        }
        Ok(url)
    }
}

/// Reports whether two parsed URLs select the same scheme, host, and effective port.
/// 报告两个已解析 URL 是否选择相同 Scheme、宿主与有效端口。
///
/// # Parameters
/// 参数
/// `candidate` is provider-returned and `authority` is the frozen initiating endpoint.
/// `candidate` 由供应商返回；`authority` 是冻结的启动端点。
///
/// # Returns
/// 返回值
/// True only for an exact network-origin match.
/// 仅在网络 Origin 精确匹配时返回真。
fn provider_resource_urls_share_origin(candidate: &reqwest::Url, authority: &reqwest::Url) -> bool {
    candidate.scheme() == authority.scheme()
        && candidate.host_str().zip(authority.host_str()).is_some_and(
            |(candidate_host, authority_host)| candidate_host.eq_ignore_ascii_case(authority_host),
        )
        && candidate.port_or_known_default() == authority.port_or_known_default()
}

/// Reports whether one manifest host matcher is a safe exact host or DNS suffix.
/// 报告一个 Manifest 宿主匹配项是否为安全的精确宿主或 DNS 后缀。
///
/// # Parameters
/// 参数
/// `suffix` is an exact IPv4/DNS host or a dot-prefixed DNS subdomain suffix.
/// `suffix` 是精确 IPv4/DNS 宿主或点前缀 DNS 子域后缀。
///
/// # Returns
/// 返回值
/// True only for bounded lowercase host syntax without wildcard or URL components.
/// 仅对不含通配符或 URL 组件的有界小写宿主语法返回真。
pub fn provider_resource_host_suffix_is_safe(suffix: &str) -> bool {
    if suffix.is_empty() || suffix.len() > 253 {
        return false;
    }
    let host = suffix.strip_prefix('.').unwrap_or(suffix);
    if host.is_empty() || host.parse::<std::net::Ipv6Addr>().is_ok() {
        return false;
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return !suffix.starts_with('.');
    }
    if !host.contains('.') || host.ends_with('.') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().next().is_some_and(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit()
            })
            && label.chars().last().is_some_and(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit()
            })
            && label.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
    })
}

/// Reports whether one parsed host belongs to a manifest-owned exact/suffix allowlist.
/// 报告一个已解析宿主是否属于 Manifest 持有的精确/后缀允许列表。
///
/// # Parameters
/// 参数
/// `host` is the parsed URL host and `suffixes` contains already validated matchers.
/// `host` 是解析后的 URL 宿主，`suffixes` 包含已经校验的匹配项。
///
/// # Returns
/// 返回值
/// True only for an exact match or a non-root dot-suffix match.
/// 仅在精确匹配或非根域点后缀匹配时返回真。
pub fn provider_resource_host_matches_suffixes(host: &str, suffixes: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    suffixes.iter().any(|suffix| {
        if suffix.starts_with('.') {
            host.len() > suffix.len() && host.ends_with(suffix)
        } else {
            host == *suffix
        }
    })
}
