use super::*;

/// Converts a browser WebUI request path into a traversal-safe relative static path.
/// 将浏览器 WebUI 请求路径转换为防穿越的相对静态路径。
///
/// # Parameters
/// 参数
/// `request_path` is the URL path received by the browser-facing static route.
/// `request_path` 是浏览器侧静态路由收到的 URL 路径。
///
/// # Returns
/// 返回值
/// A relative path rooted under the WebUI static bundle directory.
/// 返回位于 WebUI 静态包目录下的相对路径。
pub fn webui_static_relative_path(request_path: &str) -> Result<PathBuf, String> {
    let trimmed = request_path.trim_start_matches('/');
    let normalized = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };
    let mut relative_path = PathBuf::new();
    for segment in normalized.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." || segment.contains('\\') || segment.contains(':') {
            return Err(String::from("invalid WebUI static path segment"));
        }
        relative_path.push(segment);
    }
    if relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(String::from("invalid WebUI static path traversal"));
    }
    Ok(relative_path)
}

/// Infers a conservative MIME type for WebUI static assets.
/// 为 WebUI 静态资源推断保守 MIME 类型。
///
/// # Parameters
/// 参数
/// `path` is the resolved static file path or relative static asset path.
/// `path` 是已解析的静态文件路径或相对静态资源路径。
///
/// # Returns
/// 返回值
/// A static MIME type suitable for HTTP response headers.
/// 返回适用于 HTTP 响应头的静态 MIME 类型。
pub fn webui_static_mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("map") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Framework-neutral static file response returned by the WebUI static pipeline.
/// WebUI 静态文件管线返回的框架无关静态响应。
pub struct ServiceStaticResponse {
    /// Numeric HTTP status code.
    /// 数字 HTTP 状态码。
    pub status: u16,
    /// Response headers preserved in insertion order.
    /// 按插入顺序保留的响应头。
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    /// 响应体字节。
    pub body: Vec<u8>,
}

/// Resolves the WebUI static root from an optional override and fallback directory.
/// 根据可选覆盖目录和回退目录解析 WebUI 静态根目录。
///
/// # Parameters
/// 参数
/// `override_path` is the raw configured static directory and `fallback_path` is the bundled dist directory.
/// `override_path` 是原始配置静态目录，`fallback_path` 是随包发布的 dist 目录。
///
/// # Returns
/// 返回值
/// The first existing directory, preferring the override path when valid.
/// 返回第一个存在的目录，并在覆盖目录有效时优先使用覆盖目录。
pub fn webui_static_root_from_candidates(
    override_path: Option<&str>,
    fallback_path: Option<PathBuf>,
) -> Option<PathBuf> {
    let override_path = override_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_dir());
    if override_path.is_some() {
        return override_path;
    }
    fallback_path.filter(|path| path.is_dir())
}

/// Builds one WebUI static response from a configured static root.
/// 从已配置的静态根目录构建单个 WebUI 静态响应。
///
/// # Parameters
/// 参数
/// `static_root` is the optional WebUI bundle directory, `request_path` is the browser path, `cache_control` is the response cache policy, and `missing_message` is the 404 text.
/// `static_root` 是可选 WebUI 包目录，`request_path` 是浏览器路径，`cache_control` 是响应缓存策略，`missing_message` 是 404 文本。
///
/// # Returns
/// 返回值
/// A framework-neutral static response ready for HTTP adapter conversion.
/// 返回可供 HTTP 适配器转换的框架无关静态响应。
pub fn build_webui_static_response(
    static_root: Option<&Path>,
    request_path: &str,
    cache_control: &str,
    missing_message: &str,
) -> ServiceStaticResponse {
    let Some(static_root) = static_root else {
        return service_static_text_response(404, missing_message);
    };
    match webui_static_file_path_for_request(static_root, request_path) {
        Ok(Some(path)) => build_webui_static_file_response(path, cache_control),
        Ok(None) => service_static_text_response(404, missing_message),
        Err(error) => service_static_text_response(400, &error),
    }
}

