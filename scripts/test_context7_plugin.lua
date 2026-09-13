-- Deterministic Context7 plugin bundle harness with optional live API smoke coverage.
-- 带可选在线 API 冒烟覆盖的确定性 Context7 插件 Bundle 测试夹具。

-- runtime_vulcan retains the controlled runtime JSON implementation before the plugin sandbox is built.
-- runtime_vulcan 在构建插件沙箱前保留受控运行时 JSON 实现。
local runtime_vulcan = vulcan

-- state is the exact in-memory projection of the host services used by this bundle test.
-- state 是当前 Bundle 测试使用的宿主服务精确内存投影。
local state = {
    config = {
        defaultFastMode = false,
        requestTimeoutMs = 30000,
        retentionDays = 90,
    },
    credential = nil,
    meta = {},
    quota = nil,
    events = {},
    clock = 1784030000000,
    last_revision = 0,
    migration_version = 0,
    host_calls = {},
    -- fail_usage injects a query-policy failure without accessing any real database.
    -- fail_usage 注入查询策略失败，不访问任何真实数据库。
    fail_usage = false,
}

-- Advance the deterministic fixture clock by one millisecond.
-- 把确定性测试时钟推进一毫秒。
--
-- Returns the new synthetic Unix millisecond value.
-- 返回新的合成 Unix 毫秒值。
local function tick()
    state.clock = state.clock + 1
    return state.clock
end

-- Decode one tagged SQLite parameter into its exact Lua scalar.
-- 把一个带标签的 SQLite 参数解码为精确 Lua 标量。
--
-- parameter is one host protocol object emitted by the plugin.
-- parameter 是插件发出的一个宿主协议对象。
-- Returns nil for SQL null or the exact typed value.
-- 对 SQL 空值返回 nil，否则返回精确类型值。
local function sql_value(parameter)
    if parameter.kind == "null" then
        return nil
    end
    return parameter.value
end

-- Validate one SQLite request against the host's strict tagged-parameter wire contract.
-- 根据宿主严格的标签参数线路契约校验一次 SQLite 请求。
--
-- request is the exact plugin-emitted execute or query payload.
-- request 是插件发出的精确 Execute 或 Query 载荷。
-- Returns after SQL and every JSON-array parameter have been validated.
-- 在 SQL 与每个 JSON 数组参数均通过校验后返回。
local function validate_sqlite_request(request)
    assert(type(request) == "table")
    assert(type(request.sql) == "string" and request.sql ~= "")
    assert(type(request.params) == "table")
    assert(runtime_vulcan.json.encode(request.params):sub(1, 1) == "[")
    for _, parameter in ipairs(request.params) do
        assert(type(parameter) == "table")
        assert(parameter.type == nil)
        assert(parameter.kind == "string" or parameter.kind == "int64" or parameter.kind == "null")
        if parameter.kind == "null" then
            assert(parameter.value == nil)
        else
            assert(parameter.value ~= nil)
        end
    end
end

-- Copy one credential row while preserving absent encrypted columns as nil.
-- 复制一条凭据记录，同时把缺失加密列保持为 nil。
--
-- Returns one host-query-shaped credential row.
-- 返回一个符合宿主查询形状的凭据行。
local function credential_projection()
    return {
        wrappingKeyHex = state.credential.wrappingKeyHex,
        apiKeyNonceHex = state.credential.apiKeyNonceHex,
        apiKeyCiphertextHex = state.credential.apiKeyCiphertextHex,
        apiKeyTagHex = state.credential.apiKeyTagHex,
        credentialGeneration = state.credential.credentialGeneration,
        updatedAtMs = state.credential.updatedAtMs,
    }
end

