//! TokenRhythm wallet balance read with the signed-in account session.
//! 使用已登录账户会话读取的 TokenRhythm 钱包余额。

use super::super::http::PROVIDER_METADATA_REQUEST_TIMEOUT;
use super::current_unix_milliseconds;
use crate::CredentialExecutionSecret;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot};

/// Wallet resource of the integration application.
/// 集成应用的钱包资源。
#[derive(Deserialize)]
struct Wallet {
    /// Account the wallet belongs to.
    /// 钱包所属账户。
    account_id: String,
    /// Currency code of every balance.
    /// 全部余额的币种代码。
    currency: String,
    /// Exact decimal balances.
    /// 精确十进制余额。
    balances: Balances,
}

/// Exact wallet components as decimal strings.
/// 以十进制字符串表示的精确钱包分量。
#[derive(Deserialize)]
struct Balances {
    /// Spendable amount.
    /// 可用金额。
    available: String,
    /// Recharged part of the balance.
    /// 余额中的充值部分。
    recharge: String,
    /// Granted bonus part of the balance.
    /// 余额中的赠送部分。
    bonus: String,
    /// Outstanding debt.
    /// 未结清欠款。
    debt: String,
    /// Amount held by in-flight requests.
    /// 进行中请求占用的金额。
    frozen: String,
}

/// Reads the wallet of the signed-in account.
/// 读取已登录账户的钱包。
///
/// `secret` must originate from TokenRhythm account login. Returns the available balance, or a
/// controlled error without upstream body text.
/// `secret` 必须来自 TokenRhythm 账户登录。返回可用余额，或不含上游正文的受控错误。
pub(in crate::client) async fn query_tokenrhythm_wallet(
    secret: &CredentialExecutionSecret,
) -> Result<AccountUsageSnapshot, String> {
    let access = secret.tokenrhythm_account()?;
    let http = crate::tokenrhythm::client()?;
    query_at(
        &http,
        crate::tokenrhythm::ORIGIN,
        &access.access_token,
        &access.account_id,
    )
    .await
}

/// Reads the wallet from `origin`, the production authority or a local test server.
/// 从 `origin`（生产授权端或本地测试服务）读取钱包。
async fn query_at(
    http: &reqwest::Client,
    origin: &str,
    access_token: &str,
    account_id: &str,
) -> Result<AccountUsageSnapshot, String> {
    let wallet: Wallet = crate::tokenrhythm::integration_json(
        http.get(crate::tokenrhythm::integration_url(origin, "wallet"))
            .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT),
        access_token,
        "wallet",
    )
    .await?;
    normalize(wallet, account_id)
}

/// Converts one wallet into a balance snapshot after checking identity and decimals.
/// 校验身份与十进制数后，将钱包转换为余额快照。
fn normalize(wallet: Wallet, account_id: &str) -> Result<AccountUsageSnapshot, String> {
    if wallet.account_id != account_id {
        return Err(String::from(
            "TokenRhythm wallet belongs to another account.",
        ));
    }
    if wallet.currency.is_empty()
        || wallet.currency.len() > 8
        || !wallet
            .currency
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic())
    {
        return Err(String::from("TokenRhythm wallet currency is invalid."));
    }
    let balances = &wallet.balances;
    for value in [
        &balances.available,
        &balances.recharge,
        &balances.bonus,
        &balances.debt,
        &balances.frozen,
    ] {
        if !is_signed_decimal(value) {
            return Err(String::from(
                "TokenRhythm wallet contains an invalid decimal.",
            ));
        }
    }
    // The available amount is shown as reported; nothing is summed or inferred on the host.
    // 可用金额按上报值展示；宿主不做加总或推算。
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(String::from("TokenRhythm Wallet")),
        plan_id: None,
        used_amount: None,
        remaining_amount: Some(balances.available.clone()),
        currency: Some(wallet.currency.clone()),
        balance_amount: Some(balances.available.clone()),
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: (!is_zero(&balances.debt))
            .then(|| String::from("TokenRhythm reports an outstanding debt on this account.")),
    })
}

/// Accepts an optionally negative decimal with at most sixteen fraction digits.
/// 接受可带负号、小数位不超过十六位的十进制数。
fn is_signed_decimal(value: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    match unsigned.split_once('.') {
        Some((whole, fraction)) => digits(whole) && digits(fraction) && fraction.len() <= 16,
        None => digits(unsigned),
    }
}

/// Reports whether a validated decimal equals zero.
/// 判断已校验的十进制数是否为零。
fn is_zero(value: &str) -> bool {
    value.bytes().all(|byte| matches!(byte, b'0' | b'.' | b'-'))
}

#[cfg(test)]
mod tests;
