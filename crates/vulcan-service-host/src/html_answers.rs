//! HTTP isolation shared by production HTML answers and real-browser security acceptance.
//! 正式 HTML 回答与真实浏览器安全验收共用的 HTTP 隔离策略。

use axum::{
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Response},
};
use url::Url;

mod grants;
mod routes;
mod runtime;
#[cfg(test)]
mod tests_runtime;
pub use runtime::{
    HtmlAnswerContent, HtmlAnswerRuntime, HtmlAnswerRuntimeConfig, HtmlAnswerSourceResolver,
};

/// Validated origins for the trusted viewer and the separate read-only render listener.
/// 可信查看器与独立只读渲染监听器的已校验来源。
#[derive(Clone, Debug)]
pub struct HtmlAnswerOrigins {
    /// Canonical HTTP(S) origin serving only the trusted viewer application.
    /// 只提供可信查看器应用的规范 HTTP(S) 来源。
    viewer: String,
    /// Separate canonical HTTP(S) origin serving only isolated generated content.
    /// 只提供隔离生成内容的另一规范 HTTP(S) 来源。
    renderer: String,
}

impl HtmlAnswerOrigins {
    /// Validate exact, distinct `viewer` and `renderer` origins; reject paths and opaque schemes.
    /// 校验精确且不同的 `viewer` 与 `renderer` 来源；拒绝路径和不透明协议。
    /// Returns the immutable policy inputs, or a non-sensitive configuration error.
    /// 返回不可变策略输入，或者不含敏感信息的配置错误。
    pub fn new(viewer: &str, renderer: &str) -> Result<Self, &'static str> {
        for origin in [viewer, renderer] {
            validate_origin(origin)?;
        }
        if viewer == renderer {
            return Err("HTML answers require a separate render origin");
        }
        Ok(Self {
            viewer: viewer.to_owned(),
            renderer: renderer.to_owned(),
        })
    }

    /// Borrow the validated trusted viewer origin; never reconstruct it from a request Host header.
    /// 借用已校验的可信查看器来源；绝不依据请求 Host 头重新构造。
    pub fn viewer(&self) -> &str {
        &self.viewer
    }

    /// Borrow the validated generated-content origin, with no Core routes behind it.
    /// 借用已校验的生成内容来源；其后不得有 Core 路由。
    pub fn renderer(&self) -> &str {
        &self.renderer
    }

    /// Build the trusted shell's headers, restricting all child navigations to the render origin.
    /// 构造可信外壳响应头，将所有子框架导航限制到渲染来源。
    /// External executable bundles must be same-origin; `script-src` never allows inline shell code.
    /// 外部可执行包必须同源；`script-src` 绝不允许内联外壳脚本。
    pub fn viewer_headers(&self) -> HeaderMap {
        viewer_headers(Some(self.renderer()))
    }

    /// Build generated-document headers: page-local scripts only, an opaque origin and no host APIs.
    /// 构造生成文档响应头：仅允许页内脚本，使用不透明来源且无宿主 API。
    /// A response-header sandbox also protects direct loads; iframe sandbox must additionally remain set.
    /// 响应头沙箱也保护直接加载；iframe 沙箱仍必须同时保留。
    pub fn content_headers(&self) -> HeaderMap {
        let mut headers = common_headers();
        let csp = format!(
            "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; \
             style-src 'unsafe-inline'; img-src data:; font-src data:; media-src data:; \
             connect-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; \
             object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors {}",
            trusted_ancestors(&self.viewer)
        );
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(&csp).expect("validated origins form a header"),
        );
        headers
    }

    /// Serve already-authorized `source` as isolated HTML with this policy; returns an HTTP response.
    /// 按此策略将已授权的 `source` 作为隔离 HTML 提供；返回 HTTP 响应。
    /// Authorization and fresh source ownership checks are required before this method is called.
    /// 调用此方法之前必须完成授权及最新源码归属校验。
    pub fn content_response(&self, source: String) -> Response {
        // Establish standards mode and capture Escape before generated scripts register handlers.
        // 先建立标准模式并捕获 Esc，再允许生成脚本注册处理器。
        // This response-only prefix never changes persisted/copied source or exposes a host capability.
        // 此响应专用前缀绝不改变持久化或复制的源码，也不暴露宿主能力。
        const CLOSE_BOOTSTRAP: &str = concat!(
            "<!doctype html><script>",
            "addEventListener('keydown',function(event){",
            "if(event.key==='Escape')parent.postMessage('vulcan-html-answer-close-v1','*')",
            "},true);</script>"
        );
        (self.content_headers(), format!("{CLOSE_BOOTSTRAP}{source}")).into_response()
    }
}

