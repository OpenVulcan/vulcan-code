//! Localized, self-contained browser results for the native OpenVulcan flow.
//! 原生 OpenVulcan 流程的本地化自包含浏览器结果。

use crate::OpenVulcanRegion;
use tokio::{io::AsyncWriteExt, net::TcpStream};
use vulcan_i18n::{OAUTH, Translator};

/// Pre-rendered public pages contain neither transaction selectors nor credentials.
/// 预渲染的公开页面不包含事务选择器或凭据。
pub(super) struct CallbackPages {
    /// Token exchange completed; application account verification remains a separate step.
    /// 令牌交换已完成；应用内账户验证仍是独立步骤。
    pub accepted: String,
    /// A valid callback could not complete the token exchange.
    /// 有效回调未能完成令牌交换。
    pub failed: String,
    /// Unrelated or invalid local traffic must not consume the live authorization.
    /// 无关或无效本地流量不得消费进行中的授权。
    pub rejected: String,
}

impl CallbackPages {
    /// Builds each result for region and the optional saved locale, using system detection when unset.
    /// 为 region 与可选已保存语言构建各结果页；未设置时使用系统语言检测。
    /// Returns escaped HTML or an explicit catalog error before browser authorization starts.
    /// 在浏览器授权开始前返回已转义 HTML 或明确的目录错误。
    pub fn new(region: OpenVulcanRegion, locale: Option<&str>) -> Result<Self, String> {
        // translator uses the same registry and matching rules as every other product surface.
        // translator 使用与产品其他界面相同的注册表和匹配规则。
        let mut translator = Translator::new(&[&OAUTH]);
        if !translator.set_locale(locale.unwrap_or_else(|| vulcan_i18n::system_locale())) {
            return Err(String::from("Invalid OpenVulcan return page locale."));
        }
        Ok(Self {
            accepted: render(&translator, region, "accepted")?,
            failed: render(&translator, region, "failed")?,
            rejected: render(&translator, region, "rejected")?,
        })
    }
}

/// Renders a fixed outcome with catalog copy; all replacement values are HTML escaped.
/// 使用目录文案渲染固定结果；所有替换值均进行 HTML 转义。
/// Parameters identify the translator, regional website and internal outcome; returns one HTML document.
/// 参数标识翻译器、区域官网及内部结果；返回一份 HTML 文档。
fn render(
    translator: &Translator,
    region: OpenVulcanRegion,
    outcome: &str,
) -> Result<String, String> {
    // html is the authoritative resource, so the loopback listener needs no network assets.
    // html 是权威资源，因此环回监听器不需要任何网络素材。
    let mut html = vulcan_runtime_assets::OPENVULCAN_CALLBACK_HTML.to_owned();
    // copy resolves required keys explicitly instead of manufacturing missing interface text.
    // copy 显式解析必需键，不生成缺失的界面文案。
    let copy = |key: &str| {
        translator
            .text(key)
            .ok_or_else(|| format!("Missing OpenVulcan return page message: {key}"))
    };
    // edition and site_url follow the exact selected deployment; no cross-region redirect is inferred.
    // edition 和 site_url 遵循准确的所选部署；不推导跨区域重定向。
    let (edition, site_url) = match region {
        OpenVulcanRegion::China => ("openvulcan.callback.china", "https://openvulcan.cn/"),
        OpenVulcanRegion::Global => ("openvulcan.callback.global", "https://openvulcan.com/"),
    };
    for (placeholder, value) in [
        ("locale", translator.locale()),
        ("outcome", outcome),
        ("site_url", site_url),
        ("edition", copy(edition)?),
        (
            "title",
            copy(&format!("openvulcan.callback.{outcome}.title"))?,
        ),
        (
            "description",
            copy(&format!("openvulcan.callback.{outcome}.description"))?,
        ),
        ("next_title", copy("openvulcan.callback.nextTitle")?),
        (
            "next_description",
            copy(&format!("openvulcan.callback.{outcome}.next"))?,
        ),
        ("close", copy("openvulcan.callback.close")?),
        ("close_help", copy("openvulcan.callback.closeHelp")?),
        ("footer", copy("openvulcan.callback.footer")?),
    ] {
        html = html.replace(&format!("{{{{{placeholder}}}}}"), &escape_html(value));
    }
    Ok(html)
}

/// Escapes text and quoted attributes without permitting translated copy to introduce markup.
/// 转义文本与带引号的属性，防止翻译文案引入标记。
/// Accepts plain text and returns its HTML-safe representation.
/// 接收纯文本并返回可安全放入 HTML 的形式。
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Writes a complete result to stream with the supplied HTTP status and prevents caching or referrer leaks.
/// 按给定 HTTP 状态向 stream 写入完整结果，并阻止缓存或来源泄漏。
/// Returns a transport diagnostic without request data if the browser connection is gone.
/// 浏览器连接已断开时返回不含请求数据的传输诊断。
pub(super) async fn write(stream: &mut TcpStream, status: u16, body: &str) -> Result<(), String> {
    // reason is selected from the result statuses produced by this listener.
    // reason 从此监听器产出的结果状态中选择。
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        502 => "Bad Gateway",
        _ => return Err(String::from("Invalid OpenVulcan callback result status.")),
    };
    // response includes byte length rather than character count for localized non-ASCII copy.
    // response 使用字节长度而非字符数量，以支持本地化非 ASCII 文案。
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|_| String::from("Failed to write OpenVulcan authorization result page."))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every registered locale produces complete, state-specific documents for both deployments.
    /// 验证每种注册语言均为两个部署生成完整且区分状态的文档。
    #[test]
    fn result_documents_cover_registered_locales_and_regions() {
        for locale in OAUTH.locales() {
            for region in [OpenVulcanRegion::China, OpenVulcanRegion::Global] {
                // pages exercise the production renderer without any external authorization request.
                // pages 使用生产渲染器，不发起任何外部授权请求。
                let pages = CallbackPages::new(region, Some(locale)).unwrap();
                for (outcome, page) in [
                    ("accepted", pages.accepted),
                    ("failed", pages.failed),
                    ("rejected", pages.rejected),
                ] {
                    assert!(page.starts_with("<!doctype html>"));
                    assert!(page.contains(&format!("lang=\"{locale}\"")));
                    assert!(page.contains(&format!("data-outcome=\"{outcome}\"")));
                    assert!(
                        !page.contains("{{"),
                        "Unresolved template field for {locale}/{outcome}"
                    );
                }
            }
        }
        assert!(CallbackPages::new(OpenVulcanRegion::China, Some("invalid-locale")).is_err());
    }

    /// Verifies localized text cannot introduce tags, attributes, or unescaped entities.
    /// 验证本地化文本不能引入标签、属性或未转义实体。
    #[test]
    fn translated_values_are_escaped() {
        assert_eq!(escape_html("<&\"'>"), "&lt;&amp;&quot;&#39;&gt;");
    }
}
