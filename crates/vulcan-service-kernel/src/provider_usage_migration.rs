//! One-way migration from the legacy usage-statistics plugin into native provider accounting.
//! 从旧用量统计插件到原生供应商核算的一次性迁移。

use crate::plugin_storage::ControllerPluginStorageService;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use vldb_controller_client::ControllerSqliteValue;
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle,
    workspace_store::{
        ProviderUsageAttemptRecord, load_provider_usage_migration_completed,
        load_provider_usage_migration_cursor,
        persist_provider_usage_attempt_batch_and_rebuild_daily,
        persist_provider_usage_migration_completed, persist_provider_usage_migration_cursor,
        remove_duplicate_legacy_provider_usage_attempts_and_rebuild_daily,
    },
};

/// Stable bundled plugin identity whose historical database is migrated.
/// 要迁移历史数据库的稳定内建插件标识。
const LEGACY_USAGE_PLUGIN_ID: &str = "vulcan-usage-statistics";
/// Exact legacy database schema revision accepted by this importer.
/// 当前导入器接受的精确旧数据库 Schema 修订。
const LEGACY_USAGE_SCHEMA_VERSION: i64 = 4;
/// Maximum source rows copied in one Controller page and target batch.
/// 单个 Controller 分页与目标批次复制的最大来源行数。
const LEGACY_USAGE_PAGE_SIZE: i64 = 200;
/// Prefix that namespaces the opaque native migration cursor.
/// 为不透明原生迁移游标划分命名空间的前缀。
const LEGACY_USAGE_CURSOR_PREFIX: &str = "vulcan-usage-statistics-v4:rowid=";

/// Summary of one idempotent legacy statistics migration run.
/// 一次幂等旧统计迁移运行的摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LegacyUsageMigrationReport {
    /// Whether a legacy physical database existed.
    /// 是否存在旧物理数据库。
    pub source_present: bool,
    /// Number of source rows accepted in this run.
    /// 本次运行接受的来源行数。
    pub imported_rows: u64,
    /// Final source rowid persisted as the migration cursor.
    /// 作为迁移游标持久化的最终来源 rowid。
    pub final_source_rowid: i64,
}

/// Exact schema-version row from the legacy plugin database.
/// 旧插件数据库中的精确 Schema 版本行。
#[derive(Debug, Deserialize)]
struct LegacySchemaVersionRow {
    /// Highest applied plugin schema version.
    /// 已应用的最高插件 Schema 版本。
    schema_version: i64,
}

