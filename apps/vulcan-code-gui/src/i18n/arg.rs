//! Named interpolation arguments consumed by the lightweight i18n formatter.
//! 供轻量级国际化格式化器使用的命名插值参数。

use std::borrow::Cow;

/// One named interpolation argument supplied to a localized template.
/// 提供给本地化模板的一条命名插值参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I18nArg<'a> {
    /// Stable placeholder name without braces.
    /// 不带花括号的稳定占位符名称。
    pub name: &'a str,
    /// Replacement text inserted into the template.
    /// 插入模板中的替换文本。
    pub value: Cow<'a, str>,
}

impl<'a> I18nArg<'a> {
    /// Creates one named interpolation argument from a placeholder name and a replacement value.
    /// 根据占位符名称与替换值创建一条命名插值参数。
    pub fn named(name: &'a str, value: impl Into<Cow<'a, str>>) -> Self {
        Self {
            name,
            value: value.into(),
        }
    }
}
