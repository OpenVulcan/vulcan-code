//! Catalog-backed user-facing messages that clients render in their own language.
//! 由客户端按自身语言渲染、以语言目录为依据的面向用户消息。
//!
//! The service sends a `webui` catalog key with its placeholder values instead of fixed text; the
//! English rendering travels alongside for logs and clients that cannot translate.
//! 服务发送 `webui` 目录键及其占位符取值，而不是固定文本；英文渲染结果随附传输，供日志和无法翻译的
//! 客户端使用。

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// One `webui` catalog key and the values of its `{name}` placeholders.
/// 一个 `webui` 目录键及其 `{name}` 占位符的取值。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiLocalizedMessage {
    /// Stable `webui` catalog key.
    /// 稳定的 `webui` 目录键。
    pub key: String,
    /// Placeholder values keyed by placeholder name.
    /// 以占位符名称为键的占位符取值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

impl WebuiLocalizedMessage {
    /// Creates a message for `key`; the key must be a string literal so the catalog test can find it.
    /// 为 `key` 创建消息；键必须是字符串字面量，以便目录测试发现它。
    pub fn new(key: &'static str) -> Self {
        Self {
            key: key.to_string(),
            params: BTreeMap::new(),
        }
    }

    /// Adds one placeholder value; returns the message for chaining.
    /// 添加一个占位符取值；返回消息以便链式调用。
    pub fn with(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.params.insert(name.to_string(), value.into());
        self
    }

    /// Renders the message in the default catalog language; an unknown key renders as itself.
    /// 以目录默认语言渲染消息；未知键渲染为键本身。
    pub fn default_text(&self) -> String {
        static DEFAULT_MESSAGES: OnceLock<vulcan_i18n::Messages> = OnceLock::new();
        let messages = DEFAULT_MESSAGES
            .get_or_init(|| vulcan_i18n::WEBUI.messages(vulcan_i18n::DEFAULT_LOCALE));
        let template = messages
            .get(&self.key)
            .map_or(self.key.as_str(), String::as_str);
        let values: Vec<(&str, &str)> = self
            .params
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        vulcan_i18n::format_template(template, &values)
    }
}

impl std::fmt::Display for WebuiLocalizedMessage {
    /// Writes the default-language rendering into formatter; returns the formatting result.
    /// 将默认语言渲染结果写入格式器；返回格式化结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.default_text())
    }
}

#[cfg(test)]
mod tests;
