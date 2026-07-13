-- Fixed execution bridge between LuaSkills leases and Vulcan-managed plugin bundles.
-- LuaSkills 租约与 Vulcan 托管插件 bundle 之间的固定执行桥。

local M = {}
local loaded_plugin = nil
local loaded_plugin_id = nil
-- active_workspace_root is host-controlled and may change only around one serialized bridge invocation.
-- active_workspace_root 由宿主控制，只能在单次串行桥接调用期间发生变化。
local active_workspace_root = nil
-- Capture the trusted LuaSkills API before constructing any restricted plugin environment.
-- 在构建任何受限插件环境前捕获可信 LuaSkills API。
local host_vulcan = vulcan

-- Build the restricted global environment used by one real plugin bundle.
-- 构建单个真实插件 bundle 使用的受限全局环境。
--
-- plugin_id, runtime_id, and lease_scope are immutable values bound by the Vulcan host to the current lease.
-- plugin_id、runtime_id 与 lease_scope 是 Vulcan 宿主绑定到当前租约的不可变值。
-- Returns an environment exposing standard pure Lua helpers and an identity-injecting host call.
-- 返回一个环境，其中暴露标准纯 Lua 辅助能力及注入身份的宿主调用。
local function build_plugin_environment(plugin_id, runtime_id, lease_scope)
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
    }
    -- host_call closes over the trusted bridge environment and cannot be replaced by plugin input.
    -- host_call 闭包捕获可信桥接环境，不能被插件输入替换。
    local function host_call(tool_name, payload)
        if type(tool_name) ~= "string" or tool_name == "" then
            error("Vulcan plugin host tool name must be a non-empty string")
        end
        if payload == nil then
            payload = {}
        end
        if type(payload) ~= "table" then
            error("Vulcan plugin host tool payload must be a table")
        end
        -- response is the fixed LuaSkills host-call envelope, never the tool value itself.
        -- response 是固定 LuaSkills 宿主调用信封，绝不是工具值本身。
        local response = host_vulcan.host.call(tool_name, {
            plugin_id = plugin_id,
            runtime_id = runtime_id,
            lease_scope = lease_scope,
            workspace_root = active_workspace_root,
            payload = payload,
        })
        if type(response) ~= "table" or response.ok ~= true then
            -- message preserves the explicit host diagnostic without exposing envelope internals.
            -- message 保留明确的宿主诊断，同时不暴露信封内部结构。
            local message = "Vulcan plugin host tool call failed"
            if type(response) == "table" and type(response.error) == "table" and response.error.message ~= nil then
                message = tostring(response.error.message)
            end
            error(message)
        end
        return response.value
    end
    -- resolve_process closes over plugin identity and never accepts a program or cwd from the bundle.
    -- resolve_process 闭包捕获插件身份，且绝不接受 bundle 提供的 program 或 cwd。
    local function resolve_process(mode, entry_id, options)
        if type(entry_id) ~= "string" or entry_id == "" then
            error("Vulcan plugin process entry_id must be a non-empty string")
        end
        if options == nil then
            options = {}
        end
        if type(options) ~= "table" then
            error("Vulcan plugin process options must be a table")
        end
        return host_call("vulcan.plugin.process.resolve", {
            mode = mode,
            entry_id = entry_id,
            options = options,
        })
    end
    -- exec_process resolves one logical entry and starts a bounded one-shot process.
    -- exec_process 解析一个逻辑入口并启动有界的一次性进程。
    local function exec_process(entry_id, options)
        return host_vulcan.process.exec(resolve_process("exec", entry_id, options))
    end
    -- open_process_session resolves one logical entry and starts one interactive process session.
    -- open_process_session 解析一个逻辑入口并启动一个交互式进程会话。
    local function open_process_session(entry_id, options)
        return host_vulcan.process.session.open(resolve_process("session", entry_id, options))
    end
    environment.vulcan = {
        host = {
            call = host_call,
        },
        process = {
            exec = exec_process,
            session = {
                open = open_process_session,
            },
        },
        json = {
            -- Encode one JSON-safe Lua value with the trusted LuaSkills implementation.
            -- 使用可信 LuaSkills 实现编码一个 JSON 安全的 Lua 值。
            encode = function(value)
                return host_vulcan.json.encode(value)
            end,
            -- Decode one JSON string with the trusted LuaSkills implementation.
            -- 使用可信 LuaSkills 实现解码一个 JSON 字符串。
            decode = function(value)
                return host_vulcan.json.decode(value)
            end,
        },
    }
    environment._G = environment
    return environment
end

-- Require one non-empty string field from a host-owned bridge request.
-- 从宿主持有的桥接请求中取得一个非空字符串字段。
--
-- args is the exact host-owned bridge request and field_name identifies the required field.
-- args 是精确的宿主持有桥接请求，field_name 标识必填字段。
-- Returns the validated string value.
-- 返回已校验的字符串值。
local function require_string(args, field_name)
    local value = args[field_name]
    if type(value) ~= "string" or value == "" then
        error("Vulcan plugin bridge requires non-empty " .. field_name)
    end
    return value
end

-- Validate one plugin function name without accepting path or expression syntax.
-- 校验单个插件函数名，不接受路径或表达式语法。
--
-- function_name is the exact manifest function identifier.
-- function_name 是精确的清单函数标识。
-- Returns the validated identifier.
-- 返回已校验的标识。
local function require_function_name(function_name)
    if type(function_name) ~= "string" or string.match(function_name, "^[A-Za-z_][A-Za-z0-9_]*$") == nil then
        error("Vulcan plugin bridge received an invalid function name")
    end
    return function_name
