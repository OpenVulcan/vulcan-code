-- Provider usage statistics implemented through the fixed Vulcan plugin bridge.
-- 通过固定 Vulcan 插件桥接实现的 Provider 用量统计。

-- M contains every function declared by the usage statistics manifest.
-- M 包含用量统计清单声明的全部函数。
local M = {}

-- SCHEMA_VERSION is the current isolated plugin database schema version.
-- SCHEMA_VERSION 是当前插件隔离数据库的 Schema 版本。
local SCHEMA_VERSION = 3

-- EMPTY_ARRAY preserves JSON array semantics when a query has no rows.
-- EMPTY_ARRAY 在查询无结果时保留 JSON 数组语义。
local EMPTY_ARRAY = vulcan.json.array()

-- Report whether one decoded value represents JSON null.
-- 判断一个解码值是否表示 JSON null。
--
-- value is any value received from a JSON or host boundary.
-- value 是从 JSON 或宿主边界接收的任意值。
-- Returns true only for the explicit LuaSkills null sentinel.
-- 仅对 LuaSkills 显式 null 哨兵返回 true。
local function is_json_null(value)
    return value == vulcan.json.null
end

-- active_requests holds ephemeral request phases until their terminal event arrives.
-- active_requests 在请求终态事件到达前保存临时阶段。
local active_requests = {}

-- model_refs avoids repeated dimension lookups in the long-lived Hook runtime.
-- model_refs 在长生命周期 Hook 运行时中避免重复查询模型维表。
local model_refs = {}

-- last_cleanup_date prevents repeated retention cleanup during one local calendar day.
-- last_cleanup_date 防止同一宿主自然日重复执行保留期清理。
local last_cleanup_date = nil

-- schema_ready avoids repeated migration-table reads inside one persistent runtime lease.
-- schema_ready 避免在同一个持久运行时租约内重复读取迁移表。
local schema_ready = false

-- Encode one successful plugin response envelope.
-- 编码一个成功的插件响应信封。
--
-- result is the exact JSON-safe value returned to the host.
-- result 是返回宿主的精确 JSON 安全值。
-- Returns the JSON string required by the managed plugin ABI.
-- 返回受管插件 ABI 所需的 JSON 字符串。
local function success(result)
    return vulcan.json.encode({ ok = true, result = result })
end

-- Build one tagged SQLite string parameter.
-- 构建一个带标签的 SQLite 字符串参数。
local function sql_string(value)
    return { kind = "string", value = value }
end

-- Build one tagged SQLite signed integer parameter.
-- 构建一个带标签的 SQLite 有符号整数参数。
local function sql_integer(value)
    return { kind = "int64", value = value }
end

-- Build one explicit SQLite null parameter.
-- 构建一个显式 SQLite 空值参数。
local function sql_null()
    return { kind = "null" }
end

-- Convert an optional telemetry scalar into the exact tagged SQLite protocol.
-- 将可选遥测标量转换为精确的 SQLite 标签协议。
local function sql_optional(value, value_kind)
    if value == nil or is_json_null(value) then
        return sql_null()
    end
    if value_kind == "integer" then
        return sql_integer(value)
    end
    return sql_string(value)
end

-- Return the current complete schema-validated plugin configuration.
-- 返回当前完整且通过 Schema 校验的插件配置。
local function read_config()
    return vulcan.host.call("vulcan.plugin.config.get", {})
end

-- Execute one parameterized write inside the plugin-owned SQLite database.
-- 在插件自有 SQLite 数据库内执行一条参数化写语句。
local function sqlite_execute(sql, params)
    return vulcan.host.call("vulcan.plugin.storage.sqlite.execute", {
        sql = sql,
        params = params or EMPTY_ARRAY,
    })
end

-- Execute one named bounded read inside the plugin-owned SQLite database.
-- 在插件自有 SQLite 数据库内执行一次具名的有界读取。
--
-- query_name identifies the exact aggregate phase without exposing database contents.
-- query_name 标识精确聚合阶段，同时不暴露数据库内容。
local function sqlite_query(sql, params, query_name)
    -- call_ok and response preserve the host error while attaching stable query context.
    -- call_ok 与 response 保留宿主错误，同时附加稳定查询上下文。
    local call_ok, response = pcall(vulcan.host.call, "vulcan.plugin.storage.sqlite.query", {
        sql = sql,
        params = params or EMPTY_ARRAY,
    })
    if not call_ok then
        error("Statistics SQLite query failed at " .. (query_name or "unnamed") .. ": " .. tostring(response))
    end
    return response.rows
end

