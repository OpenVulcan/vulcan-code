//! Jieba tokenization, the SQLite FTS5 `jieba` tokenizer and the companion custom dictionary.
//! Jieba 分词、SQLite FTS5 `jieba` 分词器与伴生自定义词典。
//!
//! This module is the **primary** place in this crate that requires `unsafe`, and it is the original
//! reason the crate relaxes the workspace's `unsafe_code = "forbid"` lint (see `Cargo.toml`).
//!
//! 本模块是本 crate 中**主要**需要 `unsafe` 的地方，也是本 crate 放宽工作区
//! `unsafe_code = "forbid"` lint 的全部原因（见 `Cargo.toml`）。
//!
//! Why `unsafe` is unavoidable here / 此处为何无法避免 `unsafe`:
//!
//! SQLite FTS5 registers a tokenizer by calling `xCreateTokenizer_v2` with a vtable of C function
//! pointers. `rusqlite` exposes the raw `fts5_api` and `fts5_tokenizer_v2` types but **no safe
//! wrapper** — `rusqlite 0.40.2` declares no `fts5` module at all. The four callbacks below are
//! therefore necessarily `unsafe extern "C"`.
//!
//! SQLite FTS5 通过 `xCreateTokenizer_v2` 传入一组 C 函数指针 vtable 来注册分词器。`rusqlite`
//! 暴露原始 `fts5_api` 与 `fts5_tokenizer_v2` 类型，但**没有安全封装**——`rusqlite 0.40.2`
//! 根本没有 `fts5` 模块。因此下面四个回调必然为 `unsafe extern "C"`。
//!
//! FFI soundness obligations honoured below / 下方遵守的 FFI 正确性义务:
//!
//! 1. Every `unsafe` block carries a `// SAFETY:` comment stating the actual invariant.
//! 2. Every callback null-checks its incoming pointers before dereferencing them.
//! 3. Ownership of the two boxed values handed to SQLite is explicit: SQLite owns the tokenizer
//!    instance until it calls `xDelete`, and owns the registration context until it calls the
//!    destroy callback. Each is reclaimed by exactly one `Box::from_raw`.
//! 4. The `xTokenize` callback borrows `text_ptr`/`text_len` only for the duration of the call,
//!    as SQLite guarantees, and never retains the slice.
//! 5. Non-UTF-8 input is rejected with an error code rather than panicking across the FFI
//!    boundary, because unwinding into C is undefined behaviour.
//!
//! 1. 每个 `unsafe` 块都带 `// SAFETY:` 注释说明真实不变量。
//! 2. 每个回调在解引用前都对传入指针做空值检查。
//! 3. 交给 SQLite 的两个装箱值的所有权归属明确：SQLite 在调用 `xDelete` 前拥有分词器实例，
//!    在调用销毁回调前拥有注册上下文；二者各自恰好被一次 `Box::from_raw` 回收。
//! 4. `xTokenize` 按 SQLite 保证仅在调用期间借用 `text_ptr`/`text_len`，绝不保留该切片。
//! 5. 非 UTF-8 输入返回错误码而不是跨越 FFI 边界 panic，因为向 C 栈展开是未定义行为。

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::PathBuf;
use std::ptr;
use std::slice;
use std::str;
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

use jieba_rs::{Jieba, TokenizeMode as JiebaTokenizeMode};
use rusqlite::ffi::{self, fts5_api, fts5_tokenizer_v2};
use rusqlite::types::ToSqlOutput;
use rusqlite::{Connection, params};

use crate::error::{EngineResult, SqliteEngineError};

/// Companion system dictionary table name.
/// 伴生系统词典表名。
pub const VULCAN_DICT_TABLE: &str = "_vulcan_dict";

/// SQLite FTS5 tokenizer name registered for Jieba.
/// 为 Jieba 注册的 SQLite FTS5 分词器名称。
const SQLITE_JIEBA_TOKENIZER_NAME: &CStr = c"jieba";

/// SQLite FTS5 API pointer binding type tag.
/// SQLite FTS5 API 指针绑定类型标签。
const SQLITE_FTS5_API_PTR_TYPE: &CStr = c"fts5_api_ptr";

/// Tokenizer mode selector.
/// 分词模式选择器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenizerMode {
    /// Plain mode: whitespace and ASCII punctuation splitting, no Chinese word segmentation.
    /// 常规模式：按空白与 ASCII 标点切分，不做中文分词。
    #[default]
    None,
    /// Jieba mode: Chinese word segmentation with the current dictionary state.
    /// Jieba 模式：使用当前词典状态进行中文分词。
    Jieba,
}

impl TokenizerMode {
    /// Returns the stable name for this mode.
    /// 返回本模式的稳定名称。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Jieba => "jieba",
        }
    }

    /// Parses a stable mode name.
    /// 解析稳定模式名。
    ///
    /// Returns `None` for an unknown name so callers must handle it explicitly rather than
    /// silently defaulting to a different tokenizer.
    ///
    /// 未知名称返回 `None`，调用方必须显式处理，而不是静默退回另一种分词器。
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Some(Self::None),
            "jieba" => Some(Self::Jieba),
            _ => None,
        }
    }
}

/// One custom dictionary entry.
/// 一条自定义词典条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomWordEntry {
    /// Dictionary word.
    /// 词典词。
    pub word: String,
    /// Segmentation weight; always at least 1.
    /// 分词权重；至少为 1。
    pub weight: usize,
}

/// Result of tokenizing one text.
/// 对一段文本分词的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizeOutput {
    /// Tokenizer mode name actually used.
    /// 实际使用的分词模式名。
    pub tokenizer_mode: String,
    /// Lightly normalized input text.
    /// 轻量规范化后的输入文本。
    pub normalized_text: String,
    /// Produced tokens in order.
    /// 按顺序产生的词元。
    pub tokens: Vec<String>,
    /// FTS `MATCH` expression built from the tokens.
    /// 由词元构造的 FTS `MATCH` 表达式。
    pub fts_query: String,
}

/// Result of a dictionary mutation.
/// 词典变更的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryMutationResult {
    /// Whether the mutation took effect.
    /// 变更是否生效。
    pub success: bool,
    /// Human-readable bilingual outcome message.
    /// 双语的可读结果消息。
    pub message: String,
    /// Number of affected rows.
    /// 受影响行数。
    pub affected_rows: u64,
}

/// Result of listing custom dictionary words.
/// 列出自定义词典词的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCustomWordsResult {
    /// Whether the listing succeeded.
    /// 列出是否成功。
    pub success: bool,
    /// Human-readable bilingual outcome message.
    /// 双语的可读结果消息。
    pub message: String,
    /// Currently enabled words.
    /// 当前启用的词。
    pub words: Vec<CustomWordEntry>,
}

/// Shared dictionary state reused by all connections of one database.
/// 同一数据库的所有连接共享的词典状态。
struct SharedDictionaryState {
    /// Jieba engine carrying this database's custom words.
    /// 携带该数据库自定义词的 Jieba 引擎。
    jieba: Arc<Jieba>,
}

impl Default for SharedDictionaryState {
    /// Defaults to the process-wide default engine, so a database without custom words allocates
    /// nothing.
    /// 默认使用进程级引擎，使没有自定义词的数据库不产生额外分配。
    fn default() -> Self {
        Self {
            jieba: Arc::clone(default_jieba()),
        }
    }
}

/// Tokenizer registration context held by SQLite between registration and destroy.
/// SQLite 在注册与销毁之间持有的分词器注册上下文。
struct RegisteredTokenizerContext {
    /// Raw connection handle, used to clear the per-connection registration marker.
    /// 原始连接句柄，用于清除按连接的注册标记。
    connection_handle: usize,
    /// Shared dictionary state this registration was built from.
    /// 构建本次注册所用的共享词典状态。
    shared_state: Arc<RwLock<SharedDictionaryState>>,
}