end

-- Validate that one declared plugin function exists on the loaded bundle table.
-- 校验单个已声明插件函数存在于已加载 bundle 表中。
--
-- plugin is the loaded bundle table and function_name is the manifest declaration.
-- plugin 是已加载 bundle 表，function_name 是清单声明。
-- Returns no value and raises an explicit error when the function is missing.
-- 不返回值，函数缺失时抛出显式错误。
local function require_plugin_function(plugin, function_name)
    local stable_name = require_function_name(function_name)
    if type(plugin[stable_name]) ~= "function" then
        error("Vulcan plugin bundle does not export function " .. stable_name)
    end
end

-- Load one host-validated plugin bundle into the current isolated lease.
-- 把一个经过宿主校验的插件 bundle 加载到当前隔离租约。
--
-- args contains plugin identity, immutable bundle source, and every declared callable function.
-- args 包含插件身份、不可变 bundle 源码以及全部已声明可调用函数。
-- Returns true after the bundle table and its ABI have been validated.
-- bundle 表及其 ABI 校验完成后返回 true。
function M.load(args)
    if loaded_plugin ~= nil then
        error("Vulcan plugin bridge lease already contains a loaded plugin")
    end
    local plugin_id = require_string(args, "plugin_id")
    -- runtime_id routes process-wide LuaSkills host callbacks to this exact manager.
    -- runtime_id 把进程级 LuaSkills 宿主回调路由到当前精确管理器。
    local runtime_id = require_string(args, "runtime_id")
    -- lease_scope binds every host capability call to the exact lifecycle state selected by Vulcan.
    -- lease_scope 把每次宿主能力调用绑定到 Vulcan 选择的精确生命周期状态。
    local lease_scope = require_string(args, "lease_scope")
    local workspace_root = require_string(args, "workspace_root")
    local bundle_source = require_string(args, "bundle_source")
    -- chunk_name provides stable diagnostics without exposing an executable host path.
    -- chunk_name 提供稳定诊断，同时不暴露可执行宿主路径。
    local chunk_name = "@vulcan-plugin/" .. plugin_id .. "/runtime/plugin.bundle.lua"
    active_workspace_root = workspace_root
    local plugin_environment = build_plugin_environment(plugin_id, runtime_id, lease_scope)
    local loader, compile_error = load(bundle_source, chunk_name, "t", plugin_environment)
    if loader == nil then
        error("Failed to compile Vulcan plugin bundle: " .. tostring(compile_error))
    end
    local plugin = loader()
    if type(plugin) ~= "table" then
        error("Vulcan plugin bundle must return a table")
    end
    require_plugin_function(plugin, require_string(args, "invoke_function"))
    require_plugin_function(plugin, require_string(args, "shutdown_function"))
    if args.hook_shutdown_function ~= nil then
        require_plugin_function(plugin, args.hook_shutdown_function)
    end
    if type(args.hook_functions) ~= "table" then
        error("Vulcan plugin bridge requires hook_functions to be a table")
    end
    for _, function_name in ipairs(args.hook_functions) do
        require_plugin_function(plugin, function_name)
    end
    if type(args.additional_functions) ~= "table" then
        error("Vulcan plugin bridge requires additional_functions to be a table")
    end
    for _, function_name in ipairs(args.additional_functions) do
        require_plugin_function(plugin, function_name)
    end
    loaded_plugin = plugin
    loaded_plugin_id = plugin_id
    return true
end

-- Report whether the current bridge lease contains one valid loaded plugin.
-- 报告当前桥接租约是否包含一个有效的已加载插件。
--
-- Returns true only after load completed successfully.
-- 仅在 load 成功完成后返回 true。
function M.probe()
    return loaded_plugin ~= nil and loaded_plugin_id ~= nil
end

-- Invoke one exact manifest-declared function on the loaded plugin table.
-- 调用已加载插件表中由清单声明的精确函数。
--
-- args contains function_name and the unmodified host-owned plugin_args table.
-- args 包含 function_name 与未经修改的宿主持有 plugin_args 表。
-- Returns the exact plugin function result.
-- 返回精确的插件函数结果。
function M.invoke(args)
    if loaded_plugin == nil then
        error("Vulcan plugin bridge lease has no loaded plugin")
    end
    local function_name = require_function_name(args.function_name)
    local plugin_function = loaded_plugin[function_name]
    if type(plugin_function) ~= "function" then
        error("Vulcan plugin bundle does not export function " .. function_name)
    end
    if type(args.plugin_args) ~= "table" then
        error("Vulcan plugin bridge requires plugin_args to be a table")
    end
    -- previous_workspace_root restores the persistent lease context after this exact call.
    -- previous_workspace_root 在当前精确调用后恢复持久租约上下文。
    local previous_workspace_root = active_workspace_root
    if args.workspace_root ~= nil then
        active_workspace_root = require_string(args, "workspace_root")
    end
    -- call_ok and call_result ensure restoration also happens when plugin code raises an error.
    -- call_ok 与 call_result 确保插件代码抛错时同样恢复上下文。
    local call_ok, call_result = pcall(plugin_function, args.plugin_args)
    active_workspace_root = previous_workspace_root
    if not call_ok then
        error(call_result)
    end
    return call_result
end

return M