/// Exact joined event and model row read from legacy schema version four.
/// 从旧 Schema 第四版读取的精确事件与模型联接行。
#[derive(Debug, Deserialize)]
struct LegacyUsageEventRow {
    /// SQLite source row identity used only for keyset pagination.
    /// 仅用于键集分页的 SQLite 来源行标识。
    source_rowid: i64,
    /// Legacy physical request identity.
    /// 旧物理请求标识。
    request_id: String,
    /// Legacy logical execution identity.
    /// 旧逻辑执行标识。
    execution_id: String,
    /// Stable consumer identity recorded by the old telemetry layer.
    /// 旧遥测层记录的稳定消费者标识。
    owner_id: String,
    /// Physical terminal status.
    /// 物理终态状态。
    status: String,
    /// Zero-based attempt index.
    /// 从零开始的 attempt 索引。
    attempt_index: i64,
    /// First provider event timestamp when observed.
    /// 观察到首个供应商事件时的时间戳。
    first_event_at_ms: Option<i64>,
    /// First token timestamp when observed.
    /// 观察到首个 Token 时的时间戳。
    first_token_at_ms: Option<i64>,
    /// Terminal timestamp.
    /// 终态时间戳。
    completed_at_ms: i64,
    /// Legacy uncached-input column with historical zero coercion.
    /// 存在历史零值强制转换的旧非缓存输入列。
    input_tokens: i64,
    /// Legacy output column with historical zero coercion.
    /// 存在历史零值强制转换的旧输出列。
    output_tokens: i64,
    /// Legacy reasoning column with historical zero coercion.
    /// 存在历史零值强制转换的旧推理列。
    reasoning_tokens: i64,
    /// Legacy total column retained only for ambiguity detection.
    /// 仅用于歧义检测的旧合计列。
    total_tokens: i64,
    /// Optional cache-read token count.
    /// 可选缓存读取 Token 数。
    cached_input_tokens: Option<i64>,
    /// Optional cache-write token count.
    /// 可选缓存写入 Token 数。
    cache_creation_input_tokens: Option<i64>,
    /// Provider-reported total after the plugin schema-four migration.
    /// 插件 Schema 第四版迁移后的供应商报告合计。
    provider_total_tokens: Option<i64>,
    /// Legacy operation when newer telemetry supplied one.
    /// 新版遥测提供时的旧操作类别。
    operation: Option<String>,
    /// Legacy service entry identity.
    /// 旧服务入口标识。
    service_entry_id: Option<String>,
    /// Legacy credential connection identity.
    /// 旧凭据连接标识。
    connection_id: Option<String>,
    /// Exact protocol profile identity when recorded.
    /// 记录时的精确协议 Profile 标识。
    protocol_profile_id: Option<String>,
    /// Exact secondary route identity when recorded.
    /// 记录时的精确次级路由标识。
    secondary_route_id: Option<String>,
    /// Stable provider key joined from the legacy model table.
    /// 从旧模型表联接得到的稳定供应商键。
    provider_key: String,
    /// Stable model key joined from the legacy model table.
    /// 从旧模型表联接得到的稳定模型键。
    model_key: String,
    /// Optional stable error category.
    /// 可选稳定错误类别。
    error_category: Option<String>,
    /// Optional stable provider error code.
    /// 可选稳定供应商错误码。
    error_code: Option<String>,
    /// Exact historical calendar date.
    /// 精确历史日历日期。
    calendar_date: String,
}