-- Execute one source-defined plugin SQL write against the deterministic in-memory host projection.
-- 针对确定性内存宿主投影执行一条源码定义的插件 SQL 写入。
--
-- request contains the exact SQL string and tagged parameters.
-- request 包含精确 SQL 字符串与标签参数。
-- Returns one minimal host execution result.
-- 返回一个最小宿主执行结果。
local function storage_execute(request)
    validate_sqlite_request(request)
    -- sql is used only for exact statement-family routing in this fixture.
    -- sql 仅用于当前夹具内的精确语句族路由。
    local sql = request.sql
    -- params is the exact ordered tagged parameter array.
    -- params 是精确有序的标签参数数组。
    local params = request.params or {}
    if sql:find("CREATE TABLE", 1, true) or sql:find("CREATE INDEX", 1, true) then
        return { changes = 0 }
    elseif sql:find("INSERT OR IGNORE INTO context7_credential", 1, true) then
        if state.credential == nil then
            state.credential = {
                wrappingKeyHex = sql_value(params[1]),
                credentialGeneration = 0,
                updatedAtMs = tick(),
            }
            return { changes = 1 }
        end
        return { changes = 0 }
    elseif sql:find("INSERT OR IGNORE INTO context7_meta", 1, true) then
        -- key and value are the exact initialized metadata pair.
        -- key 与 value 是精确初始化元数据键值对。
        local key = sql_value(params[1])
        local value = sql_value(params[2])
        if state.meta[key] == nil then state.meta[key] = value end
        return { changes = 1 }
    elseif sql:find("api_key_nonce_hex = ?", 1, true) then
        state.credential.apiKeyNonceHex = sql_value(params[1])
        state.credential.apiKeyCiphertextHex = sql_value(params[2])
        state.credential.apiKeyTagHex = sql_value(params[3])
        state.credential.credentialGeneration = state.credential.credentialGeneration + 1
        state.credential.updatedAtMs = tick()
        return { changes = 1 }
    elseif sql:find("api_key_nonce_hex = NULL", 1, true) then
        state.credential.apiKeyNonceHex = nil
        state.credential.apiKeyCiphertextHex = nil
        state.credential.apiKeyTagHex = nil
        state.credential.credentialGeneration = state.credential.credentialGeneration + 1
        state.credential.updatedAtMs = tick()
        return { changes = 1 }
    elseif sql:find("WHERE meta_key IN ('last_credential_validation_status', 'last_credential_validation_at_ms')", 1, true) then
        -- validation_status is the exact value applied atomically to both metadata projections.
        -- validation_status 是原子应用到两项元数据投影的精确值。
        local validation_status = sql_value(params[1])
        assert(validation_status == sql_value(params[2]))
        state.meta.last_credential_validation_status = validation_status
        state.meta.last_credential_validation_at_ms = (validation_status == "valid" or validation_status == "invalid")
                and tostring(tick())
            or ""
        return { changes = 2 }
    elseif sql:find("meta_key = 'data_revision'", 1, true) then
        state.meta.data_revision = tostring((tonumber(state.meta.data_revision) or 0) + 1)
        return { changes = 1 }
    elseif sql:find("INSERT INTO context7_quota_snapshot", 1, true) then
        state.quota = {
            authMode = sql_value(params[1]),
            credentialGeneration = sql_value(params[2]),
            quotaTier = sql_value(params[3]),
            quotaLimit = sql_value(params[4]),
            quotaRemaining = sql_value(params[5]),
            quotaResetUnixSec = sql_value(params[6]),
            retryAfterSec = sql_value(params[7]),
            httpStatus = sql_value(params[8]),
            observedAtMs = tick(),
        }
        return { changes = 1 }
    elseif sql:find("INSERT INTO context7_request_events", 1, true) then
        state.events[#state.events + 1] = {
            requestId = sql_value(params[1]),
            operation = sql_value(params[2]),
            authMode = sql_value(params[3]),
            credentialGeneration = sql_value(params[4]),
            status = sql_value(params[5]),
            httpStatus = sql_value(params[6]),
            errorCategory = sql_value(params[7]),
            durationMs = sql_value(params[8]),
            createdAtMs = tick(),
        }
        return { changes = 1 }
    elseif sql:find("DELETE FROM context7_request_events", 1, true)
        or sql:find("last_cleanup_at_ms", 1, true)
        or sql:find("last_success_at_ms", 1, true)
    then
        return { changes = 0 }
    end
    error("Unhandled Context7 fixture SQL write")
