//! Host-callback dispatch for the LuaSkills SQLite provider.
//! LuaSkills SQLite provider 的宿主回调分派。
//!
//! When LuaSkills selects `HostCallback` for its SQLite provider it calls one synchronous process-global
//! closure with a [`RuntimeSqliteProviderRequest`] and uses the returned JSON as the skill-visible result.
//! The request and response contract is the one §34 of the migration ledger extracted from the pinned
//! dependency, and it is not free-form: the controller branch of the engine builds the same keys, so a host
//! that answers with a different shape would change what skills observe.
//! 当 LuaSkills 为其 SQLite provider 选择 `HostCallback` 时，它会以一个同步的进程级闭包接收
//! [`RuntimeSqliteProviderRequest`]，并把你返回的 JSON 用作技能可见的结果。请求与响应契约即迁移台账 §34 从
//! 锁定依赖中提取的那套，且它并非自由形式：引擎的控制器分支构造相同的键，因此返回不同形状的宿主会改变技能所
//! 观察到的东西。
//!
//! Every action runs through the bounded host execution channel, so the callback itself never drives a future
//! and the Lua VM thread is never the one doing database work.
//! 每个动作都经由有界宿主执行通道运行，因此回调本身绝不驱动 future，Lua VM 线程也绝不做数据库工作。
//!
//! # Cursors / 游标
//!
//! The four cursor actions are a stateful protocol over an owned registry: `QueryStream` spools the whole
//! statement through the engine and hands out a host-allocated identifier, the metrics and chunk actions read
//! that entry, and `QueryStreamClose` drops it, which is also what deletes the engine's spool file. Chunks are
//! one Arrow IPC stream split at about the requested size, so a skill concatenates them and decodes a stream,
//! exactly as it did against the retired library.
//!
//! 四个游标动作是覆盖一个自持注册表的有状态协议：`QueryStream` 通过引擎暂存整条语句并发放宿主分配的标识符，
//! 指标与分块动作读取该条目，`QueryStreamClose` 丢弃它，而丢弃也正是删除引擎暂存文件的方式。分块是一条 Arrow
//! IPC 流按所请求大小切开，因此技能把它们拼接并解码为一条流，与它针对已退役库所做的完全一致。已完成但空闲超过
//! 五分钟的游标会在下一次 provider 调用时回收，与已退役控制器的资源寿命一致。

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use luaskills::{
    RuntimeDatabaseBindingContext, RuntimeDatabaseKind, RuntimeSqliteProviderAction,
    RuntimeSqliteProviderCallback, RuntimeSqliteProviderRequest,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use vulcan_workspace_store::local_database_runtime::{
    LocalDatabaseRuntime, SqliteFileBinding, SqliteHardeningOptions, SqliteOpenOptions,
    SqlitePragmaOptions,
};
use vulcan_workspace_store::local_sql::{self, QueryStreamResult, TokenizerMode};
use vulcan_workspace_store::luaskills_sql_params::{parse_batch, parse_params};

use super::database_bridge::HostExecutionChannel;

/// Busy timeout for one skill database, in milliseconds.
/// 单个技能数据库的忙等待超时，单位毫秒。
const SKILL_SQLITE_BUSY_TIMEOUT_MS: u64 = 10_000;

/// Cache size for one skill database, in kilobytes.
/// 单个技能数据库的缓存大小，单位千字节。
const SKILL_SQLITE_CACHE_SIZE_KIB: i64 = 16_384;

/// Default FTS search limit when a request omits one, matching the engine's own default.
/// 请求未给出时 FTS 检索的默认条数，与引擎自身默认值一致。
const DEFAULT_SEARCH_FTS_LIMIT: u64 = 10;

/// Idle lifetime of one completed query stream, matching the retired controller.
/// 已完成查询流的空闲存活时间，与已退役控制器一致。
const QUERY_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// One completed query stream and the last time a caller accessed it.
/// 一条已完成的查询流及调用方最后访问它的时间。
struct SkillSqliteStreamEntry {
    /// Shared stream result whose final owner removes the spool file.
    /// 共享的流结果，最后一个持有者负责删除暂存文件。
    stream: Arc<QueryStreamResult>,
    /// Last successful registry access used by idle reaping.
    /// 空闲回收所依据的最后一次成功注册表访问时间。
    last_accessed_at: Instant,
}

/// Dispatches LuaSkills SQLite provider requests to the in-process engine.
/// 把 LuaSkills SQLite provider 请求分派到进程内引擎。
pub struct SkillSqliteProvider {
    channel: Arc<HostExecutionChannel<Arc<LocalDatabaseRuntime>>>,
    streams: Mutex<HashMap<u64, SkillSqliteStreamEntry>>,
    next_stream_id: AtomicU64,
}

impl SkillSqliteProvider {
    /// Builds one provider over an already started host execution channel.
    /// 在一个已启动的宿主执行通道之上构建一个 provider。
    ///
    /// # Parameters / 参数
    /// `channel` is shared with the provider, because the callback is installed process-wide.
    /// `channel` 与 provider 共享，因为该回调是进程级安装的。
    ///
    /// # Returns / 返回值
    /// Returns a ready provider.
    /// 返回一个就绪的 provider。
    pub fn new(channel: Arc<HostExecutionChannel<Arc<LocalDatabaseRuntime>>>) -> Self {
        Self {
            channel,
            streams: Mutex::new(HashMap::new()),
            next_stream_id: AtomicU64::new(1),
        }
    }

    /// Looks one open cursor up by identifier.
    /// 按标识符查找一个已打开的游标。
    ///
    /// # Parameters / 参数
    /// `stream_id` is the identifier the host handed out when the cursor was opened.
    /// `stream_id` 是打开游标时宿主发放的标识符。
    ///
    /// # Returns / 返回值
    /// Returns the shared stream, or an error naming the identifier when it is not open.
    /// 返回共享的流，或在该标识符未打开时命名它的错误。
    fn stream(&self, stream_id: u64) -> Result<Arc<QueryStreamResult>, String> {
        self.streams
            .lock()
            .map_err(|_| String::from("the query stream registry lock was poisoned"))?
            .get_mut(&stream_id)
            .map(|entry| {
                entry.last_accessed_at = Instant::now();
                Arc::clone(&entry.stream)
            })
            .ok_or_else(|| format!("query stream handle not found: {stream_id}"))
    }

    /// Removes completed query streams that exceeded the inherited idle lifetime.
    /// 删除超过继承空闲时限的已完成查询流。
    ///
    /// # Parameters / 参数
    /// `now` is captured by the caller so one sweep compares every entry against the same instant.
    /// `now` 由调用方捕获，使一次清理中的每个条目都与同一时刻比较。
    ///
    /// # Returns / 返回值
    /// Returns the number of removed stream entries, or a poisoned-registry error.
    /// 返回被删除的流条目数，或注册表中毒错误。
    fn reap_stale_streams(&self, now: Instant) -> Result<usize, String> {
        let mut streams = self
            .streams
            .lock()
            .map_err(|_| String::from("the query stream registry lock was poisoned"))?;
        let before = streams.len();
        streams.retain(|_, entry| {
            now.saturating_duration_since(entry.last_accessed_at) < QUERY_STREAM_IDLE_TIMEOUT
        });
        Ok(before.saturating_sub(streams.len()))
    }

    /// Builds the process-global callback value for this provider.
    /// 为本 provider 构建进程级回调值。
    ///
    /// # Parameters / 参数
    /// This method takes no parameters.
    /// 本方法不接收参数。
    ///
    /// # Returns / 返回值
    /// Returns the closure to pass to `luaskills::set_sqlite_provider_callback`.
    /// 返回要传给 `luaskills::set_sqlite_provider_callback` 的闭包。
    pub fn callback(self: &Arc<Self>) -> RuntimeSqliteProviderCallback {
        let provider = Arc::clone(self);
        Arc::new(move |request| provider.dispatch(request))
    }

    /// Dispatches one request to the matching action.
    /// 把单个请求分派到匹配的动作。
    ///
    /// # Parameters / 参数
    /// `request` is the engine's exact request, carrying the action, the binding context and the input.
    /// `request` 是引擎的精确请求，携带动作、绑定上下文与输入。
    ///
    /// # Returns / 返回值
    /// Returns the skill-visible JSON result, or an explicit error.
    /// 返回技能可见的 JSON 结果，或明确错误。
    pub fn dispatch(&self, request: &RuntimeSqliteProviderRequest) -> Result<Value, String> {
        self.reap_stale_streams(Instant::now())?;
        let input = &request.input;
        let binding = sqlite_binding(&request.binding)?;
        let tokenizer_mode = || parse_tokenizer_mode(input);
        match request.action {
            RuntimeSqliteProviderAction::ExecuteScript => {
                let sql = require_string(input, "sql")?;
                let params = parse_params(input)?;
                self.channel
                    .run("execute_script", move |runtime| async move {
                        let result =
                            local_sql::execute_script_async(&runtime, binding, sql, params).await?;
                        // The engine hardcodes one executed statement here, so the host reports the same value
                        // and a skill cannot tell the two modes apart.
                        // 引擎在此硬编码一条已执行语句，因此宿主报告相同取值，技能无法分辨两种模式。
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "rows_changed": result.rows_changed,
                            "last_insert_rowid": result.last_insert_rowid,
                            "statements_executed": 1,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::ExecuteBatch => {
                let sql = require_string(input, "sql")?;
                let items = parse_batch(input)?;
                self.channel
                    .run("execute_batch", move |runtime| async move {
                        let result =
                            local_sql::execute_batch_async(&runtime, binding, sql, items).await?;
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "rows_changed": result.rows_changed,
                            "last_insert_rowid": result.last_insert_rowid,
                            "statements_executed": result.statements_executed,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::QueryJson => {
                let sql = require_string(input, "sql")?;
                let params = parse_params(input)?;
                self.channel.run("query_json", move |runtime| async move {
                    let result =
                        local_sql::query_json_async(&runtime, binding, sql, params).await?;
                    // `rows` is the decoded form of `json_data`; the engine's controller branch returns both,
                    // and a decode failure yields an empty array rather than failing the whole query.
                    // `rows` 是 `json_data` 的解码形式；引擎的控制器分支同时返回二者，而解码失败会产生空数组而非
                    // 让整个查询失败。
                    let rows = serde_json::from_str::<Value>(&result.json_data)
                        .unwrap_or_else(|_| Value::Array(Vec::new()));
                    Ok(json!({
                        "success": true,
                        "row_count": result.row_count,
                        "json_data": result.json_data,
                        "rows": rows,
                    }))
                })
            }
            RuntimeSqliteProviderAction::TokenizeText => {
                let text = require_string(input, "text")?;
                let mode = tokenizer_mode()?;
                let search_mode = input
                    .get("search_mode")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.channel
                    .run("tokenize_text", move |runtime| async move {
                        let result =
                            local_sql::tokenize_async(&runtime, binding, mode, text, search_mode)
                                .await?;
                        Ok(json!({
                            "success": true,
                            "tokenizer_mode": result.tokenizer_mode,
                            "normalized_text": result.normalized_text,
                            "fts_query": result.fts_query,
                            "tokens": result.tokens,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::UpsertCustomWord => {
                let word = require_string(input, "word")?;
                let weight = input.get("weight").and_then(Value::as_u64).unwrap_or(1);
                self.channel
                    .run("upsert_custom_word", move |runtime| async move {
                        let result = local_sql::upsert_custom_word_async(
                            &runtime,
                            binding,
                            word.clone(),
                            weight as usize,
                        )
                        .await?;
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "affected_rows": result.affected_rows,
                            "word": word,
                            "weight": weight,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::RemoveCustomWord => {
                let word = require_string(input, "word")?;
                self.channel
                    .run("remove_custom_word", move |runtime| async move {
                        let result =
                            local_sql::remove_custom_word_async(&runtime, binding, word.clone())
                                .await?;
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "affected_rows": result.affected_rows,
                            "word": word,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::ListCustomWords => {
                self.channel
                    .run("list_custom_words", move |runtime| async move {
                        let result = local_sql::list_custom_words_async(&runtime, binding).await?;
                        let total = result.words.len();
                        let words = result
                            .words
                            .into_iter()
                            .map(|entry| json!({ "word": entry.word, "weight": entry.weight }))
                            .collect::<Vec<_>>();
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "total": total,
                            "words": words,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::EnsureFtsIndex => {
                let index_name = require_string(input, "index_name")?;
                let mode = tokenizer_mode()?;
                self.channel
                    .run("ensure_fts_index", move |runtime| async move {
                        let result =
                            local_sql::ensure_fts_index_async(&runtime, binding, index_name, mode)
                                .await?;
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "index_name": result.index_name,
                            "tokenizer_mode": result.tokenizer_mode,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::RebuildFtsIndex => {
                let index_name = require_string(input, "index_name")?;
                let mode = tokenizer_mode()?;
                self.channel
                    .run("rebuild_fts_index", move |runtime| async move {
                        let result =
                            local_sql::rebuild_fts_index_async(&runtime, binding, index_name, mode)
                                .await?;
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "index_name": result.index_name,
                            "tokenizer_mode": result.tokenizer_mode,
                            "reindexed_rows": result.reindexed_rows,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::UpsertFtsDocument => {
                let index_name = require_string(input, "index_name")?;
                let mode = tokenizer_mode()?;
                let id = require_string(input, "id")?;
                let file_path = require_string(input, "file_path")?;
                let title = require_string(input, "title")?;
                let content = require_string(input, "content")?;
                self.channel
                    .run("upsert_fts_document", move |runtime| async move {
                        let result = local_sql::upsert_fts_document_async(
                            &runtime,
                            binding,
                            index_name,
                            mode,
                            id.clone(),
                            file_path,
                            title,
                            content,
                        )
                        .await?;
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "affected_rows": result.affected_rows,
                            "index_name": result.index_name,
                            "id": id,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::DeleteFtsDocument => {
                let index_name = require_string(input, "index_name")?;
                let id = require_string(input, "id")?;
                self.channel
                    .run("delete_fts_document", move |runtime| async move {
                        let result = local_sql::delete_fts_document_async(
                            &runtime,
                            binding,
                            index_name,
                            id.clone(),
                        )
                        .await?;
                        Ok(json!({
                            "success": result.success,
                            "message": result.message,
                            "affected_rows": result.affected_rows,
                            "index_name": result.index_name,
                            "id": id,
                        }))
                    })
            }
            RuntimeSqliteProviderAction::SearchFts => {
                let index_name = require_string(input, "index_name")?;
                let mode = tokenizer_mode()?;
                let query = require_string(input, "query")?;
                let limit = input
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(DEFAULT_SEARCH_FTS_LIMIT)
                    .min(u64::from(u32::MAX)) as u32;
                let offset = input
                    .get("offset")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .min(u64::from(u32::MAX)) as u32;
                self.channel.run("search_fts", move |runtime| async move {
                    let result = local_sql::search_fts_async(
                        &runtime, binding, index_name, mode, query, limit, offset,
                    )
                    .await?;
                    let hits = result
                        .hits
                        .into_iter()
                        .map(|hit| {
                            json!({
                                "id": hit.id,
                                "file_path": hit.file_path,
                                "title": hit.title,
                                "title_highlight": hit.title_highlight,
                                "content_snippet": hit.content_snippet,
                                "score": hit.score,
                                "rank": hit.rank,
                                "raw_score": hit.raw_score,
                            })
                        })
                        .collect::<Vec<_>>();
                    Ok(json!({
                        "success": result.success,
                        "message": result.message,
                        "index_name": result.index_name,
                        "tokenizer_mode": result.tokenizer_mode,
                        "normalized_query": result.normalized_query,
                        "fts_query": result.fts_query,
                        "source": result.source,
                        "query_mode": result.query_mode,
                        "total": result.total,
                        "hits": hits,
                    }))
                })
            }
            RuntimeSqliteProviderAction::QueryStream => {
                let sql = require_string(input, "sql")?;
                let params = parse_params(input)?;
                let target_chunk_size = parse_chunk_size(input);
                let binding = sqlite_binding(&request.binding)?;
                let stream =
                    self.channel
                        .run("sqlite_query_stream", move |runtime| async move {
                            local_sql::query_stream_async(
                                &runtime,
                                binding,
                                sql,
                                params,
                                target_chunk_size,
                            )
                            .await
                        })?;
                let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
                let (row_count, chunk_count, total_bytes) =
                    (stream.row_count, stream.chunk_count, stream.total_bytes);
                self.streams
                    .lock()
                    .map_err(|_| String::from("the query stream registry lock was poisoned"))?
                    .insert(
                        stream_id,
                        SkillSqliteStreamEntry {
                            stream: Arc::new(stream),
                            last_accessed_at: Instant::now(),
                        },
                    );
                // Metrics are ready immediately because the engine spools the whole statement before it
                // returns: a cursor here is a handle over a finished result, not a live statement.
                // 指标立即可用，因为引擎在返回前已暂存整条语句：此处的游标是覆盖一个已完成结果的句柄，而非活动语句。
                Ok(json!({
                    "success": true,
                    "stream_id": stream_id,
                    "metrics_ready": true,
                    "row_count": row_count,
                    "chunk_count": chunk_count,
                    "total_bytes": total_bytes,
                }))
            }
            RuntimeSqliteProviderAction::QueryStreamWaitMetrics => {
                let stream_id = require_stream_id(input)?;
                let stream = self.stream(stream_id)?;
                Ok(json!({
                    "success": true,
                    "stream_id": stream_id,
                    "metrics_ready": true,
                    "row_count": stream.row_count,
                    "chunk_count": stream.chunk_count,
                    "total_bytes": stream.total_bytes,
                }))
            }
            RuntimeSqliteProviderAction::QueryStreamChunk => {
                let stream_id = require_stream_id(input)?;
                let index = input
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| String::from("field `index` is required"))?;
                let index = usize::try_from(index)
                    .map_err(|_| String::from("field `index` exceeds the platform range"))?;
                // The stream is cloned out of the registry so the bounded file read happens outside the lock,
                // and so a concurrent close cannot delete the spool file under an in-flight read.
                // 流被克隆出注册表，使有界的文件读取发生在锁之外，也使并发的关闭不会在读取进行中删除暂存文件。
                let stream = self.stream(stream_id)?;
                let chunk = stream
                    .read_chunk(index)
                    .map_err(|error| error.to_string())?;
                Ok(json!({
                    "success": true,
                    "stream_id": stream_id,
                    "index": index,
                    "byte_count": chunk.len(),
                    "chunk_base64": BASE64_STANDARD.encode(&chunk),
                }))
            }
            RuntimeSqliteProviderAction::QueryStreamClose => {
                let stream_id = require_stream_id(input)?;
                let removed = self
                    .streams
                    .lock()
                    .map_err(|_| String::from("the query stream registry lock was poisoned"))?
                    .remove(&stream_id)
                    .is_some();
                // The wording mirrors the retired library's, so a skill reading the message sees no change.
                // 措辞镜像已退役库的措辞，使读取该消息的技能看不到变化。
                Ok(json!({
                    "success": removed,
                    "stream_id": stream_id,
                    "message": if removed {
                        format!("query_stream handle {stream_id} closed successfully")
                    } else {
                        format!("query_stream handle {stream_id} was not found or was already closed")
                    },
                }))
            }
        }
    }
}

/// Builds the host-derived SQLite binding for one skill request.
/// 为单个技能请求构建宿主派生的 SQLite 绑定。
///
/// The path comes from the engine's own binding context, which the host populated; a skill cannot supply a
/// path, and this function refuses a context that names no database rather than inventing one.
/// 路径来自引擎自身的绑定上下文，而该上下文由宿主填充；技能无法提供路径，且本函数会拒绝未命名数据库的上下文而非
/// 凭空编造一个。
///
/// # Parameters / 参数
/// `binding` is the engine's binding context.
/// `binding` 是引擎的绑定上下文。
///
/// # Returns / 返回值
/// Returns the binding, or an explicit error when the context is for the other database kind or names no path.
/// 返回该绑定；当上下文属于另一种数据库种类或未命名路径时返回明确错误。
fn require_stream_id(input: &Value) -> Result<u64, String> {
    input
        .get("stream_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| String::from("field `stream_id` is required"))
}

/// Parses the requested cursor chunk size.
/// 解析所请求的游标分块大小。
///
/// # Parameters / 参数
/// `input` is the request input.
/// `input` 是请求输入。
///
/// # Returns / 返回值
/// Returns the requested size, or the engine's default when the field is absent or zero. Both key spellings are
/// accepted because the engine accepts both, and the engine clamps whatever it receives to its own minimum.
/// 返回所请求的大小；字段缺失或为零时返回引擎的默认值。两种键拼写都被接受，因为引擎二者都接受，且引擎会把收到的
/// 值钳制到自身的最小值。
fn parse_chunk_size(input: &Value) -> usize {
    input
        .get("chunk_bytes")
        .or_else(|| input.get("chunk_size"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(local_sql::DEFAULT_IPC_CHUNK_BYTES)
}

/// Builds the SQLite binding for one skill request.
/// 为单个技能请求构建 SQLite 绑定。
///
/// # Parameters / 参数
/// `binding` is the engine's binding context.
/// `binding` 是引擎的绑定上下文。
///
/// # Returns / 返回值
/// Returns the file binding, or an explicit error when the context is for the other kind or names no file.
/// 返回文件绑定；当上下文属于另一种种类或未命名文件时返回明确错误。
fn sqlite_binding(binding: &RuntimeDatabaseBindingContext) -> Result<SqliteFileBinding, String> {
    if binding.database_kind != RuntimeDatabaseKind::Sqlite {
        return Err(format!(
            "LuaSkills SQLite provider received a {:?} binding for skill `{}`.",
            binding.database_kind, binding.skill_id
        ));
    }
    let path = &binding.default_database_path;
    if path.trim().is_empty() {
        return Err(format!(
            "LuaSkills SQLite provider received no database path for skill `{}`.",
            binding.skill_id
        ));
    }
    Ok(SqliteFileBinding {
        path: std::path::PathBuf::from(path),
        options: SqliteOpenOptions {
            busy_timeout_ms: SKILL_SQLITE_BUSY_TIMEOUT_MS,
            pragmas: SqlitePragmaOptions {
                journal_mode: String::from("WAL"),
                synchronous: String::from("NORMAL"),
                foreign_keys: true,
                temp_store: String::from("MEMORY"),
                wal_autocheckpoint_pages: 1_000,
                cache_size_kib: SKILL_SQLITE_CACHE_SIZE_KIB,
                mmap_size_bytes: 0,
            },
            hardening: SqliteHardeningOptions {
                enforce_db_file_lock: true,
                read_only: false,
                trusted_schema: false,
                defensive: true,
            },
        },
    })
}

/// Reads one required non-empty string field, mirroring the engine's own reader.
/// 读取一个必填非空字符串字段，与引擎自身的读取器一致。
///
/// # Parameters / 参数
/// `input` is the request input and `field` the key.
/// `input` 是请求输入，`field` 是键。
///
/// # Returns / 返回值
/// Returns the trimmed-non-empty text, or an explicit error naming the field.
/// 返回去除首尾空白后非空的文本，或命名该字段的明确错误。
fn require_string(input: &Value, field: &str) -> Result<String, String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing or empty field `{field}`"))
}

/// Reads the tokenizer mode from either accepted spelling.
/// 从任一可接受的拼写读取分词器模式。
///
/// # Parameters / 参数
/// `input` is the request input.
/// `input` 是请求输入。
///
/// # Returns / 返回值
/// Returns the mode, defaulting to `None` exactly as the engine does, or an explicit error for an unknown name.
/// 返回该模式，与引擎一样默认到 `None`；对未知名称返回明确错误。
fn parse_tokenizer_mode(input: &Value) -> Result<TokenizerMode, String> {
    let name = input
        .get("tokenizer_mode")
        .or_else(|| input.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    match name.trim().to_ascii_lowercase().as_str() {
        "none" => Ok(TokenizerMode::None),
        "jieba" => Ok(TokenizerMode::Jieba),
        other => Err(format!("unknown tokenizer mode `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaskills::RuntimeDatabaseBindingContextSpec;
    use std::collections::HashMap;

    /// Builds one binding context for the given kind and path.
    /// 为给定种类与路径构建一个绑定上下文。
    fn binding_context(kind: RuntimeDatabaseKind, path: &str) -> RuntimeDatabaseBindingContext {
        RuntimeDatabaseBindingContext::new(RuntimeDatabaseBindingContextSpec {
            space_label: String::from("space"),
            skill_id: String::from("skill"),
            root_name: String::from("root"),
            space_root: String::from("space-root"),
            skill_dir: String::from("skill-dir"),
            skill_dir_name: String::from("skill-dir-name"),
            database_kind: kind,
            default_database_path: path.to_string(),
        })
    }

    /// Builds one request for the given action and input.
    /// 为给定动作与输入构建一个请求。
    fn request(
        action: RuntimeSqliteProviderAction,
        path: &str,
        input: Value,
    ) -> RuntimeSqliteProviderRequest {
        RuntimeSqliteProviderRequest {
            action,
            binding: binding_context(RuntimeDatabaseKind::Sqlite, path),
            input,
        }
    }

    /// Owns the state one provider test needs, including the runtime that must outlive the channel.
    /// 持有单个 provider 测试所需的状态，含必须比通道存活更久的运行时。
    ///
    /// The [`tokio::runtime::Runtime`] is held here on purpose: a runtime handle does not keep its runtime
    /// alive, and a dropped runtime cancels the blocking work the channel already submitted. That is a real
    /// property of the design, discovered by this test failing, not a test-only concern.
    /// 此处刻意持有 [`tokio::runtime::Runtime`]：运行时句柄不会延长其运行时的寿命，而运行时被丢弃会取消通道已
    /// 提交的阻塞工作。这是设计的真实性质，由本测试的失败发现，而非仅测试相关的问题。
    struct Harness {
        _directory: tempfile::TempDir,
        _runtime: tokio::runtime::Runtime,
        provider: Arc<SkillSqliteProvider>,
    }

    /// Builds one provider over a real in-process runtime rooted at a temporary directory.
    /// 在一个以临时目录为根的进程内真实运行时之上构建一个 provider。
    fn provider() -> Harness {
        let directory = tempfile::tempdir().expect("create skill database directory");
        let runtime =
            Arc::new(LocalDatabaseRuntime::open(HashMap::new()).expect("open in-process runtime"));
        // Independent scheduler workers keep channel work progressing while the synchronous caller waits.
        // 独立调度线程在同步调用方等待期间持续推进通道工作。
        let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("build database test runtime");
        let channel = Arc::new(HostExecutionChannel::start(
            tokio_runtime.handle().clone(),
            runtime,
        ));
        Harness {
            _directory: directory,
            _runtime: tokio_runtime,
            provider: Arc::new(SkillSqliteProvider::new(Arc::clone(&channel))),
        }
    }

    /// Verifies the required-field and tokenizer-mode readers.
    /// 验证必填字段与分词器模式读取器。
    #[test]
    fn readers_reject_missing_fields_and_accept_both_mode_spellings() {
        let input = json!({});
        assert!(require_string(&input, "sql").is_err());
        assert!(require_string(&json!({ "sql": "   " }), "sql").is_err());
        assert_eq!(
            require_string(&json!({ "sql": "SELECT 1" }), "sql").unwrap(),
            "SELECT 1"
        );

        assert_eq!(
            parse_tokenizer_mode(&json!({})).unwrap(),
            TokenizerMode::None
        );
        assert_eq!(
            parse_tokenizer_mode(&json!({ "mode": "Jieba" })).unwrap(),
            TokenizerMode::Jieba
        );
        assert_eq!(
            parse_tokenizer_mode(&json!({ "tokenizer_mode": "none" })).unwrap(),
            TokenizerMode::None
        );
        assert!(parse_tokenizer_mode(&json!({ "mode": "mecab" })).is_err());
    }

    /// Verifies the binding builder refuses an empty path and preserves significant whitespace.
    /// 验证绑定构建器拒绝空路径并保留有意义的空白。
    #[test]
    fn binding_builder_refuses_an_empty_path() {
        let empty = binding_context(RuntimeDatabaseKind::Sqlite, "   ");
        assert!(
            sqlite_binding(&empty).is_err(),
            "an empty path must be refused"
        );

        let valid = binding_context(RuntimeDatabaseKind::Sqlite, "skill.sqlite3");
        let binding = sqlite_binding(&valid).expect("build binding");
        assert_eq!(binding.path.to_string_lossy(), "skill.sqlite3");
        assert_eq!(
            binding.options.busy_timeout_ms,
            SKILL_SQLITE_BUSY_TIMEOUT_MS
        );
        assert!(binding.options.hardening.enforce_db_file_lock);

        let spaced = sqlite_binding(&binding_context(
            RuntimeDatabaseKind::Sqlite,
            " skill.sqlite3 ",
        ))
        .expect("significant path whitespace must be preserved");
        assert_eq!(spaced.path.to_string_lossy(), " skill.sqlite3 ");
    }

    /// Verifies the cursor protocol end to end: open, metrics, one chunk, and two closes.
    /// 验证游标协议的端到端：打开、指标、一个分块，以及两次关闭。
    ///
    /// The concatenated chunks are one Arrow IPC stream, and this test does not decode them: the engine's own
    /// suite already decodes its spooled chunks back through `StreamReader`, and a magic-byte assertion here
    /// would prove nothing about validity. What this test owns is the registry protocol around them.
    /// 拼接后的分块是一条 Arrow IPC 流，而本测试不解码它们：引擎自身的测试套件已把其暂存分块通过 `StreamReader`
    /// 解码回来，而此处断言魔术字节无法证明有效性。本测试负责的是围绕它们的注册表协议。
    #[test]
    fn cursor_actions_stream_rows_through_the_registry() {
        let harness = provider();
        let provider = Arc::clone(&harness.provider);
        let path = harness._directory.path().join("cursor.sqlite3");
        let path_text = path.to_string_lossy().to_string();

        provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::ExecuteScript,
                &path_text,
                json!({ "sql": "CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL);" }),
            ))
            .expect("create the table");
        provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::ExecuteBatch,
                &path_text,
                json!({
                    "sql": "INSERT INTO notes(id, body) VALUES (?1, ?2);",
                    "items": [[1, "first"], [2, "second"]]
                }),
            ))
            .expect("insert rows");

        let opened = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStream,
                &path_text,
                json!({ "sql": "SELECT id, body FROM notes ORDER BY id", "chunk_bytes": 4096 }),
            ))
            .expect("open the cursor");
        assert_eq!(opened["success"], true);
        assert_eq!(opened["metrics_ready"], true);
        assert_eq!(opened["row_count"], 2);
        let stream_id = opened["stream_id"]
            .as_u64()
            .expect("a host-allocated stream id");
        let chunk_count = opened["chunk_count"].as_u64().expect("chunk count");
        assert!(
            chunk_count >= 1,
            "a two-row stream must have at least one chunk"
        );
        assert!(
            opened["total_bytes"].as_u64().unwrap_or(0) > 0,
            "the spooled stream must have bytes"
        );

        let metrics = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStreamWaitMetrics,
                &path_text,
                json!({ "stream_id": stream_id }),
            ))
            .expect("read the metrics");
        assert_eq!(metrics["metrics_ready"], true);
        assert_eq!(metrics["row_count"], 2);
        assert_eq!(metrics["chunk_count"], chunk_count);

        let chunk = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStreamChunk,
                &path_text,
                json!({ "stream_id": stream_id, "index": 0 }),
            ))
            .expect("read chunk zero");
        assert_eq!(chunk["index"], 0);
        let encoded = chunk["chunk_base64"].as_str().expect("a base64 chunk");
        let decoded = BASE64_STANDARD.decode(encoded).expect("valid base64");
        assert_eq!(chunk["byte_count"].as_u64(), Some(decoded.len() as u64));
        assert!(!decoded.is_empty(), "a chunk must carry bytes");

        // An index beyond the stream is the engine's own state error, surfaced rather than hidden.
        // 超出流范围的下标是引擎自身的状态错误，被如实暴露而非隐藏。
        let beyond = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStreamChunk,
                &path_text,
                json!({ "stream_id": stream_id, "index": chunk_count }),
            ))
            .expect_err("an index beyond the stream must fail");
        assert!(!beyond.is_empty(), "the error must say something");

        let closed = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStreamClose,
                &path_text,
                json!({ "stream_id": stream_id }),
            ))
            .expect("close the cursor");
        assert_eq!(closed["success"], true);

        let reopened = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStreamClose,
                &path_text,
                json!({ "stream_id": stream_id }),
            ))
            .expect("a second close is reported, not raised");
        assert_eq!(reopened["success"], false);

        let missing = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStreamWaitMetrics,
                &path_text,
                json!({ "stream_id": stream_id }),
            ))
            .expect_err("a closed cursor must not report metrics");
        assert!(missing.contains("not found"), "unexpected error: {missing}");
    }

    /// Verifies an abandoned completed cursor is reaped on the next provider call.
    /// 验证被遗弃的已完成游标会在下一次 provider 调用时被回收。
    #[test]
    fn stale_cursor_is_reaped_and_its_spool_is_removed() {
        let harness = provider();
        let provider = Arc::clone(&harness.provider);
        let path = harness._directory.path().join("stale-cursor.sqlite3");
        let path_text = path.to_string_lossy().to_string();
        let opened = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStream,
                &path_text,
                json!({ "sql": "SELECT 1 AS value" }),
            ))
            .expect("open cursor");
        let stream_id = opened["stream_id"].as_u64().expect("stream id");

        let spool_path = {
            let mut streams = provider.streams.lock().expect("lock stream registry");
            let entry = streams.get_mut(&stream_id).expect("stream entry");
            entry.last_accessed_at =
                Instant::now() - QUERY_STREAM_IDLE_TIMEOUT - Duration::from_secs(1);
            entry.stream.spool_path().to_path_buf()
        };
        assert!(spool_path.exists(), "spool must exist before reaping");

        let closed = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryStreamClose,
                &path_text,
                json!({ "stream_id": stream_id }),
            ))
            .expect("the close action also performs stale reaping");
        assert_eq!(
            closed["success"], false,
            "the stale entry was already reaped"
        );
        assert!(!spool_path.exists(), "reaping must remove the spool file");
    }

    /// Verifies a real execute, query and batch round trip through the channel and the engine.
    /// 验证经通道与引擎的真实执行、查询与批次往返。
    #[test]
    fn executes_and_queries_through_the_host_channel() {
        let harness = provider();
        let provider = Arc::clone(&harness.provider);
        let path = harness._directory.path().join("sqlite-round-trip.sqlite3");
        let path_text = path.to_string_lossy().to_string();

        // The engine refuses bound parameters for a multi-statement script, so schema and rows are separate
        // calls; that is the engine's own rule, and a skill meets it too.
        // 引擎拒绝为多语句脚本绑定参数，因此建表与写入是两次调用；这是引擎自身的规则，技能也会遇到。
        let created = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::ExecuteScript,
                &path_text,
                json!({ "sql": "CREATE TABLE notes(id INTEGER PRIMARY KEY, body TEXT NOT NULL);" }),
            ))
            .expect("create the table through the host");
        assert_eq!(created["success"], true);
        assert_eq!(created["statements_executed"], 1);

        let inserted = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::ExecuteScript,
                &path_text,
                json!({
                    "sql": "INSERT INTO notes(id, body) VALUES (?1, ?2);",
                    "params": [1, "first note"]
                }),
            ))
            .expect("insert through the host");
        assert_eq!(inserted["success"], true);
        assert_eq!(inserted["rows_changed"], 1);

        let queried = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::QueryJson,
                &path_text,
                json!({ "sql": "SELECT id, body FROM notes ORDER BY id" }),
            ))
            .expect("query through the host");
        assert_eq!(queried["row_count"], 1);
        let rows: Value = serde_json::from_str(
            queried["json_data"]
                .as_str()
                .expect("the row payload is JSON text"),
        )
        .expect("decode the row payload");
        assert_eq!(rows[0]["body"], "first note");

        let batch = provider
            .dispatch(&request(
                RuntimeSqliteProviderAction::ExecuteBatch,
                &path_text,
                json!({
                    "sql": "INSERT INTO notes(id, body) VALUES (?1, ?2);",
                    "items": [[2, "second note"], [3, "third note"]]
                }),
            ))
            .expect("batch through the host");
        assert_eq!(batch["success"], true);
        assert_eq!(batch["statements_executed"], 2);
    }
}