-- Create or upgrade the statistics schema idempotently.
-- 幂等创建或升级统计 Schema。
--
-- applied_at_ms is an exact host telemetry timestamp when available.
-- applied_at_ms 是可用时的精确宿主遥测时间戳。
local function ensure_schema(applied_at_ms)
    if schema_ready then
        return
    end
    vulcan.host.call("vulcan.plugin.storage.sqlite.migrate", {
        version = SCHEMA_VERSION,
        plugin_version = "0.1.0",
        applied_at_ms = applied_at_ms or 0,
        statements = {
            [[CREATE TABLE IF NOT EXISTS models (
                model_ref INTEGER PRIMARY KEY AUTOINCREMENT,
                provider_key TEXT NOT NULL,
                provider_name TEXT NOT NULL,
                model_key TEXT NOT NULL,
                model_name TEXT NOT NULL,
                first_seen_at_ms INTEGER NOT NULL,
                last_seen_at_ms INTEGER NOT NULL,
                UNIQUE(provider_key, model_key)
            )]],
            [[CREATE TABLE IF NOT EXISTS api_request_events (
                request_id TEXT PRIMARY KEY,
                execution_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                project_id TEXT,
                workspace_id TEXT,
                model_ref INTEGER NOT NULL REFERENCES models(model_ref) ON DELETE RESTRICT,
                owner_kind TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                owner_name TEXT NOT NULL,
                status TEXT NOT NULL,
                attempt_index INTEGER NOT NULL,
                retry_count INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                first_event_at_ms INTEGER,
                first_token_at_ms INTEGER,
                completed_at_ms INTEGER NOT NULL,
                time_to_first_event_ms INTEGER,
                time_to_first_token_ms INTEGER,
                duration_ms INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                reasoning_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER,
                cache_creation_input_tokens INTEGER,
                finish_reason TEXT,
                error_category TEXT,
                error_code TEXT,
                calendar_date TEXT NOT NULL,
                timezone_offset_minutes INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL
            )]],
            [[ALTER TABLE api_request_events ADD COLUMN operation TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN usage_scope TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN service_entry_id TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN connection_id TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN protocol_profile_id TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN secondary_route_id TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN capability_revision TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN catalog_revision TEXT]],
            [[ALTER TABLE api_request_events ADD COLUMN input_item_count INTEGER]],
            [[ALTER TABLE api_request_events ADD COLUMN output_item_count INTEGER]],
            [[ALTER TABLE api_request_events ADD COLUMN vector_dimensions INTEGER]],
            [[ALTER TABLE api_request_events ADD COLUMN provider_cost TEXT]],
            [[CREATE INDEX IF NOT EXISTS idx_api_request_events_completed
                ON api_request_events(completed_at_ms DESC)]],
            [[CREATE INDEX IF NOT EXISTS idx_api_request_events_calendar_date
                ON api_request_events(calendar_date)]],
            [[CREATE INDEX IF NOT EXISTS idx_api_request_events_model_completed
                ON api_request_events(model_ref, completed_at_ms DESC)]],
            [[CREATE INDEX IF NOT EXISTS idx_api_request_events_owner_completed
                ON api_request_events(owner_kind, completed_at_ms DESC)]],
            [[CREATE INDEX IF NOT EXISTS idx_api_request_events_session_completed
                ON api_request_events(session_id, completed_at_ms DESC)]],
            [[CREATE INDEX IF NOT EXISTS idx_api_request_events_status_completed
                ON api_request_events(status, completed_at_ms DESC)]],
            [[CREATE INDEX IF NOT EXISTS idx_api_request_events_owner_date
                ON api_request_events(owner_kind, calendar_date)]],
            [[CREATE TABLE IF NOT EXISTS usage_daily_model (
                calendar_date TEXT NOT NULL,
                model_ref INTEGER NOT NULL REFERENCES models(model_ref) ON DELETE RESTRICT,
                owner_kind TEXT NOT NULL,
                request_count INTEGER NOT NULL,
                success_count INTEGER NOT NULL,
                failure_count INTEGER NOT NULL,
                cancelled_count INTEGER NOT NULL,
                retry_count INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                reasoning_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER NOT NULL,
                cache_creation_input_tokens INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                maximum_duration_ms INTEGER NOT NULL,
                maximum_request_tokens INTEGER NOT NULL,
                first_token_ms_sum INTEGER NOT NULL,
                first_token_count INTEGER NOT NULL,
                PRIMARY KEY(calendar_date, model_ref, owner_kind)
            )]],
            [[CREATE INDEX IF NOT EXISTS idx_usage_daily_model_date
                ON usage_daily_model(calendar_date)]],
            [[CREATE TABLE IF NOT EXISTS statistics_meta (
                meta_key TEXT PRIMARY KEY,
                meta_value TEXT NOT NULL
            )]],
            [[INSERT OR IGNORE INTO statistics_meta(meta_key, meta_value)
                VALUES ('data_revision', '0')]],
            [[CREATE TRIGGER IF NOT EXISTS trg_api_request_events_aggregate
                AFTER INSERT ON api_request_events
                BEGIN
                    INSERT INTO usage_daily_model(
                        calendar_date, model_ref, owner_kind, request_count,
                        success_count, failure_count, cancelled_count, retry_count, input_tokens,
                        output_tokens, reasoning_tokens, total_tokens,
                        cached_input_tokens, cache_creation_input_tokens, duration_ms,
                        maximum_duration_ms, maximum_request_tokens,
                        first_token_ms_sum, first_token_count
                    ) VALUES (
                        NEW.calendar_date, NEW.model_ref, NEW.owner_kind, 1,
                        CASE WHEN NEW.status = 'succeeded' THEN 1 ELSE 0 END,
                        CASE WHEN NEW.status = 'failed' THEN 1 ELSE 0 END,
                        CASE WHEN NEW.status = 'cancelled' THEN 1 ELSE 0 END,
                        NEW.retry_count, NEW.input_tokens, NEW.output_tokens,
                        NEW.reasoning_tokens, NEW.total_tokens,
                        COALESCE(NEW.cached_input_tokens, 0),
                        COALESCE(NEW.cache_creation_input_tokens, 0), NEW.duration_ms,
                        NEW.duration_ms, NEW.total_tokens,
                        COALESCE(NEW.time_to_first_token_ms, 0),
                        CASE WHEN NEW.time_to_first_token_ms IS NULL THEN 0 ELSE 1 END
                    )
                    ON CONFLICT(calendar_date, model_ref, owner_kind) DO UPDATE SET
                        request_count = request_count + 1,
                        success_count = success_count + excluded.success_count,
                        failure_count = failure_count + excluded.failure_count,
                        cancelled_count = cancelled_count + excluded.cancelled_count,
                        retry_count = retry_count + excluded.retry_count,
                        input_tokens = input_tokens + excluded.input_tokens,
                        output_tokens = output_tokens + excluded.output_tokens,
                        reasoning_tokens = reasoning_tokens + excluded.reasoning_tokens,
                        total_tokens = total_tokens + excluded.total_tokens,
                        cached_input_tokens = cached_input_tokens + excluded.cached_input_tokens,
                        cache_creation_input_tokens = cache_creation_input_tokens + excluded.cache_creation_input_tokens,
                        duration_ms = duration_ms + excluded.duration_ms,
                        maximum_duration_ms = MAX(maximum_duration_ms, excluded.maximum_duration_ms),
                        maximum_request_tokens = MAX(maximum_request_tokens, excluded.maximum_request_tokens),
                        first_token_ms_sum = first_token_ms_sum + excluded.first_token_ms_sum,
                        first_token_count = first_token_count + excluded.first_token_count;
                    UPDATE models
                        SET last_seen_at_ms = NEW.completed_at_ms
                        WHERE model_ref = NEW.model_ref;
                    UPDATE statistics_meta
                        SET meta_value = CAST(CAST(meta_value AS INTEGER) + 1 AS TEXT)
                        WHERE meta_key = 'data_revision';
                END]],
        },
    })
    schema_ready = true