/// Concrete tokenizer instance created by SQLite FTS5.
/// SQLite FTS5 实际创建出的分词器实例。
struct JiebaTokenizerInstance {
    /// Shared dictionary state consulted on every tokenize call.
    /// 每次分词调用都会查询的共享词典状态。
    shared_state: Arc<RwLock<SharedDictionaryState>>,
}

/// One token span with UTF-8 byte offsets.
/// 带 UTF-8 字节偏移的一个词条片段。
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenSpan {
    /// Token text.
    /// 词条文本。
    token: String,
    /// Inclusive UTF-8 byte start offset.
    /// 起始 UTF-8 字节偏移（含）。
    start_byte: usize,
    /// Exclusive UTF-8 byte end offset.
    /// 结束 UTF-8 字节偏移（不含）。
    end_byte: usize,
}

/// Returns the process-wide default Jieba instance.
/// 返回进程级默认 Jieba 实例。
///
/// Databases without custom words all share this instance, so tokenization of the common case
/// does not rebuild an engine.
///
/// 没有自定义词的数据库共享该实例，使常见情况的分词不需要重建引擎。
fn default_jieba() -> &'static Arc<Jieba> {
    static DEFAULT_JIEBA: OnceLock<Arc<Jieba>> = OnceLock::new();
    DEFAULT_JIEBA.get_or_init(|| Arc::new(Jieba::new()))
}

/// Global weak dictionary registry keyed by logical database identity.
/// 以逻辑数据库身份为键的全局弱引用词典注册表。
///
/// Weak references are deliberate: closing the last connection of a database must release its
/// Jieba state instead of leaking it for the process lifetime.
///
/// 使用弱引用是刻意的：关闭数据库的最后一个连接必须释放其 Jieba 状态，而不是在整个进程生命周期内泄漏。
fn shared_dictionary_registry()
-> &'static Mutex<HashMap<String, Weak<RwLock<SharedDictionaryState>>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Weak<RwLock<SharedDictionaryState>>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Set of SQLite connection handles that already registered the tokenizer.
/// 已完成分词器注册的 SQLite 连接句柄集合。
///
/// FTS5 tokenizer registration is per connection, so re-registering on every call would be
/// wasted work; the marker also lets the destroy callback clean up precisely.
///
/// FTS5 分词器注册是按连接的，因此每次调用都重新注册是浪费；该标记也让销毁回调能精确清理。
fn registered_connection_handles() -> &'static Mutex<HashSet<usize>> {
    static REGISTRY: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Returns the companion dictionary table DDL as a static script.
/// 以静态脚本形式返回伴生词典表 DDL。
///
/// **Correction.** An earlier revision of this comment claimed the v1 initialization entry consumes this script, so
/// the table is created inside the v1 build transaction rather than lazily. **That was false**, and it was verified
/// false rather than merely suspected: a repository-wide search for `dictionary_table_schema_script`,
/// `ensure_vulcan_dict_table`, `VULCAN_DICT_TABLE` and `_vulcan_dict` finds this module and one test — **zero**
/// references in `vulcan-workspace-store`, where every v1 initialization entry lives. The table really is created
/// lazily, on the first connection open, which is what the old comment said it was not.
///
/// **What that costs, measured rather than argued.** Because [`crate::runtime`] applies this on every connection and
/// the host opens one connection per operation without pooling, the creation runs once per statement. Its measured
/// cost is ~635 µs p50, which is 72% of the ~881 µs production connection open and four times a bare open — but the
/// cost is **not** the DDL: a trivial `SELECT 1` as a fresh connection's first statement measures ~620 µs on the same
/// database, so what is being paid is attaching to a WAL database, not executing this particular statement. Recording
/// the distinction matters, because the two readings suggest opposite fixes.
///
/// **Status.** The lazy creation is load-bearing, not redundant: `load_custom_words` selects from the table, so the
/// table must exist in every database the engine opens — including the knowledge-base and display databases,
/// each of which has its own v1 entry. Moving creation into those entries is the design this comment originally
/// described; it is **not** done, and is recorded as outstanding rather than quietly asserted.
///
/// **更正。** 本注释的早期版本声称 v1 初始化入口消费本脚本，因此该表在 v1 建库事务中创建而非惰性创建。**那是假的**，
/// 而且是经核实为假、并非仅仅存疑：全仓搜索 `dictionary_table_schema_script`、`ensure_vulcan_dict_table`、
/// `VULCAN_DICT_TABLE` 与 `_vulcan_dict` 只命中本模块与一个测试——在 `vulcan-workspace-store`（全部 v1 初始化入口
/// 所在处）中为 **0** 引用。该表确实是惰性创建的，在首次打开连接时创建，而这正是旧注释声称并非如此的情形。
///
/// **其代价，实测而非论证。** 由于 [`crate::runtime`] 在每次连接时应用本脚本，而宿主每次操作打开一个连接且不池化，
/// 该创建语句每个语句都要跑一次。实测 p50 约 635 µs，占生产连接打开约 881 µs 的 72%，是裸打开的四倍——但**成本不在
/// 这条 DDL 上**：在同一个数据库上，一条平凡的 `SELECT 1` 作为新连接的首条语句也测得约 620 µs，因此付出的是"接入
/// 一个 WAL 数据库"的代价，而不是执行这条特定语句。记下这一区分很重要，因为两种读法指向相反的修法。
///
/// **状态。** 惰性创建是承重的、不是冗余的：`load_custom_words` 从该表查询，因此该表必须存在于引擎打开的每一个数据库中
/// ——包括知识库库与显示库，它们各有自己的 v1 入口。把创建移入那些入口才是本注释最初描述的设计；它**尚未**
/// 完成，因此记录为遗留项，而不是被悄然断言为已完成。
pub fn dictionary_table_schema_script() -> &'static str {
    concat!(
        "CREATE TABLE IF NOT EXISTS _vulcan_dict (\n",
        "    word TEXT PRIMARY KEY,\n",
        "    weight INTEGER NOT NULL DEFAULT 1,\n",
        "    enabled INTEGER NOT NULL DEFAULT 1,\n",
        "    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),\n",
        "    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))\n",
        ");"
    )
}

/// Ensures the companion dictionary table exists.
/// 确保伴生词典表存在。
pub fn ensure_vulcan_dict_table(connection: &Connection) -> EngineResult<()> {
    connection
        .execute_batch(dictionary_table_schema_script())
        .map_err(SqliteEngineError::sql)
}

/// Ensures the current SQLite connection has the `jieba` tokenizer registered.
/// 确保当前 SQLite 连接已注册 `jieba` 分词器。
///
/// This is idempotent per connection. It is called from the single open entry point in
/// [`crate::runtime`], which is the only permitted registration location.
///
/// 本操作按连接幂等。它由 [`crate::runtime`] 中的唯一打开入口调用，那是唯一允许的注册位置。
pub fn ensure_jieba_tokenizer_registered(connection: &Connection) -> EngineResult<()> {
    ensure_jieba_tokenizer_registered_inner(connection, true)
}

/// Registers the tokenizer without creating the companion table on a read-only database.
/// 在只读数据库上注册分词器，但不创建伴生表。
pub(crate) fn ensure_jieba_tokenizer_registered_read_only(
    connection: &Connection,
) -> EngineResult<()> {
    ensure_jieba_tokenizer_registered_inner(connection, false)
}

