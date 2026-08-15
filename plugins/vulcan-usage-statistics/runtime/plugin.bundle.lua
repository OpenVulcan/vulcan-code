-- Provider usage statistics implemented through the host-registered system VLDB binding.
-- 通过宿主注册的系统 VLDB 绑定实现 Provider 用量统计。

-- M contains every function declared by the usage statistics manifest.
-- M 包含用量统计清单声明的全部函数。
local M = {}

-- SYSTEM_STATISTICS_BINDING is the stable logical VLDB target registered by VulcanCode.
-- SYSTEM_STATISTICS_BINDING 是 VulcanCode 注册的稳定逻辑 VLDB 目标。
local SYSTEM_STATISTICS_BINDING = "system.statistics"

-- EMPTY_ARRAY preserves JSON array semantics for empty parameter lists and result projections.
-- EMPTY_ARRAY 为无参数列表和空结果投影保留 JSON 数组语义。
local EMPTY_ARRAY = vulcan.json.array()

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

-- Return the exact schema-validated input object from one host action frame.
-- 从一个宿主 Action 帧返回精确的经 Schema 校验输入对象。
--
-- args is the fixed action frame produced by the managed plugin bridge.
-- args 是由受管插件桥产生的固定 Action 帧。
-- Returns args.arguments after strict table validation.
-- 严格校验表结构后返回 args.arguments。
local function action_arguments(args)
    if type(args) ~= "table" or type(args.arguments) ~= "table" then
        error("Vulcan Usage Statistics action requires an arguments object")
    end
    return args.arguments
end

-- Query the registered system-statistics SQLite binding.
-- 查询已注册的系统统计 SQLite 绑定。
--
-- sql is one exact parameterized query and params is its tagged scalar list.
-- sql 是一条精确参数化查询，params 是对应的标签标量列表。
-- Returns the decoded row array produced by the host VLDB service.
-- 返回宿主 VLDB 服务产生的已解码行数组。
local function sqlite_query(sql, params)
    local result = vulcan.host.call("vulcan.vldb.sqlite.query.v1", {
        binding = SYSTEM_STATISTICS_BINDING,
        sql = sql,
        params = params or EMPTY_ARRAY,
    })
    if type(result) ~= "table" or type(result.rows) ~= "table" then
        error("Vulcan Usage Statistics received an invalid VLDB query result")
    end
    return result.rows
end

-- Execute one write script against the registered system-statistics SQLite binding.
-- 对已注册的系统统计 SQLite 绑定执行一段写入脚本。
--
-- sql is one exact script and params is its tagged scalar list.
-- sql 是一段精确脚本，params 是对应的标签标量列表。
-- Returns the typed Controller execution result.
-- 返回类型化 Controller 执行结果。
local function sqlite_execute(sql, params)
    return vulcan.host.call("vulcan.vldb.sqlite.execute.v1", {
        binding = SYSTEM_STATISTICS_BINDING,
        sql = sql,
        params = params or EMPTY_ARRAY,
    })
end

-- Decode one required JSON text column from the system statistics database.
-- 从系统统计数据库解码一个必需的 JSON 文本列。
--
-- value is the persisted JSON text and field_name identifies failures.
-- value 是持久化 JSON 文本，field_name 标识失败字段。
-- Returns the decoded JSON value.
-- 返回已解码 JSON 值。
local function decode_json_text(value, field_name)
    if type(value) ~= "string" then
        error("Vulcan Usage Statistics requires JSON text field " .. field_name)
    end
    return vulcan.json.decode(value)
end

