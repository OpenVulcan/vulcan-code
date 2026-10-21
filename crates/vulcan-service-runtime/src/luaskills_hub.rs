//! Read-only LuaSkills Hub catalog search used by the skill install surfaces.
//! 技能安装界面使用的只读 LuaSkills Hub 目录搜索。
//!
//! Installation itself stays inside the LuaSkills engine through the `official_hub` source
//! type; this module only lists what the configured Hub can resolve, against the same Hub
//! base URL the running engine was built with.
//! 安装本身仍通过 `official_hub` 来源类型留在 LuaSkills 引擎内；本模块只针对运行中引擎构建时
//! 使用的同一 Hub 基础地址，列出该 Hub 可解析的技能。

use crate::luaskills_bridge::TauriLuaSkillsHandle;
use reqwest::blocking::Client;
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::time::Duration;

/// Number of skills requested for one search page.
/// 单个搜索页请求的技能数量。
const HUB_SEARCH_PAGE_SIZE: u32 = 24;

/// Longest accepted search text, counted in Unicode scalar values.
/// 可接受的最长搜索文本，按 Unicode 标量值计数。
const HUB_SEARCH_QUERY_MAX_CHARS: usize = 100;

/// Longest accepted opaque page cursor.
/// 可接受的最长不透明分页游标。
const HUB_SEARCH_CURSOR_MAX_CHARS: usize = 12;

/// Largest accepted search response body.
/// 可接受的最大搜索响应体。
const HUB_SEARCH_RESPONSE_MAX_BYTES: u64 = 1024 * 1024;

/// Longest remote error text echoed back to the caller.
/// 回显给调用方的最长远端错误文本。
const HUB_ERROR_TEXT_MAX_CHARS: usize = 200;

/// HTTP connect timeout for one Hub search request.
/// 单次 Hub 搜索请求的 HTTP 连接超时。
const HUB_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// HTTP request timeout for one complete Hub search request.
/// 单次完整 Hub 搜索请求的 HTTP 请求超时。
const HUB_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// One page of Hub search results.
/// 一页 Hub 搜索结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsHubSearchPage {
    /// Hub base URL that answered this search.
    /// 响应本次搜索的 Hub 基础地址。
    pub hub_base_url: String,
    /// Skills on this page in Hub ranking order.
    /// 按 Hub 排序的本页技能。
    pub items: Vec<LuaSkillsHubSkill>,
    /// Cursor of the next page, absent on the last page.
    /// 下一页游标，最后一页时缺省。
    pub next_cursor: Option<String>,
}

/// One installable skill listed by the Hub.
/// Hub 列出的一个可安装技能。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct LuaSkillsHubSkill {
    /// Flat skill identifier accepted by the `official_hub` install source.
    /// `official_hub` 安装来源接受的扁平技能标识。
    pub skill_id: String,
    /// Publisher display name.
    /// 发布者显示名称。
    #[serde(default)]
    pub publisher: String,
    /// Human-readable skill name.
    /// 人类可读的技能名称。
    #[serde(default)]
    pub display_name: String,
    /// One-line skill summary.
    /// 单行技能摘要。
    #[serde(default)]
    pub summary: String,
    /// Source repository URL, empty when unpublished.
    /// 源码仓库地址，未公开时为空。
    #[serde(default)]
    pub repository_url: String,
    /// Project homepage URL, empty when unpublished.
    /// 项目主页地址，未公开时为空。
    #[serde(default)]
    pub homepage_url: String,
    /// SPDX license expression declared by the publisher.
    /// 发布者声明的 SPDX 许可证表达式。
    #[serde(default)]
    pub license: String,
    /// Search tags.
    /// 搜索标签。
    #[serde(default)]
    pub tags: Vec<String>,
    /// Latest installable version.
    /// 最新可安装版本。
    #[serde(default)]
    pub latest_version: String,
    /// RFC 3339 time of the latest catalog change.
    /// 最近一次目录变更的 RFC 3339 时间。
    #[serde(default)]
    pub updated_at: String,
}

/// Wire shape of one Hub search response.
/// 单个 Hub 搜索响应的线协议形状。
#[derive(Debug, Deserialize)]
struct HubSearchResponse {
    /// Skills on this page.
    /// 本页技能。
    items: Vec<LuaSkillsHubSkill>,
    /// Next page cursor, empty on the last page.
    /// 下一页游标，最后一页为空。
    #[serde(default)]
    next_cursor: String,
}