end

-- Decode one exact host Hook invocation frame.
-- 解码一个精确的宿主 Hook 调用帧。
local function decode_invocation(args)
    if type(args.invocation_json) ~= "string" or args.invocation_json == "" then
        error("Usage statistics hook invocation_json is required")
    end
    return vulcan.json.decode(args.invocation_json)
end

-- Extract the versioned telemetry payload from one Hook invocation.
-- 从一次 Hook 调用中提取版本化遥测载荷。
local function telemetry_payload(args)
    -- invocation is the exact immutable frame emitted by the host Hook dispatcher.
    -- invocation 是宿主 Hook 调度器发出的精确不可变帧。
    local invocation = decode_invocation(args)
    -- payload is the exact Provider telemetry object wrapped by HookPayload.value.
    -- payload 是由 HookPayload.value 包裹的精确 Provider 遥测对象。
    local payload = invocation.payload.value
    if payload.schema_version ~= 1 then
        error("Unsupported Provider telemetry schema_version")
    end
    return payload
end

-- Resolve the persistent model dimension key without duplicating model data per request.
-- 解析持久模型维度键，避免每个请求重复保存模型数据。
local function resolve_model_ref(payload)
    -- cache_key is the exact unique database key encoded without ambiguity.
    -- cache_key 是无歧义编码的精确数据库唯一键。
    local cache_key = payload.provider_key .. "\0" .. payload.model_key
    -- cached_model contains the exact dimension ref and last persisted display names.
    -- cached_model 包含精确维度引用与最后持久化的显示名称。
    local cached_model = model_refs[cache_key]
    if cached_model ~= nil then
        if cached_model.provider_name ~= payload.provider_name or cached_model.model_name ~= payload.model_name then
            sqlite_execute(
                [[UPDATE models SET provider_name = ?1, model_name = ?2, last_seen_at_ms = ?3
                    WHERE model_ref = ?4]],
                {
                    sql_string(payload.provider_name), sql_string(payload.model_name),
                    sql_integer(payload.completed_at_ms), sql_integer(cached_model.model_ref),
                }
            )
            cached_model.provider_name = payload.provider_name
            cached_model.model_name = payload.model_name
        end
        return cached_model.model_ref
    end
    sqlite_execute(
        [[INSERT INTO models(
            provider_key, provider_name, model_key, model_name, first_seen_at_ms, last_seen_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(provider_key, model_key) DO UPDATE SET
            provider_name = excluded.provider_name,
            model_name = excluded.model_name,
            last_seen_at_ms = excluded.last_seen_at_ms]],
        {
            sql_string(payload.provider_key), sql_string(payload.provider_name),
            sql_string(payload.model_key), sql_string(payload.model_name),
            sql_integer(payload.started_at_ms), sql_integer(payload.completed_at_ms),
        }
    )
    -- rows contains the unique model dimension row guaranteed by the database constraint.
    -- rows 包含由数据库约束保证唯一的模型维度行。
    local rows = sqlite_query(
        [[SELECT model_ref FROM models WHERE provider_key = ?1 AND model_key = ?2]],
        { sql_string(payload.provider_key), sql_string(payload.model_key) }
    )
    if #rows ~= 1 then
        error("Provider model dimension lookup did not return exactly one row")
    end
    model_refs[cache_key] = {
        model_ref = rows[1].model_ref,
        provider_name = payload.provider_name,
        model_name = payload.model_name,
    }
    return rows[1].model_ref
end

-- Delete expired raw request facts while retaining permanent model and daily aggregate records.
-- 删除过期原始请求事实，同时永久保留模型与每日聚合记录。
local function cleanup_expired_facts(config, calendar_date)
    -- A zero retention sentinel keeps raw statistics indefinitely and performs no destructive cleanup.
    -- 零值保留期哨兵表示永久保留原始统计，并且不执行任何破坏性清理。
    if config.retentionDays == 0 then
        return
    end
    if last_cleanup_date == calendar_date then
        return
    end
    -- retention_modifier is accepted as SQLite's documented relative date modifier.
    -- retention_modifier 使用 SQLite 文档化的相对日期修饰符。
    local retention_modifier = "-" .. tostring(config.retentionDays) .. " days"
    sqlite_execute(
        [[DELETE FROM api_request_events WHERE calendar_date < date(?1, ?2)]],
        { sql_string(calendar_date), sql_string(retention_modifier) }
    )
    last_cleanup_date = calendar_date
end

-- Read the current monotonic statistics revision.
-- 读取当前单调递增统计版本。
local function current_revision()
    -- rows contains the single initialized metadata record.
    -- rows 包含唯一的已初始化元数据记录。
    local rows = sqlite_query(
        [[SELECT CAST(meta_value AS INTEGER) AS revision FROM statistics_meta WHERE meta_key = 'data_revision']],
        EMPTY_ARRAY
    )
    if #rows ~= 1 then
        error("Statistics data revision is missing")
    end
    return rows[1].revision
end

-- Return the current complete plugin configuration to the mounted UI.
-- 向挂载界面返回当前完整插件配置。
function M.settings_get(args)
    return success(read_config())
end

-- Persist one complete schema-validated plugin configuration from the mounted UI.
-- 持久化挂载界面提交的一份完整且通过 Schema 校验的插件配置。
function M.settings_update(args)
    -- updated is returned only after host-owned atomic configuration persistence succeeds.
    -- updated 仅在宿主持有的配置原子持久化成功后返回。
    local updated = vulcan.host.call("vulcan.plugin.config.update", { value = args.arguments })
    return success(updated)
end

-- Track the started phase in ephemeral Hook-runtime memory.
-- 在 Hook 运行时临时内存中跟踪请求开始阶段。
function M.provider_request_started(args)
    -- payload is the exact versioned started-event object.
    -- payload 是精确的版本化请求开始事件对象。
    local payload = telemetry_payload(args)
    active_requests[payload.request_id] = {
        started_at_ms = payload.started_at_ms,
        first_token_at_ms = nil,
        retrying = false,
    }
    return success(vulcan.json.object())
end

-- Track the first visible token phase without writing a high-frequency fact row.
-- 跟踪首个可见 Token 阶段，不写入高频事实行。
function M.provider_response_first_token(args)
    -- payload is the exact versioned first-token event object.
    -- payload 是精确的版本化首 Token 事件对象。
    local payload = telemetry_payload(args)
    -- active_request is present only when the started event was observed by this lease.
    -- active_request 仅在当前租约已观测开始事件时存在。
    local active_request = active_requests[payload.request_id]
    if active_request ~= nil then
        active_request.first_token_at_ms = payload.first_token_at_ms
    end
    return success(vulcan.json.object())
end

-- Track the retry decision in ephemeral Hook-runtime memory.
-- 在 Hook 运行时临时内存中跟踪重试决策。
function M.provider_request_retrying(args)
    -- payload is the exact versioned retrying-event object.
    -- payload 是精确的版本化重试事件对象。
    local payload = telemetry_payload(args)
    -- active_request can be absent because the failed attempt is terminal before this event.
    -- active_request 可能为空，因为失败尝试会先于当前事件进入终态。
    local active_request = active_requests[payload.request_id]
    if active_request ~= nil then
        active_request.retrying = true
    end
    return success(vulcan.json.object())
end

-- Persist one idempotent terminal Provider request fact and notify mounted UI subscribers.
-- 持久化一条幂等 Provider 请求终态事实，并通知已挂载 UI 订阅方。
function M.provider_request_completed(args)
    -- payload is the exact versioned terminal-event object.
    -- payload 是精确的版本化终态事件对象。
    local payload = telemetry_payload(args)
    if payload.status ~= "succeeded" and payload.status ~= "failed" and payload.status ~= "cancelled" then
        error("Unsupported Provider request terminal status")
    end
    active_requests[payload.request_id] = nil
    -- config determines whether new facts are captured and how long raw facts remain.
    -- config 决定是否采集新事实，以及原始事实的保留期限。
    local config = read_config()
    if not config.enabled then
        return success(vulcan.json.object())
    end
    ensure_schema(payload.completed_at_ms)
    -- model_ref points to the permanent provider-and-model dimension record.
    -- model_ref 指向永久保存的供应商与模型维度记录。
    local model_ref = resolve_model_ref(payload)
    sqlite_execute(
        [[INSERT OR IGNORE INTO api_request_events(
            request_id, execution_id, session_id, project_id, workspace_id,
            model_ref, owner_kind, owner_id,
            owner_name, usage_scope, operation, service_entry_id, connection_id,
            protocol_profile_id, secondary_route_id, capability_revision,
            catalog_revision, input_item_count, output_item_count,
            vector_dimensions, provider_cost, status, attempt_index, retry_count, started_at_ms,
            first_event_at_ms, first_token_at_ms, completed_at_ms,
            time_to_first_event_ms, time_to_first_token_ms, duration_ms,
            input_tokens, output_tokens, reasoning_tokens, total_tokens,
            cached_input_tokens, cache_creation_input_tokens, finish_reason,
            error_category, error_code, calendar_date, timezone_offset_minutes,
            created_at_ms
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
            ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40,
            ?41, ?42, ?43
        )]],
        {
            sql_string(payload.request_id), sql_string(payload.execution_id),
            sql_string(payload.session_id), sql_optional(payload.project_id, "string"),
            sql_optional(payload.workspace_id, "string"), sql_integer(model_ref),
            sql_string(payload.owner_kind), sql_string(payload.owner_id),
            sql_string(payload.owner_name), sql_optional(payload.usage_scope, "string"),
            sql_optional(payload.operation, "string"),
            sql_optional(payload.service_entry_id, "string"),
            sql_optional(payload.connection_id, "string"),
            sql_optional(payload.protocol_profile_id, "string"),
            sql_optional(payload.secondary_route_id, "string"),
            sql_optional(payload.capability_revision, "string"),
            sql_optional(payload.catalog_revision, "string"),
            sql_optional(payload.input_item_count, "integer"),
            sql_optional(payload.output_item_count, "integer"),
            sql_optional(payload.vector_dimensions, "integer"),
            sql_optional(payload.cost, "string"), sql_string(payload.status),
            sql_integer(payload.attempt_index), sql_integer(payload.retry_count),
            sql_integer(payload.started_at_ms), sql_optional(payload.first_event_at_ms, "integer"),
            sql_optional(payload.first_token_at_ms, "integer"), sql_integer(payload.completed_at_ms),
            sql_optional(payload.time_to_first_event_ms, "integer"),
            sql_optional(payload.time_to_first_token_ms, "integer"),
            sql_integer(payload.duration_ms), sql_integer(payload.uncached_input_tokens),
            sql_integer(payload.output_tokens), sql_integer(payload.reasoning_tokens),
            sql_integer(payload.total_tokens), sql_optional(payload.cached_input_tokens, "integer"),
            sql_optional(payload.cache_creation_input_tokens, "integer"),
            sql_optional(payload.finish_reason, "string"), sql_optional(payload.error_category, "string"),
            sql_optional(payload.error_code, "string"), sql_string(payload.calendar_date),
            sql_integer(payload.timezone_offset_minutes), sql_integer(payload.completed_at_ms),
        }
    )
    cleanup_expired_facts(config, payload.calendar_date)
    -- revision is incremented atomically by the fact insertion trigger.
    -- revision 由事实插入触发器原子递增。
    local revision = current_revision()
    vulcan.host.call("vulcan.plugin.ui.invalidate", {
        topic = "statistics.changed",
        revision = revision,
    })
    return success({ revision = revision })
end

-- Build one exact optional owner predicate and numbered parameter list.
-- 构建一个精确的可选归属筛选条件与编号参数列表。
--
-- owner_kind is the schema-enumerated owner, column_name is a trusted SQL identifier,
-- and parameter_index is the exact one-based SQLite placeholder position.
-- owner_kind 是 Schema 枚举归属，column_name 是可信 SQL 标识，parameter_index 是精确的一基 SQLite 占位序号。
local function owner_filter(owner_kind, column_name, parameter_index)
    if owner_kind == nil then
        -- Empty parameters must be fresh because callers may append query-specific values.
        -- 空参数必须每次新建，因为调用方可能追加查询专用值。
        return "", {}
    end
    return " AND " .. column_name .. " = ?" .. tostring(parameter_index), { sql_string(owner_kind) }
end

-- Build one exact optional model predicate and numbered integer parameter list.
-- 构建一个精确的可选模型谓词与编号整数参数列表。
--
-- model_ref is the unique persistent model dimension key, or nil for every model.
-- model_ref 是唯一持久模型维度键；nil 表示全部模型。
-- column_name is the trusted query-owned model reference column.
-- column_name 是由查询持有的可信模型引用列。
-- parameter_index is the next numbered SQLite placeholder index.
-- parameter_index 是下一个 SQLite 编号占位符索引。
-- Returns the SQL predicate suffix and its fresh ordered parameter array.
-- 返回 SQL 谓词后缀及其全新有序参数数组。
local function model_filter(model_ref, column_name, parameter_index)
    if model_ref == nil then
        return "", {}
    end
    return " AND " .. column_name .. " = ?" .. tostring(parameter_index), { sql_integer(model_ref) }
end

-- Build one optional local-calendar range predicate and its numbered parameter.
-- 构建一个可选的本地自然日范围条件及其编号参数。
--
-- range_days uses zero for all retained aggregates; positive values select an inclusive window.
-- range_days 使用零表示全部保留聚合；正数表示一个包含首尾日期的窗口。
local function range_filter(range_days, column_name, parameter_index)
    if range_days == 0 then
        return "", {}
    end
    -- modifier is derived exclusively from the schema-enumerated day count.
    -- modifier 仅由 Schema 枚举的天数派生。
    local modifier = "-" .. tostring(range_days - 1) .. " days"
    return " AND " .. column_name .. " >= date('now', 'localtime', ?" .. tostring(parameter_index) .. ")", { sql_string(modifier) }
end

-- Append all ordered values from one array into another array.
-- 将一个数组中的全部有序值追加到另一个数组。
local function append_values(target, values)
    for _, value in ipairs(values) do
        table.insert(target, value)
    end
end

-- Resolve the allowlisted aggregate expression for one dashboard metric.
-- 为一个仪表盘指标解析白名单内的聚合表达式。
local function metric_expression(metric)
    -- expressions maps schema-enumerated metric keys to trusted SQL fragments.
    -- expressions 将 Schema 枚举指标键映射为可信 SQL 片段。
    local expressions = {
        total_tokens = "SUM(total_tokens)",
        input_tokens = "SUM(input_tokens)",
        output_tokens = "SUM(output_tokens)",
        requests = "SUM(request_count)",
        duration_ms = "CASE WHEN SUM(request_count) = 0 THEN 0 ELSE SUM(duration_ms) / SUM(request_count) END",
        time_to_first_token_ms = "CASE WHEN SUM(first_token_count) = 0 THEN 0 ELSE SUM(first_token_ms_sum) / SUM(first_token_count) END",
    }
    -- expression must exist because action arguments were validated by the host schema.
    -- expression 必须存在，因为 action 参数已由宿主 Schema 校验。
    local expression = expressions[metric]
    if expression == nil then
        error("Unsupported statistics metric")
    end
    return expression
end

-- Load KPI, heatmap, line-series, and model-breakdown data from daily aggregates.
-- 从每日聚合数据加载 KPI、热力图、折线序列与模型分布。
function M.dashboard_get(args)
    ensure_schema(0)
    -- input is the exact schema-validated dashboard query.
    -- input 是通过 Schema 精确校验的仪表盘查询。
    local input = args.arguments
    -- range_sql and range_params represent the selected finite window or all retained aggregates.
    -- range_sql 与 range_params 表示选定有限窗口或全部保留聚合。
    local range_sql, range_params = range_filter(input.rangeDays, "calendar_date", 1)
    -- owner_after_range_sql and owner_params bind the optional owner after the range value.
    -- owner_after_range_sql 与 owner_params 在范围值之后绑定可选归属。
    local owner_after_range_sql, owner_params = owner_filter(input.ownerKind, "owner_kind", #range_params + 1)
    -- model_after_range_owner_sql binds the exact optional model after range and owner values.
    -- model_after_range_owner_sql 在范围与归属值之后绑定精确的可选模型。
    local model_after_range_owner_sql, model_filter_params = model_filter(
        input.modelRef,
        "model_ref",
        #range_params + #owner_params + 1
    )
    -- aggregate_filter_sql is shared by every unaliased daily aggregate query.
    -- aggregate_filter_sql 由每个无别名的每日聚合查询共享。
    local aggregate_filter_sql = range_sql .. owner_after_range_sql .. model_after_range_owner_sql
    -- summary_params keeps range, owner, and model values in exact placeholder order.
    -- summary_params 按精确占位符顺序保存范围、归属与模型值。
    local summary_params = {}
    append_values(summary_params, range_params)
    append_values(summary_params, owner_params)
    append_values(summary_params, model_filter_params)
    -- summary_rows contains one aggregate KPI record even when no usage exists.
    -- summary_rows 即使无用量也包含一条聚合 KPI 记录。
    local summary_rows = sqlite_query(
        [[SELECT
            COALESCE(SUM(request_count), 0) AS requests,
            COALESCE(SUM(success_count), 0) AS successes,
            COALESCE(SUM(failure_count), 0) AS failures,
            COALESCE(SUM(cancelled_count), 0) AS cancelled,
            COALESCE(SUM(retry_count), 0) AS retries,
            COALESCE(SUM(input_tokens), 0) AS inputTokens,
            COALESCE(SUM(output_tokens), 0) AS outputTokens,
            COALESCE(SUM(reasoning_tokens), 0) AS reasoningTokens,
            COALESCE(SUM(total_tokens), 0) AS totalTokens,
            COALESCE(SUM(cached_input_tokens), 0) AS cachedInputTokens,
            COALESCE(SUM(cache_creation_input_tokens), 0) AS cacheCreationInputTokens,
            COALESCE(SUM(duration_ms), 0) AS durationMs,
            CASE WHEN SUM(request_count) IS NULL OR SUM(request_count) = 0 THEN 0
                ELSE SUM(duration_ms) / SUM(request_count) END AS averageDurationMs,
            COALESCE(SUM(CASE WHEN owner_kind = 'main_agent' THEN total_tokens ELSE 0 END), 0) AS mainAgentTokens,
            COALESCE(SUM(CASE WHEN owner_kind = 'subagent' THEN total_tokens ELSE 0 END), 0) AS subagentTokens,
            COALESCE(SUM(CASE WHEN owner_kind IN (
                'context_compaction', 'git_commit_summary', 'internal_llm',
                'internal_embedding', 'internal_rerank', 'internal_multimodal'
            ) THEN total_tokens ELSE 0 END), 0) AS internalTokens,
            COALESCE(MAX(maximum_duration_ms), 0) AS longestRequestMs,
            COALESCE(MAX(maximum_request_tokens), 0) AS peakRequestTokens,
            CASE WHEN SUM(first_token_count) IS NULL OR SUM(first_token_count) = 0 THEN 0
                ELSE SUM(first_token_ms_sum) / SUM(first_token_count) END AS averageFirstTokenMs,
            COUNT(DISTINCT calendar_date) AS activeDays
        FROM usage_daily_model
        WHERE 1 = 1]] .. aggregate_filter_sql,
        summary_params,
        "dashboard.summary"
    )
    -- streak_rows computes exact local-calendar current and longest activity streaks.
    -- streak_rows 计算精确的宿主本地自然日当前与最长活跃连续天数。
    local streak_rows = sqlite_query(
        [[WITH active AS (
            SELECT DISTINCT calendar_date
            FROM usage_daily_model
            WHERE 1 = 1]] .. aggregate_filter_sql .. [[
        ), numbered AS (
            SELECT calendar_date,
                   julianday(calendar_date) - ROW_NUMBER() OVER (ORDER BY calendar_date) AS island
            FROM active
        ), streaks AS (
            SELECT MIN(calendar_date) AS start_date, MAX(calendar_date) AS end_date, COUNT(*) AS days
            FROM numbered GROUP BY island
        )
        SELECT COALESCE(MAX(days), 0) AS longestStreakDays,
               COALESCE(MAX(CASE WHEN end_date = date('now', 'localtime') THEN days ELSE 0 END), 0) AS currentStreakDays
        FROM streaks]],
        summary_params,
        "dashboard.streak"
    )
    -- heatmap_owner_sql binds the optional owner as the first fixed-window parameter.
    -- heatmap_owner_sql 把可选归属绑定为固定窗口查询的第一个参数。
    local heatmap_owner_sql, heatmap_owner_params = owner_filter(input.ownerKind, "owner_kind", 1)
    -- heatmap_model_sql binds the optional model after the fixed-window owner parameter.
    -- heatmap_model_sql 在固定窗口归属参数之后绑定可选模型。
    local heatmap_model_sql, heatmap_model_params = model_filter(
        input.modelRef,
        "model_ref",
        #heatmap_owner_params + 1
    )
    -- heatmap_params preserves the owner-then-model fixed-window parameter order.
    -- heatmap_params 保持固定窗口查询先归属后模型的参数顺序。
    local heatmap_params = {}
    append_values(heatmap_params, heatmap_owner_params)
    append_values(heatmap_params, heatmap_model_params)
    -- heatmap_rows contains one rich activity cell and top-model label per local calendar day.
    -- heatmap_rows 每个宿主本地自然日包含一个丰富活动单元与最常用模型名称。
    local heatmap_rows = sqlite_query(
        [[WITH filtered AS (
            SELECT calendar_date, model_ref, total_tokens, request_count, success_count
            FROM usage_daily_model
            WHERE calendar_date >= date('now', 'localtime', '-364 days')]] .. heatmap_owner_sql .. heatmap_model_sql .. [[
        ), daily AS (
            SELECT calendar_date, SUM(total_tokens) AS value,
                   SUM(request_count) AS requests, SUM(success_count) AS successes
            FROM filtered GROUP BY calendar_date
        ), ranked_models AS (
            SELECT calendar_date, model_ref, SUM(total_tokens) AS model_tokens,
                   ROW_NUMBER() OVER (
                       PARTITION BY calendar_date
                       ORDER BY SUM(total_tokens) DESC, model_ref
                   ) AS model_rank
            FROM filtered GROUP BY calendar_date, model_ref
        )
        SELECT d.calendar_date AS date, d.value AS value,
               d.requests AS requests, d.successes AS successes,
               m.provider_name AS topProviderName, m.model_name AS topModelName
        FROM daily d
        LEFT JOIN ranked_models r
            ON r.calendar_date = d.calendar_date AND r.model_rank = 1
        LEFT JOIN models m ON m.model_ref = r.model_ref
        ORDER BY d.calendar_date]],
        heatmap_params,
        "dashboard.heatmap"
    )
    -- peak_rows computes the maximum total Token volume across complete local calendar days.
    -- peak_rows 计算完整宿主本地自然日中的最大 Token 总量。
    local peak_rows = sqlite_query(
        [[SELECT COALESCE(MAX(daily_tokens), 0) AS peakDailyTokens FROM (
            SELECT calendar_date, SUM(total_tokens) AS daily_tokens
            FROM usage_daily_model
            WHERE 1 = 1]] .. aggregate_filter_sql .. [[
            GROUP BY calendar_date
        )]],
        summary_params,
        "dashboard.peak"
    )
    -- expression is selected exclusively from the schema-enumerated allowlist.
    -- expression 仅从 Schema 枚举白名单中选择。
    local expression = metric_expression(input.metric)
    -- series_params is reused by numbered placeholders in both ranked and projected CTEs.
    -- series_params 由排名与投影 CTE 中的编号占位符共同复用。
    local series_params = {}
    append_values(series_params, range_params)
    append_values(series_params, owner_params)
    append_values(series_params, model_filter_params)
    -- projected_range_sql applies the same numbered parameters to the aliased daily table.
    -- projected_range_sql 把同一组编号参数应用到带别名的日级表。
    local projected_range_sql = range_filter(input.rangeDays, "d.calendar_date", 1)
    -- projected_owner_sql applies the same optional owner parameter to the aliased daily table.
    -- projected_owner_sql 把同一个可选归属参数应用到带别名的日级表。
    local projected_owner_sql = owner_filter(input.ownerKind, "d.owner_kind", #range_params + 1)
    -- projected_model_sql applies the same optional model parameter to the aliased daily table.
    -- projected_model_sql 把同一个可选模型参数应用到带别名的日级表。
    local projected_model_sql = model_filter(
        input.modelRef,
        "d.model_ref",
        #range_params + #owner_params + 1
    )
    -- series_rows groups the top five models and one stable Other series.
    -- series_rows 把前五个模型与一个稳定的其他序列分组。
    local series_rows = sqlite_query(
        [[WITH ranked AS (
            SELECT model_ref, ]] .. expression .. [[ AS metric_value
            FROM usage_daily_model
            WHERE 1 = 1]] .. aggregate_filter_sql .. [[
            GROUP BY model_ref ORDER BY metric_value DESC, model_ref LIMIT 5
        ), projected AS (
            SELECT d.calendar_date,
                   CASE WHEN r.model_ref IS NULL THEN 0 ELSE d.model_ref END AS series_ref,
                   ]] .. expression .. [[ AS value
            FROM usage_daily_model d
            LEFT JOIN ranked r ON r.model_ref = d.model_ref
            WHERE 1 = 1]] .. projected_range_sql .. projected_owner_sql .. projected_model_sql .. [[
            GROUP BY d.calendar_date, series_ref
        )
        SELECT p.calendar_date AS date, p.series_ref AS seriesRef,
               CASE WHEN p.series_ref = 0 THEN 'other' ELSE m.provider_key || '/' || m.model_key END AS seriesKey,
               CASE WHEN p.series_ref = 0 THEN 'Other' ELSE m.provider_name || ' / ' || m.model_name END AS label,
               p.value AS value
        FROM projected p
        LEFT JOIN models m ON m.model_ref = p.series_ref
        ORDER BY p.calendar_date, p.series_ref]],
        series_params,
        "dashboard.series"
    )
    -- model_rows provides the ten most-used persistent model dimension records.
    -- model_rows 提供用量最高的十条永久模型维度记录。
    local model_params = {}
    append_values(model_params, series_params)
    local model_rows = sqlite_query(
        [[SELECT m.model_ref AS modelRef, m.provider_key AS providerKey,
               m.provider_name AS providerName, m.model_key AS modelKey,
               m.model_name AS modelName, SUM(d.total_tokens) AS totalTokens,
               SUM(d.request_count) AS requests
        FROM usage_daily_model d
        JOIN models m ON m.model_ref = d.model_ref
        WHERE 1 = 1]] .. projected_range_sql .. projected_owner_sql .. projected_model_sql .. [[
        GROUP BY m.model_ref ORDER BY totalTokens DESC, m.model_ref LIMIT 10]],
        model_params,
        "dashboard.models"
    )
    -- available_model_rows contains every permanent model dimension independently of active filters.
    -- available_model_rows 包含独立于活动筛选条件的全部永久模型维度。
    local available_model_rows = sqlite_query(
        [[SELECT model_ref AS modelRef, provider_name AS providerName, model_name AS modelName
        FROM models
        ORDER BY provider_name COLLATE NOCASE, model_name COLLATE NOCASE, model_ref]],
        {},
        "dashboard.available_models"
    )
    -- summary is the normalized KPI record augmented with streak facts.
    -- summary 是补充连续活跃事实后的规范 KPI 记录。
    local summary = summary_rows[1]
    summary.currentStreakDays = streak_rows[1].currentStreakDays
    summary.longestStreakDays = streak_rows[1].longestStreakDays
    summary.peakDailyTokens = peak_rows[1].peakDailyTokens
    summary.successRate = summary.requests == 0 and 0 or summary.successes * 100 / summary.requests
    -- cacheEligibleInputTokens combines every normalized input-token category that could produce a cache hit.
    -- cacheEligibleInputTokens 合并所有可能产生缓存命中的归一化输入 Token 类别。
    local cache_eligible_input_tokens = summary.inputTokens + summary.cachedInputTokens + summary.cacheCreationInputTokens
    summary.cacheHitRate = cache_eligible_input_tokens == 0 and 0 or summary.cachedInputTokens * 100 / cache_eligible_input_tokens
    -- model shares use the exact selected-range total and remain zero for an empty range.
    -- 模型占比使用精确的已选范围总量；空范围时保持为零。
    for _, model in ipairs(model_rows) do
        model.sharePercent = summary.totalTokens == 0 and 0 or model.totalTokens * 100 / summary.totalTokens
    end
    return success({
        revision = current_revision(),
        rangeDays = input.rangeDays,
        metric = input.metric,
        summary = summary,
        heatmap = heatmap_rows,
        series = series_rows,
        models = model_rows,
        availableModels = available_model_rows,
    })
end

-- Return only the monotonic data revision for low-cost polling fallback.
-- 仅返回单调数据修订号，用于低成本轮询回退。
function M.revision_get(args)
    ensure_schema(0)
    return success({ revision = current_revision() })
end

-- Load bounded recent raw request facts with permanent model names.
-- 加载有界的最近原始请求事实及永久模型名称。
function M.recent_get(args)
    ensure_schema(0)
    -- input is the exact schema-validated recent-request query.
    -- input 是通过 Schema 精确校验的最近请求查询。
    local input = args.arguments
    -- owner_sql and owner_params represent the documented optional owner filter.
    -- owner_sql 与 owner_params 表示文档化的可选归属筛选。
    local owner_sql, owner_params = owner_filter(input.ownerKind, "e.owner_kind", 1)
    -- model_sql and model_params bind the exact optional model after the owner value.
    -- model_sql 与 model_params 在归属值之后绑定精确的可选模型。
    local model_sql, model_params = model_filter(input.modelRef, "e.model_ref", #owner_params + 1)
    -- query_params preserves owner, model, and limit values in exact placeholder order.
    -- query_params 按精确占位符顺序保存归属、模型与限制值。
    local query_params = {}
    append_values(query_params, owner_params)
    append_values(query_params, model_params)
    -- limit_parameter_index follows the optional owner and model parameters exactly.
    -- limit_parameter_index 精确位于可选归属与模型参数之后。
    local limit_parameter_index = #query_params + 1
    table.insert(query_params, sql_integer(input.limit))
    -- rows is bounded by the validated maximum of one hundred facts.
    -- rows 受已校验的一百条最大事实数限制。
    local rows = sqlite_query(
        [[SELECT e.request_id AS requestId, e.completed_at_ms AS completedAtMs,
               m.provider_name AS providerName, m.model_name AS modelName,
               e.owner_kind AS ownerKind, e.owner_name AS ownerName, e.status AS status,
               e.input_tokens AS inputTokens, e.output_tokens AS outputTokens,
               e.total_tokens AS totalTokens, e.cached_input_tokens AS cachedInputTokens,
               e.time_to_first_token_ms AS timeToFirstTokenMs, e.duration_ms AS durationMs,
               e.retry_count AS retryCount, e.error_category AS errorCategory,
               e.error_code AS errorCode
        FROM api_request_events e
        JOIN models m ON m.model_ref = e.model_ref
        WHERE 1 = 1]] .. owner_sql .. model_sql .. [[
        ORDER BY e.completed_at_ms DESC LIMIT ?]] .. tostring(limit_parameter_index),
        query_params,
        "recent.requests"
    )
    return success({ requests = rows })
end

-- Load grouped redacted error categories from permanent daily-range facts.
-- 从永久每日范围事实加载分组后的脱敏错误类别。
function M.errors_get(args)
    ensure_schema(0)
    -- input is the exact schema-validated dashboard range query.
    -- input 是通过 Schema 精确校验的仪表盘范围查询。
    local input = args.arguments
    -- range_sql and range_params select a finite window or all retained request facts.
    -- range_sql 与 range_params 选择有限窗口或全部保留请求事实。
    local range_sql, range_params = range_filter(input.rangeDays, "e.calendar_date", 1)
    -- owner_sql and owner_params represent the documented optional owner filter.
    -- owner_sql 与 owner_params 表示文档化的可选归属筛选。
    local owner_sql, owner_params = owner_filter(input.ownerKind, "e.owner_kind", #range_params + 1)
    -- model_sql and model_params bind the exact optional model after range and owner values.
    -- model_sql 与 model_params 在范围与归属值之后绑定精确的可选模型。
    local model_sql, model_params = model_filter(
        input.modelRef,
        "e.model_ref",
        #range_params + #owner_params + 1
    )
    -- params preserves the exact range-then-owner-then-model parameter ordering.
    -- params 保持精确的范围后归属再模型参数顺序。
    local params = {}
    append_values(params, range_params)
    append_values(params, owner_params)
    append_values(params, model_params)
    -- rows contains only stable redacted error categories and codes, never raw messages.
    -- rows 仅包含稳定脱敏错误类别与错误码，绝不包含原始消息。
    local rows = sqlite_query(
        [[SELECT COALESCE(e.error_category, 'unknown') AS errorCategory,
               COALESCE(e.error_code, '') AS errorCode, COUNT(*) AS count
        FROM api_request_events e
        WHERE e.status = 'failed']] .. range_sql .. owner_sql .. model_sql .. [[
        GROUP BY e.error_category, e.error_code ORDER BY count DESC LIMIT 20]],
        params,
        "errors.grouped"
    )
    return success({ errors = rows })
end

-- Create the durable schema immediately after plugin package installation.
-- 在插件包安装完成后立即创建持久 Schema。
function M.on_install(args)
    ensure_schema(0)
    return success({ installed = true })
end

-- Prepare the plugin schema before enabling event capture.
-- 在启用事件采集前准备插件 Schema。
function M.on_enable(args)
    ensure_schema(0)
    return success({ enabled = true })
end

-- Flush durable SQLite state while disabling the plugin.
-- 停用插件时刷新持久 SQLite 状态。
function M.on_disable(args)
    vulcan.host.call("vulcan.plugin.storage.sqlite.flush", {})
    return success({ enabled = false })
end

-- Flush statistics before package removal while leaving deletion policy to the host.
-- 在移除插件包前刷新统计数据，并把数据删除策略交由宿主处理。
function M.on_uninstall(args)
    vulcan.host.call("vulcan.plugin.storage.sqlite.flush", {})
    return success({ uninstalled = true, data_retained = true })
end

-- Reject generic model-visible dispatch because this plugin exposes actions and Hooks only.
-- 拒绝通用模型可见分发，因为当前插件仅暴露 Action 与 Hook。
function M.dispatch(args)
    error("Vulcan Usage Statistics does not expose provider-visible plugin tools")
end

-- Close one action runtime lease without additional resources.
-- 关闭一个 Action 运行时租约，不保留额外资源。
function M.shutdown(args)
    return success({ closed = true })
end

-- Clear ephemeral Hook state before the persistent Hook lease closes.
-- 在持久 Hook 租约关闭前清理临时 Hook 状态。
function M.shutdown_hooks(args)
    active_requests = {}
    model_refs = {}
    schema_ready = false
    return success({ closed = true })
end

return M
