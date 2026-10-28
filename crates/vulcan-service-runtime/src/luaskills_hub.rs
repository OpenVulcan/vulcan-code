//! Read-only LuaSkills Hub catalog search used by the skill install surfaces.
//! 技能安装界面使用的只读 LuaSkills Hub 目录搜索。
//!
//! Installation itself stays inside the LuaSkills engine through the `official_hub` source
//! type; this module only lists what the configured Hub can resolve, against the same Hub
//! base URL the running engine currently uses.
//! 安装本身仍通过 `official_hub` 来源类型留在 LuaSkills 引擎内；本模块只针对运行中引擎当前
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

/// Largest accepted skill detail response body; a translated README can exceed the original.
/// 可接受的最大技能详情响应体；翻译后的 README 可能比原文更长。
const HUB_DETAIL_RESPONSE_MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Longest accepted skill identifier.
/// 可接受的最长技能标识。
const HUB_SKILL_ID_MAX_CHARS: usize = 64;

/// Longest accepted content language code; the Hub resolves the exact code and rejects unknown ones.
/// 可接受的最长内容语言代码；由 Hub 解析准确代码并拒绝未知代码。
const HUB_LOCALE_MAX_CHARS: usize = 35;

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
    /// Refined content in the requested language; absent without a language or before refinement finishes.
    /// 所请求语言的精炼内容；未指定语言或精炼完成前缺省。
    #[serde(default)]
    pub content: Option<LuaSkillsHubContent>,
}

/// Refined content of a skill's latest version in one language.
/// 技能最新版本在某一语言下的精炼内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct LuaSkillsHubContent {
    /// Language the client asked for.
    /// 客户端请求的语言。
    #[serde(default)]
    pub requested_locale: String,
    /// Language of this content.
    /// 本内容的语言。
    #[serde(default)]
    pub locale: String,
    /// `original`, `translation`, or `original_fallback` while no translation exists.
    /// `original`、`translation`，或尚无译文时的 `original_fallback`。
    #[serde(default)]
    pub kind: String,
    /// Language the publisher wrote in, `other` outside the Hub's languages.
    /// 发布者使用的语言；不在 Hub 语言之内时为 `other`。
    #[serde(default)]
    pub source_locale: String,
    /// One-line summary in this language.
    /// 本语言的单行摘要。
    #[serde(default)]
    pub summary: String,
    /// AI-written introduction.
    /// AI 撰写的介绍。
    #[serde(default)]
    pub intro: String,
    /// AI-extracted feature points.
    /// AI 提取的功能要点。
    #[serde(default)]
    pub features: Vec<String>,
    /// Translated README, present only on skill detail for a translation.
    /// 翻译后的 README，仅在技能详情且为译文时出现。
    #[serde(default)]
    pub readme: Option<String>,
}

/// One skill's detail: the localized entry, its original content and the latest version's README.
/// 一个技能的详情：本地化条目、原文内容与最新版本的 README。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsHubSkillDetail {
    /// Hub base URL that answered this request.
    /// 响应本次请求的 Hub 基础地址。
    pub hub_base_url: String,
    /// Skill entry with content in the requested language.
    /// 带所请求语言内容的技能条目。
    pub skill: LuaSkillsHubSkill,
    /// Original-language content; absent without a language or before refinement finishes.
    /// 原文语言内容；未指定语言或精炼完成前缺省。
    pub original: Option<LuaSkillsHubContent>,
    /// Latest installable version.
    /// 最新可安装版本。
    pub latest: LuaSkillsHubVersion,
}

/// One published version as shown on skill detail.
/// 技能详情中显示的一个已发布版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct LuaSkillsHubVersion {
    /// Version string.
    /// 版本字符串。
    pub version: String,
    /// Original README of this version.
    /// 该版本的原文 README。
    #[serde(default)]
    pub readme: String,
    /// RFC 3339 publication time.
    /// RFC 3339 发布时间。
    #[serde(default)]
    pub published_at: String,
}

