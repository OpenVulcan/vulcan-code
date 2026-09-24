//! V1 unified session physical storage.
//! V1 统一会话物理存储。
//!
//! This module owns the production V1 session directory contract: exact
//! layout, atomic publication with stage-level fault injection, typed small
//! documents, strict JSONL segments, whole-tail flush with rollover, strict
//! loading, event windows, and generation-based history edit or fork.
//! 本模块负责生产环境 V1 会话目录契约：精确布局、带阶段级故障注入的原子发布、
//! 强类型小文档、严格 JSONL 分片、带滚段的完整尾部刷新、严格加载、事件窗口，
//! 以及基于 generation 的历史编辑或分叉。
pub(crate) mod atomic_io;
pub mod documents;
pub mod errors;
pub mod event_window;
pub mod generation;
pub mod layout;
pub mod loader;
mod metadata_migration;
pub mod segment;
pub mod writer;

#[cfg(test)]
mod tests;
