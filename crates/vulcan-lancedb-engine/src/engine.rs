//! Embeddable LanceDB engine: table lifecycle, typed writes, vector search and compaction.
//! 可嵌入的 LanceDB 引擎：表生命周期、类型化写入、向量检索与整理。
//!
//! Concurrency model / 并发模型:
//!
//! Every operation first takes its **per-table** lock, then a **process-wide concurrency permit**.
//! The per-table coordinator gives ordinary mutations an exclusive barrier while letting snapshot
//! reads and native compaction share it, and a separate per-table gate serializes compactors so two
//! compactions cannot target one table at once. Compactors acquire the gate **before** joining the
//! read side of the barrier; acquiring in the other order would let several compactors each hold a
//! read permit while all waiting on the same exclusive gate. Taking table ownership before the
//! global permit also prevents same-table waiters from consuming every global slot and starving
//! unrelated tables.
//!
//! 每个操作先取**表级**锁，再取**进程级并发许可**。表级协调器为普通变更提供独占屏障，同时允许快照
//! 读取与原生整理共享该屏障；另一个表级闸门串行化整理者，使两次整理不能同时作用于一张表。整理者
//! **先**获取闸门再加入屏障读取侧；若顺序相反，多个整理者会各自持有读许可并等待同一独占闸门。先取得
//! 表所有权再取全局许可，也可避免同表等待者占满全部全局槽位并饿死无关表。
//!
//! Format baseline / 格式基线:
//!
//! Table creation both applies the v1 pin and **verifies** the resolved on-disk format, and refuses
//! any schema that would force the forbidden `V2_2` upgrade. See [`crate::storage_format`].
//!
//! 建表既施加 v1 固定项，又**校验**磁盘上实际解析出的格式，并拒绝任何会迫使执行被禁止的 `V2_2`
//! 升级的 Schema。见 [`crate::storage_format`]。

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

use arrow_array::builder::{
    BooleanBuilder, FixedSizeListBuilder, Float32Builder, Float64Builder, Int32Builder,
    Int64Builder, LargeStringBuilder, StringBuilder, UInt32Builder, UInt64Builder,
};
use arrow_array::{
    Array, ArrayRef, BooleanArray, FixedSizeListArray, Float32Array, Float64Array, Int32Array,
    Int64Array, LargeStringArray, RecordBatch, RecordBatchIterator, RecordBatchReader, StringArray,
    UInt32Array, UInt64Array,
};
use arrow_ipc::reader::StreamReader;
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::database::CreateTableMode;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::table::AddDataMode;
use lancedb::table::optimize::{CompactionOptions, OptimizeAction};
use lancedb::{Connection, Table};
use serde_json::{Map, Value};
use tokio::sync::{
    OwnedMutexGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, OwnedSemaphorePermit, RwLock,
    Semaphore,
};

use crate::error::{EngineResult, LanceDbEngineError};
use crate::storage_format::{blob_v2_refusal_reason, verify_table_format};
use crate::types::{
    LanceDbColumnDef, LanceDbColumnType, LanceDbCompactTableInput, LanceDbCompactTableResult,
    LanceDbCompactionMetrics, LanceDbCompactionOptions, LanceDbCreateTableInput,
    LanceDbCreateTableResult, LanceDbDeleteInput, LanceDbDeleteResult, LanceDbDistanceMetric,
    LanceDbDropTableInput, LanceDbDropTableResult, LanceDbFragmentSummary, LanceDbInputFormat,
    LanceDbOutputFormat, LanceDbSearchInput, LanceDbSearchResult, LanceDbTableStatisticsInput,
    LanceDbTableStatisticsResult, LanceDbUpsertInput, LanceDbUpsertResult,
};

/// Largest worker-thread count a compaction request may ask for.
/// 整理请求可要求的最大工作线程数。
pub const MAX_COMPACTION_THREADS: usize = 8;

/// Default search limit applied when a request asks for zero rows.
/// 请求行数为零时采用的默认检索条数。
const DEFAULT_SEARCH_LIMIT: usize = 10;

/// Engine options controlling safety limits and concurrency for one instance.
/// 控制单实例安全限制与并发行为的引擎选项。
#[derive(Debug, Clone)]
pub struct LanceDbEngineOptions {
    /// Maximum accepted upsert payload size in bytes.
    /// 可接受的最大写入载荷字节数。
    pub max_upsert_payload: usize,
    /// Maximum rows a single search may return.
    /// 单次检索可返回的最大行数。
    pub max_search_limit: usize,
    /// Maximum concurrent in-flight requests for this engine.
    /// 本引擎的最大并发在途请求数。
    pub max_concurrent_requests: usize,
}

impl Default for LanceDbEngineOptions {
    /// Provides the migration's stable defaults.
    /// 提供迁移的稳定默认值。
    fn default() -> Self {
        Self {
            max_upsert_payload: 50 * 1024 * 1024,
            max_search_limit: 10_000,
            max_concurrent_requests: 500,
        }
    }
}

/// Embeddable LanceDB engine over one database connection.
/// 基于单个数据库连接的可嵌入 LanceDB 引擎。
#[derive(Clone)]
pub struct LanceDbEngine {
    /// Shared engine state.
    /// 共享的引擎状态。
    state: Arc<LanceDbEngineState>,
}

/// Shared state behind one engine clone family.
/// 一个引擎克隆族共享的状态。
struct LanceDbEngineState {
    /// Database connection.
    /// 数据库连接。
    db: Arc<Connection>,
    /// Per-table synchronization coordinator.
    /// 表级同步协调器。
    table_access: TableAccessCoordinator,
    /// Process-wide in-flight request limiter.
    /// 进程级在途请求限制器。
    concurrency_limiter: Arc<Semaphore>,
    /// Safety limits and defaults.
    /// 安全限制与默认值。
    options: LanceDbEngineOptions,
    /// Test-only observation proving a query completed while native compaction was in flight.
    /// 仅供测试观察，用于证明查询在原生整理执行期间完成。
    #[cfg(test)]
    native_compaction_active: std::sync::atomic::AtomicBool,
}

/// Coordinates snapshot reads, ordinary mutations and exclusive compaction per table.
/// 协调每张表的快照读取、普通变更与独占整理。
#[derive(Default)]
struct TableAccessCoordinator {
    /// Per-table lock sets, created on first use.
    /// 表级锁集合，首次使用时创建。
    locks: Mutex<HashMap<String, Arc<TableAccessLock>>>,
}

/// Synchronization primitives shared by one normalized table identity.
/// 一份规范化表身份共用的同步原语。
#[derive(Default)]
struct TableAccessLock {
    /// Barrier excluding ordinary mutations from reads and compaction.
    /// 将普通变更与读取、整理互斥的屏障。
    mutation_barrier: Arc<RwLock<()>>,
    /// Exclusive gate preventing two native compactions on one table.
    /// 防止两个原生整理作用于同一张表的独占闸门。
    compaction_gate: Arc<tokio::sync::Mutex<()>>,
}

/// Owned guard permitting snapshot reads while excluding mutation and duplicate compaction.
/// 允许快照读取，同时排斥普通变更与重复整理的持有型守卫。
struct TableCompactionGuard {
    /// Compaction ownership held for the whole native operation.
    /// 在完整原生操作期间保留的整理所有权。
    _compaction: OwnedMutexGuard<()>,
    /// Shared barrier compatible with snapshot readers, exclusive with writers.
    /// 与快照读取兼容、与写入互斥的共享屏障。
    _mutation_barrier: OwnedRwLockReadGuard<()>,
}

impl TableAccessCoordinator {
    /// Returns the shared lock set for one table's synchronization identity.
    /// 返回一份表的同步身份共用的锁集合。
    fn lock_for(&self, table_name: &str) -> Arc<TableAccessLock> {
        let mut guard = self
            .locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        Arc::clone(
            guard
                .entry(lock_identity(table_name))
                .or_insert_with(|| Arc::new(TableAccessLock::default())),
        )
    }

    /// Acquires a snapshot-read guard that stays compatible with native compaction.
    /// 获取与原生整理兼容的快照读取守卫。
    async fn acquire_read(&self, table_name: &str) -> OwnedRwLockReadGuard<()> {
        self.lock_for(table_name)
            .mutation_barrier
            .clone()
            .read_owned()
            .await
    }

    /// Acquires an ordinary mutation guard excluding reads and native compaction.
    /// 获取排斥读取与原生整理的普通变更守卫。
    async fn acquire_write(&self, table_name: &str) -> OwnedRwLockWriteGuard<()> {
        self.lock_for(table_name)
            .mutation_barrier
            .clone()
            .write_owned()
            .await
    }

    /// Acquires exclusive compaction ownership while still permitting snapshot readers.
    /// 获取独占整理所有权，同时仍允许快照读取。
    async fn acquire_compaction(&self, table_name: &str) -> TableCompactionGuard {
        let access = self.lock_for(table_name);
        let compaction = access.compaction_gate.clone().lock_owned().await;
        let mutation_barrier = access.mutation_barrier.clone().read_owned().await;
        TableCompactionGuard {
            _compaction: compaction,
            _mutation_barrier: mutation_barrier,
        }
    }
}

impl LanceDbEngine {
    /// Creates an engine over an existing shared connection.
    /// 基于既有共享连接创建引擎。
    pub fn new(db: Arc<Connection>, options: LanceDbEngineOptions) -> Self {
        let max = options.max_concurrent_requests.max(1);
        Self {
            state: Arc::new(LanceDbEngineState {
                db,
                table_access: TableAccessCoordinator::default(),
                concurrency_limiter: Arc::new(Semaphore::new(max)),
                options,
                #[cfg(test)]
                native_compaction_active: std::sync::atomic::AtomicBool::new(false),
            }),
        }
    }

    /// Creates an engine, taking ownership of a non-shared connection.
    /// 创建引擎并取得非共享连接的所有权。
    pub fn from_connection(db: Connection, options: LanceDbEngineOptions) -> Self {
        Self::new(Arc::new(db), options)
    }

    /// Returns the underlying shared connection.
    /// 返回底层共享连接。
    pub fn connection(&self) -> Arc<Connection> {
        Arc::clone(&self.state.db)
    }