/// Migrates all currently available legacy usage rows through Controller-managed bindings.
/// 通过 Controller 管理的绑定迁移当前全部可用旧用量行。
///
/// # Parameters
/// 参数
/// `storage` owns the isolated plugin binding and `controller` owns the native application binding.
/// `storage` 持有隔离插件绑定，`controller` 持有原生应用绑定。
///
/// # Returns
/// 返回值
/// An idempotent migration report, or a controlled error without deleting legacy data.
/// 返回幂等迁移摘要；失败时返回受控错误且不删除旧数据。
pub(crate) async fn migrate_legacy_usage_statistics(
    storage: &ControllerPluginStorageService,
    controller: &ControllerRuntimeHandle,
) -> Result<LegacyUsageMigrationReport, String> {
    if load_provider_usage_migration_completed(controller).await? {
        let persisted_cursor = load_provider_usage_migration_cursor(controller).await?;
        return Ok(LegacyUsageMigrationReport {
            source_present: !persisted_cursor.is_empty(),
            imported_rows: 0,
            final_source_rowid: parse_legacy_usage_cursor(persisted_cursor.as_str())?,
        });
    }
    let repair_timestamp = current_unix_millis()?;
    remove_duplicate_legacy_provider_usage_attempts_and_rebuild_daily(controller, repair_timestamp)
        .await?;
    let persisted_cursor = load_provider_usage_migration_cursor(controller).await?;
    if !persisted_cursor.is_empty() {
        persist_provider_usage_migration_completed(controller, repair_timestamp).await?;
        return Ok(LegacyUsageMigrationReport {
            source_present: true,
            imported_rows: 0,
            final_source_rowid: parse_legacy_usage_cursor(persisted_cursor.as_str())?,
        });
    }
    let Some(schema_rows) = storage
        .query_existing_sqlite_rows(
            LEGACY_USAGE_PLUGIN_ID,
            "SELECT MAX(version) AS schema_version FROM schema_migrations;",
            Vec::new(),
        )
        .await?
    else {
        persist_provider_usage_migration_completed(controller, repair_timestamp).await?;
        return Ok(LegacyUsageMigrationReport {
            source_present: false,
            imported_rows: 0,
            final_source_rowid: 0,
        });
    };
    let schema_rows = serde_json::from_value::<Vec<LegacySchemaVersionRow>>(schema_rows)
        .map_err(|error| format!("旧用量插件 Schema 版本行无效：{error}"))?;
    let schema_version = schema_rows
        .into_iter()
        .next()
        .ok_or_else(|| String::from("旧用量插件缺少 Schema 版本行。"))?
        .schema_version;
    if schema_version != LEGACY_USAGE_SCHEMA_VERSION {
        return Err(format!(
            "旧用量插件 Schema 版本为 {schema_version}，迁移器仅接受 {LEGACY_USAGE_SCHEMA_VERSION}。"
        ));
    }

    let mut source_rowid = 0;
    let mut imported_rows = 0_u64;
    loop {
        let rows = storage
            .query_existing_sqlite_rows(
                LEGACY_USAGE_PLUGIN_ID,
                legacy_usage_page_sql(),
                vec![
                    ControllerSqliteValue::Int64(source_rowid),
                    ControllerSqliteValue::Int64(LEGACY_USAGE_PAGE_SIZE),
                ],
            )
            .await?
            .ok_or_else(|| String::from("旧用量插件数据库在迁移期间消失。"))?;
        let rows = serde_json::from_value::<Vec<LegacyUsageEventRow>>(rows)
            .map_err(|error| format!("旧用量插件事件分页无效：{error}"))?;
        if rows.is_empty() {
            break;
        }
        let next_source_rowid = rows
            .last()
            .map(|row| row.source_rowid)
            .ok_or_else(|| String::from("旧用量插件分页缺少末尾游标。"))?;
        let records = rows
            .iter()
            .map(legacy_row_to_native_attempt)
            .collect::<Result<Vec<_>, _>>()?;
        let updated_at_ms = current_unix_millis()?;
        persist_provider_usage_attempt_batch_and_rebuild_daily(
            controller,
            records.as_slice(),
            updated_at_ms,
        )
        .await?;
        persist_provider_usage_migration_cursor(
            controller,
            format!("{LEGACY_USAGE_CURSOR_PREFIX}{next_source_rowid}").as_str(),
            updated_at_ms,
        )
        .await?;
        imported_rows = imported_rows
            .checked_add(
                u64::try_from(rows.len())
                    .map_err(|_| String::from("旧用量插件分页行数无法转换为无符号计数。"))?,
            )
            .ok_or_else(|| String::from("旧用量插件迁移计数溢出。"))?;
        source_rowid = next_source_rowid;
    }
    persist_provider_usage_migration_completed(controller, current_unix_millis()?).await?;
    Ok(LegacyUsageMigrationReport {
        source_present: true,
        imported_rows,
        final_source_rowid: source_rowid,
    })
}

/// Returns the immutable schema-four keyset pagination query.
/// 返回不可变的 Schema 第四版键集分页查询。
///
/// # Returns
/// 返回值
/// A read-only query joining every required model identity at the source.
/// 返回在来源侧联接全部必需模型标识的只读查询。
fn legacy_usage_page_sql() -> &'static str {
    r#"
SELECT
    e.rowid AS source_rowid,
    e.request_id,
    e.execution_id,
    e.owner_id,
    e.status,
    e.attempt_index,
    e.first_event_at_ms,
    e.first_token_at_ms,
    e.completed_at_ms,
    e.input_tokens,
    e.output_tokens,
    e.reasoning_tokens,
    e.total_tokens,
    e.cached_input_tokens,
    e.cache_creation_input_tokens,
    e.provider_total_tokens,
    e.operation,
    e.service_entry_id,
    e.connection_id,
    e.protocol_profile_id,
    e.secondary_route_id,
    m.provider_key,
    m.model_key,
    e.error_category,
    e.error_code,
    e.calendar_date
