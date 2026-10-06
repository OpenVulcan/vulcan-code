//! Column, payload-format and operation payload types for the in-process LanceDB engine.
//! 进程内 LanceDB 引擎的列、载荷格式与操作载荷类型。
//!
//! Every type here is owned by this crate. The migrated source already had local types at this
//! layer, so this module is a near-verbatim port; the substantive change is that
//! [`LanceDbOutputFormat::as_str`] replaces the upstream `as_wire_name` now that no wire protocol
//! exists to name.
//!
//! 此处的每个类型都由本 crate 拥有。迁入源码在该层本就使用本地类型，因此本模块基本为原样迁入；
//! 实质变化是 [`LanceDbOutputFormat::as_str`] 取代了上游的 `as_wire_name` —— 既然已不存在传输协议，
//! 也就没有"wire"需要命名。

/// Supported column types.
/// 支持的列类型。
///
/// The set is deliberately closed. There is no binary, blob or struct member, which is what keeps
/// the v1 Lance format baseline reachable: `lancedb` force-bumps a table to Lance file version
/// `V2_2` when its schema carries a `lance.blob.v2` column, and that upgrade is forbidden here.
///
/// 该集合刻意封闭。其中没有二进制、Blob 或结构体成员，这正是 v1 Lance 格式基线得以保持的原因：
/// 当表 Schema 带有 `lance.blob.v2` 列时，`lancedb` 会强制把表提升到 Lance 文件版本 `V2_2`，
/// 而此处禁止该升级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanceDbColumnType {
    /// No declared type; the caller must specify one.
    /// 未声明类型；调用方必须指定。
    #[default]
    Unspecified,
    /// UTF-8 string column.
    /// UTF-8 字符串列。
    String,
    /// Signed 64-bit integer column.
    /// 有符号 64 位整数列。
    Int64,
    /// 64-bit floating point column.
    /// 64 位浮点列。
    Float64,
    /// Boolean column.
    /// 布尔列。
    Bool,
    /// Fixed-size list of 32-bit floats, used for embeddings.
    /// 32 位浮点定长列表，用于向量嵌入。
    VectorFloat32,
    /// 32-bit floating point column.
    /// 32 位浮点列。
    Float32,
    /// Unsigned 64-bit integer column.
    /// 无符号 64 位整数列。
    Uint64,
    /// Signed 32-bit integer column.
    /// 有符号 32 位整数列。
    Int32,
    /// Unsigned 32-bit integer column.
    /// 无符号 32 位整数列。
    Uint32,
}

/// Definition of one column when creating a table.
/// 建表时一个列的定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanceDbColumnDef {
    /// Column name.
    /// 列名。
    pub name: String,
    /// Declared column type.
    /// 声明的列类型。
    pub column_type: LanceDbColumnType,
    /// Vector dimension, meaningful only for [`LanceDbColumnType::VectorFloat32`].
    /// 向量维度，仅对 [`LanceDbColumnType::VectorFloat32`] 有意义。
    pub vector_dim: u32,
    /// Whether the column accepts null.
    /// 该列是否接受空值。
    pub nullable: bool,
}

/// Payload format accepted for incoming data.
/// 接受的数据载荷格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanceDbInputFormat {
    /// No format declared; the caller must specify one.
    /// 未声明格式；调用方必须指定。
    #[default]
    Unspecified,
    /// JSON array of row objects.
    /// 行对象的 JSON 数组。
    JsonRows,
    /// Arrow IPC stream bytes.
    /// Arrow IPC 流字节。
    ArrowIpc,
}

/// Encoding applied to returned rows.
/// 返回行的编码方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanceDbOutputFormat {
    /// No format declared; resolves to Arrow IPC, matching upstream behaviour.
    /// 未声明格式；解析为 Arrow IPC，与上游行为一致。
    #[default]
    Unspecified,
    /// Arrow IPC stream bytes.
    /// Arrow IPC 流字节。
    ArrowIpc,
    /// JSON array of row objects.
    /// 行对象的 JSON 数组。
    JsonRows,
}

impl LanceDbOutputFormat {
    /// Returns the stable format label.
    /// 返回稳定的格式名称。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified | Self::ArrowIpc => "arrow_ipc",
            Self::JsonRows => "json",
        }
    }
}

