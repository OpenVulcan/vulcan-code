-- Execute production page actions with a host that rejects any unrelated dashboard query.
-- 使用拒绝无关仪表盘查询的宿主执行生产分页 Action。
local runtime_json = vulcan.json
-- Read the exact production bundle without changing user configuration or databases.
-- 读取精确生产 Bundle，不修改用户配置或数据库。
local handle = assert(io.open(args.plugin_path, "rb"))
local source = handle:read("*a")
handle:close()
-- Exercise each projection independently and a concurrent revision change.
-- 独立验证每种投影，以及并发修订变化。
local results = {}
for _, view in ipairs({ "daily-page", "recent-page", "detail-pages" }) do
    -- Calls record only the admitted page queries and their source-bound offsets.
    -- calls 仅记录准入的分页查询及其源码绑定偏移量。
    local calls = {}
    local revision_reads = 0
    -- Reject unrelated SQL instead of silently returning empty overview data.
    -- 拒绝无关 SQL，而不是静默返回空的概览数据。
    -- operation/request are real hostcall arguments; returns controlled fixture rows.
    -- operation/request 为真实 Hostcall 参数；返回受控夹具记录。
    local function host_call(operation, request)
        if operation == "vulcan.plugin.config.get" then return { modelOverrides = {} } end
        assert(operation == "vulcan.vldb.sqlite.query.v1")
        local sql = request.sql
        if sql:find("AS dashboardRevision", 1, true) then
            revision_reads = revision_reads + 1
            -- Move the revision after the opening read, then hold it steady for the retry.
            -- 在首次读取版本后推进版本，随后在重试期间保持稳定。
            if args.change_revision == "once" and revision_reads >= 2 then
                return { rows = {{ dashboardRevision = 8, generatedAtMs = 123456 }} }
            end
            return { rows = {{ dashboardRevision = args.change_revision == true and revision_reads or 7, generatedAtMs = 123456 }} }
        end
        assert(request.params[3].value == 100 and request.params[4].value == 50)
        assert(request.params[9].value == "embedding")
        if sql:find("LIMIT 100 OFFSET", 1, true) then
            assert(view ~= "recent-page", "recent page must not query model usage")
            calls[#calls + 1] = "daily"
        elseif sql:find("LIMIT 50 OFFSET", 1, true) then
            assert(view ~= "daily-page", "usage page must not query recent attempts")
            calls[#calls + 1] = "recent"
        elseif sql:find("SELECT COUNT(*) AS count FROM range_usage;", 1, true) then
            assert(view ~= "recent-page")
            calls[#calls + 1] = "daily-count"
            return { rows = {{ count = 150 }} }
        elseif sql:find("SELECT COUNT(*) AS count FROM recent_model_attempts;", 1, true) then
            assert(view ~= "daily-page")
            calls[#calls + 1] = "recent-count"
            return { rows = {{ count = 75 }} }
        else
            error("Unexpected query in page projection: " .. sql)
        end
        return { rows = runtime_json.array() }
    end
    -- Isolate the plugin while reusing the runtime's real JSON implementation.
    -- 隔离插件，同时复用运行时真实 JSON 实现。
    local environment = {
        assert = assert, error = error, ipairs = ipairs, pairs = pairs, next = next,
        pcall = pcall, tonumber = tonumber, tostring = tostring, type = type,
        math = math, string = string, table = table, setmetatable = setmetatable,
        vulcan = { json = runtime_json, host = { call = host_call } },
    }
    environment._G = environment
    local plugin = assert(load(source, "@statistics-page-test", "t", environment))()
    local ok, response = pcall(plugin.dashboard_get, { arguments = {
        view = view, dailyOffset = 100, recentOffset = 50, recentModelType = "embedding",
    } })
    if args.change_revision == true then
        assert(not ok and tostring(response):find("consistent page", 1, true), tostring(response))
    else
        assert(ok, tostring(response))
        local page = runtime_json.decode(response).result
        assert(page.view == view and page.aggregateRevision == (args.change_revision == "once" and 8 or 7))
        assert(page.summary == nil and page.accounts == nil and page.heatmap == nil)
        assert(page.series == nil and page.consumerIds == nil and page.pricingCatalog == nil)
        assert((page.daily ~= nil) == (view ~= "recent-page"))
        assert((page.recentAttempts ~= nil) == (view ~= "daily-page"))
    end
    assert(revision_reads == (args.change_revision and 4 or 2))
    assert(#calls == (view == "detail-pages" and 4 or 2) * (args.change_revision and 2 or 1))
    results[view] = { calls = calls, rejected_changed_revision = args.change_revision == true }
end
return results