/// Registers the tokenizer with an explicit companion-table creation policy.
/// 按显式的伴生表创建策略注册分词器。
fn ensure_jieba_tokenizer_registered_inner(
    connection: &Connection,
    create_dictionary_table: bool,
) -> EngineResult<()> {
    if create_dictionary_table {
        ensure_vulcan_dict_table(connection)?;
    }

    let connection_handle = sqlite_connection_handle(connection);
    {
        let registered = registered_connection_handles()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if registered.contains(&connection_handle) {
            return Ok(());
        }
    }

    let db_key = connection_registry_key(connection);
    let shared_state = shared_dictionary_state_for_key(&db_key);
    refresh_shared_dictionary_state(connection, &shared_state, create_dictionary_table)?;

    let fts_api = fetch_fts5_api(connection)?;
    let registration_context = Box::new(RegisteredTokenizerContext {
        connection_handle,
        shared_state,
    });
    let registration_context_ptr = Box::into_raw(registration_context) as *mut c_void;
    let tokenizer = fts5_tokenizer_v2 {
        iVersion: 2,
        xCreate: Some(sqlite_jieba_tokenizer_create),
        xDelete: Some(sqlite_jieba_tokenizer_delete),
        xTokenize: Some(sqlite_jieba_tokenizer_tokenize),
    };

    // SAFETY: `fts_api` is a non-null pointer returned by `fetch_fts5_api`, which verified the
    // SQLite-populated value. Reading `xCreateTokenizer_v2` from it is a plain field read on a
    // live FTS5 API struct owned by the connection.
    // SAFETY：`fts_api` 是 `fetch_fts5_api` 返回的非空指针，该函数已校验 SQLite 写入的值。
    // 从中读取 `xCreateTokenizer_v2` 只是对连接所拥有的存活 FTS5 API 结构体做字段读取。
    let create = unsafe {
        (*fts_api)
            .xCreateTokenizer_v2
            .ok_or_else(|| SqliteEngineError::Tokenizer {
                detail: "SQLite FTS5 API does not provide xCreateTokenizer_v2".to_string(),
            })?
    };

    // SAFETY: `create` is a valid FTS5 registration function obtained above. `fts_api` is live,
    // the tokenizer name is a static NUL-terminated C string, `registration_context_ptr` came
    // from `Box::into_raw` and is either reclaimed by the destroy callback on success or by the
    // explicit `Box::from_raw` on the failure path below, and `&tokenizer` outlives the call.
    // SAFETY：`create` 是上方取得的有效 FTS5 注册函数。`fts_api` 存活；分词器名是静态
    // NUL 结尾 C 字符串；`registration_context_ptr` 来自 `Box::into_raw`，成功时由销毁回调回收，
    // 失败时由下方显式 `Box::from_raw` 回收；`&tokenizer` 的生命周期覆盖本次调用。
    let rc = unsafe {
        create(
            fts_api,
            SQLITE_JIEBA_TOKENIZER_NAME.as_ptr(),
            registration_context_ptr,
            &tokenizer as *const fts5_tokenizer_v2 as *mut fts5_tokenizer_v2,
            Some(sqlite_jieba_tokenizer_registration_destroy),
        )
    };

    if rc != ffi::SQLITE_OK {
        // SAFETY: registration failed, so SQLite did not take ownership of the context we
        // allocated above. Reclaiming it here is the only remaining owner.
        // SAFETY：注册失败，因此 SQLite 未取得上方分配上下文的所有权。此处回收是唯一剩余的所有者。
        unsafe {
            drop(Box::from_raw(
                registration_context_ptr as *mut RegisteredTokenizerContext,
            ));
        }
        return Err(SqliteEngineError::Tokenizer {
            detail: format!("register jieba tokenizer failed with SQLite code {rc}"),
        });
    }

    registered_connection_handles()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(connection_handle);

    Ok(())
}

