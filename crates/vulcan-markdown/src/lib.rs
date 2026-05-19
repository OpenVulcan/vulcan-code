//! Public facade for the Vulcan-owned Markdown parsing and viewing core.
//! Vulcan 自主管理的 Markdown 解析与展示核心公共门面。

mod content;
pub mod item;
pub mod parse;
pub mod settings;
pub mod style;
pub mod transcript;

pub use content::*;