FROM api_request_events AS e
INNER JOIN models AS m ON m.model_ref = e.model_ref
WHERE e.rowid > ?1
ORDER BY e.rowid ASC
LIMIT ?2;
"#
}

/// Parses only the namespaced rowid cursor emitted by this migration.
/// 仅解析当前迁移产生的带命名空间 rowid 游标。
///
/// # Parameters
/// 参数
/// `cursor` is the opaque value persisted in native provider usage metadata.
/// `cursor` 是持久化在原生供应商用量元数据中的不透明值。
///
/// # Returns
/// 返回值
/// Zero for an empty first-run cursor or the exact non-negative source rowid.
/// 首次运行的空游标返回零，否则返回精确非负来源 rowid。
fn parse_legacy_usage_cursor(cursor: &str) -> Result<i64, String> {
    if cursor.is_empty() {
        return Ok(0);
    }
    let value = cursor
        .strip_prefix(LEGACY_USAGE_CURSOR_PREFIX)
        .ok_or_else(|| format!("无法识别供应商用量迁移游标 `{cursor}`。"))?
        .parse::<i64>()
        .map_err(|error| format!("供应商用量迁移游标 `{cursor}` 无效：{error}"))?;
    if value < 0 {
        return Err(format!("供应商用量迁移游标 `{cursor}` 不能为负数。"));
    }
    Ok(value)
}