end

-- Execute one source-defined plugin SQL read against the deterministic in-memory host projection.
-- 针对确定性内存宿主投影执行一条源码定义的插件 SQL 读取。
--
-- request contains the exact SQL string and tagged parameters.
-- request 包含精确 SQL 字符串与标签参数。
-- Returns one host query result containing ordered rows.
-- 返回一个包含有序行的宿主查询结果。
local function storage_query(request)
    validate_sqlite_request(request)
    -- sql is used only for exact statement-family routing in this fixture.
    -- sql 仅用于当前夹具内的精确语句族路由。
    local sql = request.sql
    -- params is the exact ordered tagged parameter array.
    -- params 是精确有序的标签参数数组。
    local params = request.params or {}
    if sql:find("SELECT singleton_id FROM context7_credential", 1, true) then
        return { rows = state.credential == nil and {} or { { singleton_id = 1 } } }
    elseif sql:find("AS validationStatus", 1, true) then
        -- projection combines the encrypted singleton with exact validation metadata in one row.
        -- projection 在一行中合并加密单例与精确验证元数据。
        local projection = credential_projection()
        projection.validationStatus = state.meta.last_credential_validation_status
        projection.validatedAtText = state.meta.last_credential_validation_at_ms
        return { rows = { projection } }
    elseif sql:find("wrapping_key_hex AS wrappingKeyHex", 1, true) then
        return { rows = { credential_projection() } }
    elseif sql:find("SELECT meta_value AS value", 1, true) then
        return { rows = { { value = state.meta[sql_value(params[1])] } } }
    elseif sql:find("COUNT(*) AS requests", 1, true) then
        if state.fail_usage then error("Plugin SQLite statement cannot use END.") end
        -- aggregate contains the exact bounded local event totals.
        -- aggregate 包含精确有界本地事件总计。
        local aggregate = {
            requests = #state.events,
            successes = 0,
            failures = 0,
            averageDurationMs = 0,
            anonymousRequests = 0,
            apiKeyRequests = 0,
            resolveRequests = 0,
            queryRequests = 0,
        }
        -- duration_total accumulates exact event durations for the average.
        -- duration_total 累积用于平均值的精确事件时长。
        local duration_total = 0
        for _, event in ipairs(state.events) do
            aggregate.successes = aggregate.successes + (event.status == "success" and 1 or 0)
            aggregate.failures = aggregate.failures + (event.status == "error" and 1 or 0)
            aggregate.anonymousRequests = aggregate.anonymousRequests + (event.authMode == "anonymous" and 1 or 0)
            aggregate.apiKeyRequests = aggregate.apiKeyRequests + (event.authMode == "api_key" and 1 or 0)
            aggregate.resolveRequests = aggregate.resolveRequests + (event.operation == "resolve_library_id" and 1 or 0)
            aggregate.queryRequests = aggregate.queryRequests + (event.operation == "query_docs" and 1 or 0)
            duration_total = duration_total + event.durationMs
        end
        aggregate.averageDurationMs = #state.events == 0 and 0 or duration_total / #state.events
        if sql:find("WITH usage_stats AS", 1, true) then
            aggregate.revisionText = state.meta.data_revision
            if state.quota ~= nil
                and state.quota.authMode == sql_value(params[2])
                and state.quota.credentialGeneration == sql_value(params[3])
            then
                aggregate.quotaAuthMode = state.quota.authMode
                aggregate.quotaCredentialGeneration = state.quota.credentialGeneration
                aggregate.quotaTier = state.quota.quotaTier
                aggregate.quotaLimit = state.quota.quotaLimit
                aggregate.quotaRemaining = state.quota.quotaRemaining
                aggregate.quotaResetUnixSec = state.quota.quotaResetUnixSec
                aggregate.retryAfterSec = state.quota.retryAfterSec
                aggregate.quotaHttpStatus = state.quota.httpStatus
                aggregate.quotaObservedAtMs = state.quota.observedAtMs
            end
        end
        return { rows = { aggregate } }
    elseif sql:find("FROM context7_quota_snapshot", 1, true) then
        if state.quota ~= nil
            and state.quota.authMode == sql_value(params[1])
            and state.quota.credentialGeneration == sql_value(params[2])
        then
            return { rows = { state.quota } }
        end
        return { rows = {} }
    end
    error("Unhandled Context7 fixture SQL query")
