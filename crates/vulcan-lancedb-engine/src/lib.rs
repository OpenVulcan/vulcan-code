//! LanceDB client for Vulcan Code over the run-time loaded native library.
//! Vulcan Code 的 LanceDB 客户端，基于运行时加载的原生库。
//!
//! LanceDB, DataFusion and their Arrow tree live in `vulcan-lancedb-native`, a separate library the
//! host loads at run time, so the host executable neither carries nor relinks them. This crate keeps
//! the engine's public paths: [`runtime::LanceDbRuntime`] and [`engine::LanceDbEngine`] are handles
//! whose operations run inside the library, while [`types`], [`error`], [`db_lock`] and
//! [`manager::DatabaseRuntimeConfig`] are the shared definitions from `vulcan-lancedb-contract`.
//!
//! LanceDB、DataFusion 及其 Arrow 依赖树位于 `vulcan-lancedb-native`，这是宿主在运行时加载的独立库，因此宿主
//! 可执行文件既不携带也不重新链接它们。本 crate 保留引擎的公开路径：[`runtime::LanceDbRuntime`] 与
//! [`engine::LanceDbEngine`] 是在库内执行操作的句柄，而 [`types`]、[`error`]、[`db_lock`] 与
//! [`manager::DatabaseRuntimeConfig`] 是来自 `vulcan-lancedb-contract` 的共用定义。

#![warn(missing_docs)]

pub use vulcan_lancedb_contract::{db_lock, error, types};

pub mod engine;
mod library;
pub mod runtime;

/// Database runtime configuration.
/// 数据库运行时配置。
pub mod manager {
    pub use vulcan_lancedb_contract::config::DatabaseRuntimeConfig;
}
