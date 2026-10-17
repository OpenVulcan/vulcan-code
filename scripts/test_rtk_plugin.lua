-- Exercise the real RTK bundle against the native process-result and CLI table contracts.
-- 根据原生进程结果和 CLI 表格契约验证真实 RTK Bundle。

-- runtime_vulcan retains the real JSON codec while process execution is replaced with deterministic results.
-- runtime_vulcan 保留真实 JSON 编解码器，同时用确定性结果替换进程执行。
local runtime_vulcan = vulcan

-- Read one fixture input; path is an explicit repository file and the return value is its complete text.
-- 读取一份夹具输入；path 为明确的仓库文件路径，返回其完整文本。
local function read_text(path)
    -- handle owns the input file until its exact bytes have been read.
    -- handle 在读取完整字节前持有输入文件。
    local handle = assert(io.open(path, "rb"))
    -- text preserves the production bundle and configuration without copying their definitions into this test.
    -- text 保留生产 Bundle 和配置，避免在测试中复制其定义。
    local text = handle:read("*a")
    handle:close()
    return text
end

-- state owns isolated configuration, queued process outcomes, and inspected call options.
-- state 持有隔离配置、预设进程结果和受检查的调用选项。
local state = {
    config = runtime_vulcan.json.decode(read_text(args.defaults_path)),
    calls = {},
    rewrite = {},
    gain_failed = false,
    reset_failed = false,
    storage_failed = false,
    invalidate_failed = false,
    revision = 0,
    invalidations = {},
}

-- Serve declared host callbacks; name selects the capability, payload carries its request, and results follow that capability.
-- 提供已声明宿主回调；name 选择能力，payload 承载请求，结果遵循对应能力。
local function host_call(name, payload)
    if name == "vulcan.plugin.config.get" then
        return state.config
    end
    if name == "vulcan.plugin.storage.sqlite.migrate" then
        if state.storage_failed then
            error("Fixture storage failure")
        end
        return {}
    end
    if name == "vulcan.plugin.storage.sqlite.execute" then
        state.revision = state.revision + 1
        return {}
    end
    if name == "vulcan.plugin.storage.sqlite.query" then
        return { rows = {{ revision = state.revision }} }
    end
    assert(name == "vulcan.plugin.ui.invalidate")
    if state.invalidate_failed then
        error("Fixture notification failure")
    end
    table.insert(state.invalidations, payload)
    return { published = true }
end

