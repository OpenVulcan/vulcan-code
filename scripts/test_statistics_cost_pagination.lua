-- Exercise the production exact-cost accumulator across multiple bounded host query pages.
-- 跨多个有界宿主查询分页验证生产精确费用累加器。
local runtime_vulcan = vulcan
-- overrides supplies six hundred explicit four-dimensional price contracts.
-- overrides 提供六百个明确的四维价格契约。
local overrides = {}
for index = 1, 600 do
    overrides["fixture-" .. index] = {
        inputPerMillion = "1", outputPerMillion = "2",
        cacheReadPerMillion = "0", cacheWritePerMillion = "0",
    }
end
-- pages records offsets received through the real plugin's typed parameter list.
-- pages 记录真实插件标签参数列表传来的偏移量。
local pages = {}
-- revision models the authoritative daily-aggregate revision committed by the host.
-- revision 模拟宿主提交的权威每日聚合版本。
local revision = 1
-- host_call returns exact synthetic query projections without SQL or network access.
-- host_call 返回精确合成查询投影，不访问 SQL 或网络。
-- operation and request are production hostcall values; returns the declared result shape.
-- operation 与 request 是生产 Hostcall 值；返回声明的结果形状。
local function host_call(operation, request)
    if operation == "vulcan.plugin.config.get" then return { modelOverrides = overrides } end
    assert(operation == "vulcan.vldb.sqlite.query.v1")
    local sql = request.sql
    if sql:find("AS generatedAtMs", 1, true) then
        return { rows = {{ generatedAtMs = 0, schemaRevision = 1, aggregateRevision = revision,
            healthStatus = "ready", dailyCount = 1203, recentCount = 0, consumerIdsJson = "[]" }} }
    elseif sql:find("AS dashboardRevision", 1, true) then
        return { rows = {{ dashboardRevision = revision }} }
    elseif sql:find("AS rowCount", 1, true) then
        local offset = request.params[8].value
        pages[#pages + 1] = offset
        -- A concurrent commit between cost pages must invalidate the whole mixed projection.
        -- 费用分页间的并发提交必须使整个混合投影失效。
        if offset == 1000 and (args.change_revision == true or (args.change_revision == "once" and revision == 1)) then
            revision = revision + 1
        end
        local rows = runtime_vulcan.json.array()
        for index = offset + 1, math.min(offset + 1000, 1200) do
            rows[#rows + 1] = {
                modelId = "fixture-" .. ((index - 1) % 600 + 1), rowCount = 1,
                uncachedInputTokens = 10 * revision, outputTokens = 20, cacheReadTokens = 0,
                cacheWriteTokens = index <= 600 and 0 or runtime_vulcan.json.null,
                -- Keep a known subtotal from the group with missing input and ignore missing zero-rate cache usage.
                -- 保留输入部分缺失分组的已知小计，零费率缓存用量缺失不影响覆盖判断。
                uncachedInputTokensMissing = args.partial_usage and index == 1 and 1 or 0,
                cacheWriteTokensMissing = index > 600 and 1 or 0,
            }
        end
        if offset == 1000 then
            -- Preserve a subtotal on the second page even with one missing charged dimension.
            -- 第二页有一项收费维度缺失时，仍应保留可计算的小计。
            rows[#rows + 1] = { modelId = "fixture-1", rowCount = 1,
                uncachedInputTokens = 1, outputTokens = runtime_vulcan.json.null,
                cacheReadTokens = 0, cacheWriteTokens = 0 }
            -- Known free cache usage alone cannot establish a subtotal.
            -- 仅有已知免费缓存用量时，不能据此生成小计。
            rows[#rows + 1] = { modelId = "fixture-2", rowCount = 1,
                uncachedInputTokens = runtime_vulcan.json.null, outputTokens = runtime_vulcan.json.null,
                cacheReadTokens = 0, cacheWriteTokens = 0 }
        end
        return { rows = rows }
    elseif sql:find("FROM scoped_attempts\nGROUP BY provider_id", 1, true) then
        return { rows = {{ providerId = "fixture-provider", attempts = revision }} }
    elseif sql:find("AS attempts", 1, true) then
        return { rows = {{ attempts = 1203, successes = 1203, reported = 1202, totalTokens = 36000,
            inputTokens = 12000, outputTokens = 24000, cacheReadTokens = 0, cacheWriteTokens = 0 }} }
    end
    return { rows = runtime_vulcan.json.array() }
end
-- environment exposes the controlled JSON implementation and synthetic host only.
-- environment 仅暴露受控 JSON 实现与合成宿主。
local environment = {
    assert = assert, error = error, ipairs = ipairs, pairs = pairs, next = next,
    pcall = pcall, tonumber = tonumber, tostring = tostring, type = type,
    math = math, string = string, table = table, setmetatable = setmetatable,
    vulcan = { json = runtime_vulcan.json, host = { call = host_call } },
}
environment._G = environment
-- handle reads the production bundle supplied by the controlled test runner.
-- handle 读取受控测试运行器传入的生产 Bundle。
local handle = assert(io.open(args.plugin_path, "rb"))
local source = handle:read("*a")
handle:close()
local plugin = assert(load(source, "@statistics-cost-test", "t", environment))()
-- Execute the actual bundle so a stale projection cannot pass through a handwritten accumulator.
-- 执行实际 Bundle，避免手写累加器掩盖过期投影。
local ok, response = pcall(plugin.dashboard_get, { arguments = { view = args.view or "overview", rangeDays = 30 } })
if args.change_revision == true then
    assert(not ok, "continued revision changes must reject the mixed dashboard")
    assert(#pages == 4 and pages[1] == 0 and pages[2] == 1000 and pages[3] == 0 and pages[4] == 1000)
    return { rejected_mixed_revision = true, pages = pages }
end
assert(ok, tostring(response))
local result = runtime_vulcan.json.decode(response).result
local cost = result.summary.costs[1]
if args.change_revision == "once" then
    assert(result.aggregateRevision == 2 and cost.amount == "0.072001")
    assert(#pages == 4 and pages[1] == 0 and pages[2] == 1000 and pages[3] == 0 and pages[4] == 1000)
else
    assert(result.aggregateRevision == 1 and cost.amount == "0.060001")
    assert(#pages == 2 and pages[1] == 0 and pages[2] == 1000)
end
if args.view == "providers" then
    assert(result.providers[1].attempts == result.aggregateRevision)
end
assert(cost.pricedRows == 1201)
assert(cost.incompleteRows == 1)
assert(cost.unpricedRows == 1)
assert(cost.hasPartialUsage == true)
return { amount = cost.amount, priced_rows = cost.pricedRows,
    incomplete_rows = cost.incompleteRows, unpriced_rows = cost.unpricedRows, pages = pages }
