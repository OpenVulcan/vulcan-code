//! In-process LanceDB engine for Vulcan Code.
//! Vulcan Code 进程内 LanceDB 引擎。
//!
//! This crate owns LanceDB technical capability only: per-database shared engine reuse, table
//! creation, typed batch writes, vector search, deletion and table drop, physical statistics, and
//! file compaction, with database-level and table-level concurrency control.
//!
//! 本 crate 只负责 LanceDB 技术能力：每数据库共享引擎复用、建表、类型化批量写入、向量检索、
//! 删除与删表、物理统计和文件合并，以及数据库级与表级并发控制。
//!
//! Explicit non-responsibilities / 明确的非职责:
//!
//! - It does not know about projects, sessions, plugins, skills or user identity.
//! - It does not accept an "arbitrary relative path under a space root" style input.
//! - It does not read Vulcan configuration files.
//! - It does not launch out-of-process threads, ports or network services.
//! - It does not carry frontend state, VCE job state or business generations.
//! - It returns structured local errors, never gRPC status codes.
//!
//! v1 data format baseline: every table creation path pins [`lancedb::LanceFileVersion::V2_1`]
//! explicitly, and Lance Blob v2 columns are prohibited.
//!
//! v1 数据格式基线：所有建表路径显式固定 `LanceFileVersion::V2_1`，并禁止 Lance Blob v2 列。

#![warn(missing_docs)]

pub mod db_lock;
pub mod engine;
pub mod error;
pub mod manager;
pub mod runtime;
pub mod storage_format;
pub mod types;
