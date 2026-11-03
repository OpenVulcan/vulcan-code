//! Fixed OpenCode Console authorities and workspace identity boundaries.
//! 固定的 OpenCode Console 授权端与工作区身份边界。

use crate::oauth::OAUTH_HTTP_REQUEST_TIMEOUT;

/// Console authority used by device authorization and account reads.
/// 设备授权与账号读取使用的 Console 授权端。
pub(crate) const CONSOLE_URL: &str = "https://opencode.ai/console";
/// Go inference authority published by the authenticated Console config, separate from pay-as-you-go.
/// 已鉴权 Console 配置公布的 Go 推理授权端，与按量计费分开。
pub(crate) const INFERENCE_URL: &str = "https://opencode.ai/inference/go";
/// Vulcan Code's own public device-flow identifier, accepted by the production Console.
/// 生产 Console 接受的 Vulcan Code 自有公开设备授权标识。
pub(crate) const CLIENT_ID: &str = "vulcan-code";
/// Workspace header shared by Console account and inference requests.
/// Console 账号与推理请求共用的工作区请求头。
pub(crate) const ORGANIZATION_HEADER: &str = "x-org-id";

/// Builds a bounded client that never forwards credentials through redirects.
/// 构建有界且绝不经重定向转发凭据的客户端。
///
/// No parameters; returns the configured client or a non-secret construction error.
/// 无参数；返回已配置客户端或不含秘密的构建错误。
pub(crate) fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
        .user_agent(crate::execution::vulcan_code_user_agent())
        .build()
        .map_err(|_| String::from("Failed to initialize the OpenCode Console client."))
}

/// Validates one workspace identifier using the Console's published org/wrk identity shape.
/// 按 Console 公布的 org/wrk 身份结构校验单个工作区标识。
///
/// `id` is the authority-issued workspace; returns success or an explicit identity error.
/// `id` 是授权端签发的工作区；返回成功或显式身份错误。
pub(crate) fn validate_organization_id(id: &str) -> Result<(), String> {
    if !(id.starts_with("org_") || id.starts_with("wrk_"))
        || id.len() <= 4
        || id.len() > 256
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(String::from("OpenCode workspace identity is invalid."));
    }
    Ok(())
}