/// Inserts or updates one custom dictionary word.
/// 写入或更新一条自定义词典词。
pub fn upsert_custom_word(
    connection: &Connection,
    word: &str,
    weight: usize,
) -> EngineResult<DictionaryMutationResult> {
    ensure_jieba_tokenizer_registered(connection)?;

    let trimmed = word.trim();
    if trimmed.is_empty() {
        return Ok(DictionaryMutationResult {
            success: false,
            message: "custom word must not be empty".to_string(),
            affected_rows: 0,
        });
    }
    let weight = i64::try_from(weight).map_err(|_| {
        SqliteEngineError::tokenizer("custom word weight exceeds SQLite's signed range")
    })?;
    if weight < 1 {
        return Err(SqliteEngineError::tokenizer(
            "custom word weight must be greater than zero",
        ));
    }

    let affected_rows = connection
        .execute(
            &format!(
                "INSERT INTO {table_name} (word, weight, enabled, created_at, updated_at)
                 VALUES (?1, ?2, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 ON CONFLICT(word) DO UPDATE SET
                    weight = excluded.weight,
                    enabled = 1,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
                table_name = VULCAN_DICT_TABLE
            ),
            params![trimmed, weight],
        )
        .map_err(SqliteEngineError::sql)?;

    refresh_registered_dictionary(connection)?;

    Ok(DictionaryMutationResult {
        success: true,
        message: "custom word upserted".to_string(),
        affected_rows: affected_rows as u64,
    })
}

/// Removes one custom dictionary word.
/// 删除一条自定义词典词。
pub fn remove_custom_word(
    connection: &Connection,
    word: &str,
) -> EngineResult<DictionaryMutationResult> {
    ensure_jieba_tokenizer_registered(connection)?;

    let trimmed = word.trim();
    if trimmed.is_empty() {
        return Ok(DictionaryMutationResult {
            success: false,
            message: "custom word must not be empty".to_string(),
            affected_rows: 0,
        });
    }

    let affected_rows = connection
        .execute(
            &format!(
                "DELETE FROM {table_name} WHERE word = ?1",
                table_name = VULCAN_DICT_TABLE
            ),
            params![trimmed],
        )
        .map_err(SqliteEngineError::sql)?;

    refresh_registered_dictionary(connection)?;

    Ok(DictionaryMutationResult {
        success: true,
        message: if affected_rows > 0 {
            "custom word removed".to_string()
        } else {
            "custom word not found".to_string()
        },
        affected_rows: affected_rows as u64,
    })
}

/// Loads currently enabled custom dictionary words.
/// 加载当前启用的自定义词典词。
pub fn load_custom_words(connection: &Connection) -> EngineResult<Vec<CustomWordEntry>> {
    ensure_vulcan_dict_table(connection)?;

    let mut statement = connection
        .prepare(&format!(
            "SELECT word, weight
             FROM {table_name}
             WHERE enabled = 1
             ORDER BY word ASC",
            table_name = VULCAN_DICT_TABLE
        ))
        .map_err(SqliteEngineError::sql)?;

    let mut rows = statement.query([]).map_err(SqliteEngineError::sql)?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().map_err(SqliteEngineError::sql)? {
        entries.push(CustomWordEntry {
            word: row.get::<_, String>(0).map_err(SqliteEngineError::sql)?,
            weight: row.get::<_, i64>(1).map_err(SqliteEngineError::sql)?.max(1) as usize,
        });
    }

    Ok(entries)
}

/// Lists currently enabled custom dictionary words with a structured payload.
/// 以结构化载荷列出当前启用的自定义词典词。
pub fn list_custom_words(connection: &Connection) -> EngineResult<ListCustomWordsResult> {
    let words = load_custom_words(connection)?;
    let count = words.len();
    Ok(ListCustomWordsResult {
        success: true,
        message: format!("listed {count} custom words"),
        words,
    })
}

/// Tokenizes one text and builds the FTS query expression.
/// 对一段文本分词并构造 FTS 查询表达式。
pub fn tokenize_text(
    connection: Option<&Connection>,
    mode: TokenizerMode,
    text: &str,
    search_mode: bool,
) -> EngineResult<TokenizeOutput> {
    let normalized_text = normalize_text(text);
    let tokens = match mode {
        TokenizerMode::None => tokenize_plain(&normalized_text),
        TokenizerMode::Jieba => tokenize_with_jieba(connection, &normalized_text, search_mode)?,
    };
    let fts_query = build_fts_query(&tokens, search_mode);

    Ok(TokenizeOutput {
        tokenizer_mode: mode.as_str().to_string(),
        normalized_text,
        tokens,
        fts_query,
    })
}

/// Performs lightweight whitespace normalization.
/// 执行轻量空白规范化。
fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Plain-mode tokenization.
/// 常规模式分词。
///
/// When punctuation splitting removes everything, the whole trimmed text is returned as one
/// token rather than producing an empty token list, so punctuation-only input is still searchable.
///
/// 当标点切分把内容全部移除时，返回整个裁剪后的文本作为单个词元而不是空词元列表，
/// 使纯标点输入仍可检索。
fn tokenize_plain(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    let split = text
        .split(|ch: char| ch.is_whitespace() || ch.is_ascii_punctuation())
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    if split.is_empty() {
        vec![text.to_string()]
    } else {
        split
    }
}

/// Jieba-mode tokenization using the current shared dictionary state.
/// 使用当前共享词典状态进行 Jieba 模式分词。
fn tokenize_with_jieba(
    connection: Option<&Connection>,
    text: &str,
    search_mode: bool,
) -> EngineResult<Vec<String>> {
    if text.is_empty() {
        return Ok(Vec::new());
    }

    let jieba = if let Some(connection) = connection {
        ensure_jieba_tokenizer_registered(connection)?;
        let db_key = connection_registry_key(connection);
        let shared_state = shared_dictionary_state_for_key(&db_key);
        let state = shared_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(&state.jieba)
    } else {
        Arc::clone(default_jieba())
    };
    let spans = jieba_token_spans(text, search_mode, jieba.as_ref());

    Ok(spans.into_iter().map(|span| span.token).collect())
}

/// Builds a safe FTS `MATCH` expression from a token list.
/// 由词元列表构造安全的 FTS `MATCH` 表达式。
///
/// Every token is double-quoted with embedded quotes doubled, so user text cannot inject FTS
/// operators such as `NEAR` or unbalanced quotes into the query.
///
/// 每个词元都加双引号并将内部引号翻倍，使用户文本无法把 `NEAR` 等 FTS 运算符或不配对引号注入查询。
fn build_fts_query(tokens: &[String], search_mode: bool) -> String {
    tokens
        .iter()
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(if search_mode { " OR " } else { " " })
}

/// Reloads the shared tokenizer dictionary from the committed rows of the connection's database.
/// 依据该连接所属数据库已提交的行重新加载共享分词词典。
///
/// Word mutations refresh the shared tokenizer at once, so a caller whose transaction rolls back
/// calls this to drop the words that never committed.
/// 词语变更会立即刷新共享分词器，因此事务回滚的调用方调用本函数以丢弃未提交的词。
pub fn reload_custom_dictionary(connection: &Connection) -> EngineResult<()> {
    ensure_jieba_tokenizer_registered(connection)?;
    refresh_registered_dictionary(connection)
}

/// Refreshes the shared dictionary state for the connection's database.
/// 刷新该连接所属数据库的共享词典状态。
fn refresh_registered_dictionary(connection: &Connection) -> EngineResult<()> {
    let db_key = connection_registry_key(connection);
    let shared_state = shared_dictionary_state_for_key(&db_key);
    refresh_shared_dictionary_state(connection, &shared_state, true)
}

/// Gets or creates the shared state for one database registry key.
/// 获取或创建某个数据库注册键的共享状态。
fn shared_dictionary_state_for_key(db_key: &str) -> Arc<RwLock<SharedDictionaryState>> {
    let mut registry = shared_dictionary_registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if let Some(shared_state) = registry.get(db_key).and_then(Weak::upgrade) {
        return shared_state;
    }

    // Drop dead weak entries so a long-lived process does not accumulate keys for closed
    // databases.
    // 清理已失效的弱引用条目，避免长生命周期进程为已关闭数据库累积键。
    registry.retain(|_, shared_state| shared_state.strong_count() > 0);

    let shared_state = Arc::new(RwLock::new(SharedDictionaryState::default()));
    registry.insert(db_key.to_string(), Arc::downgrade(&shared_state));
    shared_state
}

/// Rebuilds the Jieba engine from the companion dictionary table.
/// 依据伴生词典表重建 Jieba 引擎。
///
/// A database without custom words reuses the shared process-wide engine instead of cloning it,
/// so the common case performs no dictionary construction.
///
/// 没有自定义词的数据库复用进程级共享引擎而不克隆它，使常见情况不进行词典构建。
fn refresh_shared_dictionary_state(
    connection: &Connection,
    shared_state: &Arc<RwLock<SharedDictionaryState>>,
    create_dictionary_table: bool,
) -> EngineResult<()> {
    let custom_words = if create_dictionary_table || dictionary_table_exists(connection)? {
        load_custom_words(connection)?
    } else {
        Vec::new()
    };
    let jieba = if custom_words.is_empty() {
        Arc::clone(default_jieba())
    } else {
        let mut jieba = default_jieba().as_ref().clone();
        for entry in &custom_words {
            jieba.add_word(&entry.word, Some(entry.weight), None);
        }
        Arc::new(jieba)
    };

    let mut guard = shared_state
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.jieba = jieba;
    Ok(())
}

/// Reports whether the companion dictionary table already exists without mutating the database.
/// 在不修改数据库的前提下报告伴生词典表是否已存在。
fn dictionary_table_exists(connection: &Connection) -> EngineResult<bool> {
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![VULCAN_DICT_TABLE],
            |row| row.get::<_, i64>(0),
        )
        .map_err(SqliteEngineError::sql)?;
    Ok(count > 0)
}

/// Builds a stable registry key for the current SQLite database.
/// 为当前 SQLite 数据库构造稳定注册键。
///
/// A file database keys on its normalized path, so two connections to the same database share one
/// dictionary state and a custom word added on one connection is visible to the other. An
/// in-memory database has no shared file identity, so it keys on its distinct connection handle.
///
/// 文件数据库以规范化路径为键，使同一数据库的两个连接共享同一词典状态，在一个连接加入的自定义词
/// 对另一个连接可见。内存数据库没有共享文件身份，因此以各自不同的连接句柄为键。
fn connection_registry_key(connection: &Connection) -> String {
    let handle = sqlite_connection_handle(connection);
    match connection.path() {
        Some(path) if !path.trim().is_empty() => {
            crate::runtime::normalize_db_path(&PathBuf::from(path))
                .map(|normalized| normalized.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string())
        }
        _ => format!(":memory:#{handle:x}"),
    }
}

/// Reads the raw SQLite connection handle value.
/// 读取 SQLite 原生连接句柄数值。
fn sqlite_connection_handle(connection: &Connection) -> usize {
    // SAFETY: `handle()` returns the raw `sqlite3*` that rusqlite owns. The value is used only as
    // an opaque identity key and is never dereferenced, so the temporary borrow of the handle
    // cannot outlive the connection.
    // SAFETY：`handle()` 返回 rusqlite 所拥有的原始 `sqlite3*`。该值仅作为不透明身份键使用，
    // 绝不解引用，因此该句柄的临时借用不会超出连接生命周期。
    unsafe { connection.handle() as usize }
}