    ///
    /// A `create` replaces nothing unless `overwrite_if_exists` is set: the engine passes `CreateTableMode::Create`,
    /// so a second create without the flag is refused and the existing table and its rows survive. When the format
    /// read-back below fails, the created table is deliberately **left in place** rather than dropped — with
    /// `overwrite_if_exists` it may have replaced a table this call did not create, and silently destroying that data
    /// would be worse than reporting a table that needs attention.
    ///
    /// 除设置 `overwrite_if_exists` 外，`create` 不替换任何东西：引擎传入 `CreateTableMode::Create`，因此未设该标志的
    /// 第二次建表会被拒绝，已存在的表及其行得以保留。当下方格式读回失败时，已建表被刻意**留在原处**而非删除——若设置了
    /// `overwrite_if_exists`，它可能替换了并非本次调用创建的表，静默销毁那些数据比报告一张需要关注的表更糟。
    /// Creates a table, then verifies its on-disk Lance format matches the v1 baseline.
    /// 建表，并校验其磁盘上的 Lance 格式符合 v1 基线。
    pub async fn create_table(
        &self,
        input: LanceDbCreateTableInput,
    ) -> EngineResult<LanceDbCreateTableResult> {
        let table_name = normalize_table_name(&input.table_name);
        validate_table_name(&table_name)?;
        if table_name.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "table_name must not be empty",
            ));
        }
        if input.columns.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "columns must not be empty",
            ));
        }

        let schema = build_arrow_schema(&input.columns)?;

        // Refuse a schema that would force the forbidden V2_2 upgrade before touching the database.
        // 在触碰数据库之前，拒绝会迫使执行被禁止的 V2_2 升级的 Schema。
        if let Some(reason) = blob_v2_refusal_reason(&schema) {
            return Err(LanceDbEngineError::schema_not_allowed(reason));
        }

        let _table_guard = self.acquire_table_write(&table_name).await;
        let _concurrency_permit = self.acquire_concurrency_permit().await?;

        let mut builder = self.state.db.create_empty_table(table_name.clone(), schema);
        builder = if input.overwrite_if_exists {
            builder.mode(CreateTableMode::Overwrite)
        } else {
            builder.mode(CreateTableMode::Create)
        };
        let table = builder.execute().await.map_err(map_lancedb_error)?;

        // Setting the connection option is a request; this read-back is what makes it a guarantee.
        // 设置连接选项只是一个请求；这次读回才使它成为保证。
        verify_table_format(&table, &table_name).await?;

        Ok(LanceDbCreateTableResult {
            message: format!("table '{table_name}' is ready"),
        })
    }

    /// Appends rows, or merges them by key when key columns are supplied.
    /// 追加行，或在提供键列时按键合并。
    pub async fn vector_upsert(
        &self,
        input: LanceDbUpsertInput,
    ) -> EngineResult<LanceDbUpsertResult> {
        let table_name = normalize_table_name(&input.table_name);
        validate_table_name(&table_name)?;
        if table_name.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "table_name must not be empty",
            ));
        }
        if input.data.len() > self.state.options.max_upsert_payload {
            return Err(LanceDbEngineError::invalid_argument(format!(
                "payload too large: {} bytes (max {})",
                input.data.len(),
                self.state.options.max_upsert_payload
            )));
        }

        let _table_guard = self.acquire_table_write(&table_name).await;
        let _concurrency_permit = self.acquire_concurrency_permit().await?;
        let table = self.open_table(&table_name).await?;
        let schema = table.schema().await.map_err(map_lancedb_error)?;

        // A key column the table does not have is a caller error, and refusing it here is what keeps the failure from
        // surfacing as a low-level merge error after the write was attempted. The check runs before decoding so a
        // typo costs no CPU work either.
        // 表不具备的键列属于调用方错误；在此拒绝它，可避免该失败在写入已被尝试之后以底层合并错误的形式出现。该检查在
        // 解码之前运行，因此拼错列名也不会消耗 CPU 工作。
        for key in &input.key_columns {
            if schema.field_with_name(key).is_err() {
                return Err(LanceDbEngineError::invalid_argument(format!(
                    "key column `{key}` does not exist in table `{table_name}`"
                )));
            }
        }
        // Decoding is CPU-bound and must not occupy an async worker.
        // 解码是 CPU 密集的，不应占用异步工作线程。
        let decode_format = input.input_format;
        let decode_data = input.data.clone();
        let decode_schema = schema.clone();
        let (batches, input_rows) = tokio::task::spawn_blocking(move || {
            decode_input_to_batches(decode_format, &decode_data, decode_schema)
        })
        .await
        .map_err(|error| {
            LanceDbEngineError::internal(format!("decode task panicked: {error}"))
        })??;

        // A component that is finite as JSON can be infinite as `f32`, and an infinite component in a STORED vector
        // misleads every later nearest-neighbour search that includes the row — a silent, persistent wrong answer
        // rather than a transient one. The check runs after decoding and before the write, the last point where
        // refusing still costs nothing.
        // 作为 JSON 有限的数值分量在 `f32` 下可以是无穷，而**已存储**向量中的无穷分量会误导此后每一次包含该行的
        // 最近邻检索——是静默而持久的错误答案，而非一次性错误。该检查在解码之后、写入之前运行，那是拒绝仍无代价的
        // 最后一点。
        for batch in &batches {
            refuse_non_finite_floats(batch, &table_name)?;
        }

        if input_rows == 0 {
            return Ok(LanceDbUpsertResult {
                message: "no rows to write".to_string(),
                version: table.version().await.map_err(map_lancedb_error)?,
                input_rows: 0,
                inserted_rows: 0,
                updated_rows: 0,
                deleted_rows: 0,
            });
        }

        let schema = table.schema().await.map_err(map_lancedb_error)?;
        let reader: Box<dyn RecordBatchReader + Send> = Box::new(RecordBatchIterator::new(
            batches.into_iter().map(Ok),
            schema,
        ));

        if input.key_columns.is_empty() {
            table
                .add(reader)
                .mode(AddDataMode::Append)
                .execute()
                .await
                .map_err(map_lancedb_error)?;

            verify_table_format(&table, &table_name).await?;
            // The connection option is a request; this read-back is what makes it a guarantee for this path too.
            // 连接选项只是一个请求；这次读回使它在本文路径上同样成为保证。
            Ok(LanceDbUpsertResult {
                message: "append completed".to_string(),
                version: table.version().await.map_err(map_lancedb_error)?,
                input_rows,
                inserted_rows: input_rows,
                updated_rows: 0,
                deleted_rows: 0,
            })
        } else {
            let keys = input
                .key_columns
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let mut merge = table.merge_insert(&keys);
            merge
                .when_matched_update_all(None)
                .when_not_matched_insert_all();

            let result = merge.execute(reader).await.map_err(map_lancedb_error)?;
            verify_table_format(&table, &table_name).await?;
            // The connection option is a request; this read-back is what makes it a guarantee for this path too.
            // 连接选项只是一个请求；这次读回使它在本文路径上同样成为保证。
            Ok(LanceDbUpsertResult {
                message: "merge upsert completed".to_string(),
                version: result.version,
                input_rows,
                inserted_rows: result.num_inserted_rows,
                updated_rows: result.num_updated_rows,
                deleted_rows: result.num_deleted_rows,
            })
        }
    }

    /// Maintain a scalar merge key index without scanning the complete target on every upsert.
    /// 维护标量合并键索引，避免每次更新插入都扫描完整目标。
    /// `table_name` and `column` are explicit caller-owned identities; returns after bounded-tail maintenance.
    /// `table_name` 和 `column` 是调用方明确持有的身份；有界尾部维护完成后返回。
    pub async fn maintain_merge_key_index(
        &self,
        table_name: &str,
        column: &str,
    ) -> EngineResult<()> {
        // Serialize index metadata changes with writes to the same physical table.
        // 将索引元数据变更与同一物理表写入串行化。
        let table_name = normalize_table_name(table_name);
        validate_table_name(&table_name)?;
        let _table_guard = self.acquire_table_write(&table_name).await;
        let _permit = self.acquire_concurrency_permit().await?;
        let table = self.open_table(&table_name).await?;
        let indices = table.list_indices().await.map_err(map_lancedb_error)?;
        let existing = indices.iter().find(|index| {
            index.columns == [column] && index.index_type == lancedb::index::IndexType::BTree
        });
        if let Some(index) = existing {
            // Index statistics count the uncovered tail explicitly; missing statistics are an error.
            // 索引统计明确计算未覆盖尾部；统计缺失视为错误。
            let statistics = table
                .index_stats(&index.name)
                .await
                .map_err(map_lancedb_error)?
                .ok_or_else(|| {
                    LanceDbEngineError::internal("merge key index statistics disappeared")
                })?;
            if statistics.num_unindexed_rows >= 4096 {
                table
                    .optimize(lancedb::table::OptimizeAction::Index(Default::default()))
                    .await
                    .map_err(map_lancedb_error)?;
            }
        } else if table.count_rows(None).await.map_err(map_lancedb_error)? != 0 {
            // The first nonempty batch seeds the index; subsequent merges scan only an at-most-4096-row tail.
            // 首个非空批次建立索引；后续合并仅扫描最多 4096 行的尾部。
            table
                .create_index(&[column], lancedb::index::Index::BTree(Default::default()))
                .execute()
                .await
                .map_err(map_lancedb_error)?;
        }
        Ok(())
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
        // Canonical physical table identity selects its shared lock.
        // 规范物理表身份选择其共享锁。
        let table_name = normalize_table_name(&input.table_name);
        validate_table_name(&table_name)?;
        if table_name.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "table_name must not be empty",
            ));
        }
        if input.vector.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "vector must not be empty",
            ));
        }
        // Round 72 chose the engine as the authoritative boundary for non-finite reals; this is the same rule applied
        // to a vector. The dispatch already refuses a JSON component that overflows `f32`, but a Rust caller can
        // build `Vec<f32>` directly, and one non-finite component poisons every distance computed with it.
        // 第 72 轮把引擎选为非有限实数的权威边界；这是同一规则施加于向量。分派已拒绝溢出 `f32` 的 JSON 分量，
        // 但 Rust 调用方可以直接构造 `Vec<f32>`，而一个非有限分量就会污染用它计算的每个距离。
        if let Some((index, value)) = input
            .vector
            .iter()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(LanceDbEngineError::invalid_argument(format!(
                "vector[{index}] must be finite, but is {value}"
            )));
        }

        // The table guard is intentionally released before encoding so a long encode does not hold
        // the table barrier. Only the query itself needs it.
        // 表守卫刻意在编码前释放，使耗时的编码不会占用表屏障。只有查询本身需要它。
        let (batches, output_schema) = {
            // The read guard prevents concurrent table destruction during the query.
            // 读取守卫防止查询期间并发销毁表。
            let _table_guard = self.acquire_table_read(&table_name).await;
            // The global permit respects the configured engine concurrency ceiling.
            // 全局许可遵守已配置引擎并发上限。
            let _concurrency_permit = self.acquire_concurrency_permit().await?;
            // Open only the declared physical table.
            // 仅打开声明物理表。
            let table = self.open_table(&table_name).await?;
            // Freeze the requested distance before filters and execution.
            // 在过滤及执行之前冻结请求距离。
            let mut query = table
                .query()
                .nearest_to(input.vector.clone())
                .map_err(map_lancedb_error)?
                .distance_type(match metric {
                    LanceDbDistanceMetric::Euclidean => lancedb::DistanceType::L2,
                    LanceDbDistanceMetric::Cosine => lancedb::DistanceType::Cosine,
                    LanceDbDistanceMetric::DotProduct => lancedb::DistanceType::Dot,
                });

            if !input.vector_column.trim().is_empty() {
                query = query.column(input.vector_column.trim());
            }

            // Omission uses the engine default; explicit counts stay within its sole upper bound.
            // 省略使用引擎默认值；显式数量保持在唯一上限内。
            let limit = if input.limit == 0 {
                DEFAULT_SEARCH_LIMIT
            } else {
                (input.limit as usize).min(self.state.options.max_search_limit)
            };
            query = query.limit(limit);

            if !input.filter.trim().is_empty() {
                query = query.only_if(input.filter.trim());
            }

            // Capture the exact projected schema while the table guard is active.
            // 表守卫生效期间捕获精确投影结构。
            let output_schema = query.output_schema().await.map_err(map_lancedb_error)?;
            // The native query owns distance computation and row ordering.
            // 原生查询拥有距离计算及行顺序。
            let stream = query.execute().await.map_err(map_lancedb_error)?;
            // Collect native batches before releasing the table guard.
            // 释放表守卫前收集原生批次。
            let batches: Vec<RecordBatch> =
                stream.try_collect().await.map_err(map_lancedb_error)?;
            (batches, output_schema)
        };

        // Count logical rows independently of output encoding.
        // 独立于输出编码统计逻辑行。
        let rows = count_rows(&batches);
        // Resolve only the explicitly supported wire encoding.
        // 仅解析明确支持的线协议编码。
        let output_format = normalize_output_format(input.output_format);
        // Move the projected schema into the bounded encoding task.
        // 将投影结构移入有界编码任务。
        let schema = output_schema.clone();
        // The encoding task owns its batch handles without retaining a table lock.
        // 编码任务拥有批次句柄，不保留表锁。
        let encoded = batches.clone();
        // Offload CPU encoding and surface join or serialization failures.
        // 将编码计算移出异步线程，并暴露连接或序列化失败。
        let data = tokio::task::spawn_blocking(move || match output_format {
            LanceDbOutputFormat::JsonRows => encode_batches_as_json(&schema, &encoded),
            LanceDbOutputFormat::Unspecified | LanceDbOutputFormat::ArrowIpc => {
                encode_batches_as_arrow_ipc(&schema, &encoded)
            }
        })
        .await
        .map_err(|error| {
            LanceDbEngineError::internal(format!("encoding task panicked: {error}"))
        })??;

        Ok(LanceDbSearchResult {
            message: "search completed".to_string(),
            format: output_format,
            rows,
            data,
        })
    }

    /// Deletes rows by predicate.
    /// 按谓词删除行。
    pub async fn delete(&self, input: LanceDbDeleteInput) -> EngineResult<LanceDbDeleteResult> {
        let table_name = normalize_table_name(&input.table_name);
        validate_table_name(&table_name)?;
        if table_name.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "table_name must not be empty",
            ));
        }
        if input.condition.trim().is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "condition must not be empty",
            ));
        }

        let _table_guard = self.acquire_table_write(&table_name).await;
        let _concurrency_permit = self.acquire_concurrency_permit().await?;
        let table = self.open_table(&table_name).await?;
        let result = table
            .delete(input.condition.trim())
            .await
            .map_err(map_lancedb_error)?;

        Ok(LanceDbDeleteResult {
            message: format!("delete completed for '{table_name}'"),
            version: result.version,
            deleted_rows: result.num_deleted_rows,
        })
    }

    /// Drops a whole table.
    /// 删除整张表。
    pub async fn drop_table(
        &self,
        input: LanceDbDropTableInput,
    ) -> EngineResult<LanceDbDropTableResult> {
        let table_name = normalize_table_name(&input.table_name);
        validate_table_name(&table_name)?;
        if table_name.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "table_name must not be empty",
            ));
        }

        let _table_guard = self.acquire_table_write(&table_name).await;
        let _concurrency_permit = self.acquire_concurrency_permit().await?;
        self.state
            .db
            .drop_table(table_name.clone(), &[])
            .await
            .map_err(map_lancedb_error)?;

        Ok(LanceDbDropTableResult {
            message: format!("table '{table_name}' dropped"),
        })
    }

    /// Reads authoritative physical statistics for one table.
    /// 读取一张表的权威物理统计信息。
    pub async fn table_statistics(
        &self,
        input: LanceDbTableStatisticsInput,
    ) -> EngineResult<LanceDbTableStatisticsResult> {
        let table_name = normalize_table_name(&input.table_name);
        validate_table_name(&table_name)?;
        if table_name.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "table_name must not be empty",
            ));
        }

        validate_compaction_options(&input.compaction)?;

        let _table_guard = self.acquire_table_read(&table_name).await;
        let _concurrency_permit = self.acquire_concurrency_permit().await?;
        let table = self.open_table(&table_name).await?;
        collect_table_statistics(&table, &table_name, &input.compaction).await
    }

    /// Compacts small physical files without pruning historical table versions.
    /// 合并小型物理文件，同时不裁剪历史表版本。
    pub async fn compact_table(
        &self,
        input: LanceDbCompactTableInput,
    ) -> EngineResult<LanceDbCompactTableResult> {
        let table_name = normalize_table_name(&input.table_name);
        validate_table_name(&table_name)?;
        if table_name.is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "table_name must not be empty",
            ));
        }
        validate_compaction_options(&input.options)?;

        let _table_guard = self.acquire_table_compaction(&table_name).await;
        let _concurrency_permit = self.acquire_concurrency_permit().await?;
        let table = self.open_table(&table_name).await?;
        let before = collect_table_statistics(&table, &table_name, &input.options).await?;

        // Optimistic concurrency: refuse to compact a table that moved since the caller looked.
        // 乐观并发：若表在调用方观察之后已变化，则拒绝整理。
        if let Some(expected_version) = input.expected_version
            && before.version != expected_version
        {
            return Err(LanceDbEngineError::invalid_argument(format!(
                "table version changed: expected {expected_version}, found {}",
                before.version
            )));
        }

        let options = native_compaction_options(&input.options);

        // Native compaction never merges fragments covered by different index sets, so the
        // unindexed tail left by incremental writes must join the indexes first; otherwise an
        // indexed table can never converge to one small fragment.
        // 原生整理绝不合并索引覆盖不同的片段，因此增量写入留下的未索引尾部必须先并入索引；
        // 否则带索引的表永远无法收敛为单个小片段。
        fold_unindexed_rows_into_indexes(&table).await?;

        #[cfg(test)]
        let _native_compaction_observation =
            NativeCompactionObservationGuard::begin(&self.state.native_compaction_active);

        let optimize = table
            .optimize(OptimizeAction::Compact {
                options,
                remap_options: None,
            })
            .await
            .map_err(map_lancedb_error)?;
        let native_metrics = optimize.compaction.unwrap_or_default();
        let after = collect_table_statistics(&table, &table_name, &input.options).await?;
        let metrics = LanceDbCompactionMetrics {
            fragments_removed: native_metrics.fragments_removed as u64,
            fragments_added: native_metrics.fragments_added as u64,
            files_removed: native_metrics.files_removed as u64,
            files_added: native_metrics.files_added as u64,
        };
        validate_compaction_transition(&before, &after, &metrics)?;

        verify_table_format(&table, &table_name).await?;
        // The connection option is a request; this read-back is what makes it a guarantee for this path too.
        // 连接选项只是一个请求；这次读回使它在本文路径上同样成为保证。
        Ok(LanceDbCompactTableResult {
            message: format!("table '{table_name}' compaction completed"),
            before,
            after,
            metrics,
        })
    }

    /// Returns the engine options.
    /// 返回引擎选项。
    pub fn options(&self) -> &LanceDbEngineOptions {
        &self.state.options
    }

    /// Acquires a table-level read guard.
    /// 获取表级读守卫。
    async fn acquire_table_read(&self, table_name: &str) -> OwnedRwLockReadGuard<()> {
        self.state.table_access.acquire_read(table_name).await
    }

    /// Acquires a table-level write guard.
    /// 获取表级写守卫。
    async fn acquire_table_write(&self, table_name: &str) -> OwnedRwLockWriteGuard<()> {
        self.state.table_access.acquire_write(table_name).await
    }

    /// Acquires exclusive table compaction while preserving snapshot-read concurrency.
    /// 获取表级独占整理权限，同时保留快照读取并发。
    async fn acquire_table_compaction(&self, table_name: &str) -> TableCompactionGuard {
        self.state.table_access.acquire_compaction(table_name).await
    }

    /// Acquires one concurrency permit.
    /// 获取一个并发许可。
    async fn acquire_concurrency_permit(&self) -> EngineResult<OwnedSemaphorePermit> {
        self.state
            .concurrency_limiter
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| LanceDbEngineError::internal("engine is shutting down"))
    }

    /// Opens a table handle.
    /// 打开表句柄。
    async fn open_table(&self, table_name: &str) -> EngineResult<Table> {
        self.state
            .db
            .open_table(table_name.to_string())
            .execute()
            .await
            .map_err(map_lancedb_error)
    }
}