/// Wire shape of one Hub skill detail response.
/// 单个 Hub 技能详情响应的线协议形状。
#[derive(Debug, Deserialize)]
struct HubSkillResponse {
    /// Skill entry.
    /// 技能条目。
    skill: LuaSkillsHubSkill,
    /// Original-language content.
    /// 原文语言内容。
    #[serde(default)]
    original: Option<LuaSkillsHubContent>,
    /// Latest version detail.
    /// 最新版本详情。
    latest: LuaSkillsHubVersion,
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
    locale: Option<&str>,
) -> Result<LuaSkillsHubSearchPage, String> {
    let hub_base_url = crate::luaskills_bridge::live_network_settings(handle)?
        .effective_official_skill_hub_base_url();
    search_hub(&hub_base_url, query, cursor, locale)
}

/// Reads one skill's detail from the Hub the running LuaSkills engine installs from.
/// 从运行中 LuaSkills 引擎的安装来源 Hub 读取一个技能的详情。
///
/// # Errors
/// Returns an English error when the skill identifier or language is out of bounds, the Hub
/// base URL is invalid, or the Hub request fails.
/// 技能标识或语言越界、Hub 基础地址无效或 Hub 请求失败时返回英文错误。
pub fn get_official_hub_skill(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
    locale: Option<&str>,
) -> Result<LuaSkillsHubSkillDetail, String> {
    let hub_base_url = crate::luaskills_bridge::live_network_settings(handle)?
        .effective_official_skill_hub_base_url();
    get_hub_skill(&hub_base_url, skill_id, locale)
}

/// Searches one explicit Hub base URL.
/// 搜索一个显式指定的 Hub 基础地址。
fn search_hub(
    hub_base_url: &str,
    query: &str,
    cursor: Option<&str>,
    locale: Option<&str>,
) -> Result<LuaSkillsHubSearchPage, String> {
    let url = build_search_url(hub_base_url, query, cursor, locale)?;
    let body = fetch_hub(url, HUB_SEARCH_RESPONSE_MAX_BYTES, "search")?;
    let page: HubSearchResponse = serde_json::from_slice(&body)
        .map_err(|error| format!("LuaSkills Hub returned an invalid search response: {error}"))?;
    Ok(LuaSkillsHubSearchPage {
        hub_base_url: hub_base_url.trim_end_matches('/').to_string(),
        items: page.items,
        next_cursor: Some(page.next_cursor).filter(|cursor| !cursor.is_empty()),
    })
}

/// Reads one skill's detail from an explicit Hub base URL.
/// 从显式指定的 Hub 基础地址读取一个技能的详情。
fn get_hub_skill(
    hub_base_url: &str,
    skill_id: &str,
    locale: Option<&str>,
) -> Result<LuaSkillsHubSkillDetail, String> {
    let skill_id = skill_id.trim();
    if skill_id.is_empty() || skill_id.chars().count() > HUB_SKILL_ID_MAX_CHARS {
        return Err(format!(
            "LuaSkills Hub skill identifier must be 1 to {HUB_SKILL_ID_MAX_CHARS} characters."
        ));
    }
    let mut url = hub_api_url(hub_base_url, &["skills", skill_id])?;
    append_locale(&mut url, locale)?;
    let body = fetch_hub(url, HUB_DETAIL_RESPONSE_MAX_BYTES, "skill detail")?;
    let detail: HubSkillResponse = serde_json::from_slice(&body).map_err(|error| {
        format!("LuaSkills Hub returned an invalid skill detail response: {error}")
    })?;
    Ok(LuaSkillsHubSkillDetail {
        hub_base_url: hub_base_url.trim_end_matches('/').to_string(),
        skill: detail.skill,
        original: detail.original,
        latest: detail.latest,
    })
}

/// Sends one GET to the Hub and returns the successful body, at most `max_bytes` long.
/// 向 Hub 发送一次 GET 并返回成功的响应体，长度不超过 `max_bytes`。
fn fetch_hub(url: Url, max_bytes: u64, operation: &str) -> Result<Vec<u8>, String> {
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
        .map_err(|error| format!("LuaSkills Hub {operation} request failed: {error}"))?;
    let status = response.status();
    let body = read_bounded_body(response, max_bytes)?;
    if !status.is_success() {
        return Err(describe_hub_failure(operation, status, &body));
    }
    Ok(body)
}