/// Converts one exact legacy source row into an explicitly provenance-limited native attempt.
/// 将一条精确旧来源行转换为显式限制来源能力的原生 attempt。
///
/// # Parameters
/// 参数
/// `row` is one schema-four plugin fact joined with its model identity.
/// `row` 是一条与模型标识联接的 Schema 第四版插件事实。
///
/// # Returns
/// 返回值
/// A native attempt with unknown transport and ambiguous historical zeros preserved as metadata.
/// 返回原生 attempt，并将未知传输与有歧义的历史零值保留为元数据。
fn legacy_row_to_native_attempt(
    row: &LegacyUsageEventRow,
) -> Result<ProviderUsageAttemptRecord, String> {
    validate_legacy_status(row.status.as_str())?;
    let mut validation_flags = vec![String::from("legacy_plugin_migration")];
    let operation =
        required_legacy_dimension(row.operation.as_deref(), "operation", &mut validation_flags);
    let service_entry_id = required_legacy_dimension(
        row.service_entry_id.as_deref(),
        "service_entry_id",
        &mut validation_flags,
    );
    let connection_id = required_legacy_dimension(
        row.connection_id.as_deref(),
        "connection_id",
        &mut validation_flags,
    );
    let protocol_profile_id = required_legacy_dimension(
        row.protocol_profile_id.as_deref(),
        "protocol_profile_id",
        &mut validation_flags,
    );
    let uncached_input_tokens =
        legacy_positive_tokens(row.input_tokens, "input_tokens", &mut validation_flags)?;
    let output_tokens =
        legacy_positive_tokens(row.output_tokens, "output_tokens", &mut validation_flags)?;
    let reasoning_tokens = legacy_positive_tokens(
        row.reasoning_tokens,
        "reasoning_tokens",
        &mut validation_flags,
    )?;
    let cache_read_tokens = legacy_optional_positive_tokens(
        row.cached_input_tokens,
        "cached_input_tokens",
        &mut validation_flags,
    )?;
    let cache_write_tokens = legacy_optional_positive_tokens(
        row.cache_creation_input_tokens,
        "cache_creation_input_tokens",
        &mut validation_flags,
    )?;
    let total_tokens = legacy_optional_positive_tokens(
        row.provider_total_tokens,
        "provider_total_tokens",
        &mut validation_flags,
    )?;
    if row.total_tokens == 0 {
        push_unique_flag(&mut validation_flags, "legacy_ambiguous_zero");
    } else if row.total_tokens < 0 {
        return Err(format!(
            "旧用量请求 `{}` 的 total_tokens 为负数。",
            row.request_id
        ));
    }
    push_unique_flag(&mut validation_flags, "legacy_transport_unknown");
    push_unique_flag(&mut validation_flags, "legacy_input_total_unknown");

    let semantic_output_observed = row.first_token_at_ms.is_some();
    let dispatched = row.first_event_at_ms.is_some();
    let usage_availability = if total_tokens.is_some() && row.status == "succeeded" {
        String::from("final")
    } else if uncached_input_tokens.is_some()
        || output_tokens.is_some()
        || reasoning_tokens.is_some()
        || cache_read_tokens.is_some()
        || cache_write_tokens.is_some()
        || total_tokens.is_some()
    {
        String::from("partial")
    } else {
        String::from("missing")
    };
    let usage_source_paths_json = legacy_usage_source_paths(
        uncached_input_tokens,
        output_tokens,
        reasoning_tokens,
        cache_read_tokens,
        cache_write_tokens,
        total_tokens,
    )?;
    Ok(ProviderUsageAttemptRecord {
        logical_request_id: row.execution_id.clone(),
        attempt_id: format!("legacy-plugin:{}", row.request_id),
        attempt_index: row.attempt_index,
        failover_index: None,
        consumer_id: row.owner_id.clone(),
        operation,
        provider_id: row.provider_key.clone(),
        service_entry_id,
        connection_id,
        route_id: row.secondary_route_id.clone(),
        protocol_profile_id,
        route_revision: None,
        transport: None,
        requested_model: row.model_key.clone(),
        mapped_model: row.model_key.clone(),
        response_model: None,
        response_id: None,
        provider_request_id: None,
        dispatched_at_ms: None,
        headers_at_ms: None,
        first_semantic_at_ms: row.first_token_at_ms,
        terminal_at_ms: row.completed_at_ms,
        status: row.status.clone(),
        dispatched,
        dispatched_known: dispatched,
        semantic_output_observed,
        semantic_output_observed_known: semantic_output_observed,
        replay_safe: None,
        http_status: None,
        websocket_close_code: None,
        error_category: row.error_category.clone(),
        error_code: row.error_code.clone(),
        usage_availability,
        input_tokens: None,
        uncached_input_tokens,
        cache_read_tokens,
        cache_write_tokens,
        output_tokens,
        reasoning_tokens,
        audio_input_tokens: None,
        audio_output_tokens: None,
        total_tokens,
        usage_profile_id: Some(String::from("legacy-usage-plugin.v4")),
        usage_profile_revision: Some(LEGACY_USAGE_SCHEMA_VERSION),
        usage_source_paths_json,
        usage_validation_flags_json: Some(
            serde_json::to_string(&validation_flags)
                .map_err(|error| format!("序列化旧用量校验标记失败：{error}"))?,
        ),
        provider_billing_details_json: None,
        calendar_date: row.calendar_date.clone(),
    })
}

/// Resolves one required native dimension without pretending an absent legacy value is known.
/// 解析一个原生必需维度，且不把缺失的旧值伪装为已知事实。
///
/// # Parameters
/// 参数
/// `value` is the legacy optional value, `field` identifies its source, and `flags` receives provenance limits.
/// `value` 是旧可选值，`field` 标识其来源，`flags` 接收来源限制。
///
/// # Returns
/// 返回值
/// The exact non-empty value or the explicit `legacy_unknown` sentinel with a field-specific flag.
/// 返回精确非空值；缺失时返回显式 `legacy_unknown` 哨兵并加入字段专属标记。
fn required_legacy_dimension(value: Option<&str>, field: &str, flags: &mut Vec<String>) -> String {
    match value {
        Some(value) if !value.is_empty() => value.to_string(),
        _ => {
            push_unique_flag(flags, format!("legacy_missing_{field}").as_str());
            String::from("legacy_unknown")
        }
    }
}