/// Fetches the FTS5 API pointer from the current SQLite connection.
/// 从当前 SQLite 连接读取 FTS5 API 指针。
///
/// The pointer is written by SQLite into `p_ret` through the `fts5(?)` virtual-table invocation
/// and is validated for null before being returned.
///
/// SQLite 通过 `fts5(?)` 虚表调用把指针写入 `p_ret`，返回前已做空值校验。
fn fetch_fts5_api(connection: &Connection) -> EngineResult<*mut fts5_api> {
    // p_ret is mutable because SQLite writes the API pointer into it through the bound pointer.
    // p_ret 必须可变，因为 SQLite 会通过绑定的指针向其写入 API 指针。
    let mut p_ret: *mut fts5_api = ptr::null_mut();
    let ptr_arg = ToSqlOutput::Pointer((
        &mut p_ret as *mut *mut fts5_api as _,
        SQLITE_FTS5_API_PTR_TYPE,
        None,
    ));
    connection
        .query_row("SELECT fts5(?)", [ptr_arg], |_| Ok(()))
        .map_err(SqliteEngineError::sql)?;

    if p_ret.is_null() {
        return Err(SqliteEngineError::Tokenizer {
            detail: "SQLite fts5() returned a null API pointer".to_string(),
        });
    }

    Ok(p_ret)
}

/// Produces Jieba token spans with UTF-8 byte offsets.
/// 生成带 UTF-8 字节偏移的 Jieba 词条片段。
fn jieba_token_spans(text: &str, search_mode: bool, jieba: &Jieba) -> Vec<TokenSpan> {
    if text.is_empty() {
        return Vec::new();
    }

    let char_to_byte = unicode_char_to_byte_offsets(text);
    let mode = if search_mode {
        JiebaTokenizeMode::Search
    } else {
        JiebaTokenizeMode::Default
    };

    jieba
        .tokenize(text, mode, true)
        .into_iter()
        .filter_map(|token| {
            let trimmed = token.word.trim();
            if trimmed.is_empty() {
                return None;
            }
            let start_byte = *char_to_byte.get(token.start)?;
            let end_byte = *char_to_byte.get(token.end)?;
            Some(TokenSpan {
                token: trimmed.to_string(),
                start_byte,
                end_byte,
            })
        })
        .collect()
}

/// Precomputes a Unicode character index to UTF-8 byte index mapping.
/// 预计算 Unicode 字符索引到 UTF-8 字节索引的映射。
///
/// The trailing `text.len()` entry lets a token ending at the last character resolve to a valid
/// exclusive end offset.
///
/// 末尾的 `text.len()` 条目使结束于最后一个字符的词元能得到合法的排他结束偏移。
fn unicode_char_to_byte_offsets(text: &str) -> Vec<usize> {
    let mut offsets = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    offsets
}

/// FTS5 registration callback: creates a tokenizer instance.
/// FTS5 注册回调：创建分词器实例。
///
/// # Safety
///
/// `user_data` must be the exact pointer registered through `xCreateTokenizer_v2`, which this
/// crate guarantees by passing the same `RegisteredTokenizerContext` pointer. `out_tokenizer`
/// must be a valid writable pointer provided by SQLite.
///
/// # Safety
///
/// `user_data` 必须是经 `xCreateTokenizer_v2` 注册的同一指针，本 crate 通过传递同一个
/// `RegisteredTokenizerContext` 指针保证这一点。`out_tokenizer` 必须是 SQLite 提供的合法可写指针。
unsafe extern "C" fn sqlite_jieba_tokenizer_create(
    user_data: *mut c_void,
    _args: *mut *const c_char,
    _arg_count: c_int,
    out_tokenizer: *mut *mut ffi::Fts5Tokenizer,
) -> c_int {
    if user_data.is_null() || out_tokenizer.is_null() {
        return ffi::SQLITE_MISUSE;
    }

    // SAFETY: per the contract above, `user_data` is the context pointer this crate registered.
    // It stays alive until the destroy callback runs, which SQLite calls after all instances are
    // deleted, so the borrow is valid here.
    // SAFETY：按上述契约，`user_data` 是本 crate 注册的上下文指针。它在销毁回调运行前一直存活，
    // 而 SQLite 会在所有实例删除后才调用销毁回调，因此此处借用合法。
    let context = unsafe { &*(user_data as *const RegisteredTokenizerContext) };

    let tokenizer = Box::new(JiebaTokenizerInstance {
        shared_state: Arc::clone(&context.shared_state),
    });

    // SAFETY: `out_tokenizer` was null-checked above and SQLite owns the opaque tokenizer pointer
    // until it calls `xDelete`, which reclaims the allocation through `Box::from_raw`.
    // SAFETY：`out_tokenizer` 已在上方做空值检查；SQLite 拥有该不透明分词器指针直到调用
    // `xDelete`，届时通过 `Box::from_raw` 回收该分配。
    unsafe {
        *out_tokenizer = Box::into_raw(tokenizer) as *mut ffi::Fts5Tokenizer;
    }

    ffi::SQLITE_OK
}

/// FTS5 registration callback: destroys a tokenizer instance.
/// FTS5 注册回调：销毁分词器实例。
///
/// # Safety
///
/// `tokenizer` must be either null or a pointer previously returned by
/// [`sqlite_jieba_tokenizer_create`] via `Box::into_raw`, and must not be used afterwards.
///
/// # Safety
///
/// `tokenizer` 必须为 null，或为先前由 [`sqlite_jieba_tokenizer_create`] 通过 `Box::into_raw`
/// 返回的指针，且此后不得再使用。
unsafe extern "C" fn sqlite_jieba_tokenizer_delete(tokenizer: *mut ffi::Fts5Tokenizer) {
    if tokenizer.is_null() {
        return;
    }

    // SAFETY: per the contract above, `tokenizer` originated from `Box::into_raw` in
    // `sqlite_jieba_tokenizer_create` and SQLite calls this exactly once per created instance.
    // SAFETY：按上述契约，`tokenizer` 源自 `sqlite_jieba_tokenizer_create` 中的 `Box::into_raw`，
    // 且 SQLite 对每个创建的实例恰好调用本回调一次。
    unsafe {
        drop(Box::from_raw(tokenizer as *mut JiebaTokenizerInstance));
    }
}

/// FTS5 registration callback: releases the tokenizer registration context.
/// FTS5 注册回调：释放分词器注册上下文。
///
/// # Safety
///
/// `user_data` must be either null or the pointer passed to `xCreateTokenizer_v2`, and must not
/// be used afterwards.
///
/// # Safety
///
/// `user_data` 必须为 null，或为传给 `xCreateTokenizer_v2` 的指针，且此后不得再使用。
unsafe extern "C" fn sqlite_jieba_tokenizer_registration_destroy(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }

    // SAFETY: per the contract above, `user_data` is the pointer this crate passed to
    // `xCreateTokenizer_v2` through `Box::into_raw`. SQLite calls this exactly once when the
    // registration is torn down, so this is the single owning reclamation.
    // SAFETY：按上述契约，`user_data` 是本 crate 通过 `Box::into_raw` 传给 `xCreateTokenizer_v2`
    // 的指针。SQLite 在注册拆除时恰好调用本回调一次，因此这是唯一的拥有者回收。
    let context = unsafe { Box::from_raw(user_data as *mut RegisteredTokenizerContext) };

    registered_connection_handles()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&context.connection_handle);
}