-- Load the authoritative Provider usage dashboard directly from system VLDB.
-- 直接从系统 VLDB 加载权威 Provider 用量仪表盘。
--
-- Returns one schema-validated dashboard projection.
-- 返回一个通过 Schema 校验的仪表盘投影。
local function load_dashboard()
    local daily = sqlite_query([[
SELECT calendar_date AS calendarDate, provider_id AS providerId,
       service_entry_id AS serviceEntryId, consumer_id AS consumerId, operation,
       model_id AS modelId, currency, attempt_count AS attemptCount,
       successful_attempt_count AS successfulAttemptCount,
       reported_usage_attempt_count AS reportedUsageAttemptCount,
       input_tokens AS inputTokens, uncached_input_tokens AS uncachedInputTokens,
       cache_read_tokens AS cacheReadTokens, cache_write_tokens AS cacheWriteTokens,
       output_tokens AS outputTokens, reasoning_tokens AS reasoningTokens,
       total_tokens AS totalTokens, reported_cost AS reportedCost,
       estimated_cost AS estimatedCost, aggregate_revision AS aggregateRevision
FROM provider_usage_daily_v2
ORDER BY calendar_date DESC, provider_id, service_entry_id, consumer_id, operation, model_id, currency
LIMIT 500;
]])
    local recent_attempts = sqlite_query([[
SELECT attempts.attempt_id AS attemptId, attempts.consumer_id AS consumerId,
       attempts.operation, attempts.provider_id AS providerId,
       attempts.service_entry_id AS serviceEntryId,
       attempts.mapped_model AS modelId, attempts.transport,
       attempts.http_status AS httpStatus,
       attempts.websocket_close_code AS websocketCloseCode,
       attempts.error_category AS errorCategory, attempts.error_code AS errorCode,
       attempts.status, attempts.usage_availability AS usageAvailability,
       attempts.total_tokens AS totalTokens,
       attempts.provider_billing_details_json AS billingDetailsJson,
       attempts.reported_cost AS reportedCost,
       attempts.estimated_cost AS estimatedCost, attempts.currency,
       attempts.pricing_revision AS pricingRevision,
       pricing.source_kind AS pricingSourceKind, pricing.source_id AS pricingSourceId,
       attempts.terminal_at_ms AS terminalAtMs
FROM provider_usage_attempts_v2 AS attempts
LEFT JOIN provider_pricing_profiles_v1 AS pricing
  ON pricing.pricing_revision = attempts.pricing_revision
ORDER BY attempts.terminal_at_ms DESC, attempts.attempt_id DESC
LIMIT 200;
]])
    for _, attempt in ipairs(recent_attempts) do
        if attempt.billingDetailsJson == nil or attempt.billingDetailsJson == vulcan.json.null then
            attempt.billingDetails = vulcan.json.array()
        else
            attempt.billingDetails = decode_json_text(
                attempt.billingDetailsJson,
                "provider_billing_details_json"
            )
        end
        attempt.billingDetailsJson = nil
    end
    local accounts = sqlite_query([[
SELECT provider_id AS providerId, connection_id AS connectionId,
       snapshot_kind AS snapshotKind,
       endpoint_profile_revision AS endpointProfileRevision,
       payload_json AS payloadJson, observed_at_ms AS observedAtMs
FROM (
    SELECT provider_id, connection_id, snapshot_kind, endpoint_profile_revision,
           payload_json, observed_at_ms,
           ROW_NUMBER() OVER (
               PARTITION BY connection_id, snapshot_kind
               ORDER BY observed_at_ms DESC, snapshot_id DESC
           ) AS rowNumber
    FROM provider_account_usage_snapshots_v1
)
WHERE rowNumber = 1
ORDER BY providerId, connectionId, snapshotKind
LIMIT 500;
]])
    for _, account in ipairs(accounts) do
        account.payload = decode_json_text(account.payloadJson, "payload_json")
        account.payloadJson = nil
    end
    local meta_rows = sqlite_query([[
SELECT CAST(strftime('%s', 'now') AS INTEGER) * 1000 AS generatedAtMs,
       COALESCE(MAX(CASE WHEN meta_key = 'schema_revision'
                         THEN CAST(meta_value AS INTEGER) END), 0) AS schemaRevision,
       COALESCE(MAX(CASE WHEN meta_key = 'aggregate_revision'
                         THEN CAST(meta_value AS INTEGER) END), 0) AS aggregateRevision,
       COALESCE(MAX(CASE WHEN meta_key = 'health_status'
                         THEN meta_value END), 'unknown') AS healthStatus
FROM provider_usage_meta;
]])
    local meta = meta_rows[1]
    if type(meta) ~= "table" then
        error("Vulcan Usage Statistics could not read system statistics metadata")
    end
    return {
        generatedAtMs = meta.generatedAtMs,
        schemaRevision = meta.schemaRevision,
        aggregateRevision = meta.aggregateRevision,
        healthStatus = meta.healthStatus,
        daily = daily,
        recentAttempts = recent_attempts,
        accounts = accounts,
    }
end

-- Load one revision-bound preview for clearing all historical statistics.
-- 加载一个绑定修订的全部历史统计清理预览。
--
-- Returns the current attempt count, known Token total, and aggregate revision.
-- 返回当前请求数、已知 Token 总量和聚合修订号。
local function load_clear_preview()
    local rows = sqlite_query([[
SELECT COUNT(*) AS attemptCount,
       COALESCE(SUM(total_tokens), 0) AS totalTokens,
       COALESCE((SELECT CAST(meta_value AS INTEGER)
                 FROM provider_usage_meta
                 WHERE meta_key = 'aggregate_revision'), 0) AS aggregateRevision
FROM provider_usage_attempts_v2;
]])
    local preview = rows[1]
    if type(preview) ~= "table" then
        error("Vulcan Usage Statistics could not preview statistics cleanup")
    end
    return preview
