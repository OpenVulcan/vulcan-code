//! Shared service API types for Vulcan clients.
//! Vulcan 客户端共享的服务 API 类型。

use std::{
    fs,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL_SAFE_NO_PAD;
use http_range::HttpRange;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Maximum bytes returned for one bounded media range response.
/// 单个有界媒体 Range 响应返回的最大字节数。
pub const SERVICE_MEDIA_RANGE_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// Redacted secret placeholder returned through browser-facing HTTP snapshots.
/// 通过浏览器侧 HTTP 快照返回的敏感信息脱敏占位符。
pub const WEBUI_REDACTED_SECRET_PLACEHOLDER: &str = "********";

/// Default local port used by the browser-facing WebUI service.
/// 浏览器侧 WebUI 服务使用的默认本地端口。
pub const WEBUI_SERVICE_HTTP_PORT: u16 = 17891;

/// Last local port tried by automatic browser-facing service discovery.
/// 浏览器侧服务自动发现尝试的最后一个本地端口。
pub const WEBUI_SERVICE_HTTP_PORT_END: u16 = 17920;

/// Environment variable name used to override the browser-facing service port.
/// 用于覆盖浏览器侧服务端口的环境变量名。
pub const WEBUI_SERVICE_PORT_ENV: &str = "VULCAN_WEBUI_SERVICE_PORT";

/// Loopback host used by the browser-facing WebUI service.
/// 浏览器侧 WebUI 服务使用的回环主机。
pub const WEBUI_SERVICE_LOOPBACK_HOST: &str = "127.0.0.1";

/// Environment variable name used to override the browser-facing WebUI static directory.
/// 用于覆盖浏览器侧 WebUI 静态目录的环境变量名。
pub const WEBUI_STATIC_DIR_ENV: &str = "VULCAN_WEBUI_STATIC_DIR";

mod attachments;
mod command_events;
mod conversation;
mod conversation_hydration;
mod git_settings;
mod host_tools;
mod inference;
mod knowledge_base;
mod media;
mod planning_state;
mod provider_retry;
mod responses;
mod settings;
mod skills_catalog;
mod startup;
mod subagents;
mod vmm;
mod webui_rpc;
mod webui_service;
mod webui_static;
mod workbench;
mod workspace_context;

pub use attachments::*;
pub use command_events::*;
pub use conversation::*;
pub use conversation_hydration::*;
pub use git_settings::*;
pub use host_tools::*;
pub use inference::*;
pub use knowledge_base::*;
pub use media::*;
pub use planning_state::*;
pub use provider_retry::*;
pub use responses::*;
pub use settings::*;
pub use skills_catalog::*;
pub use startup::*;
pub use subagents::*;
pub use vmm::*;
pub use webui_rpc::*;
pub use webui_service::*;
pub use webui_static::*;
pub use workbench::*;
pub use workspace_context::*;

#[cfg(test)]
mod tests {
    use super::{
        VmmMaintenancePhase, VmmMaintenanceSummary, VmmMemoryActionResponse,
        WEBUI_REDACTED_SECRET_PLACEHOLDER, WEBUI_SERVICE_HTTP_PORT, WEBUI_SERVICE_HTTP_PORT_END,
        WEBUI_SERVICE_LOOPBACK_HOST, WEBUI_STATIC_DIR_ENV, build_webui_static_response,
        redact_webui_service_rpc_secrets, webui_service_discovery_snapshot_path,
        webui_service_http_bind_ports_from_override, webui_service_http_default_bind_ports,
        webui_service_origin_text_is_allowed, webui_service_ready_discovery_snapshot,
        webui_service_request_token_values_are_valid,
        webui_service_required_origin_text_is_allowed,
        webui_service_rpc_secret_key_should_be_redacted, webui_service_token_from_query,
        webui_static_file_path_for_request, webui_static_mime_type, webui_static_relative_path,
        webui_static_root_from_candidates,
    };
    use serde_json::{Value, json};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Verifies browser-facing RPC payloads redact nested secret fields.
    /// 验证浏览器侧 RPC 载荷会递归脱敏嵌套敏感字段。
    #[test]
    fn browser_rpc_redacts_nested_secret_fields() {
        let mut payload = json!({
            "aiCapabilityVendors": [
                {
                    "vendorId": "minimax",
                    "apiKey": "real-secret",
                    "targetChoices": [
                        {
                            "label": "safe"
                        }
                    ]
                }
            ],
            "vmm": {
                "grpcApiKey": "grpc-secret",
                "grpcTarget": "http://127.0.0.1"
            },
            "empty": {
                "api_key": ""
            }
        });

        redact_webui_service_rpc_secrets(&mut payload);

        assert_eq!(
            payload
                .pointer("/aiCapabilityVendors/0/apiKey")
                .and_then(Value::as_str),
            Some(WEBUI_REDACTED_SECRET_PLACEHOLDER)
        );
        assert_eq!(
            payload.pointer("/vmm/grpcApiKey").and_then(Value::as_str),
            Some(WEBUI_REDACTED_SECRET_PLACEHOLDER)
        );
        assert_eq!(
            payload.pointer("/empty/api_key").and_then(Value::as_str),
            Some("")
        );
    }

    /// Verifies the browser-facing secret key matcher covers current settings fields.
    /// 验证浏览器侧敏感字段匹配器覆盖当前设置字段。
    #[test]
    fn browser_rpc_secret_key_matcher_covers_settings_fields() {
        for key in ["apiKey", "api_key", "grpcApiKey", "grpc_api_key"] {
            assert!(webui_service_rpc_secret_key_should_be_redacted(key));
        }
        assert!(!webui_service_rpc_secret_key_should_be_redacted("baseUrl"));
        assert!(!webui_service_rpc_secret_key_should_be_redacted(
            "targetChoices"
        ));
    }

    /// Verifies typed VMM action responses serialize to the tagged WebUI contract.
    /// 验证强类型 VMM 动作响应会序列化为带标签的 WebUI 契约。
    #[test]
    fn vmm_action_response_serializes_tagged_maintenance_data() {
        let response = VmmMemoryActionResponse::Maintenance(VmmMaintenanceSummary {
            phase: VmmMaintenancePhase::Rebuilding,
            progress_percent: 50,
            message: String::from("rebuilding"),
            ..VmmMaintenanceSummary::default()
        });

        let value = serde_json::to_value(response).expect("serialize typed VMM response");

        assert_eq!(value["kind"], "maintenance");
        assert_eq!(value["value"]["phase"], "rebuilding");
        assert_eq!(value["value"]["progressPercent"], 50);
    }

    /// Verifies WebUI static route paths cannot traverse outside the bundle directory.
    /// 验证 WebUI 静态路由路径不能穿越到包目录外。
    #[test]
    fn webui_static_relative_path_rejects_traversal() {
        assert!(webui_static_relative_path("/../secret.txt").is_err());
        assert!(webui_static_relative_path("/assets/..\\secret.txt").is_err());
        assert!(webui_static_relative_path("/C:/Windows/system.ini").is_err());
    }

    /// Verifies WebUI static route paths normalize common safe asset paths.
    /// 验证 WebUI 静态路由路径会规范化常见安全资源路径。
    #[test]
    fn webui_static_relative_path_accepts_safe_assets() {
        assert_eq!(
            webui_static_relative_path("/").expect("root request should resolve to index.html"),
            PathBuf::from("index.html")
        );
        assert_eq!(
            webui_static_relative_path("/assets/app.js")
                .expect("safe asset request should resolve under assets"),
            PathBuf::from("assets").join("app.js")
        );
    }

    /// Verifies WebUI static assets get browser-safe MIME types.
    /// 验证 WebUI 静态资源会得到浏览器安全 MIME 类型。
    #[test]
    fn webui_static_mime_type_covers_vite_assets() {
        assert_eq!(
            webui_static_mime_type(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            webui_static_mime_type(Path::new("assets/app.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            webui_static_mime_type(Path::new("assets/app.css")),
            "text/css; charset=utf-8"
        );
    }

    /// Verifies WebUI static file resolution serves direct files and SPA fallback.
    /// 验证 WebUI 静态文件解析会提供直接文件和 SPA 回退。
    #[test]
    fn webui_static_file_path_resolves_direct_files_and_spa_fallback() {
        let root = temporary_service_api_test_directory("webui-static-path");
        fs::create_dir_all(root.join("assets")).expect("create assets directory");
        fs::write(root.join("index.html"), "<html></html>").expect("write index");
        fs::write(root.join("assets").join("app.js"), "console.log(1);").expect("write js");

        assert_eq!(
            webui_static_file_path_for_request(&root, "/assets/app.js")
                .expect("direct static asset request should resolve"),
            Some(root.join("assets").join("app.js"))
        );
        assert_eq!(
            webui_static_file_path_for_request(&root, "/conversation/abc")
                .expect("SPA route request should resolve through index fallback"),
            Some(root.join("index.html"))
        );
        assert_eq!(
            webui_static_file_path_for_request(&root, "/assets/missing.js")
                .expect("missing static asset request should be safely classified"),
            None
        );

        fs::remove_dir_all(root).expect("remove temp directory");
    }

    /// Verifies WebUI static response builder returns framework-neutral HTTP data.
    /// 验证 WebUI 静态响应构建器会返回框架无关 HTTP 数据。
    #[test]
    fn webui_static_response_builder_returns_neutral_http_data() {
        let root = temporary_service_api_test_directory("webui-static-response");
        fs::create_dir_all(&root).expect("create static root");
        fs::write(root.join("index.html"), "<html>ok</html>").expect("write index");

        let response = build_webui_static_response(
            Some(&root),
            "/",
            "no-cache",
            "Vulcan WebUI static bundle is not available.",
        );
        assert_eq!(response.status, 200);
        assert!(response.body.starts_with(b"<html>"));
        assert!(
            response
                .headers
                .iter()
                .any(|(key, value)| key == "cache-control" && value == "no-cache")
        );

        let missing = build_webui_static_response(None, "/", "no-cache", "missing");
        assert_eq!(missing.status, 404);
        assert_eq!(
            String::from_utf8(missing.body).expect("missing response body should be UTF-8 text"),
            "missing"
        );

        fs::remove_dir_all(root).expect("remove temp directory");
    }

    /// Verifies WebUI static root resolution prefers a valid override directory.
    /// 验证 WebUI 静态根目录解析会优先使用有效覆盖目录。
    #[test]
    fn webui_static_root_prefers_valid_override_directory() {
        let override_root = temporary_service_api_test_directory("webui-static-override");
        let fallback_root = temporary_service_api_test_directory("webui-static-fallback");
        fs::create_dir_all(&override_root).expect("create override root");
        fs::create_dir_all(&fallback_root).expect("create fallback root");

        assert_eq!(
            webui_static_root_from_candidates(override_root.to_str(), Some(fallback_root.clone())),
            Some(override_root.clone())
        );
        assert_eq!(WEBUI_STATIC_DIR_ENV, "VULCAN_WEBUI_STATIC_DIR");

        fs::remove_dir_all(override_root).expect("remove override root");
        fs::remove_dir_all(fallback_root).expect("remove fallback root");
    }

    /// Verifies WebUI static root resolution falls back when override is missing.
    /// 验证 WebUI 静态根目录解析会在覆盖目录缺失时回退。
    #[test]
    fn webui_static_root_falls_back_when_override_is_missing() {
        let fallback_root = temporary_service_api_test_directory("webui-static-fallback-only");
        fs::create_dir_all(&fallback_root).expect("create fallback root");

        assert_eq!(
            webui_static_root_from_candidates(Some("  "), Some(fallback_root.clone())),
            Some(fallback_root.clone())
        );
        assert_eq!(
            webui_static_root_from_candidates(Some("Z:/definitely/not/here"), None),
            None
        );

        fs::remove_dir_all(fallback_root).expect("remove fallback root");
    }

    /// Verifies browser-facing CORS only allows loopback HTTP and trusted Tauri origins.
    /// 验证浏览器侧 CORS 仅允许回环 HTTP 与受信任 Tauri 来源。
    #[test]
    fn webui_service_origin_allows_only_loopback_http_and_tauri_origins() {
        for origin in [
            "http://localhost:17601",
            "https://localhost:443",
            "http://127.0.0.1:17601",
            "http://[::1]:17601",
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
        ] {
            assert!(webui_service_origin_text_is_allowed(origin));
        }

        for origin in [
            "http://example.com",
            "https://evil.localhost.example",
            "file://localhost",
            "tauri://example.com",
            "http://evil.tauri.localhost",
            "http://not-tauri.localhost",
        ] {
            assert!(!webui_service_origin_text_is_allowed(origin));
        }
    }

    /// Verifies browser WebSocket origins must be present before they are accepted.
    /// 验证浏览器 WebSocket 来源必须存在才会被接受。
    #[test]
    fn webui_service_required_origin_rejects_missing_origin() {
        assert!(webui_service_required_origin_text_is_allowed(Some(
            "http://127.0.0.1:17601"
        )));
        assert!(!webui_service_required_origin_text_is_allowed(None));
        assert!(!webui_service_required_origin_text_is_allowed(Some(
            "http://example.com"
        )));
    }

    /// Verifies browser service token validation accepts header and query tokens.
    /// 验证浏览器服务令牌校验接受请求头与查询参数中的令牌。
    #[test]
    fn webui_service_token_validation_accepts_header_and_query() {
        let expected = "abc 123";

        assert!(webui_service_request_token_values_are_valid(
            Some("abc 123"),
            None,
            expected
        ));
        assert!(webui_service_request_token_values_are_valid(
            None,
            Some("token=abc%20123"),
            expected
        ));
        assert!(webui_service_request_token_values_are_valid(
            None,
            Some("serviceToken=abc+123"),
            expected
        ));
        assert!(!webui_service_request_token_values_are_valid(
            Some("wrong"),
            Some("token=wrong"),
            expected
        ));
    }

    /// Verifies browser service token query parser decodes supported parameter names.
    /// 验证浏览器服务令牌查询解析器会解码受支持的参数名。
    #[test]
    fn webui_service_token_query_parser_decodes_values() {
        assert_eq!(
            webui_service_token_from_query("token=abc%20123&x=1"),
            Some(String::from("abc 123"))
        );
        assert_eq!(
            webui_service_token_from_query("x=1&serviceToken=abc%2B123"),
            Some(String::from("abc+123"))
        );
        assert_eq!(webui_service_token_from_query("x=1"), None);
    }

    /// Verifies default browser service port discovery keeps the documented range.
    /// 验证默认浏览器服务端口发现保持文档约定范围。
    #[test]
    fn webui_service_default_bind_ports_cover_discovery_range() {
        let ports = webui_service_http_default_bind_ports();

        assert_eq!(ports.first(), Some(&WEBUI_SERVICE_HTTP_PORT));
        assert_eq!(ports.last(), Some(&WEBUI_SERVICE_HTTP_PORT_END));
        assert_eq!(
            ports.len(),
            usize::from(WEBUI_SERVICE_HTTP_PORT_END - WEBUI_SERVICE_HTTP_PORT + 1)
        );
    }

    /// Verifies browser service port override accepts only valid nonzero ports.
    /// 验证浏览器服务端口覆盖只接受有效的非零端口。
    #[test]
    fn webui_service_bind_ports_accept_valid_override_only() {
        assert_eq!(
            webui_service_http_bind_ports_from_override(Some(" 19000 ")),
            vec![19000]
        );
        assert_eq!(
            webui_service_http_bind_ports_from_override(Some("0")),
            webui_service_http_default_bind_ports()
        );
        assert_eq!(
            webui_service_http_bind_ports_from_override(Some("not-a-port")),
            webui_service_http_default_bind_ports()
        );
    }

    /// Verifies browser service discovery path prefers home and falls back to temp.
    /// 验证浏览器服务发现路径优先使用主目录并在缺失时回退临时目录。
    #[test]
    fn webui_service_discovery_path_uses_home_or_temp_root() {
        let home = PathBuf::from("C:/Users/example");
        let temp = PathBuf::from("C:/Temp");

        assert_eq!(
            webui_service_discovery_snapshot_path(Some(home.clone()), temp.clone()),
            home.join(".vulcan").join("code").join("webui-service.json")
        );
        assert_eq!(
            webui_service_discovery_snapshot_path(None, temp.clone()),
            temp.join(".vulcan").join("code").join("webui-service.json")
        );
    }

    /// Verifies browser service discovery snapshots keep URL and host fields consistent.
    /// 验证浏览器服务发现快照保持 URL 与主机字段一致。
    #[test]
    fn webui_service_ready_discovery_snapshot_builds_consistent_url() {
        let snapshot =
            webui_service_ready_discovery_snapshot(17891, 42, "123456789", "token-value");

        assert_eq!(snapshot.status, "ready");
        assert_eq!(snapshot.host, WEBUI_SERVICE_LOOPBACK_HOST);
        assert_eq!(snapshot.base_url, "http://127.0.0.1:17891");
        assert_eq!(snapshot.port, 17891);
        assert_eq!(snapshot.pid, 42);
        assert_eq!(snapshot.started_at, "123456789");
        assert_eq!(snapshot.service_token, "token-value");
    }

    /// Verifies browser discovery debug formatting never exposes the service token.
    /// 验证浏览器发现快照的调试格式绝不暴露服务令牌。
    #[test]
    fn webui_service_discovery_debug_redacts_service_token() {
        let snapshot =
            webui_service_ready_discovery_snapshot(17891, 42, "123456789", "token-value");
        let debug = format!("{snapshot:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(snapshot.service_token.as_str()));
    }

    /// Creates a unique temporary directory path for service-api tests.
    /// 为 service-api 测试创建唯一临时目录路径。
    fn temporary_service_api_test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("vulcan-service-api-{label}-{unique}"))
    }
}