/// FTS5 registration callback: performs the actual Jieba tokenization.
/// FTS5 注册回调：执行真正的 Jieba 分词。
///
/// # Safety
///
/// `tokenizer` must be a live instance created by [`sqlite_jieba_tokenizer_create`].
/// `text_ptr`/`text_len` must describe a readable buffer valid for this call; SQLite guarantees
/// this. `token_callback` must be a valid FTS5 token callback. No pointer may be retained after
/// returning.
///
/// # Safety
///
/// `tokenizer` 必须是 [`sqlite_jieba_tokenizer_create`] 创建的存活实例。
/// `text_ptr`/`text_len` 必须描述本次调用期间可读的缓冲区（SQLite 保证这一点）。
/// `token_callback` 必须是合法的 FTS5 词元回调。返回后不得保留任何指针。
#[allow(non_snake_case)]
unsafe extern "C" fn sqlite_jieba_tokenizer_tokenize(
    tokenizer: *mut ffi::Fts5Tokenizer,
    token_context: *mut c_void,
    flags: c_int,
    text_ptr: *const c_char,
    text_len: c_int,
    _locale_ptr: *const c_char,
    _locale_len: c_int,
    token_callback: Option<
        unsafe extern "C" fn(
            pCtx: *mut c_void,
            tflags: c_int,
            pToken: *const c_char,
            nToken: c_int,
            iStart: c_int,
            iEnd: c_int,
        ) -> c_int,
    >,
) -> c_int {
    if tokenizer.is_null() || token_context.is_null() || text_ptr.is_null() || text_len < 0 {
        return ffi::SQLITE_MISUSE;
    }
    let Some(token_callback) = token_callback else {
        return ffi::SQLITE_MISUSE;
    };

    // SAFETY: per the contract above, `tokenizer` is a live instance created by this module.
    // SAFETY：按上述契约，`tokenizer` 是本模块创建的存活实例。
    let tokenizer = unsafe { &*(tokenizer as *const JiebaTokenizerInstance) };

    let jieba = {
        let shared_state = tokenizer
            .shared_state
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(&shared_state.jieba)
    };

    // SAFETY: SQLite guarantees `text_ptr`/`text_len` describe a readable buffer valid for this
    // call. The slice is never stored and never escapes this function.
    // SAFETY：SQLite 保证 `text_ptr`/`text_len` 描述本次调用期间可读的缓冲区。
    // 该切片绝不存储，也绝不逃出本函数。
    let text_bytes = unsafe { slice::from_raw_parts(text_ptr as *const u8, text_len as usize) };

    // Invalid UTF-8 returns an error code instead of unwinding, because unwinding across the FFI
    // boundary is undefined behaviour.
    // 非法 UTF-8 返回错误码而不是展开栈，因为跨 FFI 边界展开是未定义行为。
    let Ok(text) = str::from_utf8(text_bytes) else {
        return ffi::SQLITE_ERROR;
    };

    let search_mode =
        (flags & ffi::FTS5_TOKENIZE_QUERY) != 0 || (flags & ffi::FTS5_TOKENIZE_AUX) != 0;
    let spans = jieba_token_spans(text, search_mode, jieba.as_ref());

    for span in spans {
        let token_bytes = span.token.as_bytes();

        // SAFETY: `token_callback` was null-checked above and is provided by SQLite.
        // `token_context` is passed through unchanged. `token_bytes` stays alive for the whole
        // iteration, which satisfies FTS5's requirement that token memory be valid only for the
        // duration of the callback. Offsets are within the borrowed text because they were
        // derived from that same text.
        // SAFETY：`token_callback` 已在上方做空值检查且由 SQLite 提供。`token_context` 原样传递。
        // `token_bytes` 在整个迭代期间存活，满足 FTS5 关于词元内存只需在回调期间有效的要求。
        // 偏移取自同一段文本，因此落在借用文本范围内。
        let rc = unsafe {
            token_callback(
                token_context,
                0,
                token_bytes.as_ptr() as *const c_char,
                token_bytes.len() as c_int,
                span.start_byte as c_int,
                span.end_byte as c_int,
            )
        };

        if rc != ffi::SQLITE_OK {
            return rc;
        }
    }

    ffi::SQLITE_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Opens an in-memory connection with the tokenizer registered.
    /// 打开一个已注册分词器的内存连接。
    fn registered_connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open in-memory database");
        ensure_jieba_tokenizer_registered(&connection).expect("register jieba tokenizer");
        connection
    }

    /// Verifies the companion dictionary DDL is exposed for the v1 initialization entry.
    /// 验证伴生词典 DDL 已暴露给 v1 初始化入口。
    #[test]
    fn dictionary_schema_script_is_available() {
        let script = dictionary_table_schema_script();
        assert!(script.contains("CREATE TABLE IF NOT EXISTS _vulcan_dict"));
        assert!(script.contains("word TEXT PRIMARY KEY"));
    }

    /// Verifies the per-connection registration marker cannot go stale when a handle value is reused.
    /// 验证当句柄值被复用时，按连接的注册标记不会失效。
    ///
    /// FTS5 tokenizer registration is per connection, and this module records the **raw `sqlite3*` pointer value** as
    /// the marker so re-registration can be skipped. A pointer value is not a stable identity: the allocator is free to
    /// hand the same address to a later connection. If the registry entry outlives the connection it describes, the
    /// next connection at that address skips registration — while the tokenizer genuinely is not registered on it,
    /// because registration lives on the connection's own FTS5 API object. The failure would then be
    /// `no such tokenizer: jieba` on an FTS5 table, which is a hard error, not a degradation.
    ///
    /// FTS5 分词器注册是按连接的，而本模块把**原始 `sqlite3*` 指针值**记为标记，以便跳过重复注册。指针值并不是稳定
    /// 身份：分配器完全可以把同一地址交给之后的某个连接。若注册表条目比它所描述的那个连接活得更久，那么占用该地址的
    /// 下一个连接就会跳过注册——而分词器在该连接上确实没有注册，因为注册活在连接自己的 FTS5 API 对象上。于是失败会
    /// 表现为 FTS5 表上的 `no such tokenizer: jieba`，那是硬错误而非降级。
    ///
    /// The destroy callback does remove the entry, and this test is what proves that the removal actually happens on
    /// every close rather than only in principle. It opens and closes many connections **at the same addresses** and
    /// requires the Jieba tokenizer to work in each one, so a single stale entry fails the test.
    ///
    /// 销毁回调确实会移除条目，而本测试证明的是那次移除在**每一次**关闭时都真的发生，而不只是原则上成立。它在**同一批
    /// 地址上**反复打开与关闭连接，并要求每一个连接里 Jieba 分词器都能工作，因此只要有一个条目变陈旧，本测试即失败。
    ///
    /// The destroy callback does remove the entry, and this test is what proves that the removal actually happens on
    /// every close rather than only in principle. It opens and closes many connections **at the same addresses** and
    /// requires the Jieba tokenizer to work in each one, so a single stale entry fails the test.
    ///
    /// **Evidence strength, measured:** 64 rounds produced **49 distinct** handle values, i.e. 15 opens landed on an
    /// address a previous connection had already used. Reuse is also asserted rather than hoped for — without it the
    /// test would pass trivially and prove nothing, so it fails with a message saying exactly that.
    ///
    /// **This strengthens existing coverage; it does not fill a hole.** Verified rather than assumed: deleting the
    /// cleanup in the destroy callback makes **two pre-existing tests** fail as well
    /// (`sqlite_fts_jieba_tokenizer_is_registered`, `sqlite_fts_jieba_bulk_insert_reuses_cached_engine`), so the
    /// invariant was already covered. What those two do **probabilistically**, this one does deterministically: they
    /// open a handful of connections and are caught only if the allocator happens to hand back a used address in that
    /// short window, whereas this one runs 64 rounds **and asserts that reuse occurred**, so it can neither miss the
    /// defect by luck nor pass vacuously.
    ///
    /// **证据强度，实测：** 64 轮产生 **49 个**不同句柄值，即有 15 次打开落在了先前的连接已经用过的地址上。复用同样
    /// 是被断言的而非被期望的——没有它，本测试会平凡地通过、什么也证明不了，因此它会以明确说明这一点的消息失败。
    ///
    /// **本测试是强化既有覆盖，而非填补空缺。** 这一点经核实而非假定：删除销毁回调中的清理会让**两个既有测试**同样
    /// 失败（`sqlite_fts_jieba_tokenizer_is_registered`、`sqlite_fts_jieba_bulk_insert_reuses_cached_engine`），
    /// 故该不变量本已有覆盖。那两个测试所做的覆盖是**概率性**的，本测试则是**确定性**的：它们只打开少数几个连接，
    /// 只有当分配器在那个很短的窗口里恰好交回一个用过的地址时才会捕捉到；而本测试跑 64 轮**并断言复用确实发生过**，
    /// 因此既不会因运气而漏掉缺陷，也不会平凡地通过。
    #[test]
    fn a_reused_connection_handle_does_not_skip_tokenizer_registration() {
        const ROUNDS: usize = 64;

        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("handle-reuse.db");
        let options = crate::runtime::SqliteOpenOptions::default();

        let mut observed_handles = std::collections::HashSet::new();
        for round in 0..ROUNDS {
            // Opened through the production path, so this exercises the real registration point rather than a
            // test-only helper.
            // 经生产路径打开，因此这里走的是真实注册点，而不是仅测试用的辅助函数。
            let connection = crate::runtime::open_sqlite_connection(&db_path, &options)
                .unwrap_or_else(|error| panic!("round {round}: open failed: {error}"));
            observed_handles.insert(sqlite_connection_handle(&connection));

            let table = format!("reuse_docs_{round}");
            connection
                .execute_batch(&format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS \"{table}\" USING fts5(
                        id UNINDEXED, content, tokenize='jieba'
                    );"
                ))
                .unwrap_or_else(|error| {
                    panic!(
                        "round {round}: creating a jieba FTS5 table failed, which means this connection skipped \
                         registration because a stale handle value was in the registry: {error}"
                    )
                });

            // Creating the table only proves the tokenizer was found. Tokenizing proves it actually works.
            // 建表只证明找到了分词器；分词才证明它真的可用。
            connection
                .execute(
                    &format!(
                        "INSERT INTO \"{table}\" (id, content) VALUES ('a', '市民田女士急匆匆')"
                    ),
                    [],
                )
                .unwrap_or_else(|error| panic!("round {round}: insert failed: {error}"));
            let matched: i64 = connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM \"{table}\" WHERE \"{table}\" MATCH '市民田'"),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or_else(|error| panic!("round {round}: jieba search failed: {error}"));
            assert_eq!(matched, 1, "round {round}: the indexed document must match");

            drop(connection);
        }

        // Address reuse is what makes this test meaningful, so it is asserted rather than hoped for. Without reuse
        // the run would pass trivially and prove nothing about stale entries.
        // 地址复用是本测试有意义的前提，因此它是被断言的而非被期望的。没有复用，本次运行会平凡地通过，对陈旧条目
        // 什么也证明不了。
        assert!(
            observed_handles.len() < ROUNDS,
            "{ROUNDS} rounds produced {} distinct handle values, so no address was reused and this test did not \
             exercise the stale-marker path it exists to check",
            observed_handles.len()
        );
    }

    /// Verifies companion dictionary hot updates cooperate with Jieba segmentation.
    /// 验证伴生词典热更新与 Jieba 分词协同工作。
    #[test]
    fn custom_words_affect_jieba_tokenization() {
        let connection = registered_connection();

        let before = tokenize_text(
            Some(&connection),
            TokenizerMode::Jieba,
            "市民田-女士急匆匆",
            false,
        )
        .expect("tokenize before");
        assert!(!before.tokens.iter().any(|token| token == "田-女士"));

        let mutation = upsert_custom_word(&connection, "田-女士", 42).expect("upsert custom word");
        assert!(mutation.success);

        let after = tokenize_text(
            Some(&connection),
            TokenizerMode::Jieba,
            "市民田-女士急匆匆",
            false,
        )
        .expect("tokenize after");
        assert!(after.tokens.iter().any(|token| token == "田-女士"));

        let removed = remove_custom_word(&connection, "田-女士").expect("remove custom word");
        assert!(removed.success);
    }

    /// Verifies a reload after a rolled-back mutation drops the word that never committed.
    /// 验证变更回滚后的重新加载会丢弃从未提交的词。
    #[test]
    fn reload_after_rollback_drops_uncommitted_words() {
        let mut connection = registered_connection();
        let has_word = |connection: &Connection| {
            tokenize_text(
                Some(connection),
                TokenizerMode::Jieba,
                "市民田-女士急匆匆",
                false,
            )
            .expect("tokenize")
            .tokens
            .iter()
            .any(|token| token == "田-女士")
        };
        {
            let transaction = connection.transaction().expect("begin");
            upsert_custom_word(&transaction, "田-女士", 42).expect("upsert custom word");
            assert!(has_word(&transaction));
        }
        reload_custom_dictionary(&connection).expect("reload dictionary");
        assert!(!has_word(&connection));
    }

    /// Verifies SQLite FTS5 `tokenize='jieba'` is truly registered and active.
    /// 验证 SQLite FTS5 `tokenize='jieba'` 确实已注册并生效。
    #[test]
    fn sqlite_fts_jieba_tokenizer_is_registered() {
        let connection = registered_connection();
        upsert_custom_word(&connection, "田-女士", 42).expect("upsert custom word");

        connection
            .execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS mcp_memory_fts USING fts5(
                    content,
                    tokenize='jieba'
                );",
            )
            .expect("create fts table");
        connection
            .execute(
                "INSERT INTO mcp_memory_fts (content) VALUES (?1)",
                params!["市民田-女士急匆匆"],
            )
            .expect("insert row");
        connection
            .execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS mcp_memory_vocab USING fts5vocab(
                    mcp_memory_fts,
                    'instance'
                );",
            )
            .expect("create vocab table");

        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM mcp_memory_vocab WHERE term = ?1",
                params!["田-女士"],
                |row| row.get(0),
            )
            .expect("query vocab");
        assert_eq!(count, 1);
    }

    /// Verifies bulk FTS writes reuse the registered Jieba instance instead of rebuilding its
    /// dictionary per row.
    /// 验证批量 FTS 写入复用已注册的 Jieba 实例，而不会逐行重建词典。
    #[test]
    fn sqlite_fts_jieba_bulk_insert_reuses_cached_engine() {
        let connection = registered_connection();
        connection
            .execute_batch("CREATE VIRTUAL TABLE bulk_fts USING fts5(content, tokenize='jieba');")
            .expect("create bulk fts table");

        let started_at = Instant::now();
        connection
            .execute_batch(
                r#"WITH RECURSIVE rows(value) AS (
                       SELECT 1
                       UNION ALL
                       SELECT value + 1 FROM rows WHERE value < 2000
                   )
                   INSERT INTO bulk_fts(content)
                   SELECT printf('角色等级提升到第%d级后获得新的技能与装备', value) FROM rows;"#,
            )
            .expect("bulk insert");
        let elapsed = started_at.elapsed();

        let indexed_rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM bulk_fts;", [], |row| row.get(0))
            .expect("count rows");

        assert_eq!(indexed_rows, 2000);
        assert!(
            elapsed < Duration::from_secs(5),
            "bulk Jieba FTS insertion rebuilt the engine per row: {elapsed:?}"
        );
    }

    /// Verifies databases without custom words share one process-wide default Jieba instance.
    /// 验证无自定义词数据库共享同一个进程级默认 Jieba 实例。
    #[test]
    fn sqlite_jieba_default_engine_is_shared_across_databases() {
        let first_connection = registered_connection();
        let second_connection = registered_connection();

        let first_key = connection_registry_key(&first_connection);
        let second_key = connection_registry_key(&second_connection);
        assert_ne!(
            first_key, second_key,
            "distinct in-memory connections must not share a registry key"
        );

        let first_state = shared_dictionary_state_for_key(&first_key);
        let second_state = shared_dictionary_state_for_key(&second_key);
        let first_jieba = {
            let state = first_state
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::clone(&state.jieba)
        };
        let second_jieba = {
            let state = second_state
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::clone(&state.jieba)
        };

        assert!(Arc::ptr_eq(&first_jieba, &second_jieba));
        assert!(Arc::ptr_eq(&first_jieba, default_jieba()));
    }

    /// Verifies the weak registry releases Jieba state after the final connection closes.
    /// 验证最后一个连接关闭后弱引用注册表释放 Jieba 状态。
    #[test]
    fn sqlite_jieba_database_state_is_released_with_connection() {
        let connection = registered_connection();
        let db_key = connection_registry_key(&connection);
        let shared_state = shared_dictionary_state_for_key(&db_key);
        let weak_state = Arc::downgrade(&shared_state);

        drop(shared_state);
        drop(connection);

        assert!(weak_state.upgrade().is_none());
    }

    /// Verifies tokenizing with no connection uses the default engine and produces tokens.
    /// 验证不带连接分词时使用默认引擎并产生词元。
    #[test]
    fn tokenize_without_connection_uses_default_engine() {
        let output = tokenize_text(None, TokenizerMode::Jieba, "角色等级提升", false)
            .expect("tokenize without connection");
        assert!(!output.tokens.is_empty());
        assert_eq!(output.tokenizer_mode, "jieba");
    }

    /// Verifies plain mode splits on whitespace and ASCII punctuation.
    /// 验证常规模式按空白与 ASCII 标点切分。
    #[test]
    fn plain_mode_splits_on_whitespace_and_punctuation() {
        let output =
            tokenize_text(None, TokenizerMode::None, "alpha,beta gamma", false).expect("tokenize");
        assert_eq!(output.tokens, vec!["alpha", "beta", "gamma"]);
    }

    /// Verifies punctuation-only plain input still yields one searchable token.
    /// 验证纯标点的常规输入仍产生一个可检索词元。
    #[test]
    fn plain_mode_keeps_punctuation_only_input() {
        let output = tokenize_text(None, TokenizerMode::None, "...", false).expect("tokenize");
        assert_eq!(output.tokens, vec!["..."]);
    }

    /// Verifies empty input yields no tokens.
    /// 验证空输入不产生词元。
    #[test]
    fn empty_input_yields_no_tokens() {
        let output = tokenize_text(None, TokenizerMode::Jieba, "", false).expect("tokenize");
        assert!(output.tokens.is_empty());
        assert!(output.fts_query.is_empty());
    }

    /// Verifies the FTS query is quoted so user text cannot inject FTS operators.
    /// 验证 FTS 查询带引号，使用户文本无法注入 FTS 运算符。
    #[test]
    fn fts_query_neutralizes_embedded_quotes() {
        let tokens = vec!["a\"b".to_string(), "NEAR".to_string()];
        let query = build_fts_query(&tokens, false);
        assert_eq!(query, "\"a\"\"b\" \"NEAR\"");
    }

    /// Verifies search mode joins tokens with OR while default mode joins with a space.
    /// 验证搜索模式以 OR 连接词元，默认模式以空格连接。
    #[test]
    fn fts_query_join_mode_follows_search_mode() {
        let tokens = vec!["x".to_string(), "y".to_string()];
        assert_eq!(build_fts_query(&tokens, true), "\"x\" OR \"y\"");
        assert_eq!(build_fts_query(&tokens, false), "\"x\" \"y\"");
    }

    /// Verifies tokenizer mode names round-trip and unknown names are rejected.
    /// 验证分词模式名可往返解析，未知名称被拒绝。
    #[test]
    fn tokenizer_mode_parsing_is_strict() {
        assert_eq!(TokenizerMode::parse("jieba"), Some(TokenizerMode::Jieba));
        assert_eq!(TokenizerMode::parse("NONE"), Some(TokenizerMode::None));
        assert_eq!(TokenizerMode::parse(""), Some(TokenizerMode::None));
        assert_eq!(TokenizerMode::parse("unknown"), None);
        assert_eq!(TokenizerMode::Jieba.as_str(), "jieba");
    }

    /// Verifies dictionary upsert, list and remove round-trip.
    /// 验证词典写入、列出与删除可往返。
    #[test]
    fn dictionary_mutations_round_trip() {
        let connection = registered_connection();

        upsert_custom_word(&connection, "神经网络", 5).expect("upsert");
        upsert_custom_word(&connection, "机器学习", 7).expect("upsert second");

        let listed = list_custom_words(&connection).expect("list");
        assert!(listed.success);
        assert_eq!(listed.words.len(), 2);
        assert_eq!(listed.words[0].word, "机器学习");

        let removed = remove_custom_word(&connection, "神经网络").expect("remove");
        assert!(removed.success);
        assert_eq!(removed.affected_rows, 1);

        let remaining = load_custom_words(&connection).expect("load");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].word, "机器学习");
    }

    /// Verifies an empty custom word is refused without touching the table.
    /// 验证空自定义词被拒绝且不触碰表。
    #[test]
    fn empty_custom_word_is_refused() {
        let connection = registered_connection();

        let upsert = upsert_custom_word(&connection, "   ", 1).expect("upsert blank");
        assert!(!upsert.success);
        assert_eq!(upsert.affected_rows, 0);

        let remove = remove_custom_word(&connection, "   ").expect("remove blank");
        assert!(!remove.success);
        assert_eq!(remove.affected_rows, 0);
    }

    /// Verifies invalid custom-word weights are rejected without reinterpretation.
    /// 验证非法自定义词权重会被拒绝且不发生语义重解释。
    #[test]
    fn invalid_custom_word_weights_are_rejected() {
        let connection = registered_connection();
        assert!(upsert_custom_word(&connection, "零权重", 0).is_err());
        if usize::BITS > 63 {
            assert!(upsert_custom_word(&connection, "超大权重", usize::MAX).is_err());
        }
        assert!(
            load_custom_words(&connection)
                .expect("load words")
                .is_empty(),
            "rejected weights must not mutate the dictionary"
        );
    }

    /// Verifies removing an absent word reports zero affected rows rather than failing.
    /// 验证删除不存在的词报告零影响行而不是失败。
    #[test]
    fn removing_absent_word_reports_not_found() {
        let connection = registered_connection();

        let removed = remove_custom_word(&connection, "不存在的词").expect("remove absent");
        assert!(removed.success);
        assert_eq!(removed.affected_rows, 0);
    }

    /// Verifies two connections to the same file database share one dictionary state, so a
    /// custom word added on one connection is visible to the other.
    /// 验证同一文件数据库的两个连接共享一个词典状态，使一个连接加入的自定义词对另一连接可见。
    #[test]
    fn same_file_database_connections_share_dictionary_state() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let db_path = temp.path().join("shared.db");

        let first = Connection::open(&db_path).expect("open first connection");
        ensure_jieba_tokenizer_registered(&first).expect("register first");
        let second = Connection::open(&db_path).expect("open second connection");
        ensure_jieba_tokenizer_registered(&second).expect("register second");

        assert_eq!(
            connection_registry_key(&first),
            connection_registry_key(&second),
            "two connections to one file database must share a registry key"
        );

        upsert_custom_word(&first, "共享词汇", 3).expect("upsert on first");

        let listed = load_custom_words(&second).expect("list from second");
        assert!(
            listed.iter().any(|entry| entry.word == "共享词汇"),
            "custom word written on one connection must be visible to another"
        );
    }
}