/// Input for creating one table.
/// 建一张表的输入。
#[derive(Debug, Clone)]
pub struct LanceDbCreateTableInput {
    /// Target table name.
    /// 目标表名。
    pub table_name: String,
    /// Column definitions in order.
    /// 按顺序排列的列定义。
    pub columns: Vec<LanceDbColumnDef>,
    /// Whether an existing table of the same name is replaced.
    /// 是否替换同名的既有表。
    pub overwrite_if_exists: bool,
}

/// Result of creating one table.
/// 建表结果。
#[derive(Debug, Clone)]
pub struct LanceDbCreateTableResult {
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
}

/// Input for upserting rows into one table.
/// 向一张表写入行的输入。
#[derive(Debug, Clone)]
pub struct LanceDbUpsertInput {
    /// Target table name.
    /// 目标表名。
    pub table_name: String,
    /// Encoding of `data`.
    /// `data` 的编码格式。
    pub input_format: LanceDbInputFormat,
    /// Encoded rows.
    /// 已编码的行数据。
    pub data: Vec<u8>,
    /// Columns forming the row identity used to decide insert versus update.
    /// 构成行身份、用于判断插入或更新的列。
    pub key_columns: Vec<String>,
}

/// Result of upserting rows.
/// 写入行结果。
#[derive(Debug, Clone)]
pub struct LanceDbUpsertResult {
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Authoritative table version after the write.
    /// 写入后的权威表版本。
    pub version: u64,
    /// Number of rows present in the input payload.
    /// 输入载荷中的行数。
    pub input_rows: u64,
    /// Number of rows inserted.
    /// 插入的行数。
    pub inserted_rows: u64,
    /// Number of rows updated.
    /// 更新的行数。
    pub updated_rows: u64,
    /// Number of rows deleted as part of the upsert.
    /// 作为写入一部分删除的行数。
    pub deleted_rows: u64,
}

/// Input for one vector search.
/// 一次向量检索的输入。
#[derive(Debug, Clone)]
pub struct LanceDbSearchInput {
    /// Target table name.
    /// 目标表名。
    pub table_name: String,
    /// Query vector.
    /// 查询向量。
    pub vector: Vec<f32>,
    /// Maximum number of rows to return.
    /// 返回行数上限。
    pub limit: u32,
    /// Optional SQL filter expression applied before ranking.
    /// 排序前应用的可选 SQL 过滤表达式。
    pub filter: String,
    /// Vector column to search.
    /// 待检索的向量列。
    pub vector_column: String,
    /// Encoding for the returned rows.
    /// 返回行的编码格式。
    pub output_format: LanceDbOutputFormat,
}

/// Result of one vector search.
/// 一次向量检索的结果。
#[derive(Debug, Clone)]
pub struct LanceDbSearchResult {
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Encoding actually applied to `data`.
    /// `data` 实际采用的编码格式。
    pub format: LanceDbOutputFormat,
    /// Number of rows returned.
    /// 返回行数。
    pub rows: u64,
    /// Encoded rows.
    /// 已编码的行数据。
    pub data: Vec<u8>,
}

/// Input for deleting rows.
/// 删除行的输入。
#[derive(Debug, Clone)]
pub struct LanceDbDeleteInput {
    /// Target table name.
    /// 目标表名。
    pub table_name: String,
    /// SQL predicate selecting the rows to delete.
    /// 选择待删除行的 SQL 谓词。
    pub condition: String,
}

/// Result of deleting rows.
/// 删除行结果。
#[derive(Debug, Clone)]
pub struct LanceDbDeleteResult {
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
    /// Authoritative table version after the delete.
    /// 删除后的权威表版本。
    pub version: u64,
    /// Number of rows deleted.
    /// 删除的行数。
    pub deleted_rows: u64,
}

/// Input for dropping one table.
/// 删除一张表的输入。
#[derive(Debug, Clone)]
pub struct LanceDbDropTableInput {
    /// Target table name.
    /// 目标表名。
    pub table_name: String,
}

/// Result of dropping one table.
/// 删表结果。
#[derive(Debug, Clone)]
pub struct LanceDbDropTableResult {
    /// Human-readable outcome message.
    /// 可读结果消息。
    pub message: String,
}