/// Validate a configured canonical HTTP origin, returning a non-sensitive error on rejection.
/// 校验已配置的规范 HTTP 来源；拒绝时返回不含敏感信息的错误。
fn validate_origin(origin: &str) -> Result<(), &'static str> {
    let parsed = Url::parse(origin).map_err(|_| "Invalid HTML answer origin")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.origin().ascii_serialization() != origin
    {
        return Err("HTML answers require canonical HTTP(S) origins without paths");
    }
    Ok(())
}

/// Build viewer policy before or after lazy startup; an absent renderer permits no frames.
/// 在按需启动前后构建查看器策略；缺少渲染来源时不允许任何框架。
fn viewer_headers(renderer: Option<&str>) -> HeaderMap {
    let mut headers = common_headers();
    let csp = format!(
        "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src data:; connect-src 'self'; frame-src {}; worker-src 'none'; \
         object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors {}",
        renderer.unwrap_or("'none'"),
        trusted_ancestors("'self'")
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_str(&csp).expect("validated origins form a header"),
    );
    headers
}

/// Permit the configured viewer and the one existing debug workbench origin as ancestors.
/// 允许已配置查看器及唯一既有调试工作台来源作为祖先。
/// All ancestors of nested frames must match CSP; release builds retain only the configured origin.
/// 嵌套框架的所有祖先都必须匹配 CSP；发行构建仅保留配置来源。
fn trusted_ancestors(primary: &str) -> String {
    if cfg!(debug_assertions) {
        format!("{primary} {}", crate::DEV_WEBUI_ORIGIN)
    } else {
        primary.to_owned()
    }
}

/// Return no-store, no-referrer and restricted-device defaults for either HTML surface.
/// 返回两个 HTML 界面共用的禁止缓存、禁止来源外传和设备限制默认响应头。
fn common_headers() -> HeaderMap {
    HeaderMap::from_iter([
        (
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        ),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
        (
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ),
        (
            header::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static(
                "camera=(), microphone=(), geolocation=(), display-capture=(), usb=(), serial=(), \
             bluetooth=(), hid=(), payment=(), clipboard-read=(), clipboard-write=()",
            ),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configuration cannot merge trust boundaries, inject CSP tokens, or retain URL credentials.
    /// 配置不能合并信任边界、注入 CSP 标记或保留 URL 凭据。
    #[test]
    fn html_answers_origins_require_canonical_separate_http_origins() {
        let viewer = "http://127.0.0.1:7101";
        let renderer = "http://127.0.0.1:7102";
        let origins = HtmlAnswerOrigins::new(viewer, renderer).unwrap();
        assert_eq!(origins.viewer(), viewer);
        assert_eq!(origins.renderer(), renderer);
        assert!(HtmlAnswerOrigins::new(viewer, viewer).is_err());
        for invalid in [
            "null",
            "data:text/html,x",
            "file:///x",
            "https://user:password@example.com",
            "https://example.com/a",
            "https://example.com?q=x",
            "https://example.com#fragment",
            "https://example.com; script-src *",
            "http://localhost\r\nX-Header: value",
        ] {
            assert!(
                HtmlAnswerOrigins::new(invalid, renderer).is_err(),
                "unexpectedly accepted {invalid}"
            );
            assert!(
                HtmlAnswerOrigins::new(viewer, invalid).is_err(),
                "unexpectedly accepted {invalid}"
            );
        }
    }
}