/// Test-only RAII marker covering the exact native compaction await boundary.
/// 仅供测试使用的 RAII 标记，覆盖精确的原生整理等待边界。
#[cfg(test)]
struct NativeCompactionObservationGuard<'a> {
    /// Engine-local flag reset even when the future is cancelled.
    /// 即使 Future 被取消也会复位的引擎局部标记。
    active: &'a std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl<'a> NativeCompactionObservationGuard<'a> {
    /// Marks native compaction active until the guard drops.
    /// 将原生整理标记为活动，直到守卫被释放。
    fn begin(active: &'a std::sync::atomic::AtomicBool) -> Self {
        active.store(true, std::sync::atomic::Ordering::SeqCst);
        Self { active }
    }
}

#[cfg(test)]
impl Drop for NativeCompactionObservationGuard<'_> {
    /// Clears the observation marker on every exit path.
    /// 在所有退出路径上清除观察标记。
    fn drop(&mut self) {
        self.active
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Brings every index up to date when any of them leaves rows unindexed.
/// 任一索引存在未索引行时，将全部索引更新到最新。
///
/// Index optimization rewrites index files only; rows, fragments and the index set stay unchanged,
/// so the compaction transition checks still hold. Missing index statistics are an error.
/// 索引优化只重写索引文件；行、片段与索引集合保持不变，整理转换校验仍然成立。索引统计缺失视为错误。
async fn fold_unindexed_rows_into_indexes(table: &Table) -> EngineResult<()> {
    for index in table.list_indices().await.map_err(map_lancedb_error)? {
        let statistics = table
            .index_stats(&index.name)
            .await
            .map_err(map_lancedb_error)?
            .ok_or_else(|| LanceDbEngineError::internal("index statistics disappeared"))?;
        if statistics.num_unindexed_rows > 0 {
            table
                .optimize(OptimizeAction::Index(Default::default()))
                .await
                .map_err(map_lancedb_error)?;
            return Ok(());
        }
    }
    Ok(())
}

/// Translates bounded caller options into the one native option set used to plan and to compact.
/// 将有界调用方参数转换为计划与整理共用的唯一原生参数集。
fn native_compaction_options(input: &LanceDbCompactionOptions) -> CompactionOptions {
    let mut options = CompactionOptions::default();
    if let Some(value) = input.target_rows_per_fragment {
        options.target_rows_per_fragment = value;
    }
    if let Some(value) = input.max_rows_per_group {
        options.max_rows_per_group = value;
    }
    options.max_bytes_per_file = input.max_bytes_per_file;
    options.num_threads = input.num_threads;
    options.batch_size = input.batch_size;
    options.max_source_fragments = input.max_source_fragments;
    // Materializing deletions would physically rewrite rows, which is out of scope for a v1
    // layout-only compaction.
    // 物化删除会物理重写行，超出 v1 仅整理布局的范围。
    options.materialize_deletions = false;
    options
}

/// Collects one table's physical statistics, sizing the native compaction plan with `compaction`.
/// 收集一张表的物理统计信息，并以 `compaction` 确定原生整理计划规模。
///
/// The task count comes from the same planner that compaction executes, so "needs compaction"
/// and "compaction would do something" can never disagree.
/// 任务数来自整理实际执行的同一计划器，因此“需要整理”与“整理会有动作”永远一致。
async fn collect_table_statistics(
    table: &Table,
    table_name: &str,
    compaction: &LanceDbCompactionOptions,
) -> EngineResult<LanceDbTableStatisticsResult> {
    let version = table.version().await.map_err(map_lancedb_error)?;
    let statistics = table.stats().await.map_err(map_lancedb_error)?;
    let dataset = table
        .dataset()
        .ok_or_else(|| LanceDbEngineError::internal("compaction planning requires a native table"))?
        .get()
        .await
        .map_err(map_lancedb_error)?;
    let plan =
        lance::dataset::optimize::plan_compaction(&dataset, &native_compaction_options(compaction))
            .await
            .map_err(|error| map_lancedb_error(error.into()))?;

    Ok(LanceDbTableStatisticsResult {
        table_name: table_name.to_string(),
        version,
        total_bytes: statistics.total_bytes as u64,
        row_count: statistics.num_rows as u64,
        index_count: statistics.num_indices as u64,
        fragment_count: statistics.fragment_stats.num_fragments as u64,
        small_fragment_count: statistics.fragment_stats.num_small_fragments as u64,
        compaction_task_count: plan.num_tasks() as u64,
        fragment_rows: LanceDbFragmentSummary {
            minimum_rows: statistics.fragment_stats.lengths.min as u64,
            maximum_rows: statistics.fragment_stats.lengths.max as u64,
            mean_rows: statistics.fragment_stats.lengths.mean as u64,
            p25_rows: statistics.fragment_stats.lengths.p25 as u64,
            p50_rows: statistics.fragment_stats.lengths.p50 as u64,
            p75_rows: statistics.fragment_stats.lengths.p75 as u64,
            p99_rows: statistics.fragment_stats.lengths.p99 as u64,
        },
    })
}

/// Validates compaction overrides before any planning or table mutation starts.
/// 在任何计划或表变更开始前校验整理参数覆盖值。
fn validate_compaction_options(input: &LanceDbCompactionOptions) -> EngineResult<()> {
    let positive_values = [
        ("target_rows_per_fragment", input.target_rows_per_fragment),
        ("max_rows_per_group", input.max_rows_per_group),
        ("max_bytes_per_file", input.max_bytes_per_file),
        ("num_threads", input.num_threads),
        ("batch_size", input.batch_size),
        ("max_source_fragments", input.max_source_fragments),
    ];
    for (name, value) in positive_values {
        if value == Some(0) {
            return Err(LanceDbEngineError::invalid_argument(format!(
                "{name} must be greater than zero"
            )));
        }
    }

    if input
        .num_threads
        .is_some_and(|value| value > MAX_COMPACTION_THREADS)
    {
        return Err(LanceDbEngineError::invalid_argument(format!(
            "num_threads must not exceed {MAX_COMPACTION_THREADS}"
        )));
    }

    Ok(())
}

/// Validates that a compaction changed only physical layout.
/// 校验一次整理只改变了物理布局。
///
/// `before` and `after` are authoritative snapshots surrounding one native operation, while
/// `metrics` is Lance's own report for it. Any logical data loss, index loss, identity drift,
/// version regression or contradictory fragment delta is an internal error: compaction is a
/// layout operation, so observing a logical change means an assumption is wrong and the caller
/// must not be told the operation succeeded.
///
/// `before` 与 `after` 是围绕一次原生操作的权威快照，`metrics` 是 Lance 自身的报告。任何逻辑数据
/// 丢失、索引丢失、身份漂移、版本倒退或片段增量矛盾都按内部错误处理：整理是布局操作，观测到逻辑变化
/// 说明某个假设不成立，且不得向调用方报告操作成功。
fn validate_compaction_transition(
    before: &LanceDbTableStatisticsResult,
    after: &LanceDbTableStatisticsResult,
    metrics: &LanceDbCompactionMetrics,
) -> EngineResult<()> {
    if after.table_name != before.table_name || after.version < before.version {
        return Err(LanceDbEngineError::internal(
            "native compaction changed table identity or regressed its version",
        ));
    }

    if after.row_count != before.row_count || after.index_count != before.index_count {
        return Err(LanceDbEngineError::internal(
            "native compaction changed logical rows or physical indexes",
        ));
    }

    let expected_fragments = before
        .fragment_count
        .checked_sub(metrics.fragments_removed)
        .and_then(|count| count.checked_add(metrics.fragments_added))
        .ok_or_else(|| {
            LanceDbEngineError::internal("native compaction fragment metrics are inconsistent")
        })?;

    if after.fragment_count != expected_fragments {
        return Err(LanceDbEngineError::internal(
            "native compaction fragment metrics do not match table statistics",
        ));
    }

    Ok(())
}

/// Validates that a table identifier is a single safe path segment.
/// 校验表标识为安全的单个路径段。
///
/// This is the rule the database manager applies to database names, applied to tables: `lancedb` resolves a table
/// name to `<database root>/<name>.lance`, so a name carrying a separator or a traversal segment addresses storage
/// outside the database the caller was authorized for. Host callers derive table names, but the LuaSkills actions
/// accept them from a skill, which makes this a boundary rather than a formality. Separators are refused before
/// anything else so the message names the real problem, and because on Windows a backslash is a separator that a
/// component walk would only recognize after platform-specific normalization.
/// 这是数据库管理器对数据库名施加的规则在表上的应用：`lancedb` 把表名解析为 `<database root>/<name>.lance`，
/// 因此带分隔符或穿越段的名称会寻址到调用方未被授权的那部分存储之外。宿主调用方派生表名，但 LuaSkills 动作会从
/// 技能接受表名，这使本校验成为边界而非形式。分隔符最先被拒绝，使消息指出真正的问题；同时因为在 Windows 上
/// 反斜杠是分隔符，而组件遍历只在平台相关规范化之后才会如此处理。
fn validate_table_name(table_name: &str) -> EngineResult<()> {
    if table_name.contains('/') || table_name.contains('\\') {
        return Err(LanceDbEngineError::path_not_authorized(format!(
            "table `{table_name}` must not contain path separators"
        )));
    }
    if table_name == "." || table_name == ".." {
        return Err(LanceDbEngineError::path_not_authorized(format!(
            "table `{table_name}` must not be a relative path segment"
        )));
    }
    Ok(())
}
/// Normalizes a table name into the form used against LanceDB itself.
/// 把表名规范化为对 LanceDB 本身使用的形式。
///
/// Only surrounding whitespace is removed. The name is otherwise passed through exactly as the caller wrote it,
/// because rewriting it would change which table the caller addresses.
/// 仅移除首尾空白。除此之外表名按调用方所写原样传递，因为改写它会改变调用方寻址的是哪张表。
fn normalize_table_name(table_name: &str) -> String {
    table_name.trim().to_string()
}

/// Derives the synchronization identity for one table name.
/// 为一份表名派生同步身份。
///
/// **This is deliberately not [`normalize_table_name`].** LanceDB resolves a table name to `<database root>/<name>.lance`
/// and its identity check is **case-insensitive**, while a `HashMap` key is not: measured on this build,
/// `create_table("CaseProbe")` followed by `create_table("caseprobe")` fails with `Table 'caseprobe' already exists`,
/// and `table_statistics` returns the same table for `CaseProbe`, `caseprobe` and `CASEPROBE`. Case folding here is
/// therefore what makes the lock set match the table — without it, a write through one casing and a read through
/// another would take **unrelated** locks, and two compactions through two casings would run on one table at once,
/// which is exactly what `compaction_gate` exists to prevent.
///
/// **Why the table name itself is still not folded.** Folding the name handed to LanceDB would be wrong on a
/// case-sensitive filesystem, where `CaseProbe` and `caseprobe` really are two tables: it would silently merge them.
/// Folding only the lock key can therefore err in one direction — on such a filesystem two distinct tables share one
/// lock and merely serialize against each other — and never in the other. Over-serialization is safe; under-
/// serialization is not.
///
/// **本函数刻意不复用 [`normalize_table_name`]。** LanceDB 把表名解析为 `<database root>/<name>.lance`，而其身份判定
/// **不区分大小写**，而 `HashMap` 的键区分：在本构建上实测，先 `create_table("CaseProbe")` 再
/// `create_table("caseprobe")` 会以 `Table 'caseprobe' already exists` 失败，且 `table_statistics` 对 `CaseProbe`、
/// `caseprobe` 与 `CASEPROBE` 返回同一张表。因此此处的大小写折叠正是让锁集合与表对齐的东西——没有它，经一种大小写的
/// 写入与经另一种的读取会取得**互不相关**的锁，而经两种大小写的两次整理会在同一张表上同时运行，而那恰是
/// `compaction_gate` 存在的目的。
///
/// **为何表名本身仍然不折叠。** 折叠交给 LanceDB 的名字在大小写敏感的文件系统上是错的，那里 `CaseProbe` 与
/// `caseprobe` 确实是两张表：那会静默把它们合并。因此只折叠锁键的错法只有一个方向——在那种文件系统上两张不同的表共用
/// 一把锁、仅仅是彼此串行化——而绝不会是另一个方向。过度串行化是安全的；串行化不足则不是。
fn lock_identity(table_name: &str) -> String {
    normalize_table_name(table_name).to_lowercase()
}

/// Builds the Arrow schema for a table definition.
/// 为表定义构建 Arrow Schema。
fn build_arrow_schema(columns: &[LanceDbColumnDef]) -> EngineResult<SchemaRef> {
    let mut fields = Vec::with_capacity(columns.len());
    let mut names = HashSet::with_capacity(columns.len());
    for column in columns {
        if column.name.trim().is_empty() {
            return Err(LanceDbEngineError::invalid_argument(
                "column name must not be empty",
            ));
        }
        if !names.insert(column.name.as_str()) {
            return Err(LanceDbEngineError::invalid_argument(format!(
                "duplicate column name '{}' is not allowed",
                column.name
            )));
        }
        fields.push(column_to_field(column)?);
    }

    Ok(Arc::new(Schema::new(fields)))
}

/// Converts one column definition into an Arrow field.
/// 将一个列定义转换为 Arrow Field。
fn column_to_field(column: &LanceDbColumnDef) -> EngineResult<Field> {
    let data_type = match column.column_type {
        LanceDbColumnType::String => DataType::Utf8,
        LanceDbColumnType::Int64 => DataType::Int64,
        LanceDbColumnType::Float64 => DataType::Float64,
        LanceDbColumnType::Bool => DataType::Boolean,
        LanceDbColumnType::Float32 => DataType::Float32,
        LanceDbColumnType::Uint64 => DataType::UInt64,
        LanceDbColumnType::Int32 => DataType::Int32,
        LanceDbColumnType::Uint32 => DataType::UInt32,
        LanceDbColumnType::VectorFloat32 => {
            if column.vector_dim == 0 {
                return Err(LanceDbEngineError::invalid_argument(format!(
                    "vector column '{}' must have vector_dim > 0",
                    column.name
                )));
            }
            let vector_dim = i32::try_from(column.vector_dim).map_err(|_| {
                LanceDbEngineError::invalid_argument(format!(
                    "vector column '{}' dimension {} exceeds Arrow's signed dimension range",
                    column.name, column.vector_dim
                ))
            })?;
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim,
            )
        }
        LanceDbColumnType::Unspecified => {
            return Err(LanceDbEngineError::invalid_argument(format!(
                "column '{}' has unspecified type",
                column.name
            )));
        }
    };

    Ok(Field::new(&column.name, data_type, column.nullable))
}