/// Wire shape of one Hub error response.
/// 单个 Hub 错误响应的线协议形状。
#[derive(Debug, Deserialize)]
struct HubErrorResponse {
    /// Structured error body.
    /// 结构化错误体。
    error: HubErrorBody,
}

/// Structured Hub error body.
/// 结构化 Hub 错误体。
#[derive(Debug, Deserialize)]
struct HubErrorBody {
    /// Stable machine-readable error code.
    /// 稳定的机器可读错误码。
    #[serde(default)]
    code: String,
    /// English diagnostic message.
    /// 英文诊断消息。
    #[serde(default)]
    message: String,
}

/// Searches the Hub the running LuaSkills engine installs `official_hub` skills from.
/// 搜索运行中 LuaSkills 引擎安装 `official_hub` 技能所用的 Hub。
///
/// # Errors
/// Returns an English error when the query or cursor is out of bounds, the Hub base URL is
/// invalid, or the Hub request fails.
/// 查询或游标越界、Hub 基础地址无效或 Hub 请求失败时返回英文错误。
pub fn search_official_hub(
    handle: &TauriLuaSkillsHandle,
    query: &str,
    cursor: Option<&str>,
) -> Result<LuaSkillsHubSearchPage, String> {
    let hub_base_url = handle
        .effective_runtime_settings
        .network
        .effective_official_skill_hub_base_url();
    search_hub(&hub_base_url, query, cursor)
}

/// Searches one explicit Hub base URL.
/// 搜索一个显式指定的 Hub 基础地址。
fn search_hub(
    hub_base_url: &str,
    query: &str,
    cursor: Option<&str>,
) -> Result<LuaSkillsHubSearchPage, String> {
    let url = build_search_url(hub_base_url, query, cursor)?;
    let client = Client::builder()
        .connect_timeout(HUB_CONNECT_TIMEOUT)
        .timeout(HUB_REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!(
            "Vulcan-Code/{} luaskills-hub",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|error| format!("failed to create LuaSkills Hub HTTP client: {error}"))?;
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|error| format!("LuaSkills Hub search request failed: {error}"))?;
    let status = response.status();
    let body = read_bounded_body(response)?;
    if !status.is_success() {
        return Err(describe_hub_failure(status, &body));
    }
    let page: HubSearchResponse = serde_json::from_slice(&body)
        .map_err(|error| format!("LuaSkills Hub returned an invalid search response: {error}"))?;
    Ok(LuaSkillsHubSearchPage {
        hub_base_url: hub_base_url.trim_end_matches('/').to_string(),
        items: page.items,
        next_cursor: Some(page.next_cursor).filter(|cursor| !cursor.is_empty()),
    })
}

/// Builds the search endpoint URL after validating every caller-controlled member.
/// 校验每个调用方可控成员后构建搜索端点地址。
fn build_search_url(hub_base_url: &str, query: &str, cursor: Option<&str>) -> Result<Url, String> {
    let query = query.trim();
    if query.chars().count() > HUB_SEARCH_QUERY_MAX_CHARS {
        return Err(format!(
            "LuaSkills Hub search text must be at most {HUB_SEARCH_QUERY_MAX_CHARS} characters."
        ));
    }
    let cursor = cursor.map(str::trim).filter(|cursor| !cursor.is_empty());
    if let Some(cursor) = cursor
        && (cursor.len() > HUB_SEARCH_CURSOR_MAX_CHARS
            || !cursor.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(String::from("LuaSkills Hub search cursor is invalid."));
    }
    let mut url = Url::parse(&format!(
        "{}/api/v1/skills/search",
        hub_base_url.trim_end_matches('/')
    ))
    .map_err(|error| format!("LuaSkills Hub base URL is invalid: {error}"))?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(String::from(
            "LuaSkills Hub base URL must be an absolute http or https URL.",
        ));
    }
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("limit", &HUB_SEARCH_PAGE_SIZE.to_string());
        if !query.is_empty() {
            pairs.append_pair("q", query);
        }
        if let Some(cursor) = cursor {
            pairs.append_pair("cursor", cursor);
        }
    }
    Ok(url)
}

/// Reads a response body, refusing anything above the search response bound.
/// 读取响应体，拒绝超过搜索响应上限的内容。
fn read_bounded_body(response: reqwest::blocking::Response) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    response
        .take(HUB_SEARCH_RESPONSE_MAX_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| format!("failed to read the LuaSkills Hub response: {error}"))?;
    if body.len() as u64 > HUB_SEARCH_RESPONSE_MAX_BYTES {
        return Err(format!(
            "LuaSkills Hub response exceeds {HUB_SEARCH_RESPONSE_MAX_BYTES} bytes."
        ));
    }
    Ok(body)
}