end

-- Apply one strict Context7 migration payload through the fixture's durable schema projection.
-- 通过夹具的持久 Schema 投影应用一个严格 Context7 迁移载荷。
--
-- request is the exact host migration payload emitted by the plugin.
-- request 是插件发出的精确宿主迁移载荷。
-- Returns one minimal migration result after initializing source-declared metadata.
-- 在初始化源码声明的元数据后返回一个最小迁移结果。
local function storage_migrate(request)
    assert(type(request) == "table")
    assert(request.version == 1)
    assert(request.plugin_version == "0.1.0")
    assert(request.applied_at_ms == 0)
    assert(type(request.statements) == "table")
    assert(runtime_vulcan.json.encode(request.statements):sub(1, 1) == "[")
    assert(#request.statements == 6)
    -- migration_sql proves the single host call owns every required table and metadata initializer.
    -- migration_sql 证明单次宿主调用包含全部必需表与元数据初始化器。
    local migration_sql = table.concat(request.statements, "\n")
    for _, required_text in ipairs({
        "context7_credential",
        "context7_request_events",
        "context7_quota_snapshot",
        "context7_meta",
        "last_credential_validation_status",
    }) do
        assert(migration_sql:find(required_text, 1, true) ~= nil)
    end
    if state.migration_version < request.version then
        state.meta.data_revision = state.meta.data_revision or "0"
        state.meta.last_cleanup_at_ms = state.meta.last_cleanup_at_ms or "0"
        state.meta.last_success_at_ms = state.meta.last_success_at_ms or "0"
        state.meta.last_credential_validation_status = state.meta.last_credential_validation_status or "unknown"
        state.meta.last_credential_validation_at_ms = state.meta.last_credential_validation_at_ms or ""
        state.migration_version = request.version
    end
    return { applied = true, version = state.migration_version }
end

-- Dispatch one fixed plugin host service call used by the real bundle.
-- 分发真实 Bundle 使用的一次固定插件宿主服务调用。
--
-- tool_name is the exact bridge tool and payload is its plugin-owned request.
-- tool_name 是精确桥接工具，payload 是其插件自有请求。
-- Returns the deterministic host service result.
-- 返回确定性宿主服务结果。
local function host_call(tool_name, payload)
    state.host_calls[tool_name] = (state.host_calls[tool_name] or 0) + 1
    if tool_name == "vulcan.plugin.storage.sqlite.execute" then
        return storage_execute(payload)
    elseif tool_name == "vulcan.plugin.storage.sqlite.query" then
        return storage_query(payload)
    elseif tool_name == "vulcan.plugin.storage.sqlite.migrate" then
        return storage_migrate(payload)
    elseif tool_name == "vulcan.plugin.storage.sqlite.flush" then
        return { flushed = true }
    elseif tool_name == "vulcan.plugin.config.get" then
        return state.config
    elseif tool_name == "vulcan.plugin.config.update" then
        state.config = payload.value
        return state.config
    elseif tool_name == "vulcan.plugin.ui.invalidate" then
        state.last_revision = payload.revision
        return { published = true }
    end
    error("Unhandled Context7 fixture host call: " .. tostring(tool_name))
end

-- allowed_modules is the exact test equivalent of the manifest-validated runtime module table.
-- allowed_modules 是经过 Manifest 校验的运行时模块表的精确测试等价物。
local allowed_modules = {
    ["lcurl.safe"] = require("lcurl.safe"),
    ["openssl.cipher"] = require("openssl.cipher"),
    ["openssl.rand"] = require("openssl.rand"),
}

-- plugin_vulcan is the only host surface visible to the loaded Context7 bundle.
-- plugin_vulcan 是加载后的 Context7 Bundle 可见的唯一宿主表面。
local plugin_vulcan = {
    host = { call = host_call },
    json = {
        encode = runtime_vulcan.json.encode,
        decode = runtime_vulcan.json.decode,
        object = runtime_vulcan.json.object,
        array = runtime_vulcan.json.array,
        null = runtime_vulcan.json.null,
    },
    runtime = {
        -- Return one exact allowlisted module and reject undeclared names.
        -- 返回一个精确白名单模块，并拒绝未声明名称。
        module = function(module_name)
            if allowed_modules[module_name] == nil then
                error("Undeclared fixture module: " .. tostring(module_name))
            end
            return allowed_modules[module_name]
        end,
        -- Report the current controlled runtime platform for native CA behavior.
        -- 报告当前受控运行时平台，以便选择原生 CA 行为。
        is_windows = function()
            return package.config:sub(1, 1) == "\\"
        end,
    },
}

-- handle reads the real repository bundle under test.
-- handle 读取待测试的真实仓库 Bundle。
local handle = assert(io.open(args.plugin_path, "rb"))

-- source is the immutable real plugin source.
-- source 是不可变真实插件源码。
local source = handle:read("*a")
handle:close()

-- environment is the restricted pure-Lua surface used for real bundle execution.
-- environment 是真实 Bundle 执行使用的受限纯 Lua 表面。
local environment = {
    assert = assert,
    error = error,
    ipairs = ipairs,
    next = next,
    pairs = pairs,
    pcall = pcall,
    rawequal = rawequal,
    rawget = rawget,
    rawlen = rawlen,
    rawset = rawset,
    select = select,
    setmetatable = setmetatable,
    getmetatable = getmetatable,
    tonumber = tonumber,
    tostring = tostring,
    type = type,
    xpcall = xpcall,
    coroutine = coroutine,
    math = math,
    string = string,
    table = table,
    utf8 = utf8,
    vulcan = plugin_vulcan,
}
environment._G = environment

-- loader compiles the real bundle in the restricted fixture environment.
-- loader 在受限测试环境内编译真实 Bundle。
local loader = assert(load(source, "@context7/runtime/plugin.bundle.lua", "t", environment))

-- plugin is the exact real export table returned by the bundle.
-- plugin 是 Bundle 返回的精确真实导出表。
local plugin = loader()
assert(type(plugin) == "table")

-- decode_result unwraps one successful fixed-bridge response envelope.
-- decode_result 解包一个成功的固定桥响应信封。
--
-- raw is one serialized plugin return value.
-- raw 是一个序列化插件返回值。
-- Returns the exact business result after asserting success.
-- 在断言成功后返回精确业务结果。
local function decode_result(raw)
    -- decoded is the exact JSON response envelope.
    -- decoded 是精确 JSON 响应信封。
    local decoded = runtime_vulcan.json.decode(raw)
    assert(decoded.ok == true)
    return decoded.result
end

-- tool_frame serializes one Provider payload exactly as the fixed Vulcan Plugin bridge does.
-- tool_frame 按固定 Vulcan Plugin 桥的真实方式序列化一个 Provider 载荷。
--
-- arguments is the schema-valid business object before host metadata is injected.
-- arguments 是宿主元数据注入前经过 Schema 校验的业务对象。
-- Returns the exact frame received by plugin.dispatch.
-- 返回 plugin.dispatch 收到的精确帧结构。
local function tool_frame(arguments)
    -- request_id and host_timeout_ms match the metadata inserted before runtime dispatch serialization.
    -- request_id 与 host_timeout_ms 对齐运行时分发序列化前注入的元数据。
    arguments.request_id = "context7-test-request"
    arguments.host_timeout_ms = 65000
    return {
        payload_json = runtime_vulcan.json.encode(arguments),
        host_timeout_ms = 65000,
    }
end

-- malformed_payload_ok proves dispatch rejects a bridge frame whose JSON cannot be decoded.
-- malformed_payload_ok 验证 dispatch 会拒绝 JSON 无法解码的固定桥帧。
local malformed_payload_ok, malformed_payload_error = pcall(plugin.dispatch, {
    payload_json = "{",
    host_timeout_ms = 65000,
})
assert(malformed_payload_ok == false)
assert(tostring(malformed_payload_error):find("payload_json must decode to an object", 1, true) ~= nil)

-- bootstrap proves a cold settings-page load uses one migration and a bounded number of host round trips.
-- bootstrap 证明设置页冷启动只使用一次迁移与有界数量的宿主往返。
local bootstrap = decode_result(plugin.bootstrap_get({ arguments = { rangeDays = 30 } }))
assert(bootstrap.settings.requestTimeoutMs == 30000)
assert(bootstrap.credential.mode == "anonymous")
assert(bootstrap.usage.rangeDays == 30)
assert(state.host_calls["vulcan.plugin.storage.sqlite.migrate"] == 1)
assert(state.host_calls["vulcan.plugin.storage.sqlite.query"] == 3)
assert(state.host_calls["vulcan.plugin.storage.sqlite.execute"] == 1)
assert(state.host_calls["vulcan.plugin.config.get"] == 1)

decode_result(plugin.on_enable({}))
assert(state.host_calls["vulcan.plugin.storage.sqlite.migrate"] == 1)

-- initial_status proves the plugin starts in anonymous mode.
-- initial_status 证明插件以匿名模式启动。
local initial_status = decode_result(plugin.credential_status({ arguments = {} }))
assert(initial_status.configured == false)
assert(initial_status.mode == "anonymous")

-- test_key is a disposable fixture secret that must never enter host state or action output.
-- test_key 是一个一次性测试密钥，绝不能进入宿主状态或 Action 输出。
local test_key = "ctx7sk-" .. "local-fixture-only"

-- configured_status proves encryption and the non-secret projection complete together.
-- configured_status 证明加密与非密钥投影同步完成。
local configured_status = decode_result(plugin.credential_set({ arguments = { apiKey = test_key } }))
assert(configured_status.configured == true)
assert(configured_status.mode == "api_key")
assert(configured_status.validationStatus == "untested")
assert(state.credential.apiKeyCiphertextHex ~= test_key)
assert(state.credential.wrappingKeyHex ~= test_key)
assert(runtime_vulcan.json.encode(configured_status):find(test_key, 1, true) == nil)
assert(runtime_vulcan.json.encode(state.credential):find(test_key, 1, true) == nil)

-- original_ciphertext preserves the valid envelope while the tamper check runs.
-- original_ciphertext 在篡改检查期间保留有效信封。
local original_ciphertext = state.credential.apiKeyCiphertextHex

-- replacement_nibble deterministically changes the first ciphertext nibble.
-- replacement_nibble 确定性修改密文的首个半字节。
local replacement_nibble = original_ciphertext:sub(1, 1) == "0" and "1" or "0"
state.credential.apiKeyCiphertextHex = replacement_nibble .. original_ciphertext:sub(2)

-- tamper_blocked proves authenticated decryption fails before any network event is recorded.
-- tamper_blocked 证明认证解密会在记录任何网络事件前失败。
local tamper_blocked, tamper_error = pcall(plugin.connection_test, { arguments = {} })
assert(tamper_blocked == false)
assert(tostring(tamper_error):find("credential is corrupted", 1, true) ~= nil)
assert(#state.events == 0)
state.credential.apiKeyCiphertextHex = original_ciphertext

-- deleted_status proves deletion clears only the encrypted envelope and advances generation.
-- deleted_status 证明删除仅清空加密信封并推进代次。
local deleted_status = decode_result(plugin.credential_delete({ arguments = {} }))
assert(deleted_status.configured == false)
assert(deleted_status.mode == "anonymous")
assert(deleted_status.credentialGeneration == 2)
assert(state.credential.apiKeyCiphertextHex == nil)

-- updated_settings proves the host update payload retains the exact value envelope contract.
-- updated_settings 证明宿主更新载荷保持精确 value 信封契约。
local updated_settings = decode_result(plugin.settings_update({
    arguments = {
        defaultFastMode = true,
        requestTimeoutMs = 10000,
        retentionDays = 30,
    },
}))
assert(updated_settings.defaultFastMode == true)
assert(state.config.requestTimeoutMs == 10000)

if args.online == true then
    -- search_markdown is the live anonymous resolve result rendered through the real plugin.
    -- search_markdown 是通过真实插件渲染的在线匿名解析结果。
    local search_markdown = decode_result(plugin.dispatch(tool_frame({
        op = "resolve_library_id",
        libraryName = "react",
        query = "React effect cleanup",
        fast = true,
    })))
    assert(search_markdown:find("Context7 library matches", 1, true) ~= nil)
    assert(search_markdown:find("SECURITY BOUNDARY", 1, true) ~= nil)
    -- docs_markdown is the live anonymous documentation result rendered through the real plugin.
    -- docs_markdown 是通过真实插件渲染的在线匿名文档结果。
    local docs_markdown = decode_result(plugin.dispatch(tool_frame({
        op = "query_docs",
        libraryId = "/react/react",
        query = "useEffect cleanup",
        fast = true,
    })))
    assert(docs_markdown:find("Context7 documentation", 1, true) ~= nil)
    assert(docs_markdown:find("SECURITY BOUNDARY", 1, true) ~= nil)
    assert(#state.events == 2)
    assert(state.events[1].authMode == "anonymous")
    assert(state.events[2].authMode == "anonymous")
    assert(state.quota ~= nil)
    -- invalid_key_status configures one disposable key before the explicit rejection probe.
    -- invalid_key_status 在显式拒绝探测前配置一个一次性无效 Key。
    local invalid_key_status = decode_result(plugin.credential_set({
        arguments = { apiKey = "ctx7sk-" .. "local-invalid-request" },
    }))
    assert(invalid_key_status.mode == "api_key")
    -- rejected and rejection_error prove HTTP 401 is surfaced without anonymous fallback.
    -- rejected 与 rejection_error 证明 HTTP 401 会显式返回，且不会匿名降级。
    local rejected, rejection_error = pcall(plugin.connection_test, { arguments = {} })
    assert(rejected == false)
    assert(tostring(rejection_error):find("credential_rejected", 1, true) ~= nil)
    assert(#state.events == 3)
    assert(state.events[3].authMode == "api_key")
    assert(state.events[3].httpStatus == 401)
    -- rejected_status proves the current generation remains configured and visibly invalid.
    -- rejected_status 证明当前代次仍保持配置状态并明确标记为无效。
    local rejected_status = decode_result(plugin.credential_status({ arguments = {} }))
    assert(rejected_status.configured == true)
    assert(rejected_status.validationStatus == "invalid")
    assert(type(rejected_status.validatedAtMs) == "number")
    decode_result(plugin.credential_delete({ arguments = {} }))
end

-- usage proves the real UI action schema projection remains non-secret after all fixture operations.
-- usage 证明真实 UI Action Schema 投影在全部测试操作后仍不含密钥。
local usage = decode_result(plugin.usage_get({ arguments = { rangeDays = 30 } }))
assert(usage.credential.configured == false)
assert(usage.localStats.requests == #state.events)

-- partial_bootstrap proves statistics failure preserves configuration and non-secret credentials.
-- partial_bootstrap 证明统计失败时仍保留配置和非密钥凭据。
state.fail_usage = true
local partial_bootstrap = decode_result(plugin.bootstrap_get({ arguments = { rangeDays = 30 } }))
assert(partial_bootstrap.settings.requestTimeoutMs == 10000)
assert(partial_bootstrap.credential.configured == false)
assert(partial_bootstrap.usage == runtime_vulcan.json.null)
assert(partial_bootstrap.usageError:find("local usage and quota [statement_policy]", 1, true))
assert(not partial_bootstrap.usageError:find("SELECT", 1, true))
state.fail_usage = false

return {
    compiled = true,
    partial_bootstrap = true,
    encrypted_credential_round_trip = true,
    anonymous_online_smoke = args.online == true,
    request_events = #state.events,
    revision = state.last_revision,
}
