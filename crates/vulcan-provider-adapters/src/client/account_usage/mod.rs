//! Registered account-usage primitives and their common observation clock.
//! 已注册账号用量原语及其共用观测时钟。

mod codex;
mod command_code;
mod decimal;
mod deepseek;
mod kimi;
mod minimax;
mod nodeai;
mod open_design;
mod opencode;
mod openrouter;
mod tavily;
mod tokenrhythm;
mod xiaomi;

pub(super) use codex::query_openai_codex_usage;
pub(super) use command_code::query_command_code_usage;
pub(super) use deepseek::query_deepseek_balance;
pub(super) use kimi::query_kimi_coding_plan;
pub(super) use minimax::query_minimax_token_plan;
pub(super) use nodeai::query_nodeai_balance;
pub(super) use open_design::query_open_design_wallet;
pub(super) use opencode::query_opencode_go;
pub(super) use openrouter::query_openrouter_credits;
pub(super) use tavily::query_tavily_credits;
pub(super) use tokenrhythm::query_tokenrhythm_wallet;
pub(super) use xiaomi::query_xiaomi_usage;

/// Returns the current Unix timestamp in milliseconds without manufacturing an epoch value.
/// 返回当前 Unix 毫秒时间戳，且不会伪造纪元零值。
///
/// # Returns
/// 返回值
/// The exact timestamp when the system clock is valid, otherwise `None`.
/// 系统时钟有效时返回精确时间戳，否则返回 `None`。
fn current_unix_milliseconds() -> Option<String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .ok()
}