/// Renders one failed Hub response, preferring its structured error body.
/// 渲染一个失败的 Hub 响应，优先使用其结构化错误体。
fn describe_hub_failure(status: StatusCode, body: &[u8]) -> String {
    match serde_json::from_slice::<HubErrorResponse>(body) {
        Ok(failure) => {
            let message: String = failure
                .error
                .message
                .chars()
                .take(HUB_ERROR_TEXT_MAX_CHARS)
                .collect();
            let code: String = failure
                .error
                .code
                .chars()
                .take(HUB_ERROR_TEXT_MAX_CHARS)
                .collect();
            format!("LuaSkills Hub search failed with HTTP {status} ({code}): {message}")
        }
        Err(_) => format!("LuaSkills Hub search failed with HTTP {status}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::thread;

    /// Serves exactly one canned HTTP response on a loopback port.
    /// 在回环端口上恰好提供一次预设 HTTP 响应。
    fn serve_once(status_line: &'static str, body: String) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept one request");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).expect("read request");
            let response = format!(
                "HTTP/1.1 {status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        (format!("http://{address}/"), server)
    }

    /// Verifies caller-controlled members are bounded before any request is built.
    /// 验证调用方可控成员在构建任何请求前即受边界约束。
    #[test]
    fn search_inputs_are_bounded_before_any_request() {
        let long_query = "a".repeat(HUB_SEARCH_QUERY_MAX_CHARS + 1);
        assert!(build_search_url("https://hub.example", &long_query, None).is_err());
        assert!(build_search_url("https://hub.example", "lua", Some("12a")).is_err());
        assert!(build_search_url("https://hub.example", "lua", Some("1234567890123")).is_err());
        assert!(build_search_url("file:///hub", "lua", None).is_err());

        let url = build_search_url("https://hub.example/", " lua & mcp ", Some("24"))
            .expect("bounded search url");
        assert_eq!(url.path(), "/api/v1/skills/search");
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        assert!(pairs.contains(&(String::from("q"), String::from("lua & mcp"))));
        assert!(pairs.contains(&(String::from("cursor"), String::from("24"))));
        assert!(pairs.contains(&(String::from("limit"), HUB_SEARCH_PAGE_SIZE.to_string())));
    }

    /// Verifies a Hub page is projected with its next cursor and an empty cursor ends paging.
    /// 验证 Hub 页面连同下一页游标被投影，且空游标结束分页。
    #[test]
    fn search_projects_hub_pages_and_paging() {
        let body = serde_json::json!({
            "items": [{
                "skill_id": "weather",
                "publisher": "OpenVulcan",
                "display_name": "Weather",
                "summary": "Forecasts",
                "repository_url": "",
                "homepage_url": "",
                "license": "MIT",
                "tags": ["web"],
                "latest_version": "1.2.0",
                "created_at": "2026-10-01T00:00:00Z",
                "updated_at": "2026-10-02T00:00:00Z"
            }],
            "next_cursor": "24"
        })
        .to_string();
        let (base_url, server) = serve_once("200 OK", body);
        let page = search_hub(&base_url, "weather", None).expect("search page");
        let request = server.join().expect("server thread");
        assert!(request.starts_with("GET /api/v1/skills/search?"));
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].skill_id, "weather");
        assert_eq!(page.items[0].latest_version, "1.2.0");
        assert_eq!(page.next_cursor.as_deref(), Some("24"));

        let (base_url, server) =
            serve_once("200 OK", String::from(r#"{"items":[],"next_cursor":""}"#));
        let page = search_hub(&base_url, "", Some("24")).expect("last page");
        server.join().expect("server thread");
        assert!(page.items.is_empty());
        assert_eq!(page.next_cursor, None);
    }

    /// Verifies Hub failures surface the structured error code.
    /// 验证 Hub 失败会透出结构化错误码。
    #[test]
    fn search_surfaces_structured_hub_errors() {
        let (base_url, server) = serve_once(
            "400 Bad Request",
            String::from(
                r#"{"error":{"code":"invalid_cursor","message":"The cursor is invalid."}}"#,
            ),
        );
        let error = search_hub(&base_url, "", None).expect_err("hub failure");
        server.join().expect("server thread");
        assert!(error.contains("invalid_cursor"));
        assert!(error.contains("400"));
    }
}
