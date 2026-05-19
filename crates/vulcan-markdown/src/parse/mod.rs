//! Parse-centric exports grouped under one dedicated module boundary.
//! 以解析能力为中心的导出集中收敛到一个独立模块边界下。

pub use crate::content::{
    HeadingLevel, Highlight, ParseSettings, SoftBreakBehavior, Uri, parse,
};
