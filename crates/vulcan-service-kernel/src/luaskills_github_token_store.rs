//! OS credential-store slot for the GitHub token LuaSkills sends to the official GitHub API.
//! LuaSkills 发往官方 GitHub API 的 GitHub 令牌在操作系统凭据库中的槽位。
//!
//! The token never enters runtime settings, the application database, logs or service
//! responses: it lives in this slot and in LuaSkills engine memory only.
//! 令牌绝不进入运行时设置、应用数据库、日志或服务响应：它只存在于本槽位与 LuaSkills 引擎内存中。

use keyring::{Entry, Error as KeyringError};

/// Secure-store namespace owned by Vulcan Code for the LuaSkills GitHub token.
/// Vulcan Code 为 LuaSkills GitHub 令牌持有的安全存储命名空间。
const LUASKILLS_GITHUB_TOKEN_SERVICE: &str = "vulcan-code.luaskills.github-token-v1";

/// Single account inside the namespace; one token serves the whole application.
/// 命名空间内的唯一账户；整个应用共用一个令牌。
const LUASKILLS_GITHUB_TOKEN_ACCOUNT: &str = "default";

/// Longest accepted token; GitHub tokens are far shorter, so longer input is a paste mistake.
/// 可接受的最长令牌；GitHub 令牌远短于此，更长的输入属于粘贴错误。
const LUASKILLS_GITHUB_TOKEN_MAX_BYTES: usize = 512;

/// Opens the single token slot.
/// 打开唯一的令牌槽位。
fn token_entry() -> Result<Entry, String> {
    Entry::new(LUASKILLS_GITHUB_TOKEN_SERVICE, LUASKILLS_GITHUB_TOKEN_ACCOUNT)
        .map_err(|_| String::from("Failed to open the operating-system credential store."))
}

/// Reads the stored token, returning `None` when none was saved.
/// 读取已保存的令牌；未保存时返回 `None`。
pub(crate) fn read_luaskills_github_token() -> Result<Option<String>, String> {
    match token_entry()?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(_) => Err(String::from(
            "Failed to read the LuaSkills GitHub token from the operating-system credential store.",
        )),
    }
}

/// Validates and stores one token, returning the trimmed value that was saved.
/// 校验并保存一个令牌，返回实际保存的去除首尾空白后的值。
pub(crate) fn store_luaskills_github_token(token: &str) -> Result<String, String> {
    let token = validated_token(token)?;
    token_entry()?.set_password(&token).map_err(|_| {
        String::from(
            "Failed to save the LuaSkills GitHub token to the operating-system credential store.",
        )
    })?;
    Ok(token)
}

/// Deletes the stored token; deleting an absent token succeeds.
/// 删除已保存的令牌；删除不存在的令牌同样成功。
pub(crate) fn delete_luaskills_github_token() -> Result<(), String> {
    match token_entry()?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(_) => Err(String::from(
            "Failed to delete the LuaSkills GitHub token from the operating-system credential store.",
        )),
    }
}

/// Trims one token and rejects empty, overlong or whitespace-containing input.
/// 去除令牌首尾空白，并拒绝空值、过长或包含空白的输入。
fn validated_token(token: &str) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err(String::from("The GitHub token is empty."));
    }
    if token.len() > LUASKILLS_GITHUB_TOKEN_MAX_BYTES
        || token.chars().any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(String::from(
            "The GitHub token contains spaces or is too long; paste only the token itself.",
        ));
    }
    Ok(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::validated_token;

    /// Verify surrounding whitespace is trimmed and malformed input is refused without echoing it.
    /// 验证首尾空白会被去除，格式错误的输入会被拒绝且不回显。
    #[test]
    fn token_input_is_validated() {
        assert_eq!(
            validated_token("  ghp_example  ").expect("valid token"),
            "ghp_example"
        );
        for input in ["", "   ", "ghp a", "ghp\nb"] {
            let error = validated_token(input).expect_err("malformed token");
            assert!(!error.contains("ghp"));
        }
        assert!(validated_token(&"x".repeat(513)).is_err());
    }
}
