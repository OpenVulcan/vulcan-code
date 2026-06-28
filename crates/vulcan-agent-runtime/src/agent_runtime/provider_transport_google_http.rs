//! Direct Google Gemini HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 Google Gemini HTTP 与 SSE 直连传输模块。

mod client;
mod execute;
mod messages;
mod payload;
mod response;
mod tool_conversion;
mod util;

pub(crate) use execute::execute_google_gemini_http_streaming_turn;

#[cfg(test)]
use messages::core_message_to_gemini_content;
#[cfg(test)]
use payload::build_google_gemini_request_payload;
#[cfg(test)]
use tool_conversion::provider_tool_definition_to_gemini_function_declaration;

#[cfg(test)]
mod tests;

/// Default Google Gemini REST base URL used by the host-owned direct transport.
/// 宿主自有直连传输使用的默认 Google Gemini REST 基础地址。
const GOOGLE_GEMINI_DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
