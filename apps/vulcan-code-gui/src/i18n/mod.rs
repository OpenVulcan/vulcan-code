//! Lightweight GUI localization infrastructure and embedded language-pack accessors.
//! 轻量级图形界面本地化基础设施与内置语言包访问入口。

mod arg;
mod key;
mod locale;
mod manager;
mod registry;

pub(crate) use arg::I18nArg;
pub(crate) use key::I18nKey;
pub(crate) use locale::AppLocale;
pub(crate) use manager::LanguageManager;
pub(crate) use registry::{LanguageRegistry, format_for_locale, text_for_locale};
