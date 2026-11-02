//! Types, failures and the wire protocol shared by the LanceDB client and its native library.
//! LanceDB 客户端与其原生库共用的类型、失败与传输协议。
//!
//! The LanceDB engine and its DataFusion and Arrow dependency tree are compiled into a separate
//! native library (`vulcan-lancedb-native`) that the host loads at run time, so the host executable
//! neither carries nor relinks them. This crate is everything both sides must agree on; it holds no
//! LanceDB dependency, which keeps the library's inputs stable while host code changes.
//!
//! LanceDB 引擎及其 DataFusion、Arrow 依赖树被编译进独立的原生库（`vulcan-lancedb-native`），由宿主在运行时
//! 加载，因此宿主可执行文件既不携带也不重新链接它们。本 crate 是双方必须一致的全部内容；它不依赖 LanceDB，
//! 从而在宿主代码变化时保持库的输入稳定。

#![warn(missing_docs)]

pub mod config;
pub mod db_lock;
pub mod error;
pub mod types;
pub mod wire;