/// Input identifying the table to inspect.
/// 标识待检查表的输入。
#[derive(Debug, Clone)]
pub struct LanceDbTableStatisticsInput {
    /// Stable table name to inspect.
    /// 需要检查的稳定表名。
    pub table_name: String,
    /// Compaction options the caller would execute; the native planner sizes its plan with them.
    /// 调用方将要执行的整理参数；原生计划器以其确定计划规模。
    pub compaction: LanceDbCompactionOptions,
}

/// Fragment row-count summary reported by LanceDB.
/// LanceDB 返回的片段行数摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanceDbFragmentSummary {
    /// Minimum row count among current fragments.
    /// 当前片段中的最小行数。
    pub minimum_rows: u64,
    /// Maximum row count among current fragments.
    /// 当前片段中的最大行数。
    pub maximum_rows: u64,
    /// Mean row count among current fragments.
    /// 当前片段的平均行数。
    pub mean_rows: u64,
    /// Twenty-fifth percentile fragment row count.
    /// 片段行数的第二十五百分位数。
    pub p25_rows: u64,
    /// Median fragment row count.
    /// 片段行数中位数。
    pub p50_rows: u64,
    /// Seventy-fifth percentile fragment row count.
    /// 片段行数的第七十五百分位数。
    pub p75_rows: u64,
    /// Ninety-ninth percentile fragment row count.
    /// 片段行数的第九十九百分位数。
    pub p99_rows: u64,
}

/// Physical table statistics used to decide whether compaction is beneficial.
/// 用于判断物理整理是否有收益的表统计信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanceDbTableStatisticsResult {
    /// Stable table name represented by these statistics.
    /// 这些统计信息对应的稳定表名。
    pub table_name: String,
    /// Authoritative Lance table version observed with the statistics.
    /// 与统计信息一同观测到的权威 Lance 表版本。
    pub version: u64,
    /// Total physical table size in bytes.
    /// 表的物理总字节数。
    pub total_bytes: u64,
    /// Logical row count visible in the current table version.
    /// 当前表版本中可见的逻辑行数。
    pub row_count: u64,
    /// Number of physical indices attached to the table.
    /// 表所附加的物理索引数量。
    pub index_count: u64,
    /// Number of data fragments in the current table version.
    /// 当前表版本的数据片段数量。
    pub fragment_count: u64,
    /// Number of fragments LanceDB classifies as small; informational only.
    /// LanceDB 判定为小片段的数量；仅供展示。
    pub small_fragment_count: u64,
    /// Tasks the native compaction planner would run with the requested options; the only
    /// authority on whether compaction is still needed.
    /// 原生整理计划器按所请求参数将执行的任务数；是否仍需整理的唯一权威。
    pub compaction_task_count: u64,
    /// Distribution summary of rows per current fragment.
    /// 当前每个片段行数的分布摘要。
    pub fragment_rows: LanceDbFragmentSummary,
}

/// Bounded native compaction options shared by planning and execution.
/// 计划与执行共用的有界原生整理参数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LanceDbCompactionOptions {
    /// Optional desired row count for each output fragment.
    /// 可选的每个输出片段目标行数。
    pub target_rows_per_fragment: Option<usize>,
    /// Optional maximum rows read into one compaction group.
    /// 可选的单个整理组最大读取行数。
    pub max_rows_per_group: Option<usize>,
    /// Optional maximum physical size of each output file.
    /// 可选的每个输出文件最大物理字节数。
    pub max_bytes_per_file: Option<usize>,
    /// Optional bounded worker-thread count, never greater than eight.
    /// 可选且有界的工作线程数，绝不超过八。
    pub num_threads: Option<usize>,
    /// Optional row batch size used while rewriting fragments.
    /// 重写片段时使用的可选行批次大小。
    pub batch_size: Option<usize>,
    /// Optional maximum source-fragment count admitted in one operation.
    /// 单次操作允许接纳的可选源片段数上限。
    pub max_source_fragments: Option<usize>,
}

/// Safe file-compaction input with optional bounded LanceDB overrides.
/// 安全文件合并输入，包含可选且有界的 LanceDB 参数覆盖。
#[derive(Debug, Clone)]
pub struct LanceDbCompactTableInput {
    /// Stable table name to compact.
    /// 需要整理的稳定表名。
    pub table_name: String,
    /// Optional optimistic-concurrency version that must still be current.
    /// 可选的乐观并发版本，执行时必须仍为当前版本。
    pub expected_version: Option<u64>,
    /// Options for this operation; the reported statistics are planned with the same options.
    /// 本次操作的参数；所报告统计信息以同一参数计划。
    pub options: LanceDbCompactionOptions,
}

