//! Client handle for one database engine inside the native LanceDB library.
//! 原生 LanceDB 库中一个数据库引擎的客户端句柄。
//!
//! Every method sends one request through the loaded library; table locks, concurrency permits and
//! format verification all run inside the library exactly as they did in process.
//! 每个方法都通过已加载的库发送一个请求；表锁、并发许可与格式校验都在库内执行，与进程内时完全一致。

use std::sync::Arc;

pub use vulcan_lancedb_contract::config::LanceDbEngineOptions;
use vulcan_lancedb_contract::error::EngineResult;
use vulcan_lancedb_contract::types::{
    LanceDbCompactTableInput, LanceDbCompactTableResult, LanceDbCreateTableInput,
    LanceDbCreateTableResult, LanceDbDeleteInput, LanceDbDeleteResult, LanceDbDistanceMetric,
    LanceDbDropTableInput, LanceDbDropTableResult, LanceDbSearchInput, LanceDbSearchResult,
    LanceDbTableStatisticsInput, LanceDbTableStatisticsResult, LanceDbUpsertInput,
    LanceDbUpsertResult,
};
use vulcan_lancedb_contract::wire::{Request, Response};

use crate::library::{NativeLibrary, call, unexpected};

/// Engine over one database, shared by every clone; the library handle is released with the last.
/// 基于单个数据库的引擎，由所有克隆共享；最后一个克隆释放时释放库句柄。
#[derive(Clone)]
pub struct LanceDbEngine {
    /// Shared library handle.
    /// 共享的库句柄。
    handle: Arc<EngineHandle>,
}

/// One engine handle issued by the library.
/// 库签发的一个引擎句柄。
struct EngineHandle {
    /// Library that issued the handle.
    /// 签发该句柄的库。
    library: &'static NativeLibrary,
    /// Engine handle.
    /// 引擎句柄。
    engine: u64,
    /// Identity of the database engine behind the handle.
    /// 句柄背后数据库引擎的身份。
    database: u64,
    /// Options the engine was opened with.
    /// 打开引擎时使用的选项。
    options: LanceDbEngineOptions,
}

impl Drop for EngineHandle {
    /// Releases the library handle; the engine stays alive while its runtime caches it.
    /// 释放库句柄；只要运行时仍缓存引擎，引擎就继续存活。
    fn drop(&mut self) {
        // Releasing a live handle cannot fail, and a destructor has nowhere to report to.
        // 释放存活句柄不会失败，析构函数也无处报告。
        let _ = self.library.exchange(
            &Request::ReleaseEngine {
                engine: self.engine,
            },
            &[],
        );
    }
}

impl LanceDbEngine {
    /// Wraps one engine handle issued by the library.
    /// 包装库签发的一个引擎句柄。
    pub(crate) fn from_handle(
        library: &'static NativeLibrary,
        engine: u64,
        database: u64,
        options: LanceDbEngineOptions,
    ) -> Self {
        Self {
            handle: Arc::new(EngineHandle {
                library,
                engine,
                database,
                options,
            }),
        }
    }

    /// Reports whether both engines serve the same physical database through one shared engine.
    /// 报告两个引擎是否通过同一个共享引擎服务同一物理数据库。
    pub fn shares_database_with(&self, other: &Self) -> bool {
        self.handle.database == other.handle.database
    }

    /// Returns the engine options.
    /// 返回引擎选项。
    pub fn options(&self) -> &LanceDbEngineOptions {
        &self.handle.options
    }

    /// Creates a table, then verifies its on-disk Lance format matches the v1 baseline.
    /// 建表，并校验其磁盘上的 Lance 格式符合 v1 基线。
    pub async fn create_table(
        &self,
        input: LanceDbCreateTableInput,
    ) -> EngineResult<LanceDbCreateTableResult> {
        let engine = self.handle.engine;
        match call(Request::CreateTable { engine, input }, Vec::new()).await? {
            (Response::TableCreated { result }, _) => Ok(result),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Appends rows, or merges them by key when key columns are supplied.
    /// 追加行，或在提供键列时按键合并。
    pub async fn vector_upsert(
        &self,
        mut input: LanceDbUpsertInput,
    ) -> EngineResult<LanceDbUpsertResult> {
        let engine = self.handle.engine;
        let rows = std::mem::take(&mut input.data);
        match call(Request::VectorUpsert { engine, input }, rows).await? {
            (Response::Upserted { result }, _) => Ok(result),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Maintains a scalar merge-key index so updates do not scan the whole target.
    /// 维护标量合并键索引，避免每次更新插入都扫描完整目标。
    pub async fn maintain_merge_key_index(
        &self,
        table_name: &str,
        column: &str,
    ) -> EngineResult<()> {
        let request = Request::MaintainMergeKeyIndex {
            engine: self.handle.engine,
            table_name: table_name.to_string(),
            column: column.to_string(),
        };
        match call(request, Vec::new()).await? {
            (Response::IndexMaintained, _) => Ok(()),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Runs a nearest-neighbour vector search.
    /// 执行最近邻向量检索。
    pub async fn vector_search(
        &self,
        input: LanceDbSearchInput,
    ) -> EngineResult<LanceDbSearchResult> {
        self.vector_search_with_metric(input, LanceDbDistanceMetric::Euclidean)
            .await
    }

    /// Searches `input` using the explicitly frozen `metric`; returns rows ordered by that distance.
    /// 使用显式冻结 `metric` 检索 `input`；返回按该距离排序的行。
    pub async fn vector_search_with_metric(
        &self,
        input: LanceDbSearchInput,
        metric: LanceDbDistanceMetric,
    ) -> EngineResult<LanceDbSearchResult> {
        let engine = self.handle.engine;
        match call(
            Request::VectorSearch {
                engine,
                input,
                metric,
            },
            Vec::new(),
        )
        .await?
        {
            (Response::Searched { mut result }, rows) => {
                result.data = rows;
                Ok(result)
            }
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Deletes rows by predicate.
    /// 按谓词删除行。
    pub async fn delete(&self, input: LanceDbDeleteInput) -> EngineResult<LanceDbDeleteResult> {
        let engine = self.handle.engine;
        match call(Request::Delete { engine, input }, Vec::new()).await? {
            (Response::Deleted { result }, _) => Ok(result),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Drops a whole table.
    /// 删除整张表。
    pub async fn drop_table(
        &self,
        input: LanceDbDropTableInput,
    ) -> EngineResult<LanceDbDropTableResult> {
        let engine = self.handle.engine;
        match call(Request::DropTable { engine, input }, Vec::new()).await? {
            (Response::TableDropped { result }, _) => Ok(result),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Reads authoritative physical statistics for one table.
    /// 读取一张表的权威物理统计信息。
    pub async fn table_statistics(
        &self,
        input: LanceDbTableStatisticsInput,
    ) -> EngineResult<LanceDbTableStatisticsResult> {
        let engine = self.handle.engine;
        match call(Request::TableStatistics { engine, input }, Vec::new()).await? {
            (Response::Statistics { result }, _) => Ok(result),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Compacts small physical files without pruning historical table versions.
    /// 合并小型物理文件，同时不裁剪历史表版本。
    pub async fn compact_table(
        &self,
        input: LanceDbCompactTableInput,
    ) -> EngineResult<LanceDbCompactTableResult> {
        let engine = self.handle.engine;
        match call(Request::CompactTable { engine, input }, Vec::new()).await? {
            (Response::Compacted { result }, _) => Ok(result),
            (other, _) => Err(unexpected(other)),
        }
    }
}