/// Preserves a positive historical token value and marks coerced zero as ambiguous.
/// 保留正数历史 Token 值，并将强制转换的零标记为有歧义。
///
/// # Parameters
/// 参数
/// `value` is the non-null legacy scalar, `field` names its column, and `flags` receives ambiguity metadata.
/// `value` 是非空旧标量，`field` 命名其列，`flags` 接收歧义元数据。
///
/// # Returns
/// 返回值
/// A positive value, `None` for ambiguous zero, or an error for invalid negative input.
/// 返回正数值；有歧义的零返回 `None`；负数输入返回错误。
fn legacy_positive_tokens(
    value: i64,
    field: &str,
    flags: &mut Vec<String>,
) -> Result<Option<i64>, String> {
    if value < 0 {
        return Err(format!("旧用量字段 `{field}` 不能为负数。"));
    }
    if value == 0 {
        push_unique_flag(flags, "legacy_ambiguous_zero");
        return Ok(None);
    }
    Ok(Some(value))
}

/// Preserves an optional positive historical token value and marks non-null zero as ambiguous.
/// 保留可选正数历史 Token 值，并将非空零标记为有歧义。
///
/// # Parameters
/// 参数
/// `value` is the optional legacy scalar, `field` names its column, and `flags` receives ambiguity metadata.
/// `value` 是可选旧标量，`field` 命名其列，`flags` 接收歧义元数据。
///
/// # Returns
/// 返回值
/// A positive value, `None` for absent or ambiguous zero, or an error for invalid negative input.
/// 返回正数值；缺失或有歧义的零返回 `None`；负数输入返回错误。
fn legacy_optional_positive_tokens(
    value: Option<i64>,
    field: &str,
    flags: &mut Vec<String>,
) -> Result<Option<i64>, String> {
    match value {
        Some(value) => legacy_positive_tokens(value, field, flags),
        None => Ok(None),
    }
}

/// Builds exact legacy-column provenance only for token values that were retained.
/// 仅为实际保留的 Token 值构建精确旧列来源。
///
/// # Parameters
/// 参数
/// Each argument is one retained native usage dimension.
/// 每个参数都是一个已保留的原生用量维度。
///
/// # Returns
/// 返回值
/// A JSON object mapping native dimensions to exact legacy columns, or `None` when no usage survived.
/// 返回将原生维度映射到精确旧列的 JSON 对象；没有保留用量时返回 `None`。
fn legacy_usage_source_paths(
    uncached_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    total_tokens: Option<i64>,
) -> Result<Option<String>, String> {
    let mut paths = Map::new();
    for (present, dimension, column) in [
        (
            uncached_input_tokens.is_some(),
            "uncached_input_tokens",
            "api_request_events.input_tokens",
        ),
        (
            output_tokens.is_some(),
            "output_tokens",
            "api_request_events.output_tokens",
        ),
        (
            reasoning_tokens.is_some(),
            "reasoning_tokens",
            "api_request_events.reasoning_tokens",
        ),
        (
            cache_read_tokens.is_some(),
            "cache_read_tokens",
            "api_request_events.cached_input_tokens",
        ),
        (
            cache_write_tokens.is_some(),
            "cache_write_tokens",
            "api_request_events.cache_creation_input_tokens",
        ),
        (
            total_tokens.is_some(),
            "total_tokens",
            "api_request_events.provider_total_tokens",
        ),
    ] {
        if present {
            paths.insert(dimension.to_string(), Value::String(column.to_string()));
        }
    }
    if paths.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(&paths)
        .map(Some)
        .map_err(|error| format!("序列化旧用量来源路径失败：{error}"))
}

/// Adds one validation flag without duplicating an earlier flag.
/// 添加一个校验标记，且不重复已有标记。
///
/// # Parameters
/// 参数
/// `flags` is the ordered destination and `flag` is the stable value to add.
/// `flags` 是有序目标，`flag` 是要添加的稳定值。
fn push_unique_flag(flags: &mut Vec<String>, flag: &str) {
    if !flags.iter().any(|existing| existing == flag) {
        flags.push(flag.to_string());
    }
}