end

-- Load the authoritative Provider usage dashboard from system VLDB.
-- 从系统 VLDB 加载权威 Provider 用量仪表盘。
--
-- args contains the schema-validated empty action arguments.
-- args 包含通过 Schema 校验的空 Action 参数。
-- Returns the dashboard in the managed plugin ABI envelope.
-- 在受管插件 ABI 信封中返回仪表盘。
function M.dashboard_get(args)
    action_arguments(args)
    return success(load_dashboard())
end

-- Preview clearing all historical Provider statistics without mutating data.
-- 预览清理全部 Provider 历史统计，且不修改数据。
--
-- args contains the schema-validated empty action arguments.
-- args 包含通过 Schema 校验的空 Action 参数。
-- Returns a revision-bound cleanup preview.
-- 返回一个绑定修订的清理预览。
function M.statistics_clear_preview(args)
    action_arguments(args)
    return success(load_clear_preview())
end

-- Clear all historical Provider statistics at the exact preview revision.
-- 在精确预览修订上清理全部 Provider 历史统计。
--
-- args contains expectedAggregateRevision from the latest preview.
-- args 包含最近一次预览的 expectedAggregateRevision。
-- Returns the cleared counts and the cleanup revision.
-- 返回清理数量和清理修订号。
function M.statistics_clear_apply(args)
    local input = action_arguments(args)
    local expected_revision = input.expectedAggregateRevision
    if type(expected_revision) ~= "number"
        or expected_revision % 1 ~= 0
        or expected_revision < 0 then
        error("expectedAggregateRevision must be a non-negative integer")
    end
    local preview = load_clear_preview()
    if preview.aggregateRevision ~= expected_revision then
        error("System statistics changed after cleanup preview; preview again")
    end
    local revision_literal = tostring(expected_revision)
    local result = sqlite_execute([[
BEGIN IMMEDIATE;
DELETE FROM provider_rate_limit_snapshots_v1
WHERE EXISTS (
    SELECT 1 FROM provider_usage_meta
    WHERE meta_key = 'aggregate_revision'
      AND CAST(meta_value AS INTEGER) = ]] .. revision_literal .. [[
);
DELETE FROM provider_usage_attempts_v2
WHERE EXISTS (
    SELECT 1 FROM provider_usage_meta
    WHERE meta_key = 'aggregate_revision'
      AND CAST(meta_value AS INTEGER) = ]] .. revision_literal .. [[
);
DELETE FROM provider_usage_daily_v2
WHERE EXISTS (
    SELECT 1 FROM provider_usage_meta
    WHERE meta_key = 'aggregate_revision'
      AND CAST(meta_value AS INTEGER) = ]] .. revision_literal .. [[
);
DELETE FROM provider_account_usage_snapshots_v1
WHERE EXISTS (
    SELECT 1 FROM provider_usage_meta
    WHERE meta_key = 'aggregate_revision'
      AND CAST(meta_value AS INTEGER) = ]] .. revision_literal .. [[
);
UPDATE provider_usage_meta
SET meta_value = CAST(meta_value AS INTEGER) + 1,
    updated_at_ms = CAST(strftime('%s', 'now') AS INTEGER) * 1000
WHERE meta_key = 'aggregate_revision'
  AND CAST(meta_value AS INTEGER) = ]] .. revision_literal .. [[;
COMMIT;
]])
    if type(result) ~= "table" or type(result.rows_changed) ~= "number" then
        error("Vulcan Usage Statistics received an invalid VLDB cleanup result")
    end
    if result.rows_changed < 1 then
        error("System statistics changed during cleanup; preview again")
    end
    return success({
        clearedAttemptCount = preview.attemptCount,
        clearedTokenCount = preview.totalTokens,
        aggregateRevision = expected_revision + 1,
    })
end

-- Reject generic model-visible dispatch because this plugin exposes UI actions only.
-- 拒绝通用模型可见分发，因为当前插件仅暴露界面 Action。
--
-- args contains an unsupported generic invocation.
-- args 包含一条不受支持的通用调用。
-- This function always raises an explicit capability error.
-- 当前函数始终抛出明确的能力错误。
function M.dispatch(args)
    error("Vulcan Usage Statistics does not expose provider-visible plugin tools")
end

-- Close one action runtime lease without retaining plugin-owned statistics state.
-- 关闭一个 Action 运行时租约，且不保留插件自有统计状态。
--
-- args contains the host-owned lease shutdown context.
-- args 包含宿主持有的租约关闭上下文。
-- Returns one successful close acknowledgement.
-- 返回一条成功关闭确认。
function M.shutdown(args)
    return success({ closed = true })
end

return M
