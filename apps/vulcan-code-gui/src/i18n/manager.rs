//! Stateful locale manager that resolves localized text through the embedded registry.
//! 通过内置注册表解析本地化文本的有状态语言管理器。

use super::{AppLocale, I18nArg, I18nKey, LanguageRegistry};

/// Stateful locale manager used by the GUI to resolve localized text and switch the current language.
/// 供图形界面解析本地化文本并切换当前语言使用的有状态语言管理器。
#[derive(Debug, Clone)]
pub struct LanguageManager {
    /// Embedded language-pack registry shared by the whole GUI process.
    /// 由整个图形界面进程共享的内置语言包注册表。
    registry: &'static LanguageRegistry,
    /// Currently selected locale used for lookups.
    /// 用于查询的当前选中语言。
    current_locale: AppLocale,
}

impl LanguageManager {
    /// Creates one language manager bound to the embedded registry and the requested initial locale.
    /// 创建一个绑定内置注册表并使用指定初始语言的语言管理器。
    pub fn new(initial_locale: AppLocale) -> Self {
        Self {
            registry: LanguageRegistry::embedded(),
            current_locale: initial_locale,
        }
    }

    /// Returns the currently active locale.
    /// 返回当前激活的语言。
    pub fn current_locale(&self) -> AppLocale {
        self.current_locale
    }

    /// Switches the currently active locale.
    /// 切换当前激活的语言。
    pub fn set_locale(&mut self, locale: AppLocale) {
        self.current_locale = locale;
    }

    /// Returns the complete locale list registered in the embedded registry.
    /// 返回已注册到内置注册表中的完整语言列表。
    pub fn available_locales(&self) -> &'static [AppLocale] {
        self.registry.available_locales()
    }

    /// Resolves one plain localized text by key using the current locale and fallback rules.
    /// 使用当前语言与回退规则根据键解析一条普通本地化文本。
    pub fn text(&self, key: I18nKey) -> String {
        self.registry.text(self.current_locale, key)
    }

    /// Resolves one formatted localized text by key using the current locale and named arguments.
    /// 使用当前语言与命名参数根据键解析一条带格式的本地化文本。
    pub fn format(&self, key: I18nKey, args: &[I18nArg<'_>]) -> String {
        self.registry.format(self.current_locale, key, args)
    }
}