/// Decodes an input payload into record batches.
/// 将输入载荷解码为记录批次。
fn decode_input_to_batches(
    format: LanceDbInputFormat,
    data: &[u8],
    schema: SchemaRef,
) -> EngineResult<(Vec<RecordBatch>, u64)> {
    match format {
        LanceDbInputFormat::JsonRows | LanceDbInputFormat::Unspecified => {
            decode_json_rows_to_batches(data, schema)
        }
        LanceDbInputFormat::ArrowIpc => decode_arrow_ipc_to_batches(data, &schema),
    }
}

/// Decodes an Arrow IPC payload into record batches.
/// 将 Arrow IPC 载荷解码为记录批次。
fn decode_arrow_ipc_to_batches(
    data: &[u8],
    expected_schema: &SchemaRef,
) -> EngineResult<(Vec<RecordBatch>, u64)> {
    let mut reader =
        StreamReader::try_new(Cursor::new(data.to_vec()), None).map_err(map_external_error)?;
    if reader.schema().as_ref() != expected_schema.as_ref() {
        return Err(LanceDbEngineError::invalid_argument(format!(
            "Arrow IPC schema does not match the target table schema: expected {:?}, received {:?}",
            expected_schema,
            reader.schema()
        )));
    }
    let mut batches = Vec::new();
    let mut rows = 0_u64;

    for batch in &mut reader {
        let batch = batch.map_err(map_external_error)?;
        rows += batch.num_rows() as u64;
        batches.push(batch);
    }

    Ok((batches, rows))
}

/// Decodes a JSON row array into record batches.
/// 将 JSON 行数组解码为记录批次。
fn decode_json_rows_to_batches(
    data: &[u8],
    schema: SchemaRef,
) -> EngineResult<(Vec<RecordBatch>, u64)> {
    let rows: Vec<Value> = if data.is_empty() {
        Vec::new()
    } else {
        serde_json::from_slice(data).map_err(|error| {
            LanceDbEngineError::invalid_argument(format!(
                "failed to parse JSON rows, expected a JSON array of objects: {error}"
            ))
        })?
    };

    let batch = json_rows_to_record_batch(&rows, schema)?;
    let row_count = batch.num_rows() as u64;
    Ok((vec![batch], row_count))
}

/// Converts JSON rows into one record batch.
/// 将 JSON 行转换为一个记录批次。
fn json_rows_to_record_batch(rows: &[Value], schema: SchemaRef) -> EngineResult<RecordBatch> {
    let mut arrays = Vec::<ArrayRef>::with_capacity(schema.fields().len());

    for field in schema.fields() {
        arrays.push(build_array_for_field(
            rows,
            field.name(),
            field.data_type(),
            field.is_nullable(),
        )?);
    }

    RecordBatch::try_new(schema, arrays).map_err(map_external_error)
}

/// Builds the Arrow array for one schema field from JSON rows.
/// 依据 JSON 行为一个 Schema 字段构建 Arrow 数组。
fn build_array_for_field(
    rows: &[Value],
    field_name: &str,
    data_type: &DataType,
    nullable: bool,
) -> EngineResult<ArrayRef> {
    macro_rules! build_primitive {
        ($builder:expr, $extract:expr) => {{
            let mut builder = $builder;
            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => builder.append_value($extract(value, field_name)?),
                    None => builder.append_null(),
                }
            }
            Ok(Arc::new(builder.finish()) as ArrayRef)
        }};
    }

    match data_type {
        DataType::Utf8 => build_primitive!(StringBuilder::new(), |value: &Value, name: &str| {
            expect_string(value, name).map(str::to_string)
        }),
        DataType::LargeUtf8 => {
            build_primitive!(LargeStringBuilder::new(), |value: &Value, name: &str| {
                expect_string(value, name).map(str::to_string)
            })
        }
        DataType::Int64 => build_primitive!(Int64Builder::with_capacity(rows.len()), expect_i64),
        DataType::Int32 => build_primitive!(Int32Builder::with_capacity(rows.len()), expect_i32),
        DataType::UInt64 => build_primitive!(UInt64Builder::with_capacity(rows.len()), expect_u64),
        DataType::UInt32 => build_primitive!(UInt32Builder::with_capacity(rows.len()), expect_u32),
        DataType::Float64 => {
            build_primitive!(Float64Builder::with_capacity(rows.len()), expect_f64)
        }
        DataType::Float32 => {
            build_primitive!(Float32Builder::with_capacity(rows.len()), expect_f32)
        }
        DataType::Boolean => {
            build_primitive!(BooleanBuilder::with_capacity(rows.len()), expect_bool)
        }
        DataType::FixedSizeList(child, dim) if child.data_type() == &DataType::Float32 => {
            let dim = *dim;
            let mut builder = FixedSizeListBuilder::with_capacity(
                Float32Builder::with_capacity(rows.len() * (dim as usize)),
                dim,
                rows.len(),
            );

            for row in rows {
                match extract_field_value(row, field_name, nullable)? {
                    Some(value) => {
                        let array = value.as_array().ok_or_else(|| {
                            LanceDbEngineError::invalid_argument(format!(
                                "field '{field_name}' must be a JSON array of float32 values"
                            ))
                        })?;
                        if array.len() != dim as usize {
                            return Err(LanceDbEngineError::invalid_argument(format!(
                                "field '{field_name}' length mismatch: expected {dim}, got {}",
                                array.len()
                            )));
                        }
                        for item in array {
                            builder.values().append_value(expect_f32(item, field_name)?);
                        }
                        builder.append(true);
                    }
                    None => {
                        for _ in 0..dim {
                            builder.values().append_null();
                        }
                        builder.append(false);
                    }
                }
            }

            Ok(Arc::new(builder.finish()) as ArrayRef)
        }
        other => Err(LanceDbEngineError::invalid_argument(format!(
            "unsupported field type for JSON ingestion on '{field_name}': {other:?}"
        ))),
    }
}

/// Refuses any non-finite value in a batch's float columns, naming the column and the position.
/// 拒绝批次浮点列中的任何非有限值，并给出列名与位置。
///
/// Float columns are the only place a decoded value can be non-finite, because JSON numbers are finite and it is the
/// narrowing to `f32` that can make one infinite. Lists (vector columns) are walked through to their element array.
/// 浮点列是解码后取值可能非有限的唯一之处，因为 JSON 数字本身有限，而是窄化为 `f32` 可能使其成为无穷。列表
/// （向量列）会被遍历到其元素数组。
fn refuse_non_finite_floats(batch: &RecordBatch, table_name: &str) -> EngineResult<()> {
    let refuse = |column: &str, position: usize, value: f64| -> EngineResult<()> {
        if value.is_finite() {
            return Ok(());
        }
        Err(LanceDbEngineError::invalid_argument(format!(
            "column `{column}` position {position} of table `{table_name}` holds a non-finite value ({value})"
        )))
    };

    for (index, field) in batch.schema().fields().iter().enumerate() {
        let column = batch.column(index);
        if let Some(values) = column.as_any().downcast_ref::<arrow_array::Float32Array>() {
            for (position, value) in values.iter().enumerate() {
                if let Some(value) = value {
                    refuse(field.name(), position, f64::from(value))?;
                }
            }
        } else if let Some(values) = column.as_any().downcast_ref::<arrow_array::Float64Array>() {
            for (position, value) in values.iter().enumerate() {
                if let Some(value) = value {
                    refuse(field.name(), position, value)?;
                }
            }
        } else if let Some(list) = column
            .as_any()
            .downcast_ref::<arrow_array::FixedSizeListArray>()
        {
            let elements = list.values();
            if let Some(values) = elements
                .as_any()
                .downcast_ref::<arrow_array::Float32Array>()
            {
                for (position, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        refuse(field.name(), position, f64::from(value))?;
                    }
                }
            } else if let Some(values) = elements
                .as_any()
                .downcast_ref::<arrow_array::Float64Array>()
            {
                for (position, value) in values.iter().enumerate() {
                    if let Some(value) = value {
                        refuse(field.name(), position, value)?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Extracts one field value from a JSON row object, honouring nullability.
/// 从一个 JSON 行对象中提取字段值，并遵循可空性。
fn extract_field_value<'a>(
    row: &'a Value,
    field_name: &str,
    nullable: bool,
) -> EngineResult<Option<&'a Value>> {
    let object = row.as_object().ok_or_else(|| {
        LanceDbEngineError::invalid_argument("JSON rows must be an array of JSON objects")
    })?;

    match object.get(field_name) {
        Some(Value::Null) | None => {
            if nullable {
                Ok(None)
            } else {
                Err(LanceDbEngineError::invalid_argument(format!(
                    "field '{field_name}' is missing or null and is not nullable"
                )))
            }
        }
        Some(value) => Ok(Some(value)),
    }
}

/// Expects a JSON string.
/// 期望 JSON 字符串。
fn expect_string<'a>(value: &'a Value, field_name: &str) -> EngineResult<&'a str> {
    value.as_str().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be a string"))
    })
}

/// Expects a JSON int64.
/// 期望 JSON int64。
fn expect_i64(value: &Value, field_name: &str) -> EngineResult<i64> {
    value.as_i64().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be an int64"))
    })
}

/// Expects a JSON int32.
/// 期望 JSON int32。
fn expect_i32(value: &Value, field_name: &str) -> EngineResult<i32> {
    let raw = value.as_i64().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be an int32"))
    })?;
    i32::try_from(raw).map_err(|_| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' is out of int32 range"))
    })
}

/// Expects a JSON uint64.
/// 期望 JSON uint64。
fn expect_u64(value: &Value, field_name: &str) -> EngineResult<u64> {
    value.as_u64().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be a uint64"))
    })
}

/// Expects a JSON uint32.
/// 期望 JSON uint32。
fn expect_u32(value: &Value, field_name: &str) -> EngineResult<u32> {
    let raw = value.as_u64().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be a uint32"))
    })?;
    u32::try_from(raw).map_err(|_| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' is out of uint32 range"))
    })
}

/// Expects a JSON float64.
/// 期望 JSON float64。
fn expect_f64(value: &Value, field_name: &str) -> EngineResult<f64> {
    value.as_f64().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be a float64"))
    })
}

/// Expects a JSON float32.
/// 期望 JSON float32。
fn expect_f32(value: &Value, field_name: &str) -> EngineResult<f32> {
    let raw = value.as_f64().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be a float32"))
    })?;
    Ok(raw as f32)
}

/// Expects a JSON bool.
/// 期望 JSON 布尔值。
fn expect_bool(value: &Value, field_name: &str) -> EngineResult<bool> {
    value.as_bool().ok_or_else(|| {
        LanceDbEngineError::invalid_argument(format!("field '{field_name}' must be a bool"))
    })
}

/// Encodes record batches as an Arrow IPC stream.
/// 将记录批次编码为 Arrow IPC 流。
fn encode_batches_as_arrow_ipc(
    schema: &SchemaRef,
    batches: &[RecordBatch],
) -> EngineResult<Vec<u8>> {
    let mut writer =
        StreamWriter::try_new(Vec::<u8>::new(), schema.as_ref()).map_err(map_external_error)?;
    for batch in batches {
        writer.write(batch).map_err(map_external_error)?;
    }
    writer.finish().map_err(map_external_error)?;
    writer.into_inner().map_err(map_external_error)
}

/// Encodes record batches as a JSON array of row objects.
/// 将记录批次编码为行对象的 JSON 数组。
fn encode_batches_as_json(schema: &SchemaRef, batches: &[RecordBatch]) -> EngineResult<Vec<u8>> {
    let mut rows = Vec::<Value>::new();
    for batch in batches {
        for row_idx in 0..batch.num_rows() {
            let mut object = Map::<String, Value>::new();
            for (col_idx, field) in schema.fields().iter().enumerate() {
                object.insert(
                    field.name().clone(),
                    json_value_from_array(batch.column(col_idx), row_idx)?,
                );
            }
            rows.push(Value::Object(object));
        }
    }

    serde_json::to_vec(&rows).map_err(map_external_error)
}