/// Resolves one request path into a safe WebUI static file path.
/// 将单个请求路径解析为安全的 WebUI 静态文件路径。
///
/// # Parameters
/// 参数
/// `static_root` is the WebUI bundle directory and `request_path` is the browser route path.
/// `static_root` 是 WebUI 包目录，`request_path` 是浏览器路由路径。
///
/// # Returns
/// 返回值
/// `Some(path)` for an existing static file or SPA fallback, `None` when no file should be served.
/// 对存在的静态文件或 SPA 回退返回 `Some(path)`，无可服务文件时返回 `None`。
pub fn webui_static_file_path_for_request(
    static_root: &Path,
    request_path: &str,
) -> Result<Option<PathBuf>, String> {
    let relative_path = webui_static_relative_path(request_path)?;
    let file_path = static_root.join(&relative_path);
    if file_path.is_file() {
        return Ok(Some(file_path));
    }
    if relative_path.extension().is_some() {
        return Ok(None);
    }
    let index_path = static_root.join("index.html");
    if index_path.is_file() {
        return Ok(Some(index_path));
    }
    Ok(None)
}

/// Builds one static WebUI file response with a conservative content type.
/// 使用保守的内容类型构建单个 WebUI 静态文件响应。
fn build_webui_static_file_response(path: PathBuf, cache_control: &str) -> ServiceStaticResponse {
    match fs::read(&path) {
        Ok(bytes) => ServiceStaticResponse {
            status: 200,
            headers: vec![
                (
                    String::from("content-type"),
                    String::from(webui_static_mime_type(&path)),
                ),
                (String::from("cache-control"), String::from(cache_control)),
            ],
            body: bytes,
        },
        Err(error) => {
            service_static_text_response(500, &format!("failed to read WebUI static file: {error}"))
        }
    }
}

/// Builds one text response for browser-service static file failures.
/// 为浏览器服务静态文件失败场景构建单个文本响应。
fn service_static_text_response(status: u16, message: &str) -> ServiceStaticResponse {
    ServiceStaticResponse {
        status,
        headers: vec![(
            String::from("content-type"),
            String::from("text/plain; charset=utf-8"),
        )],
        body: message.as_bytes().to_vec(),
    }
}

/// Returns whether one browser Origin string may access the loopback WebUI service.
/// 返回单个浏览器 Origin 字符串是否允许访问回环 WebUI 服务。
///
/// # Parameters
/// 参数
/// `origin` is the raw Origin header value.
/// `origin` 是原始 Origin 请求头值。
///
/// # Returns
/// 返回值
/// `true` for trusted Tauri origins, the Windows Tauri HTTP origin, and loopback HTTP origins.
/// 对受信任的 Tauri 来源、Windows Tauri HTTP 来源和回环 HTTP 来源返回 `true`。
pub fn webui_service_origin_text_is_allowed(origin: &str) -> bool {
    let Some((scheme, remainder)) = origin.split_once("://") else {
        return false;
    };
    let host_port = remainder.split('/').next().unwrap_or_default();
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    if scheme == "tauri" {
        return host.eq_ignore_ascii_case("localhost");
    }
    if scheme != "http" && scheme != "https" {
        return false;
    }
    if host.eq_ignore_ascii_case("tauri.localhost") {
        return true;
    }
    matches!(
        host.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

/// Returns whether a required browser Origin value is present and allowed.
/// 返回必需的浏览器 Origin 值是否存在且被允许。
///
/// # Parameters
/// 参数
/// `origin` is the optional raw Origin header value extracted by the transport adapter.
/// `origin` 是传输适配层提取出的可选原始 Origin 请求头值。
///
/// # Returns
/// 返回值
/// `true` only when the Origin header exists and passes loopback origin validation.
/// 仅当 Origin 请求头存在且通过回环来源校验时返回 `true`。
pub fn webui_service_required_origin_text_is_allowed(origin: Option<&str>) -> bool {
    origin.is_some_and(webui_service_origin_text_is_allowed)
}
