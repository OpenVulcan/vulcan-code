//! Bearer-token issuance and verification for MCP clients.
//! MCP 客户端 Bearer 令牌的签发与校验。

use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use vulcan_provider_config::provider_config::{McpServiceClient, PersistedMcpServiceConfiguration};

/// Prefix that makes Vulcan MCP tokens recognizable in configuration files and secret scanners.
/// 让 Vulcan MCP 令牌在配置文件与秘密扫描器中可识别的前缀。
const MCP_TOKEN_PREFIX: &str = "vmcp_";

/// Random bytes behind every token.
/// 每个令牌背后的随机字节数。
const MCP_TOKEN_RANDOM_BYTES: usize = 32;

/// Characters of the token kept as a non-secret hint.
/// 作为非秘密提示保留的令牌字符数。
const MCP_TOKEN_HINT_CHARS: usize = 4;

/// A freshly issued token and the facts the configuration stores about it.
/// 新签发的令牌以及配置为其保存的事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedClientToken {
    /// Bearer token shown to the user once.
    /// 只向用户展示一次的 Bearer 令牌。
    pub token: String,
    /// Lowercase hexadecimal SHA-256 of the token.
    /// 令牌的小写十六进制 SHA-256。
    pub token_sha256: String,
    /// Last characters of the token.
    /// 令牌末尾几个字符。
    pub token_hint: String,
}

/// Issues one new random bearer token.
/// 签发一个新的随机 Bearer 令牌。
pub fn issue_client_token() -> IssuedClientToken {
    let mut bytes = [0_u8; MCP_TOKEN_RANDOM_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    let token = format!(
        "{MCP_TOKEN_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    );
    let token_hint = token
        .chars()
        .rev()
        .take(MCP_TOKEN_HINT_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    IssuedClientToken {
        token_sha256: token_sha256_hex(&token),
        token,
        token_hint,
    }
}

/// Returns the lowercase hexadecimal SHA-256 of a token.
/// 返回令牌的小写十六进制 SHA-256。
pub fn token_sha256_hex(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Why a request could not be attributed to a client.
/// 请求无法归属到客户端的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAuthFailure {
    /// The service switch is off.
    /// 服务开关已关闭。
    ServiceDisabled,
    /// No usable bearer token was sent.
    /// 未发送可用的 Bearer 令牌。
    MissingToken,
    /// The token matches no enabled client.
    /// 令牌不匹配任何已启用客户端。
    InvalidToken,
}

/// Extracts the token from an `Authorization: Bearer` header value.
/// 从 `Authorization: Bearer` 头部取值中提取令牌。
pub(crate) fn bearer_token(authorization: Option<&str>) -> Option<&str> {
    let value = authorization?.trim();
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// Finds the enabled client owning `authorization`.
/// 查找拥有 `authorization` 的已启用客户端。
///
/// # Parameters
/// 参数
/// `configuration` is the normalized settings; `authorization` is the raw header value.
/// `configuration` 是已规范化设置；`authorization` 是原始头部取值。
///
/// # Returns
/// 返回值
/// The client, or the reason authentication failed.
/// 返回客户端，或认证失败的原因。
pub(crate) fn authenticate<'a>(
    configuration: &'a PersistedMcpServiceConfiguration,
    authorization: Option<&str>,
) -> Result<&'a McpServiceClient, McpAuthFailure> {
    if !configuration.enabled {
        return Err(McpAuthFailure::ServiceDisabled);
    }
    let token = bearer_token(authorization).ok_or(McpAuthFailure::MissingToken)?;
    let digest = token_sha256_hex(token);
    // Every client is compared so the timing does not reveal which entry matched.
    // 比较每个客户端，使耗时不暴露命中的条目。
    let mut matched = None;
    for client in &configuration.clients {
        if digests_equal(client.token_sha256.as_bytes(), digest.as_bytes()) && client.enabled {
            matched = Some(client);
        }
    }
    matched.ok_or(McpAuthFailure::InvalidToken)
}

/// Compares two digests without an early exit.
/// 无提前退出地比较两个摘要。
fn digests_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}