/// Builds an absolute Hub API URL from encoded path segments below `/api/v1`.
/// 由 `/api/v1` 之下的已编码路径段构建绝对 Hub API 地址。
fn hub_api_url(hub_base_url: &str, segments: &[&str]) -> Result<Url, String> {
    let mut url = Url::parse(&format!("{}/api/v1", hub_base_url.trim_end_matches('/')))
        .map_err(|error| format!("LuaSkills Hub base URL is invalid: {error}"))?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(String::from(
            "LuaSkills Hub base URL must be an absolute http or https URL.",
        ));
    }
    url.path_segments_mut()
        .map_err(|()| String::from("LuaSkills Hub base URL cannot carry a path."))?
        .extend(segments);
    Ok(url)
}

/// Adds the optional content language; the Hub owns language matching and rejects unknown codes.
/// 追加可选的内容语言；语言匹配由 Hub 负责，未知代码由 Hub 拒绝。
fn append_locale(url: &mut Url, locale: Option<&str>) -> Result<(), String> {
    let Some(locale) = locale.map(str::trim).filter(|locale| !locale.is_empty()) else {
        return Ok(());
    };
    if locale.len() > HUB_LOCALE_MAX_CHARS
        || !locale
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(String::from("LuaSkills Hub content language is invalid."));
    }
    url.query_pairs_mut().append_pair("lang", locale);
    Ok(())
}

/// Builds the search endpoint URL after validating every caller-controlled member.
/// 校验每个调用方可控成员后构建搜索端点地址。
fn build_search_url(
    hub_base_url: &str,
    query: &str,
    cursor: Option<&str>,
    locale: Option<&str>,
) -> Result<Url, String> {
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
    let mut url = hub_api_url(hub_base_url, &["skills", "search"])?;
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
    append_locale(&mut url, locale)?;
    Ok(url)
}

/// Reads a response body, refusing anything above `max_bytes`.
/// 读取响应体，拒绝超过 `max_bytes` 的内容。
fn read_bounded_body(
    response: reqwest::blocking::Response,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    response
        .take(max_bytes + 1)
        .read_to_end(&mut body)
        .map_err(|error| format!("failed to read the LuaSkills Hub response: {error}"))?;
    if body.len() as u64 > max_bytes {
        return Err(format!("LuaSkills Hub response exceeds {max_bytes} bytes."));
    }
    Ok(body)
}