/// Physical file-compaction metrics returned by LanceDB.
/// LanceDB 返回的物理文件合并指标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanceDbCompactionMetrics {
    /// Number of source fragments removed from the new table version.
    /// 新表版本中移除的源片段数量。
    pub fragments_removed: u64,
    /// Number of replacement fragments added to the new table version.
    /// 新表版本中新增的替换片段数量。
    pub fragments_added: u64,
    /// Number of source files removed from the new table version.
    /// 新表版本中移除的源文件数量。
    pub files_removed: u64,
    /// Number of replacement files added to the new table version.
    /// 新表版本中新增的替换文件数量。
    pub files_added: u64,
}

/// File-compaction result including authoritative before-and-after statistics.
/// 文件合并结果，包含权威的整理前后统计信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanceDbCompactTableResult {
    /// Human-readable operation outcome.
    /// 可读的操作结果说明。
    pub message: String,
    /// Authoritative table statistics immediately before compaction.
    /// 整理前立即读取的权威表统计信息。
    pub before: LanceDbTableStatisticsResult,
    /// Authoritative table statistics immediately after compaction.
    /// 整理后立即读取的权威表统计信息。
    pub after: LanceDbTableStatisticsResult,
    /// Physical fragment and file changes reported by LanceDB.
    /// LanceDB 报告的物理片段与文件变化。
    pub metrics: LanceDbCompactionMetrics,
}

#[cfg(test)]
mod tests {
    use super::{LanceDbColumnType, LanceDbOutputFormat};

    /// Verifies an undeclared output format resolves to Arrow IPC, matching upstream.
    /// 验证未声明的输出格式解析为 Arrow IPC，与上游一致。
    #[test]
    fn unspecified_output_format_resolves_to_arrow_ipc() {
        assert_eq!(LanceDbOutputFormat::Unspecified.as_str(), "arrow_ipc");
        assert_eq!(LanceDbOutputFormat::ArrowIpc.as_str(), "arrow_ipc");
        assert_eq!(LanceDbOutputFormat::JsonRows.as_str(), "json");
    }

    /// Verifies the column type set stays closed: no binary or blob member exists, which is what
    /// keeps the table schema from forcing a Lance `V2_2` upgrade.
    /// 验证列类型集合保持封闭：不存在二进制或 Blob 成员，从而使表 Schema 不会强制 Lance `V2_2` 升级。
    #[test]
    fn column_type_set_has_no_blob_member() {
        let all = [
            LanceDbColumnType::Unspecified,
            LanceDbColumnType::String,
            LanceDbColumnType::Int64,
            LanceDbColumnType::Float64,
            LanceDbColumnType::Bool,
            LanceDbColumnType::VectorFloat32,
            LanceDbColumnType::Float32,
            LanceDbColumnType::Uint64,
            LanceDbColumnType::Int32,
            LanceDbColumnType::Uint32,
        ];

        // The exhaustive match below fails to compile if a variant is added, so this test is the
        // place a future blob-like column would have to be acknowledged deliberately.
        // 若新增变体，下方穷尽匹配将无法编译，因此本测试是未来任何类 Blob 列必须被有意确认的位置。
        for column_type in all {
            let name = match column_type {
                LanceDbColumnType::Unspecified => "unspecified",
                LanceDbColumnType::String => "string",
                LanceDbColumnType::Int64 => "int64",
                LanceDbColumnType::Float64 => "float64",
                LanceDbColumnType::Bool => "bool",
                LanceDbColumnType::VectorFloat32 => "vector_f32",
                LanceDbColumnType::Float32 => "float32",
                LanceDbColumnType::Uint64 => "uint64",
                LanceDbColumnType::Int32 => "int32",
                LanceDbColumnType::Uint32 => "uint32",
            };
            assert!(
                !name.contains("blob") && !name.contains("bin"),
                "column type {name} must not introduce a blob-shaped column"
            );
        }

        assert_eq!(LanceDbColumnType::default(), LanceDbColumnType::Unspecified);
    }
}
