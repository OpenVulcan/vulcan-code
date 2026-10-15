//! In-process SQLite engine for Vulcan Code.
//! Vulcan Code 进程内 SQLite 引擎。
//!
//! This crate owns SQLite technical capability only: opening and connection configuration, SQL,
//! transactions, batches and queries, FTS5 with Jieba tokenization, and process-level file
//! ownership locking.
//!
//! 本 crate 只负责 SQLite 技术能力：打开与连接配置、SQL、事务、批量和查询、FTS5 与 Jieba
//! 分词，以及进程级文件所有权锁。
//!
//! Explicit non-responsibilities / 明确的非职责:
//!
//! - It does not know about projects, sessions, plugins, skills or user identity.
//! - It does not accept an "arbitrary relative path under a space root" style input.
//! - It does not read Vulcan configuration files.
//! - It does not launch out-of-process threads, ports or network services.
//! - It does not carry frontend state, consumer job state or business generations.
//! - It returns structured local errors, never gRPC status codes.
//!
//! The `unsafe_code` lint is relaxed in this crate for two reasons: the FTS5 Jieba tokenizer, whose
//! rationale is in `Cargo.toml`, and the test-only allocation accounting in `allocation_evidence`,
//! which cannot exist without `unsafe` and is absent from every non-test build.
//!
//! 本 crate 因两条理由放宽 `unsafe_code` lint：FTS5 Jieba 分词器（理由见 `Cargo.toml`），以及
//! `allocation_evidence` 中仅测试使用的分配记账——后者无法脱离 `unsafe` 存在，且不存在于任何非测试构建中。

#![warn(missing_docs)]

pub mod db_lock;
pub mod error;
pub mod fts;
pub mod runtime;
pub mod sql_exec;
pub mod tokenizer;

/// Test-only allocation accounting used by the plan §10 peak-memory evidence tests.
/// 仅测试使用的分配记账，供方案 §10 的峰值内存证据测试使用。
///
/// `unsafe_code` is allowed in this crate for the FTS5 Jieba tokenizer (see `Cargo.toml` and
/// `tokenizer.rs`); this module is the second, test-only reason: a global allocator cannot be written
/// without `unsafe`, and `#[cfg(test)]` keeps it out of every non-test build.
/// 本 crate 为 FTS5 Jieba 分词器放宽了 `unsafe_code`（见 `Cargo.toml` 与 `tokenizer.rs`）；本模块是第二条、
/// 仅测试的理由：全局分配器无法脱离 `unsafe` 编写，而 `#[cfg(test)]` 使它不会进入任何非测试构建。
#[cfg(test)]
pub(crate) mod allocation_evidence {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Bookkeeping allocator that tracks live bytes and their high-water mark.
    /// 记账分配器：跟踪存活字节数及其高水位。
    struct CountingAllocator;

    static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
    static HIGH_WATER_BYTES: AtomicUsize = AtomicUsize::new(0);

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    // SAFETY: every method forwards to `System` unchanged and only adds atomic bookkeeping, so the
    // allocator contract is exactly `System`'s.
    // 安全性：每个方法都原样转发给 `System`，只增加原子记账，故分配器契约与 `System` 完全一致。
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc(layout) };
            if !pointer.is_null() {
                let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
                HIGH_WATER_BYTES.fetch_max(live, Ordering::Relaxed);
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
            unsafe { System.dealloc(pointer, layout) };
        }
    }

    /// Resets the high-water mark to the currently live byte count.
    /// 把高水位重置为当前的存活字节数。
    pub(crate) fn reset_high_water() {
        HIGH_WATER_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    /// Returns the peak live bytes observed since the last reset.
    /// 返回自上次重置以来观测到的存活字节峰值。
    pub(crate) fn high_water_bytes() -> usize {
        HIGH_WATER_BYTES.load(Ordering::Relaxed)
    }
}