/// Extracts one JSON value from an Arrow array cell.
/// 从 Arrow 数组单元格中提取一个 JSON 值。
fn json_value_from_array(array: &ArrayRef, row_idx: usize) -> EngineResult<Value> {
    if array.is_null(row_idx) {
        return Ok(Value::Null);
    }

    macro_rules! downcast {
        ($ty:ty, $label:literal) => {
            array.as_any().downcast_ref::<$ty>().ok_or_else(|| {
                LanceDbEngineError::internal(format!("failed to downcast {} array", $label))
            })?
        };
    }

    match array.data_type() {
        DataType::Utf8 => Ok(Value::String(
            downcast!(StringArray, "Utf8").value(row_idx).to_string(),
        )),
        DataType::LargeUtf8 => Ok(Value::String(
            downcast!(LargeStringArray, "LargeUtf8")
                .value(row_idx)
                .to_string(),
        )),
        DataType::Int64 => Ok(Value::from(downcast!(Int64Array, "Int64").value(row_idx))),
        DataType::Int32 => Ok(Value::from(downcast!(Int32Array, "Int32").value(row_idx))),
        DataType::UInt64 => Ok(Value::from(downcast!(UInt64Array, "UInt64").value(row_idx))),
        DataType::UInt32 => Ok(Value::from(downcast!(UInt32Array, "UInt32").value(row_idx))),
        DataType::Float64 => Ok(Value::from(
            downcast!(Float64Array, "Float64").value(row_idx),
        )),
        DataType::Float32 => Ok(Value::from(
            downcast!(Float32Array, "Float32").value(row_idx) as f64,
        )),
        DataType::Boolean => Ok(Value::from(
            downcast!(BooleanArray, "Boolean").value(row_idx),
        )),
        DataType::FixedSizeList(child, _) if child.data_type() == &DataType::Float32 => {
            let list = downcast!(FixedSizeListArray, "FixedSizeList");
            let values = list.value(row_idx);
            let floats = values
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or_else(|| {
                    LanceDbEngineError::internal(
                        "failed to downcast FixedSizeList child Float32 array",
                    )
                })?;

            let mut items = Vec::with_capacity(floats.len());
            for idx in 0..floats.len() {
                if floats.is_null(idx) {
                    items.push(Value::Null);
                } else {
                    items.push(Value::from(floats.value(idx) as f64));
                }
            }
            Ok(Value::Array(items))
        }
        other => Err(LanceDbEngineError::internal(format!(
            "unsupported output type for JSON encoding: {other:?}"
        ))),
    }
}

/// Counts the total rows across record batches.
/// 统计记录批次的总行数。
fn count_rows(batches: &[RecordBatch]) -> u64 {
    batches.iter().map(|batch| batch.num_rows() as u64).sum()
}

/// Resolves the effective output format, reporting a concrete encoding when none was requested.
/// 解析有效的输出格式，在未请求时报告一个具体编码。
fn normalize_output_format(output_format: LanceDbOutputFormat) -> LanceDbOutputFormat {
    match output_format {
        LanceDbOutputFormat::Unspecified => LanceDbOutputFormat::ArrowIpc,
        other => other,
    }
}

/// Maps a displayable dependency error onto a structured engine error.
/// 将可显示的依赖错误映射为结构化引擎错误。
///
/// A missing table is reported as [`LanceDbEngineError::TableNotFound`] so a caller can distinguish
/// "you named a table that does not exist" from a genuine internal failure without string matching.
/// Everything else is internal, because at this boundary the engine has no more specific
/// information than the dependency gave it.
///
/// 表不存在按 [`LanceDbEngineError::TableNotFound`] 上报，使调用方无需字符串匹配即可把"你指定的表
/// 不存在"与真正的内部失败区分开。其余情况按内部错误处理，因为在此边界上引擎并不比依赖提供的信息更具体。
fn map_lancedb_error(error: lancedb::Error) -> LanceDbEngineError {
    match error {
        lancedb::Error::TableNotFound { name, .. } => LanceDbEngineError::table_not_found(name),
        other => LanceDbEngineError::internal(other.to_string()),
    }
}