/// Validates the exact terminal statuses emitted by the legacy plugin schema.
/// 校验旧插件 Schema 发出的精确终态状态。
///
/// # Parameters
/// 参数
/// `status` is the persisted legacy terminal status.
/// `status` 是已持久化的旧终态状态。
///
/// # Returns
/// 返回值
/// Success for the three schema-declared values, otherwise a controlled error.
/// 对 Schema 声明的三个值返回成功，否则返回受控错误。
fn validate_legacy_status(status: &str) -> Result<(), String> {
    if matches!(status, "succeeded" | "failed" | "cancelled") {
        Ok(())
    } else {
        Err(format!("旧用量插件终态 `{status}` 不受支持。"))
    }
}

/// Returns the current Unix timestamp in signed milliseconds.
/// 返回当前有符号 Unix 毫秒时间戳。
///
/// # Returns
/// 返回值
/// A SQLite-compatible timestamp or a controlled host-clock error.
/// 返回 SQLite 兼容时间戳，宿主时钟异常时返回受控错误。
fn current_unix_millis() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("宿主时钟早于 Unix 纪元：{error}"))?
        .as_millis();
    i64::try_from(millis).map_err(|_| String::from("Unix 毫秒时间戳超过 SQLite INTEGER。"))
}

#[cfg(test)]
mod tests {
    use super::{
        LEGACY_USAGE_CURSOR_PREFIX, LegacyUsageEventRow, legacy_row_to_native_attempt,
        parse_legacy_usage_cursor,
    };

    /// Verifies only empty or exactly namespaced migration cursors are accepted.
    /// 验证只接受空游标或精确带命名空间的迁移游标。
    #[test]
    fn migration_cursor_is_strictly_namespaced() {
        assert_eq!(parse_legacy_usage_cursor("").expect("empty cursor"), 0);
        assert_eq!(
            parse_legacy_usage_cursor(format!("{LEGACY_USAGE_CURSOR_PREFIX}42").as_str())
                .expect("namespaced cursor"),
            42
        );
        assert!(parse_legacy_usage_cursor("42").is_err());
        assert!(
            parse_legacy_usage_cursor(format!("{LEGACY_USAGE_CURSOR_PREFIX}-1").as_str()).is_err()
        );
    }

    /// Verifies historical non-null zeros remain unknown and carry explicit provenance flags.
    /// 验证历史非空零保持未知，并携带显式来源标记。
    #[test]
    fn legacy_zero_tokens_are_not_imported_as_reported_zero() {
        let row = LegacyUsageEventRow {
            source_rowid: 1,
            request_id: String::from("request-1"),
            execution_id: String::from("execution-1"),
            owner_id: String::from("consumer-1"),
            status: String::from("succeeded"),
            attempt_index: 0,
            first_event_at_ms: Some(2),
            first_token_at_ms: None,
            completed_at_ms: 3,
            input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
            total_tokens: 0,
            cached_input_tokens: Some(0),
            cache_creation_input_tokens: Some(0),
            provider_total_tokens: Some(0),
            operation: None,
            service_entry_id: None,
            connection_id: None,
            protocol_profile_id: None,
            secondary_route_id: None,
            provider_key: String::from("provider-1"),
            model_key: String::from("model-1"),
            error_category: None,
            error_code: None,
            calendar_date: String::from("2026-08-06"),
        };

        let record = legacy_row_to_native_attempt(&row).expect("legacy row should convert");
        assert_eq!(record.input_tokens, None);
        assert_eq!(record.uncached_input_tokens, None);
        assert_eq!(record.output_tokens, None);
        assert_eq!(record.total_tokens, None);
        assert_eq!(record.usage_availability, "missing");
        assert!(
            record
                .usage_validation_flags_json
                .as_deref()
                .is_some_and(|flags| flags.contains("legacy_ambiguous_zero"))
        );
        assert!(!record.semantic_output_observed_known);
    }
}
