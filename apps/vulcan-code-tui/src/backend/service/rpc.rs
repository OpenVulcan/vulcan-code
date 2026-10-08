// Blocking JSON-RPC client for `POST {baseUrl}/rpc`.
// `POST {baseUrl}/rpc` 的阻塞式 JSON-RPC 客户端。
//
// Envelope: request `{method, params}` with the `x-vulcan-service-token` header; response
// `{ok, result?, error?: {code, message}}`. Calls run on backend threads only, never on the UI
// thread.
// 信封：请求 `{method, params}` 并携带 `x-vulcan-service-token` 头；响应
// `{ok, result?, error?: {code, message}}`。调用只在后端线程执行，绝不在界面线程上执行。

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use super::discovery::Endpoint;
use crate::service_api::scrub_secrets;

/// Finds strings removed by the same DTO sanitizer; parameters are inspected only in memory.
/// 找出同一 DTO 清理器移除的字符串；仅在内存中检查参数。
fn removed_strings(original: &Value, sanitized: &Value, secrets: &mut Vec<String>) {
    match (original, sanitized) {
        (Value::String(before), Value::String(after)) if before != after && !before.is_empty() => {
            secrets.push(before.clone());
        }
        (Value::Object(before), Value::Object(after)) => {
            for (key, value) in before {
                if let Some(sanitized) = after.get(key) {
                    removed_strings(value, sanitized, secrets);
                }
            }
        }
        (Value::Array(before), Value::Array(after)) => {
            for (before, after) in before.iter().zip(after) {
                removed_strings(before, after, secrets);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::RpcServer;
    use super::*;

    /// Parse failures keep the expectation and position but never the offending value.
    /// 解析失败保留期望与位置，但绝不包含出错的取值。
    #[test]
    fn parse_failures_never_echo_values() {
        #[derive(Debug, serde::Deserialize)]
        #[expect(dead_code, reason = "the fields only shape the expected input")]
        struct Probe {
            count: u64,
            name: String,
        }
        let secret = "synthetic-secret-value";
        for (input, kept) in [
            (
                format!(r#"{{"count":"{secret}","name":"a"}}"#),
                "expected u64",
            ),
            (r#"{"count":1}"#.to_string(), "missing field `name`"),
        ] {
            let error = serde_json::from_str::<Probe>(&input).unwrap_err();
            let described = parse_failure(&error);
            assert!(!described.contains(secret), "value echoed: {described}");
            assert!(described.contains(kept), "expectation lost: {described}");
        }
        let error =
            serde_json::from_value::<Probe>(serde_json::json!({ "count": secret })).unwrap_err();
        assert!(!parse_failure(&error).contains(secret));
    }

    /// Catalog-backed failures render in the active locale and unknown keys keep the service text.
    /// 目录形式的失败按当前语言渲染，未知键保留服务文本。
    #[test]
    fn localized_service_errors_render_from_the_catalog() {
        let _locale = crate::i18n::lock_locale("en-US");
        let server = RpcServer::start(2, |request| {
            let key = if request.body["method"] == "known_test" {
                "serviceError.gitCommit.branchExists"
            } else {
                "serviceError.notInThisBuild"
            };
            json!({"ok":false,"error":{"code":"service_rpc_error","message":"service default text",
                "localized":{"key":key,"params":{"branch":"feature/x"}}}})
        });
        let known = server.client.call("known_test", Value::Null).unwrap_err();
        assert_eq!(
            Some(known.message),
            crate::i18n::format_known(
                "serviceError.gitCommit.branchExists",
                &[("branch", "feature/x")]
            )
        );
        let unknown = server.client.call("unknown_test", Value::Null).unwrap_err();
        assert_eq!(unknown.message, "service default text");
    }

    /// Checks the production POST envelope and removes credentials echoed by a service error.
    /// 检查实际 POST 信封，并移除服务错误回显的凭据。
    #[test]
    fn rpc_envelope_authentication_and_error_redaction() {
        // Server.
        // 当前服务器或本地合成监听器。
        let server = RpcServer::start(2, |request| {
            if request.body["method"] == "read_test" {
                json!({"ok":true,"result":{"loaded":true}})
            } else {
                json!({"ok":false,"error":{"code":"test","message":
                    "Rejected synthetic-customer-secret, synthetic-header-secret and synthetic-test-token"}})
            }
        });
        assert_eq!(
            server.client.call("read_test", Value::Null).unwrap(),
            json!({"loaded":true})
        );
        // Error.
        // 当前错误。
        let error = server
            .client
            .call(
                "write_test",
                json!({"request":{"secret":"synthetic-customer-secret",
            "headers":[{"key":"X-Custom","value":"synthetic-header-secret"}]}}),
            )
            .unwrap_err();
        assert!(!error.transport);
        assert!(!error.message.contains("synthetic-"));
        assert!(error.message.contains("[redacted]"));
        // First.
        // 第一个观察值。
        let first = server
            .requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(first.request_line, "POST /rpc HTTP/1.1");
        assert_eq!(first.body, json!({"method":"read_test","params":null}));
        assert!(
            first
                .headers
                .get(TOKEN_HEADER)
                .is_some_and(|value| value == "synthetic-test-token")
        );
        // Second.
        // 第二个观察值。
        let second = server
            .requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(
            second.body["params"]["request"]["secret"],
            "synthetic-customer-secret"
        );
        server.thread.join().unwrap();
    }
}

/// Removes input secrets and the transport token from a service or transport error.
/// 从服务或传输错误中移除输入密钥及传输令牌。
/// `message` is the displayed or sanitized error text.
/// `message` 为显示文案或脱敏错误文本。
/// Returns `String`.
/// 返回 `String`。
fn redacted_message(message: String, secrets: &[String]) -> String {
    // Message.
    // 经过脱敏的错误文案。
    let mut message = message;
    for secret in secrets {
        message = message.replace(secret, "[redacted]");
        // Errors may quote a JSON-escaped credential containing quotes or line breaks.
        // 错误可能引用含引号或换行的 JSON 转义凭据。
        let escaped = serde_json::to_string(secret).expect("JSON string");
        message = message.replace(&escaped[1..escaped.len() - 1], "[redacted]");
    }
    message
}

/// Describes why service data did not match its declared type, without echoing any value.
/// 描述服务数据为何不符合声明类型，不回显任何取值。
///
/// Serde messages can quote the offending value ("invalid type: string \"…\""), which may be a
/// credential; only the expectation, missing field names and the position are kept.
/// Serde 消息可能引用出错的取值（"invalid type: string \"…\""），而该值可能是凭据；因此只保留
/// 期望类型、缺失字段名与位置。
/// `error` is the parse failure.
/// `error` 为解析失败。
/// Returns `String`.
/// 返回 `String`。
pub fn parse_failure(error: &serde_json::Error) -> String {
    let message = error.to_string();
    let reason = if let Some((_, expected)) = message.split_once(", expected ") {
        // The expectation comes from the type, never from the data.
        // 期望来自类型，而非数据。
        let expected = expected.split(" at line ").next().unwrap_or(expected);
        format!("expected {expected}")
    } else if message.starts_with("missing field ") {
        message
            .split(" at line ")
            .next()
            .unwrap_or_default()
            .to_string()
    } else {
        match error.classify() {
            serde_json::error::Category::Io => "unreadable input",
            serde_json::error::Category::Syntax => "malformed JSON",
            serde_json::error::Category::Data => "unexpected data",
            serde_json::error::Category::Eof => "truncated JSON",
        }
        .to_string()
    };
    match error.line() {
        0 => reason,
        line => format!("{reason} (line {line}, column {})", error.column()),
    }
}

/// Header carrying the service token.
/// 携带服务令牌的请求头。
const TOKEN_HEADER: &str = "x-vulcan-service-token";
/// Largest response accepted; a full workbench snapshot runs to several MiB.
/// 接受的最大响应；完整工作台快照可达数 MiB。
const RESPONSE_LIMIT: u64 = 256 * 1024 * 1024;
/// Connecting to the loopback service either succeeds at once or not at all.
/// 连接本机回环服务要么立即成功，要么失败。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Whole-call limit. Model-backed calls (commit message generation, pushes, imports) can take
/// minutes, so the limit only guards against a hung service.
/// 整次调用的上限。依赖模型的调用（提交信息生成、推送、导入）可能耗时数分钟，因此该上限只用于
/// 防止服务挂起。
const CALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// A failed call: the service's own error, or a transport failure.
/// 失败的调用：服务自身的错误，或传输失败。
#[derive(Debug, Clone)]
pub struct RpcError {
    /// Displayable service or transport failure after credential redaction.
    /// 凭据脱敏后可显示的服务或传输失败消息。
    pub message: String,
    /// The service could not be reached at all; the connection is presumed lost.
    /// 完全无法访问服务；视为连接已断开。
    pub transport: bool,
}

impl fmt::Display for RpcError {
    /// Writes the type display or diagnostic representation into the provided formatter.
    /// 向传入格式化器写入类型显示或诊断表示。
    /// `f` is the destination formatter.
    /// `f` 为目标格式化器。
    /// Returns `fmt::Result`.
    /// 返回 `fmt::Result`。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Cheap to clone; clones share the connection pool.
/// 克隆开销低；克隆体共享连接池。
#[derive(Clone)]
pub struct RpcClient {
    /// Shared bounded HTTP connection pool with proxy discovery disabled.
    /// 禁用代理发现的共享带超时 HTTP 连接池。
    agent: ureq::Agent,
    /// Discovered service origin and transport-only authentication state.
    /// 已发现的服务来源及仅供传输使用的认证状态。
    endpoint: Arc<Endpoint>,
}

impl RpcClient {
    /// Constructs the declared state or transport from its explicit initialization inputs.
    /// 根据明确初始化输入构造声明的状态或传输。
    /// Returns `Self`.
    /// 返回 `Self`。
    pub fn new(endpoint: Endpoint) -> Self {
        // Agent.
        // 本机 RPC 连接池。
        let agent: ureq::Agent = ureq::Agent::config_builder()
            // Loopback traffic must not go through a proxy from the environment.
            // 回环流量不得经过环境变量中的代理。
            .proxy(None)
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(CALL_TIMEOUT))
            .build()
            .into();
        RpcClient {
            agent,
            endpoint: Arc::new(endpoint),
        }
    }

    /// Borrows the public transport base URL without exposing authentication.
    /// 借用公开传输基础 URL，不公开认证信息。
    /// Returns `&str`.
    /// 返回 `&str`。
    pub fn base_url(&self) -> &str {
        &self.endpoint.base_url
    }

    /// Borrows the authenticated endpoint only for backend transport setup.
    /// 仅为后端传输初始化借用已认证端点。
    /// Returns `&Endpoint`.
    /// 返回 `&Endpoint`。
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Calls one method; `Value::Null` params are sent as `null`.
    /// 调用一个方法；`Value::Null` 参数以 `null` 发送。
    /// `method` is the static WebUI RPC method name.
    /// `method` 为静态 WebUI RPC 方法名。
    /// `params` is the exact RPC parameter document.
    /// `params` 为准确 RPC 参数文档。
    /// Returns `Result<Value, RpcError>`.
    /// 返回 `Result<Value, RpcError>`。
    pub fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        // Sanitized.
        // 清理机密后的副本。
        let mut sanitized = params.clone();
        scrub_secrets(&mut sanitized);
        // Known secret values to redact.
        // 需要移除的已知机密值。
        let mut secrets = vec![self.endpoint.token().to_string()];
        removed_strings(&params, &sanitized, &mut secrets);
        secrets.retain(|secret| !secret.is_empty());
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        secrets.dedup();
        // Body.
        // 准确消息正文。
        let body = json!({ "method": method, "params": params }).to_string();
        // Url.
        // 传输使用的准确地址。
        let url = format!("{}/rpc", self.endpoint.base_url);
        // Transport.
        // 是否为传输失败。
        let transport = |error: ureq::Error| RpcError {
            message: redacted_message(format!("{method}: {error}"), &secrets),
            transport: true,
        };
        // Response.
        // 响应信封。
        let mut response = self
            .agent
            .post(&url)
            .header(TOKEN_HEADER, self.endpoint.token())
            .header("content-type", "application/json")
            .send(body)
            .map_err(transport)?;
        // Status.
        // 已声明生命周期状态。
        let status = response.status();
        // Text.
        // 当前正文。
        let text = response
            .body_mut()
            .with_config()
            .limit(RESPONSE_LIMIT)
            .read_to_string()
            .map_err(transport)?;
        // Envelope.
        // 准确协议信封。
        let envelope: Value = serde_json::from_str(&text).map_err(|error| RpcError {
            message: format!("{method}: invalid response ({status}): {error}"),
            transport: false,
        })?;
        if envelope.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(envelope.get("result").cloned().unwrap_or(Value::Null));
        }
        // Error.
        // 当前错误。
        let error = envelope.get("error");
        // Field.
        // 准确字段定义。
        let field = |name: &str| {
            error
                .and_then(|e| e.get(name))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        // Catalog-backed failures render in the active locale; unknown keys keep the service text.
        // 目录形式的失败按当前语言渲染；未知键保留服务文本。
        let localized = error
            .and_then(|e| e.get("localized"))
            .and_then(|localized| {
                let key = localized.get("key")?.as_str()?;
                let values: Vec<(&str, &str)> = localized
                    .get("params")
                    .and_then(Value::as_object)
                    .map(|params| {
                        params
                            .iter()
                            .filter_map(|(name, value)| Some((name.as_str(), value.as_str()?)))
                            .collect()
                    })
                    .unwrap_or_default();
                crate::i18n::format_known(key, &values)
            });
        Err(RpcError {
            message: redacted_message(
                localized
                    .or_else(|| field("message"))
                    .unwrap_or_else(|| format!("{method}: HTTP {status}")),
                &secrets,
            ),
            transport: false,
        })
    }

    /// Checks that the service answers with this token, using a cheap read-only call.
    /// 用一个低开销的只读调用检查服务是否以该令牌响应。
    /// Returns `Result<(), RpcError>`.
    /// 返回 `Result<(), RpcError>`。
    pub fn probe(&self) -> Result<(), RpcError> {
        self.call("startup.snapshot", Value::Null).map(|_| ())
    }
}