-- Execute a declared fixture operation; binary and options follow the real bridge and the return value is an ExecResult.
-- 执行已声明的夹具操作；binary 与 options 遵循真实桥接层，返回值为 ExecResult。
local function process_exec(binary, options)
    assert(binary == "rtk")
    assert(options.env.NO_COLOR == "1")
    assert(options.env.RTK_SUPPRESS_HOOK_WARNING == "1")
    assert(options.env.RTK_TELEMETRY_DISABLED == (state.config.telemetryDisabled and "1" or nil))
    table.insert(state.calls, options)
    if options.args[1] == "rewrite" then
        return state.rewrite
    end
    assert(options.args[1] == "gain")
    if options.args[2] == "--reset" then
        assert(options.args[3] == "--yes" and #options.args == 3)
        return {
            code = state.reset_failed and 1 or 0,
            success = not state.reset_failed,
            timed_out = false,
            stdout = "",
            stderr = state.reset_failed and "Fixture reset failure" or "",
        }
    end
    if options.args[2] == "--all" then
        assert(options.args[3] == "--format" and options.args[4] == "json")
        if state.gain_failed then
            return { code = 1, success = false, timed_out = false, stdout = "", stderr = "fixture failure" }
        end
        return {
            code = 0,
            success = true,
            timed_out = false,
            stderr = "",
            stdout = runtime_vulcan.json.encode({
                summary = { total_commands = 2, total_saved = 50, oversized_excluded = 1 },
                daily = runtime_vulcan.json.array(),
                weekly = runtime_vulcan.json.array(),
                monthly = runtime_vulcan.json.array(),
            }),
        }
    end
    assert(options.args[2] == "--text" and #options.args == 2)
    return {
        code = 0,
        success = true,
        timed_out = false,
        stderr = "",
        stdout = "By Command\n"
            .. "  #  Command                   Count  Saved  Total%    Time  Impact\n"
            .. " 1.  rtk positive-example          2   1.5K   82.0%    25ms\n"
            .. " 2.  rtk negative-example          1      0  -50.0%    1.5s\n",
    }
end

-- environment exposes the same pure-Lua globals the production bundle consumes, plus the exact fixture callbacks.
-- environment 暴露生产 Bundle 使用的同一组纯 Lua 全局变量，以及精确夹具回调。
local environment = {
    error = error,
    ipairs = ipairs,
    pairs = pairs,
    pcall = pcall,
    tonumber = tonumber,
    tostring = tostring,
    type = type,
    string = string,
    table = table,
    vulcan = {
        json = runtime_vulcan.json,
        host = { call = host_call },
        process = { exec = process_exec },
    },
}

-- loader compiles the current production bundle; plugin retains its actual exported functions.
-- loader 编译当前生产 Bundle；plugin 保存其真实导出函数。
local loader = assert(load(read_text(args.plugin_path), "@vulcan-rtk/runtime/plugin.bundle.lua", "t", environment))
local plugin = loader()

-- Decode an actual plugin envelope; raw is serialized ABI output and the return value is its successful result.
-- 解码真实插件信封；raw 为序列化 ABI 输出，返回其中的成功结果。
local function decode_result(raw)
    -- decoded is produced by the real JSON codec rather than a fixture parser.
    -- decoded 由真实 JSON 编解码器产生，不使用夹具解析器。
    local decoded = runtime_vulcan.json.decode(raw)
    assert(decoded.ok == true)
    return decoded.result
end

-- Invoke the real shell hook; command is host-owned input and the return value is its decoded HookResult.
-- 调用真实 Shell Hook；command 为宿主持有的输入，返回解码后的 HookResult。
local function shell_hook(command)
    return decode_result(plugin.transform_shell_command({
        invocation_json = runtime_vulcan.json.encode({
            registration = { id = "vulcan-plugin/vulcan-rtk/shell-command-rewrite" },
            payload = { value = {
                command = command,
                cwd = args.workspace_root,
                environment = runtime_vulcan.json.object(),
                binary_path_requests = runtime_vulcan.json.array(),
                contributions = runtime_vulcan.json.array(),
            } },
        }),
    }))
end

-- Both rewrite statuses must preserve every command line and request only the declared RTK executable.
-- 两种改写状态都必须保留全部命令行，并且只请求已声明的 RTK 可执行文件。
for _, code in ipairs({ 0, 3 }) do
    state.rewrite = {
        code = code,
        success = code == 0,
        timed_out = false,
        stdout = "rtk git status --short\nrtk cargo check",
        stderr = "",
    }
    -- result is the actual transformed host payload for this exit status.
    -- result 是此退出状态对应的真实宿主转换载荷。
    local result = shell_hook("git status --short\ncargo check").payload.value
    assert(result.command == state.rewrite.stdout)
    assert(#result.binary_path_requests == 1)
    for _, request in ipairs(result.binary_path_requests) do
        assert(request.binary_id == "rtk")
    end
    assert(result.environment.NO_COLOR == "1")
    assert(result.environment.RTK_SUPPRESS_HOOK_WARNING == "1")
end

-- Rejected, interrupted, and failed outcomes must never execute even plausible stdout as a rewritten command.
-- 拒绝、中断和失败结果都不得将看似有效的 stdout 当作改写命令执行。
for _, outcome in ipairs({
    { code = 1, timed_out = false },
    { code = 2, timed_out = false },
    { code = 9, timed_out = false },
    { timed_out = false },
    { code = 3, timed_out = true },
    { code = 0, timed_out = false },
}) do
    outcome.success = false
    outcome.stdout = "rtk git status --short"
    outcome.stderr = "fixture process outcome"
    state.rewrite = outcome
    -- result must retain the original host command and have no executable injection request.
    -- result 必须保留原宿主命令，且不包含可执行文件注入请求。
    local result = shell_hook("git status --short").payload.value
    assert(result.command == "git status --short")
    assert(#result.binary_path_requests == 0 and #result.contributions == 0)
end

state.rewrite = { code = 0, success = true, timed_out = false, stdout = "\n  ", stderr = "" }
assert(shell_hook("git status").payload.value.command == "git status")

-- The route filter inspects later lines too, so a denied second route cannot hide behind an allowed first one.
-- 路由过滤器也检查后续行，禁止的第二条路由不能藏在允许的第一条路由之后。
state.rewrite = {
    code = 3,
    success = false,
    timed_out = false,
    stdout = "rtk git status\nrtk docker ps",
    stderr = "",
}
assert(shell_hook("git status\ndocker ps").payload.value.command == "git status\ndocker ps")

-- Disabling routing must bypass process execution without rewriting persisted configuration.
-- 禁用路由必须跳过进程执行，且不改写持久化配置。
state.config.enabled = false
-- call_count captures the process boundary before the disabled invocation.
-- call_count 保存禁用调用之前的进程边界计数。
local call_count = #state.calls
assert(shell_hook("git status").payload.value.command == "git status")
assert(#state.calls == call_count)
state.config.enabled = true

-- Telemetry remains an explicit user setting while plain output and host-integrated warning behavior stay fixed.
-- 遥测保持为显式用户设置，纯文本输出与宿主集成警告行为保持固定。
state.config.telemetryDisabled = false
state.rewrite = { code = 0, success = true, timed_out = false, stdout = "rtk git status", stderr = "" }
assert(shell_hook("git status").payload.value.environment.RTK_TELEMETRY_DISABLED == nil)

-- statistics must preserve excluded-count metadata and both positive and negative weighted command rates.
-- statistics 必须保留排除数量元数据，以及正负两种命令加权节省率。
local statistics = decode_result(plugin.statistics_get({}))
assert(statistics.available and statistics.summary.oversized_excluded == 1)
assert(statistics.summary.total_commands == 2 and statistics.summary.total_saved == 50)
assert(#statistics.byCommand == 2)
for _, row in ipairs(statistics.byCommand) do
    if row.command == "rtk positive-example" then
        assert(row.savingsPct == 82 and row.savedTokens == 1500 and row.avgTimeMs == 25)
    elseif row.command == "rtk negative-example" then
        assert(row.savingsPct == -50 and row.savedTokens == 0 and row.avgTimeMs == 1500)
    else
        error("Unexpected command statistics row")
    end
end
assert(runtime_vulcan.json.encode(statistics.daily) == "[]")

state.gain_failed = true
-- unavailable is the real structured failure result and must retain JSON arrays for empty chart/table data.
-- unavailable 为真实结构化失败结果，空图表与表格数据必须保留为 JSON 数组。
local unavailable = decode_result(plugin.statistics_get({}))
assert(unavailable.available == false)
assert(runtime_vulcan.json.encode(unavailable.byCommand) == "[]")

-- Clearing statistics must use the official mutation, retain settings, and publish durable increasing revisions.
-- 清空统计必须使用官方变更命令、保留设置，并发布持久递增的修订号。
-- original_config captures the full user configuration before any reset operation.
-- original_config 保存执行任何重置操作之前的完整用户配置。
local original_config = runtime_vulcan.json.encode(state.config)
assert(decode_result(plugin.statistics_clear({ arguments = {} })).cleared == true)
-- replacement creates a fresh bundle instance while retaining the same host-owned revision storage.
-- replacement 创建全新 Bundle 实例，同时保留同一宿主持有的修订号存储。
local replacement = assert(load(read_text(args.plugin_path), "@rtk-reloaded", "t", environment))()
assert(decode_result(replacement.statistics_clear({ arguments = {} })).cleared == true)
assert(#state.invalidations == 2)
for _, event in ipairs(state.invalidations) do
    assert(event.topic == "rtk.statistics")
end
assert(state.invalidations[#state.invalidations].revision == state.revision and state.revision == 2)
assert(runtime_vulcan.json.encode(state.config) == original_config)

-- A failed reset must report failure and must not advance or publish a statistics revision.
-- 重置失败必须报告失败，且不得递增或发布统计修订号。
state.reset_failed = true
assert(not pcall(plugin.statistics_clear, { arguments = {} }))
assert(#state.invalidations == 2 and state.revision == 2)
state.reset_failed = false

-- Storage must be available before starting the destructive CLI mutation.
-- 开始具有破坏性的 CLI 变更前，存储必须可用。
state.storage_failed = true
call_count = #state.calls
assert(not pcall(plugin.statistics_clear, { arguments = {} }))
assert(#state.calls == call_count)
state.storage_failed = false

-- A later notification failure must remain an explicit action failure rather than a false success result.
-- 后续通知失败必须保持为明确的 Action 失败，不得返回虚假成功结果。
state.invalidate_failed = true
assert(not pcall(plugin.statistics_clear, { arguments = {} }))
return { passed = true }
