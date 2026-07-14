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
-- Capture the trusted module loader before the restricted plugin environment is created.
-- 在创建受限插件环境前捕获可信模块加载器。
local host_require = require
-- Capture the host path separator without exposing the Lua package table to plugin code.
-- 捕获宿主路径分隔符，同时不向插件代码暴露 Lua package 表。
local host_is_windows = package.config:sub(1, 1) == "\\"
-- Capture the trusted package table before constructing the restricted plugin environment.
-- 在构建受限插件环境前捕获可信 package 表。
local host_package = package

-- Remove Windows verbatim prefixes from every Lua package-search template entry.
-- 从每个 Lua 包搜索模板条目中移除 Windows verbatim 前缀。
--
-- search_path is one semicolon-delimited package.path or package.cpath value.
-- search_path 是一个使用分号分隔的 package.path 或 package.cpath 值。
-- Returns the same search chain with drive and UNC identities preserved and Lua placeholders untouched.
-- 返回保持盘符、UNC 身份与 Lua 占位符不变的同一搜索链。
local function normalize_windows_package_search_path(search_path)
    if type(search_path) ~= "string" or search_path == "" then
        return search_path
    end
    -- entries preserves ordering and empty entries exactly as supplied by Lua.
    -- entries 按 Lua 提供的原始顺序精确保留条目与空条目。
    local entries = {}
    -- entry_start is the first byte of the next package-search entry.
    -- entry_start 是下一个包搜索条目的首字节位置。
    local entry_start = 1
    while true do
        -- separator_index is the next literal semicolon delimiter, if present.
        -- separator_index 是下一个字面量分号分隔符的位置（若存在）。
        local separator_index = string.find(search_path, ";", entry_start, true)
        -- entry is one untouched package-search template before prefix normalization.
        -- entry 是前缀规范化前的单个原始包搜索模板。
        local entry
        if separator_index == nil then
            entry = string.sub(search_path, entry_start)
        else
            entry = string.sub(search_path, entry_start, separator_index - 1)
        end
        -- verbatim_unc_prefix identifies the extended-length UNC spelling emitted by Windows canonicalize.
        -- verbatim_unc_prefix 标识 Windows canonicalize 生成的扩展长度 UNC 写法。
        local verbatim_unc_prefix = [[\\?\UNC\]]
        -- verbatim_drive_prefix identifies the extended-length drive spelling emitted by Windows canonicalize.
        -- verbatim_drive_prefix 标识 Windows canonicalize 生成的扩展长度盘符写法。
        local verbatim_drive_prefix = [[\\?\]]
        -- forward_verbatim_unc_prefix accepts the equivalent slash spelling at the same trusted boundary.
        -- forward_verbatim_unc_prefix 在同一可信边界接受等价的正斜杠写法。
        local forward_verbatim_unc_prefix = "//?/UNC/"
        -- forward_verbatim_drive_prefix accepts the equivalent slash drive spelling.
        -- forward_verbatim_drive_prefix 接受等价的正斜杠盘符写法。
        local forward_verbatim_drive_prefix = "//?/"
        if string.lower(string.sub(entry, 1, #verbatim_unc_prefix)) == string.lower(verbatim_unc_prefix) then
            entry = [[\\]] .. string.sub(entry, #verbatim_unc_prefix + 1)
        elseif string.sub(entry, 1, #verbatim_drive_prefix) == verbatim_drive_prefix
            and string.match(string.sub(entry, #verbatim_drive_prefix + 1), "^[A-Za-z]:[\\/]") ~= nil then
            entry = string.sub(entry, #verbatim_drive_prefix + 1)
        elseif string.lower(string.sub(entry, 1, #forward_verbatim_unc_prefix)) == string.lower(forward_verbatim_unc_prefix) then
            entry = [[\\]] .. string.gsub(string.sub(entry, #forward_verbatim_unc_prefix + 1), "/", "\\")
        elseif string.sub(entry, 1, #forward_verbatim_drive_prefix) == forward_verbatim_drive_prefix
            and string.match(string.sub(entry, #forward_verbatim_drive_prefix + 1), "^[A-Za-z]:[\\/]") ~= nil then
            entry = string.gsub(string.sub(entry, #forward_verbatim_drive_prefix + 1), "/", "\\")
        end
        entries[#entries + 1] = entry
        if separator_index == nil then
            break
        end
        entry_start = separator_index + 1
    end
    return table.concat(entries, ";")
end

if host_is_windows then
    -- package.path is normalized before any declared Lua module can trigger search-path substitution.
    -- package.path 在任何已声明 Lua 模块触发搜索路径替换前完成规范化。
    host_package.path = normalize_windows_package_search_path(host_package.path)
    -- package.cpath follows the same invariant for native module and C-root searchers.
    -- package.cpath 对原生模块与 C-root 搜索器应用同一不变量。
    host_package.cpath = normalize_windows_package_search_path(host_package.cpath)
end

-- Build the restricted global environment used by one real plugin bundle.
-- 构建单个真实插件 bundle 使用的受限全局环境。
--
-- plugin_id, runtime_id, lease_scope, and lua_module_names are immutable values bound by the Vulcan host.
-- plugin_id、runtime_id、lease_scope 与 lua_module_names 是 Vulcan 宿主绑定的不可变值。
-- Returns an environment exposing standard pure Lua helpers and an identity-injecting host call.
-- 返回一个环境，其中暴露标准纯 Lua 辅助能力及注入身份的宿主调用。
local function build_plugin_environment(plugin_id, runtime_id, lease_scope, lua_module_names)
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
    -- declared_modules records the exact host-authorized names without loading native modules eagerly.
    -- declared_modules 记录宿主精确授权的名称，同时避免提前加载原生模块。
    local declared_modules = {}
    -- loaded_modules caches only declared modules requested by the plugin bundle.
    -- loaded_modules 仅缓存插件 Bundle 实际请求过的已声明模块。
    local loaded_modules = {}
    for _, module_name in ipairs(lua_module_names) do
        if type(module_name) ~= "string" or module_name == "" then
            error("Vulcan plugin bridge requires every Lua module name to be a non-empty string")
        end
        if declared_modules[module_name] ~= nil then
            error("Vulcan plugin bridge received duplicate Lua module " .. module_name)
        end
        declared_modules[module_name] = true
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
        runtime = {
            -- Load and return one exact declared module without exposing Lua package discovery.
            -- 延迟加载并返回一个精确声明的模块，同时不暴露 Lua 包发现能力。
            module = function(module_name)
                if type(module_name) ~= "string" or module_name == "" then
                    error("Vulcan plugin runtime module name must be a non-empty string")
                end
                if declared_modules[module_name] == nil then
                    error("Vulcan plugin runtime module is not declared: " .. module_name)
                end
                -- loaded_module is the cached declaration-bound module, if already requested.
                -- loaded_module 是已请求过的声明绑定模块缓存。
                local loaded_module = loaded_modules[module_name]
                if loaded_module == nil then
                    loaded_module = host_require(module_name)
                    if loaded_module == nil then
                        error("Vulcan plugin runtime module returned nil: " .. module_name)
                    end
                    loaded_modules[module_name] = loaded_module
                end
                return loaded_module
            end,
            -- Report whether the trusted bridge is running on Windows for native CA selection.
            -- 报告可信桥是否运行于 Windows，以便选择原生 CA。
            is_windows = function()
                return host_is_windows
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
    if type(args.lua_modules) ~= "table" then
        error("Vulcan plugin bridge requires lua_modules to be a table")
    end
    local plugin_environment = build_plugin_environment(
        plugin_id,
        runtime_id,
        lease_scope,
        args.lua_modules
    )
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