/// Renders one failed Hub response, preferring its structured error body.
/// 渲染一个失败的 Hub 响应，优先使用其结构化错误体。
fn describe_hub_failure(operation: &str, status: StatusCode, body: &[u8]) -> String {
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
            format!("LuaSkills Hub {operation} failed with HTTP {status} ({code}): {message}")
        }
        Err(_) => format!("LuaSkills Hub {operation} failed with HTTP {status}."),
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
        assert!(build_search_url("https://hub.example", &long_query, None, None).is_err());
        assert!(build_search_url("https://hub.example", "lua", Some("12a"), None).is_err());
        assert!(
            build_search_url("https://hub.example", "lua", Some("1234567890123"), None).is_err()
        );
        assert!(build_search_url("file:///hub", "lua", None, None).is_err());

        let url = build_search_url(
            "https://hub.example/",
            " lua & mcp ",
            Some("24"),
            Some("es-ES"),
        )
        .expect("bounded search url");
        assert_eq!(url.path(), "/api/v1/skills/search");
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        assert!(pairs.contains(&(String::from("q"), String::from("lua & mcp"))));
        assert!(pairs.contains(&(String::from("cursor"), String::from("24"))));
        assert!(pairs.contains(&(String::from("limit"), HUB_SEARCH_PAGE_SIZE.to_string())));
        assert!(pairs.contains(&(String::from("lang"), String::from("es-ES"))));
        assert!(build_search_url("https://hub.example", "", None, Some("ja JP")).is_err());
        assert!(
            build_search_url(
                "https://hub.example",
                "",
                None,
                Some(&"a".repeat(HUB_LOCALE_MAX_CHARS + 1))
            )
            .is_err()
        );
        let unlocalized =
            build_search_url("https://hub.example", "", None, Some(" ")).expect("blank language");
        assert!(!unlocalized.query_pairs().any(|(name, _)| name == "lang"));
    }

    /// Verifies skill detail encodes the identifier as one path segment and projects both content nodes.
    /// 验证技能详情将标识编码为单个路径段，并投影两个内容节点。
    #[test]
    fn skill_detail_projects_localized_and_original_content() {
        assert!(get_hub_skill("https://hub.example", " ", None).is_err());
        assert!(
            get_hub_skill(
                "https://hub.example",
                &"a".repeat(HUB_SKILL_ID_MAX_CHARS + 1),
                None
            )
            .is_err()
        );
        let content = |kind: &str, summary: &str, readme: Option<&str>| {
            serde_json::json!({
                "requested_locale": "es-ES",
                "locale": if kind == "translation" { "es-ES" } else { "en-US" },
                "kind": kind,
                "source_locale": "en-US",
                "summary": summary,
                "intro": "Intro",
                "features": ["One", "Two", "Three"],
                "readme": readme,
            })
        };
        let body = serde_json::json!({
            "skill": {
                "skill_id": "weather",
                "display_name": "Weather",
                "summary": "Forecasts",
                "latest_version": "1.2.0",
                "content": content("translation", "Pronósticos", Some("# Tiempo")),
            },
            "original": content("original", "Forecasts", None),
            "latest": {"version": "1.2.0", "readme": "# Weather", "published_at": "2026-10-02T00:00:00Z", "archive_sha256": "00"},
            "publisher_profile": null,
        })
        .to_string();
        let (base_url, server) = serve_once("200 OK", body);
        let detail = get_hub_skill(&base_url, "weather/../x", Some("es-ES")).expect("skill detail");
        let request = server.join().expect("server thread");
        assert!(
            request.starts_with("GET /api/v1/skills/weather%2F..%2Fx?lang=es-ES "),
            "{request}"
        );
        let localized = detail.skill.content.as_ref().expect("localized content");
        assert_eq!(localized.kind, "translation");
        assert_eq!(localized.readme.as_deref(), Some("# Tiempo"));
        assert_eq!(
            detail
                .original
                .as_ref()
                .map(|original| original.summary.as_str()),
            Some("Forecasts")
        );
        assert_eq!(detail.latest.readme, "# Weather");

        let unlocalized = serde_json::json!({
            "skill": {"skill_id": "weather"},
            "original": null,
            "latest": {"version": "1.2.0", "readme": "# Weather"},
        })
        .to_string();
        let (base_url, server) = serve_once("200 OK", unlocalized);
        let detail = get_hub_skill(&base_url, "weather", None).expect("unlocalized detail");
        let request = server.join().expect("server thread");
        assert!(
            request.starts_with("GET /api/v1/skills/weather "),
            "{request}"
        );
        assert_eq!(detail.skill.content, None);
        assert_eq!(detail.original, None);
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
        let page = search_hub(&base_url, "weather", None, None).expect("search page");
        let request = server.join().expect("server thread");
        assert!(request.starts_with("GET /api/v1/skills/search?"));
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].skill_id, "weather");
        assert_eq!(page.items[0].latest_version, "1.2.0");
        assert_eq!(page.next_cursor.as_deref(), Some("24"));

        let (base_url, server) =
            serve_once("200 OK", String::from(r#"{"items":[],"next_cursor":""}"#));
        let page = search_hub(&base_url, "", Some("24"), None).expect("last page");
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
        let error = search_hub(&base_url, "", None, None).expect_err("hub failure");
        server.join().expect("server thread");
        assert!(error.contains("invalid_cursor"));
        assert!(error.contains("400"));
    }
}