/// Maps a non-LanceDB dependency error (Arrow, I/O or JSON) onto an internal engine error.
/// 将非 LanceDB 的依赖错误（Arrow、I/O 或 JSON）映射为内部引擎错误。
///
/// These never indicate a caller mistake: reaching them means this crate's own encode or decode
/// step failed, so [`LanceDbEngineError::Internal`] is the honest classification.
///
/// 它们绝不表示调用方出错：触达它们意味着本 crate 自身的编码或解码步骤失败，因此
/// [`LanceDbEngineError::Internal`] 是诚实的分类。
fn map_external_error(error: impl std::fmt::Display) -> LanceDbEngineError {
    LanceDbEngineError::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        LanceDbEngine, LanceDbEngineOptions, MAX_COMPACTION_THREADS, TableAccessCoordinator,
        build_arrow_schema, normalize_output_format, normalize_table_name,
        validate_compaction_options, validate_compaction_transition,
    };
    use crate::types::{
        LanceDbColumnDef, LanceDbColumnType, LanceDbCompactTableInput, LanceDbCompactionMetrics,
        LanceDbCompactionOptions, LanceDbCreateTableInput, LanceDbDeleteInput,
        LanceDbDistanceMetric, LanceDbFragmentSummary, LanceDbInputFormat, LanceDbOutputFormat,
        LanceDbSearchInput, LanceDbTableStatisticsInput, LanceDbTableStatisticsResult,
        LanceDbUpsertInput,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Creates an isolated engine whose database directory is deleted after the test.
    /// 创建测试结束后自动删除数据库目录的隔离引擎。
    async fn isolated_engine(max_concurrent_requests: usize) -> (TempDir, LanceDbEngine) {
        let directory = tempfile::tempdir().expect("temporary database directory should exist");
        let connection = lancedb::connect(directory.path().to_string_lossy().as_ref())
            .storage_option(
                crate::storage_format::PINNED_STORAGE_VERSION_OPTION,
                crate::storage_format::PINNED_STORAGE_VERSION_VALUE,
            )
            .execute()
            .await
            .expect("temporary LanceDB connection should open");

        let engine = LanceDbEngine::from_connection(
            connection,
            LanceDbEngineOptions {
                max_concurrent_requests,
                ..LanceDbEngineOptions::default()
            },
        );
        (directory, engine)
    }

    /// Builds the stable table definition shared by engine tests.
    /// 构建引擎测试共用的稳定表定义。
    fn documents_definition(table_name: &str) -> LanceDbCreateTableInput {
        LanceDbCreateTableInput {
            table_name: table_name.to_string(),
            columns: vec![
                LanceDbColumnDef {
                    name: "id".to_string(),
                    column_type: LanceDbColumnType::String,
                    vector_dim: 0,
                    nullable: false,
                },
                LanceDbColumnDef {
                    name: "content".to_string(),
                    column_type: LanceDbColumnType::String,
                    vector_dim: 0,
                    nullable: false,
                },
                LanceDbColumnDef {
                    name: "vector".to_string(),
                    column_type: LanceDbColumnType::VectorFloat32,
                    vector_dim: 3,
                    nullable: false,
                },
            ],
            overwrite_if_exists: false,
        }
    }

    /// Builds one JSON row payload.
    /// 构建一份 JSON 行载荷。
    fn json_row(id: &str, content: &str, vector: [f32; 3]) -> serde_json::Value {
        json!({ "id": id, "content": content, "vector": vector })
    }

    /// Wraps JSON rows into an upsert payload.
    /// 将 JSON 行包装为写入载荷。
    fn json_upsert(table_name: &str, rows: Vec<serde_json::Value>) -> LanceDbUpsertInput {
        LanceDbUpsertInput {
            table_name: table_name.to_string(),
            input_format: LanceDbInputFormat::JsonRows,
            data: serde_json::to_vec(&rows).expect("rows must serialize"),
            key_columns: Vec::new(),
        }
    }

    #[test]
    fn normalize_table_name_trims_whitespace() {
        assert_eq!(normalize_table_name("  demo  "), "demo");
    }

    /// Verifies one physical table has exactly one synchronization identity, under every name that addresses it.
    /// 验证一张物理表在每一个能寻址到它的名字下都恰好只有一个同步身份。
    ///
    /// The engine's contract is that reads, ordinary mutations and compaction are coordinated **per table**, and that
    /// `compaction_gate` is an "exclusive gate preventing two native compactions on one table". Both are false if the
    /// lock identity is finer than the table identity, so this test pins both sides of that relationship: the premise
    /// that LanceDB folds case, and the consequence that the lock must fold it too.
    /// 本引擎的契约是按表协调读取、普通变更与整理，且 `compaction_gate` 是"防止两个原生整理作用于同一张表的独占闸门"。
    /// 若锁身份比表身份更细，这两条都不成立，因此本测试钉住这一关系的两侧：LanceDB 折叠大小写这一前提，以及锁也必须
    /// 折叠它的这一后果。
    #[tokio::test]
    async fn one_physical_table_keeps_one_lock_across_every_name_casing() {
        let (_directory, engine) = isolated_engine(4).await;

        // Premise, measured here rather than taken on trust: LanceDB's table identity ignores case.
        // 前提，在此实测而非凭信：LanceDB 的表身份不区分大小写。
        engine
            .create_table(documents_definition("CaseProbe"))
            .await
            .expect("create the mixed-case table");
        assert!(
            engine
                .create_table(documents_definition("caseprobe"))
                .await
                .is_err(),
            "premise failed: LanceDB must treat the two casings as one table, but the second create succeeded, so \
             this test would be checking a relationship that does not exist"
        );
        for name in ["CaseProbe", "caseprobe", "CASEPROBE"] {
            engine
                .table_statistics(LanceDbTableStatisticsInput {
                    table_name: name.to_string(),
                    compaction: LanceDbCompactionOptions::default(),
                })
                .await
                .unwrap_or_else(|error| {
                    panic!("premise failed: `{name}` must resolve to the one table: {error}")
                });
        }

        // Consequence: the synchronization identity must follow the table identity.
        // 后果：同步身份必须跟随表身份。
        let coordinator = TableAccessCoordinator::default();
        let mixed = coordinator.lock_for("CaseProbe");
        let lower = coordinator.lock_for("caseprobe");
        let upper = coordinator.lock_for("CASEPROBE");
        assert!(
            std::sync::Arc::ptr_eq(&mixed, &lower),
            "one physical table must have one lock, but `CaseProbe` and `caseprobe` took unrelated locks, so a write \
             through one casing would not exclude a read through the other and two compactions could run at once"
        );
        assert!(
            std::sync::Arc::ptr_eq(&mixed, &upper),
            "one physical table must have one lock, but `CaseProbe` and `CASEPROBE` took unrelated locks"
        );

        // Negative control, so the fix cannot be satisfied by folding everything: names that really are different
        // tables must keep different locks.
        // 反向对照，使该修复无法靠"折叠一切"蒙混过关：确实是不同表的名称必须保有各自的锁。
        engine
            .create_table(documents_definition("trail"))
            .await
            .expect("create `trail`");
        assert!(
            engine
                .create_table(documents_definition("trail."))
                .await
                .is_ok(),
            "premise failed: `trail.` must be a table distinct from `trail`"
        );
        assert!(
            !std::sync::Arc::ptr_eq(
                &coordinator.lock_for("trail"),
                &coordinator.lock_for("trail.")
            ),
            "two distinct tables must not share one lock, or the fix would trade a missing lock for a spurious one"
        );

        // And surrounding whitespace, which is normalized away before the table is addressed, must not split the lock
        // either — that half of the identity already worked and must keep working.
        // 而首尾空白在寻址表之前就被规范化掉，因此它同样不得把锁拆开——身份的这一半本来就是对的，必须保持正确。
        engine
            .create_table(documents_definition("spaced"))
            .await
            .expect("create `spaced`");
        assert!(
            engine
                .create_table(documents_definition("spaced "))
                .await
                .is_err(),
            "premise failed: surrounding whitespace must not change which table is addressed"
        );
        assert!(
            std::sync::Arc::ptr_eq(
                &coordinator.lock_for("spaced"),
                &coordinator.lock_for("spaced ")
            ),
            "surrounding whitespace must not change the lock identity"
        );
    }

    #[test]
    fn engine_options_default_values_are_stable() {
        let options = LanceDbEngineOptions::default();
        assert_eq!(options.max_upsert_payload, 50 * 1024 * 1024);
        assert_eq!(options.max_search_limit, 10_000);
        assert_eq!(options.max_concurrent_requests, 500);
    }

    /// Verifies duplicate field identities are refused before LanceDB sees an ambiguous schema.
    /// 验证重复字段身份会在 LanceDB 接收到歧义 Schema 前被拒绝。
    #[test]
    fn duplicate_column_names_are_refused() {
        let columns = vec![
            LanceDbColumnDef {
                name: "id".to_string(),
                column_type: LanceDbColumnType::String,
                vector_dim: 0,
                nullable: false,
            },
            LanceDbColumnDef {
                name: "id".to_string(),
                column_type: LanceDbColumnType::Int64,
                vector_dim: 0,
                nullable: false,
            },
        ];

        assert!(build_arrow_schema(&columns).is_err());
    }

    /// Verifies a vector dimension cannot wrap while converting to Arrow's signed dimension.
    /// 验证向量维度转换为 Arrow 有符号维度时不能发生回绕。
    #[test]
    fn vector_dimension_above_arrow_range_is_refused() {
        let columns = vec![LanceDbColumnDef {
            name: "vector".to_string(),
            column_type: LanceDbColumnType::VectorFloat32,
            vector_dim: (i32::MAX as u32) + 1,
            nullable: false,
        }];

        assert!(build_arrow_schema(&columns).is_err());
    }

    #[test]
    fn normalize_output_format_defaults_to_arrow_ipc() {
        assert_eq!(
            normalize_output_format(LanceDbOutputFormat::Unspecified),
            LanceDbOutputFormat::ArrowIpc
        );
        assert_eq!(
            normalize_output_format(LanceDbOutputFormat::JsonRows),
            LanceDbOutputFormat::JsonRows
        );
    }

    #[tokio::test]
    async fn table_access_coordinator_reuses_trimmed_table_lock() {
        let coordinator = TableAccessCoordinator::default();
        let lock_a = coordinator.lock_for("demo");
        let lock_b = coordinator.lock_for(" demo ");
        assert!(Arc::ptr_eq(&lock_a, &lock_b));
    }

    #[tokio::test]
    async fn table_write_waits_for_active_reader() {
        let coordinator = Arc::new(TableAccessCoordinator::default());
        let reader_guard = coordinator.acquire_read("demo").await;

        let writer = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move {
                let _guard = coordinator.acquire_write("demo").await;
            })
        };

        // Give the writer a chance to observe the barrier; it must not complete while the reader
        // holds the read side.
        // 给写入者机会观察屏障；在读者持有读侧期间，它不得完成。
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !writer.is_finished(),
            "an ordinary mutation must wait for an active reader"
        );

        drop(reader_guard);
        writer.await.expect("writer task must complete");
    }

    /// Verifies same-table waiters cannot consume every global permit and starve an unrelated table.
    /// 验证同表等待者不会占满全部全局许可并饿死无关表。
    #[tokio::test]
    async fn same_table_waiters_do_not_starve_an_unrelated_table() {
        let (_directory, engine) = isolated_engine(2).await;
        for table_name in ["blocked", "unrelated"] {
            engine
                .create_table(documents_definition(table_name))
                .await
                .expect("create test table");
        }

        let blocked_guard = engine.acquire_table_write("blocked").await;
        let mut waiters = Vec::new();
        for _ in 0..2 {
            let engine = engine.clone();
            waiters.push(tokio::spawn(async move {
                engine
                    .table_statistics(LanceDbTableStatisticsInput {
                        table_name: "blocked".to_string(),
                        compaction: LanceDbCompactionOptions::default(),
                    })
                    .await
            }));
        }
        tokio::task::yield_now().await;

        let unrelated = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            engine.table_statistics(LanceDbTableStatisticsInput {
                table_name: "unrelated".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            }),
        )
        .await;
        assert!(
            matches!(unrelated, Ok(Ok(_))),
            "waiters blocked on one table must not consume both global permits"
        );

        drop(blocked_guard);
        for waiter in waiters {
            waiter
                .await
                .expect("waiter task should finish")
                .expect("statistics should succeed after the table is released");
        }
    }

    #[tokio::test]
    async fn compaction_guard_is_exclusive_but_admits_readers() {
        let coordinator = Arc::new(TableAccessCoordinator::default());
        let compaction_guard = coordinator.acquire_compaction("demo").await;

        // A second compactor must wait on the exclusive gate.
        // 第二个整理者必须在独占闸门上等待。
        let second = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move {
                let _guard = coordinator.acquire_compaction("demo").await;
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !second.is_finished(),
            "a second compaction on one table must wait"
        );

        // A snapshot reader must still be admitted while compaction holds its guard.
        // 在整理持有守卫期间，快照读取仍应被接纳。
        let reader = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            coordinator.acquire_read("demo"),
        )
        .await;
        assert!(
            reader.is_ok(),
            "a snapshot read must be admitted while compaction is in flight"
        );

        drop(compaction_guard);
        second.await.expect("second compactor must complete");
    }

    #[tokio::test]
    async fn table_write_waits_for_in_flight_compaction() {
        let coordinator = Arc::new(TableAccessCoordinator::default());
        // Only the compaction guard is held in this test, so a writer that does not finish is
        // provably waiting on compaction rather than on a reader.
        // 本测试只持有整理守卫，故一个未完成的写入者可证是在等待整理，而不是等待读者。
        let compaction_guard = coordinator.acquire_compaction("demo").await;

        let writer = {
            let coordinator = Arc::clone(&coordinator);
            tokio::spawn(async move {
                let _guard = coordinator.acquire_write("demo").await;
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            !writer.is_finished(),
            "an ordinary mutation must wait for an in-flight compaction"
        );

        drop(compaction_guard);
        writer
            .await
            .expect("writer task must complete after compaction");
    }
    #[test]
    fn compaction_input_rejects_zero_and_excessive_values() {
        let base = LanceDbCompactionOptions::default();

        assert!(validate_compaction_options(&base).is_ok());

        let zero = LanceDbCompactionOptions {
            batch_size: Some(0),
            ..base
        };
        assert!(validate_compaction_options(&zero).is_err());

        let too_many_threads = LanceDbCompactionOptions {
            num_threads: Some(MAX_COMPACTION_THREADS + 1),
            ..base
        };
        assert!(validate_compaction_options(&too_many_threads).is_err());

        let bounded_threads = LanceDbCompactionOptions {
            num_threads: Some(MAX_COMPACTION_THREADS),
            ..base
        };
        assert!(validate_compaction_options(&bounded_threads).is_ok());
    }

    /// Builds deterministic statistics for the compaction postcondition guard.
    /// 为整理后置条件保护构造确定性统计信息。
    fn statistics_fixture(
        version: u64,
        rows: u64,
        indexes: u64,
        fragments: u64,
    ) -> LanceDbTableStatisticsResult {
        LanceDbTableStatisticsResult {
            table_name: "vectors".to_string(),
            version,
            total_bytes: 4096,
            row_count: rows,
            index_count: indexes,
            fragment_count: fragments,
            small_fragment_count: fragments,
            compaction_task_count: u64::from(fragments > 1),
            fragment_rows: LanceDbFragmentSummary {
                minimum_rows: rows.checked_div(fragments).unwrap_or(0),
                maximum_rows: rows,
                mean_rows: rows.checked_div(fragments).unwrap_or(0),
                p25_rows: rows.checked_div(fragments).unwrap_or(0),
                p50_rows: rows.checked_div(fragments).unwrap_or(0),
                p75_rows: rows.checked_div(fragments).unwrap_or(0),
                p99_rows: rows,
            },
        }
    }

    fn metrics(removed: u64, added: u64) -> LanceDbCompactionMetrics {
        LanceDbCompactionMetrics {
            fragments_removed: removed,
            fragments_added: added,
            files_removed: removed,
            files_added: added,
        }
    }

    #[test]
    fn compaction_transition_accepts_a_layout_only_change() {
        let before = statistics_fixture(1, 100, 0, 10);
        let after = statistics_fixture(2, 100, 0, 4);
        assert!(validate_compaction_transition(&before, &after, &metrics(10, 4)).is_ok());
    }

    #[test]
    fn compaction_transition_rejects_logical_changes() {
        let before = statistics_fixture(1, 100, 1, 10);

        // Row loss.
        // 行丢失。
        let rows_lost = statistics_fixture(2, 99, 1, 4);
        assert!(validate_compaction_transition(&before, &rows_lost, &metrics(10, 4)).is_err());

        // Index loss.
        // 索引丢失。
        let index_lost = statistics_fixture(2, 100, 0, 4);
        assert!(validate_compaction_transition(&before, &index_lost, &metrics(10, 4)).is_err());

        // Version regression.
        // 版本倒退。
        let regressed = statistics_fixture(0, 100, 1, 4);
        assert!(validate_compaction_transition(&before, &regressed, &metrics(10, 4)).is_err());

        // Fragment arithmetic that does not match the reported metrics.
        // 与所报指标不符的片段算术。
        let mismatched = statistics_fixture(2, 100, 1, 5);
        assert!(validate_compaction_transition(&before, &mismatched, &metrics(10, 4)).is_err());

        // Metrics that claim more removals than existed.
        // 声称移除数超过实际存在的指标。
        let impossible = statistics_fixture(2, 100, 1, 4);
        assert!(validate_compaction_transition(&before, &impossible, &metrics(11, 5)).is_err());
    }

    /// Verifies engine construction exposes its options.
    /// 验证引擎构造会暴露其选项。
    #[tokio::test]
    async fn engine_exposes_options() {
        let (_directory, engine) = isolated_engine(4).await;
        assert_eq!(engine.options().max_concurrent_requests, 4);
    }

    /// Verifies a created table resolves to the pinned v1 Lance file format.
    /// 验证建出的表解析为固定的 v1 Lance 文件格式。
    #[tokio::test]
    async fn created_table_uses_the_pinned_storage_format() {
        let (_directory, engine) = isolated_engine(4).await;
        // `create_table` verifies the resolved format internally and fails if it drifted, so a
        // success here is the assertion.
        // `create_table` 内部会校验解析出的格式并在其漂移时失败，因此此处成功即构成断言。
        engine
            .create_table(documents_definition("pinned_docs"))
            .await
            .expect("create_table must produce a table at the pinned format");
    }

    /// Verifies the full write-then-search path returns the written row.
    /// 验证"写入后检索"的完整路径返回所写入的行。
    #[tokio::test]
    async fn upsert_then_search_returns_the_written_row() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let upsert = engine
            .vector_upsert(json_upsert(
                "docs",
                vec![
                    json_row("a", "alpha content", [1.0, 0.0, 0.0]),
                    json_row("b", "beta content", [0.0, 1.0, 0.0]),
                ],
            ))
            .await
            .expect("upsert rows");

        assert_eq!(upsert.input_rows, 2);
        assert_eq!(upsert.inserted_rows, 2);

        let search = engine
            .vector_search(LanceDbSearchInput {
                table_name: "docs".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                limit: 10,
                filter: String::new(),
                vector_column: "vector".to_string(),
                output_format: LanceDbOutputFormat::JsonRows,
            })
            .await
            .expect("search");

        assert_eq!(search.rows, 2);

        let rows: Vec<serde_json::Value> =
            serde_json::from_slice(&search.data).expect("search output must be JSON");
        assert_eq!(rows[0]["id"], json!("a"), "nearest row must rank first");
    }

    /// Verifies `vector_search_with_metric` ranks one table by each explicit metric; uses only an isolated local database.
    /// 验证 `vector_search_with_metric` 按各显式度量排列同表；仅使用隔离本地数据库。
    /// Takes no parameters and returns no value; failures identify incorrect engine distance admission.
    /// 无参数及返回值；失败指出错误的引擎距离准入。
    #[tokio::test]
    async fn explicit_distance_metrics_rank_distinct_nearest_rows() {
        // A single nonnormalized fixture distinguishes L2, cosine and dot product rankings.
        // 单份非归一化夹具区分 L2、余弦及点积排名。
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("metrics"))
            .await
            .unwrap();
        engine
            .vector_upsert(json_upsert(
                "metrics",
                vec![
                    json_row("near", "near", [1.0, 1.0, 0.0]),
                    json_row("parallel", "parallel", [10.0, 0.0, 0.0]),
                    json_row("large", "large", [100.0, 50.0, 0.0]),
                ],
            ))
            .await
            .unwrap();
        for (metric, expected) in [
            (LanceDbDistanceMetric::Euclidean, "near"),
            (LanceDbDistanceMetric::Cosine, "parallel"),
            (LanceDbDistanceMetric::DotProduct, "large"),
        ] {
            // Only the declared metric changes; vector, data and query shape remain identical.
            // 仅声明度量变化；向量、数据及查询形状保持一致。
            let result = engine
                .vector_search_with_metric(
                    LanceDbSearchInput {
                        table_name: "metrics".into(),
                        vector: vec![1.0, 0.0, 0.0],
                        limit: 1,
                        filter: String::new(),
                        vector_column: "vector".into(),
                        output_format: LanceDbOutputFormat::JsonRows,
                    },
                    metric,
                )
                .await
                .unwrap();
            // The top result is a semantic rank assertion, independent of fixture insertion order.
            // 首位结果为语义排名断言，独立于夹具插入顺序。
            let rows: Vec<serde_json::Value> = serde_json::from_slice(&result.data).unwrap();
            assert_eq!(
                rows.first().unwrap()["id"],
                json!(expected),
                "wrong rank for {metric:?}"
            );
        }
    }

    /// Verifies a keyed upsert updates rather than duplicating.
    /// 验证带键写入是更新而非重复插入。
    #[tokio::test]
    async fn keyed_upsert_updates_instead_of_duplicating() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        for content in ["first", "second"] {
            engine
                .maintain_merge_key_index("docs", "id")
                .await
                .expect("maintain exact merge key index");
            let mut upsert = json_upsert("docs", vec![json_row("a", content, [1.0, 0.0, 0.0])]);
            upsert.key_columns = vec!["id".to_string()];
            engine.vector_upsert(upsert).await.expect("keyed upsert");
        }

        let stats = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics");

        assert_eq!(stats.row_count, 1, "a keyed upsert must not duplicate rows");
        let table = engine.open_table("docs").await.expect("open indexed table");
        assert!(
            table
                .list_indices()
                .await
                .unwrap()
                .iter()
                .any(|index| index.columns == ["id"]
                    && index.index_type == lancedb::index::IndexType::BTree)
        );
    }
    /// Merge-key maintenance folds the uncovered tail into the index without duplicating existing IDs.
    /// 合并键维护将未覆盖尾部纳入索引，不重复已有 ID。
    #[tokio::test]
    async fn keyed_upsert_index_bounds_unindexed_tail() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("indexed_tail"))
            .await
            .unwrap();
        let mut seed = json_upsert(
            "indexed_tail",
            vec![json_row("seed", "seed", [1.0, 0.0, 0.0])],
        );
        seed.key_columns = vec!["id".into()];
        engine.vector_upsert(seed).await.unwrap();
        engine
            .maintain_merge_key_index("indexed_tail", "id")
            .await
            .unwrap();
        let mut tail = json_upsert(
            "indexed_tail",
            (0..4096)
                .map(|index| json_row(&format!("row-{index}"), "tail", [1.0, 0.0, 0.0]))
                .collect(),
        );
        tail.key_columns = vec!["id".into()];
        engine.vector_upsert(tail).await.unwrap();
        engine
            .maintain_merge_key_index("indexed_tail", "id")
            .await
            .unwrap();
        let table = engine.open_table("indexed_tail").await.unwrap();
        let indices = table.list_indices().await.unwrap();
        let index = indices
            .iter()
            .find(|index| index.columns == ["id"])
            .unwrap();
        assert_eq!(
            table
                .index_stats(&index.name)
                .await
                .unwrap()
                .unwrap()
                .num_unindexed_rows,
            0
        );
        assert_eq!(table.count_rows(None).await.unwrap(), 4097);
    }

    /// Verifies a payload larger than the retired controller's 4 MiB ceiling is written locally.
    /// 验证大于已退役控制器 4 MiB 上限的载荷在本地直接写入。
    ///
    /// The controller's RPC path capped payloads at 4 MiB, and plan section 8.2 requires evidence that large
    /// batches no longer pass through that transport. The engine's own limit is 50 MiB by default, so the only
    /// payload that can show the ceiling is gone is one that would have hit it: a fixture under 4 MiB would pass
    /// with or without a ceiling, and a test that used one would prove nothing.
    /// 控制器的 RPC 路径把载荷上限定为 4 MiB，而方案 §8.2 要求给出大批量不再经过该传输的证据。引擎自身默认上限为
    /// 50 MiB，因此能显示该上限已消失的唯一载荷正是本会触及它的那种：小于 4 MiB 的夹具无论有无上限都会通过，用它
    /// 的测试什么也证明不了。
    #[tokio::test]
    async fn a_payload_above_four_mib_is_written_without_transport_chunking() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        // Each row carries 64 KiB of content, so the encoded payload is several times the retired ceiling.
        // 每行携带 64 KiB 内容，故编码后的载荷是该已退役上限的数倍。
        let filler = "x".repeat(64 * 1024);
        let rows = (0..80)
            .map(|index| json_row(&format!("row-{index}"), &filler, [1.0, 0.0, 0.0]))
            .collect::<Vec<_>>();
        let payload_bytes = serde_json::to_vec(&rows)
            .expect("encode the fixture rows")
            .len();
        assert!(
            payload_bytes > 4 * 1024 * 1024,
            "the fixture must exceed the retired 4 MiB ceiling, measured {payload_bytes} bytes"
        );

        let mut upsert = json_upsert("docs", rows);
        upsert.key_columns = vec!["id".to_string()];
        let result = engine
            .vector_upsert(upsert)
            .await
            .expect("a payload above 4 MiB must be accepted locally");

        assert_eq!(result.input_rows, 80);
        assert_eq!(
            result.inserted_rows, 80,
            "every row of a large batch must be written"
        );

        let stats = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics");
        assert_eq!(
            stats.row_count, 80,
            "the physical row count must match the batch size"
        );
    }

    /// Verifies a non-nullable field that is absent from a JSON row is refused.
    /// 验证 JSON 行中缺失的非空字段被拒绝。
    #[tokio::test]
    async fn missing_non_nullable_field_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let result = engine
            .vector_upsert(json_upsert(
                "docs",
                vec![json!({ "id": "a", "vector": [1.0, 0.0, 0.0] })],
            ))
            .await;

        assert!(
            result.is_err(),
            "a missing non-nullable field must be refused"
        );
    }

    /// Verifies a vector whose length does not match the declared dimension is refused.
    /// 验证长度与声明维度不符的向量被拒绝。
    #[tokio::test]
    async fn vector_length_mismatch_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let result = engine
            .vector_upsert(json_upsert(
                "docs",
                vec![json!({ "id": "a", "content": "x", "vector": [1.0, 0.0] })],
            ))
            .await;

        assert!(result.is_err(), "a vector length mismatch must be refused");
    }

    /// Verifies the upsert payload bound is enforced before any write.
    /// 验证写入载荷上限在任何写入之前得到强制。
    #[tokio::test]
    async fn oversized_upsert_payload_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let mut upsert = json_upsert(
            "docs",
            vec![json_row("a", &"x".repeat(2048), [1.0, 0.0, 0.0])],
        );
        upsert.data = vec![0_u8; engine.options().max_upsert_payload + 1];

        let result = engine.vector_upsert(upsert).await;
        assert!(result.is_err(), "an oversized payload must be refused");
    }

    /// Verifies deleting by predicate removes the matching rows.
    /// 验证按谓词删除会移除匹配行。
    #[tokio::test]
    async fn delete_by_predicate_removes_matching_rows() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");
        engine
            .vector_upsert(json_upsert(
                "docs",
                vec![
                    json_row("a", "alpha", [1.0, 0.0, 0.0]),
                    json_row("b", "beta", [0.0, 1.0, 0.0]),
                ],
            ))
            .await
            .expect("upsert");

        let deleted = engine
            .delete(LanceDbDeleteInput {
                table_name: "docs".to_string(),
                condition: "id = 'a'".to_string(),
            })
            .await
            .expect("delete");
        assert_eq!(deleted.deleted_rows, 1);

        let stats = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics");
        assert_eq!(stats.row_count, 1);
    }

    /// Verifies an empty delete condition is refused.
    /// 验证空的删除条件被拒绝。
    #[tokio::test]
    async fn empty_delete_condition_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        let result = engine
            .delete(LanceDbDeleteInput {
                table_name: "docs".to_string(),
                condition: "   ".to_string(),
            })
            .await;
        assert!(result.is_err());
    }

    /// Verifies dropping a table makes it unresolvable and is reported as not found.
    /// 验证删表后该表不可解析，并按表不存在上报。
    #[tokio::test]
    async fn drop_table_then_statistics_reports_table_not_found() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");
        engine
            .drop_table(crate::types::LanceDbDropTableInput {
                table_name: "docs".to_string(),
            })
            .await
            .expect("drop table");

        let result = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await;

        assert!(
            matches!(
                result,
                Err(crate::error::LanceDbEngineError::TableNotFound { .. })
            ),
            "a dropped table must be reported as TableNotFound, got {result:?}"
        );
    }

    /// Verifies an empty table name is refused before reaching the database.
    /// 验证空表名在触达数据库之前即被拒绝。
    #[tokio::test]
    async fn empty_table_name_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;

        assert!(
            engine
                .table_statistics(LanceDbTableStatisticsInput {
                    table_name: "   ".to_string(),
                    compaction: LanceDbCompactionOptions::default(),
                })
                .await
                .is_err()
        );
        assert!(
            engine
                .create_table(documents_definition("  "))
                .await
                .is_err()
        );
    }

    /// Verifies a table definition with no columns is refused.
    /// 验证无列的表定义被拒绝。
    #[tokio::test]
    async fn table_without_columns_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        let result = engine
            .create_table(LanceDbCreateTableInput {
                table_name: "empty_table".to_string(),
                columns: Vec::new(),
                overwrite_if_exists: false,
            })
            .await;
        assert!(result.is_err());
    }

    /// Verifies a vector column without a dimension is refused.
    /// 验证未指定维度的向量列被拒绝。
    #[tokio::test]
    async fn vector_column_without_dimension_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        let result = engine
            .create_table(LanceDbCreateTableInput {
                table_name: "bad_vector".to_string(),
                columns: vec![LanceDbColumnDef {
                    name: "vector".to_string(),
                    column_type: LanceDbColumnType::VectorFloat32,
                    vector_dim: 0,
                    nullable: false,
                }],
                overwrite_if_exists: false,
            })
            .await;
        assert!(result.is_err());
    }

    /// Verifies compaction preserves logical rows and reports a before/after pair.
    /// 验证整理保持逻辑行不变并报告前后统计。
    #[tokio::test]
    async fn compaction_preserves_rows_and_reports_statistics() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        // Several separate writes produce multiple fragments, giving compaction something to do.
        // 多次独立写入会产生多个片段，使整理有实际工作。
        for index in 0..4 {
            engine
                .vector_upsert(json_upsert(
                    "docs",
                    vec![json_row(
                        &format!("doc-{index}"),
                        "content",
                        [index as f32, 1.0, 0.0],
                    )],
                ))
                .await
                .expect("upsert");
        }

        let compacted = engine
            .compact_table(LanceDbCompactTableInput {
                table_name: "docs".to_string(),
                expected_version: None,
                options: LanceDbCompactionOptions {
                    target_rows_per_fragment: Some(128),
                    max_rows_per_group: Some(1_024),
                    max_bytes_per_file: None,
                    num_threads: Some(2),
                    batch_size: Some(128),
                    max_source_fragments: Some(512),
                },
            })
            .await
            .expect("compaction must succeed");

        assert_eq!(
            compacted.after.row_count, compacted.before.row_count,
            "compaction must not change the logical row count"
        );
        assert!(compacted.after.version >= compacted.before.version);
        // The planner that sized the work must find nothing left once the same options ran.
        // 确定工作量的计划器在以同一参数执行后必须不再找到剩余工作。
        assert!(compacted.before.compaction_task_count > 0);
        assert_eq!(compacted.after.compaction_task_count, 0);
        // Fragment evidence for plan §10: the per-fragment row statistics before and after compaction.
        // 方案 §10 的片段证据：整理前后逐片段的行统计。
        eprintln!(
            "[perf-fragments] before_min={} before_max={} before_mean={} before_p50={} \
             after_min={} after_max={} after_mean={} after_p50={}",
            compacted.before.fragment_rows.minimum_rows,
            compacted.before.fragment_rows.maximum_rows,
            compacted.before.fragment_rows.mean_rows,
            compacted.before.fragment_rows.p50_rows,
            compacted.after.fragment_rows.minimum_rows,
            compacted.after.fragment_rows.maximum_rows,
            compacted.after.fragment_rows.mean_rows,
            compacted.after.fragment_rows.p50_rows,
        );
        assert!(
            compacted.after.fragment_rows.maximum_rows
                >= compacted.after.fragment_rows.minimum_rows,
            "compaction must leave a coherent per-fragment row distribution"
        );
    }

    /// Records the fragment count produced by one large batch write, for plan §10.
    /// 记录单次大批量写入所产生的片段数，供方案 §10 使用。
    #[tokio::test]
    async fn performance_large_batch_reports_bounded_fragment_count() {
        const ROWS: usize = 10_000;

        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("large_batch"))
            .await
            .expect("create table");

        // One single write of many rows: the case §10 asks about.
        // 单次写入大量行：这正是 §10 所问的情形。
        let rows = (0..ROWS)
            .map(|index| json_row(&format!("doc-{index}"), "content", [index as f32, 1.0, 0.0]))
            .collect::<Vec<_>>();
        engine
            .vector_upsert(json_upsert("large_batch", rows))
            .await
            .expect("large batch upsert");

        let statistics = engine
            .table_statistics(crate::types::LanceDbTableStatisticsInput {
                table_name: "large_batch".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics");

        eprintln!(
            "[perf-fragments] rows={} row_count={} fragment_count={} small_fragment_count={} \
         rows_per_fragment_mean={} rows_per_fragment_p50={}",
            ROWS,
            statistics.row_count,
            statistics.fragment_count,
            statistics.small_fragment_count,
            statistics.fragment_rows.mean_rows,
            statistics.fragment_rows.p50_rows,
        );

        assert_eq!(statistics.row_count as usize, ROWS);
        assert!(
            statistics.fragment_count < ROWS as u64,
            "one large batch must not create a physical fragment per row"
        );
    }
    /// Records LanceDB latency at two scales across its six operations, for plan §10.
    /// 记录两种规模下 LanceDB 六项操作的延迟，供方案 §10 使用。
    #[tokio::test]
    #[ignore = "explicit §10 LanceDB engine performance evidence"]
    async fn performance_lancedb_operations_report_p50_p95() {
        const SAMPLES: usize = 20;

        for rows_per_table in [1_000usize, 10_000] {
            let (_directory, engine) = isolated_engine(4).await;

            // One write of the whole scale, on a table created for each sample.
            // 单次写入整个规模的行数，每个样本各建一张表。
            let mut upsert_micros = Vec::with_capacity(SAMPLES);
            for sample in 0..SAMPLES {
                let table = format!("perf_upsert_{rows_per_table}_{sample}");
                engine
                    .create_table(documents_definition(&table))
                    .await
                    .expect("create table");
                let rows = (0..rows_per_table)
                    .map(|index| {
                        json_row(&format!("doc-{index}"), "content", [index as f32, 1.0, 0.0])
                    })
                    .collect::<Vec<_>>();
                let started = std::time::Instant::now();
                engine
                    .vector_upsert(json_upsert(&table, rows))
                    .await
                    .expect("upsert");
                upsert_micros.push(started.elapsed().as_micros());
            }

            // One prepared table then serves search, statistics, delete and compaction samples.
            // 随后用一张已备好的表做检索、统计、删除与整理的样本。
            let table = format!("perf_query_{rows_per_table}");
            engine
                .create_table(documents_definition(&table))
                .await
                .expect("create table");
            let rows = (0..rows_per_table)
                .map(|index| json_row(&format!("doc-{index}"), "content", [index as f32, 1.0, 0.0]))
                .collect::<Vec<_>>();
            engine
                .vector_upsert(json_upsert(&table, rows))
                .await
                .expect("prepare table");

            let mut search_micros = Vec::with_capacity(SAMPLES);
            let mut statistics_micros = Vec::with_capacity(SAMPLES);
            let mut delete_micros = Vec::with_capacity(SAMPLES);
            let mut compaction_micros = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let started = std::time::Instant::now();
                engine
                    .vector_search(LanceDbSearchInput {
                        table_name: table.clone(),
                        vector: vec![1.0, 0.0, 0.0],
                        limit: 10,
                        filter: String::new(),
                        vector_column: "vector".to_string(),
                        output_format: LanceDbOutputFormat::JsonRows,
                    })
                    .await
                    .expect("search");
                search_micros.push(started.elapsed().as_micros());

                let started = std::time::Instant::now();
                engine
                    .table_statistics(crate::types::LanceDbTableStatisticsInput {
                        table_name: table.clone(),
                        compaction: LanceDbCompactionOptions::default(),
                    })
                    .await
                    .expect("statistics");
                statistics_micros.push(started.elapsed().as_micros());

                let started = std::time::Instant::now();
                engine
                    .delete(LanceDbDeleteInput {
                        table_name: table.clone(),
                        condition: "id = 'doc-0'".to_string(),
                    })
                    .await
                    .expect("delete");
                delete_micros.push(started.elapsed().as_micros());

                let started = std::time::Instant::now();
                engine
                    .compact_table(LanceDbCompactTableInput {
                        table_name: table.clone(),
                        expected_version: None,
                        options: LanceDbCompactionOptions {
                            target_rows_per_fragment: Some(128),
                            max_rows_per_group: Some(1_024),
                            max_bytes_per_file: None,
                            num_threads: Some(2),
                            batch_size: Some(128),
                            max_source_fragments: Some(512),
                        },
                    })
                    .await
                    .expect("compaction");
                compaction_micros.push(started.elapsed().as_micros());
            }

            let env_os = std::env::consts::OS;
            let env_arch = std::env::consts::ARCH;
            // The first iteration is a warmup: its work runs and warms the path, its sample is discarded.
            // 首次迭代为预热：其工作照常执行并预热路径，其样本被丢弃。
            for samples in [
                &mut upsert_micros,
                &mut search_micros,
                &mut statistics_micros,
                &mut delete_micros,
                &mut compaction_micros,
            ] {
                samples.remove(0);
            }
            let (upsert_p50, upsert_p95) = lancedb_percentile_micros(&mut upsert_micros);
            let (search_p50, search_p95) = lancedb_percentile_micros(&mut search_micros);
            let (statistics_p50, statistics_p95) =
                lancedb_percentile_micros(&mut statistics_micros);
            let (delete_p50, delete_p95) = lancedb_percentile_micros(&mut delete_micros);
            let (compaction_p50, compaction_p95) =
                lancedb_percentile_micros(&mut compaction_micros);
            eprintln!(
                "[perf-lancedb] samples={SAMPLES} rows_per_table={rows_per_table} \
             upsert_p50_us={upsert_p50} upsert_p95_us={upsert_p95} \
             search_p50_us={search_p50} search_p95_us={search_p95} \
             statistics_p50_us={statistics_p50} statistics_p95_us={statistics_p95} \
             delete_p50_us={delete_p50} delete_p95_us={delete_p95} \
             compaction_p50_us={compaction_p50} compaction_p95_us={compaction_p95} env_os={env_os} env_arch={env_arch}"
            );
        }
    }

    /// Returns the p50 and p95 of a sample set, in microseconds.
    /// 返回一组样本的 p50 与 p95，单位为微秒。
    fn lancedb_percentile_micros(samples: &mut [u128]) -> (u128, u128) {
        samples.sort_unstable();
        let pick = |percent: usize| samples[(samples.len() - 1) * percent / 100];
        (pick(50), pick(95))
    }
    /// Verifies a stale expected version is refused before any compaction work starts.
    /// 验证过期的期望版本在任何整理工作开始前即被拒绝。
    #[tokio::test]
    async fn compaction_with_stale_expected_version_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        // Version 0 cannot be current for a table that has just been created.
        // 对刚创建的表而言，版本 0 不可能是当前版本。
        let result = engine
            .compact_table(LanceDbCompactTableInput {
                table_name: "docs".to_string(),
                expected_version: Some(0),
                options: LanceDbCompactionOptions::default(),
            })
            .await;

        assert!(result.is_err(), "a stale expected version must be refused");
    }

    /// Verifies Arrow IPC output round-trips back through the Arrow IPC reader.
    /// 验证 Arrow IPC 输出可通过 Arrow IPC 读取器往返。
    #[tokio::test]
    async fn arrow_ipc_output_round_trips() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");
        engine
            .vector_upsert(json_upsert(
                "docs",
                vec![json_row("a", "alpha", [1.0, 0.0, 0.0])],
            ))
            .await
            .expect("upsert");

        let search = engine
            .vector_search(LanceDbSearchInput {
                table_name: "docs".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                limit: 5,
                filter: String::new(),
                vector_column: "vector".to_string(),
                // Leaving the format unspecified must still produce a readable Arrow stream.
                // 保持格式未指定时仍应产生可读的 Arrow 流。
                output_format: LanceDbOutputFormat::Unspecified,
            })
            .await
            .expect("search");

        assert_eq!(search.format, LanceDbOutputFormat::ArrowIpc);
        assert_eq!(search.rows, 1);

        let mut reader =
            arrow_ipc::reader::StreamReader::try_new(std::io::Cursor::new(search.data), None)
                .expect("search output must open as an Arrow IPC stream");
        let batches = reader
            .by_ref()
            .collect::<Result<Vec<_>, _>>()
            .expect("search output must decode");
        let decoded: usize = batches.iter().map(|batch| batch.num_rows()).sum();
        assert_eq!(decoded, 1);
    }

    /// Verifies a search filter is applied.
    /// 验证检索过滤条件生效。
    #[tokio::test]
    async fn search_filter_is_applied() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");
        engine
            .vector_upsert(json_upsert(
                "docs",
                vec![
                    json_row("a", "alpha", [1.0, 0.0, 0.0]),
                    json_row("b", "beta", [1.0, 0.0, 0.0]),
                ],
            ))
            .await
            .expect("upsert");

        let search = engine
            .vector_search(LanceDbSearchInput {
                table_name: "docs".to_string(),
                vector: vec![1.0, 0.0, 0.0],
                limit: 10,
                filter: "id = 'b'".to_string(),
                vector_column: "vector".to_string(),
                output_format: LanceDbOutputFormat::JsonRows,
            })
            .await
            .expect("filtered search");

        assert_eq!(search.rows, 1, "the filter must restrict the result set");

        let rows: Vec<serde_json::Value> =
            serde_json::from_slice(&search.data).expect("JSON output");
        assert_eq!(rows[0]["id"], json!("b"));
    }

    /// Verifies an empty query vector is refused.
    /// 验证空查询向量被拒绝。
    #[tokio::test]
    async fn empty_query_vector_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        let result = engine
            .vector_search(LanceDbSearchInput {
                table_name: "docs".to_string(),
                vector: Vec::new(),
                limit: 10,
                filter: String::new(),
                vector_column: "vector".to_string(),
                output_format: LanceDbOutputFormat::ArrowIpc,
            })
            .await;
        assert!(result.is_err());
    }

    /// Verifies Arrow IPC ingestion is accepted and counted.
    /// 验证 Arrow IPC 输入被接受并正确计数。
    #[tokio::test]
    async fn arrow_ipc_ingestion_is_accepted() {
        use arrow_array::{Float32Array, RecordBatch, StringArray};
        use arrow_schema::{DataType, Field, Schema};

        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 3),
                false,
            ),
        ]));

        let mut vector_builder =
            arrow_array::builder::FixedSizeListBuilder::new(Float32Array::builder(1), 3);
        vector_builder.values().append_slice(&[1.0, 2.0, 3.0]);
        vector_builder.append(true);

        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["ipc-1"])),
                Arc::new(StringArray::from(vec!["from arrow"])),
                Arc::new(vector_builder.finish()),
            ],
        )
        .expect("build batch");

        let mut writer =
            arrow_ipc::writer::StreamWriter::try_new(Vec::<u8>::new(), schema.as_ref())
                .expect("ipc writer");
        writer.write(&batch).expect("write batch");
        writer.finish().expect("finish stream");
        let data = writer.into_inner().expect("take bytes");

        let upsert = engine
            .vector_upsert(LanceDbUpsertInput {
                table_name: "docs".to_string(),
                input_format: LanceDbInputFormat::ArrowIpc,
                data,
                key_columns: Vec::new(),
            })
            .await
            .expect("arrow ipc upsert");

        assert_eq!(upsert.input_rows, 1);
        assert_eq!(upsert.inserted_rows, 1);
    }

    /// Verifies an Arrow stream cannot reorder fields while the reader claims the target table schema.
    /// 验证 Arrow 流不能在读取器声明目标表 Schema 时重排字段。
    #[tokio::test]
    async fn arrow_ipc_schema_mismatch_is_refused_before_write() {
        use arrow_array::{Float32Array, RecordBatch, StringArray};
        use arrow_schema::{DataType, Field, Schema};

        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let reordered_schema = Arc::new(Schema::new(vec![
            Field::new("content", DataType::Utf8, false),
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 3),
                false,
            ),
        ]));
        let mut vector_builder =
            arrow_array::builder::FixedSizeListBuilder::new(Float32Array::builder(1), 3);
        vector_builder.values().append_slice(&[1.0, 2.0, 3.0]);
        vector_builder.append(true);
        let batch = RecordBatch::try_new(
            Arc::clone(&reordered_schema),
            vec![
                Arc::new(StringArray::from(vec!["content-value"])),
                Arc::new(StringArray::from(vec!["id-value"])),
                Arc::new(vector_builder.finish()),
            ],
        )
        .expect("build reordered batch");
        let mut writer =
            arrow_ipc::writer::StreamWriter::try_new(Vec::<u8>::new(), reordered_schema.as_ref())
                .expect("ipc writer");
        writer.write(&batch).expect("write batch");
        writer.finish().expect("finish stream");
        let data = writer.into_inner().expect("take bytes");

        let error = engine
            .vector_upsert(LanceDbUpsertInput {
                table_name: "docs".to_string(),
                input_format: LanceDbInputFormat::ArrowIpc,
                data,
                key_columns: Vec::new(),
            })
            .await
            .expect_err("a reordered Arrow schema must be refused");
        assert!(error.to_string().contains("schema does not match"));

        let statistics = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics");
        assert_eq!(
            statistics.row_count, 0,
            "a refused payload must write no rows"
        );
    }

    /// Verifies an empty payload writes no rows and reports the current version.
    /// 验证空载荷不写入任何行并报告当前版本。
    #[tokio::test]
    async fn empty_payload_writes_no_rows() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let upsert = engine
            .vector_upsert(json_upsert("docs", Vec::new()))
            .await
            .expect("empty upsert");

        assert_eq!(upsert.input_rows, 0);
        assert_eq!(upsert.inserted_rows, 0);
        assert_eq!(upsert.message, "no rows to write");
    }

    /// Verifies an unknown key column is refused before any write is attempted.
    /// 验证未知键列在任何写入被尝试之前即被拒绝。
    ///
    /// A caller that misspells a key column used to reach LanceDB's merge and fail there, after the write had been
    /// attempted and with a message about the storage engine rather than about the caller's argument. The row count
    /// assertion is the part that matters: a refusal must leave the table exactly as it was.
    /// 拼错键列的调用方此前会抵达 LanceDB 的合并并在那里失败——那已在写入被尝试之后，且消息谈的是存储引擎而非调用方的
    /// 参数。行数断言才是要紧之处：拒绝必须让表保持原样。
    #[tokio::test]
    async fn an_unknown_key_column_is_refused_before_any_write() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let mut valid = json_upsert("docs", vec![json_row("a", "content", [1.0, 0.0, 0.0])]);
        valid.key_columns = vec!["id".to_string()];
        engine
            .vector_upsert(valid)
            .await
            .expect("a valid keyed upsert");

        let mut invalid = json_upsert("docs", vec![json_row("b", "content", [0.0, 1.0, 0.0])]);
        invalid.key_columns = vec!["missing".to_string()];
        let error = engine
            .vector_upsert(invalid)
            .await
            .expect_err("an unknown key column must be refused");
        assert!(
            error.to_string().contains("missing"),
            "the refusal must name the unknown key column: {error}"
        );

        let stats = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics");
        assert_eq!(
            stats.row_count, 1,
            "a refused upsert must not have written anything"
        );
    }
    /// Verifies creating an already existing table is refused rather than silently replacing it.
    /// 验证对已存在的表再次建表会被拒绝，而不是被静默替换。
    ///
    /// The engine passes `CreateTableMode::Create` unless the caller sets `overwrite_if_exists`, so a caller that
    /// forgot the flag must lose its write rather than the data underneath it. Without this test the safety property
    /// rests on a builder call nobody exercises.
    /// 除调用方设置 `overwrite_if_exists` 外，引擎传入 `CreateTableMode::Create`，因此忘记该标志的调用方应当丢掉它的
    /// 写入，而不是丢掉它下面的数据。缺少本测试时，该安全性质就只依靠一处无人行使的构建器调用。
    #[tokio::test]
    async fn creating_an_existing_table_is_refused_without_the_overwrite_flag() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("first create");
        engine
            .vector_upsert(json_upsert(
                "docs",
                vec![json_row("a", "content", [1.0, 0.0, 0.0])],
            ))
            .await
            .expect("upsert");

        engine
            .create_table(documents_definition("docs"))
            .await
            .expect_err("a second create without the overwrite flag must be refused");

        let stats = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("the original table must survive the refused create");
        assert_eq!(
            stats.row_count, 1,
            "a refused create must leave the existing table and its rows untouched"
        );
    }

    /// Verifies the explicit overwrite flag replaces the table and its rows.
    /// 验证显式覆盖标志会替换该表及其行。
    #[tokio::test]
    async fn the_overwrite_flag_replaces_the_table_and_its_rows() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("first create");
        engine
            .vector_upsert(json_upsert(
                "docs",
                vec![
                    json_row("a", "content", [1.0, 0.0, 0.0]),
                    json_row("b", "content", [0.0, 1.0, 0.0]),
                ],
            ))
            .await
            .expect("upsert");

        let mut overwrite = documents_definition("docs");
        overwrite.overwrite_if_exists = true;
        engine
            .create_table(overwrite)
            .await
            .expect("an explicit overwrite must succeed");

        let stats = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics after overwrite");
        assert_eq!(
            stats.row_count, 0,
            "an overwrite must leave an empty table, not the previous rows"
        );
    }
    /// Verifies a table name carrying a path separator is refused as an authorization failure.
    /// 验证带路径分隔符的表名被作为授权失败拒绝。
    #[tokio::test]
    async fn a_table_name_with_a_separator_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        for name in ["nested/docs", "nested\\docs"] {
            let error = engine
                .create_table(documents_definition(name))
                .await
                .expect_err("a table name with a separator must be refused");
            assert!(
                error.to_string().contains("path separators"),
                "unexpected error for `{name}`: {error}"
            );
        }
    }

    /// Verifies a traversal table name is refused as an authorization failure.
    /// 验证穿越表名被作为授权失败拒绝。
    #[tokio::test]
    async fn a_traversal_table_name_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        for name in ["..", "."] {
            let error = engine
                .create_table(documents_definition(name))
                .await
                .expect_err("a relative path segment must be refused as a table name");
            assert!(
                error.to_string().contains("relative path segment"),
                "unexpected error for `{name}`: {error}"
            );
        }
    }

    /// Verifies an infinite query vector is refused rather than poisoning every distance.
    /// 验证无穷的检索向量被拒绝，而不是污染每个距离。
    ///
    /// The guard runs before the table is opened, so the refusal does not depend on the table existing. Round 73
    /// closed this path at the dispatch, where a skill's JSON is parsed; this closes it at the engine, which is the
    /// boundary a Rust caller reaches and the one round 72 chose for non-finite reals.
    /// 该守卫在打开表之前运行，故拒绝不依赖表是否存在。第 73 轮在分派处（技能 JSON 的解析点）关闭了该路径；此处
    /// 在引擎处关闭它——那是 Rust 调用方抵达的边界，也是第 72 轮为非有限实数选定的边界。
    #[tokio::test]
    async fn an_infinite_query_vector_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        let error = engine
            .vector_search(LanceDbSearchInput {
                table_name: "docs".to_string(),
                vector: vec![1.0, f32::INFINITY, 0.0],
                limit: 10,
                filter: String::new(),
                vector_column: "vector".to_string(),
                output_format: LanceDbOutputFormat::JsonRows,
            })
            .await
            .expect_err("an infinite query vector must be refused");
        assert!(
            error.to_string().contains("vector[1]") && error.to_string().contains("finite"),
            "the refusal must name the index and the reason: {error}"
        );
    }

    /// Verifies an upsert vector component that overflows `f32` is refused before the write.
    /// 验证溢出 `f32` 的上传向量分量在写入之前被拒绝。
    ///
    /// This is the stored-value path round 73 recorded and round 74 left open: `1e300` is legal JSON, becomes
    /// `f32::INFINITY`, and would have been persisted as the row's vector, misleading every later search that
    /// includes it. The row-count assertion is the part that matters.
    /// 这是第 73 轮记录、第 74 轮留待处理的存储取值路径：`1e300` 是合法 JSON，会变成 `f32::INFINITY`，并会作为该行的
    /// 向量被持久化，从而误导此后每一次包含它的检索。行数断言才是要紧之处。
    #[tokio::test]
    async fn an_upsert_vector_that_overflows_f32_is_refused() {
        let (_directory, engine) = isolated_engine(4).await;
        engine
            .create_table(documents_definition("docs"))
            .await
            .expect("create table");

        let error = engine
            .vector_upsert(json_upsert(
                "docs",
                vec![serde_json::json!({
                    "id": "a",
                    "content": "content",
                    "vector": [1.0, 1e300, 0.0],
                })],
            ))
            .await
            .expect_err("a component that overflows f32 must be refused");
        assert!(
            error.to_string().contains("non-finite"),
            "the refusal must say what was wrong: {error}"
        );

        let stats = engine
            .table_statistics(LanceDbTableStatisticsInput {
                table_name: "docs".to_string(),
                compaction: LanceDbCompactionOptions::default(),
            })
            .await
            .expect("statistics");
        assert_eq!(
            stats.row_count, 0,
            "a refused upsert must not have written anything"
        );
    }
}
